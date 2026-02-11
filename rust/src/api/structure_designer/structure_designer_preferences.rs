//! User preferences for the structure designer.
//!
//! # Versioning Strategy (Tolerant Reader Pattern)
//!
//! These preferences are persisted to `<config_dir>/atomCAD/preferences.json`.
//! We use a tolerant reader pattern for forward/backward compatibility:
//!
//! - All struct fields have `#[serde(default)]` so missing fields get defaults
//! - Extra fields in JSON are silently ignored (forward compatibility)
//! - Use `#[serde(alias = "old_name")]` when renaming fields (backward compatibility)
//!
//! This approach avoids explicit version numbers while maintaining compatibility.

use crate::api::common_api_types::APIIVec3;
use crate::structure_designer::layout::LayoutAlgorithm;
use flutter_rust_bridge::frb;
use serde::{Deserialize, Serialize};

#[frb]
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Default)]
pub enum GeometryVisualization {
    SurfaceSplatting,
    #[default]
    ExplicitMesh,
}

/// Enum to control mesh smoothing behavior during tessellation
#[frb]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MeshSmoothing {
    /// Smooth normals: averages normals at each vertex from all connected faces
    Smooth,
    /// Sharp normals: uses face normals directly, duplicates vertices as needed
    Sharp,
    /// Smoothing group based: averages normals within the same smoothing group,
    /// duplicates vertices at smoothing group boundaries
    #[default]
    SmoothingGroupBased,
}

#[frb]
#[derive(Clone, Serialize, Deserialize)]
pub struct GeometryVisualizationPreferences {
    #[frb(non_final)]
    #[serde(default)]
    pub geometry_visualization: GeometryVisualization,
    #[frb(non_final)]
    #[serde(default)]
    pub wireframe_geometry: bool,
    #[frb(non_final)]
    #[serde(default = "default_samples_per_unit_cell")]
    pub samples_per_unit_cell: i32,
    #[frb(non_final)]
    #[serde(default = "default_sharpness_angle_threshold")]
    pub sharpness_angle_threshold_degree: f64,
    #[frb(non_final)]
    #[serde(default)]
    pub mesh_smoothing: MeshSmoothing,
    #[frb(non_final)]
    #[serde(default)]
    pub display_camera_target: bool,
}

fn default_samples_per_unit_cell() -> i32 {
    1
}
fn default_sharpness_angle_threshold() -> f64 {
    29.0
}

impl Default for GeometryVisualizationPreferences {
    fn default() -> Self {
        Self {
            geometry_visualization: GeometryVisualization::ExplicitMesh,
            wireframe_geometry: false,
            samples_per_unit_cell: 1,
            sharpness_angle_threshold_degree: 29.0,
            mesh_smoothing: MeshSmoothing::SmoothingGroupBased,
            display_camera_target: false,
        }
    }
}

#[frb]
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Default)]
pub enum NodeDisplayPolicy {
    #[default]
    Manual,
    PreferSelected,
    PreferFrontier,
}

#[frb]
#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NodeDisplayPreferences {
    #[frb(non_final)]
    #[serde(default)]
    pub display_policy: NodeDisplayPolicy,
}

#[frb]
#[derive(PartialEq, Clone, Debug, Serialize, Deserialize, Default)]
pub enum AtomicStructureVisualization {
    #[default]
    BallAndStick,
    SpaceFilling,
}

#[frb]
#[derive(PartialEq, Clone, Debug, Serialize, Deserialize, Default)]
pub enum AtomicRenderingMethod {
    TriangleMesh,
    #[default]
    Impostors,
}

#[frb]
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct AtomicStructureVisualizationPreferences {
    #[frb(non_final)]
    #[serde(default)]
    pub visualization: AtomicStructureVisualization,
    #[frb(non_final)]
    #[serde(default)]
    pub rendering_method: AtomicRenderingMethod,
    #[frb(non_final)]
    #[serde(default = "default_ball_and_stick_cull_depth")]
    pub ball_and_stick_cull_depth: Option<f64>,
    #[frb(non_final)]
    #[serde(default = "default_space_filling_cull_depth")]
    pub space_filling_cull_depth: Option<f64>,
}

fn default_ball_and_stick_cull_depth() -> Option<f64> {
    Some(8.0)
}
fn default_space_filling_cull_depth() -> Option<f64> {
    Some(3.0)
}

impl Default for AtomicStructureVisualizationPreferences {
    fn default() -> Self {
        Self {
            visualization: AtomicStructureVisualization::BallAndStick,
            rendering_method: AtomicRenderingMethod::Impostors,
            ball_and_stick_cull_depth: Some(8.0),
            space_filling_cull_depth: Some(3.0),
        }
    }
}

#[frb]
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct BackgroundPreferences {
    #[frb(non_final)]
    #[serde(default = "default_background_color")]
    pub background_color: APIIVec3,
    #[frb(non_final)]
    #[serde(default = "default_show_axes")]
    pub show_axes: bool,
    #[frb(non_final)]
    #[serde(default = "default_show_grid")]
    pub show_grid: bool,
    #[frb(non_final)]
    #[serde(default = "default_grid_size")]
    pub grid_size: i32,
    #[frb(non_final)]
    #[serde(default = "default_grid_color")]
    pub grid_color: APIIVec3,
    #[frb(non_final)]
    #[serde(default = "default_grid_strong_color")]
    pub grid_strong_color: APIIVec3,
    #[frb(non_final)]
    #[serde(default = "default_show_lattice_axes")]
    pub show_lattice_axes: bool,
    #[frb(non_final)]
    #[serde(default)]
    pub show_lattice_grid: bool,
    #[frb(non_final)]
    #[serde(default = "default_lattice_grid_color")]
    pub lattice_grid_color: APIIVec3,
    #[frb(non_final)]
    #[serde(default = "default_lattice_grid_strong_color")]
    pub lattice_grid_strong_color: APIIVec3,
    #[frb(non_final)]
    #[serde(default = "default_drawing_plane_grid_color")]
    pub drawing_plane_grid_color: APIIVec3,
    #[frb(non_final)]
    #[serde(default = "default_drawing_plane_grid_strong_color")]
    pub drawing_plane_grid_strong_color: APIIVec3,
}

