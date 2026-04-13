use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::structure::Structure;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureData {}

impl NodeData for StructureData {
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
        // Pin 0: optional base Structure. If unconnected, use diamond defaults.
        let base = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            0,
            None,
            NetworkResult::extract_optional_structure,
        ) {
            Ok(value) => value.unwrap_or_else(Structure::diamond),
            Err(error) => return EvalOutput::single(error),
        };

        // Pin 1: optional LatticeVecs override (default = base.lattice_vecs).
        let lattice_vecs = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            base.lattice_vecs.clone(),
            NetworkResult::extract_unit_cell,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // Pin 2: optional Motif override (default = base.motif).
        let motif = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            base.motif.clone(),
            NetworkResult::extract_motif,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // Pin 3: optional motif_offset override (default = base.motif_offset).
        let motif_offset = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            3,
            base.motif_offset,
            NetworkResult::extract_vec3,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        EvalOutput::single(NetworkResult::Structure(Structure {
            lattice_vecs,
            motif,
            motif_offset,
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
        // All four inputs are optional; defaults come from the diamond structure
        // (or, if `structure` is connected, pass through unchanged from the base).
        m.insert(
            "structure".to_string(),
            (
                false,
                Some(
                    "Base structure to modify. If unconnected, a fresh diamond structure is used."
                        .to_string(),
                ),
            ),
        );
        m.insert(
            "lattice_vecs".to_string(),
            (
                false,
                Some("Overrides the lattice vectors. Default: base structure's lattice vectors (diamond if no base).".to_string()),
            ),
        );
        m.insert(
            "motif".to_string(),
            (
                false,
                Some("Overrides the motif. Default: base structure's motif (diamond zincblende if no base).".to_string()),
            ),
        );
        m.insert(
            "motif_offset".to_string(),
            (
                false,
                Some("Overrides the motif offset. Default: base structure's motif offset (zero if no base).".to_string()),
            ),
        );
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "structure".to_string(),
        description: "Constructs or modifies a `Structure` value (lattice vectors + motif + motif offset). \
            All four inputs are optional. When `structure` is connected, unconnected fields pass through from the base; \
            when `structure` is unconnected, unconnected fields use diamond defaults."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::OtherBuiltin,
        parameters: vec![
            Parameter {
                id: None,
                name: "structure".to_string(),
                data_type: DataType::Structure,
            },
            Parameter {
                id: None,
                name: "lattice_vecs".to_string(),
                data_type: DataType::LatticeVecs,
            },
            Parameter {
                id: None,
                name: "motif".to_string(),
                data_type: DataType::Motif,
            },
            Parameter {
                id: None,
                name: "motif_offset".to_string(),
                data_type: DataType::Vec3,
            },
        ],
        output_pins: OutputPinDefinition::single(DataType::Structure),
        public: true,
        node_data_creator: || Box::new(StructureData {}),
        node_data_saver: generic_node_data_saver::<StructureData>,
        node_data_loader: generic_node_data_loader::<StructureData>,
    }
}
