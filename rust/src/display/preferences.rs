#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshSmoothing {
    Smooth,
    Sharp,
    SmoothingGroupBased,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AtomicStructureVisualization {
    BallAndStick,
    SpaceFilling,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AtomicRenderingMethod {
    TriangleMesh,
    Impostors,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AtomicStructureVisualizationPreferences {
    pub visualization: AtomicStructureVisualization,
    pub rendering_method: AtomicRenderingMethod,
    pub ball_and_stick_cull_depth: Option<f64>,
    pub space_filling_cull_depth: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct GeometryVisualizationPreferences {
    pub wireframe_geometry: bool,
    pub mesh_smoothing: MeshSmoothing,
    pub display_camera_target: bool,
}

#[derive(Clone, Debug)]
pub struct BackgroundPreferences {
    pub show_axes: bool,
    pub show_grid: bool,
    pub grid_size: i32,

    pub grid_color: [u8; 3],
    pub grid_strong_color: [u8; 3],

    pub show_lattice_axes: bool,
    pub show_lattice_grid: bool,

    pub lattice_grid_color: [u8; 3],
    pub lattice_grid_strong_color: [u8; 3],

    pub drawing_plane_grid_color: [u8; 3],
    pub drawing_plane_grid_strong_color: [u8; 3],
}

#[derive(Clone, Debug)]
pub struct DisplayPreferences {
    pub geometry_visualization: GeometryVisualizationPreferences,
    pub atomic_structure_visualization: AtomicStructureVisualizationPreferences,
    pub background: BackgroundPreferences,
}
