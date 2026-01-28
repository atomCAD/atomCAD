#!/bin/bash
# atomCAD Skill Setup for Claude Code
# Installs the skill globally and adds CLI to PATH

set -e

# Find atomCAD installation directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ATOMCAD_DIR="$(dirname "$SCRIPT_DIR")"
CLI_PATH="$ATOMCAD_DIR/cli"
SKILL_SOURCE_DIR="$ATOMCAD_DIR/claude-skill/atomcad"

echo "atomCAD Skill Setup for Claude Code"
echo ""

# Verify files exist
if [[ ! -f "$CLI_PATH/atomcad-cli" ]]; then
    echo "Error: atomcad-cli not found in $CLI_PATH" >&2
    exit 1
fi
if [[ ! -f "$SKILL_SOURCE_DIR/skill.md" ]]; then
    echo "Error: skill.md not found in $SKILL_SOURCE_DIR" >&2
    exit 1
fi

# Add CLI to PATH
shell_rc="$HOME/.bashrc"
if [[ "$SHELL" == *"zsh"* ]]; then
    shell_rc="$HOME/.zshrc"
fi

echo "Adding atomcad-cli to PATH..."
if ! grep -q "$CLI_PATH" "$shell_rc" 2>/dev/null; then
    echo "" >> "$shell_rc"
    echo "# atomCAD CLI" >> "$shell_rc"
    echo "export PATH=\"\$PATH:$CLI_PATH\"" >> "$shell_rc"
    echo "  Added $CLI_PATH to $shell_rc"
else
    echo "  CLI path already in $shell_rc"
fi

# Install skill globally
echo "Installing skill globally..."
global_skill_dir="$HOME/.claude/skills"
mkdir -p "$global_skill_dir"
rm -rf "$global_skill_dir/atomcad"
cp -r "$SKILL_SOURCE_DIR" "$global_skill_dir/"
echo "  Installed skill to $global_skill_dir/atomcad"

echo ""
echo "Setup complete!"
echo ""
echo "Next steps:"
echo "  1. Restart your terminal (or run 'source $shell_rc')"
echo "  2. Start atomCAD"
echo "  3. Open Claude Code in your project"
echo "  4. Use /atomcad or ask Claude to create atomic structures"
echo ""
