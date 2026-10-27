# Proposal: deliberate node placement — UI alignment + AI `place`

**Date:** 2026-10-26 · **Status:** proposal, input for a design doc.
**Follows:** `reports/relational_layout_assessment_2026-10-26.md` (why this
shape and not reladraw-style persisted relations).

**One-sentence summary.** Humans get snapping and arrange commands while
dragging. The AI gets the same vocabulary in text: it **reads** the drawing's
boxes, **states** where its nodes go (relative to other nodes, or at an
absolute anchor), **chooses** how room is made, and **hears back** exactly what
moved and why. Nothing relational is ever stored. Every statement becomes
ordinary coordinates during the edit that contains it.

## Principles (the design doc should keep these)

1. **Coordinates stay the only stored truth.** No groups, constraints or
   relations in `.cnnd`. So there is no migration, no round-trip tax, and no
   question of what a drag does to a relation.
2. **One vocabulary, three users.** The snapping guides, the arrange
   commands, the AI's layout statements and the read-out all speak the same
   words: `right_of`, `below`, `top_even_with`, `straight_from`, `column`,
   `row`, gaps. The arrange commands and the AI's `column` / `row` /
   `arrange` / `straighten` share one Rust implementation.
3. **The AI owns the nodes it creates.** It may place those freely. It moves
   nodes it did not create only when the user asks. This rule lives in the
   skill guidance and is not enforced in code.
4. **The AI chooses the operation and where; the code executes it and keeps
   the invariants.** The AI picks where its nodes go and, optionally, how room
   is made (`insert`). It never picks primitive parameters: shift lines, fixed
   sets, push directions. Those carry the wire-safety guarantees of
   `layout/motion.rs`.
5. **The automatic pass stays as a safety net.** Whatever the AI's statements
   leave unhandled (nodes it did not place, an overlap it caused, a backward
   wire) is repaired by today's incremental rules, exactly as now.
6. **No silent effects.** Every move is reported with its cause, and the AI
   can preview an edit's moves before committing (`--dry-run`).

> **Correction to the assessment (§3a there):** it suggested "overlaps are
> reported, not repaired", reladraw's rule. That rule fits reladraw because
> its author owns every box. In atomCAD the AI owns only the boxes it places.
> The others belong to a human, and the incremental layout is the agreed way
> to make room in a human's drawing. Principles 4–5 replace that rule.

---

## Part 1 — UI: alignment and snapping

Today the node editor has no snapping and no align commands. It paints a grid
that nothing snaps to. The corpus shows humans aligning by eye: 52–54 % of
nodes sit within 8 px of another node's x, but under 5 % share an exact x.
These features turn that eyeballing into exact alignment, which also makes
the AI read-out (Part 2) cleaner.

### U1 — Smart guides while dragging

While a node (or a selection) is dragged, the **grabbed node's** box snaps to
nearby boxes, independently per axis, and a dashed guide shows why. Guides
appear only during the drag, as in draw.io and Inkscape (mechadense's picture,
with the crosses and dashed lines hidden until needed).

| Snap | Axis | Guide drawn |
|---|---|---|
| **Straight wire**: the dragged node's input pin y = the source's output pin y, for any wire touching the dragged node (either end) | y | the wire turns solid/highlighted |
| Left edge = left edge (a column) | x | vertical dashed line through both |
| Top edge = top edge (a row) | y | horizontal dashed line |
| Right edge = right edge, bottom = bottom | x / y | dashed line |
| **Normal gap**: one `normal` gap right of / below a neighbour (50 px across, 30 px down) | x / y | a short bracket labelled `50` / `30` |
| **Equal spacing**: the same gap as the neighbouring pair in the column/row the node is joining | x / y | brackets on both gaps, labelled `= 42` |

- **Candidates:** same-scope nodes (comments included) whose box is within
  the viewport, excluding the selection itself. A body node snaps only to its
  body siblings.
