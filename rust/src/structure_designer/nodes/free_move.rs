use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::display::gadget::Gadget;
use crate::geo_tree::GeoNode;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput};
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{
    Alignment, BlueprintData, MoleculeData, NetworkResult, runtime_type_error_in_input,
    worsen_alignment_with_reason,
};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::structure_designer::utils::xyz_gadget_utils;
use crate::util::serialization_utils::dvec3_serializer;
use crate::util::transform::Transform;
use glam::f64::DQuat;
use glam::f64::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct FreeMoveEvalCache {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeMoveData {
    #[serde(with = "dvec3_serializer")]
    pub translation: DVec3,
}

impl NodeData for FreeMoveData {
    fn provide_gadget(
        &self,
        structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let _cache = eval_cache.downcast_ref::<FreeMoveEvalCache>()?;

        Some(Box::new(FreeMoveGadget::new(self.translation)))
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
        context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext,
    ) -> EvalOutput {
        let input_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if let NetworkResult::Error(_) = input_val {
            return EvalOutput::single(input_val);
        }

        let translation = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.translation,
            NetworkResult::extract_vec3,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        if network_stack.len() == 1 {
            context.selected_node_eval_cache = Some(Box::new(FreeMoveEvalCache {}));
        }

        let tr = Transform::new(translation, DQuat::IDENTITY);

        match input_val {
            NetworkResult::Blueprint(shape) => {
                let mut alignment = shape.alignment;
                let mut alignment_reason = shape.alignment_reason;
                worsen_alignment_with_reason(
                    &mut alignment,
                    &mut alignment_reason,
                    Alignment::LatticeUnaligned,
                    || {
                        format!(
                            "free_move translates the cutter by ({:.3}, {:.3}, {:.3}) in world space (off-lattice)",
                            translation.x, translation.y, translation.z
                        )
                    },
                );
                EvalOutput::single(NetworkResult::Blueprint(BlueprintData {
                    structure: shape.structure,
                    geo_tree_root: GeoNode::transform(tr, Box::new(shape.geo_tree_root)),
                    alignment,
                    alignment_reason,
                }))
            }
            NetworkResult::Molecule(mol) => {
                let mut atoms = mol.atoms;
                atoms.transform(&DQuat::IDENTITY, &translation);
                let new_geo = mol
                    .geo_tree_root
                    .map(|gt| GeoNode::transform(tr, Box::new(gt)));
                EvalOutput::single(NetworkResult::Molecule(MoleculeData {
                    atoms,
                    geo_tree_root: new_geo,
                }))
            }
            _ => EvalOutput::single(runtime_type_error_in_input(0)),
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![("translation".to_string(), TextValue::Vec3(self.translation))]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("translation") {
            self.translation = v
                .as_vec3()
                .ok_or_else(|| "translation must be a Vec3".to_string())?;
        }
        Ok(())
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        if connected_input_pins.contains("translation") {
            return None;
        }
        Some(format!(
            "({:.2}, {:.2}, {:.2})",
            self.translation.x, self.translation.y, self.translation.z
        ))
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("input".to_string(), (true, None));
        m
    }
}

#[derive(Clone)]
pub struct FreeMoveGadget {
    pub translation: DVec3,
    pub dragged_handle_index: Option<i32>,
    pub start_drag_offset: f64,
    pub start_drag_translation: DVec3,
}

impl Tessellatable for FreeMoveGadget {
    fn tessellate(&self, output: &mut TessellationOutput) {
        let output_mesh: &mut Mesh = &mut output.mesh;
        xyz_gadget_utils::tessellate_xyz_gadget(
            output_mesh,
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            &self.translation,
            false,
        );
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for FreeMoveGadget {
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        xyz_gadget_utils::xyz_gadget_hit_test(
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            &self.translation,
            &ray_origin,
            &ray_direction,
            false,
        )
    }

    fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        self.dragged_handle_index = Some(handle_index);
        self.start_drag_offset = xyz_gadget_utils::get_dragged_axis_offset(
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            &self.translation,
            handle_index,
            &ray_origin,
            &ray_direction,
        );
        self.start_drag_translation = self.translation;
    }

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        let current_offset = xyz_gadget_utils::get_dragged_axis_offset(
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            &self.translation,
            handle_index,
            &ray_origin,
            &ray_direction,
        );
        let offset_delta = current_offset - self.start_drag_offset;
        if self.apply_drag_offset(handle_index, offset_delta) {
            self.start_drag(handle_index, ray_origin, ray_direction);
        }
    }

    fn end_drag(&mut self) {
        self.dragged_handle_index = None;
    }
}

impl NodeNetworkGadget for FreeMoveGadget {
    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(d) = data.as_any_mut().downcast_mut::<FreeMoveData>() {
            d.translation = self.translation;
        }
    }

    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }
}

impl FreeMoveGadget {
    pub fn new(translation: DVec3) -> Self {
        Self {
            translation,
            dragged_handle_index: None,
            start_drag_offset: 0.0,
            start_drag_translation: translation,
        }
    }

    fn apply_drag_offset(&mut self, axis_index: i32, offset_delta: f64) -> bool {
        if !(0..=2).contains(&axis_index) {
            return false;
        }

        let axis_direction = match xyz_gadget_utils::get_local_axis_direction(
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            axis_index,
        ) {
            Some(dir) => dir,
            None => return false,
        };

        let movement_vector = axis_direction * offset_delta;
        self.translation = self.start_drag_translation + movement_vector;

        true
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "free_move".to_string(),
        description: "Translates an unanchored object (Blueprint or Molecule) by a vector in world space (Cartesian coordinates).
For a Blueprint, only the geometry (the cutter) moves; the structure stays fixed. This can drift the cutter off-lattice.
For a Molecule, atoms and geometry move together freely.
Crystal inputs are rejected (exit_structure first to get a Molecule, or use structure_move to stay in lattice space).".to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "input".to_string(),
                data_type: DataType::Unanchored,
            },
            Parameter {
                id: None,
                name: "translation".to_string(),
                data_type: DataType::Vec3,
            },
        ],
        output_pins: OutputPinDefinition::single_same_as("input"),
        public: true,
        node_data_creator: || {
            Box::new(FreeMoveData {
                translation: DVec3::ZERO,
            })
        },
        node_data_saver: generic_node_data_saver::<FreeMoveData>,
        node_data_loader: generic_node_data_loader::<FreeMoveData>,
    }
}
