---
name: atomcad
description: Interact with atomCAD node networks programmatically. Query, edit, and replace CAD geometry nodes for atomic/molecular structure design. Use when working with atomCAD projects or when the user wants to manipulate node networks, create CSG shapes, or design atomic structures.
license: MIT
metadata:
  author: atomCAD
  version: "2.0"
allowed-tools: Bash(atomcad-cli:*), Bash(./atomcad-cli:*)
---

# atomCAD Skill

Programmatically interact with atomCAD node networks via CLI. Requires atomCAD to be running.

## Prerequisites

- atomCAD running (the CLI connects to a running instance)

**CLI access** (automatic detection):
- **In atomCAD repo root**: `./atomcad-cli` works directly (no PATH setup needed)
- **Elsewhere**: requires `atomcad-cli` on PATH

## Command Resolution

Before running CLI commands, detect the appropriate command:

1. **Development mode**: If `./atomcad-cli` exists in current directory → use `./atomcad-cli`
2. **Installed mode**: If `atomcad-cli` is in PATH → use `atomcad-cli`
3. **Not found**: If neither works, inform the user they need to set up the CLI

**Setup instructions** (if CLI not found):
- Run the setup script from the atomCAD installation directory:
  - Windows: `.\setup\setup-skill.bat`
  - Linux/macOS: `bash setup/setup-skill.sh`
- Or add the CLI manually to PATH (the CLI is in the `cli/` folder of the atomCAD installation)

## Core Concepts

### Node Networks

atomCAD designs are parametric node networks (DAGs):

- **Nodes** have typed input pins (parameters) and one or more output pins (pin 0 is the primary result; e.g. `atom_edit` also has a `diff` pin, referenced as `edited.diff`), plus a title-bar function pin (`@name`) that exposes the node as a function value
- **Wires** connect output→input of compatible types
- **Evaluation** is lazy: only visible nodes trigger computation
- **Custom nodes** are subnetworks with matching names

Each network can have an **output node** (set via `output <node_name>`) that defines what value the network returns when used as a custom node.

### Lattice Coordinates

**Important:** Lattice geometry nodes (`cuboid`, `sphere`, `half_space`, `circle`, `rect`, …) take coordinates (positions, sizes, radii) in discrete **lattice units** (integers), not angstroms. The actual physical dimensions depend on the unit cell. This ensures all atomic structures are naturally lattice-aligned. The `free_*` nodes (`free_sphere`, `free_circle`, `free_move`, `free_rot`) are the exception: they work in real space (Å, floats).

### Structure, Lattice and Motif

- **Default structure:** cubic diamond — a cubic unit cell with a 3.567 Å lattice constant and the cubic zincblende motif with carbon atoms (pure diamond) — when nothing is wired
- 3D geometry nodes have an optional, wire-only `structure` input (type `Structure`) to specify a different lattice/motif; build one with the `structure` node from `lattice_vecs`, `motif` and `motif_offset` (all optional; unwired ones default to diamond). 2D nodes take a `d_plane` (`DrawingPlane`) input instead
- Non-cubic unit cells cause shapes like `cuboid` to become parallelepipeds (and `sphere` an ellipsoid)

### Data Types

| Type | Description |
|------|-------------|
| `Bool`, `String`, `Int`, `Float` | Primitives |
| `Vec2`, `Vec3`, `IVec2`, `IVec3` | 2D/3D vectors (float/int) |
| `IMat2`, `IMat3`, `Mat3` | Matrices (int/float) |
| `Geometry2D` | 2D shapes (for extrusion) |
| `Blueprint` | 3D geometry (SDF-based) + the `Structure` it will be carved from; output of `cuboid`, `sphere`, `union`, … |
| `Crystal` | Atoms (+ bonds) still bound to a structure; output of `materialize` |
| `Molecule` | Free-floating atoms, no structure; output of `exit_structure`, `import_xyz` |
| `HasAtoms`, `HasStructure`, `HasFreeLinOps` | Abstract input-pin types: Crystal or Molecule / Blueprint or Crystal / Blueprint or Molecule |
| `Structure` | Lattice vectors + motif + motif offset |
| `LatticeVecs` | Unit cell basis vectors |
| `Motif` | Crystal motif definition |
| `[T]` | Array of type T |
| `Iter[T]` | Lazy stream (`range`, `map`, `filter`); turn it into `[T]` with `collect` |
| `Record(Name)`, `{a: Int, b: Float}` | Named / anonymous record |
| `A -> B` | Function type |
| `Unit` | Result of effect nodes (`foreach`, `export_atoms`) |

**Implicit conversions:** `Int`↔`Float`, `IVec2`↔`Vec2`, `IVec3`↔`Vec3`, `IMat3`↔`Mat3`, `T`→`[T]`, `[T]` or `T`→`Iter[T]`, and a concrete phase type to its abstract supertypes (e.g. `Crystal`→`HasAtoms`). `Iter[T]`→`[T]` is **not** implicit (use `collect`).

Array pins (marked with dot) accept multiple wires; values are concatenated.

### Text Format Syntax

```
# Set network description (single-line)
description "A parametric gear assembly"

# Or multi-line with triple quotes
description """
This network creates a parametric gear.

Parameters:
- teeth: number of teeth
- radius: outer radius
"""

# Create nodes: id = type { parameter: value, ... }
sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }
cuboid1 = cuboid { min_corner: (-5, -5, -5), extent: (10, 10, 10) }

# Wire nodes by referencing node IDs as input values
union1 = union { shapes: [sphere1, cuboid1], visible: true }

# Set network output node
output union1

# Delete a node
delete sphere1

# Anchor a comment to what it documents: a node, or one wire
note1 = Comment { text: "the chassis", on: cuboid1 }
note2 = Comment { text: "the union branch", on: sphere1 -> union1.shapes }
```

**Parameter values:**
- Integers: `42`, `-10`
- Floats: `3.14`, `1.5e-3`
- Booleans: `true`, `false`
- Strings: `"hello"` (single-line) or `"""multi\nline"""` (triple-quoted for multi-line)
- Vectors: `(x, y)` or `(x, y, z)`
- Arrays: `[a, b, c]`
- Node references: use the node's ID

**Note on vectors:** The tuple syntax `(x, y, z)` is for literal values in node parameters (e.g., `sphere { center: (5, 5, 5) }`). The vector constructor nodes (`vec2`, `vec3`, `ivec2`, `ivec3`) use separate component inputs instead:
```
vec3 { x: 1.5, y: 2.5, z: 3.5 }
ivec3 { x: 1, y: 2, z: 3 }
```
A tuple may hold only numbers: to build a vector from node values, wire them into a vector node (`ext = ivec3 { x: size, y: size, z: size }`, then `extent: ext`). `(size, size, size)` is a parse error.

### Zone Bodies (`map`, `filter`, `fold`, `foreach`, `zip_with`, `closure`)

These node types carry an **inline body** — a nested network that runs once per
element (or, for `closure`, is exposed as a `Function` value). The body is
projected as a `body { … }` **block** in the node's braces, and it is the last
item in the property list:

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

