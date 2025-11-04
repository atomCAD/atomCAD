#!/bin/bash

# atomCAD macOS Release Build Script
# 
# This script builds atomCAD for macOS and creates a distributable zip file.
# Output: dist/atomCAD-macos-v[VERSION].zip containing all necessary files for distribution.
#
# Usage Examples:
#   ./build_macos_release.sh                           # Build with default version (1.0.0)
#   ./build_macos_release.sh --version "1.2.3"        # Build with specific version
#   ./build_macos_release.sh --version "2.0.0" --skip-flutter-clean  # Skip clean for faster builds
#   ./build_macos_release.sh --skip-rust-build        # Skip Rust build if already built
#
# The created zip file contains the complete macOS application ready for distribution.

set -e  # Exit on any error

# Default values
VERSION="1.0.0"
SKIP_RUST_BUILD=false
SKIP_FLUTTER_CLEAN=false

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --version)
            VERSION="$2"
            shift 2
            ;;
        --skip-rust-build)
            SKIP_RUST_BUILD=true
            shift
            ;;
        --skip-flutter-clean)
            SKIP_FLUTTER_CLEAN=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [--version VERSION] [--skip-rust-build] [--skip-flutter-clean]"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo "=== atomCAD macOS Release Build ==="
echo "Version: $VERSION"
echo ""

# Get project root directory
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$PROJECT_ROOT"

# Step 1: Build Rust backend in release mode
if [ "$SKIP_RUST_BUILD" = false ]; then
    echo "Building Rust backend (release mode)..."
    cd rust
    cargo build --release
    if [ $? -ne 0 ]; then
        echo "Error: Rust build failed"
        exit 1
    fi
    cd ..
    echo "Rust build completed successfully"
    echo ""
else
    echo "Skipping Rust build (skip-rust-build specified)"
    echo ""
fi

# Step 2: Clean Flutter build (optional)
if [ "$SKIP_FLUTTER_CLEAN" = false ]; then
    echo "Cleaning Flutter build cache..."
    flutter clean
    echo "Flutter clean completed"
    echo ""
else
    echo "Skipping Flutter clean (skip-flutter-clean specified)"
    echo ""
fi

# Step 3: Get Flutter dependencies
echo "Getting Flutter dependencies..."
flutter pub get
if [ $? -ne 0 ]; then
    echo "Error: Flutter pub get failed"
    exit 1
fi
echo "Flutter dependencies updated"
echo ""

# Step 4: Build Flutter macOS release
echo "Building Flutter macOS release..."
flutter build macos --release
if [ $? -ne 0 ]; then
    echo "Error: Flutter macOS build failed"
    exit 1
fi
echo "Flutter macOS build completed"
echo ""

# Step 5: Verify release files exist
RELEASE_PATH="build/macos/Build/Products/Release"
APP_PATH="$RELEASE_PATH/atomCAD.app"

if [ ! -d "$RELEASE_PATH" ]; then
    echo "Error: Release directory not found: $RELEASE_PATH"
    exit 1
fi

if [ ! -d "$APP_PATH" ]; then
    echo "Error: Application bundle not found: $APP_PATH"
    exit 1
fi

echo "Release files verified"
echo ""

# Step 6: Create zip archive
DIST_DIR="$PROJECT_ROOT/dist"
if [ ! -d "$DIST_DIR" ]; then
    mkdir -p "$DIST_DIR"
    echo "Created dist directory: $DIST_DIR"
fi

ZIP_FILENAME="atomCAD-macos-v$VERSION.zip"
ZIP_PATH="$DIST_DIR/$ZIP_FILENAME"

# Remove existing zip if it exists
if [ -f "$ZIP_PATH" ]; then
    echo "Removing existing zip file: $ZIP_FILENAME"
    rm -f "$ZIP_PATH"
fi

echo "Creating zip archive: $ZIP_FILENAME"

# Create zip from Release folder contents (including the .app bundle)
cd "$RELEASE_PATH"
zip -r "$ZIP_PATH" atomCAD.app
cd "$PROJECT_ROOT"

if [ ! -f "$ZIP_PATH" ]; then
    echo "Error: Failed to create zip archive: $ZIP_PATH"
    exit 1
fi

# Get zip file size for display
ZIP_SIZE=$(du -h "$ZIP_PATH" | cut -f1)

echo "Zip archive created successfully"
echo ""

# Step 7: Display results
echo "=== Build Complete ==="
echo "Release application: $APP_PATH"
echo "Zip archive: dist/$ZIP_FILENAME ($ZIP_SIZE)"
echo ""
echo "The zip file contains the complete macOS application ready for distribution."
echo "Extract and run atomCAD.app on the target macOS machine."
echo ""
echo "Note: Users may need to right-click and select 'Open' the first time"
echo "to bypass macOS Gatekeeper (since the app is not code signed)."
