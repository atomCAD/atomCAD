use glam::i32::IVec2;
use glam::i32::IVec3;
use glam::f64::DVec2;
use glam::f64::DVec3;
use crate::common::atomic_structure::AtomicStructure;
use crate::util::transform::Transform;
use crate::util::transform::Transform2D;
use crate::structure_designer::geo_tree::GeoNode;
use crate::api::structure_designer::structure_designer_api_types::APIDataType;

#[derive(Clone)]
pub struct GeometrySummary2D {
  pub frame_transform: Transform2D,
  pub geo_tree_root: GeoNode,
}

#[derive(Clone)]
pub struct GeometrySummary {
  pub frame_transform: Transform,
  pub geo_tree_root: GeoNode,
}

#[derive(Clone)]
pub enum NetworkResult {
  None,
  Bool(bool),
  String(String),
  Int(i32),
  Float(f64),
  Vec2(DVec2),
  Vec3(DVec3),
  IVec2(IVec2),
  IVec3(IVec3),
  Geometry2D(GeometrySummary2D),
  Geometry(GeometrySummary),
  Atomic(AtomicStructure),
  Error(String),
}

impl NetworkResult {
  /// Returns the APIDataType corresponding to this NetworkResult variant
  pub fn get_data_type(&self) -> APIDataType {
    match self {
      NetworkResult::None => APIDataType::None,
      NetworkResult::Bool(_) => APIDataType::Bool,
      NetworkResult::String(_) => APIDataType::String,
      NetworkResult::Int(_) => APIDataType::Int,
      NetworkResult::Float(_) => APIDataType::Float,
      NetworkResult::Vec2(_) => APIDataType::Vec2,
      NetworkResult::Vec3(_) => APIDataType::Vec3,
      NetworkResult::IVec2(_) => APIDataType::IVec2,
      NetworkResult::IVec3(_) => APIDataType::IVec3,
      NetworkResult::Geometry2D(_) => APIDataType::Geometry2D,
      NetworkResult::Geometry(_) => APIDataType::Geometry,
      NetworkResult::Atomic(_) => APIDataType::Atomic,
      NetworkResult::Error(_) => APIDataType::None, // Errors don't have a meaningful data type
    }
  }

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
  pub fn convert_to(self, target_type: APIDataType) -> NetworkResult {
    // If types already match, return self
    if self.get_data_type() == target_type {
      return self;
    }

    // Handle Error and None cases - they cannot be converted
    match &self {
      NetworkResult::Error(_) | NetworkResult::None => return self,
      _ => {}
    }

    // Perform conversions
    match (self, target_type) {
      // Bool -> Int
      (NetworkResult::Bool(value), APIDataType::Int) => {
        NetworkResult::Int(if value { 1 } else { 0 })
      }
      
      // Int -> Bool (0 = false, non-zero = true)
      (NetworkResult::Int(value), APIDataType::Bool) => {
        NetworkResult::Bool(value != 0)
      }
      
      // Int -> Float
      (NetworkResult::Int(value), APIDataType::Float) => {
        NetworkResult::Float(value as f64)
      }
      
      // Float -> Int (rounded)
      (NetworkResult::Float(value), APIDataType::Int) => {
        NetworkResult::Int(value.round() as i32)
      }
      
      // IVec2 -> Vec2
      (NetworkResult::IVec2(vec), APIDataType::Vec2) => {
        NetworkResult::Vec2(DVec2::new(vec.x as f64, vec.y as f64))
      }
      
      // Vec2 -> IVec2 (rounded)
      (NetworkResult::Vec2(vec), APIDataType::IVec2) => {
        NetworkResult::IVec2(IVec2::new(vec.x.round() as i32, vec.y.round() as i32))
      }
      
      // IVec3 -> Vec3
      (NetworkResult::IVec3(vec), APIDataType::Vec3) => {
        NetworkResult::Vec3(DVec3::new(vec.x as f64, vec.y as f64, vec.z as f64))
      }
      
      // Vec3 -> IVec3 (rounded)
      (NetworkResult::Vec3(vec), APIDataType::IVec3) => {
        NetworkResult::IVec3(IVec3::new(vec.x.round() as i32, vec.y.round() as i32, vec.z.round() as i32))
      }
      
      // All other conversions are invalid - return an error
      (original, target) => {
        NetworkResult::Error(format!(
          "Cannot convert {:?} to {:?}", 
          original.get_data_type(), 
          target
        ))
      }
    }
  }

  /// Returns a user-readable string representation for displayable variants.
  /// Returns None for Geometry2D, Geometry, Atomic, and Error variants.
  pub fn to_display_string(&self) -> Option<String> {
    match self {
      NetworkResult::None => None,
      NetworkResult::Bool(value) => Some(value.to_string()),
      NetworkResult::String(value) => Some(value.to_string()),
      NetworkResult::Int(value) => Some(value.to_string()),
      NetworkResult::Float(value) => Some(format!("{:.6}", value)),
      NetworkResult::Vec2(vec) => Some(format!("({:.6}, {:.6})", vec.x, vec.y)),
      NetworkResult::Vec3(vec) => Some(format!("({:.6}, {:.6}, {:.6})", vec.x, vec.y, vec.z)),
      NetworkResult::IVec2(vec) => Some(format!("({}, {})", vec.x, vec.y)),
      NetworkResult::IVec3(vec) => Some(format!("({}, {}, {})", vec.x, vec.y, vec.z)),
      NetworkResult::Geometry2D(_) => None,
      NetworkResult::Geometry(_) => None,
      NetworkResult::Atomic(_) => None,
      NetworkResult::Error(_) => None,
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