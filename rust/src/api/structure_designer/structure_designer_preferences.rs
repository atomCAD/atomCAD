//! Dart-facing twins of the structure designer's user preferences.
//!
//! **The authoritative definitions live in
//! [`crate::structure_designer::preferences`]**, which is also what
//! `StructureDesigner` holds and what is persisted to
//! `<config_dir>/atomCAD/preferences.json`. These are transport copies: D9.2 of
//! `doc/design_rust_crate_split.md` moved the settings down into the domain (they
//! are persisted domain state, not DTOs) and D9a keeps a same-named twin here so
//! the generated Dart symbols and file path do not move. Each twin converts with
//! the `From` impls at the bottom of this file, and conversion happens **only** at
//! the api boundary — `get_structure_designer_preferences` /
//! `set_structure_designer_preferences`.
//!
//! Two rules when editing this file:
//!
//! - **Never rename a twin to `API…`.** These identifiers *are* the generated
//!   Dart symbols; renaming one breaks every Flutter reference.
//! - **A field added on one side must be added on the other**, together with its
//!   line in the `From` impls — the compiler enforces that for struct twins
//!   (exhaustive struct literals) and for enum twins (exhaustive `match`), which
//!   is why the impls are written out longhand rather than with a wildcard arm.
//!
//! `AtomicStructureVisualization` is the odd one out: its authoritative
//! definition is one level further down, in `atomcad_crystolecule`, because
//! `AtomicStructure::hit_test` needs it (D6, Phase 4).

use crate::api::common_api_types::APIIVec3;
// Path-qualified rather than imported bare: every api-side twin below
// deliberately keeps the same identifier as the domain type it mirrors (D9a).
use crate::structure_designer::preferences as domain;
use atomcad_crystolecule::visualization::AtomicStructureVisualization as DomainAtomicStructureVisualization;
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
#[derive(Clone, PartialEq, Serialize, Deserialize)]
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
    #[frb(non_final)]
    #[serde(default = "default_show_geometry_shell_for_atomic")]
    pub show_geometry_shell_for_atomic: bool,
    /// Wireframe line color for the active node's geometry (RGB, 0-255).
    #[frb(non_final)]
    #[serde(default = "default_wireframe_active_color")]
    pub wireframe_active_color: APIIVec3,
    /// Wireframe line color for non-active nodes' geometry (RGB, 0-255).
    #[frb(non_final)]
    #[serde(default = "default_wireframe_inactive_color")]
    pub wireframe_inactive_color: APIIVec3,
    /// When true, edges shared by two near-coplanar faces are not drawn in
    /// wireframe mode (hides interior triangulation lines for better visibility).
    #[frb(non_final)]
    #[serde(default = "default_hide_coplanar_wireframe_edges")]
    pub hide_coplanar_wireframe_edges: bool,
}

