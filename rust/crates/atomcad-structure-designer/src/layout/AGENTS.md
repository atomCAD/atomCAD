# Layout - Agent Instructions

Automatic layout algorithms for repositioning nodes in a network.

## Files

| File | Purpose |
|------|---------|
| `size.rs` | `rendered_node_size` — **the** node-size function (see below) |
| `delta.rs` | `EditDelta` / `diff_scope` — the incremental pass's input |
| `common.rs` | Shared types and constants, depth computation, and the comment-placement pass |
| `topological_grid.rs` | Simple layered layout (fast, reliable) |
| `sugiyama.rs` | Sugiyama-style layout with crossing minimization |

## Entry Points

- `layout_network(network, registry)` → applies layout in-place
- `compute_layout(network, registry)` → returns `LayoutResult` without mutating

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
