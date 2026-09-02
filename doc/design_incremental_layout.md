# Design: incremental layout for AI/human co-editing

**Status:** second draft for review, revised after `doc/design_hof_body_text_format.md`
landed. Research background: `doc/research_intent_preserving_layout.md`.

**Problem.** Every AI text edit re-runs Sugiyama over the whole network
(`auto_layout_after_edit` defaults to `true`), so a human's arrangement is
destroyed on every edit. Bodies are worse: there is no layout pass inside a HOF
body at all, and a body edit that grows the body grows the owning HOF in its
parent, overlapping whatever sits next to it.

**Approach.** Characterize the edit as a delta per scope, lay out only the added
nodes, fit them into the existing drawing, and repair locally. Existing nodes
move only when forced, and then by a rigid translation. Scopes are processed
inside-out so a body settles before its HOF's new footprint is repaired in the
parent. A full Sugiyama reflow stays available as an explicit user command.

---

## What the hand-drawn corpora look like

Measured over the maintainer's working file (`from_mechadense.cnnd`) and the
tidier **demolib** library (`demolib/baselib_with_demos.cnnd`); top-level
networks with at least two nodes, node boxes estimated at 160×83:

| Property | from_mechadense | demolib |
|---|---|---|
| Networks / nodes / wires | 82 / 2,300 / 2,058 | 65 / 901 / 977 |
| Nodes on a canonical Sugiyama column (`100 + col·210`) | 0.2% | 0.3% |
| Nodes sharing an **exact** x with another node | 2.6% | 4.7% |
| Nodes within **8 px** of another node's x | **51.8%** | **54.1%** |
| Wires pointing rightward | 91.4% | **99.2%** |
| Backward wires | 177, in 39 networks | 8, in 2 networks |
| Overlapping node pairs already present | 152 | 21 |

Three facts shape the design:

1. **Zero nodes are where the algorithm would put them.** A full reflow
   rewrites every position. There is no "mostly correct" case to exploit.
