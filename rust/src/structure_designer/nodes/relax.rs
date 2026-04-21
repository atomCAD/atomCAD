use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::simulation::minimize_energy;
use crate::crystolecule::simulation::uff::VdwMode;
use crate::structure_designer::data_type::DataType;
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

/// Maximum number of atoms the relax node will process before returning an error.
/// Minimization has O(N²) complexity due to nonbonded pair enumeration, so large
/// structures would block the UI thread for an unacceptable time (issue #271).
pub const MAX_RELAX_ATOMS: usize = 2000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelaxData {}

#[derive(Debug, Clone)]
pub struct RelaxEvalCache {
    pub relax_message: String,
}

impl NodeData for RelaxData {
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

        let mut wrapper = match input_val {
            NetworkResult::Crystal(_) | NetworkResult::Molecule(_) => input_val,
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "relax: expected atomic input, got {:?}",
                    other.infer_data_type()
                )));
            }
        };

        let atoms_ref = match &mut wrapper {
            NetworkResult::Crystal(c) => &mut c.atoms,
            NetworkResult::Molecule(m) => &mut m.atoms,
            _ => unreachable!(),
        };

        let num_atoms = atoms_ref.get_num_of_atoms();
        if num_atoms > MAX_RELAX_ATOMS {
            return EvalOutput::single(NetworkResult::Error(format!(
                "Structure has {} atoms, which exceeds the relax node limit of {}. \
                 Large structures cause excessive computation time. \
                 Consider reducing the structure size or using a smaller region.",
                num_atoms, MAX_RELAX_ATOMS
            )));
        }

        let vdw_mode = if context.use_vdw_cutoff {
            VdwMode::Cutoff(6.0)
        } else {
            VdwMode::AllPairs
        };
        match minimize_energy(atoms_ref, vdw_mode) {
            Ok(result) => {
                // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
                // Only store for direct evaluations of visible nodes, not for upstream dependency calculations
                if network_stack.len() == 1 {
                    let eval_cache = RelaxEvalCache {
                        relax_message: result.message.clone(),
                    };
                    context.selected_node_eval_cache = Some(Box::new(eval_cache));
                }

                EvalOutput::single(wrapper)
            }
            Err(error_msg) => EvalOutput::single(NetworkResult::Error(error_msg)),
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
        m.insert("molecule".to_string(), (true, None)); // required
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "relax".to_string(),
        description: "".to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![Parameter {
            id: None,
            name: "molecule".to_string(),
            data_type: DataType::HasAtoms,
        }],
        output_pins: OutputPinDefinition::single_same_as("molecule"),
        public: true,
        node_data_creator: || Box::new(RelaxData {}),
        node_data_saver: generic_node_data_saver::<RelaxData>,
        node_data_loader: generic_node_data_loader::<RelaxData>,
    }
}
