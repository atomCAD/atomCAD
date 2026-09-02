# Design: incremental layout for AI/human co-editing

**Status:** second draft for review, revised after `doc/design_hof_body_text_format.md`
landed (all five phases). Framework proposed by the maintainer; details filled
in here. Research background: `doc/research_intent_preserving_layout.md`.

**Problem.** `auto_layout_after_edit` defaults to `true` and the default
algorithm is Sugiyama, so every AI text edit re-derives every node position from
the topology alone. A human's arrangement is destroyed on every edit. And since
the AI can now edit *inside* HOF bodies, a body edit that grows the body grows
the owning HOF in its parent network, which today overlaps whatever sits next to
it — there is no layout pass inside bodies at all.

**Approach.** Characterize the edit as a delta (added / modified / removed) per
scope, lay out only the added nodes, fit them into the existing drawing, and
repair locally. Existing nodes move only when something forces them to, and then
by a rigid translation that preserves their relative structure. Scopes are
processed inside-out, so a body settles before its owning HOF's new footprint is
known and repaired in the parent. A full non-incremental Sugiyama stays available
as an explicit user command — the user trades familiarity for algorithmic
optimality when *they* decide to.

---

## Why "just re-run the layout" cannot work

Measured over two hand-drawn corpora: the maintainer's current working file
(`from_mechadense.cnnd` in the repo root; `SPM-tip-with-tool_…cnnd` is a second
save of the same project and agrees with it) and the **demolib** library
(`demolib/baselib_with_demos.cnnd`), the older and much tidier networks —
included because a review of the first draft pointed out, correctly, that a
working file is not representative of a finished one. Top-level networks with
at least two nodes; node boxes estimated at 160×83:

| Property | from_mechadense | demolib |
|---|---|---|
| Networks / nodes / wires | 82 / 2,300 / 2,058 | 65 / 901 / 977 |
| Nodes on a canonical column x (`START_X + col·COLUMN_WIDTH` = `100 + col·210`) | 5 (0.2%) | 3 (0.3%) |
| Distinct x values | 2,266 for 2,300 nodes | 880 for 901 nodes |
| Nodes sharing an **exact** x with another node | 2.6% | 4.7% |
| Nodes within **8 px** of another node's x | **51.8%** | **54.1%** |
| Wires pointing rightward | 91.4% | **99.2%** |
| Wires rightward with a full node width of clearance | 81.7% | 96.9% |
| Backward wires | 177, in 39 networks | **8, in 2 networks** |
| Overlapping node pairs already present | 152 | 21 |

Three consequences that shape this design:

1. **Zero nodes are where the algorithm would put them.** A full reflow rewrites
   essentially 100% of positions in every network of both corpora. There is no
   "mostly already correct" case to exploit.
