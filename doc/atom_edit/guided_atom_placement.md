# Guided Atom Placement — Design Document

---

# Part I — Design

---

## 1. Problem Statement

Currently, atomCAD's **Add Atom** tool places atoms via ray-plane intersection with no
awareness of bonding geometry, valence, or neighboring atoms. The user must manually
position each atom at the correct bond distance and angle, then manually add bonds.

**Guided atom placement:** when the user clicks an existing atom, the system computes
chemically valid candidate positions for a new bonded atom and displays them as
interactive guide points. Clicking a guide point places and bonds the atom in one action.

## 2. Scope

**In scope:** sp3/sp2/sp1 guided placement (all bond-count cases), dihedral-aware
positioning, saturation detection, integration with existing Add Atom tool.

**Out of scope:** Dative bonds, exotic chemistry, metal haptic bonds, aromatic ring
shortcuts, auto-chain building.

## 3. Tool Integration Decision

**Extend the existing Add Atom tool** rather than creating a separate tool:
- **Click empty space** → current free placement (unchanged)
- **Click existing atom** → enter guided placement mode (new)

This works because: (1) free placement uses a camera-facing plane at nearby-atom depth,
so the ray never "reaches through" to distant atoms; (2) atom hit radius is only
0.3-0.5 A, leaving plenty of gap for free placement clicks; (3) guided mode is easily
cancelled via Escape or clicking empty space; (4) Shift-click can force free placement
as a fallback if needed.

## 4. User Workflow

```
User selects Add Atom tool, picks an element (e.g., Carbon)
  |
  +- Clicks empty space -> atom placed at ray-plane intersection (unchanged)
  |
  +- Clicks existing atom (the "anchor") -> system enters Guided Placement mode
       |
       +- Anchor is saturated -> flash feedback, stay in Idle
       |
       +- Anchor has open bonding sites -> show guide dots
            |
            +- User clicks a guide dot -> atom placed, bond created, return to Idle
            +- User clicks empty space -> cancel, return to Idle
            +- User clicks a different atom -> switch anchor, recompute guides
            +- User presses Escape -> cancel, return to Idle
```

**Anchor selection:** ray-cast hit test -> determine hybridization via UFF atom type
assignment -> count existing neighbors -> compute candidate positions.

**Placement:** add new atom at guide position, create single bond, clear guides,
return to Idle.

## 5. Visual Design

### 5.1 Guide dots

| Property         | Value                                           |
|------------------|-------------------------------------------------|
| Shape            | Sphere                                          |
| Size (primary)   | ~0.2 A radius                                   |
| Size (secondary) | ~0.15 A radius                                  |
| Color            | Selection magenta (`to_selected_color`, rgb(1.0, 0.2, 1.0)) |

Selection magenta is the most distinct color from any element color. Gray would be
too close to carbon.

### 5.2 Trans vs cis differentiation (sp3 case 1 only)

When 6 guide dots are shown (3 trans + 3 cis), differentiate by **size only**:
trans = 0.20 A (preferred staggered), cis = 0.15 A (eclipsed).

### 5.3 Bond preview cylinders

Reuse existing **anchor arrow** rendering (orange cylinders, `ANCHOR_ARROW_COLOR`)
from diff view. No new rendering code needed.

### 5.4 Anchor highlight

Use existing `AtomDisplayState::Marked` (yellow `MARKER_COLOR`), same as Add Bond tool.

### 5.5 Saturation feedback

Brief red flash (~300ms) on the atom + status bar text "Atom is fully bonded".
No guide dots shown; tool stays in Idle.

### 5.6 Free placement sphere (case 0 — no existing bonds)

