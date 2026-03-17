# Atom Hover Tooltip — Design Document

## Overview

Display a tooltip near the mouse cursor when hovering over any displayed atom in the
3D viewport. The tooltip shows element identity, position, bond count, and other
useful information. The feature applies to **all** displayed `AtomicStructure` nodes,
not only the atom_edit node.

## Requirements

1. **Universal scope** — Works for any displayed atom in any `AtomicStructure` in the
   scene, regardless of which node type produced it.
2. **Debounced activation** — The tooltip appears only after the mouse pointer has been
   stationary for ~100 ms over an atom. Moving the cursor resets the timer.
3. **Conflict avoidance** — The tooltip is suppressed when the atom_edit AddAtom tool
   is active (it already displays its own element label).
4. **Rich, extensible data** — The tooltip can contain more information than the
   atom_edit property panel's info card, and is easy to extend with new fields.
5. **No evaluation or tessellation** — The ray-cast hit test is lightweight
   (sphere/cylinder intersection math only). No network re-evaluation or mesh
   generation is triggered.

## Architecture

### Data flow

```
Mouse stationary for 100 ms
          ↓
Flutter timer fires → compute ray from cursor position
          ↓
Call Rust API:  query_hovered_atom_info(ray_origin, ray_direction)
          ↓
Rust iterates ALL visible AtomicStructures in the scene
  → hit_test each structure (sphere intersection)
  → pick closest hit across all structures
  → look up AtomInfo from ATOM_INFO table
  → compute bond count, neighbor info
  → return APIHoveredAtomInfo (or None)
          ↓
Flutter setState → render tooltip overlay
```

### Layers

| Layer | Responsibility |
|-------|----------------|
| **Flutter viewport** | Debounce timer, ray generation, overlay widget |
| **Rust API** (`common_api.rs`) | Entry point, coordinate conversion |
| **Rust core** (`structure_designer.rs`) | Scene-wide hit test across all visible `AtomicStructure` nodes |
| **Rust data** (`atomic_constants.rs`, `AtomicStructure`) | Element lookup, bond queries |

### Why a scene-level API (not atom_edit-specific)

The existing `atom_edit_select_by_ray` and `hit_test_atom_only` only operate on the
**selected node's** output structure. The hover tooltip must work on **any** displayed
atom from **any** visible node. This requires a new function that iterates all
`NodeOutput::Atomic` entries in `StructureDesignerScene.node_data`.

## Relationship to Existing Ray-Cast Infrastructure

The codebase already has a scene-wide ray-cast that serves as a reference pattern:
`adjust_camera_target()` in `rust/src/api/common_api.rs:337-387` iterates ALL visible
`NodeOutput::Atomic` entries in `StructureDesignerScene.node_data`, calls
`atomic_structure.hit_test()` on each, and picks the closest hit. It delegates to
`StructureDesigner::raytrace()` in `structure_designer.rs:2086-2154`.

**Why we don't reuse `raytrace()` directly:**
- It returns only `Option<f64>` (distance) — the camera only needs *where* to set its
  pivot point, so it discards the atom ID. We need the atom ID and structure reference.
- It hard-codes `SpaceFilling` visualization for radius calculation — the camera
  intentionally targets the atom's full visual volume regardless of display mode. Our
  hover tooltip must use the **user's current display preference** (ball-and-stick vs
  space-filling) so the hit test matches what's visually rendered. Pointing at empty
  space that would only be filled in space-filling mode should not trigger a tooltip in
  ball-and-stick mode.
- It also raytraces `ImplicitGeometry3D` objects — we only need atomic structures.

**What we reuse as-is (no new code needed):**
- `getRayFromPointerPos()` (`lib/common/cad_viewport.dart:329`) — ray generation from
  screen coordinates, handles both orthographic and perspective. Already called in
  `_onHover()`.
- `atomic_structure.hit_test()` (`rust/src/crystolecule/atomic_structure/mod.rs:501`) —
  the core sphere/cylinder intersection test per structure. Both `raytrace()` and our
  new `hit_test_all_atomic_structures` call the same function.
- `get_displayed_atom_radius()` (`rust/src/display/atomic_tessellator.rs:222`) — atom
  radius calculation from visualization preference. Same function used by rendering,
  camera ray-cast, and selection hit tests.
- `_projectWorldToScreen()` (`lib/structure_designer/structure_designer_viewport.dart:372`)
  — projects 3D world coordinates to 2D screen space. Already used by the AddBond
  rubber-band overlay. It is private to `_StructureDesignerViewportState`, which is
  exactly where the tooltip overlay is built — no need to move it.
- `ATOM_INFO` lookup (`rust/src/crystolecule/atomic_constants.rs`) — element symbol,
  name, radii, color. Same table used everywhere.

