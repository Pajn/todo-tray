//! Core FFI facade for Todo Tray
//!
//! This module provides the main interface exposed to Swift via UniFFI.

use crate::autostart;
use crate::calendar::{CalendarClient, CalendarEventSection};
use crate::config::{default_snooze_durations, Config};
use crate::github::{GithubClient, GithubNotificationSection};
use crate::linear::LinearClient;
use crate::task::{group_tasks, TaskList};
use crate::todoist::TodoistClient;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::sync::Mutex;

// Global tokio runtime for async operations
static TOKIO_RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    eprintln!("[Rust] Creating Tokio runtime...");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");
    eprintln!("[Rust] Tokio runtime created successfully");
    rt
});

/// Error types for Todo Tray
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum TodoTrayError {
    #[error("Configuration error: {message}")]
    Config { message: String },

    #[error("Network error: {message}")]
    Network { message: String },

    #[error("Not found: {message}")]
    NotFound { message: String },

    #[error("Unexpected error: {message}")]
    Unexpected { message: String },
}

impl From<anyhow::Error> for TodoTrayError {
    fn from(err: anyhow::Error) -> Self {
        TodoTrayError::Unexpected {
            message: err.to_string(),
        }
    }
}

/// Application state exposed to Swift
#[derive(uniffi::Record, Clone, Debug, Default)]
pub struct AppState {
    pub overdue_count: u32,
    pub today_count: u32,
    pub tomorrow_count: u32,
    pub in_progress_count: u32,
    pub github_notification_count: u32,
    pub calendar_event_count: u32,
    pub tasks: TaskList,
    pub github_notifications: Vec<GithubNotificationSection>,
    pub calendar_events: Vec<CalendarEventSection>,
    pub snooze_durations: Vec<String>,
    pub is_loading: bool,
    pub error_message: Option<String>,
    pub autostart_enabled: bool,
}

/// Trait implemented by Swift to receive state updates
#[uniffi::export(with_foreign)]
pub trait EventHandler: Send + Sync {
    /// Called when the app state changes
    fn on_state_changed(&self, state: AppState);

    /// Called when a task is completed
    fn on_task_completed(&self, task_name: String);

    /// Called when an error occurs
    fn on_error(&self, error: String);
}

/// Main Todo Tray core
#[derive(uniffi::Object)]
pub struct TodoTrayCore {
    state: Arc<Mutex<AppState>>,
    todoist_client: Arc<TodoistClient>,
    linear_client: Option<Arc<LinearClient>>,
    github_clients: Vec<Arc<GithubClient>>,
    calendar_clients: Vec<Arc<CalendarClient>>,
    snooze_durations: Vec<SnoozeDuration>,
    event_handler: Arc<dyn EventHandler>,
}

#[derive(Clone, Debug)]
struct SnoozeDuration {
    label: String,
    duration: chrono::Duration,
}

