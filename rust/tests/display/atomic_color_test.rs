//! Phase 1 of `doc/design_style_rules.md` — per-atom `atom_color` override
//! consumed at the tessellation seam.
//!
//! These CPU-only display tests (no GPU) exercise renderer consumption of the
//! decorator's `atom_color` map in both the impostor and triangle-mesh paths:
//! the override replaces the element-derived albedo, the marker / param-element
//! colors win over it, ghost desaturation applies on top of the override, the
//! selection albedo/rim stay above it, and a styled + ghosted atom routes into
//! the transparent mesh carrying the override color.

use glam::Vec3;
use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::display::atomic_tessellator::{
    AtomicTessellatorParams, tessellate_atomic_structure, tessellate_atomic_structure_impostors,
};
use rust_lib_flutter_cad::display::preferences::{
    AtomicRenderingMethod, AtomicStructureVisualization, AtomicStructureVisualizationPreferences,
};
use rust_lib_flutter_cad::renderer::atom_impostor_mesh::AtomImpostorMesh;
use rust_lib_flutter_cad::renderer::bond_impostor_mesh::BondImpostorMesh;
use rust_lib_flutter_cad::renderer::mesh::Mesh;
use rust_lib_flutter_cad::renderer::transparent_impostor_mesh::TransparentImpostorMesh;

// ============================================================================
// Helpers
// ============================================================================

fn ball_and_stick_prefs() -> AtomicStructureVisualizationPreferences {
    AtomicStructureVisualizationPreferences {
        visualization: AtomicStructureVisualization::BallAndStick,
        rendering_method: AtomicRenderingMethod::Impostors,
        ball_and_stick_cull_depth: None,
        space_filling_cull_depth: None,
        scene_transparency_enabled: false,
        scene_alpha: 1.0,
        label_scale: 0.7,
    }
}

fn mesh_params() -> AtomicTessellatorParams {
    AtomicTessellatorParams {
        ball_and_stick_sphere_horizontal_divisions: 12,
        ball_and_stick_sphere_vertical_divisions: 6,
        space_filling_sphere_horizontal_divisions: 36,
        space_filling_sphere_vertical_divisions: 18,
        cylinder_divisions: 8,
    }
}

/// Impostor-tessellate a structure with ball-and-stick prefs; return the opaque
/// atom mesh and the transparent mesh.
fn tessellate_impostors(s: &AtomicStructure) -> (AtomImpostorMesh, TransparentImpostorMesh) {
    let mut atom = AtomImpostorMesh::new();
    let mut bond = BondImpostorMesh::new();
    let mut transparent = TransparentImpostorMesh::new();
    tessellate_atomic_structure_impostors(
        &mut atom,
        &mut bond,
        &mut transparent,
        s,
        &ball_and_stick_prefs(),
    );
    (atom, transparent)
}

/// Triangle-mesh-tessellate a single-atom structure; every vertex belongs to
/// that one atom's sphere, so they all share one albedo.
fn tessellate_mesh(s: &AtomicStructure) -> Mesh {
    let mut mesh = Mesh::new();
    tessellate_atomic_structure(&mut mesh, s, &mesh_params(), &ball_and_stick_prefs());
    mesh
}

fn approx(a: [f32; 3], b: [f32; 3]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6 && (a[2] - b[2]).abs() < 1e-6
}

// ============================================================================
// Impostor path
// ============================================================================

/// The styled atom's quad carries the override albedo; the un-styled neighbor
/// keeps a different (element) albedo.
#[test]
fn impostor_styled_atom_carries_override_albedo() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(-1.0, 0.0, 0.0));
    s.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    let override_c = Vec3::new(0.2, 0.4, 0.6);
    s.set_atom_color(a, override_c);

    let (atom_mesh, _) = tessellate_impostors(&s);
    let arr = override_c.to_array();

    assert!(
        atom_mesh.vertices.iter().any(|v| approx(v.albedo, arr)),
        "styled atom carries the override albedo"
    );
    assert!(
        atom_mesh.vertices.iter().any(|v| !approx(v.albedo, arr)),
        "neighbor carries the element-derived albedo, not the override"
    );
}

