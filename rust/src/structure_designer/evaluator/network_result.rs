use glam::i32::IVec2;
use glam::i32::IVec3;
use glam::f64::DVec2;
use glam::f64::DVec3;
use crate::common::atomic_structure::AtomicStructure;
use crate::util::transform::Transform;
use crate::util::transform::Transform2D;
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;

#[derive(Debug, Clone)]
pub struct UnitCellStruct {
  pub a: DVec3,
  pub b: DVec3,
  pub c: DVec3,
}

impl UnitCellStruct {
  /// Creates a cubic diamond unit cell using the standard diamond lattice parameter
  /// 
  /// Returns a UnitCellStruct with orthogonal basis vectors aligned with the coordinate axes,
  /// each with length equal to the diamond unit cell size (3.567 Ångströms).
  pub fn cubic_diamond() -> Self {
    let size = DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
    UnitCellStruct {
      a: DVec3::new(size, 0.0, 0.0),
      b: DVec3::new(0.0, size, 0.0),
      c: DVec3::new(0.0, 0.0, size),
    }
  }

  /// Converts lattice coordinates to real space coordinates using the unit cell basis vectors.
  /// 
  /// # Arguments
  /// * `lattice_pos` - Position in lattice coordinates as DVec3
  /// 
  /// # Returns
  /// Position in real space coordinates as DVec3
  pub fn dvec3_lattice_to_real(&self, lattice_pos: &DVec3) -> DVec3 {
    lattice_pos.x * self.a + lattice_pos.y * self.b + lattice_pos.z * self.c
  }

  /// Converts lattice coordinates to real space coordinates using the unit cell basis vectors.
  /// 
  /// # Arguments
  /// * `lattice_pos` - Position in lattice coordinates as IVec3
  /// 
  /// # Returns
  /// Position in real space coordinates as DVec3
  pub fn ivec3_lattice_to_real(&self, lattice_pos: &IVec3) -> DVec3 {
    self.dvec3_lattice_to_real(&lattice_pos.as_dvec3())
  }

  pub fn dvec2_lattice_to_real(&self, lattice_pos: &DVec2) -> DVec2 {
    (lattice_pos.x * self.a + lattice_pos.y * self.b).truncate()
  }

  pub fn ivec2_lattice_to_real(&self, lattice_pos: &IVec2) -> DVec2 {
    self.dvec2_lattice_to_real(&lattice_pos.as_dvec2())
  }

  pub fn float_lattice_to_real(&self, lattice_value: f64) -> f64 {
    lattice_value * self.a.length()
  }

  pub fn int_lattice_to_real(&self, lattice_value: i32) -> f64 {
    self.float_lattice_to_real(lattice_value as f64)
  }
}

#[derive(Clone)]
pub struct GeometrySummary2D {
  pub unit_cell: UnitCellStruct,
  pub frame_transform: Transform2D,
  pub geo_tree_root: GeoNode,
}

#[derive(Clone)]
pub struct GeometrySummary {
  pub unit_cell: UnitCellStruct,
  pub frame_transform: Transform,
  pub geo_tree_root: GeoNode,
}

#[derive(Clone)]
pub struct Closure {
  pub node_network_name: String,
  pub node_id: u64,
  pub captured_argument_values: Vec<NetworkResult>,
}

#[derive(Clone)]
pub enum NetworkResult {
  None, // Always equivalent with no input pin connected
  Bool(bool),
  String(String),
  Int(i32),
  Float(f64),
  Vec2(DVec2),
  Vec3(DVec3),
  IVec2(IVec2),
  IVec3(IVec3),
  UnitCell(UnitCellStruct),
  Geometry2D(GeometrySummary2D),
  Geometry(GeometrySummary),
  Atomic(AtomicStructure),
  Array(Vec<NetworkResult>),
  Function(Closure), 
  Error(String),
}

impl Default for NetworkResult {
  fn default() -> Self {
    NetworkResult::None
  }
}

impl NetworkResult {

  /// Returns true if this NetworkResult is an Error variant
  pub fn is_error(&self) -> bool {
    matches!(self, NetworkResult::Error(_))
  }

  /// If this is an Error variant, returns it. Otherwise returns None.
  /// Useful for early error propagation in node evaluation.
  pub fn propagate_error(self) -> Option<NetworkResult> {
    match self {
      NetworkResult::Error(_) => Some(self),
      _ => None,
    }
  }

