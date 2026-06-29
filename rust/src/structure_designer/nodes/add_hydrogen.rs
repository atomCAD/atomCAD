use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::hydrogen_passivation::{AddHydrogensOptions, add_hydrogens_filtered};
use crate::crystolecule::lattice_fill::DEFAULT_REGION_MARGIN;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::atom_op::map_atomic_in_region;
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
pub struct HydrogenPassivateData {}

#[derive(Debug, Clone)]
pub struct HydrogenPassivateEvalCache {
    pub message: String,
}

impl NodeData for HydrogenPassivateData {
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

        // Optional `region` pin (param index 1). Disconnected → passivate every
        // atom. Connected → only in-region host atoms are passivated (see
        // design_blueprint_region_atom_edits.md §A1/§A4).
        let region_input =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
        let region_geo = match region_input {
            NetworkResult::None => None,
            NetworkResult::Error(_) => return EvalOutput::single(region_input),
            NetworkResult::Blueprint(bp) => Some(bp.geo_tree_root),
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "add_hydrogen.region: expected Blueprint, got {:?}",
                    other.infer_data_type()
                )));
            }
        };

        let at_root = network_stack.len() == 1;
        let output = map_atomic_in_region(
            input_val,
            region_geo.as_ref(),
            DEFAULT_REGION_MARGIN,
            |mut structure, in_region| {
                let options = AddHydrogensOptions {
                    selected_only: false,
                    skip_already_passivated: true,
                };
                let result = add_hydrogens_filtered(&mut structure, &options, in_region);

                if at_root {
                    context.selected_node_eval_cache = Some(Box::new(HydrogenPassivateEvalCache {
                        message: format!("Added {} hydrogens", result.hydrogens_added),
                    }));
                }
                structure
            },
        );
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
        m.insert("region".to_string(), (false, None)); // optional
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "add_hydrogen".to_string(),
        description: "Adds hydrogen atoms to satisfy valence requirements \
                       of all undersaturated atoms in the input structure."
            .to_string(),
        summary: Some("Add H to open bonds".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "molecule".to_string(),
                data_type: DataType::HasAtoms,
            },
            Parameter {
                id: None,
                name: "region".to_string(),
                data_type: DataType::Blueprint,
            },
        ],
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(HydrogenPassivateData {}),
        node_data_saver: generic_node_data_saver::<HydrogenPassivateData>,
        node_data_loader: generic_node_data_loader::<HydrogenPassivateData>,
    }
}
