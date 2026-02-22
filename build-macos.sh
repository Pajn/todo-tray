#!/bin/bash
set -e

# Build script for Todo Tray macOS app
# This script:
# 1. Builds the Rust core library
# 2. Generates Swift bindings with UniFFI
# 3. Creates an xcframework
# 4. Copies generated Swift file
# 5. Builds the Swift app

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

CORE_DIR="todo-tray-core"
SWIFT_DIR="SwiftApp/TodoTray"
GENERATED_DIR="$SWIFT_DIR/Generated"
SOURCES_DIR="$SWIFT_DIR/Sources"

echo "=== Building Todo Tray Core ==="

# Add rust targets if not already added
rustup target add aarch64-apple-darwin 2>/dev/null || true

# Build Rust library for macOS
echo "Building Rust library for aarch64-apple-darwin..."
cargo build -p todo-tray-core --release --target aarch64-apple-darwin

# Generate Swift bindings
echo "Generating Swift bindings..."
mkdir -p "$GENERATED_DIR"
cargo run -p todo-tray-core --bin uniffi-bindgen generate \
    --library target/aarch64-apple-darwin/release/libtodo_tray_core.a \
    --language swift \
    --out-dir "$GENERATED_DIR"

# Rename modulemap
if [ -f "$GENERATED_DIR/todo_tray_coreFFI.modulemap" ]; then
    mv "$GENERATED_DIR/todo_tray_coreFFI.modulemap" "$GENERATED_DIR/module.modulemap"
fi

# Copy generated Swift file to Sources
echo "Copying generated Swift bindings..."
cp "$GENERATED_DIR/todo_tray_core.swift" "$SOURCES_DIR/"

# Create xcframework
echo "Creating xcframework..."
rm -rf "$SWIFT_DIR/todo_tray_core.xcframework"
xcodebuild -create-xcframework \
    -library target/aarch64-apple-darwin/release/libtodo_tray_core.a \
    -headers "$GENERATED_DIR" \
    -output "$SWIFT_DIR/todo_tray_core.xcframework"

echo ""
echo "=== Building Swift App ==="
cd "$SWIFT_DIR"

# Build the Swift app with the static library
LIBRARY_PATH="../todo_tray_core.xcframework/macos-arm64" swift build -c release

echo ""
echo "=== Build Complete ==="
echo ""
echo "The app binary is at: SwiftApp/TodoTray/.build/release/TodoTray"
