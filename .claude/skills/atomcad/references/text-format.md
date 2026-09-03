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
| Type (for `data_type`, `input_type`, `element_type`, …) | As `query` prints it | `Int`, `[String]`, `HasStructure -> HasStructure`, `(Int, Float) -> Bool`, `() -> Int`, `Iter[Int]`, `Optional[Vec3]`, `Record(ElementMapping)`, `[HasStructure -> HasStructure]` |

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

### Removing a wire

Mentioning an input assigns its **whole** wire set, so you remove a wire by
saying what the input should be instead:

```
# Was `radius: r`; now a stored value, and the wire to r is gone
sphere1 = sphere { radius: 5 }

# Was [part1, part2, part3]; now just part1 — the other two are disconnected
combined = union { shapes: [part1] }

# Disconnect an array input entirely
combined = union { shapes: [] }
```

An input you do **not** mention keeps its wire. `delete <node_id>` is the only
way to remove wires without naming the input they land on.

Two cases where a literal does not remove a wire, because the literal is not
applied either: a **wire-only** input (a pin with no stored-value backing, e.g.
`half_plane.m_index`) warns that the value was ignored and keeps its wire; and a
stored property that is not an input at all (e.g. `polygon.vertices`) has no
wire to remove.

## Zone Bodies

Six node types carry an **inline body** — a nested network of their own:
`map`, `filter`, `fold`, `foreach`, `zip_with` and `closure`. The body is
written as a `body { … }` **block** inside the node's braces:

```
m1 = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, b: ^scale, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    output d
  }
}
```

Syntax rules:

- `body` is followed by `{`, **not** `:` — it is a block, not a property value.
  (This is what keeps it unambiguous against an anonymous record literal, which
  is always in property-value position after a `:`.)
- The items inside a block are **statements**, so they carry no separating
  commas; the properties around it still do. The block may appear anywhere in
  the property list, and a node has at most one.
- A statement carrying a body spans multiple lines when serialized. A consumer
  that splits query output into per-node statements must brace-match, not split
  on newlines.
- An **empty** body serializes as no block at all.
- A `body { … }` block on a node type that owns no body is refused with a
  "has no body" error, not silently ignored. Likewise a `$name` written outside
  any body is reported rather than dropped.

### Outward references

Inside a body, four reference forms reach outward. With `k` = the number of
leading `^` characters:

| Written | Resolves to | Wire encoding |
|---------|-------------|---------------|
| `n` | a node in this body | `NodeOutput`, depth 0 |
| `^n` | a node one scope out | `NodeOutput`, depth 1 |
| `^^n` | a node two scopes out | `NodeOutput`, depth 2 |
| `$element` | this body's own owner's iteration value | `ZoneInput`, depth 1 |
| `^$element` | the enclosing HOF's iteration value | `ZoneInput`, depth 2 |
| `^^$element` | two HOFs out | `ZoneInput`, depth 3 |

One rule generates the table: **`k` carets → `NodeOutput` at depth `k`;
a `$` prefix → `ZoneInput` at depth `k + 1`.** The `+ 1` is a genuine asymmetry
in the underlying model — a `NodeOutput` depth counts *networks* (0 = this body)
while a `ZoneInput` depth counts *owning-HOF body frames* (1 = this body's own
owner).

A wire that crosses the body boundary is a **capture**. `^$element` is both a
zone input and a capture: it is the outer HOF's per-iteration value, frozen once
per instantiation of the inner body.

A **bare** name is resolved lexically: this body first, then each enclosing
scope, inner shadowing outer. A `$name` is **never** resolved outward — that
would silently turn a per-iteration read into a capture — so an outer element
must be written with an explicit `^`. Serialization always emits the explicit
`^` form, so a round trip never depends on shadowing.

Reaching past the outermost scope (`^^^n` at depth 2, `^$x` at the top level) is
an error, not a silent drop.

### Zone pin names

They are not uniform — only `map` and `zip_with` call the result `result`:

| Node | Zone inputs (`$…`) | Zone output (`output …`) |
|------|--------------------|--------------------------|
| `map` | `element` | `result` |
| `filter` | `element` | `keep` (`Bool`) |
| `foreach` | `element` | `out` (`Unit`) |
| `fold` | `acc`, `element` | `new_acc` |
| `zip_with` | `element1` … `elementN` | `result` |
| `closure` | its own `params` | `new_acc` (fold kind), `out` (foreach kind), else `result` |

