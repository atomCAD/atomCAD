use flutter_rust_bridge::frb;

#[frb]
#[derive(PartialEq, Clone)]
pub enum GeometryVisualization {
  SurfaceSplatting,
  DualContouring,
}

#[frb]
#[derive(Clone)]
pub struct GeometryVisualizationPreferences {
  #[frb(non_final)]
  pub geometry_visualization: GeometryVisualization,
  #[frb(non_final)]
  pub wireframe_geometry: bool,
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
      },
    }
  }
}