fn default_samples_per_unit_cell() -> i32 {
    1
}
fn default_sharpness_angle_threshold() -> f64 {
    29.0
}
fn default_show_geometry_shell_for_atomic() -> bool {
    true
}
fn default_wireframe_active_color() -> APIIVec3 {
    APIIVec3 {
        x: 255,
        y: 255,
        z: 255,
    }
}
fn default_wireframe_inactive_color() -> APIIVec3 {
    APIIVec3 {
        x: 128,
        y: 140,
        z: 153,
    }
}
fn default_hide_coplanar_wireframe_edges() -> bool {
    true
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
            show_geometry_shell_for_atomic: true,
            wireframe_active_color: default_wireframe_active_color(),
            wireframe_inactive_color: default_wireframe_inactive_color(),
            hide_coplanar_wireframe_edges: true,
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

/// Dart-facing twin of [`atomcad_crystolecule::visualization::AtomicStructureVisualization`].
///
/// The authoritative definition moved down into `atomcad-crystolecule` (D6),
/// because `AtomicStructure::hit_test` needs it and the old
/// `crystolecule → api` import was one of the four back-edges this refactor
/// exists to delete. This declaration stays here under its **existing** name —
/// it is what the generated Dart declares (D9a) — and it stays in *this* file so
/// the generated Dart path does not move either.
///
/// It is the only twin here whose original is *not* in
/// `structure_designer::preferences`: D6 sent this one enum down to
/// `atomcad-crystolecule` in Phase 4, one level below the other 12, which
/// followed in Phase 6 (D9.2).
#[frb]
#[derive(PartialEq, Clone, Debug, Serialize, Deserialize, Default)]
pub enum AtomicStructureVisualization {
    #[default]
    BallAndStick,
    SpaceFilling,
}

impl From<&AtomicStructureVisualization> for DomainAtomicStructureVisualization {
    fn from(v: &AtomicStructureVisualization) -> Self {
        match v {
            AtomicStructureVisualization::BallAndStick => {
                DomainAtomicStructureVisualization::BallAndStick
            }
            AtomicStructureVisualization::SpaceFilling => {
                DomainAtomicStructureVisualization::SpaceFilling
            }
        }
    }
}

impl From<AtomicStructureVisualization> for DomainAtomicStructureVisualization {
    fn from(v: AtomicStructureVisualization) -> Self {
        (&v).into()
    }
}

impl From<&DomainAtomicStructureVisualization> for AtomicStructureVisualization {
    fn from(v: &DomainAtomicStructureVisualization) -> Self {
        match v {
            DomainAtomicStructureVisualization::BallAndStick => {
                AtomicStructureVisualization::BallAndStick
            }
            DomainAtomicStructureVisualization::SpaceFilling => {
                AtomicStructureVisualization::SpaceFilling
            }
        }
    }
}

impl From<DomainAtomicStructureVisualization> for AtomicStructureVisualization {
    fn from(v: DomainAtomicStructureVisualization) -> Self {
        (&v).into()
    }
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
    /// When true, every atom/bond renders semi-transparent at `scene_alpha` —
    /// a global "see through everything" viewing lens, independent of `xray`
    /// nodes and composed with them by multiplication. Impostor mode only.
    #[frb(non_final)]
    #[serde(default)]
    pub scene_transparency_enabled: bool,
    /// Global scene alpha in `[0, 1]` used when `scene_transparency_enabled`.
    #[frb(non_final)]
    #[serde(default = "default_scene_alpha")]
    pub scene_alpha: f64,
    /// World-space em height of atom labels, in Å — labels scale with zoom, the
    /// way their atoms do. Clamped to `[0.05, 10.0]` in the UI and again at the
    /// Rust use site, mirroring `scene_alpha`. See `doc/design_atom_labels.md`
    /// §Label size.
    #[frb(non_final)]
    #[serde(default = "default_label_scale")]
    pub label_scale: f64,
}

fn default_ball_and_stick_cull_depth() -> Option<f64> {
    Some(8.0)
}
fn default_space_filling_cull_depth() -> Option<f64> {
    Some(3.0)
}
fn default_scene_alpha() -> f64 {
    0.5
}
/// Roughly a ball-and-stick carbon's diameter: big enough to read against its
/// own atom, small enough that two labelled neighbours do not collide at
/// default zoom.
fn default_label_scale() -> f64 {
    0.7
}

impl Default for AtomicStructureVisualizationPreferences {
    fn default() -> Self {
        Self {
            visualization: AtomicStructureVisualization::BallAndStick,
            rendering_method: AtomicRenderingMethod::Impostors,
            ball_and_stick_cull_depth: Some(8.0),
            space_filling_cull_depth: Some(3.0),
            scene_transparency_enabled: false,
            scene_alpha: 0.5,
            label_scale: 0.7,
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
    #[frb(non_final)]
    #[serde(default = "default_unit_cell_wireframe_color")]
    pub unit_cell_wireframe_color: APIIVec3,
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
fn default_unit_cell_wireframe_color() -> APIIVec3 {
    APIIVec3 {
        x: 0,
        y: 200,
        z: 200,
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
            unit_cell_wireframe_color: default_unit_cell_wireframe_color(),
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

/// Preferences for energy minimization simulation.
#[frb]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationPreferences {
    /// Use spatial grid with distance cutoff for van der Waals interactions.
    /// When false, all nonbonded pairs are computed exactly (O(N^2)).
    /// When true (default), a 6 A cutoff is used for faster computation on large structures.
    #[frb(non_final)]
    #[serde(default = "default_true")]
    pub use_vdw_cutoff: bool,

    /// Number of steepest descent steps per drag frame.
    /// Higher values give more relaxation per frame but cost more CPU time.
    /// Default: 4 (matches Avogadro's default).
    #[frb(non_final)]
    #[serde(default = "default_steps_per_frame")]
    pub continuous_minimization_steps_per_frame: u32,

    /// Number of steepest descent steps to run as a "settle burst" when
    /// the user releases the mouse after dragging. Lets the structure
    /// relax further without a jarring full-minimize snap.
    /// Default: 50.
    #[frb(non_final)]
    #[serde(default = "default_settle_steps")]
    pub continuous_minimization_settle_steps: u32,

    /// Maximum displacement (in Angstroms) for any single atom per steepest
    /// descent step during continuous minimization.
    /// Lower values make the structure respond more lazily to drags.
    /// Higher values make it more rigid/responsive.
    /// Default: 0.1 Å.
    #[frb(non_final)]
    #[serde(default = "default_max_displacement")]
    pub continuous_minimization_max_displacement: f64,
}

impl Default for SimulationPreferences {
    fn default() -> Self {
        Self {
            use_vdw_cutoff: true,
            continuous_minimization_steps_per_frame: 4,
            continuous_minimization_settle_steps: 50,
            continuous_minimization_max_displacement: 0.1,
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_steps_per_frame() -> u32 {
    4
}
fn default_settle_steps() -> u32 {
    50
}
fn default_max_displacement() -> f64 {
    0.1
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
    #[serde(default)]
    pub simulation_preferences: SimulationPreferences,
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

// ---------------------------------------------------------------------------
// Twin conversions (D9a)
//
// One `From<&Twin> for Domain` and one `From<&Domain> for Twin` per type, plus
// owned wrappers on the top-level `StructureDesignerPreferences` (the only pair
// the api actually calls). Declared here rather than in the domain module
// because the orphan rule needs the *local* type on one side, and the twins are
// the local ones.
// ---------------------------------------------------------------------------

impl From<&APIIVec3> for domain::PrefColor {
    fn from(v: &APIIVec3) -> Self {
        domain::PrefColor {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }
}

impl From<&domain::PrefColor> for APIIVec3 {
    fn from(v: &domain::PrefColor) -> Self {
        APIIVec3 {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }
}

impl From<&GeometryVisualization> for domain::GeometryVisualization {
    fn from(v: &GeometryVisualization) -> Self {
        match v {
            GeometryVisualization::SurfaceSplatting => {
                domain::GeometryVisualization::SurfaceSplatting
            }
            GeometryVisualization::ExplicitMesh => domain::GeometryVisualization::ExplicitMesh,
        }
    }
}

impl From<&domain::GeometryVisualization> for GeometryVisualization {
    fn from(v: &domain::GeometryVisualization) -> Self {
        match v {
            domain::GeometryVisualization::SurfaceSplatting => {
                GeometryVisualization::SurfaceSplatting
            }
            domain::GeometryVisualization::ExplicitMesh => GeometryVisualization::ExplicitMesh,
        }
    }
}

impl From<&MeshSmoothing> for domain::MeshSmoothing {
    fn from(v: &MeshSmoothing) -> Self {
        match v {
            MeshSmoothing::Smooth => domain::MeshSmoothing::Smooth,
            MeshSmoothing::Sharp => domain::MeshSmoothing::Sharp,
            MeshSmoothing::SmoothingGroupBased => domain::MeshSmoothing::SmoothingGroupBased,
        }
    }
}

impl From<&domain::MeshSmoothing> for MeshSmoothing {
    fn from(v: &domain::MeshSmoothing) -> Self {
        match v {
            domain::MeshSmoothing::Smooth => MeshSmoothing::Smooth,
            domain::MeshSmoothing::Sharp => MeshSmoothing::Sharp,
            domain::MeshSmoothing::SmoothingGroupBased => MeshSmoothing::SmoothingGroupBased,
        }
    }
}

impl From<&GeometryVisualizationPreferences> for domain::GeometryVisualizationPreferences {
    fn from(p: &GeometryVisualizationPreferences) -> Self {
        domain::GeometryVisualizationPreferences {
            geometry_visualization: (&p.geometry_visualization).into(),
            wireframe_geometry: p.wireframe_geometry,
            samples_per_unit_cell: p.samples_per_unit_cell,
            sharpness_angle_threshold_degree: p.sharpness_angle_threshold_degree,
            mesh_smoothing: (&p.mesh_smoothing).into(),
            display_camera_target: p.display_camera_target,
            show_geometry_shell_for_atomic: p.show_geometry_shell_for_atomic,
            wireframe_active_color: (&p.wireframe_active_color).into(),
            wireframe_inactive_color: (&p.wireframe_inactive_color).into(),
            hide_coplanar_wireframe_edges: p.hide_coplanar_wireframe_edges,
        }
    }
}

impl From<&domain::GeometryVisualizationPreferences> for GeometryVisualizationPreferences {
    fn from(p: &domain::GeometryVisualizationPreferences) -> Self {
        GeometryVisualizationPreferences {
            geometry_visualization: (&p.geometry_visualization).into(),
            wireframe_geometry: p.wireframe_geometry,
            samples_per_unit_cell: p.samples_per_unit_cell,
            sharpness_angle_threshold_degree: p.sharpness_angle_threshold_degree,
            mesh_smoothing: (&p.mesh_smoothing).into(),
            display_camera_target: p.display_camera_target,
            show_geometry_shell_for_atomic: p.show_geometry_shell_for_atomic,
            wireframe_active_color: (&p.wireframe_active_color).into(),
            wireframe_inactive_color: (&p.wireframe_inactive_color).into(),
            hide_coplanar_wireframe_edges: p.hide_coplanar_wireframe_edges,
        }
    }
}

impl From<&NodeDisplayPolicy> for domain::NodeDisplayPolicy {
    fn from(v: &NodeDisplayPolicy) -> Self {
        match v {
            NodeDisplayPolicy::Manual => domain::NodeDisplayPolicy::Manual,
            NodeDisplayPolicy::PreferSelected => domain::NodeDisplayPolicy::PreferSelected,
            NodeDisplayPolicy::PreferFrontier => domain::NodeDisplayPolicy::PreferFrontier,
        }
    }
}

impl From<&domain::NodeDisplayPolicy> for NodeDisplayPolicy {
    fn from(v: &domain::NodeDisplayPolicy) -> Self {
        match v {
            domain::NodeDisplayPolicy::Manual => NodeDisplayPolicy::Manual,
            domain::NodeDisplayPolicy::PreferSelected => NodeDisplayPolicy::PreferSelected,
            domain::NodeDisplayPolicy::PreferFrontier => NodeDisplayPolicy::PreferFrontier,
        }
    }
}

impl From<&NodeDisplayPreferences> for domain::NodeDisplayPreferences {
    fn from(p: &NodeDisplayPreferences) -> Self {
        domain::NodeDisplayPreferences {
            display_policy: (&p.display_policy).into(),
        }
    }
}

impl From<&domain::NodeDisplayPreferences> for NodeDisplayPreferences {
    fn from(p: &domain::NodeDisplayPreferences) -> Self {
        NodeDisplayPreferences {
            display_policy: (&p.display_policy).into(),
        }
    }
}

impl From<&AtomicRenderingMethod> for domain::AtomicRenderingMethod {
    fn from(v: &AtomicRenderingMethod) -> Self {
        match v {
            AtomicRenderingMethod::TriangleMesh => domain::AtomicRenderingMethod::TriangleMesh,
            AtomicRenderingMethod::Impostors => domain::AtomicRenderingMethod::Impostors,
        }
    }
}

impl From<&domain::AtomicRenderingMethod> for AtomicRenderingMethod {
    fn from(v: &domain::AtomicRenderingMethod) -> Self {
        match v {
            domain::AtomicRenderingMethod::TriangleMesh => AtomicRenderingMethod::TriangleMesh,
            domain::AtomicRenderingMethod::Impostors => AtomicRenderingMethod::Impostors,
        }
    }
}

impl From<&AtomicStructureVisualizationPreferences>
    for domain::AtomicStructureVisualizationPreferences
{
    fn from(p: &AtomicStructureVisualizationPreferences) -> Self {
        domain::AtomicStructureVisualizationPreferences {
            visualization: (&p.visualization).into(),
            rendering_method: (&p.rendering_method).into(),
            ball_and_stick_cull_depth: p.ball_and_stick_cull_depth,
            space_filling_cull_depth: p.space_filling_cull_depth,
            scene_transparency_enabled: p.scene_transparency_enabled,
            scene_alpha: p.scene_alpha,
            label_scale: p.label_scale,
        }
    }
}

impl From<&domain::AtomicStructureVisualizationPreferences>
    for AtomicStructureVisualizationPreferences
{
    fn from(p: &domain::AtomicStructureVisualizationPreferences) -> Self {
        AtomicStructureVisualizationPreferences {
            visualization: (&p.visualization).into(),
            rendering_method: (&p.rendering_method).into(),
            ball_and_stick_cull_depth: p.ball_and_stick_cull_depth,
            space_filling_cull_depth: p.space_filling_cull_depth,
            scene_transparency_enabled: p.scene_transparency_enabled,
            scene_alpha: p.scene_alpha,
            label_scale: p.label_scale,
        }
    }
}

impl From<&BackgroundPreferences> for domain::BackgroundPreferences {
    fn from(p: &BackgroundPreferences) -> Self {
        domain::BackgroundPreferences {
            background_color: (&p.background_color).into(),
            show_axes: p.show_axes,
            show_grid: p.show_grid,
            grid_size: p.grid_size,
            grid_color: (&p.grid_color).into(),
            grid_strong_color: (&p.grid_strong_color).into(),
            show_lattice_axes: p.show_lattice_axes,
            show_lattice_grid: p.show_lattice_grid,
            lattice_grid_color: (&p.lattice_grid_color).into(),
            lattice_grid_strong_color: (&p.lattice_grid_strong_color).into(),
            drawing_plane_grid_color: (&p.drawing_plane_grid_color).into(),
            drawing_plane_grid_strong_color: (&p.drawing_plane_grid_strong_color).into(),
            unit_cell_wireframe_color: (&p.unit_cell_wireframe_color).into(),
        }
    }
}

impl From<&domain::BackgroundPreferences> for BackgroundPreferences {
    fn from(p: &domain::BackgroundPreferences) -> Self {
        BackgroundPreferences {
            background_color: (&p.background_color).into(),
            show_axes: p.show_axes,
            show_grid: p.show_grid,
            grid_size: p.grid_size,
            grid_color: (&p.grid_color).into(),
            grid_strong_color: (&p.grid_strong_color).into(),
            show_lattice_axes: p.show_lattice_axes,
            show_lattice_grid: p.show_lattice_grid,
            lattice_grid_color: (&p.lattice_grid_color).into(),
            lattice_grid_strong_color: (&p.lattice_grid_strong_color).into(),
            drawing_plane_grid_color: (&p.drawing_plane_grid_color).into(),
            drawing_plane_grid_strong_color: (&p.drawing_plane_grid_strong_color).into(),
            unit_cell_wireframe_color: (&p.unit_cell_wireframe_color).into(),
        }
    }
}

impl From<&LayoutAlgorithmPreference> for domain::LayoutAlgorithmPreference {
    fn from(v: &LayoutAlgorithmPreference) -> Self {
        match v {
            LayoutAlgorithmPreference::TopologicalGrid => {
                domain::LayoutAlgorithmPreference::TopologicalGrid
            }
            LayoutAlgorithmPreference::Sugiyama => domain::LayoutAlgorithmPreference::Sugiyama,
        }
    }
}

impl From<&domain::LayoutAlgorithmPreference> for LayoutAlgorithmPreference {
    fn from(v: &domain::LayoutAlgorithmPreference) -> Self {
        match v {
            domain::LayoutAlgorithmPreference::TopologicalGrid => {
                LayoutAlgorithmPreference::TopologicalGrid
            }
            domain::LayoutAlgorithmPreference::Sugiyama => LayoutAlgorithmPreference::Sugiyama,
        }
    }
}

impl From<&SimulationPreferences> for domain::SimulationPreferences {
    fn from(p: &SimulationPreferences) -> Self {
        domain::SimulationPreferences {
            use_vdw_cutoff: p.use_vdw_cutoff,
            continuous_minimization_steps_per_frame: p.continuous_minimization_steps_per_frame,
            continuous_minimization_settle_steps: p.continuous_minimization_settle_steps,
            continuous_minimization_max_displacement: p.continuous_minimization_max_displacement,
        }
    }
}

impl From<&domain::SimulationPreferences> for SimulationPreferences {
    fn from(p: &domain::SimulationPreferences) -> Self {
        SimulationPreferences {
            use_vdw_cutoff: p.use_vdw_cutoff,
            continuous_minimization_steps_per_frame: p.continuous_minimization_steps_per_frame,
            continuous_minimization_settle_steps: p.continuous_minimization_settle_steps,
            continuous_minimization_max_displacement: p.continuous_minimization_max_displacement,
        }
    }
}

impl From<&LayoutPreferences> for domain::LayoutPreferences {
    fn from(p: &LayoutPreferences) -> Self {
        domain::LayoutPreferences {
            layout_algorithm: (&p.layout_algorithm).into(),
            auto_layout_after_edit: p.auto_layout_after_edit,
        }
    }
}

impl From<&domain::LayoutPreferences> for LayoutPreferences {
    fn from(p: &domain::LayoutPreferences) -> Self {
        LayoutPreferences {
            layout_algorithm: (&p.layout_algorithm).into(),
            auto_layout_after_edit: p.auto_layout_after_edit,
        }
    }
}

impl From<&StructureDesignerPreferences> for domain::StructureDesignerPreferences {
    fn from(p: &StructureDesignerPreferences) -> Self {
        domain::StructureDesignerPreferences {
            geometry_visualization_preferences: (&p.geometry_visualization_preferences).into(),
            node_display_preferences: (&p.node_display_preferences).into(),
            atomic_structure_visualization_preferences: (&p
                .atomic_structure_visualization_preferences)
                .into(),
            background_preferences: (&p.background_preferences).into(),
            layout_preferences: (&p.layout_preferences).into(),
            simulation_preferences: (&p.simulation_preferences).into(),
        }
    }
}

impl From<StructureDesignerPreferences> for domain::StructureDesignerPreferences {
    fn from(p: StructureDesignerPreferences) -> Self {
        (&p).into()
    }
}

impl From<&domain::StructureDesignerPreferences> for StructureDesignerPreferences {
    fn from(p: &domain::StructureDesignerPreferences) -> Self {
        StructureDesignerPreferences {
            geometry_visualization_preferences: (&p.geometry_visualization_preferences).into(),
            node_display_preferences: (&p.node_display_preferences).into(),
            atomic_structure_visualization_preferences: (&p
                .atomic_structure_visualization_preferences)
                .into(),
            background_preferences: (&p.background_preferences).into(),
            layout_preferences: (&p.layout_preferences).into(),
            simulation_preferences: (&p.simulation_preferences).into(),
        }
    }
}

impl From<domain::StructureDesignerPreferences> for StructureDesignerPreferences {
    fn from(p: domain::StructureDesignerPreferences) -> Self {
        (&p).into()
    }
}
