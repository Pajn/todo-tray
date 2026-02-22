//! Core FFI facade for Todo Tray
//!
//! This module provides the main interface exposed to Swift via UniFFI.

use crate::autostart;
use crate::config::Config;
use crate::linear::LinearClient;
use crate::task::{group_tasks, TaskList};
use crate::todoist::TodoistClient;
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
    pub tasks: TaskList,
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
    event_handler: Arc<dyn EventHandler>,
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
                ..Default::default()
            })),
            todoist_client,
            linear_client,
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
                if let Err(e) = core_clone.refresh_tasks().await {
                    eprintln!("[Rust] Initial refresh failed: {}", e);
                }
                eprintln!("[Rust] Initial refresh complete");

                // Refresh every 5 minutes
                let mut interval = tokio::time::interval(Duration::from_secs(300));
                loop {
                    interval.tick().await;
                    if let Err(e) = core_clone.refresh_tasks().await {
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
        TOKIO_RUNTIME.block_on(async { self.refresh_tasks().await })
    }

    /// Complete a task (synchronous wrapper)
    pub fn complete(&self, task_id: String) -> Result<(), TodoTrayError> {
        TOKIO_RUNTIME.block_on(async { self.complete_task(task_id).await })
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

    // Internal async implementations

    async fn refresh_tasks(&self) -> Result<(), TodoTrayError> {
        let todoist = self.todoist_client.get_tasks();
        let linear = async {
            match &self.linear_client {
                Some(client) => client.get_in_progress_issues().await.map(Some),
                None => Ok(None),
            }
        };
        let (mut tasks, linear_tasks) =
            tokio::try_join!(todoist, linear).map_err(|e| TodoTrayError::Network {
                message: e.to_string(),
            })?;

        if let Some(mut linear_tasks) = linear_tasks {
            tasks.append(&mut linear_tasks);
        }

        let grouped = group_tasks(tasks);

        let mut state = self.state.lock().await;
        state.overdue_count = grouped.overdue.len() as u32;
        state.today_count = grouped.today.len() as u32;
        state.tomorrow_count = grouped.tomorrow.len() as u32;
        state.in_progress_count = grouped.in_progress.len() as u32;
        state.tasks = grouped;
        state.is_loading = false;
        state.error_message = None;

        let state_copy = state.clone();
        drop(state);

        self.event_handler.on_state_changed(state_copy);

        Ok(())
    }

    async fn complete_task(&self, task_id: String) -> Result<(), TodoTrayError> {
        // Lookup the task first so we can block completion for non-Todoist sources.
        let selected_task = {
            let state = self.state.lock().await;
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
                message: "This task is read-only and cannot be completed from Todo Tray."
                    .to_string(),
            });
        }

        self.todoist_client
            .complete_task(&task_id)
            .await
            .map_err(|e| TodoTrayError::Network {
                message: e.to_string(),
            })?;

        // Notify
        self.event_handler.on_task_completed(task_name);

        // Refresh tasks
        self.refresh_tasks().await?;

        Ok(())
    }
}
