// The domain twin is path-qualified rather than imported bare, because the
// api-side type deliberately keeps the same identifier (D9a).
use atomcad_crystolecule::atomic_structure::SelectModifier as DomainSelectModifier;
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

/// 2x2 integer matrix, row-major: `m[i]` is row i.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct APIIMat2 {
    pub m: [[i32; 2]; 2],
}

/// 3x3 integer matrix, row-major: `m[i]` is row i.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct APIIMat3 {
    pub m: [[i32; 3]; 3],
}

/// 3x3 float matrix, row-major: `m[i]` is row i.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIMat3 {
    pub m: [[f64; 3]; 3],
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
    /// Resolved world-space navigation-up axis (turntable screen-vertical).
    /// Consumed by the Flutter turntable math. See issue #349 / Phase 2.
    pub nav_up: APIVec3,
}

/// Navigation-up-axis state for the view-up dialog and camera-row indicator
/// (issue #349, Phase 2). See `doc/design_view_up_axis.md` (D7 / `get_view_up`).
pub struct APIViewUpInfo {
    /// The resolved world-space nav-up unit vector.
    pub axis: APIVec3,
    /// Cosmetic provenance label (e.g. `"Z"`, `"(1 1 1)"`, `"[1 1 0]"`).
    pub label: String,
    /// True when `axis` is (within epsilon) the default `+Z` — drives the
    /// highlight on the camera-row control.
    pub is_default: bool,
    /// What lattice Miller/direction indices currently resolve against (the
    /// active node's name, or the cubic-diamond fallback). Surfaced in the
    /// dialog so the fallback is never silent (D5).
    pub lattice_source_label: String,
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

/// Dart-facing twin of [`atomcad_crystolecule::atomic_structure::SelectModifier`].
///
/// The authoritative definition moved down into `atomcad-crystolecule` (D6);
/// this declaration stays here, under its **existing** name, because that is the
/// symbol the generated Dart already declares and Flutter already calls — see
/// `doc/design_rust_crate_split.md` D9a, which is explicit that a down-moved
/// type's twin must *not* be renamed to `API…`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SelectModifier {
    Replace,
    Toggle,
    Expand,
}

impl From<&SelectModifier> for DomainSelectModifier {
    fn from(m: &SelectModifier) -> Self {
        match m {
            SelectModifier::Replace => DomainSelectModifier::Replace,
            SelectModifier::Toggle => DomainSelectModifier::Toggle,
            SelectModifier::Expand => DomainSelectModifier::Expand,
        }
    }
}

impl From<SelectModifier> for DomainSelectModifier {
    fn from(m: SelectModifier) -> Self {
        (&m).into()
    }
}

impl From<&DomainSelectModifier> for SelectModifier {
    fn from(m: &DomainSelectModifier) -> Self {
        match m {
            DomainSelectModifier::Replace => SelectModifier::Replace,
            DomainSelectModifier::Toggle => SelectModifier::Toggle,
            DomainSelectModifier::Expand => SelectModifier::Expand,
        }
    }
}

impl From<DomainSelectModifier> for SelectModifier {
    fn from(m: DomainSelectModifier) -> Self {
        (&m).into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementSummary {
    pub atomic_number: i16,
    pub symbol: String,
    pub element_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIResult {
    pub success: bool,
    pub error_message: String,
}