| Property    | Value                                        |
|-------------|----------------------------------------------|
| Shape       | Wireframe sphere centered on anchor          |
| Radius      | Sum of covalent radii (anchor + new element) |
| Color       | Gray (#606060)                               |
| Interaction | Click anywhere on user-facing hemisphere     |

A guide dot tracks the cursor on the sphere surface.

### 5.7 Free rotation ring (sp3 case 1 without dihedral reference)

When the anchor has exactly 1 bond but no dihedral reference is available:

| Property    | Value                                         |
|-------------|-----------------------------------------------|
| Shape       | Wireframe circle (cone intersection)          |
| Axis        | Along the existing bond direction             |
| Half-angle  | 109.47 deg from the existing bond (tetrahedral) |
| Interaction | 3 guide dots track cursor rotation on ring    |

The 3 positions maintain 120 deg spacing and rotate together. Clicking places one atom
at the clicked position.

## 6. Geometry Computation

### 6.1 Bond distance

```
bond_distance = covalent_radius(A) + covalent_radius(B)
```

Using `ATOM_INFO` from `atomic_constants.rs`. Special case: C-H uses 1.09 A
(`C_H_BOND_LENGTH`).

### 6.2 Hybridization detection

Use `assign_uff_type()` from `simulation/uff/typer.rs`:

| UFF suffix | Hybridization | Max neighbors | Geometry        |
|------------|---------------|---------------|-----------------|
| `_3`       | sp3           | 4             | Tetrahedral     |
| `_2`       | sp2           | 3             | Trigonal planar |
| `_1`       | sp1           | 2             | Linear          |
| `_R`       | Aromatic      | 3             | Trigonal planar |

**Fallback for bare atoms:** C,Si,Ge -> sp3; N,P -> sp3 (max 3); O,S -> sp3 (max 2);
B,Al -> sp2; Halogens -> max 1.

### 6.3 Saturation check

| Element group           | Hybridization | Effective max neighbors |
|-------------------------|---------------|------------------------|
| C, Si, Ge, Sn          | sp3           | 4                      |
| N, P, As, Sb           | sp3           | 3                      |
| O, S, Se, Te           | sp3           | 2                      |
| F, Cl, Br, I           | any           | 1                      |
| B, Al                  | sp2           | 3                      |
| C (double bond context)| sp2           | 3                      |
| C (triple bond context)| sp1           | 2                      |
| Noble gases            | --            | 0                      |

`remaining_slots = effective_max_neighbors - current_neighbor_count`

### 6.4 sp3 candidate positions (tetrahedral, 109.47 deg)

**Case 4 (saturated):** No guides.

**Case 3 (1 remaining):** `d4 = normalize(-(b1 + b2 + b3))` — opposite centroid
of existing directions.

**Case 2 (2 remaining):** Given `b1, b2`:
1. `mid = normalize(b1 + b2)`, `n = normalize(b1 x b2)`
2. Two unknowns lie symmetrically about `-mid` in the plane of `-mid` and `n`
3. Reconstruct: fit ideal tetrahedron to b1,b2 via Rodrigues' rotation, extract
   other two vertices

**Case 1 (3 remaining):** 3 directions on a cone of half-angle 109.47 deg around
`-b1`, spaced 120 deg apart. Requires **dihedral reference** for orientation:
- Walk upstream: A's neighbor B, look at B's other neighbor C
- If found: 6 dots (3 trans at 60 deg offset, 3 cis at 0 deg offset from reference)
- If not found: ring mode (see Section 5.7)

**Case 0 (4 remaining):** Free sphere (see Section 5.6).

### 6.5 sp2 candidate positions (trigonal planar, 120 deg)

**Case 3 (saturated):** No guides.

**Case 2 (1 remaining):** `d3 = normalize(-(b1 + b2))` — same logic as sp3 case 3,
result naturally lies in the b1-b2 plane.

**Case 1 (2 remaining):** Need planar reference from upstream topology. With
reference: 2 dots at +/-120 deg from b1 in that plane. Without: show a ring.

**Case 0:** Free sphere.

### 6.6 sp1 candidate positions (linear, 180 deg)

**Case 2 (saturated):** No guides.

**Case 1:** `d2 = -b1` — directly opposite the existing bond.

**Case 0:** Free sphere.

## 7. State Machine

### 7.1 Add Atom tool states

```
                    +--------------------------------------+
                    |                                      |
   +--------+  click atom   +------------------+          |
   |        |-------------->|                  |  click    |
   |  Idle  |               | GuidedPlacement  |--guide--> place atom
   |        |<--------------|                  |  dot      & bond,
   |        |  Esc / click  |  (anchor, dots)  |          return to Idle
   +--------+  empty space  +------------------+
       |                         |
       | click                   | click different atom
       | empty space             |
       v                         v
   place atom            recompute guides for
   (current behavior)    new anchor atom
```

### 7.2 Rust state representation (`types.rs`)

```rust
pub enum AddAtomToolState {
    Idle { atomic_number: i16 },
    GuidedPlacement {
        atomic_number: i16,
        anchor_atom_id: u32,
        guide_positions: Vec<GuideDot>,
    },
}

pub struct GuideDot {
    pub position: DVec3,
    pub dot_type: GuideDotType,
}

pub enum GuideDotType {
    Primary,    // Standard candidate position
    Secondary,  // e.g., cis position in sp3 case 1
}
```

### 7.3 Event handling

**Idle + pointer down:** hit test atoms -> if hit: compute guides, enter GuidedPlacement;
if miss: free placement (current behavior).

**GuidedPlacement + pointer down:** hit test guide dots first -> if hit: place atom +
bond, return to Idle; else hit test atoms -> if different atom: recompute guides;
if same/miss: cancel to Idle.

**Escape in GuidedPlacement:** Cancel to Idle.

## 8. Rendering Integration

### 8.1 Why not the gadget system

The gadget system (`NodeNetworkGadget`) was rejected because: (1) drag-centric API
vs our click interaction; (2) lifecycle tied to evaluation cycles vs transient tool
state; (3) `sync_data()` syncs continuous parameters vs our discrete action;
(4) the gadget slot is already used by `AtomEditSelectionGadget` (XYZ translation
gizmo for selection dragging).

### 8.2 Renderer capabilities

Four rendering paths share the same depth buffer (correct mutual occlusion):

| Path | Mesh type | Use case |
|------|-----------|----------|
| Triangle mesh | `Mesh` | Solid surfaces, gadgets |
| Line mesh | `LineMesh` | Wireframes, grid, axes |
| Atom impostors | `AtomImpostorMesh` | Billboard spheres |
| Bond impostors | `BondImpostorMesh` | Billboard cylinders |

Key facts: `LineMesh` supports `add_line_with_positions()` and `add_dotted_line()`.
Impostors and triangle meshes are mutually exclusive per atom (auto-selected by
`AtomicRenderingMethod`). All mesh types share the depth buffer. The gadget render
pass is separate (always-on-top) and unsuitable for guide dots.

### 8.3 Chosen approach: Decoration phase rendering

Guide dots render during the atom_edit node's **decoration phase** (`eval()` with
`decorate: bool`), the same mechanism used for selection highlights, anchor arrows,
and delete markers. This fits because guide dots only appear when the atom_edit node
is selected, and the phase has access to the full evaluated structure.

