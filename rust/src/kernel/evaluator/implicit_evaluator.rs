use glam::f32::Vec3;
use glam::f32::Quat;
use crate::kernel::node_network::NodeNetwork;
use crate::kernel::node_network::Node;
use crate::kernel::node_type::ParameterData;
use crate::kernel::node_type::SphereData;
use crate::kernel::node_type::CuboidData;
use crate::kernel::node_type::HalfSpaceData;
use crate::kernel::node_type::GeoTransData;
use crate::kernel::node_type_registry::NodeTypeRegistry;
use std::collections::HashMap;
use std::f32::consts::PI;

#[derive(Clone)]
pub struct NetworkStackElement<'a> {
  pub node_network: &'a NodeNetwork,
  pub node_id: u64,
}

fn eval_cuboid<'a>(
  _evaluator: &ImplicitEvaluator,
  _registry: &NodeTypeRegistry,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &Vec3) -> f32 {
  let cuboid_data = &node.data.as_any_ref().downcast_ref::<CuboidData>().unwrap();

  let max_corner = cuboid_data.min_corner + cuboid_data.extent;
  let x_val = f32::max((cuboid_data.min_corner.x as f32) - sample_point.x, sample_point.x - (max_corner.x as f32));
  let y_val = f32::max((cuboid_data.min_corner.y as f32) - sample_point.y, sample_point.y - (max_corner.y as f32));
  let z_val = f32::max((cuboid_data.min_corner.z as f32) - sample_point.z, sample_point.z - (max_corner.z as f32));

  return f32::max(f32::max(x_val, y_val), z_val);
}

fn eval_sphere<'a>(
  _evaluator: &ImplicitEvaluator,
  _registry: &NodeTypeRegistry,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &Vec3) -> f32 {
  let sphere_data = &node.data.as_any_ref().downcast_ref::<SphereData>().unwrap();

  return (sample_point - Vec3::new(sphere_data.center.x as f32, sphere_data.center.y as f32, sphere_data.center.z as f32)).length() 
    - (sphere_data.radius as f32);
}

fn eval_half_space<'a>(
  _evaluator: &ImplicitEvaluator,
  _registry: &NodeTypeRegistry,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &Vec3) -> f32 {
  let half_space_data = &node.data.as_any_ref().downcast_ref::<HalfSpaceData>().unwrap();
  let float_miller = half_space_data.miller_index.as_vec3();
  let miller_magnitude = float_miller.length();
  return (float_miller.dot(sample_point.clone()) - (half_space_data.shift as f32)) / miller_magnitude;
}

fn eval_geo_trans<'a>(evaluator: &ImplicitEvaluator,
    registry: &NodeTypeRegistry,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node: &Node,
    sample_point: &Vec3) -> f32 {
    let geo_trans_data = &node.data.as_any_ref().downcast_ref::<GeoTransData>().unwrap();
    let translation = geo_trans_data.translation.as_vec3();
    let rotation_euler = geo_trans_data.rotation.as_vec3() * PI * 0.5;

    let rotation_quat = Quat::from_euler(
        glam::EulerRot::XYX,
        rotation_euler.x, 
        rotation_euler.y, 
        rotation_euler.z);

    let transformed_point = rotation_quat.inverse().mul_vec3(sample_point - translation);

    match node.arguments[0].get_node_id() {
        Some(node_id) => evaluator.implicit_eval(
            network_stack,
            node_id, 
            &transformed_point,
            registry)[0],
        None => f32::MAX
    }
}

fn eval_union<'a>(
    evaluator: &ImplicitEvaluator,
    registry: &NodeTypeRegistry,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node: &Node,
    sample_point: &Vec3) -> f32 {
  node.arguments[0].argument_node_ids.iter().map(|node_id| {
    evaluator.implicit_eval(network_stack, *node_id, sample_point, registry)[0]
  }).reduce(f32::min).unwrap_or(f32::MAX)
}

fn eval_intersect<'a>(
  evaluator: &ImplicitEvaluator,
  registry: &NodeTypeRegistry,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &Vec3) -> f32 {
    node.arguments[0].argument_node_ids.iter().map(|node_id| {
      evaluator.implicit_eval(network_stack, *node_id, sample_point, registry)[0]
    }).reduce(f32::max).unwrap_or(f32::MIN)
}

fn eval_diff<'a>(
  evaluator: &ImplicitEvaluator,
  registry: &NodeTypeRegistry,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &Vec3) -> f32 {

  let ubase = node.arguments[0].argument_node_ids.iter().map(|node_id| {
    evaluator.implicit_eval(network_stack, *node_id, sample_point, registry)[0]
  }).reduce(f32::min).unwrap_or(f32::MAX);

  let usub = node.arguments[1].argument_node_ids.iter().map(|node_id| {
    evaluator.implicit_eval(network_stack, *node_id, sample_point, registry)[0]
  }).reduce(f32::min).unwrap_or(f32::MAX);

  return f32::max(ubase, -usub)
}

