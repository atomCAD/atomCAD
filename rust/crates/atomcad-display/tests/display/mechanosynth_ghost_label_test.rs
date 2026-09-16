//! The element-symbol labels on the `mechanosynth_edit` ghost preview.
//!
//! A ghost's albedo says what the step *does* (green = added), so it cannot
//! also say what the atom *is* — and a Cl and a Si ghost at the same site are
//! otherwise the same translucent sphere. `tessellate_mechanosynth_ghosts_impostors`
//! therefore writes the element symbol into the label mesh for the kinds where
//! the identity is the news. These CPU-only tests (no GPU) assert mesh
//! *contents*, not pixels, like `atomic_color_test.rs`.
//!
//! What is worth guarding: which kinds get a label (Added and Changed, not
//! Moved or Deleted), that a near miss keeps its label, that the label sits on
//! the ghost's *destination* and clears its sphere, that it honours the
//! display's `label_scale`, and that labelling leaves the sphere pass alone.

use atomcad_crystolecule::atomic_structure::atomic_structure_decorator::MechanosynthGhostVisuals;
use atomcad_crystolecule::mechanosynth::place::{GhostAtom, GhostKind};
use atomcad_display::atomic_tessellator::tessellate_mechanosynth_ghosts_impostors;
use atomcad_display::preferences::{
    AtomicRenderingMethod, AtomicStructureVisualization, AtomicStructureVisualizationPreferences,
};
use atomcad_renderer::label_mesh::LabelMesh;
use atomcad_renderer::transparent_impostor_mesh::TransparentImpostorMesh;
use glam::f64::DVec3;

// ============================================================================
// Helpers
// ============================================================================

const SILICON: i16 = 14;
const CHLORINE: i16 = 17;
const HYDROGEN: i16 = 1;

fn prefs(label_scale: f32) -> AtomicStructureVisualizationPreferences {
    AtomicStructureVisualizationPreferences {
        visualization: AtomicStructureVisualization::BallAndStick,
        rendering_method: AtomicRenderingMethod::Impostors,
        ball_and_stick_cull_depth: None,
        space_filling_cull_depth: None,
        scene_transparency_enabled: false,
        scene_alpha: 1.0,
        label_scale,
    }
}

fn ghost(kind: GhostKind, atomic_number: i16, position: DVec3) -> GhostAtom {
    GhostAtom {
        kind,
        position,
        from: position,
        atomic_number,
    }
}

fn visuals(ghosts: Vec<GhostAtom>, near_miss: bool) -> MechanosynthGhostVisuals {
    MechanosynthGhostVisuals {
        ghosts,
        bonds: Vec::new(),
        near_miss,
    }
}

fn tessellate(
    v: &MechanosynthGhostVisuals,
    label_scale: f32,
) -> (TransparentImpostorMesh, LabelMesh) {
    let mut transparent = TransparentImpostorMesh::new();
    let mut labels = LabelMesh::new();
    tessellate_mechanosynth_ghosts_impostors(&mut transparent, &mut labels, v, &prefs(label_scale));
    (transparent, labels)
}

/// Quads in the label mesh — one per character.
fn glyph_quads(mesh: &LabelMesh) -> usize {
    assert_eq!(mesh.vertices.len() % 4, 0, "vertices must come in quads");
    assert_eq!(
        mesh.indices.len(),
        mesh.vertices.len() / 4 * 6,
        "6 indices per quad"
    );
    mesh.vertices.len() / 4
}

/// Half the width of the label's bounding box in the billboard plane.
fn half_extent_x(mesh: &LabelMesh) -> f32 {
    mesh.vertices
        .iter()
        .map(|v| v.plane_offset[0].abs())
        .fold(0.0, f32::max)
}

// ============================================================================
// Which kinds are labelled
// ============================================================================

/// The reported case: a Cl and a Si ghost at the same site. Both are added,
/// both get their two-letter symbol.
#[test]
fn added_ghost_carries_its_element_symbol() {
    let (_, cl) = tessellate(
        &visuals(vec![ghost(GhostKind::Added, CHLORINE, DVec3::ZERO)], false),
        0.7,
    );
    let (_, si) = tessellate(
        &visuals(vec![ghost(GhostKind::Added, SILICON, DVec3::ZERO)], false),
        0.7,
    );
    assert_eq!(glyph_quads(&cl), 2, "\"Cl\" is two glyphs");
    assert_eq!(glyph_quads(&si), 2, "\"Si\" is two glyphs");
}

