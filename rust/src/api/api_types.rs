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

pub struct APIIVec3 {
  pub x: i32,
  pub y: i32,
  pub z: i32,
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
  pub data_type: String,
  pub multi: bool, 
}

#[frb]
pub struct NodeView {
  pub id: u64,
  pub node_type_name: String,
  #[frb(non_final)]
  pub position: APIVec2,
  pub input_pins: Vec<InputPinView>,
  pub output_type: String,
  pub selected: bool,
  pub displayed: bool,
}

pub struct WireView {
  pub source_node_id: u64,
  pub dest_node_id: u64,
  pub dest_param_index: usize,
  pub selected: bool,
}

pub struct NodeNetworkView {
  pub name: String,
  pub nodes: HashMap<u64, NodeView>,
  pub wires: Vec<WireView>,
}

pub struct APICuboidData {
  pub min_corner: APIIVec3,
  pub extent: APIIVec3,
}

pub struct APISphereData {
  pub center: APIIVec3,
  pub radius: i32,
}

pub struct APIHalfSpaceData {
  pub miller_index: APIIVec3,
  pub shift: i32,
}

pub struct APIGeoTransData {
  pub translation: APIIVec3,
  pub rotation: APIIVec3,
}
