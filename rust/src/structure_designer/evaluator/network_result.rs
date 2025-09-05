use glam::i32::IVec2;
use glam::i32::IVec3;
use glam::f64::DVec2;
use glam::f64::DVec3;
use crate::common::atomic_structure::AtomicStructure;
use crate::util::transform::Transform;
use crate::util::transform::Transform2D;
use crate::structure_designer::geo_tree::GeoNode;

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
  /// Returns a user-readable string representation for displayable variants.
  /// Returns None for Geometry2D, Geometry, Atomic, and Error variants.
  pub fn to_display_string(&self) -> Option<String> {
    match self {
      NetworkResult::None => None,
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