### `output` inside a block

An `output <name>` statement **inside** a body block sets the owning node's
zone-output wire — *not* the body network's own return node. The same keyword at
the top level sets the network's return node; the position is what
distinguishes them.

### `closure` properties

`closure` is the one zone-bearing type whose interface is data, so it carries
three text properties:

| Property | Value | Meaning |
|----------|-------|---------|
| `kind` | `"map"`, `"filter"`, `"fold"`, `"foreach"`, `"custom"` | the shape template |
| `params` | array of strings | parameter names (`kind: "custom"` only); what the body's `$x` binds to |
| `type_args` | array of data types | the free type slots for the kind |

```
f1 = closure {
  kind: "custom",
  params: ["x", "y"],
  type_args: [Int, Int, Int],
  body {
    p = expr { a: $x, b: $y, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    output p
  }
}
```

Properties are always applied **before** the body block, whatever order they are
written in, so `$x` binds even when `params:` follows `body { … }`.

### `f:` overrides `body`

Every HOF also has an optional `f:` function pin. When `f` is wired it drives
the node and the inline body is ignored. Serialization emits both when both
exist; do not author a body for a node whose `f:` you are also wiring.

### Path-addressed statements

The second edit granularity. **The prefix selects the scope, the last segment
names the node in it** — one rule for all three statement forms:

```
m1/d = expr { a: $element, expression: "a * 4", parameters: [{ name: "a", data_type: Int }] }
output m1/d                          # re-point m1's zone-output wire
delete m1/e                          # remove one body node
outer/inner/n = int { value: 2 }     # depth 2
```

- The separator is `/`, never `.` — `.` already means pin access
  (`atom_edit.diff`), so `output m1.d` would be ambiguous.
- Paths split on the `/` token only, so a backtick-quoted segment containing a
  slash (`` m1/`a/b` ``) stays one segment.
- References inside a path statement are **relative to the addressed scope**:
  bare names resolve in that body, `$…` are its zone inputs, `^…` walks outward
  from it. `m1/x = …` therefore means exactly what the same statement means
  written inside `m1`'s block.
- A path never *creates* the scope it addresses. A segment naming no node, or
  naming a node that owns no body, is an error.
- Serialization never emits paths — they are input-only sugar, so query output
  keeps one statement shape per node.

### What a block assigns, and what a path merges

| Statement | Effect on the body | Effect on the zone-output wire |
|-----------|--------------------|--------------------------------|
| `m1 = map { … }` with no `body` block | untouched | untouched |
| `m1 = map { … body { … output e } }` | replaced wholesale | set to `e` |
| `m1 = map { … body { … } }` with no `output` | replaced wholesale | **cleared** |
| `m1 = map { … body { } }` | emptied | cleared |
| `m1/x = …` | creates or updates `x` only | untouched |
| `output m1/x` | untouched | set to `x` |
| `delete m1/x` | removes `x` | cleared **iff** `x` was its source |
| `delete m1` | node and body go together | — |

A mentioned `body { … }` block assigns the **whole** body — the same rule as any
other property — and it is total over the zone-output wire too: a block with no
`output` clears it. That is the only reading under which
`query` → `edit --replace` is exact.

Statements apply in source order, so a script containing both
`m1 = map { body { … } }` and `m1/x = …` wipes and then merges. Do not straddle
the two unless that is what you want.

**Identity and layout.** Rebuilt body nodes are matched to existing ones by
name, per scope, and keep their node id and canvas position. Names are unique
per scope only, so `m1/a` and `m2/a` are different nodes and both may exist.

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

Inside a `body { … }` block the same statement sets the owning node's
zone-output wire instead. A path form addresses one from outside the block:

```
output m1/e        # sets m1's zone-output wire to the body node `e`
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

Deleting a node also removes any wires connected to it. A path deletes one body
node; deleting a zone-bearing node takes its whole body with it:

```
delete m1/e        # removes `e` from m1's body
delete m1          # removes m1 and everything in its body
```

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
- Inputs you mention are reassigned; inputs you leave out keep their wires. See
  [Removing a wire](#removing-a-wire)
- Wires *from* the node (to its consumers) are untouched either way

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
