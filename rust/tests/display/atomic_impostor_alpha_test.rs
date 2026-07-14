//! Phase 3 of `doc/design_xray_node.md` — transparent impostor mesh routing.
//!
//! `tessellate_atomic_structure_impostors` routes atoms and bonds whose
//! display alpha is `< 1.0` into the merged `TransparentImpostorMesh` instead
//! of the opaque atom/bond impostor meshes. These CPU-only tests exercise that
//! routing (no GPU): atom routing by alpha, the min-alpha bond rule,
//! `quad_centers` bookkeeping, and that delete-marker / space-filling filtering
//! survive the refactor.

use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_DELETED, BOND_SINGLE,
};
use rust_lib_flutter_cad::display::atomic_tessellator::tessellate_atomic_structure_impostors;
use rust_lib_flutter_cad::display::preferences::{
    AtomicRenderingMethod, AtomicStructureVisualization, AtomicStructureVisualizationPreferences,
    BackgroundPreferences, DisplayPreferences, GeometryVisualizationPreferences, MeshSmoothing,
};
use rust_lib_flutter_cad::display::scene_tessellator::tessellate_scene_content;
use rust_lib_flutter_cad::renderer::atom_impostor_mesh::AtomImpostorMesh;
use rust_lib_flutter_cad::renderer::bond_impostor_mesh::BondImpostorMesh;
use rust_lib_flutter_cad::renderer::camera::Camera;
use rust_lib_flutter_cad::renderer::transparent_impostor_mesh::TransparentImpostorMesh;
use rust_lib_flutter_cad::structure_designer::structure_designer_scene::{
    NodeOutput, NodeSceneData, StructureDesignerScene,
};

// ============================================================================
// Helpers
// ============================================================================

fn ball_and_stick_prefs() -> AtomicStructureVisualizationPreferences {
    AtomicStructureVisualizationPreferences {
        visualization: AtomicStructureVisualization::BallAndStick,
        rendering_method: AtomicRenderingMethod::Impostors,
        ball_and_stick_cull_depth: None,
        space_filling_cull_depth: None,
    }
}

fn space_filling_prefs() -> AtomicStructureVisualizationPreferences {
    AtomicStructureVisualizationPreferences {
        visualization: AtomicStructureVisualization::SpaceFilling,
        rendering_method: AtomicRenderingMethod::Impostors,
        ball_and_stick_cull_depth: None,
        space_filling_cull_depth: None,
    }
}

/// Result meshes from one tessellation call.
struct Meshes {
    atom: AtomImpostorMesh,
    bond: BondImpostorMesh,
    transparent: TransparentImpostorMesh,
}

fn tessellate(
    structure: &AtomicStructure,
    prefs: &AtomicStructureVisualizationPreferences,
) -> Meshes {
    let mut atom = AtomImpostorMesh::new();
    let mut bond = BondImpostorMesh::new();
    let mut transparent = TransparentImpostorMesh::new();
    tessellate_atomic_structure_impostors(&mut atom, &mut bond, &mut transparent, structure, prefs);
    Meshes {
        atom,
        bond,
        transparent,
    }
}

/// Count of quads in an opaque atom impostor mesh (4 vertices / 6 indices each).
fn atom_quads(m: &AtomImpostorMesh) -> usize {
    assert_eq!(m.vertices.len() % 4, 0, "atom vertices not a multiple of 4");
    assert_eq!(m.indices.len(), m.vertices.len() / 4 * 6);
    m.vertices.len() / 4
}

fn bond_quads(m: &BondImpostorMesh) -> usize {
    assert_eq!(m.vertices.len() % 4, 0, "bond vertices not a multiple of 4");
    assert_eq!(m.indices.len(), m.vertices.len() / 4 * 6);
    m.vertices.len() / 4
}

fn transparent_quads(m: &TransparentImpostorMesh) -> usize {
    assert_eq!(
        m.vertices.len() % 4,
        0,
        "transparent vertices not a multiple of 4"
    );
    assert_eq!(m.indices.len(), m.vertices.len() / 4 * 6);
    assert_eq!(
        m.quad_centers.len(),
        m.vertices.len() / 4,
        "one quad_center per quad"
    );
    m.vertices.len() / 4
}

/// Number of transparent quads of a given kind (0 = atom, 1 = bond).
fn transparent_quads_of_kind(m: &TransparentImpostorMesh, kind: u32) -> usize {
    // Each quad is 4 identical-kind vertices; count and divide.
    let verts = m.vertices.iter().filter(|v| v.kind == kind).count();
    assert_eq!(verts % 4, 0);
    verts / 4
}

// ============================================================================
// Atom routing
// ============================================================================

