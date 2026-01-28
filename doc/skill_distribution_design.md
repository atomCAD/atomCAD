# atomCAD Skill + CLI Distribution Design

## Executive Summary

This document outlines the design for integrating the atomCAD skill and CLI into the release process, enabling users to use Claude Code with atomCAD for AI-assisted atomic structure design.

## Current State Analysis

### What Exists Today

1. **atomcad-cli** - A Dart CLI application (`bin/atomcad_cli.dart`, ~1260 lines)
   - Communicates with running atomCAD via HTTP on port 19847
   - Supports both command mode and interactive REPL
   - Development wrappers: `atomcad-cli` (bash) and `atomcad-cli.ps1` (PowerShell)
   - These wrappers require Dart SDK (`dart run bin/atomcad_cli.dart`)

2. **atomCAD Skill** (`.claude/skills/atomcad/` directory)
   - Claude Code skill following the Anthropic AI Agent Standard
   - Complete directory structure (not just a single file):
     ```
     .claude/skills/atomcad/
     ├── skill.md              # Main skill definition + CLI commands
     └── references/
         ├── data-types.md     # Complete type system documentation
         └── text-format.md    # Text format specification
     ```
   - The skill directory must be copied in its entirety to preserve the structure

3. **HTTP Server** (embedded in atomCAD)
   - `lib/ai_assistant/http_server.dart`
   - Starts automatically when atomCAD GUI launches
   - Listens on localhost:19847

### What's Missing from Releases

The current release process (`build_*_release.*` scripts and GitHub Actions workflow) only packages:
- The compiled atomCAD GUI application
- Required DLLs/libraries

**Not included:**
- The atomcad-cli executable
- The skill directory for Claude Code integration (`.claude/skills/atomcad/`)
- Any setup/installation tooling for the CLI

## Design Goals

1. **Zero Friction**: Users should be able to use atomCAD with Claude Code with minimal setup
2. **Self-Contained**: The release package should contain everything needed
3. **Cross-Platform**: Work consistently on Windows, Linux, and macOS
4. **Developer Friendly**: Support both installed atomCAD and development environment

## Architecture

```
atomCAD Release Package
├── atomCAD.exe / atomCAD.app / atomCAD (main application)
├── [other runtime files]
├── cli/
│   ├── atomcad-cli.exe (Windows) / atomcad-cli (Linux/macOS)
│   └── README.md (CLI quick-start guide)
├── claude-skill/
│   └── atomcad/                    # Complete skill directory (copied as-is)
│       ├── skill.md                # Main skill definition
│       └── references/
│           ├── data-types.md       # Type system reference
│           └── text-format.md      # Text format specification
└── setup/
    ├── setup-skill.ps1 (Windows)
    └── setup-skill.sh (Linux/macOS)
```

**Note:** The `claude-skill/atomcad/` directory mirrors the structure required by the Anthropic AI Agent Standard for multi-file skills. When installed, this becomes `~/.claude/skills/atomcad/` (global) or `<project>/.claude/skills/atomcad/` (per-project).

## Implementation Plan

### Phase 1: Compile CLI to Native Executable

The current CLI uses `dart run`, requiring Dart SDK. We need standalone executables.

**Dart Compilation Command:**
```bash
# Windows
dart compile exe bin/atomcad_cli.dart -o atomcad-cli.exe

# Linux/macOS
dart compile exe bin/atomcad_cli.dart -o atomcad-cli
```

**Advantages of native compilation:**
- No Dart SDK required on target machine
- Fast startup (no JIT)
- Single file distribution

### Phase 2: Update Build Scripts

#### Windows (`build_windows_release.ps1`)

Add after Flutter build (around line 88):

