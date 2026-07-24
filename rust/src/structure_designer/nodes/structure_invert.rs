use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_structure_diff::extract_diff;
use crate::crystolecule::motif_symmetry::inversion_preserves_motif;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::display::gadget::{Gadget, GadgetPickContext};
use crate::geo_tree::GeoNode;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput};
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator,
};
use crate::structure_designer::evaluator::network_result::{
    Alignment, BlueprintData, CrystalData, MoleculeData, NetworkResult,
    runtime_type_error_in_input, worsen_alignment_with_reason,
};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::serialization_utils::ivec3_serializer;
use glam::f64::DVec3;
use glam::i32::IVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct StructureInvertEvalCache {
    pub unit_cell: UnitCellStruct,
    pub pivot_point: IVec3,
    pub subdivision: i32,
}

/// Wraps an extracted (or empty) diff as the `Molecule` value for the node's
/// `diff` output pin (issue #295, `doc/design_diff_outputs_for_atom_ops.md` §2).
/// A `Blueprint` input has no atoms, so it yields an empty diff (§2.3).
fn diff_pin(mut diff: AtomicStructure) -> NetworkResult {
    diff.decorator_mut().show_anchor_arrows = true;
    NetworkResult::Molecule(MoleculeData {
        atoms: diff,
        geo_tree_root: None,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureInvertData {
    #[serde(with = "ivec3_serializer")]
    pub pivot_point: IVec3,
    /// The pivot is `pivot_point / subdivision` in lattice coordinates, so
    /// half-lattice and bond-center pivots are expressible (diamond's
    /// inversion centers sit at bond midpoints: pivot (1,1,1), subdivision 8).
    #[serde(default = "default_subdivision")]
    pub subdivision: i32,
}

fn default_subdivision() -> i32 {
    1
}

impl NodeData for StructureInvertData {
    fn provide_gadget(
        &self,
        structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let cache = eval_cache.downcast_ref::<StructureInvertEvalCache>()?;

        let gadget =
            StructureInvertGadget::new(cache.pivot_point, cache.subdivision, &cache.unit_cell);
        Some(Box::new(gadget))
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
        let input_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

        if let NetworkResult::Error(_) = input_val {
            // Propagate the error on both pins (result + diff) so diff consumers
            // don't silently see `None` on pin 1 (§2).
            return EvalOutput::multi(vec![input_val.clone(), input_val]);
        }

        let pivot_point = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.pivot_point,
            NetworkResult::extract_ivec3,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::multi(vec![error.clone(), error]),
        };

        let subdivision = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            self.subdivision,
            NetworkResult::extract_int,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::multi(vec![error.clone(), error]),
        }
        .max(1);

        let structure_ref = match &input_val {
            NetworkResult::Blueprint(bp) => &bp.structure,
            NetworkResult::Crystal(c) => &c.structure,
            _ => {
                let err = runtime_type_error_in_input(0);
                return EvalOutput::multi(vec![err.clone(), err]);
            }
        };
        let unit_cell = structure_ref.lattice_vecs.clone();

        let pivot_frac = pivot_point.as_dvec3() / subdivision as f64;
        let pivot_real = unit_cell.dvec3_lattice_to_real(&pivot_frac);

        // Alignment hierarchy. The full crystal (lattice ⊗ motif) maps onto
        // itself iff the motif check passes — that is the "Aligned" criterion,
        // and it can hold even when 2·pivot is NOT a lattice vector (diamond's
        // bond-center pivot: 2·(1,1,1)/8 = (1,1,1)/4, yet CORNER ↔ INTERIOR
        // sublattices swap onto each other). When the motif is broken, the
        // fallback distinction is whether the translation part 2·pivot at
        // least maps the Bravais lattice onto itself (the linear part −I is in
        // every Bravais point group): on-lattice → MotifUnaligned (same level
        // as a non-motif structure_rot), off-lattice → LatticeUnaligned (same
        // level as a fractional structure_move).
        let lattice_preserved = (2 * pivot_point.x).rem_euclid(subdivision) == 0
            && (2 * pivot_point.y).rem_euclid(subdivision) == 0
            && (2 * pivot_point.z).rem_euclid(subdivision) == 0;

        let motif_preserved = inversion_preserves_motif(structure_ref, pivot_point, subdivision);

        let worsen = |alignment: &mut Alignment, alignment_reason: &mut Option<String>| {
            if motif_preserved {
                return;
            }
            if lattice_preserved {
                worsen_alignment_with_reason(
                    alignment,
                    alignment_reason,
                    Alignment::MotifUnaligned,
                    || {
                        "structure_invert through a pivot that is not a motif inversion center"
                            .to_string()
                    },
                );
            } else {
                worsen_alignment_with_reason(
                    alignment,
                    alignment_reason,
                    Alignment::LatticeUnaligned,
                    || {
                        format!(
                            "structure_invert through pivot ({}, {}, {})/{} (neither a motif inversion center nor a half-lattice point)",
                            pivot_point.x, pivot_point.y, pivot_point.z, subdivision
                        )
                    },
                );
            }
        };

        if network_stack.len() == 1 {
            context.selected_node_eval_cache = Some(Box::new(StructureInvertEvalCache {
                unit_cell: unit_cell.clone(),
                pivot_point,
                subdivision,
            }));
        }

        match input_val {
            NetworkResult::Blueprint(shape) => {
                let mut alignment = shape.alignment;
                let mut alignment_reason = shape.alignment_reason;
                worsen(&mut alignment, &mut alignment_reason);
                EvalOutput::multi(vec![
                    NetworkResult::Blueprint(BlueprintData {
                        structure: shape.structure.clone(),
                        geo_tree_root: GeoNode::point_invert(
                            pivot_real,
                            Box::new(shape.geo_tree_root),
                        ),
                        alignment,
                        alignment_reason,
                    }),
                    // Blueprint has no atoms → empty diff (§2.3).
                    diff_pin(AtomicStructure::new_diff()),
                ])
            }
            NetworkResult::Crystal(crystal) => {
                let mut atoms = crystal.atoms;
                // Snapshot before the in-place transform; atom ids are stable, so
                // the diff is an exact id-keyed comparison (§1.5).
                let before = atoms.clone();
                atoms.invert_through(pivot_real);
                let diff = extract_diff(&before, &atoms, 0.0);

                let new_geo_tree_root = crystal
                    .geo_tree_root
                    .map(|gt| GeoNode::point_invert(pivot_real, Box::new(gt)));

                let mut alignment = crystal.alignment;
                let mut alignment_reason = crystal.alignment_reason;
                worsen(&mut alignment, &mut alignment_reason);
                EvalOutput::multi(vec![
                    NetworkResult::Crystal(CrystalData {
                        structure: crystal.structure,
                        atoms,
                        geo_tree_root: new_geo_tree_root,
                        alignment,
                        alignment_reason,
                    }),
                    diff_pin(diff),
                ])
            }
            _ => {
                let err = runtime_type_error_in_input(0);
                EvalOutput::multi(vec![err.clone(), err])
            }
        }
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        let show_pivot = !connected_input_pins.contains("pivot_point");
        let show_subdivision =
            !connected_input_pins.contains("subdivision") && self.subdivision != 1;

        let mut parts = Vec::new();
        if show_pivot {
            parts.push(format!(
                "pivot: ({},{},{})",
                self.pivot_point.x, self.pivot_point.y, self.pivot_point.z
            ));
        }
        if show_subdivision {
            parts.push(format!("sub: {}", self.subdivision));
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "pivot_point".to_string(),
                TextValue::IVec3(self.pivot_point),
            ),
            ("subdivision".to_string(), TextValue::Int(self.subdivision)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("pivot_point") {
            self.pivot_point = v
                .as_ivec3()
                .ok_or_else(|| "pivot_point must be an IVec3".to_string())?;
        }
        if let Some(v) = props.get("subdivision") {
            self.subdivision = v
                .as_int()
                .ok_or_else(|| "subdivision must be an integer".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("input".to_string(), (true, None));
        m
    }
}

/// Display-only gadget: renders the inversion pivot as a red sphere (the same
/// marker `structure_rot` uses). No drag interaction.
#[derive(Clone)]
pub struct StructureInvertGadget {
    pub pivot_point: IVec3,
    pub subdivision: i32,
    pub unit_cell: UnitCellStruct,
}

impl Tessellatable for StructureInvertGadget {
    fn tessellate(&self, output: &mut TessellationOutput) {
        let output_mesh: &mut Mesh = &mut output.mesh;

        let pivot_frac = self.pivot_point.as_dvec3() / self.subdivision.max(1) as f64;
        let pivot_real = self.unit_cell.dvec3_lattice_to_real(&pivot_frac);

        let red_material =
            crate::renderer::mesh::Material::new(&glam::f32::Vec3::new(1.0, 0.0, 0.0), 0.4, 0.0);

        crate::renderer::tessellator::tessellator::tessellate_sphere(
            output_mesh,
            &pivot_real,
            0.4,
            12,
            12,
            &red_material,
        );
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for StructureInvertGadget {
    fn hit_test(
        &self,
        _ray_origin: DVec3,
        _ray_direction: DVec3,
        _pick_ctx: &GadgetPickContext,
    ) -> Option<i32> {
        None
    }

    fn start_drag(&mut self, _handle_index: i32, _ray_origin: DVec3, _ray_direction: DVec3) {}

    fn drag(&mut self, _handle_index: i32, _ray_origin: DVec3, _ray_direction: DVec3) {}

    fn end_drag(&mut self) {}
}

impl NodeNetworkGadget for StructureInvertGadget {
    fn sync_data(&self, _data: &mut dyn NodeData) {}

    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }
}

impl StructureInvertGadget {
    pub fn new(pivot_point: IVec3, subdivision: i32, unit_cell: &UnitCellStruct) -> Self {
        Self {
            pivot_point,
            subdivision,
            unit_cell: unit_cell.clone(),
        }
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "structure_invert".to_string(),
        description: "Inverts a structure-bound object (Blueprint or Crystal) through a pivot point in lattice space: every position p maps to 2·pivot − p.
The pivot is pivot_point/subdivision in lattice coordinates, so half-lattice and bond-center pivots are expressible (diamond's inversion centers sit at bond midpoints: pivot (1,1,1) with subdivision 8).
Alignment is preserved when the pivot is an inversion center of the crystal (lattice + motif). Otherwise alignment worsens: to MotifUnaligned when 2·pivot is still a lattice vector, to LatticeUnaligned when it is not.
Combined with structure_rot, inversion reaches every improper lattice symmetry: a reflection equals this node followed by a 180° structure_rot about the mirror normal.
For a Blueprint, only the geometry (the cutter) is inverted.
For a Crystal, atoms and geometry are inverted together.
Molecule inputs are rejected.
The `diff` output pin captures the atom motion only (a Molecule diff applicable via apply_diff); geometry/structure motion is not represented in the diff. A Blueprint input yields an empty diff."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::Geometry3D,
        parameters: vec![
            Parameter {
                id: None,
                name: "input".to_string(),
                data_type: DataType::HasStructure,
            },
            Parameter {
                id: None,
                name: "pivot_point".to_string(),
                data_type: DataType::IVec3,
            },
            Parameter {
                id: None,
                name: "subdivision".to_string(),
                data_type: DataType::Int,
            },
        ],
        output_pins: vec![
            OutputPinDefinition::same_as_input("result", "input"),
            OutputPinDefinition::fixed("diff", DataType::Molecule),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || {
            Box::new(StructureInvertData {
                pivot_point: IVec3::new(0, 0, 0),
                subdivision: 1,
            })
        },
        node_data_saver: generic_node_data_saver::<StructureInvertData>,
        node_data_loader: generic_node_data_loader::<StructureInvertData>,
    }
}
