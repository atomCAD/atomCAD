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

### Lattice Coordinates

**Important:** Geometry coordinates (positions, sizes, radii) are in discrete **lattice units** (integers), not angstroms. The actual physical dimensions depend on the unit cell. This ensures all atomic structures are naturally lattice-aligned.

### Unit Cell and Motif

- **Default unit cell:** Cubic diamond (3.567Å lattice constant) if none specified
- **Default motif:** Cubic zincblende with carbon atoms (pure diamond)
- Geometry nodes accept a `unit_cell` input to specify different lattices
- Non-cubic unit cells cause shapes like `cuboid` to become parallelepipeds

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

**Note:** `diff` and `diff_2d` accept arrays on both inputs. They implicitly union each array before computing the difference: `diff(base, sub) = base₁ ∪ base₂ ∪ ... − (sub₁ ∪ sub₂ ∪ ...)`

### Creating realistic atomic structures

```bash
# Passivate dangling bonds with hydrogen
atomcad-cli edit --code="atoms = atom_fill { shape: geom, passivate: true, visible: true }"

# Remove single-bond atoms recursively (cleaner structures)
atomcad-cli edit --code="atoms = atom_fill { shape: geom, remove_single_bond: true, visible: true }"

# Override motif elements (e.g., silicon carbide instead of diamond)
atomcad-cli edit --code="atoms = atom_fill { shape: geom, element_values: \"PRIMARY Si\\nSECONDARY C\", visible: true }"
```

**atom_fill options:**
- `passivate: true` — Add hydrogen atoms to dangling bonds
- `remove_single_bond: true` — Recursively remove atoms with only one bond
- `reconstruct: true` — Enable surface reconstruction (cubic diamond (100) 2×1 only)
- `element_values: "..."` — Override motif parameter elements (newline-separated `PARAM_NAME Element`)
- `motif_offset: (x, y, z)` — Fractional offset (0-1) to adjust cut position

### Parametric design with custom nodes

Custom nodes are created by defining subnetworks:

1. Create a node network named `my_shape`
2. Add `parameter` nodes to define inputs (each parameter becomes an input pin)
3. Set the network's output node via `output <node_id>`
4. Use `my_shape` as a node type in other networks

```bash
# In network "scaled_sphere":
atomcad-cli edit --code="
size = parameter { name: \"size\", type: \"Int\", default: 5 }
s = sphere { radius: size }
output s
"

# Then use it elsewhere:
atomcad-cli edit --code="part1 = scaled_sphere { size: 10, visible: true }"
```

### Functional programming with map

The `map` node applies a function to each array element. Combined with partial application:

```bash
# range creates [0, 1, 2, ...] array
# pattern is a custom node with inputs: index (Int), gap (Int)
# When wired to map's f input, gap is bound; only index varies
atomcad-cli edit --code="
r = range { start: 0, count: 5, step: 1 }
result = map { xs: r, f: pattern, gap: 3, visible: true }
"
```

Extra parameters beyond the expected function signature are bound at wire-time (partial application), enabling parametric patterns.

### Mathematical expressions with expr

The `expr` node evaluates mathematical expressions with dynamic input pins:

```bash
atomcad-cli edit --code="
x = int { value: 5 }
y = int { value: 3 }
result = expr { expression: \"x * 2 + y\", x: x, y: y }
"
```

**Supported in expr:**
- Arithmetic: `+`, `-`, `*`, `/`, `%`, `^` (exponent)
- Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`
- Logic: `&&`, `||`, `!`
- Conditionals: `if condition then value1 else value2`
- Vectors: `vec2(x,y)`, `vec3(x,y,z)`, `ivec2`, `ivec3`, member access `.x`, `.y`, `.z`
- Functions: `sin`, `cos`, `tan`, `sqrt`, `abs`, `floor`, `ceil`, `round`, `length2`, `length3`, `normalize2`, `normalize3`, `dot2`, `dot3`, `cross`, `distance2`, `distance3`

## Important Notes

- **facet_shell:** Currently only works correctly with cubic unit cells
- **lattice_move/lattice_rot:** Discrete lattice transformations (integers only). For continuous transforms on atomic structures, use `atom_trans` instead
- **half_space:** Creates infinite half-spaces; useful for clipping geometry via intersection
- **Lone atoms:** `atom_fill` automatically removes atoms with zero bonds after the geometry cut

## See Also

- `atomcad-cli describe <node>` for detailed node documentation
- `atomcad-cli nodes` to browse available node types
- `references/text-format.md` for complete text format specification
- `references/data-types.md` for detailed type system documentation