2. **There is no grid, but there is loose alignment everywhere.** The first
   draft read "almost no column structure" off the exact-x row, which is the
   wrong metric: in *both* corpora about half the nodes sit within a few pixels
   of another node's x. A scheme that snaps the drawing onto a grid is still
   rewriting it — but a scheme that cuts through one of those loose columns,
   moving some members and not others, is destroying structure the human did
   put there. The [window rule](#the-window-rule-where-a-shift-may-cut) exists
   for exactly this.
3. **Rightward flow is a real shared invariant** — 91.4% in the working file
   and 99.2% in the polished one — but not a universal one. 177 backward wires
   and 152 overlaps already exist in a drawing its author is content with. So
   the algorithm must never "fix" pre-existing irregularities — only ones the
   current edit introduced. The polished corpus says the invariant only gets
   stronger as a network matures, which is the case that matters most.

Point 3 generalizes to the rule this design follows throughout:

> **Repair only what this edit broke.** The pre-edit drawing is the baseline,
> however irregular. Its quirks are carried forward untouched.

*(Overlap count is approximate: it uses the same 160×83 estimate the layout code
uses, so it undercounts — comments are 200×100+ and HOF bodies larger. See
[Prerequisite: one size function](#prerequisite-one-size-function).)*

*(The measurement covers **top-level networks only**: the working corpus also
holds 48 HOF nodes whose bodies contain 141 more nodes, and nothing here is
known about how those hand-drawn body layouts look.)*

---

## What changed since the first draft

Three premises of the first draft are no longer true, and the revision below
follows from them.

- **Undo is solved, more simply than assumed.** The AI edit is now one
  whole-network snapshot command (`TextEditNetworkCommand`, pushed by
  `StructureDesigner::ai_text_edit`) that carries every zone. Any layout move in
  any scope rides the after-snapshot for free. The `ScopedMoves` +
  `CompositeCommand` bundling only concerns the GUI reflow path.
- **The name-keyed identity snapshot exists.** The editor's Pass 0 walk
  (`text_format::snapshot_node_positions`) records position by *name path*,
  bodies included, and the AI edit path already diffs it for the history log's
  moved-list. Phase 1 extends that walk rather than building a second snapshot.
- **The current layout path is body-blind in both directions.** After an edit,
  the root scope gets a full Sugiyama that sizes every node at 160×83 in a fixed
  210-wide column, so an expanded HOF already overlaps its neighbours on every
  edit. Body nodes get no layout pass at all: their only placement is the
  creation-time placer (`text_format::auto_layout::calculate_new_node_position`),
  which in an empty body drops the first node at (100, 100) inside a default
  320×180 body. That first node alone exceeds the default height, so **every
  fresh AI-written body grows its HOF immediately.**

---

## Design decisions

**D1 — The unit of work is a diff, not a network.** Layout consumes an
`EditDelta` (added / modified / removed node ids, plus added and removed wires),
computed by comparing a pre-edit snapshot against the post-edit network. This is
possible because `NetworkEditor::apply(code, replace=false)` already merges
incrementally: node ids and positions of untouched nodes survive the edit. There
is **one delta per scope** (D12); each is shaped identically.

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
accepted; the explicit full reflow is the cure. The same holds one scope down: a
body that *shrinks* leaves its HOF's footprint where it was, bounded below by the
stored body size, and nothing in the parent moves inward.

**D5 — `Node.hand_moved: bool`.** Set when a user drags a node, in any scope.
Persisted, undoable, `#[serde(default)]`. Used as a *tiebreaker*, never as a hard
constraint (see [Uses of `hand_moved`](#uses-of-hand_moved)) — a hard "never
move" would make some edits unsatisfiable.

**D6 — Exactly two motion primitives, one per axis, and every situation is a
composition of them.** The **horizontal half-plane shift** (rigid, global) and
the **vertical cascade** (minimal, local). Placing a block, growing a node, and
repairing a backward wire all reduce to these two. The first draft asked for
"one collision primitive"; the landed GUI code has a third, the diagonal
*quadrant shift* (`node_inlining::make_space_for_inline`), and this design
**retires it** on both the AI and the GUI paths. Why two, why these two, and why
the quadrant shift loses, is argued in
[The two motion primitives](#the-two-motion-primitives).

**D7 — Determinism is a requirement, not a nicety.** Stability is untestable
without it: "did this edit move anything?" has no answer if two runs of the same
input disagree. Every iteration over nodes in new code is over ids sorted
ascending; every iteration over scopes is by depth, then by name path.

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

The measured cascade figures in [Step 5](#step-5--fit-the-block-in) already
reflect this rule: that simulation used real comment dimensions and treated
comments as ordinary obstacles.

**D10 — A drifted comment is pulled back to its anchor, if the spot is free.**
D9 lets an anchored comment and its anchor be displaced differently, which
stretches the leader line. A final pass (see [Step 8](#step-8--restore-drifted-comments))
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

**D11 — Growth is two-dimensional, and `grow_rect` is its one operation.** A
node whose rendered footprint grew — an HOF whose body got bigger, an `expr`
that gained a pin, an Auto-mode HOF whose `f` wire was removed — grows **right
and down from a fixed top-left**. The width delta is absorbed by a horizontal
half-plane shift at the node's old right edge (or left of it, per D15); the
height delta by a vertical
cascade over the new rect. One routine, `grow_rect`, does both in that order,
and it is the *only* way a grown node makes room, on every path
(see [`grow_rect`](#grow_rect)).

**D12 — Scopes are processed inside-out, and a settled body reports its HOF's
growth to the parent as a `grown` entry.** A parent's node sizes are not known
until its children's bodies have settled, because an expanded HOF's footprint
*is* a function of its body's content bbox. So: deepest scope first; when a body
settles, its owning HOF's footprint is re-measured and, if it exceeds the
pre-edit footprint, the HOF joins the parent delta's `modified` set with that
delta vector. See [Scopes: inside-out](#scopes-inside-out).

**D13 — `modified` means "footprint grew, measured", not "parameter count
changed".** The delta classifies a kept node as modified by comparing its
pre-edit and post-edit rendered footprint from the one size function. That
single rule subsumes every growth trigger — added pins, body growth, the
collapse flip when a text edit unwires `f:` on an Auto-mode HOF — and needs no
per-trigger detection.

**D14 — The identity snapshot carries the node's layout state, not just its
position.** The text format has no `body_width`, `body_height` or
`collapse_mode`, so a `--replace` round-trip today resets every HOF to the
320×180 default body and to `Auto`. A user-collapsed HOF re-expands, which is
exactly the footprint jump D11 then has to absorb in the parent. The Pass 0
snapshot therefore records, per name path, **position, rendered footprint,
`body_width`, `body_height`, `collapse_mode` and `hand_moved`**, and the editor
re-applies all of them on a name match — an extension of
`design_hof_body_text_format.md` D8, listed there as carrying "`position` and
nothing else" precisely so that this design could add the rest.

**D15 — A half-plane shift cuts inside a window, and snaps into whitespace
within it.** Every shift has nodes that must move and nodes that must not, and
those bound where the line may fall. Within that window the threshold moves
**left only**, to the nearest x that cuts no loose column and no node box; an
empty window means the shift is skipped. See
[The window rule](#the-window-rule-where-a-shift-may-cut).

---

## The edit surface

The AI reaches the network through exactly one function:

```rust
#[frb(sync)]
pub fn ai_edit_network(code: String, replace: bool) -> String   // JSON EditResult
```

which is a thin wrapper over `StructureDesigner::ai_text_edit`
(`crates/atomcad-structure-designer/src/ai_text_edit.rs`), the choke point
every transport funnels through — the HTTP `/edit` handler, the CLI REPL's
`edit` / `replace` modes, the `atomcad` skill's `atomcad-cli edit [--replace]`.

**`code` is a whole multi-statement script, not a single statement.**
`ai_text_edit` takes the identity snapshot, runs `text_edit_network` (Pass 0–3,
scope-aware), reinserts the network, validates, and then — today — calls
`layout::layout_network` **once** on the root scope. Then it takes the log's
after-pair and pushes the whole-network undo snapshot.

So the transaction granularity is already right:

> **N statements in one call = one transaction = one layout pass.**

The incremental pass replaces that single `layout_network` call and needs no
batching of its own. The snapshot for D1 is the one already captured at the top
of `ai_text_edit` (extended per D14), and the per-scope diffs are computed after
validation.

| Mode | Flag | Behaviour |
|---|---|---|
| Incremental merge | *(default)* | Statements merge into the existing network. Node ids and positions of untouched nodes survive. |
| Replace | `--replace` | The network is cleared first, then the script is applied. Every node is rebuilt, and matched to its old identity by name (D14). |

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

**Bodies are on the surface too.** A `body { … }` block assigns a whole body
(name-matched, so surviving nodes keep id and position); `m1/x = …`,
`output m1/x` and `delete m1/x` address one node inside one. One script can
therefore touch the root scope and several bodies at once, which is why the
delta is per scope (D12).

---

## The edit delta

```rust
pub struct EditDelta {
    /// Present after, absent before.
    pub added: Vec<u64>,
    /// Present in both, and the rendered footprint grew in either axis (D13).
    /// Carries the pre-edit and post-edit footprint.
    pub modified: Vec<(u64, DVec2, DVec2)>,
    /// Present before, absent after.
    pub removed: Vec<u64>,
    /// Wires that did not exist before. Keyed like `WireAnchor` (#427).
    pub added_wires: Vec<WireKey>,
    /// Wires that existed before and do not now. Layout-inert; see below.
    pub removed_wires: Vec<WireKey>,
}
```

One `EditDelta` per scope, computed by
`diff_scope(before: &LayoutSnapshot, scope: &[u64], after: &NodeNetwork)`.
`LayoutSnapshot` is the Pass 0 walk of D14: `(name path) → {position,
footprint, body_width, body_height, collapse_mode, hand_moved}`, transient, not
persisted. Diffing is **by name path**, never by id or id-scope-path: a
`--replace` mints fresh ids for every node, so the scope path `[m1_id]` before
and after the edit are different numbers naming the same body. Ids are resolved
from names *after* the match, for applying moves. Both wire lists fall out of
the same set difference, so `removed_wires` is free once `added_wires` is
computed.

`modified` matters for one reason only, and it is geometric: **the node's box
got bigger** and may now overlap a neighbour. Growth is measured (D13), so the
delta does not need to know *why* — it is the same entry whether an `expr`
gained a parameter, a `map`'s body acquired a node, or an Auto-mode HOF lost its
`f` wire and flipped to expanded.

A node whose *value* changed but whose footprint did not is not a layout event
at all and is excluded. A node that **shrank** is excluded too (D4).

### Wire removal is layout-inert

Since `1b93cab8` the text format can remove wires (see
[The edit surface](#the-edit-surface)), so `removed_wires` is routinely
non-empty. It triggers **no repair**, and the reason is worth stating rather
than leaving implicit:

- it **cannot create an overlap** — nothing moves and nothing grows;
- it **cannot create a backward wire** — removing an edge relaxes a constraint,
  it never adds one. Step 6's check exists for wires that *appeared*;
- it can leave a node with no wires at all. That node stays exactly where it is,
  by D3 and by the same reasoning as D4: an orphan sitting in place is a hole,
  and holes are left alone.

So a node whose only change is losing wires is **not** classified `modified`.
`removed_wires` is carried in the delta because the delta should faithfully
describe the edit — and because a large rewiring is exactly the signal a future
"this network was substantially restructured, re-lay it out?" prompt would key
on (`research_intent_preserving_layout.md` §8) — not because layout acts on it.

The one exception is indirect and already covered by D13: removing the `f` wire
of an Auto-mode HOF flips it from compact to expanded. That is a footprint
change, the footprint comparison catches it, and the HOF lands in `modified`
like any other grown node. Nothing keys on the wire itself.

Two second-order effects, both already handled elsewhere:

- **A comment's wire anchor can dangle.** #427's D6 drops an anchor whose wire
  is gone, in `repair_node_network`. Step 8 then finds `anchors[0]`
  unresolvable and skips the comment, which is the documented behaviour. Worth
  noting only because removing a wire is now easy, so that path fires far more
  often than it did when the format could not express a disconnect.
- **A rewire is one statement, not two.** `diff1 = diff { base: newthing }`
  removes the old `base` wire and adds a new one in a single assignment. The
  removal is inert; the addition goes through Step 6's backward-wire check as
  normal. No special handling for the pair.

### Wires that cross a scope boundary

Inside a body three wire kinds have no source node in the body's own network: a
zone input (`$element`), a capture (`^name`), and an outer HOF's zone input
(`^$element`). The zone output (`output x` inside the block) has no destination
node there either. All four appear in the delta as ordinary `WireKey`s — they
are needed for **anchors** (Step 4) — but none is ever **repaired**: Step 6's
backward-wire check applies to same-scope wires only. A capture that "points
backwards" is left alone; `layout/common.rs::wire_midpoint` already treats the
destination pin as standing in for the missing source.

### Name-based identity for `replace` mode

In replace mode the network is cleared before the script is applied, so without
a match every node would be `added`, every position lost, and the incremental
path would degenerate into exactly the full reflow this design exists to avoid.

**This is not an edge case.** `ai_query_network`'s output is *designed* to be
fed straight back: the `atomcad` skill documents that "the header and footer use
comment syntax (`#`), so the output is valid input to `edit --replace`". A
query → modify → `edit --replace` round-trip is a normal thing for the AI to do.

The match **already exists**, built by `design_hof_body_text_format.md` D8: the
editor's Pass 0 snapshot is taken ahead of `clear_network`, keyed `(name path)`,
and a rebuilt node whose name path matches inherits the old position. A matched
node is `kept`, not `added`, and the rest of the algorithm proceeds unchanged.
What this design adds is D14: the same match also carries the footprint, the
stored body size, the collapse mode and `hand_moved`, so a round-trip is
layout-neutral for HOFs as well.

Names are a reliable key here because **every node has one**. Every
node-creation path in `node_network.rs` sets `custom_name: Some(display_name)`,
and `network_editor.rs` states the invariant outright — *"all nodes now have
persistent names assigned at creation time"*. Both sides of the text format
already key on it exclusively.

Two residual cases, both handled by the same fallback:

- `Node.custom_name` is still typed `Option<String>`, so a legacy `.cnnd`
  predating the invariant could carry `None`;
- the AI may deliberately rename a node, which *is* a new identity.

In both, the node simply fails to match and is treated as `added` — placed by
Steps 3–5 like any other new node. Falling back to "added" is always safe;
matching the wrong node would not be.

---

## Algorithm

Eight steps per scope, in order. Each is small. The order matters more than it
did in the first draft: **grown nodes are repaired before new blocks are
placed**, so that a block sees the settled obstacles. The other way round, a
block placed against an HOF's old size is pushed by that HOF's growth a moment
later — motion the block did not need.

### Step 1 — Remove

Delete the `removed` nodes and their wires. Do nothing else (D4).

### Step 2 — Repair grown nodes

For every entry `(id, old_size, new_size)` in `modified`, in ascending id:
`grow_rect(scope, id, old_size, new_size)`. Positions are re-read between
entries, so a second grown HOF that the first one's shift moved is repaired at
its new position. The grown node itself never moves (D3 — its top-left is the
anchor).

### Step 3 — Lay out the added nodes as blocks

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

Two things the existing pipeline must learn first, both from
[Prerequisite: one size function](#prerequisite-one-size-function): node
heights come from the unified size, and **column width is per layer**, the
widest node in the layer plus the gap, instead of the fixed `COLUMN_WIDTH`.
Without both, a new `map` beside a new `int` in the same block overlap. An added
HOF's size here is its **settled** footprint, because its body was laid out
before its parent (D12).

The result is a set of local positions per block; take its bounding box
`(W, H)`.

Blocks are processed in a deterministic order: by `(min anchor x, min node id)`
— see Step 4 for anchors. Each block, once placed, joins the obstacle set for
the next.

### Step 4 — Choose each block's target position

Define the block's **anchors** — kept things wired to it:

- `U` = kept nodes with a wire *into* the block (upstream);
- `D` = kept nodes fed *by* the block (downstream).

**Inside a body, the scope's own edges are anchors too.** Zone inputs, captures
and the zone output are the dominant wires in a body — the corpus mean body is
2.9 nodes, and nearly every one reads `$element` and ends in `output`. They have
no node position, and under the rule above a typical body block would have no
anchors, land right of the body's bounding box, and grow the HOF every time. So:

| Wire kind | Anchor synthesized |
|---|---|
| `$name` (zone input, any depth) and `^name` (capture) | a zero-width box at the body's **left edge** (`x = 0`), at the pin's rendered y — `FIRST_PIN_OFFSET + index · PER_PARAM_HEIGHT` from the body top for zone inputs, the body top for captures |
| the body's `output` (zone output) | a zero-width box at the body's **right edge** (`x = body_width`), at the output pin's y |

A `$element → mul → output` body then lays out left to right on its own.
Everything below applies unchanged with these boxes in `U` and `D`.

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
| `x_min > x_max` | **no room**: `shift_half_plane(x_min, x_min - x_max)` first, then `x = x_min` — with the threshold snapped per the [window rule](#the-window-rule-where-a-shift-may-cut), whose window here is `(max over U of u.x, min over D of d.x]` |

Two things can go wrong with the threshold here, and the window rule handles
both. **A consumer can sit left of `x_min`** — its left edge overlapping an
input horizontally, in another row. A shift at `x_min` would then miss that
consumer and displace everything right of `x_min` for nothing, which is what
the first draft did; the window's right end is `min d.x`, so the threshold is
clamped there and the consumer moves. **A consumer can sit at or left of an
input's left edge.** Then the window is empty: no threshold can move every
consumer without also moving an input, and moving an input moves `x_min` with
it, so the room is never created. Instead: no shift, `x = x_min`, Step 5
resolves any overlap, and the new wire to that consumer points backward. The
consumer being left of the input is a pre-existing arrangement, and the
baseline rule leaves those alone.

**Vertical.** Rather than centring the block, align it by its connections. For
every wire between a block-internal node `b` and an anchor `a`, the ideal offset
is `a.y_center - b.y_center_local`. Take the mean:

```
y_offset = mean over external wires of (anchor.y_center - internal.y_center_local)
```

This naturally places a block feeding one node level with that node, and a block
straddling two anchors between them. With no anchors, place below the drawing's
bbox. In an **empty body** there is no bbox: the block goes at the body's left
padding, level with the first zone-input pin.

**Body coordinates are non-negative.** A body's content extent is measured from
its origin (`rendered_body_size`, and Flutter's `_computeBodySize`), so a node at
a negative position is invisible to the size computation and renders clipped.
Every candidate position in a body is clamped to `≥ 0` on both axes; the "no
room" shift above is the one thing that can push content past the right edge,
and that is growth the HOF absorbs (D12).

### Step 5 — Fit the block in

The block now has a target rect `R = (x, y_offset, W, H)` inflated by `GAP`.
Two mechanisms, tried in order:

**(5a) Slide the block.** Search for a free `y` near the target — alternating
down and up in increments of `VERTICAL_GAP` — within a bounded window
(`SLIDE_WINDOW`, suggested: two node heights). Nothing existing moves. If a free
`y` is found, done.

**Inside a body, growth is not free, so the slide is slack-first.** At top level
"right of the bbox" or "below the bbox" costs nothing; in a body every pixel
past the stored `body_width` / `body_height` grows the HOF and cascades into the
parent. So the slide first searches candidates whose rect stays inside the
stored body size minus its padding, over the *whole* body height rather than
`SLIDE_WINDOW`, and only when none is free falls back to the ordinary window
and accepts the growth. This is the one placement rule a body has that the top
level does not.

Comments are ordinary obstacles here, at their real `CommentData` size (D9).

**(5b) Push the wavefront open.** Otherwise place the block at its target `y`
and run the [vertical cascade](#the-vertical-cascade) over `R`, choosing the
direction per node by which side of `R`'s centre it lies on.

Measured by simulation on both corpora (`scripts/layout_cascade_sim.py`) —
dropping a node-sized rect, inflated by the gap, exactly onto each existing
node's position with that node removed from the obstacle set, so the drop lands
in the densest spot the node's neighbours allow; per-node estimated heights,
real comment dimensions, top-level networks only:

| | from_mechadense (2,300 drops) | demolib (901 drops) |
|---|---|---|
| Cascades pushing **nothing** | 50.4% | **65.0%** |
| Pushing ≤ 1 node | 77.4% | 85.5% |
| Pushing ≤ 5 nodes | 97.7% | 99.1% |
| Max nodes pushed | 11 | 10 |
| Final x-extent ÷ `R` width, median / p90 / p99 / max | 1.00× / 1.74× / 2.89× / 5.04× | 1.00× / **1.01×** / 1.84× / 2.48× |

The polished corpus is the easier one on every row: two drops in three
disturb nothing, and nine in ten widen the disturbed region by nothing at all.
The 5× worst cases are a few hundred pixels of extent in networks a few
thousand pixels wide. All of these figures overstate the real cost, because
(5a) runs first and a dead-centre drop is the densest possible start.

*(The first draft quoted 50.9% / 77.0% / 98.1% / max 10 for the working corpus
from a script that was not kept; the numbers above come from a
reimplementation of the pseudocode, calibrated to reproduce those within a
point, and the script is now in the repo so the measurement can be repeated.)*

`SLIDE_WINDOW` is the design's one real tuning constant. It sets the trade
between long wires (slide too far) and disturbed neighbours (push too eagerly).

### Step 6 — Repair new backward wires

For each wire in `added_wires` whose endpoints are both kept and both in this
scope, if `source.x + width(source) + GAP > dest.x`, then
`shift_half_plane(dest.x, deficit)`, threshold snapped within the window
`(source.x, dest.x]` per the [window rule](#the-window-rule-where-a-shift-may-cut).
Wires that were already backward before the
edit are left alone — that is the baseline rule, and the measurement says 177 of
them exist in the working corpus (8 in demolib). Cross-scope wires are skipped (see
[Wires that cross a scope boundary](#wires-that-cross-a-scope-boundary)).

### Step 7 — New comment nodes

Existing comments need no step at all: they are ordinary nodes throughout
(D9). The only comment-specific rule is the *initial* position of a comment the
edit **created**, which by definition has no prior position to preserve.

- **Anchored** (`note1 = comment { text: "…", on: mybox }` — the text format
  gained `on:` in `design_wire_annotations.md` Phase 2): place it at the first
  collision-free position among the four sides of its anchor's box, using the
  landed `anchor_placement_box` and `surrounding_candidates` helpers, then fall
  back to Step 5 like any other block. Here the four-sides rule is exactly
  right: there is no human intent to override.
- **Unanchored:** it is an anchorless block, and Step 4's "`U` and `D` both
  empty" case already covers it — right of the drawing's bounding box.

Neither case requires touching `place_comments`, which keeps serving the
full-reflow path unchanged.

### Step 8 — Restore drifted comments

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

---

## The two motion primitives

Every movement of an existing node in this design is one of two operations.
Both preserve order along their axis, and neither can create an overlap. They
differ in what else they preserve and in how much they disturb.

### `shift_half_plane`

```rust
/// Translate every node with `x >= x_threshold` right by `dx`.
fn shift_half_plane(network: &mut NodeNetwork, x_threshold: f64, dx: f64);
```

A rigid translation of a half-plane: **global, dumb, safe**. Every node past the
line moves by the same amount whether or not anything collided. It cannot
introduce an overlap — a moved node only moves away from every unmoved one, and
moved nodes keep their relative positions — it cannot reorder anything, and it
preserves every alignment and every deliberate gap within the moved set and
within the unmoved set. It makes the drawing wider, which is the honest cost of
inserting something. Crucially, **it can never turn a forward wire backward**:
every wire crossing the line left-to-right only gets longer, and wires on either
side keep their length.

Used in three places: Step 2 (a node grew wider, via `grow_rect`), Step 4 (no
horizontal room for a block) and Step 6 (a rewire made a wire point backwards).

#### The window rule: where a shift may cut

A shift is safe at *any* threshold — but not equally good at any threshold.
The measurement says about half of all nodes sit within a few pixels of
another node's x: loose columns, not a grid. A line dropped at an arbitrary x
cuts through such a column, moving some of its members and not others, and
that is the one disturbance a rigid shift cannot excuse — the moved and unmoved
sets were one visual group. So the threshold is not taken as given; it is
chosen.

Every shift site names the nodes that **must move** and the nodes that **must
not**, and those two sets bound the line:

| Shift site | Must not move | Must move | Window for `T` |
|---|---|---|---|
| Step 4, no room for a block | every upstream anchor `u` | every downstream anchor `d` | `(max u.x, min d.x]` |
| Step 2, `grow_rect` width delta | the grown node | everything right of its old right edge, in its band | `(node.x, old right edge]` |
| Step 6, new backward wire | the source | the destination | `(source.x, dest.x]` |

The rule, applied identically at all three:

1. **Start at the site's default threshold, clamped to the window's right
   end.** The old right edge and `dest.x` *are* their windows' right ends.
   `x_min` usually lies inside its window but can lie right of it, when a
   consumer's left edge overlaps an input horizontally; clamping to `min d.x`
   is what makes that consumer move (see Step 4).
2. **Snap left, never right.** Move `T` down to the nearest x at which no
   node's left edge lies within the alignment tolerance (8 px, the same figure
   the measurement used) and no node's box straddles the line. Left, because
   moving more nodes rightward stays overlap-free and every node that had to
   move still does; never right, because that would leave a required node
   unmoved. Give up and keep the default if no such gap exists within one node
   width.
3. **Never leave the window.** The lower bound is strict: a threshold at or
   below an upstream anchor moves that anchor, and with it `x_min`, so the
   room is never created. The same for the grown node and for the source.
4. **An empty window means no shift.** Some consumer's left edge is at or
   left of some input's left edge (Step 4), or the destination is at or left
   of the source (Step 6, in which case the wire was never forward and Step 6
   does not fire anyway). Nothing can satisfy both sets; skip the shift and
   let Step 5 handle overlap.

Why the default is the *left* end of the useful range rather than the
minimum-motion choice: in Step 4 one could shift at `min d.x` and move fewer
nodes, but the nodes in the strip between `x_min` and the consumers would then
collide with the block and be pushed *vertically* by the cascade. Those nodes
are horizontally inside the column that is widening; moving them right with the
consumers is the "insert a column" a human would do, while pushing them down
splits them from their row. `x_min` vacates the strip the block will occupy,
so the block cascades nothing. Snapping only ever moves the line further left,
into whitespace, within the window.

*Refinement, not for v1:* shift only the destination's **downstream cone**
instead of the whole half-plane. More surgical, but a translated cone can
collide with non-cone nodes, so it needs the cascade afterwards. The half-plane
version is unconditionally safe.

### The vertical cascade

```
cascade(R, dir_of):
    W ← every kept node overlapping R in BOTH axes,
        ordered by |node.y_center − R.y_center|
    for each n in W:  dir(n) ← dir_of(n)          # up / down

    while W not empty:
        n ← pop W
        push n by the minimum amount along dir(n) to clear its blocker
            (R on the first round; otherwise the node that enqueued it)
        for every node m in the WHOLE network — not merely the initial set —
            with x-overlap(n, m) and y-overlap(n, m) and m further along dir(n):
                dir(m) ← dir(n); push m to clear n; enqueue m
```

A vertical Force-Scan: **local, minimal, safe**. Only nodes that actually
collide move, by the least amount that clears the collision, and the push
propagates only through further actual collisions. Step 5 chooses `dir_of` by
which side of `R`'s centre a node lies on; `grow_rect` forces `down`.

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
cascade could reach everything; in practice it runs out of collisions first
(the Step 5 figures: half to two thirds of the simulated drops push nothing).

**No cap is imposed on the cascade.** A limit would be a tuning constant with no
evidence behind it, and the fallback it would need (abandon the push, slide the
block arbitrarily far) is not obviously better than a wide push. Phase 4 asserts
a generous bound in tests instead, to catch a regression rather than to shape
behaviour.

### `grow_rect`

```rust
/// `node_id`'s rendered footprint grew from `old` to `new`, top-left fixed.
/// Make room for it. Returns the moves for the caller's undo bookkeeping.
fn grow_rect(network, registry, node_id, old: DVec2, new: DVec2) -> Vec<(u64, DVec2, DVec2)> {
    let anchor = node.position;
    let dw = max(0, new.x - old.x);
    let dh = max(0, new.y - old.y);
    if dw > 0 { shift_half_plane(snap(anchor.x + old.x,          // 1. width: old right edge,
                                      window = (anchor.x, anchor.x + old.x]), dw); } //    snapped left (D15)
    if dh > 0 { cascade(R = (anchor, new) inflated by GAP,       // 2. height: new rect
                        dir_of = |_| down,
                        excluding node_id and any node that already overlapped (anchor, old)); }
    diff positions
}
```

Horizontal first, so the cascade sees post-shift positions. The grown node's own
x is left of the threshold, so it is naturally exempt from the shift. A node
that already overlapped the *old* rect is a pre-existing irregularity and is
left alone — the baseline rule — which is also what makes forcing `down`
correct: after the shift, every *new* collision with the new rect lies below the
old bottom edge.

### Why one primitive per axis, and why these two

The obvious alternative is the landed quadrant shift
(`node_inlining::make_space_for_inline`): classify every node by its top-left
corner against the growing node's box and shift the lower-right quadrant
diagonally, the right band right, the lower band down. It looks like the
symmetric choice. It is not one operation — gate a horizontal half-plane shift
at the old right edge and a vertical one at the old bottom edge and the four
regions fall out — so the real question is **half-plane or cascade, per axis**,
and the drawing decides each axis differently.

- **x carries a directional invariant, y carries none.** Nine wires in ten
  point right in the working corpus, all but a handful in the polished one,
  and that is the property the design protects. A horizontal
  *cascade* can break it: the grown node pushes B right, B feeds C in a row that
  did not collide, C stays, and the B→C wire flips backward. Step 6 would then
  fix that with a half-plane shift anyway, so a horizontal cascade is a
  half-plane shift done in two steps with extra motion. A horizontal half-plane
  shift provably never flips a forward wire. Vertically there is no direction to
  protect, so the minimal operation is free to be minimal.
- **Width growth moves a pin, height growth does not.** A body grows from a
  fixed top-left. Growing wider moves the HOF's output pin right by `dw`, and
  everything downstream is wired to that pin; shifting all of it by exactly `dw`
  keeps every downstream wire the length the human chose. Growing taller moves
  no pin at all. Nodes below have no wire reason to move, only a collision
  reason, so the operation that moves them only on collision is the right one.
- **The corpus says a horizontal cascade would not be local anyway.** With
  nearly one distinct x per node the x-overlap graph is one connected component
  across the whole network, so a horizontal cascade would propagate through most
  of the right-hand side — raggedly, row by row. The half-plane shift is the
  rigid version of what the cascade would approximately do. Vertically the
  opposite holds: half to two thirds of the simulated insertions pushed nothing
  and nearly all pushed at most five nodes, because vertical whitespace stops a
  cascade almost immediately. A vertical half-plane shift would instead move every independent
  pipeline stacked below the grown node, for no benefit, and the AI edit history
  would list hundreds of moved nodes for a body that grew by thirty pixels.

So the combination is not less principled than the quadrant shift; it is more.
It uses the rigid global operation exactly where the drawing has global
structure to protect, and the minimal local operation exactly where it does
not. The quadrant shift applies the global operation on both axes because it
was written without node sizes and without the rightward invariant in mind.

Two concrete defects of the quadrant shift follow from that origin, and both
are reasons to **retire** it rather than keep it as the GUI's primitive:

- **Its vertical half violates D3.** It moves every node below the growing
  node's top and right of its left edge by `dh`, colliding or not.
- **Its middle band can create overlaps.** Because it is size-blind, it gates
  on the *near* corner and splits the overlap band by a cross product against
  the node's diagonal. Two neighbours straddling that diagonal go in orthogonal
  directions by different amounts; when `dw` is large and `dh` is small, the
  right-moved one lands on the down-moved one. Two plain half-plane shifts
  cannot do that, and neither can `grow_rect`. Once the size function is
  unified the heuristic has no reason to exist.

**Retirement plan.** `make_space_for_inline` has three call sites, all in
`structure_designer.rs`: the GUI reflow cascade
(`reflow_for_footprint_change`), `inline_custom_node`, and
`convert_instance_to_closure`. All three become `grow_rect` calls with the same
`(anchor, old_size, new_size)` arguments — for inlining, `old` is the instance's
footprint and `new` the content's bounding box, exactly as today. The scope
cascade in `reflow_for_footprint_change`, its `capture_footprint_chain`
contract and its `ScopedMoves` / `CompositeCommand` undo bundling are untouched;
only the spatial step inside the loop changes. `design_reflow_on_footprint_change.md`'s
spatial half is thereby superseded; its undo half stands. Existing
`reflow_test.rs` expectations change where they asserted that a *non-colliding*
node below the grown HOF moved — under `grow_rect` it does not.

---

## Scopes: inside-out

A body is a full `NodeNetwork` with its own coordinate system, so the algorithm
above runs on it unchanged. What is new is the **driver** that orders the
scopes and connects a body's outcome to its parent's delta.

```
snapshot ← Pass 0 walk, before the edit (D14):
    (name path) → { position, footprint, body_width, body_height, collapse_mode, hand_moved }
    for every node at every depth

apply the edit; validate

scopes ← every scope in the post-edit network, deepest first, ties by name path
grown  ← ∅                                     # (parent scope, hof id, old, new)

for scope in scopes:
    delta ← diff_scope(snapshot, scope)        # by name path; ids resolved after
    delta.modified ∪= grown entries whose parent scope is this one
    run Steps 1–8 on scope

    if scope is a body owned by hof H in parent P:
        new ← rendered footprint of H            # settled: the body just laid out
        old ← snapshot[H].footprint              # absent ⇒ H is new ⇒ already in P.added
        if old exists and new exceeds old in either axis:
            grown ∪= (P, H, old, new)
```

Points that are easy to get wrong:

- **A parent's delta cannot be finalized until its children have settled.**
  That is the whole reason for the ordering, and why `modified` is completed
  inside the loop rather than computed up front. The footprint measurement is
  the recursive `rendered_body_size` rule (`max(stored, content + padding)`,
  nested HOFs included), which is correct as long as it is called *after* the
  inner scopes are done.
- **A new HOF is `added` in its parent, at its settled size.** Its body nodes
  are `added` in the body scope, laid out first against synthesized anchors
  (Step 4); then the HOF's footprint is known; then Step 3 in the parent sizes
  the block correctly. Nothing about the HOF is special beyond its size.
- **Several bodies under one parent, several grown HOFs.** Step 2 repairs them
  in ascending id, re-reading positions between them (D7).
- **Growth at depth ≥ 2 cascades naturally.** The grandparent sees the parent
  HOF's `grown` entry only after the parent scope — itself a body — has run
  Steps 1–8, including the Step 2 repair that may have widened its content.
  Nothing climbs more than one level per iteration, and nothing needs the
  GUI's pre-captured `old_sizes` chain: the snapshot holds every node's old
  footprint already.
- **Shrinking is a hole** (D4). The stored body size is a floor, so an HOF never
  shrinks below what the user sized it to, and the parent never compacts.
- **The creation-time placer is superseded.** Until Phase 5 wires this pass in,
  `calculate_new_node_position` is the only placement a body node gets, and it
  sizes nodes by type name alone. After Phase 5 its output is a throwaway
  initial position, overwritten by Step 3. It is not worth improving in
  between.

---

## Uses of `hand_moved`

The flag is a tiebreaker in three places. None of them is a hard constraint —
a node that must move to keep the drawing correct still moves.

1. **Cascade direction (Step 5b).** When both up and down clear the block, push
   the direction that displaces fewer hand-moved nodes; on a tie, fewer nodes;
   on a tie, the smaller total displacement; on a tie, up.
2. **Slide vs. push (5a→5b).** Extend `SLIDE_WINDOW` when every obstacle in the
   band is hand-moved — prefer to route the new block around a deliberately
   arranged region rather than through it.
3. **Explicit full reflow.** A "respect manually placed nodes" option on the
   Auto-Layout command treats hand-moved nodes as fixed. Off by default: the
   whole point of the explicit command is to get the algorithmic result.

The flag is set in the drag handler (Flutter → API → `Node.hand_moved = true`),
in whatever scope the dragged node lives, persisted in `.cnnd`, and carried
through copy/paste and the name-match path (D14).

**Open:** whether a full reflow *clears* the flags on the nodes it moved. It
should — after the algorithm has placed a node, "a human placed this" is false —
but that makes the reflow destroy intent in a second, less obvious way. See
[Open questions](#open-questions).

---

## Prerequisite: one size function

Every collision test, every anchor, every footprint comparison and every
block bounding box above needs a node's actual box. There are **four** size
functions today, and no two agree:

| Function | Comments | Expanded HOF bodies |
|---|---|---|
| `layout/common.rs::node_size` / `node_height` (Sugiyama, `place_comments`) | real `CommentData` dimensions | **ignored** — 160×83 |
| `node_inlining::estimate_node_size_in_network` / `rendered_body_size` (inlining, GUI reflow) | pin estimate — **wrong** | correct: `resolve_body_collapsed`, `max(stored, content + padding)`, recursive |
| `text_format/auto_layout::get_node_size` (creation-time placer) | by type name — wrong | by type name — wrong |
| Flutter `scope_resolver.dart::_computeBodySize` | ground truth | ground truth |

Two divergent size functions is how a collision test and a reflow disagree
about the same node; four is why no two subsystems agree on whether two nodes
overlap at all. The prerequisite is to **unify into one**
`rendered_node_size(node, registry)` — the inlining rule for bodies, the layout
rule for comments, recursive into nested HOFs — and route all four callers
through it. A test fixture pins its output for a comment, an expanded HOF, a
collapsed HOF, a closure and a two-level nested HOF against the Flutter rule.

It is a **Phase 1** prerequisite, not Phase 2 as the first draft had it: D13's
`modified` is a footprint comparison, and D12 reports a body's growth to its
parent as a footprint, so the delta is meaningless without it.

Two consequences for the full-reflow path, also Phase 1: Sugiyama's column
width becomes **per layer** (the widest node in the layer plus the gap) rather
than the fixed 210, since an expanded HOF is 320 px wide by default; and
`place_comments` gets the body-aware obstacle sizes for free.

Note also that a stored `body_width` / `body_height` is a floor, not the size —
the rendered body is `max(stored, content + padding)`. Layout should let the max
take over rather than writing the stored values, which are a user choice.

---

## Interaction with other subsystems

**Comment nodes (#427, landed).** Existing comments are ordinary nodes on this
path (D9) — obstacles, pushable, shiftable — so they can neither overlap nor be
gratuitously relocated, and `place_comments` is left untouched for the
full-reflow path. Only a comment the edit *created* needs a rule, and that is
[Step 7](#step-7--new-comment-nodes). An anchored comment may drift from its
anchor when the two are pushed differently; [Step 8](#step-8--restore-drifted-comments)
pulls it back when the original spot is free, and the leader line keeps the
association legible when it is not.

**HOF bodies in the text format (`design_hof_body_text_format.md`, landed).**
That design built the per-scope name match and the pre-clear position snapshot
(its D8) and deliberately carried "`position` and nothing else". This design
consumes both and extends the snapshot (D14). A body edit produces a body-scope
`EditDelta` shaped exactly like a top-level one, which is what lets the
algorithm run inside bodies with no new machinery beyond the driver in
[Scopes: inside-out](#scopes-inside-out) and the two body-only rules
(synthesized anchors in Step 4, slack-first slide in Step 5a).

**Reflow on footprint growth (`design_reflow_on_footprint_change.md`, landed).**
Its **undo half stands**: `reflow_for_footprint_change`'s walk up the scope
chain, `capture_footprint_chain`, `ScopedMoves`, `CompositeCommand` and the
per-case bundling for GUI cases A (f-disconnect), B (`set_collapse_mode`) and C
(in-body add / paste / duplicate / connect). Its **spatial half is superseded**:
the quadrant shift it built on is replaced by `grow_rect` at all three call
sites, per the retirement plan above. The GUI path and the AI path then agree
on what "this body grew" does to the parent.

**Full reflow (the explicit user command).** Stays Sugiyama, stays user-invoked,
but becomes body-aware in Phase 5: per-layer column widths from the unified
size, and the same inside-out recursion — lay out the body, size the HOF, lay
out the parent. Without that, D4's "the reflow is the cure" is false for any
network with an expanded HOF, which today's reflow overlaps with its right
neighbour every time.

**AI edit history (`design_ai_edit_history.md`).** `ai_edit_log::LayoutPath`
already reserves an `Incremental` variant "for `doc/design_incremental_layout.md`;
nothing produces it yet". Phase 5 produces it. The `moved` list is keyed by name
path and already covers bodies, so a body-only edit that moves nothing outside
the body reports exactly that.

**Undo.** The AI edit is one whole-network snapshot command
(`TextEditNetworkCommand`), taken after validation and after layout, and it
carries every zone. Every move this design makes, in every scope, is inside
that snapshot; **no `MoveNodesCommand` and no composite is needed on the AI
path**. The GUI reflow path keeps its `ScopedMoves` bundling unchanged. Setting
`hand_moved` on a drag is a persisted mutation and rides in the existing move
command rather than becoming a separate undo entry.

**Preferences.** `auto_layout_after_edit: bool` is replaced by a tri-state, or
simply repurposed: the incremental pass always runs (it is repair, not layout),
and the full reflow is only ever user-invoked. See open question 1.

---

## Phases

### Phase 1 — Foundations
The unified `rendered_node_size` with the Flutter-parity fixture, routed through
all four callers; per-layer column width in Sugiyama. `Node.hand_moved` with
`#[serde(default)]`, set from the scope-aware drag path, persisted and undoable.
The identity snapshot extended per D14 and the editor re-applying `body_width`
/ `body_height` / `collapse_mode` / `hand_moved` on a name match. The
`find_connected_components` seed sorted (D7). `diff_scope` producing a
per-scope `EditDelta` with footprint-based `modified`, with tests but not yet
wired to anything.

*Tests:* the size fixture; an expanded 320-wide HOF in a Sugiyama layer no
longer overlaps the next column; delta correctly classifies add / grow / remove
/ rewire; a value-only change produces an empty delta; a statement that shrinks
an array pin (`[a, b]` → `[a]`) yields the dropped wire in `removed_wires` and
leaves `modified` empty; a literal on a wired scalar pin does the same;
unwiring `f:` on an Auto-mode `map` classifies it `modified` with the expanded
footprint; a body statement that adds a node classifies the **owning HOF**
`modified` in the parent's delta (D12/D13); `hand_moved` round-trips through
`.cnnd` and copy/paste; a pre-flag `.cnnd` loads with `hand_moved = false`; a
network with two equal-size disconnected components lays out byte-identically
across two processes (D7); a `replace`-mode rebuild of an unchanged script
matches every node by name and yields an **empty** delta in every scope; a
`replace` round-trip of a **`Collapsed`** HOF leaves it collapsed and its body
size bit-identical (D14); a renamed node in a `replace` rebuild is classified
`added`, not matched to its old identity.

### Phase 2 — Motion primitives, and the GUI migration
`shift_half_plane`, the vertical cascade, `grow_rect`. Step 2 as a callable
unit. **Retire the quadrant shift**: switch the three `make_space_for_inline`
call sites to `grow_rect`, delete it, and adjust `reflow_test.rs`. This phase
is independently valuable — it fixes the GUI's overlap-creating growth today —
and it validates the primitive on the real cases A, B, C before the AI path
depends on it.

*Tests:* `grow_rect` on width alone shifts exactly the half-plane and moves
nothing below; on height alone pushes only colliding nodes down, by the
minimum, and a node with whitespace above it does not move; the
right-moved / down-moved overlap the quadrant shift produced (large `dw`, small
`dh`, two neighbours straddling the diagonal) does **not** occur; a node that
already overlapped the old rect is left alone; the cascade terminates on a
dense column and pushes a node whose x-interval overlaps a *pushed* node but
not `R` (the widening case); a forward wire crossing the shift line stays
forward; **the window rule** — a loose column straddling the default threshold
(members at `T − 3` and `T + 3`) moves as a whole; a snap never lands at or
below the grown node's x; the default is kept when the nearest gap is more than
a node width away; existing case A / B / C reflow tests pass with the new
expectations, and their single-step undo/redo is unchanged.

### Phase 3 — Block layout and placement
`layout_subgraph` with unified sizes, block decomposition, anchor computation
including the synthesized body anchors, Step 4 target position, Step 5a slide
with the slack-first rule in bodies, Step 5b via the cascade. The inside-out
driver over scopes.

*Tests:* one added node lands beside its input and **no existing node moves**; a
20-node connected addition is laid out internally by Sugiyama and placed as one
block with no existing node moving; an addition with no anchors goes right of
the drawing; a block needing a new column shifts the half-plane and nothing
reorders; a consumer whose left edge overlaps an input horizontally is still
moved by the no-room shift (threshold clamped to the window); a block whose
consumer sits at or left of an input's left edge produces **no** shift and no
input moves; an existing comment that nothing collides with stays at its **exact**
original position, on whichever side of its anchor the human put it (D9 — the
four-sides rule must not fire); a `$element → mul → output` body lays out left
to right against the synthesized anchors and **does not grow the body** when
the default body has room; a node added to a body with slack lands inside the
stored size and the parent's delta is empty; a node added to a full body grows
the HOF and the HOF's right neighbour in the parent shifts by exactly the width
delta; two-level nesting cascades to the grandparent; a new `map` with a
three-node body created in one script is placed in the parent at its settled
footprint and overlaps nothing; no body node ever receives a negative
coordinate.

### Phase 4 — Repair passes and tiebreakers
Step 6 (backward wires), Step 7 (new comments), Step 8 (drifted comments), the
`hand_moved` tiebreakers.

*Tests:* a rewire that points backward shifts the half-plane; a wire that was
**already** backward before the edit is not touched; a cross-scope wire is never
repaired; **removing** a wire moves nothing at all, and a node left with no
wires stays at its exact position; a 400×300 comment in the band is pushed like
any other node, by the minimum amount, and never ends up overlapped (D9); a
comment left behind by a half-plane shift is pulled back to its exact original
offset; a drifted comment whose original spot is now occupied stays where the
cascade left it; a comment that drifted *closer* to its anchor is not moved; a
comment with an unresolvable anchor is skipped; the pass never introduces an
overlap; a newly created `on:`-anchored comment lands beside its anchor. Plus a
generous guard assertion — no cascade pushes more than ~20 nodes — to catch a
regression, not to bound behaviour.

### Phase 5 — Wiring it up
Replace the `layout_network` call in `ai_text_edit` with the incremental pass,
producing `LayoutPath::Incremental`. Make the explicit full reflow body-aware
(inside-out recursion). Preferences and the Auto-Layout menu item ("respect
manually placed nodes"). Reference guide: `doc/reference_guide/node_networks.md`
(what happens to your layout when the AI edits, and how to get a full reflow)
and `doc/reference_guide/ui.md` for the menu/preference.

*Tests:* end-to-end through `ai_text_edit`; a **corpus regression** on both
corpora — load `from_mechadense.cnnd` and `demolib/baselib_with_demos.cnnd`,
apply a synthetic edit to one network, assert that every node outside the
delta and outside the pushed band is at its exact original position, bodies
included, and that no loose column of the polished corpus was split by a
shift; a `query` → `edit --replace` round-trip of
a body-bearing network moves nothing; a full reflow of a network with an
expanded HOF produces no overlap.

*Manual verification* (per `feedback_manual_test_for_editor_ui`): AI-add a node
in a dense region; AI-add a subassembly; AI-add a node inside a `map` body and
confirm the map's neighbour shifts right by the growth and nothing below moves;
delete a node and confirm the hole stays; drag a node then AI-edit near it;
collapse an HOF, `query` → `edit --replace`, confirm it is still collapsed;
full reflow and undo.

### Phase 6 (later) — Refinements
Downstream-cone shifting instead of half-plane. Hole reuse (place a block into a
deletion hole when one fits). The Step 8 partial-move refinement. Applying
`grow_rect` to the user's zone-resize drag (`set_zone_size`), which the reflow
design left out of scope.

---

## Open questions

1. **Does `auto_layout_after_edit` survive as a preference?** The incremental
   pass is repair, not layout — it should probably always run, with the full
   reflow purely user-invoked. That would make the current default (destroy the
   layout on every edit) unreachable, which the measurement says is a feature.
2. **Does a full reflow clear `hand_moved`?** Clearing is semantically honest
   but silently discards intent. Alternative: keep the flags, so a subsequent
   "respect manually placed nodes" reflow can restore the distinction.
3. ~~**Does the incremental pass recurse into HOF bodies in v1, or later?**~~
   **Answered: v1**, and now designed — see [Scopes: inside-out](#scopes-inside-out).
4. **Is `SLIDE_WINDOW` exposed?** Preference is: no. One internal constant,
   documented, not tunable.
5. **Should the drift after many edits be surfaced?** A cheap counter (e.g.
   number of backward wires introduced since the last full reflow) could drive a
   passive "this network could use a tidy-up" hint. Suggestion only, never
   automatic.
6. ~~**Does `replace` mode get name-matching in v1?**~~ **Answered: it already
   has it**, built by `design_hof_body_text_format.md` D8. What remains is D14's
   extension of what the match carries.
7. **Should Step 8 fall back to a partial move when the exact target
   collides?** Specified as all-or-nothing. The interpolation refinement is
   noted in Step 8; it recovers more cases but introduces a sample count.
8. **Should the slack-first slide in a body also prefer the stored size over
   the wire-derived x?** As specified, Step 4 computes the target x from anchors
   and Step 5a slides only in y within the slack. A block whose anchor-derived x
   already lies past the stored width grows the body regardless. Accepting that
   keeps the body rule to one clause; the alternative is a two-axis search.
9. **Should the zone-resize drag reflow?** The reflow design left
   `set_zone_size` out of scope because the user is already dragging. With
   `grow_rect` available it is a one-line addition; deferred to Phase 6 until
   someone wants it.
