# Node Network Text Format

*A text-based format for representing and editing atomCAD node networks.*

## Overview

This document specifies the text format used for AI assistant integration with atomCAD. The format serves two purposes:

1. **Query results**: atomCAD serializes the active node network to this format
2. **Edit commands**: AI assistants send modifications using this same format

The format is designed to be:
- Human-readable and LLM-friendly
- Unambiguous and parseable
- Expressive enough to represent all node types and connections

## Syntax

### Basic Structure

A document consists of lines, each containing an assignment, statement, or comment:

```
# This is a comment
sphere1 = sphere { center: (0, 0, 0), radius: 5 }
union1 = union { shapes: [sphere1, box1] }
output union1
```

### Assignments

Assignments create or update nodes:

```
name = type { property: value, property: value }
```

- **name**: Identifier for the node (e.g., `sphere1`, `myCustomNode`)
- **type**: Node type name (e.g., `sphere`, `cuboid`, `union`)
- **properties**: Key-value pairs for node data and input connections

### Statements

```
output nodename    # Set the return/output node of the network
delete nodename    # Remove a node and its connections
```

### Comments

```
# Single-line comments start with #
```

## Data Types and Literals

### Primitive Types

| Type | Syntax | Examples |
|------|--------|----------|
| `Bool` | `true` or `false` | `true`, `false` |
| `Int` | Integer literal | `42`, `-10`, `0` |
| `Float` | Decimal or scientific notation | `3.14`, `-1.5`, `2.5e-3`, `1.0` |
| `String` | Double-quoted | `"hello"`, `"path/to/file.xyz"` |

### Vector Types

Vectors use parenthesized comma-separated components:

| Type | Syntax | Examples |
|------|--------|----------|
| `IVec2` | `(int, int)` | `(1, 2)`, `(-3, 5)` |
| `IVec3` | `(int, int, int)` | `(1, 2, 3)`, `(0, 0, 0)` |
| `Vec2` | `(float, float)` | `(1.0, 2.5)`, `(0.0, -1.5)` |
| `Vec3` | `(float, float, float)` | `(1.0, 2.5, 3.0)` |

**Type inference rule**: If all components are integers without decimal points, the type is `IVec2`/`IVec3`. If any component has a decimal point, the type is `Vec2`/`Vec3`.

```
(1, 2, 3)       # IVec3
(1.0, 2.0, 3.0) # Vec3
(1, 2.0, 3)     # Vec3 (mixed → float)
```

### Arrays

Arrays use square brackets:

```
[1, 2, 3]                    # Array of Int
[sphere1, box1, cylinder1]   # Array of node references
[(0, 0, 0), (1, 1, 1)]       # Array of IVec3
```

### Multi-line Strings

For properties containing multi-line text (e.g., `expr` expressions, `motif` definitions), use triple-quoted strings:

```
motif1 = motif {
  definition: """
    PARAM PRIMARY C
    PARAM SECONDARY C
    SITE CORNER PRIMARY 0 0 0
    SITE FACE_Z PRIMARY 0.5 0.5 0
    BOND INTERIOR1 ...CORNER
  """
}
```

Triple-quoted strings preserve internal newlines and leading whitespace.

## Node References and Connections

### Regular Output References

Reference a node's evaluated output by its name:

```
union1 = union { shapes: [sphere1, box1] }
```

Here `sphere1` and `box1` refer to the regular output (pin index 0) of those nodes.

### Function Pin References

To reference a node's **function pin** (pin index -1) instead of its evaluated result, prefix with `@`:

```
map1 = map {
  input_type: Int,
  output_type: Geometry,
  xs: range1,
  f: @pattern
}
```

The `@pattern` syntax means "use the `pattern` node as a callable function" rather than evaluating it immediately. This is essential for higher-order functions like `map`.

**When to use `@`:**
- When connecting to a function-typed input pin (e.g., `map.f`)
- When the destination expects a function, not a value

## Properties and Input Pins

The format treats node properties and input pin connections uniformly. Both are specified as key-value pairs:

```
cuboid1 = cuboid {
  min_corner: (0, 0, 0),     # Property (stored in node data)
  extent: (3, 3, 3),         # Property (stored in node data)
  unit_cell: uc1             # Input connection (wire from uc1)
}
```