fn default_background_color() -> APIIVec3 {
    APIIVec3 { x: 0, y: 0, z: 0 }
}
fn default_show_axes() -> bool {
    true
}
fn default_show_grid() -> bool {
    true
}
fn default_grid_size() -> i32 {
    200
}
fn default_grid_color() -> APIIVec3 {
    APIIVec3 {
        x: 90,
        y: 90,
        z: 90,
    }
}
fn default_grid_strong_color() -> APIIVec3 {
    APIIVec3 {
        x: 180,
        y: 180,
        z: 180,
    }
}
fn default_show_lattice_axes() -> bool {
    true
}
fn default_lattice_grid_color() -> APIIVec3 {
    APIIVec3 {
        x: 60,
        y: 90,
        z: 90,
    }
}
fn default_lattice_grid_strong_color() -> APIIVec3 {
    APIIVec3 {
        x: 100,
        y: 150,
        z: 150,
    }
}
fn default_drawing_plane_grid_color() -> APIIVec3 {
    APIIVec3 {
        x: 70,
        y: 70,
        z: 100,
    }
}
fn default_drawing_plane_grid_strong_color() -> APIIVec3 {
    APIIVec3 {
        x: 110,
        y: 110,
        z: 160,
    }
}

impl Default for BackgroundPreferences {
    fn default() -> Self {
        Self {
            background_color: default_background_color(),
            show_axes: true,
            show_grid: true,
            grid_size: 200,
            grid_color: default_grid_color(),
            grid_strong_color: default_grid_strong_color(),
            show_lattice_axes: true,
            show_lattice_grid: false,
            lattice_grid_color: default_lattice_grid_color(),
            lattice_grid_strong_color: default_lattice_grid_strong_color(),
            drawing_plane_grid_color: default_drawing_plane_grid_color(),
            drawing_plane_grid_strong_color: default_drawing_plane_grid_strong_color(),
        }
    }
}

/// Layout algorithm preference for full network auto-layout operations.
///
/// These algorithms reorganize the entire network. They are used:
/// - When "Auto-Layout Network" is triggered from the menu
/// - After AI edit operations (when auto_layout_after_edit is enabled)
///
/// Note: Incremental positioning of new nodes during editing is handled
/// separately by the auto_layout module, not through this enum.
#[frb]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutAlgorithmPreference {
    /// Simple layered layout based on topological depth. Fast and reliable.
    /// Organizes nodes into columns by their depth in the dependency graph.
    TopologicalGrid,
    /// Sophisticated layered layout with crossing minimization.
    /// Uses the Sugiyama algorithm for better visual quality on complex graphs.
    #[default]
    Sugiyama,
}

impl From<LayoutAlgorithmPreference> for LayoutAlgorithm {
    fn from(pref: LayoutAlgorithmPreference) -> Self {
        match pref {
            LayoutAlgorithmPreference::TopologicalGrid => LayoutAlgorithm::TopologicalGrid,
            LayoutAlgorithmPreference::Sugiyama => LayoutAlgorithm::Sugiyama,
        }
    }
}

/// Preferences for auto-layout operations.
#[frb]
#[derive(Clone, Serialize, Deserialize)]
pub struct LayoutPreferences {
    /// The layout algorithm to use for auto-layout operations.
    #[frb(non_final)]
    #[serde(default)]
    pub layout_algorithm: LayoutAlgorithmPreference,
    /// Whether to automatically apply layout after AI edit operations.
    /// When true, the full network layout is recomputed after each edit.
    /// When false, only new nodes are positioned incrementally.
    #[frb(non_final)]
    #[serde(default = "default_auto_layout_after_edit")]
    pub auto_layout_after_edit: bool,
}

fn default_auto_layout_after_edit() -> bool {
    true
}

impl Default for LayoutPreferences {
    fn default() -> Self {
        Self {
            layout_algorithm: LayoutAlgorithmPreference::Sugiyama,
            auto_layout_after_edit: true,
        }
    }
}

#[frb]
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct StructureDesignerPreferences {
    #[serde(default)]
    pub geometry_visualization_preferences: GeometryVisualizationPreferences,
    #[serde(default)]
    pub node_display_preferences: NodeDisplayPreferences,
    #[serde(default)]
    pub atomic_structure_visualization_preferences: AtomicStructureVisualizationPreferences,
    #[serde(default)]
    pub background_preferences: BackgroundPreferences,
    #[serde(default)]
    pub layout_preferences: LayoutPreferences,
}

impl StructureDesignerPreferences {
    #[flutter_rust_bridge::frb(sync)]
    pub fn new() -> Self {
        Self::default()
    }

    #[flutter_rust_bridge::frb(sync)]
    pub fn clone_self(&self) -> StructureDesignerPreferences {
        self.clone()
    }
}
