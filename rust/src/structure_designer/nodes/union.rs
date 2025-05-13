use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec3;

pub fn implicit_eval_union<'a>(
  evaluator: &ImplicitEvaluator,
  registry: &NodeTypeRegistry,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec3) -> f64 {
node.arguments[0].argument_node_ids.iter().map(|node_id| {
  evaluator.implicit_eval(network_stack, *node_id, sample_point, registry)[0]
  }).reduce(f64::min).unwrap_or(f64::MAX)
}
