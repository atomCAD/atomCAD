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
    /// When true, all atoms/bonds in the scene render semi-transparent at
    /// `scene_alpha` — a global viewing lens, independent of `xray` nodes.
    /// Impostor rendering only. Composes with per-atom `xray` alpha by
    /// multiplication (see `doc/design_xray_node.md`).
    pub scene_transparency_enabled: bool,
    /// Global scene alpha in `[0, 1]` applied when `scene_transparency_enabled`.
    pub scene_alpha: f32,
    /// World-space em height of atom labels, in Å — labels scale with zoom, as
    /// the atoms they annotate do (see `doc/design_atom_labels.md` §Label size).
    /// Clamped at the use site, mirroring `scene_alpha`.
    pub label_scale: f32,
}

#[derive(Clone, Debug)]
pub struct GeometryVisualizationPreferences {
    pub wireframe_geometry: bool,
    pub mesh_smoothing: MeshSmoothing,
    pub display_camera_target: bool,
    /// Wireframe line color for the active node's geometry (RGB, 0.0-1.0).
    pub wireframe_active_color: [f32; 3],
    /// Wireframe line color for non-active nodes' geometry (RGB, 0.0-1.0).
    pub wireframe_inactive_color: [f32; 3],
    /// When true, edges shared by two near-coplanar faces are not drawn in wireframe mode.
    pub hide_coplanar_edges: bool,
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

    pub unit_cell_wireframe_color: [u8; 3],
}

#[derive(Clone, Debug)]
pub struct DisplayPreferences {
    pub geometry_visualization: GeometryVisualizationPreferences,
    pub atomic_structure_visualization: AtomicStructureVisualizationPreferences,
    pub background: BackgroundPreferences,
}