/*
 * Implicit evaluator.
 * The implicit evaluator is able to evaluate nodes in a node network which are implicit geometry functions.
 * It does this by treating the abstract operators (nodes) in the node network as implicit geometry functions. 
 */
pub struct ImplicitEvaluator {
    built_in_functions: HashMap<String,fn(&ImplicitEvaluator, &NodeTypeRegistry, &Vec<NetworkStackElement>, &Node, &Vec3) -> f32>,
}

impl ImplicitEvaluator {
    pub fn new() -> Self {
        let mut ret = Self {
          built_in_functions: HashMap::new(),    
        };
    
        ret.built_in_functions.insert("cuboid".to_string(), eval_cuboid);
        ret.built_in_functions.insert("sphere".to_string(), eval_sphere);
        ret.built_in_functions.insert("half_space".to_string(), eval_half_space);
        ret.built_in_functions.insert("union".to_string(), eval_union);
        ret.built_in_functions.insert("intersect".to_string(), eval_intersect);
        ret.built_in_functions.insert("diff".to_string(), eval_diff);
        ret.built_in_functions.insert("geo_trans".to_string(), eval_geo_trans);
    
        return ret;
    }

    pub fn get_gradient(&self, network: &NodeNetwork, node_id: u64, sample_point: &Vec3, registry: &NodeTypeRegistry) -> Vec3 {
        let epsilon = 0.001; // Small value for finite difference approximation
        
        let mut network_stack = Vec::new();
        // We assign the root node network zero node id. It is not used in the evaluation.
        network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });
    
        // Calculate partial derivatives using central differences
        let dx = (
          self.implicit_eval(&network_stack, node_id, &(sample_point + Vec3::new(epsilon, 0.0, 0.0)), registry)[0] -
          self.implicit_eval(&network_stack, node_id, &(sample_point - Vec3::new(epsilon, 0.0, 0.0)), registry)[0]
        ) / (2.0 * epsilon);
        
        let dy = (
          self.implicit_eval(&network_stack, node_id, &(sample_point + Vec3::new(0.0, epsilon, 0.0)), registry)[0] -
          self.implicit_eval(&network_stack, node_id, &(sample_point - Vec3::new(0.0, epsilon, 0.0)), registry)[0]
        ) / (2.0 * epsilon);
        
        let dz = (
          self.implicit_eval(&network_stack, node_id, &(sample_point + Vec3::new(0.0, 0.0, epsilon)), registry)[0] -
          self.implicit_eval(&network_stack, node_id, &(sample_point - Vec3::new(0.0, 0.0, epsilon)), registry)[0]
        ) / (2.0 * epsilon);
    
        let gradient = Vec3::new(dx, dy, dz);
        
        // Normalize the gradient vector
        if gradient.length_squared() > 0.0 {
          gradient.normalize()
        } else {
          gradient
        }
    }

    pub fn eval(&self, network: &NodeNetwork, node_id: u64, sample_point: &Vec3, registry: &NodeTypeRegistry) -> Vec<f32> {
        let mut network_stack = Vec::new();
        // We assign the root node network zero node id. It is not used in the evaluation.
        network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });
        return self.implicit_eval(&network_stack, node_id, sample_point, registry);
    }

  /*
   * This is a naive but relatively simple way to evaluate the implicit function. We need this now
   * for rapid development and later to have a correct reference implementation.
   * Future possible optimizations:
   * - Do not refer to node types by string: use an internal id
   * - Do not do this recursion per sampled point, but do it for a cubic array at a time, and work with
   * cubic array of f32 values at once.
   * - Do not sample everywhere. If we know the max gradient length we can infer that there is no sign change in big ranges.
   * - Ultimatly to achieve very high performance we can consider generating GPU code so that evaluation can be done
   * per sampled point again, but massively paralelly in compute shader using generated GPU shader code.
   * The GPU compute shader needs to be regenerated on node network edit operations though, the cost of which
   * needs to be investigated. If partial recompilation of shader code is possible that would be a huge win.
   * Not all optimizations fit all use cases or even compatible with each other, so we might use multiple approaches
   * in different cases.
   */
  fn implicit_eval<'a>(&self, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, sample_point: &Vec3, registry: &NodeTypeRegistry) -> Vec<f32> {
    let node = network_stack.last().unwrap().node_network.nodes.get(&node_id).unwrap();

    if node.node_type_name == "parameter" {
      let parent_node_id = network_stack.last().unwrap().node_id;

      let param_data = &(*node.data).as_any_ref().downcast_ref::<ParameterData>().unwrap();
      let mut parent_network_stack = network_stack.clone();
      parent_network_stack.pop();
      let parent_node = parent_network_stack.last().unwrap().node_network.nodes.get(&parent_node_id).unwrap();
      let args : Vec<Vec<f32>> = parent_node.arguments[param_data.param_index].argument_node_ids.iter().map(|&arg_node_id| {
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
