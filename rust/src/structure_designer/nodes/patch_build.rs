//! `patch_build` — the "draw, don't assemble" authoring step for surface
//! reconstruction patches (see `doc/design_surface_patches.md` §4).
//!
//! The user draws an ordinary big slab of the reconstructed surface on its bulk
//! plus **one tile's volume** as a `Blueprint`. `patch_build` extracts the tile
//! automatically: interior atoms (inside the cut volume) are kept as real tile
//! atoms; outside atoms bonded to the interior are copied as **patch-ghosts**
//! (the neighbour-tile / bulk-collar copies that weld at apply time). The
//! extracted atoms and the cut volume are re-expressed relative to a reference
//! lattice point `R` so the patch's local origin is a lattice point.
//!
//! The output is the built-in `Patch` record
//! `{ tile: Molecule, tiling_vectors: Array[IVec3], cut_volume: Blueprint }`.

use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::geo_tree::GeoNode;
use crate::geo_tree::implicit_geometry::ImplicitGeometry3D;
use crate::structure_designer::data_type::{DataType, RecordType};
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{
    BlueprintData, MoleculeData, NetworkResult,
};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::transform::Transform;
use glam::f64::{DQuat, DVec3};
use glam::i32::IVec3;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Default build threshold `ε` (Å). A slab atom counts as interior when its
/// `cut_volume` membership SDF ≤ `ε`. Must be large enough to catch atoms
/// authored right on the cut surface, but well below the nearest interplanar
/// spacing so it never grabs the layer below. See design §8, open question 1.
pub const DEFAULT_BUILD_THRESHOLD: f64 = 0.1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchBuildData {
    /// Build threshold `ε` for the interior/ghost split (real-space Å).
    #[serde(default = "default_epsilon")]
    pub epsilon: f64,
}

fn default_epsilon() -> f64 {
    DEFAULT_BUILD_THRESHOLD
}

impl Default for PatchBuildData {
    fn default() -> Self {
        Self {
            epsilon: DEFAULT_BUILD_THRESHOLD,
        }
    }
}

/// The result of extracting one tile from a slab + cut volume: the tile
/// (interior + patch-ghosts + their bonds, a plain `AtomicStructure`) and the
/// cut volume geometry, both re-expressed relative to the reference lattice
/// point `R`.
pub struct ExtractedTile {
    pub tile: AtomicStructure,
    pub cut_volume: GeoNode,
    /// The reference lattice point the tile/cut were re-expressed against.
    /// Exposed for testing/inspection; `R` itself is not stored in the patch.
    pub reference_lattice_point: IVec3,
}

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

/// Computes the reference lattice point `R`: the lattice cell at the cut's
/// reference (min) corner, approximated by the min corner of the realized
/// interior atom set (the atoms actually inside the cut). Any lattice point is
/// correct — `R` only fixes a phase the user later shifts with
/// `patch_latticefill.origin` — and flooring to a cell keeps `R` a lattice
/// point, so each atom's fractional motif offset survives the shift (phase
/// preserved). Returns the origin cell when there are no interior atoms.
fn compute_reference_lattice_point(
    interior_positions: &[DVec3],
    lattice: &UnitCellStruct,
) -> IVec3 {
    if interior_positions.is_empty() {
        return IVec3::ZERO;
    }
    let mut min_real = interior_positions[0];
    for p in &interior_positions[1..] {
        min_real = min_real.min(*p);
    }
    let min_lattice = lattice.real_to_dvec3_lattice(&min_real);
    IVec3::new(
        min_lattice.x.floor() as i32,
        min_lattice.y.floor() as i32,
        min_lattice.z.floor() as i32,
    )
}