The parser determines whether a key corresponds to a property or input pin based on the node type definition. Values that are node references create wire connections; literal values set properties.

**Precedence rule**: If both a property and an input connection are specified for the same parameter, the input connection takes precedence at evaluation time.

## Type Annotations

Some nodes have dynamic types that must be specified explicitly:

### Parameter Node

```
size_param = parameter {
  param_name: "size",
  data_type: Int,
  sort_order: 0,
  default: int1              # Optional: connection to default value
}
```

### Expr Node

```
expr1 = expr {
  expression: "x * 2 + y",
  parameters: [
    { name: "x", type: Int },
    { name: "y", type: Float }
  ]
}
```

### Map Node

```
map1 = map {
  input_type: Int,
  output_type: Geometry,
  xs: range1,
  f: @pattern
}
```

## Complete Node Reference

### Math and Programming Nodes

```
# Primitive value nodes
int1 = int { value: 42 }
float1 = float { value: 3.14 }
bool1 = bool { value: true }
string1 = string { value: "hello" }
ivec2_1 = ivec2 { x: 1, y: 2 }
ivec3_1 = ivec3 { x: 1, y: 2, z: 3 }
vec2_1 = vec2 { x: 1.0, y: 2.0 }
vec3_1 = vec3 { x: 1.0, y: 2.0, z: 3.0 }

# Range (for functional programming)
range1 = range { start: 0, step: 1, count: 10 }

# Expression evaluator
expr1 = expr {
  expression: "x * 2 + 1",
  parameters: [{ name: "x", type: Int }]
}

# Higher-order map
map1 = map {
  input_type: Int,
  output_type: Geometry,
  xs: range1,
  f: @some_function
}

# Network parameter (for custom nodes)
param1 = parameter {
  param_name: "radius",
  data_type: Int,
  sort_order: 0
}
```

### 2D Geometry Nodes

```
rect1 = rect { min_corner: (0, 0), extent: (5, 3) }
circle1 = circle { center: (0, 0), radius: 5 }
polygon1 = polygon { vertices: [(0, 0), (3, 0), (1, 2)] }
reg_poly1 = reg_poly { center: (0, 0), radius: 5, num_sides: 6 }
half_plane1 = half_plane { p1: (0, 0), p2: (1, 0) }

union2d1 = union_2d { shapes: [rect1, circle1] }
intersect2d1 = intersect_2d { shapes: [rect1, circle1] }
diff2d1 = diff_2d { base: rect1, sub: circle1 }
```

### 3D Geometry Nodes

```
cuboid1 = cuboid { min_corner: (0, 0, 0), extent: (3, 3, 3) }
sphere1 = sphere { center: (0, 0, 0), radius: 5 }
half_space1 = half_space { center: (0, 0, 0), miller_index: (1, 0, 0), shift: 0 }

extrude1 = extrude { shape_2d: rect1, z_min: 0, z_max: 5 }

union1 = union { shapes: [sphere1, cuboid1] }
intersect1 = intersect { shapes: [sphere1, cuboid1] }
diff1 = diff { base: sphere1, sub: cuboid1 }

lattice_move1 = lattice_move { geometry: sphere1, offset: (1, 0, 0) }
lattice_rot1 = lattice_rot { geometry: sphere1, rotation_index: 0 }
```

### Atomic Structure Nodes

```
# Unit cell definition
uc1 = unit_cell { a: 3.567, b: 3.567, c: 3.567, alpha: 90, beta: 90, gamma: 90 }

# Motif definition
motif1 = motif {
  definition: """
    PARAM PRIMARY C
    PARAM SECONDARY C
    SITE CORNER PRIMARY 0 0 0
    SITE FACE_Z PRIMARY 0.5 0.5 0
  """
}

# Fill geometry with atoms
fill1 = atom_fill {
  shape: sphere1,
  motif: motif1,
  parameter_element_value_definition: """
    PRIMARY Si
    SECONDARY C
  """,
  m_offset: (0.0, 0.0, 0.0),
  passivate: true,
  rm_single: false,
  surf_recon: false
}

# Transform atomic structure
trans1 = atom_trans {
  molecule: fill1,
  translation: (10.0, 0.0, 0.0),
  rotation: (0.0, 0.0, 0.0, 1.0)
}

# Import/export
import1 = import_xyz { filename: "molecule.xyz" }
export1 = export_xyz { molecule: fill1, filename: "output.xyz" }
```

