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
to these two. The shift takes a **fixed set**, the site's must-not-move nodes
and their upstream closure, which it leaves in place; the cascade clears
whatever the shift landed on them. The cascade repairs only overlaps it
created. The landed quadrant shift (`node_inlining::make_space_for_inline`)
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

**D15 — A half-plane shift line is placed by the window rule.** It starts at
the site's default, never right of the leftmost must-move node, and snaps left
into whitespace. Must-not-move nodes are excluded from the shift rather than
bounding the line, so there is no empty-window case and a consumer sitting left
of its input is moved past it. See [The window rule](#the-window-rule).

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
    pub added_wires: Vec<WireKey>,
    pub removed_wires: Vec<WireKey>, // layout-inert, carried for faithfulness
}

/// A wire by the name paths of its endpoints: source path + output pin,
/// destination path + the slot `WireAnchor` (#427) uses. Never by node id:
/// `--replace` mints fresh ids, and an id-keyed diff would report every wire
/// of the network as `added` and hand the whole drawing to Step 6.
pub struct WireKey { /* … */ }
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
- **Name matching is total**: every node has a `custom_name` from creation,
  and `.cnnd` load assigns one to every legacy node that lacks it
  (`serializable_to_node_network`, recursing into bodies), so a file whose
  nodes carry no `custom_name` on disk (demolib is one) is fully named in
  memory. A renamed node simply fails to match and is `added`, which is
  always safe.

---

## Algorithm

Eight steps per scope, in order. Grown nodes are repaired **before** blocks are
placed, so blocks see settled obstacles.

Layout runs on the **post-edit** network: the `removed` nodes are already gone,
and the `added` nodes already exist, sitting at the throwaway positions the
editor gave them at creation (`calculate_new_node_position`; a matched node
under `--replace` is back at its snapshot position). **An added node is
invisible until its block is placed in Step 5** (a new comment until Step 7):
it is not an obstacle, not a snap candidate, not part of the drawing's bbox,
and neither primitive moves it. The obstacle set of a scope is its kept nodes
plus the blocks placed so far; "placed node" below means a member of that set.
Get this wrong and Step 2 pushes junk-positioned new nodes around while Step 5
slides around phantoms.

### Step 1 — Take stock

