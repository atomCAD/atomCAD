//! Surface-reconstruction **patches** — the node-free core of
//! `doc/design_surface_patches.md` §4–§6.
//!
//! A patch is one tile of a reconstructed surface plus the volume it displaces:
//! `{ tile, tiling_vectors, cut_volume }`. This module holds both halves of the
//! model, so it is testable on plain [`AtomicStructure`]s without the
//! node-network machinery:
//!
//! - **Authoring** (§4) — [`extract_patch_tile`] pulls a tile out of an authored
//!   slab given a cut volume: interior atoms become real tile atoms, outward
//!   bonded neighbours are copied as **patch-ghosts** (the neighbour-tile /
//!   bulk-collar copies that weld at apply time). [`validate_tiling_vectors`]
//!   enforces the 1–3 linearly independent tiling vectors.
//! - **Applying** (§5) — [`select_patch_cells`] decides which cells `o = origin +
//!   Σ kᵢ·vᵢ` receive a tile, and [`apply_patch`] runs the cut → place → weld →
//!   drop → passivate pipeline, returning the reconstructed atoms plus a
//!   [`CompatibilityReport`] (§6).
//!
//! The `patch_build` / `patch_latticefill` nodes in `atomcad-structure-designer`
//! are thin wrappers over these functions.

use crate::atomic_structure::{AtomicStructure, TagError};
use crate::guided_placement::{Hybridization, covalent_max_neighbors};
use crate::hydrogen_passivation::{AddHydrogensOptions, add_hydrogens};
use crate::unit_cell_struct::UnitCellStruct;
use crate::weld::weld_coincident_atoms;
use atomcad_geo_tree::GeoNode;
use atomcad_geo_tree::implicit_geometry::ImplicitGeometry3D;
use atomcad_util::daabox::DAABox;
use glam::f64::{DQuat, DVec3};
use glam::i32::IVec3;
use std::collections::{HashMap, HashSet};

/// Default build threshold `ε` (Å). A slab atom counts as interior when its
/// `cut_volume` membership SDF ≤ `ε`. Must be large enough to catch atoms
/// authored right on the cut surface, but well below the nearest interplanar
/// spacing so it never grabs the layer below. See design §8, open question 1.
pub const DEFAULT_BUILD_THRESHOLD: f64 = 0.1;

/// Default weld tolerance (Å). Below the smallest interatomic spacing so
/// distinct lattice sites never over-merge (§3 / §5).
pub const DEFAULT_WELD_TOLERANCE: f64 = 0.1;

/// Membership threshold for the cut: a substrate atom is removed when its
/// position is inside the (translated) `cut_volume` SDF within this margin.
/// Mirrors `patch_build`'s interior threshold so the displaced surface the build
/// step captured as interior is exactly the surface the apply step removes.
const CUT_MEMBERSHIP_EPSILON: f64 = 0.1;

/// Membership threshold for the cell-selection inclusion test (Å). A projected
/// test point counts as inside the region when its SDF ≤ this — so the boundary
/// belongs to the region, and a test plane that lands *on* the region boundary
/// (e.g. origin-height mode when the surface is built through the lattice
/// origin) still selects rather than failing on a hair of floating-point /
/// sub-Ångström offset. Matches the cut/build threshold.
const REGION_MEMBERSHIP_EPSILON: f64 = 0.1;

/// Statistics produced by `apply_patch`, surfaced (eventually) as a
/// compatibility badge (§6). Falls directly out of the weld:
/// - `welded_ghosts` — patch-ghosts that found a real twin and fused (the
///   realized periodic / collar bonds).
/// - `orphaned_ghosts` — patch-ghosts with no real twin, dropped as true
///   reconstruction edges (a high count at the expected depth means the patch
///   was applied too high — floating, un-welded collars).
/// - `overcoordinated_atoms` — real atoms left with more bonds than their
///   element's tetrahedral ceiling after welding (the "applied too low /
///   sub-surface" failure mode).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompatibilityReport {
    /// Number of cells (tiles) selected and placed. **Zero means nothing was
    /// tiled** — typically the test plane missed the target (see
    /// `test_height_at_origin`), so the other three counts being zero is *not*
    /// success.
    pub placed_cells: usize,
    pub welded_ghosts: usize,
    pub orphaned_ghosts: usize,
    pub overcoordinated_atoms: usize,
}

