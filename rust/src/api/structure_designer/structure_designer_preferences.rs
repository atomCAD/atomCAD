use flutter_rust_bridge::frb;

#[frb]
#[derive(PartialEq, Clone)]
pub enum GeometryVisualization {
  SurfaceSplatting,
  DualContouring,
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
#[derive(Clone, PartialEq)]
pub struct AtomicStructureVisualizationPreferences {
  #[frb(non_final)]
  pub ball_and_stick_cull_depth: Option<f32>,
}

#[frb]
#[derive(Clone)]
pub struct StructureDesignerPreferences {
  pub geometry_visualization_preferences: GeometryVisualizationPreferences,
  pub node_display_preferences: NodeDisplayPreferences,
  pub atomic_structure_visualization_preferences: AtomicStructureVisualizationPreferences,
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
        display_policy: NodeDisplayPolicy::PreferSelected,
      },
      atomic_structure_visualization_preferences: AtomicStructureVisualizationPreferences {
        ball_and_stick_cull_depth: Some(8.0), // Conservative depth culling at 8.0 Angstroms
      },
    }
  }

  #[flutter_rust_bridge::frb(sync)]
  pub fn clone_self(&self) -> StructureDesignerPreferences {
    self.clone()
  }  
}
