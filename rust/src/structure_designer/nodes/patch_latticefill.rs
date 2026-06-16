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
use crate::crystolecule::atomic_structure::AtomicStructure;
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

/// The 2^n corners of the cell parallelepiped at anchor `t` spanned by the real
/// tiling vectors (the tile's periodic footprint — §5).
fn footprint_corners(t: DVec3, periodic_real: &[DVec3]) -> Vec<DVec3> {
    let n = periodic_real.len();
    let mut corners = Vec::with_capacity(1usize << n);
    for mask in 0u32..(1u32 << n) {
        let mut p = t;
        for (i, rv) in periodic_real.iter().enumerate() {
            if mask & (1 << i) != 0 {
                p += *rv;
            }
        }
        corners.push(p);
    }
    corners
}

/// Tests whether a footprint corner is inside the region's *shadow* — its
/// projection onto the periodic subspace lies in the region, free along the
/// non-periodic direction(s). Implemented by sliding the corner along the free
/// directions to the region's centre plane (exact for convex prismatic regions)
/// and testing the region there (§5, "projected containment").
fn corner_in_region_shadow(
    corner: DVec3,
    free_dirs: &[DVec3],
    region_center: DVec3,
    region_volume: Option<&GeoNode>,
    region_bounds: &DAABox,
) -> bool {
    let mut sample = corner;
    for d in free_dirs {
        // Replace `sample`'s component along `d` with `region_center`'s.
        sample += *d * (region_center - sample).dot(*d);
    }
    match region_volume {
        Some(geo) => geo.implicit_eval_3d(&sample) <= 0.0,
        None => region_bounds.contains_point(sample),
    }
}

/// Selects the absolute cell vectors `c = origin + Σ kᵢ·vᵢ` that receive a tile:
/// those whose footprint, projected onto the periodic subspace, lies fully
/// inside the region (whole-cell containment in the periodic directions, free
/// along the non-periodic ones — §5). `region_bounds` bounds the integer search;
/// `region_volume` (when present) is the actual containment gate.
///
/// Public for node-free testing of the containment rule (§9 Phase 3 test 5).
pub fn select_patch_cells(
    origin: IVec3,
    tiling_vectors: &[IVec3],
    region_lattice: &UnitCellStruct,
    region_volume: Option<&GeoNode>,
    region_bounds: &DAABox,
) -> Vec<IVec3> {
    let periodic_real: Vec<DVec3> = tiling_vectors
        .iter()
        .map(|v| region_lattice.ivec3_lattice_to_real(v))
        .collect();
    let free_dirs = free_directions(&periodic_real);
    let region_center = region_bounds.center();
    let diag = region_bounds.size().length();

    // Bound |kᵢ| by how many tiling steps could possibly land inside the search
    // box, plus a margin cell.
    let step_bounds: Vec<i32> = periodic_real
        .iter()
        .map(|rv| {
            let len = rv.length();
            if len < 1e-9 {
                0
            } else {
                (diag / len).ceil() as i32 + 1
            }
        })
        .collect();

    let mut cells = Vec::new();
    for k in iter_step_tuples(&step_bounds) {
        let mut c = origin;
        for (ki, v) in k.iter().zip(tiling_vectors.iter()) {
            c += *v * *ki;
        }
        let t = region_lattice.ivec3_lattice_to_real(&c);
        let corners = footprint_corners(t, &periodic_real);
        let inside = corners.iter().all(|corner| {
            corner_in_region_shadow(
                *corner,
                &free_dirs,
                region_center,
                region_volume,
                region_bounds,
            )
        });
        if inside {
            cells.push(c);
        }
    }
    cells
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

/// Applies a patch over a region (§5). `region_volume` is the containment SDF
/// (`None` → fall back to `region_bounds`); `region_bounds` bounds the integer
/// cell search. Returns the reconstructed atoms plus a [`CompatibilityReport`].
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
) -> (AtomicStructure, CompatibilityReport) {
    let cells = select_patch_cells(
        origin,
        tiling_vectors,
        region_lattice,
        region_volume,
        region_bounds,
    );

    let mut result = target.clone();

    let ghosts_per_tile = tile.atoms_values().filter(|a| a.is_patch_ghost()).count();
    let total_placed_ghosts = ghosts_per_tile * cells.len();

    // Step 3 — Cut: remove substrate atoms inside the translated cut_volume.
    // Runs before any tile is placed, so only substrate atoms are removed. The
    // cut volume translated to cell `c` evaluates as `cut_volume(p - t)`.
    for c in &cells {
        let t = region_lattice.ivec3_lattice_to_real(c);
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

    // Step 4 — Place: add a copy of the tile translated to each cell. The
    // patch-ghost flag rides along (`add_atomic_structure` copies all flags).
    for c in &cells {
        let t = region_lattice.ivec3_lattice_to_real(c);
        let mut copy = tile.clone();
        copy.transform(&DQuat::IDENTITY, &t);
        result.add_atomic_structure(&copy);
    }

    // Step 5 — Weld: fuse tile↔tile (periodic) and tile↔bulk (collar) at once.
    // A weld touching any real atom yields a real survivor (the ghost flag is
    // cleared); a cluster of only patch-ghosts stays a patch-ghost.
    weld_coincident_atoms(&mut result, tolerance);

    // §6 stats: any atom still flagged patch-ghost found no real twin.
    let orphaned_ghosts = result.atoms_values().filter(|a| a.is_patch_ghost()).count();
    let welded_ghosts = total_placed_ghosts.saturating_sub(orphaned_ghosts);
    let overcoordinated_atoms = count_overcoordinated(&result);

    // Step 6 — Drop unwelded patch-ghosts (true reconstruction edges / collars
    // with no substrate partner), leaving a dangling bond on the boundary atom.
    let to_drop: Vec<u32> = result
        .iter_atoms()
        .filter(|(_, a)| a.is_patch_ghost())
        .map(|(id, _)| *id)
        .collect();
    for id in to_drop {
        result.delete_atom(id);
    }

    // Step 7 — Passivate the residual danglers (reuses the general H passivation).
    if passivate {
        add_hydrogens(&mut result, &AddHydrogensOptions::default());
    }

    let report = CompatibilityReport {
        welded_ghosts,
        orphaned_ghosts,
        overcoordinated_atoms,
    };
    (result, report)
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

        // Pin 3: origin (IVec3, optional). Defaults to the lattice point nearest
        // the region's centre.
        let origin =
            match network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 3) {
                NetworkResult::Error(e) => return EvalOutput::single(NetworkResult::Error(e)),
                NetworkResult::IVec3(v) => v,
                NetworkResult::None => {
                    region_lattice.real_to_ivec3_lattice(&region_bounds.center())
                }
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

        let (atoms, report) = apply_patch(
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
        );

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