// ============================================================================
// Authoring (§4 "Extraction")
// ============================================================================

/// Validates the tiling vectors per design §4: there must be 1–3 of them and
/// they must be linearly independent.
pub fn validate_tiling_vectors(vectors: &[IVec3]) -> Result<(), String> {
    match vectors.len() {
        0 => Err("patch_build: tiling_vectors must have 1–3 entries, got 0".to_string()),
        1 => {
            if vectors[0] == IVec3::ZERO {
                Err("patch_build: the single tiling vector is zero (degenerate)".to_string())
            } else {
                Ok(())
            }
        }
        2 => {
            // Linearly independent iff the cross product is non-zero.
            let cross = vectors[0].as_dvec3().cross(vectors[1].as_dvec3());
            if cross.length_squared() < 1e-9 {
                Err("patch_build: tiling vectors are linearly dependent".to_string())
            } else {
                Ok(())
            }
        }
        3 => {
            // Linearly independent iff the scalar triple product is non-zero.
            let det = vectors[0]
                .as_dvec3()
                .dot(vectors[1].as_dvec3().cross(vectors[2].as_dvec3()));
            if det.abs() < 1e-9 {
                Err("patch_build: tiling vectors are linearly dependent".to_string())
            } else {
                Ok(())
            }
        }
        n => Err(format!(
            "patch_build: tiling_vectors must have 1–3 entries, got {n}"
        )),
    }
}

/// Extracts the tile from `source` given the `cut_volume` geometry (a real-space
/// SDF) and the build threshold `epsilon` (§4 "Extraction").
///
/// The extracted atoms are kept **in the coordinates they were drawn in** — they
/// came straight off the authored slab, so they are already lattice-registered.
/// `patch_latticefill` then places the tile by whole-lattice-vector translations
/// only (the tiling steps plus the optional `origin` offset), which keeps every
/// atom on the lattice so the welds line up; at the default offset nothing is
/// moved, so the patch reappears exactly where it was authored.
///
/// This is the node-free core so the extraction logic is testable without the
/// node-network machinery.
pub fn extract_patch_tile(
    source: &AtomicStructure,
    cut_volume: &GeoNode,
    epsilon: f64,
) -> AtomicStructure {
    // 1. Interior `I` = slab atoms inside the cut volume (membership SDF ≤ ε).
    let mut interior: HashSet<u32> = HashSet::new();
    for (id, atom) in source.iter_atoms() {
        if cut_volume.implicit_eval_3d(&atom.position) <= epsilon {
            interior.insert(*id);
        }
    }

    // 2. Ghosts `G` = atoms *outside* the cut bonded to some interior atom
    //    (distance-1 only). These are the neighbour-tile and bulk-collar copies.
    let mut ghosts: HashSet<u32> = HashSet::new();
    for id in &interior {
        let atom = source.get_atom(*id).expect("interior atom exists");
        for bond in &atom.bonds {
            let partner = bond.other_atom_id();
            if !interior.contains(&partner) {
                ghosts.insert(partner);
            }
        }
    }

    // 3. Build the tile: interior atoms (real) + ghost atoms (patch-ghost flag).
    //    Sort ids for a deterministic id assignment in the new structure.
    let mut tile = AtomicStructure::new();
    let mut id_map: HashMap<u32, u32> = HashMap::new();

    let mut interior_ids: Vec<u32> = interior.iter().copied().collect();
    interior_ids.sort_unstable();
    let mut ghost_ids: Vec<u32> = ghosts.iter().copied().collect();
    ghost_ids.sort_unstable();

    for id in &interior_ids {
        let a = source.get_atom(*id).expect("interior atom exists");
        let new_id = tile.add_atom(a.atomic_number, a.position);
        // Preserve structurally-meaningful per-atom metadata; the rest (select,
        // display-ghost) starts cleared (`add_atom` zeroes flags).
        tile.set_atom_frozen(new_id, a.is_frozen());
        tile.set_atom_hybridization_override(new_id, a.hybridization_override());
        id_map.insert(*id, new_id);
    }
    for id in &ghost_ids {
        let a = source.get_atom(*id).expect("ghost atom exists");
        let new_id = tile.add_atom(a.atomic_number, a.position);
        tile.set_atom_frozen(new_id, a.is_frozen());
        tile.set_atom_hybridization_override(new_id, a.hybridization_override());
        tile.set_atom_patch_ghost(new_id, true);
        id_map.insert(*id, new_id);
    }

    // 4. Bonds: every slab bond with at least one endpoint in `I`
    //    (interior–interior and interior–ghost). Ghost–ghost bonds are dropped:
    //    we only walk interior atoms, and an interior atom's outside partners
    //    are exactly the ghosts, so both endpoints are always mapped.
    let mut seen: HashSet<(u32, u32)> = HashSet::new();
    for id in &interior_ids {
        let a = source.get_atom(*id).expect("interior atom exists");
        for bond in &a.bonds {
            let partner = bond.other_atom_id();
            let Some(&new_partner) = id_map.get(&partner) else {
                continue;
            };
            let key = if *id < partner {
                (*id, partner)
            } else {
                (partner, *id)
            };
            if seen.insert(key) {
                tile.add_bond(id_map[id], new_partner, bond.bond_order());
            }
        }
    }

    // The tile keeps its authored absolute coordinates — no re-expression. The
    // cut volume is likewise stored as-drawn (see `eval`).
    tile
}

