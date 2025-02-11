use glam::f32::Vec2;
use std::collections::HashMap;
use std::collections::HashSet;
use super::node_type::NodeType;
use super::node_type::NodeData;
use super::node_type::NoData;

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
}

impl NodeNetwork {

  pub fn new(node_type: NodeType) -> Self {
    let ret = Self {
      next_node_id: 1,
      node_type,
      nodes: HashMap::new(),
      return_node_id: None,
    };

    return ret;
  }

  pub fn add_node(&mut self, node_type_name: &str, position: Vec2) {
    let node = Node {
      id: self.next_node_id,
      node_type_name: node_type_name.to_string(),
      position,
      arguments: Vec::new(),
      data: Box::new(NoData{}),
    };
    self.next_node_id += 1;
    self.nodes.insert(node.id, node);
  }

  pub fn move_node(&mut self, node_id: u64, position: Vec2) {
    if let Some(node) = self.nodes.get_mut(&node_id) {
      node.position = position;
    }
  }
}