- **Guide dots:** phantom atoms with `AtomDisplayState::GuideDot`/`GuideDotSecondary`,
  rendered by existing atom renderer with overridden color/size
- **Anchor-to-dot cylinders:** reuse anchor arrow tessellation by setting
  `set_anchor_position(guide_dot_atom_id, anchor_atom_position)`
- **Wireframe sphere/ring:** `LineMesh` geometry via line rendering pipeline,
  depth-tested with the rest of the scene

### 8.4 Hit testing guide dots

Store positions in tool state. Test ray against guide dot spheres (hit radius ~0.3 A)
**before** testing real atoms. Guide dots have priority.

### 8.5 Anchor highlight

Mark anchor atom with `AtomDisplayState::Marked` (yellow), same as Add Bond tool.

## 9. API Design

### 9.1 Core Rust function

```rust
pub fn compute_guided_placement(
    structure: &AtomicStructure,
    anchor_atom_id: u32,
    new_element_atomic_number: i16,
) -> Option<GuidedPlacementInfo>

pub struct GuidedPlacementInfo {
    pub anchor_atom_id: u32,
    pub hybridization: Hybridization,
    pub guide_dots: Vec<GuideDot>,
    pub bond_distance: f64,
    pub is_saturated: bool,
}

pub enum Hybridization { Sp3, Sp2, Sp1 }
```