**What is new (the scene-iteration loop):**
The `hit_test_all_atomic_structures` method is a simple for-loop over
`scene.node_data.values()` filtering for `NodeOutput::Atomic` — structurally identical
to the loop in `raytrace()` but returning `(atom_id, &AtomicStructure)`. The overlap
is ~10 lines of loop boilerplate; abstracting a shared helper would add indirection
without meaningful deduplication, so a separate method is the right call.

## Rust Changes

### 1. New API type: `APIHoveredAtomInfo`

**File:** `rust/src/api/structure_designer/structure_designer_api_types.rs`

```rust
/// Information about the atom under the cursor, returned by hover hit test.
#[flutter_rust_bridge::frb]
#[derive(Debug, Clone)]
pub struct APIHoveredAtomInfo {
    // Identity
    pub symbol: String,           // e.g., "C"
    pub element_name: String,     // e.g., "Carbon"
    pub atomic_number: i32,

    // Position (world-space Angstroms — used both for display and
    // for Flutter to project the tooltip anchor to screen space)
    pub x: f64,
    pub y: f64,
    pub z: f64,

    // Bonding
    pub bond_count: u32,          // coordination number
}
```

A dedicated struct (not reusing `APIMeasurement::AtomInfo`) because:
- It will grow independently with hover-specific fields (neighbor list, formal charge,
  hybridization label, etc.) without affecting the measurement enum.
- The `x/y/z` fields serve double duty: displayed in the tooltip text and used by Flutter
  to project the tooltip anchor to screen space, placing it next to the atom rather than
  at the raw cursor position.

### 2. New API function: `query_hovered_atom_info`

**File:** `rust/src/api/structure_designer/structure_designer_api.rs`

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn query_hovered_atom_info(
    ray_origin: APIVec3,
    ray_direction: APIVec3,
) -> Option<APIHoveredAtomInfo>
```

Implementation:
1. Acquire read access to `CAD_INSTANCE`.
2. Get `&StructureDesignerScene`.
3. For each `(node_id, NodeSceneData)` where `output` is `NodeOutput::Atomic(structure)`:
   a. Call `structure.hit_test(ray_origin, ray_dir, visualization, atom_radius_fn, bond_radius)`.
   b. If `HitTestResult::Atom(id, distance)`, track the closest hit across all structures.
4. For the closest atom, look up `ATOM_INFO` by `atomic_number`, read `bonds.len()`,
   read `position`, and populate `APIHoveredAtomInfo`.
5. Return `Some(info)` or `None`.

This function is `sync` because it performs no mutation and only does math — no
evaluation, no tessellation, no state change.

### 3. Scene-level hit test helper

**File:** `rust/src/structure_designer/structure_designer.rs`

Add a method:

```rust
impl StructureDesigner {
    /// Hit-test across ALL visible AtomicStructures in the scene.
    /// Returns (atom_id, &AtomicStructure) for the closest atom hit.
    pub fn hit_test_all_atomic_structures(
        &self,
        ray_origin: &DVec3,
        ray_direction: &DVec3,
    ) -> Option<(u32, &AtomicStructure)> { ... }
}
```

This iterates `last_generated_structure_designer_scene.node_data` and picks the
globally closest atom across all visible `NodeOutput::Atomic` entries. It uses the
same `get_displayed_atom_radius` and visualization preferences as the existing
selection hit test.

## Flutter Changes

### 1. State fields in `_StructureDesignerViewportState`

**File:** `lib/structure_designer/structure_designer_viewport.dart`

```dart
Timer? _hoverDebounceTimer;
APIHoveredAtomInfo? _hoveredAtomInfo;
Offset? _lastHoverPos;
```

### 2. Clearing helper

A single method handles all tooltip dismissal. This is called from multiple sites
(hover, pointer-down, tool change, exit) to guarantee the tooltip never lingers.

```dart
void _clearHoverTooltip() {
  _hoverDebounceTimer?.cancel();
  if (_hoveredAtomInfo != null) {
    setState(() => _hoveredAtomInfo = null);
  }
}
```

### 3. Debounce + hit-test logic

Modify `_onHover` — integrate with the existing guided-placement tracking that
already lives there. The movement threshold prevents flicker from hand tremor:
a small move (< 4 px) keeps the current tooltip visible and re-arms the timer
at the new position, while a larger move clears it immediately.

```dart
void _onHover(PointerHoverEvent event) {
  final pos = event.localPosition;
  setState(() => _cursorPosition = pos);

  // Movement threshold: suppress flicker from micro-movements (< 4 px).
  // If the cursor barely moved, keep the current tooltip and just re-arm
  // the timer at the new position.
  final moved = _lastHoverPos != null
      ? (pos - _lastHoverPos!).distance
      : double.infinity;
  if (moved >= 4.0) {
    // Significant move — clear tooltip immediately, restart debounce.
    _clearHoverTooltip();
  } else {
    // Micro-move — cancel timer but keep existing tooltip visible.
    _hoverDebounceTimer?.cancel();
  }

  _scheduleHoverHitTest(pos);

  // Existing guided placement tracking (unchanged)
  if (atom_edit_api.atomEditIsInGuidedPlacement()) {
    final ray = getRayFromPointerPos(pos);
    final changed = atom_edit_api.atomEditGuidedPlacementPointerMove(
      rayStart: vector3ToApiVec3(ray.start),
      rayDir: vector3ToApiVec3(ray.direction),
    );
    if (changed) renderingNeeded();
  }
}

