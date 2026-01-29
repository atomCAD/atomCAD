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

**See [Node Network Text Format](./node_network_text_format.md) for the complete specification.**

### Quick Example

```
# Query result / Edit command
sphere1 = sphere { center: (0, 0, 0), radius: 5 }
box1 = cuboid { min_corner: (-2, -2, -2), extent: (4, 4, 4) }
diff1 = diff { base: sphere1, sub: box1, visible: true }
output diff1
```

### Key Features

- **Unified syntax** for properties and input connections
- **Function pin references** with `@` prefix (e.g., `f: @pattern` for `map` node)
- **Multi-line strings** with triple quotes for `expr` and `motif` definitions
- **Type annotations** where required (e.g., `parameter`, `expr`, `map` nodes)
- **Visibility control** with `visible: true` (default is invisible)

### Edit Semantics

| Statement | Name Exists? | Effect |
|-----------|--------------|--------|
| `sphere1 = sphere { radius: 4 }` | Yes | Update properties |
| `cylinder1 = cylinder { ... }` | No | Create node |
| `union1 = union { shapes: [a, b] }` | Yes, inputs changed | Rewire connections |
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

### Node Visibility

Controls whether a node's output is rendered in the viewport:

- **Query**: Visible nodes have `visible: true`, invisible nodes have no `visible` property
- **Edit**: `visible: true` makes a node visible, omitting `visible` makes it invisible (default)

```
# Visible node (rendered in viewport)
sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }

# Invisible node (default, participates in computations but not rendered)
int1 = int { value: 42 }
```

**Design rationale**: Defaulting to invisible keeps the format compact. The AI must be deliberate when it wants to display something.

## CLI Interface

```bash
# Query active network
atomcad-cli query
# → outputs text to stdout

# Edit network (incremental - merges with existing)
atomcad-cli edit --code="sphere1 = sphere { radius: 3.0 }"
# → outputs JSON result to stdout

# Edit network (replace - clears and rebuilds entire network)
atomcad-cli edit --replace --code="sphere1 = sphere { radius: 3.0 }"
# → outputs JSON result to stdout
```

The `--code` parameter receives multiline text from the AI tool framework (not shell pipes/heredocs).

| Flag | Description |
|------|-------------|
| `--code` | The edit commands (required) |
| `--replace` | Replace entire network instead of incremental merge |

## HTTP API

Flutter runs an HTTP server on localhost (e.g., port 19847).

```
GET  /query              → returns text representation
POST /edit               → body is edit commands, returns JSON result
POST /edit?replace=true  → full replace mode
```

## Open Questions

- Exact port number and configurability
- Error response format
- Authentication (probably unnecessary for localhost)
- How to handle atomCAD not running when CLI is invoked
