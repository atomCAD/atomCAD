# Guided Atom Placement — Design Document

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

The concern is: "What if the user wants to place a free atom where clicking would
also hit an atom behind it?" In practice this is a non-issue because:

1. **Overlapping atoms are chemically invalid.** You almost never want a free atom
   directly on top of an existing one. If the click hits an existing atom, the user
   almost certainly intended to interact with it.

2. **Hit test uses visual radius.** In ball-and-stick mode the clickable radius of an
   atom is roughly 0.3–0.5 Å on screen. The gaps between atoms are large enough that
   clicking between them reliably triggers free placement.

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

### 4.3 Hover preview (stretch goal)

When the cursor hovers over a guide dot, the system could show a translucent preview
of the atom at that position — giving the user confidence before clicking. This is
a nice-to-have that can be added after the core flow works.

## 5. Visual Design

### 5.1 Guide dots

Guide dots are small spheres rendered in the 3D viewport at candidate positions.
They should be visually distinct from real atoms.

| Property             | Value                                     |
|----------------------|-------------------------------------------|
| Shape                | Sphere                                    |
| Size                 | ~0.2 Å radius (smaller than most atoms)   |
| Color (primary)      | Dark gray / charcoal (#404040)            |
| Color (secondary)    | Lighter gray (#808080)                    |
| Opacity              | Semi-transparent (70%)                    |
| Highlight on hover   | Brighten to element color of selected element |

**Why gray, not element-colored?** Guide dots represent *potential* positions, not
real atoms. Gray makes them visually subordinate to the actual structure. On hover,
they shift to the element color as a preview of what will be placed.

### 5.2 Trans vs cis differentiation (sp3 case 1 only)

When 6 guide dots are shown (3 trans + 3 cis), differentiate them:

| Type  | Radius   | Color        | Meaning                              |
|-------|----------|--------------|--------------------------------------|
| Trans | 0.22 Å   | Dark (#404040) | Preferred staggered positions (ABC) |
| Cis   | 0.15 Å   | Light (#909090) | Eclipsed positions (ABA)           |

This matches the proposal's "bigger black dots" vs "smaller black dots."

### 5.3 Bond preview lines

A thin dashed line connects the anchor atom to each guide dot, indicating the
potential bond. This helps the user understand the spatial relationship.

| Property   | Value                         |
|------------|-------------------------------|
| Style      | Thin line (1px / 0.02 Å)     |
| Color      | Same as guide dot color       |
| Opacity    | 40%                           |

### 5.4 Anchor highlight

The anchor atom (the one the user clicked) should be visually distinguished:

| Property    | Value                         |
|-------------|-------------------------------|
| Effect      | Bright ring / outline         |
| Color       | Yellow (matching selection)    |

This can reuse the existing `AtomDisplayState::Marked` rendering (yellow highlight),
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
| Color       | Gray (#606060), 30% opacity                  |
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

### 8.1 Guide dot rendering

Guide dots need to be rendered in the 3D viewport alongside regular atoms. Options:

**Option A: Render as special atoms in the AtomicStructure (recommended)**
Add guide dot positions as temporary atoms with a special display state
(`AtomDisplayState::GuideDot`) during the decoration phase of evaluation. This
reuses the existing atom rendering pipeline with minimal changes.

Pros: No new rendering code, just a new display state and color.
Cons: Guide dots are spheres only (cannot easily do wireframes).

**Option B: Render as overlay geometry**
Add a separate rendering pass for guide dots, bond preview lines, and the
sphere/ring wireframes.

Pros: Full visual control (wireframe spheres, dashed lines, etc.).
Cons: Significant rendering work.

**Recommendation:** Start with Option A for guide dot spheres (minimal effort),
add Option B later for wireframe sphere/ring in cases 0 and 1-without-reference.
For the initial implementation, cases 0 and 1-without-reference can show
individual dots evenly distributed on the sphere/ring as an approximation.

### 8.2 Hit testing guide dots

Guide dots need to be clickable. Since they're rendered as small spheres, the
existing ray-sphere hit test works. Approach:

- Store guide dot positions in the tool state
- Before testing real atoms, test ray against guide dot spheres
- Guide dots have priority over atoms in the hit test (they're always in front
  of or near the anchor, so the user's intent when clicking near them is clear)

### 8.3 Anchor atom highlighting

When in GuidedPlacement state, mark the anchor atom using
`AtomDisplayState::Marked` (yellow). This is identical to how the Add Bond tool
marks the first-clicked atom.

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
/// Called when user clicks an atom while Add Atom tool is active.
/// Enters guided placement mode.
/// Returns the guide info, or None if atom is saturated.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_start_guided_placement(
    anchor_atom_id: u32,
    atomic_number: i16,
) -> Option<APIGuidedPlacementInfo>

/// Called when user clicks a guide dot to place the atom.
/// Places atom and creates bond.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_place_guided_atom(
    guide_dot_index: usize,
) -> bool

/// Called to cancel guided placement.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_cancel_guided_placement()
```

### 9.3 Flutter-side changes

In `structure_designer_viewport.dart`, the Add Atom tool's pointer handler needs
to be upgraded from a direct `atom_edit_add_atom_by_ray` call to a state-aware
handler that:
1. On click: hit tests, calls `start_guided_placement` if atom hit
2. In guided mode: hit tests guide dots, calls `place_guided_atom` or cancels

## 10. Implementation Phases

### Phase 1 — sp3 cases 2, 3, 4 (easiest, highest value)

**What:** Guided placement for atoms with 2 or 3 existing bonds (sp3). Saturation
feedback for 4 bonds. These cases have fully determined geometry — no dihedral
reference needed.

**Deliverables:**
- `compute_guided_placement()` for sp3 with 2–4 existing bonds
- Guide dot rendering via special display state
- Hit testing for guide dots
- State machine in Add Atom tool (Idle ↔ GuidedPlacement)
- Anchor highlighting
- Saturation feedback

**Why first:** Most common scenario when editing diamond-lattice structures. All
the UX infrastructure (state machine, rendering, hit testing) gets built here
and reused by later phases.

### Phase 2 — sp3 case 0 (free sphere placement)

**What:** When clicking a bare atom (no bonds), show a sphere of valid positions.

**Deliverables:**
- Wireframe sphere rendering (or approximation via distributed dots)
- Cursor-tracking guide dot on sphere surface
- Ray-sphere intersection for placement

**Why second:** Simple geometry but requires new rendering (wireframe sphere or
dense dot pattern). Gives free placement capability for atoms not on a lattice.

### Phase 3 — sp3 case 1 (dihedral-aware, the "main feature")

**What:** When clicking an atom with exactly 1 bond, walk upstream topology to find
dihedral reference. Show 6 guide dots (3 trans + 3 cis) or ring if no reference.

**Deliverables:**
- Topology walking for dihedral reference
- Trans/cis dot computation and differentiated rendering
- Fallback to ring mode without reference
- Ring rendering and cursor-tracking

**Why third:** Most complex case. Requires topology walking and the trans/cis
distinction. The ring fallback adds another rendering mode.

### Phase 4 — sp2 and sp1

**What:** Extend all cases to sp2 (trigonal planar) and sp1 (linear) geometries.

**Deliverables:**
- Geometry computation for sp2 (120° angles in a plane)
- Geometry computation for sp1 (180° linear)
- Hybridization-aware dispatch in `compute_guided_placement()`
- Update saturation limits per hybridization

**Why last:** sp2 and sp1 are less common in atomCAD's primary use case (diamond
lattice). The algorithm structure is identical to sp3 — only the target angles
and max neighbor counts change.

## 11. File Locations

| Component                     | Location                                                 |
|-------------------------------|----------------------------------------------------------|
| Guided placement logic        | `rust/src/crystolecule/guided_placement.rs` (new)        |
| Tool state machine            | `rust/src/structure_designer/nodes/atom_edit/atom_edit.rs`|
| API entry points              | `rust/src/api/structure_designer/atom_edit_api.rs`        |
| Viewport interaction (Flutter)| `lib/structure_designer/structure_designer_viewport.dart` |
| Guide dot rendering           | `rust/src/display/atomic_tessellator.rs`                 |
| Hybridization detection       | `rust/src/crystolecule/simulation/uff/typer.rs` (reuse)  |
| Bond distances                | `rust/src/crystolecule/atomic_constants.rs` (reuse)      |
| Tests                         | `rust/tests/crystolecule/guided_placement_test.rs` (new) |

## 12. Open Questions

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