/// No alphas → transparent mesh empty; opaque atom mesh carries every atom.
#[test]
fn no_alpha_leaves_transparent_mesh_empty() {
    let mut s = AtomicStructure::new();
    s.add_atom(6, DVec3::new(-1.0, 0.0, 0.0));
    s.add_atom(6, DVec3::new(1.0, 0.0, 0.0));

    let m = tessellate(&s, &ball_and_stick_prefs());

    assert_eq!(atom_quads(&m.atom), 2, "both atoms opaque");
    assert_eq!(m.transparent.vertices.len(), 0, "transparent mesh empty");
    assert_eq!(m.transparent.indices.len(), 0);
    assert_eq!(m.transparent.quad_centers.len(), 0);
}

/// Alpha on a subset → exactly those atoms route transparent (kind 0, alpha on
/// every vertex); the rest stay opaque; total atom count conserved.
#[test]
fn alpha_subset_routes_only_those_atoms() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(-1.0, 0.0, 0.0));
    s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    s.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    s.set_atom_alpha(a, 0.3);

    let m = tessellate(&s, &ball_and_stick_prefs());

    assert_eq!(atom_quads(&m.atom), 2, "two opaque atoms remain");
    assert_eq!(transparent_quads(&m.transparent), 1, "one ghost atom");
    assert_eq!(transparent_quads_of_kind(&m.transparent, 0), 1, "kind 0");
    for v in &m.transparent.vertices {
        assert_eq!(v.kind, 0);
        assert!((v.alpha - 0.3).abs() < 1e-6, "alpha on every vertex");
    }
    // Totals conserved: 3 atoms in, 2 opaque + 1 transparent out.
    assert_eq!(atom_quads(&m.atom) + transparent_quads(&m.transparent), 3);
}

// ============================================================================
// Bond routing — min-alpha rule
// ============================================================================

/// Both endpoints ghosted → bond transparent (kind 1) with the min alpha.
#[test]
fn bond_both_endpoints_transparent_uses_min_alpha() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    s.add_bond(a, b, BOND_SINGLE);
    s.set_atom_alpha(a, 0.6);
    s.set_atom_alpha(b, 0.2);

    let m = tessellate(&s, &ball_and_stick_prefs());

    assert_eq!(bond_quads(&m.bond), 0, "no opaque bond");
    assert_eq!(
        transparent_quads_of_kind(&m.transparent, 1),
        1,
        "ghost bond"
    );
    let bond_vert = m.transparent.vertices.iter().find(|v| v.kind == 1).unwrap();
    assert!(
        (bond_vert.alpha - 0.2).abs() < 1e-6,
        "bond alpha = min(0.6, 0.2)"
    );
}

/// One ghosted endpoint, one opaque → bond still transparent with the lower
/// alpha (min < 1.0).
#[test]
fn bond_mixed_endpoints_routes_transparent() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    s.add_bond(a, b, BOND_SINGLE);
    s.set_atom_alpha(a, 0.4);
    // b left fully opaque (1.0).

    let m = tessellate(&s, &ball_and_stick_prefs());

    assert_eq!(bond_quads(&m.bond), 0, "no opaque bond");
    assert_eq!(transparent_quads_of_kind(&m.transparent, 1), 1);
    let bond_vert = m.transparent.vertices.iter().find(|v| v.kind == 1).unwrap();
    assert!(
        (bond_vert.alpha - 0.4).abs() < 1e-6,
        "bond alpha = min(0.4, 1.0)"
    );
    // One opaque atom (b), one ghost atom (a).
    assert_eq!(atom_quads(&m.atom), 1);
    assert_eq!(transparent_quads_of_kind(&m.transparent, 0), 1);
}

/// Both endpoints opaque → bond routes to the opaque mesh, transparent empty.
#[test]
fn bond_both_opaque_stays_opaque() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    s.add_bond(a, b, BOND_SINGLE);

    let m = tessellate(&s, &ball_and_stick_prefs());

    assert_eq!(bond_quads(&m.bond), 1, "opaque bond");
    assert_eq!(m.transparent.vertices.len(), 0, "transparent empty");
}

// ============================================================================
// quad_centers bookkeeping
// ============================================================================

/// Atom quads record the atom center; bond quads record the segment midpoint;
/// and `quad_centers.len() * 6 == indices.len()`.
#[test]
fn quad_centers_track_atom_center_and_bond_midpoint() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(-1.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    s.add_bond(a, b, BOND_SINGLE);
    s.set_atom_alpha(a, 0.5);
    s.set_atom_alpha(b, 0.5);

    let m = tessellate(&s, &ball_and_stick_prefs());

    // 2 atom quads + 1 bond quad.
    assert_eq!(transparent_quads(&m.transparent), 3);
    assert_eq!(
        m.transparent.quad_centers.len() * 6,
        m.transparent.indices.len()
    );

    // Atom centers present.
    let has_center = |x: f32| {
        m.transparent
            .quad_centers
            .iter()
            .any(|c| (c.x - x).abs() < 1e-6 && c.y.abs() < 1e-6 && c.z.abs() < 1e-6)
    };
    assert!(has_center(-1.0), "atom a center recorded");
    assert!(has_center(3.0), "atom b center recorded");
    // Bond midpoint of (-1,0,0)-(3,0,0) is (1,0,0).
    assert!(has_center(1.0), "bond midpoint recorded");
}

