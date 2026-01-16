---
name: atomcad
description: Interact with atomCAD node networks programmatically. Query, edit, and replace CAD geometry nodes for atomic/molecular structure design. Use when working with atomCAD projects or when the user wants to manipulate node networks, create CSG shapes, or design atomic structures.
license: MIT
metadata:
  author: atomCAD
  version: "1.0"
allowed-tools: Bash(atomcad-cli:*)
---

# atomCAD Skill

Interact with atomCAD node networks programmatically. Requires atomCAD to be running.

## Prerequisites

- atomCAD installed and running
- `atomcad-cli` on PATH (if running atomCAD from the repo, add the repo root to your PATH)

## Commands

### Query the active network
```bash
atomcad-cli query
```
Returns the node network in text format.

### Edit the network (single line)
```bash
atomcad-cli edit --code="<text format code>"
```
Adds/updates nodes without removing existing ones.

### Replace entire network (single line)
```bash
atomcad-cli edit --code="<text format code>" --replace
```
Clears the network and creates only the specified nodes.

### Multi-line edit
```bash
atomcad-cli edit
```
Reads text format from stdin until an empty line or `.` on its own line. Useful for multi-line edits.

### Multi-line replace
```bash
atomcad-cli edit --replace
```
Same as above, but replaces the entire network.

### REPL mode
```bash
atomcad-cli
```
Enters interactive REPL mode. Available commands:
- `query` or `q` - Show current network
- `edit` - Enter edit mode (incremental)
- `edit --replace` or `replace` or `r` - Enter edit mode (replace)
- `help` or `?` - Show help
- `quit` or `exit` - Exit REPL

In edit mode, type text format commands, then:
- Empty line to send
- `.` on its own line to send
- Ctrl+C to cancel

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