void _scheduleHoverHitTest(Offset pos) {
  _hoverDebounceTimer?.cancel();

  // Suppress while AddAtom tool is active (it has its own cursor label)
  if (widget.graphModel.isNodeTypeActive('atom_edit') &&
      widget.graphModel.activeAtomEditTool == APIAtomEditTool.addAtom) {
    return;
  }

  _lastHoverPos = pos;
  _hoverDebounceTimer = Timer(const Duration(milliseconds: 100), () {
    _performHoverHitTest(pos);
  });
}

void _performHoverHitTest(Offset pos) {
  final ray = getRayFromPointerPos(pos);
  final info = structure_designer_api.queryHoveredAtomInfo(
    rayOrigin: vector3ToApiVec3(ray.start),
    rayDirection: vector3ToApiVec3(ray.direction),
  );
  if (mounted && _lastHoverPos == pos) {
    setState(() => _hoveredAtomInfo = info);
  }
}
```

### 4. Clearing on pointer-down, exit, and dispose

During any drag (selection, camera rotation, gadget manipulation, add-bond),
`MouseRegion.onHover` **stops firing** — the `Listener` in the base class handles
pointer events instead. Without explicit clearing, the tooltip would persist
throughout the entire drag.

**In `onPointerDown` (base class override):**

```dart
@override
void onPointerDown(PointerDownEvent event) {
  _clearHoverTooltip();   // dismiss before any drag begins
  super.onPointerDown(event);
}
```

**In `onExit`:**

```dart
onExit: (_) {
  _clearHoverTooltip();
  setState(() => _cursorPosition = null);
},
```

**In `dispose()`:**

```dart
@override
void dispose() {
  _hoverDebounceTimer?.cancel();
  _elementAccumulator.dispose();
  _focusNode.dispose();
  super.dispose();
}
```

### 5. Clearing on tool change and structure mutation

**Tool switch** — In `_onKeyEvent`, after any tool-switching key (D, Q, J) calls
`setActiveAtomEditTool`, call `_clearHoverTooltip()`. The tooltip must disappear
immediately when the user activates a different tool.

**Structure change** — In `refreshFromKernel()` (or wherever the model calls
`notifyListeners()` after a Rust state change), clear the tooltip to avoid showing
stale data for a deleted or moved atom:

```dart
void refreshFromKernel() {
  _clearHoverTooltip();
  // ... existing refresh logic ...
}
```

### 6. Tooltip overlay in `build()`

**File:** `lib/structure_designer/structure_designer_viewport.dart`

Build the tooltip alongside the existing overlays. The tooltip is clamped to
the viewport bounds so it never extends off-screen:

```dart
Widget? atomTooltipOverlay;
if (_hoveredAtomInfo != null) {
  final info = _hoveredAtomInfo!;
  final screenPos = _projectWorldToScreen(
    info.x, info.y, info.z,
  );
  if (screenPos != null) {
    // Offset from atom center; will be clamped after layout.
    const offsetX = 20.0;
    const offsetY = -10.0;
    // Estimated tooltip size for clamping (actual size varies).
    const estW = 180.0;
    const estH = 70.0;

    final left = (screenPos.dx + offsetX)
        .clamp(4.0, viewportWidth - estW - 4.0);
    final top = (screenPos.dy + offsetY)
        .clamp(4.0, viewportHeight - estH - 4.0);

    atomTooltipOverlay = Positioned(
      left: left,
      top: top,
      child: IgnorePointer(child: AtomTooltip(info: info)),
    );
  }
}
```

Added to the `Stack`:

```dart
Stack(
  children: [
    super.build(context),
    if (_marqueeRect != null) ...,
    if (addBondOverlay != null) addBondOverlay,
    if (elementSymbolOverlay != null) elementSymbolOverlay,
    if (atomTooltipOverlay != null) atomTooltipOverlay,
  ],
)
```

### 7. `AtomTooltip` widget

**File:** `lib/common/atom_tooltip.dart` (new file)

A small, self-contained stateless widget. Separated into `common/` because:
- It is not atom_edit specific (the feature is viewport-wide).
- It can be reused if we add tooltips elsewhere.
- It keeps the viewport file focused on event routing.

Visual style matches the existing element symbol overlay (`Color(0xDD303030)`
background, `Color(0xFF4FC3F7)` accent, `BorderRadius.circular(4)`):

```dart
class AtomTooltip extends StatelessWidget {
  const AtomTooltip({super.key, required this.info});