#[test]
fn one_letter_symbol_is_one_glyph() {
    let (_, h) = tessellate(
        &visuals(vec![ghost(GhostKind::Added, HYDROGEN, DVec3::ZERO)], false),
        0.7,
    );
    assert_eq!(glyph_quads(&h), 1);
}

/// An element swap is exactly where the new element is the news.
#[test]
fn changed_ghost_is_labelled() {
    let (_, labels) = tessellate(
        &visuals(
            vec![ghost(GhostKind::Changed, CHLORINE, DVec3::ZERO)],
            false,
        ),
        0.7,
    );
    assert_eq!(glyph_quads(&labels), 2);
}

/// A moved atom keeps its element, and a deleted ghost sits on a real atom that
/// already shows what it is: neither is labelled.
#[test]
fn moved_and_deleted_ghosts_are_not_labelled() {
    let moved = GhostAtom {
        kind: GhostKind::Moved,
        position: DVec3::new(2.0, 0.0, 0.0),
        from: DVec3::ZERO,
        atomic_number: SILICON,
    };
    let deleted = ghost(GhostKind::Deleted, CHLORINE, DVec3::new(5.0, 0.0, 0.0));
    let (transparent, labels) = tessellate(&visuals(vec![moved, deleted], false), 0.7);
    assert_eq!(glyph_quads(&labels), 0);
    // Sanity: the sphere pass still drew both (plus the moved atom's trail).
    assert!(!transparent.vertices.is_empty());
}

/// A near miss is amber throughout, but the symbol still says what you are
/// looking at.
#[test]
fn near_miss_keeps_its_label() {
    let (_, labels) = tessellate(
        &visuals(vec![ghost(GhostKind::Added, CHLORINE, DVec3::ZERO)], true),
        0.7,
    );
    assert_eq!(glyph_quads(&labels), 2);
}

// ============================================================================
// Where the label sits
// ============================================================================

/// The label is anchored on the ghost's destination and pushed toward the eye
/// past the ghost's own sphere, so it is not swallowed by it.
#[test]
fn label_is_anchored_on_the_ghost_and_clears_its_sphere() {
    let position = DVec3::new(1.0, 2.0, 3.0);
    let (transparent, labels) = tessellate(
        &visuals(vec![ghost(GhostKind::Added, SILICON, position)], false),
        0.7,
    );
    let ghost_radius = transparent
        .vertices
        .iter()
        .map(|v| v.radius)
        .fold(0.0, f32::max);
    assert!(ghost_radius > 0.0, "the sphere pass drew the ghost");
    assert!(!labels.vertices.is_empty(), "the label was drawn");
    for v in &labels.vertices {
        assert_eq!(v.anchor_position, position.as_vec3().to_array());
        assert!(
            v.depth_offset > ghost_radius,
            "depth offset {} must clear the ghost radius {}",
            v.depth_offset,
            ghost_radius
        );
    }
}

/// The preview's text scales with the same `label_scale` the scene's labels
/// use, so the two match.
#[test]
fn label_honours_the_display_label_scale() {
    let v = visuals(vec![ghost(GhostKind::Added, CHLORINE, DVec3::ZERO)], false);
    let (_, small) = tessellate(&v, 0.5);
    let (_, large) = tessellate(&v, 1.0);
    let ratio = half_extent_x(&large) / half_extent_x(&small);
    assert!(
        (ratio - 2.0).abs() < 1e-4,
        "doubling label_scale doubles the label, got ratio {ratio}"
    );
}

/// Labelling is additive: the sphere pass is the same with or without labels
/// being requested — one quad per ghost, nothing extra.
#[test]
fn labelling_leaves_the_sphere_pass_alone() {
    let (transparent, _) = tessellate(
        &visuals(
            vec![
                ghost(GhostKind::Added, CHLORINE, DVec3::ZERO),
                ghost(GhostKind::Added, SILICON, DVec3::new(4.0, 0.0, 0.0)),
            ],
            false,
        ),
        0.7,
    );
    assert_eq!(transparent.vertices.len(), 2 * 4, "two sphere quads");
}
