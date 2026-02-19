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
positioning, saturation detection, dative bond awareness (via bond mode toggle),
integration with existing Add Atom tool.

**Out of scope:** Exotic chemistry, metal haptic bonds, aromatic ring
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

**Anchor selection:** ray-cast hit test -> determine hybridization (auto-detect via
UFF, or use manual override from UI dropdown) -> count existing neighbors -> compute
candidate positions.

**Hybridization override:** The atom edit panel includes a **Hybridization** dropdown
with options: **Auto** (default), **sp3**, **sp2**, **sp1**. When set to Auto, the
system infers hybridization from the anchor atom's current bonding state via UFF type
assignment. When the user explicitly selects sp3/sp2/sp1, that override is used
instead, resolving any ambiguity (e.g., a carbon with 1 bond could be sp3, sp2, or
sp1 — the user decides). The dropdown resets to Auto when switching tools.

**Bond mode toggle:** The atom edit panel includes a **Bond Mode** toggle with options:
**Covalent** (default) and **Dative**. This controls the saturation limit used when
computing guide dots:

- **Covalent:** uses conservative element-specific max neighbors (e.g., N sp3 = 3,
  O sp3 = 2). This is the safe default for standard covalent bonding.
- **Dative:** uses the geometric max — the full number of hybridization directions
  (sp3 = 4, sp2 = 3, sp1 = 2). This unlocks lone pair and empty orbital positions
  for coordinate bonding (e.g., NH3 nitrogen shows 1 additional guide dot at the
  lone pair position).

The two controls are independent: hybridization determines geometry (bond angles),
bond mode determines how many of those directions are available for bonding. The
toggle resets to Covalent when switching tools.

**Important:** dative bonding is a placement-time consideration only. The bond created
is stored as a regular bond in `AtomicStructure` — no `BondKind` distinction is
persisted. Once formed, a dative bond is physically identical to a covalent bond;
the distinction only matters for determining which positions are available during
guide dot computation.

**Bond length mode dropdown:** The atom edit panel includes a **Bond Length** dropdown
with options: **Crystal** (default) and **UFF**. This controls how the bond distance
(anchor-to-guide-dot distance) is computed:

- **Crystal:** uses a hardcoded lookup table of experimentally determined bond lengths
  from common semiconductor crystal structures (diamond, silicon, SiC, III-V, II-VI
  compounds). When the (anchor element, new element) pair is in the table, the crystal
  value is used. When the pair is not in the table, falls back to UFF rest bond length.
  This is the correct choice when extending crystal lattice structures, as it places
  atoms exactly at lattice-compatible positions.
- **UFF:** always uses the Universal Force Field rest bond length formula
  (`calc_bond_rest_length`), which accounts for hybridization-specific covalent radii,
  bond order correction, and electronegativity correction. Better suited for molecular
  chemistry where crystal lattice context is not relevant.

The dropdown resets to Crystal when switching tools.

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

Orange cylinders (`ANCHOR_ARROW_COLOR`, `ANCHOR_ARROW_RADIUS`) from anchor atom to
each guide dot, tessellated directly via `tessellate_cylinder()` calls during the
decoration phase. Same visual style as diff-view anchor arrows but without the
red anchor sphere (which would be semantically wrong here).

### 5.4 Anchor highlight

Use existing `AtomDisplayState::Marked` (yellow `MARKER_COLOR`), same as Add Bond tool.

### 5.5 Saturation feedback

Show a SnackBar notification using the existing
`ScaffoldMessenger.of(context).showSnackBar()` pattern (used throughout the app, e.g.,
`factor_into_subnetwork_dialog.dart`, `import_cnnd_library_dialog.dart`). Duration
~2 seconds. No guide dots shown; tool stays in Idle.

The message depends on the atom's bonding capacity:

- **Fully saturated (no lone pairs/empty orbitals):** "Atom is fully bonded"
- **Covalently saturated but has lone pairs/empty orbitals:** "Atom is covalently
  saturated. Switch to Dative bond mode to access additional bonding positions."

This guides the user toward the bond mode toggle when appropriate.

### 5.6 Free placement sphere (case 0 — no existing bonds)

| Property    | Value                                        |
|-------------|----------------------------------------------|
| Shape       | Wireframe sphere centered on anchor          |
| Radius      | Bond distance (from bond length mode)        |
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

Bond distance depends on the **bond length mode** dropdown:

**Crystal mode** (default): look up `(min(Z_a, Z_b), max(Z_a, Z_b))` in a hardcoded
table of sp3 semiconductor crystal bond lengths. If found, use that value. If not
found, fall back to UFF mode.

| Crystal        | Pair   | Bond length (A) | Source             |
|----------------|--------|------------------|--------------------|
| Diamond        | C-C    | 1.545            | a=3.567, a*sqrt(3)/4 |
| Silicon        | Si-Si  | 2.352            | a=5.431            |
| 3C-SiC         | Si-C   | 1.889            | a=4.358            |
| Germanium      | Ge-Ge  | 2.450            | a=5.658            |
| alpha-Sn       | Sn-Sn  | 2.810            | a=6.489            |
| c-BN           | B-N    | 1.567            | a=3.615            |
| BP             | B-P    | 1.966            | a=4.538            |
| AlN (ZB)       | Al-N   | 1.897            | a=4.380            |
| AlP            | Al-P   | 2.367            | a=5.463            |
| AlAs           | Al-As  | 2.443            | a=5.639            |
| GaN (ZB)       | Ga-N   | 1.946            | a=4.492            |
| GaP            | Ga-P   | 2.360            | a=5.450            |
| GaAs           | Ga-As  | 2.448            | a=5.653            |
| InP            | In-P   | 2.541            | a=5.869            |
| InAs           | In-As  | 2.623            | a=6.058            |
| InSb           | In-Sb  | 2.806            | a=6.479            |
| ZnS (ZB)       | Zn-S   | 2.342            | a=5.409            |
| ZnSe           | Zn-Se  | 2.454            | a=5.667            |
| ZnTe           | Zn-Te  | 2.637            | a=6.089            |
| CdTe           | Cd-Te  | 2.806            | a=6.482            |

