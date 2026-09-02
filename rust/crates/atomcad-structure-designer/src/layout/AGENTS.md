# Layout - Agent Instructions

Automatic layout algorithms for repositioning nodes in a network.

## Files

| File | Purpose |
|------|---------|
| `size.rs` | `rendered_node_size` — **the** node-size function (see below) |
| `delta.rs` | `EditDelta` / `diff_scope` — the incremental pass's input |
| `motion.rs` | `shift_half_plane` / `cascade` / `grow_rect` — the two motion primitives (see below) |
| `incremental.rs` | The incremental pass, step by step (Steps 1-5 and the driver) |
| `common.rs` | Shared types and constants, depth computation, and the comment-placement pass |
| `topological_grid.rs` | Simple layered layout (fast, reliable) |
| `sugiyama.rs` | Sugiyama-style layout with crossing minimization |

## Entry Points

- `layout_network(network, registry, algorithm)` → applies a full reflow in-place
- `compute_layout(network, registry, algorithm)` → the same positions without mutating
- `layout_subgraph(network, registry, ids, algorithm)` → positions for **only**
  the nodes in `ids`, wired to each other alone (see below)
- `incremental::layout_incremental(network, registry, snapshot, wires, algorithm)`
  → the incremental pass over every scope, inside-out. Not wired into
  `ai_text_edit` yet; Phase 5 does that.

## Algorithms

**TopologicalGrid:** Assigns nodes to layers by topological order, then arranges vertically within each layer. Simple and predictable.

**Sugiyama:** Multi-phase algorithm: layer assignment → crossing reduction → coordinate assignment. Better results for complex graphs but more expensive.

## Usage

