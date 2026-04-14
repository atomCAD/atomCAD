use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::hydrogen_passivation::{RemoveHydrogensOptions, remove_hydrogens};
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::atom_op::map_atomic;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydrogenDepassivateData {}

#[derive(Debug, Clone)]
pub struct HydrogenDepassivateEvalCache {
    pub message: String,
}

impl NodeData for HydrogenDepassivateData {
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

        let at_root = network_stack.len() == 1;
        let output = map_atomic(input_val, |mut structure| {
            let options = RemoveHydrogensOptions {
                selected_only: false,
            };
            let result = remove_hydrogens(&mut structure, &options);

            if at_root {
                context.selected_node_eval_cache = Some(Box::new(HydrogenDepassivateEvalCache {
                    message: format!("Removed {} hydrogens", result.hydrogens_removed),
                }));
            }
            structure
        });
        EvalOutput::single(output)
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
        m.insert("molecule".to_string(), (true, None)); // required
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "remove_hydrogen".to_string(),
        description: "Removes all hydrogen atoms from the input structure.".to_string(),
        summary: Some("Strip all H atoms".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![Parameter {
            id: None,
            name: "molecule".to_string(),
            data_type: DataType::Atomic,
        }],
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        public: true,
        node_data_creator: || Box::new(HydrogenDepassivateData {}),
        node_data_saver: generic_node_data_saver::<HydrogenDepassivateData>,
        node_data_loader: generic_node_data_loader::<HydrogenDepassivateData>,
    }
}
