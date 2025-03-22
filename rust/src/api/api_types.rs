use std::collections::HashMap;

use flutter_rust_bridge::frb;

pub enum Editor {
  None,
  StructureDesigner,
  SceneComposer
}

pub enum SelectModifier {
  Replace,
  Toggle,
  Expand
}

pub struct APIVec2 {
  pub x: f64,
  pub y: f64,
}

pub struct APIVec3 {
  pub x: f64,
  pub y: f64,
  pub z: f64,
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
  pub aspect: f64,
  pub fovy: f64, // in radians
  pub znear: f64,
  pub zfar: f64,
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
  pub transform_only_frame: bool,
}

pub struct APIAtomTransData {
  pub translation: APIVec3,
  pub rotation: APIVec3, // intrinsic euler angles in radians
}
