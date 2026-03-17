use serde::{Deserialize, Serialize};

pub struct APIVec2 {
    pub x: f64,
    pub y: f64,
}

pub struct APIVec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

pub struct APIIVec2 {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub orthographic: bool,     // Whether to use orthographic projection
    pub ortho_half_height: f64, // Half height for orthographic projection (controls zoom level)
    pub pivot_point: APIVec3,
}

pub struct APITransform {
    pub translation: APIVec3,
    pub rotation: APIVec3, // intrinsic euler angles in degrees
}

pub enum APICameraCanonicalView {
    Custom,
    Top,
    Bottom,
    Front,
    Back,
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SelectModifier {
    Replace,
    Toggle,
    Expand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementSummary {
    pub atomic_number: i16,
    pub element_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIResult {
    pub success: bool,
    pub error_message: String,
}
