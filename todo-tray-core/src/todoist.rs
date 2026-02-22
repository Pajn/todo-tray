//! Todoist API client

use crate::task::{TodoTask, TodoistTask};
use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

const TODOIST_API_URL: &str = "https://api.todoist.com/api/v1";

/// Todoist API client
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

    /// Get tasks for today, overdue, and tomorrow
    pub async fn get_tasks(&self) -> Result<Vec<TodoTask>> {
        let url = format!("{}/tasks/filter", TODOIST_API_URL);
        let mut all_tasks = Vec::new();
        let mut cursor: Option<String> = None;

        // Fetch all pages
        loop {
            let mut request = self
                .client
                .get(&url)
                .header("Authorization", format!("Bearer {}", self.api_token))
                .query(&[("query", "today | overdue | tomorrow")])
                .query(&[("limit", "100")]);

            if let Some(ref c) = cursor {
                request = request.query(&[("cursor", c.as_str())]);
            }

            let response = request
                .send()
                .await
                .context("Failed to connect to Todoist API")?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(anyhow::anyhow!("Todoist API error ({}): {}", status, body));
            }

            #[derive(Deserialize)]
            struct FilterResponse {
                results: Vec<TodoistTask>,
                next_cursor: Option<String>,
            }

            let data: FilterResponse = response
                .json()
                .await
                .context("Failed to parse Todoist response")?;

            all_tasks.extend(data.results);

            // Check if there are more pages
            match data.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }

        Ok(all_tasks.into_iter().map(TodoTask::from_todoist).collect())
    }

    /// Complete a task
    pub async fn complete_task(&self, task_id: &str) -> Result<()> {
        let url = format!("{}/tasks/{}/close", TODOIST_API_URL, task_id);

        let response = self
            .client
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
}