Nothing moves. The `removed` nodes are gone already and leave holes (D4); their
only layout use is Step 8, where a comment whose anchor was removed is skipped.
Build the obstacle set: the kept nodes.

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
| `U`, `D` both empty | `x` = the drawing's bbox left edge (see below) |
| `U` empty | `x = x_max` |
| `D` empty | `x = x_min` |
| `x_min ≤ x_max` | `x = x_min` |
| `x_min > x_max` | **no room**: `shift_half_plane(T, x_min − x_max, fixed = U ∪ upstream(U))` with `T` from the [window rule](#the-window-rule), default `x_min`, right end `min d.x`; then `x = x_min`. A consumer at or left of an input is moved past the block like any other; the input and its upstream closure stay. |

**Vertical:** align by connections, `y_offset = mean over external wires of
(anchor.y_center − internal.y_center_local)`. No anchors: below the drawing's
bbox + `VERTICAL_GAP`. Empty body: at the body's left padding, level with the
first zone-input pin.

**Why an anchorless block goes bottom-left, not right.** An anchorless
addition is nearly always a *source* — a constant, a `parameter`, a
`unit_cell`, an `import`, a comment — and in a rightward-flow drawing sources
belong on the left. Placed at the drawing's leftmost x, every wire it later
receives is forward, so Step 6 never has to widen the drawing for it; placed
right of the drawing (the first draft's rule) the very next wire out of it was
backward and Step 6 shifted everything downstream of the destination right by
the whole drawing's width. Repeated anchorless additions stack into a column
down the left side, which is what people draw by hand anyway, and the drawing
never grows wider, only taller. Verified on the maintainer's walkthrough:
`int` → far right → wired into a `structure_move` → 780 px shift of the rest.

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
[vertical cascade](#the-vertical-cascade) over `R` with empty `fixed` and
`ignore` sets, direction per first-round node by which side of `R`'s centre it
lies on.

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
`source.x + width(source) + GAP > dest.x`,
`shift_half_plane(T, deficit, fixed = {source} ∪ upstream(source))` with the
window's right end at `dest.x`. This covers the genuinely backward wire, a
destination **left** of its source, and not only a consumer crowding its
producer: the destination and everything else at or right of `T` move right by
the deficit while the source and its upstream closure stay, so the wire ends
forward and no other wire flips (see [the primitive](#shift_half_planet-dx-fixed)).
A plain half-plane shift could never do this, since it preserves x-order and
any line at or left of the destination would carry the source along. The price
is proportional to how far left the destination sat, the same widening any
insertion costs. Wires already backward before the edit are left alone;
cross-scope wires are skipped.

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

Both preserve order along their axis among the nodes they move, and a call to
either ends with no overlap it did not inherit.

### `shift_half_plane(T, dx, fixed)`

Translate every placed node with `x ≥ T` right by `dx`, except the nodes in
`fixed`. `fixed` is the site's must-not-move nodes together with their
**upstream closure**, every node with a wire path into them. The closure is
what keeps the shift wire-safe:

- a wire from an unmoved node into a moved one only gets longer;
- a wire from a moved node into a fixed one cannot exist: its source would be
  upstream of a fixed node and therefore fixed itself;
- wires between two moved nodes, or between two unmoved nodes, are unchanged.

So **a forward wire never turns backward**, and alignment and gaps are
preserved exactly within the moved set and within the unmoved set. The
exclusion has one cost: a moved node can land on a fixed node that sits at or
right of `T`. After the translation, each such fixed node is handed to the
cascade as `R` (direction per intruder by which side of the fixed node it lies
on, `ignore` = the nodes that already overlapped it before the shift), which
pushes the intruders off vertically. Fixed nodes at or right of `T` are rare
(an input whose consumer sat left of it), so this is usually a no-op. The
drawing gets wider, which is the honest cost of inserting something. Used by
Step 2 (via `grow_rect`), Step 4 and Step 6.

### The window rule

A shift is safe at any threshold but not equally good at any: dropped at an
arbitrary x it cuts through a loose column (half the nodes are within 8 px of
another node's x). Each shift site has nodes that must move and nodes that
must not. The latter go into `fixed`; the former bound the line on the right:

| Site | Fixed (plus upstream closure) | Must move | Right end of window |
|---|---|---|---|
| Step 4, no room | every upstream anchor | every downstream anchor | `min d.x` |
| Step 2, `grow_rect` width | the grown node | everything right of its old right edge | the old right edge |
| Step 6, backward wire | the source | the destination | `dest.x` |

1. Start at the site's default (`x_min`, the old right edge, `dest.x`),
   clamped to the right end. `x_min` can lie right of `min d.x` when a
   consumer's left edge overlaps an input horizontally; clamping is what makes
   that consumer move.
2. Snap **left, never right**: to the nearest x where no placed node's left
   edge is within 8 px and no placed **non-fixed** node's box straddles the
   line. Left keeps every required node moving; right would not. A fixed node
   still counts for the 8 px test (a line at its left edge would move its
   column-mates and not it) but its box may be cut: it stays whichever side of
   the line it is on, which is why the grown node's own box, or the source's,
   cannot block the snap. Without that exclusion the snap could never enter
   the box that spans the whole `grow_rect` window, and the default would
   always be kept. Search at most one node width left of the default; keep the
   default if no gap exists within it.
3. There is no left bound and no empty case. Whatever `fixed` holds stays put
   wherever the line falls, and the deficit was measured against those fixed
   nodes, so any `T` at or left of the right end creates the room. A consumer
   at or left of its input, or a destination at or left of its source, is
   simply moved past it.

Why the default is the left end of the useful range rather than `min d.x`: the
nodes in the strip between `x_min` and the consumers are inside the column that
is widening; shifting them right with the consumers is the "insert a column" a
human would do, while leaving them to the cascade splits them from their row.

### The vertical cascade

```
cascade(R, dir_of, fixed, ignore):
    Q ← placed nodes overlapping R in both axes, not in fixed, not in ignore,
        ascending id, each tagged dir(n) ← dir_of(n) and blocker ← R
    while Q not empty:
        (n, blocker) ← pop front
        before ← n's rect
        move n along dir(n) by the minimum (possibly zero) that clears blocker + GAP;
            then, while n overlaps a fixed node it did not overlap at `before`,
            move it further along dir(n) to clear that node too
        for every placed m ∉ fixed, m ≠ n, ascending id, with x-overlap(n, m),
            y-overlap(n, m) now and NO y-overlap(before, m):
                dir(m) ← dir(n); push (m, blocker ← n) to the back of Q
```

A vertical Force-Scan restricted to **new** overlaps: only colliding nodes
move, by the minimum, and the push propagates only through collisions the
cascade itself created. A pre-existing overlap, whether listed in `ignore` or
between any two nodes that already overlapped, is never repaired, so a node
that already sat on `R` is neither enqueued nor an obstacle to those that are.
It must test against every placed node, not a band: a pushed node can land on
a node whose x-interval overlaps *its* but not `R`'s.

Why the enqueue test is "newly overlapped" and not "further along `dir`": a
node that `n`'s move newly overlaps lies **entirely on the `dir` side of `n`'s
previous rect** (it did not overlap that rect, and `n` moved toward it), so
pushing it the same way keeps the pair in its original order whatever their
centres say. A centre test misses a taller node, a comment or an expanded HOF,
whose box `n` has entered but whose centre is still behind `n`'s, and leaves
that overlap standing.

Correctness, in three parts. (i) The up-set and the down-set never meet: a
first-round down node has its centre below `R`'s, every node it newly
overlaps lies below its old bottom, and so on down the chain, so every
down-set node has its top below `R`'s centre; symmetrically every up-set node
has its bottom above it, and no node can be both. (ii) It terminates: a node is
pushed only by nodes that were originally entirely on its far side with an
overlapping x-interval, a strict order, so each push sets the node to at most
the longest-path bound over that order, every move is monotone along `dir`,
and by induction over the order each node is pushed finitely often. (iii) At
exit no node overlaps `R`, a fixed node, or any node it did not already
overlap before the call. What stops it in practice is vertical whitespace,
which the simulation says arrives almost immediately. No cap is imposed;
Phase 4 asserts a generous bound in tests.

### `grow_rect`

```rust
/// node_id's footprint grew from `old` to `new`, top-left fixed. Returns the moves.
fn grow_rect(network, registry, node_id, old: DVec2, new: DVec2) -> Vec<(u64, DVec2, DVec2)> {
    let anchor = node.position;
    let (dw, dh) = (max(0, new.x - old.x), max(0, new.y - old.y));
    let fixed = {node_id} ∪ upstream(node_id);
    if dw > 0 { shift_half_plane(snap(default = anchor.x + old.x, right_end = anchor.x + old.x), dw, fixed); }
    if dh > 0 { cascade(R = (anchor, new) inflated by GAP, dir_of = |_| down,
                        fixed = {node_id}, ignore = nodes overlapping (anchor, old)); }
    diff positions
}
```

Horizontal first so the cascade sees post-shift positions. Pre-existing overlaps
with the old rect go into `ignore`, which is also what makes forcing `down`
correct: after the shift every *new* collision lies below the old bottom edge
(a node left of the line that reaches into the widened strip already overlapped
the old rect horizontally, so if it is new it is new in y, and the rect only
grew downward).

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

## Testing

The per-phase lists below are scenario catalogues. They are not the safety
net; the invariants are, and every test asserts them mechanically.

**The oracle.** One shared support module,
`tests/structure_designer/layout_oracle.rs` (the `diff_test_support.rs`
precedent), exposes `check(before, after, delta, fixed)` and is called by every
layout test and by the corpus run. It walks every scope (`walk_all_nodes`) and
asserts:

1. **No new overlap:** the set of overlapping placed-node pairs after ⊆ before,
   per scope, at `rendered_node_size`.
2. **No wire flipped:** every wire forward before (source right edge + `GAP` ≤
   dest x) is forward after; cross-scope wires excluded.
3. **Untouched means untouched:** every node not in the delta and not in the
   returned move list is at its exact pre-edit position, bit-identical.
4. **Rigid shifts:** among nodes a `shift_half_plane` moved, pairwise offsets
   are unchanged; `fixed` nodes did not move.
5. **Order among the pushed:** for every x-overlapping pair the cascade moved,
   the vertical order is the pre-edit order.
6. **Bodies:** no negative coordinate; every HOF's rendered footprint contains
   its body content plus padding; a body that did not grow past its stored
   size left the parent's delta empty.
7. **Determinism:** the run is repeated from the same input and compared
   bit-identical (D7).
8. **Bound:** no single cascade moved more than 20 nodes.

A scenario test therefore states only what is *specific* to it (which node
landed where, what moved) and gets the eight properties for free. If a
property is expected to be violated, the test says so explicitly and why (the
only known case: pre-existing overlaps, which invariant 1 already tolerates).

**Corpus.** `demolib/baselib_with_demos.cnnd` is tracked and is the CI corpus;
`from_mechadense.cnnd` is the maintainer's private file and runs only when
`ATOMCAD_LAYOUT_CORPUS` names it. Both load through the real `.cnnd` loader
(so names are total, see [the delta](#the-edit-delta)), not by parsing JSON.
Two runs:

- *Randomized edits*, `tests/structure_designer/layout_corpus_test.rs`: a
  seeded generator applies, to every network with ≥ 2 nodes, each of: add one
  node wired from a random kept node; add a 3-node chain between two random
  kept nodes; grow a random node by a pin; add a node to a random body; rewire
  a random consumer to a producer right of it; delete a random node; add an
  anchored comment. Every edit goes through `ai_text_edit` and the oracle. A
  failure prints the network name, seed and script so it can be pinned as a
  scenario test.
- *Moved-list snapshots*: for one fixed edit per demolib network, the
  name-path keyed `LayoutOutcome.moved` list (the AI edit log's own
  measurement, not test bookkeeping) is an `insta` snapshot. An algorithm
  change then shows exactly which drawings it touched and `cargo insta review`
  is the review. Positions themselves are not snapshotted; the oracle covers
  them.

**Flutter parity of the size function.** The Rust size fixture is not
hand-copied numbers. A Dart test in `test/` (`flutter_test`, the existing
`node_network_content_test.dart` style) loads
`tests/fixtures/layout_size_parity.cnnd` through `ScopeResolver` and writes
`tests/fixtures/layout_size_parity.json`: name path → rendered size, for a
comment, an expanded HOF, a collapsed HOF, a closure and a two-level nested
HOF. The JSON is checked in; the Rust test reads it and asserts
`rendered_node_size` equal to the pixel. Regenerating the JSON is the Dart
test's job, so a Flutter-side size change fails the Rust test until the
fixture is refreshed, which is the point.

**What stays manual.** Anything that needs the rendered canvas: the
walkthrough in Phase 5, run by the maintainer
(`feedback_manual_test_for_editor_ui`; the Flutter smoke test is never run by
an agent).

---

## Phases

### Phase 1 — Foundations
`rendered_node_size` with the Flutter-parity fixture, routed through all four
callers; per-layer column width in Sugiyama. `Node.hand_moved`, set from the
scope-aware drag path. The identity snapshot extended per D14 and re-applied on
a name match. The `find_connected_components` seed sorted (D7). `diff_scope`
producing per-scope `EditDelta`s, tested but not wired. The
[oracle](#testing) module and the parity fixture pipeline, so every later
phase's tests have them from the start.

*Tests:* the size fixture against the Dart-generated JSON; a 320-wide HOF in a Sugiyama layer no longer overlaps
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
`R`; a short node pushed into a taller comment's box pushes the comment too,
whatever the centres say; a pushed node that reaches a fixed node clears it;
the up-set and the down-set never touch; a pre-existing overlap is neither
repaired nor an obstacle; a forward wire crossing the line stays forward; a
loose column straddling the default threshold (`T − 3`, `T + 3`) moves as a
whole; a snap into the grown node's own box is accepted and moves nothing of
the grown node; an input of the grown node sitting right of the line stays
(upstream closure) and the node that lands on it is pushed off; the default
is kept when the nearest gap is over a node width away; cases A / B / C reflow
tests pass with new expectations and single-step undo/redo unchanged.

### Phase 3 — Block layout and placement
`layout_subgraph`, blocks, anchors including the synthesized body anchors,
Step 4, Step 5 with the slack-first rule, the inside-out driver.

*Tests:* one added node lands beside its input and nothing moves; a 20-node
connected addition is placed as one block and nothing moves; an anchorless
addition goes below the drawing at its left edge; a block needing a new column shifts the
half-plane and nothing reorders; a consumer whose left edge overlaps an input
is still moved (threshold clamped); a consumer at or left of an input's left
edge is moved right past the block while the input and its upstream chain
stay; a new node dropped by `calculate_new_node_position` onto a kept node
moves nothing before its block is placed; an uncollided comment stays at its
exact position whichever side of its anchor it is on; a `$element → mul →
output` body lays out left to right and does not grow a default body; a node
added to a body with slack leaves the parent's delta empty; a node added to a
full body shifts the HOF's right neighbour by exactly the width delta;
two-level nesting cascades to the grandparent; a new `map` with a three-node
body is placed at its settled footprint and overlaps nothing; no body node gets
a negative coordinate.

### Phase 4 — Repair passes and tiebreakers
Steps 6, 7, 8 and the `hand_moved` tiebreakers.

*Tests:* a rewire whose destination sits left of its source moves the
destination (and what lies right of it) past the source while the source's
upstream chain stays, and no other wire flips; a rewire within one loose
column widens the gap; a `--replace` of an unchanged script classifies no wire
as added; an already-backward wire is untouched; a cross-scope wire is never
repaired; removing a wire moves nothing;
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

*Tests:* end-to-end through `ai_text_edit`; the randomized corpus run and the
moved-list snapshots of [Testing](#testing) switched on (demolib in CI, the
working file behind the env var), plus one hand-checked demolib case where a
loose column straddles the shift line and is asserted to move as a whole; a
`query` → `edit --replace` round-trip of a body-bearing network moves
nothing; a full reflow with an expanded HOF produces no overlap.

*Manual verification* (`feedback_manual_test_for_editor_ui`): AI-add a node in
a dense region; AI-add a subassembly; AI-add a node inside a `map` body and
confirm only the map's right neighbour moves; delete a node and confirm the
hole stays; drag a node then AI-edit near it; collapse an HOF, round-trip via
`--replace`, confirm it stays collapsed; full reflow and undo.

### Phase 6 (later)
Downstream-cone shifting where the half-plane moves too much. Hole reuse. A partial-move
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
