//! Regression tests for `extrude` preserving the full crystal `Structure`
//! (including the motif) from the 2D shape's drawing plane, instead of forcing
//! the default carbon zincblende motif via `Structure::from_lattice_vecs`.
//!
//! The bug: `extrude` reconstituted the Blueprint's structure from only the
//! drawing plane's unit cell + the default carbon motif, so a shape drawn on a
//! silicon structure produced carbon atoms on a silicon-spaced lattice. The fix
//! makes the `DrawingPlane` carry the full structure (single source of truth,
//! consistent with why extrude's `structure` pin was deprecated) and extrude
//! read it. See doc/design_drawing_plane_carries_structure.md.

use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::network_validator::validate_network;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::edit_network;

fn setup(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// Applies text-format code to a named network (remove → edit → validate →
/// reinsert), mirroring `apply_text_to_active_network`'s borrow dance.
fn apply_text(designer: &mut StructureDesigner, network_name: &str, text: &str) {
    let mut network = designer
        .node_type_registry
        .node_networks
        .remove(network_name)
        .expect("network exists");
    let edit_result = edit_network(&mut network, &designer.node_type_registry, text, true);
    assert!(
        edit_result.errors.is_empty(),
        "text edit errors: {:?}",
        edit_result.errors
    );
    validate_network(&mut network, &mut designer.node_type_registry, None);
    designer
        .node_type_registry
        .node_networks
        .insert(network_name.to_string(), network);
}

fn node_id_by_type(designer: &StructureDesigner, network_name: &str, type_name: &str) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    network
        .nodes
        .values()
        .find(|n| n.node_type_name == type_name)
        .map(|n| n.id)
        .unwrap_or_else(|| panic!("no `{type_name}` node in network"))
}

