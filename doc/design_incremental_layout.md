# Design: incremental layout for AI/human co-editing

**Status:** first draft for review. Framework proposed by the maintainer;
details filled in here. Research background: `doc/research_intent_preserving_layout.md`.

**Problem.** `auto_layout_after_edit` defaults to `true` and the default
algorithm is Sugiyama, so every AI text edit re-derives every node position from
the topology alone. A human's arrangement is destroyed on every edit.

**Approach.** Characterize the edit as a delta (added / modified / removed),
lay out only the added nodes, fit them into the existing drawing, and repair
locally. Existing nodes move only when something forces them to, and then by a
rigid translation that preserves their relative structure. A full
non-incremental Sugiyama stays available as an explicit user command — the user
trades familiarity for algorithmic optimality when *they* decide to.

---

## Why "just re-run the layout" cannot work

Measured over the two hand-drawn `.cnnd` files in the repo root
(`from_mechadense.cnnd` and `SPM-tip-with-tool_…cnnd` — two saves of the same
project, so one corpus: 76 networks, 2,262 nodes, 2,057 wires):

| Property | Hand-drawn reality |
|---|---|
| Nodes on a canonical column x (`START_X + col·COLUMN_WIDTH` = `100 + col·210`) | **0 / 2262 = 0.0%** |
| Distinct x values | 2159 for 2262 nodes |
| Nodes sharing an exact x with another node | 8.7% |
| Wires pointing rightward | 91.4% |
| Wires rightward with a full node width of clearance | 81.7% |
| Overlapping node pairs already present (est. 160×83 boxes) | ~140 |

Three consequences that shape this design:

1. **Zero nodes are where the algorithm would put them.** A full reflow rewrites
   100% of positions in every one of the 76 networks. There is no "mostly
   already correct" case to exploit.
2. **There is almost no column structure to rediscover.** Nearly every node has
   its own x. Any scheme that tries to snap the human drawing onto a grid is
   rewriting it, not preserving it.
3. **Rightward flow is a real shared invariant** (91.4%), but not a universal
   one. 177 backward wires and ~140 overlaps already exist in a drawing its
   author is content with. So the algorithm must never "fix" pre-existing
   irregularities — only ones the current edit introduced.

Point 3 generalizes to the rule this design follows throughout:

> **Repair only what this edit broke.** The pre-edit drawing is the baseline,
> however irregular. Its quirks are carried forward untouched.