### 9.2 API entry points (exposed to Flutter)

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_start_guided_placement(
    ray_start: APIVec3, ray_dir: APIVec3, atomic_number: i16,
) -> GuidedPlacementApiResult

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_place_guided_atom(
    ray_start: APIVec3, ray_dir: APIVec3,
) -> bool

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_cancel_guided_placement()
```

### 9.3 Flutter-side changes

In `structure_designer_viewport.dart`, upgrade the Add Atom pointer handler from
direct `atom_edit_add_atom_by_ray` to a state-aware dispatcher that attempts guided
placement first, falling back to free placement if no atom is hit.

## 10. File Locations

| Component                     | Location                                                          |
|-------------------------------|-------------------------------------------------------------------|
| Guided placement logic        | `rust/src/crystolecule/guided_placement.rs` (new)                 |
| Tool state types              | `rust/src/structure_designer/nodes/atom_edit/types.rs`            |
| Add Atom tool functions       | `rust/src/structure_designer/nodes/atom_edit/add_atom_tool.rs`    |
| Decoration phase (eval)       | `rust/src/structure_designer/nodes/atom_edit/atom_edit_data.rs`   |
| Data accessors / tool mgmt    | `rust/src/structure_designer/nodes/atom_edit/atom_edit_data.rs`   |
| API entry points              | `rust/src/api/structure_designer/atom_edit_api.rs`                |
| Viewport interaction (Flutter)| `lib/structure_designer/structure_designer_viewport.dart`          |
| Guide dot rendering           | `rust/src/display/atomic_tessellator.rs`                          |
| Wireframe rendering           | `rust/src/display/guided_placement_tessellator.rs` (new)          |
| Hybridization detection       | `rust/src/crystolecule/simulation/uff/typer.rs` (reuse)           |
| Bond distances                | `rust/src/crystolecule/atomic_constants.rs` (reuse)               |
| Tests                         | `rust/tests/crystolecule/guided_placement_test.rs` (new)          |

## 11. Open Questions

1. **Auto-chain placement:** Should the newly placed atom auto-become the next anchor?
2. **Bond order selection:** Should double/triple bonds be selectable during placement?
3. **Energy preview:** Show UFF energy before confirming? (Probably overkill for v1.)
4. **Lattice snapping:** Snap guide dots to crystal lattice positions?
5. **Multi-atom placement:** Extend to molecular fragments (-CH3, -OH)?
6. **Undo integration:** Placement should be a single undo step (atom + bond).

---

# Part II — Implementation Plan

---

Four phases, each independently shippable. Ordered by value and complexity.

| Phase | Scope | Key deliverables |
|-------|-------|------------------|
| A | sp3 cases 2, 3, 4 | Core infrastructure: geometry, state machine, rendering, API, Flutter integration, saturation feedback |
| B | sp3 case 0 (free sphere) | Wireframe sphere rendering, ray-sphere placement, pointer-move tracking |
| C | sp3 case 1 (dihedral + ring) | Topology walking, trans/cis dots, wireframe ring, ring rotation interaction |
| D | sp2 and sp1 | 120 deg and 180 deg geometry, hybridization-aware dispatch |

---

## Phase A — sp3 Cases 2, 3, 4 (Core Infrastructure)

**Goal:** Guided placement for sp3 atoms with 2-3 existing bonds. Saturation feedback
for 4 bonds. Builds ALL scaffolding that later phases extend.

### A.1 Geometry Computation — `guided_placement.rs`

**New file:** `rust/src/crystolecule/guided_placement.rs`

Pure-geometry library, no dependencies on node system/rendering/API. Independently testable.

#### A.1.1 Hybridization detection

```rust
pub enum Hybridization { Sp3, Sp2, Sp1 }

pub fn detect_hybridization(structure: &AtomicStructure, atom_id: u32) -> Hybridization
```

1. Get atom's `atomic_number` and `bonds` (as `InlineBond` slice)
2. Call `assign_uff_type(atomic_number, bonds)` from `simulation/uff/typer.rs`
3. `hybridization_from_label(label)` returns `'3'`/`'2'`/`'1'`/`'R'`
4. Map: `'3'` -> Sp3, `'2'`/`'R'` -> Sp2, `'1'` -> Sp1
5. Fallback for bare atoms: element defaults from Section 6.2

#### A.1.2 Saturation check

```rust
pub fn effective_max_neighbors(atomic_number: i16, hybridization: Hybridization) -> usize
pub fn remaining_slots(structure: &AtomicStructure, atom_id: u32, hybridization: Hybridization) -> usize
```

Counts active bonds (bond_order > 0), subtracts from `effective_max_neighbors`.

#### A.1.3 Bond distance

```rust
pub fn bond_distance(anchor_atomic_number: i16, new_atomic_number: i16) -> f64
```

Returns `covalent_radius(A) + covalent_radius(B)` from `ATOM_INFO`. Special case:
C-H returns 1.09 A (`C_H_BOND_LENGTH` — move/re-export to `atomic_constants.rs`).

#### A.1.4 sp3 candidate position computation

```rust
pub fn compute_sp3_candidates(
    anchor_pos: DVec3,
    existing_bond_dirs: &[DVec3],  // normalized
    bond_dist: f64,
) -> Vec<GuideDot>
```

**Case 4:** Empty vec. **Case 3:** `d4 = normalize(-(b1+b2+b3))`, 1 Primary dot.
**Case 2:** Fit ideal tetrahedron to b1,b2 via Rodrigues' rotation, extract other
two vertices. 2 Primary dots. **Case 1/0:** Empty vec (stubs for Phase B/C).

#### A.1.5 Top-level entry point

```rust
pub struct GuidedPlacementResult {
    pub anchor_atom_id: u32,
    pub hybridization: Hybridization,
    pub guide_dots: Vec<GuideDot>,
    pub bond_distance: f64,
    pub remaining_slots: usize,
}

