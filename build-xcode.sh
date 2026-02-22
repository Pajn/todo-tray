#!/bin/bash
set -e

# Build script for Todo Tray using Xcode

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Building Todo Tray with Xcode ==="

# Generate Xcode project if needed
if [ ! -d "SwiftApp/TodoTray.xcodeproj" ]; then
    echo "Generating Xcode project..."
    cd SwiftApp
    xcodegen generate
    cd ..
fi

# Build the app
echo "Building..."
cd SwiftApp
xcodebuild -project TodoTray.xcodeproj \
    -scheme TodoTray \
    -configuration Release \
    -derivedDataPath build \
    build

echo ""
echo "=== Build Complete ==="
echo ""
echo "The app is at: SwiftApp/build/Build/Products/Release/TodoTray.app"
echo ""
echo "To run:"
echo "  open SwiftApp/build/Build/Products/Release/TodoTray.app"