*(Overlap count is approximate: it uses the same 160×83 estimate the layout code
uses, so it undercounts — comments are 200×100+ and HOF bodies larger. See
[Prerequisite: real node sizes](#prerequisite-real-node-sizes).)*

---

## Design decisions

**D1 — The unit of work is a diff, not a network.** Layout consumes an
`EditDelta` (added / modified / removed node ids, plus added and removed wires),
computed by comparing a pre-edit snapshot against the post-edit network. This is
possible because `NetworkEditor::apply(code, replace=false)` already merges
incrementally: node ids and positions of untouched nodes survive the edit.

**D2 — Added nodes are laid out as a group, by Sugiyama, in isolation.** New
nodes have no layout history, so there is nothing to preserve and the best
available algorithm should be used. The result is a **block** with local
coordinates and a bounding box, placed as a rigid unit.

**D3 — Existing nodes are never re-derived, only translated.** No existing node
ever gets a position computed from the topology. It either stays exactly where
it is, or it is moved by a translation shared with a whole set of nodes. A rigid
translation preserves relative order, alignment, spacing and whitespace within
the translated set exactly — which is the entire point.

**D4 — Deletion leaves holes.** Removing a node does not compact the drawing.
Compaction would move untouched nodes, which is the thing being avoided, and the
hole is free space the next block can use. Drift from accumulated holes is
accepted; the explicit full reflow is the cure.

**D5 — `Node.hand_moved: bool`.** Set when a user drags a node. Persisted,
undoable, `#[serde(default)]`. Used as a *tiebreaker*, never as a hard
constraint (see [Uses of `hand_moved`](#uses-of-hand_moved)) — a hard "never
move" would make some edits unsatisfiable.

**D6 — One collision primitive, used everywhere.** Placing a block, growing a
node, and repairing a backward wire all reduce to "put this rectangle here and
make it not overlap anything". A single routine serves all three.

**D7 — Determinism is a requirement, not a nicety.** Every iteration over nodes
is over ids sorted ascending. Layout today ties-breaks on `HashMap` iteration
order, which std randomizes per process; stability is untestable until that is
fixed. This is a prerequisite of the first phase.

**D8 — No global post-pass.** No compaction, no beautification, no
"while we're here" improvements. The output differs from the input only where
the delta forced it to.

---

## The edit delta

```rust
pub struct EditDelta {
    /// Present after, absent before.
    pub added: Vec<u64>,
    /// Present in both, but wiring, node type, or parameter count changed.
    pub modified: Vec<u64>,
    /// Present before, absent after.
    pub removed: Vec<u64>,
    /// Wires that did not exist before. Keyed like `WireAnchor` (#427).
    pub added_wires: Vec<WireKey>,
}
```

Computed by `diff_networks(before: &LayoutSnapshot, after: &NodeNetwork)`, where
`LayoutSnapshot` is a transient pre-edit capture of `{id → (position, size,
wire set)}`. Not persisted.

`modified` matters for two reasons only, both geometric:

- **the node got taller** (parameter count changed) and may now overlap a
  neighbour;
- **the node was rewired** and a new wire may now point backwards.

A node whose *value* changed but whose size and wiring did not is not a layout
event at all and is excluded.

### Name-based identity for `replace` mode

`text_edit_network(…, replace: bool)` clears the network when `replace == true`,
so every node is "added" and the incremental path degenerates to a full reflow.
The text format is name-based, so this is recoverable cheaply: when replace mode
rebuilds the network, **match new nodes to snapshot nodes by `custom_name`** and
carry the old position and `hand_moved` flag across. A matched node is `kept`,
not `added`. This makes the design work for both edit modes and costs one
`HashMap<String, u64>` lookup per node.

---

## Algorithm

Five steps, in order. Each is small.

### Step 1 — Remove

Delete the `removed` nodes and their wires. Do nothing else (D4).

### Step 2 — Lay out the added nodes as blocks

Restrict the graph to `added` and split into connected components (using only
wires **between added nodes**). Each component is a **block**.

For each block, run the existing Sugiyama on the subgraph alone:

```rust
// layout/mod.rs — new entry point
pub fn layout_subgraph(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    ids: &HashSet<u64>,
    algorithm: LayoutAlgorithm,
) -> HashMap<u64, DVec2>;
```

Implemented by threading an optional id filter through
`compute_node_depths` / `group_by_depth` / the barycenter passes, so that wires
leaving the subset are ignored. This reuses the whole existing pipeline rather
than duplicating it, and `layout_network` becomes `layout_subgraph` over all
ids.

The result is a set of local positions per block; take its bounding box
`(W, H)`.

Blocks are processed in a deterministic order: by `(min anchor x, min node id)`
— see Step 3 for anchors. Each block, once placed, joins the obstacle set for
the next.

### Step 3 — Choose each block's target position

Define the block's **anchors** — kept nodes wired to it:

- `U` = kept nodes with a wire *into* the block (upstream);
- `D` = kept nodes fed *by* the block (downstream).

**Horizontal.** Let `GAP` be the standard horizontal gap
(`node_layout::DEFAULT_HORIZONTAL_GAP`).

```
x_min = max over u in U of (u.x + width(u) + GAP)      // right of every input
x_max = min over d in D of (d.x - GAP - W)             // left of every consumer
```

| Case | Placement |
|---|---|
| `U` and `D` both empty | right of the whole drawing's bbox + `GAP` |
| `U` empty | `x = x_max` (hug the consumers) |
| `D` empty | `x = x_min` (hug the inputs) |
| `x_min ≤ x_max` | `x = x_min` — leftmost valid spot, keeps wires short |
| `x_min > x_max` | **no room**: `shift_half_plane(x_min, x_min - x_max)` first, then `x = x_min` |

**Vertical.** Rather than centring the block, align it by its connections. For
every wire between a block-internal node `b` and an anchor `a`, the ideal offset
is `a.y_center - b.y_center_local`. Take the mean:

```
y_offset = mean over external wires of (anchor.y_center - internal.y_center_local)
```

This naturally places a block feeding one node level with that node, and a block
straddling two anchors between them. With no anchors, place below the drawing's
bbox.

### Step 4 — Fit the block in (the collision primitive)

The block now has a target rect `R = (x, y_offset, W, H)` inflated by `GAP`.
Two mechanisms, tried in order:

**(4a) Slide the block.** Search for a free `y` near the target — alternating
down and up in increments of `VERTICAL_GAP` — within a bounded window
(`SLIDE_WINDOW`, suggested: two node heights). Nothing existing moves. If a free
`y` is found, done.

**(4b) Push the wavefront open.** Otherwise place the block at its target `y`
and displace the kept nodes that are in the way — a vertical Force-Scan:

```
W ← every kept node overlapping R in BOTH axes,
    ordered by |node.y_center − R.y_center|
for each n in W:  dir(n) ← up if n.y_center < R.y_center else down

while W not empty:
    n ← pop W
    push n by the minimum amount along dir(n) to clear its blocker
        (R on the first round; otherwise the node that enqueued it)
    for every node m in the WHOLE network — not merely the initial set —
        with x-overlap(n, m) and y-overlap(n, m) and m further along dir(n):
            dir(m) ← dir(n); push m to clear n; enqueue m
```

**The propagation must test against every node, not a fixed band.** A node
pushed upward can land on a node whose x-interval overlaps *its* — but not
`R`'s. Restricting the closure to nodes that overlap `R` would leave that
collision unresolved and silently create a new overlap. Consequently the
horizontal footprint of a push is **emergent and monotonically non-decreasing**:
it is whatever the closure happens to reach, and it can end up considerably
wider than `R`. Think of it as a wavefront moving in one vertical direction, not
as a band.

Two properties make this safe:

- **It terminates.** Every node enqueued by `n` is strictly further along
  `dir(n)` than `n` is, and no node ever moves back. A cycle would require a
  node to be both above and below another. Finite node count, monotone
  displacement. Naive cost is O(n²); the largest network in the corpus has 155
  nodes.
- **Vertical ordering is preserved by construction** — Misue et al.'s
  orthogonal-ordering property, which is what a hand drawing actually encodes.
  Nodes move by the *minimum* amount, so displacement stays small.

**What stops the cascade is vertical whitespace, not the band edge.** x-overlap
is necessary but not sufficient to propagate; an actual y-collision is required.
This matters because x-overlap is rampant in real drawings — the corpus has
nearly one distinct x per node with 160px boxes, so the "x-intervals overlap"
graph is usually a single component spanning the whole network. In principle one
cascade could reach everything; in practice it runs out of collisions first.

Measured over 2,280 simulated insertions on `from_mechadense.cnnd` — dropping a
node-sized rect exactly onto each existing node's position, a guaranteed initial
collision, using real comment dimensions:

| | |
|---|---|
| Cascades pushing **nothing** | 50.9% |
| Pushing ≤ 1 node | 77.0% |
| Pushing ≤ 5 nodes | 98.1% |
| Max nodes pushed | 10 |
| Final x-extent ÷ `R` width | median 1.00×, p90 1.76×, p99 2.83×, max 5.80× |

The 5.80× worst case is 571px of extent in a 1052px-wide network; the worst on a
large network was 608px of 5837px. Both figures overstate the real cost, because
(4a) runs first and a dead-centre drop is the densest possible start.

**No cap is imposed on the cascade.** A limit would be a tuning constant with no
evidence behind it, and the fallback it would need (abandon the push, slide the
block arbitrarily far) is not obviously better than a wide push. Phase 3 asserts
a generous bound in tests instead, to catch a regression rather than to shape
behaviour.

`SLIDE_WINDOW` is the design's one real tuning constant. It sets the trade
between long wires (slide too far) and disturbed neighbours (push too eagerly).

### Step 5 — Repair modified nodes

Only for nodes in `modified`, and only for the two geometric cases:

**Grew taller.** Run (4b) with the node's own new rect as `R`, with the node
itself excluded from the obstacle set. Same primitive (D6).

**New backward wire.** For each wire in `added_wires` whose endpoints are both
kept, if `source.x + width(source) + GAP > dest.x`, then
`shift_half_plane(dest.x, deficit)`. Wires that were already backward before the
edit are left alone — that is the baseline rule, and the measurement says 177 of
them exist.

### The `shift_half_plane` primitive

```rust
/// Translate every node with `x >= x_threshold` right by `dx`.
fn shift_half_plane(network: &mut NodeNetwork, x_threshold: f64, dx: f64);
```

A rigid translation of a half-plane. It cannot introduce an overlap, it cannot
reorder anything, and it preserves every alignment and every deliberate gap
within the moved set and within the unmoved set. It makes the drawing wider,
which is the honest cost of inserting something.

Used in exactly two places: Step 3 (no horizontal room for a block) and Step 5
(a rewire made an existing wire point backwards).

*Refinement, not for v1:* shift only the destination's **downstream cone**
instead of the whole half-plane. More surgical, but a translated cone can
collide with non-cone nodes, so it needs (4b) afterwards. The half-plane version
is unconditionally safe.

---

## Uses of `hand_moved`

The flag is a tiebreaker in three places. None of them is a hard constraint —
a node that must move to keep the drawing correct still moves.

1. **Push direction (4b).** When both up and down clear the block, push the
   direction that displaces fewer hand-moved nodes; on a tie, fewer nodes; on a
   tie, the smaller total displacement; on a tie, up.
2. **Slide vs. push (4a→4b).** Extend `SLIDE_WINDOW` when every obstacle in the
   band is hand-moved — prefer to route the new block around a deliberately
   arranged region rather than through it.
3. **Explicit full reflow.** A "respect manually placed nodes" option on the
   Auto-Layout command treats hand-moved nodes as fixed. Off by default: the
   whole point of the explicit command is to get the algorithmic result.

The flag is set in the drag handler (Flutter → API → `Node.hand_moved = true`),
persisted in `.cnnd`, and carried through copy/paste and the name-match path.

**Open:** whether a full reflow *clears* the flags on the nodes it moved. It
should — after the algorithm has placed a node, "a human placed this" is false —
but that makes the reflow destroy intent in a second, less obvious way. See
[Open questions](#open-questions).

---

## Prerequisite: real node sizes

Every collision test above needs a node's actual box. Today both layout
algorithms size nodes with `node_layout::estimate_node_height(params, outputs,
subtitle)`, which for a comment yields **83 px** against a real default of
**100** and routine resized values of **300+**, and which does not model HOF
bodies (`body_width` / `body_height`) at all. Placing correctly while sizing
wrongly produces overlaps in a tidier arrangement.

A single `fn node_box(node, registry) -> DVec2` that returns the real dimensions
for comments and HOF bodies, and the estimate otherwise, is a prerequisite of
Phase 2. `design_wire_annotations.md` Phase 5 needs the same fix and should
share it.

---

## Interaction with other subsystems

**Comment nodes (#427).** Comments are graph isolates; they never join a block
and are excluded from Step 2. Once `design_wire_annotations.md` lands,
an **anchored** comment follows its anchor: if the anchor moved by `Δ` in this
pass, the comment moves by `Δ` too. An unanchored comment moves only when a band
push displaces it. This is strictly better than that document's D9 scaffolding
and supersedes it — D10 anticipated exactly this replacement.

**HOF bodies.** Layout does not recurse into `node.zone` today (verified: no
mention of `zone` or `walk_all_nodes` anywhere in `layout/`). The delta and this
whole algorithm are scope-local, so the natural rule is: run it independently
per network, including bodies, on the scope the edit touched. A body whose
content grew may need its `body_width`/`body_height` increased, and the node then
counts as "grew" for Step 5 in its *parent* scope.

**Undo.** The whole AI edit, including the layout adjustment, must be one undo
step. `layout_active_network()` already wraps a reflow in one `MoveNodesCommand`
(#270); the incremental pass produces a smaller set of moves and uses the same
command. Setting `hand_moved` on a drag is a persisted mutation and needs to
ride in the existing move command rather than becoming a separate undo entry.

**Preferences.** `auto_layout_after_edit: bool` is replaced by a tri-state, or
simply repurposed: the incremental pass always runs (it is repair, not layout),
and the full reflow is only ever user-invoked. See open question 1.

---

## Phases

### Phase 1 — Foundations
`Node.hand_moved` with `#[serde(default)]`, set from the drag path, persisted
and undoable. `node_box()` returning real sizes. Deterministic id ordering
throughout `layout/` (D7). `LayoutSnapshot` + `diff_networks` producing an
`EditDelta`, with tests but not yet wired to anything.

*Tests:* delta correctly classifies add / modify / remove / rewire; a value-only
change produces an empty delta; `hand_moved` round-trips through `.cnnd` and
copy/paste; a pre-flag `.cnnd` loads with `hand_moved = false`; layout output is
byte-identical across two processes.

### Phase 2 — Block layout and placement
`layout_subgraph`, block decomposition, anchor computation, Step 3 target
position, Step 4a slide. No pushing yet: if the target is occupied, the block
falls below the drawing. `shift_half_plane` for the no-horizontal-room case.

*Tests:* one added node lands beside its input and **no existing node moves**; a
20-node connected addition is laid out internally by Sugiyama and placed as one
block with no existing node moving; an addition with no anchors goes right of
the drawing; a block needing a new column shifts the half-plane and nothing
reorders.

### Phase 3 — Band push and repair
Step 4b Force-Scan band push with cascade, the `hand_moved` tiebreakers, and
Step 5 (grown node, backward wire).

*Tests:* a block placed into an occupied band displaces the minimum number of
nodes and **never inverts a vertical order**; the cascade terminates on a dense
column; the cascade pushes a node whose x-interval overlaps a *pushed* node but
not `R` (the widening case — the narrow reading of the closure fails this one);
a node that grew pushes its neighbours down and nothing else; a rewire that
points backward shifts the half-plane; a wire that was **already** backward
before the edit is not touched. Plus a generous guard assertion — no cascade
pushes more than ~20 nodes — to catch a regression, not to bound behaviour.

### Phase 4 — Wiring it up
Replace the `layout_network` call in `ai_assistant_api.rs` with the incremental
pass. Preferences and the Auto-Layout menu item ("respect manually placed
nodes"). Reference guide: `doc/reference_guide/node_networks.md` (what happens
to your layout when the AI edits, and how to get a full reflow) and
`doc/reference_guide/ui.md` for the menu/preference.

*Tests:* end-to-end through `text_edit_network`; and a **corpus regression** —
load `from_mechadense.cnnd`, apply a synthetic edit to one network, assert that
every node outside the delta and outside the pushed band is at its exact
original position.

*Manual verification* (per `feedback_manual_test_for_editor_ui`): AI-add a node
in a dense region; AI-add a subassembly; delete a node and confirm the hole
stays; drag a node then AI-edit near it; full reflow and undo.

### Phase 5 (later) — Refinements
Downstream-cone shifting instead of half-plane. Hole reuse (place a block into a
deletion hole when one fits). Anchored-comment following once #427 lands.
Recursion into HOF bodies if Phase 4 leaves it out.

---

## Open questions

1. **Does `auto_layout_after_edit` survive as a preference?** The incremental
   pass is repair, not layout — it should probably always run, with the full
   reflow purely user-invoked. That would make the current default (destroy the
   layout on every edit) unreachable, which the measurement says is a feature.
2. **Does a full reflow clear `hand_moved`?** Clearing is semantically honest
   but silently discards intent. Alternative: keep the flags, so a subsequent
   "respect manually placed nodes" reflow can restore the distinction.
3. **Does the incremental pass recurse into HOF bodies in v1, or Phase 5?**
4. **Is `SLIDE_WINDOW` exposed?** Preference is: no. One internal constant,
   documented, not tunable.
5. **Should the drift after many edits be surfaced?** A cheap counter (e.g.
   number of backward wires introduced since the last full reflow) could drive a
   passive "this network could use a tidy-up" hint. Suggestion only, never
   automatic.
6. **Does `replace` mode get name-matching in v1?** It is cheap and makes the
   feature work regardless of which mode the AI uses, but it needs a rule for
   nodes without a `custom_name`.
