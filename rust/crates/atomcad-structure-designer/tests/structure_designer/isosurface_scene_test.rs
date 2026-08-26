//! The opaque/transparent routing in `tessellate_scene_content`
//! (P4 of `doc/design_isosurface_node.md`).
//!
//! The `alpha >= 1.0` split is a *routing* decision made at merge time, and it
//! is what forces two isosurface meshes rather than one: the singleton-mesh
//! model gives one pipeline per mesh, so a surface cannot choose a pipeline
//! without choosing a mesh. These tests assert the mesh, never pixels — which
//! mesh a surface lands in is the whole observable content of the decision.

use atomcad_display::isosurface::{SurfaceComponent, SurfaceMesh};
use atomcad_display::preferences::{
    AtomicRenderingMethod, AtomicStructureVisualization, AtomicStructureVisualizationPreferences,
    BackgroundPreferences, DisplayPreferences, GeometryVisualizationPreferences, MeshSmoothing,
};
use atomcad_renderer::camera::Camera;
use atomcad_renderer::mesh::Mesh;
use atomcad_renderer::transparent_surface_mesh::TransparentSurfaceMesh;
use atomcad_structure_designer::node_network::NodeRef;
use atomcad_structure_designer::scene_tessellator::tessellate_scene_content;
use atomcad_structure_designer::structure_designer_scene::{
    NodeOutput, NodeSceneData, StructureDesignerScene,
};
use glam::f32::Vec3;
use glam::f64::DVec3;

// ============================================================================
// Helpers
// ============================================================================

fn test_camera() -> Camera {
    Camera {
        eye: DVec3::new(0.0, -30.0, 10.0),
        target: DVec3::ZERO,
        up: DVec3::new(0.0, 0.32, 0.95).normalize(),
        aspect: 1.0,
        fovy: std::f64::consts::PI * 0.15,
        znear: 1.5,
        zfar: 2400.0,
        orthographic: false,
        ortho_half_height: 10.0,
        pivot_point: DVec3::ZERO,
        nav_up: DVec3::Z,
        nav_up_label: "Z".to_string(),
    }
}

/// Inert preferences: this scene carries no atoms, no CSG geometry and no
/// background, so nothing here but the isosurface arm runs.
fn display_prefs() -> DisplayPreferences {
    DisplayPreferences {
        geometry_visualization: GeometryVisualizationPreferences {
            wireframe_geometry: false,
            mesh_smoothing: MeshSmoothing::Smooth,
            display_camera_target: false,
            wireframe_active_color: [1.0, 1.0, 1.0],
            wireframe_inactive_color: [0.5, 0.5, 0.5],
            hide_coplanar_edges: false,
        },
        atomic_structure_visualization: AtomicStructureVisualizationPreferences {
            visualization: AtomicStructureVisualization::BallAndStick,
            rendering_method: AtomicRenderingMethod::Impostors,
            ball_and_stick_cull_depth: None,
            space_filling_cull_depth: None,
            scene_transparency_enabled: false,
            scene_alpha: 1.0,
            label_scale: 0.7,
        },
        background: BackgroundPreferences {
            show_axes: false,
            show_grid: false,
            grid_size: 10,
            grid_color: [0, 0, 0],
            grid_strong_color: [0, 0, 0],
            show_lattice_axes: false,
            show_lattice_grid: false,
            lattice_grid_color: [0, 0, 0],
            lattice_grid_strong_color: [0, 0, 0],
            drawing_plane_grid_color: [0, 0, 0],
            drawing_plane_grid_strong_color: [0, 0, 0],
            unit_cell_wireframe_color: [0, 0, 0],
        },
    }
}

fn one_triangle_surface(offset: Vec3, alpha: f32) -> SurfaceMesh {
    let positions = vec![offset, offset + Vec3::X, offset + Vec3::Y];
    let centroid = positions.iter().copied().sum::<Vec3>() / 3.0;
    SurfaceMesh {
        positions,
        normals: vec![Vec3::Z; 3],
        albedo: vec![Vec3::new(0.2, 0.4, 0.9); 3],
        indices: vec![0, 1, 2],
        components: vec![SurfaceComponent {
            first_index: 0,
            index_count: 3,
            centroid,
        }],
        alpha,
    }
}

