use glam::i32::IVec2;
use glam::i32::IVec3;
use glam::f64::DVec2;
use glam::f64::DVec3;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::util::transform::Transform;
use crate::util::transform::Transform2D;
use crate::geo_tree::GeoNode;
use crate::structure_designer::data_type::DataType;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::crystolecule::motif::Motif;
use crate::crystolecule::drawing_plane::DrawingPlane;

#[derive(Clone)]
pub struct GeometrySummary2D {
  pub drawing_plane: DrawingPlane,
  pub frame_transform: Transform2D,
  pub geo_tree_root: GeoNode,
}

impl GeometrySummary2D {
  /// Returns a detailed string representation for snapshot testing.
  pub fn to_detailed_string(&self) -> String {
    let dp = &self.drawing_plane;
    let t = &self.frame_transform;
    format!(
      "drawing_plane:\n  miller_index: ({}, {}, {})\n  center: ({}, {}, {})\n  shift: {}\n  subdivision: {}\n  u_axis: ({}, {}, {})\n  v_axis: ({}, {}, {})\nframe_transform:\n  translation: ({:.6}, {:.6})\n  rotation: {:.6}\ngeo_tree:\n{}",
      dp.miller_index.x, dp.miller_index.y, dp.miller_index.z,
      dp.center.x, dp.center.y, dp.center.z,
      dp.shift,
      dp.subdivision,
      dp.u_axis.x, dp.u_axis.y, dp.u_axis.z,
      dp.v_axis.x, dp.v_axis.y, dp.v_axis.z,
      t.translation.x, t.translation.y,
      t.rotation,
      self.geo_tree_root
    )
  }

  /// Checks if this geometry's drawing plane is compatible with another geometry's drawing plane.
  /// 
  /// This is useful for CSG operations where geometries must have compatible drawing planes.
  /// Uses approximate equality with tolerance for small calculation errors.
  /// 
  /// # Arguments
  /// * `other` - The other GeometrySummary2D to compare drawing planes with
  /// 
  /// # Returns
  /// * `true` if the drawing planes are compatible (same unit cell and plane orientation)
  /// * `false` if they differ significantly
  pub fn has_compatible_drawing_plane(&self, other: &GeometrySummary2D) -> bool {
    self.drawing_plane.is_compatible(&other.drawing_plane)
  }

  /// Checks if all geometries in a vector have compatible drawing planes.
  /// 
  /// Compares each geometry's drawing plane to the first geometry's drawing plane.
  /// Returns true if the vector is empty or has only one element.
  /// 
  /// # Arguments
  /// * `geometries` - Vector of GeometrySummary2D objects to check
  /// 
  /// # Returns
  /// * `true` if all drawing planes are compatible or vector has ≤1 elements
  /// * `false` if any drawing plane is incompatible with the first
  pub fn all_have_compatible_drawing_planes(geometries: &[GeometrySummary2D]) -> bool {
    if geometries.len() <= 1 {
      return true;
    }
    
    let first_drawing_plane = &geometries[0].drawing_plane;
    geometries.iter().skip(1).all(|geometry| {
      first_drawing_plane.is_compatible(&geometry.drawing_plane)
    })
  }
}

#[derive(Clone)]
pub struct GeometrySummary {
  pub unit_cell: UnitCellStruct,
  pub frame_transform: Transform,
  pub geo_tree_root: GeoNode,
}

impl GeometrySummary {
  /// Returns a detailed string representation for snapshot testing.
  pub fn to_detailed_string(&self) -> String {
    let uc = &self.unit_cell;
    let t = &self.frame_transform;
    format!(
      "unit_cell:\n  a: ({:.6}, {:.6}, {:.6})\n  b: ({:.6}, {:.6}, {:.6})\n  c: ({:.6}, {:.6}, {:.6})\nframe_transform:\n  translation: ({:.6}, {:.6}, {:.6})\n  rotation: ({:.6}, {:.6}, {:.6}, {:.6})\ngeo_tree:\n{}",
      uc.a.x, uc.a.y, uc.a.z,
      uc.b.x, uc.b.y, uc.b.z,
      uc.c.x, uc.c.y, uc.c.z,
      t.translation.x, t.translation.y, t.translation.z,
      t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w,
      self.geo_tree_root
    )
  }

  /// Checks if this geometry's unit cell is compatible with another geometry's unit cell.
  /// 
  /// This is useful for CSG operations where geometries must have compatible unit cells.
  /// Uses approximate equality with tolerance for small calculation errors.
  /// 
  /// # Arguments
  /// * `other` - The other GeometrySummary to compare unit cells with
  /// 
  /// # Returns
  /// * `true` if the unit cells are approximately equal within tolerance
  /// * `false` if they differ significantly
  pub fn has_compatible_unit_cell(&self, other: &GeometrySummary) -> bool {
    self.unit_cell.is_approximately_equal(&other.unit_cell)
  }

