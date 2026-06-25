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

/// `lattice_vecs_unpack` reads the three stored basis vectors back out of a
/// `LatticeVecs` value. It is the (vector-form) inverse of the `lattice_vecs`
/// constructor — a stateless, fixed-pin destructure node. See
/// `doc/design_structure_lattice_unpack_nodes.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeVecsUnpackData {}

impl NodeData for LatticeVecsUnpackData {
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
            // `lattice_vecs` node.
            NetworkResult::None => EvalOutput::multi(vec![NetworkResult::None; 3]),
            NetworkResult::Error(_) => EvalOutput::multi(vec![arg.clone(), arg.clone(), arg]),
            NetworkResult::LatticeVecs(uc) => EvalOutput::multi(vec![
                NetworkResult::Vec3(uc.a),
                NetworkResult::Vec3(uc.b),
                NetworkResult::Vec3(uc.c),
            ]),
            _ => {
                let e =
                    NetworkResult::Error("lattice_vecs_unpack: expected a LatticeVecs".to_string());
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
            "lattice_vecs".to_string(),
            (
                false,
                Some(
                    "Lattice vectors to unpack. If unconnected, every output pin emits None."
                        .to_string(),
                ),
            ),
        );
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "lattice_vecs_unpack".to_string(),
        description: "Reads the three basis vectors `a`, `b`, `c` out of a `LatticeVecs` value. \
            The (vector-form) inverse of the `lattice_vecs` constructor."
            .to_string(),
        summary: Some("Read basis vectors of lattice vecs".to_string()),
        category: NodeTypeCategory::OtherBuiltin,
        parameters: vec![Parameter {
            id: None,
            name: "lattice_vecs".to_string(),
            data_type: DataType::LatticeVecs,
        }],
        output_pins: vec![
            OutputPinDefinition::fixed("a", DataType::Vec3),
            OutputPinDefinition::fixed("b", DataType::Vec3),
            OutputPinDefinition::fixed("c", DataType::Vec3),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(LatticeVecsUnpackData {}),
        node_data_saver: generic_node_data_saver::<LatticeVecsUnpackData>,
        node_data_loader: generic_node_data_loader::<LatticeVecsUnpackData>,
    }
}
