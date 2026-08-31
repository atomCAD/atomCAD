# Layout - Agent Instructions

Automatic layout algorithms for repositioning nodes in a network.

## Files

| File | Purpose |
|------|---------|
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
   sizing one wrongly just produces overlaps in a tidier arrangement.
   `common::node_size` is the single place that dispatches on this.
3. **Comments never displace graph nodes.** They always yield: moving a node
   would undo the layout just computed and break column alignment. The two
   tiers - beside the anchor, else the nearest gutter - are documented on
   `place_comments` and `gutter_position`.

The whole section is **disposable scaffolding** (D10): a separate design will
rework auto-layout around preserving human intent for *all* nodes. Keep it small
and keep its constants cosmetic - no tuning surface, and nothing about a
placement written to the file. `doc/research_intent_preserving_layout.md` is
where the replacement is being worked out.

`node_layout::input_pin_position` / `output_pin_position` estimate where a wire
meets a node. A wire's anchor point is its Bezier midpoint, which reduces to the
mean of its two pin positions; the painter's `_anchorTargetPoint`
(`node_network_painter.dart`) computes the same point from real geometry, and
the two must not diverge - it is one decision, not two.
