//! User preferences: the persisted settings themselves, and the load/save
//! functions that read and write them.
//!
//! Preferences are stored in `<config_dir>/atomCAD/preferences.json`.
//!
//! # Platform-Specific Locations
//!
//! - **Windows:** `%APPDATA%\atomCAD\preferences.json`
//! - **macOS:** `~/Library/Application Support/atomCAD/preferences.json`
//! - **Linux:** `~/.config/atomCAD/preferences.json`
//!
//! # Where these types come from
//!
//! They used to live in `api/structure_designer/structure_designer_preferences.rs`.
//! D9.2 of `doc/design_rust_crate_split.md` moves the authoritative definitions
//! here, because these are *persisted domain settings*, not transport DTOs — the
//! domain reads them on nearly every evaluation path, and reaching up into `api`
//! for them was part of the `structure_designer → api` back-edge. Each type
//! keeps a same-named Dart-facing twin in that api file, with `From` impls both
//! ways (D9a). `AtomicStructureVisualization` is the exception: it went further
//! down, to `atomcad_crystolecule::visualization`, in Phase 4, and is used here
//! directly.
//!
//! # Versioning Strategy (Tolerant Reader Pattern)
//!
//! - All struct fields have `#[serde(default)]` so missing fields get defaults
//! - Extra fields in JSON are silently ignored (forward compatibility)
//! - Use `#[serde(alias = "old_name")]` when renaming fields (backward compatibility)
//!
//! This approach avoids explicit version numbers while maintaining compatibility.
//! **The serialized shape is the compatibility contract** — it is what users'
//! existing `preferences.json` files contain — so `PrefColor`'s fields are
//! `x`/`y`/`z` rather than `r`/`g`/`b`: they mirror the `APIIVec3` the api-side
//! twin still uses.

use crate::structure_designer::layout::LayoutAlgorithm;
use atomcad_crystolecule::visualization::AtomicStructureVisualization;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// An RGB colour (0-255 per channel) as persisted in `preferences.json`.
///
/// The api-side twins spell this `APIIVec3`, which is a general-purpose
/// Dart-facing vector type and does **not** move down; this is the domain
/// counterpart for the colour fields alone. Field names must stay `x`/`y`/`z`
/// to keep already-written preferences files readable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrefColor {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl PrefColor {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Default)]
pub enum GeometryVisualization {
    SurfaceSplatting,
    #[default]
    ExplicitMesh,
}

