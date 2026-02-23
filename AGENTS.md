# AGENTS.md - Technical Documentation for AI Agents

> This document provides technical context for AI assistants working on this codebase.

## Project Overview

**Name:** Todo Tray  
**Type:** macOS menubar application  
**Language:** Rust (core) + Swift (UI) via UniFFI  
**Purpose:** Display Todoist tasks, Linear issues, and GitHub notifications in macOS menu bar

## Architecture

This is a hybrid Rust + Swift application using UniFFI for FFI bindings:

```
┌─────────────────────────────────────────────────────────────┐
│                    Swift UI Layer                           │
│  SwiftApp/TodoTray/Sources/                                 │
│  └── StatusBarController.swift - Main UI controller         │
│  └── AppDelegate.swift - App lifecycle                      │
│  └── EventHandler.swift - Implements Rust trait             │
└───────────────────────┬─────────────────────────────────────┘
                        │ UniFFI bindings (todo_tray_core.swift)
┌───────────────────────▼─────────────────────────────────────┐
│                    Rust Core Layer                          │
│  src/                                                       │
│  ├── lib.rs         - UniFFI setup, module exports          │
│  ├── core.rs        - FFI facade (TodoTrayCore, AppState)   │
│  ├── config.rs      - Config file loading                   │
│  ├── todoist.rs     - Todoist API v1 client                 │
│  ├── github.rs      - GitHub notifications API client       │
│  ├── linear.rs      - Linear GraphQL API client             │
│  ├── task.rs        - Task data structures for FFI          │
│  └── autostart.rs   - macOS LaunchAgent management          │
└─────────────────────────────────────────────────────────────┘
```

### Data Flow

```
[Config] → [TodoTrayCore::new()]
               ↓
         [Tokio Runtime] (background thread)
               ↓
    ┌──────────┼──────────┐
    ↓          ↓          ↓
[Todoist]  [Linear]   [GitHub]
    ↓          ↓          ↓
    └──────────┼──────────┘
               ↓
         [AppState] (Arc<Mutex<AppState>>)
               ↓
         [EventHandler::on_state_changed()]
               ↓
         [Swift UI Update]
```

## Key Dependencies

### Rust Crates

| Crate | Version | Purpose |
|-------|---------|---------|
| `uniffi` | 0.28 | FFI bindings generation |
| `tokio` | 1.x | Async runtime |
| `reqwest` | 0.12 | HTTP client with JSON |
| `serde` | 1.x | JSON/TOML serialization |
| `chrono` | 0.4 | Date/time handling |
| `dirs` | 5.x | XDG config paths |
| `thiserror` | 1.x | Error types |
| `anyhow` | 1.x | Error handling |

### Swift Frameworks

- `Cocoa` - AppKit UI
- `UserNotifications` - macOS notifications
- `SystemConfiguration` - Network status
- `Security` - Keychain (future)

## FFI Layer (UniFFI)

### Core Types Exposed to Swift

```rust
// Main entry point - created once at app launch
#[derive(uniffi::Object)]
pub struct TodoTrayCore { ... }

// Application state - passed to Swift on each update
#[derive(uniffi::Record, Clone)]
pub struct AppState {
    pub overdue_count: u32,
    pub today_count: u32,
    pub tomorrow_count: u32,
    pub in_progress_count: u32,
    pub github_notification_count: u32,
    pub tasks: TaskList,
    pub github_notifications: Vec<GithubNotificationSection>,
    pub snooze_durations: Vec<String>,
    pub is_loading: bool,
    pub error_message: Option<String>,
    pub autostart_enabled: bool,
}

// Callback trait implemented in Swift
#[uniffi::export(with_foreign)]
pub trait EventHandler: Send + Sync {
    fn on_state_changed(&self, state: AppState);
    fn on_task_completed(&self, task_name: String);
    fn on_error(&self, error: String);
}
```

### Swift Implementation Pattern

```swift
// EventHandler implementation in Swift
class EventHandler: TodoTrayEventHandler {
    weak var controller: StatusBarController?
    
    func onStateChanged(_ state: AppState) {
        DispatchQueue.main.async { [weak self] in
            self?.controller?.updateFromState(state)
        }
    }
    
    func onTaskCompleted(_ taskName: String) {
        // Show notification
    }
    
    func onError(_ error: String) {
        // Handle error
    }
}
```

## API Details

