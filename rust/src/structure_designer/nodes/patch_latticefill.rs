//! `patch_latticefill` — applies a surface-reconstruction patch over a region
//! (see `doc/design_surface_patches.md` §5).
//!
//! Tiles a patch's tile across the cells that fit in the fill region, cuts the
//! displaced substrate, welds the placed copies to each other (periodic bonds)
//! and to the surrounding bulk (collar), drops the patch-ghosts that found no
//! real twin (true reconstruction edges), and hydrogen-passivates the residual
//! danglers. The same `weld_coincident_atoms` primitive realizes both the
//! tile↔tile and tile↔bulk interfaces in one pass.
//!
//! The core `apply_patch` is a plain function (node-free testable). It also
//! returns a [`CompatibilityReport`] — the welded-vs-orphaned collar counts and
//! a post-weld over-coordination count (§6), the data behind a future
//! compatibility badge.

use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure::{AtomicStructure, TagError};
use crate::crystolecule::guided_placement::{Hybridization, covalent_max_neighbors};
use crate::crystolecule::hydrogen_passivation::{AddHydrogensOptions, add_hydrogens};
use crate::crystolecule::structure::Structure;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::crystolecule::weld::weld_coincident_atoms;
use crate::geo_tree::GeoNode;
use crate::geo_tree::implicit_geometry::ImplicitGeometry3D;
use crate::structure_designer::common_constants::{
    REAL_IMPLICIT_VOLUME_MAX, REAL_IMPLICIT_VOLUME_MIN,
};
use crate::structure_designer::data_type::{DataType, RecordType};
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{Alignment, CrystalData, NetworkResult};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::daabox::DAABox;
use glam::f64::{DQuat, DVec3};
use glam::i32::IVec3;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchLatticeFillData {
    /// Hydrogen-passivate the residual danglers after welding (default true).
    #[serde(default = "default_true")]
    pub passivate: bool,
    /// Weld tolerance in Å (default 0.1).
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
    /// Cell-selection test height. When `false` (**default**), derive the height
    /// from the **target** slab's own extent — robust to a target offset from the
    /// lattice origin along the normal (the usual case: surfaces are authored at
    /// the height where they sit). When `true`, project onto the periodic
    /// subspace through the **lattice origin** (height 0) — simpler and
    /// predictable, but selects nothing when the target does not straddle the
    /// origin. See `doc/design_patch_cell_selection.md`.
    #[serde(default)]
    pub test_height_at_origin: bool,
    /// Debug: place the patch atoms at their projected positions on the test
    /// plane (in-plane kept, normal = centre depth), with no cut/weld — shows
    /// exactly what cell selection tests. Non-physical; default false.
    #[serde(default)]
    pub debug_project_to_test_plane: bool,
    /// Debug: also place the one-cell-wider frontier of tiles (Cartesian product
    /// of the selected index ranges ±1), flagging the not-selected ones frozen,
    /// so the excluded neighbours are visible. Default false.
    #[serde(default)]
    pub debug_show_frontier_tiles: bool,
    /// Compatibility stats from the most recent successful evaluation (§6),
    /// surfaced to the property panel as a compatibility badge. Interior
    /// mutability because `eval` takes `&self`; transient (not serialized) and
    /// repopulated on the next evaluation. `None` until the node has evaluated.
    #[serde(skip)]
    pub last_report: RefCell<Option<CompatibilityReport>>,
}

fn default_true() -> bool {
    true
}

fn default_tolerance() -> f64 {
    DEFAULT_WELD_TOLERANCE
}

