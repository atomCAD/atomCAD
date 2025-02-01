use glam::f32::Vec2;
use glam::f32::Vec3;
use std::collections::HashMap;
use std::collections::HashSet;

enum DataType {
  SDF,
  Atomic
}

enum NodeType {
  Cuboid,
  Union,
  Intersection,
  Negation,
  Diff,
  SDFFunction,
  AtomicFromSDF,
  EditAtomic,
  TransformAtomic
}

pub struct PinType {
  pub name: String,
  pub data_type: DataType,
}

pub struct NodeTypeInfo {
  pub node_type: NodeType,
  pub name: String,
  pub inputs: Vec<PinType>,
  pub outputs: Vec<PinType>,
}

pub struct Pin {
  pub other_node_id: u64,
  pub other_pin_index: usize,
}

pub struct Node {
  pub id: u64,
  pub node_type: NodeType,
  pub position: Vec2,
  pub inputs: Vec<Option<Pin>>,
  pub outputs: Vec<Option<Pin>>,
}

pub struct NodeNetwork {
  pub next_id: u64,
  pub node_types: HashMap<NodeType, NodeTypeInfo>,
  pub nodes: HashMap<u64, Node>,
}

impl NodeNetwork {

  pub fn new() -> Self {
    let ret = Self {
      next_id: 1,
      node_types: HashMap::new(),
      nodes: HashMap::new(),
    };

    let cuboid_type = NodeTypeInfo {
      node_type: NodeType::Cuboid,
      name: "Cuboid".to_string(),
      inputs: Vec::new(),
      outputs: Vec::new(),
    };

    ret
  }
}

/*
  Cuboid,
  Union,
  Intersection,
  Negation,
  Diff,
  SDFFunction,
  AtomicFromSDF,
  EditAtomic,
  TransformAtomic
*/