- `body { … }` is a **block**, not a property value — there is no `:` after
  `body`, and the statements inside carry no commas.
- `output e` **inside** the block feeds the HOF's zone-output pin. `output m1`
  **outside** it sets the network's return node. Same keyword; the position
  disambiguates.
- A node carrying a body spans several lines. Consumers that assumed
  "one statement = one line" must brace-match.
- A node whose body is **empty emits no block at all** — that is the shape of an
  HOF driven through its `f:` pin.
- A `body { … }` block on a node type that has no body (`i = int { value: 1,
  body { … } }`) is refused, not ignored.

#### Referring outward from inside a body

Four spellings, and nothing else. With `k` = the number of leading `^`:
**`k` carets → a node `k` scopes out; a `$` prefix → a zone input `k + 1` frames
out.**

| Written | Resolves to | Encoding |
|---------|-------------|----------|
| `n` | a node in this body | `NodeOutput`, depth 0 |
| `^n` | a node one scope out (a *capture*) | `NodeOutput`, depth 1 |
| `^^n` | a node two scopes out | `NodeOutput`, depth 2 |
| `$element` | this body's own HOF's iteration value | `ZoneInput`, depth 1 |
| `^$element` | the **enclosing** HOF's iteration value | `ZoneInput`, depth 2 |
| `^^$element` | two HOFs out | `ZoneInput`, depth 3 |

The `+ 1` is a real asymmetry, not a typo: `^n` counts networks, while `$name`
already names this body's own owner, so reaching the *enclosing* HOF's element
needs one caret.

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
outer `map`'s.

A **bare** name is resolved lexically — this body first, then outward — so
`b: scale` also works above. `$name` **never** searches outward: promoting a
per-iteration read into a capture would change evaluation semantics, not just
the referent. `query` always emits the explicit `^` form.

#### Zone pin names are not uniform

Only `map` and `zip_with` call the result `result`. Read them from this table,
or from `atomcad-cli describe <node>`:

| Node | Zone inputs (`$…`) | Zone output (`output …`) |
|------|--------------------|--------------------------|
| `map` | `element` | `result` |
| `filter` | `element` | **`keep`** (`Bool`) |
| `foreach` | `element` | **`out`** (`Unit`) |
| `fold` | `acc`, `element` | `new_acc` |
| `zip_with` | `element1` … `elementN` | `result` |
| `closure` | its own `params` | per kind: `new_acc` (fold), `out` (foreach), else `result` |

#### `closure` nodes

A `closure` carries its own definition, and its zone inputs are named by its
`params`:

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

- `kind` — `"map"`, `"filter"`, `"fold"`, `"foreach"` or `"custom"` (quoted).
- `params` — the parameter names, and therefore what the body's `$x` binds to.
  Only meaningful for `kind: "custom"`; the preset kinds name their own
  (`element`, or `acc`/`element` for `fold`).
- `type_args` — the free type slots: `[T, U]` for `map`, `[T]` for `filter` and
  `foreach`, `[A, T]` for `fold`, `[p0, …, pN-1, R]` for `custom`.

Write these properties in any order relative to the block — the editor always
applies a statement's properties **before** its body, so `$x` binds correctly
even when `params:` comes after `body { … }`.

#### `f:` overrides `body`

Every HOF also has an optional `f:` function pin. **When `f` is wired it
overrides the inline body** — the body is ignored at evaluation. `query` emits
both when both exist, so a network can show a `body { … }` that never runs.
Don't author a body for a node whose `f:` you are also wiring.

#### Editing a body: a block replaces, a path merges

```
# Whole-body assign: m1's body becomes exactly what the block says.
m1 = map { xs: r, body { a = int { value: 1 } output a } }

# Path-addressed: merges into the existing body, siblings untouched.
m1/d = expr { a: $element, expression: "a * 4", parameters: [{ name: "a", data_type: Int }] }
m1/new1 = int { value: 7 }         # add a node to m1's body
output m1/new1                      # re-point m1's zone-output wire
delete m1/e                         # remove one body node
outer/inner/n1 = int { value: 2 }   # depth 2
```

Rules that matter:

- **A mentioned `body { … }` block assigns the whole body**, exactly like any
  other property. Omit `body` and the body is left alone.
- **A block with no `output` statement *clears* the zone-output wire.** The
  block is total over the parent's zone-output too — that is what makes
  `query` → `edit --replace` exact. `body { }` empties the body and clears the
  wire.
- **A path merges.** `m1/x = …` creates or updates `x` and leaves every sibling's
  identity and canvas position alone.
- **The separator is `/`, never `.`** — `.` already means pin access
  (`atom_edit.diff`).
- **Inside a path statement everything is relative to the scope it lands on:**
  bare names resolve in that body, `$…` are *its* zone inputs, and `^…` walks
  outward from *it*. So `m1/x = …` means exactly what the same statement means
  written inside `m1`'s block.
- **A path never creates the HOF it addresses.** `m1` must already exist and own
  a body, or the statement is an error, not a silent no-op.
- **Ordering within one script is significant.** A `body` block followed by a
  path statement for the same node applies in order: the block wipes, the path
  merges into the result. Don't straddle the two unless you mean to.
- **Rebuilt body nodes keep their identity by name**, so an `edit --replace` of
  unchanged `query` output changes no ids and moves nothing on the canvas.

#### `success` means *validates*, not merely *parsed*

`edit` returns JSON. `success: true` means the script parsed, applied **and**
the resulting network validates. Blocking validation errors land in `errors`
(and fail the edit); non-blocking ones land in `warnings`. Names in
`nodes_created` / `nodes_updated` / `nodes_deleted` and in error text are **full
paths** (`m1/a`), because `m1/a` and `m2/a` are different nodes.

An edit is **all or nothing**: if a statement fails (unknown node type, wrong
literal type, `delete` / `output` of a missing node), the network is left
exactly as it was and the last error says `Nothing was applied`. Resend the
whole corrected script. A validation error, by contrast, comes from an edit
that did apply; that edit stays.

Two things a body edit commonly trips on, both reported this way:

- A body with **no wire into its zone-output pin** is a blocking error — a
  freshly created HOF gets an *empty* body, so `m1 = map { xs: r }` alone leaves
  `m1` invalid until you give it a `body` or wire its `f:`.
- A `^` depth that walks past the outermost scope is not silently dropped: the
  wire is not made and `warnings` names it (`'^^scale' reaches past the
  top-level network`).

**Round-trip:** piped or redirected stdin is read to EOF, blank lines included,
so `query` output can be fed straight back to `edit --replace`. Only an
*interactive* (terminal) `edit` stops at the first empty line or a lone `.`.

#### The `layout` section: what the edit did to the canvas

The text format carries no positions, so the JSON is also the only place you
learn what an edit did to the drawing. When it moved a node that already
existed, left a problem behind, or had layout statements, the result has a
`layout` object:

```json
"layout": {
  "placed": [ { "node": "n", "box": [100, 370, 160, 60] } ],
  "auto_placed": [ { "node": "m", "box": [520, 100, 160, 60] } ],
  "moved": [
    { "node": "note", "from": [100, 370], "to": [100, 460], "cause": "push",
      "dir": "down", "why": "line 3: place n below d push down" },
    { "node": "b", "from": [520, 100], "to": [730, 100], "cause": "auto",
      "why": "made room for new node m" }
  ],
  "findings": [],
  "warnings": []
}
```

- **Coordinates:** canvas pixels, x right, y **down**; a position is a node's
  top-left corner, a `box` is `[x, y, width, height]`. Body nodes are in body
  coordinates (origin at the body's top-left). Whole pixels, rounded.
- `placed` — every node a `place` statement positioned, new or existing, with
  its final box.
- `auto_placed` — every other node the edit created, with its **real rendered
  box** (an expanded `map` is far wider than an ordinary 160 px node).
- `moved` — every *pre-existing* node whose position changed: `from` / `to`,
  the `cause` of its first move, a short `why`, and `also` (later, different
  causes) when there were any.
- `cause` is one of: `place` (a `place` statement put it there), `arrange` (an
  `arrange` / `straighten` listed it), `push` (a placed or arranged node landed
  on it; `dir` is `up` / `down` / `right`), `grow` (a node the edit enlarged
  pushed it), `auto` (room made for a node no statement placed, a wire kept
  running left to right, a comment returned beside its anchor). For a statement
  cause, `why` is `line N: <statement>`.
- `findings` — what this edit **created** and nothing repaired, each
  `{ "kind", "message", "nodes" }` with `kind`
  `overlap` (two nodes overlap that did not before), `backward_wire` (a new
  wire runs right to left, e.g. because it closes a cycle), `wire_over_node`
  (a wire now runs behind a node it does not connect to; `nodes` = the wire's
  source, its destination, the node) or `crossing` (two wires now cross;
  `nodes` = both wires' source and destination). Wires are judged as drawn:
  one curve between the two pins. Pre-existing overlaps, backward wires,
  hidden wires and crossings are the user's baseline and never reported.
- `warnings` — layout statements that could not be carried out:
  `{ "line", "statement", "problem", "fallback" }`. The edit itself still
  applied; the `fallback` says what happened to the subject instead.
- **No `layout` key** means no existing node moved, nothing was found, and the
  edit had no layout statements.

#### `--dry-run`: preview an edit

`atomcad-cli edit --dry-run …` runs the whole edit (parse, apply, validate,
layout), prints exactly the JSON the real edit would print plus
`"dry_run": true`, and then reverts it: no undo step, no dirty flag, no AI
History entry, no redraw. Combines with `--replace` and with stdin input.

#### `query --layout`: where things are on the canvas

The text has no positions; `atomcad-cli query --layout` prints the drawing as
boxes:

```
# Main: extent (100,20)–(1150,490), 11 nodes; 7 in box (0,0)–(700,600)
w        100,100  160×60
h        100,190  160×60
d        100,280  160×60
note     100,370  180×80   comment
dims     310,160  160×110
hole_r   310,390  160×60   ai
hole     520,390  160×80   ai
# groups (inferred)
column   w, h, d            gap 30
row      hole_r, hole       gap 50
```

```bash
atomcad-cli query --layout                        # whole network
atomcad-cli query --layout --nodes=d,note,hole_r  # only these (by name)
atomcad-cli query --layout --box=0,0,700,600      # nodes touching this area; empty = free
atomcad-cli query --layout --scope=m1             # a HOF body (path: m1/m2)
```

- Each line: `name  x,y  w×h  [tags]`. Canvas pixels, x right, **y down**,
  rounded; `x,y` is the top-left corner; sorted by top, then left. In a body,
  coordinates are relative to the body's top-left.
- Header: the **extent** of the whole scope regardless of filter (everything
  outside it is free), the node count, and the filter's match count.
- Tags: `ai` = an AI edit created it and no human has positioned it since;
  **every untagged node is the user's**. `comment` = a comment box. `body` =
  an expanded HOF; list inside with `--scope`. The `ai` tag is not saved, so
  after a reload every node is the user's.
- `# groups (inferred)`: maximal columns (lefts within 8 px, gaps ≤ 60) and
  rows (tops within 8 px, gaps ≤ 100), with the median gap. A guess,
  recomputed on every query. Listed when any member is selected; unlisted
  members print as `…`.
- `--nodes` and `--box` are exclusive. An unknown name prints
  `(no such node)`. Use `--box=…` (with `=`) when a coordinate is negative.

#### `query --layout --image`: a picture of the same area

Add `--image=<path.png>` to any `query --layout` and atomCAD also renders that
area of the canvas, as the user sees it, to the PNG. Without `--image` nothing
is rendered.

```bash
atomcad-cli query --layout --box=0,0,700,600 --image=area.png   # exactly this area
atomcad-cli query --layout --nodes=cube,hole,diff --image=v.png # framed on these nodes
atomcad-cli query --layout --scope=m1 --image=body.png          # one HOF body
atomcad-cli query --layout --image=all.png --scale=2            # whole scope, 2× pixels
```

The read-out is printed as usual, and its last line says what the image shows:

```
# image: C:\workrea.png  734×634 px at scale 1; shows (0,0)–(700,600); content from pixel (34,34), rulers labelled in these coordinates
```

- **A `--box` crops; it does not filter**: everything inside it is drawn,
  including wires running off its edge. `--nodes` frames those nodes, and no
  filter the whole scope, both with a 40 px margin.
- **Same coordinates as the read-out.** The image is drawn at 100% zoom,
  whatever the canvas shows: one image pixel is one canvas pixel times the
  scale. The rulers along the top and left edges are labelled in the
  read-out's coordinates (body-local with `--scope`); `--no-rulers` drops them.
- **Size.** The longest side is capped at 2000 px, and the scale reduced to fit
  (the line says so). An area that would need a scale under 0.25 is refused:
  pick a smaller box. `--scale` (above 0, at most 4) and `--no-rulers` need
  `--image`.
- **Selection and active-node highlights are not drawn.** A collapsed body
  cannot be pictured. Read the PNG to look at it.
- **When to use it:** to check a non-trivial layout after an edit, or to see
  what a `wire_over_node` / `crossing` finding means. Not after every edit: an
  image costs far more context than the text read-out, and the read-out and
  the findings already say what moved and what broke.

#### Layout statements: saying where things go

An edit script can also place nodes. Three statements, written anywhere in the
script; they run after the network change, **in the order written**, each
seeing the drawing the earlier ones left. Nothing of them is stored.

```
place      <subject> at (<x>, <y>)          [push up|down|right]
place      <name> | [<name>, …] by (<dx>, <dy>)  [push …]
place      <subject> <clause> [, <clause>]  [push …]
arrange    [<name>, …]                      [push …]
straighten [<name>, …]                      [push …]
```

- **Subject:** `n`, or `[a, b] column` / `[a, b] row` (stacked in written order,
  lefts / tops aligned, optional `(gap: G)`), placed as one box.
- **Clauses:** `right_of T`, `left_of T` (alone: tops aligned too),
  `below T`, `above T` (alone: lefts aligned too), `top_even_with T`,
  `bottom_even_with T`, `left_even_with T`, `right_even_with T`,
  `straight_from S` (the wire `S` → subject runs level), `straight_to D`
  (subject → `D` runs level). Each axis stated exactly once: a lone direction
  supplies the other axis, an explicit clause replaces it.
- **Targets:** `T`, `a and b` (their joint box), or `all` (the whole drawing's
  extent, the subject excluded, measured when the statement runs; a node named
  all is `` `all` ``). A node this edit deletes is still a target, at its old box.
- **Options** on the four directions: `(gap: G, dx: n, dy: n)`. `G` is `tight`
  / `normal` / `wide` (25 / 50 / 100 px across, 15 / 30 / 60 down) or pixels.
- **`at (x, y)`** is a top-left corner in `query --layout`'s coordinates.
  **`by (dx, dy)`** moves existing nodes from where they stand *when the
  statement runs* — `place x by (0, 40)`, `place [a, b, c] by (210, 0)`. Neither
  combines with a clause.
- **`push`**: what the subject lands on always moves out of its way. No `push`:
  each such node goes up or down, whichever is cheaper. `push down` / `push up`:
  that way, and what they hit. `push right`: everything at or right of the
  subject's left edge shifts right together, what feeds the subject stays.
  There is no `push left`; to make room before `T`, place after `T`'s
  predecessor and `push right`.
- **`arrange [a, b, c]`** lays those nodes out by their wiring among
  themselves, keeping their top-left corner; **`straighten [a, b]`** moves each
  vertically until the wire from its first connected input runs level. Unlisted
  nodes move only to make room.
- **Afterwards:** what a statement placed, and what it measured against, stays
  put for the rest of the edit; new nodes no statement placed go around them.
  Vertical relations hold exactly; horizontal ones keep their order and can
  only widen (when a wire must be kept running left to right).
- Inside a body, write the statements in its `body { … }` block or as paths
  (`place m1/x below m1/d`); coordinates are body coordinates, and a negative
  one is clamped to 0 with a warning.
- **Rejected statements** (an unknown node, an axis twice or never, a node
  placed twice, a new node used as a target before it is placed, `by` on a new
  node, a deleted node as subject, `straight_*` with no wire) are dropped and
  listed in `layout.warnings`; the edit still applies. A statement that does not
  parse (`push left`, `at` plus a clause, `push` twice) rejects the whole edit.
- Nodes named `place`, `arrange` or `straighten` still work:
  `place = int { value: 1 }` is an assignment.

```
place [n, pitch] column below d push down   # into a column; what is under d makes way
place r below all, left_even_with hole_r     # a new lane under everything
place holes right_of r, straight_from r      # attach relative to r
place rot right_of slab push right           # into a chain
place note by (0, 40)
straighten [cut, fill]
```

#### Every edit is one undo step

Each `edit` call is recorded in the app's undo stack as a single step labelled
*AI edit network*, so the user can revert a whole edit — zone bodies included —
with Ctrl+Z, and reapply it with Ctrl+Y. An edit that applied but then failed
validation is still undoable. Tell the user this rather than trying to
reconstruct a previous state yourself; there is no CLI undo command.

## CLI Commands

### Global Options

```bash
atomcad-cli --help       # Show help
atomcad-cli --port=PORT  # Custom server port (default: 19847)
atomcad-cli --document=DOC <command>  # Refuse unless DOC is the active tab
atomcad-cli --label="Opus / skill v2" <command>  # Name this client in the AI History panel (or set ATOMCAD_CLIENT_LABEL)
```

### Tabs: always pass `--document`

atomCAD has **tabs**, one open design per tab, and every command acts on
whichever tab is **active**. The user can switch tabs at any time, so without
a guard an `edit` meant for one design can land in another. Pass
`--document <path-or-id>` on **every** command once you know which design
you are working on; a command naming anything but the active tab is refused
before it does anything, with a message naming the active one:

```bash
atomcad-cli documents                 # list the tabs: id, path, active (*), dirty
# * 2  C:/work/host.cnnd  (modified)
#   3  Untitled

atomcad-cli --document C:/work/host.cnnd query       # a saved design: its path
atomcad-cli edit --document 3 --code="s = sphere {}" # an Untitled one: its id
```

- `load` prints the document id and path of the tab it opened, `new` the
  document id; use it from then on. `file` prints it too (`Document: 2`).
- `--document` may go before or after the command. A relative path is
  resolved against the CLI's working directory; any spelling of the path
  works.
- When a command is refused, **do not retry with the other document**: the
  user switched tabs on purpose. Tell them, and ask whether to continue there
  or wait until they switch back.
- `load`, `new` and `documents` are not guarded (they do not act on the
  active tab).

### Network Operations

```bash
# Query current network state
atomcad-cli query

# Edit network (add/update nodes, keeps existing)
atomcad-cli edit --code="sphere1 = sphere { radius: 10 }"

# Replace entire network (clears first)
atomcad-cli edit --code="..." --replace

# Multi-line edit (piped stdin is read to EOF, blank lines included;
# interactively, an empty line or "." sends)
atomcad-cli edit
atomcad-cli edit --replace

# Preview: same JSON (plus "dry_run": true), then everything is reverted
atomcad-cli edit --dry-run --code="..."

# Say where things go (see "Layout statements")
atomcad-cli edit --code="n = int { value: 3 }
place n below h push down"

# Node boxes on the canvas, and inferred columns/rows (see "query --layout")
atomcad-cli query --layout [--nodes=a,b | --box=x0,y0,x1,y1] [--scope=m1/m2]

# ...and a picture of the same area (see "query --layout --image")
atomcad-cli query --layout --box=x0,y0,x1,y1 --image=area.png [--scale=2] [--no-rulers]
```

**Query output format:**
```
# Network: Main
description "A boolean union example"

sphere1 = sphere { center: (0, 0, 0), radius: 5 }
cuboid1 = cuboid { min_corner: (0, 0, 0), extent: (10, 10, 10) }
result = union { shapes: [sphere1, cuboid1], visible: true }
output result

# 3 nodes
```

The output includes:
- Header with active network name
- Description statement (if set), then `summary` statement (if set)
- Node definitions in topological order (zone bodies as nested `body { … }` blocks)
- Output statement (if set)
- Footer with node count

The header and footer use comment syntax (`#`), so the output is valid input to `edit --replace`.

### Multi-line Input

For multi-line code, **stdin piping is recommended** as it avoids shell quoting issues:

```bash
# Recommended: pipe multi-line code via stdin
echo "base = cuboid { extent: (10, 10, 10) }
hole = sphere { center: (5, 5, 5), radius: 4 }
result = diff { base: [base], sub: [hole], visible: true }" | atomcad-cli edit --replace

# Or use a heredoc
atomcad-cli edit --replace <<'EOF'
base = cuboid { extent: (10, 10, 10) }
hole = sphere { center: (5, 5, 5), radius: 4 }
result = diff { base: [base], sub: [hole], visible: true }
EOF
```

**Alternative:** The `--code` flag supports `\n` escape sequences for newlines:

```bash
# Escape sequences: \n for newline, \t for tab, \\ for literal backslash
# (the CLI turns every \n into a real newline, including one inside a string
#  literal, which is then an unterminated string: a "\n" meant for a string
#  must reach the CLI as \\n — use stdin for such scripts)
atomcad-cli edit --replace --code="base = cuboid { extent: (10, 10, 10) }\nhole = sphere { radius: 4 }\nresult = diff { base: [base], sub: [hole], visible: true }"
```

**Note:** A literal newline inside `--code="..."` works too (the wrappers and the CLI handle it), but shell quoting of multi-line strings varies across platforms, so stdin piping or the `\n` escape form is the safer habit.

### Node Network Management

atomCAD designs can contain multiple node networks. Use these commands to manage them:

```bash
# List all node networks
atomcad-cli networks

# Create a new network
atomcad-cli networks add                    # Auto-named (UNTITLED, UNTITLED1, ...)
atomcad-cli networks add --name "my_shape"  # Specific name

# Switch active network (query/edit will operate on this)
atomcad-cli networks activate <name>

# Delete a network
atomcad-cli networks delete <name>

# Rename a network
atomcad-cli networks rename <old> <new>
```

**Output example for `networks`:**
```
Node Networks:
* Main  (active)
  cube
  pattern  [ERROR: Invalid parameter type]

3 networks (1 with errors)
```

**Behavior notes:**
- `networks add` auto-activates the new network
- `networks delete` fails if the network is referenced by other networks
- `networks rename` updates all references in other networks automatically

**Note:** `query` and `edit` always operate on the active network.

### Linked Libraries

A design can **link** other `.cnnd` files. A linked library's networks and
record types appear under its alias (`demolib.half_space`, `demolib.Miller`,
nested `demolib.common.slab`) and are used like local ones — but they are
**read-only**: `edit` on a linked network is refused, and `networks` marks
them `(linked, read-only)`. `query` on one prints a `# linked from <path>`
line under the `# Network:` header. To change one, open the library file
(`load <path>` — it opens in its own tab, or switches to it), edit with
`--document <library path>`, save, then `load` the design's path again: that
switches back to the design's tab, which picks up the saved library on the
way.

```bash
atomcad-cli libraries                                # list, with status
atomcad-cli libraries link libs/demolib.cnnd demolib # path relative to the design
atomcad-cli libraries refresh [demolib]              # re-read one, or all
atomcad-cli libraries unlink demolib                 # refused while used
atomcad-cli libraries rename demolib libs.demolib    # renames every use too
atomcad-cli libraries make-local demolib             # copy into the design, unlink
```

Each is one undo step. `refresh` prints one line per wire it disconnected.
Refer to a library's content by its full name in the text format, backticked
when it contains dots: ``f = `demolib.half_space` { … }``.

### File Operations

Load, save, and manage `.cnnd` project files. The application has **tabs**,
one open design per tab, and every command acts on the **active** tab:

```bash
# Open a .cnnd file in a new tab (or switch to its tab if already open)
atomcad-cli load design.cnnd

# Save current project
atomcad-cli save                      # Save to current file
atomcad-cli save design.cnnd          # Save to new path (overwrites if exists)
atomcad-cli save other/design.cnnd --copy-deps  # …and copy linked libraries / data files along
atomcad-cli save other/design.cnnd --no-deps    # …or save the design alone

# Check file status
atomcad-cli file

# Create new project (in a new tab)
atomcad-cli new
```

**Output examples:**

```bash
# load success
Loaded: /path/to/design.cnnd (3 networks, document 2)
# load of a file already open in a tab
Already open; switched to its tab: /path/to/design.cnnd (3 networks, document 2)

# save success
Saved: /path/to/design.cnnd

# file status
File: /path/to/design.cnnd
Modified: yes
Networks: 3
Document: 2

# file status (an Untitled tab)
File: (none)
Modified: no
Networks: 1
Document: 3

# new success
New project created in a new tab (document 4).
```

**Behavior notes:**
- `load` and `new` never discard anything: they open a tab (an untouched
  empty *Untitled* tab is replaced). `--force` is accepted and does nothing.
  The user may switch tabs too — pass `--document` (see *Tabs* above).
- `save` without a path saves to the current file; fails if no file is loaded
- `save <path>` into another folder fails, listing the files, when the design
  reads libraries or data files by a relative path that would not be there;
  choose `--copy-deps` (existing different files are kept) or `--no-deps`
- `new` opens a fresh design with one "Main" network in a new tab
- Relative paths are resolved relative to the CLI's working directory

### Evaluate Node Results

Evaluate a specific node and return its computed result (useful for verification and debugging).

```bash
# Evaluate a node by ID or custom name
atomcad-cli evaluate <node_id>
atomcad-cli evaluate sphere1

# Verbose output (detailed info for complex types)
atomcad-cli evaluate <node_id> --verbose
atomcad-cli evaluate atoms1 --verbose
```

**Output formats:**
- **Primitives** (Float, Int, Bool, String): Display the value directly (e.g., `42`, `3.140000`, `true`)
- **Vectors** (Vec3, IVec3, etc.): Display coordinates (e.g., `(5.000000, 10.000000, 3.000000)`, `(1, 2, 3)`)
- **Blueprint/Geometry2D**: Display `Blueprint` or `Geometry2D` (brief); verbose shows the lattice vectors, motif offset and CSG tree
- **Crystal/Molecule**: Atom/bond counts, formula, bond orders and bounding box (brief); atom/bond counts and the first 10 atoms (verbose)
- **Iterators**: Display `Iterator` (wire into `collect` to see the elements)

**Examples:**
```bash
$ atomcad-cli evaluate sphere1
Blueprint

$ atomcad-cli evaluate count1
42

$ atomcad-cli evaluate pos1
(5.000000, 10.000000, 3.000000)

$ atomcad-cli evaluate atoms1
5746 atoms, 8918 bonds
C4030H1716
bonds: 8918 single
bbox: 29.8 x 29.8 x 29.8 A

$ atomcad-cli evaluate atoms1 --verbose
Crystal:
atoms: 5746
bonds: 8918
first 10 atoms:
  [1] Z=6 pos=(0.000000, 0.000000, 0.000000) depth=0.000 bonds=4
  ...
```

Only top-level nodes of the active network are found by name.

### Run a chemisorb search

`chemisorb` is the one node whose result evaluation never computes: `query` and
`evaluate` only ever see its *plan* (`stats.searched = false`, no candidates).
The search runs on an explicit action — the panel's Run button, or:

```bash
atomcad-cli run <node>      # by custom name or id, active network
atomcad-cli run mount
```

It prints a summary (hypotheses relaxed, candidates listed, best score and bond
inventory, a warning if the budget truncated the search). The result is stored on
the node and output until an input or a setting changes; it is not saved with
the file, so after `load` run again. Seconds to minutes.

H transfers (OH legs handing their H to the surface; `to_substrate` only) are
enabled through the optional `transfers` pin — note the `Record(Name)` spelling
of the element type:

```
h_off = array { element_type: Record(ChemisorbTransfer), elements: [{ element: 1, direction: "to_substrate" }] }
mount = chemisorb { adsorbate: tool, substrate: surface, adsorbate_tag: "feet", transfers: h_off, formed_bonds: 3 }
```

To see what the search did at one step, select an item of its search tree —
the same as clicking it in the panel. The tree alternates *states* (the root,
a leg: the bonds made so far) and *next feet* (one foot's test from a state:
the foot, the sites it accepted, the test's shape). The item shows on the
node's `debug` (pin 2, the structure with its markings) and `debug_shapes`
(pin 3, a next foot's test shape); `--show` displays both, then take a
`screenshot`:

```bash
atomcad-cli debug-select mount                  # the root: the posed inputs
atomcad-cli debug-select mount 12 --show        # next foot 12: its anchor sphere
atomcad-cli debug-select mount 12-45            # one leg bonded, seated
atomcad-cli debug-select mount 12-45,13         # next foot 13 after it: the shell
atomcad-cli debug-select mount 12-45,13-61,14   # next foot 14: the leg-3 ring
atomcad-cli debug-select mount 12-45,13-61 --form seated   # after run
```

A path is the legs as foot-site atom-id pairs (the ids of a candidate's `sites`
field), optionally followed by a foot id alone for that next foot, or `#N` for
tree row N. The command prints what the item marks (bonded, the foot, accepted
sites, clashing, near misses with how far they missed) and its children's
paths. A relaxed row that is not a listed candidate is re-relaxed first, so it
can take a few seconds.

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
atomcad-cli describe materialize
```

Use `describe` to discover input pins, types, defaults, and behavior for any node.

### Camera Control

Control the viewport camera position and projection mode.

```bash
# Get current camera state (returns JSON)
atomcad-cli camera

# Set camera position and orientation
atomcad-cli camera --eye x,y,z --target x,y,z --up x,y,z

# Switch projection mode
atomcad-cli camera --orthographic    # Orthographic projection (no perspective)
atomcad-cli camera --perspective     # Perspective projection (default)

# Set orthographic zoom level (half-height of viewport)
atomcad-cli camera --ortho-height 50

# Combined example: position camera and switch to orthographic
atomcad-cli camera --eye 30,30,30 --target 0,0,0 --up 0,0,1 --orthographic
```

**Important:** Camera coordinates are in **angstroms**, while geometry node coordinates are in **lattice units**. For cubic diamond (default), 1 lattice unit ≈ 3.567 Å. A 20×20×20 lattice unit object is ~71 Å across, so position the camera 150-250 Å away to see the full object.

**Parameters:**
- `--eye x,y,z` — Camera position in angstroms (world coordinates)
- `--target x,y,z` — Point the camera looks at (angstroms)
- `--up x,y,z` — Up vector for camera orientation (typically `0,0,1`)
- `--orthographic` — Enable orthographic projection (parallel rays, no perspective distortion)
- `--perspective` — Enable perspective projection (default, realistic depth)
- `--ortho-height N` — Half-height of orthographic viewport (controls zoom level)

**Response:** Returns JSON with current camera state:
```json
{
  "success": true,
  "camera": {
    "eye": [30.0, 30.0, 30.0],
    "target": [0.0, 0.0, 0.0],
    "up": [0.0, 0.0, 1.0],
    "orthographic": true,
    "ortho_half_height": 25.0
  }
}
```

### Display Settings

Control viewport display preferences including atomic visualization and geometry rendering.

```bash
# Get current display settings (returns JSON)
atomcad-cli display

# Set atomic visualization mode
atomcad-cli display --atomic-viz ball-and-stick    # Small atoms with bond sticks
atomcad-cli display --atomic-viz space-filling     # Large van der Waals spheres

# Set geometry visualization mode
atomcad-cli display --geometry-viz surface-splatting  # Implicit SDF rendering
atomcad-cli display --geometry-viz solid              # Solid mesh (default)
atomcad-cli display --geometry-viz wireframe          # Wireframe mesh

# Set node display policy
atomcad-cli display --node-policy manual           # User controls visibility (default)
atomcad-cli display --node-policy prefer-selected  # Show selected node
atomcad-cli display --node-policy prefer-frontier  # Show output nodes

# Set background color
atomcad-cli display --background 0,0,0             # Black (default)
atomcad-cli display --background 255,255,255       # White

# Combine multiple settings
atomcad-cli display --atomic-viz space-filling --geometry-viz wireframe
```

**Parameters:**
- `--atomic-viz <mode>` — Atomic visualization: `ball-and-stick` (default) or `space-filling`
- `--geometry-viz <mode>` — Geometry visualization: `solid` (default), `wireframe`, or `surface-splatting`
- `--node-policy <policy>` — Node display: `manual` (default), `prefer-selected`, or `prefer-frontier`
- `--background R,G,B` — Background color as RGB values 0-255 (default: 0,0,0)

Display settings are the user's saved preferences (they persist across sessions), so change them only when needed.

**Response:** Returns JSON with current display state:
```json
{
  "success": true,
  "display": {
    "atomic_visualization": "ball-and-stick",
    "geometry_visualization": "solid",
    "node_display_policy": "manual",
    "background_color": [0, 0, 0]
  }
}
```

### Screenshot Capture

Capture the current viewport to a PNG image file.

```bash
# Capture with default resolution (current viewport size)
atomcad-cli screenshot -o output.png

# Auto-generate timestamped filename (screenshot_YYYY-MM-DD_HHMMSS.png)
atomcad-cli screenshot -o auto

# Capture with specific resolution
atomcad-cli screenshot -o output.png -w 1920 -h 1080

# Capture with custom background color (R,G,B values 0-255)
atomcad-cli screenshot -o output.png --background 255,255,255
```

**Parameters:**
- `-o, --output <path>` — Output PNG file path (required). Use `auto` to generate a timestamped filename
- `-w, --width N` — Image width in pixels (optional, uses viewport size if not specified)
- `-h, --height N` — Image height in pixels (optional, uses viewport size if not specified)
- `--background R,G,B` — Background color as RGB values 0-255 (default: 30,30,30 dark gray)

**Response:** Returns confirmation with file path and actual dimensions:
```
Screenshot saved: /path/to/output.png (1920x1080)
```

**Note:** Relative paths are resolved relative to the CLI's working directory, not atomCAD's.

**Always verify screenshots:** After capturing, read the PNG file to check the result. If the view is too close (only atoms visible, no overall shape) or too far (object too small), adjust the camera and retake. Common issue: using small camera coordinates puts you inside the atomic structure. If the entire screenshot is one flat geometry colour, you are probably inside or too close to a shape (solid geometry is drawn deep orange for the active node, blue-gray otherwise).

**Recommended visualization styles for attractive screenshots:**
1. **Ball-and-stick with visible geometry:** Use `--atomic-viz ball-and-stick` and make the geometry node feeding into `materialize` visible (`visible: true`). This shows atoms running along the surface of the geometry shape—a visually striking combination.
2. **Space-filling only:** Use `--atomic-viz space-filling` and hide geometry nodes (`visible: false`). The large van der Waals spheres will mostly occlude any geometry anyway, so keeping geometry hidden produces cleaner results.

### Typical AI Agent Workflow

Combine geometry creation, camera control, display settings, and screenshots for visual verification:

```bash
# 1. Create geometry
atomcad-cli edit --code="s = sphere { radius: 10, visible: true }"

# 2. Position camera for good viewing angle
atomcad-cli camera --eye 30,30,30 --target 0,0,0 --up 0,0,1 --orthographic

# 3. Adjust display settings for clear visualization
atomcad-cli display --atomic-viz space-filling --background 255,255,255

# 4. Capture screenshot for visual verification
atomcad-cli screenshot -o sphere_check.png

# 5. Verify result by viewing the image
# (The AI agent can read the PNG file to see the rendered geometry)
```

### REPL Mode

```bash
atomcad-cli              # Enter interactive mode
```

Commands:
- `query`/`q` — Show current node network
- `edit` — Enter edit mode (incremental)
- `replace`/`r` — Enter edit mode (replace entire network)
- `evaluate`/`e <node>` — Evaluate a node
- `run <node>` — Run a `chemisorb` node's search
- `debug-select <node> [<path>] [posed|seated|relaxed] [show]` — Show a `chemisorb` search-tree item on its debug pins
- `nodes`/`n` — List available node types
- `describe`/`d <node>` — Describe a node type
- `networks` — List all node networks
- `networks add [--name X]` — Create new network
- `networks delete <name>` — Delete a network
- `networks activate <name>` — Switch to a network
- `networks rename <old> <new>` — Rename a network
- `camera`/`c` — Get/set camera state
- `display` — Get/set display preferences
- `screenshot`/`s <path>` — Capture viewport to PNG
- `load <path>` — Open a .cnnd file in a tab
- `save [path]` — Save to file
- `file` — Show current file status
- `new` — New project in a new tab
- `documents` — List the open tabs
- `libraries`/`libs […]` — Linked libraries (same subcommands as on the command line)
- `help`/`?` — Show help
- `quit`/`exit` — Exit REPL

## Common Patterns

### Create a simple atomic structure

```bash
# Create sphere geometry and fill with atoms
atomcad-cli edit --code="s = sphere { radius: 5, visible: true }"
atomcad-cli edit --code="atoms = materialize { shape: s, visible: true }"
```

### Boolean operations on geometry

```bash
# Using heredoc (recommended for multi-line)
atomcad-cli edit --replace <<'EOF'
base = cuboid { extent: (10, 10, 10), visible: false }
hole = sphere { center: (5, 5, 5), radius: 4 }
result = diff { base: [base], sub: [hole], visible: true }
EOF

# Or using escape sequences
atomcad-cli edit --replace --code="base = cuboid { extent: (10, 10, 10), visible: false }\nhole = sphere { center: (5, 5, 5), radius: 4 }\nresult = diff { base: [base], sub: [hole], visible: true }"
```

**Note:** `diff` and `diff_2d` accept arrays on both inputs. They implicitly union each array before computing the difference: `diff(base, sub) = base₁ ∪ base₂ ∪ ... − (sub₁ ∪ sub₂ ∪ ...)`

### Editing individual atoms with atom_edit

The `atom_edit` node applies a diff (additions, deletions, replacements, moves) to an input atomic structure. Edits are specified via a line-based text format in the `diff` property.

```bash
# Fill with Si atoms and edit it (T-center defect). The PRIMARY/SECONDARY override
# changes only the elements: the lattice stays the default diamond one (3.567 Å),
# which is what these coordinates are in.
atomcad-cli edit --replace <<'EOF'
geom = cuboid { extent: (2, 2, 2), visible: false }
atoms = materialize { shape: geom, parameter_element_value_definition: "PRIMARY Si\nSECONDARY Si", rm_single: true, visible: false }
edited = atom_edit { molecule: atoms, diff: """# Replace two Si with C, add bridging H
~C @ (0.89175, 2.67525, 2.67525)
~C @ (1.7835, 1.7835, 3.567)
+H @ (1.337625, 2.229375, 3.121125)
bond 1-3 single
bond 2-3 single""", visible: true }
EOF
```

**Diff format — atom lines:**
- `+El @ (x, y, z)` — Add a new atom (e.g., `+H @ (1.0, 2.0, 3.0)`)
- `~El @ (x, y, z)` — Replace the base atom at this position with element El (e.g., `~C @ (0.89, 2.67, 2.67)` replaces Si with C)
- `~El @ (x, y, z) [from (ox, oy, oz)]` — Move atom: match base atom at anchor position, place at new position
- `- @ (x, y, z)` — Delete the base atom at this position

**Diff format — bond lines (1-based atom indices referencing atom lines above):**
- `bond A-B order` — Add bond (orders: single, double, triple, quadruple, aromatic, dative, metallic)
- `unbond A-B` — Delete bond between atoms A and B

**Notes:**
- Positions are in **angstroms** (real-space coordinates), not lattice units
- Element symbols are standard (C, Si, N, O, H, Fe, etc.) and case-insensitive
- The `~` prefix means the atom is expected to match a base atom (replacement or move). The `+` prefix means a new atom addition. Both use positional matching internally, but `~` preserves user intent on round-trip.
- Deletion matches atoms in the input structure by position (within tolerance)
- Lines starting with `#` are comments. Blank lines are ignored.

**Multi-line diff with heredoc:**

```bash
atomcad-cli edit <<'EOF'
edited = atom_edit { molecule: atoms, diff: """
+C @ (0.0, 0.0, 10.0)
+C @ (0.0, 0.0, 13.567)
bond 1-2 single
""", visible: true }
EOF
```

**atom_edit outputs and options:**
- Two output pins: `result` (pin 0, the edited structure — a `Crystal` in gives a `Crystal` out, a `Molecule` a `Molecule`; with no input it is a `Molecule` built from the diff alone) and `diff` (pin 1, the diff itself as a `Molecule`). Reference the diff as `edited.diff`; display it with `visible: [diff]`. (The old `output_diff` property is deprecated: display the `diff` pin instead.)
- `show_anchor_arrows` — In the `diff` output, show arrows from anchor positions to moved atoms (default: true)
- `base_bonds` — Include base bonds between matched diff atoms in the `diff` output (default: true)
- `tolerance` — Positional matching tolerance in angstroms (default: 0.1)
- `error_on_stale` — Report an error when diff lines no longer match the input (default: false)

### Creating realistic atomic structures

```bash
# Passivate dangling bonds with hydrogen
atomcad-cli edit --code="atoms = materialize { shape: geom, passivate: true, visible: true }"

# Remove single-bond atoms recursively (cleaner structures)
atomcad-cli edit --code="atoms = materialize { shape: geom, rm_single: true, visible: true }"

# Override motif elements (e.g., silicon carbide instead of diamond);
# stdin keeps the string's \n away from the --code escape processing
atomcad-cli edit <<'EOF'
atoms = materialize { shape: geom, parameter_element_value_definition: "PRIMARY Si\nSECONDARY C", visible: true }
EOF
```

**materialize options:**
- `passivate: true` — Add terminating atoms to dangling bonds (default: true); hydrogen unless `passiv_elem` (atomic number, default 1) picks F/Cl/Br/I
- `rm_single: true` — Recursively remove atoms with only one bond (default: false)
- `rm_unbonded: true` — Remove atoms left with zero bonds after the cut (default: true)
- `surf_recon: true` — Enable surface reconstruction (cubic diamond (100) 2×1 only)
- `parameter_element_value_definition: "..."` — Override motif parameter elements (newline-separated `PARAM_NAME Element`, e.g., `"PRIMARY Si\nSECONDARY Si"` for pure silicon)

Run `atomcad-cli describe materialize` for the full, current pin list.

### Parametric design with custom nodes

Custom nodes are created by defining subnetworks:

1. Create a node network named `my_shape` using `networks add --name "my_shape"`
2. Add `parameter` nodes to define inputs (each parameter becomes an input pin): `param_name` is the pin name, `data_type` its type, `sort_order` its position, and the optional `default` is **wire-only** (wire a value node into it)
3. Set the network's output node via `output <node_name>`
4. Switch to another network and use `my_shape` as a node type

```bash
# Create a custom node (subnetwork) for a parameterized shape
atomcad-cli networks add --name "rounded_cube"
atomcad-cli edit --replace <<'EOF'
size_default = int { value: 10 }
size = parameter { param_name: "size", data_type: Int, sort_order: 0, default: size_default }
radius_default = int { value: 2 }
radius = parameter { param_name: "corner_radius", data_type: Int, sort_order: 1, default: radius_default }
ext = ivec3 { x: size, y: size, z: size }
base = cuboid { extent: ext }
corner = sphere { radius: radius }
result = intersect { shapes: [base, corner] }
output result
EOF

# Switch to Main and use the custom node
atomcad-cli networks activate "Main"
atomcad-cli edit --code="part = rounded_cube { size: 20, corner_radius: 3, visible: true }"

# Fill with atoms
atomcad-cli edit --code="atoms = materialize { shape: part, passivate: true, visible: true }"
```

**Simpler example:**

```bash
# Create a reusable scaled sphere node
atomcad-cli networks add --name "scaled_sphere"
atomcad-cli edit --replace <<'EOF'
size_default = int { value: 5 }
size = parameter { param_name: "size", data_type: Int, default: size_default }
s = sphere { radius: size }
output s
EOF

# Use it in Main
atomcad-cli networks activate "Main"
atomcad-cli edit --code="part1 = scaled_sphere { size: 10, visible: true }"
```

### Functional programming with map

The `map` node lazily applies a function to each element of `xs` and outputs an **iterator** (`Iter[T]`); wire it into `collect` (pin `iter`) to get an array you can display or evaluate. The function is either its inline `body { … }` (see *Zone Bodies*) or a function value wired into `f`. A node's function pin `@name` turns the node into a function of its **unwired** inputs; its wired inputs are baked in (partial application):

```bash
# range yields 0, 1, 2, ... (an Iter[Int])
# p has inputs index and gap; gap is wired, so @p is a function of index alone.
# A custom node works the same way: p = pattern { gap: g }, then f: @p
atomcad-cli edit <<'EOF'
r = range { start: 0, count: 5, step: 1 }
g = int { value: 3 }
p = expr { expression: "index * gap", parameters: [{ name: "index", data_type: Int }, { name: "gap", data_type: Int }], gap: g }
m = map { xs: r, input_type: Int, output_type: Int, f: @p }
c = collect { iter: m, visible: true }
EOF
```

`map.f` accepts any function whose first parameter matches the element type; `@p` with more than one unwired input yields an iterator of functions of the remaining ones.

### Mathematical expressions with expr

The `expr` node evaluates mathematical expressions with dynamic input pins.

**Parameters are the input pins.** A fresh `expr` has no parameters and the expression `0`. Wiring a name declares it: each wired input becomes a parameter typed by its source, so you rarely need to write `parameters`. Write `parameters` explicitly only to declare an input you will not wire, or to pin a type; a statement that sets `parameters` declares exactly what it lists, and a wire to any other name is a warning.

```bash
# Wired names declare themselves, typed by their sources
atomcad-cli edit <<'EOF'
a = int { value: 5 }
b = float { value: 3.0 }
sum = expr { expression: "x + y", x: a, y: b }
EOF

# Explicit parameters (e.g. a parameter with no wire yet)
atomcad-cli edit <<'EOF'
a = int { value: 5 }
scaled = expr {
  expression: "x * k",
  parameters: [{ name: "x", data_type: Int }, { name: "k", data_type: Int }],
  x: a
}
EOF
```

**Available data types for parameters:** `Int`, `Float`, `Bool`, `Vec2`, `Vec3`, `IVec2`, `IVec3`

**Supported in expr:**
- Arithmetic: `+`, `-`, `*`, `/`, `%`, `^` (exponent)
- Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`
- Logic: `&&`, `||`, !
- Conditionals: `if condition then value1 else value2`
- Vectors: `vec2(x,y)`, `vec3(x,y,z)`, `ivec2`, `ivec3`, member access `.x`, `.y`, `.z`
- Functions: `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `sindeg`, `cosdeg`, `tandeg`, `asindeg`, `acosdeg`, `atandeg`, `atan2deg`, `degrees`, `radians`, `sqrt`, `exp`, `log`, `abs`, `min`, `max`, `clamp`, `sign`, `lerp`, `floor`, `ceil`, `round`, `length2`, `length3`, `normalize2`, `normalize3`, `dot2`, `dot3`, `cross`, `distance2`, `distance3`
  - `min`/`max` are variadic (≥2 args); `min`/`max`/`clamp`/`sign`/`abs` are type-preserving (all-Int args → Int result, else Float). `abs_int` is a deprecated Int-only alias of `abs`. `log` is natural log; `lerp(a,b,t)` is unclamped and returns Float.
  - Classic trig (`sin`/`asin`/`atan2`/…) works in radians; the `deg`-suffixed variants (`sindeg`/`asindeg`/`atan2deg`/…) take/return degrees. `degrees(x)`/`radians(x)` convert between them.
- Constants: `pi` (Float; a same-named parameter shadows it)

## Important Notes

- **facet_shell:** Currently only works correctly with cubic unit cells
- **structure_move/structure_rot:** Discrete lattice transformations (integers only). For continuous transforms on atomic structures, convert the `Crystal` to a `Molecule` with `exit_structure`, then use `free_move` / `free_rot` (they reject a `Crystal`):
  ```bash
  atoms = materialize { shape: geom, visible: false }
  mol = exit_structure { input: atoms }
  moved = free_move { input: mol, translation: (10.0, 0.0, 0.0), visible: true }
  ```
- **half_space:** Creates infinite half-spaces; useful for clipping geometry via intersection
- **Lone atoms:** `materialize` removes atoms with zero bonds after the geometry cut (its `rm_unbonded` pin, on by default)

## See Also

- `atomcad-cli describe <node>` for detailed node documentation
- `atomcad-cli nodes` to browse available node types
- `references/text-format.md` for complete text format specification
- `references/data-types.md` for detailed type system documentation