pub struct GuideDot {
    pub position: DVec3,
    pub dot_type: GuideDotType,
}

pub enum GuideDotType { Primary, Secondary }

pub fn compute_guided_placement(
    structure: &AtomicStructure,
    anchor_atom_id: u32,
    new_element_atomic_number: i16,
) -> GuidedPlacementResult
```

Orchestrates: detect hybridization -> check saturation -> compute bond distance ->
dispatch to `compute_sp3_candidates` (sp2/sp1 in future phases).

#### A.1.6 Tests

**New file:** `rust/tests/crystolecule/guided_placement_test.rs` (register in `mod.rs`).

- **sp3 case 3:** CH3 -> 4th direction opposite centroid, angle ~109.47 deg to each bond
- **sp3 case 2:** CH2 -> 2 guides, all 4 mutual angles ~109.47 deg
- **sp3 saturated:** CH4 -> 0 dots, `remaining_slots == 0`
- **Bond distance:** C-C ~1.52 A, C-H 1.09 A, C-N ~1.47 A
- **Hybridization:** C+4 single -> Sp3, C+double -> Sp2, N+3 single -> Sp3
- **Saturation limits:** N(sp3) at 3, O(sp3) at 2, F at 1

### A.2 Tool State Machine

The atom_edit module is split across multiple files under
`rust/src/structure_designer/nodes/atom_edit/`:

| File                  | Responsibility                                      |
|-----------------------|-----------------------------------------------------|
| `types.rs`            | `AddAtomToolState`, `AtomEditTool`, shared types     |
| `add_atom_tool.rs`    | Add Atom tool interaction logic (placement functions)|
| `atom_edit_data.rs`   | `AtomEditData` struct, `eval()`, decoration phase    |
| `selection.rs`        | Ray-based and marquee selection                      |
| `operations.rs`       | Shared mutation operations (delete, replace, move)   |
| `default_tool.rs`     | Default tool pointer event state machine             |
| `add_bond_tool.rs`    | Add Bond tool interaction logic                      |

#### A.2.1 State enum change — `types.rs`

Current: `pub struct AddAtomToolState { pub atomic_number: i16 }`

New:
```rust
pub enum AddAtomToolState {
    Idle { atomic_number: i16 },
    GuidedPlacement {
        atomic_number: i16,
        anchor_atom_id: u32,
        guide_dots: Vec<GuideDot>,
        bond_distance: f64,
    },
}
```

**Migration:** All match arms accessing `atomic_number` must handle both variants.
These are spread across `types.rs` (enum definition), `atom_edit_data.rs` (accessors
like `set_add_atom_tool_atomic_number`, `set_active_tool`), and `atom_edit_api.rs`
(API functions that read/write the tool state).

#### A.2.2 New functions in `add_atom_tool.rs`

**`start_guided_placement(structure_designer, ray_start, ray_dir, atomic_number) -> GuidedPlacementApiResult`:**
1. Hit test result structure -> if miss: return `NoAtomHit`
2. Resolve hit atom to diff atom ID (same pattern as `draw_bond_by_ray`)
3. If not in diff, promote it (same pattern as drag)
4. Call `compute_guided_placement()`
5. If `remaining_slots == 0` -> return `AtomSaturated`
6. Store GuidedPlacement state, mark data changed
7. Return `GuidedPlacementStarted`

**`place_guided_atom(structure_designer, guide_dot_index) -> bool`:**
1. Extract state, validate index
2. `add_atom_to_diff()` + `add_bond_in_diff(anchor, new, 1)`
3. Transition to Idle, mark data changed

**`cancel_guided_placement(structure_designer)`:**
Extract atomic_number, transition to Idle, mark data changed.

#### A.2.3 Decoration phase integration — `atom_edit_data.rs`

In `eval()` within `if decorate { ... }`, for `AddAtom(GuidedPlacement { .. })`:
1. Mark anchor atom with `AtomDisplayState::Marked`
2. For each guide dot: add phantom atom with `GuideDot`/`GuideDotSecondary` display state
3. For each guide dot: set anchor position to anchor atom's position (triggers anchor
   arrow cylinder rendering automatically)

#### A.2.4 Guide dot hit testing — `add_atom_tool.rs`

```rust
pub fn hit_test_guide_dots(
    ray_start: &DVec3, ray_dir: &DVec3,
    guide_dots: &[GuideDot], hit_radius: f64,
) -> Option<usize>
```

Uses `sphere_hit_test` from `hit_test_utils`, returns index of closest hit (or None).
Called from API layer before atom/empty-space hit testing.

### A.3 Rendering — Display States and Tessellation

#### A.3.1 New `AtomDisplayState` variants

```rust
pub enum AtomDisplayState {
    Normal, Marked, SecondaryMarked,
    GuideDot,           // primary (magenta, 0.2 A radius)
    GuideDotSecondary,  // secondary (magenta, 0.15 A radius)
}
```

#### A.3.2 Tessellator changes

In `tessellate_atom()` and the impostor path:

- **Color override:** `GuideDot`/`GuideDotSecondary` -> `Vec3::new(1.0, 0.2, 1.0)`
- **Size override:** `GuideDot` -> 0.2, `GuideDotSecondary` -> 0.15
- **No crosshair:** these variants skip the 3D crosshair rendering
- **Impostor path:** override `albedo` and `radius` in `add_atom_quad()` too

#### A.3.3 Anchor-to-dot cylinders

Set `result.set_anchor_position(guide_dot_atom_id, anchor_atom_position)` for each
phantom atom. The existing anchor arrow rendering draws orange cylinders automatically.
Enable `show_anchor_arrows` during guided placement, or conditionally render for
guide-dot atoms regardless of the toggle.

### A.4 API Layer — `atom_edit_api.rs`

#### A.4.1 New types

```rust
pub enum GuidedPlacementApiResult {
    NoAtomHit,
    AtomSaturated,
    GuidedPlacementStarted { guide_count: usize, anchor_atom_id: u32 },
}
```

#### A.4.2 New API functions

Three functions following the standard pattern (`with_mut_cad_instance` ->
operation -> `refresh_structure_designer_auto`):

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_start_guided_placement(ray_start: APIVec3, ray_dir: APIVec3, atomic_number: i16) -> GuidedPlacementApiResult

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_place_guided_atom(ray_start: APIVec3, ray_dir: APIVec3) -> bool
// Performs guide dot hit test internally, then calls place_guided_atom

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_cancel_guided_placement()

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_is_in_guided_placement() -> bool
```

