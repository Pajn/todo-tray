//! Tray application using tray-icon.
//!
//! This module implements the status bar item and menu.

use crate::autostart;
use crate::todoist::{sort_tasks, Task, TodoistClient};
use crate::{icon, notification};
use anyhow::Result;
use chrono::{Local, Timelike};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuId, MenuItemBuilder, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};
use winit::{
    event::Event,
    event_loop::EventLoop,
};

/// Commands from the event loop
#[derive(Debug, Clone)]
pub enum TrayCommand {
    RefreshTasks,
    CompleteTask(String),
    ToggleAutostart,
    Quit,
}

/// Shared state for the tray application
pub struct TrayState {
    pub tasks: Vec<Task>,
    pub overdue_count: usize,
    pub today_count: usize,
    pub tomorrow_count: usize,
    pub previous_overdue_count: usize,
    pub previous_overdue_names: Vec<String>,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            tasks: Vec::new(),
            overdue_count: 0,
            today_count: 0,
            tomorrow_count: 0,
            previous_overdue_count: 0,
            previous_overdue_names: Vec::new(),
        }
    }
}

pub fn run_event_loop(client: TodoistClient) -> Result<()> {
    // Create event loop with user event support
    let event_loop = EventLoop::<TrayCommand>::with_user_event().build()?;
    
    // Use std::sync::mpsc for event handlers (they run outside tokio runtime)
    let (event_tx, event_rx) = std::sync::mpsc::channel::<TrayCommand>();
    
    // Use tokio::sync::mpsc for async task communication
    let (update_tx, mut update_rx) = mpsc::channel::<TrayUpdate>(32);
    
    // Shared state
    let state = Arc::new(Mutex::new(TrayState::default()));
    let client = Arc::new(client);
    
    // Set up menu event handler - uses std::sync::mpsc
    let event_tx_clone = event_tx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let cmd = parse_menu_event(&event.id.0);
        let _ = event_tx_clone.send(cmd);
    }));
    
    // Create initial tray icon
    let tray_icon = create_tray_icon()?;
    let tray = TrayIconBuilder::new()
        .with_tooltip("Todo Tray - Loading...")
        .with_icon(tray_icon)
        .build()?;
    
    // Spawn background task for initial refresh and periodic updates
    let client_clone = client.clone();
    let update_tx_clone = update_tx.clone();
    tokio::spawn(async move {
        // Initial fetch
        fetch_and_send_update(&client_clone, &update_tx_clone).await;
        
        // Refresh every 5 minutes
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            fetch_and_send_update(&client_clone, &update_tx_clone).await;
        }
    });
    
    // Run the event loop
    event_loop.run(move |event, elwt| {
        match event {
            Event::UserEvent(cmd) => {
                handle_command(cmd, &client, &state, &tray, &update_tx);
            }
            Event::AboutToWait => {
                // Process events from std::sync::mpsc (non-blocking)
                while let Ok(cmd) = event_rx.try_recv() {
                    handle_command(cmd, &client, &state, &tray, &update_tx);
                }
                
                // Process updates from async tasks
                while let Ok(update) = update_rx.try_recv() {
                    match update {
                        TrayUpdate::TasksFetched(mut tasks) => {
                            sort_tasks(&mut tasks);
                            let mut s = state.lock().unwrap();
                            
                            // Check for new overdue tasks
                            let new_overdue: Vec<_> = tasks
                                .iter()
                                .filter(|t| t.is_overdue)
                                .map(|t| t.content.clone())
                                .collect();
                            
                            let new_overdue_count = new_overdue.len();
                            
                            // Notify if more overdue tasks than before
                            if new_overdue_count > s.previous_overdue_count {
                                let new_task_names: Vec<_> = new_overdue
                                    .iter()
                                    .skip(s.previous_overdue_count)
                                    .cloned()
                                    .collect();
                                let _ = notification::notify_overdue(
                                    new_overdue_count - s.previous_overdue_count,
                                    &new_task_names,
                                );
                            }
                            
                            s.overdue_count = new_overdue_count;
                            s.today_count = tasks.iter().filter(|t| t.is_today() && !t.is_overdue).count();
                            s.tomorrow_count = tasks.iter().filter(|t| t.is_tomorrow()).count();
                            s.previous_overdue_count = new_overdue_count;
                            s.previous_overdue_names = new_overdue;
                            s.tasks = tasks;
                            
                            // Update tray
                            update_tray(&tray, &s);
                        }
                        TrayUpdate::TaskCompleted(task_name) => {
                            let _ = notification::notify_task_completed(&task_name);
                        }
                        TrayUpdate::Error(e) => {
                            tracing::error!("Error: {}", e);
                            let _ = tray.set_tooltip(Some(&format!("Todo Tray - Error: {}", e)));
                        }
                    }
                }
            }
            Event::LoopExiting => {
                elwt.exit();
            }
            _ => {}
        }
    })?;
    
    Ok(())
}

