use std::collections::HashMap;

use flutter_rust_bridge::frb;

pub struct APIVec2 {
  pub x: f32,
  pub y: f32,
}

pub struct APIVec3 {
  pub x: f32,
  pub y: f32,
  pub z: f32,
}

pub struct APICamera {
  pub eye: APIVec3,
  pub target: APIVec3,
  pub up: APIVec3,
  pub aspect: f32,
  pub fovy: f32, // in radians
  pub znear: f32,
  pub zfar: f32,
}

pub struct InputPinView {
  pub name: String,
}

#[frb]
pub struct NodeView {
  pub id: u64,
  pub node_type_name: String,
  #[frb(non_final)]
  pub position: APIVec2,
  pub input_pins: Vec<InputPinView>,
}

pub struct WireView {
  pub source_node_id: u64,
  pub dest_node_id: u64,
  pub dest_param_index: usize,
}

pub struct NodeNetworkView {
  pub name: String,
  pub nodes: HashMap<u64, NodeView>,
  pub wires: Vec<WireView>,
}
