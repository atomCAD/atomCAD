// Dematerialize: Crystal -> Blueprint. Discards atoms and returns to the
// pre-carving blueprint stage. Errors if the Crystal has no geometry shell,
// because a Blueprint must carry geometry.
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{
    BlueprintData, NetworkResult, runtime_type_error_in_input,
};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DematerializeData {}

impl NodeData for DematerializeData {
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
        let input_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if let NetworkResult::Error(_) = input_val {
            return EvalOutput::single(input_val);
        }

        match input_val {
            NetworkResult::Crystal(crystal) => match crystal.geo_tree_root {
                Some(geo_tree_root) => {
                    EvalOutput::single(NetworkResult::Blueprint(BlueprintData {
                        structure: crystal.structure,
                        geo_tree_root,
                        alignment: crystal.alignment,
                        alignment_reason: crystal.alignment_reason,
                    }))
                }
                None => EvalOutput::single(NetworkResult::Error(
                    "dematerialize: Crystal has no geometry to return to Blueprint".to_string(),
                )),
            },
            _ => EvalOutput::single(runtime_type_error_in_input(0)),
        }
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
        m.insert("input".to_string(), (true, None));
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "dematerialize".to_string(),
        description: "Converts a Crystal back to a Blueprint by discarding its carved atoms. \
                      The geometry shell is preserved as the Blueprint's geometry. \
                      Destructive: any atom edits are lost. \
                      Errors if the Crystal has no geometry shell, since a Blueprint must carry geometry."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![Parameter {
            id: None,
            name: "input".to_string(),
            data_type: DataType::Crystal,
        }],
        output_pins: OutputPinDefinition::single_fixed(DataType::Blueprint),
        public: true,
        node_data_creator: || Box::new(DematerializeData::default()),
        node_data_saver: generic_node_data_saver::<DematerializeData>,
        node_data_loader: generic_node_data_loader::<DematerializeData>,
    }
}