/// Tessellates a scene of displayed `isosurface` outputs at the given alphas,
/// and returns the two meshes the split routes between: the opaque one — which
/// *is* `main_mesh` — and the transparent one.
fn tessellate_surfaces(alphas: &[f32], lightweight: bool) -> (Mesh, TransparentSurfaceMesh) {
    let mut scene = StructureDesignerScene::new();
    for (i, &alpha) in alphas.iter().enumerate() {
        // Spread them out so nothing about the routing depends on overlap.
        let offset = Vec3::new(10.0 * i as f32, 0.0, 0.0);
        scene.node_data.insert(
            NodeRef::top(i as u64),
            NodeSceneData::new(NodeOutput::Isosurface(one_triangle_surface(offset, alpha))),
        );
    }

    let (
        _lightweight,
        _gadget_lines,
        main_mesh,
        _wireframe,
        _atoms,
        _bonds,
        _ghosts,
        _labels,
        _gadget_atoms,
        _gadget_bonds,
        surfaces,
    ) = tessellate_scene_content(&scene, &test_camera(), lightweight, &display_prefs());
    (main_mesh, surfaces)
}

// ============================================================================
// Tests
// ============================================================================

/// `>=`, not `==`, so anything at or above full opacity takes the opaque fast
/// path — including a value that rounded *up* past `1.0` on its way from the
/// node's `f64` property to the mesh's `f32`.
///
/// The design's worked example for this — "a slider landing on `0.9999999`" —
/// does not survive the `f32`: the nearest representable value is `0.99999994`,
/// which is below `1.0` and so takes the transparent path under either
/// comparison. Harmlessly, since a surface at that opacity is visually opaque;
/// what `>=` actually buys is the other side of `1.0`. See the P4 note in
/// `doc/design_isosurface_node.md`.
#[test]
fn alpha_at_or_above_one_takes_the_opaque_path() {
    for alpha in [1.0f32, 1.000_000_1] {
        let (main_mesh, surfaces) = tessellate_surfaces(&[alpha], false);
        assert_eq!(
            main_mesh.vertices.len(),
            3,
            "alpha {alpha} should join the opaque mesh"
        );
        assert!(
            surfaces.is_empty(),
            "alpha {alpha} should leave the transparent mesh empty"
        );
    }
}

#[test]
fn alpha_below_one_takes_the_transparent_path() {
    for alpha in [0.999f32, 0.999_999_94] {
        let (main_mesh, surfaces) = tessellate_surfaces(&[alpha], false);
        assert!(
            main_mesh.vertices.is_empty(),
            "alpha {alpha} should not join the opaque mesh"
        );
        assert_eq!(surfaces.mesh.vertices.len(), 3);
    }
}

/// Opaque vertices carry `alpha = 1.0`, so the shader change is a no-op for
/// `main_mesh` — which is what let the opaque path ship a phase early.
#[test]
fn the_opaque_mesh_stays_fully_opaque() {
    let (main_mesh, _) = tessellate_surfaces(&[1.0], false);
    assert!(main_mesh.vertices.iter().all(|v| v.alpha == 1.0));
}

/// A mixed scene — one opaque density envelope plus one transparent orbital —
/// is a normal thing to want on screen, and each surface must reach its own
/// mesh.
#[test]
fn a_mixed_scene_splits_across_both_meshes() {
    let (main_mesh, surfaces) = tessellate_surfaces(&[1.0, 0.4], false);
    assert_eq!(main_mesh.vertices.len(), 3);
    assert_eq!(surfaces.mesh.vertices.len(), 3);
    assert!(surfaces.mesh.vertices.iter().all(|v| v.alpha == 0.4));
    assert_eq!(surfaces.components.len(), 1);
}

/// Two transparent surfaces at different alphas share one merged mesh and one
/// pooled component list, and each keeps its own opacity — the failure the
/// vertex attribute exists to prevent.
#[test]
fn two_transparent_surfaces_pool_their_components() {
    let (_main, surfaces) = tessellate_surfaces(&[0.4, 0.85], false);
    assert_eq!(surfaces.mesh.vertices.len(), 6);
    assert_eq!(surfaces.components.len(), 2);
    assert_eq!(surfaces.components[0].first_index, 0);
    assert_eq!(surfaces.components[1].first_index, 3);
    assert_eq!(surfaces.components[0].index_count, 3);
    assert_eq!(surfaces.components[1].index_count, 3);

    // Two runs of three, not one run of six: the alphas did not collapse into
    // whichever surface was merged last. Sorted, because the scene iterates a
    // `HashMap` and the merge order of the two nodes is not defined.
    let mut runs: Vec<f32> = surfaces.mesh.vertices.iter().map(|v| v.alpha).collect();
    runs.dedup();
    runs.sort_by(f32::total_cmp);
    assert_eq!(runs, vec![0.4, 0.85]);
}

/// Lightweight refreshes skip non-lightweight content entirely, so the
/// transparent surface mesh comes back empty rather than stale.
#[test]
fn lightweight_mode_produces_no_surfaces() {
    let (main_mesh, surfaces) = tessellate_surfaces(&[0.4, 1.0], true);
    assert!(main_mesh.vertices.is_empty());
    assert!(surfaces.is_empty());
    assert!(surfaces.components.is_empty());
}
