use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::structure::Structure;
use crate::geo_tree::GeoNode;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::Alignment;
use crate::structure_designer::evaluator::network_result::BlueprintData;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::structure_designer::evaluator::network_result::input_missing_error;
use crate::structure_designer::evaluator::network_result::propagate_alignment_with_reason;
use crate::structure_designer::evaluator::network_result::structure_mismatch_error;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffData {}

impl NodeData for DiffData {
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
        //let _timer = Timer::new("eval_diff");
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        let base_input_name = registry.get_parameter_name(node, 0);
        let sub_input_name = registry.get_parameter_name(node, 1);

        if node.arguments[0].is_empty() {
            return EvalOutput::single(input_missing_error(&base_input_name));
        }

        let (mut geometry, output_structure, base_alignment, base_reason) = match helper_union(
            network_evaluator,
            network_stack,
            node_id,
            0,
            registry,
            context,
        ) {
            Ok(parts) => parts,
            Err(HelperUnionError::NoShapes) => {
                return EvalOutput::single(error_in_input(&base_input_name));
            }
            Err(HelperUnionError::StructureMismatch) => {
                return EvalOutput::single(structure_mismatch_error());
            }
        };

        let mut alignment = base_alignment;
        let mut alignment_reason = base_reason;

        if !node.arguments[1].is_empty() {
            let (sub_geometry, sub_structure, sub_alignment, sub_reason) = match helper_union(
                network_evaluator,
                network_stack,
                node_id,
                1,
                registry,
                context,
            ) {
                Ok(parts) => parts,
                Err(HelperUnionError::NoShapes) => {
                    return EvalOutput::single(error_in_input(&sub_input_name));
                }
                Err(HelperUnionError::StructureMismatch) => {
                    return EvalOutput::single(structure_mismatch_error());
                }
            };

            if !output_structure.is_approximately_equal(&sub_structure) {
                return EvalOutput::single(structure_mismatch_error());
            }

            propagate_alignment_with_reason(
                &mut alignment,
                &mut alignment_reason,
                sub_alignment,
                &sub_reason,
            );

            geometry = GeoNode::difference_3d(Box::new(geometry), Box::new(sub_geometry));
        }

        EvalOutput::single(NetworkResult::Blueprint(BlueprintData {
            structure: output_structure,
            geo_tree_root: geometry,
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

    fn get_parameter_metadata(&self) -> std::collections::HashMap<String, (bool, Option<String>)> {
        let mut m = std::collections::HashMap::new();
        m.insert("base".to_string(), (true, None)); // required
        m.insert("sub".to_string(), (true, None)); // required
        m
    }
}

enum HelperUnionError {
    /// The input array was empty, missing, or contained a non-Blueprint value.
    NoShapes,
    /// Two or more Blueprints in the array carried different Structures.
    StructureMismatch,
}

fn helper_union<'a>(
    network_evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,
    parameter_index: usize,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
) -> Result<(GeoNode, Structure, Alignment, Option<String>), HelperUnionError> {
    let mut shapes: Vec<GeoNode> = Vec::new();

    let shapes_val = network_evaluator.evaluate_arg_required(
        network_stack,
        node_id,
        registry,
        context,
        parameter_index,
    );

    if let NetworkResult::Error(_) = shapes_val {
        return Err(HelperUnionError::NoShapes);
    }

    let shape_results = if let NetworkResult::Array(array_elements) = shapes_val {
        array_elements
    } else {
        return Err(HelperUnionError::NoShapes);
    };

    if shape_results.is_empty() {
        return Err(HelperUnionError::NoShapes);
    }

    let mut blueprints: Vec<BlueprintData> = Vec::new();
    for shape_val in shape_results {
        if let NetworkResult::Blueprint(shape) = shape_val {
            blueprints.push(shape);
        } else {
            return Err(HelperUnionError::NoShapes);
        }
    }

    if !BlueprintData::all_have_same_structure(&blueprints) {
        return Err(HelperUnionError::StructureMismatch);
    }

    let first_structure = blueprints[0].structure.clone();
    let mut alignment = Alignment::Aligned;
    let mut alignment_reason: Option<String> = None;
    for bp in blueprints.into_iter() {
        propagate_alignment_with_reason(
            &mut alignment,
            &mut alignment_reason,
            bp.alignment,
            &bp.alignment_reason,
        );
        shapes.push(bp.geo_tree_root);
    }

    Ok((
        GeoNode::union_3d(shapes),
        first_structure,
        alignment,
        alignment_reason,
    ))
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "diff".to_string(),
        description: "Computes the Boolean difference of two 3D geometries.".to_string(),
        summary: None,
        category: NodeTypeCategory::Geometry3D,
        parameters: vec![
            Parameter {
                id: None,
                name: "base".to_string(),
                data_type: DataType::Array(Box::new(DataType::Blueprint)), // If multiple shapes are given, they are unioned.
            },
            Parameter {
                id: None,
                name: "sub".to_string(),
                data_type: DataType::Array(Box::new(DataType::Blueprint)), // A set of shapes to subtract from base
            },
        ],
        output_pins: OutputPinDefinition::single(DataType::Blueprint),
        public: true,
        node_data_creator: || Box::new(DiffData {}),
        node_data_saver: generic_node_data_saver::<DiffData>,
        node_data_loader: generic_node_data_loader::<DiffData>,
    }
}
