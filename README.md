# Todo Tray

A macOS menubar application for Todoist with optional Linear and GitHub integrations.

Built with Rust (core logic) + Swift (native UI) via UniFFI.

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

## Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- Xcode 15+ with Xcode Command Line Tools
- [xcodegen](https://github.com/yonaskolb/Xcodegen) (`brew install xcodegen`)
- [just](https://github.com/casey/just) (`brew install just`)

## Installation

### 1. Get your Todoist API token

Go to [Todoist Integrations Settings](https://app.todoist.com/prefs/integrations) and copy your API token.

### 2. Create the config file

```bash
just setup-config
# Then edit the config file with your token:
open -e ~/Library/Application\ Support/todo-tray/config.toml
```

Or manually:

```bash
mkdir -p ~/Library/Application\ Support/todo-tray
cat > ~/Library/Application\ Support/todo-tray/config.toml << 'EOF'
todoist_api_token = "YOUR_API_TOKEN_HERE"

# Optional: Linear in-progress issues
# linear_api_token = "lin_api_..."

# Optional: GitHub notifications (repeat block for multiple accounts)
# [[github_accounts]]
# name = "work"
# token = "ghp_..."

# Optional: Snooze durations (default: 30m, 1d)
# snooze_durations = ["30m", "1d"]
EOF
```

### 3. Build and run

```bash
just run
```

## Development

```bash
just build-core     # Build Rust library only
just build-app      # Build complete app
just rebuild        # Clean + full rebuild (use when Rust code changed)
just run            # Build and open app
just fresh          # Rebuild and run
just lint           # Run clippy
just fmt            # Format code
```

## Configuration

Config file location: `~/Library/Application Support/todo-tray/config.toml`

```toml
todoist_api_token = "your_todoist_api_token"

# Optional: include Linear issues assigned to you that are In Progress
linear_api_token = "your_linear_api_key"

# Optional: GitHub notifications grouped by account
[[github_accounts]]
name = "work"
token = "ghp_..."

[[github_accounts]]
name = "personal"
token = "ghp_..."

# Optional: todoist snooze options
snooze_durations = ["30m", "1d"]

# Optional: auto-launch at login
autostart = true
```

## Architecture

- **Rust Core** (`src/`): Business logic, API clients, state management
- **Swift UI** (`SwiftApp/TodoTray/Sources/`): Native AppKit menubar app
- **UniFFI**: Generates Swift bindings from Rust types

## License

MIT
