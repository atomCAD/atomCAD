use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::structure::Structure;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
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
use crate::structure_designer::evaluator::network_result::unit_cell_mismatch_error;
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

        let (mut geometry, base_lattice_vecs, base_alignment) = helper_union(
            network_evaluator,
            network_stack,
            node_id,
            0,
            registry,
            context,
        );

        if geometry.is_none() {
            return EvalOutput::single(error_in_input(&base_input_name));
        }

        if base_lattice_vecs.is_none() {
            return EvalOutput::single(unit_cell_mismatch_error());
        }

        let result_lattice_vecs = base_lattice_vecs.unwrap();
        let mut alignment = base_alignment;

        if !node.arguments[1].is_empty() {
            let (sub_geometry, sub_lattice_vecs, sub_alignment) = helper_union(
                network_evaluator,
                network_stack,
                node_id,
                1,
                registry,
                context,
            );

            if sub_geometry.is_none() {
                return EvalOutput::single(error_in_input(&sub_input_name));
            }

            if sub_lattice_vecs.is_none() {
                return EvalOutput::single(unit_cell_mismatch_error());
            }

            if !result_lattice_vecs.is_approximately_equal(&sub_lattice_vecs.unwrap()) {
                return EvalOutput::single(unit_cell_mismatch_error());
            }

            alignment.worsen_to(sub_alignment);

            geometry = Some(GeoNode::difference_3d(
                Box::new(geometry.unwrap()),
                Box::new(sub_geometry.unwrap()),
            ));
        }

        EvalOutput::single(NetworkResult::Blueprint(BlueprintData {
            structure: Structure::from_lattice_vecs(result_lattice_vecs),
            geo_tree_root: geometry.unwrap(),
            alignment,
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

fn helper_union<'a>(
    network_evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,
    parameter_index: usize,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
) -> (Option<GeoNode>, Option<UnitCellStruct>, Alignment) {
    let mut shapes: Vec<GeoNode> = Vec::new();

    let shapes_val = network_evaluator.evaluate_arg_required(
        network_stack,
        node_id,
        registry,
        context,
        parameter_index,
    );

    if let NetworkResult::Error(_) = shapes_val {
        return (None, None, Alignment::Aligned);
    }

    // Extract the array elements from shapes_val
    let shape_results = if let NetworkResult::Array(array_elements) = shapes_val {
        array_elements
    } else {
        return (None, None, Alignment::Aligned);
    };

    let shape_count = shape_results.len();

    if shape_count == 0 {
        return (None, None, Alignment::Aligned);
    }

    // Extract geometries and check lattice vector compatibility
    let mut blueprints: Vec<BlueprintData> = Vec::new();
    for shape_val in shape_results {
        if let NetworkResult::Blueprint(shape) = shape_val {
            blueprints.push(shape);
        } else {
            return (None, None, Alignment::Aligned);
        }
    }

    if !BlueprintData::all_have_compatible_lattice_vecs(&blueprints) {
        return (None, None, Alignment::Aligned);
    }

    let first_lattice_vecs = blueprints[0].structure.lattice_vecs.clone();
    let mut alignment = Alignment::Aligned;
    for bp in blueprints.into_iter() {
        alignment.worsen_to(bp.alignment);
        shapes.push(bp.geo_tree_root);
    }

    (
        Some(GeoNode::union_3d(shapes)),
        Some(first_lattice_vecs),
        alignment,
    )
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
