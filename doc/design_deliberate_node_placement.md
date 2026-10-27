# Design: deliberate node placement

**Status:** draft for review.

**Background:** `reports/relational_layout_assessment_2026-10-26.md` (why
not reladraw-style persisted relations), `reports/relational_layout_proposal_2026-10-26.md`
(how the shape below was reached). Builds on `doc/design_incremental_layout.md`
(the incremental pass this design extends) and
`doc/research_intent_preserving_layout.md` §8 (whose rejection of AI-written
positions is revisited in [D1](#decisions)).

---

## Summary

Today an AI edit says *what* the network is, and the incremental layout pass
decides *where* every new node goes, using the wiring alone. Wiring is good
evidence for left-to-right order. It says nothing about what users actually
talk about: which group a node belongs to, which lane a branch lives in, what
goes under what. The AI has heard that from the user, but it has no way to
say it. It cannot even see the drawing.

This design gives the AI three channels, and gives humans the matching
direct-manipulation tools:

| Channel | The AI provides | The AI gets back |
|---|---|---|
| **Ask**: `query --layout` | a filter: named nodes, a box, a body scope | the boxes of the nodes in it, aligned groups, the drawing's extent |
| **Tell**: layout statements in the edit script | for each subject: a position (absolute, relative to other nodes, or an offset from where it stands) and, optionally, which way the nodes it lands on are pushed | — |
| **Hear**: the `layout` section of the edit result (also available as a `--dry-run` preview) | — | the box of every node it placed, every existing node that moved with from/to and its cause, new overlaps and backward wires, rejected statements |

For humans (Part B): **snapping guides** while dragging, and an **Arrange**
menu for a selection. The words are the same in both parts: `right_of`,
`below`, `top_even_with`, `straight_from`, `column`, `row`, and gaps. The code
behind them is shared as well.

The governing split: **the AI chooses the operation and where; the code
executes it and keeps the invariants; the report says who moved what.**

---

## Decisions

**D1. Coordinates remain the only stored truth.** No relation, group or
constraint is persisted. A layout statement acts once, inside the edit that
contains it, and `query` never prints it back. So there is no file format
change, no migration, no round-trip tax, and no question of what a drag does
to a relation. AI-written positions were rejected in research §8 for three
reasons. None of them survives this design: the AI now gets feedback (Ask and
Hear), positions only its own nodes unless asked (D3), and pays the token
cost only when it chooses to.

**D2. Read geometry, write relations.** The AI reads plain boxes. It writes
mostly relations, because relations defer the arithmetic until its inputs
exist: new nodes' rendered sizes, pin positions, and positions this same edit
changes. The commonest anchor, "somewhere out of the way", is itself a
relation: `below all` / `right_of all` measure against the whole drawing as
it stands. The AI may also write an absolute position, for anchoring a group
at a specific free spot. Offsets already make every position reachable, so
withholding absolute positions would buy no safety. The rule taught to the AI
is **anchor to the drawing or absolutely, attach relatively**. Moving an existing node is the
third case. "40 px down" is relative to the node's own position, so that
form (`by`) is late-bound too: it is measured from where the node stands when
the statement runs, not from where a query saw it.

**D3. The AI owns the nodes it creates.** It places those freely. It moves,
arranges or straightens nodes it did not create only when the user asks. This
is skill guidance, not enforced in code: the user may well ask. The agent
cannot remember across conversations which nodes it created, so ownership is
read from the `hand` tag (D12): a node without it is the AI's, and a node with
it is the user's. An AI node the user later drags becomes the user's.

**D4. The AI chooses the operation, never its parameters.** A statement may
name the direction in which the nodes it lands on are pushed (`push down`).
It cannot pick distances, shift lines or the set of nodes that stay put. That
set is the wire-safety proof of `shift_half_plane` (`layout/AGENTS.md`, "Two
motion primitives"). Letting it be chosen freely could turn forward wires
backward.

**D5. The incremental pass stays, as a safety net.** Growth repair, placement
of every new node the AI did not place, and backward-wire repair all run as
today. If the AI's statements are complete,
the net catches nothing. There is no "manual" mode: it would only add the
ability to leave a broken drawing.

**D6. Statements run in written order, and what they relate to stays put.**
The layout statements form a small sequential program. Each one sees the
drawing as the earlier ones left it, so the AI can reason about its script
line by line. To **pin** a node is to add it to the fixed set: no later step
of the pass moves it, and other nodes are moved around it instead. Pinning
happens at two points:

- **During the statements,** a node a statement places or arranges is pinned
  as soon as that statement finishes. Whatever it lands on is pushed out of
  the way, in the direction the AI named or else in the direction the code
  finds cheapest. A placed node never slides aside on its own.
- **After the last statement,** every node a statement placed, moved, arranged
  or *measured against* (its targets) is pinned for the automatic steps that
  follow. Room-making for unplaced nodes and backward-wire repair work around
  them, so a relation the AI wrote still holds when the edit ends. The
  pseudo-target `all` pins nothing; otherwise one `below all` would freeze
  the whole drawing.

Between statements, a target is not pinned. A later statement may push an
earlier one's target, but that is the AI's own program doing it, visible in
its own script.

**D7. Nothing moves silently.** Every position change of an existing node is
reported with its cause. Every edit can be previewed with `--dry-run`.

**D8. A broken statement never fails the edit.** The network change is what
matters. The statement is dropped, its subject falls back to what would have
happened without it, and the report says so.

**D9. Lone-direction defaults follow how people draw.** `right_of X` /
`left_of X` align tops (a row reads along the titles). `above X` / `below X`
align lefts (a column). reladraw centres instead; node heights vary too much
here for that to read well.

**D10. Gaps have three words and a number.** `tight` / `normal` / `wide` are
25 / 50 / 100 px across and 15 / 30 / 60 px down. `normal` is today's
`DEFAULT_HORIZONTAL_GAP` and `VERTICAL_GAP`. Any other gap is a number of
pixels.

**D11. Human arrange commands do not push; AI ones do.** The user watches a
GUI arrange happen and can judge an overlap at once. Nobody is watching the
canvas while the AI works, so an AI `arrange` / `straighten` pushes what it
lands on (and reports it).

**D12. `hand_moved` means "a human placed or moved this".** Until now it was
set only by a drag. It is now also set on every node a GUI action creates:
the add-node menu, a drop from the panel, paste and duplicate,
insert-on-wire, and GUI commands that position nodes themselves
(promote-to-parameter, factor selection, inline). Those nodes are the user's
even where the code chose the spot. GUI arrange commands set it too, because
a human ordered them. Nodes an AI edit creates or places, explicitly or
automatically, never get it. Nodes created in the GUI before this change load
without it. That legacy gap is accepted: such nodes look like the AI's until
the user drags them.

---

## Part A — The AI interface

### Coordinates

All three channels use one coordinate system, the one `Node.position` is
stored in: logical canvas pixels, x to the right, y **down**. A node's
position is its **top-left corner**. Its box is its rendered footprint
(`layout::size::rendered_node_size`, the one size function, pinned to
Flutter's by the parity fixture). A node inside a HOF body has body
coordinates, with the origin at the body's top-left. All numbers are
reported and accepted as whole pixels.

### A1. Ask: `query --layout`

```
atomcad-cli query --layout [--nodes a,b,…] [--box x0,y0,x1,y1] [--scope m1[/m2…]]
```

**What the AI provides:**

| Argument | Selects |
|---|---|
| *(no filter)* | every node in the scope |
| `--nodes a,b,…` | exactly these nodes; an unknown name is listed as `a  (no such node)` |
| `--box x0,y0,x1,y1` | every node whose box intersects the rectangle (corners in any order); an empty result means the area is free |
| `--scope m1/m2` | the body of the HOF at that path, in body coordinates; combines with either filter (default: the network's root) |

`--nodes` and `--box` are mutually exclusive. The usual sequence is
`--nodes` to locate what the user mentioned, then `--box` over the area the
AI means to use.

**What it gets back:**

```
# Main: extent (100,20)–(1150,490), 11 nodes; 7 in box (0,0)–(700,600)
w        100,100  160×60   hand
h        100,190  160×60   hand
d        100,280  160×60   hand
note     100,370  180×80   comment
dims     310,160  160×110
hole_r   310,390  160×60
hole     520,390  160×80
# groups (inferred)
column   w, h, d            gap 30
row      hole_r, hole       gap 50
```

| Part | Content |
|---|---|
| Header | network (and scope) name, the **extent** (bounding box of all nodes in the scope, whatever the filter), node count, and the filter's match count. Everything outside the extent is free. |
| Node lines | `name  x,y  w×h  [tags]`, sorted by top then left. Names are spelled as `query` spells them (backtick-quoted when needed, `/`-paths never needed since a listing is one scope). |
| Tags | `hand`: a human placed or moved it (`hand_moved`, D12), so it is the user's node and is left alone unless asked. `comment`: a comment box. `body`: an expanded HOF; query inside with `--scope`. |
| Groups | maximal **columns** (≥ 2 nodes, lefts within 8 px, consecutive vertical gaps ≤ 60 px) and **rows** (≥ 2 nodes, tops within 8 px, consecutive horizontal gaps ≤ 100 px), detected over the whole scope. A group is listed if any member is selected, and members outside the selection are shown as `…`. `gap` is the median gap. |

Groups are the only inference in the output, and they are labelled as such.
They are recomputed on every query and never stored: a persisted guess would
become indistinguishable from intent (`design_wire_annotations.md` D10).
There is deliberately no free-space summary, since a `--box` query answers
"is it free?" exactly.

The 8 px tolerance is `motion::COLUMN_TOLERANCE`, the same constant the
window rule uses to recognise a loose column, so the read-out and the motion
primitives agree on what a column is.

### A2. Tell: layout statements

Three new statement kinds, alongside `output` and `delete`, written anywhere
in an edit script. Their order **among themselves** is their execution order
(D6). Their position relative to node definitions does not matter: every
statement runs after the whole network change is applied. A body's statements go inside its `body { … }` block, or
path-addressed (`place m1/x below m1/d`).

```
place      <subject> at (<x>, <y>)          [push <dir>]
place      <movable> by (<dx>, <dy>)        [push <dir>]
place      <subject> <clause> [, <clause>]  [push <dir>]
arrange    [<name>, …]                      [push <dir>]
straighten [<name>, …]                      [push <dir>]

dir      := up | down | right

subject  := <name>
          | [<name>, …] column [(gap: <G>)]
          | [<name>, …] row    [(gap: <G>)]

movable  := <name> | [<name>, …]              # existing nodes only

clause   := <direction> <targets> [(<option>, …)]
          | <edge>_even_with <targets>
          | straight_from <name>
          | straight_to <name>

direction := right_of | left_of | above | below
edge      := top | bottom | left | right
targets   := <name> [and <name> …]
           | all
option    := gap: <G> | dx: <px> | dy: <px>
G         := tight | normal | wide | <px>
```

`place`, `arrange` and `straighten` are statement heads only when no `=`
follows the first token, so a node named `place` stays legal (`place = int { … }`).

#### What each piece specifies

| Piece | Axis | Meaning |
|---|---|---|
| `at (x, y)` | both | the subject's top-left corner. Never combined with a clause. |
| `by (dx, dy)` | both | moves existing nodes by this offset from where they stand when the statement runs (after Step 2 and earlier statements). A list moves rigidly, keeping its internal arrangement; unlike `[…] column`, nothing is re-stacked. Never combined with a clause. |
| `right_of T` | x (+ y) | one gap right of `T`'s right edge. Alone, it also aligns tops with `T` (D9). |
| `left_of T` | x (+ y) | one gap left of `T`'s left edge; alone, also aligns tops |
| `below T` | y (+ x) | one gap under `T`'s bottom edge. Alone, it also aligns lefts with `T`. |
| `above T` | y (+ x) | one gap over `T`'s top edge; alone, also aligns lefts |
| `top_even_with T` / `bottom_even_with T` | y | that edge level with `T`'s |
| `left_even_with T` / `right_even_with T` | x | that edge level with `T`'s |
| `straight_from S` | y | the wire from `S` into the subject runs horizontal (the first such wire in the subject's pin order) |
| `straight_to D` | y | the wire from the subject into `D` runs horizontal |
| `gap: G` | — | replaces the direction's default `normal` gap |
| `dx: n`, `dy: n` | — | shifts the clause's result by a fixed offset; any position near a target is reachable |
| several targets, `a and b` | — | the clause measures against the bounding box of all of them |
| target `all` | — | the extent of every node in the scope (comments included, the subject excluded), measured when the statement runs, the same box `query --layout` reports in its header. A subject placed outside it lands on nothing, so nothing is pushed. `below all` is a new lane under everything, left edge level with the drawing's; `right_of all` a new column right of everything, top level with the drawing's. A node named `all` is written `` `all` ``. |
| `[…] column` / `[…] row` | — | the members, in written order, stacked with lefts aligned (column) or laid side by side with tops aligned (row), with gap `G` (default `normal`). The clauses then position the group's bounding box as one subject. |

**Each axis is stated exactly once.** A lone direction supplies its default
for the other axis. An explicit clause on that axis replaces the default, and
two explicit clauses on one axis are an error.

#### `push`: which way the nodes it lands on go

A placed subject never moves (D6), so when it lands on existing nodes,
**they** are pushed out of the way. That always happens. The optional
`push <dir>` at the end of a statement says which way. The distance is
always computed from the real rendered boxes, so the AI never supplies one.

| | What moves | Primitive |
|---|---|---|
| *(no `push`)* | each overlapped node, up **or** down, whichever clears it more cheaply, and anything it then hits. The code chooses per node: a centre above the subject's goes up, below goes down; when hand-placed nodes are near, the options are simulated and the one displacing fewest of them wins. | `cascade`, `Split` or scored (today's Step 5b) |
| `push down` | every overlapped node **down**, and anything it then hits. Nothing above the subject moves. | `cascade`, `Down` |
| `push up` | the same, **up** | `cascade`, `Up` |
| `push right` | **everything** at or right of the subject's left edge (the line snapped by the window rule) shifts right, rigidly, by the least distance that clears what the subject overlaps. What feeds the subject stays. | `shift_half_plane`, fixed = the subject's upstream closure ∪ pinned nodes |

Typical uses:

- **Into a column:** `place n below h push down`, in a full column `w, h, d`.
  `d` and what is under it move down, and the column reads `w, h, n, d`.
  Without `push`, a member overlapped above the subject's centre would be
  pushed *up*, and the order would be decided by geometry.
- **Into a chain:** `place rot right_of slab push right`, in a full row
  `slab → cut → fill`. `cut`, `fill` and everything right of them move right
  together, and the row stays a row. Without `push`, `cut` would be pushed up
  or down, out of the row.
- **Anywhere:** `place r at (310, 540) push down`. `push` works with every
  kind of statement, absolute ones included.

**Why `right` pushes a whole half-plane and `up`/`down` do not.** The two
axes are not alike (`design_incremental_layout.md` D6). x carries an
invariant, that wires run rightward. Pushing only the overlapped node right
could carry it past a node it feeds and turn that wire backward. The rigid
half-plane shift, with the upstream closure held fixed, provably cannot. y
carries no invariant, so a vertical push moves only what it must.

**Why there is no `push left`.** A leftward move can turn the wires *into*
the moved nodes backward, and there is no wire-safe primitive for it. To make
room before `T`, place after `T`'s predecessor and push right.

`push` is optional, at most once per statement. On `arrange` / `straighten`
it applies to whatever their result lands on.

#### `arrange` and `straighten`

These are the Arrange commands of [Part B](#b2-arrange-commands), with the
same code (`layout/arrange.rs`). `arrange [a, b, …]` runs Sugiyama on the
listed nodes only (`layout_subgraph`) and keeps their bounding box's top-left
corner. `straighten [a, b, …]` moves each listed node vertically, in
topological order, until the wire from its first wired input runs
horizontal. Unlisted nodes never move except to be pushed off the result
(D11). They exist for "tidy this up" requests, which today have only one
answer: the global reflow.

#### When a statement is rejected

Each of these drops the statement (D8). Its subject keeps what would have
happened without it: a new node is placed automatically, an existing one
stays where it is. A `layout.warnings` entry names the line and the problem.

| Problem | Example |
|---|---|
| unknown target, or a target in another scope | `place mv below d` inside a body |
| an axis stated twice | `place hs below slab, straight_from chamfer` |
| an axis left open | `place x left_even_with y`, which says nothing about height |
| a node placed, moved or arranged by two statements (the second is dropped) | `place n …` and `place [n, m] column …` |
| a target that is a new node no earlier statement placed (it has no position yet; reorder the lines, or place it first) | `place a right_of b` before `place b at (…)`, `b` new |
| `at` or `by` together with a clause; `push` twice, or with an unknown direction (`push left`) | |
| `by` on a node this edit creates (it has no position to offset from) | `place n by (0, 40)` where `n` is new; `n` is placed automatically |
| `straight_from S` / `straight_to D` with no wire between them | |
| a body subject clamped to non-negative coordinates (applied, then reported) | |

### Where statements run

Statements extend the per-scope incremental pass (`layout/incremental.rs`,
`layout_scope`). Scopes are still processed inside-out, so a body's
statements run during that body's turn, and a body that grows from them is
repaired in its parent like any other growth.

| Step | What runs | Moves are tagged |
|---|---|---|
| 2 | grown nodes repaired, as today | `grow` |
| **2½** | **all layout statements, in written order.** A `place`: the subject's box is measured (`rendered_node_size`; a group is stacked first), its position resolved against the drawing *as it stands now*, what it lands on pushed (its `push` direction, else the code's), and the subject **pinned**. An `arrange` / `straighten`: run, its result pushed off what it lands on, and the listed nodes pinned. After the last statement, every named target is pinned too (D6; `all` pins nothing). | `place` (an existing subject), `arrange`, `push` |
| 3–5 | the remaining added nodes as blocks, as today, with pinned nodes as anchors and fixed obstacles | `auto` |
| 6 | backward-wire repair, as today, pinned nodes fixed | `auto` |
| 7–8 | comments, as today | `auto` |

Resolving "as it stands now" is what makes relations late-bound. A target
moved by Step 2, or pushed or arranged by an earlier statement, is measured
where it ended up. So `straighten [a, b]` followed by `place c below b` puts
`c` under `b`'s straightened position.

### A3. Hear: the layout report

An edit result (`EditResult`, the JSON `atomcad-cli edit` prints) gains a
`layout` object when there is something to say: an existing node moved, a
finding or warning arose, or the edit contained layout statements. Otherwise
the key is absent, so an edit that leaves the drawing alone says nothing about
layout, and the agent has nothing to relay. A `--dry-run` always includes it.

```json
"layout": {
  "placed": [
    { "node": "n",       "box": [100, 370, 160, 60] },
    { "node": "pitch",   "box": [100, 460, 160, 60] },
    { "node": "r",       "box": [310, 660, 160, 82] },
    { "node": "holes",   "box": [520, 640, 370, 240] },
    { "node": "holes_u", "box": [940, 660, 160, 60] }
  ],
  "auto_placed": [
    { "node": "holes/x", "box": [12, 30, 160, 104] }
  ],
  "moved": [
    { "node": "note", "from": [100, 370], "to": [100, 550], "cause": "push",
      "dir": "down", "why": "line 18: place [n, pitch] column below d push down" },
    { "node": "cut",  "from": [730, 100], "to": [1150, 100], "cause": "auto",
      "why": "keeps wire holes_u -> cut.sub forward" },
    { "node": "fill", "from": [940, 100], "to": [1360, 100], "cause": "auto",
      "why": "same shift as cut" }
  ],
  "findings": [],
  "warnings": []
}
```

| Field | Content |
|---|---|
| `placed` | every node a `place` statement positioned, with its **final box**. The AI learns the real size of what it created (above: how wide the `map` renders) without querying again. |
| `auto_placed` | every new node the incremental rules positioned, with its final box |
| `moved` | every **existing** node whose position changed, with `from` / `to` and the cause of its *first* move (later moves by other causes are folded into `to`, and listed as `also`) |
| `findings` | what this edit **created** and nothing repaired (below) |
| `warnings` | rejected statements: `{ "line", "statement", "problem", "fallback" }` |

Node names are scoped paths (`holes/x`), as everywhere in `EditResult`.

**Causes:**

| `cause` | The node moved because… | Asked for by the AI? |
|---|---|---|
| `place` | the AI `place`d this existing node | yes |
| `arrange` | an `arrange` / `straighten` statement listed it | yes |
| `push` | a placed or arranged node landed on it; `dir` says which way it went | yes if the statement said `push <dir>`, otherwise only the landing was |
| `grow` | a node this edit enlarged pushed it (Step 2) | no, a mechanical consequence |
| `auto` | the safety net: room for a node the AI did not place, or a backward-wire repair | no, the plan missed something |

**Findings** are reported in reladraw's style, one line each, saying what,
where and how much, so the fix is plain without a picture:

| `kind` | Reported when |
|---|---|
| `overlap` | two boxes overlap after the edit and did not before. Can only involve a pinned node, since everything else is pushed. Example: `"pitch overlaps r (both placed) by 160×20 px"` |
| `backward_wire` | a wire this edit created runs right-to-left, and Step 6 could not repair it (a cycle, or both ends pinned) |

Overlaps and backward wires that existed before the edit are baseline, and
never reported here: both hand-drawn corpora contain overlaps their authors
are content with. `atomcad-cli check-layout` lists every finding in the
network, each marked `new` or `baseline`.

#### `--dry-run`

```
atomcad-cli edit --dry-run [--replace] --code="…"
```

Runs the entire edit (parse, apply, validate, layout pass) and returns the
same result with `"dry_run": true`. Nothing changes: no undo entry, no
dirty flag, no AI-log record, no scene refresh. The AI previews an edit that
pushes or works in a crowded area, reads `moved`, then commits as is
(telling the user what will move) or revises.

Implementation: `ai_text_edit` already builds a `TextEditNetworkCommand`
holding the pre-edit snapshot and the call-site wiring, because an edit can
remove parameters and so rewire callers in other networks. A dry run applies
the edit, captures the result, runs that command's undo in place, and drops
the command. That is exactly the restore Ctrl+Z performs, so a dry run cannot
leave anything behind that an undo would not also clear.

---

## Part B — Human interface

### B1. Snapping guides while dragging

The node editor has no snapping today, and the corpus shows people aligning
by eye: over half the nodes sit within 8 px of another node's x, but under
5 % share one exactly. While a node or selection is dragged, the **grabbed
node** snaps per axis, and a dashed guide shows why. The guides exist only
during the drag.

| Snap target | Axis | Guide |
|---|---|---|
| **straight wire**: an input pin level with its source's output pin, or an output pin level with a destination's input pin | y | the wire is highlighted |
| left = left, right = right (a column) | x | vertical dashed line through both boxes |
| top = top, bottom = bottom (a row) | y | horizontal dashed line |
| `normal` gap right of / below a neighbour | x / y | bracket labelled `50` / `30` |
| **equal spacing**: the gap of the neighbouring pair in the row or column being joined | x / y | brackets on both gaps, labelled `= 42` |

- **Candidates:** nodes of the same scope (comments included) with a box in
  the viewport, outside the selection. Body nodes snap to body siblings only.
- **Threshold:** 6 *screen* px, so the feel is the same at every zoom. If
  several are in range: straight wire, then edge, then gap, then equal
  spacing; nearest first within each.
- **Off switches:** hold **Alt** for one drag; Preferences › Layout ›
  "Snap while dragging" (default on).
- **Insert-on-wire wins.** While a dragged node highlights a wire it would be
  spliced into (`doc/design_insert_node_on_wire.md`), snapping is suspended,
  so the two gestures never fight.
- **Multi-selection:** only the grabbed node snaps, and the rest of the
  selection follows by the same delta.
- **Cost:** Dart-only. Drag ticks never call `notifyListeners` (an existing
  rule). At drag start, sorted candidate edge lists and pin y's are built from
  the `LayoutCache`. A tick is a binary search plus an overlay repaint. The
  commit is the existing `end_move_nodes`, so undo and `hand_moved` are
  unchanged.

### B2. Arrange commands

The node context menu gains **Arrange ▸**, enabled for ≥ 2 selected nodes in
one scope. Each command is one undo step (`MoveNodesCommand`) and sets
`hand_moved` on the nodes it moves (D12).

| Command | Effect | Same as the AI's |
|---|---|---|
| Align left / right / top / bottom | to the selection's extreme edge | — |
| Stack as column | ordered by current y, lefts on the leftmost, `normal` gap | `[…] column` |
| Stack as row | ordered by current x, tops on the topmost, `normal` gap | `[…] row` |
| Distribute vertically / horizontally | extremes kept, gaps made equal | — |
| Straighten wires | as `straighten` | `straighten` |
| Arrange selection | as `arrange` | `arrange` |

Only selected nodes move, and nothing is pushed (D11).

---

## Implementation map

| Area | Where | What |
|---|---|---|
| Shared arrange ops | `layout/arrange.rs` (new) | `arrange(scope, sizes, ids, ArrangeOp) -> Vec<Move>`: align, stack, distribute, straighten, sugiyama-subset. Used by B2 and A2. |
| Read-out | `layout/readout.rs` (new) | `layout_readout(network, registry, scope, filter) -> String`, including group detection |
| Statements | `text_format/parser.rs`, `network_editor.rs` | `Statement::Place` / `Arrange` / `Straighten` parsed and carried by name path to the layout pass. They are not applied to the network. |
| Resolution | `layout/placement.rs` (new) | written-order execution, clause resolution, `push` dispatch to `motion.rs`, pinning (subjects per statement, targets at the end) |
| Pass | `layout/incremental.rs` | Step 2½; a pinned set passed into `fit`, the cascade and Step 6; every primitive's moves recorded with a cause in a `MoveLog` |
| Ownership tag | GUI creation paths in `structure_designer.rs` (add node, paste/duplicate, insert-on-wire, panel drop, promote/factor/inline) | set `hand_moved` on the created nodes (D12) |
| Report | `ai_edit_log.rs`, `text_format::EditResult` | `MovedNode` gains `cause` / `why`; `EditResult` gains `layout: LayoutReport`; the AI History panel's Layout tab shows the causes |
| Dry run | `ai_text_edit.rs` | apply → capture → `TextEditNetworkCommand::undo` → drop |
| API | `rust/src/api/structure_designer/ai_assistant_api.rs` | `ai_edit_network(code, replace, dry_run)`, `ai_query_layout(scope, nodes, box)`, `ai_check_layout()`; `arrange_selected_nodes(op)` for B2. FRB regen. |
| HTTP + CLI | `lib/ai_assistant/http_server.dart`, `bin/atomcad_cli.dart` | `/layout`, `/check-layout`, `dry_run` on `/edit`; the matching CLI flags |
| GUI | `lib/structure_designer/node_network/` | `snap_guides.dart` (new: candidates and resolution), guide overlay in the painter, drag hook in `node_widget.dart`, Arrange submenu |
| Skill | `.claude/skills/atomcad/` | the guidance below |

### What the skill teaches

The skill teaches the vocabulary, **not when to use it**. It does not say
when to query the layout, when to write statements, when to preview, or
what to tell the user about moves. Until the [Gate](#gate) replay shows how
agents actually behave, that is left to the agent's judgement or to the
principle the user states. Usage guidance is written from that evidence
later, not guessed now.

- **Ask, Tell, Hear.** What `query --layout` returns, the statement forms
  and their meaning, and what each `layout` report field and cause means.
- **Anchor to the drawing or absolutely, attach relatively.** Anchor the first
  node of a new group with `below all` / `right_of all` (free by
  construction, no query needed), or with `at (x, y)` at a specific free spot
  you have confirmed. Attach everything else
  with relations, always so when a position depends on the size of a node
  this edit creates, on pin positions, or on a node this edit may move. Never
  compute a position from a new node's assumed size. 160 px wide holds only
  for ordinary nodes.
- **Nudge with `by`.** To move existing nodes a little, or a cluster as a
  whole, write `place x by (0, 40)` or `place [a, b, c] by (210, 0)`. Don't
  compute an `at` from a queried box: the edit may move the node before your
  statement runs.
- **Choose which way things make room.** Use `push down` to put something
  *into* a column and `push right` to put it into a chain or row. Without
  `push`, what you land on goes up or down, wherever the code finds cheapest.
  There is no `push left`: to make room before `T`, place after its
  predecessor and push right.
- **Preview.** `--dry-run` returns the full result, `layout` included, and
  changes nothing.
- **Respect ownership.** Never place, arrange or straighten a node tagged
  `hand` unless asked (D3). The tag marks the user's nodes, whatever this
  conversation remembers.

---

## Example

A network the user arranged: a parameter column `w, h, d` feeding
`dims → slab → cut → fill` along the top, a note under the column, and
`hole_r → hole` feeding `cut.sub` along the bottom.

*"Replace the single hole with a row of `n` holes, `pitch` cells apart. Put
n and pitch with the other size parameters, and keep the hole machinery as
its own lane at the bottom."*

**Ask.** `query --layout --nodes d,note,hole_r` shows the column `w, h, d`,
the note directly under it, and `hole_r` at `x = 310`. No `--box` query is
needed: the new lane is anchored to the drawing.

**Tell**, previewed with `--dry-run`:

```
n     = int { value: 3 }
pitch = int { value: 6 }
r     = range { start: 0, step: 1, count: n }
holes = map { xs: r, input_type: Int, output_type: Geometry, body { … } }
holes_u = union { shapes: holes }
cut     = diff { base: slab, sub: holes_u }

place [n, pitch] column below d push down      # into the column; the note makes way
place r below all, left_even_with hole_r        # a lane under everything, lined up with the old one
place holes   right_of r,     straight_from r   # attach: holes' width is unknown
place holes_u right_of holes, straight_from holes
```

**Hear.** The report is the one shown in [A3](#a3-hear-the-layout-report).
The `push down` of the note is the one the AI chose. `below all` is measured
after that push, so `r` lands under the note's new position (bottom 630) at
`y = 660`, not under the old extent: an `at` computed from the query would
have been stale. The `auto` shift of
`cut` and `fill` it did not foresee: the `map` renders 370 px wide, so the
safety net shifted the top chain to keep `holes_u → cut.sub` forward. The
AI commits and tells the user both. The user then drags the note beside `h`
(a guide snaps it level with `h`) and straightens the chain from the Arrange
menu. The next query reports `column w, h, d, n, pitch`, and the next edit's
`place z0 below pitch` moves nothing.

Without this design, `n` would have joined the new block wherever that
landed, `pitch` (referenced only from inside the body) would likely have
gone to the drawing's bottom-left, the lane would have been aligned with its
wire into `cut` in the crowded top row, and the user's nodes would have moved
with no account of why.

---

## Phases

Parts A and B are independent and can run in parallel. Every phase updates
the reference guide (`doc/reference_guide/node_networks.md` /
`ui.md` for B, the text format pages and the skill's
`references/text-format.md` for A) and `layout/AGENTS.md` where it adds a
module.

### Phase 1: Snapping guides (B1)

`snap_guides.dart`, the overlay, the drag hook, the preference, and
suspension during insert-on-wire.

*Tests:* Dart unit tests of snap resolution (candidates, threshold in screen
px at two zoom levels, the priority order, Alt bypass, multi-selection,
body scoping); a widget test that a drag tick does not notify listeners.
The snapping feel itself is a manual walkthrough step.

### Phase 2: Shared arrange ops + Arrange menu (B2)

`layout/arrange.rs`, `arrange_selected_nodes`, and the submenu.

*Tests:* Rust tests per op (exact positions on fixtures; only listed nodes
move; `straighten` order with chained nodes; `arrange` keeps the top-left);
undo restores positions and `hand_moved` in one step.

### Phase 3: Hear (A3)

`MoveLog` and causes threaded through `layout_incremental` (only `grow` and
`auto` exist yet), `EditResult.layout`, findings, `check-layout`, and
`--dry-run`.

*Tests:* the existing `layout_corpus_test` additionally asserts that the
report's `moved` set equals the actual position diff for every generated
edit, and that every move carries a cause, and that `layout` is absent
exactly when no existing node moved, no finding or warning arose, and the
edit had no layout statements. Findings fixtures: a new overlap
is reported, a pre-existing one is not. Dry run: the network, undo stack,
dirty flag and AI log are bit-identical afterwards, including when the edit
removes a parameter that other networks call.

### Phase 4: Ask (A1)

`layout/readout.rs`, `ai_query_layout`, `/layout`, and the CLI flags.
`hand_moved` set on every GUI creation path (D12), inside each path's
existing undo command.

*Tests:* every GUI creation path listed in D12 sets `hand_moved`, and its undo
removes the node along with the flag; an AI edit's new nodes never have it;
snapshot tests of the read-out on demolib networks (every filter,
a body scope); group detection on hand-drawn columns, including a column cut
by the box edge; an unknown name in `--nodes`.

### Gate

Replay 5–10 real co-edit tasks (the 2026-09-03 session plan has several).
Write the statements by hand, and compare the result against the automatic
placement Phases 3–4 can already measure. Build Phase 5 only if the placements
are clearly better.

The replay is also where usage guidance comes from. Note whether agents
query too often or too rarely, narrate moves nobody cares about, or miss moves
that matter, and write the skill's "when" guidance from that.

### Phase 5: Tell (A2)

Parser, `layout/placement.rs`, Step 2½, `push`, pinning, warnings, and
the skill guidance.

*Tests:* one fixture per clause and per `push` row of the tables above
(including a `push right` whose subject overlaps only part of a node);
`by` on a single node and on a list (the list's relative positions are
unchanged), and `by` measured after a Step 2 growth moved the node;
every rejection in the warnings table falls back as specified; late binding
(a target moved by Step 2, or pushed by an earlier statement, is measured where it
ended); `all` (measured after an earlier statement's push, excludes an
existing subject, pushes nothing, pins nothing, scoped to a body inside
one, and `` `all` `` still names a node); written order (`straighten [a, b]` then `place c below b` puts `c`
under the straightened `b`; swapping the two lines gives the other result);
pinning (no later step moves a placed node, and an automatic placement that
would cascade a `place` target pushes something else instead); body statements
(clamping, growth reaches the parent); `--replace` with statements. The
corpus test gains a generated edit kind that places its new nodes `below`
an existing node, so the oracle runs over pushes off pinned nodes.

---

## Testing method

The layout oracle (`tests/structure_designer/layout_oracle.rs`) remains the
judge of every drawing a test produces, and the corpus test remains the only
place real hand-drawn geometry is exercised. Every phase that moves nodes
runs it. The report is tested against the diff it describes, never against
itself: whatever the report says moved must be exactly what moved.
Interaction feel (snapping, Arrange) is verified by the maintainer's manual
walkthrough, as for other thin editor UI.

## Not in this design

- **Persisted relations or groups** that move together when dragged
  (reladraw-style, or mechadense's star-linked grids). That is the
  "layout is data" destination (research F11). Revisit only if the Arrange
  commands show that people want alignments to *stay*.
- **Standalone room-making** (`open_column` / `open_row` without a subject).
  `push` covers room made *for* something, and that is nearly every case.
- **Per-axis absolute positions** (`x_at 100, below h`). Use `dx` / `dy` on a
  relation.
- **Snap-to-grid.** No hand-drawn file uses a grid.