```powershell
# Step 5.5: Compile CLI to native executable
Write-Host "Compiling atomcad-cli..." -ForegroundColor Yellow
$CLIDir = "$ReleasePath\cli"
New-Item -ItemType Directory -Path $CLIDir -Force | Out-Null
dart compile exe bin/atomcad_cli.dart -o "$CLIDir\atomcad-cli.exe"
if ($LASTEXITCODE -ne 0) {
    throw "CLI compilation failed with exit code $LASTEXITCODE"
}
Write-Host "CLI compiled successfully" -ForegroundColor Green

# Step 5.6: Copy skill directory (entire structure)
Write-Host "Copying Claude skill directory..." -ForegroundColor Yellow
$SkillDir = "$ReleasePath\claude-skill"
New-Item -ItemType Directory -Path $SkillDir -Force | Out-Null
Copy-Item ".claude\skills\atomcad" "$SkillDir\" -Recurse
Write-Host "Skill directory copied (including references/)" -ForegroundColor Green

# Step 5.7: Copy setup scripts
Write-Host "Copying setup scripts..." -ForegroundColor Yellow
$SetupDir = "$ReleasePath\setup"
New-Item -ItemType Directory -Path $SetupDir -Force | Out-Null
Copy-Item "setup\setup-skill.ps1" "$SetupDir\"
Copy-Item "setup\setup-skill.sh" "$SetupDir\"
Write-Host "Setup scripts copied" -ForegroundColor Green
```

#### Linux (`build_linux_release.sh`)

Add after Flutter build (around line 120):

```bash
# Step 5.5: Compile CLI to native executable
echo "Compiling atomcad-cli..."
CLI_DIR="$RELEASE_PATH/cli"
mkdir -p "$CLI_DIR"
dart compile exe bin/atomcad_cli.dart -o "$CLI_DIR/atomcad-cli"
chmod +x "$CLI_DIR/atomcad-cli"
echo "CLI compiled successfully"

# Step 5.6: Copy skill directory (entire structure)
echo "Copying Claude skill directory..."
SKILL_DIR="$RELEASE_PATH/claude-skill"
mkdir -p "$SKILL_DIR"
cp -r ".claude/skills/atomcad" "$SKILL_DIR/"
echo "Skill directory copied (including references/)"

# Step 5.7: Copy setup scripts
echo "Copying setup scripts..."
SETUP_DIR="$RELEASE_PATH/setup"
mkdir -p "$SETUP_DIR"
cp "setup/setup-skill.ps1" "$SETUP_DIR/"
cp "setup/setup-skill.sh" "$SETUP_DIR/"
chmod +x "$SETUP_DIR/setup-skill.sh"
echo "Setup scripts copied"
```

#### macOS (`build_macos_release.sh`)

Same pattern as Linux. The compiled CLI binary goes inside the .app bundle's Resources folder or alongside it:

```bash
# For macOS, CLI goes alongside .app (not inside bundle, for easy access)
CLI_DIR="$RELEASE_PATH/cli"
mkdir -p "$CLI_DIR"
dart compile exe bin/atomcad_cli.dart -o "$CLI_DIR/atomcad-cli"
chmod +x "$CLI_DIR/atomcad-cli"
```

### Phase 3: Create Setup Scripts

These scripts help users configure their system to use atomcad-cli with Claude Code.

#### Create Directory: `setup/`

#### Windows Setup Script: `setup/setup-skill.ps1`

