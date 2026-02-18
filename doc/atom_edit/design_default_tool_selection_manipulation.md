# Design: Default Tool — Selection & Manipulation

Detailed design for upgrading the atom_edit **Default tool** with marquee selection and direct manipulation. This replaces the current select-then-panel workflow with fluid click/drag interactions.

**Date:** 2026-02-18
**Scope:** Default tool only (AddAtom and AddBond tools unchanged)
**Prerequisite reading:** `doc/atom_edit_ux_research.md`

---

## 1. Current State

### What the Default tool does today

| Input | Behavior |
|---|---|
| Left-click atom | Select it (Replace / Shift=Expand / Ctrl=Toggle) |
| Left-click bond | Select it |
| Left-click empty | Nothing (selection unchanged) |
| Left-drag | Nothing (no drag handling) |
| Right-click | Camera rotate (Shift+Right = camera pan) |

After selecting, the user navigates to the **property panel** to delete, replace element, or apply transforms. There is no gadget, no marquee, no direct manipulation.

### Key limitations

- **No drag interaction at all** — left-drag is wasted
- **No marquee selection** — must click atoms one by one
- **No direct manipulation** — must use panel buttons for every operation
- **No gadget** — `provide_gadget()` returns `None`
- **Click on empty space** does nothing (should clear selection)

---

## 2. Design Overview

### Core principle: click vs drag threshold

A **pixel threshold** (5 device pixels) distinguishes click from drag:

- Mouse down → mouse up with < 5px movement → **click** (selection)
- Mouse down → mouse move > 5px → **drag** (manipulation or marquee)

This is the standard pattern used by Figma, Unity, Blender, and virtually all interactive editors.

### Interaction summary (new)

| Input | Target | Behavior |
|---|---|---|
| **Click** | Unselected atom | Select it (modifiers apply) |
| **Click** | Selected atom | Keep selection (Ctrl = deselect it) |
| **Click** | Bond | Select it (modifiers apply) |
| **Click** | Empty space | Clear selection |
| **Drag** | Unselected atom | Select it, then screen-plane drag |
| **Drag** | Selected atom | Screen-plane drag entire selection |
| **Drag** | Gadget handle | Axis-constrained drag (existing gadget system) |
| **Drag** | Empty space | Marquee rectangle selection |
| Right-click | Atom | Delete it (Phase 1 shortcut, separate design) |
| Right-click | Empty | Camera rotate (unchanged) |

Shift and Ctrl modifiers apply to all selection actions (click and marquee) with existing semantics: Shift = Expand, Ctrl = Toggle, neither = Replace.

---

## 3. Interaction State Machine

The Default tool transitions through states based on mouse events:

```
                  ┌──────────────────────────────────────────────┐
                  │                    IDLE                        │
                  │           (waiting for mouse-down)             │
                  └──┬──────────┬──────────┬──────────┬──────────┘
                     │          │          │          │
               mouse-down mouse-down mouse-down mouse-down
               on gadget  on atom    on bond    on empty
                     │          │          │          │
                     v          v          v          v
               (existing  ┌──────────┐ ┌──────────┐ ┌──────────┐
                gadget *)  │ PENDING  │ │ PENDING  │ │ PENDING  │
                           │ ATOM     │ │ BOND     │ │ MARQUEE  │
                           └─────┬────┘ └─────┬────┘ └─────┬────┘
                                 │            │            │
                        ┌────────┴──────┐   mouse-up  moved > 5px?
                   moved > 5px?    < 5px      │            │
                        │            │        │            v
                        v       mouse-up      │      ┌──────────┐
                 ┌──────────┐      │          │      │ MARQUEE  │
                 │ SCREEN   │      └─────┬────┘      │ ACTIVE   │
                 │ PLANE    │            v           └──────────┘
                 │ DRAGGING │      ┌──────────┐
                 └──────────┘      │  CLICK   │
                                   │  SELECT  │
                                   └──────────┘

  * Gadget drag is handled by the existing gadget system, not by
    DefaultToolInteractionState. Shown here for completeness.
```

### State definitions

#### IDLE
- Default state. No mouse button held.
- Gadget is visible if atoms are selected.

#### PENDING_ATOM
- Mouse-down occurred on an atom. Threshold not yet exceeded.
- Record: hit atom ID, mouse-down position, **snapshot of the current selection** (for potential revert).
- **If atom was unselected:** apply the selection modifier immediately as a **tentative selection** so that any subsequent drag operates on the correct selection set:
  - Replace: clear all others, select only this atom
  - Shift (Expand): add this atom to the existing selection
  - Ctrl (Toggle): add this atom to the existing selection (toggling OFF is only for click, not drag)
- **If atom was already selected:** do nothing yet (preserve selection for potential drag).
- On mouse-up without exceeding threshold → transition to CLICK_SELECT, which uses the snapshot to apply final modifier logic (e.g., Ctrl-click on an already-selected atom reverts the tentative selection and deselects the atom instead).

#### PENDING_BOND
- Mouse-down occurred on a bond. Threshold is tracked but **does not change behavior** — bonds are not draggable.
- Record: hit bond index, mouse-down position.
- On mouse-up (regardless of threshold): transition to CLICK_SELECT, which selects the bond with the active modifier.

#### PENDING_MARQUEE
- Mouse-down occurred on empty space. Threshold not yet exceeded.
- Record: mouse-down position.