enum TrayUpdate {
    TasksFetched(Vec<Task>),
    TaskCompleted(String),
    Error(String),
}

async fn fetch_and_send_update(client: &Arc<TodoistClient>, tx: &mpsc::Sender<TrayUpdate>) {
    match client.get_today_tasks().await {
        Ok(tasks) => {
            let _ = tx.send(TrayUpdate::TasksFetched(tasks)).await;
        }
        Err(e) => {
            let _ = tx.send(TrayUpdate::Error(e.to_string())).await;
        }
    }
}

fn handle_command(
    cmd: TrayCommand,
    client: &Arc<TodoistClient>,
    state: &Arc<Mutex<TrayState>>,
    tray: &TrayIcon,
    update_tx: &mpsc::Sender<TrayUpdate>,
) {
    match cmd {
        TrayCommand::RefreshTasks => {
            let client = client.clone();
            let tx = update_tx.clone();
            tokio::spawn(async move {
                fetch_and_send_update(&client, &tx).await;
            });
        }
        TrayCommand::CompleteTask(task_id) => {
            let client = client.clone();
            let tx = update_tx.clone();
            let state = state.clone();
            
            tokio::spawn(async move {
                // Get task name before completing
                let task_name = {
                    let s = state.lock().unwrap();
                    s.tasks
                        .iter()
                        .find(|t| t.id == task_id)
                        .map(|t| t.content.clone())
                };
                
                if let Some(name) = task_name {
                    match client.complete_task(&task_id).await {
                        Ok(()) => {
                            let _ = tx.send(TrayUpdate::TaskCompleted(name)).await;
                            // Refresh tasks
                            fetch_and_send_update(&client, &tx).await;
                        }
                        Err(e) => {
                            let _ = tx.send(TrayUpdate::Error(format!("Failed to complete task: {}", e))).await;
                        }
                    }
                }
            });
        }
        TrayCommand::ToggleAutostart => {
            if autostart::is_enabled() {
                if let Err(e) = autostart::disable() {
                    tracing::error!("Failed to disable autostart: {}", e);
                }
            } else if let Err(e) = autostart::enable() {
                tracing::error!("Failed to enable autostart: {}", e);
            }
            // Rebuild menu to reflect new state
            let s = state.lock().unwrap();
            let menu = build_menu(&s.tasks, autostart::is_enabled());
            let _ = tray.set_menu(Some(Box::new(menu)));
        }
        TrayCommand::Quit => {
            std::process::exit(0);
        }
    }
}

fn parse_menu_event(id: &str) -> TrayCommand {
    match id {
        "refresh" => TrayCommand::RefreshTasks,
        "toggle_autostart" => TrayCommand::ToggleAutostart,
        "quit" => TrayCommand::Quit,
        task_id if !task_id.is_empty() && task_id != "header" => {
            TrayCommand::CompleteTask(task_id.to_string())
        }
        _ => TrayCommand::RefreshTasks,
    }
}

