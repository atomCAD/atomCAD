# Guided Atom Placement — Design Document

---

# Part I — Design

---

## 1. Problem Statement

Currently, atomCAD's **Add Atom** tool places atoms via ray-plane intersection — the
user clicks empty space and an atom appears on a camera-facing plane. There is no
awareness of bonding geometry, valence, or neighboring atoms. This makes it tedious
and error-prone to build structures atom-by-atom, because the user must manually
position each atom at the correct bond distance and angle, then manually add bonds.

This feature introduces **guided atom placement**: when the user clicks on an
existing atom, the system analyzes its bonding state, computes chemically valid
candidate positions for a new bonded atom, and displays them as interactive guide
points. The user clicks a guide point to place and bond the atom in one action.

## 2. Scope

**In scope:**
- sp3 (tetrahedral) guided placement — all cases (0–4 existing bonds)
- sp2 (trigonal planar) guided placement — all cases (0–3 existing bonds)
- sp1 (linear) guided placement — all cases (0–2 existing bonds)
- Dihedral-aware positioning for the single-bond case
- Saturation detection and user feedback
- Integration with the existing Add Atom tool

**Out of scope (future work):**
- Dative bonds (B, Al accepting electron pairs)
- Exotic chemistry (noble gas bonds, forced confinement)
- Metal haptic bonds
- Aromatic ring placement shortcuts
- Auto-chain building (rapid sequential placement)

## 3. Tool Integration Decision

### Recommendation: Extend the existing Add Atom tool

The guided placement workflow is a natural extension of "I want to add an atom."
Introducing a separate tool would fragment a single user intent across two tools.

**Behavior change:**
- **Click on empty space** → Current behavior (free placement on camera-facing plane)
- **Click on an existing atom** → Enter guided placement mode (new behavior)

### Why this works (addressing the overlap concern)

The concern is: "What if the user wants to place a free atom at a screen position
where the ray would also hit an atom deeper in the scene?" In practice this is a
non-issue because of how free placement depth works:

1. **Free placement uses a camera-facing plane at nearby-atom depth.** The current
   Add Atom tool places atoms on a plane whose depth is determined by the closest
   atom to the click ray (`find_closest_atom_to_ray`). This means free-placement
   clicks put the new atom *beside* existing atoms at the same depth — not behind
   them. The ray never "reaches through" to hit a distant atom when the user
   intended to place near a closer one.

2. **Hit test uses visual radius.** In ball-and-stick mode the clickable radius of an
   atom is roughly 0.3–0.5 Å on screen. The gaps between atoms are large enough that
   clicking between them reliably triggers free placement. Only a direct click on an
   atom's rendered sphere triggers guided mode.

3. **Guided mode is easily cancelled.** Pressing Escape or clicking empty space
   exits guided mode, so an accidental activation costs only one extra click.

4. **Escape hatch: modifier key.** If edge cases arise, holding Shift could force
   free placement regardless of hit test. This is a low-priority fallback.

### Alternative considered: Separate tool

A dedicated "Bond Atom" tool would avoid any ambiguity but adds cognitive overhead
(users must choose between "Add Atom" and "Bond Atom"). We reject this for now but
may revisit if user testing reveals confusion.

## 4. User Workflow

### 4.1 Overview

```
User selects Add Atom tool, picks an element (e.g., Carbon)
  │
  ├─ Clicks empty space → atom placed at ray-plane intersection (unchanged)
  │
  └─ Clicks existing atom (the "anchor") → system enters Guided Placement mode
       │
       ├─ Anchor is saturated → flash feedback, stay in Idle
       │
       └─ Anchor has open bonding sites → show guide dots
            │
            ├─ User clicks a guide dot → atom placed, bond created, return to Idle
            ├─ User clicks empty space → cancel, return to Idle
            ├─ User clicks a different atom → switch anchor, recompute guides
            └─ User presses Escape → cancel, return to Idle
```

### 4.2 Detailed step-by-step

**Step 1 — Tool activation:**
The user selects the Add Atom tool from the toolbar and picks an element from the
element selector (same as today). Nothing changes here.

**Step 2 — Click on an existing atom (anchor selection):**
The user clicks an atom in the viewport. The system:
1. Performs a ray-cast hit test against all visible atoms
2. If the ray hits an atom, that atom becomes the **anchor**
3. Determines the anchor's hybridization (sp3, sp2, or sp1) using the existing
   UFF atom type assignment
4. Counts the anchor's existing bonded neighbors
5. Computes candidate positions (see Section 6)

**Step 3 — Guide display:**
The system renders guide dots at the candidate positions (see Section 5 for visual
design). The anchor atom is highlighted to show it's the active target.

**Step 4 — Placement:**
The user clicks one of the guide dots. The system:
1. Adds a new atom of the selected element at the guide position
2. Creates a single bond between the anchor and the new atom
3. Clears the guide display
4. Returns to Idle state

**Step 5 — Cancellation:**
At any point during guide display, the user can:
- Press **Escape** to cancel and return to Idle
- Click **empty space** to cancel and return to Idle
- Click a **different atom** to switch the anchor (recomputes guides)

## 5. Visual Design

### 5.1 Guide dots

Guide dots are small spheres rendered in the 3D viewport at candidate positions.
They should be visually distinct from real atoms.

| Property             | Value                                     |
|----------------------|-------------------------------------------|
| Shape                | Sphere                                    |
| Size (primary)       | ~0.2 Å radius (smaller than most atoms)   |
| Size (secondary)     | ~0.15 Å radius                             |
| Color                | Selection color (bright magenta, `to_selected_color`) |

**Why selection color?** The selection highlight (bright magenta, rgb(1.0, 0.2, 1.0))
is the most distinct color from any element color in the periodic table. During
guided placement the user is unlikely to be using selection, so repurposing the
magenta avoids visual confusion. Gray (the alternative) is too close to carbon's
color.

### 5.2 Trans vs cis differentiation (sp3 case 1 only)

When 6 guide dots are shown (3 trans + 3 cis), differentiate them by **size only**
(both use selection color):

| Type  | Radius   | Meaning                              |
|-------|----------|--------------------------------------|
| Trans | 0.20 Å   | Preferred staggered positions (ABC) |
| Cis   | 0.15 Å   | Eclipsed positions (ABA)            |

This matches the proposal's "bigger black dots" vs "smaller black dots."

### 5.3 Bond preview cylinders

A cylinder connects the anchor atom to each guide dot, indicating the potential
bond. This reuses the existing **anchor arrow** rendering used in diff view (when
an atom has been moved from its anchor position).

| Property   | Value                                          |
|------------|------------------------------------------------|
| Style      | Cylinder (same as diff anchor arrow)           |
| Radius     | Same as anchor arrow cylinder radius           |
| Color      | Same as anchor arrow color (`ANCHOR_ARROW_COLOR`, orange) |

This requires no new rendering code — the anchor arrow tessellation already
produces cylinders between two 3D points.

### 5.4 Anchor highlight

The anchor atom (the one the user clicked) should be visually distinguished:

| Property    | Value                         |
|-------------|-------------------------------|
| Effect      | Marker ring / outline                               |
| Color       | Yellow (`MARKER_COLOR`, rgb(1.0, 1.0, 0.0))        |

This reuses the existing `AtomDisplayState::Marked` rendering (yellow marker),
already used by the Add Bond tool for the same purpose.

### 5.5 Saturation feedback

When the user clicks a fully saturated atom:

| Property    | Value                                     |
|-------------|-------------------------------------------|
| Effect      | Brief red flash on the atom               |
| Duration    | ~300ms fade                                |
| Message     | Status bar text: "Atom is fully bonded"   |

No guide dots are shown. The tool remains in Idle state.

### 5.6 Free placement sphere (case 0)

When the anchor has no bonds, the system cannot determine bond angles. Instead:

