//! Phase 3 of `doc/design_atom_labels.md` — the atom-label display seam.
//!
//! `tessellate_atom_labels` turns the decorator's `atom_label` map into
//! billboarded glyph quads. These CPU-only tests (no GPU) assert mesh
//! *contents*, not pixels — the same pattern `atomic_color_test.rs` and
//! `atomic_render_style_test.rs` use.
//!
//! The behaviors worth guarding are the compositions, not the happy path:
//! labels ride the atom's *displayed* radius (so `render_style` composes without
//! either `StyleRule` field knowing about the other), a fully invisible atom
//! loses its label with the atom, a ghosted one keeps it, and labels survive in
//! `TriangleMesh` mode — which they only do because the call sits outside the
//! rendering-method match.

use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::{AtomRenderStyle, AtomicStructure};
use rust_lib_flutter_cad::display::atomic_tessellator::{
    get_displayed_atom_radius, tessellate_atom_labels,
};
use rust_lib_flutter_cad::display::preferences::{
    AtomicRenderingMethod, AtomicStructureVisualization, AtomicStructureVisualizationPreferences,
    BackgroundPreferences, DisplayPreferences, GeometryVisualizationPreferences, MeshSmoothing,
};
use rust_lib_flutter_cad::display::scene_tessellator::tessellate_scene_content;
use rust_lib_flutter_cad::renderer::camera::Camera;
use rust_lib_flutter_cad::renderer::label_mesh::LabelMesh;
use rust_lib_flutter_cad::structure_designer::structure_designer_scene::{
    NodeOutput, NodeSceneData, StructureDesignerScene,
};

// ============================================================================
// Helpers
// ============================================================================

const DEFAULT_LABEL_SCALE: f32 = 0.7;

fn prefs() -> AtomicStructureVisualizationPreferences {
    AtomicStructureVisualizationPreferences {
        visualization: AtomicStructureVisualization::BallAndStick,
        rendering_method: AtomicRenderingMethod::Impostors,
        ball_and_stick_cull_depth: None,
        space_filling_cull_depth: None,
        scene_transparency_enabled: false,
        scene_alpha: 1.0,
        label_scale: DEFAULT_LABEL_SCALE,
    }
}

fn labels(structure: &AtomicStructure, p: &AtomicStructureVisualizationPreferences) -> LabelMesh {
    let mut mesh = LabelMesh::new();
    tessellate_atom_labels(&mut mesh, structure, p);
    mesh
}

/// Quads in the mesh — one per character.
fn quad_count(mesh: &LabelMesh) -> usize {
    assert_eq!(mesh.vertices.len() % 4, 0, "vertices must come in quads");
    assert_eq!(
        mesh.indices.len(),
        mesh.vertices.len() / 4 * 6,
        "6 indices per quad"
    );
    mesh.vertices.len() / 4
}

/// A single labeled carbon at the origin.
fn labeled_carbon(text: &str) -> (AtomicStructure, u32) {
    let mut s = AtomicStructure::new();
    let id = s.add_atom(6, DVec3::ZERO);
    s.set_atom_label(id, text.to_string());
    (s, id)
}

