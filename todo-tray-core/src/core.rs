//! Core FFI facade for Todo Tray
//!
//! This module provides the main interface exposed to Swift via UniFFI.

use crate::autostart;
use crate::config::Config;
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
    client: Arc<TodoistClient>,
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
        
        let client = Arc::new(TodoistClient::new(config.api_token));
        
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
            client,
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
    
    /// Refresh tasks from Todoist (synchronous wrapper)
    pub fn refresh(&self) -> Result<(), TodoTrayError> {
        TOKIO_RUNTIME.block_on(async {
            self.refresh_tasks().await
        })
    }
    
    /// Complete a task (synchronous wrapper)
    pub fn complete(&self, task_id: String) -> Result<(), TodoTrayError> {
        TOKIO_RUNTIME.block_on(async {
            self.complete_task(task_id).await
        })
    }
    
    /// Get the current app state
    pub fn get_state(&self) -> AppState {
        TOKIO_RUNTIME.block_on(async {
            self.state.lock().await.clone()
        })
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
        let tasks = self.client.get_tasks().await.map_err(|e| {
            TodoTrayError::Network {
                message: e.to_string(),
            }
        })?;
        
        let grouped = group_tasks(tasks);
        
        let mut state = self.state.lock().await;
        state.overdue_count = grouped.overdue.len() as u32;
        state.today_count = grouped.today.len() as u32;
        state.tomorrow_count = grouped.tomorrow.len() as u32;
        state.tasks = grouped;
        state.is_loading = false;
        state.error_message = None;
        
        let state_copy = state.clone();
        drop(state);
        
        self.event_handler.on_state_changed(state_copy);
        
        Ok(())
    }
    
    async fn complete_task(&self, task_id: String) -> Result<(), TodoTrayError> {
        // Get task name before completing
        let task_name = {
            let state = self.state.lock().await;
            state.tasks.overdue
                .iter()
                .chain(state.tasks.today.iter())
                .chain(state.tasks.tomorrow.iter())
                .find(|t| t.id == task_id)
                .map(|t| t.content.clone())
        };
        
        self.client.complete_task(&task_id).await.map_err(|e| {
            TodoTrayError::Network {
                message: e.to_string(),
            }
        })?;
        
        // Notify
        if let Some(name) = task_name {
            self.event_handler.on_task_completed(name);
        }
        
        // Refresh tasks
        self.refresh_tasks().await?;
        
        Ok(())
    }
}
