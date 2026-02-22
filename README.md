# Todo Tray

A macOS menubar application for Todoist with optional Linear in-progress issues, built in Rust.

## Features

- 🔴 Shows count of overdue tasks in the menubar
- 📋 Click to see today's tasks sorted chronologically
- ⚠️ Overdue tasks appear at the top
- ✅ Click a task to mark it as complete
- 🟦 Optional Linear integration for assigned in-progress issues
- 🔔 Notifications for newly overdue tasks
- 🔄 Auto-refreshes every 5 minutes

## Screenshot

```
┌─────────────────────────────────┐
│ 🔴 3                            │  ← Menubar shows overdue count
├─────────────────────────────────┤
│ ⚠️ OVERDUE                      │
│ ⚠️ Call dentist · yesterday     │  ← Click to complete
│ ⚠️ Submit report · 2d ago       │
├─────────────────────────────────┤
│ 📋 TODAY                        │
│ ☐ Team meeting · 9:00 AM        │
│ ☐ Review PR · 2:00 PM           │
│ ☐ Write docs · 4:30 PM          │
├─────────────────────────────────┤
│ 🔄 Refresh                      │
│ ❌ Quit                         │
└─────────────────────────────────┘
```

## Installation

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- macOS 11+ (Big Sur or later for SF Symbols support)

### Setup

1. **Get your Todoist API token**:
   - Go to [Todoist Integrations Settings](https://app.todoist.com/prefs/integrations)
   - Copy your API token

2. **Create the config file**:
    ```bash
    # macOS
    mkdir -p ~/Library/Application\ Support/todo-tray
    echo 'todoist_api_token = "YOUR_API_TOKEN_HERE"' > ~/Library/Application\ Support/todo-tray/config.toml
    # optional
    echo 'linear_api_token = "YOUR_LINEAR_API_KEY"' >> ~/Library/Application\ Support/todo-tray/config.toml
    ```

3. **Build and run**:
   ```bash
   cargo build --release
   ./target/release/todo-tray
   ```

### Optional: Create a macOS App Bundle

To make it a proper macOS app that can be added to login items:

```bash
cargo install cargo-bundle
cargo bundle --release
```

Then run:
```bash
open target/release/bundle/macos/Todo\ Tray.app
```

## Configuration

Config file location:
- **macOS**: `~/Library/Application Support/todo-tray/config.toml`

```toml
todoist_api_token = "your_todoist_api_token_here"
# Optional: include Linear issues assigned to you that are In Progress
linear_api_token = "your_linear_api_key_here"
```

## Development

```bash
# Run in development mode with logging
RUST_LOG=debug cargo run

# Build release
cargo build --release
```

## Architecture

```
src/
├── main.rs         # Entry point
├── config.rs       # Config file handling
├── todoist.rs      # Todoist API client
├── linear.rs       # Linear API client (assigned in-progress issues)
├── tray.rs         # Tray icon & menu management
├── notification.rs # macOS notifications
└── icon.rs         # Tray icon generation
```

## Tech Stack

- **GUI**: [tray-icon](https://docs.rs/tray-icon) + [winit](https://docs.rs/winit)
- **HTTP**: [reqwest](https://docs.rs/reqwest) (async)
- **Runtime**: [tokio](https://docs.rs/tokio)
- **Notifications**: [mac-notification-sys](https://docs.rs/mac-notification-sys)

## License

MIT
