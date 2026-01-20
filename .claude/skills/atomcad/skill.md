---
name: atomcad
description: Interact with atomCAD node networks programmatically. Query, edit, and replace CAD geometry nodes for atomic/molecular structure design. Use when working with atomCAD projects or when the user wants to manipulate node networks, create CSG shapes, or design atomic structures.
license: MIT
metadata:
  author: atomCAD
  version: "2.0"
allowed-tools: Bash(atomcad-cli:*)
---

# atomCAD Skill

Programmatically interact with atomCAD node networks via CLI. Requires atomCAD to be running.

## Prerequisites

- atomCAD installed and running
- `atomcad-cli` on PATH (add repo root to PATH if running from source)

## Core Concepts

### Node Networks

atomCAD designs are parametric node networks (DAGs):

- **Nodes** have typed input pins (parameters) and one output pin
- **Wires** connect output→input of compatible types
- **Evaluation** is lazy: only visible nodes trigger computation
- **Custom nodes** are subnetworks with matching names

Each network can have an **output node** (set via `output <node_id>`) that defines what value the network returns when used as a custom node.

### Data Types

| Type | Description |
|------|-------------|
| `Bool`, `String`, `Int`, `Float` | Primitives |
| `Vec2`, `Vec3`, `IVec2`, `IVec3` | 2D/3D vectors (float/int) |
| `Geometry2D` | 2D shapes (for extrusion) |
| `Geometry` | 3D geometry (SDF-based) |
| `Atomic` | Atomic structure (atoms + bonds) |
| `UnitCell` | Crystal lattice parameters |
| `Motif` | Crystal motif definition |
| `[T]` | Array of type T |
| `A -> B` | Function type |

**Implicit conversions:** `Int`↔`Float`, `IVec`↔`Vec`, `T`→`[T]`

Array pins (marked with dot) accept multiple wires; values are concatenated.

### Text Format Syntax

```
# Create nodes: id = type { property: value, ... }
sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }
cuboid1 = cuboid { min_corner: (-5, -5, -5), extent: (10, 10, 10) }

# Wire nodes by referencing IDs in properties
union1 = union { shapes: [sphere1, cuboid1], visible: true }

# Set network output node
output union1

# Delete a node
delete sphere1
```

**Property values:**
- Integers: `42`, `-10`
- Floats: `3.14`, `1.5e-3`
- Booleans: `true`, `false`
- Strings: `"hello"`
- Vectors: `(x, y)` or `(x, y, z)`
- Arrays: `[a, b, c]`
- Node references: use the node's ID

## CLI Commands

### Global Options

```bash
atomcad-cli --help       # Show help
atomcad-cli --port=PORT  # Custom server port (default: 19847)
```

### Network Operations

```bash
# Query current network state
atomcad-cli query

# Edit network (add/update nodes, keeps existing)
atomcad-cli edit --code="sphere1 = sphere { radius: 10 }"

# Replace entire network (clears first)
atomcad-cli edit --code="..." --replace

# Multi-line edit (reads stdin until empty line or ".")
atomcad-cli edit
atomcad-cli edit --replace
```

### Node Discovery

```bash
# List all node types by category
atomcad-cli nodes

# List nodes in specific category
atomcad-cli nodes --category=Geometry3D

# List with descriptions (verbose mode, can combine with --category)
atomcad-cli nodes --verbose

# Get detailed info about any node type (built-in or custom)
atomcad-cli describe <node-name>
atomcad-cli describe sphere
atomcad-cli describe atom_fill
```

Use `describe` to discover input pins, types, defaults, and behavior for any node.

### REPL Mode

```bash
atomcad-cli              # Enter interactive mode
```

Commands: `query`/`q`, `edit`, `replace`/`r`, `nodes`, `describe <node>`, `help`/`?`, `quit`/`exit`

## Common Patterns

### Create a simple atomic structure

```bash
# Create sphere geometry and fill with atoms
atomcad-cli edit --code="s = sphere { radius: 5, visible: true }"
atomcad-cli edit --code="atoms = atom_fill { shape: s, visible: true }"
```

### Boolean operations on geometry

```bash
atomcad-cli edit --replace --code="
base = cuboid { extent: (10, 10, 10), visible: false }
hole = sphere { center: (5, 5, 5), radius: 4 }
result = diff { base: [base], sub: [hole], visible: true }
"
```

### Parametric design with custom nodes

1. Create a subnetwork named `my_shape` with `parameter` nodes for inputs
2. Set the output node in `my_shape`
3. Use `my_shape` as a node in other networks:

```bash
atomcad-cli edit --code="part1 = my_shape { size: 10, visible: true }"
```

## See Also

- `atomcad-cli describe <node>` for detailed node documentation
- `atomcad-cli nodes` to browse available node types
- `references/text-format.md` for complete text format specification
- `references/data-types.md` for detailed type system documentation
