use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

const TODOIST_API_URL: &str = "https://api.todoist.com/api/v1";

#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub content: String,
    pub due_datetime: Option<DateTime<Utc>>,
    pub is_overdue: bool,
}

impl Task {
    pub fn is_today(&self) -> bool {
        if let Some(dt) = self.due_datetime {
            let today = Local::now().date_naive();
            dt.with_timezone(&Local).date_naive() == today
        } else {
            false
        }
    }
    
    pub fn is_tomorrow(&self) -> bool {
        if let Some(dt) = self.due_datetime {
            let tomorrow = Local::now().date_naive() + chrono::Duration::days(1);
            dt.with_timezone(&Local).date_naive() == tomorrow
        } else {
            false
        }
    }
    
    pub fn display_time(&self) -> String {
        if let Some(dt) = self.due_datetime {
            let local = dt.with_timezone(&Local);
            if self.is_overdue {
                // Show how overdue
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
}

#[derive(Debug, Deserialize)]
struct TodoistTask {
    id: String,
    content: String,
    due: Option<TodoistDue>,
    // Note: API doesn't return is_overdue, we calculate it
}

#[derive(Debug, Deserialize)]
struct TodoistDue {
    // The date field can be either "YYYY-MM-DD" or "YYYY-MM-DDTHH:MM:SS"
    date: String,
}

pub struct TodoistClient {
    client: Client,
    api_token: String,
}

impl TodoistClient {
    pub fn new(api_token: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        
        Self { client, api_token }
    }
    
    pub async fn get_today_tasks(&self) -> Result<Vec<Task>> {
        let url = format!("{}/tasks/filter", TODOIST_API_URL);
        
        // Use GET request with query parameter
        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .query(&[("query", "today | overdue | tomorrow")])
            .send()
            .await
            .context("Failed to connect to Todoist API")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Todoist API error ({}): {}",
                status,
                body
            ));
        }
        
        // Response is paginated with "results" field
        #[derive(Deserialize)]
        struct FilterResponse {
            results: Vec<TodoistTask>,
        }
        
        let data: FilterResponse = response
            .json()
            .await
            .context("Failed to parse Todoist response")?;
        
        Ok(data.results.into_iter().map(|t| self.convert_task(t)).collect())
    }
    
    pub async fn complete_task(&self, task_id: &str) -> Result<()> {
        let url = format!("{}/tasks/{}/close", TODOIST_API_URL, task_id);
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .send()
            .await
            .context("Failed to connect to Todoist API")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Failed to complete task ({}): {}",
                status,
                body
            ));
        }
        
        Ok(())
    }
    
    fn convert_task(&self, task: TodoistTask) -> Task {
        let due_datetime = task.due.and_then(|d| {
            // Try parsing as datetime first (YYYY-MM-DDTHH:MM:SS)
            if d.date.contains('T') {
                // Parse as datetime without timezone - Todoist returns local time
                chrono::NaiveDateTime::parse_from_str(&d.date, "%Y-%m-%dT%H:%M:%S")
                    .ok()
                    .and_then(|dt| dt.and_local_timezone(Local).earliest())
                    .map(|local| local.with_timezone(&Utc))
            } else {
                // Parse as date only (YYYY-MM-DD) - assume end of day in local time
                chrono::NaiveDate::parse_from_str(&d.date, "%Y-%m-%d")
                    .ok()
                    .and_then(|d| {
                        d.and_hms_opt(23, 59, 59)
                            .and_then(|dt| dt.and_local_timezone(Local).earliest())
                            .map(|local| local.with_timezone(&Utc))
                    })
            }
        });
        
        // Calculate if overdue (due time is in the past)
        let is_overdue = due_datetime
            .map(|dt| dt < Utc::now())
            .unwrap_or(false);
        
        Task {
            id: task.id,
            content: task.content,
            due_datetime,
            is_overdue,
        }
    }
}

/// Sort tasks: overdue first, then chronologically
pub fn sort_tasks(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| {
        // Overdue tasks first
        match (a.is_overdue, b.is_overdue) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                // Then by due datetime
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