// ============================================================================
// Refactor guards — delete markers and space-filling filtering
// ============================================================================

/// A delete-marker bond (bond_order 0 in a diff structure) with ghosted
/// endpoints routes through the transparent path, not the opaque one.
#[test]
fn delete_marker_bond_routes_transparent() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    s.add_bond(a, b, BOND_DELETED);
    s.set_is_diff(true);
    s.set_atom_alpha(a, 0.5);
    s.set_atom_alpha(b, 0.5);

    let m = tessellate(&s, &ball_and_stick_prefs());

    assert_eq!(bond_quads(&m.bond), 0, "no opaque delete marker");
    assert_eq!(
        transparent_quads_of_kind(&m.transparent, 1),
        1,
        "delete marker ghosted"
    );
}

/// Space-filling still filters non-overstretched bonds: two touching carbons
/// produce ghost atoms but no bond, in either mesh.
#[test]
fn space_filling_non_overstretched_bond_still_filtered() {
    let mut s = AtomicStructure::new();
    // Carbons ~1.5 Å apart — well within their van der Waals spheres, so the
    // bond is not overstretched and is not drawn in space-filling mode.
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    s.add_bond(a, b, BOND_SINGLE);
    s.set_atom_alpha(a, 0.5);
    s.set_atom_alpha(b, 0.5);

    let m = tessellate(&s, &space_filling_prefs());

    assert_eq!(
        transparent_quads_of_kind(&m.transparent, 0),
        2,
        "ghost atoms"
    );
    assert_eq!(
        transparent_quads_of_kind(&m.transparent, 1),
        0,
        "bond filtered, none transparent"
    );
    assert_eq!(bond_quads(&m.bond), 0, "bond filtered, none opaque");
}

// ============================================================================
// Scene-level routing (Phase 4) — the seam the renderer actually consumes.
// ============================================================================

/// A default camera looking at the origin; the non-lightweight tessellation
/// path the transparent mesh comes from does not use it, but the entry point
/// requires one.
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

/// Minimal display preferences with the given atomic rendering method; other
/// fields are inert for this test (no geometry, no background rendering hit).
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
            visualization: AtomicStructureVisualization::BallAndStick,
            rendering_method,
            ball_and_stick_cull_depth: None,
            space_filling_cull_depth: None,
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

/// Build a one-node scene whose displayed output is an alpha-carrying molecule.
fn scene_with_ghost_atoms() -> StructureDesignerScene {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(-1.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    s.add_bond(a, b, BOND_SINGLE);
    s.set_atom_alpha(a, 0.3);
    s.set_atom_alpha(b, 0.3);

    let mut scene = StructureDesignerScene::new();
    scene
        .node_data
        .insert(0, NodeSceneData::new(NodeOutput::Atomic(s, None)));
    scene
}

/// `tessellate_scene_content` with impostor prefs routes the alpha-carrying
/// structure into a non-empty transparent mesh — the exact value the renderer
/// uploads and draws.
#[test]
fn scene_impostor_mode_returns_nonempty_transparent_mesh() {
    let scene = scene_with_ghost_atoms();
    let (.., transparent, _gadget_atoms, _gadget_bonds) = tessellate_scene_content(
        &scene,
        &test_camera(),
        false, // not lightweight
        &display_prefs(AtomicRenderingMethod::Impostors),
    );

    // Two ghost atoms + one ghost bond (both endpoints ghosted).
    assert_eq!(transparent_quads_of_kind(&transparent, 0), 2, "ghost atoms");
    assert_eq!(transparent_quads_of_kind(&transparent, 1), 1, "ghost bond");
    assert!(!transparent.vertices.is_empty());
}

/// In `TriangleMesh` mode atoms tessellate opaque; the transparent mesh the
/// renderer consumes stays empty regardless of alpha.
#[test]
fn scene_triangle_mesh_mode_leaves_transparent_empty() {
    let scene = scene_with_ghost_atoms();
    let (.., transparent, _gadget_atoms, _gadget_bonds) = tessellate_scene_content(
        &scene,
        &test_camera(),
        false, // not lightweight
        &display_prefs(AtomicRenderingMethod::TriangleMesh),
    );

    assert_eq!(
        transparent.vertices.len(),
        0,
        "transparent empty in mesh mode"
    );
    assert_eq!(transparent.indices.len(), 0);
    assert_eq!(transparent.quad_centers.len(), 0);
}