#[uniffi::export]
impl TodoTrayCore {
    /// Create a new TodoTrayCore instance (synchronous)
    #[uniffi::constructor]
    pub fn new(event_handler: Arc<dyn EventHandler>) -> Result<Arc<Self>, TodoTrayError> {
        eprintln!("[Rust] TodoTrayCore::new() called");

        // Force runtime initialization
        let _runtime = &*TOKIO_RUNTIME;
        eprintln!("[Rust] Runtime initialized");

        // Load config
        let config = Config::load().map_err(|e| {
            eprintln!("[Rust] Config load error: {}", e);
            TodoTrayError::Config {
                message: e.to_string(),
            }
        })?;
        eprintln!("[Rust] Config loaded successfully");

        let todoist_client = Arc::new(TodoistClient::new(config.todoist_api_token));
        let linear_client = config
            .linear_api_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(|token| Arc::new(LinearClient::new(token.to_string())));
        let github_clients = config
            .github_accounts
            .iter()
            .map(|account| {
                Arc::new(GithubClient::new(
                    account.name.trim().to_string(),
                    account.token.trim().to_string(),
                ))
            })
            .collect::<Vec<_>>();
        let calendar_clients = config
            .calendar_feeds
            .iter()
            .map(|feed| {
                Arc::new(CalendarClient::new(
                    feed.name.trim().to_string(),
                    feed.ical_url.trim().to_string(),
                ))
            })
            .collect::<Vec<_>>();
        let raw_snooze = if config.snooze_durations.is_empty() {
            default_snooze_durations()
        } else {
            config.snooze_durations.clone()
        };
        let snooze_durations = raw_snooze
            .into_iter()
            .map(|raw| {
                let label = raw.trim().to_string();
                parse_snooze_duration(&label).map(|duration| SnoozeDuration { label, duration })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|message| TodoTrayError::Config { message })?;

        let autostart_enabled = autostart::is_enabled();

        // Sync autostart with config
        if config.autostart && !autostart_enabled {
            let _ = autostart::enable();
        } else if !config.autostart && autostart_enabled {
            let _ = autostart::disable();
        }

        let core = Arc::new(Self {
            state: Arc::new(Mutex::new(AppState {
                autostart_enabled: autostart::is_enabled(),
                is_loading: true,
                snooze_durations: snooze_durations
                    .iter()
                    .map(|entry| entry.label.clone())
                    .collect(),
                ..Default::default()
            })),
            todoist_client,
            linear_client,
            github_clients,
            calendar_clients,
            snooze_durations,
            event_handler,
        });

        // Start background refresh loop
        let core_clone = core.clone();
        std::thread::spawn(move || {
            eprintln!("[Rust] Background thread started, entering tokio runtime...");
            // Run async code in the tokio runtime
            TOKIO_RUNTIME.block_on(async move {
                eprintln!("[Rust] Inside tokio runtime, starting background task...");

                // Initial refresh
                eprintln!("[Rust] About to call refresh_tasks()...");
                if let Err(e) = refresh_tasks(&core_clone).await {
                    eprintln!("[Rust] Initial refresh failed: {}", e);
                }
                eprintln!("[Rust] Initial refresh complete");

                // Refresh every 5 minutes
                let mut interval = tokio::time::interval(Duration::from_secs(300));
                loop {
                    interval.tick().await;
                    if let Err(e) = refresh_tasks(&core_clone).await {
                        eprintln!("[Rust] Refresh failed: {}", e);
                    }
                }
            });
        });

        eprintln!("[Rust] TodoTrayCore::new() returning...");

        Ok(core)
    }

    /// Refresh tasks from Todoist and Linear (synchronous wrapper)
    pub fn refresh(&self) -> Result<(), TodoTrayError> {
        TOKIO_RUNTIME.block_on(async { refresh_tasks(self).await })
    }

    /// Complete a task (synchronous wrapper)
    pub fn complete(&self, task_id: String) -> Result<(), TodoTrayError> {
        TOKIO_RUNTIME.block_on(async { complete_task(self, task_id).await })
    }

    /// Snooze a Todoist task by the provided duration label (e.g. "30m", "1d").
    pub fn snooze_task(
        &self,
        task_id: String,
        duration_label: String,
    ) -> Result<(), TodoTrayError> {
        TOKIO_RUNTIME.block_on(async { snooze_task(self, task_id, duration_label).await })
    }

    /// Resolve a GitHub notification thread for one configured account.
    pub fn resolve_github_notification(
        &self,
        account_name: String,
        thread_id: String,
    ) -> Result<(), TodoTrayError> {
        TOKIO_RUNTIME.block_on(async {
            resolve_github_notification_internal(self, account_name, thread_id).await
        })
    }

    /// Get the current app state
    pub fn get_state(&self) -> AppState {
        TOKIO_RUNTIME.block_on(async { self.state.lock().await.clone() })
    }

    /// Toggle autostart
    pub fn toggle_autostart(&self) -> Result<bool, TodoTrayError> {
        let enabled = if autostart::is_enabled() {
            autostart::disable().map_err(|e| TodoTrayError::Unexpected {
                message: e.to_string(),
            })?;
            false
        } else {
            autostart::enable().map_err(|e| TodoTrayError::Unexpected {
                message: e.to_string(),
            })?;
            true
        };

        // Update state
        let state = self.state.clone();
        let event_handler = self.event_handler.clone();
        TOKIO_RUNTIME.spawn(async move {
            let mut s = state.lock().await;
            s.autostart_enabled = enabled;
            let state_copy = s.clone();
            drop(s);
            event_handler.on_state_changed(state_copy);
        });

        Ok(enabled)
    }

    /// Check if autostart is enabled
    pub fn is_autostart_enabled(&self) -> bool {
        autostart::is_enabled()
    }
}

// Internal async implementations

async fn refresh_tasks(core: &TodoTrayCore) -> Result<(), TodoTrayError> {
    let todoist = core.todoist_client.get_tasks();
    let linear = async {
        match &core.linear_client {
            Some(client) => client.get_in_progress_issues().await.map(Some),
            None => Ok(None),
        }
    };
    let (mut tasks, linear_tasks) =
        tokio::try_join!(todoist, linear).map_err(|e| TodoTrayError::Network {
            message: e.to_string(),
        })?;
    let github_sections = fetch_github_notifications(core).await?;
    let calendar_sections = fetch_calendar_events(core).await?;

    if let Some(mut linear_tasks) = linear_tasks {
        tasks.append(&mut linear_tasks);
    }

    let grouped = group_tasks(tasks);

    let mut state = core.state.lock().await;
    apply_grouped_tasks_to_state(&mut state, grouped);
    state.github_notification_count = github_sections
        .iter()
        .map(|section| section.notifications.len() as u32)
        .sum();
    state.calendar_event_count = calendar_sections
        .iter()
        .map(|section| section.events.len() as u32)
        .sum();
    state.github_notifications = github_sections;
    state.calendar_events = calendar_sections;

    let state_copy = state.clone();
    drop(state);

    core.event_handler.on_state_changed(state_copy);

    Ok(())
}

async fn complete_task(core: &TodoTrayCore, task_id: String) -> Result<(), TodoTrayError> {
    // Lookup the task first so we can block completion for non-Todoist sources.
    let selected_task = {
        let state = core.state.lock().await;
        state
            .tasks
            .overdue
            .iter()
            .chain(state.tasks.today.iter())
            .chain(state.tasks.tomorrow.iter())
            .chain(state.tasks.in_progress.iter())
            .find(|t| t.id == task_id)
            .map(|t| (t.content.clone(), t.can_complete))
    };

    let (task_name, can_complete) = selected_task.ok_or_else(|| TodoTrayError::NotFound {
        message: format!("Task not found: {}", task_id),
    })?;

    if !can_complete {
        return Err(TodoTrayError::Unexpected {
            message: "This task is read-only and cannot be completed from Todo Tray.".to_string(),
        });
    }

    core.todoist_client
        .complete_task(&task_id)
        .await
        .map_err(|e| TodoTrayError::Network {
            message: e.to_string(),
        })?;

    // Notify
    core.event_handler.on_task_completed(task_name);

    // Refresh only Todoist-backed task sections; other sources refresh on interval.
    refresh_todoist_tasks(core).await?;

    Ok(())
}

async fn snooze_task(
    core: &TodoTrayCore,
    task_id: String,
    duration_label: String,
) -> Result<(), TodoTrayError> {
    let duration = core
        .snooze_durations
        .iter()
        .find(|entry| entry.label == duration_label)
        .map(|entry| entry.duration)
        .ok_or_else(|| TodoTrayError::Unexpected {
            message: format!("Unknown snooze duration: {}", duration_label),
        })?;

    let current_due = {
        let state = core.state.lock().await;
        state
            .tasks
            .overdue
            .iter()
            .chain(state.tasks.today.iter())
            .chain(state.tasks.tomorrow.iter())
            .find(|t| t.id == task_id && t.source == "todoist")
            .and_then(|t| t.due_datetime.clone())
    }
    .ok_or_else(|| TodoTrayError::NotFound {
        message: "Todoist task with due date not found".to_string(),
    })?;

    let due = DateTime::parse_from_rfc3339(&current_due)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| TodoTrayError::Unexpected {
            message: format!("Invalid due datetime on task: {}", e),
        })?;
    let new_due = due + duration;
    let due_datetime = new_due.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    core.todoist_client
        .update_task_due_datetime(&task_id, &due_datetime)
        .await
        .map_err(|e| TodoTrayError::Network {
            message: e.to_string(),
        })?;

