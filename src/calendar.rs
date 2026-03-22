//! iCalendar feed client and parser for today's events.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime, Utc};
use reqwest::Client;
use rrule::RRuleSet;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[derive(uniffi::Record, Clone, Debug)]
pub struct CalendarEvent {
    pub event_id: String,
    pub title: String,
    pub start_at: Option<String>, // RFC3339
    pub end_at: Option<String>,   // RFC3339
    pub display_time: String,
    pub open_url: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug, Default)]
pub struct CalendarEventSection {
    pub account_name: String,
    pub events: Vec<CalendarEvent>,
}

pub struct CalendarClient {
    client: Client,
    account_name: String,
    ical_url: String,
}

impl CalendarClient {
    pub fn new(account_name: String, ical_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            account_name,
            ical_url,
        }
    }

    pub async fn get_today_events(&self) -> Result<CalendarEventSection> {
        let response = self
            .client
            .get(&self.ical_url)
            .send()
            .await
            .with_context(|| {
                format!(
                    "Failed to connect to calendar feed for account '{}'",
                    self.account_name
                )
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Calendar feed error for account '{}' ({}): {}",
                self.account_name,
                status,
                body
            ));
        }

        let body = response.text().await.with_context(|| {
            format!(
                "Failed to read calendar feed body for account '{}'",
                self.account_name
            )
        })?;

        let parsed_feed = parse_ical_feed(&body);
        let section_name = if parsed_feed.calendar_name.trim().is_empty() {
            self.account_name.clone()
        } else {
            parsed_feed.calendar_name
        };

        let now_local = Local::now();
        let today = now_local.date_naive();
        let day_start_local = local_midnight(today)?;
        let day_end_local = day_start_local + ChronoDuration::days(1);

        let mut events = raw_events_to_calendar_events(
            parsed_feed.events,
            today,
            day_start_local,
            day_end_local,
        );

        events.sort_by(|a, b| match (&a.start_at, &b.start_at) {
            (Some(left), Some(right)) => left.cmp(right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
        });

        Ok(CalendarEventSection {
            account_name: section_name,
            events,
        })
    }
}

#[derive(Default)]
struct ParsedFeed {
    calendar_name: String,
    events: Vec<RawEvent>,
}