| Property    | Value                                        |
|-------------|----------------------------------------------|
| Shape       | Wireframe sphere centered on anchor          |
| Radius      | Sum of covalent radii (anchor + new element) |
| Color       | Gray (#606060)                               |
| Interaction | Click anywhere on the user-facing hemisphere |

A single guide dot appears under the cursor as it moves over the sphere surface,
snapping to the sphere at the correct bond distance from the anchor.

### 5.7 Free rotation ring (sp3 case 1 without dihedral reference)

When the anchor has exactly 1 bond but no dihedral reference atom is available
(the neighbor of the anchor has no other bonds), the 3 remaining tetrahedral
positions can freely rotate around the single bond axis. Instead of 6 dots:

| Property    | Value                                            |
|-------------|--------------------------------------------------|
| Shape       | Wireframe circle (cone intersection)             |
| Axis        | Along the existing bond direction                |
| Half-angle  | 109.47° from the existing bond (tetrahedral)     |
| Radius      | Bond distance projected onto the cone             |
| Interaction | 3 guide dots track the cursor rotation on ring    |

The 3 tetrahedral positions maintain 120° spacing between them but rotate
together to follow the cursor around the ring. Clicking places all three?
No — clicking places one atom at the clicked position. The other two dots show
where the remaining slots will be after placement.

## 6. Geometry Computation

### 6.1 Bond distance

For an anchor atom of element A and a new atom of element B:

```
bond_distance = covalent_radius(A) + covalent_radius(B)
```

Using the existing `ATOM_INFO` lookup table in `atomic_constants.rs`. Special case:
C-H bonds use 1.09 Å (existing constant `C_H_BOND_LENGTH`).

### 6.2 Hybridization detection

Use the existing UFF atom type assignment (`assign_uff_type()` in
`simulation/uff/typer.rs`) to determine the anchor atom's hybridization:

| UFF suffix | Hybridization | Max neighbors | Geometry          |
|------------|---------------|---------------|-------------------|
| `_3`       | sp3           | 4             | Tetrahedral       |
| `_2`       | sp2           | 3             | Trigonal planar   |
| `_1`       | sp1 (sp)      | 2             | Linear            |
| `_R`       | Aromatic      | 3             | Trigonal planar   |

**Fallback:** If hybridization cannot be determined (e.g., bare atom with no bonds),
default to the element's most common hybridization:
- C, Si, Ge → sp3
- N, P → sp3 (but saturated at 3 neighbors)
- O, S → sp3 (but saturated at 2 neighbors)
- B, Al → sp2
- Halogens (F, Cl, Br, I) → saturated at 1 neighbor

### 6.3 Saturation check

An atom is saturated when its number of bonded neighbors equals or exceeds the
max for its hybridization and element type.

**Effective max neighbors** (accounting for elements that don't fill all
hybridization slots):

| Element group               | Hybridization | Effective max neighbors |
|-----------------------------|---------------|------------------------|
| C, Si, Ge, Sn              | sp3           | 4                      |
| N, P, As, Sb               | sp3           | 3                      |
| O, S, Se, Te               | sp3           | 2                      |
| F, Cl, Br, I               | any           | 1                      |
| B, Al                      | sp2           | 3                      |
| C (in double bond context) | sp2           | 3                      |
| C (in triple bond context) | sp1           | 2                      |
| Noble gases                 | —             | 0                      |

`remaining_slots = effective_max_neighbors - current_neighbor_count`

If `remaining_slots <= 0`, the atom is saturated.

### 6.4 sp3 candidate positions (tetrahedral geometry)

The four vertices of a regular tetrahedron centered at the origin, with one vertex
pointing in direction `d`, are:

```
v1 = d
v2 = rotate(d, 109.47°, about any axis perpendicular to d)
v3 = rotate(v2, 120°, about d)
v4 = rotate(v2, 240°, about d)
```

Where 109.47° is the tetrahedral angle (arccos(-1/3)).

#### Case 4 — saturated (0 remaining slots)
No guides. Show saturation feedback.

#### Case 3 — one remaining slot
Given 3 existing bond directions `b1, b2, b3` (normalized), the 4th tetrahedral
direction is:

```
d4 = normalize(-(b1 + b2 + b3))
```

This is the vector opposite to the centroid of the three existing directions.
Show 1 guide dot at `anchor_pos + d4 * bond_distance`.

#### Case 2 — two remaining slots
Given 2 existing bond directions `b1, b2`:
1. Compute the bisector: `mid = normalize(b1 + b2)`
2. Compute the normal to the b1-b2 plane: `n = normalize(b1 × b2)`
3. The two remaining directions lie in the plane perpendicular to `mid`, at the
   tetrahedral angle from each existing bond:

```
d3 = normalize(-mid * cos(θ/2) + n * sin(θ/2))
d4 = normalize(-mid * cos(θ/2) - n * sin(θ/2))
```

Where θ ≈ 109.47° needs adjustment based on the actual angle between b1 and b2.

More precisely: construct the full tetrahedron that best fits the two known
directions, then extract the two unknown vertices. This can be done via:
1. Find rotation R that maps ideal tetrahedron vertex 1 → b1 and vertex 2 → b2
2. Apply R to ideal vertices 3 and 4 to get d3, d4

Show 2 guide dots.

#### Case 1 — three remaining slots (the main feature)
Given 1 existing bond direction `b1`:
- The 3 remaining tetrahedral directions lie on a cone of half-angle 109.47°
  around `-b1`, spaced 120° apart
- The rotational orientation around the cone is undetermined without a
  **dihedral reference**

**Finding the dihedral reference:**
Walk upstream in the bond topology:
1. Let `A` = the anchor atom, `B` = its single bonded neighbor
2. Look at B's other bonds (excluding the bond back to A)
3. If B has at least one other neighbor `C`, then A-B-C defines a plane,
   and the dihedral reference direction is the B→C vector

**With dihedral reference (6 guide dots):**
Given reference direction `b_ref` (the B→C vector projected perpendicular to b1):
- **3 trans positions** (staggered, 60° offset from reference):
  ```
  trans_i = rotate_on_cone(b_ref_projected, i * 120° + 60°, about -b1)
  ```
- **3 cis positions** (eclipsed, 0° offset from reference):
  ```
  cis_i = rotate_on_cone(b_ref_projected, i * 120°, about -b1)
  ```

Trans and cis are rendered at different sizes (see Section 5.2).

**Without dihedral reference (ring mode):**
Show a ring and 3 guide dots that rotate with the cursor (see Section 5.7).

#### Case 0 — no bonds (free sphere)
Show a wireframe sphere. Guide dot tracks cursor on sphere surface
(see Section 5.6).

### 6.5 sp2 candidate positions (trigonal planar geometry)

Three directions in a plane, 120° apart.

#### Case 3 — saturated
No guides. Show saturation feedback.

#### Case 2 — one remaining slot
Given 2 existing bond directions `b1, b2` (which should be roughly coplanar):
```
d3 = normalize(-(b1 + b2))
```
Show 1 guide dot. (Same logic as sp3 case 3 but the result naturally lies in
the plane of b1 and b2.)

#### Case 1 — two remaining slots
Given 1 existing bond direction `b1`:
- The 2 remaining directions are in the plane perpendicular to... which plane?
- Need a **planar reference** to determine the sp2 plane orientation
- If a reference exists (from upstream topology): compute the plane, place 2 dots
  at ±120° from b1 within that plane
- If no reference: show a ring (any rotation of the 2 dots around b1 is valid)
  The ring is a full circle (not a cone) perpendicular to b1 at 120° angle.

#### Case 0 — no bonds
Same as sp3 case 0: show a sphere.

### 6.6 sp1 candidate positions (linear geometry)

Two directions, 180° apart.

#### Case 2 — saturated
No guides.

#### Case 1 — one remaining slot
Given 1 existing bond direction `b1`:
```
d2 = -b1
```
Show 1 guide dot directly opposite the existing bond.

#### Case 0 — no bonds
Same as sp3 case 0: show a sphere.

## 7. State Machine

### 7.1 Add Atom tool states

```
                    ┌──────────────────────────────────────┐
                    │                                      │
   ┌────────┐  click atom   ┌──────────────────┐          │
   │        │──────────────→│                  │  click    │
   │  Idle  │               │ GuidedPlacement  │──guide──→ place atom
   │        │←──────────────│                  │  dot      & bond,
   │        │  Esc / click  │  (anchor, dots)  │          return to Idle
   └────────┘  empty space  └──────────────────┘
       │                         │
       │ click                   │ click different atom
       │ empty space             │
       ↓                         ↓
   place atom            recompute guides for
   (current behavior)    new anchor atom
```

### 7.2 Rust state representation

```rust
pub enum AddAtomToolState {
    Idle {
        atomic_number: i16,
    },
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
    /// Standard candidate position
    Primary,
    /// Secondary (e.g., cis position in sp3 case 1)
    Secondary,
}
```

### 7.3 Event handling

**Pointer down in Idle state:**
1. Perform hit test against atoms
2. If hit → determine hybridization, compute guides, transition to GuidedPlacement
3. If no hit → perform ray-plane intersection, place atom (current behavior)

**Pointer down in GuidedPlacement state:**
1. Perform hit test against guide dots first (small spheres)
2. If guide dot hit → place atom at guide position, create bond, transition to Idle
3. Else perform hit test against atoms
4. If different atom hit → recompute guides for new anchor, stay in GuidedPlacement
5. If same anchor hit → ignore (or cancel)
6. If no hit → cancel, transition to Idle

**Escape key in GuidedPlacement state:**
Cancel, transition to Idle.

## 8. Rendering Integration

### 8.1 Approach considered: Gadget system

The existing **gadget system** (`NodeNetworkGadget` trait) provides built-in 3D
rendering, hit testing, and drag interaction. It was considered for guide dots
but rejected due to several mismatches:

1. **Drag-centric API.** Gadgets have `start_drag()`, `drag()`, `end_drag()` —
   designed for continuous handle dragging. Guide dot selection is a single click,
   not a drag. The interaction model is fundamentally different.

2. **Lifecycle tied to evaluation.** The gadget is recreated via `provide_gadget()`
   after every partial/full network evaluation. But guided placement is transient
   tool state that should appear/disappear based on user clicks, not evaluation
   cycles. A background re-evaluation would reset the guide state.

3. **`sync_data()` pattern mismatch.** Gadgets sync continuous parameter changes
   back into `NodeData` (e.g., translation offset after dragging). Guided placement
   produces a discrete action (add atom + bond), not a parameter update.

4. **Single gadget slot.** `StructureDesigner` has one `gadget: Option<Box<dyn
   NodeNetworkGadget>>`. Using it for guide dots would block future gadget uses
   on the atom_edit node (e.g., a transform gizmo for moving selected atoms).

### 8.2 Renderer capabilities analysis

The renderer supports four rendering paths in its primary render pass, all sharing
the same depth buffer (correct mutual occlusion):

| Path | Mesh type | Pipeline | Topology | Use case |
|------|-----------|----------|----------|----------|
| Triangle mesh | `Mesh` | `triangle_pipeline` | TriangleList | Solid surfaces, gadgets |
| Line mesh | `LineMesh` | `line_pipeline` | LineList | Wireframes, grid, axes |
| Atom impostors | `AtomImpostorMesh` | `atom_impostor_pipeline` | TriangleList | Billboard spheres |
| Bond impostors | `BondImpostorMesh` | `bond_impostor_pipeline` | TriangleList | Billboard cylinders |

Key facts:

1. **Line rendering is fully supported.** `LineMesh` (`renderer/line_mesh.rs`)
   provides `add_line_with_positions()` and even `add_dotted_line()` with
   configurable dot/gap lengths. Lines have their own shader (`line_mesh.wgsl`)
   with per-vertex color and depth testing. Wireframe spheres and circles are
   feasible using existing infrastructure.

2. **Impostors and triangle meshes are mutually exclusive for atoms.** The scene
   tessellator (`display/scene_tessellator.rs`) branches on
   `AtomicRenderingMethod` — atoms either go to impostor meshes or to the
   triangle mesh, never both. Guide dots rendered as atoms in the
   `AtomicStructure` will automatically use whichever mode is active.

3. **All mesh types share the depth buffer.** Lines, triangles, atom impostors,
   and bond impostors all depth-test against each other in the primary render
   pass. A wireframe circle (line mesh) correctly occludes and is occluded by
   impostor atoms.

4. **The gadget render pass is separate.** It clears the depth buffer and renders
   on top of everything (always-visible). This is **not** suitable for guide dots
   since they should be occluded by atoms in front of them.

5. **`TessellationOutput` contains both meshes.** The struct has `mesh: Mesh`
   (triangles) and `line_mesh: LineMesh` (lines), so a single tessellation step
   can output both solid and wireframe geometry.

### 8.3 Chosen approach: Decoration phase rendering

Guide dots and wireframe guides are rendered during the atom_edit node's
**decoration phase** — the same mechanism used for existing visual overlays:
selection highlights, anchor arrows, and delete markers.

**How the decoration phase works:** The `eval()` method of `AtomEditData` receives
a `decorate: bool` flag, which is `true` only when the atom_edit node is the
selected node in the node network editor (`is_node_selected(node_id)`). All
interactive visual state — selection, markers, guide dots — is added during this
phase.

**Why this fits:**
- Guide dots only need to appear when the atom_edit node is selected (the user
  can't use the Add Atom tool on an unselected node anyway).
- If the user deselects the node, guide dots disappear — which is correct behavior
  (deselecting should cancel guided placement).
- No new rendering pipeline needed: guide dots are small spheres rendered by the
  existing atom renderer with a new `AtomDisplayState` variant.
- The decoration phase already has access to the full evaluated structure (needed
  to compute neighbor bonds and hybridization).

**Guide dot rendering (all cases):** When `decorate` is true and the active tool
is in `GuidedPlacement` state, the decoration code adds phantom atoms at guide dot
positions to the output `AtomicStructure` with `AtomDisplayState::GuideDot`
(primary) or `AtomDisplayState::GuideDotSecondary` (for cis positions). The
tessellator renders these with the appropriate colors and sizes from Section 5.
This works automatically with both impostor and triangle mesh rendering modes.

**Anchor-to-dot cylinders (all cases):** Reuse the existing anchor arrow
tessellation code which already renders cylinders between two 3D points (used in
diff view for showing atom movement). Same color (`ANCHOR_ARROW_COLOR`, orange)
and radius.

**Wireframe sphere (case 0) and wireframe ring (case 1 without dihedral
reference):** Rendered as `LineMesh` geometry using the existing line rendering
pipeline. The wireframe sphere is tessellated as 3 great circles (XY, XZ, YZ
planes) using `add_line_with_positions()`. The wireframe ring is a single circle
of line segments. Both participate in depth testing with the rest of the scene
(atoms, bonds) via the shared depth buffer, so they are correctly occluded by
atoms in front and visible through gaps.

### 8.4 Hit testing guide dots

Guide dots need to be clickable. Since they're rendered as small spheres, the
existing ray-sphere hit test works. Approach:

- Store guide dot positions in the tool state (`GuidedPlacement` variant)
- In the tool's pointer handler, test ray against guide dot spheres **before**
  testing against real atoms
- Guide dots have priority over atoms in the hit test (they're always near the
  anchor, so the user's intent when clicking near them is clear)

### 8.5 Anchor atom highlighting

When in GuidedPlacement state, mark the anchor atom using
`AtomDisplayState::Marked` (yellow `MARKER_COLOR`). This is identical to how the
Add Bond tool marks the first-clicked atom.

## 9. API Design

### 9.1 New Rust functions

```rust
/// Determines guide positions for guided atom placement.
/// Called when the user clicks an atom in Add Atom mode.
/// Returns None if the atom is saturated.
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
    /// If true, user should see "atom is saturated" message
    pub is_saturated: bool,
}

pub enum Hybridization {
    Sp3,
    Sp2,
    Sp1,
}
```

### 9.2 New API entry points (exposed to Flutter)

```rust
/// Called when user clicks while Add Atom tool is active.
/// Handles the full click dispatch: hit tests atoms, starts guided placement
/// if an atom is hit, or falls through to free placement if not.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_start_guided_placement(
    ray_start: APIVec3,
    ray_dir: APIVec3,
    atomic_number: i16,
) -> GuidedPlacementApiResult

/// Called when user clicks while in GuidedPlacement state.
/// Hit tests guide dots first, then atoms (to switch anchor), then empty space.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_place_guided_atom(
    ray_start: APIVec3,
    ray_dir: APIVec3,
) -> bool

/// Called to cancel guided placement (Escape key or tool switch).
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_cancel_guided_placement()
```

### 9.3 Flutter-side changes

In `structure_designer_viewport.dart`, the Add Atom tool's pointer handler needs
to be upgraded from a direct `atom_edit_add_atom_by_ray` call to a state-aware
handler that:
1. On click: hit tests, calls `start_guided_placement` if atom hit
2. In guided mode: hit tests guide dots, calls `place_guided_atom` or cancels

## 10. File Locations

| Component                     | Location                                                 |
|-------------------------------|----------------------------------------------------------|
| Guided placement logic        | `rust/src/crystolecule/guided_placement.rs` (new)        |
| Tool state machine            | `rust/src/structure_designer/nodes/atom_edit/atom_edit.rs`|
| API entry points              | `rust/src/api/structure_designer/atom_edit_api.rs`        |
| Viewport interaction (Flutter)| `lib/structure_designer/structure_designer_viewport.dart` |
| Guide dot rendering           | `rust/src/display/atomic_tessellator.rs`                 |
| Wireframe rendering           | `rust/src/display/guided_placement_tessellator.rs` (new) |
| Hybridization detection       | `rust/src/crystolecule/simulation/uff/typer.rs` (reuse)  |
| Bond distances                | `rust/src/crystolecule/atomic_constants.rs` (reuse)      |
| Tests                         | `rust/tests/crystolecule/guided_placement_test.rs` (new) |

## 11. Open Questions

1. **Auto-chain placement:** After placing a guided atom, should the newly placed
   atom automatically become the next anchor? This would enable rapid chain building
   (click-click-click to build a carbon chain). Could be a togglable option.

2. **Bond order selection:** The current design always creates single bonds. Should
   the user be able to choose double/triple bonds during guided placement? This
   would affect the hybridization computation.

3. **Energy preview:** Should clicking a guide dot show the UFF energy of the
   proposed placement before confirming? This might be overkill for the initial
   implementation.

4. **Lattice snapping:** atomCAD structures are often constrained to crystal
   lattices. Should the guide dots snap to the nearest lattice position instead of
   ideal bond geometry? Or should the atom be placed at the guide position and then
   the user can run minimization?

5. **Multi-atom placement:** The proposal from the user suggests this is for single
   atom placement. Could it extend to placing molecular fragments (e.g., -CH3, -OH)
   at guide positions? This would be a significant scope expansion.

6. **Undo integration:** Placing a guided atom should be a single undo step (one
   atom + one bond). Is the current undo system granular enough for this?

---

# Part II — Implementation Plan

---

The feature is delivered in four phases. Each phase is independently shippable —
it adds user-visible functionality and leaves the codebase in a working state.
Phases are ordered by value and complexity: the most common use cases ship first,
and each phase reuses infrastructure built by earlier ones.

**Phase summary:**

| Phase | Scope | Key deliverables |
|-------|-------|------------------|
| A | sp3 cases 2, 3, 4 | Core infrastructure: geometry, state machine, rendering, API, Flutter integration, saturation feedback |
| B | sp3 case 0 (free sphere) | Wireframe sphere rendering, ray-sphere placement, pointer-move tracking |
| C | sp3 case 1 (dihedral + ring) | Topology walking, trans/cis dots, wireframe ring, ring rotation interaction |
| D | sp2 and sp1 | 120° and 180° geometry, hybridization-aware dispatch |

---

## Phase A — sp3 Cases 2, 3, 4 (Core Infrastructure)

**Goal:** Guided placement for sp3 atoms with 2 or 3 existing bonds. Saturation
feedback for 4 bonds. This phase builds ALL the scaffolding (state machine,
rendering, API, Flutter integration) that later phases extend.

### A.1 Geometry Computation — `guided_placement.rs`

**New file:** `rust/src/crystolecule/guided_placement.rs`

This module is a pure-geometry library with no dependencies on the node system,
rendering, or API layer. It takes an `AtomicStructure` reference and returns
candidate positions. This makes it independently testable.

#### A.1.1 Hybridization detection

Create a function that wraps the UFF typer to extract hybridization:

```rust
pub enum Hybridization {
    Sp3,
    Sp2,
    Sp1,
}

pub fn detect_hybridization(
    structure: &AtomicStructure,
    atom_id: u32,
) -> Hybridization
```

**How it works:**
1. Get the atom's `atomic_number` and `bonds` (as `InlineBond` slice)
2. Call `assign_uff_type(atomic_number, bonds)` from `simulation/uff/typer.rs`
3. Call `hybridization_from_label(label)` — returns `'3'`, `'2'`, `'1'`, `'R'`
4. Map: `'3'` → Sp3, `'2'` or `'R'` → Sp2, `'1'` → Sp1
5. Fallback for bare atoms (0 bonds, UFF typer may fail): use element defaults
   from Section 6.2 of the design (C→Sp3, B→Sp2, etc.)

**Note:** `assign_uff_type` takes `&[InlineBond]`. The atom's bonds are available
via `atom.bonds` on the `Atom` struct in `AtomicStructure`. Bonds with
`bond_order == 0` (deleted) are already filtered inside `assign_uff_type`.

#### A.1.2 Saturation check

```rust
pub fn effective_max_neighbors(atomic_number: i16, hybridization: Hybridization) -> usize
```

Uses the table from Section 6.3 of the design. Returns the maximum number of
bonded neighbors for the given element and hybridization.

```rust
pub fn remaining_slots(
    structure: &AtomicStructure,
    atom_id: u32,
    hybridization: Hybridization,
) -> usize
```

Counts active bonds (bond_order > 0), subtracts from `effective_max_neighbors`.

#### A.1.3 Bond distance

```rust
pub fn bond_distance(anchor_atomic_number: i16, new_atomic_number: i16) -> f64
```

Returns `covalent_radius(A) + covalent_radius(B)` using `ATOM_INFO` from
`atomic_constants.rs`. Special case: if one is C (6) and the other is H (1),
return 1.09 Å (the `C_H_BOND_LENGTH` constant from `hydrogen_passivation.rs` —
move or re-export this constant to `atomic_constants.rs` for reuse).

#### A.1.4 sp3 candidate position computation

```rust
pub fn compute_sp3_candidates(
    anchor_pos: DVec3,
    existing_bond_dirs: &[DVec3],  // normalized directions from anchor to neighbors
    bond_dist: f64,
) -> Vec<GuideDot>
```

**Case 4 (0 remaining):** Return empty vec (saturated).

**Case 3 (1 remaining):** Given 3 existing bond directions `b1, b2, b3`:
```
d4 = normalize(-(b1 + b2 + b3))
```
Return 1 `GuideDot::Primary` at `anchor_pos + d4 * bond_dist`.

**Case 2 (2 remaining):** Given 2 existing bond directions `b1, b2`:
1. `mid = normalize(b1 + b2)`
2. `n = normalize(b1 × b2)` — normal to the b1-b2 plane
3. `anti_mid = -mid`
4. The two remaining directions bisect the "empty" half of the tetrahedron.
   Reconstruct by fitting the ideal tetrahedron to the two known directions:
   - Compute the angle between b1 and b2: `cos_alpha = b1 · b2`
   - The two unknowns lie symmetrically about the anti-mid direction, in the
     plane defined by anti_mid and n
   - `d3 = normalize(anti_mid * cos(β) + n * sin(β))`
   - `d4 = normalize(anti_mid * cos(β) - n * sin(β))`
   - Where `β` satisfies the tetrahedral constraint: the angle between d3 and
     each of b1, b2 should be ~109.47°. In practice, use the Gram-Schmidt
     reconstruction: place the ideal tetrahedron, align two vertices to b1 and
     b2 using Rodrigues' rotation, and extract the other two.

Return 2 `GuideDot::Primary`.

**Case 1 (3 remaining) — stub for Phase C:**
Return empty vec. This case requires dihedral reference walking.

**Case 0 (4 remaining) — stub for Phase B:**
Return empty vec. This case requires wireframe sphere interaction.

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

pub enum GuideDotType {
    Primary,
    Secondary,
}

pub fn compute_guided_placement(
    structure: &AtomicStructure,
    anchor_atom_id: u32,
    new_element_atomic_number: i16,
) -> GuidedPlacementResult
```

Orchestrates: detect hybridization → check saturation → compute bond distance →
dispatch to `compute_sp3_candidates` (or sp2/sp1 in future phases) → return
result. If `remaining_slots == 0`, returns result with empty `guide_dots`.

#### A.1.6 Tests

**New file:** `rust/tests/crystolecule/guided_placement_test.rs`

Register in `rust/tests/crystolecule/mod.rs`.

Test cases:
- **sp3 case 3 (1 slot):** Build CH3 (3 H bonded to C), verify the 4th direction
  is opposite to the centroid of the 3 H positions. Check angle to each existing
  bond ≈ 109.47°.
- **sp3 case 2 (2 slots):** Build CH2 (2 H bonded to C), verify 2 guide positions.
  All 4 angles (2 existing + 2 guides) should be ≈ 109.47° to each other.
- **sp3 saturated:** Build CH4, verify 0 guide dots and `remaining_slots == 0`.
- **Bond distance:** C-C → ~1.52 Å (0.76+0.76), C-H → 1.09 Å (special case),
  C-N → ~1.47 Å.
- **Hybridization detection:** C with 4 single bonds → Sp3, C with double bond →
  Sp2, N with 3 single bonds → Sp3.
- **Saturation limits:** N(sp3) saturated at 3, O(sp3) at 2, F at 1.

### A.2 Tool State Machine — `atom_edit.rs`

Extend the existing `AddAtomToolState` to support the Idle ↔ GuidedPlacement
state transition described in Design Section 7.

#### A.2.1 State enum change

Current:
```rust
pub struct AddAtomToolState {
    pub atomic_number: i16,
}
```

New:
```rust
pub enum AddAtomToolState {
    Idle {
        atomic_number: i16,
    },
    GuidedPlacement {
        atomic_number: i16,
        anchor_atom_id: u32,           // diff atom ID of anchor
        guide_dots: Vec<GuideDot>,     // from compute_guided_placement
        bond_distance: f64,
    },
}
```

**Migration:** Every place that accesses `AddAtomToolState::atomic_number`
(currently `self.active_tool` match arms and `set_add_atom_tool_atomic_number`)
must be updated to match both variants and extract `atomic_number`.

#### A.2.2 New functions in `atom_edit.rs`

**`start_guided_placement`:**
```rust
pub fn start_guided_placement(
    structure_designer: &mut StructureDesigner,
    ray_start: &DVec3,
    ray_dir: &DVec3,
    atomic_number: i16,
) -> GuidedPlacementApiResult
```

1. Get the evaluated result structure (immutable borrow)
2. Hit test against atoms using existing `result_structure.hit_test()`
3. If no atom hit → return `NoAtomHit`
4. Resolve hit atom to diff atom ID (via provenance, same pattern as
   `draw_bond_by_ray` — handle both diff-view and result-view)
5. If atom not yet in diff, add it (same pattern as drag: base atoms are
   promoted to diff on interaction)
6. Get the result structure again (for neighbor geometry)
7. Call `compute_guided_placement(result_structure, result_atom_id, atomic_number)`
8. If `remaining_slots == 0` → return `AtomSaturated`
9. Store `GuidedPlacement` state in `AddAtomToolState`
10. Mark node data changed (to trigger re-evaluation → decoration phase renders
    guide dots)
11. Return `GuidedPlacementStarted { guide_count, anchor_atom_id }`

**`place_guided_atom`:**
```rust
pub fn place_guided_atom(
    structure_designer: &mut StructureDesigner,
    guide_dot_index: usize,
) -> bool
```

1. Extract `GuidedPlacement` state (anchor_atom_id, guide_dots, atomic_number)
2. Validate `guide_dot_index < guide_dots.len()`
3. Get the guide dot position
4. Call `atom_edit_data.add_atom_to_diff(atomic_number, position)` — existing fn
5. Call `atom_edit_data.add_bond_in_diff(anchor_atom_id, new_atom_id, 1)` —
   existing fn (creates single bond)
6. Transition to `Idle { atomic_number }` state
7. Mark node data changed
8. Return true

**`cancel_guided_placement`:**
```rust
pub fn cancel_guided_placement(
    structure_designer: &mut StructureDesigner,
)
```

1. Extract `atomic_number` from current `GuidedPlacement` state
2. Transition to `Idle { atomic_number }`
3. Mark node data changed (to clear guide dot rendering)

#### A.2.3 Decoration phase integration

In the `eval()` method of `AtomEditData`, within the `if decorate { ... }` block
(currently lines ~535-644), add handling for the `AddAtom(GuidedPlacement { .. })`
tool state:

1. Mark the anchor atom with `AtomDisplayState::Marked` (yellow crosshair) —
   identical to how `AddBond` marks its `last_atom_id`
2. For each guide dot: add a phantom atom to the output `AtomicStructure` with a
   new display state (`AtomDisplayState::GuideDot` or `GuideDotSecondary`)
3. For each guide dot: add an anchor arrow (orange cylinder) from anchor position
   to guide dot position — reuse the existing anchor arrow rendering path

The phantom atoms are added to the result structure with:
- Position: guide dot position
- Atomic number: the selected element (so they show at correct visual size)
- A new `AtomDisplayState` variant to override their color to selection magenta

#### A.2.4 Guide dot hit testing

In `start_guided_placement`, the hit test is against real atoms.

For `place_guided_atom`, the caller (API layer) passes an index. But the API
needs a way to determine *which* guide dot was clicked. Add a
`hit_test_guide_dots` function:

```rust
pub fn hit_test_guide_dots(
    ray_start: &DVec3,
    ray_dir: &DVec3,
    guide_dots: &[GuideDot],
    hit_radius: f64,
) -> Option<usize>
```

Uses `sphere_hit_test` from `hit_test_utils` against each guide dot position
with a fixed hit radius (e.g., 0.3 Å). Returns the index of the closest hit
guide dot, or None.

This is called from the API layer before falling through to atom/empty-space
hit testing.

### A.3 Rendering — Display States and Tessellation

#### A.3.1 New `AtomDisplayState` variants

In `atomic_structure_decorator.rs`, extend the enum:

```rust
pub enum AtomDisplayState {
    Normal,
    Marked,
    SecondaryMarked,
    GuideDot,           // NEW — primary guide dot (selection magenta, 0.2 Å radius)
    GuideDotSecondary,  // NEW — secondary guide dot (selection magenta, 0.15 Å radius)
}
```

#### A.3.2 Tessellator changes in `atomic_tessellator.rs`

In `tessellate_atom()`, add handling for the new display states:

**Color override:** When `display_state` is `GuideDot` or `GuideDotSecondary`,
override the element color with selection magenta `(1.0, 0.2, 1.0)`:
```rust
let atom_color = match display_state {
    AtomDisplayState::GuideDot | AtomDisplayState::GuideDotSecondary => {
        Vec3::new(1.0, 0.2, 1.0)  // Selection magenta (to_selected_color)
    }
    _ => /* existing element color logic */
};
```

**Size override:** Override the visual radius for guide dots so they are smaller
than real atoms regardless of element:
```rust
let visual_radius = match display_state {
    AtomDisplayState::GuideDot => 0.2,           // Primary guide dot
    AtomDisplayState::GuideDotSecondary => 0.15, // Secondary (cis) guide dot
    _ => get_displayed_atom_radius(atom, visualization),
};
```

**No marker rendering:** Guide dot atoms should NOT render the 3D crosshair
(that's for `Marked` and `SecondaryMarked` only). Ensure the match arm for
crosshair rendering does not include the new variants.

**Impostor rendering:** The impostor path (`tessellate_atomic_structure_impostors`)
must also handle `GuideDot`/`GuideDotSecondary` — override `albedo` to magenta and
`radius` to 0.2/0.15 Å in `atom_impostor_mesh.add_atom_quad()`. Without this,
guide dots would only appear in triangle-mesh mode.

#### A.3.3 Anchor-to-dot cylinders

In the decoration phase (A.2.3), when adding guide dots to the result
structure, also add anchor arrow entries. The existing anchor arrow rendering in
`atomic_tessellator.rs` (lines 181-209) draws orange cylinders between anchor
positions and atom positions. By setting the anchor position of each phantom
guide-dot atom to the anchor atom's position, the existing rendering code will
draw cylinders from anchor to guide dot automatically — no new tessellation code
needed.

Specifically, for each guide dot phantom atom added to the result structure:
```rust
result.set_anchor_position(guide_dot_atom_id, anchor_atom_position);
```

This piggybacks on the existing `show_anchor_arrows` rendering path. The anchor
arrows must be enabled during guided placement decoration (set
`show_anchor_arrows = true` on the decorator for the result structure, or
conditionally render anchor arrows for guide-dot atoms regardless of the
`show_anchor_arrows` toggle).

**Alternative:** If the anchor arrow toggle should remain independent, add the
cylinders explicitly in the decoration phase by inserting them into the
tessellation output directly. This requires passing the mesh through decoration,
which is more invasive. Prefer the anchor-position approach first.

### A.4 API Layer — `atom_edit_api.rs`

#### A.4.1 New API types

In `structure_designer_api_types.rs`:

```rust
pub enum GuidedPlacementApiResult {
    NoAtomHit,
    AtomSaturated,
    GuidedPlacementStarted {
        guide_count: usize,
        anchor_atom_id: u32,
    },
}
```

#### A.4.2 New API functions

Three new functions following the existing pattern:

**`atom_edit_start_guided_placement`:**
```rust
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_start_guided_placement(
    ray_start: APIVec3,
    ray_dir: APIVec3,
    atomic_number: i16,
) -> GuidedPlacementApiResult
```

Wraps `atom_edit::start_guided_placement`. The ray-based API (rather than passing
`anchor_atom_id` directly) keeps the hit testing on the Rust side, consistent
with `add_atom_by_ray` and `draw_bond_by_ray`.

**`atom_edit_place_guided_atom`:**
```rust
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_place_guided_atom(
    ray_start: APIVec3,
    ray_dir: APIVec3,
) -> bool
```

Performs guide dot hit test internally (A.2.4), then calls
`atom_edit::place_guided_atom` with the matched index. Returns false if no guide
dot was hit.

**`atom_edit_cancel_guided_placement`:**
```rust
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_cancel_guided_placement()
```

Wraps `atom_edit::cancel_guided_placement`.

All three follow the standard pattern: `with_mut_cad_instance` →
perform operation → `refresh_structure_designer_auto`.

#### A.4.3 Existing API updates

`set_add_atom_tool_atomic_number` currently accesses `AddAtomToolState` as a
struct. After the enum change (A.2.1), update to match both variants:
```rust
AtomEditTool::AddAtom(AddAtomToolState::Idle { ref mut atomic_number })
| AtomEditTool::AddAtom(AddAtomToolState::GuidedPlacement { ref mut atomic_number, .. })
=> { *atomic_number = num; true }
```

Similarly for `get_active_tool()` which reads the atomic number for reporting.

### A.5 Flutter Integration

This section covers all the Flutter-side changes needed to wire up the guided
placement feature. The Rust backend does the heavy lifting (hit testing, geometry,
state management); Flutter is a thin dispatcher.

#### A.5.1 Current Add Atom click flow (what changes)

Currently in `structure_designer_viewport.dart`, `onAtomEditClick()`:
```dart
if (activeAtomEditTool == APIAtomEditTool.addAtom) {
  widget.graphModel.atomEditAddAtomByRay(
    atomEditData.addAtomToolAtomicNumber!,
    planeNormal,
    ray.start,
    ray.direction,
  );
}
```

This unconditionally calls `atomEditAddAtomByRay` for every click. The new flow
must first attempt guided placement (which does its own hit testing internally),
and only fall back to free placement if no atom was hit.

#### A.5.2 New click dispatch in `onAtomEditClick()`

Replace the Add Atom branch with a two-phase dispatch:

```dart
if (activeAtomEditTool == APIAtomEditTool.addAtom) {
  final atomicNumber = atomEditData.addAtomToolAtomicNumber!;

  // Check if we're currently in GuidedPlacement state
  if (atom_edit_api.isInGuidedPlacement()) {
    // Phase 2: In guided mode — try to place at a guide dot
    final placed = widget.graphModel.atomEditPlaceGuidedAtom(
      ray.start, ray.direction,
    );
    if (!placed) {
      // Clicked empty space or a different atom — handled by Rust
      // (Rust cancels or switches anchor internally)
      // Try starting guided placement on whatever was clicked
      final result = widget.graphModel.atomEditStartGuidedPlacement(
        ray.start, ray.direction, atomicNumber,
      );
      if (result == GuidedPlacementApiResult.noAtomHit) {
        // Clicked empty space — cancel guided placement
        widget.graphModel.atomEditCancelGuidedPlacement();
      }
    }
  } else {
    // Phase 1: In idle — try to start guided placement
    final result = widget.graphModel.atomEditStartGuidedPlacement(
      ray.start, ray.direction, atomicNumber,
    );
    if (result == GuidedPlacementApiResult.noAtomHit) {
      // No atom hit — fall back to current free placement behavior
      final camera = common_api.getCamera();
      final cameraTransform = getCameraTransform(camera);
      final planeNormal = cameraTransform!.forward;
      widget.graphModel.atomEditAddAtomByRay(
        atomicNumber, planeNormal, ray.start, ray.direction,
      );
    }
    // AtomSaturated and GuidedPlacementStarted handled by Rust
    // (saturation feedback rendered by Rust, guide dots rendered by Rust)
  }
}
```

**Key insight:** The Rust API determines the tool state, performs hit testing, and
manages transitions. Flutter just dispatches the right API call based on whether
guided placement is currently active.

#### A.5.3 New API query: `isInGuidedPlacement()`

Add a lightweight API function to check if the Add Atom tool is currently in
`GuidedPlacement` state. This avoids Flutter needing to track state redundantly.

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_is_in_guided_placement() -> bool
```

Returns `true` if the active tool is `AddAtom(GuidedPlacement { .. })`.

#### A.5.4 New model methods in `structure_designer_model.dart`

Add three methods following the existing pattern (API call → refreshFromKernel):

```dart
GuidedPlacementApiResult atomEditStartGuidedPlacement(
    vector_math.Vector3 rayStart,
    vector_math.Vector3 rayDir,
    int atomicNumber) {
  if (nodeNetworkView == null) return GuidedPlacementApiResult.noAtomHit;
  final result = atom_edit_api.atomEditStartGuidedPlacement(
    rayStart: vector3ToApiVec3(rayStart),
    rayDir: vector3ToApiVec3(rayDir),
    atomicNumber: atomicNumber,
  );
  refreshFromKernel();
  return result;
}

bool atomEditPlaceGuidedAtom(
    vector_math.Vector3 rayStart,
    vector_math.Vector3 rayDir) {
  if (nodeNetworkView == null) return false;
  final result = atom_edit_api.atomEditPlaceGuidedAtom(
    rayStart: vector3ToApiVec3(rayStart),
    rayDir: vector3ToApiVec3(rayDir),
  );
  refreshFromKernel();
  return result;
}

void atomEditCancelGuidedPlacement() {
  if (nodeNetworkView == null) return;
  atom_edit_api.atomEditCancelGuidedPlacement();
  refreshFromKernel();
}
```

#### A.5.5 Escape key handling

Currently there is no Escape key handler in the viewport. Add one using Flutter's
`KeyboardListener` or `RawKeyboardListener` pattern:

In `_StructureDesignerViewportState`, override the keyboard handler to intercept
Escape:

```dart
bool handleKeyEvent(KeyEvent event) {
  if (event is KeyDownEvent && event.logicalKey == LogicalKeyboardKey.escape) {
    if (atom_edit_api.atomEditIsInGuidedPlacement()) {
      widget.graphModel.atomEditCancelGuidedPlacement();
      renderingNeeded();
      return true;  // consumed
    }
  }
  return false;  // not consumed, propagate
}
```

Attach this handler to the existing keyboard event flow (likely via `Focus` widget
wrapping the viewport, or the existing `RawKeyboardListener` if present).

#### A.5.6 Tool switch cancellation

When the user switches from Add Atom to another tool (Default, Add Bond), any
active guided placement must be cancelled. The existing
`setActiveAtomEditTool()` Rust function should be updated to cancel guided
placement as a side effect when switching away from `AddAtom`.

In `atom_edit.rs`, in the `set_active_atom_edit_tool` function, if the current
tool is `AddAtom(GuidedPlacement { .. })` and the new tool is different, transition
to `AddAtom(Idle { .. })` first.

### A.6 Saturation Feedback

When the user clicks a saturated atom, the system must provide visual feedback
that the atom cannot accept more bonds.

#### A.6.1 Detection (Rust side)

Already covered by `compute_guided_placement()` returning `remaining_slots == 0`.
The API returns `GuidedPlacementApiResult::AtomSaturated`.

#### A.6.2 Visual feedback approach

**Approach: Rust-side temporary display state + Flutter timer.**

When `AtomSaturated` is returned:
1. **Rust side:** Set a temporary `AtomDisplayState::SaturationFlash` on the
   clicked atom in the decoration phase. This renders the atom with a red tint
   (`DELETE_MARKER_COLOR`, rgb(0.9, 0.1, 0.1)).
2. **Flutter side:** On receiving `AtomSaturated`, start a 300ms timer. When the
   timer fires, call a new API `atom_edit_clear_saturation_flash()` to remove the
   flash and trigger re-render.

```rust
// New display state variant
pub enum AtomDisplayState {
    Normal,
    Marked,
    SecondaryMarked,
    GuideDot,
    GuideDotSecondary,
    SaturationFlash,  // NEW — red tint, temporary
}
```

In the tessellator, `SaturationFlash` overrides the atom color to red:
```rust
AtomDisplayState::SaturationFlash => {
    Vec3::new(0.9, 0.1, 0.1)  // DELETE_MARKER_COLOR (red)
}
```

**Flutter timer:**
```dart
if (result == GuidedPlacementApiResult.atomSaturated) {
  // Flash is already rendering via Rust decoration
  Future.delayed(const Duration(milliseconds: 300), () {
    atom_edit_api.atomEditClearSaturationFlash();
    refreshFromKernel();
    renderingNeeded();
  });
}
```

**Alternative (simpler, no timer):** Skip the animated flash. Just show a brief
tooltip or status bar message ("Atom is fully bonded") and do nothing visually.
This is acceptable for Phase A — the animated flash can be added as polish later.

#### A.6.3 Rust state for saturation flash

Add `saturated_flash_atom_id: Option<u32>` to `AtomEditData`. Set it when
`AtomSaturated` is returned by `start_guided_placement`. Clear it on next click,
tool switch, or explicit clear call.

In the decoration phase, if `saturated_flash_atom_id` is `Some(id)`, set
`AtomDisplayState::SaturationFlash` on that atom.

### A.7 Implementation Order

1. **Geometry module** (A.1) — pure computation, independently testable
2. **Tests for geometry** (A.1.6) — validate math before integration
3. **Display state enum** (A.3.1) — small, no-risk change
4. **Tessellator changes** (A.3.2) — render new display states (both triangle
   mesh and impostor paths)
5. **Tool state enum** (A.2.1) — structural change, fix all match arms
6. **Decoration phase** (A.2.3) — connect geometry to rendering
7. **Tool functions** (A.2.2) — start/place/cancel logic
8. **API layer** (A.4) — expose to Flutter
9. **FRB codegen** — `flutter_rust_bridge_codegen generate`
10. **Flutter model methods** (A.5.4) — wrap new APIs
11. **Flutter click dispatch** (A.5.2) — wire up viewport handler
12. **Flutter escape handling** (A.5.5) — keyboard cancellation
13. **Saturation feedback** (A.6) — flash + optional timer
14. **Integration testing** — build, run, verify guide dots appear and are clickable

Steps 1-2 can be done and verified in isolation. Steps 3-4 are safe additions
(new enum variants, new match arms). Steps 5-8 are the Rust integration work.
Steps 9-13 are the Flutter integration. Step 14 requires running the full app.

---

## Phase B — sp3 Case 0 (Free Sphere Placement)

**Goal:** When clicking a bare atom with no bonds, show a wireframe sphere at the
correct bond distance. The user clicks anywhere on the sphere to place an atom
there with a bond.

**Prerequisite:** Phase A infrastructure (state machine, rendering, API, Flutter
integration).

### B.1 Wireframe Sphere Rendering

**New file:** `rust/src/display/guided_placement_tessellator.rs`

This module provides wireframe geometry helpers for guided placement overlays.
It outputs to `LineMesh` and is called during the decoration/tessellation phase.

#### B.1.1 Wireframe circle function

```rust
pub fn tessellate_wireframe_circle(
    line_mesh: &mut LineMesh,
    center: &DVec3,
    radius: f64,
    normal: &DVec3,   // circle plane normal
    segments: u32,     // number of line segments (e.g., 48)
    color: &[f32; 3],
)
```

Generates `segments` line segments forming a circle:
1. Build an orthonormal basis from `normal` (two perpendicular vectors `u`, `v`)
2. For `i` in `0..segments`:
   - `angle = i * TAU / segments`
   - `p_i = center + radius * (u * cos(angle) + v * sin(angle))`
3. Add line segments `(p_i, p_{i+1})` with wrap-around

Uses `line_mesh.add_line_with_uniform_color()` for each segment.

#### B.1.2 Wireframe sphere function

```rust
pub fn tessellate_wireframe_sphere(
    line_mesh: &mut LineMesh,
    center: &DVec3,
    radius: f64,
    color: &[f32; 3],
)
```

Renders 3 great circles (XY, XZ, YZ planes) using `tessellate_wireframe_circle`
with normals `(0,0,1)`, `(0,1,0)`, `(1,0,0)` respectively. Each circle uses
48 segments for smooth appearance.

#### B.1.3 Integration with tessellation pipeline

The wireframe sphere must appear in the main scene's `LineMesh` (not the gadget
line mesh), so it depth-tests correctly against atoms and bonds.

**Option A (recommended):** During the decoration phase in `atom_edit.rs`, when
the tool is in `GuidedPlacement` state with case 0, store the sphere parameters
(center, radius, color) in the `AtomicStructureDecorator` or in a new field on
the result structure. Then in `scene_tessellator.rs`, after tessellating the
atomic structure, check for wireframe overlays and add them to the wireframe
`LineMesh`.

**Option B:** Add a `line_mesh_overlay: Option<LineMesh>` field to the
`AtomicStructure` decorator. The decoration phase fills it, and the scene
tessellator merges it into the wireframe output.

### B.2 Case 0 Geometry

In `guided_placement.rs`, implement the case 0 handler:

```rust
/// For case 0 (no bonds), returns a single GuideDot at a default position
/// (e.g., along +X from anchor) and the sphere radius.
/// The actual placement position is determined interactively via cursor tracking.
pub fn compute_case0_sphere(
    anchor_pos: DVec3,
    bond_dist: f64,
) -> GuidedPlacementCase0 {
    GuidedPlacementCase0 {
        center: anchor_pos,
        radius: bond_dist,
    }
}
```

Since case 0 has no fixed guide dots (the user chooses any point on the sphere),
the `GuidedPlacement` state needs a new variant or flag:

```rust
GuidedPlacement {
    atomic_number: i16,
    anchor_atom_id: u32,
    placement_mode: GuidedPlacementMode,
}

pub enum GuidedPlacementMode {
    FixedDots {
        guide_dots: Vec<GuideDot>,
    },
    FreeSphere {
        center: DVec3,
        radius: f64,
        /// Current cursor-tracked position on sphere (updated on mouse move)
        preview_position: Option<DVec3>,
    },
}
```

### B.3 Cursor Tracking on Sphere

When in `FreeSphere` mode, the user's mouse position must be projected onto the
sphere surface to show a preview guide dot.

#### B.3.1 Rust: Ray-sphere intersection

```rust
pub fn ray_sphere_intersection(
    ray_start: &DVec3,
    ray_dir: &DVec3,
    sphere_center: &DVec3,
    sphere_radius: f64,
) -> Option<DVec3>
```

Standard ray-sphere intersection. Returns the closest intersection point on the
**front** hemisphere (the half facing the camera). If the ray misses the sphere,
returns `None`.

#### B.3.2 New API: Pointer move handler

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_guided_placement_pointer_move(
    ray_start: APIVec3,
    ray_dir: APIVec3,
) -> bool  // returns true if preview position changed (needs re-render)
```

Called on every pointer move while in `FreeSphere` mode. Computes ray-sphere
intersection, updates `preview_position`, marks data changed if position changed.

#### B.3.3 Flutter: Pointer move dispatch

In `structure_designer_viewport.dart`, add pointer-move handling for the Add Atom
tool when in guided placement mode:

**Option A (recommended): Use the `PrimaryPointerDelegate` pattern.**
Change the Add Atom tool to use a delegate (like the Default tool does) instead
of the simple `onDefaultClick()` path. This allows handling `onPrimaryMove()`:

```dart
class _AddAtomGuidedDelegate implements PrimaryPointerDelegate {
  @override
  bool onPrimaryDown(Offset pos) {
    // Hit test guide dots or start guided placement
    return true;
  }

  @override
  bool onPrimaryMove(Offset pos) {
    if (atom_edit_api.atomEditIsInGuidedPlacement()) {
      final ray = _viewport.getRayFromPointerPos(pos);
      final changed = atom_edit_api.atomEditGuidedPlacementPointerMove(
        rayStart: vector3ToApiVec3(ray.start),
        rayDir: vector3ToApiVec3(ray.direction),
      );
      if (changed) _viewport.renderingNeeded();
      return true;
    }
    return false;
  }

  @override
  bool onPrimaryUp(Offset pos) {
    // Place atom on sphere or at guide dot
    return true;
  }
}
```

**Option B (simpler):** Add an `onPointerHover` handler to the viewport widget
that fires on mouse movement (not just during drag). This is simpler but adds
overhead for every mouse move.

The delegate approach is preferred because it only tracks moves during an active
guided placement interaction, and it aligns with how the Default tool works.

**However**, cursor tracking on the sphere should work even without pressing
the mouse button. The user hovers over the sphere and sees the preview dot move.
This means we need `onPointerHover`, not `onPrimaryMove`:

```dart
@override
void onPointerHover(PointerHoverEvent event) {
  if (atom_edit_api.atomEditIsInGuidedPlacement()) {
    final ray = getRayFromPointerPos(event.localPosition);
    final changed = atom_edit_api.atomEditGuidedPlacementPointerMove(
      rayStart: vector3ToApiVec3(ray.start),
      rayDir: vector3ToApiVec3(ray.direction),
    );
    if (changed) renderingNeeded();
  }
}
```

Add this to `CadViewportState` or `_StructureDesignerViewportState`.

#### B.3.4 Rendering the preview dot

In the decoration phase, when `FreeSphere { preview_position: Some(pos), .. }`,
add a single phantom atom at `pos` with `AtomDisplayState::GuideDot` and an
anchor arrow from the sphere center to the preview position.

### B.4 Placement on Sphere

When the user clicks while a `preview_position` is shown, the existing
`atom_edit_place_guided_atom` API can be reused: the Rust side checks if
`FreeSphere` mode is active and uses `preview_position` as the placement point
instead of doing guide-dot hit testing.

### B.5 Tests

- **Ray-sphere intersection:** Test rays that hit, miss, and are tangent.
  Test that only front-hemisphere intersections are returned.
- **Wireframe circle geometry:** Verify circle points lie on the correct plane
  at the correct radius.
- **Case 0 dispatch:** Verify `compute_guided_placement` with 0 bonds returns
  a `FreeSphere` result.

---

## Phase C — sp3 Case 1 (Dihedral-Aware + Ring Fallback)

**Goal:** When clicking an atom with exactly 1 bond, walk upstream topology to
find a dihedral reference. Show 6 guide dots (3 trans + 3 cis) if reference
found, or a wireframe ring with 3 rotating guide dots if not.

**Prerequisite:** Phase A (core infrastructure) and Phase B (wireframe rendering
helpers, pointer-move tracking).

### C.1 Topology Walking for Dihedral Reference

In `guided_placement.rs`:

```rust
/// Finds a dihedral reference for sp3 case 1.
/// Returns the reference direction (projected perpendicular to the bond axis),
/// or None if no reference is available.
pub fn find_dihedral_reference(
    structure: &AtomicStructure,
    anchor_atom_id: u32,
    neighbor_atom_id: u32,
) -> Option<DVec3>
```

**Algorithm:**
1. Let `A` = anchor atom, `B` = neighbor atom (the single bonded neighbor)
2. Iterate B's bonds, excluding the bond back to A
3. If B has at least one other neighbor `C`:
   - Compute `b_ref = normalize(C.position - B.position)` (the B→C direction)
   - Project `b_ref` perpendicular to the A→B bond axis:
     `b_ref_perp = normalize(b_ref - (b_ref · bond_axis) * bond_axis)`
   - Return `Some(b_ref_perp)`
4. If B has no other neighbors, return `None`

If B has multiple other neighbors, pick the first one (any reference breaks the
rotational symmetry equivalently).

### C.2 Trans/Cis Computation

In `guided_placement.rs`:

```rust
pub fn compute_sp3_case1_with_dihedral(
    anchor_pos: DVec3,
    bond_dir: DVec3,       // normalized, anchor→neighbor
    ref_perp: DVec3,       // dihedral reference, perpendicular to bond_dir
    bond_dist: f64,
) -> Vec<GuideDot>
```

**Algorithm:**
1. The 3 candidate directions lie on a cone:
   - Axis: `-bond_dir` (opposite the existing bond)
   - Half-angle from axis: `180° - 109.47° = 70.53°`
   - This means 109.47° from `bond_dir`, which is the tetrahedral angle
2. Compute the cone radius in the perpendicular plane:
   - `cone_radius = bond_dist * sin(109.47°)` (perpendicular component)
   - `cone_height = bond_dist * cos(109.47°)` (along `-bond_dir`)
   - Actually, this is the cone at distance `bond_dist` from anchor
3. **Trans positions** (staggered, preferred):
   - `trans_0 = rotate(ref_perp, 60°, about bond_dir)` then project onto cone
   - `trans_1 = rotate(ref_perp, 180°, about bond_dir)` then project onto cone
   - `trans_2 = rotate(ref_perp, 300°, about bond_dir)` then project onto cone
4. **Cis positions** (eclipsed):
   - `cis_0 = rotate(ref_perp, 0°, about bond_dir)` then project onto cone
   - `cis_1 = rotate(ref_perp, 120°, about bond_dir)` then project onto cone
   - `cis_2 = rotate(ref_perp, 240°, about bond_dir)` then project onto cone

Return 6 `GuideDot`s: 3 `Primary` (trans) + 3 `Secondary` (cis).

More precisely, each position is:
```
pos = anchor_pos + (-bond_dir * cos(tet_angle) + perp * sin(tet_angle)) * bond_dist
```
where `perp` is `ref_perp` rotated by the appropriate angle around `bond_dir`.

### C.3 Ring Fallback (No Dihedral Reference)

When no dihedral reference is found, the 3 candidate positions can rotate freely
around the bond axis. Show a wireframe ring and 3 guide dots that track the cursor.

#### C.3.1 Ring geometry

```rust
pub fn compute_sp3_case1_ring(
    anchor_pos: DVec3,
    bond_dir: DVec3,
    bond_dist: f64,
) -> GuidedPlacementRing {
    let tet_angle = std::f64::consts::FRAC_PI_3 * 109.47 / 60.0; // ~1.911 rad
    GuidedPlacementRing {
        center: anchor_pos - bond_dir * bond_dist * (1.0/3.0_f64).sqrt(),
        normal: -bond_dir,
        radius: bond_dist * (109.47_f64.to_radians()).sin(),
    }
}
```

Actually the ring center is at `anchor_pos + (-bond_dir) * bond_dist * cos(tet_angle)`
and the ring radius is `bond_dist * sin(tet_angle)`, where `tet_angle = 109.47°`.

#### C.3.2 Ring rendering

Reuse `tessellate_wireframe_circle()` from Phase B with the computed center,
normal (`-bond_dir`), and radius.

#### C.3.3 Cursor-tracking dots on ring

When the user moves the cursor near the ring, project the cursor ray onto the ring
plane and find the closest point on the ring circle. Then place 3 guide dots at
120° intervals starting from that point.

New API for pointer move in ring mode:
```rust
pub fn ring_mode_pointer_move(
    ray_start: &DVec3,
    ray_dir: &DVec3,
    ring: &GuidedPlacementRing,
    bond_dir: &DVec3,
) -> [DVec3; 3]
```

1. Intersect ray with ring plane → get point on plane
2. Project to ring: `ring_point = ring_center + normalize(plane_point - ring_center) * ring_radius`
3. Compute 3 positions at 0°, 120°, 240° rotation from `ring_point` around `ring.normal`

Uses the same Flutter `onPointerHover` handler from Phase B.

#### C.3.4 Tool state extension

Add `FreeRing` to `GuidedPlacementMode`:

```rust
pub enum GuidedPlacementMode {
    FixedDots { guide_dots: Vec<GuideDot> },
    FreeSphere { center: DVec3, radius: f64, preview_position: Option<DVec3> },
    FreeRing {
        ring_center: DVec3,
        ring_normal: DVec3,
        ring_radius: f64,
        preview_positions: Option<[DVec3; 3]>,  // 3 dots at 120° spacing
    },
}
```

### C.4 Tests

- **Dihedral reference finding:** Build ethane (C-C with H's on one end), verify
  reference direction is perpendicular to C-C axis.
- **Trans/cis positions:** Verify 6 dots are at correct angles. Trans dots should
  be staggered 60° from the reference neighbors. Cis dots should be eclipsed
  (aligned with reference neighbors).
- **All angles ≈ 109.47°:** Each candidate direction should be ~109.47° from the
  existing bond direction.
- **Ring geometry:** Verify ring center, normal, and radius match the cone
  intersection with the sphere at bond distance.
- **No-reference detection:** Build C-C where the second C has no other bonds,
  verify `find_dihedral_reference` returns `None`.

---

## Phase D — sp2 and sp1 Geometry

**Goal:** Extend guided placement to sp2 (trigonal planar, 120°) and sp1 (linear,
180°) hybridizations. This adds support for atoms with double/triple bonds and
aromatic systems.

**Prerequisite:** Phase A (core infrastructure). Phases B and C are independent
and can be done in parallel with Phase D.

### D.1 sp2 Candidate Position Computation

In `guided_placement.rs`:

```rust
pub fn compute_sp2_candidates(
    anchor_pos: DVec3,
    existing_bond_dirs: &[DVec3],
    bond_dist: f64,
) -> Vec<GuideDot>
```

#### D.1.1 Case 3 (saturated)
Return empty vec.

#### D.1.2 Case 2 (1 remaining)
Given 2 existing bond directions `b1, b2`:
```
d3 = normalize(-(b1 + b2))
```
Return 1 `GuideDot::Primary`. Same math as sp3 case 3 — the result naturally
lies in the plane of b1 and b2 when the input bonds are coplanar.

#### D.1.3 Case 1 (2 remaining)
Given 1 existing bond direction `b1`:
- Need a planar reference to determine the sp2 plane orientation
- **With reference:** Find the sp2 plane from upstream topology (same walk as
  dihedral reference). The plane is defined by the anchor atom, its neighbor, and
  the neighbor's other bonded atom. Place 2 dots at ±120° from `b1` within this
  plane.
- **Without reference:** Show a ring. The 2 candidate directions can rotate freely
  around `b1`. The ring lies on a cone of half-angle `180° - 120° = 60°` from
  `-b1`, which is a circle in a plane perpendicular to `b1`.

```rust
pub fn find_sp2_planar_reference(
    structure: &AtomicStructure,
    anchor_atom_id: u32,
    neighbor_atom_id: u32,
    bond_dir: DVec3,
) -> Option<DVec3>  // returns the plane normal
```

Walk upstream: if the neighbor has another bond, the three atoms define the sp2
plane. The normal to this plane is `normalize(bond_dir × ref_dir)`.

With reference, compute 2 dots:
```
// In the sp2 plane, 120° from bond_dir
perp_in_plane = normalize(ref_projected)
d2 = rotate(bond_dir, +120°, about plane_normal) * bond_dist + anchor_pos
d3 = rotate(bond_dir, -120°, about plane_normal) * bond_dist + anchor_pos
```

Actually the rotation should be: rotate `-bond_dir` by ±60° around the plane
normal. This gives two directions in the sp2 plane at 120° from `bond_dir`.

#### D.1.4 Case 0 (3 remaining)
Same as sp3 case 0: show a sphere (reuse Phase B).

### D.2 sp1 Candidate Position Computation

```rust
pub fn compute_sp1_candidates(
    anchor_pos: DVec3,
    existing_bond_dirs: &[DVec3],
    bond_dist: f64,
) -> Vec<GuideDot>
```

#### D.2.1 Case 2 (saturated)
Return empty vec.

#### D.2.2 Case 1 (1 remaining)
Given 1 existing bond direction `b1`:
```
d2 = -b1
```
Return 1 `GuideDot::Primary` at `anchor_pos + (-b1) * bond_dist`.

This is the simplest case — just the opposite direction.

#### D.2.3 Case 0 (2 remaining)
Same as sp3 case 0: show a sphere (reuse Phase B).

### D.3 Hybridization-Aware Dispatch

Update `compute_guided_placement()` to dispatch based on detected hybridization:

```rust
pub fn compute_guided_placement(
    structure: &AtomicStructure,
    anchor_atom_id: u32,
    new_element_atomic_number: i16,
) -> GuidedPlacementResult {
    let hyb = detect_hybridization(structure, anchor_atom_id);
    let bond_dirs = get_existing_bond_directions(structure, anchor_atom_id);
    let bond_dist = bond_distance(
        structure.get_atom(anchor_atom_id).unwrap().atomic_number,
        new_element_atomic_number,
    );
    let slots = remaining_slots(structure, anchor_atom_id, hyb);

    let guide_dots = if slots == 0 {
        vec![]
    } else {
        match hyb {
            Hybridization::Sp3 => compute_sp3_candidates(anchor_pos, &bond_dirs, bond_dist),
            Hybridization::Sp2 => compute_sp2_candidates(anchor_pos, &bond_dirs, bond_dist),
            Hybridization::Sp1 => compute_sp1_candidates(anchor_pos, &bond_dirs, bond_dist),
        }
    };

    GuidedPlacementResult { anchor_atom_id, hybridization: hyb, guide_dots, bond_distance: bond_dist, remaining_slots: slots }
}
```

### D.4 Saturation Limits Update

The `effective_max_neighbors` function (from Phase A) already handles all
hybridization types via the table in Section 6.3. Verify it covers:

| Element + Hybridization | Max |
|--------------------------|-----|
| C sp3 | 4 |
| C sp2 | 3 |
| C sp1 | 2 |
| N sp3 | 3 |
| N sp2 | 3 |
| O sp3 | 2 |
| O sp2 | 2 |
| B sp2 | 3 |

### D.5 Tests

- **sp2 case 2:** Build formaldehyde (C=O with 1 H), verify the remaining sp2
  direction is opposite to the H, in the molecular plane. Angle to existing bonds
  ≈ 120°.
- **sp2 case 1:** Build C=O (just the double bond), verify 2 guide positions at
  ±120° from the C=O direction in the correct plane.
- **sp1 case 1:** Build C≡C (triple bond on one side), verify guide dot is at
  180° (directly opposite).
- **Hybridization dispatch:** C with double bond → sp2. C with triple bond → sp1.
  Aromatic C → sp2.
- **Saturation:** C sp2 with 3 bonds → saturated. C sp1 with 2 bonds → saturated.
  N sp2 with 3 bonds → saturated.
- **sp2 ring fallback (case 1 without reference):** Verify ring geometry uses
  120° angle, not 109.47°.