  /// Extracts an UnitCellStruct value from the NetworkResult, returns None if not a UnitCell
  pub fn extract_unit_cell(self) -> Option<UnitCellStruct> {
    match self {
      NetworkResult::UnitCell(uc) => Some(uc),
      _ => None,
    }
  }

  /// Extracts an IVec3 value from the NetworkResult, returns None if not an IVec3
  pub fn extract_ivec3(self) -> Option<IVec3> {
    match self {
      NetworkResult::IVec3(vec) => Some(vec),
      _ => None,
    }
  }

  /// Extracts an IVec2 value from the NetworkResult, returns None if not an IVec2
  pub fn extract_ivec2(self) -> Option<IVec2> {
    match self {
      NetworkResult::IVec2(vec) => Some(vec),
      _ => None,
    }
  }

  /// Extracts a String value from the NetworkResult, returns None if not a String
  pub fn extract_string(self) -> Option<String> {
    match self {
      NetworkResult::String(value) => Some(value),
      _ => None,
    }
  }

  /// Extracts a Bool value from the NetworkResult, returns None if not a Bool
  pub fn extract_bool(self) -> Option<bool> {
    match self {
      NetworkResult::Bool(value) => Some(value),
      _ => None,
    }
  }

  /// Extracts an Int value from the NetworkResult, returns None if not an Int
  pub fn extract_int(self) -> Option<i32> {
    match self {
      NetworkResult::Int(value) => Some(value),
      _ => None,
    }
  }

  /// Extracts a Float value from the NetworkResult, returns None if not a Float
  pub fn extract_float(self) -> Option<f64> {
    match self {
      NetworkResult::Float(value) => Some(value),
      _ => None,
    }
  }

  /// Extracts a Vec2 value from the NetworkResult, returns None if not a Vec2
  pub fn extract_vec2(self) -> Option<DVec2> {
    match self {
      NetworkResult::Vec2(vec) => Some(vec),
      _ => None,
    }
  }

  /// Extracts a Vec3 value from the NetworkResult, returns None if not a Vec3
  pub fn extract_vec3(self) -> Option<DVec3> {
    match self {
      NetworkResult::Vec3(vec) => Some(vec),
      _ => None,
    }
  }

  /// Extracts a String value from the NetworkResult, returns None if not a String
  pub fn extract_atomic(self) -> Option<AtomicStructure> {
    match self {
      NetworkResult::Atomic(value) => Some(value),
      _ => None,
    }
  }

  /// Converts this NetworkResult to the specified target data type
  /// Returns self if the types already match, otherwise performs conversion
  /// 
  /// # Parameters
  /// * `source_type` - The data type of this NetworkResult
  /// * `target_type` - The desired target data type
  pub fn convert_to(self, source_type: &DataType, target_type: &DataType) -> NetworkResult {
    // If types already match, return self
    if DataType::can_be_converted_to(source_type, target_type) && source_type == target_type {
      return self;
    }
    
    // If conversion is possible and both types are functions, return self unmodified
    // Function values don't need runtime conversion - partial evaluation happens at type level
    if DataType::can_be_converted_to(source_type, target_type) {
      if let (DataType::Function(_), DataType::Function(_)) = (source_type, target_type) {
        return self;
      }
    }

    // Handle Error and None cases - they cannot be converted
    match &self {
      NetworkResult::Error(_) | NetworkResult::None => return self,
      _ => {}
    }

    // Check if we can convert T to [T] (single element to array)
    if let DataType::Array(target_element_type) = target_type {
      if DataType::can_be_converted_to(source_type, target_element_type) {
        // Convert the single element to the target element type, then wrap in array
        let converted_element = self.convert_to(source_type, target_element_type);
        return NetworkResult::Array(vec![converted_element]);
      }
    }

    // Handle array to array conversion (element-wise conversion)
    if let (DataType::Array(source_element_type), DataType::Array(target_element_type)) = (source_type, target_type) {
      if let NetworkResult::Array(elements) = self {
        let converted_elements: Vec<NetworkResult> = elements
          .into_iter()
          .map(|element| element.convert_to(source_element_type, target_element_type))
          .collect();
        return NetworkResult::Array(converted_elements);
      }
    }

    // Perform basic type conversions
    match (self, target_type) {
      // Bool -> Int
      (NetworkResult::Bool(value), DataType::Int) => {
        NetworkResult::Int(if value { 1 } else { 0 })
      }
      
      // Int -> Bool (0 = false, non-zero = true)
      (NetworkResult::Int(value), DataType::Bool) => {
        NetworkResult::Bool(value != 0)
      }
      
      // Int -> Float
      (NetworkResult::Int(value), DataType::Float) => {
        NetworkResult::Float(value as f64)
      }
      
      // Float -> Int (rounded)
      (NetworkResult::Float(value), DataType::Int) => {
        NetworkResult::Int(value.round() as i32)
      }
      
      // IVec2 -> Vec2
      (NetworkResult::IVec2(vec), DataType::Vec2) => {
        NetworkResult::Vec2(DVec2::new(vec.x as f64, vec.y as f64))
      }
      
      // Vec2 -> IVec2 (rounded)
      (NetworkResult::Vec2(vec), DataType::IVec2) => {
        NetworkResult::IVec2(IVec2::new(vec.x.round() as i32, vec.y.round() as i32))
      }
      
      // IVec3 -> Vec3
      (NetworkResult::IVec3(vec), DataType::Vec3) => {
        NetworkResult::Vec3(DVec3::new(vec.x as f64, vec.y as f64, vec.z as f64))
      }
      
      // Vec3 -> IVec3 (rounded)
      (NetworkResult::Vec3(vec), DataType::IVec3) => {
        NetworkResult::IVec3(IVec3::new(vec.x.round() as i32, vec.y.round() as i32, vec.z.round() as i32))
      }
    
      (original, _target) => {
        /*
        NetworkResult::Error(format!(
          "Cannot convert {:?} to {:?}", 
          source_type, 
          target
        ))
        */
        original
      }
      /*
      we could return a runtime error here, but for technical reasons None types are converted
      to any value in runtime (due to the Value node), so we just return self for now.
      */
    }
  }