  final APIHoveredAtomInfo info;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
      constraints: const BoxConstraints(maxWidth: 220),
      decoration: BoxDecoration(
        color: const Color(0xDD303030),
        borderRadius: BorderRadius.circular(4),
        border: Border.all(color: const Color(0x88FFFFFF), width: 0.5),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          // Line 1: element identity — bold, accent color
          Text(
            '${info.symbol} (${info.elementName})',
            style: const TextStyle(
              color: Color(0xFF4FC3F7),
              fontSize: 13,
              fontWeight: FontWeight.w600,
              decoration: TextDecoration.none,
            ),
          ),
          const SizedBox(height: 2),
          // Line 2: bond count
          Text(
            '${info.bondCount} bond${info.bondCount == 1 ? '' : 's'}',
            style: const TextStyle(
              color: Color(0xCCFFFFFF),
              fontSize: 11,
              fontWeight: FontWeight.normal,
              decoration: TextDecoration.none,
            ),
          ),
          // Line 3: position in Angstroms (3 decimal places)
          Text(
            '${info.x.toStringAsFixed(3)}, '
            '${info.y.toStringAsFixed(3)}, '
            '${info.z.toStringAsFixed(3)} \u00c5',
            style: const TextStyle(
              color: Color(0x99FFFFFF),
              fontSize: 11,
              fontWeight: FontWeight.normal,
              decoration: TextDecoration.none,
            ),
          ),
        ],
      ),
    );
  }
}
```

## Suppression Rules

| Condition | Mechanism | Show tooltip? | Reason |
|-----------|-----------|:---:|--------|
| atom_edit AddAtom tool active | `_scheduleHoverHitTest` returns early | No | Conflicts with `+Element` cursor label |
| Pointer down (any button) | `onPointerDown` → `_clearHoverTooltip()` | No | Drag is starting |
| Camera drag in progress | Cleared by pointer-down; `onHover` doesn't fire during drag | No | User is navigating |
| Gadget drag in progress | Same as camera drag | No | User is manipulating a gizmo |
| atom_edit AddBond tool dragging | Same as camera drag | No | Rubber-band line is the focus |
| Tool switch (D/Q/J keys) | `_onKeyEvent` → `_clearHoverTooltip()` | No | Different mode now active |
| Structure mutation | `refreshFromKernel` → `_clearHoverTooltip()` | No | Stale data |
| Mouse is moving (≥ 4 px) | `_onHover` → `_clearHoverTooltip()` + restart timer | No | Debounce resets |
| Mouse micro-move (< 4 px) | Timer restarts but tooltip stays | Yes | Prevents tremor flicker |
| Mouse exits viewport | `onExit` → `_clearHoverTooltip()` | No | Nothing to hover |
| No atom under cursor | `_performHoverHitTest` returns null | No | Nothing to show |
| Otherwise (idle over atom) | Timer fires → setState | **Yes** | |

The debounce timer handles the "mouse is moving" case naturally. Pointer-down
clearing handles all drag-type suppressions uniformly — no need to check
`dragState`, `isGadgetDragging`, or tool-specific drag flags individually.

## Performance Analysis

| Operation | Cost | Frequency |
|-----------|------|-----------|
| `_onHover` event processing | Negligible (setState + timer start/cancel) | Every mouse move (~60-240 Hz) |
| Timer creation/cancellation | ~microseconds | Every mouse move |
| `query_hovered_atom_info` ray-cast | ~microseconds per structure (sphere intersection math per atom) | Once per hover dwell (after 100 ms idle) |
| `_projectWorldToScreen` | ~microseconds (matrix math) | Once per hover dwell |
| Tooltip widget build | Standard Flutter text layout | Once per hover dwell |

**Worst case:** A scene with 100,000 atoms and 10 visible atomic structures. The
ray-cast iterates all atoms in each structure doing a sphere intersection test (a few
multiplies + a comparison). At ~10 ns per atom, 100K atoms ≈ 1 ms. Well within the
16 ms frame budget, and it only runs once after 100 ms of mouse inactivity.

**No per-frame cost:** The hit test only fires when the timer elapses. During mouse
motion, only the timer reset runs (essentially free).

## Extensibility

Adding new fields to the tooltip requires:
1. Add field to `APIHoveredAtomInfo` struct in Rust.
2. Populate it in `query_hovered_atom_info`.
3. Run FRB codegen.
4. Display it in `AtomTooltip` widget.

No changes to the timer, debounce, suppression, or overlay positioning logic.

### Candidate future fields

These are fields we may want to add later. They are **not** part of the initial
implementation — listed here to validate that the architecture supports them:

- **Hybridization** (sp3/sp2/sp1) — via UFF type assignment
- **Formal charge** — from valence electron count vs bond count
- **Neighbor list** — "bonded to: C, C, H, H"
- **Bond orders** — "2× single, 1× double"
- **Node name** — which node produced this atom (from scene node_data key → network lookup)
- **Is hydrogen passivation** — flag from atom.flags

## Implementation Order

1. Add `APIHoveredAtomInfo` struct to Rust API types.
2. Add `hit_test_all_atomic_structures` to `StructureDesigner`.
3. Add `query_hovered_atom_info` API function.
4. Run FRB codegen.
5. Create `AtomTooltip` widget in `lib/common/`.
6. Add debounce timer + overlay to viewport.
7. Test with multi-node scene (atom_fill + atom_edit visible simultaneously).

---

## Phased Implementation Plan

### Phase 1 — Rust: API type + scene-level hit test + API function

**Goal:** All Rust code compiles and passes `cargo test` / `cargo clippy`. No Flutter
changes yet.

#### Step 1.1 — Add `APIHoveredAtomInfo` struct

**File:** `rust/src/api/structure_designer/structure_designer_api_types.rs`

Insert after the `APIHybridization` enum definition, before the `APINodeView` struct
block. Follow the existing pattern: `#[flutter_rust_bridge::frb]` + `#[derive(Debug, Clone)]`
+ all `pub` fields.