### Todoist API v1

**Base URL:** `https://api.todoist.com/api/v1`

```rust
// Filter endpoint with pagination
GET /tasks/filter?query=today%20|%20overdue%20|%20tomorrow&limit=100
Authorization: Bearer {token}

// Response
{
  "results": [{ "id": "123", "content": "Task", "due": { "date": "2026-02-20T14:00:00" } }],
  "next_cursor": null
}

// Complete task
POST /tasks/{id}/close
```

### GitHub API

**Base URL:** `https://api.github.com`

```rust
// Get notifications (paginated)
GET /notifications?all=false&per_page=50
Authorization: Bearer {token}
Accept: application/vnd.github+json
X-GitHub-Api-Version: 2022-11-28

// Mark as read
PATCH /notifications/threads/{thread_id}
```

### Linear GraphQL API

```rust
POST https://api.linear.app/graphql
Authorization: {token}

query AssignedIssues {
  viewer {
    assignedIssues(first: 50) {
      nodes { id, identifier, title, dueDate, state { name, type } }
    }
  }
}
```

## Build System

### Justfile Commands

```bash
just build-core          # Build Rust library
just build-core-release  # Build Rust (release)
just build-app           # Build complete app via Xcode
just rebuild             # Clean + full rebuild
just run                 # Build and open app
just fresh               # Rebuild and run
just lint                # Run clippy
just setup-config        # Create config template
```

### Xcode Build Process

The `SwiftApp/project.yml` defines a preBuildScript that:

1. Builds Rust library: `cargo build --release --target aarch64-apple-darwin`
2. Generates Swift bindings: `cargo run --bin uniffi-bindgen generate`
3. Creates xcframework from the static library

### Build Configuration

- **Target:** aarch64-apple-darwin (Apple Silicon)
- **Release profile:** Optimized for size (`opt-level = 'z'`, LTO, strip symbols)
- **Output:** `SwiftApp/build/Build/Products/Release/TodoTray.app`

## Configuration

**Location:** `~/Library/Application Support/todo-tray/config.toml`

```toml
todoist_api_token = "your_token"

# Optional: Linear in-progress issues
linear_api_token = "lin_api_..."

# Optional: GitHub notifications (multiple accounts supported)
[[github_accounts]]
name = "work"
token = "ghp_..."

# Optional: Snooze duration options
snooze_durations = ["30m", "1d"]

# Optional: Auto-launch at login
autostart = true
```

## Threading Model

1. **Main Thread:** Swift UI, AppKit event loop
2. **Background Thread:** Tokio runtime for async operations
3. **State Updates:** Via `Arc<Mutex<AppState>>`, callbacks to Swift on state change

```rust
// Runtime initialization (happens once)
static TOKIO_RUNTIME: LazyLock<tokio::runtime::Runtime> = ...;

// Background refresh loop
std::thread::spawn(move || {
    TOKIO_RUNTIME.block_on(async {
        // Initial refresh
        refresh_tasks(&core).await;
        // Then every 5 minutes
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            refresh_tasks(&core).await;
        }
    });
});
```

## Common Tasks

### Adding a New API Integration

1. Create new module in `src/` (e.g., `jira.rs`)
2. Add client struct and async methods
3. Create FFI-compatible data structures with `#[uniffi::Record]`
4. Update `core.rs` to include in `refresh_tasks()`
5. Add config fields in `config.rs`
6. Update Swift UI to display new data

### Modifying Task Display

- **Rust side:** Edit `task.rs` (TodoTask struct, display_time formatting)
- **Swift side:** Edit `StatusBarController.swift` (rebuildMenu function)

### Adding Menu Actions

1. Add method to `TodoTrayCore` in `core.rs` with `#[uniffi::export]`
2. Call from Swift in menu item handler

## Error Handling

- Uses `TodoTrayError` enum for FFI-safe errors
- Errors logged via `tracing`
- User-facing errors shown via `EventHandler::on_error()`
- Network errors don't crash the app

## Platform Notes

- **macOS only** - Uses AppKit for native UI
- **Apple Silicon** - Built for aarch64-apple-darwin
- **Dark mode** - Menu adapts to system appearance
- **Code signing** - Required for distribution

## LSP Notes

Types like `TodoTrayCore`, `AppState`, `TodoTask` are generated by UniFFI at compile time. LSP errors for these types during static analysis are expected and do not affect compilation.
