# Design: Click-to-Activate Node from Viewport Output

## Problem

When multiple nodes are set to visible, it is hard to tell which rendered output belongs to which node. The user's only option is to toggle visibility of individual nodes and observe what changes. We need a way to click on rendered output in the 3D viewport and activate the node that produced it.

## Design Overview

**Two-phase interaction:**
1. **First click** on output belonging to a non-active node: activates that node and scrolls the node network panel to reveal it.
2. **Second click** (now on the active node's output): performs the normal action (e.g., atom selection in atom_edit).

**Overlap handling:** When a click hits output from multiple nodes at similar depths, a disambiguation popup lets the user choose which node to activate.

## Detailed Behavior

### Phase 1: Click-to-Activate

When the user clicks in the 3D viewport:

1. A ray is cast from the camera through the click position (existing `getRayFromPointerPos` in `cad_viewport.dart`).
2. The ray is tested against **all visible node outputs** in the scene (`last_generated_structure_designer_scene.node_data`), collecting hits with their node IDs and distances.
3. **If all hits belong to the currently active node** — proceed with normal click handling (existing behavior, no change).
4. **If the closest hit belongs to a non-active node (unambiguous)** — activate that node, scroll the node network panel to show it, and show a SnackBar confirmation (see below). Do NOT pass the click action through.
5. **If multiple nodes have hits within an overlap threshold** — show a disambiguation popup (see below). Do NOT pass the click action through.
6. **If no hits** — existing behavior (camera operations, etc.).

### Phase 2: Normal Action

Once a node is active, all subsequent clicks on that node's output follow the existing interaction flow (atom selection, facet selection, gadget interaction, etc.). No changes needed here.

### Scroll-to-Node in Node Network Panel

When a node is activated via viewport click, the node network panel should scroll/pan to bring that node into view:

- **Visual node network mode:** Adjust `_panOffset` so the activated node's position is centered (or at least visible) in the panel. The node position is available from `NodeView.position`.
- **Text editor mode:** Scroll to the line containing the activated node's definition. This requires knowing the line range for each node in the serialized text.

### Disambiguation Popup

When overlapping outputs are detected, show a small popup near the click position. Each candidate node is displayed as a row with two interaction targets:

```
┌─────────────────────────────────┐
│  atom_fill "Base Layer"    [👁]  │
│  atom_fill "Overlay"       [👁]  │
│  sphere #42                [👁]  │
└─────────────────────────────────┘
```

- **Click the node name:** Activate that node and scroll the node network panel to it. All other nodes remain visible.
- **Click the solo icon (eye):** Activate that node, scroll to it, AND hide the **other overlapping nodes listed in this popup**. Other visible nodes elsewhere in the network that are not part of this overlap are unaffected.

This distinction matters because the user may have many visible nodes across the network, and only the ones causing the overlap at the click point need to be hidden for isolation. Hiding all visible nodes would be too aggressive.

If the user dismisses the popup (clicks away), no node change occurs.

## Overlap Detection

### Overlap Threshold

Two hits are considered overlapping when their ray distances are within a fixed epsilon:

```
|distance_a - distance_b| < OVERLAP_EPSILON
```

```rust
const OVERLAP_EPSILON: f64 = 0.1; // Angstroms
```

At atomic scales, if two outputs are more than 0.1 Å apart along the ray, they are clearly distinct objects and we simply activate the closer one. A fixed threshold is simpler and sufficient — subatomic-scale proximity is the only case where disambiguation is truly needed.

### Per-Output-Type Hit Testing

**Atomic structures:** Use the existing `AtomicStructure::hit_test()` which returns `HitTestResult::Atom(atom_id, distance)` or `Bond(ref, distance)`. This already provides per-atom/bond distance.

**Geometry (SDF/mesh):** Use `raytrace_geometry()` from `implicit_eval/ray_tracing.rs` which returns `Option<f64>` (distance). Must be called per-node rather than the current batched `raytrace_geometries()`.

**Drawing planes, 2D geometry:** These can be included but are lower priority. Drawing planes have a natural hit test (ray-plane intersection).

## Implementation Plan

### Rust: New API Function `raytrace_per_node`

Add a new function to `StructureDesigner` that returns per-node hit results instead of a single closest distance.

**File:** `rust/src/structure_designer/structure_designer.rs`

```rust
pub struct PerNodeRayHit {
    pub node_id: u64,
    pub distance: f64,
}

pub fn raytrace_per_node(
    &self,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
    visualization: &AtomicStructureVisualization,
) -> Vec<PerNodeRayHit>
```

This iterates `last_generated_structure_designer_scene.node_data` (keyed by node ID) and collects hits per node. The existing `raytrace()` method iterates `node_data.values()` — the new function iterates `node_data.iter()` to also capture the node ID keys.

**Reuse from existing code:**
- `AtomicStructure::hit_test()` — already returns distance, used in `raytrace()` and `hit_test_all_atomic_structures()`. Fully reusable.
- `raytrace_geometry()` (single geometry version) from `implicit_eval/ray_tracing.rs` — already exists, currently used via the batched `raytrace_geometries()`. Can be called per-node's `geo_tree` individually.
- `sphere_hit_test`, `cylinder_hit_test` from `util/hit_test_utils.rs` — used internally by `AtomicStructure::hit_test()`, no direct use needed.
- **No new raycast primitives are needed.** The existing per-atom and per-geometry raycast functions are sufficient; they just need to be called in a per-node loop that preserves the node ID.

### Rust: New API Endpoint

**File:** `rust/src/api/common_api.rs` (or a new file `rust/src/api/structure_designer/viewport_pick_api.rs`)

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn raytrace_per_node(
    ray_origin: APIVec3,
    ray_direction: APIVec3,
) -> Vec<PerNodeRayHitView>
```

Where `PerNodeRayHitView` contains `node_id: u64`, `distance: f64`, `node_name: String` (for disambiguation display).

### Rust: Overlap Detection and Activation

Two options for where to place the overlap detection logic:

**Option A — Rust-side (recommended):** A single API call that takes the ray, performs per-node raycasting, detects overlaps, and returns one of:
- `ActivateNode(node_id)` — unambiguous closest hit on a non-active node
- `Disambiguation(Vec<CandidateNode>)` — overlapping hits from multiple nodes
- `ActiveNodeHit` — closest hit belongs to the already-active node
- `NoHit` — ray missed everything

This keeps the logic centralized and avoids multiple round-trips.

**Option B — Flutter-side:** Return raw per-node hits, let Flutter decide. Simpler Rust code but duplicates logic.

### Flutter: Viewport Click Interception

**File:** `lib/structure_designer/structure_designer_viewport.dart`

Before delegating to the current `PrimaryPointerDelegate` or `onDefaultClick`, add a pre-check:

```dart
// In the click/pointer-down handling path:
final ray = getRayFromPointerPos(pos);
final pickResult = common_api.viewportPick(
    rayOrigin: vector3ToApiVec3(ray.start),
    rayDirection: vector3ToApiVec3(ray.direction));

if (pickResult is ActivateNode) {
    widget.graphModel.selectNode(pickResult.nodeId);
    widget.graphModel.scrollNodeNetworkToNode(pickResult.nodeId);
    renderingNeeded();
    return; // consume the click, don't pass to delegate
}
if (pickResult is Disambiguation) {
    _showDisambiguationMenu(pos, pickResult.candidates);
    return; // consume the click
}
// Otherwise: ActiveNodeHit or NoHit — proceed with existing behavior
```

This interception should happen **before** the existing `primaryPointerDelegate?.onPrimaryDown()` and `onDefaultClick()` calls.

### Flutter: Scroll-to-Node

**File:** `lib/structure_designer/node_network/node_network.dart`

Add a method to pan the visual node network to center on a given node:

```dart
void scrollToNode(u64 nodeId) {
    final node = model.nodeNetworkView?.nodes[nodeId];
    if (node == null) return;
    // Calculate panOffset to center node position in viewport
    final nodeCenter = Offset(node.position.x + NODE_WIDTH / 2,
                               node.position.y + NODE_HEIGHT / 2);
    final viewportCenter = Offset(size.width / 2, size.height / 2);
    _panOffset = (viewportCenter / _zoomLevel) - nodeCenter;
    setState(() {});
}
```

This requires exposing a method from `_NodeNetworkWidgetState` to `StructureDesignerModel`, likely via a callback or `GlobalKey`.

### Flutter: Disambiguation Menu

Show a popup overlay near the click position. Each candidate node gets a row with two interaction targets:

1. **Node name** (clickable label) — activates that node and scrolls the panel to it. Other nodes remain visible.
2. **Solo icon** (eye icon button) — activates that node, scrolls to it, AND hides the other overlapping nodes listed in this popup. Only the nodes in the disambiguation list are affected; other visible nodes elsewhere in the network are untouched.

The solo action calls `set_node_display(node_id, false)` for each of the other candidate node IDs returned in the `Disambiguation` result. The `ViewportPickResult::Disambiguation` variant must include the list of `CandidateNode { node_id, node_name }` so Flutter knows which nodes to hide.

```dart
void _showDisambiguationMenu(Offset pos, List<CandidateNode> candidates) {
    // Show popup with candidates
    // On name click: activate node, scroll to it
    // On solo icon click: activate node, scroll to it,
    //   hide other candidates:
    //   for (final other in candidates.where((c) => c.nodeId != chosen.nodeId)) {
    //       sd_api.setNodeDisplay(nodeId: other.nodeId, isDisplayed: false);
    //   }
}
```

## SnackBar Activation Feedback

When a node is activated via viewport click (both unambiguous and via disambiguation), show a brief SnackBar message confirming the activation:

```
Activated: atom_fill "Base Layer"
```

or for nodes without a custom name:

```
Activated: atom_fill #42
```

This reuses the existing SnackBar pattern already used in atomCAD for saturation feedback (`_showSaturationFeedback` in `structure_designer_viewport.dart`). It provides immediate confirmation that the click did something, even if the node network panel is small, scrolled away, or not in focus.

**This is a stopgap measure.** Once temporal output highlighting is implemented (see below), the SnackBar becomes redundant and should be removed in favor of the visual highlight, which communicates the same information more naturally.

## Hover Tooltip: Node Origin and Overlap Warning

The existing atom hover tooltip (`AtomTooltip` in `lib/common/atom_tooltip.dart`) shows element identity, bond count, frozen state, and position. It is driven by `query_hovered_atom_info()` which calls `hit_test_all_atomic_structures()` — this already raycasts across all visible nodes but only returns the closest atom without identifying which node it belongs to.

### Enhancement: Add Node Origin

Always show which node produced the hovered atom. This directly addresses the core "which output belongs to which node?" question, even when there are no overlaps.

**Tooltip without overlap:**
```
C (Carbon)
3 bonds
atom_fill "Base Layer"
Pos: (1.234, 5.678, 9.012) Å
```

The node origin line uses a muted style (like the position line) since it's informational context.

### Enhancement: Overlap Warning

When the hover raycast detects atoms from multiple nodes within the 0.1 Å overlap epsilon, add a prominent warning:

**Tooltip with overlap:**
```
C (Carbon)
3 bonds
atom_fill "Base Layer"
⚠ OVERLAP: atom_fill "Overlay", sphere #42
Pos: (1.234, 5.678, 9.012) Å
```

The overlap warning line should use red/orange styling (similar to the existing "Frozen" indicator's orange `Color(0xFFFFB74D)`, but red `Color(0xFFEF5350)` to convey a stronger warning). It lists the **other** overlapping nodes — not the closest one already shown as the origin.

This gives the user early warning before clicking: they can see that clicking here will trigger disambiguation, and they can see which nodes are involved.

### Implementation

The current `query_hovered_atom_info()` calls `hit_test_all_atomic_structures()` which returns a single `(atom_id, &AtomicStructure)`. This needs to be extended to:

1. Also return the **node ID** of the closest hit (for the origin line).
2. Also return a list of **other overlapping node IDs/names** (for the warning).

This can reuse the same `raytrace_per_node` infrastructure from click-to-activate. The `APIHoveredAtomInfo` struct gains two new fields:

```rust
pub struct APIHoveredAtomInfo {
    // ... existing fields ...

    /// Name of the node that produced this atom (custom name or "type #id")
    pub node_name: String,

    /// Names of other nodes with overlapping atoms at this position (empty if no overlap)
    pub overlapping_node_names: Vec<String>,
}
```

The `AtomTooltip` widget adds:
- A node origin line (always shown).
- An overlap warning line (only shown when `overlapping_node_names` is non-empty), styled in red.

### Scope

This enhancement covers **atoms only**. Geometry hover tooltips are not currently implemented and extending overlap detection to geometry on hover is deferred.

## Temporal Output Highlighting (Out of Scope)

A useful complement to click-to-activate would be **temporal highlighting** of the activated node's output in the viewport — e.g., a brief flash or outline that fades after a short duration. This would provide immediate visual feedback confirming which geometry belongs to the newly activated node.

However, this feature requires changes to the rendering pipeline. The current renderer operates on-demand (renders only when explicitly requested), so implementing a time-based fade animation would require either:
- A timed re-render loop during the highlight period, or
- A shader-based approach with a timestamp uniform.

**This is a separate design effort and is not in scope for this document.** It should be designed independently once the click-to-activate infrastructure is in place.

## Interaction Matrix

| Scenario | Behavior |
|---|---|
| Click on output of non-active node (unambiguous) | Activate node, scroll panel to it |
| Click on output of active node | Normal action (atom select, etc.) |
| Click where outputs from multiple non-active nodes overlap | Show disambiguation menu |
| Click where active node overlaps with non-active node(s) | If active node is closest: normal action. If non-active is closest: activate it. If overlapping: disambiguation menu |
| User clicks node name in disambiguation menu | Activate chosen node, scroll panel, keep all nodes visible |
| User clicks solo icon in disambiguation menu | Activate chosen node, scroll panel, hide the other overlapping nodes from the popup (other visible nodes in the network are unaffected) |
| User dismisses disambiguation menu | No change |
| Click on empty space | Existing behavior (deselect, camera, etc.) |

## Files to Modify

| File | Change |
|------|--------|
| `rust/src/structure_designer/structure_designer.rs` | Add `raytrace_per_node()` method |
| `rust/src/api/common_api.rs` or new `viewport_pick_api.rs` | Add `viewport_pick()` API endpoint |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Extend `query_hovered_atom_info()` to return node origin and overlap info |
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | Add `PerNodeRayHitView`, `ViewportPickResult` types; extend `APIHoveredAtomInfo` with `node_name` and `overlapping_node_names` |
| `lib/structure_designer/structure_designer_viewport.dart` | Add click interception before delegate dispatch |
| `lib/structure_designer/node_network/node_network.dart` | Add `scrollToNode()` method |
| `lib/structure_designer/structure_designer_model.dart` | Add `scrollNodeNetworkToNode()` method |
| `lib/common/atom_tooltip.dart` | Add node origin line and overlap warning line |

## Existing Code Reuse Summary

| Existing Code | Location | Reuse |
|---|---|---|
| `AtomicStructure::hit_test()` | `crystolecule/atomic_structure/mod.rs` | Direct reuse — returns `HitTestResult` with distance |
| `raytrace_geometry()` | `structure_designer/implicit_eval/ray_tracing.rs` | Direct reuse — call per-node instead of batched |
| `StructureDesigner::raytrace()` | `structure_designer/structure_designer.rs` | Reference pattern — new `raytrace_per_node` follows same iteration but preserves node IDs |
| `hit_test_all_atomic_structures()` | `structure_designer/structure_designer.rs` | Reference pattern — similar per-node iteration for atoms |
| `getRayFromPointerPos()` | `lib/common/cad_viewport.dart` | Direct reuse — ray generation from screen coordinates |
| `sphere_hit_test()`, `cylinder_hit_test()` | `util/hit_test_utils.rs` | Indirect reuse — called by `AtomicStructure::hit_test()` |
| `logicalToScreen()` / `screenToLogical()` | `lib/structure_designer/node_network/node_network.dart` | Direct reuse — for scroll-to-node pan calculation |
| **New code needed** | — | Per-node raycast loop, overlap detection logic, disambiguation UI, scroll-to-node, viewport click interception |