#### A.4.3 Existing API updates

`set_add_atom_tool_atomic_number` (in `atom_edit_data.rs`) and
`get_active_atom_edit_tool` (in `atom_edit_api.rs`) must match both
`Idle { atomic_number }` and `GuidedPlacement { atomic_number, .. }` variants.

### A.5 Flutter Integration

Rust does the heavy lifting; Flutter is a thin dispatcher.

#### A.5.1 New click dispatch

Replace the Add Atom branch in `onAtomEditClick()`:

```dart
if (activeAtomEditTool == APIAtomEditTool.addAtom) {
  final atomicNumber = atomEditData.addAtomToolAtomicNumber!;

  if (atom_edit_api.isInGuidedPlacement()) {
    // Try to place at a guide dot
    final placed = widget.graphModel.atomEditPlaceGuidedAtom(ray.start, ray.direction);
    if (!placed) {
      // Try switching anchor
      final result = widget.graphModel.atomEditStartGuidedPlacement(
        ray.start, ray.direction, atomicNumber);
      if (result == GuidedPlacementApiResult.noAtomHit) {
        widget.graphModel.atomEditCancelGuidedPlacement();
      }
    }
  } else {
    // Try to start guided placement
    final result = widget.graphModel.atomEditStartGuidedPlacement(
      ray.start, ray.direction, atomicNumber);
    if (result == GuidedPlacementApiResult.noAtomHit) {
      // Fall back to free placement
      widget.graphModel.atomEditAddAtomByRay(atomicNumber, planeNormal, ray.start, ray.direction);
    }
  }
}
```

#### A.5.2 New model methods

Three methods following existing pattern (API call -> `refreshFromKernel()`):
`atomEditStartGuidedPlacement()`, `atomEditPlaceGuidedAtom()`,
`atomEditCancelGuidedPlacement()`.

#### A.5.3 Escape key handling

Add handler in `_StructureDesignerViewportState`: if Escape pressed and
`isInGuidedPlacement()`, call `cancelGuidedPlacement()`.

#### A.5.4 Tool switch cancellation

In `AtomEditData::set_active_tool()` (in `atom_edit_data.rs`), if current tool is
`AddAtom(GuidedPlacement)` and switching away, transition to `Idle` first.

