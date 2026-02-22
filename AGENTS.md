# AGENTS.md - Technical Documentation for AI Agents

> This document provides technical context for AI assistants working on this codebase.

## Project Overview

**Name:** Todo Tray  
**Type:** macOS menubar application  
**Language:** Rust (Edition 2021)  
**Purpose:** Display Todoist tasks in macOS menu bar with overdue tracking

## Architecture

```
src/
├── main.rs         # Entry point, initializes tokio runtime and starts event loop
├── config.rs       # Config file loading from ~/.config/todo-tray/config.toml
├── todoist.rs      # Todoist API v1 client (fetch tasks, complete tasks)
├── tray.rs         # System tray icon, menu building, event handling
├── notification.rs # macOS notifications via mac-notification-sys
└── icon.rs         # Dynamic tray icon generation (RGBA pixel buffer)
```

### Data Flow

```
[Config] → [TodoistClient] ←──→ [Todoist API v1]
                ↓
           [TrayState] (Arc<Mutex<TrayState>>)
                ↓
          [TrayIcon] ←→ [Menu]
                ↓
           [User Click] → [Complete Task] → [API] → [Refresh]
```

## Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `tokio` | 1.x | Async runtime (full features) |
| `reqwest` | 0.12 | HTTP client with JSON support |
| `tray-icon` | 0.19 | System tray icon with menu |
| `winit` | 0.30 | Event loop (required by tray-icon on macOS) |
| `muda` | 0.15 | Menu utilities (re-exported by tray-icon) |
| `chrono` | 0.4 | Date/time handling with timezone support |
| `serde` | 1.x | JSON/TOML serialization |
| `mac-notification-sys` | 0.6 | Native macOS notifications |
| `anyhow` | 1.x | Error handling |
| `dirs` | 5.x | XDG config directory paths |

## API Details

### Todoist API v1 Endpoints

**Base URL:** `https://api.todoist.com/api/v1`

**Authentication:** Bearer token in `Authorization` header

```rust
// Get tasks (GET with filter query parameter)
GET /tasks/filter?query=today%20|%20overdue
Authorization: Bearer {token}

// Response: Paginated with "results" field
{
  "results": [
    {
      "id": "12345",
      "content": "Task name",
      "due": {
        "date": "2026-02-20T14:00:00",  // Can be datetime or just date "YYYY-MM-DD"
        "timezone": null,
        "string": "tomorrow at 2pm",
        "is_recurring": false
      }
      // Note: is_overdue is NOT returned - calculate from due date vs now
    }
  ],
  "next_cursor": null  // For pagination
}

// Complete a task
POST /tasks/{task_id}/close
Authorization: Bearer {token}
// Returns 204 No Content on success
```

### Task Data Structure

```rust
pub struct Task {
    pub id: String,
    pub content: String,
    pub due_datetime: Option<DateTime<Utc>>,
    pub is_overdue: bool,
}

// Sorting: overdue first, then chronologically by due_datetime
pub fn sort_tasks(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| {
        match (a.is_overdue, b.is_overdue) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => compare_by_datetime(a, b)
        }
    });
}
```

## Event Loop Architecture

### Main Thread Constraints (macOS)

On macOS, the tray icon and menu **must** be created on the main thread. This requires careful coordination between:

1. **winit EventLoop** - Runs on main thread, handles menu events
2. **tokio Runtime** - Handles async API calls on worker threads
3. **Cross-thread communication** - Via two types of channels:
   - `std::sync::mpsc` for event handlers (synchronous callbacks from Cocoa)
   - `tokio::sync::mpsc` for async task results

```rust
// Use std::sync::mpsc for event handlers (they run outside tokio runtime)
let (event_tx, event_rx) = std::sync::mpsc::channel::<TrayCommand>();

// Use tokio::sync::mpsc for async → main thread updates
let (update_tx, mut update_rx) = mpsc::channel::<TrayUpdate>(32);

// Menu events forwarded via std::sync::mpsc (non-blocking)
MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
    let _ = event_tx.send(parse_event(event));  // std::sync::mpsc::Sender::send
}));

// In event loop, poll both channels
while let Ok(cmd) = event_rx.try_recv() {
    handle_command(cmd, ...);
}
while let Ok(update) = update_rx.try_recv() {
    handle_update(update, ...);
}
```