/// Extracts the tile from `source` given the `cut_volume` geometry (a real-space
/// SDF) and the build threshold `epsilon`, then re-expresses the atoms and the
/// cut volume relative to the reference lattice point `R` (§4 "Extraction" /
/// "Coordinate frame").
///
/// This is the node-free core so the extraction logic is testable without the
/// node-network machinery.
pub fn extract_patch_tile(
    source: &AtomicStructure,
    lattice: &UnitCellStruct,
    cut_volume: &GeoNode,
    epsilon: f64,
) -> ExtractedTile {
    // 1. Interior `I` = slab atoms inside the cut volume (membership SDF ≤ ε).
    let mut interior: HashSet<u32> = HashSet::new();
    let mut interior_positions: Vec<DVec3> = Vec::new();
    for (id, atom) in source.iter_atoms() {
        if cut_volume.implicit_eval_3d(&atom.position) <= epsilon {
            interior.insert(*id);
            interior_positions.push(atom.position);
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

    // 5. Re-express atoms and cut volume relative to R (a lattice point).
    let reference = compute_reference_lattice_point(&interior_positions, lattice);
    let r_real = lattice.ivec3_lattice_to_real(&reference);
    tile.transform(&DQuat::IDENTITY, &(-r_real));
    let cut_volume_rel = GeoNode::transform(
        Transform::new(-r_real, DQuat::IDENTITY),
        Box::new(cut_volume.clone()),
    );

    ExtractedTile {
        tile,
        cut_volume: cut_volume_rel,
        reference_lattice_point: reference,
    }
}

impl NodeData for PatchBuildData {
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
        // Pin 0: source slab (HasAtoms). Only its atoms are read.
        let source_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if let NetworkResult::Error(_) = source_val {
            return EvalOutput::single(source_val);
        }
        let source_atoms = match source_val.extract_atomic() {
            Some(atoms) => atoms,
            None => {
                return EvalOutput::single(NetworkResult::Error(
                    "patch_build: source must be a Crystal or Molecule".to_string(),
                ));
            }
        };

        // Pin 1: lattice (HasStructure) — supplies the lattice vectors used to
        // interpret tiling vectors and to derive the reference lattice point.
        let lattice_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 1);
        if let NetworkResult::Error(_) = lattice_val {
            return EvalOutput::single(lattice_val);
        }
        let lattice_vecs = match lattice_val.get_unit_cell() {
            Some(uc) => uc,
            None => {
                return EvalOutput::single(NetworkResult::Error(
                    "patch_build: lattice must be a Crystal or Blueprint providing lattice vectors"
                        .to_string(),
                ));
            }
        };

        // Pin 2: tiling_vectors (Array[IVec3]).
        let tiling_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 2);
        let tiling_vectors: Vec<IVec3> = match tiling_val {
            NetworkResult::Error(_) => return EvalOutput::single(tiling_val),
            NetworkResult::Array(elements) => {
                let mut vs = Vec::with_capacity(elements.len());
                for element in elements {
                    match element {
                        NetworkResult::IVec3(v) => vs.push(v),
                        NetworkResult::Error(_) => return EvalOutput::single(element),
                        other => {
                            return EvalOutput::single(NetworkResult::Error(format!(
                                "patch_build: tiling_vectors must be Array[IVec3], found element {}",
                                other.to_display_string()
                            )));
                        }
                    }
                }
                vs
            }
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "patch_build: tiling_vectors must be an Array[IVec3], got {}",
                    other.to_display_string()
                )));
            }
        };
        if let Err(msg) = validate_tiling_vectors(&tiling_vectors) {
            return EvalOutput::single(NetworkResult::Error(msg));
        }

        // Pin 3: cut_volume (Blueprint). Defines the interior at build time and
        // is stored in the patch to drive removal at apply time.
        let cut_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 3);
        let cut_bp = match cut_val {
            NetworkResult::Error(_) => return EvalOutput::single(cut_val),
            NetworkResult::Blueprint(bp) => bp,
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "patch_build: cut_volume must be a Blueprint, got {}",
                    other.to_display_string()
                )));
            }
        };

        // Extract the tile and re-express it relative to R.
        let extracted = extract_patch_tile(
            &source_atoms,
            &lattice_vecs,
            &cut_bp.geo_tree_root,
            self.epsilon,
        );

        // Assemble the built-in `Patch` record.
        let tile_result = NetworkResult::Molecule(MoleculeData {
            atoms: extracted.tile,
            geo_tree_root: None,
        });
        let tiling_result = NetworkResult::Array(
            tiling_vectors
                .into_iter()
                .map(NetworkResult::IVec3)
                .collect(),
        );
        let cut_result = NetworkResult::Blueprint(BlueprintData {
            structure: cut_bp.structure,
            geo_tree_root: extracted.cut_volume,
            alignment: cut_bp.alignment,
            alignment_reason: cut_bp.alignment_reason,
        });

        EvalOutput::single(NetworkResult::record(vec![
            ("tile".to_string(), tile_result),
            ("tiling_vectors".to_string(), tiling_result),
            ("cut_volume".to_string(), cut_result),
        ]))
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
        vec![("epsilon".to_string(), TextValue::Float(self.epsilon))]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("epsilon") {
            self.epsilon = v
                .as_float()
                .ok_or_else(|| "epsilon must be a float".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("source".to_string(), (true, None));
        m.insert("lattice".to_string(), (true, None));
        m.insert("tiling_vectors".to_string(), (true, None));
        m.insert("cut_volume".to_string(), (true, None));
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "patch_build".to_string(),
        description:
            "Extracts a tileable surface-reconstruction patch from an authored slab and a cut \
            volume. Interior atoms (inside the cut volume) become real tile atoms; outside atoms \
            bonded to the interior are copied as patch-ghosts that weld onto neighbour tiles / \
            the bulk at apply time. Outputs the built-in Patch record \
            {tile: Molecule, tiling_vectors: Array[IVec3], cut_volume: Blueprint}, with the tile \
            and cut volume re-expressed relative to a reference lattice point. See \
            doc/design_surface_patches.md §4."
                .to_string(),
        summary: Some("Extract a tileable surface patch".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "source".to_string(),
                data_type: DataType::HasAtoms,
            },
            Parameter {
                id: None,
                name: "lattice".to_string(),
                data_type: DataType::HasStructure,
            },
            Parameter {
                id: None,
                name: "tiling_vectors".to_string(),
                data_type: DataType::Array(Box::new(DataType::IVec3)),
            },
            Parameter {
                id: None,
                name: "cut_volume".to_string(),
                data_type: DataType::Blueprint,
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Record(RecordType::Named(
            "Patch".to_string(),
        ))),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(PatchBuildData::default()),
        node_data_saver: generic_node_data_saver::<PatchBuildData>,
        node_data_loader: generic_node_data_loader::<PatchBuildData>,
    }
}
