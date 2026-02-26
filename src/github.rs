//! GitHub notifications API client

use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

const GITHUB_API_URL: &str = "https://api.github.com";
const GITHUB_API_VERSION: &str = "2022-11-28";
const USER_AGENT: &str = "todo-tray";
const PAGE_SIZE: usize = 50;
const MAX_PAGES: usize = 10;

#[derive(uniffi::Record, Clone, Debug)]
pub struct GithubNotification {
    pub thread_id: String,
    pub title: String,
    pub repository: String,
    pub reason: String,
    pub web_url: String,
    pub updated_at: Option<String>, // RFC3339
    pub display_time: String,
}

#[derive(uniffi::Record, Clone, Debug, Default)]
pub struct GithubNotificationSection {
    pub account_name: String,
    pub notifications: Vec<GithubNotification>,
}

/// GitHub API client for one account
pub struct GithubClient {
    client: Client,
    account_name: String,
    api_token: String,
}

impl GithubClient {
    pub fn new(account_name: String, api_token: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            account_name,
            api_token,
        }
    }

    pub fn account_name(&self) -> &str {
        self.account_name.as_str()
    }

    /// Fetch unread notifications for this account.
    pub async fn get_notifications(&self) -> Result<GithubNotificationSection> {
        let mut notifications = Vec::new();

        for page in 1..=MAX_PAGES {
            let url = format!("{}/notifications", GITHUB_API_URL);
            let response = self
                .client
                .get(url)
                .header("Authorization", format!("Bearer {}", self.api_token))
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
                .header("User-Agent", USER_AGENT)
                .query(&[
                    ("all", "false"),
                    ("participating", "false"),
                    ("per_page", &PAGE_SIZE.to_string()),
                    ("page", &page.to_string()),
                ])
                .send()
                .await
                .with_context(|| {
                    format!(
                        "Failed to connect to GitHub API for account '{}'",
                        self.account_name
                    )
                })?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(anyhow::anyhow!(
                    "GitHub API error for account '{}' ({}): {}",
                    self.account_name,
                    status,
                    body
                ));
            }

            let page_items: Vec<GithubThread> = response.json().await.with_context(|| {
                format!(
                    "Failed to parse GitHub notifications for account '{}'",
                    self.account_name
                )
            })?;

            let item_count = page_items.len();
            notifications.extend(page_items.into_iter().filter(|n| n.unread).map(|thread| {
                let updated = parse_updated_at(&thread.updated_at);
                let web_url = build_web_url(&thread);
                GithubNotification {
                    thread_id: thread.id.clone(),
                    title: thread.subject.title,
                    repository: thread.repository.full_name,
                    reason: humanize_reason(&thread.reason),
                    web_url,
                    updated_at: updated.map(|dt| dt.to_rfc3339()),
                    display_time: format_relative_time(updated),
                }
            }));

            if item_count < PAGE_SIZE {
                break;
            }
        }

        Ok(GithubNotificationSection {
            account_name: self.account_name.clone(),
            notifications,
        })
    }

    /// Mark one notification thread as read.
    pub async fn mark_notification_as_read(&self, thread_id: &str) -> Result<()> {
        let url = format!("{}/notifications/threads/{}", GITHUB_API_URL, thread_id);
        let response = self
            .client
            .patch(url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
            .header("User-Agent", USER_AGENT)
            .send()
            .await
            .with_context(|| {
                format!(
                    "Failed to connect to GitHub API for account '{}'",
                    self.account_name
                )
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Failed to resolve GitHub notification for account '{}' ({}): {}",
                self.account_name,
                status,
                body
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct GithubThread {
    id: String,
    unread: bool,
    reason: String,
    updated_at: String,
    subject: GithubSubject,
    repository: GithubRepository,
}

#[derive(Debug, Deserialize)]
struct GithubSubject {
    title: String,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubRepository {
    full_name: String,
}

fn build_web_url(thread: &GithubThread) -> String {
    // Prefer opening the underlying issue/PR when available.
    if let Some(url) = thread
        .subject
        .url
        .as_deref()
        .and_then(api_subject_url_to_web_url)
    {
        return url;
    }

    // Fallback to inbox thread query for unsupported notification types.
    format!(
        "https://github.com/notifications?query=thread%3A{}",
        thread.id
    )
}

fn api_subject_url_to_web_url(url: &str) -> Option<String> {
    let path = url.strip_prefix("https://api.github.com/")?;
    let mut parts = path.split('/');

    if parts.next()? != "repos" {
        return None;
    }

    let owner = parts.next()?;
    let repo = parts.next()?;
    let kind = parts.next()?;
    let number = parts.next()?;

    match kind {
        "issues" => Some(format!(
            "https://github.com/{}/{}/issues/{}",
            owner, repo, number
        )),
        "pulls" => Some(format!(
            "https://github.com/{}/{}/pull/{}",
            owner, repo, number
        )),
        // GitHub notification subjects for releases use API paths like
        // /repos/{owner}/{repo}/releases/{id}. Web URLs are tag-based, so
        // map to the repo releases page when we only have an ID.
        "releases" => Some(format!("https://github.com/{}/{}/releases", owner, repo)),
        _ => None,
    }
}

fn parse_updated_at(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn format_relative_time(updated_at: Option<DateTime<Utc>>) -> String {
    let Some(updated_at) = updated_at else {
        return "recent".to_string();
    };

    let now = Utc::now();
    let diff = now.signed_duration_since(updated_at);
    if diff.num_days() > 0 {
        format!("{}d ago", diff.num_days())
    } else if diff.num_hours() > 0 {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_minutes() > 0 {
        format!("{}m ago", diff.num_minutes())
    } else {
        let local = updated_at.with_timezone(&Local);
        local.format("%H:%M").to_string()
    }
}

fn humanize_reason(reason: &str) -> String {
    let mut chars = reason.chars();
    let Some(first) = chars.next() else {
        return "notification".to_string();
    };
    let mut value = first.to_uppercase().collect::<String>();
    value.push_str(chars.as_str());
    value
}

#[cfg(test)]
mod tests {
    use super::api_subject_url_to_web_url;

    #[test]
    fn converts_issue_subject_url_to_web_url() {
        let url = "https://api.github.com/repos/octo-org/octo-repo/issues/123";
        assert_eq!(
            api_subject_url_to_web_url(url).as_deref(),
            Some("https://github.com/octo-org/octo-repo/issues/123")
        );
    }

    #[test]
    fn converts_pull_subject_url_to_web_url() {
        let url = "https://api.github.com/repos/octo-org/octo-repo/pulls/456";
        assert_eq!(
            api_subject_url_to_web_url(url).as_deref(),
            Some("https://github.com/octo-org/octo-repo/pull/456")
        );
    }

    #[test]
    fn returns_none_for_other_subject_url_types() {
        let url = "https://api.github.com/repos/octo-org/octo-repo/commits/abcdef";
        assert_eq!(api_subject_url_to_web_url(url), None);
    }

    #[test]
    fn converts_release_subject_url_to_releases_page() {
        let url = "https://api.github.com/repos/octo-org/octo-repo/releases/123456";
        assert_eq!(
            api_subject_url_to_web_url(url).as_deref(),
            Some("https://github.com/octo-org/octo-repo/releases")
        );
    }
}
