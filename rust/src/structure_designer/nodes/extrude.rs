use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::DQuat;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_result::input_missing_error;
use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::structure_designer::evaluator::network_result::GeometrySummary;
use crate::util::transform::Transform;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::node_type::NodeType;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtrudeData {
  pub height: i32,
}

impl NodeData for ExtrudeData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
      None
    }

    fn eval<'a>(
      &self,
      network_evaluator: &NetworkEvaluator,
      network_stack: &Vec<NetworkStackElement<'a>>,
      node_id: u64,
      registry: &NodeTypeRegistry,
      _decorate: bool,
      context: &mut NetworkEvaluationContext,
    ) -> NetworkResult {
      //let _timer = Timer::new("eval_extrude");
      let node = NetworkStackElement::get_top_node(network_stack, node_id);
      let shape_input_name = registry.get_parameter_name(&node, 0);
    
      if node.arguments[0].is_empty() {
        return input_missing_error(&shape_input_name);
      }
    
      let input_node_id = node.arguments[0].get_node_id().unwrap();
      let shape_val = network_evaluator.evaluate(
        network_stack,
        input_node_id,
        0,
        registry,
        false,
        context
      );
    
      if let NetworkResult::Error(_error) = shape_val {
        return error_in_input(&shape_input_name);
      }
      if let NetworkResult::Geometry2D(shape) = shape_val {
        let frame_translation_2d = shape.frame_transform.translation;
    
        let frame_transform = Transform::new(
          DVec3::new(frame_translation_2d.x, 0.0, frame_translation_2d.y),
          DQuat::from_rotation_y(shape.frame_transform.rotation),
        );
    
        let s = shape.geo_tree_root;
        return NetworkResult::Geometry(GeometrySummary { 
          frame_transform,
          geo_tree_root: GeoNode::Extrude { 
            height: self.height,
            shape: Box::new(s),
          },
        });
      } else {
        return error_in_input(&shape_input_name);
      }
    }
}



