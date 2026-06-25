use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
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

/// `structure_unpack` reads the three constituents back out of a `Structure`
/// value: its lattice vectors, motif, and motif offset. It is the inverse of the
/// `structure` constructor — a stateless, fixed-pin destructure node. Chaining it
/// with `lattice_vecs_unpack` yields the basis vectors of a structure. See
/// `doc/design_structure_lattice_unpack_nodes.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureUnpackData {}

impl NodeData for StructureUnpackData {
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
        let arg = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
        match arg {
            // No input wired: emit None on every pin (non-blocking; downstream
            // just gets None). A user wanting diamond defaults wires a
            // `structure` node.
            NetworkResult::None => EvalOutput::multi(vec![NetworkResult::None; 3]),
            NetworkResult::Error(_) => EvalOutput::multi(vec![arg.clone(), arg.clone(), arg]),
            NetworkResult::Structure(s) => EvalOutput::multi(vec![
                NetworkResult::LatticeVecs(s.lattice_vecs),
                NetworkResult::Motif(s.motif),
                NetworkResult::Vec3(s.motif_offset),
            ]),
            _ => {
                let e = NetworkResult::Error("structure_unpack: expected a Structure".to_string());
                EvalOutput::multi(vec![e.clone(), e.clone(), e])
            }
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn default_display_all_output_pins(&self) -> bool {
        true
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        None
    }

    fn get_parameter_metadata(&self) -> std::collections::HashMap<String, (bool, Option<String>)> {
        let mut m = std::collections::HashMap::new();
        m.insert(
            "structure".to_string(),
            (
                false,
                Some(
                    "Structure to unpack. If unconnected, every output pin emits None.".to_string(),
                ),
            ),
        );
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "structure_unpack".to_string(),
        description: "Reads the `lattice_vecs`, `motif`, and `motif_offset` out of a `Structure` \
            value. The inverse of the `structure` constructor."
            .to_string(),
        summary: Some("Read fields of a structure".to_string()),
        category: NodeTypeCategory::OtherBuiltin,
        parameters: vec![Parameter {
            id: None,
            name: "structure".to_string(),
            data_type: DataType::Structure,
        }],
        output_pins: vec![
            OutputPinDefinition::fixed("lattice_vecs", DataType::LatticeVecs),
            OutputPinDefinition::fixed("motif", DataType::Motif),
            OutputPinDefinition::fixed("motif_offset", DataType::Vec3),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(StructureUnpackData {}),
        node_data_saver: generic_node_data_saver::<StructureUnpackData>,
        node_data_loader: generic_node_data_loader::<StructureUnpackData>,
    }
}
