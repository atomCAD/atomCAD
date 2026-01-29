use crate::structure_designer::{node_network::NodeNetwork, node_type::NodeType};
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::node_data::NoData;
use crate::structure_designer::node_type::no_data_saver;
use crate::structure_designer::node_type::no_data_loader;
use crate::structure_designer::evaluator::network_result::Closure;
use crate::structure_designer::nodes::value::ValueData;
use glam::f64::DVec2;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;

/*
 * FunctionEvaluator is capable of evaluating a function, more precisely evaluating a closure.
 * The type of the function needs to be known at construction time,
 * and the constructed FunctionEvaluator instance can be reused for multiple evaluations
 * where the actual arguments can change.
 * Construction of the FunctionEvaluator instance is somewhat expensive,
 * but changing only the argument values is relativey cheap,
 * so a constructed FunctionEvaluator should be reused as much as possible.
 * 
 * Internally the FunctionEvaluator is a bit of a hack now: it builds a little node network so that nodes
 * can be evaluated in a node network context.
 * In the future this could be refactored only by rethinking the evaluation philosophy of the
 * whole node network: if the evaluation would work by a framework evaluating the input
 * pins of a node and the node would only get the input values, nodes could be evaluated
 * without building a node network first. We need to think thorugh though: is it not too
 * restricting? Are there nodes which need to decide which input pins they evaluate and how many times?
 */
pub struct FunctionEvaluator {
  node_network: NodeNetwork,
  main_node_id: u64,
  value_node_ids: Vec<u64>,
}

impl FunctionEvaluator {
  pub fn new(closure: Closure, registry: &NodeTypeRegistry) -> Self {
    let mut ret = Self {
      node_network: NodeNetwork::new(NodeType {
        name: "_tmp_".to_string(),
        description: "".to_string(),
        summary: None,
        category: crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory::OtherBuiltin,
        parameters: Vec::new(),
        output_type: DataType::None,
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
        public: false,
      }),
      value_node_ids: Vec::new(),
      main_node_id: 0,
    };
    // Add the main node.

    let network = match registry.node_networks.get(&closure.node_network_name) {
      Some(network) => network,
      None => return ret,
    };

    let node = match network.nodes.get(&closure.node_id) {
      Some(node) => node,
      None => return ret,
    };

    let node_data = network.get_node_network_data(closure.node_id);

    //TODO: pass custom node type too.
    let cloned_node_data = match node_data {
      Some(data) => Some(data.clone_box()),
      None => None,
    };
    
    let main_node_id = ret.node_network.add_node(
      &node.node_type_name, 
      DVec2::new(0.0, 0.0), 
      closure.captured_argument_values.len(), 
      cloned_node_data.unwrap_or_else(|| Box::new(crate::structure_designer::node_data::NoData {}))
    );

    // Add value nodes corresponding to parameters.
    for i in 0..closure.captured_argument_values.len() {
      let value = closure.captured_argument_values[i].clone();
      let node_id = ret.node_network.add_node(
        "value", 
        DVec2::new(0.0, 0.0), 
        0, 
        Box::new(ValueData { value }));
      ret.value_node_ids.push(node_id);
      ret.node_network.connect_nodes(node_id, 0, main_node_id, i, false);
    }
    ret.main_node_id = main_node_id;
    ret
  }

  pub fn set_argument_value(&mut self, arg_index: usize, value: NetworkResult) {
    let node_id = self.value_node_ids[arg_index];
    self.node_network.set_node_network_data(node_id, Box::new(ValueData { value }));
  }

  pub fn evaluate(
    &self,
    evaluator: &NetworkEvaluator,
    registry: &NodeTypeRegistry) -> NetworkResult {

      let mut network_stack = Vec::new();
      // We assign the root node network zero node id. It is not used in the evaluation.
      network_stack.push(NetworkStackElement { node_network: &self.node_network, node_id: 0 });

      // TODO: think about whether the context is ok this way?
      evaluator.evaluate(
        &network_stack,
        self.main_node_id,
        0,
        registry,
        false,
        &mut NetworkEvaluationContext::new())
  }
}
