All values derived from zinc blende / diamond cubic unit cell parameter `a` via
`bond_length = a * sqrt(3) / 4`. The table is keyed by ordered atomic number pair
`(min(Z_a, Z_b), max(Z_a, Z_b))` — element order does not matter.

**UFF mode**: use `calc_bond_rest_length(bond_order, uff_params_a, uff_params_b)` from
`simulation/uff/params.rs`. This computes:

```
r0 = ri + rj - lambda*(ri+rj)*ln(bond_order) - electronegativity_correction
```

Where `ri`, `rj` are hybridization-specific UFF covalent radii (e.g., C\_3 = 0.757,
C\_2 = 0.732) and the electronegativity correction accounts for polar bonds. In
Phase A, `bond_order` is always 1.0 (single bond). UFF params for the anchor atom
come from its detected UFF type (already computed for hybridization detection); UFF
params for the new atom use the default type for that element.

**Why two modes:** atomCAD's primary use case is extending crystal lattice structures,
where bond lengths must match the lattice geometry exactly. Sum-of-covalent-radii
gives Si-Si = 2.22 A (5.6% error vs crystal 2.352 A); UFF gives ~2.30 A (2.2% error).
Only the crystal table value places atoms at real lattice sites. For molecular
chemistry without lattice context, UFF provides physically-motivated values that
handle hybridization and electronegativity naturally.

### 6.2 Hybridization detection

**Two modes:** automatic (default) and manual override.

**Manual override:** The user selects sp3, sp2, or sp1 from the Hybridization dropdown
in the atom edit panel. When set, this value is used directly — no inference needed.
This resolves the fundamental ambiguity: when an atom has few bonds, its intended
hybridization cannot be reliably inferred from the current bonding state alone (e.g., a
carbon with 1 single bond could be building toward sp3, sp2, or sp1).

**Auto mode** (dropdown set to "Auto"): use `assign_uff_type()` from
`simulation/uff/typer.rs`:

| UFF suffix | Hybridization | Max neighbors | Geometry        |
|------------|---------------|---------------|-----------------|
| `_3`       | sp3           | 4             | Tetrahedral     |
| `_2`       | sp2           | 3             | Trigonal planar |
| `_1`       | sp1           | 2             | Linear          |
| `_R`       | Aromatic      | 3             | Trigonal planar |

**Fallback for bare atoms:** C,Si,Ge -> sp3; N,P -> sp3 (max 3); O,S -> sp3 (max 2);
B,Al -> sp2; Halogens -> max 1.

Auto mode works well when there are enough existing bonds to disambiguate (sp3 with
2+ bonds, any saturated atom). For atoms with 0-1 bonds, the user should use the
manual override if the default (sp3) is not the intended hybridization.

### 6.3 Saturation check

The saturation limit depends on the **bond mode** toggle:

| Element group           | Hybridization | Covalent max | Geometric max |
|-------------------------|---------------|--------------|---------------|
| C, Si, Ge, Sn          | sp3           | 4            | 4             |
| N, P, As, Sb           | sp3           | 3            | 4             |
| O, S, Se, Te           | sp3           | 2            | 4             |
| F, Cl, Br, I           | any           | 1            | 1             |
| B, Al                  | sp2           | 3            | 3             |
| C (double bond context)| sp2           | 3            | 3             |
| C (triple bond context)| sp1           | 2            | 2             |
| Noble gases            | --            | 0            | 0             |
| N, P                   | sp2           | 3            | 3             |
| O, S                   | sp2           | 2            | 3             |

**Covalent max** (bond mode = Covalent): element-specific limit reflecting standard
covalent bonding. Used by default.

**Geometric max** (bond mode = Dative): the full number of hybridization directions
(sp3 = 4, sp2 = 3, sp1 = 2). Unlocks lone pair and empty orbital positions for
coordinate bonding. For elements where covalent max already equals geometric max
(e.g., carbon sp3), the toggle has no effect.

`remaining_slots = effective_max_neighbors(bond_mode) - current_neighbor_count`

### 6.4 sp3 candidate positions (tetrahedral, 109.47 deg)

**Case 4 (saturated):** No guides.

**Case 3 (1 remaining):** `d4 = normalize(-(b1 + b2 + b3))` — opposite centroid
of existing directions.

**Case 2 (2 remaining):** Given `b1, b2`:
1. `mid = normalize(b1 + b2)`, `n = normalize(b1 x b2)`
2. Two new directions lie symmetrically about `-mid`:
   `d = -mid * cos(a) +/- n * sin(a)` where `a` is chosen so that
   `dot(b1, d) = cos(109.47 deg)`

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

### 8.3 Chosen approach: Direct tessellation in decoration phase

Guide dots render during the atom_edit node's **decoration phase** (`eval()` with
`decorate: bool`), the same mechanism used for selection highlights, anchor arrows,
and delete markers. This fits because guide dots only appear when the atom_edit node
is selected, and the phase has access to the full evaluated structure.

Guide dot geometry is tessellated directly into the scene's triangle `Mesh` — no
phantom atoms are added to the `AtomicStructure`, and no changes to
`AtomDisplayState` are needed. This keeps tool-specific visualization out of the
`crystolecule` module.

- **Guide dot spheres:** `tessellate_sphere()` calls with magenta material and
  appropriate radius (0.2 A primary, 0.15 A secondary)
- **Anchor-to-dot cylinders:** `tessellate_cylinder()` calls with `ANCHOR_ARROW_COLOR`
  material from anchor atom position to each guide dot position
- **Anchor highlight:** mark anchor atom with `AtomDisplayState::Marked` (yellow)
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
    hybridization_override: Option<Hybridization>,  // None = auto-detect
    bond_mode: BondMode,                            // Covalent or Dative
    bond_length_mode: BondLengthMode,               // Crystal or Uff
) -> Option<GuidedPlacementInfo>