// ============================================================================
// Cell selection (§5 "Which cells get a tile")
// ============================================================================

/// Computes an orthonormal basis of the **non-periodic** complement of the
/// subspace spanned by the (real-space) tiling vectors. Returns 0–2 directions:
/// 3 periodic vectors → no free axis; 2 → the surface normal; 1 → the two
/// transverse directions. Used to "free" the containment test along the
/// non-periodic axes (§5).
fn free_directions(periodic_real: &[DVec3]) -> Vec<DVec3> {
    // Orthonormal basis of the periodic span (Gram-Schmidt).
    let mut span: Vec<DVec3> = Vec::new();
    for &v in periodic_real {
        let mut w = v;
        for b in &span {
            w -= *b * w.dot(*b);
        }
        if w.length() > 1e-9 {
            span.push(w.normalize());
        }
    }
    // Complete to a full R^3 basis with the standard axes; the leftovers span
    // the complement.
    let mut free: Vec<DVec3> = Vec::new();
    for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
        let mut w = axis;
        for b in span.iter().chain(free.iter()) {
            w -= *b * w.dot(*b);
        }
        if w.length() > 1e-6 {
            free.push(w.normalize());
        }
    }
    free
}

/// A selected cell: its integer step indices `k` (one per tiling vector, needed
/// to box the frontier in the debug view) and the resulting lattice offset.
pub struct SelectedCell {
    pub k: Vec<i32>,
    pub offset: IVec3,
}

/// Per free (non-periodic) direction, the midpoint of the **target** atoms'
/// min/max projection onto that direction. This is the one axis of an oriented
/// bounding box that matters for choosing a test height: measured along the real
/// normal (not global XYZ), it always lands between the slab's bottom and top
/// layers, so it is inside a prismatic region regardless of how the slab is
/// tilted. Returns `0.0` for a direction with no target atoms. See
/// `doc/design_patch_cell_selection.md`.
pub fn region_center_depths(target: &AtomicStructure, free_dirs: &[DVec3]) -> Vec<f64> {
    free_dirs
        .iter()
        .map(|d| {
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for atom in target.atoms_values() {
                let t = atom.position.dot(*d);
                lo = lo.min(t);
                hi = hi.max(t);
            }
            if lo.is_finite() { 0.5 * (lo + hi) } else { 0.0 }
        })
        .collect()
}

