#!/bin/bash
set -e

# Setup script for Todo Tray Xcode project

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Setting up Todo Tray Xcode Project ==="

# Check if xcodegen is installed
if ! command -v xcodegen &> /dev/null; then
    echo "Installing xcodegen..."
    brew install xcodegen
fi

# Generate the Xcode project
echo "Generating Xcode project..."
cd SwiftApp
xcodegen generate

echo ""
echo "=== Setup Complete ==="
echo ""
echo "The Xcode project has been generated at:"
echo "  SwiftApp/TodoTray.xcodeproj"
echo ""
echo "To open in Xcode:"
echo "  open SwiftApp/TodoTray.xcodeproj"
echo ""
echo "Or use the build script:"
echo "  ./build-xcode.sh"