2. **There is no grid, but there is loose alignment everywhere.** Half the
   nodes sit within a few pixels of another node's x. A shift line that cuts
   through such a column, moving some members and not others, destroys
   structure the human put there. The [window rule](#the-window-rule) exists
   for this.
3. **Rightward flow is a real invariant**, nearly absolute in polished
   networks, but backward wires and overlaps do exist in drawings their authors
   are content with. Hence the rule the design follows throughout:

> **Repair only what this edit broke.** The pre-edit drawing is the baseline,
> however irregular.

*(Bodies were not measured: the working corpus has 48 HOF bodies holding 141
nodes, and nothing is known about how they are arranged.)*

---

## Design decisions

**D1 — The unit of work is a diff.** Layout consumes one `EditDelta` per scope
(added / grown / removed nodes, added / removed wires), computed by comparing a
pre-edit snapshot against the post-edit network.

**D2 — Added nodes are laid out as a group, by Sugiyama, in isolation.** The
result is a **block** with local coordinates and a bounding box, placed as a
rigid unit.

**D3 — Existing nodes are never re-derived, only translated**, and only when
something forces them to. A rigid translation preserves relative order,
alignment, spacing and whitespace within the translated set exactly.

**D4 — Deletion leaves holes.** No compaction: it would move untouched nodes.
The same one scope down: a body that shrinks leaves its HOF's footprint where
it was (bounded below by the stored body size); the parent never moves inward.

**D5 — `Node.hand_moved: bool`**, set on drag in any scope, persisted,
undoable, `#[serde(default)]`. A tiebreaker, never a hard constraint.

**D6 — Exactly two motion primitives, one per axis.** The **horizontal
half-plane shift** (rigid, global) and the **vertical cascade** (minimal,
local). Placing a block, growing a node and repairing a backward wire all reduce
to these two. The landed quadrant shift (`node_inlining::make_space_for_inline`)
is **retired on both the AI and the GUI paths**. See
[The two motion primitives](#the-two-motion-primitives).

**D7 — Determinism.** Every iteration over nodes is by ascending id, over scopes
by depth then name path. One existing hole must be closed first:
`sugiyama::find_connected_components` seeds its BFS from `HashMap` order, so two
equal-size disconnected components stack in per-process-random order. Sort the
seed ids.

**D8 — No global post-pass.** The output differs from the input only where the
delta forced it to.

**D9 — Comment nodes are ordinary nodes on this path.** Obstacles at their real
`CommentData` size, pushed by the cascade, shifted by the half-plane, never
re-placed by rule. `place_comments` is not called and not modified; it keeps
serving the full reflow. Since #427 the leader line carries the association, so
a comment's position no longer encodes it and drift is cosmetic.

**D10 — A drifted anchored comment is pulled back to its exact original offset
from its anchor, if that spot is free.** All-or-nothing, no constant.

**D11 — Growth is two-dimensional and `grow_rect` is its one operation.** A
grown node grows right and down from a fixed top-left; the width delta is a
half-plane shift, the height delta a downward cascade. Every path that grows a
node uses it.

**D12 — Scopes are processed inside-out.** A parent's node sizes are not known
until its children's bodies have settled. When a body settles, its HOF's
footprint is re-measured and, if it grew, the HOF joins the parent delta's
`grown` set.

**D13 — `grown` means "footprint grew, measured"**, by comparing pre- and
post-edit rendered footprint from the one size function. That subsumes added
pins, body growth, and the collapse flip when `f:` is unwired on an Auto-mode
HOF; no per-trigger detection.

**D14 — The identity snapshot carries layout state, not just position.** The
text format has no `body_width`, `body_height` or `collapse_mode`, so a
`--replace` round-trip today resets every HOF to the 320×180 default and to
`Auto`, re-expanding a user-collapsed HOF. The snapshot records, per name path,
**position, footprint, `body_width`, `body_height`, `collapse_mode`,
`hand_moved`**, and the editor re-applies all of them on a name match. (This
extends `design_hof_body_text_format.md` D8, which carries position only.)

**D15 — A half-plane shift cuts inside a window and snaps left into whitespace
within it.** An empty window means no shift. See [The window rule](#the-window-rule).

---

## The edit surface

Every AI edit funnels through `StructureDesigner::ai_text_edit`
(`crates/atomcad-structure-designer/src/ai_text_edit.rs`): it takes the identity
snapshot, runs the scope-aware `text_edit_network`, validates, and today calls
`layout::layout_network` once on the root scope, then pushes a whole-network
undo snapshot. One call is one multi-statement script, so **one call = one
transaction = one layout pass**; the incremental pass replaces that single
`layout_network` call.

| Mode | Behaviour |
|---|---|
| Incremental merge *(default)* | Statements merge in. Untouched nodes keep id and position. |
| `--replace` | The network is cleared and rebuilt; nodes are matched to their old identity by name path (D14). |

Wires are assigned, not accumulated (`1b93cab8`): mentioning a property
assigns that pin's whole inbound wire set, so wire removal is a routine edit. A
`body { … }` block assigns a whole body; `m1/x = …`, `output m1/x`,
`delete m1/x` address one node inside one. One script can touch the root scope
and several bodies, which is why the delta is per scope.

---

## The edit delta

```rust
pub struct EditDelta {
    pub added: Vec<u64>,
    /// Present in both; rendered footprint grew in either axis (D13).
    pub grown: Vec<(u64, DVec2 /* old */, DVec2 /* new */)>,
    pub removed: Vec<u64>,
    pub added_wires: Vec<WireKey>,   // keyed like `WireAnchor` (#427)
    pub removed_wires: Vec<WireKey>, // layout-inert, carried for faithfulness
}
```

Computed by `diff_scope(snapshot, scope, network)`, where the snapshot is the
D14 map `(name path) → {position, footprint, body_width, body_height,
collapse_mode, hand_moved}` taken by the editor's Pass 0 walk
(`text_format::snapshot_node_positions`, extended). Diffing is **by name path**,
never by id: `--replace` mints fresh ids, so `[m1_id]` before and after are
different numbers naming the same body. Ids are resolved after the match.

- A node whose value changed but whose footprint did not is not a layout
  event. A node that shrank is excluded (D4).
- **Wire removal triggers no repair.** It cannot create an overlap or a
  backward wire, and a node left with no wires stays put. The one indirect
  effect, an Auto-mode HOF expanding when its `f` wire goes, is a footprint
  change and is caught by D13.
- **Cross-scope wires** — a zone input `$element`, a capture `^name`, an outer
  zone input `^$element`, and the body's `output` — appear in the delta and
  serve as anchors (Step 4) but are never repaired (Step 6).
- **Name matching is total**: every node has a `custom_name` from creation. A
  renamed node, or a legacy node without one, simply fails to match and is
  `added`, which is always safe.

---

## Algorithm

Eight steps per scope, in order. Grown nodes are repaired **before** blocks are
placed, so blocks see settled obstacles.

### Step 1 — Remove

Delete the `removed` nodes and their wires. Nothing else (D4).

### Step 2 — Repair grown nodes

For each `(id, old, new)` in `grown`, ascending id, re-reading positions between
entries: `grow_rect(scope, id, old, new)`. The grown node itself never moves.

### Step 3 — Lay out the added nodes as blocks

Restrict the graph to `added`, split into connected components over wires
between added nodes, and run Sugiyama on each component alone:

```rust
pub fn layout_subgraph(network, registry, ids: &HashSet<u64>, algorithm) -> HashMap<u64, DVec2>;
```

Implemented by threading an id filter through `compute_node_depths` /
`group_by_depth` / the barycenter passes; `layout_network` becomes
`layout_subgraph` over all ids. Two prerequisites from
[one size function](#prerequisite-one-size-function): node sizes come from the
unified function, and **column width is per layer** (widest node plus gap), not
the fixed `COLUMN_WIDTH`. An added HOF's size is its settled footprint (D12).

Each block yields local positions and a bounding box `(W, H)`. Blocks are
placed in order of `(min anchor x, min node id)`; each placed block joins the
obstacle set.

### Step 4 — Choose each block's target position

**Anchors:** `U` = kept nodes with a wire into the block, `D` = kept nodes fed by
it. Inside a body the scope's own edges are anchors too, since `$element` and
`output` are the dominant wires of a 2.9-node body and would otherwise leave
every block anchorless:

| Wire | Synthesized anchor |
|---|---|
| `$name` (any depth), `^name` | zero-width box at the body's left edge, `x = 0`, at the pin's rendered y (`FIRST_PIN_OFFSET + index · PER_PARAM_HEIGHT`; body top for captures) |
| the body's `output` | zero-width box at the body's right edge, `x = body_width`, at the output pin's y |

**Horizontal**, with `GAP = DEFAULT_HORIZONTAL_GAP`:

```
x_min = max over u in U of (u.x + width(u) + GAP)
x_max = min over d in D of (d.x - GAP - W)
```

| Case | Placement |
|---|---|
| `U`, `D` both empty | right of the drawing's bbox + `GAP` |
| `U` empty | `x = x_max` |
| `D` empty | `x = x_min` |
| `x_min ≤ x_max` | `x = x_min` |
| `x_min > x_max` | **no room**: `shift_half_plane(T, x_min − x_max)` with `T` from the [window rule](#the-window-rule), window `(max u.x, min d.x]`; then `x = x_min`. Empty window ⇒ no shift, `x = x_min`, Step 5 handles overlap, the wire to that consumer stays backward. |

**Vertical:** align by connections, `y_offset = mean over external wires of
(anchor.y_center − internal.y_center_local)`. No anchors: below the drawing's
bbox. Empty body: at the body's left padding, level with the first zone-input
pin.

**Body coordinates are non-negative**: content extent is measured from the
origin (`rendered_body_size`, Flutter `_computeBodySize`), so candidates are
clamped to `≥ 0`.

### Step 5 — Fit the block in

`R` = the target rect inflated by `GAP`.

**(5a) Slide.** Search a free `y` near the target, alternating down and up in
`VERTICAL_GAP` steps within `SLIDE_WINDOW` (suggested: two node heights).
Nothing moves. **Inside a body the slide is slack-first**: growth past the
stored body size cascades into the parent, so candidates inside the stored size
(minus padding) are searched first over the whole body height, and the ordinary
window is used only when none is free.

**(5b) Push.** Otherwise place at the target `y` and run the
[vertical cascade](#the-vertical-cascade) over `R`, direction per node by which
side of `R`'s centre it lies on.

Simulated on both corpora (`scripts/layout_cascade_sim.py`: a node-sized rect
dropped onto each node's position with that node removed from the obstacles):

| | from_mechadense | demolib |
|---|---|---|
| Cascades pushing nothing / ≤ 1 / ≤ 5 nodes | 50% / 77% / 98% | 65% / 86% / 99% |
| Max nodes pushed | 11 | 10 |
| x-extent ÷ `R` width, p90 / max | 1.74× / 5.04× | 1.01× / 2.48× |

`SLIDE_WINDOW` is the design's one tuning constant.

### Step 6 — Repair new backward wires

For each wire in `added_wires` with both endpoints kept and in this scope: if
`source.x + width(source) + GAP > dest.x`, `shift_half_plane(T, deficit)` with
window `(source.x, dest.x]`. Wires already backward before the edit are left
alone; cross-scope wires are skipped.

### Step 7 — New comment nodes

Only a comment the edit *created* needs a rule. Anchored (`on:`): first free
position among the four sides of its anchor, via the landed
`anchor_placement_box` / `surrounding_candidates`, then Step 5 as fallback.
Unanchored: an anchorless block (Step 4).

### Step 8 — Restore drifted comments

For each anchored comment, ascending id: if its distance to its anchor grew
during this edit, compute `target = anchor_after + (comment_before −
anchor_before)`; if `target` is collision-free, move it there, else leave it.
This never moves a comment away from its anchor, never creates an overlap, and
has no constant. The motivating case is a comment just left of a shift line
whose anchor moved right.

---

## The two motion primitives

Both preserve order along their axis and neither can create an overlap.

### `shift_half_plane(x_threshold, dx)`

Translate every node with `x ≥ x_threshold` right by `dx`. Rigid and global:
alignment and gaps within the moved and the unmoved set are preserved exactly,
and **a forward wire can never turn backward** (wires crossing the line only get
longer). The drawing gets wider, which is the honest cost of inserting
something. Used by Step 2 (via `grow_rect`), Step 4 and Step 6.

### The window rule

A shift is safe at any threshold but not equally good at any: dropped at an
arbitrary x it cuts through a loose column (half the nodes are within 8 px of
another node's x). Each shift site has nodes that must move and nodes that
must not, and they bound the line:

| Site | Must not move | Must move | Window for `T` |
|---|---|---|---|
| Step 4, no room | every upstream anchor | every downstream anchor | `(max u.x, min d.x]` |
| Step 2, `grow_rect` width | the grown node | everything right of its old right edge | `(node.x, old right edge]` |
| Step 6, backward wire | the source | the destination | `(source.x, dest.x]` |

1. Start at the site's default (`x_min`, the old right edge, `dest.x`),
   clamped to the window's right end. `x_min` can lie right of `min d.x` when a
   consumer's left edge overlaps an input horizontally; clamping is what makes
   that consumer move.
2. Snap **left, never right**: to the nearest x where no node's left edge is
   within 8 px and no node box straddles the line. Left keeps every required
   node moving; right would not. Keep the default if no gap exists within one
   node width.
3. Never leave the window: at or below an upstream anchor, the anchor moves
   and `x_min` with it, and no room is created.
4. Empty window (a consumer at or left of an input's left edge; a destination
   at or left of its source) ⇒ no shift.

Why the default is the left end of the useful range rather than `min d.x`: the
nodes in the strip between `x_min` and the consumers are inside the column that
is widening; shifting them right with the consumers is the "insert a column" a
human would do, while leaving them to the cascade splits them from their row.

### The vertical cascade

```
cascade(R, dir_of):
    W ← nodes overlapping R in both axes, ordered by |y_center − R.y_center|
    dir(n) ← dir_of(n) for n in W                 # up / down
    while W not empty:
        n ← pop W
        push n by the minimum along dir(n) to clear its blocker
            (R on the first round, else the node that enqueued it)
        for every node m in the WHOLE network with x-overlap(n, m),
            y-overlap(n, m) and m further along dir(n):
                dir(m) ← dir(n); push m to clear n; enqueue m
```

A vertical Force-Scan: only colliding nodes move, by the minimum, and the push
propagates only through further collisions. It must test against every node,
not a band: a pushed node can land on a node whose x-interval overlaps *its*
but not `R`'s. It terminates (every enqueued node is strictly further along
`dir`, nothing moves back) and preserves vertical order by construction. What
stops it is vertical whitespace, which the simulation says arrives almost
immediately. No cap is imposed; Phase 4 asserts a generous bound in tests.

### `grow_rect`

```rust
/// node_id's footprint grew from `old` to `new`, top-left fixed. Returns the moves.
fn grow_rect(network, registry, node_id, old: DVec2, new: DVec2) -> Vec<(u64, DVec2, DVec2)> {
    let anchor = node.position;
    let (dw, dh) = (max(0, new.x - old.x), max(0, new.y - old.y));
    if dw > 0 { shift_half_plane(snap(anchor.x + old.x, window = (anchor.x, anchor.x + old.x]), dw); }
    if dh > 0 { cascade(R = (anchor, new) inflated by GAP, dir_of = |_| down,
                        excluding node_id and any node that already overlapped (anchor, old)); }
    diff positions
}
```

Horizontal first so the cascade sees post-shift positions. Pre-existing overlaps
with the old rect are left alone, which is also what makes forcing `down`
correct: after the shift every *new* collision lies below the old bottom edge.

### Why one primitive per axis

The quadrant shift is two half-plane shifts (x at the old right edge, y at the
old bottom edge) plus a size-blind corner heuristic for the band in between. So
the real choice is half-plane or cascade, per axis, and the drawing decides
each differently:

- **x carries a directional invariant, y none.** A horizontal cascade can flip
  a forward wire backward (push B right, its consumer C in an uncollided row
  stays); the half-plane shift provably cannot. Vertically there is nothing to
  protect, so the minimal operation is free to be minimal.
- **Width growth moves the output pin, height growth moves no pin.** Shifting
  everything downstream by exactly `dw` keeps every downstream wire the length
  the human chose; nodes below have only a collision reason to move.
- **A horizontal cascade would not be local anyway**: with nearly one distinct
  x per node the x-overlap graph spans the network, so it would push most of
  the right-hand side raggedly. A vertical half-plane shift would move every
  pipeline stacked below for no benefit.

The quadrant shift's vertical half moves non-colliding nodes (violating D3),
and its diagonal split can itself create an overlap when `dw` is large and `dh`
small. **Retirement:** its three call sites in `structure_designer.rs`
(`reflow_for_footprint_change`, `inline_custom_node`,
`convert_instance_to_closure`) become `grow_rect` calls with the same
`(anchor, old, new)` arguments; the scope walk, `capture_footprint_chain` and
the `ScopedMoves` / `CompositeCommand` undo bundling of
`design_reflow_on_footprint_change.md` are untouched.

---

## Scopes: inside-out

A body is a full `NodeNetwork` with its own coordinates, so Steps 1–8 run on it
unchanged. The driver orders the scopes and feeds a body's outcome to its
parent:

```
snapshot ← Pass 0 walk before the edit (D14), every node at every depth
apply the edit; validate
scopes ← every scope in the post-edit network, deepest first, ties by name path
grown  ← ∅                                     # (parent scope, hof id, old, new)

for scope in scopes:
    delta ← diff_scope(snapshot, scope)
    delta.grown ∪= grown entries whose parent scope is this one
    run Steps 1–8 on scope
    if scope is a body owned by hof H in parent P:
        new ← rendered footprint of H            # settled: the body just laid out
        old ← snapshot[H].footprint              # absent ⇒ H is new ⇒ already in P.added
        if old exists and new exceeds old in either axis: grown ∪= (P, H, old, new)
```

- The parent's `grown` set is completed inside the loop, after its children
  settle; the footprint is the recursive `rendered_body_size` rule
  (`max(stored, content + padding)`), correct only when called after the inner
  scopes are done.
- A new HOF is `added` in its parent at its settled size; its body nodes are
  `added` in the body scope and laid out first against synthesized anchors.
- Growth at depth ≥ 2 cascades one level per iteration; no pre-captured size
  chain is needed, the snapshot holds every old footprint.
- Until Phase 5, `calculate_new_node_position` is the only placement a body
  node gets. Afterwards its output is a throwaway overwritten by Step 3.

---

## Uses of `hand_moved`

A tiebreaker in three places, never a hard constraint:

1. **Cascade direction (5b):** when both directions clear the block, push the
   one displacing fewer hand-moved nodes; then fewer nodes; then smaller total
   displacement; then up.
2. **Slide vs. push (5a→5b):** extend `SLIDE_WINDOW` when every obstacle in the
   band is hand-moved.
3. **Explicit full reflow:** a "respect manually placed nodes" option treats
   them as fixed. Off by default.

Set in the drag handler in whatever scope the node lives, persisted in `.cnnd`,
carried through copy/paste and the name match (D14), and recorded inside the
existing move command rather than as its own undo entry.

---

## Prerequisite: one size function

Four size functions exist and no two agree:

| Function | Comments | Expanded HOF bodies |
|---|---|---|
| `layout/common.rs::node_size` (Sugiyama, `place_comments`) | real dimensions | ignored: 160×83 |
| `node_inlining::estimate_node_size_in_network` (inlining, GUI reflow) | pin estimate | correct, recursive, `max(stored, content + padding)` |
| `text_format/auto_layout::get_node_size` (creation-time placer) | by type name | by type name |
| Flutter `scope_resolver.dart::_computeBodySize` | ground truth | ground truth |

Unify into one `rendered_node_size(node, registry)` — the inlining rule for
bodies, the layout rule for comments — and route all callers through it, with
a fixture pinning its output against the Flutter rule for a comment, an
expanded HOF, a collapsed HOF, a closure and a nested HOF. This is a **Phase 1**
prerequisite: D13's `grown` is a footprint comparison. Two consequences for the
full reflow, also Phase 1: per-layer column width, and body-aware obstacles in
`place_comments`. The stored body size is a floor; layout never writes it.

---

## Interaction with other subsystems

- **HOF bodies in the text format (landed).** Built the per-scope name match
  and the position snapshot this design extends (D14). A body edit yields a
  body-scope delta shaped like a top-level one, so the only body-specific
  machinery is the driver above, the synthesized anchors (Step 4) and the
  slack-first slide (5a).
- **Reflow on footprint growth (landed).** Its undo half stands; its spatial
  half (the quadrant shift) is superseded by `grow_rect`.
- **Full reflow.** Stays Sugiyama, user-invoked, but becomes body-aware in
  Phase 5: per-layer column widths and the same inside-out recursion.
  Otherwise "the reflow is the cure" (D4) is false for any network with an
  expanded HOF.
- **AI edit history.** `ai_edit_log::LayoutPath::Incremental` is reserved and
  Phase 5 produces it; the `moved` list is name-path keyed and already covers
  bodies.
- **Undo.** The AI edit is one whole-network snapshot command
  (`TextEditNetworkCommand`) carrying every zone, taken after layout; no move
  commands are needed on the AI path. The GUI reflow path keeps its
  `ScopedMoves` bundling.
- **Preferences.** The incremental pass always runs (it is repair, not
  layout); the full reflow is only ever user-invoked. See open question 1.

---

## Phases

### Phase 1 — Foundations
`rendered_node_size` with the Flutter-parity fixture, routed through all four
callers; per-layer column width in Sugiyama. `Node.hand_moved`, set from the
scope-aware drag path. The identity snapshot extended per D14 and re-applied on
a name match. The `find_connected_components` seed sorted (D7). `diff_scope`
producing per-scope `EditDelta`s, tested but not wired.

*Tests:* the size fixture; a 320-wide HOF in a Sugiyama layer no longer overlaps
the next column; delta classifies add / grow / remove / rewire; a value-only
change gives an empty delta; shrinking an array pin or putting a literal on a
wired pin yields `removed_wires` and no `grown`; unwiring `f:` on an Auto-mode
`map` classifies it `grown`; a body statement that adds a node classifies the
owning HOF `grown` in the parent; `hand_moved` round-trips through `.cnnd` and
copy/paste, and a pre-flag `.cnnd` loads with `false`; two equal-size
components lay out identically across processes; a `--replace` of an unchanged
script yields an empty delta in every scope and leaves a `Collapsed` HOF
collapsed with its body size bit-identical; a renamed node is `added`.

### Phase 2 — Motion primitives, and the GUI migration
`shift_half_plane` with the window rule, the vertical cascade, `grow_rect`,
Step 2. Switch the three `make_space_for_inline` call sites to `grow_rect`,
delete it, adjust `reflow_test.rs`. Independently valuable: it fixes the GUI's
overlap-creating growth today.

*Tests:* width-only growth shifts exactly the half-plane and moves nothing
below; height-only growth pushes only colliding nodes, by the minimum; the
quadrant shift's right-moved / down-moved overlap does not occur; a node that
already overlapped the old rect is left alone; the cascade terminates on a
dense column and reaches a node whose x-interval overlaps a pushed node but not
`R`; a forward wire crossing the line stays forward; a loose column straddling
the default threshold (`T − 3`, `T + 3`) moves as a whole; a snap never lands
at or below the grown node's x; the default is kept when the nearest gap is
over a node width away; cases A / B / C reflow tests pass with new
expectations and single-step undo/redo unchanged.

### Phase 3 — Block layout and placement
`layout_subgraph`, blocks, anchors including the synthesized body anchors,
Step 4, Step 5 with the slack-first rule, the inside-out driver.

*Tests:* one added node lands beside its input and nothing moves; a 20-node
connected addition is placed as one block and nothing moves; an anchorless
addition goes right of the drawing; a block needing a new column shifts the
half-plane and nothing reorders; a consumer whose left edge overlaps an input
is still moved (threshold clamped); a consumer at or left of an input's left
edge produces no shift and no input moves; an uncollided comment stays at its
exact position whichever side of its anchor it is on; a `$element → mul →
output` body lays out left to right and does not grow a default body; a node
added to a body with slack leaves the parent's delta empty; a node added to a
full body shifts the HOF's right neighbour by exactly the width delta;
two-level nesting cascades to the grandparent; a new `map` with a three-node
body is placed at its settled footprint and overlaps nothing; no body node gets
a negative coordinate.

### Phase 4 — Repair passes and tiebreakers
Steps 6, 7, 8 and the `hand_moved` tiebreakers.

*Tests:* a backward rewire shifts the half-plane; an already-backward wire is
untouched; a cross-scope wire is never repaired; removing a wire moves nothing;
a 400×300 comment is pushed by the minimum and never overlapped; a comment left
behind by a shift is pulled back to its exact offset, one whose spot is taken
stays, one that drifted closer is not moved, one with an unresolvable anchor is
skipped, and the pass never creates an overlap; a new `on:`-anchored comment
lands beside its anchor; a guard that no cascade pushes more than ~20 nodes.

### Phase 5 — Wiring it up
Replace the `layout_network` call in `ai_text_edit`, producing
`LayoutPath::Incremental`. Body-aware full reflow. Preferences and the
Auto-Layout menu item. Reference guide: `doc/reference_guide/node_networks.md`
and `doc/reference_guide/ui.md`.

*Tests:* end-to-end through `ai_text_edit`; a corpus regression on both
corpora (a synthetic edit to one network; every node outside the delta and the
pushed band is at its exact position, bodies included; no loose column of
demolib split); a `query` → `edit --replace` round-trip of a body-bearing
network moves nothing; a full reflow with an expanded HOF produces no overlap.

*Manual verification* (`feedback_manual_test_for_editor_ui`): AI-add a node in
a dense region; AI-add a subassembly; AI-add a node inside a `map` body and
confirm only the map's right neighbour moves; delete a node and confirm the
hole stays; drag a node then AI-edit near it; collapse an HOF, round-trip via
`--replace`, confirm it stays collapsed; full reflow and undo.

### Phase 6 (later)
Downstream-cone shifting instead of half-plane. Hole reuse. A partial-move
fallback in Step 8. `grow_rect` on the zone-resize drag (`set_zone_size`).

---

## Open questions

1. **Does `auto_layout_after_edit` survive as a preference?** Proposed: the
   incremental pass always runs; the full reflow is only user-invoked.
2. **Does a full reflow clear `hand_moved`?** Honest but destroys intent;
   alternative: keep the flags for a later "respect manually placed" reflow.
3. **Is `SLIDE_WINDOW` exposed?** Proposed: no.
4. **Should drift be surfaced?** A backward-wire counter since the last reflow
   could drive a passive "tidy up?" hint. Never automatic.
5. **Should the slack-first slide also search in x?** As specified it slides
   only in y; a block whose anchor-derived x already exceeds the stored width
   grows the body regardless.
6. **Should the zone-resize drag reflow?** One line once `grow_rect` exists;
   deferred to Phase 6.