/// Projects a point onto the test plane: in-plane coordinates kept, each
/// non-periodic component overwritten with the region's centre depth along that
/// direction. This is how "ignore how far it sticks out along the normal" is
/// realized — the normal coordinate is replaced by a height known to be inside
/// the region.
fn project_to_test_plane(p: DVec3, free_dirs: &[DVec3], center_depths: &[f64]) -> DVec3 {
    let mut s = p;
    for (d, depth) in free_dirs.iter().zip(center_depths.iter()) {
        s += *d * (*depth - s.dot(*d));
    }
    s
}

/// True if `s` is inside the region (`region_volume` SDF when present, else the
/// bounding box).
fn point_in_region(s: DVec3, region_volume: Option<&GeoNode>, region_bounds: &DAABox) -> bool {
    match region_volume {
        Some(geo) => geo.implicit_eval_3d(&s) <= REGION_MEMBERSHIP_EPSILON,
        None => region_bounds
            .expand(REGION_MEMBERSHIP_EPSILON)
            .contains_point(s),
    }
}

/// Selects the cells `o = origin + Σ kᵢ·vᵢ` that receive a tile: those whose
/// **interior atoms**, placed at the cell and projected onto the test plane,
/// **all** lie inside the region (whole-cell containment in the periodic
/// directions, free along the non-periodic ones — §5). The atoms carry both the
/// real tile shape and its true position, so there is no synthetic anchor and no
/// rhombus approximation. `origin` is the user's whole-cell offset (default zero
/// = as authored). `region_bounds` bounds the integer search; `region_volume`
/// (when present) is the actual containment gate. Returns each cell with its
/// step indices `k` (needed to box the frontier debug view).
///
/// Public for node-free testing of the containment rule.
/// See `doc/design_patch_cell_selection.md`.
#[allow(clippy::too_many_arguments)]
pub fn select_patch_cells(
    origin: IVec3,
    tiling_vectors: &[IVec3],
    region_lattice: &UnitCellStruct,
    region_volume: Option<&GeoNode>,
    region_bounds: &DAABox,
    interior_positions: &[DVec3],
    free_dirs: &[DVec3],
    center_depths: &[f64],
) -> Vec<SelectedCell> {
    // No interior atoms → nothing to sample the cut footprint with (a purely
    // subtractive patch would need cut-SDF sampling, not implemented). Select
    // nothing rather than vacuously selecting every cell.
    if interior_positions.is_empty() {
        return Vec::new();
    }

    let periodic_real: Vec<DVec3> = tiling_vectors
        .iter()
        .map(|v| region_lattice.ivec3_lattice_to_real(v))
        .collect();
    let region_center = region_bounds.center();
    let diag = region_bounds.size().length();
    // Centroid of the interior atoms — the reference for bounding the integer
    // search so the tiling can reach the region even when the authored patch is
    // far from it (small patch, large workpiece).
    let centroid = if interior_positions.is_empty() {
        DVec3::ZERO
    } else {
        interior_positions.iter().copied().sum::<DVec3>() / interior_positions.len() as f64
    };
    let centroid_to_center = (region_center - centroid).length();

    // Bound |kᵢ| by how many tiling steps could possibly land inside the search
    // box, plus a margin cell.
    let step_bounds: Vec<i32> = periodic_real
        .iter()
        .map(|rv| {
            let len = rv.length();
            if len < 1e-9 {
                0
            } else {
                ((diag + centroid_to_center) / len).ceil() as i32 + 1
            }
        })
        .collect();

    let mut cells = Vec::new();
    for k in iter_step_tuples(&step_bounds) {
        let mut o = origin;
        for (ki, v) in k.iter().zip(tiling_vectors.iter()) {
            o += *v * *ki;
        }
        let place = region_lattice.ivec3_lattice_to_real(&o);
        let inside = interior_positions.iter().all(|p| {
            let s = project_to_test_plane(*p + place, free_dirs, center_depths);
            point_in_region(s, region_volume, region_bounds)
        });
        if inside {
            cells.push(SelectedCell { k, offset: o });
        }
    }
    cells
}