pub struct GuidedPlacementInfo {
    pub anchor_atom_id: u32,
    pub hybridization: Hybridization,
    pub guide_dots: Vec<GuideDot>,
    pub bond_distance: f64,
    pub is_saturated: bool,
}

pub enum Hybridization { Sp3, Sp2, Sp1 }
pub enum BondMode { Covalent, Dative }
pub enum BondLengthMode { Crystal, Uff }
```

When `hybridization_override` is `Some(h)`, `h` is used directly. When `None`,
hybridization is auto-detected via UFF type assignment (see Section 6.2).

`bond_mode` controls which saturation limit is used: `Covalent` uses element-specific
max, `Dative` uses geometric max (see Section 6.3).

`bond_length_mode` controls bond distance computation: `Crystal` uses the hardcoded
semiconductor crystal bond length table with UFF fallback; `Uff` always uses the UFF
rest bond length formula (see Section 6.1).

### 9.2 API entry points (exposed to Flutter)

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_start_guided_placement(
    ray_start: APIVec3, ray_dir: APIVec3, atomic_number: i16,
    hybridization_override: Option<APIHybridization>,  // added in Phase D; None = auto
    bond_mode: APIBondMode,                            // added in Phase D; Covalent default
    bond_length_mode: APIBondLengthMode,               // Crystal default
) -> GuidedPlacementApiResult

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_place_guided_atom(
    ray_start: APIVec3, ray_dir: APIVec3,
) -> bool

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_cancel_guided_placement()
```

`APIHybridization` is an FRB-friendly enum: `{ auto_, sp3, sp2, sp1 }`.
`APIBondMode` is an FRB-friendly enum: `{ covalent, dative }`.
`APIBondLengthMode` is an FRB-friendly enum: `{ crystal, uff }`.
The Flutter dropdowns map directly to these. Hybridization and bond mode are
introduced in Phase D; bond length mode is introduced in Phase A.

### 9.3 Flutter-side changes

In `structure_designer_viewport.dart`, upgrade the Add Atom pointer handler from
direct `atom_edit_add_atom_by_ray` to a state-aware dispatcher that attempts guided
placement first, falling back to free placement if no atom is hit.

**Hybridization dropdown (Phase D):** Add a dropdown selector to the atom edit panel
(alongside the element selector) with options: Auto (default), sp3, sp2, sp1. The
selected value is passed to `atom_edit_start_guided_placement()` as
`hybridization_override`. The dropdown resets to Auto when switching away from the
Add Atom tool.

**Bond mode toggle (Phase D):** Add a Covalent/Dative toggle to the atom edit panel
(alongside the hybridization dropdown). The selected value is passed to
`atom_edit_start_guided_placement()` as `bond_mode`. The toggle resets to Covalent
when switching away from the Add Atom tool.

**Bond length mode dropdown (Phase A):** Add a Crystal/UFF dropdown to the atom edit
panel. The selected value is passed to `atom_edit_start_guided_placement()` as
`bond_length_mode`. Default: Crystal. The dropdown resets to Crystal when switching
away from the Add Atom tool. This control is introduced in Phase A because crystal
bond lengths are immediately valuable for the primary use case (sp3 semiconductor
lattice building).

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
| Guide dot tessellation        | `rust/src/display/atomic_tessellator.rs` (new function, no atom pipeline changes) |
| Wireframe rendering           | `rust/src/display/guided_placement_tessellator.rs` (new)          |
| Hybridization detection       | `rust/src/crystolecule/simulation/uff/typer.rs` (reuse)           |
| Crystal bond length table     | `rust/src/crystolecule/guided_placement.rs` (new, inline table)   |
| UFF bond length               | `rust/src/crystolecule/simulation/uff/params.rs` (reuse `calc_bond_rest_length`) |
| Tests                         | `rust/tests/crystolecule/guided_placement_test.rs` (new)          |

## 11. Open Questions

1. **Auto-chain placement:** Should the newly placed atom auto-become the next anchor?
2. **Bond order selection:** Should double/triple bonds be selectable during placement?
3. **Energy preview:** Show UFF energy before confirming? (Probably overkill for v1.)
4. **Lattice snapping:** Snap guide dots to crystal lattice positions? (The crystal
   bond length table ensures correct *distances* for semiconductor lattices, but true
   lattice snapping would also require matching crystal orientation and lattice sites.)
5. **Multi-atom placement:** Extend to molecular fragments (-CH3, -OH)?
6. **Undo integration:** Placement should be a single undo step (atom + bond).

## 12. Resolved Decisions

1. **Hybridization ambiguity:** Rather than inferring hybridization solely from the
   current bonding state (which is ambiguous for atoms with 0-1 bonds) or using
   placeholder "radical" atoms, the system provides a **Hybridization dropdown** in
   the atom edit panel (Auto / sp3 / sp2 / sp1). Auto-detection via UFF handles the
   common case; the manual override resolves ambiguity when needed. This avoids
   complicating the data model with phantom atoms while giving expert users explicit
   control.

2. **Dative bonds:** The guided placement system supports dative (coordinate) bonding
   via a **Bond Mode toggle** (Covalent / Dative) that is independent of the
   Hybridization dropdown. The two controls address orthogonal concerns:
   - **Hybridization** determines geometry (number of orbital directions and their
     angles): sp3 = 4 directions at 109.47 deg, sp2 = 3 at 120 deg, sp1 = 2 at 180 deg.
   - **Bond mode** determines how many of those directions are available for bonding:
     Covalent uses element-specific limits (e.g., N sp3 = 3, reserving 1 lone pair),
     Dative uses the geometric maximum (all hybridization directions).

   **No bond model changes:** dative bonding is a placement-time consideration only.
   The bond created is stored as a regular bond in `AtomicStructure`. Once formed, a
   dative bond is physically identical to a covalent bond; the distinction only matters
   for which guide dot positions are offered. This avoids complicating the bond model,
   serialization, and simulation systems. If dative bond rendering (arrows) or export
   distinction is needed in the future, that would be a separate feature adding
   `BondKind` to the bond model — not a guided placement concern.

   **Saturation feedback** is context-aware: when an atom is covalently saturated but
   has lone pairs or empty orbitals, the SnackBar message tells the user to switch to
   Dative bond mode, rather than just saying "fully bonded."