/// Enum to control mesh smoothing behavior during tessellation
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

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryVisualizationPreferences {
    #[serde(default)]
    pub geometry_visualization: GeometryVisualization,
    #[serde(default)]
    pub wireframe_geometry: bool,
    #[serde(default = "default_samples_per_unit_cell")]
    pub samples_per_unit_cell: i32,
    #[serde(default = "default_sharpness_angle_threshold")]
    pub sharpness_angle_threshold_degree: f64,
    #[serde(default)]
    pub mesh_smoothing: MeshSmoothing,
    #[serde(default)]
    pub display_camera_target: bool,
    #[serde(default = "default_show_geometry_shell_for_atomic")]
    pub show_geometry_shell_for_atomic: bool,
    /// Wireframe line color for the active node's geometry (RGB, 0-255).
    #[serde(default = "default_wireframe_active_color")]
    pub wireframe_active_color: PrefColor,
    /// Wireframe line color for non-active nodes' geometry (RGB, 0-255).
    #[serde(default = "default_wireframe_inactive_color")]
    pub wireframe_inactive_color: PrefColor,
    /// When true, edges shared by two near-coplanar faces are not drawn in
    /// wireframe mode (hides interior triangulation lines for better visibility).
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
fn default_wireframe_active_color() -> PrefColor {
    PrefColor::new(255, 255, 255)
}
fn default_wireframe_inactive_color() -> PrefColor {
    PrefColor::new(128, 140, 153)
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

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Default)]
pub enum NodeDisplayPolicy {
    #[default]
    Manual,
    PreferSelected,
    PreferFrontier,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NodeDisplayPreferences {
    #[serde(default)]
    pub display_policy: NodeDisplayPolicy,
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize, Default)]
pub enum AtomicRenderingMethod {
    TriangleMesh,
    #[default]
    Impostors,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct AtomicStructureVisualizationPreferences {
    #[serde(default)]
    pub visualization: AtomicStructureVisualization,
    #[serde(default)]
    pub rendering_method: AtomicRenderingMethod,
    #[serde(default = "default_ball_and_stick_cull_depth")]
    pub ball_and_stick_cull_depth: Option<f64>,
    #[serde(default = "default_space_filling_cull_depth")]
    pub space_filling_cull_depth: Option<f64>,
    /// When true, every atom/bond renders semi-transparent at `scene_alpha` —
    /// a global "see through everything" viewing lens, independent of `xray`
    /// nodes and composed with them by multiplication. Impostor mode only.
    #[serde(default)]
    pub scene_transparency_enabled: bool,
    /// Global scene alpha in `[0, 1]` used when `scene_transparency_enabled`.
    #[serde(default = "default_scene_alpha")]
    pub scene_alpha: f64,
    /// World-space em height of atom labels, in Å — labels scale with zoom, the
    /// way their atoms do. Clamped to `[0.05, 10.0]` in the UI and again at the
    /// Rust use site, mirroring `scene_alpha`. See `doc/design_atom_labels.md`
    /// §Label size.
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

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct BackgroundPreferences {
    #[serde(default = "default_background_color")]
    pub background_color: PrefColor,
    #[serde(default = "default_show_axes")]
    pub show_axes: bool,
    #[serde(default = "default_show_grid")]
    pub show_grid: bool,
    #[serde(default = "default_grid_size")]
    pub grid_size: i32,
    #[serde(default = "default_grid_color")]
    pub grid_color: PrefColor,
    #[serde(default = "default_grid_strong_color")]
    pub grid_strong_color: PrefColor,
    #[serde(default = "default_show_lattice_axes")]
    pub show_lattice_axes: bool,
    #[serde(default)]
    pub show_lattice_grid: bool,
    #[serde(default = "default_lattice_grid_color")]
    pub lattice_grid_color: PrefColor,
    #[serde(default = "default_lattice_grid_strong_color")]
    pub lattice_grid_strong_color: PrefColor,
    #[serde(default = "default_drawing_plane_grid_color")]
    pub drawing_plane_grid_color: PrefColor,
    #[serde(default = "default_drawing_plane_grid_strong_color")]
    pub drawing_plane_grid_strong_color: PrefColor,
    #[serde(default = "default_unit_cell_wireframe_color")]
    pub unit_cell_wireframe_color: PrefColor,
}

fn default_background_color() -> PrefColor {
    PrefColor::new(0, 0, 0)
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
fn default_grid_color() -> PrefColor {
    PrefColor::new(90, 90, 90)
}
fn default_grid_strong_color() -> PrefColor {
    PrefColor::new(180, 180, 180)
}
fn default_show_lattice_axes() -> bool {
    true
}
fn default_lattice_grid_color() -> PrefColor {
    PrefColor::new(60, 90, 90)
}
fn default_lattice_grid_strong_color() -> PrefColor {
    PrefColor::new(100, 150, 150)
}
fn default_drawing_plane_grid_color() -> PrefColor {
    PrefColor::new(70, 70, 100)
}
fn default_drawing_plane_grid_strong_color() -> PrefColor {
    PrefColor::new(110, 110, 160)
}
fn default_unit_cell_wireframe_color() -> PrefColor {
    PrefColor::new(0, 200, 200)
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

/// Preferences for energy minimization simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationPreferences {
    /// Use spatial grid with distance cutoff for van der Waals interactions.
    /// When false, all nonbonded pairs are computed exactly (O(N^2)).
    /// When true (default), a 6 A cutoff is used for faster computation on large structures.
    #[serde(default = "default_true")]
    pub use_vdw_cutoff: bool,

    /// Number of steepest descent steps per drag frame.
    /// Higher values give more relaxation per frame but cost more CPU time.
    /// Default: 4 (matches Avogadro's default).
    #[serde(default = "default_steps_per_frame")]
    pub continuous_minimization_steps_per_frame: u32,

    /// Number of steepest descent steps to run as a "settle burst" when
    /// the user releases the mouse after dragging. Lets the structure
    /// relax further without a jarring full-minimize snap.
    /// Default: 50.
    #[serde(default = "default_settle_steps")]
    pub continuous_minimization_settle_steps: u32,

    /// Maximum displacement (in Angstroms) for any single atom per steepest
    /// descent step during continuous minimization.
    /// Lower values make the structure respond more lazily to drags.
    /// Higher values make it more rigid/responsive.
    /// Default: 0.1 Å.
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
#[derive(Clone, Serialize, Deserialize)]
pub struct LayoutPreferences {
    /// The layout algorithm to use for auto-layout operations.
    #[serde(default)]
    pub layout_algorithm: LayoutAlgorithmPreference,
    /// Whether to automatically apply layout after AI edit operations.
    /// When true, the full network layout is recomputed after each edit.
    /// When false, only new nodes are positioned incrementally.
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

const CONFIG_DIR_NAME: &str = "atomCAD";
const PREFERENCES_FILE_NAME: &str = "preferences.json";

/// Returns the path to the preferences file, or None if the config directory cannot be determined.
pub fn get_preferences_path() -> Option<PathBuf> {
    let config_dir = dirs::config_dir()?;
    Some(config_dir.join(CONFIG_DIR_NAME).join(PREFERENCES_FILE_NAME))
}

/// Loads preferences from the user's config directory.
///
/// Returns the loaded preferences, or defaults if:
/// - The config directory cannot be determined
/// - The preferences file doesn't exist (first run)
/// - The file is corrupted or invalid JSON
///
/// This function never fails - it always returns usable preferences.
pub fn load_preferences() -> StructureDesignerPreferences {
    let Some(path) = get_preferences_path() else {
        eprintln!("[preferences] Could not determine config directory, using defaults");
        return StructureDesignerPreferences::default();
    };

    if !path.exists() {
        // First run or file was deleted - silently use defaults
        return StructureDesignerPreferences::default();
    }

    match fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str(&contents) {
            Ok(prefs) => prefs,
            Err(e) => {
                eprintln!(
                    "[preferences] Failed to parse {}: {}, using defaults",
                    path.display(),
                    e
                );
                StructureDesignerPreferences::default()
            }
        },
        Err(e) => {
            eprintln!(
                "[preferences] Failed to read {}: {}, using defaults",
                path.display(),
                e
            );
            StructureDesignerPreferences::default()
        }
    }
}

/// Saves preferences to the user's config directory.
///
/// Creates the config directory if it doesn't exist.
/// Logs warnings on failure but doesn't propagate errors (preferences not saving is non-critical).
pub fn save_preferences(prefs: &StructureDesignerPreferences) {
    let Some(path) = get_preferences_path() else {
        eprintln!("[preferences] Could not determine config directory, preferences not saved");
        return;
    };

    // Create the config directory if it doesn't exist
    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        eprintln!(
            "[preferences] Failed to create config directory {}: {}",
            parent.display(),
            e
        );
        return;
    }

    // Serialize with pretty printing for human readability
    let json = match serde_json::to_string_pretty(prefs) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("[preferences] Failed to serialize preferences: {}", e);
            return;
        }
    };

    if let Err(e) = fs::write(&path, json) {
        eprintln!("[preferences] Failed to write {}: {}", path.display(), e);
    }
}
