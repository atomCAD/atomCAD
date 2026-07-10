//! Tests for the `relax` node's `diff` output pin (issue #295, Phase 2 of
//! `doc/design_diff_outputs_for_atom_ops.md`).
//!
//! Uses the `value`-node harness pattern from `apply_diff_node_test.rs` and the
//! shared `assert_node_diff_roundtrip` helper from `diff_test_support.rs`.

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::atomic_structure_diff::apply_diff;
use rust_lib_flutter_cad::crystolecule::simulation::MAX_MINIMIZE_FREE_ATOMS;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    Alignment, CrystalData, MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

use crate::diff_test_support::{APPLY_TOLERANCE, assert_node_diff_roundtrip, evaluate_pin};

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn add_value_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    value: NetworkResult,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.add_node("value", DVec2::ZERO, 0, Box::new(ValueData { value }))
}

fn molecule_value(structure: AtomicStructure) -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms: structure,
        geo_tree_root: None,
    })
}

fn crystal_value(structure: AtomicStructure) -> NetworkResult {
    NetworkResult::Crystal(CrystalData {
        structure: Structure::diamond(),
        atoms: structure,
        geo_tree_root: None,
        alignment: Alignment::Aligned,
        alignment_reason: None,
    })
}

/// Sets the `diff_min_move` property on a relax node.
fn set_diff_min_move(
    designer: &mut StructureDesigner,
    network_name: &str,
    relax_id: u64,
    value: f64,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&relax_id).unwrap();
    let mut props = HashMap::new();
    props.insert("diff_min_move".to_string(), TextValue::Float(value));
    node.data.set_text_properties(&props).unwrap();
}

/// A stretched, bonded C–C pair (2.0 Å; equilibrium ≈ 1.5 Å). Both atoms move
/// symmetrically inward under minimization, so both enter the diff. Proven to
/// relax cleanly (see `relax_node_atom_limit_test::frozen_bulk_with_free_pair`).
fn strained_pair() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    s.add_bond(a, b, BOND_SINGLE);
    s
}

fn add_relax(designer: &mut StructureDesigner, value_id: u64) -> u64 {
    let relax_id = designer.add_node("relax", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, relax_id, 0);
    relax_id
}

// ============================================================================
// Test 1: node-level roundtrip (Molecule and Crystal), diff_min_move = 0.0
// ============================================================================

#[test]
fn relax_diff_roundtrip_molecule() {
    let net = "test";
    let input = strained_pair();
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(input.clone()));
    let relax_id = add_relax(&mut designer, value_id);

    assert_node_diff_roundtrip(&designer, net, relax_id, &input);
}

#[test]
fn relax_diff_roundtrip_crystal() {
    let net = "test";
    let input = strained_pair();
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, crystal_value(input.clone()));
    let relax_id = add_relax(&mut designer, value_id);

    assert_node_diff_roundtrip(&designer, net, relax_id, &input);
}

// ============================================================================
// Test 2: mockup → monster (the issue's workflow)
// ============================================================================

#[test]
fn relax_diff_mockup_to_monster() {
    let net = "test";
    let mockup = strained_pair();

    // monster = mockup atoms at identical coords + far-away extra atoms.
    let mut monster = strained_pair();
    let extra1 = monster.add_atom(6, DVec3::new(50.0, 0.0, 0.0));
    let extra2 = monster.add_atom(6, DVec3::new(52.0, 0.0, 0.0));
    monster.add_bond(extra1, extra2, BOND_SINGLE);

    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(mockup));
    let relax_id = add_relax(&mut designer, value_id);

    // Relaxed mockup (pin 0) tells us where the two atoms ended up.
    let relaxed = match evaluate_pin(&designer, net, relax_id, 0) {
        NetworkResult::Molecule(m) => m.atoms,
        other => panic!("expected Molecule, got {:?}", other.infer_data_type()),
    };
    let relaxed_xs: Vec<f64> = relaxed.atoms_values().map(|a| a.position.x).collect();

    // Apply the relax diff (pin 1) onto the monster.
    let diff = match evaluate_pin(&designer, net, relax_id, 1) {
        NetworkResult::Molecule(m) => m.atoms,
        other => panic!("expected Molecule diff, got {:?}", other.infer_data_type()),
    };
    let application = apply_diff(&monster, &diff, APPLY_TOLERANCE);

    // No anchored diff atom failed to find its base atom.
    assert_eq!(
        application.stats.orphaned_tracked_atoms, 0,
        "every mockup diff atom must match a monster base atom"
    );

    let result = application.result;
    assert_eq!(result.get_num_of_atoms(), 4, "monster keeps all 4 atoms");

    // The two mockup atoms are now at their relaxed positions.
    for x in &relaxed_xs {
        assert!(
            result
                .atoms_values()
                .any(|a| (a.position.x - x).abs() < 1e-6
                    && a.position.y.abs() < 1e-6
                    && a.position.z.abs() < 1e-6),
            "expected a relaxed mockup atom near x={x} in the result"
        );
    }

    // The far-away extra atoms are untouched.
    for x in [50.0_f64, 52.0] {
        assert!(
            result
                .atoms_values()
                .any(|a| (a.position.x - x).abs() < 1e-9),
            "extra atom at x={x} must be untouched"
        );
    }
}

// ============================================================================
// Test 3: frozen exclusion — no frozen atom appears in the diff
// ============================================================================

