use flutter_rust_bridge::frb;
use crate::api::common_api_types::APIVec2;
use crate::api::common_api_types::APIIVec2;
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

#[derive(PartialEq)]
pub enum APIDataTypeBase {
  None,
  Bool,
  String,
  Int,
  Float,
  Vec2,
  Vec3,
  IVec2,
  IVec3,
  UnitCell,
  Geometry2D,
  Geometry,
  Atomic,
  Motif,
  Custom
 }
 
 pub struct APIDataType {
   pub data_type_base: APIDataTypeBase,
   pub custom_data_type: Option<String>, // Not None if and only if data_type_base == APIDataTypeBase::Custom
   pub array: bool, // combined with built_in_data_type, but only redundant with custom_data_type as the outermost array is within the string in that case.
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
    pub function_type: String,
    pub selected: bool,
    pub displayed: bool,
    pub return_node: bool,
    pub error: Option<String>,
    pub output_string: Option<String>,
    pub subtitle: Option<String>,
  }
  
  pub struct WireView {
    pub source_node_id: u64,
    pub source_output_pin_index: i32,
    pub dest_node_id: u64,
    pub dest_param_index: usize,
    pub selected: bool,
  }
  
  pub struct NodeNetworkView {
    pub name: String,
    pub nodes: HashMap<u64, NodeView>,
    pub wires: Vec<WireView>,
  }

  pub struct APIIntData {
    pub value: i32,
  }

  pub struct APIStringData {
    pub value: String,
  }

  pub struct APIBoolData {
    pub value: bool,
  }

  pub struct APIFloatData {
    pub value: f64,
  }

  pub struct APIIVec2Data {
    pub value: APIIVec2,
  }
  
  pub struct APIIVec3Data {
    pub value: APIIVec3,
  }

  pub struct APIRangeData {
    pub start: i32,
    pub step: i32,
    pub count: i32,
  }

  pub struct APIVec2Data {
    pub value: APIVec2,
  }
  
  pub struct APIVec3Data {
    pub value: APIVec3,
  }

  pub struct APIRectData {
    pub min_corner: APIIVec2,
    pub extent: APIIVec2,
  }
  
  pub struct APICircleData {
    pub center: APIIVec2,
    pub radius: i32,
  }

  pub struct APIExtrudeData {
    pub height: i32,
  }

  pub struct APICuboidData {
    pub min_corner: APIIVec3,
    pub extent: APIIVec3,
  }
  
  pub struct APISphereData {
    pub center: APIIVec3,
    pub radius: i32,
  }
  
  pub struct APIHalfPlaneData {
    pub point1: APIIVec2,
    pub point2: APIIVec2,
  }

  pub struct APIHalfSpaceData {
    pub max_miller_index: i32,
    pub miller_index: APIIVec3,
    pub center: APIIVec3,
    pub shift: i32,
  }

  pub struct APIFacet {
    pub miller_index: APIIVec3,
    pub shift: i32,
    pub symmetrize: bool,
    pub visible: bool,
  }

  pub struct APIFacetShellData {
    pub max_miller_index: i32,
    pub center: APIIVec3,
    pub facets: Vec<APIFacet>,
    pub selected_facet_index: Option<usize>,
  }

  pub struct APIGeoTransData {
    pub translation: APIIVec3,
    pub rotation: APIIVec3,
    pub transform_only_frame: bool,
  }
  

  pub struct APIRotationalSymmetry {
    pub axis: APIVec3,
    pub n_fold: u32,
  }

  pub struct APILatticeSymopData {
    pub translation: APIIVec3,
    pub rotation_axis: Option<APIVec3>,
    pub rotation_angle_degrees: f64,
    pub transform_only_frame: bool,
    pub rotational_symmetries: Vec<APIRotationalSymmetry>,
    pub crystal_system: String,
  }

  pub struct APILatticeMoveData {
    pub translation: APIIVec3,
  }

  pub struct APILatticeRotData {
    pub axis_index: Option<i32>,
    pub step: i32,
    pub pivot_point: APIIVec3,
    pub rotational_symmetries: Vec<APIRotationalSymmetry>,
    pub crystal_system: String,
  }

  pub struct APIAtomTransData {
    pub translation: APIVec3,
    pub rotation: APIVec3, // intrinsic euler angles in radians
  }

  pub struct APIEditAtomData {
    pub active_tool: APIEditAtomTool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub bond_tool_last_atom_id: Option<u32>,
    pub replacement_atomic_number: Option<i16>,
    pub add_atom_tool_atomic_number: Option<i16>,
    pub has_selected_atoms: bool,
    pub has_selection: bool,
    pub selection_transform: Option<APITransform>,
  }

  pub struct APIRegPolyData {
    pub num_sides: i32,
    pub radius: i32,
  }

  pub struct APIParameterData {
    pub param_index: usize,
    pub param_name: String,
    pub data_type: APIDataType,
    pub sort_order: i32,
    pub error: Option<String>,
  }

  pub struct APINetworkWithValidationErrors {
    pub name: String,
    pub validation_errors: Option<String>,
  }

pub struct APIExprParameter {
  pub name: String,
  pub data_type: APIDataType,
}

pub struct APIExprData {
  pub parameters: Vec<APIExprParameter>,
  pub expression: String,
  pub error: Option<String>,
  pub output_type: Option<APIDataType>,
}

pub struct APIImportXYZData {
  pub file_name: Option<String>,
}

pub struct APIExportXYZData {
  pub file_name: String,
}

pub struct APIAtomCutData {
  pub cut_sdf_value: f64,
  pub unit_cell_size: f64,
}

pub struct APIMapData {
  pub input_type: APIDataType,
  pub output_type: APIDataType,
}

pub struct APIUnitCellData {
  pub cell_length_a: f64,
  pub cell_length_b: f64,
  pub cell_length_c: f64,
  pub cell_angle_alpha: f64, // in degrees
  pub cell_angle_beta: f64,  // in degrees
  pub cell_angle_gamma: f64, // in degrees
  pub crystal_system: String,
}

pub struct APIMotifData {
  pub definition: String, // The motif definition text
  pub name: Option<String>, // Optional name for the motif
  pub error: Option<String>, // Optional error message from parsing
}

#[flutter_rust_bridge::frb]
pub struct APIAtomFillData {
  pub parameter_element_value_definition: String, // The parameter element value definition text
  pub motif_offset: APIVec3, // Offset in fractional lattice coordinates
  pub hydrogen_passivation: bool, // Whether to apply hydrogen passivation
  pub remove_single_bond_atoms_before_passivation: bool, // Whether to remove atoms with exactly one bond before passivation
  pub surface_reconstruction: bool, // Whether to apply surface reconstruction
  pub error: Option<String>, // Optional error message from parsing
}
