3. **Bond length strategy:** Rather than using a simple sum of covalent radii (which
   gives Si-Si = 2.22 A, 5.6% off the crystal value of 2.352 A), the system uses a
   **two-tier approach** controlled by a **Bond Length dropdown** (Crystal / UFF):

   - **Crystal** (default): a hardcoded lookup table of ~20 sp3 semiconductor bond
     lengths derived from experimentally determined zinc blende / diamond cubic unit
     cell parameters via `bond_length = a * sqrt(3) / 4`. Covers group IV (C, Si, Ge,
     Sn, SiC), III-V (BN, AlP, GaAs, InSb, etc.), and II-VI (ZnS, CdTe, etc.)
     compounds. Falls back to UFF when the element pair is not in the table.
   - **UFF**: uses the existing `calc_bond_rest_length()` from the UFF force field
     module, which accounts for hybridization-specific covalent radii, bond order
     correction, and electronegativity correction.

   This was chosen because: (1) atomCAD's primary use case is extending crystal
   lattice structures, where only lattice-derived bond lengths place atoms at real
   lattice sites; (2) the UFF dependency already exists (for hybridization detection),
   so using `calc_bond_rest_length()` adds no new dependencies; (3) UFF eliminates
   the need for special cases (e.g., hardcoded C-H = 1.09 A) in the molecular
   chemistry case; (4) a simple dropdown gives the user explicit control.

---

# Part II — Implementation Plan

---

Four phases, each independently shippable. Ordered by value and complexity.

| Phase | Scope | Key deliverables |
|-------|-------|------------------|
| A | sp3 cases 2, 3, 4 | Core infrastructure: geometry, state machine, rendering, API, Flutter integration, saturation feedback, bond length mode dropdown |
| B | sp3 case 0 (free sphere) | Wireframe sphere rendering, ray-sphere placement, pointer-move tracking |
| C | sp3 case 1 (dihedral + ring) | Topology walking, trans/cis dots, wireframe ring, ring rotation interaction |
| D | sp2/sp1 + expert controls | 120 deg and 180 deg geometry, hybridization dropdown, bond mode toggle (dative support) |

---

## Phase A — sp3 Cases 2, 3, 4 (Core Infrastructure)

**Goal:** Guided placement for sp3 atoms with 2-3 existing bonds. Saturation feedback
for 4 bonds. Builds ALL scaffolding that later phases extend.

### A.1 Geometry Computation — `guided_placement.rs`

**New file:** `rust/src/crystolecule/guided_placement.rs`

Pure-geometry library with UFF dependency (for hybridization detection and bond length
computation), no dependencies on node system/rendering/API. Independently testable.

#### A.1.1 Hybridization detection

```rust
pub enum Hybridization { Sp3, Sp2, Sp1 }

pub fn detect_hybridization(
    structure: &AtomicStructure,
    atom_id: u32,
    hybridization_override: Option<Hybridization>,
) -> Hybridization
```

If `hybridization_override` is `Some(h)`, return `h` directly (user explicitly chose).

Otherwise (auto-detect):
1. Get atom's `atomic_number` and `bonds` (as `InlineBond` slice)
2. Call `assign_uff_type(atomic_number, bonds)` from `simulation/uff/typer.rs`
3. `hybridization_from_label(label)` returns `'3'`/`'2'`/`'1'`/`'R'`
4. Map: `'3'` -> Sp3, `'2'`/`'R'` -> Sp2, `'1'` -> Sp1
5. Fallback for bare atoms: element defaults from Section 6.2

#### A.1.2 Saturation check

```rust
pub fn effective_max_neighbors(
    atomic_number: i16, hybridization: Hybridization, bond_mode: BondMode,
) -> usize
pub fn remaining_slots(
    structure: &AtomicStructure, atom_id: u32,
    hybridization: Hybridization, bond_mode: BondMode,
) -> usize
```

Counts active bonds (bond_order > 0), subtracts from `effective_max_neighbors`.
In Phase A, `bond_mode` is always `Covalent` (the toggle is added in Phase D).
The `BondMode` parameter is included from the start so the function signature is
stable across phases.

#### A.1.3 Bond distance

```rust
pub enum BondLengthMode { Crystal, Uff }

/// Hardcoded table of sp3 semiconductor crystal bond lengths.
/// Key: (min(Z_a, Z_b), max(Z_a, Z_b)). Values in Angstroms.
/// Derived from zinc blende / diamond cubic unit cell parameter a
/// via bond_length = a * sqrt(3) / 4.
const CRYSTAL_BOND_LENGTHS: &[((i16, i16), f64)] = &[
    ((6, 6), 1.545),    // Diamond C-C
    ((14, 14), 2.352),  // Silicon Si-Si
    ((6, 14), 1.889),   // 3C-SiC
    ((32, 32), 2.450),  // Germanium Ge-Ge
    ((50, 50), 2.810),  // alpha-Sn
    ((5, 7), 1.567),    // c-BN
    ((5, 15), 1.966),   // BP
    ((13, 7), 1.897),   // AlN (zinc blende)
    ((13, 15), 2.367),  // AlP
    ((13, 33), 2.443),  // AlAs
    ((31, 7), 1.946),   // GaN (zinc blende)
    ((31, 15), 2.360),  // GaP
    ((31, 33), 2.448),  // GaAs
    ((49, 15), 2.541),  // InP
    ((49, 33), 2.623),  // InAs
    ((49, 51), 2.806),  // InSb
    ((30, 16), 2.342),  // ZnS (zinc blende)
    ((30, 34), 2.454),  // ZnSe
    ((30, 52), 2.637),  // ZnTe
    ((48, 52), 2.806),  // CdTe
];

pub fn bond_distance(
    anchor_atomic_number: i16,
    new_atomic_number: i16,
    anchor_uff_type: &str,        // from hybridization detection
    new_element_default_uff_type: &str,  // default UFF type for new element
    bond_length_mode: BondLengthMode,
) -> f64
```

