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
