use glam::f32::Vec2;
use glam::i32::IVec3;
use std::collections::HashMap;
use std::collections::HashSet;
use super::node_type::NodeType;
use super::node_type::NodeData;
use super::node_type::NoData;
use super::node_type::SphereData;
use super::node_type::CuboidData;
use super::node_type::HalfSpaceData;
use super::node_type_registry::NodeTypeRegistry;

pub struct Argument {
  // A set of argument values as parameters can have the 'multiple' flag set.
  pub argument_node_ids: HashSet<u64>, // Set of node ids for which the output is referenced
}

pub struct Node {
  pub id: u64,
  pub node_type_name: String,
  pub position: Vec2,
  pub arguments: Vec<Argument>,
  pub data: Box<dyn NodeData>,
}

/*
 * A node network is a network of nodes used by users to create geometries and atomic structures.
 * A node network can also be an implementation of a non-built-in node type.
 * In this case it might or might not have parameters.
 */
pub struct NodeNetwork {
  pub next_node_id: u64,
  pub node_type: NodeType, // This is the node type when this node network is used as a node in another network. (analog to a function header in programming)
  pub nodes: HashMap<u64, Node>,
  pub return_node_id: Option<u64>, // Only node networks with a return node can be used as a node (a.k.a can be called)
  pub displayed_node_ids: HashSet<u64>, // Set of nodes that are currently displayed
}

impl NodeNetwork {

  pub fn new(node_type: NodeType) -> Self {
    let ret = Self {
      next_node_id: 1,
      node_type,
      nodes: HashMap::new(),
      return_node_id: None,
      displayed_node_ids: HashSet::new(),
    };

    return ret;
  }

  pub fn add_node(&mut self, node_type_name: &str, position: Vec2, num_of_parameters: usize) -> u64 {
    let node_id = self.next_node_id;
    let mut arguments: Vec<Argument> = Vec::new();
    for _i in 0..num_of_parameters {
      arguments.push(Argument { argument_node_ids: HashSet::new() });
    }
    
    // Create default node data based on node type
    let default_data: Box<dyn NodeData> = match node_type_name {
      "sphere" => Box::new(SphereData {
        center: IVec3::new(0, 0, 0),
        radius: 1,
      }),
      "cuboid" => Box::new(CuboidData {
        min_corner: IVec3::new(-1, -1, -1),
        extent: IVec3::new(2, 2, 2),
      }),
      "half_space" => Box::new(HalfSpaceData {
        miller_index: IVec3::new(1, 0, 0), // Default normal along x-axis
        shift: 0,
      }),
      _ => Box::new(NoData{}),
    };

    let node = Node {
      id: node_id,
      node_type_name: node_type_name.to_string(),
      position,
      arguments,
      data: default_data,
    };
    
    self.next_node_id += 1;
    self.nodes.insert(node_id, node);
    return node_id;
  }

  pub fn move_node(&mut self, node_id: u64, position: Vec2) {
    if let Some(node) = self.nodes.get_mut(&node_id) {
      node.position = position;
    }
  }

  pub fn connect_nodes(&mut self, source_node_id: u64, dest_node_id: u64, dest_param_index: usize, dest_param_is_multi: bool) {
    if let Some(dest_node) = self.nodes.get_mut(&dest_node_id) {
      let argument = &mut dest_node.arguments[dest_param_index];
      // In case of single parameters we need to disconnect the existing parameter first
      if (!dest_param_is_multi) && (!argument.argument_node_ids.is_empty()) {
        argument.argument_node_ids.clear();
      }
      argument.argument_node_ids.insert(source_node_id);
    }
  }

  pub fn set_node_network_data(&mut self, node_id: u64, data: Box<dyn NodeData>) {
    if let Some(node) = self.nodes.get_mut(&node_id) {
      node.data = data;
    }
  }

  pub fn set_node_display(&mut self, node_id: u64, is_displayed: bool) {
    if self.nodes.contains_key(&node_id) {
      if is_displayed {
        self.displayed_node_ids.insert(node_id);
      } else {
        self.displayed_node_ids.remove(&node_id);
      }
    }
  }
}
