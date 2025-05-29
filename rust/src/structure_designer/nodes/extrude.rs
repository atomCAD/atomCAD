use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use glam::Vec3Swizzles;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtrudeData {
  pub height: i32,
}

impl NodeData for ExtrudeData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

pub fn implicit_eval_extrude<'a>(
  evaluator: &ImplicitEvaluator,
  registry: &NodeTypeRegistry,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec3) -> f64 {
  let extrude_data = &node.data.as_any_ref().downcast_ref::<ExtrudeData>().unwrap();

  let y_val = f64::max(-sample_point.y, sample_point.y - (extrude_data.height as f64));

  let input_val = match node.arguments[0].get_node_id() {
    Some(node_id) => evaluator.implicit_eval_2d(
        network_stack,
        node_id, 
        &sample_point.xz(),
        registry)[0],
    None => f64::MAX
  };

  return f64::max(y_val, input_val);
}
