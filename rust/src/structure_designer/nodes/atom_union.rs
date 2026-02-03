use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::util::transform::Transform;
use glam::f64::{DVec3, DQuat};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomUnionData {}

impl NodeData for AtomUnionData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
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
    ) -> NetworkResult {
        // Evaluate the structures array input (required)
        let structures_val = network_evaluator.evaluate_arg_required(
            network_stack,
            node_id,
            registry,
            context,
            0,
        );

        if let NetworkResult::Error(_) = structures_val {
            return structures_val;
        }

        // Extract the array elements
        let structure_results = if let NetworkResult::Array(array_elements) = structures_val {
            array_elements
        } else {
            return NetworkResult::Error("Expected array of atomic structures".to_string());
        };

        let structure_count = structure_results.len();

        if structure_count == 0 {
            return NetworkResult::Error("atom_union requires at least one input structure".to_string());
        }

        // Extract atomic structures and collect frame translations for averaging
        let mut atomic_structures: Vec<AtomicStructure> = Vec::new();
        let mut frame_translation_sum = DVec3::ZERO;

        for structure_val in structure_results {
            if let NetworkResult::Atomic(structure) = structure_val {
                frame_translation_sum += structure.frame_transform().translation;
                atomic_structures.push(structure);
            } else {
                return NetworkResult::Error("All inputs must be atomic structures".to_string());
            }
        }

        // Start with the first structure as base
        let mut result = atomic_structures.remove(0);

        // Merge all subsequent structures into the result
        for other in &atomic_structures {
            result.add_atomic_structure(other);
        }

        // Set the frame transform to the average translation with identity rotation
        let avg_translation = frame_translation_sum / structure_count as f64;
        result.set_frame_transform(Transform::new(avg_translation, DQuat::IDENTITY));

        NetworkResult::Atomic(result)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
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
        output_type: DataType::Atomic,
        public: true,
        node_data_creator: || Box::new(AtomUnionData {}),
        node_data_saver: generic_node_data_saver::<AtomUnionData>,
        node_data_loader: generic_node_data_loader::<AtomUnionData>,
    }
}