  /// Checks if all geometries in a vector have approximately the same unit cells.
  /// 
  /// Compares each geometry's unit cell to the first geometry's unit cell.
  /// Returns true if the vector is empty or has only one element.
  /// 
  /// # Arguments
  /// * `geometries` - Vector of GeometrySummary objects to check
  /// 
  /// # Returns
  /// * `true` if all unit cells are approximately equal or vector has ≤1 elements
  /// * `false` if any unit cell differs significantly from the first
  pub fn all_have_compatible_unit_cells(geometries: &[GeometrySummary]) -> bool {
    if geometries.len() <= 1 {
      return true;
    }
    
    let first_unit_cell = &geometries[0].unit_cell;
    geometries.iter().skip(1).all(|geometry| {
      first_unit_cell.is_approximately_equal(&geometry.unit_cell)
    })
  }
}

#[derive(Clone)]
pub struct Closure {
  pub node_network_name: String,
  pub node_id: u64,
  pub captured_argument_values: Vec<NetworkResult>,
}

#[derive(Clone)]
#[derive(Default)]
pub enum NetworkResult {
  #[default]
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
  DrawingPlane(DrawingPlane),
  Geometry2D(GeometrySummary2D),
  Geometry(GeometrySummary),
  Atomic(AtomicStructure),
  Motif(Motif),
  Array(Vec<NetworkResult>),
  Function(Closure), 
  Error(String),
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

  /// Extracts a DrawingPlane value from the NetworkResult, returns None if not a DrawingPlane
  pub fn extract_drawing_plane(self) -> Option<DrawingPlane> {
    match self {
      NetworkResult::DrawingPlane(dp) => Some(dp),
      _ => None,
    }
  }

