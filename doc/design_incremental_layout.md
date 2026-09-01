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

**D7 — Determinism is a requirement, not a nicety.** Stability is untestable
without it: "did this edit move anything?" has no answer if two runs of the same
input disagree. Every iteration over nodes in new code is over ids sorted
ascending.

Existing layout is **mostly** already deterministic, and the gap is narrower
than it first appears. `group_by_depth` ends with `layer.sort()`, so the
within-layer seed is ascending node id; `reorder_by_barycenter` uses a stable
sort, so equal barycenters preserve that seed; and `place_comments` sorts
comment ids explicitly. One genuine hole remains:
`sugiyama::find_connected_components` seeds its BFS by iterating
`network.nodes.keys()` — a `HashMap`, randomized per process — and
`components.sort_by_key(Reverse(len))` is stable, so **two disconnected
components of equal size stack in a per-process-random order**. (#427 removed
the largest source of this by excluding comments from the component walk; what
is left is genuine equal-size components.) Fixing it is a one-line sort of the
seed ids, and it is a prerequisite of Phase 1.

**D8 — No global post-pass.** No compaction, no beautification, no
"while we're here" improvements. The output differs from the input only where
the delta forced it to.

**D9 — On the incremental path, comment nodes are ordinary nodes.** They are
obstacles at their real `CommentData` size, they are pushed by the cascade, and
they translate with a half-plane shift — exactly like any other existing node.
There is **no** comment placement pass, and `place_comments` is not called and
not modified.

This deliberately diverges from `design_wire_annotations.md`, where comments are
excluded from layout (its D8) and re-placed by rule afterwards, never displacing
graph nodes. That treatment is right for a *full reflow*, where the graph has
just been re-derived and a comment's stored position is stale — and it stays in
force there, unchanged.

It is wrong here, for a reason that only became true once #427 landed:
**the dashed leader line now carries the association explicitly, so a comment's
position no longer encodes it.** Before anchors, proximity *was* the
association, which is why guarding it mattered. Now drift is cosmetic, visible,
and fixable with one drag; no information is lost.

Treating them as ordinary nodes is also strictly better on this path:

- a colliding comment is displaced by the *minimum* amount, with vertical
  ordering preserved, instead of being relocated to a four-sides candidate or
  banished to the gutter;
- it cannot overlap anything, because the cascade guarantees that for every node
  it touches;
- it requires **no change to the landed Phase 5 code**;
- and it obeys D3, which a re-derivation would violate — a comment the human
  positioned is a position the human chose.

