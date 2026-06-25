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
use glam::f64::DVec3;
use serde::{Deserialize, Serialize};

/// `lattice_vecs_params` exposes the crystallographic parameter view (lengths and
/// angles) of a `LatticeVecs` value. It is the (parameter-form) inverse of the
/// `lattice_vecs` constructor — a stateless, fixed-pin destructure node. Angles are
/// in degrees (`alpha = b∠c`, `beta = a∠c`, `gamma = a∠b`). `UnitCellStruct` keeps
/// the parameter view consistent with the basis vectors on every construction path,
/// so this node does no geometry — it reads the stored fields directly. See
/// `doc/design_structure_lattice_unpack_nodes.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeVecsParamsData {}

impl NodeData for LatticeVecsParamsData {
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
            NetworkResult::None => EvalOutput::multi(vec![NetworkResult::None; 8]),
            NetworkResult::Error(_) => EvalOutput::multi(vec![arg; 8]),
            NetworkResult::LatticeVecs(uc) => EvalOutput::multi(vec![
                NetworkResult::Float(uc.cell_length_a),
                NetworkResult::Float(uc.cell_length_b),
                NetworkResult::Float(uc.cell_length_c),
                NetworkResult::Float(uc.cell_angle_alpha),
                NetworkResult::Float(uc.cell_angle_beta),
                NetworkResult::Float(uc.cell_angle_gamma),
                NetworkResult::Vec3(DVec3::new(
                    uc.cell_length_a,
                    uc.cell_length_b,
                    uc.cell_length_c,
                )),
                NetworkResult::Vec3(DVec3::new(
                    uc.cell_angle_alpha,
                    uc.cell_angle_beta,
                    uc.cell_angle_gamma,
                )),
            ]),
            _ => {
                let e =
                    NetworkResult::Error("lattice_vecs_params: expected a LatticeVecs".to_string());
                EvalOutput::multi(vec![e; 8])
            }
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
        m.insert(
            "lattice_vecs".to_string(),
            (
                false,
                Some(
                    "Lattice vectors to read cell parameters from. If unconnected, every \
                        output pin emits None."
                        .to_string(),
                ),
            ),
        );
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "lattice_vecs_params".to_string(),
        description: "Reads the crystallographic cell parameters of a `LatticeVecs` value: \
            lengths `a`, `b`, `c` and angles `alpha`, `beta`, `gamma` (degrees; \
            alpha = b∠c, beta = a∠c, gamma = a∠b), plus `lengths`/`angles` packed as \
            Vec3. The (parameter-form) inverse of the `lattice_vecs` constructor."
            .to_string(),
        summary: Some("Read cell parameters of lattice vecs".to_string()),
        category: NodeTypeCategory::OtherBuiltin,
        parameters: vec![Parameter {
            id: None,
            name: "lattice_vecs".to_string(),
            data_type: DataType::LatticeVecs,
        }],
        output_pins: vec![
            OutputPinDefinition::fixed("a", DataType::Float),
            OutputPinDefinition::fixed("b", DataType::Float),
            OutputPinDefinition::fixed("c", DataType::Float),
            OutputPinDefinition::fixed("alpha", DataType::Float),
            OutputPinDefinition::fixed("beta", DataType::Float),
            OutputPinDefinition::fixed("gamma", DataType::Float),
            OutputPinDefinition::fixed("lengths", DataType::Vec3),
            OutputPinDefinition::fixed("angles", DataType::Vec3),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(LatticeVecsParamsData {}),
        node_data_saver: generic_node_data_saver::<LatticeVecsParamsData>,
        node_data_loader: generic_node_data_loader::<LatticeVecsParamsData>,
    }
}
