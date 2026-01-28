#!/bin/bash

# atomCAD Linux Release Build Script
# 
# This script builds atomCAD for Linux and creates a distributable tar.gz file.
# Output: dist/atomCAD-linux-v[VERSION].tar.gz containing all necessary files for distribution.
#
# Usage Examples:
#   ./build_linux_release.sh                           # Build with default version (1.0.0)
#   ./build_linux_release.sh --version "1.2.3"        # Build with specific version
#   ./build_linux_release.sh --version "2.0.0" --skip-flutter-clean  # Skip clean for faster builds
#   ./build_linux_release.sh --skip-rust-build        # Skip Rust build if already built
#
# The created tar.gz file contains the complete Linux application ready for distribution.
# Compatible with Ubuntu, Debian, Fedora, Arch, and most other Linux distributions.

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

echo "=== atomCAD Linux Release Build ==="
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

# Step 4: Build Flutter Linux release
echo "Building Flutter Linux release..."
flutter build linux --release
if [ $? -ne 0 ]; then
    echo "Error: Flutter Linux build failed"
    exit 1
fi
echo "Flutter Linux build completed"
echo ""

# Step 5: Verify release files exist
RELEASE_PATH="build/linux/x64/release/bundle"
EXECUTABLE_PATH="$RELEASE_PATH/atomCAD"

if [ ! -d "$RELEASE_PATH" ]; then
    echo "Error: Release directory not found: $RELEASE_PATH"
    exit 1
fi

if [ ! -f "$EXECUTABLE_PATH" ]; then
    echo "Error: Executable not found: $EXECUTABLE_PATH"
    exit 1
fi

echo "Release files verified"
echo ""

# Step 5.5: Compile CLI to native executable
echo "Compiling atomcad-cli..."
CLI_DIR="$RELEASE_PATH/cli"
mkdir -p "$CLI_DIR"
dart compile exe bin/atomcad_cli.dart -o "$CLI_DIR/atomcad-cli"
if [ $? -ne 0 ]; then
    echo "Error: CLI compilation failed"
    exit 1
fi
chmod +x "$CLI_DIR/atomcad-cli"
echo "CLI compiled successfully"
echo ""

# Step 5.6: Copy skill directory (entire structure)
echo "Copying Claude skill directory..."
SKILL_DIR="$RELEASE_PATH/claude-skill"
mkdir -p "$SKILL_DIR"
cp -r ".claude/skills/atomcad" "$SKILL_DIR/"
echo "Skill directory copied (including references/)"
echo ""

# Step 5.7: Copy setup scripts
echo "Copying setup scripts..."
SETUP_DIR="$RELEASE_PATH/setup"
mkdir -p "$SETUP_DIR"
cp "setup/setup-skill.ps1" "$SETUP_DIR/"
cp "setup/setup-skill.sh" "$SETUP_DIR/"
chmod +x "$SETUP_DIR/setup-skill.sh"
echo "Setup scripts copied"
echo ""

# Step 6: Create tar.gz archive
DIST_DIR="$PROJECT_ROOT/dist"
if [ ! -d "$DIST_DIR" ]; then
    mkdir -p "$DIST_DIR"
    echo "Created dist directory: $DIST_DIR"
fi

ARCHIVE_FILENAME="atomCAD-linux-v$VERSION.tar.gz"
ARCHIVE_PATH="$DIST_DIR/$ARCHIVE_FILENAME"

# Remove existing archive if it exists
if [ -f "$ARCHIVE_PATH" ]; then
    echo "Removing existing archive file: $ARCHIVE_FILENAME"
    rm -f "$ARCHIVE_PATH"
fi

echo "Creating tar.gz archive: $ARCHIVE_FILENAME"

# Create tar.gz from Release bundle contents
cd "build/linux/x64/release"
tar -czf "$ARCHIVE_PATH" bundle/
cd "$PROJECT_ROOT"

if [ ! -f "$ARCHIVE_PATH" ]; then
    echo "Error: Failed to create tar.gz archive: $ARCHIVE_PATH"
    exit 1
fi

# Get archive file size for display
ARCHIVE_SIZE=$(du -h "$ARCHIVE_PATH" | cut -f1)

echo "Archive created successfully"
echo ""

# Step 7: Display results
echo "=== Build Complete ==="
echo "Release executable: $EXECUTABLE_PATH"
echo "Archive: dist/$ARCHIVE_FILENAME ($ARCHIVE_SIZE)"
echo ""
echo "The tar.gz file contains the complete Linux application ready for distribution."
echo "Extract and run: tar -xzf $ARCHIVE_FILENAME && cd bundle && ./atomCAD"
echo ""
echo "Compatible with Ubuntu, Debian, Fedora, Arch, and most Linux distributions."
echo "Requires: glibc 2.17+ (available on most modern Linux systems)"