```rust
/// Information about the atom under the cursor, returned by hover hit test.
#[flutter_rust_bridge::frb]
#[derive(Debug, Clone)]
pub struct APIHoveredAtomInfo {
    // Identity
    pub symbol: String,
    pub element_name: String,
    pub atomic_number: i32,

    // Position (world-space Angstroms — used both for display and
    // for Flutter to project the tooltip anchor to screen space)
    pub x: f64,
    pub y: f64,
    pub z: f64,

    // Bonding
    pub bond_count: u32,
}
```

No new imports needed — file uses only primitive types and existing FRB macros.

#### Step 1.2 — Add `hit_test_all_atomic_structures` to `StructureDesigner`

**File:** `rust/src/structure_designer/structure_designer.rs`

Insert a new method on the existing `impl StructureDesigner` block, directly after
`raytrace()`, before the preferences section divider.

The method structurally mirrors the atomic-structure loop in `raytrace()` but differs in
two ways:
- Returns `Option<(u32, &AtomicStructure)>` instead of `Option<f64>` — callers need the
  atom ID and structure reference, not just the distance.
- Uses the **user's current visualization preference** (not hard-coded `SpaceFilling`) so
  the hit test matches the rendered display mode.

Imports needed (already in scope from `raytrace()`):
- `crate::structure_designer::structure_designer_scene::NodeOutput` (already used in `raytrace()`)
- `crate::crystolecule::atomic_structure::HitTestResult` (already used in `raytrace()`)
- `crate::display::atomic_tessellator::{get_displayed_atom_radius, BAS_STICK_RADIUS}` (already used in `raytrace()`)
- `crate::display::preferences as display_prefs` — add this `use` inside the method (local scope, matching `add_atom_tool.rs` pattern)

The visualization preference lives at
`self.preferences.atomic_structure_visualization_preferences.visualization`. Convert it
to the display crate's enum using the same match block found in `selection.rs`'s
`hit_test_atom_only` helper and `raytrace()`.

```rust
/// Hit-test across ALL visible AtomicStructures in the scene.
/// Returns (atom_id, &AtomicStructure) for the closest atom hit,
/// using the user's current visualization preference for radius calculation.
pub fn hit_test_all_atomic_structures(
    &self,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
) -> Option<(u32, &AtomicStructure)> {
    use crate::display::preferences as display_prefs;
    use crate::structure_designer::structure_designer_scene::NodeOutput;

    let visualization = &self
        .preferences
        .atomic_structure_visualization_preferences
        .visualization;
    let display_visualization = match visualization {
        AtomicStructureVisualization::BallAndStick => {
            display_prefs::AtomicStructureVisualization::BallAndStick
        }
        AtomicStructureVisualization::SpaceFilling => {
            display_prefs::AtomicStructureVisualization::SpaceFilling
        }
    };

    let mut closest: Option<(u32, &AtomicStructure, f64)> = None;

    for node_data in self
        .last_generated_structure_designer_scene
        .node_data
        .values()
    {
        if let NodeOutput::Atomic(atomic_structure) = &node_data.output {
            if let HitTestResult::Atom(atom_id, distance) = atomic_structure.hit_test(
                ray_origin,
                ray_direction,
                visualization,
                |atom| get_displayed_atom_radius(atom, &display_visualization),
                BAS_STICK_RADIUS,
            ) {
                if closest.as_ref().map_or(true, |c| distance < c.2) {
                    closest = Some((atom_id, atomic_structure, distance));
                }
            }
        }
    }

    closest.map(|(id, structure, _)| (id, structure))
}
```