impl Default for PatchLatticeFillData {
    fn default() -> Self {
        Self {
            passivate: true,
            tolerance: DEFAULT_WELD_TOLERANCE,
            test_height_at_origin: false,
            debug_project_to_test_plane: false,
            debug_show_frontier_tiles: false,
            last_report: RefCell::new(None),
        }
    }
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
fn atom_aabb(structure: &AtomicStructure) -> Option<DAABox> {
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

// ============================================================================
// Node wrapper
// ============================================================================

/// Pulls the three fields out of a `Patch` record value.
struct PatchFields {
    tile: AtomicStructure,
    tiling_vectors: Vec<IVec3>,
    cut_volume: GeoNode,
}

fn read_patch_record(patch: &NetworkResult) -> Result<PatchFields, String> {
    let tile = match patch.extract_record_field("tile") {
        Some(NetworkResult::Molecule(m)) => m.atoms.clone(),
        Some(NetworkResult::Crystal(c)) => c.atoms.clone(),
        _ => return Err("patch_latticefill: patch.tile must be a Molecule".to_string()),
    };
    let tiling_vectors = match patch.extract_record_field("tiling_vectors") {
        Some(NetworkResult::Array(elements)) => {
            let mut vs = Vec::with_capacity(elements.len());
            for element in elements {
                match element {
                    NetworkResult::IVec3(v) => vs.push(*v),
                    _ => {
                        return Err(
                            "patch_latticefill: patch.tiling_vectors must be Array[IVec3]"
                                .to_string(),
                        );
                    }
                }
            }
            vs
        }
        _ => {
            return Err(
                "patch_latticefill: patch.tiling_vectors must be an Array[IVec3]".to_string(),
            );
        }
    };
    let cut_volume = match patch.extract_record_field("cut_volume") {
        Some(NetworkResult::Blueprint(bp)) => bp.geo_tree_root.clone(),
        _ => return Err("patch_latticefill: patch.cut_volume must be a Blueprint".to_string()),
    };
    Ok(PatchFields {
        tile,
        tiling_vectors,
        cut_volume,
    })
}

/// Extracts `(structure, geo, alignment, alignment_reason)` from a region-like
/// result (Blueprint or Crystal). Returns `None` for other variants.
fn region_structure(
    value: &NetworkResult,
) -> Option<(Structure, Option<GeoNode>, Alignment, Option<String>)> {
    match value {
        NetworkResult::Blueprint(bp) => Some((
            bp.structure.clone(),
            Some(bp.geo_tree_root.clone()),
            bp.alignment,
            bp.alignment_reason.clone(),
        )),
        NetworkResult::Crystal(c) => Some((
            c.structure.clone(),
            c.geo_tree_root.clone(),
            c.alignment,
            c.alignment_reason.clone(),
        )),
        _ => None,
    }
}

impl NodeData for PatchLatticeFillData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        // Clear the cached compatibility stats; only a successful apply below
        // repopulates them, so an error path leaves the badge hidden rather than
        // showing stats from a previous, now-invalid input.
        *self.last_report.borrow_mut() = None;

        // Pin 0: target (HasAtoms) — the structure being reconstructed.
        let target_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if let NetworkResult::Error(_) = target_val {
            return EvalOutput::single(target_val);
        }
        let target_atoms = match target_val.clone().extract_atomic() {
            Some(atoms) => atoms,
            None => {
                return EvalOutput::single(NetworkResult::Error(
                    "patch_latticefill: target must be a Crystal or Molecule".to_string(),
                ));
            }
        };

        // Pin 1: region (HasStructure, optional). Defaults to `target` (which
        // must then be a Crystal so it carries a structure).
        let region_val =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
        if let NetworkResult::Error(_) = region_val {
            return EvalOutput::single(region_val);
        }
        let region_source = if matches!(region_val, NetworkResult::None) {
            &target_val
        } else {
            &region_val
        };
        let (out_structure, region_geo, alignment, alignment_reason) =
            match region_structure(region_source) {
                Some(parts) => parts,
                None => {
                    return EvalOutput::single(NetworkResult::Error(
                        "patch_latticefill: region must be a Crystal or Blueprint (or connect a \
                         Crystal target so its structure can be used)"
                            .to_string(),
                    ));
                }
            };
        let region_lattice = out_structure.lattice_vecs.clone();

        // Pin 2: patch (the built-in Patch record).
        let patch_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 2);
        if let NetworkResult::Error(_) = patch_val {
            return EvalOutput::single(patch_val);
        }
        let patch = match read_patch_record(&patch_val) {
            Ok(p) => p,
            Err(msg) => return EvalOutput::single(NetworkResult::Error(msg)),
        };

        // Bound the integer cell search by the region's geometry extent when it
        // is known (a Blueprint always has one); otherwise the target atoms'
        // extent, expanded by a margin so boundary cells are considered.
        let margin = region_lattice.cell_length_a
            + region_lattice.cell_length_b
            + region_lattice.cell_length_c;
        let region_bounds = atom_aabb(&target_atoms)
            .map(|b| b.expand(margin))
            .unwrap_or_else(|| DAABox::new(REAL_IMPLICIT_VOLUME_MIN, REAL_IMPLICIT_VOLUME_MAX));

