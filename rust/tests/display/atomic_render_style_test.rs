//! Phase 3 of `doc/design_style_rules.md` — per-atom render-style override
//! (`AtomRenderStyle`) consumed at the tessellation and picking seams.
//!
//! These CPU-only tests (no GPU) exercise the four behaviors driven by the
//! decorator's `atom_render_style` map: per-atom radius / sphere subdivisions,
//! Decision 1's bond matrix (a bond is ball-and-stick iff at least one endpoint
//! is), Decision 2's both-thresholds culling, Decision 3's occluder narrowing,
//! and `hit_test`'s per-atom bond pickability. No-override scenes stay
//! byte-identical to the legacy single-global-mode behavior.

use glam::f64::DVec3;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization as ApiVisualization;
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    Atom, AtomRenderStyle, AtomicStructure, HitTestResult,
};
use rust_lib_flutter_cad::display::atomic_tessellator::{
    AtomicTessellatorParams, BAS_STICK_RADIUS, effective_displayed_atom_radius,
    get_displayed_atom_radius, tessellate_atomic_structure, tessellate_atomic_structure_impostors,
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

const BS: AtomicStructureVisualization = AtomicStructureVisualization::BallAndStick;
const SF: AtomicStructureVisualization = AtomicStructureVisualization::SpaceFilling;

fn prefs(
    viz: AtomicStructureVisualization,
    bs_cull: Option<f64>,
    sf_cull: Option<f64>,
) -> AtomicStructureVisualizationPreferences {
    AtomicStructureVisualizationPreferences {
        visualization: viz,
        rendering_method: AtomicRenderingMethod::Impostors,
        ball_and_stick_cull_depth: bs_cull,
        space_filling_cull_depth: sf_cull,
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

fn impostors(
    s: &AtomicStructure,
    p: &AtomicStructureVisualizationPreferences,
) -> (AtomImpostorMesh, BondImpostorMesh, TransparentImpostorMesh) {
    let mut atom = AtomImpostorMesh::new();
    let mut bond = BondImpostorMesh::new();
    let mut transparent = TransparentImpostorMesh::new();
    tessellate_atomic_structure_impostors(&mut atom, &mut bond, &mut transparent, s, p);
    (atom, bond, transparent)
}

fn mesh_vertex_count(s: &AtomicStructure, p: &AtomicStructureVisualizationPreferences) -> usize {
    let mut m = Mesh::new();
    tessellate_atomic_structure(&mut m, s, &mesh_params(), p);
    m.vertices.len()
}

/// The displayed impostor radius a lone carbon gets in a given global mode.
fn single_carbon_radius(viz: AtomicStructureVisualization) -> f64 {
    let mut s = AtomicStructure::new();
    s.add_atom(6, DVec3::ZERO);
    let (m, _, _) = impostors(&s, &prefs(viz, None, None));
    m.vertices[0].radius as f64
}

/// A per-atom effective-visualization radius closure for `hit_test` — the same
/// `effective_displayed_atom_radius` every production caller injects.
fn effective_radius(
    s: &AtomicStructure,
    global: AtomicStructureVisualization,
) -> impl Fn(&Atom) -> f64 + '_ {
    move |atom: &Atom| effective_displayed_atom_radius(s, atom, &global)
}

// ============================================================================
// Per-atom radius / subdivisions
// ============================================================================

/// A space-filling-styled atom carries the vdW impostor radius while its
/// un-styled neighbor keeps the ball-and-stick radius (global mode B&S).
#[test]
fn impostor_styled_atom_uses_vdw_radius_neighbor_stays_ball_and_stick() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(-2.0, 0.0, 0.0));
    s.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    s.set_atom_render_style(a, AtomRenderStyle::SpaceFilling);

    let (atom_mesh, _, _) = impostors(&s, &prefs(BS, None, None));

    let sf_radius = single_carbon_radius(SF);
    let bas_radius = single_carbon_radius(BS);
    assert!(sf_radius > bas_radius);

    for v in &atom_mesh.vertices {
        if v.center_position == [-2.0, 0.0, 0.0] {
            assert!(
                (v.radius as f64 - sf_radius).abs() < 1e-5,
                "styled atom at vdW radius"
            );
        } else if v.center_position == [2.0, 0.0, 0.0] {
            assert!(
                (v.radius as f64 - bas_radius).abs() < 1e-5,
                "neighbor stays ball-and-stick radius"
            );
        } else {
            panic!(
                "unexpected impostor vertex position {:?}",
                v.center_position
            );
        }
    }
}