#[test]
fn relax_diff_excludes_frozen_atoms() {
    let net = "test";

    // Free strained pair + two frozen atoms placed far away (outside the vdW
    // shell, so they neither move nor perturb the free pair).
    let mut mockup = strained_pair();
    let f1 = mockup.add_atom(6, DVec3::new(30.0, 0.0, 0.0));
    let f2 = mockup.add_atom(6, DVec3::new(32.0, 0.0, 0.0));
    mockup.add_bond(f1, f2, BOND_SINGLE);
    mockup.set_atom_frozen(f1, true);
    mockup.set_atom_frozen(f2, true);

    let non_frozen_count = mockup.atoms_values().filter(|a| !a.is_frozen()).count();

    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(mockup));
    let relax_id = add_relax(&mut designer, value_id);

    let diff = match evaluate_pin(&designer, net, relax_id, 1) {
        NetworkResult::Molecule(m) => m.atoms,
        other => panic!("expected Molecule diff, got {:?}", other.infer_data_type()),
    };

    // With ε = 0, exactly the non-frozen atoms move and enter the diff.
    assert_eq!(
        diff.get_num_of_atoms(),
        non_frozen_count,
        "diff must contain exactly the non-frozen atoms"
    );
}

// ============================================================================
// Test 4: diff_min_move pruning — large ε ⇒ empty diff; pin 0 unaffected
// ============================================================================

#[test]
fn relax_diff_min_move_prunes_to_empty() {
    let net = "test";
    let input = strained_pair();
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(input));
    let relax_id = add_relax(&mut designer, value_id);
    // A huge threshold prunes every moved atom.
    set_diff_min_move(&mut designer, net, relax_id, 1000.0);

    let diff = match evaluate_pin(&designer, net, relax_id, 1) {
        NetworkResult::Molecule(m) => m.atoms,
        other => panic!("expected Molecule diff, got {:?}", other.infer_data_type()),
    };
    assert_eq!(diff.get_num_of_atoms(), 0, "large ε must empty the diff");

    // Pin 0 (the relaxed structure) is unaffected by the pruning threshold.
    let relaxed = match evaluate_pin(&designer, net, relax_id, 0) {
        NetworkResult::Molecule(m) => m.atoms,
        other => panic!("expected Molecule, got {:?}", other.infer_data_type()),
    };
    assert_eq!(relaxed.get_num_of_atoms(), 2);
}

// ============================================================================
// Test 5: pin 1 shape (is_diff + show_anchor_arrows); pin 0 phase preserved
// ============================================================================

#[test]
fn relax_diff_pin_shape_molecule() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(strained_pair()));
    let relax_id = add_relax(&mut designer, value_id);

    // Pin 0 preserves the Molecule phase.
    assert!(matches!(
        evaluate_pin(&designer, net, relax_id, 0),
        NetworkResult::Molecule(_)
    ));

    // Pin 1 is a diff Molecule with anchor arrows enabled.
    match evaluate_pin(&designer, net, relax_id, 1) {
        NetworkResult::Molecule(m) => {
            assert!(m.atoms.is_diff(), "pin 1 must be a diff");
            assert!(
                m.atoms.decorator().show_anchor_arrows,
                "diff must have show_anchor_arrows set"
            );
        }
        other => panic!("expected Molecule diff, got {:?}", other.infer_data_type()),
    }
}

#[test]
fn relax_diff_pin_shape_crystal() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, crystal_value(strained_pair()));
    let relax_id = add_relax(&mut designer, value_id);

    // Pin 0 preserves the Crystal phase.
    assert!(matches!(
        evaluate_pin(&designer, net, relax_id, 0),
        NetworkResult::Crystal(_)
    ));

    // Pin 1 is always a diff Molecule, regardless of the Crystal input phase.
    match evaluate_pin(&designer, net, relax_id, 1) {
        NetworkResult::Molecule(m) => {
            assert!(m.atoms.is_diff(), "pin 1 must be a diff");
            assert!(m.atoms.decorator().show_anchor_arrows);
        }
        other => panic!("expected Molecule diff, got {:?}", other.infer_data_type()),
    }
}

// ============================================================================
// Test 6: non-atomic input ⇒ both pins are Error (no panic on pin-1 consumers)
// ============================================================================

#[test]
fn relax_diff_error_input_both_pins() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    // A non-atomic value (Float) reaches relax's "expected atomic input" arm.
    let value_id = add_value_node(&mut designer, net, NetworkResult::Float(1.0));
    let relax_id = add_relax(&mut designer, value_id);

    for pin in [0, 1] {
        match evaluate_pin(&designer, net, relax_id, pin) {
            NetworkResult::Error(_) => {}
            other => panic!(
                "pin {pin}: expected Error, got {:?}",
                other.infer_data_type()
            ),
        }
    }
}

// ============================================================================
// Test 7: over-atom-limit input ⇒ both pins are Error
// ============================================================================

#[test]
fn relax_diff_over_limit_both_pins() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);

    // More free atoms than minimize_energy allows → minimize returns Err, which
    // relax surfaces on both pins.
    let mut over_limit = AtomicStructure::new();
    for i in 0..(MAX_MINIMIZE_FREE_ATOMS + 1) {
        over_limit.add_atom(6, DVec3::new(i as f64 * 2.0, 0.0, 0.0));
    }
    let value_id = add_value_node(&mut designer, net, molecule_value(over_limit));
    let relax_id = add_relax(&mut designer, value_id);

    for pin in [0, 1] {
        match evaluate_pin(&designer, net, relax_id, pin) {
            NetworkResult::Error(_) => {}
            other => panic!(
                "pin {pin}: expected Error, got {:?}",
                other.infer_data_type()
            ),
        }
    }
}
