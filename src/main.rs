//! Todo Tray - A macOS menubar app for Todoist tasks
//!
//! Features:
//! - Shows count of overdue tasks in menubar
//! - Click to see today's tasks sorted chronologically
//! - Overdue tasks appear at top
//! - Click task to mark as complete
//! - Notifications for new overdue tasks

mod autostart;
mod config;
mod icon;
mod notification;
mod todoist;
mod tray;

use anyhow::Result;
use config::Config;
use todoist::TodoistClient;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize simple logging
    tracing_subscriber::fmt::init();
    
    tracing::info!("Starting Todo Tray...");
    
    // Load config
    let config = Config::load().map_err(|e| {
        tracing::error!("Failed to load config: {}", e);
        e
    })?;
    
    tracing::info!("Config loaded successfully");
    
    // Handle autostart setting
    if config.autostart && !autostart::is_enabled() {
        if let Err(e) = autostart::enable() {
            tracing::warn!("Failed to enable autostart: {}", e);
        }
    } else if !config.autostart && autostart::is_enabled() {
        if let Err(e) = autostart::disable() {
            tracing::warn!("Failed to disable autostart: {}", e);
        }
    }
    
    // Create Todoist client
    let client = TodoistClient::new(config.api_token);
    
    // Run the tray application
    tray::run_event_loop(client)?;
    
    Ok(())
}