fn evaluate_node(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

/// A circle drawn on a silicon `drawing_plane`, extruded, must yield a Blueprint
/// whose structure carries the silicon motif (1 PARAM defaulting to Si=14), not
/// the default carbon zincblende motif (2 PARAMs defaulting to C=6).
#[test]
fn extrude_preserves_drawing_plane_motif() {
    let network_name = "test_extrude_motif";
    let mut designer = setup(network_name);

    // A minimal, distinguishable silicon structure: one PARAM (Si), one site.
    // The motif is never materialized here — we only inspect what extrude carries.
    let text = r#"
lv = lattice_vecs { cell_length_a: 5.431, cell_length_b: 5.431, cell_length_c: 5.431, cell_angle_alpha: 90.0, cell_angle_beta: 90.0, cell_angle_gamma: 90.0 }
mot = motif { definition: """PARAM PRIMARY Si
SITE A PRIMARY 0 0 0""", name: "si_test" }
st = structure { lattice_vecs: lv, motif: mot }
dp = drawing_plane { structure: st, m_index: (0, 0, 1) }
c = circle { radius: 3, d_plane: dp }
ex = extrude { shape: c, height: 1, dir: (0, 0, 1) }
"#;
    apply_text(&mut designer, network_name, text);

    let extrude_id = node_id_by_type(&designer, network_name, "extrude");
    let result = evaluate_node(&designer, network_name, extrude_id);

    let blueprint = match result {
        NetworkResult::Blueprint(bp) => bp,
        NetworkResult::Error(e) => panic!("extrude returned error: {e}"),
        other => panic!(
            "expected extrude to produce a Blueprint, got {:?}",
            other.infer_data_type()
        ),
    };

    // Lattice constant should be silicon's (sanity: the cell already flowed through).
    assert!(
        (blueprint.structure.lattice_vecs.a.length() - 5.431).abs() < 1e-6,
        "extrude Blueprint should keep the 5.431 A cell, got {}",
        blueprint.structure.lattice_vecs.a.length()
    );

    // The motif must be the silicon one authored on the drawing plane, not the
    // forced default carbon zincblende (which has 2 params, both C=6, 8 sites).
    let motif = &blueprint.structure.motif;
    assert_eq!(
        motif.parameters.len(),
        1,
        "extrude should carry the authored 1-parameter silicon motif, not the 2-param default zincblende"
    );
    assert_eq!(
        motif.parameters[0].default_atomic_number, 14,
        "extrude should carry the silicon PARAM default (Z=14), not carbon (Z=6)"
    );
}

// ============================================================================
// Plane-normal extrusion mode (issue #364)
//
// `extrude` can derive its direction from the drawing plane's normal
// (`plane_normal: true`) instead of a fixed stored vector. The observable that
// captures the bug: a fixed `dir` that is parallel to a (reoriented) plane
// errors out ("parallel to plane"), while plane-normal mode always extrudes
// perpendicular to whatever plane it is drawn on.
// ============================================================================

/// Helper: build a circle-on-plane-then-extrude network with the given
/// `m_index` and extrude property snippet, and return the extrude result.
fn extrude_on_plane(m_index: &str, extrude_props: &str) -> NetworkResult {
    let network_name = "test_extrude_plane_normal";
    let mut designer = setup(network_name);
    let text = format!(
        r#"
dp = drawing_plane {{ m_index: {m_index} }}
c = circle {{ radius: 3, d_plane: dp }}
ex = extrude {{ shape: c, {extrude_props} }}
"#
    );
    apply_text(&mut designer, network_name, &text);
    let extrude_id = node_id_by_type(&designer, network_name, "extrude");
    evaluate_node(&designer, network_name, extrude_id)
}

/// Baseline: on the axis-aligned (0,0,1) plane a fixed `dir: (0,0,1)` works —
/// this is the legacy direct-mode behavior that existing files rely on.
#[test]
fn extrude_direct_mode_axis_plane_ok() {
    let result = extrude_on_plane(
        "(0, 0, 1)",
        "height: 1, dir: (0, 0, 1), plane_normal: false",
    );
    assert!(
        matches!(result, NetworkResult::Blueprint(_)),
        "direct-mode extrude along (0,0,1) on a (0,0,1) plane should succeed, got {:?}",
        result.infer_data_type()
    );
}

/// The bug: a fixed `dir: (0,0,1)` on a reoriented (1,0,0) plane is parallel to
/// that plane, so direct mode errors out. This is exactly the "brittle
/// hardcoded setting" the issue describes — rotating the plane later strands
/// the stored direction.
#[test]
fn extrude_direct_mode_reoriented_plane_errors() {
    let result = extrude_on_plane(
        "(1, 0, 0)",
        "height: 1, dir: (0, 0, 1), plane_normal: false",
    );
    assert!(
        matches!(result, NetworkResult::Error(_)),
        "direct-mode extrude with a stale (0,0,1) dir on a (1,0,0) plane should error, got {:?}",
        result.infer_data_type()
    );
}

/// The fix: plane-normal mode extrudes perpendicular to the (1,0,0) plane with
/// no fixed direction, so the same reorientation that broke direct mode just
/// works — the direction tracks the plane.
#[test]
fn extrude_plane_normal_mode_reoriented_plane_ok() {
    let result = extrude_on_plane("(1, 0, 0)", "height: 1, plane_normal: true");
    assert!(
        matches!(result, NetworkResult::Blueprint(_)),
        "plane-normal extrude on a (1,0,0) plane should succeed, got {:?}",
        result.infer_data_type()
    );
}

/// Plane-normal mode is robust across plane orientations: the very plane that
/// errors in direct mode (1,0,0) and the axis plane (0,0,1) both succeed,
/// because the direction is recomputed from each plane rather than stored.
#[test]
fn extrude_plane_normal_mode_tracks_multiple_planes() {
    for m_index in ["(0, 0, 1)", "(1, 0, 0)", "(0, 1, 0)", "(1, 1, 1)"] {
        let result = extrude_on_plane(m_index, "height: 1, plane_normal: true");
        assert!(
            matches!(result, NetworkResult::Blueprint(_)),
            "plane-normal extrude on plane {m_index} should succeed, got {:?}",
            result.infer_data_type()
        );
    }
}

/// A wired/explicit `dir` still overrides plane-normal mode. Here both are set:
/// `plane_normal: true` *and* a stored `dir: (0,0,1)` on a (1,0,0) plane. Since
/// no `dir` *pin* is wired in the text network, the mode wins and it succeeds —
/// confirming plane-normal takes precedence over the stored vector.
#[test]
fn extrude_plane_normal_beats_stored_direction() {
    let result = extrude_on_plane("(1, 0, 0)", "height: 1, dir: (0, 0, 1), plane_normal: true");
    assert!(
        matches!(result, NetworkResult::Blueprint(_)),
        "plane-normal mode should ignore the stored (0,0,1) dir and extrude along the plane normal, got {:?}",
        result.infer_data_type()
    );
}