**Crystal mode:** look up `(min(Z_a, Z_b), max(Z_a, Z_b))` in `CRYSTAL_BOND_LENGTHS`.
If found, return the crystal value. If not found, fall back to UFF mode.

**UFF mode:** call `calc_bond_rest_length(1.0, uff_params_a, uff_params_b)` using the
anchor atom's detected UFF params and the new element's default UFF params. The UFF
params are already available from the hybridization detection step (A.1.1), which
calls `assign_uff_type()` — no additional UFF dependency is needed.

For the new atom's UFF type, use the default type for that element (e.g., `"C_3"` for
carbon, `"N_3"` for nitrogen). This is the same fallback used by `assign_uff_type()`
for atoms with no bonds.

#### A.1.4 sp3 candidate position computation

```rust
pub fn compute_sp3_candidates(
    anchor_pos: DVec3,
    existing_bond_dirs: &[DVec3],  // normalized
    bond_dist: f64,
) -> Vec<GuideDot>
```

**Case 4:** Empty vec. **Case 3:** `d4 = normalize(-(b1+b2+b3))`, 1 Primary dot.
**Case 2:** Given `b1, b2`: `mid = normalize(b1 + b2)`, `n = normalize(b1 x b2)`.
The two new directions lie symmetrically about `-mid`:
`d = -mid * cos(a) +/- n * sin(a)` where `a` is chosen so that `dot(b1, d)` equals
`cos(109.47 deg)`. Yields 2 Primary dots. **Case 1/0:** Empty vec (stubs for
Phase B/C).

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
    hybridization_override: Option<Hybridization>,  // None = auto-detect
    bond_mode: BondMode,                            // Covalent or Dative
    bond_length_mode: BondLengthMode,               // Crystal or Uff
) -> GuidedPlacementResult
```

Orchestrates: detect hybridization (using override or auto-detect) -> check saturation
(using bond_mode) -> compute bond distance (using bond_length_mode; crystal table
lookup with UFF fallback, or pure UFF) -> dispatch to `compute_sp3_candidates`
(sp2/sp1 in future phases). In Phase A, callers always pass `BondMode::Covalent`.
`BondLengthMode` defaults to `Crystal`.

#### A.1.6 Tests

**New file:** `rust/tests/crystolecule/guided_placement_test.rs` (register in `mod.rs`).

- **sp3 case 3:** CH3 -> 4th direction opposite centroid, angle ~109.47 deg to each bond
- **sp3 case 2:** CH2 -> 2 guides, all 4 mutual angles ~109.47 deg
- **sp3 saturated:** CH4 -> 0 dots, `remaining_slots == 0`
- **Bond distance (crystal):** C-C = 1.545 A, Si-Si = 2.352 A, Si-C = 1.889 A,
  GaAs = 2.448 A, BN = 1.567 A
- **Bond distance (UFF):** C-C ~1.51 A (UFF rest length, not sum of covalent radii),
  C-H ~1.08 A (no special case needed)
- **Bond distance (crystal fallback):** C-H not in crystal table -> falls back to UFF
- **Bond distance (UFF mode):** C-C in UFF mode -> uses UFF even though crystal table
  has an entry
- **Hybridization auto:** C+4 single -> Sp3, C+double -> Sp2, N+3 single -> Sp3
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
4. Call `compute_guided_placement()` with `bond_length_mode` (auto-detect hybridization; override added in Phase D)
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
2. Store guide dot positions and anchor position in the `AtomicStructureDecorator`
   (new field: `guide_placement_visuals: Option<GuidePlacementVisuals>`)

The tessellator reads `guide_placement_visuals` from the decorator and renders:
- `tessellate_sphere()` for each guide dot (magenta, 0.2/0.15 A radius)
- `tessellate_cylinder()` for each anchor-to-dot line (orange, `ANCHOR_ARROW_RADIUS`)

No phantom atoms are added to the structure. No changes to `AtomDisplayState`.

#### A.2.4 Guide dot hit testing — `add_atom_tool.rs`

```rust
pub fn hit_test_guide_dots(
    ray_start: &DVec3, ray_dir: &DVec3,
    guide_dots: &[GuideDot], hit_radius: f64,
) -> Option<usize>
```

Uses `sphere_hit_test` from `hit_test_utils`, returns index of closest hit (or None).
Called from API layer before atom/empty-space hit testing.

### A.3 Rendering — Direct Tessellation

No changes to `AtomDisplayState` or the per-atom rendering pipeline. Guide dot
visuals are tessellated directly from decorator data.

#### A.3.1 Decorator extension

Add to `AtomicStructureDecorator`:

```rust
pub struct GuidePlacementVisuals {
    pub anchor_pos: DVec3,
    pub guide_dots: Vec<GuideDot>,  // from guided_placement.rs
}
```

#### A.3.2 Tessellator additions — `atomic_tessellator.rs`

New function called after atom/bond tessellation:

```rust
pub fn tessellate_guide_placement(
    output_mesh: &mut Mesh,
    visuals: &GuidePlacementVisuals,
)
```

For each guide dot:
- `tessellate_sphere()` at dot position with magenta material
  `Vec3::new(1.0, 0.2, 1.0)`, radius 0.2 A (Primary) or 0.15 A (Secondary)
- `tessellate_cylinder()` from `anchor_pos` to dot position with
  `ANCHOR_ARROW_COLOR` material, `ANCHOR_ARROW_RADIUS`

This reuses existing tessellation helpers with no changes to the atom/impostor
rendering paths.

### A.4 API Layer — `atom_edit_api.rs`

#### A.4.1 New types

```rust
pub enum GuidedPlacementApiResult {
    NoAtomHit,
    AtomSaturated { has_additional_capacity: bool },
    GuidedPlacementStarted { guide_count: usize, anchor_atom_id: u32 },
}
```

`has_additional_capacity` is `true` when the atom is covalently saturated but has
lone pairs or empty orbitals (i.e., geometric max > covalent max). Flutter uses this
to show a context-aware SnackBar message directing the user to the bond mode toggle.

#### A.4.2 New API functions

Three functions following the standard pattern (`with_mut_cad_instance` ->
operation -> `refresh_structure_designer_auto`):

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_start_guided_placement(
    ray_start: APIVec3, ray_dir: APIVec3, atomic_number: i16,
    bond_length_mode: APIBondLengthMode,  // Crystal default
) -> GuidedPlacementApiResult
// hybridization_override parameter added in Phase D

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
        ray.start, ray.direction, atomicNumber, atomEditData.bondLengthMode);
      if (result == GuidedPlacementApiResult.noAtomHit) {
        widget.graphModel.atomEditCancelGuidedPlacement();
      }
    }
  } else {
    // Try to start guided placement
    final result = widget.graphModel.atomEditStartGuidedPlacement(
      ray.start, ray.direction, atomicNumber, atomEditData.bondLengthMode);
    if (result == GuidedPlacementApiResult.noAtomHit) {
      // Fall back to free placement
      widget.graphModel.atomEditAddAtomByRay(atomicNumber, planeNormal, ray.start, ray.direction);
    }
  }
}
```