/// The mesh path picks sphere subdivision counts from the atom's effective mode:
/// a space-filling-styled atom in a global ball-and-stick scene tessellates with
/// the (denser) space-filling subdivision counts.
#[test]
fn mesh_styled_atom_uses_effective_mode_subdivisions() {
    let mut carbon_sf = AtomicStructure::new();
    carbon_sf.add_atom(6, DVec3::ZERO);
    let sf_verts = mesh_vertex_count(&carbon_sf, &prefs(SF, None, None));

    let mut carbon_bs = AtomicStructure::new();
    carbon_bs.add_atom(6, DVec3::ZERO);
    let bs_verts = mesh_vertex_count(&carbon_bs, &prefs(BS, None, None));
    assert_ne!(
        sf_verts, bs_verts,
        "the two modes use different subdivisions"
    );

    let mut styled = AtomicStructure::new();
    let a = styled.add_atom(6, DVec3::ZERO);
    styled.set_atom_render_style(a, AtomRenderStyle::SpaceFilling);
    // Global mode is ball-and-stick, but the styled atom follows space-filling.
    assert_eq!(mesh_vertex_count(&styled, &prefs(BS, None, None)), sf_verts);
}

// ============================================================================
// Decision 1 — bond matrix
// ============================================================================

/// A bond with at least one ball-and-stick endpoint renders as a stick bond
/// (drawn, at `BAS_STICK_RADIUS`) even in a global space-filling scene.
#[test]
fn bond_matrix_ball_and_stick_endpoint_draws_stick_bond() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    s.add_bond(a, b, 1);
    s.set_atom_render_style(a, AtomRenderStyle::BallAndStick);

    let (_, bond_mesh, _) = impostors(&s, &prefs(SF, None, None));
    assert!(!bond_mesh.vertices.is_empty(), "mixed B&S–SF bond is drawn");
    assert!(
        bond_mesh
            .vertices
            .iter()
            .all(|v| (v.radius as f64 - BAS_STICK_RADIUS).abs() < 1e-6),
        "at stick radius"
    );
}

/// A bond whose endpoints are both space-filling renders only when overstretched
/// (and then at `SPACE_FILLING_BOND_RADIUS_SCALE`× the stick radius).
#[test]
fn bond_matrix_space_filling_pair_absent_unless_overstretched() {
    // Close carbons (spheres overlap) → not overstretched → no bond.
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    s.add_bond(a, b, 1);
    let (_, bond_mesh, _) = impostors(&s, &prefs(SF, None, None));
    assert!(
        bond_mesh.vertices.is_empty(),
        "SF–SF non-overstretched bond is absent"
    );

    // Far carbons (spheres don't touch) → overstretched → drawn at 4× radius.
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(6.0, 0.0, 0.0));
    s.add_bond(a, b, 1);
    let (_, bond_mesh, _) = impostors(&s, &prefs(SF, None, None));
    assert!(
        !bond_mesh.vertices.is_empty(),
        "overstretched SF–SF bond is drawn"
    );
    let expected = (BAS_STICK_RADIUS * 4.0) as f32;
    assert!(
        bond_mesh
            .vertices
            .iter()
            .all(|v| (v.radius - expected).abs() < 1e-6),
        "at 4× stick radius"
    );
}

/// A plain ball-and-stick scene (no overrides) draws bonds at stick radius.
#[test]
fn bond_matrix_no_override_ball_and_stick_stick_radius() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    s.add_bond(a, b, 1);
    let (_, bond_mesh, _) = impostors(&s, &prefs(BS, None, None));
    assert!(!bond_mesh.vertices.is_empty());
    assert!(
        bond_mesh
            .vertices
            .iter()
            .all(|v| (v.radius as f64 - BAS_STICK_RADIUS).abs() < 1e-6)
    );
}

