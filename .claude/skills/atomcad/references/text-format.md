# atomCAD Text Format Specification

Complete specification for atomCAD's text-based node network format.

## Overview

The text format enables programmatic creation and modification of node networks. Each line represents one operation: creating a node, setting the output, or deleting a node.

## Node Creation Syntax

```
<node_id> = <node_type> { <inputs> }
```

### Node ID Rules

- Must be unique within the network
- Can contain letters, numbers, and underscores
- Cannot start with a number
- Case-sensitive

### Examples

```
# Minimal (no inputs specified, uses defaults)
sphere1 = sphere {}

# With inputs
sphere1 = sphere { center: (0, 0, 0), radius: 5 }

# With visibility
sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }
```

## Input Syntax

Inputs are specified as key-value pairs inside braces, separated by commas.

```
{ key1: value1, key2: value2, ... }
```

### Value Types

| Type | Syntax | Examples |
|------|--------|----------|
| Integer | Decimal number | `42`, `-10`, `0` |
| Float | Decimal with point or exponent | `3.14`, `-1.5`, `1e-3`, `.5` |
| Boolean | `true` or `false` | `true`, `false` |
| String | Double-quoted | `"hello"`, `"path/to/file.xyz"` |
| Vec2/IVec2 | 2-tuple | `(1, 2)`, `(0.5, -1.0)` |
| Vec3/IVec3 | 3-tuple | `(1, 2, 3)`, `(0.0, 0.0, 0.0)` |
| Array | Bracketed list | `[1, 2, 3]`, `[node1, node2]` |
| Node reference | Node ID | `sphere1`, `my_shape` |

### Special Inputs

- `visible: true/false` - Controls whether the node's output is rendered in the viewport

## Wire Connections

Wires are created implicitly by referencing node IDs as input values:

```
# Create two shapes
sphere1 = sphere { radius: 5 }
cuboid1 = cuboid { extent: (10, 10, 10) }

# Wire them to a union node (shapes input references the nodes)
result = union { shapes: [sphere1, cuboid1] }
```

For single-value inputs:
```
filled = atom_fill { shape: sphere1 }
```

For array inputs (multiple wires):
```
combined = union { shapes: [part1, part2, part3] }
```

## Output Node Syntax

Sets which node provides the network's output value (for use as a custom node):

```
output <node_id>
```

Example:
```
sphere1 = sphere { radius: 5 }
output sphere1
```

## Delete Syntax

Removes a node and its connections:

```
delete <node_id>
```

Example:
```
delete sphere1
```

Deleting a node also removes any wires connected to it.

## Comments

Lines starting with `#` are comments:

```
# This is a comment
sphere1 = sphere { radius: 5 }  # Inline comments are NOT supported
```

## Multi-line Input

When using `atomcad-cli edit` without `--code`, input is read from stdin:

- Enter text format commands, one per line
- End input with an empty line or a line containing only `.`
- Press Ctrl+C to cancel without applying changes

Example session:
```
$ atomcad-cli edit
sphere1 = sphere { radius: 5 }
cuboid1 = cuboid { extent: (10, 10, 10) }
result = union { shapes: [sphere1, cuboid1], visible: true }
.
```

## Edit vs Replace Mode

### Edit Mode (default)
- Adds new nodes to the existing network
- Updates inputs of existing nodes (matched by ID)
- Does not remove nodes not mentioned

```bash
atomcad-cli edit --code="sphere1 = sphere { radius: 10 }"
```

### Replace Mode
- Clears the entire network first
- Creates only the nodes specified

```bash
atomcad-cli edit --replace --code="sphere1 = sphere { radius: 10 }"
```

## Node ID Reuse

When editing (not replacing), if a node ID already exists:
- The existing node is updated with the new input values
- The node type cannot be changed (create a new node instead)
- Existing wires to/from the node are preserved unless overwritten

## Error Handling

Common errors and their causes:

| Error | Cause |
|-------|-------|
| Unknown node type | Node type doesn't exist (check spelling, use `atomcad-cli nodes`) |
| Unknown input | Input name not valid for this node type (use `atomcad-cli describe`) |
| Type mismatch | Value type doesn't match expected input type |
| Unknown node reference | Referenced node ID doesn't exist |
| Duplicate node ID | In replace mode, same ID used twice |
| Cycle detected | Wire connections form a cycle (not allowed in DAG) |

## Complete Example

```
# Create a hollowed cube with atoms

# Base geometry
outer = cuboid { min_corner: (0, 0, 0), extent: (20, 20, 20) }
inner = cuboid { min_corner: (2, 2, 2), extent: (16, 16, 16) }

# Boolean difference to create hollow shell
shell = diff { base: [outer], sub: [inner] }

# Fill with atoms
atoms = atom_fill { shape: shell, passivate: true, visible: true }

# Set as network output
output atoms
```