#### CLICK_SELECT
- Threshold was never exceeded, mouse-up occurred.
- Apply selection logic based on what was under the mouse-down point:
  - **Atom (unselected):** Select with modifier (Replace/Expand/Toggle)
  - **Atom (already selected):** With Replace modifier: do nothing (keep selection). With Toggle: deselect it.
  - **Bond:** Select with modifier
  - **Empty:** Clear all selection
- Transition → IDLE.

#### SCREEN_PLANE_DRAGGING
- Threshold exceeded after mouse-down on atom.
- All selected atoms move on the **camera-parallel plane** passing through the selection centroid.
- Mouse-move: compute world-space delta on that plane, apply to all selected atoms.
- Mouse-up: commit positions, transition → IDLE.

#### MARQUEE_ACTIVE
- Threshold exceeded after mouse-down on empty space.
- Draw selection rectangle overlay on viewport.
- All atoms whose screen-space projections fall inside the rectangle are **previewed** as selected (visual highlight).
- Mouse-up: commit marquee selection (with modifiers), transition → IDLE.

#### GADGET_DRAGGING (external — not tracked in DefaultToolInteractionState)
- Mouse-down hit a gadget handle. The delegate starts the existing gadget drag system and then **defers move/up to the base class**, which drives the gadget drag with its existing pipeline.
- Axis-constrained translation or rotation.
- Mouse-up: commit, transition → IDLE.

---

## 4. Marquee Selection

### Screen-space projection approach

The simplest and most predictable approach: project all atom positions to 2D screen coordinates, then test containment in the 2D rectangle.

```
For each atom in the result structure:
    screen_pos = project_to_screen(atom.position)
    if screen_pos is inside marquee rectangle:
        include in selection
```

This is preferred over frustum-based selection because:
- Easier to implement and debug
- Matches user expectation (what you see is what you select)
- Works identically in orthographic and perspective projection
- Atoms behind other atoms but visually inside the rectangle ARE selected (consistent with most 3D editors)

### Projection (Rust-internal, not an API function)

Marquee hit testing requires projecting 3D atom positions to 2D screen coordinates. This is a **Rust-internal utility function** — it runs inside a loop over all atoms on the Rust side, never crossing the FFI boundary per-atom.

```rust
/// Rust-internal helper. Projects a world position to screen coordinates.
/// Returns None if the point is behind the camera.
fn project_to_screen(
    world_pos: DVec3,
    view_proj: &DMat4,
    viewport_width: f64,
    viewport_height: f64,
) -> Option<DVec2>
```

The view-projection matrix is obtained from `Camera::build_view_projection_matrix()` which already exists in `rust/src/renderer/camera.rs`. The `StructureDesigner` can access it via `cad_instance.renderer.camera`.

This is the inverse of Flutter's `getRayFromPointerPos()` — world → screen instead of screen → world.

### Marquee visual overlay

- Rendered as a 2D rectangle on the Flutter canvas (not in the 3D scene).
- Semi-transparent fill with solid border (standard selection rectangle look).
- Color: matches the existing selection highlight color.

### Camera access for projection

The marquee commit needs the view-projection matrix. The API layer already has access to the full `CadInstance` via `with_mut_cad_instance`, which includes `cad_instance.renderer.camera`. The camera's `build_view_projection_matrix()` method already exists. No new data needs to cross the FFI boundary — Flutter sends the screen rectangle, Rust does everything else internally.

**Data flow for marquee commit:**
```
Flutter:  pointer_up(screen_pos, ray, modifier, viewport_w, viewport_h)
    │
    ▼
Rust API:  with_mut_cad_instance(|cad_instance| {
    │         let vp_matrix = cad_instance.renderer.camera.build_view_projection_matrix();
    │         let structure = get_result_structure(cad_instance);
    │         select_atoms_in_screen_rect(structure, vp_matrix, rect, viewport, modifier);
    │       })
    │
    ▼
Flutter:  receives PointerUpResult::MarqueeCommitted
```

### Modifier behavior during marquee

| Modifier | Behavior |
|---|---|
| None (Replace) | Clear previous selection, select all atoms in rectangle |
| Shift (Expand) | Add atoms in rectangle to existing selection |
| Ctrl (Toggle) | Toggle selection state of atoms in rectangle |

### Preview during drag

While the marquee is being dragged, atoms inside the rectangle should show a **preview highlight** (e.g., a dimmer version of the selection color). This gives immediate feedback about what will be selected on release.

Implementing preview requires either:
- **(A)** Running the screen-space test every frame during drag and sending a "preview selection" set to the renderer — accurate but potentially expensive for large structures.
- **(B)** Only updating the preview every N milliseconds (throttled) — good compromise.
- **(C)** Only showing the rectangle, no atom preview — simplest, acceptable for v1.

**Recommendation:** Start with **(C)** for v1. The rectangle alone provides sufficient feedback. Add preview highlighting later if users request it.

---

## 5. Screen-Plane Dragging

### The constraint plane