Check that `AtomicStructureVisualization` (the preferences enum, not the display crate
enum) is already imported at the top of the file. If not, add:
```rust
use crate::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization;
```

Also verify `HitTestResult` is in scope. It is used fully qualified in `raytrace()`; the new
method can either use the same fully qualified path or add a local `use`. Prefer the local
`use` for readability since the method uses it in an `if let` pattern.

#### Step 1.3 — Add `query_hovered_atom_info` API function

**File:** `rust/src/api/structure_designer/structure_designer_api.rs`

Add the new function near the end of the file (or after existing read-only query functions).
Follow the `with_cad_instance_or` + `#[flutter_rust_bridge::frb(sync)]` pattern used by
`get_node_network_view()` and `can_connect_nodes()`.

Add import at top of file:
```rust
use super::structure_designer_api_types::APIHoveredAtomInfo;
```

Function body:

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn query_hovered_atom_info(
    ray_origin: APIVec3,
    ray_direction: APIVec3,
) -> Option<APIHoveredAtomInfo> {
    let ray_origin = from_api_vec3(&ray_origin);
    let ray_direction = from_api_vec3(&ray_direction);

    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let (atom_id, structure) = cad_instance
                    .structure_designer
                    .hit_test_all_atomic_structures(&ray_origin, &ray_direction)?;

                let atom = structure.atoms.get(&atom_id)?;
                let atom_info = crate::crystolecule::atomic_constants::ATOM_INFO
                    .get(&(atom.atomic_number as i32))
                    .unwrap_or(&crate::crystolecule::atomic_constants::DEFAULT_ATOM_INFO);

                let bond_count = structure
                    .bonds
                    .values()
                    .filter(|b| b.atom1 == atom_id || b.atom2 == atom_id)
                    .count() as u32;

                Some(APIHoveredAtomInfo {
                    symbol: atom_info.symbol.clone(),
                    element_name: atom_info.element_name.clone(),
                    atomic_number: atom_info.atomic_number,
                    x: atom.position.x,
                    y: atom.position.y,
                    z: atom.position.z,
                    bond_count,
                })
            },
            None,
        )
    }
}
```

Note: `atom.position` is already in world-space Angstroms (the scene stores evaluated
output structures). The `x/y/z` fields serve both as display values in the tooltip text
and as world coordinates for Flutter's screen-space projection of the tooltip anchor.

#### Step 1.4 — Verify

```bash
cd rust && cargo build && cargo test && cargo clippy
```

All must pass with zero new warnings. The only pre-existing clippy warning is the
`get_or_insert` issue in `poly_mesh`.

---

### Phase 2 — FRB codegen

**Goal:** Generated Dart bindings include `queryHoveredAtomInfo()` and
`APIHoveredAtomInfo`. Flutter compiles.

#### Step 2.1 — Run codegen

```bash
flutter_rust_bridge_codegen generate
```

This regenerates `lib/src/rust/` files including:
- `lib/src/rust/api/structure_designer/structure_designer_api.dart` — will contain
  `queryHoveredAtomInfo()`.
- `lib/src/rust/api/structure_designer/structure_designer_api_types.dart` — will contain
  `APIHoveredAtomInfo` class.
- `lib/src/rust/api/structure_designer/structure_designer_api_types.freezed.dart` —
  updated freezed output.
- `lib/src/rust/frb_generated.dart` and `rust/src/frb_generated.rs` — updated wire
  functions.

#### Step 2.2 — Verify

```bash
flutter analyze
```

Should report no new errors (pre-existing warnings are fine).

---

### Phase 3 — Flutter: `AtomTooltip` widget

**Goal:** A standalone stateless widget exists in `lib/common/atom_tooltip.dart`,
importable but not yet wired to the viewport.

#### Step 3.1 — Create `lib/common/atom_tooltip.dart`

New file. The widget takes an `APIHoveredAtomInfo` and renders three lines of text in a
dark rounded container matching the existing element-symbol overlay style
(`Color(0xDD303030)` background, `Color(0xFF4FC3F7)` accent,
`BorderRadius.circular(4)`).

Content lines:
1. `"C (Carbon)"` — symbol + element name, bold cyan, 13 px.
2. `"3 bonds"` — bond count with plural handling, white 60% opacity, 11 px.
3. `"1.234, 5.678, 9.012 Å"` — position to 3 decimal places, white 40% opacity, 11 px.

All `Text` widgets must set `decoration: TextDecoration.none` to prevent underlines when
rendered inside `IgnorePointer` overlay. Wrap in `Container` with
`constraints: BoxConstraints(maxWidth: 220)`.

Import:
```dart
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
```

The widget does NOT use any API-prefixed import since it only receives data — it is a
pure display widget.

#### Step 3.2 — Verify

```bash
flutter analyze
```

No new errors. The file is not yet imported anywhere, so it has no effect.

---

### Phase 4 — Flutter: viewport integration

**Goal:** Tooltip appears on hover over any visible atom, correctly suppressed in all
conflict cases.

#### Step 4.1 — Add state fields

**File:** `lib/structure_designer/structure_designer_viewport.dart`

Add three fields to `_StructureDesignerViewportState` after the existing
`_elementAccumulator` field:

```dart
// Hover tooltip state
Timer? _hoverDebounceTimer;
APIHoveredAtomInfo? _hoveredAtomInfo;
Offset? _lastHoverPos;
```

Add imports at top of file:
```dart
import 'dart:async';  // for Timer — not currently imported in this file
import 'package:flutter_cad/common/atom_tooltip.dart';
```

The generated `queryHoveredAtomInfo` is already available via the existing import:
```dart
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as structure_designer_api;
```
Call it as `structure_designer_api.queryHoveredAtomInfo()`.

#### Step 4.2 — Add `_clearHoverTooltip()` helper

Add after the state fields, before `dispose()`:

```dart
void _clearHoverTooltip() {
  _hoverDebounceTimer?.cancel();
  if (_hoveredAtomInfo != null) {
    setState(() => _hoveredAtomInfo = null);
  }
}
```

#### Step 4.3 — Update `dispose()`

Add `_hoverDebounceTimer?.cancel();` as the first line of the existing `dispose()` method.
The method becomes:

```dart
@override
void dispose() {
  _hoverDebounceTimer?.cancel();
  _elementAccumulator.dispose();
  _focusNode.dispose();
  super.dispose();
}
```

#### Step 4.4 — Add `_scheduleHoverHitTest()` and `_performHoverHitTest()`

Add after `_clearHoverTooltip()`:

```dart
void _scheduleHoverHitTest(Offset pos) {
  _hoverDebounceTimer?.cancel();

  // Suppress while AddAtom tool is active (it has its own cursor label)
  if (widget.graphModel.isNodeTypeActive('atom_edit') &&
      widget.graphModel.activeAtomEditTool == APIAtomEditTool.addAtom) {
    return;
  }

  _lastHoverPos = pos;
  _hoverDebounceTimer = Timer(const Duration(milliseconds: 100), () {
    _performHoverHitTest(pos);
  });
}

