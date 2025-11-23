use flutter_rust_bridge::frb;
use crate::api::common_api_types::APIIVec3;

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
}

#[frb]
#[derive(Clone)]
pub struct StructureDesignerPreferences {
  pub geometry_visualization_preferences: GeometryVisualizationPreferences,
  pub node_display_preferences: NodeDisplayPreferences,
  pub atomic_structure_visualization_preferences: AtomicStructureVisualizationPreferences,
  pub background_preferences: BackgroundPreferences,
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
        background_color: APIIVec3 { x: 150, y: 150, z: 150 },
        show_grid: true,
        grid_size: 200,
        grid_color: APIIVec3 { x: 132, y: 132, z: 132 },
        grid_strong_color: APIIVec3 { x: 90, y: 90, z: 90 },
      },
    }
  }

  #[flutter_rust_bridge::frb(sync)]
  pub fn clone_self(&self) -> StructureDesignerPreferences {
    self.clone()
  }  
}
















