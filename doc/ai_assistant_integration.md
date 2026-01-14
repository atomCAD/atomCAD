# AI Assistant Integration for atomCAD

## Overview

This document describes the design for enabling AI coding assistants (Claude Code, etc.) to interact with atomCAD. The integration allows assistants to query and edit node networks programmatically.

## Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  AI Assistant   │────▶│  atomcad-cli    │────▶│    atomCAD      │
│  (Claude Code)  │     │                 │     │  Flutter HTTP   │
└─────────────────┘     └─────────────────┘     │  Server → Rust  │
   tool call              HTTP request          └─────────────────┘
```

- **atomcad-cli**: Thin CLI proxy that forwards commands to atomCAD
- **Flutter HTTP Server**: Listens on localhost, handles requests, calls Rust backend, triggers UI refresh
- **Transport**: HTTP on localhost (cross-platform, debuggable)

The Flutter server solves the Rust→Flutter UI refresh problem: after processing an edit, Flutter can call `notifyListeners()` directly.

## Commands

### Phase 1 (Initial Focus)

| Command | Description |
|---------|-------------|
| `query` | Returns the active node network as text |
| `edit`  | Modifies the node network using the same text format |

### Future Commands (Not Yet Designed)

- `screenshot` - Capture viewport image
- `camera` - Move viewport camera
- `networks list` - List available node networks
- `networks select` - Switch active network

## Text Format

The same format is used for both query results and edit commands.

### Grammar

```
line       := assignment | statement
assignment := name '=' type '{' params '}'
statement  := 'output' name | 'delete' name
params     := (name ':' value ',')*
name       := identifier (e.g., sphere1, box1)
type       := PascalCase node type (e.g., Sphere, Union)
value      := literal | name
```

### Example: Query Result

```
sphere1 = Sphere { radius: 2.0 }
box1 = Box { size: [1, 2, 3] }
union1 = Union { a: sphere1, b: box1 }
output union1
```

### Example: Edit Command

```
sphere1 = Sphere { radius: 4.0 }
cylinder1 = Cylinder { radius: 1.0, height: 3.0 }
union1 = Union { a: sphere1, b: cylinder1 }
delete box1
```

### Edit Semantics

| Statement | Name Exists? | Effect |
|-----------|--------------|--------|
| `sphere1 = Sphere { radius: 4.0 }` | Yes | Update params |
| `cylinder1 = Cylinder { ... }` | No | Create node |
| `union1 = Union { a: sphere1, b: cylinder1 }` | Yes, inputs changed | Rewire connections |
| `delete box1` | Yes | Remove node and connections |

Nodes not mentioned in an edit command remain unchanged.

### Node Naming

- Names are generated deterministically: `{typename}{counter}` (e.g., sphere1, sphere2)
- Names are assigned via topological sort for stability
- LLM uses these names to reference nodes in edit commands
- Internal node IDs are hidden from the LLM interface

### Node Positions

- **Not exposed** in query or edit
- LLM edits semantics (data flow), not layout
- New nodes placed automatically (simple algorithm, known limitation)
- Users can manually organize layout after AI edits

## CLI Interface

```bash
# Query active network
atomcad-cli query
# → outputs text to stdout

# Edit network (multiline via tool parameter)
atomcad-cli edit --code="sphere1 = Sphere { radius: 3.0 }"
# → outputs JSON result to stdout
```

The `--code` parameter receives multiline text from the AI tool framework (not shell pipes/heredocs).

## HTTP API

Flutter runs an HTTP server on localhost (e.g., port 19847).

```
GET  /query  → returns text representation
POST /edit   → body is edit commands, returns JSON result
```

## Open Questions

- Exact port number and configurability
- Error response format
- Authentication (probably unnecessary for localhost)
- How to handle atomCAD not running when CLI is invoked