/// Delete-marker, unchanged-marker, and param-element atoms are semantic UI and
/// keep their fixed colors even with an override set.
#[test]
fn impostor_marker_and_param_atoms_ignore_override() {
    let red = Vec3::new(1.0, 0.0, 0.0);

    // Delete marker (atomic_number 0): dark neutral impostor albedo [0,0,0].
    let mut s = AtomicStructure::new();
    let id = s.add_atom(0, DVec3::ZERO);
    s.set_atom_color(id, red);
    let (m, _) = tessellate_impostors(&s);
    assert!(
        m.vertices.iter().all(|v| approx(v.albedo, [0.0, 0.0, 0.0])),
        "delete marker keeps its fixed albedo"
    );

    // Unchanged marker (atomic_number -1): light blue (0.4, 0.6, 0.9).
    let mut s = AtomicStructure::new();
    let id = s.add_atom(-1, DVec3::ZERO);
    s.set_atom_color(id, red);
    let (m, _) = tessellate_impostors(&s);
    assert!(
        m.vertices.iter().all(|v| approx(v.albedo, [0.4, 0.6, 0.9])),
        "unchanged marker keeps its fixed albedo"
    );

    // Param element (PARAM_ELEMENT_BASE = -100): first param color (0.9,0.4,0.9).
    let mut s = AtomicStructure::new();
    let id = s.add_atom(-100, DVec3::ZERO);
    s.set_atom_color(id, red);
    let (m, _) = tessellate_impostors(&s);
    assert!(
        m.vertices.iter().all(|v| approx(v.albedo, [0.9, 0.4, 0.9])),
        "param element keeps its fixed color"
    );
}

/// A ghost-state styled atom gets the *desaturated* override (blend 50% toward
/// gray), proving desaturation applies on top of the override.
#[test]
fn impostor_ghost_styled_atom_desaturates_the_override() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::ZERO);
    s.set_atom_color(a, Vec3::new(0.2, 0.4, 0.6));
    s.set_atom_ghost(a, true);

    let (m, _) = tessellate_impostors(&s);

    // lerp((0.2,0.4,0.6), (0.5,0.5,0.5), 0.5) = (0.35, 0.45, 0.55)
    assert!(
        m.vertices
            .iter()
            .all(|v| approx(v.albedo, [0.35, 0.45, 0.55])),
        "ghost desaturation applied on top of the override"
    );
}

/// A selected styled atom keeps its selection rim; the impostor path never folds
/// selection into albedo, so the albedo stays the override.
#[test]
fn impostor_selected_styled_atom_keeps_rim_and_override_albedo() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::ZERO);
    let override_c = Vec3::new(0.2, 0.4, 0.6);
    s.set_atom_color(a, override_c);
    s.set_atom_selected(a, true);

    let (m, _) = tessellate_impostors(&s);
    let arr = override_c.to_array();

    assert!(
        m.vertices.iter().all(|v| approx(v.albedo, arr)),
        "impostor albedo stays the override on a selected atom"
    );
    // SELECTED_RIM_COLOR = [1.0, 0.55, 0.0, 1.0] — non-zero rim (not NO_RIM).
    assert!(
        m.vertices.iter().all(|v| v.rim_color[3] > 0.0),
        "selected styled atom keeps a visible selection rim"
    );
}

// ============================================================================
// Triangle-mesh path
// ============================================================================

/// The override lands in the sphere material's albedo in the mesh path.
#[test]
fn mesh_override_lands_in_sphere_material() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::ZERO);
    let override_c = Vec3::new(0.2, 0.4, 0.6);
    s.set_atom_color(a, override_c);

    let mesh = tessellate_mesh(&s);
    let arr = override_c.to_array();

    assert!(!mesh.vertices.is_empty(), "sphere tessellated");
    assert!(
        mesh.vertices.iter().all(|v| approx(v.albedo, arr)),
        "every sphere vertex carries the override albedo"
    );
}

/// A *selected* styled atom still shows the selection albedo (selection folds
/// into albedo in the mesh path and stays above the style color).
#[test]
fn mesh_selected_styled_atom_shows_selection_albedo() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::ZERO);
    s.set_atom_color(a, Vec3::new(0.2, 0.4, 0.6));
    s.set_atom_selected(a, true);

    let mesh = tessellate_mesh(&s);

    // to_selected_color() returns fixed selection orange (0.9, 0.5, 0.0).
    assert!(
        mesh.vertices
            .iter()
            .all(|v| approx(v.albedo, [0.9, 0.5, 0.0])),
        "selection albedo stays above the style color"
    );
}

// ============================================================================
// Transparent routing (color + alpha compose)
// ============================================================================

/// An atom with both a color override and `atom_alpha < 1` routes into the
/// transparent mesh carrying the override color.
#[test]
fn styled_and_ghosted_atom_routes_transparent_with_override() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::ZERO);
    let override_c = Vec3::new(0.2, 0.4, 0.6);
    s.set_atom_color(a, override_c);
    s.set_atom_alpha(a, 0.3);

    let (atom_mesh, transparent) = tessellate_impostors(&s);
    let arr = override_c.to_array();

    assert_eq!(atom_mesh.vertices.len(), 0, "styled atom is not opaque");
    let atom_verts: Vec<_> = transparent
        .vertices
        .iter()
        .filter(|v| v.kind == 0)
        .collect();
    assert_eq!(atom_verts.len(), 4, "one transparent atom quad");
    assert!(
        atom_verts.iter().all(|v| approx(v.color, arr)),
        "transparent atom carries the override color"
    );
    assert!(
        atom_verts.iter().all(|v| (v.alpha - 0.3).abs() < 1e-6),
        "and its alpha"
    );
}