Layout is triggered by:
- AI text format edits (auto-layout new nodes via `text_format/auto_layout.rs`)
- "Auto-Layout Network" menu action in the UI, through
  `StructureDesigner::layout_active_network()` — **not** `layout_network()`
  directly. That wrapper diffs old against new positions and records the whole
  rearrangement as one undoable `MoveNodesCommand` (#270); calling
  `layout_network()` from a user-facing path would silently bypass undo.
- Node size estimation uses constants from `node_layout.rs` in the parent directory

## Comments are not invisible to layout any more

Comment nodes used to enter the layering as degenerate vertices and distort it:
depth 0 with no consumers put them at the bottom of the leftmost column
(`TopologicalGrid`), and having no wires at all made each its own singleton
component stacked below the whole graph in `HashMap`-iteration order
(`Sugiyama`). Since `doc/design_wire_annotations.md` Phase 5 they are handled by
a dedicated pass instead. Three rules to keep true:

1. **No comment ever participates in layer assignment** (D8). `TopologicalGrid`
   drops them from `depths`; `Sugiyama` excludes them from
   `find_connected_components`. One invariant covers both anchored comments
   (placed by rule afterwards) and unanchored ones (whose position is the thing
   being preserved). A new algorithm must do the same and then call
   `common::place_comments` as its last step.
2. **Comment boxes use the real `CommentData.width` / `.height`**, never
   `node_layout::estimate_node_height` — the estimate is 83 px against a default
   of 100 and a routine resized value of 300+, on a 210 px column pitch, so
   sizing one wrongly just produces overlaps in a tidier arrangement. This is
   now one case of the general rule in `size.rs`, which `common::node_size`
   forwards to.
3. **Comments never displace graph nodes.** They always yield: moving a node
   would undo the layout just computed and break column alignment. The two
   tiers - beside the anchor, else the nearest gutter - are documented on
   `place_comments` and `gutter_position`.

The whole section is **disposable scaffolding** (D10): a separate design will
rework auto-layout around preserving human intent for *all* nodes. Keep it small
and keep its constants cosmetic - no tuning surface, and nothing about a
placement written to the file. `doc/research_intent_preserving_layout.md` is
where the replacement is being worked out.

## One size function: `size::rendered_node_size`

Four size functions used to exist and no two agreed — the Sugiyama helper
ignored HOF bodies, the inlining helper ignored comments, the creation-time
placer knew only a type name, and Flutter's `ScopeResolver` knew both and is the
one that actually paints. `doc/design_incremental_layout.md` (Phase 1) collapsed
them into **`layout::size::rendered_node_size(node, registry)`**, which mirrors
`effectiveNodeSizeLogical` / `_computeBodySize` rule for rule: a comment is its
own `CommentData` box, an expanded HOF is gutters plus a *recursively measured*
body (`max(stored, content + padding)`), everything else is title + pins +
subtitle + padding. The subtitle row is **computed** from
`NodeData::get_subtitle`, not assumed present the way all three old estimates
assumed it.

Three rules follow:

- **Anything holding a real `Node` calls it.** `common::node_size` /
  `node_height` / `node_width`, `node_inlining::estimate_node_size_in_network`
  and `text_format/auto_layout`'s obstacle scan are all forwarders now. The one
  legitimate estimate left is `auto_layout::get_node_size`, which sizes a node
  that *does not exist yet* and so has no body, no comment box and no data to
  derive a subtitle from.
- **The stored `body_width` / `body_height` are a floor, never the value**, and
  layout never writes them back. A freshly built `closure` carries the flat
  320x180 default even when its body holds a nested `map` that renders far
  wider.
- **`tests/fixtures/layout_size_parity.json` pins the agreement with Flutter.**
  `test/layout_size_parity_test.dart` regenerates it from a network description
  the Rust test writes (`layout_size_parity_input.json`), so a Flutter-side size
  change fails a *Rust* test until the fixture is refreshed. That is the point:
  the two rules have drifted before and the drift was invisible until something
  overlapped on screen. The refresh cycle is in `layout_size_test.rs`'s module
  docs — Rust, then Dart, then Rust again.

## Two motion primitives, one per axis

`doc/design_incremental_layout.md` D6. Everything that has to make room in an
existing drawing — the incremental pass, and since Phase 2 the GUI's
growth reflow as well — goes through exactly two operations in `motion.rs`,
and adding a third is a design change, not a refactor:

- **`shift_half_plane(T, dx, fixed)`** — horizontal, rigid, global. Every placed
  node with `x >= T` moves right by `dx`, except a `fixed` set. `fixed` must be
  closed upstream (`upstream_closure`), and that is the whole correctness
  argument: with it, a wire from a moved node into an unmoved one cannot exist,
  so **a forward wire can never turn backward**. Drop the closure and the
  primitive stops being wire-safe.
- **`cascade(R, dir, fixed, ignore)`** — vertical, minimal, local. Only nodes
  that actually collide move, by the least that clears the collision, and the
  push propagates **only through overlaps the cascade itself created**. A
  pre-existing overlap is never repaired and never blocks anything — the design
  rule is *repair only what this edit broke*, and both hand-drawn corpora
  contain overlaps their authors are content with.

The pairing is not symmetric by accident: x carries a directional invariant
(rightward flow) and y carries none, so x gets the operation that provably
preserves order and y gets the one that moves the least.

**`grow_rect(node, old, new)` is the one growth operation** (D11) — width delta
as a shift, height delta as a downward cascade, in that order. Every path where
a node's footprint grows in place calls it: `reflow_for_footprint_change`,
`inline_custom_node`, `convert_instance_to_closure`, and Step 2 of the
incremental pass. It replaced `node_inlining::make_space_for_inline`, the
quadrant shift, which swept the whole lower-right region on both axes — moving
nodes nothing was going to collide with, and occasionally driving its
right-moved and down-moved halves into each other.

Two things to know before calling any of them:

- **They take a measured `sizes` map, not a `&NodeTypeRegistry`.** A
  `NodeNetwork` lives *inside* the registry (`NodeTypeRegistry::node_networks`),
  so `&mut NodeNetwork` and `&NodeTypeRegistry` cannot be held at once. Call
  `measure_scope` before taking the mutable borrow. Sizes are position-
  independent, so one measurement is good for a whole pass over one scope.
- **The map's keys are the placed set.** A node absent from `sizes` is invisible:
  not an obstacle, not moved. From Phase 3 that is how a freshly added node stays
  out of the way until its block is placed, so do not "helpfully" fall back to
  every node in the network.

## The incremental pass: blocks, and the invisibility rule

`incremental.rs` runs Steps 1-5 per scope, inside-out over
`delta::scopes_inside_out` (Steps 6-8 are Phase 4). Three things about it are
easy to break and expensive to debug:

- **An added node is invisible until its block is placed.** It is not an
  obstacle, not a snap candidate, not in the drawing's bounding box, and neither
  primitive moves it. This is expressed by *excluding it from the `sizes` map*
  (`measure_scope` minus `delta.added`), which is why "the map's keys are the
  placed set" above is a rule and not a description. It joins the map the moment
  its block lands. The same exclusion applies to the body measurement in
  `body_frame`: a body's right edge — where the `output` anchor sits — is where
  the body renders *before* this edit's nodes are placed, never a reading of
  their throwaway creation-time positions.
- **The inside-out order is what feeds a body's growth to its parent.** An HOF's
  footprint is `max(stored, body content + padding)`, so a settled body makes its
  owner measure bigger, and the parent's `EditDelta` — computed at the parent's
  own turn, against the live network — already classifies the owner `grown` at
  its settled size. Do not *also* thread a `grown` accumulator up from the child:
  `grow_rect` moves by `new - old`, so repairing one HOF twice with two different
  `new`s shifts its neighbours twice.
- **A block is placed rigidly.** `Block.rects` are block-local with the bounding
  box at the origin, so a placement is one translation; nothing inside a block is
  ever re-derived against the drawing it landed in.

Comments are ordinary nodes on this path (D9): obstacles at their real size,
pushed by the cascade, never re-placed by rule. `place_comments` is *not* called
here — it keeps serving the full reflow — which is why `layout_subgraph` stops
short of it and each algorithm's `layout` calls it afterwards.

## `layout_subgraph`: the same algorithms over a subset

`layout_subgraph(network, registry, ids, algorithm)` restricts a full layout to
`ids`: only those nodes are placed, and **only the wires between them are
edges**. Step 3 lays a block out with it, and `layout` in both algorithms is now
`layout_subgraph` over every id plus `place_comments`, so the two cannot drift.

The restriction goes all the way down — `common::compute_node_depths_within`,
component discovery, dummy nodes, barycenters. Two consequences worth knowing:

- A node fed only from outside the subset is a **source at depth 0**, which is
  what lets a block start its own drawing instead of inheriting the column index
  its producer happens to sit in.
- **A cross-scope wire is never an edge** (`source_scope_depth != 0`). `$element`
  names the owning HOF in the *parent* scope and ids are unique per network, so
  a body node can carry that same number without being connected to anything.
  Every graph walk here filters on depth for that reason.

## Sugiyama columns are per layer

A column is as wide as its widest member plus `common::COLUMN_GAP`, not a fixed
`COLUMN_WIDTH` pitch. A network of ordinary 160 px nodes lands exactly where it
did before; an expanded HOF no longer overhangs the next column. Keep the
equality test (`layout_columns_test::a_uniform_network_keeps_the_old_column_pitch`)
green — a "tidier" pitch would shift every existing drawing the first time
someone reflows it.

`find_connected_components` seeds its BFS from **sorted** ids. The size sort
after it is stable, so `HashMap` order used to decide how two equal-size
components stacked, per process. Any new iteration over nodes or scopes must be
ordered the same way (`doc/design_incremental_layout.md` D7).

`node_layout::input_pin_position` / `output_pin_position` estimate where a wire
meets a node. A wire's anchor point is its Bezier midpoint, which reduces to the
mean of its two pin positions; the painter's `_anchorTargetPoint`
(`node_network_painter.dart`) computes the same point from real geometry, and
the two must not diverge - it is one decision, not two.