#### A.5.2 Hybridization dropdown

Deferred to Phase D. In Phase A, auto-detection via UFF is always used (sp3 for most
common cases). The dropdown and `hybridization_override` API parameter are added in
Phase D when sp2/sp1 geometry makes the override meaningful.

#### A.5.2b Bond length mode dropdown

Add a Crystal/UFF dropdown to the atom edit panel (alongside the element selector).
Add `bondLengthMode` property to `AtomEditData` (Flutter model), default:
`APIBondLengthMode.crystal`. The selected value is passed to
`atomEditStartGuidedPlacement()`. The dropdown resets to `crystal` when switching
away from the Add Atom tool.

This is introduced in Phase A (not deferred to Phase D) because crystal bond lengths
are immediately valuable for the primary use case — building on sp3 semiconductor
crystal lattices.

#### A.5.3 New model methods

Three methods following existing pattern (API call -> `refreshFromKernel()`):
`atomEditStartGuidedPlacement()`, `atomEditPlaceGuidedAtom()`,
`atomEditCancelGuidedPlacement()`.

#### A.5.4 Escape key handling

Add handler in `_StructureDesignerViewportState`: if Escape pressed and
`isInGuidedPlacement()`, call `cancelGuidedPlacement()`.

#### A.5.5 Tool switch cancellation

In `AtomEditData::set_active_tool()` (in `atom_edit_data.rs`), if current tool is
`AddAtom(GuidedPlacement)` and switching away, transition to `Idle` first.

### A.6 Saturation Feedback

When `AtomSaturated` is returned, Flutter shows a SnackBar notification:

```dart
if (result case GuidedPlacementApiResult.atomSaturated(:final hasAdditionalCapacity)) {
  final message = hasAdditionalCapacity
      ? 'Atom is covalently saturated. Switch to Dative bond mode to access additional bonding positions.'
      : 'Atom is fully bonded';
  ScaffoldMessenger.of(context).showSnackBar(
    SnackBar(content: Text(message), duration: const Duration(seconds: 2)),
  );
}
```

This follows the existing notification pattern used throughout the app (e.g.,
`factor_into_subnetwork_dialog.dart`, `import_cnnd_library_dialog.dart`). No Rust-side
rendering changes needed — purely a Flutter-side notification. The context-aware
message guides the user toward the bond mode toggle when the atom has unused lone
pairs or empty orbitals.

### A.7 Implementation Order

1. Geometry module (A.1) — pure computation, independently testable
2. Tests for geometry (A.1.6) — validate math before integration
3. Decorator extension (A.3.1) — add `GuidePlacementVisuals` to decorator
4. Tessellation function (A.3.2) — `tessellate_guide_placement()` in `atomic_tessellator.rs`
5. Tool state enum (A.2.1) — change `AddAtomToolState` in `types.rs`, fix match arms
   across `types.rs`, `atom_edit_data.rs`, and `atom_edit_api.rs`
6. Decoration phase (A.2.3) — connect geometry to rendering in `atom_edit_data.rs` `eval()`
7. Tool functions (A.2.2) — start/place/cancel logic in `add_atom_tool.rs`
8. API layer (A.4) — expose to Flutter via `atom_edit_api.rs`
9. FRB codegen
10. Flutter model methods (A.5.3)
11. Flutter bond length mode dropdown (A.5.2b)
12. Flutter click dispatch (A.5.1)
13. Flutter escape handling (A.5.4)
14. Saturation feedback (A.6)
15. Integration testing

Steps 1-2 in isolation; 3-4 safe additions; 5-8 Rust integration; 9-14 Flutter; 15 full app.

### A.8 Manual Testing Checklist

After Phase A is complete, you can manually verify these behaviors in the running app:

1. **Basic guided placement (sp3 case 3 — e.g., CH3 methyl):**
   - Create an atom edit node. Place a carbon atom (free placement in empty space).
   - Add 3 hydrogens bonded to it manually (or use any method to get a carbon with 3 bonds).
   - Select the **Add Atom** tool, pick an element (e.g., H).
   - Click the carbon atom. You should see:
     - The carbon turns **yellow** (anchor highlight).
     - **1 magenta guide dot** appears at the 4th tetrahedral position, opposite the
       centroid of the 3 existing bonds.
     - An **orange cylinder** connects the carbon to the guide dot.
   - Click the guide dot → a hydrogen is placed and bonded. Guides disappear.

