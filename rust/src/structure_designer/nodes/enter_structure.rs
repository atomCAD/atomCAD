// EnterStructure: (Molecule, Structure) -> Crystal. Re-associates a free
// Molecule with a structure. Pure packaging: atoms are not snapped to lattice
// positions.
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{
    Alignment, CrystalData, NetworkResult, runtime_type_error_in_input,
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
pub struct EnterStructureData {}

impl NodeData for EnterStructureData {
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

        let structure_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 1);
        if let NetworkResult::Error(_) = structure_val {
            return EvalOutput::single(structure_val);
        }

        let structure = match structure_val {
            NetworkResult::Structure(s) => s,
            _ => return EvalOutput::single(runtime_type_error_in_input(1)),
        };

        match input_val {
            NetworkResult::Molecule(mol) => {
                EvalOutput::single(NetworkResult::Crystal(CrystalData {
                    structure,
                    atoms: mol.atoms,
                    geo_tree_root: mol.geo_tree_root,
                    alignment: Alignment::LatticeUnaligned,
                    alignment_reason: Some(
                        "enter_structure: molecule atoms are not registered to the target structure's lattice".to_string(),
                    ),
                }))
            }
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
        m.insert("structure".to_string(), (true, None));
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "enter_structure".to_string(),
        description: "Converts a Molecule into a Crystal by re-associating it with a Structure. \
                      Pure packaging: atoms are not snapped to lattice positions."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "input".to_string(),
                data_type: DataType::Molecule,
            },
            Parameter {
                id: None,
                name: "structure".to_string(),
                data_type: DataType::Structure,
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Crystal),
        public: true,
        node_data_creator: || Box::new(EnterStructureData::default()),
        node_data_saver: generic_node_data_saver::<EnterStructureData>,
        node_data_loader: generic_node_data_loader::<EnterStructureData>,
    }
}
