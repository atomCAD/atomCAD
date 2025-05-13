use glam::f64::DVec3;
use crate::structure_designer::node_network::NodeNetwork;
use crate::structure_designer::node_network::Node;
use crate::structure_designer::nodes::parameter::ParameterData;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use std::collections::HashMap;
use crate::structure_designer::nodes::cuboid::implicit_eval_cuboid;
use crate::structure_designer::nodes::sphere::implicit_eval_sphere;
use crate::structure_designer::nodes::half_space::implicit_eval_half_space;
use crate::structure_designer::nodes::geo_trans::implicit_eval_geo_trans;
use crate::structure_designer::nodes::union::implicit_eval_union;
use crate::structure_designer::nodes::intersect::implicit_eval_intersect;
use crate::structure_designer::nodes::diff::implicit_eval_diff;

#[derive(Clone)]
pub struct NetworkStackElement<'a> {
  pub node_network: &'a NodeNetwork,
  pub node_id: u64,
}

impl<'a> NetworkStackElement<'a> {
  pub fn get_top_node(network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64) -> &'a Node {
    return network_stack.last().unwrap().node_network.nodes.get(&node_id).unwrap();
  }
}

/*
 * Implicit evaluator.
 * The implicit evaluator is able to evaluate nodes in a node network which are implicit geometry functions.
 * It does this by treating the abstract operators (nodes) in the node network as implicit geometry functions. 
 */
pub struct ImplicitEvaluator {
    built_in_functions: HashMap<String,fn(&ImplicitEvaluator, &NodeTypeRegistry, &Vec<NetworkStackElement>, &Node, &DVec3) -> f64>,
}

impl ImplicitEvaluator {
    pub fn new() -> Self {
        let mut ret = Self {
          built_in_functions: HashMap::new(),    
        };
    
        ret.built_in_functions.insert("cuboid".to_string(), implicit_eval_cuboid);
        ret.built_in_functions.insert("sphere".to_string(), implicit_eval_sphere);
        ret.built_in_functions.insert("half_space".to_string(), implicit_eval_half_space);
        ret.built_in_functions.insert("union".to_string(), implicit_eval_union);
        ret.built_in_functions.insert("intersect".to_string(), implicit_eval_intersect);
        ret.built_in_functions.insert("diff".to_string(), implicit_eval_diff);
        ret.built_in_functions.insert("geo_trans".to_string(), implicit_eval_geo_trans);
    
        return ret;
    }

    // Calculate gradient using one sided differences
    // This is faster than using central differences but potentially less accurate
    // It also returns the value at the sampled point, so that the value can be reused. 
    pub fn get_gradient(&self, network: &NodeNetwork, node_id: u64, sample_point: &DVec3, registry: &NodeTypeRegistry) -> (DVec3, f64) {
      let epsilon: f64 = 0.001; // Small value for finite difference approximation

      let value = self.eval(&network, node_id, sample_point, registry)[0];
      let gradient = DVec3::new(
        (self.eval(&network, node_id, &(sample_point + DVec3::new(epsilon, 0.0, 0.0)), registry)[0] - value) / epsilon,
        (self.eval(&network, node_id, &(sample_point + DVec3::new(0.0, epsilon, 0.0)), registry)[0] - value) / epsilon,
        (self.eval(&network, node_id, &(sample_point + DVec3::new(0.0, 0.0, epsilon)), registry)[0] - value) / epsilon
      );
      (gradient, value)
    }

    pub fn eval(&self, network: &NodeNetwork, node_id: u64, sample_point: &DVec3, registry: &NodeTypeRegistry) -> Vec<f64> {
        let mut network_stack = Vec::new();
        // We assign the root node network zero node id. It is not used in the evaluation.
        network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });

        return self.implicit_eval(&network_stack, node_id, sample_point, registry);
    }

  /*
   * Future possible optimizations:
   * - Do not refer to node types by string: use an internal id
   * - Do not do this recursion per sampled point, but do it for a cubic array at a time, and work with
   * cubic array of f32 values at once.
   * - Ultimatly to achieve very high performance we can consider generating GPU code so that evaluation can be done
   * massively paralelly in compute shader using generated GPU shader code.
   * The GPU compute shader needs to be regenerated on node network edit operations though, the cost of which
   * needs to be investigated. If partial recompilation of shader code is possible that would be a huge win.
   * Not all optimizations fit all use cases or even compatible with each other, so we might use multiple approaches
   * in different cases.
   */
  pub fn implicit_eval<'a>(&self, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, sample_point: &DVec3, registry: &NodeTypeRegistry) -> Vec<f64> {
    let node = network_stack.last().unwrap().node_network.nodes.get(&node_id).unwrap();

    if node.node_type_name == "parameter" {
      let parent_node_id = network_stack.last().unwrap().node_id;

      let param_data = &(*node.data).as_any_ref().downcast_ref::<ParameterData>().unwrap();
      let mut parent_network_stack = network_stack.clone();
      parent_network_stack.pop();
      let parent_node = parent_network_stack.last().unwrap().node_network.nodes.get(&parent_node_id).unwrap();
      let args : Vec<Vec<f64>> = parent_node.arguments[param_data.param_index].argument_node_ids.iter().map(|&arg_node_id| {
        self.implicit_eval(&parent_network_stack, arg_node_id, sample_point, registry)
      }).collect();
      return args.concat();
    }
    if let Some(built_in_function) = self.built_in_functions.get(&node.node_type_name) {
      let ret = built_in_function(self, registry, network_stack, node, sample_point);
      return vec![ret];
    }
    if let Some(child_network) = registry.node_networks.get(&node.node_type_name) {
      let mut child_network_stack = network_stack.clone();
      child_network_stack.push(NetworkStackElement { node_network: child_network, node_id });
      return self.implicit_eval(&child_network_stack, child_network.return_node_id.unwrap(), sample_point, registry);
    }
    return vec![0.0];
  }

}