```powershell
#!/usr/bin/env pwsh
# atomCAD Skill Setup for Claude Code
#
# Usage:
#   .\setup-skill.ps1 --global          # Install skill globally for all projects
#   .\setup-skill.ps1 --project <path>  # Install skill to specific project
#   .\setup-skill.ps1 --add-to-path     # Add CLI to user PATH

param(
    [switch]$Global,
    [string]$Project,
    [switch]$AddToPath,
    [switch]$Help
)

$ErrorActionPreference = "Stop"

# Find atomCAD installation directory (where this script is located)
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$AtomCADDir = Split-Path -Parent $ScriptDir
$CLIPath = Join-Path $AtomCADDir "cli"
$SkillSourceDir = Join-Path $AtomCADDir "claude-skill\atomcad"

if ($Help -or (-not $Global -and -not $Project -and -not $AddToPath)) {
    Write-Host "atomCAD Skill Setup for Claude Code" -ForegroundColor Green
    Write-Host ""
    Write-Host "This script configures your system to use atomCAD with Claude Code."
    Write-Host ""
    Write-Host "Options:"
    Write-Host "  --global        Install skill globally (~/.claude/skills/atomcad/)"
    Write-Host "  --project PATH  Install skill to a specific project"
    Write-Host "  --add-to-path   Add atomcad-cli to user PATH environment variable"
    Write-Host "  --help          Show this help"
    Write-Host ""
    Write-Host "Examples:"
    Write-Host "  .\setup-skill.ps1 --global --add-to-path"
    Write-Host "  .\setup-skill.ps1 --project C:\Users\me\my-project"
    Write-Host ""
    exit 0
}

# Verify CLI and skill directory exist
if (-not (Test-Path (Join-Path $CLIPath "atomcad-cli.exe"))) {
    Write-Host "Error: atomcad-cli.exe not found in $CLIPath" -ForegroundColor Red
    exit 1
}
if (-not (Test-Path (Join-Path $SkillSourceDir "skill.md"))) {
    Write-Host "Error: skill.md not found in $SkillSourceDir" -ForegroundColor Red
    exit 1
}

# Add CLI to PATH
if ($AddToPath) {
    Write-Host "Adding atomcad-cli to PATH..." -ForegroundColor Yellow
    $CurrentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($CurrentPath -notlike "*$CLIPath*") {
        [Environment]::SetEnvironmentVariable("Path", "$CurrentPath;$CLIPath", "User")
        Write-Host "  Added $CLIPath to user PATH" -ForegroundColor Green
        Write-Host "  NOTE: Restart your terminal for changes to take effect" -ForegroundColor Cyan
    } else {
        Write-Host "  CLI path already in PATH" -ForegroundColor Yellow
    }
}

# Install skill globally (copy entire directory structure)
if ($Global) {
    Write-Host "Installing skill globally..." -ForegroundColor Yellow
    $GlobalSkillDir = Join-Path $env:USERPROFILE ".claude\skills"
    New-Item -ItemType Directory -Path $GlobalSkillDir -Force | Out-Null
    # Remove existing atomcad skill if present, then copy new one
    $TargetDir = Join-Path $GlobalSkillDir "atomcad"
    if (Test-Path $TargetDir) {
        Remove-Item $TargetDir -Recurse -Force
    }
    Copy-Item $SkillSourceDir $GlobalSkillDir -Recurse
    Write-Host "  Installed skill to $TargetDir" -ForegroundColor Green
    Write-Host "  (includes skill.md and references/ subdirectory)" -ForegroundColor Gray
}

# Install skill to specific project (copy entire directory structure)
if ($Project) {
    Write-Host "Installing skill to project..." -ForegroundColor Yellow
    if (-not (Test-Path $Project)) {
        Write-Host "Error: Project directory not found: $Project" -ForegroundColor Red
        exit 1
    }
    $ProjectSkillDir = Join-Path $Project ".claude\skills"
    New-Item -ItemType Directory -Path $ProjectSkillDir -Force | Out-Null
    # Remove existing atomcad skill if present, then copy new one
    $TargetDir = Join-Path $ProjectSkillDir "atomcad"
    if (Test-Path $TargetDir) {
        Remove-Item $TargetDir -Recurse -Force
    }
    Copy-Item $SkillSourceDir $ProjectSkillDir -Recurse
    Write-Host "  Installed skill to $TargetDir" -ForegroundColor Green
    Write-Host "  (includes skill.md and references/ subdirectory)" -ForegroundColor Gray
}

Write-Host ""
Write-Host "Setup complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Cyan
Write-Host "  1. Start atomCAD"
Write-Host "  2. Open Claude Code in your project"
Write-Host "  3. Use /atomcad or ask Claude to create atomic structures"
Write-Host ""
```

#### Linux/macOS Setup Script: `setup/setup-skill.sh`

