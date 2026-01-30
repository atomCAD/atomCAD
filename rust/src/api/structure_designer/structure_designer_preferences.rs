use flutter_rust_bridge::frb;
use crate::api::common_api_types::APIIVec3;
use crate::structure_designer::layout::LayoutAlgorithm;

#[frb]
#[derive(PartialEq, Clone)]
pub enum GeometryVisualization {
  SurfaceSplatting,
  ExplicitMesh,
}

/// Enum to control mesh smoothing behavior during tessellation
#[frb]
#[derive(Debug, Clone)]
pub enum MeshSmoothing {
    /// Smooth normals: averages normals at each vertex from all connected faces
    Smooth,
    /// Sharp normals: uses face normals directly, duplicates vertices as needed
    Sharp,
    /// Smoothing group based: averages normals within the same smoothing group,
    /// duplicates vertices at smoothing group boundaries
    SmoothingGroupBased,
}

#[frb]
#[derive(Clone)]
pub struct GeometryVisualizationPreferences {
  #[frb(non_final)]
  pub geometry_visualization: GeometryVisualization,
  #[frb(non_final)]
  pub wireframe_geometry: bool,
  #[frb(non_final)]
  pub samples_per_unit_cell: i32,
  #[frb(non_final)]
  pub sharpness_angle_threshold_degree: f64,
  #[frb(non_final)]
  pub mesh_smoothing: MeshSmoothing,
  #[frb(non_final)]
  pub display_camera_target: bool,
}

#[frb]
#[derive(PartialEq, Clone)]
pub enum NodeDisplayPolicy {
  Manual,
  PreferSelected,
  PreferFrontier,
}

#[frb]
#[derive(Clone, PartialEq)]
pub struct NodeDisplayPreferences {
  #[frb(non_final)]
  pub display_policy: NodeDisplayPolicy,
}

#[frb]
#[derive(PartialEq, Clone, Debug)]
pub enum AtomicStructureVisualization {
  BallAndStick,
  SpaceFilling,
}

#[frb]
#[derive(PartialEq, Clone, Debug)]
pub enum AtomicRenderingMethod {
  TriangleMesh,
  Impostors,
}

#[frb]
#[derive(Clone, PartialEq)]
pub struct AtomicStructureVisualizationPreferences {
  #[frb(non_final)]
  pub visualization: AtomicStructureVisualization,
  #[frb(non_final)]
  pub rendering_method: AtomicRenderingMethod,
  #[frb(non_final)]
  pub ball_and_stick_cull_depth: Option<f64>,
  #[frb(non_final)]
  pub space_filling_cull_depth: Option<f64>,
}

#[frb]
#[derive(Clone, PartialEq)]
pub struct BackgroundPreferences {
  #[frb(non_final)]
  pub background_color: APIIVec3,
  #[frb(non_final)]
  pub show_grid: bool,
  #[frb(non_final)]
  pub grid_size: i32,
  #[frb(non_final)]
  pub grid_color: APIIVec3,
  #[frb(non_final)]
  pub grid_strong_color: APIIVec3,
  #[frb(non_final)]
  pub show_lattice_axes: bool,
  #[frb(non_final)]
  pub show_lattice_grid: bool,
  #[frb(non_final)]
  pub lattice_grid_color: APIIVec3,
  #[frb(non_final)]
  pub lattice_grid_strong_color: APIIVec3,
  #[frb(non_final)]
  pub drawing_plane_grid_color: APIIVec3,
  #[frb(non_final)]
  pub drawing_plane_grid_strong_color: APIIVec3,
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayoutAlgorithmPreference {
    /// Simple layered layout based on topological depth. Fast and reliable.
    /// Organizes nodes into columns by their depth in the dependency graph.
    #[default]
    TopologicalGrid,
    /// Sophisticated layered layout with crossing minimization.
    /// Uses the Sugiyama algorithm for better visual quality on complex graphs.
    /// (Not yet implemented - falls back to TopologicalGrid)
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
#[derive(Clone)]
pub struct LayoutPreferences {
    /// The layout algorithm to use for auto-layout operations.
    #[frb(non_final)]
    pub layout_algorithm: LayoutAlgorithmPreference,
    /// Whether to automatically apply layout after AI edit operations.
    /// When true, the full network layout is recomputed after each edit.
    /// When false, only new nodes are positioned incrementally.
    #[frb(non_final)]
    pub auto_layout_after_edit: bool,
}

#[frb]
#[derive(Clone)]
pub struct StructureDesignerPreferences {
  pub geometry_visualization_preferences: GeometryVisualizationPreferences,
  pub node_display_preferences: NodeDisplayPreferences,
  pub atomic_structure_visualization_preferences: AtomicStructureVisualizationPreferences,
  pub background_preferences: BackgroundPreferences,
  pub layout_preferences: LayoutPreferences,
}

impl StructureDesignerPreferences {
  #[flutter_rust_bridge::frb(sync)]
  pub fn new() -> Self {
    Self {
      geometry_visualization_preferences: GeometryVisualizationPreferences {
        geometry_visualization: GeometryVisualization::ExplicitMesh,
        wireframe_geometry: false,
        samples_per_unit_cell: 1,
        sharpness_angle_threshold_degree: 29.0,
        mesh_smoothing: MeshSmoothing::SmoothingGroupBased,
        display_camera_target: false,
      },
      node_display_preferences: NodeDisplayPreferences {
        display_policy: NodeDisplayPolicy::Manual,
      },
      atomic_structure_visualization_preferences: AtomicStructureVisualizationPreferences {
        visualization: AtomicStructureVisualization::BallAndStick,
        rendering_method: AtomicRenderingMethod::Impostors, // Default to high-performance impostor rendering
        ball_and_stick_cull_depth: Some(8.0), // Conservative depth culling at 8.0 Angstroms
        space_filling_cull_depth: Some(3.0),
      },
      background_preferences: BackgroundPreferences {
        background_color: APIIVec3 { x: 0, y: 0, z: 0 },
        show_grid: true,
        grid_size: 200,
        grid_color: APIIVec3 { x: 90, y: 90, z: 90 },
        grid_strong_color: APIIVec3 { x: 180, y: 180, z: 180 },
        show_lattice_axes: true,
        show_lattice_grid: false,
        lattice_grid_color: APIIVec3 { x: 60, y: 90, z: 90 },
        lattice_grid_strong_color: APIIVec3 { x: 100, y: 150, z: 150 },
        drawing_plane_grid_color: APIIVec3 { x: 70, y: 70, z: 100 },
        drawing_plane_grid_strong_color: APIIVec3 { x: 110, y: 110, z: 160 },
      },
      layout_preferences: LayoutPreferences {
        layout_algorithm: LayoutAlgorithmPreference::TopologicalGrid,
        auto_layout_after_edit: true,
      },
    }
  }

  #[flutter_rust_bridge::frb(sync)]
  pub fn clone_self(&self) -> StructureDesignerPreferences {
    self.clone()
  }  
}
