  /// Returns a user-readable string representation for all variants.
  /// For complex variants like Geometry2D, Geometry, Atomic, and Error, returns the variant name.
  pub fn to_display_string(&self) -> String {
    match self {
      NetworkResult::None => "None".to_string(),
      NetworkResult::Bool(value) => value.to_string(),
      NetworkResult::String(value) => value.to_string(),
      NetworkResult::Int(value) => value.to_string(),
      NetworkResult::Float(value) => format!("{:.6}", value),
      NetworkResult::Vec2(vec) => format!("({:.6}, {:.6})", vec.x, vec.y),
      NetworkResult::Vec3(vec) => format!("({:.6}, {:.6}, {:.6})", vec.x, vec.y, vec.z),
      NetworkResult::IVec2(vec) => format!("({}, {})", vec.x, vec.y),
      NetworkResult::IVec3(vec) => format!("({}, {}, {})", vec.x, vec.y, vec.z),
      NetworkResult::Array(elements) => {
        let element_strings: Vec<String> = elements
          .iter()
          .map(|element| element.to_display_string())
          .collect();
        format!("[{}]", element_strings.join(", "))
      },
      NetworkResult::Function(closure) => format!("network: {} node: {}", closure.node_network_name, closure.node_id),
      NetworkResult::UnitCell(unit_cell) => {
        format!("UnitCell:\n  a: ({:.6}, {:.6}, {:.6})\n  b: ({:.6}, {:.6}, {:.6})\n  c: ({:.6}, {:.6}, {:.6})", 
          unit_cell.a.x, unit_cell.a.y, unit_cell.a.z,
          unit_cell.b.x, unit_cell.b.y, unit_cell.b.z,
          unit_cell.c.x, unit_cell.c.y, unit_cell.c.z)
      },
      NetworkResult::Geometry2D(_) => "Geometry2D".to_string(),
      NetworkResult::Geometry(_) => "Geometry".to_string(),
      NetworkResult::Atomic(_) => "Atomic".to_string(),
      NetworkResult::Error(_) => "Error".to_string(),
    }
  }
}

/// Creates a consistent error message for missing input in node evaluation
/// 
/// # Arguments
/// * `input_name` - The name of the missing input (e.g., 'molecule', 'shape')
/// 
/// # Returns
/// * `NetworkResult::Error` with a formatted error message
pub fn input_missing_error(input_name: &str) -> NetworkResult {
  NetworkResult::Error(format!("{} input is missing", input_name))
}

pub fn error_in_input(input_name: &str) -> NetworkResult {
  NetworkResult::Error(format!("error in {} input", input_name))
}

pub fn runtime_type_error_in_input(input_param_index: usize) -> NetworkResult {
  NetworkResult::Error(format!("runtime type error in the {} indexed input", input_param_index))
}