### A.6 Saturation Feedback

When `AtomSaturated` is returned:
1. **Rust:** Set `saturated_flash_atom_id: Option<u32>` on `AtomEditData` (in
   `atom_edit_data.rs`). In decoration phase of `eval()`, render with
   `AtomDisplayState::SaturationFlash` (red, `DELETE_MARKER_COLOR`).
2. **Flutter:** Start 300ms timer, then call `atom_edit_clear_saturation_flash()`.
3. Clear on next click, tool switch, or explicit clear call.

**Simpler alternative for v1:** Skip the flash, just show status bar message.

### A.7 Implementation Order

1. Geometry module (A.1) — pure computation, independently testable
2. Tests for geometry (A.1.6) — validate math before integration
3. Display state enum (A.3.1) — small, no-risk change in `atomic_structure_decorator.rs`
4. Tessellator changes (A.3.2) — both triangle mesh and impostor paths
5. Tool state enum (A.2.1) — change `AddAtomToolState` in `types.rs`, fix match arms
   across `types.rs`, `atom_edit_data.rs`, and `atom_edit_api.rs`
6. Decoration phase (A.2.3) — connect geometry to rendering in `atom_edit_data.rs` `eval()`
7. Tool functions (A.2.2) — start/place/cancel logic in `add_atom_tool.rs`
8. API layer (A.4) — expose to Flutter via `atom_edit_api.rs`
9. FRB codegen
10. Flutter model methods (A.5.2)
11. Flutter click dispatch (A.5.1)
12. Flutter escape handling (A.5.3)
13. Saturation feedback (A.6)
14. Integration testing

Steps 1-2 in isolation; 3-4 safe additions; 5-8 Rust integration; 9-13 Flutter; 14 full app.

---

## Phase B — sp3 Case 0 (Free Sphere Placement)

**Goal:** Wireframe sphere for bare atoms (no bonds). Click anywhere on sphere to place.

**Prerequisite:** Phase A.

### B.1 Wireframe Rendering

**New file:** `rust/src/display/guided_placement_tessellator.rs`

```rust
pub fn tessellate_wireframe_circle(
    line_mesh: &mut LineMesh, center: &DVec3, radius: f64,
    normal: &DVec3, segments: u32, color: &[f32; 3],
)

pub fn tessellate_wireframe_sphere(
    line_mesh: &mut LineMesh, center: &DVec3, radius: f64, color: &[f32; 3],
)
```

Circle: build orthonormal basis from normal, generate `segments` points, add line
segments with wrap-around. Sphere: 3 great circles (XY, XZ, YZ), 48 segments each.

**Integration:** Store sphere params in decorator during decoration phase. In
`scene_tessellator.rs`, merge into wireframe `LineMesh` after atomic structure
tessellation. Must use main scene `LineMesh` (not gadget) for correct depth testing.

### B.2 Case 0 Geometry and State

```rust
pub enum GuidedPlacementMode {
    FixedDots { guide_dots: Vec<GuideDot> },
    FreeSphere {
        center: DVec3, radius: f64,
        preview_position: Option<DVec3>,  // cursor-tracked
    },
}
```

### B.3 Cursor Tracking

```rust
pub fn ray_sphere_intersection(
    ray_start: &DVec3, ray_dir: &DVec3,
    sphere_center: &DVec3, sphere_radius: f64,
) -> Option<DVec3>  // front hemisphere only

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_guided_placement_pointer_move(
    ray_start: APIVec3, ray_dir: APIVec3,
) -> bool  // true if preview changed
```

Flutter: add `onPointerHover` handler (cursor tracking works without button press).
When `preview_position` is set, render a phantom guide dot + anchor arrow.

### B.4 Placement

Reuse `atom_edit_place_guided_atom` API. Rust checks for `FreeSphere` mode and uses
`preview_position` instead of guide-dot hit testing.

### B.5 Tests

- Ray-sphere intersection: hit, miss, tangent, front-hemisphere only
- Wireframe circle: points on correct plane at correct radius
- Case 0 dispatch: 0 bonds -> `FreeSphere` result

---

## Phase C — sp3 Case 1 (Dihedral-Aware + Ring Fallback)

**Goal:** 1 existing bond -> walk topology for dihedral reference. 6 dots (3 trans +
3 cis) with reference, wireframe ring with 3 rotating dots without.

**Prerequisite:** Phase A + Phase B (wireframe helpers, pointer-move tracking).