The measured cascade figures in [Step 4](#step-4--fit-the-block-in-the-collision-primitive)
already reflect this rule: that simulation used real comment dimensions and
treated comments as ordinary obstacles.

**D10 — A drifted comment is pulled back to its anchor, if the spot is free.**
D9 lets an anchored comment and its anchor be displaced differently, which
stretches the leader line. A final pass (see [Step 7](#step-7--restore-drifted-comments))
tries to restore the comment's original offset from its anchor exactly, and
accepts the result only if it is collision-free. All-or-nothing: no partial
moves, no second-choice positions.

This is cheap, has **no tuning constant**, and cannot make anything worse — the
target restores the offset the human chose, the pass only runs when the distance
actually grew, and a colliding target is simply abandoned.

It replaces a ratio heuristic considered earlier ("if the distance grew beyond
~120%, try to shorten it"). Exact restoration is strictly better: the ratio form
needs a threshold *and* a rule for how far to move, and it can only ever
approximate the placement it is trying to recover.

It is not a violation of D8. D8 forbids a post-pass that *improves* the drawing;
this one only undoes disturbance **this edit caused**, to a node this edit
already moved, in the direction of where the human had put it. That is squarely
"repair only what this edit broke", running restoratively.

---

## The edit surface

The AI reaches the network through exactly one function:

```rust
#[frb(sync)]
pub fn ai_edit_network(code: String, replace: bool) -> String   // JSON EditResult
```

surfaced by the `atomcad` skill as `atomcad-cli edit [--replace]`, with the
script passed via `--code` or (recommended) stdin / a heredoc. The CLI REPL has
`edit` and `replace` modes where a block accumulates until a blank line or `.`.

**`code` is a whole multi-statement script, not a single statement.**
`NetworkEditor::apply` runs a first pass creating and updating nodes and
collecting pending connections, a second pass resolving those connections, then
`validate_network`, then — today — `layout_network` **once**
(`ai_assistant_api.rs:~228`).

So the transaction granularity is already right:

> **N statements in one call = one transaction = one layout pass.**

The incremental pass replaces that single `layout_network` call and needs no
batching of its own. The `LayoutSnapshot` for D1 is captured at the top of
`ai_edit_network`, before `NetworkEditor` runs, and the diff is computed after
validation.

| Mode | Flag | Behaviour |
|---|---|---|
| Incremental merge | *(default)* | Statements merge into the existing network. Node ids and positions of untouched nodes survive. |
| Replace | `--replace` | The network is cleared first, then the script is applied. Every node is new. |

**Wires are assigned, not accumulated** (since `1b93cab8`). Mentioning a
property assigns that pin's *whole* inbound wire set — what the statement names
replaces what was there — for array pins as well as scalar ones. So
`shapes: [a, b, c]` shrinks to `shapes: [a]`, and an entirely literal value
(`radius: 5.0`, or `[]` on an array pin) **disconnects** the pin. A property
left out of the statement is untouched.

Before that fix the format could add a wire but not remove one: array pins only
grew, and a literal on a wired pin silently coexisted with the live wire. The
delta below assumes the fixed semantics — wire removal is now a first-class,
routine edit.

---

## The edit delta

```rust
pub struct EditDelta {
    /// Present after, absent before.
    pub added: Vec<u64>,
    /// Present in both, but geometry-relevant state changed.
    pub modified: Vec<u64>,
    /// Present before, absent after.
    pub removed: Vec<u64>,
    /// Wires that did not exist before. Keyed like `WireAnchor` (#427).
    pub added_wires: Vec<WireKey>,
    /// Wires that existed before and do not now. Layout-inert; see below.
    pub removed_wires: Vec<WireKey>,
}
```

Computed by `diff_networks(before: &LayoutSnapshot, after: &NodeNetwork)`, where
`LayoutSnapshot` is a transient pre-edit capture of `{id → (position, size,
wire set)}`. Not persisted. Both wire lists fall out of the same set difference,
so `removed_wires` is free once `added_wires` is computed.

`modified` matters for two reasons only, both geometric:

- **the node got taller** (parameter count changed) and may now overlap a
  neighbour;
- **the node gained a wire** that may now point backwards.

A node whose *value* changed but whose size and wiring did not is not a layout
event at all and is excluded.

### Wire removal is layout-inert

Since `1b93cab8` the text format can remove wires (see
[The edit surface](#the-edit-surface)), so `removed_wires` is routinely
non-empty. It triggers **no repair**, and the reason is worth stating rather
than leaving implicit:

- it **cannot create an overlap** — nothing moves and nothing grows;
- it **cannot create a backward wire** — removing an edge relaxes a constraint,
  it never adds one. Step 5's check exists for wires that *appeared*;
- it can leave a node with no wires at all. That node stays exactly where it is,
  by D3 and by the same reasoning as D4: an orphan sitting in place is a hole,
  and holes are left alone.

So a node whose only change is losing wires is **not** classified `modified`.
`removed_wires` is carried in the delta because the delta should faithfully
describe the edit — and because a large rewiring is exactly the signal a future
"this network was substantially restructured, re-lay it out?" prompt would key
on (`research_intent_preserving_layout.md` §8) — not because layout acts on it.

Two second-order effects, both already handled elsewhere:

- **A comment's wire anchor can dangle.** #427's D6 drops an anchor whose wire
  is gone, in `repair_node_network`. Step 7 then finds `anchors[0]`
  unresolvable and skips the comment, which is the documented behaviour. Worth
  noting only because removing a wire is now easy, so that path fires far more
  often than it did when the format could not express a disconnect.
- **A rewire is one statement, not two.** `diff1 = diff { base: newthing }`
  removes the old `base` wire and adds a new one in a single assignment. The
  removal is inert; the addition goes through Step 5's backward-wire check as
  normal. No special handling for the pair.

### Name-based identity for `replace` mode

In replace mode the network is cleared before the script is applied, so every
node is `added`, every position is lost, and the incremental path degenerates
into exactly the full reflow this design exists to avoid.

**This is not an edge case.** `ai_query_network`'s output is *designed* to be
fed straight back: the `atomcad` skill documents that "the header and footer use
comment syntax (`#`), so the output is valid input to `edit --replace`". A
query → modify → `edit --replace` round-trip is a normal thing for the AI to do,
and today it discards the entire hand layout every time.

The fix is cheap and, importantly, **total**: match rebuilt nodes to snapshot
nodes by name, and carry the old `position` and `hand_moved` across. A matched
node is `kept`, not `added`, and the rest of the algorithm proceeds unchanged.

Names are a reliable key here because **every node has one**. Every
node-creation path in `node_network.rs` sets `custom_name: Some(display_name)`,
and `network_editor.rs:244` states the invariant outright — *"all nodes now have
persistent names assigned at creation time"*. Both sides of the text format
already key on it exclusively: the serializer's `get_node_name` reads
`custom_name` and nothing else, and the editor's `build_existing_name_map`
inserts only nodes that have one. So there is no "unnamed node" case needing a
special rule, and a name that survives a round-trip is exactly a node whose
identity the AI intended to preserve.

Two residual cases, both handled by the same fallback:

- `Node.custom_name` is still typed `Option<String>`, so a legacy `.cnnd`
  predating the invariant could carry `None`;
- the AI may deliberately rename a node, which *is* a new identity.

In both, the node simply fails to match and is treated as `added` — placed by
Steps 2–4 like any other new node. Falling back to "added" is always safe;
matching the wrong node would not be.

This makes the design work for both edit modes at the cost of one
`HashMap<String, u64>` lookup per node.

---

## Algorithm

Seven steps, in order. Each is small.

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

Comments are ordinary obstacles here, at their real `CommentData` size (D9).

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

### Step 6 — New comment nodes

Existing comments need no step at all: they are ordinary nodes throughout
(D9). The only comment-specific rule is the *initial* position of a comment the
edit **created**, which by definition has no prior position to preserve.

- **Anchored** (`note1 = comment { text: "…", on: mybox }` — the text format
  gained `on:` in `design_wire_annotations.md` Phase 2): place it at the first
  collision-free position among the four sides of its anchor's box, using the
  landed `anchor_placement_box` and `surrounding_candidates` helpers, then fall
  back to Step 4 like any other block. Here the four-sides rule is exactly
  right: there is no human intent to override.
- **Unanchored:** it is an anchorless block, and Step 3's "`U` and `D` both
  empty" case already covers it — right of the drawing's bounding box.

Neither case requires touching `place_comments`, which keeps serving the
full-reflow path unchanged.

### Step 7 — Restore drifted comments

A final pass over anchored comments only, in ascending node id order (D10).
Let `anchor_box(positions)` be the landed `anchor_placement_box` resolving
`anchors[0]` — a node's box, or a wire's Bezier midpoint as a zero-size box.

```
for each anchored comment c, by ascending id:
    a_before = anchor_box(pre-edit positions).center()
    a_after  = anchor_box(settled positions).center()

    d_before = |c.center_before − a_before|
    d_after  = |c.center_after  − a_after |
    if d_after <= d_before:            skip     # no drift, or it drifted closer

    target = a_after + (c.pos_before − a_before)   # restores the offset exactly
    if target collides with any node in the settled layout (c excluded):  skip

    c.pos = target
    update the obstacle set
```

Four properties, none of which need arguing about:

- **It never moves a comment away from its anchor.** `target` reproduces the
  original offset, so its distance is exactly `d_before`, and the pass only runs
  when `d_after > d_before`.
- **It never creates an overlap.** A colliding target is abandoned outright.
- **It never touches anything else.** Only comments, only drifted ones, and only
  their own position.
- **It has no constants and cannot loop.** One collision test per drifted
  anchored comment.

An unresolvable `anchors[0]` skips, as everywhere else. A comment whose anchor
did not move has `d_after == d_before` unless the *comment* was pushed, which is
exactly the case worth repairing.

The motivating case is `shift_half_plane`: it splits the drawing at a threshold,
so a comment sitting just left of the line stays while its anchor moves right by
`dx`. The drift is guaranteed, the fix is a translation by `dx`, and the target
is usually free because the region it moves into was vacated by the same shift.

*Possible later refinement, deliberately not specified:* when `target` collides,
sample a few points along the segment from the settled position toward `target`
and take the furthest free one. It recovers partial ground in the cases this
pass currently abandons, at the cost of turning an all-or-nothing rule into one
with a sample count.

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

Every collision test above needs a node's actual box.
`layout/common.rs::node_height` / `node_size` — landed with
`design_wire_annotations.md` Phase 5 — already returns real `CommentData`
dimensions for comments and the `estimate_node_height(params, outputs,
subtitle)` estimate otherwise, so **the comment half of this is done** and this
design uses those functions rather than adding its own.

What remains: **HOF bodies are still unsized.** A node with a `zone` falls
through to the parameter-count estimate and its `body_width` / `body_height` —
which can be many hundreds of pixels — are ignored. A block placed next to a
collapsed-vs-expanded HOF will overlap it. Extending `node_size` to return the
body box for an expanded HOF is a prerequisite of Phase 2, and it improves
`place_comments` on the full-reflow path for free.

---

## Interaction with other subsystems

**Comment nodes (#427, landed).** Existing comments are ordinary nodes on this
path (D9) — obstacles, pushable, shiftable — so they can neither overlap nor be
gratuitously relocated, and `place_comments` is left untouched for the
full-reflow path. Only a comment the edit *created* needs a rule, and that is
[Step 6](#step-6--new-comment-nodes). An anchored comment may drift from its
anchor when the two are pushed differently; [Step 7](#step-7--restore-drifted-comments)
pulls it back when the original spot is free, and the leader line keeps the
association legible when it is not.

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
and undoable. `node_size` extended to return the body box for an expanded HOF.
The `find_connected_components` seed sorted, closing the last cross-process
nondeterminism (D7). `LayoutSnapshot` + `diff_networks` producing an
`EditDelta`, including the `replace`-mode name match, with tests but not yet
wired to anything.

*Tests:* delta correctly classifies add / modify / remove / rewire; a value-only
change produces an empty delta; a statement that shrinks an array pin
(`[a, b]` → `[a]`) yields the dropped wire in `removed_wires` and leaves
`modified` empty; a literal on a wired scalar pin does the same; `hand_moved` round-trips through `.cnnd` and
copy/paste; a pre-flag `.cnnd` loads with `hand_moved = false`; a network with
two equal-size disconnected components lays out byte-identically across two
processes (D7); a `replace`-mode rebuild of an unchanged script matches every
node by name and yields an **empty** delta; a renamed node in a `replace`
rebuild is classified `added`, not matched to its old identity.

### Phase 2 — Block layout and placement
`layout_subgraph`, block decomposition, anchor computation, Step 3 target
position, Step 4a slide. No pushing yet: if the target is occupied, the block
falls below the drawing. `shift_half_plane` for the no-horizontal-room case.

*Tests:* one added node lands beside its input and **no existing node moves**; a
20-node connected addition is laid out internally by Sugiyama and placed as one
block with no existing node moving; an addition with no anchors goes right of
the drawing; a block needing a new column shifts the half-plane and nothing
reorders; an existing comment that nothing collides with stays at its **exact**
original position, on whichever side of its anchor the human put it (D9 — the
four-sides rule must not fire); a newly created `on:`-anchored comment lands
beside its anchor (Step 6).

### Phase 3 — Band push and repair
Step 4b Force-Scan band push with cascade, the `hand_moved` tiebreakers, and
Step 5 (grown node, backward wire).

*Tests:* a block placed into an occupied band displaces the minimum number of
nodes and **never inverts a vertical order**; the cascade terminates on a dense
column; the cascade pushes a node whose x-interval overlaps a *pushed* node but
not `R` (the widening case — the narrow reading of the closure fails this one);
a node that grew pushes its neighbours down and nothing else; a rewire that
points backward shifts the half-plane; a wire that was **already** backward
before the edit is not touched; **removing** a wire moves nothing at all, and a
node left with no wires stays at its exact position; a 400x300 comment in the band is pushed like any
other node, by the minimum amount, and never ends up overlapped (D9);
**Step 7** — a comment left behind by a half-plane shift is pulled back to its
exact original offset; a drifted comment whose original spot is now occupied
stays where the cascade left it; a comment that drifted *closer* to its anchor
is not moved; a comment with an unresolvable anchor is skipped; the pass never
introduces an overlap. Plus a generous guard assertion — no cascade
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
6. **Does `replace` mode get name-matching in v1?** Recommended yes — it is a
   `HashMap` lookup per node and it protects an advertised workflow
   (query → `edit --replace`). The unnamed-node concern that originally made
   this a question turned out not to exist: every node carries a
   `custom_name` from creation.
7. **Should Step 7 fall back to a partial move when the exact target
   collides?** Specified as all-or-nothing. The interpolation refinement is
   noted in Step 7; it recovers more cases but introduces a sample count.