/// The **frontier** cells for the debug view: the Cartesian product of each
/// periodic direction's selected-index range widened by one (`[min−1, max+1]`),
/// minus the cells that were actually selected. When **nothing** was selected
/// the range degenerates, so we instead show the `[−1, +1]` block around the
/// origin — otherwise the debug view would be empty exactly when the user most
/// needs to see where the (rejected) tiles would have gone.
fn compute_frontier(
    selected: &[SelectedCell],
    origin: IVec3,
    tiling_vectors: &[IVec3],
) -> Vec<IVec3> {
    let dims = tiling_vectors.len();
    let (mins, maxs) = if selected.is_empty() {
        (vec![-1; dims], vec![1; dims])
    } else {
        let mut mins = vec![i32::MAX; dims];
        let mut maxs = vec![i32::MIN; dims];
        for c in selected {
            for i in 0..dims {
                mins[i] = mins[i].min(c.k[i]);
                maxs[i] = maxs[i].max(c.k[i]);
            }
        }
        for i in 0..dims {
            mins[i] -= 1;
            maxs[i] += 1;
        }
        (mins, maxs)
    };
    let selected_keys: std::collections::HashSet<Vec<i32>> =
        selected.iter().map(|c| c.k.clone()).collect();

    // Cartesian product of the widened ranges.
    let mut tuples: Vec<Vec<i32>> = vec![vec![]];
    for i in 0..dims {
        let mut next = Vec::new();
        for prefix in &tuples {
            for ki in mins[i]..=maxs[i] {
                let mut t = prefix.clone();
                t.push(ki);
                next.push(t);
            }
        }
        tuples = next;
    }

    tuples
        .into_iter()
        .filter(|k| !selected_keys.contains(k))
        .map(|k| {
            let mut o = origin;
            for (ki, v) in k.iter().zip(tiling_vectors.iter()) {
                o += *v * *ki;
            }
            o
        })
        .collect()
}

/// Enumerates every integer tuple `k` with `kᵢ ∈ [-bounds[i], bounds[i]]`.
fn iter_step_tuples(bounds: &[i32]) -> Vec<Vec<i32>> {
    let mut result: Vec<Vec<i32>> = vec![vec![]];
    for &b in bounds {
        let mut next = Vec::new();
        for prefix in &result {
            for k in -b..=b {
                let mut tuple = prefix.clone();
                tuple.push(k);
                next.push(tuple);
            }
        }
        result = next;
    }
    result
}

// ============================================================================
// Core apply (§5 "Algorithm")
// ============================================================================

/// Real-space axis-aligned bounding box of a structure's atoms, if any.
pub fn atom_aabb(structure: &AtomicStructure) -> Option<DAABox> {
    let mut iter = structure.atoms_values();
    let first = iter.next()?;
    let mut min = first.position;
    let mut max = first.position;
    for atom in iter {
        min = min.min(atom.position);
        max = max.max(atom.position);
    }
    Some(DAABox::from_min_max(min, max))
}

