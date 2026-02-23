# Todo Tray

A macOS menubar application for Todoist with optional Linear and GitHub integrations, built in Rust.

## Features

- 🔴 Shows count of overdue tasks in the menubar
- 📋 Click to see today's tasks sorted chronologically
- ⚠️ Overdue tasks appear at the top
- ✅ Click a task to mark it as complete
- ⏱️ Todoist submenu actions: Resolve + configurable Snooze durations
- 🟦 Optional Linear integration for assigned in-progress issues
- 🐙 Optional GitHub notifications with multiple accounts
- 🔔 Notifications for newly overdue tasks
- 🔄 Auto-refreshes every 5 minutes

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
    # optional (repeat for multiple accounts)
    cat >> ~/Library/Application\ Support/todo-tray/config.toml <<'EOF'
    [[github_accounts]]
    name = "work"
    token = "ghp_..."
    EOF
    # optional
    echo 'snooze_durations = ["30m", "1d"]' >> ~/Library/Application\ Support/todo-tray/config.toml
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
# Optional: include GitHub notifications grouped by account
[[github_accounts]]
name = "work"
token = "ghp_..."
[[github_accounts]]
name = "personal"
token = "ghp_..."
# Optional: todoist submenu snooze options
snooze_durations = ["30m", "1d"]
```

## Development

```bash
# Run in development mode with logging
RUST_LOG=debug cargo run

# Build release
cargo build --release
```

## License

MIT
