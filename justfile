# Todo Tray - Justfile
# Install just: cargo install just

# Default: show available commands
default:
    @just --list

# Build the Rust core library
build-core:
    cargo build

# Build the Rust core library (release)
build-core-release:
    cargo build --release

# Generate Xcode project (requires xcodegen)
gen-xcode:
    cd SwiftApp && xcodegen generate

# Build the macOS app (incremental - may use cached Rust code)
build-app:
    ./build-xcode.sh

# Force a complete rebuild from scratch (recommended when Rust code changed)
rebuild: kill clean
    ./build-xcode.sh

# Build and run (incremental)
run: build-app
    open SwiftApp/build/Build/Products/Release/TodoTray.app

# Rebuild from scratch and run (use this when Rust code changed)
fresh: rebuild
    open SwiftApp/build/Build/Products/Release/TodoTray.app

# Open the built app (no rebuild)
open-bundle:
    open SwiftApp/build/Build/Products/Release/TodoTray.app

# Run tests
test:
    cargo test

# Check for compilation errors
check:
    cargo check

# Check with all warnings
check-all:
    RUSTFLAGS="-D warnings" cargo check

# Format code
fmt:
    cargo fmt

# Check formatting
fmt-check:
    cargo fmt --check

# Run clippy linter
lint:
    cargo clippy -- -D warnings

# Run all quality checks
ci: fmt-check lint test
    cargo check

# Create config directory and template
setup-config:
    mkdir -p ~/Library/Application\ Support/todo-tray
    @echo 'todoist_api_token = "YOUR_API_TOKEN_HERE"' > ~/Library/Application\ Support/todo-tray/config.toml
    @echo '# Optional: Linear API key for assigned in-progress issues' >> ~/Library/Application\ Support/todo-tray/config.toml
    @echo '# linear_api_token = "lin_api_..."' >> ~/Library/Application\ Support/todo-tray/config.toml
    @echo '# Optional: GitHub notifications (repeat block for multiple accounts)' >> ~/Library/Application\ Support/todo-tray/config.toml
    @echo '# [[github_accounts]]' >> ~/Library/Application\ Support/todo-tray/config.toml
    @echo '# name = "work"' >> ~/Library/Application\ Support/todo-tray/config.toml
    @echo '# token = "ghp_..."' >> ~/Library/Application\ Support/todo-tray/config.toml
    @echo '# Optional: todoist submenu snooze durations (default: ["30m", "1d"])' >> ~/Library/Application\ Support/todo-tray/config.toml
    @echo '# snooze_durations = ["30m", "1d"]' >> ~/Library/Application\ Support/todo-tray/config.toml
    @echo "Config created at ~/Library/Application Support/todo-tray/config.toml"
    @echo "Edit it with your Todoist API token from:"
    @echo "  https://app.todoist.com/prefs/integrations"

# Show config location
config-path:
    @echo "Config file: ~/Library/Application Support/todo-tray/config.toml"

# Install xcodegen (required for building)
install-xcodegen:
    brew install xcodegen

# Clean all build artifacts
clean:
    cargo clean
    rm -rf SwiftApp/build SwiftApp/TodoTray/todo_tray_core.xcframework SwiftApp/TodoTray/Generated

# Update dependencies
update:
    cargo update

# Show dependency tree
deps:
    cargo tree

# Kill any running TodoTray instances
kill:
    killall TodoTray 2>/dev/null || true