/// The bond-alpha min-rule is unchanged on a mixed (B&S–SF) bond: a bond with a
/// half-transparent endpoint routes into the transparent mesh at that alpha.
#[test]
fn bond_matrix_mixed_bond_keeps_alpha_min_rule() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    s.add_bond(a, b, 1);
    s.set_atom_render_style(a, AtomRenderStyle::SpaceFilling); // mixed bond
    s.set_atom_alpha(b, 0.5);

    let (_, bond_mesh, transparent) = impostors(&s, &prefs(BS, None, None));
    assert!(bond_mesh.vertices.is_empty(), "faded bond is not opaque");
    let bond_verts: Vec<_> = transparent
        .vertices
        .iter()
        .filter(|v| v.kind == 1)
        .collect();
    assert!(
        !bond_verts.is_empty(),
        "the bond routed to the transparent mesh"
    );
    assert!(
        bond_verts.iter().all(|v| (v.alpha - 0.5).abs() < 1e-6),
        "bond alpha = min endpoint alpha"
    );
}

// ============================================================================
// Decision 2 — depth culling (both thresholds)
// ============================================================================

/// Headline case: a space-filling-styled dopant at depth 5 inside a global
/// ball-and-stick crystal (B&S threshold 8, SF threshold 3) stays visible — it
/// exceeds only the effective SF threshold, not both.
#[test]
fn culling_space_filling_dopant_visible_in_ball_and_stick_crystal() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::ZERO);
    s.set_atom_render_style(a, AtomRenderStyle::SpaceFilling);
    s.set_atom_in_crystal_depth(a, 5.0);

    let (atom_mesh, _, _) = impostors(&s, &prefs(BS, Some(8.0), Some(3.0)));
    assert_eq!(atom_mesh.vertices.len(), 4, "dopant not culled");
}

/// Mirrored case: a ball-and-stick-styled interior atom at depth 5 in a global
/// space-filling crystal stays visible.
#[test]
fn culling_ball_and_stick_dopant_visible_in_space_filling_crystal() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::ZERO);
    s.set_atom_render_style(a, AtomRenderStyle::BallAndStick);
    s.set_atom_in_crystal_depth(a, 5.0);

    let (atom_mesh, _, _) = impostors(&s, &prefs(SF, Some(8.0), Some(3.0)));
    assert_eq!(atom_mesh.vertices.len(), 4, "interior atom not culled");
}

/// A match-all space-filling restyle keeps the global mode's culling budget: an
/// atom past the global B&S threshold (8) is still culled.
#[test]
fn culling_match_all_space_filling_keeps_global_budget() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::ZERO);
    s.set_atom_render_style(a, AtomRenderStyle::SpaceFilling);
    s.set_atom_in_crystal_depth(a, 9.0);

    let (atom_mesh, _, _) = impostors(&s, &prefs(BS, Some(8.0), Some(3.0)));
    assert_eq!(
        atom_mesh.vertices.len(),
        0,
        "atom past global budget is culled"
    );
}

/// With no overrides the both-thresholds rule collapses to a single global
/// threshold, byte-identical to the legacy behavior.
#[test]
fn culling_no_overrides_matches_single_threshold() {
    let mut s = AtomicStructure::new();
    let shallow = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let deep = s.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    s.set_atom_in_crystal_depth(shallow, 5.0);
    s.set_atom_in_crystal_depth(deep, 9.0);

    let (atom_mesh, _, _) = impostors(&s, &prefs(BS, Some(8.0), None));
    assert_eq!(
        atom_mesh.vertices.len(),
        4,
        "only the shallow atom survives"
    );
    assert!(
        atom_mesh
            .vertices
            .iter()
            .all(|v| v.center_position == [0.0, 0.0, 0.0])
    );
}

// ============================================================================
// Decision 3 — occluders
// ============================================================================

