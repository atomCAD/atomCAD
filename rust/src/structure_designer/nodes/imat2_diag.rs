use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::common_constants::CONNECTED_PIN_SYMBOL;
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
use crate::structure_designer::text_format::TextValue;
use crate::util::serialization_utils::ivec2_serializer;
use glam::i32::IVec2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stored state for the `imat2_diag` node: a diagonal vector `v`. The output
/// matrix is `diag(v.x, v.y)`. Default `v = (1,1)` yields the identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IMat2DiagData {
    #[serde(with = "ivec2_serializer")]
    pub v: IVec2,
}

impl Default for IMat2DiagData {
    fn default() -> Self {
        Self {
            v: IVec2::new(1, 1),
        }
    }
}

impl NodeData for IMat2DiagData {
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
        let v = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            0,
            self.v,
            NetworkResult::extract_ivec2,
        ) {
            Ok(v) => v,
            Err(e) => return EvalOutput::single(e),
        };

        EvalOutput::single(NetworkResult::IMat2([[v.x, 0], [0, v.y]]))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        if connected_input_pins.contains("v") {
            Some(format!("diag({})", CONNECTED_PIN_SYMBOL))
        } else {
            Some(format!("diag({},{})", self.v.x, self.v.y))
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![("v".to_string(), TextValue::IVec2(self.v))]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(val) = props.get("v") {
            self.v = val
                .as_ivec2()
                .ok_or_else(|| "v must be an IVec2".to_string())?;
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "imat2_diag".to_string(),
        description: "Constructs a diagonal IMat2 from a single IVec2. \
            Output is diag(v.x, v.y). Default v = (1,1) yields the identity."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![Parameter {
            id: None,
            name: "v".to_string(),
            data_type: DataType::IVec2,
        }],
        output_pins: OutputPinDefinition::single(DataType::IMat2),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(IMat2DiagData::default()),
        node_data_saver: generic_node_data_saver::<IMat2DiagData>,
        node_data_loader: generic_node_data_loader::<IMat2DiagData>,
    }
}
