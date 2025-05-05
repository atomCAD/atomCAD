use serde::{Serialize, Deserialize};

pub enum Editor {
  None,
  StructureDesigner,
  SceneComposer
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

pub struct APITransform {
  pub translation: APIVec3,
  pub rotation: APIVec3, // intrinsic euler angles in degrees
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SelectModifier {
  Replace,
  Toggle,
  Expand
}