    // Refresh only Todoist-backed task sections; other sources refresh on interval.
    refresh_todoist_tasks(core).await
}

async fn resolve_github_notification_internal(
    core: &TodoTrayCore,
    account_name: String,
    thread_id: String,
) -> Result<(), TodoTrayError> {
    let client = core
        .github_clients
        .iter()
        .find(|client| client.account_name() == account_name)
        .cloned()
        .ok_or_else(|| TodoTrayError::NotFound {
            message: format!("GitHub account not found: {}", account_name),
        })?;

    client
        .mark_notification_as_read(&thread_id)
        .await
        .map_err(|e| TodoTrayError::Network {
            message: e.to_string(),
        })?;

    // Refresh only this account's GitHub notifications; other sources refresh on interval.
    refresh_single_github_account(core, &account_name).await
}

async fn refresh_todoist_tasks(core: &TodoTrayCore) -> Result<(), TodoTrayError> {
    let mut todoist_tasks = core
        .todoist_client
        .get_tasks()
        .await
        .map_err(|e| TodoTrayError::Network {
            message: e.to_string(),
        })?;

    // Keep currently-cached Linear tasks; they will be refreshed on the regular interval.
    let cached_linear = {
        let state = core.state.lock().await;
        state.tasks.in_progress.clone()
    };
    todoist_tasks.extend(cached_linear);

    let grouped = group_tasks(todoist_tasks);

    let mut state = core.state.lock().await;
    apply_grouped_tasks_to_state(&mut state, grouped);
    let state_copy = state.clone();
    drop(state);

    core.event_handler.on_state_changed(state_copy);
    Ok(())
}