void _performHoverHitTest(Offset pos) {
  final ray = getRayFromPointerPos(pos);
  final info = structure_designer_api.queryHoveredAtomInfo(
    rayOrigin: vector3ToApiVec3(ray.start),
    rayDirection: vector3ToApiVec3(ray.direction),
  );
  if (mounted && _lastHoverPos == pos) {
    setState(() => _hoveredAtomInfo = info);
  }
}
```

Note: `getRayFromPointerPos` is inherited from `CadViewportState` (defined in
`lib/common/cad_viewport.dart:329`). `vector3ToApiVec3` is from
`lib/common/api_utils.dart` (already imported in this file).

#### Step 4.5 — Modify `_onHover()`

Replace the current `_onHover` body to add movement-threshold debounce before the
existing guided-placement tracking:

```dart
void _onHover(PointerHoverEvent event) {
  final pos = event.localPosition;
  setState(() => _cursorPosition = pos);

  // Movement threshold: suppress flicker from micro-movements (< 4 px).
  final moved = _lastHoverPos != null
      ? (pos - _lastHoverPos!).distance
      : double.infinity;
  if (moved >= 4.0) {
    _clearHoverTooltip();
  } else {
    _hoverDebounceTimer?.cancel();
  }

  _scheduleHoverHitTest(pos);

  // Existing guided placement tracking (unchanged)
  if (atom_edit_api.atomEditIsInGuidedPlacement()) {
    final ray = getRayFromPointerPos(event.localPosition);
    final changed = atom_edit_api.atomEditGuidedPlacementPointerMove(
      rayStart: vector3ToApiVec3(ray.start),
      rayDir: vector3ToApiVec3(ray.direction),
    );
    if (changed) {
      renderingNeeded();
    }
  }
}
```

#### Step 4.6 — Clear on pointer-down

Override `onPointerDown` in `_StructureDesignerViewportState`. The base class
`CadViewportState` defines `onPointerDown(PointerDownEvent)` as a regular method in
`cad_viewport.dart`. Add:

```dart
@override
void onPointerDown(PointerDownEvent event) {
  _clearHoverTooltip();
  super.onPointerDown(event);
}
```

#### Step 4.7 — Clear on exit

Modify the `onExit` handler in the `MouseRegion` to also clear the tooltip:

```dart
onExit: (_) {
  _clearHoverTooltip();
  setState(() => _cursorPosition = null);
},
```

#### Step 4.8 — Clear on tool switch

In `_onKeyEvent`, add `_clearHoverTooltip();` after each tool switch:
- After `setActiveAtomEditTool(APIAtomEditTool.default_)` (D key handler)
- After `setActiveAtomEditTool(APIAtomEditTool.addAtom)` (Q key handler)
- After `setActiveAtomEditTool(APIAtomEditTool.addBond)` (J key handler)

#### Step 4.9 — Clear on structure mutation

In the viewport's `refreshFromKernel()` override, add `_clearHoverTooltip();` as the
first line:

```dart
@override
void refreshFromKernel() {
  _clearHoverTooltip();
  widget.graphModel.refreshFromKernel();
  if (_springLoadedDeferRelease) {
    _completeSpringLoadedRelease();
  }
}
```

#### Step 4.10 — Build tooltip overlay in `build()`

In the `build()` method, after the `elementSymbolOverlay` block and before the
`return Focus(...)` statement, add:

```dart
// Build atom hover tooltip overlay
Widget? atomTooltipOverlay;
if (_hoveredAtomInfo != null) {
  final info = _hoveredAtomInfo!;
  final screenPos = _projectWorldToScreen(
    info.x, info.y, info.z,
  );
  if (screenPos != null) {
    const offsetX = 20.0;
    const offsetY = -10.0;
    const estW = 180.0;
    const estH = 70.0;

    final viewportSize = context.size;
    final vw = viewportSize?.width ?? 800.0;
    final vh = viewportSize?.height ?? 600.0;

    final left = (screenPos.dx + offsetX).clamp(4.0, vw - estW - 4.0);
    final top = (screenPos.dy + offsetY).clamp(4.0, vh - estH - 4.0);

    atomTooltipOverlay = Positioned(
      left: left,
      top: top,
      child: IgnorePointer(child: AtomTooltip(info: info)),
    );
  }
}
```

Then add it to the `Stack` children (after `elementSymbolOverlay`):

```dart
Stack(
  children: [
    super.build(context),
    if (_marqueeRect != null)
      Positioned.fill(
        child: IgnorePointer(
          child: CustomPaint(
            painter: MarqueePainter(rect: _marqueeRect!),
          ),
        ),
      ),
    if (addBondOverlay != null) addBondOverlay,
    if (elementSymbolOverlay != null) elementSymbolOverlay,
    if (atomTooltipOverlay != null) atomTooltipOverlay,
  ],
)
```

#### Step 4.11 — Verify

```bash
flutter analyze
dart format lib/common/atom_tooltip.dart lib/structure_designer/structure_designer_viewport.dart
```

No new errors.

---

### Phase 5 — Manual testing

#### Test cases

| # | Scenario | Expected |
|---|----------|----------|
| 1 | Hover idle over atom (single atom_edit node) | Tooltip appears after ~100 ms showing symbol, bonds, position |
| 2 | Hover over atom in atom_fill node (non-selected) | Tooltip appears (scene-wide hit test) |
| 3 | Move cursor quickly across atoms | No tooltip while moving; appears after stopping |
| 4 | Micro-movement (hand tremor < 4 px) | Tooltip stays, no flicker |
| 5 | Click and drag (camera rotate) | Tooltip disappears on pointer-down |
| 6 | Switch to AddAtom tool (Q key) | Tooltip disappears; suppressed while AddAtom is active |
| 7 | Switch to Default tool (D key) | Tooltip disappears; re-appears on next hover dwell |
| 8 | Spring-loaded AddBond (hold J) | Tooltip disappears on tool switch |
| 9 | Delete atom under cursor | Tooltip disappears (refreshFromKernel clears) |
| 10 | Move cursor off viewport | Tooltip disappears (onExit) |
| 11 | Hover over bond (not atom) | No tooltip (hit_test_all_atomic_structures only returns atoms) |
| 12 | Hover over empty space | No tooltip |
| 13 | Multi-node scene: 2+ visible atomic nodes | Correct atom identified (closest across all structures) |
| 14 | Orthographic camera | Tooltip positioned correctly via _projectWorldToScreen |
| 15 | Perspective camera | Tooltip positioned correctly |
| 16 | Tooltip near viewport edge | Clamped within bounds, not clipped |

---

### Summary of files changed

| File | Change | Phase |
|------|--------|-------|
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | Add `APIHoveredAtomInfo` struct | 1 |
| `rust/src/structure_designer/structure_designer.rs` | Add `hit_test_all_atomic_structures()` method | 1 |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Add `query_hovered_atom_info()` function + import | 1 |
| `lib/src/rust/` (generated) | FRB codegen output | 2 |
| `rust/src/frb_generated.rs` (generated) | FRB codegen output | 2 |
| `lib/common/atom_tooltip.dart` | **New file** — `AtomTooltip` widget | 3 |
| `lib/structure_designer/structure_designer_viewport.dart` | State fields, debounce, overlay, clearing | 4 |