```bash
#!/bin/bash
# atomCAD Skill Setup for Claude Code
#
# Usage:
#   ./setup-skill.sh --global          # Install skill globally
#   ./setup-skill.sh --project <path>  # Install skill to specific project
#   ./setup-skill.sh --add-to-path     # Add CLI to PATH

set -e

# Find atomCAD installation directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ATOMCAD_DIR="$(dirname "$SCRIPT_DIR")"
CLI_PATH="$ATOMCAD_DIR/cli"
SKILL_SOURCE_DIR="$ATOMCAD_DIR/claude-skill/atomcad"

# Default values
DO_GLOBAL=false
DO_PROJECT=""
DO_PATH=false

usage() {
    echo "atomCAD Skill Setup for Claude Code"
    echo ""
    echo "This script configures your system to use atomCAD with Claude Code."
    echo ""
    echo "Options:"
    echo "  --global         Install skill globally (~/.claude/skills/atomcad/)"
    echo "  --project PATH   Install skill to a specific project"
    echo "  --add-to-path    Add atomcad-cli to PATH (modifies shell rc file)"
    echo "  --help           Show this help"
    echo ""
    echo "Examples:"
    echo "  ./setup-skill.sh --global --add-to-path"
    echo "  ./setup-skill.sh --project ~/my-project"
}

add_to_path() {
    local shell_rc="$HOME/.bashrc"
    if [[ "$SHELL" == *"zsh"* ]]; then
        shell_rc="$HOME/.zshrc"
    fi

    echo "Adding atomcad-cli to PATH..."
    if ! grep -q "$CLI_PATH" "$shell_rc" 2>/dev/null; then
        echo "" >> "$shell_rc"
        echo "# atomCAD CLI" >> "$shell_rc"
        echo "export PATH=\"\$PATH:$CLI_PATH\"" >> "$shell_rc"
        echo "  Added $CLI_PATH to $shell_rc"
        echo "  NOTE: Run 'source $shell_rc' or restart your terminal"
    else
        echo "  CLI path already in $shell_rc"
    fi
}

install_global() {
    echo "Installing skill globally..."
    local global_skill_dir="$HOME/.claude/skills"
    mkdir -p "$global_skill_dir"
    # Remove existing atomcad skill if present, then copy new one
    rm -rf "$global_skill_dir/atomcad"
    cp -r "$SKILL_SOURCE_DIR" "$global_skill_dir/"
    echo "  Installed skill to $global_skill_dir/atomcad"
    echo "  (includes skill.md and references/ subdirectory)"
}

install_project() {
    local project_dir="$1"
    echo "Installing skill to project..."
    if [[ ! -d "$project_dir" ]]; then
        echo "Error: Project directory not found: $project_dir" >&2
        exit 1
    fi
    local project_skill_dir="$project_dir/.claude/skills"
    mkdir -p "$project_skill_dir"
    # Remove existing atomcad skill if present, then copy new one
    rm -rf "$project_skill_dir/atomcad"
    cp -r "$SKILL_SOURCE_DIR" "$project_skill_dir/"
    echo "  Installed skill to $project_skill_dir/atomcad"
    echo "  (includes skill.md and references/ subdirectory)"
}

# Verify files exist
verify_files() {
    if [[ ! -f "$CLI_PATH/atomcad-cli" ]]; then
        echo "Error: atomcad-cli not found in $CLI_PATH" >&2
        exit 1
    fi
    if [[ ! -d "$SKILL_SOURCE_DIR" ]]; then
        echo "Error: skill directory not found at $SKILL_SOURCE_DIR" >&2
        exit 1
    fi
    if [[ ! -f "$SKILL_SOURCE_DIR/skill.md" ]]; then
        echo "Error: skill.md not found in $SKILL_SOURCE_DIR" >&2
        exit 1
    fi
}

# Parse arguments
if [[ $# -eq 0 ]]; then
    usage
    exit 0
fi

while [[ $# -gt 0 ]]; do
    case $1 in
        --global)
            DO_GLOBAL=true
            shift
            ;;
        --project)
            DO_PROJECT="$2"
            shift 2
            ;;
        --add-to-path)
            DO_PATH=true
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage
            exit 1
            ;;
    esac
done

verify_files

if $DO_PATH; then
    add_to_path
fi

if $DO_GLOBAL; then
    install_global
fi

if [[ -n "$DO_PROJECT" ]]; then
    install_project "$DO_PROJECT"
fi

echo ""
echo "Setup complete!"
echo ""
echo "Next steps:"
echo "  1. Start atomCAD"
echo "  2. Open Claude Code in your project"
echo "  3. Use /atomcad or ask Claude to create atomic structures"
```

