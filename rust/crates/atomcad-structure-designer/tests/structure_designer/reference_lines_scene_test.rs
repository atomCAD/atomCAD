//! A displayed drawing plane's grid goes to the reference-line mesh, not the
//! content wireframe (`doc/design_network_thumbnails.md` D5): it is an editing
//! aid, so a thumbnail leaves it out, and its ±`grid_size` extent must not
//! enter the content bounds — framing a 400-cell grid shrank the extruded
//! solids beside it to a pixel.

use atomcad_crystolecule::drawing_plane::DrawingPlane;
use atomcad_display::preferences::{
    AtomicRenderingMethod, AtomicStructureVisualization, AtomicStructureVisualizationPreferences,
    BackgroundPreferences, DisplayPreferences, GeometryVisualizationPreferences, MeshSmoothing,
};
use atomcad_renderer::camera::Camera;
use atomcad_structure_designer::node_network::NodeRef;
use atomcad_structure_designer::scene_tessellator::tessellate_scene_content;
use atomcad_structure_designer::structure_designer_scene::{
    NodeOutput, NodeSceneData, StructureDesignerScene,
};
use glam::f64::DVec3;

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
            show_tool_envelopes: false,
            tool_envelope_color: [0, 0, 0],
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

#[test]
fn drawing_plane_grid_goes_to_reference_lines_not_wireframe() {
    let mut scene = StructureDesignerScene::new();
    scene.node_data.insert(
        NodeRef::top(0),
        NodeSceneData::new(NodeOutput::DrawingPlane(DrawingPlane::default())),
    );

    let (_, _, _, wireframe, reference_lines, ..) =
        tessellate_scene_content(&scene, &test_camera(), false, &display_prefs());

    assert!(
        wireframe.indices.is_empty(),
        "the drawing-plane grid must not be content"
    );
    assert!(
        !reference_lines.indices.is_empty(),
        "the drawing-plane grid is still drawn live"
    );
}