### Thread Safety

- `TrayState` is wrapped in `Arc<Mutex<_>>` for shared access
- `TodoistClient` is wrapped in `Arc` for cheap cloning
- `TrayIcon` is NOT `Send` - must stay on main thread
- Async tasks spawn with `tokio::spawn` and send results via channel

## Menu Building

Menus are rebuilt on each refresh using `muda`:

```rust
fn build_menu(tasks: &[Task]) -> Menu {
    let menu = Menu::new();
    
    // Headers are non-interactive
    let header = MenuItemBuilder::new()
        .text("⚠️ OVERDUE")
        .enabled(false)
        .id(MenuId::new("header"))
        .build();
    
    // Task items have task ID as menu ID
    let item = MenuItemBuilder::new()
        .text("☐ Task name · 2:00 PM")
        .enabled(true)
        .id(MenuId::new("task_id_here"))
        .build();
    
    menu.append(&item);
    menu
}
```

## Configuration

**Location (macOS):** `~/Library/Application Support/todo-tray/config.toml`

```toml
todoist_api_token = "your_todoist_api_token"
```

**Loading:**
```rust
impl Config {
    pub fn load() -> Result<Self> {
        let path = dirs::config_dir()
            .join("todo-tray")
            .join("config.toml");
        let content = fs::read_to_string(&path)?;
        toml::from_str(&content)
    }
}
```

## Notifications

Uses `mac-notification-sys` for native macOS notifications:

```rust
// Simple notification
mac_notification_sys::send_notification(
    "Title",
    Some("Subtitle"),
    "Message body",
    None,  // No custom Notification options
);
```

## Common Tasks

### Adding a New Menu Action

1. Add variant to `TrayCommand` enum in `tray.rs`
2. Add menu item with unique ID in `build_menu()`
3. Handle in `parse_menu_event()` and `handle_command()`

### Adding a New API Endpoint

1. Add method to `TodoistClient` in `todoist.rs`
2. Use `self.client` (reqwest::Client) for HTTP
3. Parse response with `serde`

### Modifying Task Display

1. Edit `format_task_menu_item()` in `icon.rs`
2. Or edit `format_tray_title()` for menubar display

## Build Commands

```bash
# Development
just build          # Debug build
just run            # Run debug
just run-debug      # Run with RUST_LOG=debug

# Production
just build-release  # Optimized build
just bundle         # Create .app bundle
just open-bundle    # Build and open the app

# Quality
just lint           # Run clippy
just fmt            # Format code
just ci             # All checks

# Setup
just setup-config   # Create config template
```

## Error Handling

- Uses `anyhow::Result` throughout
- Errors are logged via `tracing`
- User-facing errors shown in tray tooltip
- API errors don't crash the app - graceful degradation

## Logging

```bash
# Enable debug logging
RUST_LOG=debug cargo run

# Trace all events
RUST_LOG=trace cargo run

# Module-specific
RUST_LOG=todo_tray::todoist=debug cargo run
```

## Platform Notes

- **macOS only** - Uses macOS-specific APIs for tray and notifications
- **Requires main thread** - Event loop must run on main thread
- **Dark mode** - Tray icon is gray to work in both modes
- **Code signing** - For distribution, the .app needs to be signed

## Future Enhancements

Potential areas for improvement:

1. **Keychain storage** - Store API token securely in macOS Keychain
2. **Multiple accounts** - Support switching between Todoist accounts
3. **Custom refresh interval** - Configurable in config.toml
4. **Task priorities** - Show priority colors in menu
5. **Projects** - Filter by project
6. **Keyboard shortcuts** - Global hotkey to show menu
