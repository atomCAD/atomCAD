# atomCAD Skill and CLI Distribution Plan

This document outlines the plan for making atomCAD accessible to AI coding assistants (Claude Code, Cursor, etc.) running in any project.

**Related Documents:**
- [AI Assistant Integration](./ai_assistant_integration.md) - High-level architecture and requirements
- [AI Assistant Implementation Plan](./ai_assistant_implementation_plan.md) - Implementation details for Phases 1-5
- [Node Network Text Format](./node_network_text_format.md) - Text format specification

## Goal

Enable AI assistants to interact with atomCAD from any project by providing:
1. A CLI tool (`atomcad-cli`) available system-wide
2. A skill definition file that can be distributed to other projects

## Current State

- **HTTP Server**: Running on `localhost:19847` with `/query` and `/edit` endpoints (Phase 5 complete)
- **CLI Tool**: `bin/atomcad_cli.dart` exists but requires `dart run` to execute
- **Skill File**: Does not exist yet

---

## Requirements

### Dev Environment
- [ ] Simple way to invoke CLI during development without typing `dart run bin/atomcad_cli.dart`

### Installed Package
- [ ] CLI binary bundled with Windows, Linux, and macOS releases
- [ ] CLI available in system PATH after installation

### Agent Skill Distribution
- [ ] Skill definition file (`atomcad-skill.md`) describing how to use the CLI
- [ ] Easy for users to add atomCAD skill to their projects

---

## Implementation Plan

### Phase 1: Dev Environment Convenience

**Task:** Create wrapper script for development use.

Create `atomcad-cli.ps1` in project root:
```powershell
#!/usr/bin/env pwsh
dart run bin/atomcad_cli.dart $args
```

Usage:
```powershell
./atomcad-cli query
./atomcad-cli edit --code="sphere1 = sphere { radius: 5 }"
```

### Phase 2: Build Process Integration

**Task:** Compile CLI as part of release builds.

- [ ] Add compilation step to build process:
  ```bash
  dart compile exe bin/atomcad_cli.dart -o build/atomcad-cli.exe
  ```
- [ ] Bundle compiled binary with release artifacts
- [ ] Document in build instructions

### Phase 3: Installer Integration

**Task:** Ensure CLI is in PATH after installation.

**Windows:**
- [ ] Include `atomcad-cli.exe` in installer
- [ ] Add installation directory to user PATH (or provide option during install)

**macOS:**
- [ ] Include `atomcad-cli` in app bundle or separate location
- [ ] Symlink to `/usr/local/bin/` or document PATH setup

**Linux:**
- [ ] Include `atomcad-cli` in package
- [ ] Install to `/usr/local/bin/` or similar

### Phase 4: Skill File Creation

**Task:** Create distributable skill definition.

Create `atomcad-skill.md`:

```markdown
# atomCAD Skill

Interact with atomCAD node networks programmatically. Requires atomCAD to be running.

## Prerequisites

- atomCAD installed and running
- `atomcad-cli` in PATH (included with atomCAD installation)

## Commands

### Query the active network
```bash
atomcad-cli query
```
Returns the node network in text format.

### Edit the network (incremental)
```bash
atomcad-cli edit --code="<text format code>"
```
Adds/updates nodes without removing existing ones.

### Replace entire network
```bash
atomcad-cli edit --code="<text format code>" --replace
```
Clears the network and creates only the specified nodes.

## Text Format Quick Reference

```
# Create nodes
sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }
cuboid1 = cuboid { min_corner: (0, 0, 0), extent: (10, 10, 10) }

# Connect nodes
union1 = union { shapes: [sphere1, cuboid1], visible: true }

# Set output
output union1

# Delete a node
delete sphere1
```

## Example Workflow

1. Query current state: `atomcad-cli query`
2. Make changes: `atomcad-cli edit --code="..."`
3. Verify changes: `atomcad-cli query`
```

### Phase 5: Distribution

**Task:** Make skill easily discoverable and usable.

Options to consider:
- [ ] Include `atomcad-skill.md` in atomCAD installation

---

## Future Considerations

- **Auto-start atomCAD**: Should the skill/CLI be able to launch atomCAD if not running?
- **Project file management**: Should the CLI support opening specific `.cnnd` files?

---

## Open Questions

1. What should happen if atomCAD is not running when CLI is invoked?
3. How do users add the skill to their projects? Copy the file? Reference a URL?
4. Should the skill file include the full text format reference or link to docs?
