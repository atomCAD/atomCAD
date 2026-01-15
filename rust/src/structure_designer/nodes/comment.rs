use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::text_format::TextValue;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::{NodeType, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentData {
    pub label: String,
    pub text: String,
    pub width: f64,
    pub height: f64,
}

impl Default for CommentData {
    fn default() -> Self {
        Self {
            label: String::new(),
            text: String::new(),
            width: 200.0,
            height: 100.0,
        }
    }
}

impl NodeData for CommentData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        _network_evaluator: &NetworkEvaluator,
        _network_stack: &Vec<NetworkStackElement<'a>>,
        _node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext
    ) -> NetworkResult {
        NetworkResult::None
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        if self.label.is_empty() {
            None
        } else {
            Some(self.label.clone())
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("label".to_string(), TextValue::String(self.label.clone())),
            ("text".to_string(), TextValue::String(self.text.clone())),
            ("width".to_string(), TextValue::Float(self.width)),
            ("height".to_string(), TextValue::Float(self.height)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("label") {
            self.label = v.as_string().ok_or_else(|| "label must be a string".to_string())?.to_string();
        }
        if let Some(v) = props.get("text") {
            self.text = v.as_string().ok_or_else(|| "text must be a string".to_string())?.to_string();
        }
        if let Some(v) = props.get("width") {
            self.width = v.as_float().ok_or_else(|| "width must be a float".to_string())?;
        }
        if let Some(v) = props.get("height") {
            self.height = v.as_float().ok_or_else(|| "height must be a float".to_string())?;
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "Comment".to_string(),
        description: "Add text annotations to document your node network.".to_string(),
        category: NodeTypeCategory::Annotation,
        parameters: vec![],
        output_type: DataType::None,
        public: true,
        node_data_creator: || Box::new(CommentData::default()),
        node_data_saver: generic_node_data_saver::<CommentData>,
        node_data_loader: generic_node_data_loader::<CommentData>,
    }
}