fn display_prefs(rendering_method: AtomicRenderingMethod) -> DisplayPreferences {
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
            rendering_method,
            ..prefs()
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

fn test_camera() -> Camera {
    Camera {
        eye: DVec3::new(0.0, -30.0, 10.0),
        target: DVec3::ZERO,
        up: DVec3::new(0.0, 0.32, 0.95),
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

/// A one-node scene whose displayed output is a labeled molecule.
fn scene_with_labeled_atom() -> StructureDesignerScene {
    let (s, _) = labeled_carbon("C");
    let mut scene = StructureDesignerScene::new();
    scene.node_data.insert(
        rust_lib_flutter_cad::structure_designer::node_network::NodeRef::top(0),
        NodeSceneData::new(NodeOutput::Atomic(s, None)),
    );
    scene
}

// ============================================================================
// Emission
// ============================================================================

/// One quad per character: a two-char label is 2 quads = 8 vertices, 12 indices.
#[test]
fn two_char_label_emits_two_quads() {
    let (s, _) = labeled_carbon("Si");
    let mesh = labels(&s, &prefs());

    assert_eq!(quad_count(&mesh), 2);
    assert_eq!(mesh.vertices.len(), 8);
    assert_eq!(mesh.indices.len(), 12);
}

/// An unlabeled atom emits nothing — labels are opt-in per atom.
#[test]
fn unlabeled_atom_emits_no_quads() {
    let mut s = AtomicStructure::new();
    s.add_atom(6, DVec3::ZERO);

    assert_eq!(quad_count(&labels(&s, &prefs())), 0);
}

/// `set_atom_label("")` is the reset value — it clears rather than drawing an
/// empty label, so nothing is emitted.
#[test]
fn empty_label_emits_no_quads() {
    let (s, _) = labeled_carbon("");

    assert_eq!(quad_count(&labels(&s, &prefs())), 0);
}

/// A space advances the pen but has an empty SDF cell, so it emits no quad:
/// "A B" is 3 chars but only 2 glyphs.
#[test]
fn blank_glyphs_emit_no_quads() {
    let (s, _) = labeled_carbon("A B");

    assert_eq!(quad_count(&labels(&s, &prefs())), 2);
}

// ============================================================================
// Billboard anchoring
// ============================================================================

/// Every vertex of a label shares the anchor and the depth offset — the shader
/// expands the billboard from those, so a per-glyph drift would tear the label
/// apart as the camera moves.
#[test]
fn all_vertices_share_anchor_and_depth_offset() {
    let mut s = AtomicStructure::new();
    let id = s.add_atom(6, DVec3::new(1.5, -2.5, 3.5));
    s.set_atom_label(id, "Si".to_string());
    let mesh = labels(&s, &prefs());

    let expected_anchor = [1.5f32, -2.5, 3.5];
    let depth = mesh.vertices[0].depth_offset;
    for v in &mesh.vertices {
        assert_eq!(v.anchor_position, expected_anchor);
        assert_eq!(v.depth_offset, depth);
    }
}

/// The label's advance span is centered on the anchor. Centering is
/// advance-based, so the padded glyph cells overhang it slightly — the assertion
/// is that the plane offsets straddle zero, not that they are exactly symmetric.
#[test]
fn plane_offsets_are_centered_on_zero() {
    let (s, _) = labeled_carbon("Si");
    let mesh = labels(&s, &prefs());

    let min_x = mesh
        .vertices
        .iter()
        .fold(f32::MAX, |m, v| m.min(v.plane_offset[0]));
    let max_x = mesh
        .vertices
        .iter()
        .fold(f32::MIN, |m, v| m.max(v.plane_offset[0]));

    assert!(min_x < 0.0, "label extends left of the anchor: {min_x}");
    assert!(max_x > 0.0, "label extends right of the anchor: {max_x}");
    // Symmetric to within a fraction of the label height (padding asymmetry).
    assert!(
        (min_x + max_x).abs() < 0.5 * DEFAULT_LABEL_SCALE,
        "advance span not centered: [{min_x}, {max_x}]"
    );
}

/// `label_scale` is the em→Å factor: doubling it doubles the quads' extent.
#[test]
fn label_scale_scales_the_quads() {
    let (s, _) = labeled_carbon("Si");

    let extent = |scale: f32| -> f32 {
        let p = AtomicStructureVisualizationPreferences {
            label_scale: scale,
            ..prefs()
        };
        let mesh = labels(&s, &p);
        let min = mesh
            .vertices
            .iter()
            .fold(f32::MAX, |m, v| m.min(v.plane_offset[0]));
        let max = mesh
            .vertices
            .iter()
            .fold(f32::MIN, |m, v| m.max(v.plane_offset[0]));
        max - min
    };

    let single = extent(DEFAULT_LABEL_SCALE);
    let double = extent(DEFAULT_LABEL_SCALE * 2.0);
    assert!(
        (double - 2.0 * single).abs() < 1e-4,
        "expected {} to be twice {single}, got {double}",
        2.0 * single
    );
}

/// A zero or negative `label_scale` is clamped rather than collapsing every quad
/// to a degenerate point — a broken-looking scene reads as a broken feature.
#[test]
fn nonpositive_label_scale_is_clamped_to_a_visible_size() {
    let (s, _) = labeled_carbon("Si");
    for scale in [0.0, -3.0] {
        let p = AtomicStructureVisualizationPreferences {
            label_scale: scale,
            ..prefs()
        };
        let mesh = labels(&s, &p);
        let max_x = mesh
            .vertices
            .iter()
            .fold(f32::MIN, |m, v| m.max(v.plane_offset[0]));
        assert!(max_x > 0.0, "label_scale {scale} collapsed the quads");
    }
}

// ============================================================================
// Composition with render_style (displayed radius)
// ============================================================================

/// The label rides the atom's *displayed* radius: a `space_filling` atom's label
/// is pushed out to the vdW surface while a ball-and-stick neighbour's stays
/// close in — the two `StyleRule` fields compose without either knowing about
/// the other.
#[test]
fn depth_offset_tracks_the_displayed_radius() {
    let mut s = AtomicStructure::new();
    let bas = s.add_atom(6, DVec3::new(-5.0, 0.0, 0.0));
    let sf = s.add_atom(6, DVec3::new(5.0, 0.0, 0.0));
    s.set_atom_label(bas, "a".to_string());
    s.set_atom_label(sf, "b".to_string());
    s.set_atom_render_style(sf, AtomRenderStyle::SpaceFilling);

    let mesh = labels(&s, &prefs());
    assert_eq!(quad_count(&mesh), 2);

    // Recover each atom's depth offset by its anchor's x.
    let offset_at = |x: f32| -> f32 {
        mesh.vertices
            .iter()
            .find(|v| v.anchor_position[0] == x)
            .expect("a label anchored at this atom")
            .depth_offset
    };
    let bas_offset = offset_at(-5.0);
    let sf_offset = offset_at(5.0);

    assert!(
        sf_offset > bas_offset,
        "space-filling label ({sf_offset}) should sit further out than \
         ball-and-stick ({bas_offset})"
    );

    // And each is the displayed radius plus a small epsilon.
    let bas_radius = get_displayed_atom_radius(
        s.get_atom(bas).unwrap(),
        &AtomicStructureVisualization::BallAndStick,
    ) as f32;
    let sf_radius = get_displayed_atom_radius(
        s.get_atom(sf).unwrap(),
        &AtomicStructureVisualization::SpaceFilling,
    ) as f32;

    assert!(
        (bas_offset - bas_radius).abs() < 0.1,
        "{bas_offset} vs {bas_radius}"
    );
    assert!(
        (sf_offset - sf_radius).abs() < 0.1,
        "{sf_offset} vs {sf_radius}"
    );
    assert!(bas_offset > bas_radius, "label must clear its own sphere");
    assert!(sf_offset > sf_radius, "label must clear its own sphere");
}

// ============================================================================
// Culling and alpha
// ============================================================================

/// A culled atom emits no label — a label can never outlive its atom.
#[test]
fn culled_atom_emits_no_label() {
    let (mut s, id) = labeled_carbon("C");
    s.set_atom_in_crystal_depth(id, 10.0);

    let p = AtomicStructureVisualizationPreferences {
        ball_and_stick_cull_depth: Some(1.0),
        ..prefs()
    };
    assert_eq!(quad_count(&labels(&s, &p)), 0);

    // Sanity: without the threshold the same atom is labeled.
    assert_eq!(quad_count(&labels(&s, &prefs())), 1);
}

/// A fully transparent atom loses its label with the atom. Otherwise the label
/// would float anchored to nothing — and its depth writes would invisibly
/// occlude ghosts behind it.
#[test]
fn fully_transparent_atom_emits_no_label() {
    let (mut s, id) = labeled_carbon("C");
    s.set_atom_alpha(id, 0.0);

    assert_eq!(quad_count(&labels(&s, &prefs())), 0);
}

/// A ghosted atom keeps its label — that is precisely how a deliberately-faded
/// atom stays identifiable.
#[test]
fn ghosted_atom_keeps_its_label() {
    let (mut s, id) = labeled_carbon("C");
    s.set_atom_alpha(id, 0.5);

    assert_eq!(quad_count(&labels(&s, &prefs())), 1);
}

/// The scene-transparency preference composes by multiplication, so
/// `scene_alpha = 0` makes every atom invisible and empties the label mesh.
#[test]
fn zero_scene_alpha_empties_the_label_mesh() {
    let (s, _) = labeled_carbon("C");
    let p = AtomicStructureVisualizationPreferences {
        scene_transparency_enabled: true,
        scene_alpha: 0.0,
        ..prefs()
    };

    assert_eq!(quad_count(&labels(&s, &p)), 0);

    // A partial scene alpha still labels: only *fully* invisible atoms are skipped.
    let partial = AtomicStructureVisualizationPreferences {
        scene_alpha: 0.5,
        ..p
    };
    assert_eq!(quad_count(&labels(&s, &partial)), 1);
}

/// `scene_alpha` is ignored while the preference is disabled, so a `0.0` left
/// over in the struct must not silently drop every label.
#[test]
fn disabled_scene_transparency_ignores_scene_alpha() {
    let (s, _) = labeled_carbon("C");
    let p = AtomicStructureVisualizationPreferences {
        scene_transparency_enabled: false,
        scene_alpha: 0.0,
        ..prefs()
    };

    assert_eq!(quad_count(&labels(&s, &p)), 1);
}

// ============================================================================
// Scene seam: both rendering methods, and lightweight mode
// ============================================================================

/// Labels are emitted in BOTH rendering methods — they are their own mesh and
/// their own pipeline, independent of how atoms are drawn. This is the test that
/// fails if the call is ever moved inside the `Impostors` arm.
#[test]
fn labels_are_emitted_in_both_rendering_methods() {
    for method in [
        AtomicRenderingMethod::Impostors,
        AtomicRenderingMethod::TriangleMesh,
    ] {
        let (.., label_mesh, _gadget_atoms, _gadget_bonds) = tessellate_scene_content(
            &scene_with_labeled_atom(),
            &test_camera(),
            false, // not lightweight
            &display_prefs(method.clone()),
        );

        assert_eq!(
            quad_count(&label_mesh),
            1,
            "expected a label quad in {method:?} mode"
        );
    }
}

/// The label mesh stays empty in lightweight mode, like the transparent mesh.
#[test]
fn label_mesh_is_empty_in_lightweight_mode() {
    let (.., label_mesh, _gadget_atoms, _gadget_bonds) = tessellate_scene_content(
        &scene_with_labeled_atom(),
        &test_camera(),
        true, // lightweight
        &display_prefs(AtomicRenderingMethod::Impostors),
    );

    assert_eq!(quad_count(&label_mesh), 0);
}
