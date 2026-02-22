//! Task data structures for FFI

use chrono::{DateTime, Local, Utc};
use serde::Deserialize;

/// A task from Todoist
#[derive(uniffi::Record, Clone, Debug)]
pub struct TodoTask {
    pub id: String,
    pub content: String,
    pub due_datetime: Option<String>, // ISO 8601 format
    pub is_overdue: bool,
    pub is_today: bool,
    pub is_tomorrow: bool,
    pub display_time: String,
}

impl TodoTask {
    pub fn from_todoist(task: TodoistTask) -> Self {
        let due_datetime = task.due.and_then(|d| parse_due_date(&d.date));

        let is_overdue = due_datetime
            .as_ref()
            .map(|dt| dt < &Utc::now())
            .unwrap_or(false);

        let is_today = due_datetime
            .as_ref()
            .map(|dt| {
                let today = Local::now().date_naive();
                dt.with_timezone(&Local).date_naive() == today
            })
            .unwrap_or(false);

        let is_tomorrow = due_datetime
            .as_ref()
            .map(|dt| {
                let tomorrow = Local::now().date_naive() + chrono::Duration::days(1);
                dt.with_timezone(&Local).date_naive() == tomorrow
            })
            .unwrap_or(false);

        let display_time = format_display_time(&due_datetime, is_overdue);

        Self {
            id: task.id,
            content: task.content,
            due_datetime: due_datetime.map(|dt| dt.to_rfc3339()),
            is_overdue,
            is_today,
            is_tomorrow,
            display_time,
        }
    }
}

/// Parse a due date from Todoist API
fn parse_due_date(date_str: &str) -> Option<DateTime<Utc>> {
    if date_str.ends_with('Z') {
        // Date with 'Z' suffix is in UTC - parse directly as UTC
        chrono::NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%SZ")
            .ok()
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
    } else if date_str.contains('T') {
        // Date with time but no timezone - treat as local time
        chrono::NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S")
            .ok()
            .and_then(|dt| dt.and_local_timezone(Local).earliest())
            .map(|local| local.with_timezone(&Utc))
    } else {
        // Date only (no time) - treat as local date at end of day
        chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .ok()
            .and_then(|d| {
                d.and_hms_opt(23, 59, 59)
                    .and_then(|dt| dt.and_local_timezone(Local).earliest())
                    .map(|local| local.with_timezone(&Utc))
            })
    }
}

/// Format the display time for a task (24-hour clock)
fn format_display_time(due_datetime: &Option<DateTime<Utc>>, is_overdue: bool) -> String {
    if let Some(dt) = due_datetime {
        let local = dt.with_timezone(&Local);
        if is_overdue {
            let now = Local::now();
            let diff = now.signed_duration_since(local);
            if diff.num_days() > 0 {
                format!("{}d ago", diff.num_days())
            } else if diff.num_hours() > 0 {
                format!("{}h ago", diff.num_hours())
            } else {
                "overdue".to_string()
            }
        } else {
            local.format("%H:%M").to_string()
        }
    } else {
        "no due date".to_string()
    }
}

/// Task from Todoist API
#[derive(Debug, Deserialize)]
pub struct TodoistTask {
    pub id: String,
    pub content: String,
    pub due: Option<TodoistDue>,
}

/// Due date from Todoist API
#[derive(Debug, Deserialize)]
pub struct TodoistDue {
    pub date: String,
}

/// Grouped task lists
#[derive(uniffi::Record, Clone, Debug, Default)]
pub struct TaskList {
    pub overdue: Vec<TodoTask>,
    pub today: Vec<TodoTask>,
    pub tomorrow: Vec<TodoTask>,
}

/// Sort tasks: overdue first, then chronologically
pub fn sort_tasks(tasks: &mut [TodoTask]) {
    tasks.sort_by(|a, b| {
        // Overdue tasks first
        match (a.is_overdue, b.is_overdue) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                // Then by due datetime (string comparison works for ISO 8601)
                match (&a.due_datetime, &b.due_datetime) {
                    (Some(dt_a), Some(dt_b)) => dt_a.cmp(dt_b),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            }
        }
    });
}

/// Group tasks into overdue, today, and tomorrow
pub fn group_tasks(mut tasks: Vec<TodoTask>) -> TaskList {
    sort_tasks(&mut tasks);

    let overdue: Vec<_> = tasks.iter().filter(|t| t.is_overdue).cloned().collect();
    let today: Vec<_> = tasks
        .iter()
        .filter(|t| t.is_today && !t.is_overdue)
        .cloned()
        .collect();
    let tomorrow: Vec<_> = tasks.iter().filter(|t| t.is_tomorrow).cloned().collect();

    TaskList {
        overdue,
        today,
        tomorrow,
    }
}
