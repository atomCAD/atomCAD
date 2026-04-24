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
use crate::util::serialization_utils::dvec3_serializer;
use glam::f64::{DMat3, DVec3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stored state for the `mat3_diag` node: a diagonal vector `v`. The output
/// matrix is `diag(v.x, v.y, v.z)`. Default `v = (1,1,1)` yields the identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mat3DiagData {
    #[serde(with = "dvec3_serializer")]
    pub v: DVec3,
}

impl Default for Mat3DiagData {
    fn default() -> Self {
        Self {
            v: DVec3::new(1.0, 1.0, 1.0),
        }
    }
}

impl NodeData for Mat3DiagData {
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
            NetworkResult::extract_vec3,
        ) {
            Ok(v) => v,
            Err(e) => return EvalOutput::single(e),
        };

        EvalOutput::single(NetworkResult::Mat3(DMat3::from_cols(
            DVec3::new(v.x, 0.0, 0.0),
            DVec3::new(0.0, v.y, 0.0),
            DVec3::new(0.0, 0.0, v.z),
        )))
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
            Some(format!(
                "diag({:.2},{:.2},{:.2})",
                self.v.x, self.v.y, self.v.z
            ))
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![("v".to_string(), TextValue::Vec3(self.v))]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(val) = props.get("v") {
            self.v = val
                .as_vec3()
                .ok_or_else(|| "v must be a Vec3".to_string())?;
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "mat3_diag".to_string(),
        description: "Constructs a diagonal Mat3 from a single Vec3. \
            Output is diag(v.x, v.y, v.z). Default v = (1,1,1) yields the identity."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![Parameter {
            id: None,
            name: "v".to_string(),
            data_type: DataType::Vec3,
        }],
        output_pins: OutputPinDefinition::single(DataType::Mat3),
        public: true,
        node_data_creator: || Box::new(Mat3DiagData::default()),
        node_data_saver: generic_node_data_saver::<Mat3DiagData>,
        node_data_loader: generic_node_data_loader::<Mat3DiagData>,
    }
}
