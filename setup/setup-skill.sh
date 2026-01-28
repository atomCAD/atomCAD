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