2. **sp3 case 2 (e.g., CH2 methylene):**
   - Start with a carbon with 2 bonds (e.g., C bonded to 2 H's).
   - Click it with Add Atom tool → **2 magenta guide dots** should appear, symmetrically
     placed so all 4 bond angles are ~109.5 deg.
   - Click either dot → atom placed, guides cleared.

3. **Saturated atom feedback (sp3 case 4):**
   - Click a fully bonded atom (e.g., CH4 carbon with 4 bonds).
   - A **SnackBar** should appear: "Atom is fully bonded". No guide dots shown.

4. **Saturation with additional capacity (e.g., NH3 nitrogen):**
   - Click a nitrogen with 3 bonds. In covalent mode (default), it's saturated.
   - SnackBar should say: "Atom is covalently saturated. Switch to Dative bond mode
     to access additional bonding positions."

5. **Cancel and navigation:**
   - Enter guided placement (click an atom with open slots).
   - Press **Escape** → guides disappear, return to idle.
   - Enter guided placement again, then click **empty space** → cancel.
   - Enter guided placement, then click a **different atom** → guides recompute for
     the new anchor.

6. **Free placement still works:**
   - With Add Atom tool active, click empty space (not on any atom) → atom is placed
     at ray-plane intersection as before. Guided placement is not triggered.

7. **Bond length mode dropdown:**
   - The atom edit panel should show a **Crystal / UFF** dropdown.
   - With Crystal mode: place a carbon on an existing silicon atom → bond distance
     should be ~1.889 A (SiC crystal value).
   - Switch to UFF mode, repeat → bond distance should be the UFF-computed value
     (slightly different).
   - The dropdown resets to Crystal when switching tools.

8. **Tool switching:**
   - Enter guided placement, then switch to a different tool (e.g., Default or Add Bond).
   - Guided placement should be cancelled automatically (no lingering guide dots).

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

### B.6 Manual Testing Checklist

After Phase B is complete (requires Phase A), test these in the running app:

1. **Wireframe sphere on bare atom:**
   - Place a single carbon atom in empty space (no bonds).
   - Select Add Atom tool, pick an element (e.g., H), click the bare carbon.
   - You should see:
     - The carbon turns **yellow** (anchor highlight).
     - A **gray wireframe sphere** appears centered on the carbon, with radius equal
       to the bond distance (C-H in crystal or UFF mode).
   - No fixed guide dots — instead, a guide dot should **track your cursor** on the
     sphere surface as you move the mouse.

2. **Placement on sphere:**
   - Move your cursor to a desired position on the sphere surface.
   - Click → the new atom is placed at that position and bonded to the anchor.
   - The wireframe sphere disappears, tool returns to idle.

3. **Front hemisphere only:**
   - Move the cursor behind the sphere (from the camera's perspective). The guide dot
     should only appear on the user-facing (front) hemisphere — it should not snap to
     the back side of the sphere.

4. **Sphere radius matches bond length mode:**
   - Switch between Crystal and UFF bond length modes.
   - The wireframe sphere radius should change accordingly.

5. **Cancel from sphere mode:**
   - Enter sphere mode (click bare atom), then press Escape → sphere disappears.
   - Click bare atom again, then click empty space away from the sphere → cancels.

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

### C.5 Manual Testing Checklist

After Phase C is complete (requires Phase A + B), test these in the running app:

1. **Dihedral-aware placement (trans/cis dots):**
   - Build an ethane-like fragment: C-C where the second carbon has at least one hydrogen.
   - Select Add Atom tool (e.g., H), click the first carbon (which has 1 bond to the
     second carbon).
   - You should see **6 magenta guide dots** on a cone around the bond axis:
     - **3 larger dots** (primary, 0.2 A radius) — trans/staggered positions.
     - **3 smaller dots** (secondary, 0.15 A radius) — cis/eclipsed positions.
   - Orange cylinders connect the anchor to each dot.
   - Click a trans (larger) dot → atom placed at staggered position. Verify the
     resulting dihedral angle is ~60 deg relative to the reference atom on the other carbon.
   - Repeat, click a cis (smaller) dot → atom placed at eclipsed position (0 deg dihedral).

2. **Ring fallback (no dihedral reference):**
   - Place two carbons bonded together, with no other atoms on the second carbon
     (e.g., C-C where the second C is bare).
   - Click the first carbon with Add Atom tool.
   - You should see a **gray wireframe ring** (circle) around the bond axis, at the
     tetrahedral cone angle (109.47 deg from the existing bond).
   - **3 guide dots** should appear on the ring, spaced 120 deg apart, and they should
     **rotate together** as you move the mouse around the ring.
   - Click → one atom is placed at the clicked position.

3. **Ring geometry verification:**
   - The ring should be centered along the bond axis at the correct distance.
   - The 3 dots should maintain 120 deg spacing at all cursor positions.
   - Rotating the view should show the ring is perpendicular to the bond axis.

4. **Transition from ring to fixed dots:**
   - Start with C-C (bare, ring mode). Place one atom via the ring.
   - Now the first carbon has 2 bonds → click it again → should show 2 fixed guide
     dots (sp3 case 2 from Phase A), not a ring.

5. **Cancel from ring mode:**
   - Enter ring mode, press Escape → ring disappears. Click empty space → also cancels.

---

## Phase D — sp2/sp1 Geometry, Hybridization Dropdown, Bond Mode Toggle

**Goal:** sp2 (120 deg) and sp1 (180 deg) support. Hybridization override and dative
bond mode UI controls.

**Prerequisite:** Phase A. Phases B/C can run in parallel with D.

**Note:** This phase adds both the **Hybridization dropdown** and the **Bond Mode
toggle** to the atom edit panel. With only sp3 (Phase A), auto-detection is almost
always correct and covalent mode suffices. Once sp2/sp1 geometry is available, the
user needs the hybridization override to tell the system "this carbon should be sp2"
when the current bonding state is ambiguous. The bond mode toggle enables dative
bonding by unlocking lone pair and empty orbital positions.

### D.0 Hybridization Dropdown, Bond Mode Toggle, and API Changes

**Hybridization dropdown:** Add `hybridization_override: Option<APIHybridization>`
parameter to `atom_edit_start_guided_placement()`. Add `hybridizationOverride`
property to `AtomEditData` (Flutter model), backed by a dropdown in the atom edit
panel. Values: `null` (Auto), `sp3`, `sp2`, `sp1`. Resets to `null` when switching
away from Add Atom tool.

**Bond mode toggle:** Add `bond_mode: APIBondMode` parameter to
`atom_edit_start_guided_placement()`. Add `bondMode` property to `AtomEditData`
(Flutter model), backed by a Covalent/Dative toggle in the atom edit panel. Default:
`covalent`. Resets to `covalent` when switching away from Add Atom tool.

Update Flutter click dispatch to pass both overrides through to the Rust API.

`APIHybridization` is an FRB-friendly enum: `{ auto_, sp3, sp2, sp1 }`.
`APIBondMode` is an FRB-friendly enum: `{ covalent, dative }`.

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

### D.3 Hybridization-Aware and Bond-Mode-Aware Dispatch

Update `compute_guided_placement()` to accept `bond_mode` and match on hybridization
(from override or auto-detect) to dispatch to `compute_sp3/sp2/sp1_candidates`.

When the user sets the hybridization dropdown to sp2 or sp1, the override flows
through and selects the correct geometry even when auto-detection would default to sp3.

When the user sets bond mode to Dative, `effective_max_neighbors` returns the geometric
max (sp3=4, sp2=3, sp1=2) instead of the element-specific covalent max. This unlocks
lone pair and empty orbital positions. For elements where covalent max already equals
geometric max (e.g., carbon sp3 = 4), the toggle has no effect.

Verify `effective_max_neighbors` covers all element+hybridization+bond_mode combos:
- Covalent: C sp2=3, C sp1=2, N sp3=3, N sp2=3, O sp3=2, O sp2=2, B sp2=3
- Dative: N sp3=4, O sp3=4, O sp2=3, B sp2=3 (same; empty p-orbital is out of scope)

### D.4 Tests

- **sp2 case 2:** formaldehyde (C=O + 1H) -> remaining direction in molecular plane, ~120 deg
- **sp2 case 1:** C=O only -> 2 guides at +/-120 deg in correct plane
- **sp1 case 1:** C triple bond -> guide at 180 deg (opposite)
- **Hybridization dispatch:** C+double -> sp2, C+triple -> sp1, aromatic C -> sp2
- **Hybridization override:** C+1 single with override=Sp2 -> uses Sp2 geometry
- **Saturation:** C sp2 at 3, C sp1 at 2, N sp2 at 3
- **sp2 ring fallback:** ring uses 120 deg, not 109.47 deg
- **Bond mode — dative N sp3:** NH3 (3 bonds) + Covalent -> 0 dots (saturated);
  NH3 + Dative -> 1 dot at lone pair position
- **Bond mode — dative O sp3:** H2O (2 bonds) + Covalent -> 0 dots;
  H2O + Dative -> 2 dots at lone pair positions
- **Bond mode — no effect on C sp3:** CH3 (3 bonds) + Covalent -> 1 dot;
  CH3 + Dative -> 1 dot (same, because covalent max = geometric max = 4)
- **Bond mode — dative B:** BH3 (3 bonds, sp2) + Covalent -> 0 dots;
  user overrides hybridization to sp3 + Dative -> 1 dot (acceptor orbital)
- **Saturation feedback:** NH3 + Covalent -> `AtomSaturated` with
  `has_additional_capacity: true` (for context-aware SnackBar message)

### D.5 Manual Testing Checklist

After Phase D is complete (requires Phase A; B/C independent), test these in the running app:

1. **Hybridization dropdown:**
   - The atom edit panel should show a **Hybridization** dropdown: Auto / sp3 / sp2 / sp1.
   - Default is Auto. It resets to Auto when switching away from Add Atom tool.

2. **sp2 geometry (120 deg):**
   - Place a carbon, set hybridization to **sp2**.
   - Bond 2 atoms to it. Click the carbon with Add Atom → **1 guide dot** should appear
     in the molecular plane at ~120 deg from both existing bonds.
   - With only 1 bond: **2 guide dots** at +/-120 deg in the plane (if upstream reference
     exists) or a ring (if no reference).

3. **sp1 geometry (180 deg):**
   - Place a carbon, set hybridization to **sp1**.
   - Bond 1 atom to it. Click with Add Atom → **1 guide dot** directly opposite the
     existing bond (180 deg).

4. **Hybridization override resolves ambiguity:**
   - Place a carbon with 1 single bond. In Auto mode it defaults to sp3.
   - Switch dropdown to sp2 → guide dots change to 120 deg geometry.
   - Switch to sp1 → single dot at 180 deg.
   - Switch back to Auto → returns to sp3 (tetrahedral).

5. **Bond mode toggle (Covalent / Dative):**
   - The atom edit panel should show a **Bond Mode** toggle: Covalent (default) / Dative.
   - It resets to Covalent when switching away from Add Atom tool.

6. **Dative mode — nitrogen (NH3):**
   - Build NH3: nitrogen with 3 hydrogen bonds.
   - In Covalent mode, click the nitrogen → SnackBar says "Atom is covalently saturated.
     Switch to Dative bond mode to access additional bonding positions."
   - Switch to Dative mode, click the nitrogen → **1 guide dot** appears at the lone
     pair position (4th tetrahedral direction).

7. **Dative mode — oxygen (H2O):**
   - Build H2O: oxygen with 2 hydrogen bonds.
   - Covalent mode → saturated (SnackBar). Dative mode → **2 guide dots** at the two
     lone pair positions.

8. **Dative mode — no effect on carbon:**
   - Build CH3 (carbon with 3 bonds). Both Covalent and Dative modes should show
     **1 guide dot** (carbon sp3 covalent max = geometric max = 4).

9. **Combined controls:**
   - Build a boron with 3 bonds (sp2 auto-detected).
   - Covalent mode → saturated (sp2 = 3 max).
   - Override hybridization to sp3 + Dative mode → **1 guide dot** appears
     (acceptor orbital position above/below the plane).

10. **All three dropdowns together:**
    - Verify that Hybridization, Bond Mode, and Bond Length Mode dropdowns all appear
      in the atom edit panel and work independently.
    - Change each one and confirm guided placement responds correctly.
    - Switch to a different tool and back → all three reset to defaults
      (Auto, Covalent, Crystal).