## Edit Semantics

### Edit Modes

The edit command supports two modes:

| Mode | CLI Flag | Behavior |
|------|----------|----------|
| **Incremental** (default) | (none) | Merge changes into existing network |
| **Replace** | `--replace` | Replace entire network with specified content |

### Incremental Mode (Default)

When processing an edit command in incremental mode:

| Statement | Name Exists? | Effect |
|-----------|--------------|--------|
| `sphere1 = sphere { radius: 4.0 }` | Yes | Update properties |
| `cylinder1 = cylinder { ... }` | No | Create new node |
| `union1 = union { shapes: [a, b] }` | Yes, inputs changed | Rewire connections |
| `delete box1` | Yes | Remove node and all connections |

**Nodes not mentioned in an edit command remain unchanged.**

### Replace Mode

When using `--replace`, the entire network is replaced:
- All existing nodes are removed
- Only nodes specified in the edit command are created
- Useful when the AI wants to define a complete network from scratch

```bash
# Incremental: modifies existing network
atomcad-cli edit --code="sphere1 = sphere { radius: 10 }"

# Replace: clears network and creates only what's specified
atomcad-cli edit --replace --code="sphere1 = sphere { radius: 10 }"
```

## Formal Grammar

```
document     := line*
line         := assignment | statement | comment | blank
assignment   := name '=' type '{' props '}'
statement    := 'output' name | 'delete' name
comment      := '#' any-text-to-eol
props        := (prop (',' prop)* ','?)?
prop         := name ':' value
value        := literal | name | '@' name | array | object
literal      := bool | int | float | string | vector
bool         := 'true' | 'false'
int          := [+-]? digit+
float        := [+-]? digit* '.' digit+ ([eE] [+-]? digit+)?
             |  digit+ [eE] [+-]? digit+
string       := '"' chars '"' | '"""' multiline-chars '"""'
vector       := '(' number ',' number (',' number)? ')'
array        := '[' (value (',' value)*)? ']'
object       := '{' props '}'
name         := [a-zA-Z_][a-zA-Z0-9_]*
type         := [a-z][a-z0-9_]*
number       := int | float
```

## Examples

### Simple Geometry with Boolean Operations

```
# Create two shapes
sphere1 = sphere { center: (0, 0, 0), radius: 8 }
box1 = cuboid { min_corner: (-3, -3, -3), extent: (6, 6, 6) }

# Subtract box from sphere
diff1 = diff { base: sphere1, sub: box1 }

output diff1
```

### Atomic Structure

```
# Custom unit cell
uc1 = unit_cell { a: 5.43, b: 5.43, c: 5.43, alpha: 90, beta: 90, gamma: 90 }

# Geometry
sphere1 = sphere { center: (0, 0, 0), radius: 5, unit_cell: uc1 }

# Fill with silicon
fill1 = atom_fill {
  shape: sphere1,
  parameter_element_value_definition: """
    PRIMARY Si
    SECONDARY Si
  """,
  passivate: true
}

output fill1
```

### Functional Programming Pattern

```
# Create a range of integers
range1 = range { start: 0, step: 1, count: 5 }

# Define gap parameter for the pattern
gap_value = int { value: 3 }

# Map the pattern function over the range
# @pattern references the "pattern" network's function pin
result = map {
  input_type: Int,
  output_type: Geometry,
  xs: range1,
  f: @pattern
}

# pattern.gap will be bound to gap_value when the function is created
# This is partial application / closure creation

output result
```

## Implementation Notes

### Name Generation (Query)

When serializing a network to text:
- Names are generated as `{typename}{counter}` (e.g., `sphere1`, `sphere2`)
- Nodes are output in topological order (dependencies before dependents)
- Counter is per-type and increments in topological order

### Name Resolution (Edit)

When parsing an edit command:
- Names must match existing nodes or be new
- Forward references within the same edit are allowed
- Circular references are an error

### Node Positions

Node positions (layout) are **not exposed** in this format:
- The LLM edits semantics (data flow), not visual layout
- New nodes are placed automatically
- Users can manually reorganize after AI edits