### Phase 4: Update GitHub Actions Workflow

Update `.github/workflows/release.yml` to include CLI compilation steps:

```yaml
# In build-windows job, after Flutter build step:
- name: Compile CLI
  run: |
    $CLIDir = "build\windows\x64\runner\Release\cli"
    New-Item -ItemType Directory -Path $CLIDir -Force
    dart compile exe bin/atomcad_cli.dart -o "$CLIDir\atomcad-cli.exe"
  shell: powershell

- name: Copy skill directory and setup files
  run: |
    mkdir -p build/windows/x64/runner/Release/claude-skill
    mkdir -p build/windows/x64/runner/Release/setup
    # Copy entire skill directory (preserves references/ subdirectory)
    cp -r .claude/skills/atomcad build/windows/x64/runner/Release/claude-skill/
    cp setup/setup-skill.ps1 build/windows/x64/runner/Release/setup/
    cp setup/setup-skill.sh build/windows/x64/runner/Release/setup/
  shell: bash

# In build-linux job, after Flutter build step:
- name: Compile CLI
  run: |
    mkdir -p build/linux/x64/release/bundle/cli
    dart compile exe bin/atomcad_cli.dart -o build/linux/x64/release/bundle/cli/atomcad-cli
    chmod +x build/linux/x64/release/bundle/cli/atomcad-cli

- name: Copy skill directory and setup files
  run: |
    mkdir -p build/linux/x64/release/bundle/claude-skill
    mkdir -p build/linux/x64/release/bundle/setup
    # Copy entire skill directory (preserves references/ subdirectory)
    cp -r .claude/skills/atomcad build/linux/x64/release/bundle/claude-skill/
    cp setup/setup-skill.ps1 build/linux/x64/release/bundle/setup/
    cp setup/setup-skill.sh build/linux/x64/release/bundle/setup/
    chmod +x build/linux/x64/release/bundle/setup/setup-skill.sh

# In build-macos job, after Flutter build step:
- name: Compile CLI
  run: |
    mkdir -p build/macos/Build/Products/Release/cli
    dart compile exe bin/atomcad_cli.dart -o build/macos/Build/Products/Release/cli/atomcad-cli
    chmod +x build/macos/Build/Products/Release/cli/atomcad-cli

- name: Copy skill directory and setup files
  run: |
    mkdir -p build/macos/Build/Products/Release/claude-skill
    mkdir -p build/macos/Build/Products/Release/setup
    # Copy entire skill directory (preserves references/ subdirectory)
    cp -r .claude/skills/atomcad build/macos/Build/Products/Release/claude-skill/
    cp setup/setup-skill.ps1 build/macos/Build/Products/Release/setup/
    cp setup/setup-skill.sh build/macos/Build/Products/Release/setup/
    chmod +x build/macos/Build/Products/Release/setup/setup-skill.sh
```

### Phase 5: Update Skill.md for Installed Users

The current skill.md needs to handle both development and installed scenarios.

Update the "Command Resolution" section in `.claude/skills/atomcad/skill.md`:

```markdown
## Command Resolution

Before running CLI commands, detect the appropriate command:
1. If `./atomcad-cli` exists in current directory → use `./atomcad-cli` (development mode)
2. If `atomcad-cli` is found in PATH → use `atomcad-cli` (installed mode)
3. Otherwise → inform user to run the setup script or add CLI to PATH

To set up the CLI, users should run the setup script from their atomCAD installation:
- Windows: `<atomcad-path>\setup\setup-skill.ps1 --add-to-path --global`
- Linux/macOS: `<atomcad-path>/setup/setup-skill.sh --add-to-path --global`
```