/// A transparent space-filling neighbor must NOT occlude (opaque-only filter):
/// with the neighbor opaque, the tessellated atom's hidden cap is culled and the
/// mesh has strictly fewer vertices than when the neighbor is transparent.
#[test]
fn occluders_transparent_space_filling_neighbor_does_not_occlude() {
    let build = |b_alpha: f32| {
        let mut s = AtomicStructure::new();
        let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
        s.add_bond(a, b, 1);
        if b_alpha < 1.0 {
            s.set_atom_alpha(b, b_alpha);
        }
        mesh_vertex_count(&s, &prefs(SF, None, None))
    };

    let opaque = build(1.0);
    let transparent = build(0.5);
    assert!(
        opaque < transparent,
        "opaque neighbor occludes (fewer verts: {opaque}); transparent does not ({transparent})"
    );
}

// ============================================================================
// Picking — per-atom bond pickability + radius
// ============================================================================

/// A space-filling-styled atom is picked at its vdW radius: a ray offset beyond
/// the ball-and-stick radius but within the vdW radius hits it.
#[test]
fn hit_test_styled_space_filling_atom_picked_at_vdw_radius() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::ZERO);
    s.set_atom_render_style(a, AtomRenderStyle::SpaceFilling);

    let bas_radius = single_carbon_radius(BS);
    let sf_radius = single_carbon_radius(SF);
    let offset = 1.0_f64;
    assert!(
        offset > bas_radius && offset < sf_radius,
        "ray offset isolates the two radii"
    );

    let hit = s.hit_test(
        &DVec3::new(offset, 0.0, 10.0),
        &DVec3::new(0.0, 0.0, -1.0),
        &ApiVisualization::BallAndStick,
        effective_radius(&s, BS),
        BAS_STICK_RADIUS,
    );
    assert!(
        matches!(hit, HitTestResult::Atom(id, _) if id == a),
        "styled atom picked at its vdW radius, got {hit:?}"
    );
}

/// A mixed (B&S–SF) bond is pickable: a ray through the bond stick, clear of both
/// atoms' pick spheres, returns the bond.
#[test]
fn hit_test_mixed_bond_is_pickable() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(5.0, 0.0, 0.0));
    s.add_bond(a, b, 1);
    s.set_atom_render_style(a, AtomRenderStyle::BallAndStick);

    let hit = s.hit_test(
        &DVec3::new(2.5, 0.0, 10.0),
        &DVec3::new(0.0, 0.0, -1.0),
        &ApiVisualization::SpaceFilling,
        effective_radius(&s, SF),
        BAS_STICK_RADIUS,
    );
    assert!(
        matches!(hit, HitTestResult::Bond(_, _)),
        "mixed B&S–SF bond is pickable, got {hit:?}"
    );
}

/// An overstretched space-filling–space-filling bond stays rendered-but-
/// unpickable: the same ray through its stick returns nothing.
#[test]
fn hit_test_overstretched_space_filling_bond_not_pickable() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(5.0, 0.0, 0.0));
    s.add_bond(a, b, 1);

    let hit = s.hit_test(
        &DVec3::new(2.5, 0.0, 10.0),
        &DVec3::new(0.0, 0.0, -1.0),
        &ApiVisualization::SpaceFilling,
        effective_radius(&s, SF),
        BAS_STICK_RADIUS,
    );
    assert!(
        matches!(hit, HitTestResult::None),
        "SF–SF bond stays unpickable, got {hit:?}"
    );
}

/// No-override picking is byte-identical to the legacy behavior: a plain
/// ball-and-stick scene still picks bonds.
#[test]
fn hit_test_no_overrides_ball_and_stick_bond_pickable() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    s.add_bond(a, b, 1);

    let radius_fn = |atom: &Atom| get_displayed_atom_radius(atom, &BS);
    let hit = s.hit_test(
        &DVec3::new(1.5, 0.0, 10.0),
        &DVec3::new(0.0, 0.0, -1.0),
        &ApiVisualization::BallAndStick,
        radius_fn,
        BAS_STICK_RADIUS,
    );
    assert!(matches!(hit, HitTestResult::Bond(_, _)), "got {hit:?}");
}