- **Threshold:** 6 screen px, so it stays the same at every zoom. If several
  candidates are in range: straight wire > edge alignment > gap > equal
  spacing, then nearest.
- **Escape hatch:** holding **Alt** disables snapping for that drag, plus a
  preference toggle (Preferences › Layout, "Snap while dragging", default on).
- **Multi-selection:** only the grabbed node snaps, and the rest of the
  selection moves by the same delta.
- **Performance:** drag ticks must not call `notifyListeners` (an existing
  rule). At drag start, build sorted lists of candidate edges and pin y's from
  the `LayoutCache`. Each tick is then a binary search plus an overlay repaint.
  This is Dart-only. Commit goes through the existing `end_move_nodes`, so
  undo and `hand_moved` are unchanged.

### U2 — Arrange commands on a selection

These live in the node context menu under **Arrange ▸** (with shortcuts) and
apply to ≥ 2 selected nodes in one scope. Each is one undo step
(`MoveNodesCommand`) and sets `hand_moved` on the nodes it moved, since the
human ordered the move.

| Command | Effect |
|---|---|
| Align left / right / top / bottom | to the selection's extreme edge |
| **Stack as column** | ordered by current y; lefts aligned to the leftmost; `normal` vertical gap |
| **Stack as row** | ordered by current x; tops aligned to the topmost; `normal` horizontal gap |
| Distribute vertically / horizontally | keep the two extreme nodes; make the gaps equal |
| **Straighten wires** | in topological order, move each selected node in y so that the wire from its first wired input is horizontal; unselected nodes never move |
| Arrange selection | Sugiyama on the selection only (`layout_subgraph` exists from the incremental work), with the block's top-left kept where the selection's was |

- **Only selected nodes move.** If the result overlaps an unselected node, it
  is left visible for the user to judge, because the user just saw what they
  asked for.
- **One Rust implementation**, `layout/arrange.rs`
  (`arrange(scope, ids, ArrangeOp) -> moves`), is used by these commands and
  by the AI's `column` / `row` / `arrange` / `straighten` (A3), as
  Principle 2 requires.

**Not in Part 1:** snap-to-grid (no hand-drawn file uses a grid, so add it
later if asked), persistent groups that drag together (the assessment's §5
track, only on evidence), and auto-arranging on drop.

---

## Part 2 — AI: deliberate placement

Three features, A1–A3. Each is useful alone, and they build on each other in
this order.

### A1 — The layout report, and `--dry-run`

The AI edit result JSON gains a `layout` section. The AI History panel's
*Layout* tab already shows the same facts to the human; this hands them to
the AI.

```json
"layout": {
  "placed":      ["n", "pitch", "r", "holes", "holes_u"],
  "auto_placed": [],
  "moved": [
    { "node": "note", "dy": 180, "cause": "insert", "why": "below d, for [n, pitch]" },
    { "node": "cut",  "dx": 260, "cause": "auto",   "why": "keeps holes_u -> cut.sub forward" },
    { "node": "fill", "dx": 260, "cause": "auto",   "why": "same shift as cut" }
  ],
  "findings": [],
  "warnings": []
}
```