### Phase 6: Documentation Updates

#### Add CLI Quick-Start: `cli/README.md` (included in release)

```markdown
# atomcad-cli Quick Start

The atomCAD CLI allows AI assistants (like Claude Code) to interact with atomCAD programmatically.

## Prerequisites

- atomCAD must be running (the CLI connects to a running instance on localhost:19847)

## Setup

Run the setup script to add the CLI to your PATH and install the Claude Code skill:

**Windows (PowerShell):**
```powershell
.\setup\setup-skill.ps1 --add-to-path --global
```

**Linux/macOS:**
```bash
./setup/setup-skill.sh --add-to-path --global
```

Then restart your terminal.

## Usage with Claude Code

1. Start atomCAD
2. Open Claude Code in your project
3. Ask Claude to create atomic structures, or invoke the skill with `/atomcad`

## Manual CLI Usage

```bash
# Query current network
atomcad-cli query

# Create a sphere
atomcad-cli edit --code="s = sphere { radius: 10, visible: true }"

# List available nodes
atomcad-cli nodes

# Interactive mode
atomcad-cli
```

See the skill.md file for complete documentation.
```

#### Update Main README.md

Add section after "Installation":

```markdown
## Using with Claude Code

atomCAD integrates with Claude Code for AI-assisted atomic structure design.

### Quick Setup

After installing atomCAD:

1. Run the setup script from your atomCAD installation:
   - **Windows**: `.\setup\setup-skill.ps1 --global --add-to-path`
   - **Linux/macOS**: `./setup/setup-skill.sh --global --add-to-path`
2. Restart your terminal
3. Start atomCAD
4. Open Claude Code in your project
5. Use `/atomcad` or ask Claude to create atomic structures

### What Gets Installed

- `atomcad-cli` added to your PATH (for CLI access)
- Claude Code skill installed to `~/.claude/skills/atomcad/`

### Developer Setup

When working in the atomCAD repository, the skill is already available at `.claude/skills/atomcad/`. Use the development wrapper scripts (`./atomcad-cli` or `.\atomcad-cli.ps1`) instead.
```

## User Experience Flows

### Flow 1: New User Setup (Recommended)

```
1. Download atomCAD-windows-v1.0.0.zip (or Linux/macOS equivalent)
2. Extract to: C:\Program Files\atomCAD\ (or ~/Applications/atomCAD)
3. Open terminal in extracted folder
4. Run: .\setup\setup-skill.ps1 --global --add-to-path
5. Restart terminal
6. Start atomCAD.exe
7. In any project: Open Claude Code, ask "Create a diamond sphere"
```

### Flow 2: Project-Specific Installation

```
1. atomCAD installed somewhere on system
2. User wants skill only in specific project
3. Run: <atomcad-path>\setup\setup-skill.ps1 --project C:\myproject
4. Skill is now in C:\myproject\.claude\skills\atomcad\
5. Claude Code in that project can use atomCAD
```

### Flow 3: Developer Workflow

```
1. Clone atomCAD repository
2. Skill directory already exists at .claude/skills/atomcad/
   (includes skill.md and references/ subdirectory)
3. Use ./atomcad-cli (Linux/macOS) or .\atomcad-cli.ps1 (Windows)
4. No additional setup needed
```

## Testing Plan

1. **Build Tests**
   - Verify `dart compile exe` succeeds on all platforms
   - Verify compiled binary runs standalone (no Dart SDK)

2. **Integration Tests**
   - CLI can connect to running atomCAD
   - Basic commands work (query, edit, screenshot)

3. **Setup Script Tests**
   - Scripts work on clean systems
   - PATH modification is correct
   - Skill file copies to correct location

4. **End-to-End**
   - Fresh Windows/Linux/macOS install
   - Run setup script
   - Start atomCAD
   - Claude Code can invoke skill and create structures

## Security Considerations

