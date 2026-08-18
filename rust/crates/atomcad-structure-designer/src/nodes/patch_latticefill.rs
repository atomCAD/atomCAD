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
//! The core `apply_patch` is a plain function living in
//! [`atomcad_crystolecule::patch`] (node-free testable). It also
//! returns a [`CompatibilityReport`] — the welded-vs-orphaned collar counts and
//! a post-weld over-coordination count (§6), the data behind a future
//! compatibility badge. The report (now including `placed_cells`, where 0 means
//! nothing tiled and surfaces as a red "No tiles placed" badge) is cached in a
//! `#[serde(skip)] RefCell<Option<_>>` on `PatchLatticeFillData` — interior
//! mutability, because `eval` takes `&self`; the same pattern as
//! `MaterializeData::available_parameters`.
//!
//! # Coordinate frame (`doc/design_patch_cell_selection.md`)
//!
//! `extract_patch_tile` keeps the tile in **authored absolute coordinates**,
//! with no hidden re-anchoring. `origin` here is a whole-cell **offset**
//! (default `(0,0,0)` = as-drawn), so build → apply-to-the-same-crystal is the
//! identity. Every placement is a whole-lattice-vector translation, so the
//! `origin` pin's offset reaches atoms as `p + lattice·(origin + Σ kᵢ·vᵢ)`
//! (target-mapped) before any test runs.
//!
//! # Cell selection (`select_patch_cells`)
//!
//! Tests the tile's **interior (non-ghost) atoms**: placed, then projected onto
//! the test plane (in-plane components kept, normal component → `center_depth`),
//! and **all** must be inside the region. There is no rhombus and no synthetic
//! anchor — this replaced `tile_reference_anchor` / `footprint_corners` /
//! `corner_in_region_shadow`, keeping only `free_directions`. An empty-interior
//! guard returns no cells.
//!
//! `center_depth` is chosen per free direction by the bool
//! `test_height_at_origin`: **false** (the default) uses the target-derived
//! `region_center_depths` midpoint, robust to an off-origin or thin slab;
//! `true` uses the lattice origin (0), which is simpler but selects nothing when
//! the target doesn't straddle the origin.
//!
//! # Debug flags
//!
//! Two bools, both default false. `debug_project_to_test_plane` flattens placed
//! atoms onto the test plane and skips the weld. `debug_show_frontier_tiles`
//! places the ±1 Cartesian box of cells and flags the not-selected ones
//! **frozen** (an empty selection falls back to the −1..+1 block around origin).
//! The report is always computed from the real selected-cell weld.
//!
//! **After editing `APIPatchLatticeFillData`, re-run
//! `flutter_rust_bridge_codegen generate`** — `frb_generated.rs` constructs it.

use crate::common_constants::{REAL_IMPLICIT_VOLUME_MAX, REAL_IMPLICIT_VOLUME_MIN};
use crate::data_type::{DataType, RecordType};
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::{Alignment, CrystalData, NetworkResult};
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::patch::{
    CompatibilityReport, DEFAULT_WELD_TOLERANCE, apply_patch, atom_aabb,
};
use atomcad_crystolecule::structure::Structure;
use atomcad_geo_tree::GeoNode;
use atomcad_util::daabox::DAABox;
use glam::i32::IVec3;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;

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