/// Counts real (non-ghost, non-hydrogen) atoms left with more bonds than their
/// element's tetrahedral ceiling — the over-coordination failure mode (§6).
fn count_overcoordinated(structure: &AtomicStructure) -> usize {
    structure
        .atoms_values()
        .filter(|atom| {
            if atom.is_patch_ghost() {
                return false; // about to be dropped; not part of the result
            }
            let z = structure.effective_atomic_number(atom);
            if z <= 1 {
                return false; // hydrogens / markers
            }
            // Per-element tetrahedral ceiling (the most permissive coordination).
            let ceiling = covalent_max_neighbors(z, Hybridization::Sp3);
            let bonds = atom.bonds.iter().filter(|b| !b.is_delete_marker()).count();
            bonds > ceiling
        })
        .count()
}

/// Places a copy of `tile` at offset `o` for a debug view: optionally projected
/// onto the test plane and/or flagged frozen. Used only by the debug branches of
/// `apply_patch` (see `doc/design_patch_cell_selection.md`).
#[allow(clippy::too_many_arguments)]
fn place_debug_tile(
    out: &mut AtomicStructure,
    tile: &AtomicStructure,
    o: &IVec3,
    region_lattice: &UnitCellStruct,
    project: bool,
    free_dirs: &[DVec3],
    center_depths: &[f64],
    frozen: bool,
) -> Result<(), TagError> {
    let t = region_lattice.ivec3_lattice_to_real(o);
    let mut copy = tile.clone();
    copy.transform(&DQuat::IDENTITY, &t);
    if project || frozen {
        let ids: Vec<u32> = copy.atom_ids().copied().collect();
        for id in ids {
            if project {
                let p = copy.get_atom(id).expect("placed atom").position;
                copy.set_atom_position(id, project_to_test_plane(p, free_dirs, center_depths));
            }
            if frozen {
                copy.set_atom_frozen(id, true);
            }
        }
    }
    out.add_atomic_structure(&copy)?;
    Ok(())
}