1. **Local Only**: CLI only connects to localhost:19847, no remote access
2. **No Credentials**: No sensitive data stored or transmitted
3. **Explicit User Action**: User must run setup script to modify PATH
4. **Read/Write**: CLI can modify node networks - this is intended functionality

## Future Enhancements

1. **Auto-Discovery**: atomCAD could write its install path to a well-known config file
2. **GUI Setup Wizard**: "Setup Claude Integration" button in atomCAD preferences
3. **Version Checking**: Skill could verify CLI version matches atomCAD version
4. **Package Managers**: Publish via Chocolatey (Windows), Homebrew (macOS), apt/snap (Linux)
5. **VS Code Extension**: Dedicated extension that manages CLI and skill automatically

## Summary of Required Changes

| File | Change Type | Description |
|------|-------------|-------------|
| `build_windows_release.ps1` | Modify | Add CLI compilation + skill directory copying steps |
| `build_linux_release.sh` | Modify | Add CLI compilation + skill directory copying steps |
| `build_macos_release.sh` | Modify | Add CLI compilation + skill directory copying steps |
| `.github/workflows/release.yml` | Modify | Add CLI compilation + skill directory copying to all platform jobs |
| `setup/setup-skill.ps1` | **NEW** | Windows setup script (copies entire skill directory) |
| `setup/setup-skill.sh` | **NEW** | Linux/macOS setup script (copies entire skill directory) |
| `.claude/skills/atomcad/skill.md` | Modify | Update command resolution section |
| `README.md` | Modify | Add "Using with Claude Code" section |

### Skill Directory Structure to Copy

The entire `.claude/skills/atomcad/` directory must be copied recursively:
```
atomcad/
├── skill.md              # Main skill definition
└── references/
    ├── data-types.md     # Type system reference
    └── text-format.md    # Text format specification
```

## Appendix: Directory Structure After Implementation

### Source Repository Structure

```
flutter_cad/
├── bin/
│   └── atomcad_cli.dart              # CLI source (unchanged)
├── setup/                            # NEW directory
│   ├── setup-skill.ps1               # Windows setup script
│   └── setup-skill.sh                # Linux/macOS setup script
└── .claude/
    └── skills/
        └── atomcad/                  # Complete skill directory
            ├── skill.md              # Main skill definition
            └── references/
                ├── data-types.md     # Type system reference
                └── text-format.md    # Text format specification
```

### Release Package Structure (after build)

```
build/windows/x64/runner/Release/
├── atomCAD.exe
├── cli/
│   ├── atomcad-cli.exe
│   └── README.md
├── claude-skill/
│   └── atomcad/                      # Complete skill directory (copied as-is)
│       ├── skill.md
│       └── references/
│           ├── data-types.md
│           └── text-format.md
└── setup/
    ├── setup-skill.ps1
    └── setup-skill.sh
```

### User's System After Setup (--global)

```
~/.claude/skills/
└── atomcad/                          # Complete skill directory
    ├── skill.md
    └── references/
        ├── data-types.md
        └── text-format.md
```

## Conclusion

This design enables atomCAD users to leverage Claude Code for AI-assisted atomic structure design with a simple setup process. The key innovations are:

1. **Native CLI compilation**: Eliminates Dart SDK dependency for end users
2. **Self-contained release packages**: Everything needed is in the zip/tarball
3. **Complete skill directory**: Follows Anthropic AI Agent Standard with multi-file skill structure (skill.md + references/)
4. **Setup scripts**: Automate PATH and skill installation, copying entire directory structure
5. **Clear documentation**: Guide users through the process

The implementation requires changes to 4 existing files and creation of 2 new files (the setup scripts). The result is a seamless integration between atomCAD and Claude Code.

### Skill Structure Benefits

The multi-file skill structure provides:
- **Main skill.md**: Core CLI commands and usage patterns
- **references/data-types.md**: Complete type system documentation for advanced users
- **references/text-format.md**: Full specification of the text-based network format

This allows Claude to access detailed reference documentation when needed, improving the quality of AI-assisted atomic structure design.
