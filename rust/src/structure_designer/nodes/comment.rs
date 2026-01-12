use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_result::NetworkResult;
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