/// Applies a patch over a region (§5). `region_volume` is the containment SDF
/// (`None` → fall back to `region_bounds`); `region_bounds` bounds the integer
/// cell search. Returns the reconstructed atoms plus a [`CompatibilityReport`].
///
/// `debug_project` / `debug_frontier` enable the two debug visualizations; both
/// leave the [`CompatibilityReport`] computed from the real (non-debug) weld of
/// the selected cells, so the badge stays truthful. See
/// `doc/design_patch_cell_selection.md`.
///
/// This is the node-free core so the model is testable on plain
/// `AtomicStructure`s without the node-network machinery.
#[allow(clippy::too_many_arguments)]
pub fn apply_patch(
    target: &AtomicStructure,
    region_lattice: &UnitCellStruct,
    region_volume: Option<&GeoNode>,
    region_bounds: &DAABox,
    tile: &AtomicStructure,
    tiling_vectors: &[IVec3],
    cut_volume: &GeoNode,
    origin: IVec3,
    passivate: bool,
    tolerance: f64,
    test_height_at_origin: bool,
    debug_project: bool,
    debug_frontier: bool,
) -> Result<(AtomicStructure, CompatibilityReport), TagError> {
    // Test-plane frame: the periodic subspace is spanned by the tiling vectors;
    // the free (non-periodic) directions are its complement. The centre depth
    // along each is either the lattice origin's height (0 — simple, default) or
    // the target slab's own mid-height (robust to a target offset from the
    // origin along the normal). See doc/design_patch_cell_selection.md.
    let periodic_real: Vec<DVec3> = tiling_vectors
        .iter()
        .map(|v| region_lattice.ivec3_lattice_to_real(v))
        .collect();
    let free_dirs = free_directions(&periodic_real);
    let center_depths = if test_height_at_origin {
        vec![0.0; free_dirs.len()]
    } else {
        region_center_depths(target, &free_dirs)
    };
    let interior_positions: Vec<DVec3> = tile
        .atoms_values()
        .filter(|a| !a.is_patch_ghost())
        .map(|a| a.position)
        .collect();

    let selected = select_patch_cells(
        origin,
        tiling_vectors,
        region_lattice,
        region_volume,
        region_bounds,
        &interior_positions,
        &free_dirs,
        &center_depths,
    );
    let selected_offsets: Vec<IVec3> = selected.iter().map(|c| c.offset).collect();

    // ---- Real pipeline on the selected cells (drives both the result and the
    //      report; the report is captured here even in debug modes). ----
    let mut result = target.clone();
    let ghosts_per_tile = tile.atoms_values().filter(|a| a.is_patch_ghost()).count();
    let total_placed_ghosts = ghosts_per_tile * selected_offsets.len();

    // Step 3 — Cut: remove substrate atoms inside the translated cut_volume.
    for o in &selected_offsets {
        let t = region_lattice.ivec3_lattice_to_real(o);
        let to_remove: Vec<u32> = result
            .iter_atoms()
            .filter(|(_, a)| {
                cut_volume.implicit_eval_3d(&(a.position - t)) <= CUT_MEMBERSHIP_EPSILON
            })
            .map(|(id, _)| *id)
            .collect();
        for id in to_remove {
            result.delete_atom(id);
        }
    }

    // Step 4 — Place: add a copy of the tile translated by each offset.
    for o in &selected_offsets {
        let t = region_lattice.ivec3_lattice_to_real(o);
        let mut copy = tile.clone();
        copy.transform(&DQuat::IDENTITY, &t);
        result.add_atomic_structure(&copy)?;
    }

    // Step 5 — Weld: fuse tile↔tile (periodic) and tile↔bulk (collar) at once.
    weld_coincident_atoms(&mut result, tolerance);

    // §6 stats: any atom still flagged patch-ghost found no real twin.
    let orphaned_ghosts = result.atoms_values().filter(|a| a.is_patch_ghost()).count();
    let welded_ghosts = total_placed_ghosts.saturating_sub(orphaned_ghosts);
    let overcoordinated_atoms = count_overcoordinated(&result);

    // Step 6 — Drop unwelded patch-ghosts, leaving a dangling bond.
    let to_drop: Vec<u32> = result
        .iter_atoms()
        .filter(|(_, a)| a.is_patch_ghost())
        .map(|(id, _)| *id)
        .collect();
    for id in to_drop {
        result.delete_atom(id);
    }

    // Step 7 — Passivate the residual danglers.
    if passivate {
        add_hydrogens(&mut result, &AddHydrogensOptions::default());
    }

    let report = CompatibilityReport {
        placed_cells: selected_offsets.len(),
        welded_ghosts,
        orphaned_ghosts,
        overcoordinated_atoms,
    };

    if !debug_project && !debug_frontier {
        return Ok((result, report));
    }

    // ---- Debug visualizations (output only; the report above is preserved) ----
    let frontier_offsets = if debug_frontier {
        compute_frontier(&selected, origin, tiling_vectors)
    } else {
        Vec::new()
    };

    let output = if debug_project {
        // Footprint view: target atoms (unprojected) + the selected and frontier
        // tiles flattened onto the test plane; frontier tiles flagged frozen. No
        // cut, no weld — this shows exactly what the inclusion test sees.
        let mut out = target.clone();
        for o in &selected_offsets {
            place_debug_tile(
                &mut out,
                tile,
                o,
                region_lattice,
                true,
                &free_dirs,
                &center_depths,
                false,
            )?;
        }
        for o in &frontier_offsets {
            place_debug_tile(
                &mut out,
                tile,
                o,
                region_lattice,
                true,
                &free_dirs,
                &center_depths,
                true,
            )?;
        }
        out
    } else {
        // Frontier overlay: the real welded result plus the excluded neighbour
        // tiles placed raw and flagged frozen.
        let mut out = result;
        for o in &frontier_offsets {
            place_debug_tile(
                &mut out,
                tile,
                o,
                region_lattice,
                false,
                &free_dirs,
                &center_depths,
                true,
            )?;
        }
        out
    };

    Ok((output, report))
}
