use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec2;

pub fn implicit_eval_diff_2d<'a>(
  evaluator: &ImplicitEvaluator,
  registry: &NodeTypeRegistry,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec2) -> f64 {

  let ubase = node.arguments[0].argument_node_ids.iter().map(|node_id| {
    evaluator.implicit_eval_2d(network_stack, *node_id, sample_point, registry)[0]
  }).reduce(f64::min).unwrap_or(f64::MAX);

  let usub = node.arguments[1].argument_node_ids.iter().map(|node_id| {
    evaluator.implicit_eval_2d(network_stack, *node_id, sample_point, registry)[0]
  }).reduce(f64::min).unwrap_or(f64::MAX);

  return f64::max(ubase, -usub)
}