        // Pin 3: origin (IVec3, optional). A whole-cell offset applied to the
        // entire reconstruction; the default (0,0,0) places it exactly where it
        // was authored (same lattice registration).
        let origin =
            match network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 3) {
                NetworkResult::Error(e) => return EvalOutput::single(NetworkResult::Error(e)),
                NetworkResult::IVec3(v) => v,
                NetworkResult::None => IVec3::ZERO,
                other => {
                    return EvalOutput::single(NetworkResult::Error(format!(
                        "patch_latticefill: origin must be an IVec3, got {}",
                        other.to_display_string()
                    )));
                }
            };

        // Pin 4: passivate (Bool, optional, default from stored property).
        let passivate = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            4,
            self.passivate,
            NetworkResult::extract_bool,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // Pin 5: tolerance (Float, optional, default from stored property).
        let tolerance = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            5,
            self.tolerance,
            NetworkResult::extract_float,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let (atoms, report) = match apply_patch(
            &target_atoms,
            &region_lattice,
            region_geo.as_ref(),
            &region_bounds,
            &patch.tile,
            &patch.tiling_vectors,
            &patch.cut_volume,
            origin,
            passivate,
            tolerance,
            self.test_height_at_origin,
            self.debug_project_to_test_plane,
            self.debug_show_frontier_tiles,
        ) {
            Ok(v) => v,
            Err(e) => return EvalOutput::single(NetworkResult::Error(e.to_string())),
        };

        // Cache the compatibility stats for the property-panel badge (§6).
        *self.last_report.borrow_mut() = Some(report);

        EvalOutput::single(NetworkResult::Crystal(CrystalData {
            structure: out_structure,
            atoms,
            geo_tree_root: region_geo,
            alignment,
            alignment_reason,
        }))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        None
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("passivate".to_string(), TextValue::Bool(self.passivate)),
            ("tolerance".to_string(), TextValue::Float(self.tolerance)),
            (
                "test_height_at_origin".to_string(),
                TextValue::Bool(self.test_height_at_origin),
            ),
            (
                "debug_project_to_test_plane".to_string(),
                TextValue::Bool(self.debug_project_to_test_plane),
            ),
            (
                "debug_show_frontier_tiles".to_string(),
                TextValue::Bool(self.debug_show_frontier_tiles),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("passivate") {
            self.passivate = v
                .as_bool()
                .ok_or_else(|| "passivate must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("tolerance") {
            self.tolerance = v
                .as_float()
                .ok_or_else(|| "tolerance must be a float".to_string())?;
        }
        if let Some(v) = props.get("test_height_at_origin") {
            self.test_height_at_origin = v
                .as_bool()
                .ok_or_else(|| "test_height_at_origin must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("debug_project_to_test_plane") {
            self.debug_project_to_test_plane = v
                .as_bool()
                .ok_or_else(|| "debug_project_to_test_plane must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("debug_show_frontier_tiles") {
            self.debug_show_frontier_tiles = v
                .as_bool()
                .ok_or_else(|| "debug_show_frontier_tiles must be a boolean".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("target".to_string(), (true, None));
        m.insert("region".to_string(), (false, None));
        m.insert("patch".to_string(), (true, None));
        m.insert("origin".to_string(), (false, None));
        m.insert("passivate".to_string(), (false, None));
        m.insert("tolerance".to_string(), (false, None));
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "patch_latticefill".to_string(),
        description:
            "Tiles a surface-reconstruction patch across a region and welds it in. Cuts the \
            displaced substrate, places a copy of the patch tile at each commensurate cell, welds \
            coincident atoms (realizing both periodic tile↔tile bonds and tile↔bulk collar bonds), \
            drops the patch-ghosts left at true edges, and hydrogen-passivates the residual \
            danglers. Outputs the reconstructed Crystal. See doc/design_surface_patches.md §5."
                .to_string(),
        summary: Some("Tile and weld a surface patch".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "target".to_string(),
                data_type: DataType::HasAtoms,
            },
            Parameter {
                id: None,
                name: "region".to_string(),
                data_type: DataType::HasStructure,
            },
            Parameter {
                id: None,
                name: "patch".to_string(),
                data_type: DataType::Record(RecordType::Named("Patch".to_string())),
            },
            Parameter {
                id: None,
                name: "origin".to_string(),
                data_type: DataType::IVec3,
            },
            Parameter {
                id: None,
                name: "passivate".to_string(),
                data_type: DataType::Bool,
            },
            Parameter {
                id: None,
                name: "tolerance".to_string(),
                data_type: DataType::Float,
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Crystal),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(PatchLatticeFillData::default()),
        node_data_saver: generic_node_data_saver::<PatchLatticeFillData>,
        node_data_loader: generic_node_data_loader::<PatchLatticeFillData>,
    }
}
