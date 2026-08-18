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
//!
//! The extraction itself ([`extract_patch_tile`]) and the tiling-vector
//! validation are domain code and live in [`atomcad_crystolecule::patch`];
//! this file is the node wrapper around them.

use crate::data_type::{DataType, RecordType};
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::{BlueprintData, MoleculeData, NetworkResult};
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::patch::{
    DEFAULT_BUILD_THRESHOLD, extract_patch_tile, validate_tiling_vectors,
};
use glam::i32::IVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

        // Pin 1: lattice (HasStructure) — retained so the tiling vectors are
        // declared against a concrete lattice (commensurability intent); the
        // tile is kept in authored coordinates, so no reference lattice point is
        // derived here. Still evaluated to surface a clear error on mis-wiring.
        let lattice_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 1);
        if let NetworkResult::Error(_) = lattice_val {
            return EvalOutput::single(lattice_val);
        }
        if lattice_val.get_unit_cell().is_none() {
            return EvalOutput::single(NetworkResult::Error(
                "patch_build: lattice must be a Crystal or Blueprint providing lattice vectors"
                    .to_string(),
            ));
        }

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

        // Extract the tile in its authored coordinates.
        let tile = extract_patch_tile(&source_atoms, &cut_bp.geo_tree_root, self.epsilon);

        // Assemble the built-in `Patch` record. The tile and cut volume are both
        // kept as drawn, so applying with the default `origin` reproduces the
        // authored reconstruction in place.
        let tile_result = NetworkResult::Molecule(MoleculeData {
            atoms: tile,
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
            geo_tree_root: cut_bp.geo_tree_root,
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
            and cut volume kept in their authored coordinates. See \
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
