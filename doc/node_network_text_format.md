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

Both take a **path** when they address a node inside a zone body — `output m1/e`,
`delete m1/e`. See [Zone Bodies](#zone-bodies).

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

### Multi-Output Pin References

A node can expose more than one output pin. To reference a non-default output, suffix the source name with `.pinname`:

```
edit1 = atom_edit { base: input1 }
applied = apply_diff { base: input1, diff: edit1.diff }
```

Here `edit1.diff` selects the `diff` output pin (pin index 1) of `atom_edit`. An unqualified reference like `edit1` always selects the primary output (pin index 0), so `edit1` and `edit1.result` are equivalent for `atom_edit`.

**Rules:**
- Unqualified references default to pin 0. This keeps the format backward compatible with single-output networks.
- Qualified references use the pin name as defined by the node type (e.g. `atom_edit` exposes `result` and `diff`).
- An unknown pin name (`foo.does_not_exist`) is reported as an editor error during validation.

**Serialization:** When the network is serialized back to text, the `.pinname` suffix is emitted only for pin indices greater than 0. Wires from pin 0 always serialize unqualified.

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

**Serialization rule**: When a parameter has both a stored default value and an input connection, only the connection is serialized (the stored value is omitted). This keeps the format clean and unambiguous for LLM consumption. The connection represents the actual runtime value.

**Edit semantics**: A property you mention assigns that pin's **whole** set of inbound wires — whatever the statement names replaces what was there.

- `radius: int1` connects the pin to `int1`, replacing any wire it had.
- `radius: 5` removes any existing connection and sets the stored value. (The wire otherwise keeps winning at evaluation and the serialization rule above hides the stored value, so a literal that left the wire in place would be a silent no-op.)
- `shapes: [a, b]` on an array pin sets the wire list to exactly `a` and `b` — a source that was there and is not named is disconnected. `shapes: []` disconnects the pin entirely; an array pin has no stored-value form, so `[]` is how you empty one.

A property you **omit** is left alone, wire and all — that is what makes an incremental edit incremental. So the only pin an edit ever disconnects is one it names.

Two pins are deliberately exempt from the literal rule, because their literal is not applied either:

- a **wire-only** pin (a parameter with no stored-value backing, such as `half_plane.m_index`) warns that the literal was ignored and keeps its wire — clearing it would leave the pin with neither a wire nor a value;
- a **stored property that is not a pin** (such as `polygon.vertices`) names no wire to begin with.

## Visibility

The `visible` property controls whether a node's output is rendered in the viewport:

```
# Visible node - rendered in the 3D viewport
sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }

# Invisible node - participates in computations but not rendered
int1 = int { value: 42 }
```

**Query behavior**: Only visible nodes include `visible: true`. Invisible nodes omit the property entirely.

**Edit behavior**:
- `visible: true` → Node is displayed (added to `displayed_node_ids`)
- `visible` property omitted → Node is invisible (default)
- `visible: false` → Explicitly invisible (same as omitting)

**Design rationale**: Defaulting to invisible keeps the format compact. The AI must be deliberate when it wants to display something. This avoids cluttering the viewport with intermediate computation nodes.

## Comment Anchors

A `comment` node's `on` property records what the note documents — a node, or a
single wire — so the association survives an edit or an automatic re-layout
instead of living only in the reader's head:

```
note1 = Comment { text: "the chassis", on: mybox }
note2 = Comment { text: "passivation", on: mybox -> union.a }
note3 = Comment { text: "the diff branch", on: edit1.diff -> applied.diff }
note4 = Comment { text: "both", on: [mybox, sphere1 -> union.a] }
```

- `on: <node>` — a **node anchor**.
- `on: <source>[.<pin>] -> <dest>.<param>` — a **wire anchor**. The source side
  is an ordinary node reference (optionally qualified by an output pin name);
  the destination side names the receiving parameter. This is what makes a note
  about one branch of a fan-out expressible: an anchor on `mybox` alone cannot
  say *which* consumer the note is about.
- An array holds several anchors, mixing both forms freely.
- `on: []` clears a comment's anchors.

`on` is only accepted on `comment` nodes, and an anchor may only name a node or
wire in the **same network** as the comment.

**Query behavior**: Only anchored comments include `on`. A single anchor is
written bare, several as an array.

**Edit behavior**: An anchor that cannot be resolved — an unknown name, or a
wire that does not exist between the two named endpoints — produces a **warning
and is dropped**, never re-pointed at something nearby. A leader line pointing
at the wrong wire is documentation that actively lies, so the only two outcomes
are "the same target" or "no anchor". A comment statement that says nothing
about `on` leaves its existing anchors alone.

## Zone Bodies

Six node types own an **inline body** — a nested network that belongs to the
node itself: the higher-order functions `map`, `filter`, `fold`, `foreach` and
`zip_with`, and the `closure` node. The body is projected into the text format
as a `body { … }` **block** among the node's properties.

```
r = range { start: 0, count: 5, step: 1 }
scale = int { value: 3 }

m1 = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, b: ^scale, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    e = expr { a: d, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }
    output e
  }
}

output m1
```

`body` is followed by `{`, never by `:` — it is a **block**, not a property
value. That single missing colon is what tells it apart from an anonymous record
literal (which is always in property-value position) on one token of lookahead.
The items inside are statements, so they carry no separating commas; the
properties around the block still do. A node has at most one body, and the block
may appear anywhere in the property list.

A statement carrying a body therefore spans several lines. Any consumer that
assumed "one statement = one line" must brace-match instead.

An **empty** body emits no block at all. That is the shape of an HOF driven
through its `f:` pin, and it means an omitted `body` and an emitted-nothing body
agree — the round trip stays exact.

A `body { … }` block written on a node type that owns no body is refused with a
"has no body" error rather than ignored, and a `$name` written outside any body
is reported rather than dropped.

### Referring outward from inside a body

A body communicates with the outside in exactly four ways, so there are exactly
four reference spellings. With `k` = the number of leading `^`:

| Written | Resolves to | Wire |
|---------|-------------|------|
| `n` | a node in this body | `NodeOutput`, depth 0 |
| `^n` | a node one scope out | `NodeOutput`, depth 1 |
| `^^n` | a node two scopes out | `NodeOutput`, depth 2 |
| `$element` | this body's own owner's iteration value | `ZoneInput`, depth 1 |
| `^$element` | the enclosing HOF's iteration value | `ZoneInput`, depth 2 |
| `^^$element` | two HOFs out | `ZoneInput`, depth 3 |

One rule generates the table: **`k` carets → `NodeOutput` at depth `k`; a `$`
prefix → `ZoneInput` at depth `k + 1`.**

`^` is a *scope* operator — "go up one level" — and what follows names something
in the scope it lands on. The `+ 1` is not a quirk of the format: a `NodeOutput`
depth counts networks (`0` is this body), while a `ZoneInput` depth counts
owning-HOF body frames (`1` is this body's own owner), so `$name` on its own
already addresses one level and each caret steps one owner further out.

A wire crossing the body boundary is a **capture** — an outer value frozen once
per instantiation of the body rather than recomputed per iteration.
`^$element` is a capture in exactly that sense, which is why it carries the same
`^`.

Nested bodies compose:

```
outer = map {
  xs: rows,
  body {
    inner = map {
      xs: $element,
      body {
        p = expr { a: $element, b: ^$element, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
        output p
      }
    }
    output inner
  }
}
```

`$element` in the inner body is the inner `map`'s element; `^$element` is the
outer `map`'s. Both are `ZoneInput` wires — they differ only in depth.

**Resolution of a bare name is lexical**: this body first, then each enclosing
scope outward, inner shadowing outer. A `$name` is **never** resolved outward —
silently promoting a per-iteration read into a capture would change evaluation
semantics, not just the referent — so an outer element always needs an explicit
`^`. The serializer always emits the explicit `^` form, so a round trip never
depends on shadowing.

Reaching past the outermost scope is an error, not a silent drop.

Names are unique **per scope**, not globally: `m1/a` and `m2/a` are different
nodes and both may exist. `visible: true` likewise works per scope.

### `output` inside a block

An `output <name>` statement inside a body block sets the **owning node's**
zone-output wire — not the body network's own return node. The same keyword at
the top level sets the network's return node. Position is what distinguishes
them.

### Zone pin names

Zone pin names are not uniform, so read them from the node type rather than
assuming `element` / `result`:

| Node | Zone inputs (`$…`) | Zone output (`output …`) |
|------|--------------------|--------------------------|
| `map` | `element` | `result` |
| `filter` | `element` | `keep` (`Bool`) |
| `foreach` | `element` | `out` (`Unit`) |
| `fold` | `acc`, `element` | `new_acc` |
| `zip_with` | `element1` … `elementN` | `result` |
| `closure` | its own `params` | `new_acc` (fold kind), `out` (foreach kind), else `result` |

### `closure` properties

`closure` is the one zone-bearing type whose interface is data rather than
fixed by the node type, so it carries three text properties:

| Property | Value | Meaning |
|----------|-------|---------|
| `kind` | `"map"`, `"filter"`, `"fold"`, `"foreach"`, `"custom"` | the shape template |
| `params` | array of strings | parameter names (`kind: "custom"` only) — what the body's `$x` binds to |
| `type_args` | array of data types | the kind's free type slots |

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

A statement's properties are always applied **before** its body block, whatever
order they are written in, so `$x` binds to parameter 0 even when `params:`
follows the block.

### `f:` overrides `body`

Every HOF also exposes an optional `f:` function pin. When `f` is wired it drives
the node and the inline body is ignored at evaluation. Serialization emits both
when both exist (faithfulness over tidiness), so a network can legitimately show
a `body { … }` that never runs.

### Path-addressed statements

The second edit granularity, alongside the whole-body block. **The prefix selects
the scope and the last segment names the node in it** — one rule, applied to all
three statement forms:

```
m1/d = expr { a: $element, expression: "a * 4", parameters: [{ name: "a", data_type: Int }] }
output m1/d                          # re-point m1's zone-output wire
delete m1/e                          # remove one body node
outer/inner/n = int { value: 2 }     # depth 2
```

- The separator is `/`, never `.`: `.` already means pin access in value
  position, so `output m1.d` would be ambiguous between "scope `m1`, node `d`"
  and "the `d` output pin of `m1`".
- Paths split on the `/` token only, so a backtick-quoted segment containing a
  slash (`` m1/`a/b` ``) remains a single segment.
- Everything inside a path statement is **relative to the scope it lands on**:
  bare names resolve in that body, `$…` are its zone inputs, `^…` walks outward
  from it. `m1/x = …` means exactly what the same statement means written inside
  `m1`'s block.
- A path never *creates* the scope it addresses, and never guesses a node type
  for it. A segment naming no node, or naming a node that owns no body, is an
  error rather than a silent no-op.

The serializer emits only the block form; paths are input-only sugar, which keeps
query output single-valued and one node to one statement shape.

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
export1 = export_atoms { molecule: fill1, file_name: "output.xyz" }
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
| `union1 = union { shapes: [a, b] }` | Yes, inputs changed | Set the pin's wires to exactly `a` and `b` |
| `sphere1 = sphere { radius: 4.0 }` | Yes, `radius` was wired | Disconnect `radius`, store `4.0` |
| `union1 = union { shapes: [] }` | Yes | Disconnect the pin |
| `delete box1` | Yes | Remove node and all connections |
| `m1 = map { body { ... } }` | Yes | Replace `m1`'s **whole** body; set its zone-output wire from the block's `output` |
| `m1 = map { xs: r }` (no `body`) | Yes | Leave the body alone |
| `m1/x = ...` | — | Merge into `m1`'s body: create or update `x`, siblings untouched |
| `output m1/x` | — | Set `m1`'s zone-output wire to body node `x` |
| `delete m1/x` | — | Remove `x` from `m1`'s body |

**Nodes not mentioned in an edit command remain unchanged**, and so do properties not mentioned on a node that is. See *Edit semantics* above for how a mentioned property assigns its pin.

Deleting a node is the only way to remove a wire *without* naming the pin it lands on.

A `body { ... }` block follows the same "a mentioned property assigns its whole
value" rule, and it is total over the owning node's **zone-output wire** too: a
block carrying no `output` statement *clears* that wire, and `body { }` empties
the body and clears it. That is the only reading under which
query → `edit --replace` is exact. A path-addressed statement is surgical by
contrast — it never touches the zone-output wire unless it is itself an
`output m1/x`, or a `delete m1/x` that removes the wire's source (which clears
it rather than leaving it dangling).

Statements apply in source order, so a script containing both
`m1 = map { body { ... } }` and `m1/x = ...` wipes and then merges. This needs no
special rule, but straddling the two forms for one node is an easy way to lose
body nodes by accident.

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
document     := statement*
statement    := assignment | output-stmt | delete-stmt | description | summary
assignment   := path '=' type '{' block-items '}'
output-stmt  := 'output' path
delete-stmt  := 'delete' path
comment      := '#' any-text-to-eol
path         := (name '/')* name
block-items  := (block-item (',' block-item)* ','?)?
block-item   := prop | body
prop         := name ':' value
body         := 'body' '{' statement* '}'
value        := literal | source-ref | array | object
source-ref   := '^'* ( '$' name | '@' name | name ('.' name)? )
literal      := bool | int | float | string | vector
bool         := 'true' | 'false'
int          := [+-]? digit+
float        := [+-]? digit* '.' digit+ ([eE] [+-]? digit+)?
             |  digit+ [eE] [+-]? digit+
string       := '"' chars '"' | '"""' multiline-chars '"""'
vector       := '(' number ',' number (',' number)? ')'
array        := '[' (value (',' value)*)? ']'
object       := '{' props '}'
props        := (prop (',' prop)* ','?)?
name         := [a-zA-Z_][a-zA-Z0-9_]* | '`' relaxed-chars '`'
type         := [a-z][a-z0-9_]*
number       := int | float
```

Notes on the zone-body productions:

- `body` is distinguished from a `prop` on **one** token of lookahead: an
  identifier followed by `:` opens a property, one followed by `{` opens the
  body. No backtracking.
- A `body` block contains *statements*, not properties, so its items are not
  comma-separated. Its `output` sets the owning node's zone-output wire.
- `'^'*` counts carets: `k` carets name a node `k` scopes out, and a `$` prefix
  names a zone input `k + 1` frames out. See
  [Zone Bodies](#referring-outward-from-inside-a-body).
- A `path` splits on the `/` **token** only, so a backtick-quoted segment may
  contain a slash.

## Examples

### Simple Geometry with Boolean Operations

```
# Create two shapes (invisible - intermediate computation)
sphere1 = sphere { center: (0, 0, 0), radius: 8 }
box1 = cuboid { min_corner: (-3, -3, -3), extent: (6, 6, 6) }

# Subtract box from sphere (visible - final result)
diff1 = diff { base: sphere1, sub: box1, visible: true }

output diff1
```

### Atomic Structure

```
# Custom unit cell
uc1 = unit_cell { a: 5.43, b: 5.43, c: 5.43, alpha: 90, beta: 90, gamma: 90 }

# Geometry
sphere1 = sphere { center: (0, 0, 0), radius: 5, unit_cell: uc1 }

# Fill with silicon (visible - final atomic structure)
fill1 = atom_fill {
  shape: sphere1,
  parameter_element_value_definition: """
    PRIMARY Si
    SECONDARY Si
  """,
  passivate: true,
  visible: true
}

output fill1
```

### Multi-Output Pin Reference

```
# atom_edit exposes two output pins: "result" (pin 0) and "diff" (pin 1)
edit1 = atom_edit { base: imported }

# Re-apply the same diff to a different base
applied = apply_diff { base: other_base, diff: edit1.diff, visible: true }

output applied
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

Because a position can never be *written*, it can only be **carried**. Before an
edit runs — and, in replace mode, before the network is cleared — the editor
snapshots every node's `(scope path, name) -> (id, position)`. A node it then
creates under a name that was in the snapshot inherits that node's id and
position instead of being auto-placed. This is what makes an `edit --replace` of
unchanged query output a no-op on the canvas, inside zone bodies as well as at
the top level.
