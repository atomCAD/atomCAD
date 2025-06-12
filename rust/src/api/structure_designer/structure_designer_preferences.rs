use flutter_rust_bridge::frb;

#[frb]
#[derive(PartialEq, Clone)]
pub enum GeometryVisualization {
  SurfaceSplatting,
  DualContouring,
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
}

#[frb]
#[derive(Clone)]
pub struct StructureDesignerPreferences {
  pub geometry_visualization_preferences: GeometryVisualizationPreferences,
}

impl StructureDesignerPreferences {
  pub fn new() -> Self {
    Self {
      geometry_visualization_preferences: GeometryVisualizationPreferences {
        geometry_visualization: GeometryVisualization::DualContouring,
        wireframe_geometry: false,
        samples_per_unit_cell: 4,
        sharpness_angle_threshold_degree: 29.0,
        mesh_smoothing: MeshSmoothing::SmoothingGroupBased,
      },
    }
  }
}