fn update_tray(tray: &TrayIcon, state: &TrayState) {
    // Update title/icon
    let title = icon::format_tray_title(state.overdue_count, state.today_count);
    let _ = tray.set_title(Some(&title));
    let _ = tray.set_tooltip(Some(&format!(
        "Todo Tray - {} overdue, {} today",
        state.overdue_count, state.today_count
    )));
    
    // Build menu with current autostart state
    let autostart_enabled = autostart::is_enabled();
    let menu = build_menu(&state.tasks, autostart_enabled);
    let _ = tray.set_menu(Some(Box::new(menu)));
}

fn build_menu(tasks: &[Task], autostart_enabled: bool) -> Menu {
    let menu = Menu::new();
    
    // Separate overdue, today, and tomorrow tasks
    let overdue: Vec<_> = tasks.iter().filter(|t| t.is_overdue).collect();
    let today: Vec<_> = tasks
        .iter()
        .filter(|t| t.is_today() && !t.is_overdue)
        .collect();
    let tomorrow: Vec<_> = tasks.iter().filter(|t| t.is_tomorrow()).collect();
    
    // Check if we should show tomorrow section (after noon)
    let show_tomorrow = Local::now().hour() >= 12;
    
    // Overdue section
    if !overdue.is_empty() {
        let header = MenuItemBuilder::new()
            .text("Overdue")
            .enabled(false)
            .id(MenuId::new("header"))
            .build();
        let _ = menu.append(&header);
        
        for task in overdue {
            let item = MenuItemBuilder::new()
                .text(icon::format_task_menu_item(task))
                .enabled(true)
                .id(MenuId::new(&task.id))
                .build();
            let _ = menu.append(&item);
        }
        let _ = menu.append(&PredefinedMenuItem::separator());
    }
    
    // Today section
    if !today.is_empty() {
        let header = MenuItemBuilder::new()
            .text("Today")
            .enabled(false)
            .id(MenuId::new("header"))
            .build();
        let _ = menu.append(&header);
        
        for task in today {
            let item = MenuItemBuilder::new()
                .text(icon::format_task_menu_item(task))
                .enabled(true)
                .id(MenuId::new(&task.id))
                .build();
            let _ = menu.append(&item);
        }
        let _ = menu.append(&PredefinedMenuItem::separator());
    }
    
    // Tomorrow section (only after noon)
    if show_tomorrow && !tomorrow.is_empty() {
        let header = MenuItemBuilder::new()
            .text("Tomorrow")
            .enabled(false)
            .id(MenuId::new("header"))
            .build();
        let _ = menu.append(&header);
        
        for task in tomorrow {
            let item = MenuItemBuilder::new()
                .text(icon::format_task_menu_item(task))
                .enabled(true)
                .id(MenuId::new(&task.id))
                .build();
            let _ = menu.append(&item);
        }
        let _ = menu.append(&PredefinedMenuItem::separator());
    }
    
    // No tasks message
    if tasks.is_empty() {
        let item = MenuItemBuilder::new()
            .text("No tasks for today")
            .enabled(false)
            .id(MenuId::new("header"))
            .build();
        let _ = menu.append(&item);
        let _ = menu.append(&PredefinedMenuItem::separator());
    }
    
    // Controls
    let refresh_item = MenuItemBuilder::new()
        .text("Refresh")
        .enabled(true)
        .id(MenuId::new("refresh"))
        .build();
    let _ = menu.append(&refresh_item);
    
    // Autostart toggle
    let autostart_text = if autostart_enabled {
        "✓ Autostart"
    } else {
        "Autostart"
    };
    let autostart_item = MenuItemBuilder::new()
        .text(autostart_text)
        .enabled(true)
        .id(MenuId::new("toggle_autostart"))
        .build();
    let _ = menu.append(&autostart_item);
    
    let quit_item = MenuItemBuilder::new()
        .text("Quit")
        .enabled(true)
        .id(MenuId::new("quit"))
        .build();
    let _ = menu.append(&quit_item);
    
    menu
}

fn create_tray_icon() -> Result<tray_icon::Icon> {
    let rgba = icon::generate_tray_icon();
    tray_icon::Icon::from_rgba(rgba, 22, 22)
        .map_err(|e| anyhow::anyhow!("Failed to create icon: {}", e))
}