- `placed` lists nodes positioned by a `place` statement, and `auto_placed`
  lists those positioned by the incremental rules (today's Step 4).
- `moved` lists every existing node that moved, with a **cause**:

  | `cause` | Meaning | Did the AI ask for it? |
  |---|---|---|
  | `place` | the AI `place`d this existing node (a requested move) | yes, directly |
  | `insert` | room made by a `place … (insert)` clause | yes, it chose the operation |
  | `arrange` | an `arrange` / `straighten` statement | yes |
  | `push` | pushed off a placed node (no `insert`) | indirectly: it placed something there |
  | `grow` | a node this edit grew pushed it (Step 2) | no, a mechanical consequence |
  | `auto` | the safety net: backward-wire repair, or making room for a node the AI did not place | no |

  `auto` moves mean the AI's plan was incomplete. This is the "I didn't ask
  you to move that" channel: the AI can tell the user before they notice.
- `findings` covers what this edit **created** and the rules did not fix:
  node overlaps (`"pitch covers note by 160×20 px"`) and backward wires. They
  are reported in reladraw's style: what, where, how much. Overlaps and
  backward wires that existed before the edit are baseline and are not
  reported (the private corpus has 152 overlaps that nobody minds).
- `warnings` covers layout statements that could not be honoured (A3 rules),
  with the fallback that was used instead.
- `atomcad-cli check-layout [--network N]` prints all findings, including the
  pre-existing ones, each flagged `new` or `baseline`.

**`edit --dry-run`** runs the whole edit (network changes and layout pass) on
a copy and returns the same result JSON without applying anything. No undo
entry, no dirty flag, nothing visible in the GUI. The AI uses it when an edit
uses `insert` or touches a crowded area, so it can see the moves before the
user does. It then either commits as is and tells the user what will move, or
revises its statements. The edit path already takes an identity snapshot and
computes a delta per scope, so a dry run means running the pass against a
cloned network and discarding it.

### A2 — `query --layout`: reading the drawing

This is opt-in, so the ordinary `query` output and its token cost do not
change. The read-out reports facts (node boxes), plus one clearly labelled
inference (aligned groups). It does not try to describe the drawing as
`place` statements.

One command, `query --layout`, with filters that select which nodes are
listed. The output format is the same whatever the filter:

| Filter | Lists |
|---|---|
| *(none)* | every node in the scope |
| `--nodes a,b,…` | just these nodes: "where is X, how big is it?" |
| `--box x0,y0,x1,y1` | every node whose box **intersects** the area, including an empty list ("is this area free?") |
| `--scope m1[/m2…]` | combines with either; a body is queried in its own body coordinates |

The usual sequence is `--nodes` to locate the nodes the user mentioned, then
`--box` over the area the AI is thinking of using.

```
$ atomcad-cli query --layout --nodes d
# Main: bbox (100,20)–(1150,490), 11 nodes
d       100,280  160×60

$ atomcad-cli query --layout --box 0,0,700,600
# Main: bbox (100,20)–(1150,490), 11 nodes; 7 in box (0,0)–(700,600)
# boxes (left, top, width × height), rounded to 10 px
w       100,100  160×60
h       100,190  160×60
d       100,280  160×60
note    100,370  180×80
dims    310,160  160×110
hole_r  310,390  160×60
hole    520,390  160×80
# groups (inferred: edges within 8 px, gaps ≤ 60 px)
column  w, h, d          gap 30
row     hole_r, hole     gap 50
```

- **The header line is always printed**: the whole drawing's bbox and node
  count. The AI knows the overall extent before it picks a box, and
  everything below `y = 490` is free without asking.
- **Boxes** are the rendered footprints (`rendered_node_size`), the user's
  nodes included as they stand. No heuristic is involved. Their coordinates
  are the ones `place … at (x, y)` takes.
- **Groups** are the only inference: columns (lefts within 8 px, vertical gaps
  ≤ 2 × `normal`) and rows (tops within 8 px) of ≥ 2 nodes, detected over the
  whole scope. A group is listed if any member is in the selection, with
  members outside it marked `…`, so a column cut by the box edge still reads
  as one column. Groups are a hint for what the AI might extend ("continue
  this column"), labelled as inferred, and never stored. The D10 argument (a
  persisted guess becomes indistinguishable from intent) applies.
- **No free-space computation.** A `--box` query over the candidate area
  answers "is it free?" exactly, with no heuristic.

An earlier draft printed the drawing as `place` statements reproducing it to
within 8 px. That was dropped. On hand-drawn networks (under 5 % exact
alignment) most lines came out as `right_of X (gap: 47, dy: 13)`, which is
coordinates in disguise. The choice of parent node was arbitrary and could
flip after a small drag.

### A3 — Layout statements: `place`, `arrange`, `straighten`

New statement kinds next to `output` and `delete`. They are
**instructions** that act during the edit; `query` never prints them back.

```
place <subject> at (<x>, <y>)                      # absolute: top-left corner
place <subject> <clause> [, <clause>]              # relative
arrange    [name, …]                               # Sugiyama on these nodes only, top-left kept
straighten [name, …]                               # level each node with its first wired input

subject := name
         | [name, …] column [(gap: G)]      # stacked in written order, lefts aligned
         | [name, …] row    [(gap: G)]      # in written order, tops aligned

clause  := right_of | left_of | above | below  <targets> [(gap: G, dx: N, dy: N, insert)]
         | top_even_with | bottom_even_with | left_even_with | right_even_with  <targets>
         | straight_from <name>        # y such that the wire from <name> into the subject is horizontal
         | straight_to   <name>        # y such that the wire from the subject into <name> is horizontal

targets := name | name and name …          # several targets = the box around them
G       := tight | normal | wide | <px>    # across 25/50/100, down 15/30/60
```

#### Meaning

- **`at (x, y)`** sets the subject's top-left corner in the coordinates
  `query --layout` prints (body coordinates inside a body). It sets both axes,
  so it is never combined with a clause. For a `column` / `row` subject it
  positions the group's top-left.
- **A lone direction also fixes the other axis**, using the convention people
  already draw with: `right_of X` aligns tops (a row reads along the titles),
  and `below X` aligns lefts (a column). Any explicit clause on that axis
  overrides the default. `dx` / `dy` shift the result by a fixed offset, so
  any position near a target is reachable without leaving relative terms.
- **Each axis is stated once.** Two clauses on one axis is an error ("said
  twice"), and so are a cycle, an unknown target, or a target in another
  scope.
- **Targets** may be existing nodes or nodes created earlier in the same
  script.
- **In a body:** inside a `body { … }` block, or path-addressed
  (`place m1/x below m1/d`), a statement acts on that body. Body coordinates
  are clamped to ≥ 0, with a warning if clamped.
- **A broken statement never fails the edit.** The network change is what
  matters. The node falls back to automatic placement, and the problem
  appears in `layout.warnings`.

#### Absolute or relative: late binding

The AI could compute `right_of X` as a coordinate itself; the arithmetic is
trivial. Relations exist because they **defer that arithmetic until its
inputs exist**. When the AI writes the script, often they do not:

- **New nodes have no size yet.** Width is 160 px except for comments and
  expanded HOFs, which are as wide as their measured body. Height is
  title + 22 px per input + an optional 20 px subtitle row + padding.
  Anything placed against a new `map` depends on a width the AI can only
  guess.
- **`straight_from` needs pin positions**: top + 33 + 22 × pin index, shifted
  by the subtitle row if present.
- **The edit itself moves things.** A node that gains an input grows, and the
  Step 2 repair pushes its neighbours *before* placement runs. An earlier
  `insert` in the same script shifts its targets. A relation is resolved
  against the geometry as it then stands. A coordinate computed from the
  pre-edit query is stale.
- **The read-out is rounded to 10 px**, so coordinates derived from it lose
  exact alignment.

Since offsets already make any position reachable, withholding absolute
positions would buy no safety. An absolute position is placed, pushed around
and reported exactly like a relative one. It is offered for the case where
there is no natural neighbour: the first node of a new group, in free space
found with `--box`. Hence the skill rule: **anchor absolutely, attach
relatively.**

#### Making room: `insert`

Without `insert`, whatever a placed node lands on is pushed away by today's
Step 5b cascade, in the direction the code picks. That direction is scored to
spare hand-placed nodes; this is cause `push`. With `insert`, the AI chooses
the operation, and it is the primitive that matches the clause's direction:

| Clause | Primitive run | Effect, as the AI may rely on it |
|---|---|---|
| `right_of X (insert)` / `left_of X (insert)` | `shift_half_plane` with the line at the subject's left edge (snapped as today), X and its upstream closure fixed | everything at or right of the subject's spot moves right by its width + gap, rigidly, except what feeds X |
| `below X (insert)` / `above X (insert)` | `cascade` from the subject's box, direction fixed to Down / Up | only what the subject collides with moves down (up), plus whatever that then hits; order within the column is preserved |

The room is sized from the subject's real rendered box, so the AI never
supplies a width. `insert` is valid only on a direction clause, and at most
once per statement.

**Deliberately not exposed:**
- primitive parameters: shift-line positions, fixed sets, cascade directions,
  slide window, snap tolerance. The fixed set is the wire-safety proof
  (`layout/AGENTS.md`, "Two motion primitives"), and letting the AI choose it
  could turn forward wires backward;
- standalone `open_column` / `open_row` (making room without placing
  anything). Room is almost always made *for* something, so `insert` covers
  it. Add them later only if requests like "make some space here" turn up;
- a "manual" mode that switches the safety net off. If the AI's statements
  are complete the safety net does nothing, so turning it off would only
  allow broken drawings.

#### `arrange` and `straighten`

These are the U2 commands, with the same code. They are for "tidy this up"
requests, which today have only one answer: the global reflow.
`arrange [a, b, c]` runs Sugiyama on those nodes only and keeps their bbox's
top-left. `straighten [a, b, c]` moves each one in y, in topological order,
until the wire from its first wired input is horizontal. Unlisted nodes never
move, except that whatever the result lands on is pushed (cause `push`).
Because they normally touch the user's nodes, Principle 3 applies: use them
only on request.

#### How it fits into the incremental pass (`layout/incremental.rs`)

1. Step 2 (grown nodes) runs as today. Moves are tagged `grow`.
2. **New:** `place` statements are resolved in dependency order (ties in
   written order) against real sizes (`rendered_node_size`, with bodies
   settled inside-out as today). Each resolved node is positioned and
   performs its `insert`, if any. It then joins the obstacle set and the
   **fixed** set of every primitive for the rest of the pass, so later
   statements see the drawing as already moved.
3. **New:** `arrange` / `straighten` statements run in written order.
4. Steps 3–6 run as today for the remaining unplaced added nodes, with the
   placed ones as anchors and fixed obstacles. The cascade pushes existing
   nodes off placed nodes the AI did not `insert` (cause `push`). Step 6
   repairs a new backward wire by shifting the destination's half-plane, with
   placed nodes fixed (cause `auto`).
5. Steps 7–8 (comments) as today. Every move goes into `layout.moved` with
   its cause (A1).

#### When a placed node lands on something

| Lands on | What happens | Reported as |
|---|---|---|
| an existing node, `insert` given | the chosen primitive makes room before the node lands | `moved`, cause `insert` |
| an existing node, no `insert` | the cascade pushes it, and whatever it then hits, away, minimal and local, as Step 5b does today | `moved`, cause `push` |
| a new node the AI did not `place` | nothing: that node is placed afterwards and avoids placed nodes | — |
| another placed node | nothing: both are pinned, so this is the AI's own conflict | `findings: overlap …` |
| a node fixed by a primitive in the same pass | nothing | `findings: overlap …` |

Pushes are bounded as today: simulated over both corpora, a push moves ≤ 5
nodes 98–99 % of the time and at most 11.

*Deliberately not chosen:* letting the placed node slide aside on its own
(Step 5a's slide). That would protect the user's nodes, but `place` would
then not mean exactly what it says, and the AI could not rely on its own
statements.

**Existing nodes:** `place` on an existing node is an explicit move (cause
`place`), under the same pinning. It does not touch `hand_moved`, which keeps
meaning "a human dragged this".

#### What the skill teaches (`.claude/skills/atomcad/`)

- **Look first.** Before adding more than one or two nodes, or whenever the
  user uses spatial words ("next to", "below", "with the others", "tidy"),
  locate the nodes involved with `query --layout --nodes …`. Then check the
  area you intend to use with `--box`.
- **Anchor absolutely, attach relatively.** Use `at (x, y)` for the first
  node of a new group when there is no natural neighbour, typically a lane in
  free space you confirmed with `--box`. Attach everything else with
  relations. Always use relations when a position depends on the size of a
  node this edit creates, on pin positions, or on a node this edit may move.
  Never compute a position from a new node's assumed size.
- **Place by meaning.** Parameter columns go next to the existing parameter
  column, new chains go in a lane below the chain they branch from, and chain
  links use `straight_from`.
- **Choose how room is made.** Use `(insert)` to insert into an existing
  column or row (`below X (insert)`) or into a chain (`right_of X (insert)`).
  Without it, whatever you land on is pushed wherever the code finds best.
- **Preview when it matters.** For edits with `insert`, or in crowded areas,
  run `edit --dry-run` first and read `layout.moved`.
- **Report.** After every edit, read `layout.moved` and `layout.findings`.
  Tell the user about moves of their nodes. Treat `auto` moves as a sign the
  plan missed something. Fix findings involving your own nodes with a
  follow-up `place`.
- **Respect ownership.** Never `place`, `arrange` or `straighten` a node you
  did not create unless asked.

---

## Example session

**The network** (`Main`, arranged by hand earlier):

```
 x:  100          310          520          730          940
     ┌───┐                     ┌──────┐     ┌─────┐      ┌──────┐
     │ w ├──┐                  │ slab ├────▶│ cut ├─────▶│ fill │ (visible)
     └───┘  │    ┌──────┐  ┌──▶└──────┘ ┌──▶└─────┘      └──────┘
     ┌───┐  ├───▶│ dims ├──┘            │
     │ h ├──┤    └──────┘               │
     └───┘  │                           │
     ┌───┐  │                           │
     │ d ├──┘                           │
     └───┘          ┌────────┐  ┌──────┐│
     ┌──────────┐   │ hole_r ├─▶│ hole ├┘
     │ note:    │   └────────┘  └──────┘
     │ "dims in │
     │  cells"  │
     └──────────┘
```

`w, h, d → dims → slab`; `hole_r → hole`; `cut = diff { base: slab, sub: hole }`;
`fill = atom_fill { … }`.

**User:** *"Replace the single hole with a row of `n` holes, `pitch` cells
apart along x. Put n and pitch with the other size parameters, and keep the
hole machinery as its own lane at the bottom."*

### 1. The AI looks

```
$ atomcad-cli query                       # the network, as today
$ atomcad-cli query --layout --nodes d,hole
# Main: bbox (100,20)–(1150,490), 11 nodes
d       100,280  160×60
hole    520,390  160×80

$ atomcad-cli query --layout --box 0,0,700,600      # the area around both
# Main: bbox (100,20)–(1150,490), 11 nodes; 7 in box (0,0)–(700,600)
w       100,100  160×60
h       100,190  160×60
d       100,280  160×60
note    100,370  180×80
dims    310,160  160×110
hole_r  310,390  160×60
hole    520,390  160×80
# groups (inferred)
column  w, h, d          gap 30
row     hole_r, hole     gap 50
```

From this the AI learns what the plain text cannot tell it: `w, h, d` are a
column, the user's note sits directly under that column, and nothing lies
below `y = 490`. That is room for a new lane.

### 2. The AI writes the edit and previews it

```
n     = int { value: 3 }
pitch = int { value: 6 }
r     = range { start: 0, step: 1, count: n }
holes = map {
  xs: r,
  input_type: Int,
  output_type: Geometry,
  body {
    x   = expr { a: $element, b: ^pitch, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    off = ivec3 { x: x, y: 0, z: 0 }
    mv  = lattice_move { geometry: ^hole, offset: off }
    output mv
  }
}
holes_u = union { shapes: holes }
cut     = diff { base: slab, sub: holes_u }

place [n, pitch] column below d (insert)          # continue the column; the note makes way downward
place r at (310, 540)                             # anchor the lane in the free space
place holes right_of r, straight_from r           # attach: holes' width is unknown to the AI
place holes_u right_of holes, straight_from holes
```

(Node types are illustrative. The body's own nodes are left to the automatic
body layout, which handles a 3-node body well.)

The AI sends this with `--dry-run` first:

```json
"layout": {
  "placed": ["n", "pitch", "r", "holes", "holes_u"],
  "moved": [
    { "node": "note", "dy": 180, "cause": "insert", "why": "below d, for [n, pitch]" },
    { "node": "cut",  "dx": 260, "cause": "auto",   "why": "keeps holes_u -> cut.sub forward" },
    { "node": "fill", "dx": 260, "cause": "auto",   "why": "same shift as cut" }
  ],
  "findings": []
}
```

The note move is the one the AI chose. The `auto` shift is the one it missed:
the `map` renders wide, so `holes_u` ends up right of `cut`, and the safety
net shifts `cut` and `fill` to keep that wire forward. Starting the lane
further left would not help, because the map is wider than the gap. The AI
accepts the shift, commits the same edit without `--dry-run`, and tells the
user what happened:

> *Added `n` and `pitch` under `d` (your note moved down to make room), and
> the holes as their own lane at the bottom (`r → holes → holes_u`, wired
> into `cut.sub`). The `map` body is wide, so `cut` and `fill` shift 260 px
> right to keep the wire into `cut` running forward. Want me to put the note
> beside the column instead?*

### 3. The human tidies up with Part 1

The user drags the note to the right of `h`. While dragging, a dashed guide
snaps its top to `h`'s top, and a `50` gap bracket appears against the column.
Then they select `r, holes, holes_u, cut, fill` and choose **Arrange ›
Straighten wires**, which lines `cut` and `fill` up with the `holes_u` wire
the user now cares about. Two gestures, both exact, one Ctrl+Z each.

### 4. A later edit benefits from the tidying

**User:** *"Also make the holes' z position a parameter."*

```
$ atomcad-cli query --layout --box 0,0,700,600
…
note    310,190  180×80
# groups (inferred)
column  w, h, d, n, pitch    gap 30
row     h, note              gap 50
…
```

The read-out now shows one five-member column (the alignment is exact, so it
is detected as a group, not as five loose nodes) and the note where the user
put it. The AI adds `z0 = int { value: 0 }`, rewires `off`, and writes
`place z0 below pitch`. The space is free, so no `insert` is needed. The
result: `moved: []`, `findings: []`.

### 5. A tidy-up request

**User:** *"The three parameters feeding `dims` are a bit messy, tidy them."*

The nodes are the user's, but the user asked for the move. They have no
inputs, so `straighten` has nothing to level them against. The AI writes
`place [w, h, d] column (gap: normal) at (100, 100)`, and only those three
move (cause `place`). Today's only option would have been the
global reflow, which rewrites every position in the network.

---

## How this beats letting today's auto-layout do it all

What the incremental layout would do in the same session (Steps 3–6 of
`doc/design_incremental_layout.md`):

| Moment | Today's automatic placement | With these features |
|---|---|---|
| Where do `n`, `pitch` go? | `n` feeds the new `r`, so it is part of the new block and lands left of `r`, wherever the block lands. `pitch` is likely referenced only from inside the body (`^pitch`), so it probably has no root-scope wire and goes to the anchorless spot, the bottom-left of the drawing. Neither ends up "with the other size parameters". | Exactly in the column, after `d`. `insert` makes the column below them move down, so column order is decided by the AI, not by pin heights. |
| Where does the hole lane go? | Step 4 aligns the block vertically with its wired anchors. Its main external wire is into `cut` in the top row, so it is placed level with the main chain, where there is no room. Step 5 slides it or pushes the user's nodes away. "Its own lane at the bottom" is not expressible in wiring. | Anchored in confirmed free space, chain links attached `straight_from` one another. |
| The user's nodes move | Moved too, with no explanation, and the AI cannot tell that it happened. | Same rules, same safety, but previewed with `--dry-run` and reported with a cause, so the AI warns the user and offers a fix. |
| "Tidy this up" | Only the global reflow, which rewrites every position in the network. | `place … column`, `arrange` or `straighten` on the named nodes, or the user's Arrange commands. All of them are local and exact. |
| "Where is X?" / "Is Y next to Z?" | The AI cannot answer. | The read-out answers it. |
| The next edit | Starts from whatever the heuristic produced last time. | Starts from a drawing the human tidied *exactly* (snapping), which the read-out then reports as clean groups the AI can extend. |

**For a plain chain insertion, today's pass already does the right thing.**
Its "no room" branch performs exactly the shift `right_of X (insert)` would.
There, the explicit form buys only predictability: the AI knows the shift
will happen before it reads the report.

**The general point.** Automatic placement only knows the **wiring**. Wiring
determines left-to-right order well, and it is why the incremental rules
work at all. It says nothing about the things users actually talk about:
*which group a node belongs to, which lane a branch lives in, what goes under
what*. Those come from the conversation, and the AI is the only participant
that has heard it. These features give it a channel for exactly that
information. The geometry it is unreliable at (new nodes' sizes, pin
offsets, collisions, wire-safe room-making) stays with the code.

**What does not get better, honestly:**
- The incremental layout code stays. It still places everything the AI does
  not `place`, and repairs whatever the AI's statements leave broken.
- A careless AI can place badly. The damage is bounded, though: the same
  room-making as today, previewable, reported, and one Ctrl+Z away.
- The read-out costs tokens when it is used (about one line per node in the
  queried area). Layout statements cost about one line per group, and a dry
  run costs one extra call.

---

## Phases (for the design doc)

| Phase | Content | Size | Depends on |
|---|---|---|---|
| 1 | U1 smart guides + snapping (Dart only) | M | — |
| 2 | `layout/arrange.rs` + U2 Arrange menu | M | — |
| 3 | A1 layout report with move causes + `check-layout` + `edit --dry-run` | M | — |
| 4 | A2 `query --layout [--nodes │ --box] [--scope]` | M | — |
| — | **Gate:** replay 5–10 real co-edit tasks with hand-resolved layout statements; build A3 only if the placements are clearly better than Step 4's | S | 3, 4 |
| 5 | A3 `place` (relative, `at`, `insert`) + `arrange` / `straighten` statements + incremental-pass integration + skill guidance | L | 2, 3, gate |

Parts 1 and 2 are independent and can run in parallel. Each phase needs
reference-guide updates: `node_networks.md` / `ui.md` for U1/U2, the text
format doc and the skill's `references/text-format.md` for A1–A3.

## Decisions the design doc must make

1. **Lone-direction defaults:** tops aligned for `right_of`, lefts aligned for
   `below`. This proposal picks those over reladraw's centre alignment
   because node heights vary and a row reads along the titles. Confirm.
2. **Gap words:** are 25/50/100 across and 15/30/60 down right? `normal` is
   today's `DEFAULT_HORIZONTAL_GAP` / `VERTICAL_GAP`.
3. **Statement order:** `place` in dependency order (ties in written order),
   then `arrange` / `straighten` in written order. Is the fixed split
   enough, or must an `arrange` be able to precede a `place` that targets its
   result?
4. **Per-axis absolute positions** (`x_at 100, below h`)? This proposal
   offers only `at (x, y)` for both axes. `dx` / `dy` on a relation cover most
   mixed cases.
5. **Should a `place` on an existing node set `hand_moved`?** The proposal
   says no (keep "a human dragged this" literal). The alternative: yes, when
   the user asked for the move.
6. **Should Arrange commands push unselected nodes they land on?** The
   proposal says no for the GUI (the user sees it immediately), but the AI's
   `arrange` / `straighten` push (cause `push`), since the user is not
   watching the canvas while the AI works. Confirm the asymmetry.
7. **Snap to the pins of nodes being dragged *into* a wire (the existing
   "insert on wire" drop)?** These two interactions must not fight. Decide
   which wins near a wire.