When dragging atoms, movement is constrained to a plane that is:
- **Parallel to the camera's view plane** (perpendicular to the camera forward direction)
- **Passing through the selection centroid** (or the clicked atom's position if single atom)

This is the same approach Avogadro 2 uses, and it's the most intuitive: atoms move "with the mouse" from the user's perspective.

```
Camera forward direction: F
Selection centroid: C
Constraint plane: P(x) where dot(x - C, F) = 0
```

### Computing world-space delta

On each mouse-move during drag:

```
1. Get mouse ray from current pointer position
2. Intersect ray with constraint plane → world_pos_current
3. Delta = world_pos_current - world_pos_at_drag_start
4. Apply delta to all selected atoms
```

The intersection of a ray with a plane is:

```
t = dot(C - ray_origin, F) / dot(ray_direction, F)
intersection = ray_origin + t * ray_direction
```

Where `F` is the plane normal (camera forward) and `C` is a point on the plane (centroid).

### Applying movement to selected atoms

Reuse the existing `transform_selected()` infrastructure but with a relative translation:

```rust
fn drag_selected_atoms(
    structure_designer: &mut StructureDesigner,
    delta: DVec3,  // world-space displacement
)
```

This applies `delta` to every selected atom's position in the diff. The implementation is similar to `transform_selected()` but takes a simple displacement vector rather than a full `Transform`.

### Drag start behavior

When drag starts on an **unselected** atom:
1. First, select that atom (Replace mode, unless Shift/Ctrl held)
2. Then begin dragging the (now single-atom) selection

When drag starts on an **already selected** atom:
1. Do not change selection
2. Begin dragging the entire current selection

This means: click-drag on any atom always works. You never need a separate "move" step.

### Committing the drag

On mouse-up:
- The atoms are already at their new positions (updated incrementally during drag).
- Mark the diff as dirty.
- Trigger a full network refresh (downstream nodes re-evaluate).
- Update `selection_transform` to reflect new positions.

### Lightweight refresh during drag

During drag, use **lightweight refresh** (re-render only, no network re-evaluation) for smooth performance. The full re-evaluation happens only on mouse-up. This matches the existing gadget drag pattern.

**Mechanism:** During `ScreenPlaneDragging`, `pointer_move` modifies atom positions in the diff directly and calls `mark_needs_render()` on the `StructureDesigner` — this triggers a re-tessellation of the affected atoms and gadget without re-evaluating the node network. On `pointer_up`, the handler calls `refresh_structure_designer_auto(cad_instance)` to trigger a full network re-evaluation (downstream nodes see the new positions). The `pointer_move` return value `Dragging` tells Flutter to call `renderingNeeded()` but **not** `refreshFromKernel()` — the full refresh happens only on `pointer_up`.

---

## 6. Selection Gadget

### When to show

A transform gadget appears at the selection centroid whenever **one or more atoms are selected** in the Default tool. This uses the existing `XYZ gadget` infrastructure.

### Gadget specification

```rust
pub struct AtomEditSelectionGadget {
    pub center: DVec3,           // Selection centroid
    pub dragged_handle: Option<i32>,
    pub start_drag_offset: f64,
    pub start_drag_center: DVec3,
    // Snapshot of selected atom positions at drag start
    pub drag_start_positions: Vec<(u32, DVec3)>,  // (atom_id, position)
}
```

The gadget provides:
- **3 translation axes** (X=red, Y=green, Z=blue arrows) — handle indices 0, 1, 2
- **No rotation handles** for v1 (rotation via panel is sufficient; add later if needed)

### Gadget interaction

- Click-drag on an axis arrow → constrained translation along that axis for all selected atoms
- The gadget uses the existing `xyz_gadget_utils` for hit testing, rendering, and axis offset calculation
- Gadget axes are **world-aligned** (not local to selection), matching the `AtomMoveGadget` pattern

### Hit test priority

The gadget hit test must happen **before** the atom hit test in the mouse-down handler:

```
1. gadget.hit_test(ray)  →  if hit: GADGET_DRAGGING (existing system)
2. atom_hit_test(ray)    →  if hit: PENDING_ATOM
3. bond_hit_test(ray)    →  if hit: PENDING_BOND
4. nothing hit           →  PENDING_MARQUEE
```

This ensures clicking on a gadget arrow doesn't accidentally select/deselect the atom behind it.

### Gadget vs atom_edit architecture

Currently, `provide_gadget()` returns `None` for atom_edit. The gadget system syncs data back via `NodeNetworkGadget::sync_data()`, which writes to `NodeData` fields. But atom_edit's selection transform isn't a simple field — it's derived from selected atom positions.

**Two approaches:**

**(A) Use the existing gadget system.** Implement `NodeNetworkGadget` for `AtomEditSelectionGadget`. The `sync_data()` method would compute the delta from the gadget's position change and apply it to all selected atoms in the diff. This integrates cleanly with the existing viewport drag pipeline.

**(B) Handle gadget internally in atom_edit.** atom_edit manages its own gadget rendering and hit testing, bypassing the `provide_gadget()` system. This gives more control but duplicates infrastructure.

**Recommendation: (A)** — use the existing system. The `sync_data()` method is the only tricky part, and it's straightforward: compute the displacement from the original centroid to the gadget's current position, then apply that displacement to all selected atoms.

---

## 7. Implementation Plan

### Rust changes

#### 7.1 New: Interaction state machine (`atom_edit.rs`)

Add to `DefaultToolState`:

```rust
pub struct DefaultToolState {
    pub replacement_atomic_number: i16,
    pub interaction_state: DefaultToolInteractionState,
}

pub enum DefaultToolInteractionState {
    Idle,
    PendingAtom {
        hit_atom_id: u32,
        hit_is_base: bool,        // true = base atom, false = diff atom
        was_selected: bool,        // was the atom already selected before mouse-down?
        mouse_down_screen: DVec2,  // screen position at mouse-down
        mouse_down_ray: (DVec3, DVec3),  // (origin, direction)
        selection_snapshot: Vec<(u32, bool)>,  // snapshot of selection before tentative change
    },
    PendingBond {
        bond_index: usize,
        is_base: bool,
        mouse_down_screen: DVec2,
    },
    PendingMarquee {
        mouse_down_screen: DVec2,
    },
    ScreenPlaneDragging {
        plane_normal: DVec3,       // camera forward
        plane_point: DVec3,        // centroid at drag start
        last_world_pos: DVec3,     // world position on plane at last frame
        start_world_pos: DVec3,    // world position on plane at drag start
    },
    MarqueeActive {
        start_screen: DVec2,
        current_screen: DVec2,
    },
    // GadgetDragging is handled by the existing gadget system, not here
}
```

#### 7.2 New API: Mouse event functions

Replace the single `select_atom_or_bond_by_ray()` entry point with a proper event-driven API. All three functions use `with_mut_cad_instance` internally — they access the full `CadInstance`, which includes both the `StructureDesigner` (via `selected_structure_designer()`) and `renderer.camera` (needed for the view-projection matrix in marquee selection). They do **not** take `StructureDesigner` as a parameter.

```rust
// Called on mouse-down
pub fn default_tool_pointer_down(
    screen_pos: DVec2,          // 2D screen position (for threshold checking)
    ray_origin: DVec3,
    ray_direction: DVec3,
    select_modifier: SelectModifier,
) -> PointerDownResult

pub enum PointerDownResult {
    GadgetHit { handle_index: i32 },  // Flutter should use existing gadget drag
    StartedOnAtom,                     // Entered PendingAtom state
    StartedOnBond,                     // Entered PendingBond state
    StartedOnEmpty,                    // Entered PendingMarquee state
}

// Called on every mouse-move while button held
pub fn default_tool_pointer_move(
    screen_pos: DVec2,
    ray_origin: DVec3,
    ray_direction: DVec3,
    viewport_width: f64,
    viewport_height: f64,
) -> PointerMoveResult

pub enum PointerMoveResult {
    StillPending,                     // Threshold not exceeded yet
    Dragging,                         // Screen-plane drag in progress
    MarqueeUpdated { rect: [f64; 4] }, // Marquee rectangle [x, y, w, h] in screen coords
}

// Called on mouse-up
pub fn default_tool_pointer_up(
    screen_pos: DVec2,
    ray_origin: DVec3,
    ray_direction: DVec3,
    select_modifier: SelectModifier,
    viewport_width: f64,
    viewport_height: f64,
) -> PointerUpResult

pub enum PointerUpResult {
    SelectionChanged,     // Click-select happened
    DragCommitted,        // Screen-plane drag finished
    MarqueeCommitted,     // Marquee selection applied
    NothingHappened,      // Click on empty with no prior selection (no-op)
}
```

#### 7.3 New: Marquee selection commit (Rust-internal)

The marquee selection is committed entirely on the Rust side. When `pointer_up` is called in `MarqueeActive` state:

```rust
/// Called internally by pointer_up when marquee is committed.
/// NOT an API function — runs entirely in Rust.
/// The view_proj matrix is computed by the caller from cad_instance.renderer.camera.
fn select_atoms_in_screen_rect(
    structure_designer: &mut StructureDesigner,
    view_proj: &DMat4,             // from camera.build_view_projection_matrix()
    screen_rect: (DVec2, DVec2),   // min and max corners in screen coords
    viewport_width: f64,
    viewport_height: f64,
    select_modifier: SelectModifier,
) -> bool  // true if selection changed
```

This function:
1. Gets the view-projection matrix from `cad_instance.renderer.camera.build_view_projection_matrix()` (already exists in `camera.rs`)
2. Gets the result atomic structure from the selected node
3. Loops over all atoms, calling the internal `project_to_screen()` helper for each
4. Tests each projected position against the screen rectangle
5. Collects matching atom IDs (resolving provenance for base vs diff atoms)
6. Applies the selection modifier to the selection sets
7. Recalculates `selection_transform`

The per-atom projection helper is a simple Rust-internal function (see Section 4).

#### 7.4 New: Selection gadget

```rust
pub struct AtomEditSelectionGadget {
    center: DVec3,
    original_center: DVec3,
    dragged_handle: Option<i32>,
    start_drag_offset: f64,
}

impl Gadget for AtomEditSelectionGadget { ... }
impl NodeNetworkGadget for AtomEditSelectionGadget { ... }
```

Update `AtomEditData::provide_gadget()` to return `Some(...)` when atoms are selected.

#### 7.5 Modified: Selection functions

- `select_atom_or_bond_by_ray()` — refactored, logic moved into state machine functions
- New `select_atoms_in_screen_rect()` — for marquee selection commit
- New `drag_selected_by_delta()` — applies world-space displacement to all selected atoms

### Flutter changes

#### 7.6 Modified: Viewport event handling

`structure_designer_viewport.dart` needs to replace the current `onAtomEditClick()` with a proper mouse event pipeline:

```dart
// Override pointer handlers for atom_edit Default tool
void onAtomEditPointerDown(Offset screenPos, Ray ray, SelectModifier modifier) {
    final result = atom_edit_api.defaultToolPointerDown(
        screenPos, ray.start, ray.direction, modifier);

    if (result == PointerDownResult.gadgetHit) {
        // Delegate to existing gadget drag system
        startGadgetDrag(result.handleIndex, ray);
    }
    // PendingAtom and PendingMarquee states are tracked in Rust
}

void onAtomEditPointerMove(Offset screenPos, Ray ray) {
    final result = atom_edit_api.defaultToolPointerMove(
        screenPos, ray.start, ray.direction, viewportWidth, viewportHeight);

    if (result is MarqueeUpdated) {
        marqueeRect = result.rect;  // Store for painting
        renderingNeeded();
    } else if (result == PointerMoveResult.dragging) {
        renderingNeeded();  // Atoms moved, need re-render
    }
}

void onAtomEditPointerUp(Offset screenPos, Ray ray, SelectModifier modifier) {
    final result = atom_edit_api.defaultToolPointerUp(
        screenPos, ray.start, ray.direction, modifier, viewportWidth, viewportHeight);

    marqueeRect = null;  // Clear marquee overlay
    refreshFromKernel();
    renderingNeeded();
}
```

#### 7.7 New: Marquee rectangle overlay

Paint the marquee rectangle on the Flutter canvas layer (on top of the 3D viewport):

```dart
// In viewport paint method
if (marqueeRect != null) {
    final paint = Paint()
        ..color = selectionColor.withOpacity(0.15)
        ..style = PaintingStyle.fill;
    canvas.drawRect(marqueeRect!, paint);

    final borderPaint = Paint()
        ..color = selectionColor
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1.0;
    canvas.drawRect(marqueeRect!, borderPaint);
}
```

### API surface changes

| New API function | Direction | Purpose |
|---|---|---|
| `default_tool_pointer_down()` | Flutter → Rust | Start interaction (hit test, enter pending state) |
| `default_tool_pointer_move()` | Flutter → Rust | Continue interaction (drag atoms / update marquee rect) |
| `default_tool_pointer_up()` | Flutter → Rust | Commit interaction (click-select / finish drag / commit marquee) |

These are the **only 3 new FFI-crossing functions**. All heavy work (atom projection loops, selection updates, position deltas) happens inside Rust. Flutter only sends screen coordinates, rays, viewport dimensions, and modifiers.

| New Rust-internal function | Purpose |
|---|---|
| `project_to_screen()` | Project one world position to screen coords (used in loop) |
| `select_atoms_in_screen_rect()` | Loop all atoms, project, test rect, update selection |
| `drag_selected_by_delta()` | Apply world-space displacement to all selected atom positions |

| Modified | Change |
|---|---|
| `AtomEditData::provide_gadget()` | Return `Some(AtomEditSelectionGadget)` when atoms selected |
| `select_atoms_in_screen_rect()` | Added `view_proj: &DMat4` parameter (from camera) |
| `atom_edit_select_by_ray()` | Deprecated — logic moves into `pointer_down` / `pointer_up` |

---

## 8. Edge Cases

### Drag threshold on high-DPI displays

The 5px threshold should be in **logical pixels** (device-independent), not physical pixels. Flutter's `Offset` is already in logical pixels, so no conversion needed.

### Drag starting on atom near gadget

The gadget hit test uses cylinder/sphere intersection with the XYZ arrows. If the atom is near the gadget center, the gadget should win (tested first). The gadget's hit volumes are intentionally larger than their visual representation for easy clicking.

### Selection change during drag

Keyboard modifier state is captured at **mouse-down** and used for the entire interaction. Pressing/releasing Shift mid-drag does not change behavior.

### Empty selection after marquee

If the marquee rectangle contains no atoms, the behavior depends on modifier:
- Replace mode: clear selection (same as clicking empty space)
- Expand/Toggle mode: no change (adding/toggling nothing)

### Dragging a single atom onto another

Screen-plane dragging doesn't snap to lattice or other atoms. The atom moves freely on the plane. Lattice snapping is a separate feature (already exists for AddAtom). Energy minimization can be run afterward to find the nearest stable position.

### Performance during drag

- **Lightweight refresh** during drag: only re-tessellate the moved atoms and gadget, no network re-evaluation
- **Full refresh** on drag commit: re-evaluate downstream nodes
- For large selections (100+ atoms), the per-frame cost of updating positions should be monitored. If needed, defer position updates to every Nth frame.

### Undo interaction

This design does not add undo/redo (that's Phase 3 per the roadmap). A complete drag operation (mouse-down to mouse-up) modifies the diff atomically. If undo is later added, the entire drag should be one undo step.

---

## 9. What This Design Does NOT Cover

These are explicitly out of scope and will be separate designs:

- **Right-click context actions** (delete atom, delete bond) — separate Phase 1 feature
- **Keyboard element switching** (type "N" to change element) — separate Phase 1 feature
- **Bond order cycling** (click bond to cycle single/double/triple) — separate Phase 1 feature
- **Double-click select connected** (flood-fill selection) — Phase 2 feature
- **Rotation handles on the gadget** — can be added later to the selection gadget
- **Undo/redo** — Phase 3 feature
- **Selection preview during marquee** (highlighting atoms inside rectangle in real-time) — v2 enhancement

---

## 10. Summary of Changes

```
RUST (atom_edit.rs)
├── DefaultToolState         MODIFIED  — add interaction_state
├── DefaultToolInteractionState  NEW   — state machine enum (includes PendingBond)
├── default_tool_pointer_down()  NEW   — mouse-down handler (gadget→atom→bond→empty)
├── default_tool_pointer_move()  NEW   — mouse-move handler
├── default_tool_pointer_up()    NEW   — mouse-up handler
├── select_atoms_in_screen_rect()  NEW — marquee selection (Rust-internal, not API)
├── drag_selected_by_delta()     NEW   — screen-plane drag (Rust-internal, not API)
├── project_to_screen()          NEW   — 3D→2D projection (Rust-internal, not API)
├── AtomEditSelectionGadget      NEW   — gadget for selected atoms
└── provide_gadget()           MODIFIED — return gadget when selected

RUST (atom_edit_api.rs)
├── default_tool_pointer_down()  NEW   — API wrapper
├── default_tool_pointer_move()  NEW   — API wrapper
└── default_tool_pointer_up()    NEW   — API wrapper

FLUTTER (cad_viewport.dart)
├── PrimaryPointerDelegate     NEW     — abstract delegate interface
├── primaryPointerDelegate     NEW     — protected getter (returns null by default)
├── startGadgetDragFromHandle() NEW    — start gadget drag without re-hit-testing
├── onPointerDown()          MODIFIED  — consult delegate for primary button
├── onPointerMove()          MODIFIED  — consult delegate for primary button
└── onPointerUp()            MODIFIED  — consult delegate for primary button

FLUTTER (structure_designer_viewport.dart)
├── AtomEditDefaultDelegate    NEW     — PrimaryPointerDelegate impl
├── primaryPointerDelegate   MODIFIED  — return delegate when atom_edit+default
├── _marqueeRect               NEW     — stored marquee rect for overlay
└── build()                  MODIFIED  — add marquee CustomPaint overlay

FLUTTER (atom_edit_editor.dart)
└── No changes needed (panel UI unchanged)
```

---

## 11. Flutter Architecture: PrimaryPointerDelegate Pattern

### Problem

The base `CadViewportState` owns the entire primary-button pipeline: click-threshold detection, gadget hit testing, and the `onDefaultClick()` dispatch. The atom_edit Default tool needs to **take over** left-button events (down/move/up) to forward them to Rust's state machine, while:

- Other tools (AddAtom, AddBond) and other node types keep the simple click-only path
- The existing gadget drag system continues to work unchanged
- Camera controls (right-click rotate, middle-click pan, scroll zoom) are unaffected

### Solution: Delegate hook in CadViewportState

Add a `PrimaryPointerDelegate` interface that can **consume** primary button events before the base class processes them. When the delegate returns `true`, the base class skips its own click-threshold / gadget logic. When it returns `false`, the base class runs normally.

```dart
/// Interface for tools that need to take over primary mouse button interactions.
abstract class PrimaryPointerDelegate {
  /// Called on primary button down. Return true to consume (base won't do
  /// click-threshold / gadget hit test). Return false to let base handle it.
  bool onPrimaryDown(Offset pos);

  /// Called on primary button move while consumed. Return true to consume.
  /// Return false to let base handle it (e.g., for gadget dragging).
  bool onPrimaryMove(Offset pos);

  /// Called on primary button up while consumed. Return true to consume.
  /// Return false to let base handle it.
  bool onPrimaryUp(Offset pos);
}
```

### Base class changes (cad_viewport.dart)

```dart
abstract class CadViewportState<T extends CadViewport> extends State<T> {
  // ...existing fields...

  bool _delegateConsumedDown = false;

  /// Override in subclass to provide a delegate for the active tool.
  @protected
  PrimaryPointerDelegate? get primaryPointerDelegate => null;

  /// Start a gadget drag from a known handle index (no Flutter-side hit test).
  /// Used when Rust already determined the gadget was hit.
  @protected
  void startGadgetDragFromHandle(int handleIndex, Offset pointerPos) {
    dragState = ViewportDragState.defaultDrag;
    _dragStartPointerPos = pointerPos;
    final ray = getRayFromPointerPos(pointerPos);
    isGadgetDragging = true;
    draggedGadgetHandle = transformDraggedGadgetHandle(handleIndex);
    gadgetStartDrag(
      handleIndex: draggedGadgetHandle,
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
    );
    renderingNeeded();
  }

  void onPointerDown(PointerDownEvent event) {
    if (event.kind == PointerDeviceKind.mouse) {
      switch (event.buttons) {
        case kPrimaryMouseButton:
          final delegate = primaryPointerDelegate;
          if (delegate != null && delegate.onPrimaryDown(event.localPosition)) {
            _delegateConsumedDown = true;
            return;
          }
          _delegateConsumedDown = false;
          startPrimaryDrag(event.localPosition);
          break;
        // secondary/middle unchanged...
      }
    }
  }

  void onPointerMove(PointerMoveEvent event) {
    if (event.kind == PointerDeviceKind.mouse) {
      if (_delegateConsumedDown) {
        final delegate = primaryPointerDelegate;
        if (delegate != null && delegate.onPrimaryMove(event.localPosition)) {
          return;
        }
        // Delegate declined move (gadget dragging) — fall through to base
      }
      // ...existing camera/gadget move handling...
    }
  }

  void onPointerUp(PointerUpEvent event) {
    if (event.kind == PointerDeviceKind.mouse) {
      if (_delegateConsumedDown) {
        final delegate = primaryPointerDelegate;
        if (delegate != null && delegate.onPrimaryUp(event.localPosition)) {
          _delegateConsumedDown = false;
          return;
        }
        // Delegate declined up (gadget dragging) — fall through to base
      }
      _delegateConsumedDown = false;
      endDrag(event.localPosition);
    }
  }
}
```

### Gadget handoff: delegate defers move/up to base

The critical design point: when Rust's `pointer_down` returns `GadgetHit`, the delegate **starts the existing gadget drag** and **returns true for down** (preventing `startPrimaryDrag` from double-starting the interaction), then **returns false for move/up**, letting `CadViewportState` drive the gadget drag with its existing `defaultDrag()` / `endDrag()` pipeline.

```dart
class AtomEditDefaultDelegate implements PrimaryPointerDelegate {
  final _StructureDesignerViewportState viewport;
  SelectModifier? _storedModifier;  // captured at down, used at up

  AtomEditDefaultDelegate({required this.viewport});

  @override
  bool onPrimaryDown(Offset pos) {
    final ray = viewport.getRayFromPointerPos(pos);
    _storedModifier = getSelectModifierFromKeyboard();

    final result = atom_edit_api.defaultToolPointerDown(
      screenPos: offsetToApiVec2(pos),
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
      selectModifier: _storedModifier!,
    );

    if (result is PointerDownResultGadgetHit) {
      // Hand off to the EXISTING gadget system. The delegate consumes the
      // down event (preventing startPrimaryDrag from double-starting), but
      // returns false on move/up so base class drives the gadget drag.
      viewport.startGadgetDragFromHandle(result.handleIndex, pos);
      return true;  // consumed — but move/up will defer to base for gadget
    }

    // PendingAtom, PendingBond, or PendingMarquee — delegate owns the interaction
    return true;
  }

  @override
  bool onPrimaryMove(Offset pos) {
    // If gadget is dragging, let base handle it
    if (viewport.isGadgetDragging) return false;

    final ray = viewport.getRayFromPointerPos(pos);
    final result = atom_edit_api.defaultToolPointerMove(
      screenPos: offsetToApiVec2(pos),
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
      viewportWidth: viewport.viewportWidth,
      viewportHeight: viewport.viewportHeight,
    );

    if (result is PointerMoveResultMarqueeUpdated) {
      viewport.setMarqueeRect(apiRectToRect(result.rect));
      viewport.renderingNeeded();
    } else if (result == PointerMoveResult.dragging) {
      viewport.renderingNeeded();
    }
    return true;
  }

  @override
  bool onPrimaryUp(Offset pos) {
    // If gadget is dragging, let base handle it
    if (viewport.isGadgetDragging) return false;

    final ray = viewport.getRayFromPointerPos(pos);
    atom_edit_api.defaultToolPointerUp(
      screenPos: offsetToApiVec2(pos),
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
      selectModifier: _storedModifier ?? SelectModifier.replace,
      viewportWidth: viewport.viewportWidth,
      viewportHeight: viewport.viewportHeight,
    );

    viewport.setMarqueeRect(null);  // clear marquee overlay
    viewport.refreshFromKernel();
    viewport.renderingNeeded();
    return true;
  }
}
```

### Viewport subclass changes (structure_designer_viewport.dart)

```dart
class _StructureDesignerViewportState
    extends CadViewportState<StructureDesignerViewport> {
  AtomEditDefaultDelegate? _atomEditDefaultDelegate;
  Rect? _marqueeRect;

  void setMarqueeRect(Rect? rect) {
    setState(() => _marqueeRect = rect);
  }

  @override
  PrimaryPointerDelegate? get primaryPointerDelegate {
    if (!widget.graphModel.isNodeTypeActive("atom_edit")) return null;
    final tool = atom_edit_api.getActiveAtomEditTool();
    if (tool != APIAtomEditTool.default_) return null;
    _atomEditDefaultDelegate ??= AtomEditDefaultDelegate(viewport: this);
    return _atomEditDefaultDelegate;
  }

  // onDefaultClick() still handles AddAtom, AddBond, facet_shell, edit_atom
  // (unchanged — only reached when delegate is null)

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        // ... existing Texture widget with Listener ...
        super.build(context),
        // Marquee overlay
        if (_marqueeRect != null)
          Positioned.fill(
            child: IgnorePointer(
              child: CustomPaint(
                painter: MarqueePainter(rect: _marqueeRect!),
              ),
            ),
          ),
      ],
    );
  }
}
```

### Marquee overlay painter

```dart
class MarqueePainter extends CustomPainter {
  final Rect rect;
  MarqueePainter({required this.rect});

  @override
  void paint(Canvas canvas, Size size) {
    final fillPaint = Paint()
      ..color = const Color(0x264FC3F7)  // light blue, ~15% opacity
      ..style = PaintingStyle.fill;
    canvas.drawRect(rect, fillPaint);

    final borderPaint = Paint()
      ..color = const Color(0xFF4FC3F7)  // light blue, solid
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.0;
    canvas.drawRect(rect, borderPaint);
  }

  @override
  bool shouldRepaint(MarqueePainter oldDelegate) => rect != oldDelegate.rect;
}
```

### Event flow summary

```
Primary mouse down
  │
  ├─ delegate active? ──no──► base startPrimaryDrag() (existing path)
  │
  └─ yes ──► delegate.onPrimaryDown()
              │
              ├─ Rust returns GadgetHit ──► startGadgetDragFromHandle()
              │                              return true (consumed; move/up defer to base)
              │
              └─ Rust returns PendingAtom/PendingMarquee
                  return true (delegate owns drag)

Primary mouse move
  │
  ├─ delegate consumed down? ──no──► base defaultDrag() / cameraMove()
  │
  └─ yes ──► delegate.onPrimaryMove()
              │
              ├─ isGadgetDragging? ──► return false (base runs dragGadget())
              │
              └─ call Rust pointer_move
                  ├─ MarqueeUpdated ──► store rect, repaint
                  └─ Dragging ──► repaint (atoms moved)

Primary mouse up
  │
  ├─ delegate consumed down? ──no──► base endDrag() (click or gadget end)
  │
  └─ yes ──► delegate.onPrimaryUp()
              │
              ├─ isGadgetDragging? ──► return false (base runs gadgetEndDrag())
              │
              └─ call Rust pointer_up ──► clear marquee, refresh
```

---

## 12. Implementation Plan

### Phase A: Infrastructure (Flutter delegate + Rust state machine skeleton)

**Goal:** Wire up the delegate pattern and state machine without changing any visible behavior.

| Step | Layer | Description |
|------|-------|-------------|
| A1 | Flutter | Add `PrimaryPointerDelegate` interface and hook into `CadViewportState` pointer handlers |
| A2 | Flutter | Extract `startGadgetDragFromHandle()` from `startPrimaryDrag()` |
| A3 | Rust | Add `DefaultToolInteractionState` enum to `DefaultToolState` (starts at `Idle`) |
| A4 | Rust | Add `default_tool_pointer_down/move/up` API stubs that return no-op results |
| A5 | Rust | Regenerate FFI bindings |
| A6 | Flutter | Implement `AtomEditDefaultDelegate` that calls the Rust stubs |
| A7 | Flutter | Override `primaryPointerDelegate` in `StructureDesignerViewportState` |
| A8 | | **Verify:** app runs, existing click-select still works via `onDefaultClick()` fallback, camera/gadget unaffected |

### Phase B: Click-select through the new pipeline

**Goal:** Click-select works through the delegate → Rust state machine path instead of `onDefaultClick()`.

| Step | Layer | Description |
|------|-------|-------------|
| B1 | Rust | Implement `pointer_down` hit testing: gadget → atom → bond → empty → set Pending state |
| B2 | Rust | Implement `pointer_up` click-select: apply selection with modifier logic (atoms and bonds) |
| B3 | Rust | Implement `pointer_move` threshold check (but only transition to `Idle` / `StillPending`) |
| B4 | Rust | Handle click-on-empty → clear selection |
| B5 | Flutter | Wire delegate to use real results from Rust |
| B6 | | **Verify:** click-select atoms/bonds works, click-empty clears, Shift/Ctrl modifiers work, gadget hit hands off to existing system, bond click-drag does not start marquee or move |

### Phase C: Marquee selection

**Goal:** Drag on empty space draws a selection rectangle and selects enclosed atoms on release.

| Step | Layer | Description |
|------|-------|-------------|
| C1 | Rust | Implement `PendingMarquee` → `MarqueeActive` transition on threshold exceeded |
| C2 | Rust | Return `MarqueeUpdated { rect }` from `pointer_move` |
| C3 | Rust | Implement `project_to_screen()` helper |
| C4 | Rust | Implement `select_atoms_in_screen_rect()` called from `pointer_up` in `MarqueeActive` |
| C5 | Flutter | Add `MarqueePainter` and `_marqueeRect` state to viewport |
| C6 | Flutter | Store/clear marquee rect in delegate move/up handlers |
| C7 | | **Verify:** drag on empty draws rectangle, atoms inside are selected on release, modifiers work, rectangle clears on release |

### Phase D: Screen-plane atom dragging

**Goal:** Drag on a selected atom moves all selected atoms on the camera-parallel plane.

| Step | Layer | Description |
|------|-------|-------------|
| D1 | Rust | Implement `PendingAtom` → `ScreenPlaneDragging` transition on threshold exceeded |
| D2 | Rust | Compute constraint plane (camera-parallel, through centroid) |
| D3 | Rust | Implement `drag_selected_by_delta()` — apply world displacement to selected atoms |
| D4 | Rust | Implement ray-plane intersection in `pointer_move` for `ScreenPlaneDragging` |
| D5 | Rust | Implement `pointer_up` commit: mark diff dirty, trigger refresh |
| D6 | Rust | Handle drag-start on unselected atom: select first, then drag |
| D7 | | **Verify:** drag selected atoms moves them, drag unselected atom selects + moves, release commits positions, downstream nodes re-evaluate |

### Phase E: Selection gadget

**Goal:** Translation gadget appears at selection centroid, axis-constrained dragging works.

| Step | Layer | Description |
|------|-------|-------------|
| E1 | Rust | Implement `AtomEditSelectionGadget` struct |
| E2 | Rust | Implement `Gadget` and `NodeNetworkGadget` traits |
| E3 | Rust | Update `provide_gadget()` to return gadget when atoms are selected |
| E4 | Rust | Implement `sync_data()` — compute delta, apply to selected atoms |
| E5 | | **Verify:** gadget appears when atoms selected, axis-drag works, gadget disappears when selection cleared |

### Phase F: Polish and edge cases

**Goal:** Handle remaining edge cases and clean up.

| Step | Layer | Description |
|------|-------|-------------|
| F1 | Rust | Handle pointer cancel / reset interaction state |
| F2 | Rust | Lightweight refresh during drag (no network re-eval until commit) |
| F3 | Rust | Empty marquee with Replace modifier clears selection |
| F4 | Flutter | Invalidate / recreate delegate when switching nodes or tools |
| F5 | Flutter | Clean up `onAtomEditClick` — remove Default tool branch (now handled by delegate) |
| F6 | | **Verify:** full integration test — all interactions work together, tool switching is clean, no regressions in other node types |