### C.1 Dihedral Reference

```rust
pub fn find_dihedral_reference(
    structure: &AtomicStructure, anchor_atom_id: u32, neighbor_atom_id: u32,
) -> Option<DVec3>  // projected perpendicular to bond axis
```

Walk: A=anchor, B=neighbor. If B has other neighbor C, return
`normalize(B->C projected perp to A->B axis)`. Pick first if multiple.

### C.2 Trans/Cis Computation

```rust
pub fn compute_sp3_case1_with_dihedral(
    anchor_pos: DVec3, bond_dir: DVec3, ref_perp: DVec3, bond_dist: f64,
) -> Vec<GuideDot>  // 3 Primary (trans) + 3 Secondary (cis)
```

3 candidates on cone: axis = `-bond_dir`, half-angle = 70.53 deg (= 180-109.47).
Each position: `anchor_pos + (-bond_dir * cos(tet) + perp * sin(tet)) * bond_dist`
where `perp` is `ref_perp` rotated by appropriate angle around `bond_dir`.

Trans: 60, 180, 300 deg offsets from reference. Cis: 0, 120, 240 deg.

### C.3 Ring Fallback

Ring center: `anchor_pos + (-bond_dir) * bond_dist * cos(109.47 deg)`.
Ring radius: `bond_dist * sin(109.47 deg)`. Normal: `-bond_dir`.

Reuse `tessellate_wireframe_circle()`. Cursor tracking: intersect ray with ring plane,
project to ring circle, place 3 dots at 120 deg intervals.

```rust
pub enum GuidedPlacementMode {
    FixedDots { .. }, FreeSphere { .. },
    FreeRing {
        ring_center: DVec3, ring_normal: DVec3, ring_radius: f64,
        preview_positions: Option<[DVec3; 3]>,
    },
}
```

### C.4 Tests

- **Dihedral reference:** ethane (C-C + H's on one end) -> ref perp to C-C axis
- **Trans/cis positions:** 6 dots at correct angles; trans staggered 60 deg, cis eclipsed
- **All angles ~109.47 deg** from existing bond direction
- **Ring geometry:** center, normal, radius match cone/sphere intersection
- **No-reference:** C-C where second C has no other bonds -> returns `None`

---

## Phase D — sp2 and sp1 Geometry

**Goal:** sp2 (120 deg) and sp1 (180 deg) support.

**Prerequisite:** Phase A. Phases B/C can run in parallel with D.

### D.1 sp2 Candidates

```rust
pub fn compute_sp2_candidates(
    anchor_pos: DVec3, existing_bond_dirs: &[DVec3], bond_dist: f64,
) -> Vec<GuideDot>
```

**Case 3 (saturated):** empty. **Case 2:** `d3 = normalize(-(b1+b2))`, 1 dot.
**Case 1:** need planar reference from upstream topology. With reference: 2 dots at
+/-120 deg from b1 in the plane. Without: ring (cone half-angle 60 deg from `-b1`).
**Case 0:** sphere.

```rust
pub fn find_sp2_planar_reference(
    structure: &AtomicStructure, anchor_atom_id: u32,
    neighbor_atom_id: u32, bond_dir: DVec3,
) -> Option<DVec3>  // plane normal
```

### D.2 sp1 Candidates

```rust
pub fn compute_sp1_candidates(
    anchor_pos: DVec3, existing_bond_dirs: &[DVec3], bond_dist: f64,
) -> Vec<GuideDot>
```

**Case 2 (saturated):** empty. **Case 1:** `d2 = -b1`, 1 dot opposite.
**Case 0:** sphere.

### D.3 Hybridization-Aware Dispatch

Update `compute_guided_placement()` to match on hybridization and dispatch to
`compute_sp3/sp2/sp1_candidates`. Verify `effective_max_neighbors` covers all
element+hybridization combos (C sp2=3, C sp1=2, N sp2=3, O sp2=2, B sp2=3).

### D.4 Tests

- **sp2 case 2:** formaldehyde (C=O + 1H) -> remaining direction in molecular plane, ~120 deg
- **sp2 case 1:** C=O only -> 2 guides at +/-120 deg in correct plane
- **sp1 case 1:** C triple bond -> guide at 180 deg (opposite)
- **Hybridization dispatch:** C+double -> sp2, C+triple -> sp1, aromatic C -> sp2
- **Saturation:** C sp2 at 3, C sp1 at 2, N sp2 at 3
- **sp2 ring fallback:** ring uses 120 deg, not 109.47 deg
