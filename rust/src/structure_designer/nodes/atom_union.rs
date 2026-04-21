use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{
    Alignment, NetworkResult, propagate_alignment_with_reason,
};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomUnionData {}

impl NodeData for AtomUnionData {
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
        // Evaluate the structures array input (required)
        let structures_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

        if let NetworkResult::Error(_) = structures_val {
            return EvalOutput::single(structures_val);
        }

        // Extract the array elements
        let structure_results = if let NetworkResult::Array(array_elements) = structures_val {
            array_elements
        } else {
            return EvalOutput::single(NetworkResult::Error(
                "Expected array of atomic structures".to_string(),
            ));
        };

        if structure_results.is_empty() {
            return EvalOutput::single(NetworkResult::Error(
                "atom_union requires at least one input structure".to_string(),
            ));
        }

        // Preserve the concrete variant of the first element (Crystal or Molecule).
        // Per OQ1, mixed-phase arrays are a validation error; the evaluator trusts
        // that validation has ensured all elements share the same variant and
        // debug-asserts that here.
        let mut iter = structure_results.into_iter();
        let mut output = iter.next().expect("non-empty checked above");
        if !matches!(
            output,
            NetworkResult::Crystal(_) | NetworkResult::Molecule(_)
        ) {
            return EvalOutput::single(NetworkResult::Error(
                "All inputs must be atomic structures".to_string(),
            ));
        }

        let mut atomic_structures: Vec<AtomicStructure> = Vec::new();
        let mut rest_alignment = Alignment::Aligned;
        let mut rest_reason: Option<String> = None;
        for structure_val in iter {
            debug_assert_eq!(
                structure_val.infer_data_type(),
                output.infer_data_type(),
                "atom_union: mixed-phase array reached evaluator"
            );
            if let NetworkResult::Crystal(ref c) = structure_val {
                propagate_alignment_with_reason(
                    &mut rest_alignment,
                    &mut rest_reason,
                    c.alignment,
                    &c.alignment_reason,
                );
            }
            if let Some(structure) = structure_val.extract_atomic() {
                atomic_structures.push(structure);
            } else {
                return EvalOutput::single(NetworkResult::Error(
                    "All inputs must be atomic structures".to_string(),
                ));
            }
        }

        let merged_ref = match &mut output {
            NetworkResult::Crystal(c) => {
                propagate_alignment_with_reason(
                    &mut c.alignment,
                    &mut c.alignment_reason,
                    rest_alignment,
                    &rest_reason,
                );
                &mut c.atoms
            }
            NetworkResult::Molecule(m) => &mut m.atoms,
            _ => unreachable!(),
        };
        for other in &atomic_structures {
            merged_ref.add_atomic_structure(other);
        }

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
        m.insert("structures".to_string(), (true, None)); // required
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "atom_union".to_string(),
        description: "Merges multiple atomic structures into one. The `structures` input accepts an array of `Atomic` values (array-typed input; you can connect multiple wires and they will be concatenated).".to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "structures".to_string(),
                data_type: DataType::Array(Box::new(DataType::Atomic)),
            },
        ],
        output_pins: OutputPinDefinition::single_same_as_array_elements("structures"),
        public: true,
        node_data_creator: || Box::new(AtomUnionData {}),
        node_data_saver: generic_node_data_saver::<AtomUnionData>,
        node_data_loader: generic_node_data_loader::<AtomUnionData>,
    }
}
