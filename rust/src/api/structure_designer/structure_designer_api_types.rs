use flutter_rust_bridge::frb;
use crate::api::common_api_types::APIVec2;
use crate::api::common_api_types::APIIVec3;
use crate::api::common_api_types::APIVec3;
use crate::api::common_api_types::APITransform;
use std::collections::HashMap;

#[derive(Clone)]
pub enum APIEditAtomTool {
  Default,
  AddAtom,
  AddBond,
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
    pub return_node: bool,
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
  
  pub struct APIGeoToAtomData {
    pub primary_atomic_number: i32,
    pub secondary_atomic_number: i32,
  }

  pub struct APIAtomTransData {
    pub translation: APIVec3,
    pub rotation: APIVec3, // intrinsic euler angles in radians
  }

  pub struct APIEditAtomData {
    pub active_tool: APIEditAtomTool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub bond_tool_last_atom_id: Option<u64>,
    pub replacement_atomic_number: Option<i32>,
    pub add_atom_tool_atomic_number: Option<i32>,
    pub has_selected_atoms: bool,
    pub has_selection: bool,
    pub selection_transform: Option<APITransform>,
  }

  pub struct APIAnchorData {
    pub position: Option<APIIVec3>,
  }

  #[derive(Clone, Debug)]
  pub struct APICrystalTypeInfo {
    pub primary_atomic_number: i32,
    pub secondary_atomic_number: i32,
    pub unit_cell_size: f64,
    pub name: String,
  }

  pub struct APIStampPlacement {
    pub position: APIIVec3,
    pub x_dir: i32, // +x, -x, +y, -y, +z, -z: 6 possibilities
    pub y_dir: i32, // 4 possibilities
  }

  pub struct APIStampView {
    pub selected_stamp_placement: Option<APIStampPlacement>,
  }
