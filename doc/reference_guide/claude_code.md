# Using with Claude Code

← Back to [Reference Guide hub](../atomCAD_reference_guide.md)

atomCAD integrates with [Claude Code](https://claude.ai/claude-code), Anthropic's AI coding assistant, enabling AI-assisted atomic structure design through natural language conversations. The CLI tool and skill file also work with other coding agent frameworks that support agent skills.

## What This Enables

- **Conversational Design:** Describe what you want to build in plain English, and Claude can create node networks, position cameras, and capture screenshots to verify results.
- **Iterative Refinement:** Ask Claude to modify existing designs—adjust parameters, add features, or fix issues—without manually editing nodes.
- **Learning Aid:** Claude can explain what each node does and suggest approaches for achieving specific geometries.

## How It Works

The integration consists of two parts:

1. **atomcad-cli:** A command-line tool that connects to a running atomCAD instance via a local server. It can query the current node network, edit nodes, control the camera, adjust display settings, and capture screenshots.

2. **Claude Code Skill:** A skill file (`.claude/skills/atomcad/skill.md`) that teaches Claude how to use the CLI effectively, including the text format for node definitions and best practices for atomic structure design.

## Setup

After installing atomCAD, run the setup script from your installation directory:

- **Windows:** `.\setup\setup-skill.bat`
- **Linux/macOS:** `bash setup/setup-skill.sh`

This installs the CLI to your PATH and copies the skill file to Claude Code's global skills directory.

## Usage

1. **Start atomCAD** (the CLI requires a running instance to connect to).
2. **Open Claude Code** in your terminal or IDE.
3. **Use the `/atomcad` command** or simply ask Claude to create atomic structures.

**Example prompts:**
- "Create a diamond sphere with radius 8 and fill it with atoms"
- "Add a cylindrical hole through the center of the current shape"
- "Take a screenshot from a 45-degree angle"

To explore atomcad-cli features, run `atomcad-cli --help`. For the complete CLI reference and text format specification, see the skill file at `.claude/skills/atomcad/skill.md` in your atomCAD installation.