#[derive(Clone, Default)]
struct RawEvent {
    uid: Option<String>,
    summary: Option<String>,
    url: Option<String>,
    conference_url: Option<String>,
    starts_at: Option<EventTime>,
    ends_at: Option<EventTime>,
    dtstart_line: Option<String>,
    recurrence_lines: Vec<String>,
    recurrence_id: Option<EventTime>,
    status: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum EventTime {
    Date(NaiveDate),
    DateTime(DateTime<Utc>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum OccurrenceKey {
    Date(NaiveDate),
    DateTime(DateTime<Utc>),
}

fn parse_ical_feed(content: &str) -> ParsedFeed {
    let unfolded = unfold_lines(content);
    let mut parsed = ParsedFeed::default();
    let mut current_event: Option<RawEvent> = None;

    for line in unfolded {
        let Some((name, params, value)) = parse_property_line(&line) else {
            continue;
        };

        if name == "BEGIN" && value == "VEVENT" {
            current_event = Some(RawEvent::default());
            continue;
        }
        if name == "END" && value == "VEVENT" {
            if let Some(event) = current_event.take() {
                parsed.events.push(event);
            }
            continue;
        }

        if let Some(event) = current_event.as_mut() {
            match name.as_str() {
                "UID" => event.uid = Some(value),
                "SUMMARY" => event.summary = Some(unescape_ical_text(&value)),
                "URL" => event.url = Some(value),
                "X-GOOGLE-CONFERENCE" => event.conference_url = Some(value),
                "STATUS" => event.status = Some(value.to_uppercase()),
                "DTSTART" => {
                    event.starts_at = parse_event_time(&value, &params);
                    event.dtstart_line = Some(line.clone());
                }
                "DTEND" => event.ends_at = parse_event_time(&value, &params),
                "RRULE" | "RDATE" | "EXDATE" | "EXRULE" => event.recurrence_lines.push(line.clone()),
                "RECURRENCE-ID" => event.recurrence_id = parse_event_time(&value, &params),
                _ => {}
            }
            continue;
        }

        if name == "X-WR-CALNAME" && parsed.calendar_name.is_empty() {
            parsed.calendar_name = unescape_ical_text(&value);
        }
    }

    parsed
}

fn raw_event_to_calendar_event(
    raw: RawEvent,
    today: NaiveDate,
    day_start_local: DateTime<Local>,
    day_end_local: DateTime<Local>,
) -> Option<CalendarEvent> {
    let open_url = raw
        .conference_url
        .as_deref()
        .and_then(normalize_event_url)
        .or_else(|| raw.url.as_deref().and_then(normalize_event_url));
    let title = raw
        .summary
        .unwrap_or_else(|| "(Untitled event)".to_string());
    let start = raw.starts_at?;
    let event_id = raw.uid.unwrap_or_else(|| {
        let start_hint = match &start {
            EventTime::Date(date) => date.to_string(),
            EventTime::DateTime(dt) => dt.to_rfc3339(),
        };
        format!("{}-{}", title, start_hint)
    });

    match start {
        EventTime::Date(start_date) => {
            let end_exclusive = match raw.ends_at {
                Some(EventTime::Date(date)) => date,
                Some(EventTime::DateTime(dt)) => dt.with_timezone(&Local).date_naive(),
                None => start_date + ChronoDuration::days(1),
            };

            let is_today = today >= start_date && today < end_exclusive;
            if !is_today {
                return None;
            }

            let start_local = local_midnight(start_date).ok()?;
            let end_local = local_midnight(end_exclusive).ok()?;

            Some(CalendarEvent {
                event_id,
                title,
                start_at: Some(start_local.with_timezone(&Utc).to_rfc3339()),
                end_at: Some(end_local.with_timezone(&Utc).to_rfc3339()),
                display_time: "All day".to_string(),
                open_url: open_url.clone(),
            })
        }
        EventTime::DateTime(start_utc) => {
            let start_local = start_utc.with_timezone(&Local);
            let end_local = match raw.ends_at {
                Some(EventTime::DateTime(dt)) => dt.with_timezone(&Local),
                Some(EventTime::Date(date)) => local_midnight(date).ok()?,
                None => start_local + ChronoDuration::hours(1),
            };

            if start_local >= day_end_local || end_local <= day_start_local {
                return None;
            }

            let display_time = if end_local > start_local {
                format!(
                    "{}-{}",
                    start_local.format("%H:%M"),
                    end_local.format("%H:%M")
                )
            } else {
                start_local.format("%H:%M").to_string()
            };

            Some(CalendarEvent {
                event_id,
                title,
                start_at: Some(start_local.with_timezone(&Utc).to_rfc3339()),
                end_at: Some(end_local.with_timezone(&Utc).to_rfc3339()),
                display_time,
                open_url,
            })
        }
    }
}

fn raw_events_to_calendar_events(
    raw_events: Vec<RawEvent>,
    today: NaiveDate,
    day_start_local: DateTime<Local>,
    day_end_local: DateTime<Local>,
) -> Vec<CalendarEvent> {
    let mut overrides: HashMap<String, HashMap<OccurrenceKey, RawEvent>> = HashMap::new();
    let mut masters = Vec::new();

    for event in raw_events {
        if let (Some(uid), Some(recurrence_id)) = (event.uid.clone(), event.recurrence_id.clone()) {
            overrides
                .entry(uid)
                .or_default()
                .insert(OccurrenceKey::from_event_time(&recurrence_id), event);
        } else {
            masters.push(event);
        }
    }

    let mut used_overrides: HashSet<(String, OccurrenceKey)> = HashSet::new();
    let mut events = Vec::new();

    for event in masters {
        if event.status.as_deref() == Some("CANCELLED") {
            continue;
        }
        let override_entries = event.uid.as_ref().and_then(|uid| overrides.get(uid));
        let expanded = if event.recurrence_lines.is_empty() {
            raw_event_to_calendar_event(event, today, day_start_local, day_end_local)
                .into_iter()
                .collect()
        } else {
            expand_recurring_event(
                &event,
                today,
                day_start_local,
                day_end_local,
                override_entries,
                &mut used_overrides,
            )
        };
        events.extend(expanded);
    }

    for (uid, uid_overrides) in overrides {
        for (key, event) in uid_overrides {
            if used_overrides.contains(&(uid.clone(), key)) {
                continue;
            }
            if event.status.as_deref() == Some("CANCELLED") {
                continue;
            }
            if let Some(event) = raw_event_to_calendar_event(event, today, day_start_local, day_end_local) {
                events.push(event);
            }
        }
    }

    events
}

fn expand_recurring_event(
    raw: &RawEvent,
    today: NaiveDate,
    day_start_local: DateTime<Local>,
    day_end_local: DateTime<Local>,
    overrides: Option<&HashMap<OccurrenceKey, RawEvent>>,
    used_overrides: &mut HashSet<(String, OccurrenceKey)>,
) -> Vec<CalendarEvent> {
    let Some(rrule_set) = build_rrule_set(raw, day_start_local, day_end_local) else {
        return raw_event_to_calendar_event(raw.clone(), today, day_start_local, day_end_local)
            .into_iter()
            .collect();
    };

    let mut events = Vec::new();
    let uid = raw.uid.clone().unwrap_or_default();
    let Some(starts_at) = raw.starts_at.as_ref() else {
        return events;
    };
    for occurrence in rrule_set.all(512).dates {
        let key = OccurrenceKey::from_occurrence(starts_at, occurrence);
        if let Some(override_event) = overrides.and_then(|items| items.get(&key)) {
            used_overrides.insert((uid.clone(), key));
            if override_event.status.as_deref() == Some("CANCELLED") {
                continue;
            }
            if let Some(event) = raw_event_to_calendar_event(
                override_event.clone(),
                today,
                day_start_local,
                day_end_local,
            ) {
                events.push(event);
            }
            continue;
        }

        let Some(instance) = instantiate_occurrence(raw, occurrence) else {
            continue;
        };
        if let Some(event) = raw_event_to_calendar_event(instance, today, day_start_local, day_end_local) {
            events.push(event);
        }
    }

    events
}

fn build_rrule_set(
    raw: &RawEvent,
    day_start_local: DateTime<Local>,
    day_end_local: DateTime<Local>,
) -> Option<RRuleSet> {
    let dtstart_line = raw.dtstart_line.as_ref()?;
    let mut rules = String::new();
    rules.push_str(dtstart_line);
    for line in &raw.recurrence_lines {
        rules.push('\n');
        rules.push_str(line);
    }

    let set: RRuleSet = rules.parse().ok()?;
    let duration = occurrence_duration(raw);
    let tz = set.get_dt_start().timezone();
    let after = (day_start_local - duration).with_timezone(&tz);
    let before = day_end_local.with_timezone(&tz);
    Some(set.after(after).before(before))
}

fn occurrence_duration(raw: &RawEvent) -> ChronoDuration {
    match (&raw.starts_at, &raw.ends_at) {
        (Some(EventTime::Date(start)), Some(EventTime::Date(end))) => *end - *start,
        (Some(EventTime::Date(_)), _) => ChronoDuration::days(1),
        (Some(EventTime::DateTime(start)), Some(EventTime::DateTime(end))) => *end - *start,
        (Some(EventTime::DateTime(start)), Some(EventTime::Date(end))) => {
            local_midnight(*end)
                .ok()
                .map(|end_local| end_local.with_timezone(&Utc) - *start)
                .unwrap_or_else(|| ChronoDuration::hours(1))
        }
        _ => ChronoDuration::hours(1),
    }
}

fn instantiate_occurrence(raw: &RawEvent, occurrence: DateTime<rrule::Tz>) -> Option<RawEvent> {
    let mut instance = raw.clone();
    instance.recurrence_lines.clear();
    instance.recurrence_id = None;
    instance.dtstart_line = None;

    match raw.starts_at.as_ref()? {
        EventTime::Date(_) => {
            let start_date = occurrence.date_naive();
            let duration = occurrence_duration(raw).num_days().max(1);
            instance.starts_at = Some(EventTime::Date(start_date));
            instance.ends_at = Some(EventTime::Date(start_date + ChronoDuration::days(duration)));
        }
        EventTime::DateTime(_) => {
            let start_utc = occurrence.with_timezone(&Utc);
            instance.starts_at = Some(EventTime::DateTime(start_utc));
            instance.ends_at = Some(EventTime::DateTime(start_utc + occurrence_duration(raw)));
        }
    }

    if let Some(uid) = &raw.uid {
        instance.uid = Some(format!(
            "{}:{}",
            uid,
            occurrence.with_timezone(&Utc).to_rfc3339()
        ));
    }

    Some(instance)
}

impl OccurrenceKey {
    fn from_event_time(event_time: &EventTime) -> Self {
        match event_time {
            EventTime::Date(date) => Self::Date(*date),
            EventTime::DateTime(dt) => Self::DateTime(*dt),
        }
    }

    fn from_occurrence(starts_at: &EventTime, occurrence: DateTime<rrule::Tz>) -> Self {
        match starts_at {
            EventTime::Date(_) => Self::Date(occurrence.date_naive()),
            EventTime::DateTime(_) => Self::DateTime(occurrence.with_timezone(&Utc)),
        }
    }
}

fn normalize_event_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn unfold_lines(content: &str) -> Vec<String> {
    let mut unfolded: Vec<String> = Vec::new();
    for raw_line in content.replace("\r\n", "\n").replace('\r', "\n").lines() {
        if raw_line.starts_with(' ') || raw_line.starts_with('\t') {
            if let Some(last) = unfolded.last_mut() {
                last.push_str(raw_line.trim_start());
            }
        } else {
            unfolded.push(raw_line.to_string());
        }
    }
    unfolded
}

fn parse_property_line(line: &str) -> Option<(String, HashMap<String, String>, String)> {
    let colon = line.find(':')?;
    let (left, right) = line.split_at(colon);
    let value = right.strip_prefix(':')?.to_string();

    let mut parts = left.split(';');
    let name = parts.next()?.trim().to_uppercase();
    let mut params = HashMap::new();

    for part in parts {
        let Some((key, val)) = part.split_once('=') else {
            continue;
        };
        params.insert(key.trim().to_uppercase(), val.trim().to_string());
    }

    Some((name, params, value))
}

fn parse_event_time(value: &str, params: &HashMap<String, String>) -> Option<EventTime> {
    let value_type = params.get("VALUE").map(|v| v.to_uppercase());
    if value_type.as_deref() == Some("DATE") || looks_like_date(value) {
        return NaiveDate::parse_from_str(value, "%Y%m%d")
            .ok()
            .map(EventTime::Date);
    }

    if value.ends_with('Z') {
        let naive = parse_ical_naive_datetime(value.strip_suffix('Z')?)?;
        return Some(EventTime::DateTime(
            DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc),
        ));
    }

    // For floating times or TZID values, treat as local time.
    let naive = parse_ical_naive_datetime(value)?;
    let local = naive.and_local_timezone(Local).earliest()?;
    Some(EventTime::DateTime(local.with_timezone(&Utc)))
}

fn parse_ical_naive_datetime(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S")
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M").ok())
}

fn looks_like_date(value: &str) -> bool {
    value.len() == 8 && value.chars().all(|c| c.is_ascii_digit())
}

fn unescape_ical_text(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

fn local_midnight(date: NaiveDate) -> Result<DateTime<Local>> {
    let naive_midnight = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid date when building local midnight: {}", date))?;
    naive_midnight
        .and_local_timezone(Local)
        .earliest()
        .ok_or_else(|| anyhow::anyhow!("Could not map local midnight due to timezone shift"))
}

#[cfg(test)]
mod tests {
    use super::{local_midnight, parse_ical_feed, raw_events_to_calendar_events};
    use chrono::{Duration as ChronoDuration, Local};

    #[test]
    fn parses_calendar_name_and_event_fields() {
        let ics = "BEGIN:VCALENDAR\r\nX-WR-CALNAME:Work Calendar\r\nBEGIN:VEVENT\r\nUID:abc123\r\nSUMMARY:Daily Sync\r\nDTSTART:20260224T090000Z\r\nDTEND:20260224T093000Z\r\nURL:https://example.com/event\r\nX-GOOGLE-CONFERENCE:https://meet.google.com/nsn-dwjm-vrk\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let parsed = parse_ical_feed(ics);
        assert_eq!(parsed.calendar_name, "Work Calendar");
        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.events[0].uid.as_deref(), Some("abc123"));
        assert_eq!(parsed.events[0].summary.as_deref(), Some("Daily Sync"));
        assert_eq!(
            parsed.events[0].url.as_deref(),
            Some("https://example.com/event")
        );
        assert_eq!(
            parsed.events[0].conference_url.as_deref(),
            Some("https://meet.google.com/nsn-dwjm-vrk")
        );
    }

    #[test]
    fn expands_recurring_events_for_today() {
        let today = Local::now().date_naive();
        let start = today.format("%Y%m%d").to_string();
        let ics = format!(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:daily-1\r\nSUMMARY:Standup\r\nDTSTART;VALUE=DATE:{start}\r\nDTEND;VALUE=DATE:{}\r\nRRULE:FREQ=DAILY;COUNT=3\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            (today + ChronoDuration::days(1)).format("%Y%m%d")
        );

        let parsed = parse_ical_feed(&ics);
        let day_start = local_midnight(today).unwrap();
        let day_end = day_start + ChronoDuration::days(1);
        let events = raw_events_to_calendar_events(parsed.events, today, day_start, day_end);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Standup");
        assert_eq!(events[0].display_time, "All day");
    }

    #[test]
    fn applies_exdate_to_recurring_events() {
        let today = Local::now().date_naive();
        let ics = format!(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:daily-2\r\nSUMMARY:Focus Time\r\nDTSTART;VALUE=DATE:{start}\r\nDTEND;VALUE=DATE:{end}\r\nRRULE:FREQ=DAILY;COUNT=3\r\nEXDATE;VALUE=DATE:{start}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            start = today.format("%Y%m%d"),
            end = (today + ChronoDuration::days(1)).format("%Y%m%d")
        );

        let parsed = parse_ical_feed(&ics);
        let day_start = local_midnight(today).unwrap();
        let day_end = day_start + ChronoDuration::days(1);
        let events = raw_events_to_calendar_events(parsed.events, today, day_start, day_end);

        assert!(events.is_empty());
    }

    #[test]
    fn prefers_recurrence_override_instance() {
        let today = Local::now().date_naive();
        let start = format!("{}T090000", today.format("%Y%m%d"));
        let moved = format!("{}T110000", today.format("%Y%m%d"));
        let ics = format!(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:series-1\r\nSUMMARY:1:1\r\nDTSTART:{start}\r\nDTEND:{}T093000\r\nRRULE:FREQ=DAILY;COUNT=2\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:series-1\r\nRECURRENCE-ID:{start}\r\nSUMMARY:1:1 moved\r\nDTSTART:{moved}\r\nDTEND:{}T113000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            today.format("%Y%m%d"),
            today.format("%Y%m%d"),
        );

        let parsed = parse_ical_feed(&ics);
        let day_start = local_midnight(today).unwrap();
        let day_end = day_start + ChronoDuration::days(1);
        let events = raw_events_to_calendar_events(parsed.events, today, day_start, day_end);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "1:1 moved");
        assert_eq!(events[0].display_time, "11:00-11:30");
    }
}