  /// Returns the UnitCellStruct associated with this NetworkResult.
  /// For UnitCell, DrawingPlane, Geometry2D, and Geometry variants, returns their unit cell.
  /// For all other variants, returns None.
  pub fn get_unit_cell(&self) -> Option<UnitCellStruct> {
    match self {
      NetworkResult::UnitCell(unit_cell) => Some(unit_cell.clone()),
      NetworkResult::DrawingPlane(drawing_plane) => Some(drawing_plane.unit_cell.clone()),
      NetworkResult::Geometry2D(geometry) => Some(geometry.drawing_plane.unit_cell.clone()),
      NetworkResult::Geometry(geometry) => Some(geometry.unit_cell.clone()),
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

  /// Extracts an optional Vec3 value from the NetworkResult
  /// Returns Some(None) if NetworkResult::None (no input connected)
  /// Returns Some(Some(vec)) if NetworkResult::Vec3(vec) 
  /// Returns None if not a Vec3 or None variant
  pub fn extract_optional_dvec3(self) -> Option<Option<DVec3>> {
    match self {
      NetworkResult::None => Some(None),
      NetworkResult::Vec3(vec) => Some(Some(vec)),
      _ => None,
    }
  }

  /// Extracts an optional Int value from the NetworkResult
  /// Returns Some(None) if NetworkResult::None (no input connected)
  /// Returns Some(Some(value)) if NetworkResult::Int(value) 
  /// Returns None if not an Int or None variant
  pub fn extract_optional_int(self) -> Option<Option<i32>> {
    match self {
      NetworkResult::None => Some(None),
      NetworkResult::Int(value) => Some(Some(value)),
      _ => None,
    }
  }

  /// Extracts an AtomicStructure value from the NetworkResult, returns None if not an Atomic
  pub fn extract_atomic(self) -> Option<AtomicStructure> {
    match self {
      NetworkResult::Atomic(value) => Some(value),
      _ => None,
    }
  }

  /// Extracts a Motif value from the NetworkResult, returns None if not a Motif
  pub fn extract_motif(self) -> Option<Motif> {
    match self {
      NetworkResult::Motif(value) => Some(value),
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
      
      // UnitCell -> DrawingPlane (backward compatibility for old .cnnd files)
      // Creates a standard XY plane (001 Miller index) at the origin
      (NetworkResult::UnitCell(unit_cell), DataType::DrawingPlane) => {
        match DrawingPlane::new(
          unit_cell,
          IVec3::new(0, 0, 1), // XY plane (001 Miller index)
          IVec3::new(0, 0, 0), // Center at origin
          0,                   // No shift
          1,                   // Subdivision = 1
        ) {
          Ok(drawing_plane) => NetworkResult::DrawingPlane(drawing_plane),
          Err(err_msg) => NetworkResult::Error(format!("Failed to convert UnitCell to DrawingPlane: {}", err_msg)),
        }
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
      NetworkResult::DrawingPlane(drawing_plane) => {
        format!("DrawingPlane: miller_index=({}, {}, {}), center=({}, {}, {}), shift={}, subdivision={}", 
          drawing_plane.miller_index.x, drawing_plane.miller_index.y, drawing_plane.miller_index.z,
          drawing_plane.center.x, drawing_plane.center.y, drawing_plane.center.z,
          drawing_plane.shift, drawing_plane.subdivision)
      },
      NetworkResult::Geometry2D(_) => "Geometry2D".to_string(),
      NetworkResult::Geometry(_) => "Geometry".to_string(),
      NetworkResult::Atomic(_) => "Atomic".to_string(),
      NetworkResult::Motif(_) => "Motif".to_string(),
      NetworkResult::Error(_) => "Error".to_string(),
    }
  }

  /// Returns a detailed string representation including full contents for complex types.
  /// For Geometry/Geometry2D, shows unit cell/drawing plane, frame transform, and geo tree.
  /// For Atomic/Motif, shows counts plus first 10 atoms/sites/bonds.
  /// For other variants, delegates to to_display_string().
  pub fn to_detailed_string(&self) -> String {
    match self {
      NetworkResult::Geometry(geometry) => {
        format!("Geometry:\n{}", geometry.to_detailed_string())
      },
      NetworkResult::Geometry2D(geometry) => {
        format!("Geometry2D:\n{}", geometry.to_detailed_string())
      },
      NetworkResult::Atomic(atomic) => {
        format!("Atomic:\n{}", atomic.to_detailed_string())
      },
      NetworkResult::Motif(motif) => {
        format!("Motif:\n{}", motif.to_detailed_string())
      },
      NetworkResult::Error(msg) => {
        format!("Error: {}", msg)
      },
      _ => self.to_display_string(),
    }
  }

  /// Parse a NetworkResult from a string value based on expected DataType.
  /// Used for CLI parameter parsing.
  pub fn from_string(value_str: &str, data_type: &DataType) -> Result<Self, String> {
    match data_type {
      DataType::Bool => {
        match value_str.to_lowercase().as_str() {
          "true" => Ok(NetworkResult::Bool(true)),
          "false" => Ok(NetworkResult::Bool(false)),
          _ => Err(format!("Invalid bool value '{}'. Expected 'true' or 'false'", value_str))
        }
      }
      
      DataType::Int => {
        value_str.parse::<i32>()
          .map(NetworkResult::Int)
          .map_err(|_| format!("Invalid int value '{}'", value_str))
      }
      
      DataType::Float => {
        value_str.parse::<f64>()
          .map(NetworkResult::Float)
          .map_err(|_| format!("Invalid float value '{}'", value_str))
      }
      
      DataType::String => {
        Ok(NetworkResult::String(value_str.to_string()))
      }
      
      DataType::Vec2 => {
        let parts: Vec<&str> = value_str.split(',').map(|s| s.trim()).collect();
        if parts.len() != 2 {
          return Err(format!("Vec2 requires 2 comma-separated values, got '{}'", value_str));
        }
        let x = parts[0].parse::<f64>()
          .map_err(|_| format!("Invalid Vec2 x component: '{}'", parts[0]))?;
        let y = parts[1].parse::<f64>()
          .map_err(|_| format!("Invalid Vec2 y component: '{}'", parts[1]))?;
        Ok(NetworkResult::Vec2(DVec2::new(x, y)))
      }
      
      DataType::Vec3 => {
        let parts: Vec<&str> = value_str.split(',').map(|s| s.trim()).collect();
        if parts.len() != 3 {
          return Err(format!("Vec3 requires 3 comma-separated values, got '{}'", value_str));
        }
        let x = parts[0].parse::<f64>()
          .map_err(|_| format!("Invalid Vec3 x component: '{}'", parts[0]))?;
        let y = parts[1].parse::<f64>()
          .map_err(|_| format!("Invalid Vec3 y component: '{}'", parts[1]))?;
        let z = parts[2].parse::<f64>()
          .map_err(|_| format!("Invalid Vec3 z component: '{}'", parts[2]))?;
        Ok(NetworkResult::Vec3(DVec3::new(x, y, z)))
      }
      
      DataType::IVec2 => {
        let parts: Vec<&str> = value_str.split(',').map(|s| s.trim()).collect();
        if parts.len() != 2 {
          return Err(format!("IVec2 requires 2 comma-separated values, got '{}'", value_str));
        }
        let x = parts[0].parse::<i32>()
          .map_err(|_| format!("Invalid IVec2 x component: '{}'", parts[0]))?;
        let y = parts[1].parse::<i32>()
          .map_err(|_| format!("Invalid IVec2 y component: '{}'", parts[1]))?;
        Ok(NetworkResult::IVec2(IVec2::new(x, y)))
      }
      
      DataType::IVec3 => {
        let parts: Vec<&str> = value_str.split(',').map(|s| s.trim()).collect();
        if parts.len() != 3 {
          return Err(format!("IVec3 requires 3 comma-separated values, got '{}'", value_str));
        }
        let x = parts[0].parse::<i32>()
          .map_err(|_| format!("Invalid IVec3 x component: '{}'", parts[0]))?;
        let y = parts[1].parse::<i32>()
          .map_err(|_| format!("Invalid IVec3 y component: '{}'", parts[1]))?;
        let z = parts[2].parse::<i32>()
          .map_err(|_| format!("Invalid IVec3 z component: '{}'", parts[2]))?;
        Ok(NetworkResult::IVec3(IVec3::new(x, y, z)))
      }
      
      _ => Err(format!("Unsupported CLI parameter type: {}", data_type))
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

pub fn unit_cell_mismatch_error() -> NetworkResult {
  NetworkResult::Error("Unit cell mismatch.".to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_unit_cell_exact_equality() {
    let uc1 = UnitCellStruct {
      a: DVec3::new(1.0, 0.0, 0.0),
      b: DVec3::new(0.0, 1.0, 0.0),
      c: DVec3::new(0.0, 0.0, 1.0),
      cell_length_a: 1.0,
      cell_length_b: 1.0,
      cell_length_c: 1.0,
      cell_angle_alpha: 90.0,
      cell_angle_beta: 90.0,
      cell_angle_gamma: 90.0,
    };
    let uc2 = UnitCellStruct {
      a: DVec3::new(1.0, 0.0, 0.0),
      b: DVec3::new(0.0, 1.0, 0.0),
      c: DVec3::new(0.0, 0.0, 1.0),
      cell_length_a: 1.0,
      cell_length_b: 1.0,
      cell_length_c: 1.0,
      cell_angle_alpha: 90.0,
      cell_angle_beta: 90.0,
      cell_angle_gamma: 90.0,
    };
    
    assert!(uc1.is_approximately_equal(&uc2));
  }

  #[test]
  fn test_unit_cell_approximate_equality() {
    let uc1 = UnitCellStruct {
      a: DVec3::new(1.0, 0.0, 0.0),
      b: DVec3::new(0.0, 1.0, 0.0),
      c: DVec3::new(0.0, 0.0, 1.0),
      cell_length_a: 1.0,
      cell_length_b: 1.0,
      cell_length_c: 1.0,
      cell_angle_alpha: 90.0,
      cell_angle_beta: 90.0,
      cell_angle_gamma: 90.0,
    };
    let uc2 = UnitCellStruct {
      a: DVec3::new(1.000001, 0.0, 0.0),
      b: DVec3::new(0.0, 0.999999, 0.0),
      c: DVec3::new(0.0, 0.0, 1.000001),
      cell_length_a: 1.000001,
      cell_length_b: 0.999999,
      cell_length_c: 1.000001,
      cell_angle_alpha: 90.0,
      cell_angle_beta: 90.0,
      cell_angle_gamma: 90.0,
    };
    
    // Small differences (< 1e-5) should be considered equal
    assert!(uc1.is_approximately_equal(&uc2));
  }

  #[test]
  fn test_unit_cell_significant_difference() {
    let uc1 = UnitCellStruct {
      a: DVec3::new(1.0, 0.0, 0.0),
      b: DVec3::new(0.0, 1.0, 0.0),
      c: DVec3::new(0.0, 0.0, 1.0),
      cell_length_a: 1.0,
      cell_length_b: 1.0,
      cell_length_c: 1.0,
      cell_angle_alpha: 90.0,
      cell_angle_beta: 90.0,
      cell_angle_gamma: 90.0,
    };
    let uc2 = UnitCellStruct {
      a: DVec3::new(1.0001, 0.0, 0.0),  // Difference > 1e-5
      b: DVec3::new(0.0, 1.0, 0.0),
      c: DVec3::new(0.0, 0.0, 1.0),
      cell_length_a: 1.0001,
      cell_length_b: 1.0,
      cell_length_c: 1.0,
      cell_angle_alpha: 90.0,
      cell_angle_beta: 90.0,
      cell_angle_gamma: 90.0,
    };
    
    // Significant differences (> 1e-5) should not be considered equal
    assert!(!uc1.is_approximately_equal(&uc2));
  }

  #[test]
  fn test_cubic_diamond_compatibility() {
    let uc1 = UnitCellStruct::cubic_diamond();
    let uc2 = UnitCellStruct::cubic_diamond();
    
    assert!(uc1.is_approximately_equal(&uc2));
  }
}
