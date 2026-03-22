//! Linear API client

use crate::task::TodoTask;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";
const ASSIGNED_ISSUES_QUERY: &str = r#"
query AssignedIssues($after: String) {
  viewer {
    assignedIssues(first: 50, after: $after) {
      nodes {
        id
        identifier
        title
        dueDate
        state {
          name
          type
        }
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
}
"#;
const INBOX_NOTIFICATIONS_QUERY: &str = r#"
query InboxNotifications($after: String) {
  notifications(first: 50, after: $after, includeArchived: false) {
    nodes {
      id
      updatedAt
      readAt
      groupingKey
      title
      url
      inboxUrl
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
"#;

/// Linear API client
pub struct LinearClient {
    client: Client,
    api_token: String,
}

impl LinearClient {
    pub fn new(api_token: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, api_token }
    }

    /// Get issues assigned to the current user in "In Progress" state.
    pub async fn get_in_progress_issues(&self) -> Result<Vec<TodoTask>> {
        let mut tasks = Vec::new();
        let mut after: Option<String> = None;

        loop {
            let request = GraphqlRequest {
                query: ASSIGNED_ISSUES_QUERY,
                variables: GraphqlVariables {
                    after: after.clone(),
                },
            };

            let data = self
                .execute_graphql_request(request, "assigned_issues")
                .await?;

            let payload = data
                .data
                .context("Linear response was missing data payload")?;
            let viewer = payload.viewer.context("Linear response missing viewer")?;

            let connection = viewer
                .assigned_issues
                .context("Linear response missing assignedIssues")?;
            tasks.extend(
                connection
                    .nodes
                    .into_iter()
                    .filter(Self::is_in_progress)
                    .map(|issue| {
                        TodoTask::from_linear(
                            issue.id,
                            issue.identifier,
                            issue.title,
                            issue.due_date,
                        )
                    }),
            );

            if !connection.page_info.has_next_page {
                break;
            }

            after = connection.page_info.end_cursor;
            if after.is_none() {
                break;
            }
        }

        Ok(tasks)
    }

    /// Get unread inbox notifications tied to issues.
    pub async fn get_inbox_notifications(&self) -> Result<Vec<TodoTask>> {
        let mut newest_by_group: HashMap<String, LinearNotificationNode> = HashMap::new();
        let mut after: Option<String> = None;

        loop {
            let request = GraphqlRequest {
                query: INBOX_NOTIFICATIONS_QUERY,
                variables: GraphqlVariables {
                    after: after.clone(),
                },
            };

            let data = self
                .execute_graphql_request(request, "inbox_notifications")
                .await?;

            let payload = data
                .data
                .context("Linear response was missing data payload")?;

            let connection = payload
                .notifications
                .context("Linear response missing notifications")?;
            for notification in connection
                .nodes
                .into_iter()
                .filter(|notification| notification.read_at.is_none())
            {
                let group_key = notification
                    .grouping_key
                    .clone()
                    .filter(|key| !key.trim().is_empty())
                    .unwrap_or_else(|| format!("id:{}", notification.id));

                match newest_by_group.get(&group_key) {
                    Some(existing) if !is_newer_notification(&notification, existing) => {}
                    _ => {
                        newest_by_group.insert(group_key, notification);
                    }
                }
            }

            if !connection.page_info.has_next_page {
                break;
            }

            after = connection.page_info.end_cursor;
            if after.is_none() {
                break;
            }
        }

        let mut notifications: Vec<_> = newest_by_group.into_values().collect();
        notifications.sort_by(|a, b| {
            notification_sort_key(&b.updated_at)
                .cmp(&notification_sort_key(&a.updated_at))
                .then_with(|| a.id.cmp(&b.id))
        });

        Ok(notifications
            .into_iter()
            .map(|notification| {
                let open_url = notification
                    .url
                    .or(notification.inbox_url)
                    .filter(|url| !url.trim().is_empty());
                TodoTask::from_linear_notification(
                    notification.id,
                    notification
                        .title
                        .unwrap_or_else(|| "Linear notification".to_string()),
                    open_url,
                )
            })
            .collect())
    }

    fn is_in_progress(issue: &LinearIssueNode) -> bool {
        issue
            .state
            .kind
            .as_deref()
            .map(|kind| kind.eq_ignore_ascii_case("started"))
            .unwrap_or(false)
            || issue
                .state
                .name
                .as_deref()
                .map(|name| name.eq_ignore_ascii_case("in progress"))
                .unwrap_or(false)
    }

    async fn execute_graphql_request(
        &self,
        request: GraphqlRequest,
        operation: &str,
    ) -> Result<GraphqlResponse> {
        let response = self
            .client
            .post(LINEAR_API_URL)
            .header("Authorization", self.api_token.as_str())
            .json(&request)
            .send()
            .await
            .context("Failed to connect to Linear API")?;

        let status = response.status();
        let raw_body = response
            .text()
            .await
            .context("Failed to read Linear response body")?;

        if !status.is_success() {
            log_linear_error(
                operation,
                &format!("HTTP status: {}\nResponse body:\n{}\n", status, raw_body),
            );
            return Err(anyhow::anyhow!("Linear API error ({}): {}", status, raw_body));
        }

        let data: GraphqlResponse = serde_json::from_str(&raw_body).map_err(|error| {
            log_linear_error(
                operation,
                &format!(
                    "Failed to parse Linear response: {}\nResponse body:\n{}\n",
                    error, raw_body
                ),
            );
            anyhow::anyhow!("Failed to parse Linear response: {}", error)
        })?;

        if let Some(errors) = &data.errors {
            let message = errors
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            log_linear_error(
                operation,
                &format!(
                    "GraphQL errors: {}\nResponse body:\n{}\n",
                    message, raw_body
                ),
            );
            return Err(anyhow::anyhow!("Linear GraphQL error: {}", message));
        }

        Ok(data)
    }
}

fn linear_log_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("todo-tray").join("logs").join("linear.log"))
}

fn log_linear_error(operation: &str, details: &str) {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let entry = format!("[{}] [{}]\n{}\n", timestamp, operation, details);

    if let Some(path) = linear_log_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = file.write_all(entry.as_bytes());
            return;
        }
    }

    eprintln!("[Linear] {}", entry);
}

fn is_newer_notification(candidate: &LinearNotificationNode, existing: &LinearNotificationNode) -> bool {
    notification_sort_key(&candidate.updated_at) > notification_sort_key(&existing.updated_at)
}

fn notification_sort_key(updated_at: &Option<String>) -> String {
    updated_at.clone().unwrap_or_default()
}

#[derive(Debug, Serialize)]
struct GraphqlRequest {
    query: &'static str,
    variables: GraphqlVariables,
}

#[derive(Debug, Serialize)]
struct GraphqlVariables {
    after: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphqlResponse {
    data: Option<GraphqlData>,
    errors: Option<Vec<GraphqlError>>,
}

#[derive(Debug, Deserialize)]
struct GraphqlError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct GraphqlData {
    viewer: Option<LinearViewer>,
    notifications: Option<LinearNotificationConnection>,
}

#[derive(Debug, Deserialize)]
struct LinearViewer {
    #[serde(rename = "assignedIssues")]
    assigned_issues: Option<LinearIssueConnection>,
}

#[derive(Debug, Deserialize)]
struct LinearIssueConnection {
    nodes: Vec<LinearIssueNode>,
    #[serde(rename = "pageInfo")]
    page_info: LinearPageInfo,
}

#[derive(Debug, Deserialize)]
struct LinearNotificationConnection {
    nodes: Vec<LinearNotificationNode>,
    #[serde(rename = "pageInfo")]
    page_info: LinearPageInfo,
}

#[derive(Debug, Deserialize)]
struct LinearPageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LinearIssueNode {
    id: String,
    identifier: String,
    title: String,
    #[serde(rename = "dueDate")]
    due_date: Option<String>,
    state: LinearIssueState,
}

#[derive(Debug, Deserialize)]
struct LinearIssueState {
    name: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LinearNotificationNode {
    id: String,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
    #[serde(rename = "readAt")]
    read_at: Option<String>,
    #[serde(rename = "groupingKey")]
    grouping_key: Option<String>,
    title: Option<String>,
    url: Option<String>,
    #[serde(rename = "inboxUrl")]
    inbox_url: Option<String>,
}
