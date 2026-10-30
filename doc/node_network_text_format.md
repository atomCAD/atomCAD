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

Node names are the nodes' `custom_name`s, and `query` prints them verbatim:
within one network (and within one zone body, which is a scope of its own) a
name identifies exactly one node. Every way of creating a node keeps that
true — a copy, a paste or a duplicate carries the source's name and takes the
first free numeric suffix if the destination already holds it, so duplicating
`chassis` gives `chassis_2`, then `chassis_3`.

A name may hold almost any character — `x.shape` and `union#1` are legal, and
`query` backtick-quotes anything it cannot write bare — with two exceptions: a
backtick (the quoting delimiter itself) and a `/` (the separator in a node
*path* such as `m1/d`).

Files written before this rule existed can hold several nodes called
`to_degrees`, or a name with a `/` in it. They are healed when the file is
opened: duplicates get the same ascending-id suffixes the text format always
printed (`to_degrees`, `to_degrees_2`, `to_degrees_3`) and a `/` becomes `_`.
The text therefore reads exactly as it did before, and the names are now what
the nodes actually store.

### Statements

```
output nodename    # Set the return/output node of the network
delete nodename    # Remove a node and its connections
```

Both take a **path** when they address a node inside a zone body — `output m1/e`,
`delete m1/e`. See [Zone Bodies](#zone-bodies).

Three more statements say where nodes go on the canvas — `place`, `arrange` and
`straighten`. They are instructions to the layout, carried out once by the edit
that contains them and never stored. See [Layout Statements](#layout-statements).

### Comments

```
# A # starts a comment that runs to the end of the line
sphere1 = sphere { radius: 5 }   # it can also end a statement
```

A `#` inside a string or a backtick-quoted name is not a comment. `query` does
not print comments back.

## Data Types and Literals

### Primitive Types

| Type | Syntax | Examples |
|------|--------|----------|
| `Bool` | `true` or `false` | `true`, `false` |
| `Int` | Integer literal | `42`, `-10`, `0` |
| `Float` | Decimal or scientific notation; the integer part is required (`0.5`, not `.5`) | `3.14`, `-1.5`, `2.5e-3`, `1.0` |
| `String` | Double-quoted, with escapes `\n` `\r` `\t` `\\` `\"`; triple-quoted for multi-line (below) | `"hello"`, `"path/to/file.xyz"` |

A literal is converted to the type of the property it is written for: an `Int`
literal works on a `Float` property, and a float literal on an `Int` property is
**truncated** (`2.7` → `2`).

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
[sphere1, box1, sphere2]     # Array of node references
[(0, 0, 0), (1, 1, 1)]       # Array of IVec3
```

An array of literals is a value for a list *property* (`polygon.vertices`,
`array.elements`). On an array *input pin* (`union.shapes`) the list names the
wires; such a pin is wire-only and takes no literal values.

### Multi-line Strings

For properties containing multi-line text (e.g., `expr` expressions, `motif` definitions), use triple-quoted strings:

```
motif1 = motif {
  definition: """
    PARAM PRIMARY C
    PARAM SECONDARY C
    SITE CORNER PRIMARY 0 0 0
    SITE INTERIOR1 SECONDARY 0.25 0.25 0.25
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
c = ivec3 { x: 0, y: 0, z: 0 }
pattern = sphere { center: c, pin_roles: { structure: supplied } }
map1 = map {
  input_type: Int,
  output_type: Blueprint,
  xs: range1,
  f: @pattern
}
```

The `@pattern` syntax means "use the `pattern` node as a callable function" rather than evaluating it immediately. This is essential for higher-order functions like `map`.

The function's parameters are the node's **unwired** inputs, in pin order;
wired inputs are baked in. Above, `center` is wired and `structure` is
`supplied` (see [Function Pin Roles](#function-pin-roles)), so `@pattern` is an
`Int -> Blueprint` function of `radius`. `map.f` accepts a function whose
**first** parameter is the element type; any further parameters stay unbound,
and the output becomes a stream of functions of them.

**When to use `@`:**
- When connecting to a function-typed input pin (e.g., `map.f`)
- When the destination expects a function, not a value

### Multi-Output Pin References

A node can expose more than one output pin. To reference a non-default output, suffix the source name with `.pinname`:

```
edit1 = atom_edit { molecule: input1 }
applied = apply_diff { base: input1, diff: edit1.diff }
```

Here `edit1.diff` selects the `diff` output pin (pin index 1) of `atom_edit`. An unqualified reference like `edit1` always selects the primary output (pin index 0), so `edit1` and `edit1.result` are equivalent for `atom_edit`.

**Rules:**
- Unqualified references default to pin 0. This keeps the format backward compatible with single-output networks.
- Qualified references use the pin name as defined by the node type (e.g. `atom_edit` exposes `result` and `diff`).
- An unknown pin name (`foo.does_not_exist`) is reported as an editor error during validation.

**Serialization:** When the network is serialized back to text, the `.pinname` suffix is emitted only for pin indices greater than 0. Wires from pin 0 always serialize unqualified.

### Positional Pins (nodes whose type is unavailable)

A node that refers to a network or record type of a **linked library** that
cannot be found (the library file is missing, or no longer defines it) keeps
its wires, and normally still has pin names: they come from the interface the
file recorded when it was last saved. When no name is known for a pin, the
pin is spelled by its **position**, `@<index>`:

```
f = `demolib.foo` { @0: i1, @1: i2 }
e1 = expr { a: rd.@1, expression: "a + 1", parameters: [...] }
```

`@0: i1` wires `i1` into input pin 0 of `f`; `rd.@1` reads output pin 1 of
`rd`. The positional form is accepted **only** on such a node (anywhere else
it is an error: "positional pin only on a node whose type is unavailable"),
takes only wires, and places the wire without a type check. `query` writes it
wherever no pin name is known, so `query` → `edit --replace` keeps every wire.

## Properties and Input Pins

The format treats node properties and input pin connections uniformly. Both are specified as key-value pairs:

```
cuboid1 = cuboid {
  min_corner: (0, 0, 0),     # Property (stored in node data)
  extent: (3, 3, 3),         # Property (stored in node data)
  structure: s1              # Input connection (wire from s1)
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

**Query behavior**: Only visible nodes include `visible`. Invisible nodes omit the property entirely.

**Edit behavior**:
- `visible: true` → Node is displayed, showing its primary output (pin 0)
- `visible` property omitted → Node is invisible (default)
- `visible: false` → Explicitly invisible (same as omitting)

### Output pins and ghosting

A node with several output pins (`record_destructure`, `structure_unpack`,
`lattice_vecs_unpack`, …) can display any subset of them, and any displayed node can be
drawn as a *ghost* (the semi-transparent display type). The `visible` value
spells the whole display state:

```
d = record_destructure { record: r, visible: true }         # Normal, pin 0 only
d = record_destructure { record: r, visible: [x, y] }       # Normal, exactly these output pins
d = record_destructure { record: r, visible: ghost }        # Ghost, pin 0 only
d = record_destructure { record: r, visible: { pins: [x, y], ghost: true } }
```

Pin names are the node's output-pin names — the same vocabulary the `.pin`
wire syntax uses, so `d.y` in a wire and `y` in the list refer to the same pin.
An unknown name is a warning and the rest of the list still applies.

`query` writes `true` (or `ghost`) whenever only pin 0 is displayed and the
name list only when the displayed set is anything else, so networks that never
touched pin display print exactly as before. A mentioned `visible` assigns the
*whole* state: `visible: true` on a node that showed three pins brings it back
to pin 0, and an omitted `visible` leaves the current pins alone.

**Design rationale**: Defaulting to invisible keeps the format compact. The AI must be deliberate when it wants to display something. This avoids cluttering the viewport with intermediate computation nodes.

## Function Pin Roles

A node's title-bar function pin (`@name`) exposes the node as a function of its
input pins. By default the wiring decides which pins are parameters (unwired)
and which are baked in (wired); a per-pin override (see the reference guide's
*Function output* section) is written as `pin_roles`, keyed by pin name:

```
sm = structure_move { input: c, translation: (1, 0, 0), pin_roles: { input: delayed, translation: supplied } }
```

- `delayed` — always a parameter, even if wired (the wire is a preview).
- `supplied` — always baked in, even if unwired (the stored value applies).
- `auto` is the default and is never written; a pin absent from the map is
  `auto`.

`query` writes `pin_roles` only when some pin is overridden. In an edit, a
mentioned `pin_roles` assigns the whole map — `pin_roles: {}` clears every
override — and an omitted one leaves the node's roles as they are. An unknown
pin name or role is a warning; the other entries still apply.

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

Reaching past the outermost scope (`^^^n` at depth 2, `^$x` at the top level)
is reported, not silently dropped: the edit returns a warning ("reaches past the
top-level network" / "reaches past the outermost enclosing body") and makes no
wire.

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
| `closure` | its own `params` (custom kind), `acc`, `element` (fold kind), else `element` | `new_acc` (fold kind), `out` (foreach kind), else `result` |

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
  slash (`` m1/`a/b` ``) remains a single segment to the parser — but a node
  cannot *be* named `a/b`: a slash is rejected in a node name precisely so a
  path is unambiguous.
- Everything inside a path statement is **relative to the scope it lands on**:
  bare names resolve in that body, `$…` are its zone inputs, `^…` walks outward
  from it. `m1/x = …` means exactly what the same statement means written inside
  `m1`'s block.
- A path never *creates* the scope it addresses, and never guesses a node type
  for it. A segment naming no node, or naming a node that owns no body, is an
  error rather than a silent no-op.

The serializer emits only the block form; paths are input-only sugar, which keeps
query output single-valued and one node to one statement shape.

## Layout Statements

The text has no positions: an edit says *what* the network is, and the layout
fits new nodes in by their wiring. Layout statements let an edit also say
*where*. They act once, inside the edit that contains them — `query` never
prints one back, and nothing about them is saved — so the canvas positions they
produce are the only trace they leave.

```
place      <subject> at (<x>, <y>)          [push <dir>]
place      <nodes> by (<dx>, <dy>)          [push <dir>]
place      <subject> <clause> [, <clause>]  [push <dir>]
arrange    [<name>, …]                      [push <dir>]
straighten [<name>, …]                      [push <dir>]
```

**Coordinates** are canvas pixels, x to the right and y **down**, the same
numbers `atomcad-cli query --layout` prints. A position is a node's top-left
corner. Inside a zone body they are relative to the body's top-left corner.

**Order.** The statements run after the whole network change is applied, in the
order they are written, and each sees the drawing the earlier ones left. Where
they sit among the node statements does not matter. A body's statements go
inside its `body { … }` block, or are path-addressed (`place m1/x below m1/d`).

### Subjects

| Subject | Meaning |
|---|---|
| `n` | one node |
| `[a, b] column` | the nodes stacked top to bottom in written order, lefts aligned, then placed as one box |
| `[a, b] row` | the nodes side by side in written order, tops aligned, then placed as one box |
| `[a, b] column (gap: wide)` | the same, with another gap |
| `[a, b]` | with `by` only: the nodes moved together, as they stand |

### Clauses

| Clause | Sets | Meaning |
|---|---|---|
| `right_of T` | x (and y) | one gap right of `T`; alone, tops aligned with `T` |
| `left_of T` | x (and y) | one gap left of `T`; alone, tops aligned |
| `below T` | y (and x) | one gap under `T`; alone, lefts aligned with `T` |
| `above T` | y (and x) | one gap over `T`; alone, lefts aligned |
| `top_even_with T`, `bottom_even_with T` | y | that edge level with `T`'s |
| `left_even_with T`, `right_even_with T` | x | that edge level with `T`'s |
| `straight_from S` | y | the wire from `S` into the subject runs horizontal |
| `straight_to D` | y | the wire from the subject into `D` runs horizontal |

- **Each axis is stated once.** A lone direction (`right_of T`) supplies the
  other axis too; an explicit clause on that axis replaces it
  (`place n right_of a, bottom_even_with b`). Two clauses for one axis, or an
  axis nothing states (`place n left_even_with a` alone), drop the statement.
- **Several targets**, `a and b`, measure against the box around all of them.
- **`all`** is the extent of the whole drawing (the subject left out), measured
  when the statement runs: `place n below all` starts a new lane under
  everything, left edge level with the drawing's. A node named `all` is written
  `` `all` ``.
- **A node the same edit deletes** can still be a target, at the box it had:
  `delete hole` beside `place r left_even_with hole, below all`.
- **Options** on the four directions: `(gap: G)` replaces the default gap, and
  `(dx: n)` / `(dy: n)` shift the result by a fixed amount, e.g.
  `place n right_of a (gap: wide, dy: 20)`.

**Gaps** are `tight`, `normal` (the default) or `wide` — 25 / 50 / 100 px side
by side, 15 / 30 / 60 px stacked — or a number of pixels.

### `by`

`place n by (0, 40)` moves an existing node 40 px down from wherever it stands
when the statement runs (after anything earlier in the edit moved it).
`place [a, b, c] by (210, 0)` moves several together, keeping their arrangement.
A node the edit creates cannot be moved `by`: it has no position yet.

### `push`: which way the nodes it lands on go

A placed node never yields: whatever it lands on moves out of its way.
`push` chooses the direction.

| | What moves |
|---|---|
| *(no `push`)* | each node landed on goes up or down, whichever is cheaper, and anything it then hits |
| `push down` | the nodes landed on go down, and anything they hit; nothing above moves |
| `push up` | the same, upward |
| `push right` | everything at or right of the subject's left edge shifts right together, just far enough; what feeds the subject stays |

```
place n below h push down           # into a column: the nodes under h make room
place rot right_of slab push right  # into a chain: everything after slab moves along
```

There is no `push left`: to make room before a node, place after the node
before it and `push right`.

### `arrange` and `straighten`

`arrange [a, b, c]` lays out just those nodes by their wiring among themselves,
keeping the group's top-left corner. `straighten [a, b]` moves each listed node
up or down until the wire from its first connected input runs horizontal. They
are the *Arrange selection* and *Straighten wires* commands of the node
network's Arrange menu. Nothing unlisted moves, except to make room.

### What happens afterwards

What a statement placed, moved or arranged — and the nodes it measured against —
stays put for the rest of the edit: the automatic placement of the edit's other
new nodes goes around them. A vertical relation (`below`, `top_even_with`,
`straight_from`, …) holds exactly when the edit is done. A horizontal one keeps
its order and at least its gap; it can only widen, when the layout has to shift
part of the drawing right to keep a wire running left to right.

### When a statement cannot be carried out

A statement that parses but cannot be carried out never fails the edit. It is
dropped, its subject is placed as if it were not there (a new node
automatically, an existing one stays), and the edit result's `layout.warnings`
says which line and why: an unknown node, a node in another scope, an axis
stated twice or not at all, a node placed by two statements, a new node used as
a target before any statement placed it, `by` on a new node, a deleted node as
the subject, or `straight_from` / `straight_to` with no wire between the two. A
body node placed at a negative coordinate is clamped to 0, with a warning.

A statement that does not *parse* — `push left`, `at` together with a clause,
`push` written twice — rejects the whole edit, like any other syntax error.

## Type Annotations

Some nodes have dynamic types that must be specified explicitly:

### Type syntax

A type is written the way `query` prints it:

| Type | Syntax |
|------|--------|
| Builtin | `Int`, `Float`, `Vec3`, `Crystal`, `HasStructure`, … |
| Array | `[Int]`, `[[Int]]` |
| Function | `Int -> Float`, `(Int, Float) -> Bool`, `() -> Int` |
| Iterator / Optional | `Iter[Int]`, `Optional[Vec3]` |
| Named record | `Record(ElementMapping)` — the bare name would be read as a node reference |
| Anonymous record | `{ from: Int, to: Int }` |

These nest freely: `[HasStructure -> HasStructure]` is an array of functions.
The one-element list `[T]` doubles as the array type: on a property that takes
a single type (`data_type`, `input_type`, `element_type`, …) it is `[T]` the
type; on a property that takes a list of types (`closure.type_args`,
`zip_with.lane_types`) it is a list of one.

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
    { name: "x", data_type: Int },
    { name: "y", data_type: Float }
  ]
}
```

**Wiring a name declares it.** On a node that declares its own inputs
(`expr`, and a `record_construct` with no `schema`), a wire to a name the node
does not have yet adds that input, typed by its source, before the wire is
made:

```
a = float { value: 1.5 }
v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
e = expr { expression: "v * a", a: a, v: v }      # a: Float, v: Vec3
r = record_construct { size: a, at: v }           # fields size: Float, at: Vec3
```

The rules:

- Only a single reference declares. A literal (`y: 5`) carries no type, and an
  array of references is several wires; both leave the name undeclared.
- A statement that sets `parameters` (or `fields`) declares exactly what it
  lists, and a wire to any other name is reported as a warning.
- The name is the key as written. It must be an identifier the node does not
  reserve (`pi`, `if`, `expression`, …); otherwise the edit reports why.
- A source whose type depends on inputs declared in the same edit (an `expr`
  fed by another `expr`) is resolved after them, in whatever order the
  statements appear. A source whose type still cannot be worked out is
  reported, naming the input.
- A fresh `expr` has no parameters and the expression `0`. `query` always
  writes `parameters` out, so a `query` → `--replace` round trip never relies
  on this rule.

### Map Node

```
map1 = map {
  input_type: Int,
  output_type: Blueprint,
  xs: range1,
  f: @pattern
}
```

`map` is lazy: its output is an `Iter[T]`. Wire it into
`collect { iter: map1, element_type: Blueprint }` to get an array; `collect`'s
`element_type` (default `Int`) must match the stream's.

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
  parameters: [{ name: "x", data_type: Int }]
}

# Higher-order map (lazy: outputs Iter[Blueprint]; collect it into an array)
map1 = map {
  input_type: Int,
  output_type: Blueprint,
  xs: range1,
  f: @some_function
}
all1 = collect { iter: map1, element_type: Blueprint }

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
reg_poly1 = reg_poly { radius: 5, num_sides: 6 }
half_plane1 = half_plane { point1: (0, 0), point2: (1, 0) }

union2d1 = union_2d { shapes: [rect1, circle1] }
intersect2d1 = intersect_2d { shapes: [rect1, circle1] }
diff2d1 = diff_2d { base: rect1, sub: circle1 }
```

The 2D primitives take an optional `d_plane` (a `DrawingPlane`, from the
`drawing_plane` node); unconnected, they draw on the XY plane.

### 3D Geometry Nodes

All of these output a `Blueprint`. The lattice primitives take integer lattice
coordinates and an optional `structure` input (default: cubic diamond).

```
cuboid1 = cuboid { min_corner: (0, 0, 0), extent: (3, 3, 3) }
sphere1 = sphere { center: (0, 0, 0), radius: 5 }
free_sphere1 = free_sphere { center: (0.0, 0.0, 0.0), radius: 7.5 }   # Cartesian Å
half_space1 = half_space { m_index: (1, 0, 0), center: (0, 0, 0), shift: 0 }

extrude1 = extrude { shape: rect1, height: 5 }

union1 = union { shapes: [sphere1, cuboid1] }
intersect1 = intersect { shapes: [sphere1, cuboid1] }
diff1 = diff { base: sphere1, sub: cuboid1 }

# Discrete lattice moves (Blueprint or Crystal)
moved1 = structure_move { input: sphere1, translation: (1, 0, 0) }
rotated1 = structure_rot { input: sphere1, axis_index: 0, step: 1 }
```

### Atomic Structure Nodes

```
# Crystal structure: lattice vectors + motif
lv1 = lattice_vecs { cell_length_a: 5.43, cell_length_b: 5.43, cell_length_c: 5.43 }
motif1 = motif {
  definition: """
    PARAM PRIMARY C
    PARAM SECONDARY C
    SITE CORNER PRIMARY 0 0 0
    SITE INTERIOR1 SECONDARY 0.25 0.25 0.25
    BOND INTERIOR1 ...CORNER
  """
}
st1 = structure { lattice_vecs: lv1, motif: motif1 }
shape1 = sphere { radius: 3, structure: st1 }

# Carve atoms out of a Blueprint (Blueprint -> Crystal)
crystal1 = materialize {
  shape: shape1,
  parameter_element_value_definition: """
    PRIMARY Si
    SECONDARY C
  """,
  passivate: true,
  rm_single: false,
  surf_recon: false
}

# Continuous moves need a Molecule: leave the lattice first
mol1 = exit_structure { input: crystal1 }
moved_mol1 = free_move { input: mol1, translation: (10.0, 0.0, 0.0) }
rotated_mol1 = free_rot { input: moved_mol1, angle_degrees: 90.0, rot_axis: (0.0, 0.0, 1.0) }

# Import/export
import1 = import_xyz { file_name: "molecule.xyz" }
export1 = export_atoms { molecule: rotated_mol1, file_name: "output.xyz" }
```

## Edit Semantics

### Edit Modes

The edit command supports two modes:

| Mode | CLI Flag | Behavior |
|------|----------|----------|
| **Incremental** (default) | (none) | Merge changes into existing network |
| **Replace** | `--replace` | Replace entire network with specified content |

In both modes an edit is **all or nothing**: if any statement fails (an unknown
node type, a literal of the wrong type, a `delete` or `output` of a missing node,
a path into a missing body), the network is left exactly as it was and the last
error reads *Nothing was applied*. Warnings (an unknown property, a reference to
a missing source node) do not stop the edit. Validation errors are reported
after the edit has applied, and do not undo it.

### Incremental Mode (Default)

When processing an edit command in incremental mode:

| Statement | Name Exists? | Effect |
|-----------|--------------|--------|
| `sphere1 = sphere { radius: 4.0 }` | Yes | Update properties |
| `box2 = cuboid { ... }` | No | Create new node |
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
             |  layout-stmt
layout-stmt  := ('place' | 'arrange' | 'straighten') …   # see Layout Statements
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
int          := '-'? digit+
float        := '-'? digit+ '.' digit+ ([eE] [+-]? digit+)?
             |  '-'? digit+ [eE] [+-]? digit+
string       := '"' chars '"' | '"""' multiline-chars '"""'
vector       := '(' number ',' number (',' number)? ')'
array        := '[' (value (',' value)*)? ']'
object       := '{' props '}'
props        := (prop (',' prop)* ','?)?
name         := [a-zA-Z_][a-zA-Z0-9_]* | '`' relaxed-chars '`'
type         := name                # a built-in node type or a custom network
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
  contain a slash — though no node name may, so this never arises in practice.

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
# Silicon lattice: 5.43 Å cubic cell, default (zincblende) motif
lv1 = lattice_vecs { cell_length_a: 5.43, cell_length_b: 5.43, cell_length_c: 5.43 }
st1 = structure { lattice_vecs: lv1 }

# Geometry, in lattice units of that cell
sphere1 = sphere { center: (0, 0, 0), radius: 5, structure: st1 }

# Fill with silicon (visible - final atomic structure)
fill1 = materialize {
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
edit1 = atom_edit { molecule: imported }

# Re-apply the same diff to a different base
applied = apply_diff { base: other_base, diff: edit1.diff, visible: true }

output applied
```

### Functional Programming Pattern

```
# Create a range of integers
range1 = range { start: 0, step: 1, count: 5 }

# `pattern` is an instance of a custom network with inputs (index: Int, gap: Int).
# Wiring `gap` bakes it in; the unwired `index` stays a parameter, so
# @pattern is an `Int -> Blueprint` function.
gap_value = int { value: 3 }
pattern1 = pattern { gap: gap_value }

# Map the function over the range (lazy), then collect the stream
shapes = map {
  input_type: Int,
  output_type: Blueprint,
  xs: range1,
  f: @pattern1
}
all = collect { iter: shapes, element_type: Blueprint }
result = union { shapes: all, visible: true }

output result
```

## Implementation Notes

### Names (Query)

When serializing a network to text:
- Each node is printed under its own name (its `custom_name`, see
  [Assignments](#assignments)). A node added in the GUI is named
  `{typename}{counter}` (e.g., `sphere1`, `sphere2`) when it is created.
- Nodes are output in topological order (dependencies before dependents)

### Name Resolution (Edit)

When parsing an edit command:
- Names must match existing nodes or be new
- Forward references within the same edit are allowed
- Circular references are a validation error (`Wire cycle detected among: …`)

### Node Positions

Node positions are **not part of** the network text:
- `query` prints no coordinates; `atomcad-cli query --layout` reports the
  drawing separately
- An edit can say where nodes go with the layout statements (`place`,
  `arrange`, `straighten`), which act once and are never stored
- New nodes no statement places are placed automatically
- Users can manually reorganize after AI edits

Because a position can never be *written*, it can only be **carried**. Before an
edit runs — and, in replace mode, before the network is cleared — the editor
snapshots every node's `(scope path, name) -> (id, position)`. A node it then
creates under a name that was in the snapshot inherits that node's id and
position instead of being auto-placed. This is what makes an `edit --replace` of
unchanged query output a no-op on the canvas, inside zone bodies as well as at
the top level.
