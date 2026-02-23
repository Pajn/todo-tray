//! Linear API client

use crate::task::TodoTask;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
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

            let response = self
                .client
                .post(LINEAR_API_URL)
                .header("Authorization", self.api_token.as_str())
                .json(&request)
                .send()
                .await
                .context("Failed to connect to Linear API")?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(anyhow::anyhow!("Linear API error ({}): {}", status, body));
            }

            let data: GraphqlResponse = response
                .json()
                .await
                .context("Failed to parse Linear response")?;

            if let Some(errors) = data.errors {
                let message = errors
                    .into_iter()
                    .map(|e| e.message)
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(anyhow::anyhow!("Linear GraphQL error: {}", message));
            }

            let payload = data
                .data
                .context("Linear response was missing data payload")?;

            let connection = payload.viewer.assigned_issues;
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

    fn is_in_progress(issue: &LinearIssueNode) -> bool {
        issue.state.kind.eq_ignore_ascii_case("started")
            || issue.state.name.eq_ignore_ascii_case("in progress")
    }
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
    viewer: LinearViewer,
}

#[derive(Debug, Deserialize)]
struct LinearViewer {
    #[serde(rename = "assignedIssues")]
    assigned_issues: LinearIssueConnection,
}

#[derive(Debug, Deserialize)]
struct LinearIssueConnection {
    nodes: Vec<LinearIssueNode>,
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
    name: String,
    #[serde(rename = "type")]
    kind: String,
}