async fn refresh_single_github_account(
    core: &TodoTrayCore,
    account_name: &str,
) -> Result<(), TodoTrayError> {
    let client = core
        .github_clients
        .iter()
        .find(|client| client.account_name() == account_name)
        .cloned()
        .ok_or_else(|| TodoTrayError::NotFound {
            message: format!("GitHub account not found: {}", account_name),
        })?;

    let section = client
        .get_notifications()
        .await
        .map_err(|e| TodoTrayError::Network {
            message: e.to_string(),
        })?;

    let mut state = core.state.lock().await;
    let existing_index = state
        .github_notifications
        .iter()
        .position(|s| s.account_name == account_name);
    state
        .github_notifications
        .retain(|s| s.account_name != account_name);
    if !section.notifications.is_empty() {
        if let Some(index) = existing_index {
            let index = index.min(state.github_notifications.len());
            state.github_notifications.insert(index, section);
        } else {
            state.github_notifications.push(section);
        }
    }
    state.github_notification_count = state
        .github_notifications
        .iter()
        .map(|section| section.notifications.len() as u32)
        .sum();
    state.is_loading = false;
    state.error_message = None;
    let state_copy = state.clone();
    drop(state);

    core.event_handler.on_state_changed(state_copy);
    Ok(())
}

fn apply_grouped_tasks_to_state(state: &mut AppState, grouped: TaskList) {
    state.overdue_count = grouped.overdue.len() as u32;
    state.today_count = grouped.today.len() as u32;
    state.tomorrow_count = grouped.tomorrow.len() as u32;
    state.in_progress_count = grouped.in_progress.len() as u32;
    state.tasks = grouped;
    state.is_loading = false;
    state.error_message = None;
}

async fn fetch_github_notifications(
    core: &TodoTrayCore,
) -> Result<Vec<GithubNotificationSection>, TodoTrayError> {
    let mut sections = Vec::new();
    for client in &core.github_clients {
        let section = client
            .get_notifications()
            .await
            .map_err(|e| TodoTrayError::Network {
                message: e.to_string(),
            })?;
        if !section.notifications.is_empty() {
            sections.push(section);
        }
    }
    Ok(sections)
}

async fn fetch_calendar_events(
    core: &TodoTrayCore,
) -> Result<Vec<CalendarEventSection>, TodoTrayError> {
    let mut sections = Vec::new();
    for client in &core.calendar_clients {
        let section = client
            .get_today_events()
            .await
            .map_err(|e| TodoTrayError::Network {
                message: e.to_string(),
            })?;
        if !section.events.is_empty() {
            sections.push(section);
        }
    }
    Ok(sections)
}

fn parse_snooze_duration(input: &str) -> Result<chrono::Duration, String> {
    let value = input.trim().to_lowercase();
    if value.len() < 2 {
        return Err(format!("Invalid snooze duration '{}'", input));
    }

    let (number_part, unit_part) = value.split_at(value.len() - 1);
    let amount: i64 = number_part
        .parse()
        .map_err(|_| format!("Invalid snooze duration '{}'", input))?;
    if amount <= 0 {
        return Err(format!("Snooze duration must be positive: '{}'", input));
    }

    match unit_part {
        "m" => Ok(chrono::Duration::minutes(amount)),
        "h" => Ok(chrono::Duration::hours(amount)),
        "d" => Ok(chrono::Duration::days(amount)),
        _ => Err(format!(
            "Unsupported snooze duration unit in '{}'. Use m, h, or d.",
            input
        )),
    }
}
