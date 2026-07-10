//! Tests for the `diff` output pin on the four movement nodes (`free_move`,
//! `free_rot`, `structure_move`, `structure_rot`) — Phase 3 of
//! `doc/design_diff_outputs_for_atom_ops.md` (issue #295).
//!
//! Uses the `value`-node harness and the shared `assert_node_diff_roundtrip`
//! helper from `diff_test_support.rs`.

use glam::f64::{DVec2, DVec3};
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::atomic_structure_diff::{apply_diff, compose_two_diffs};
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    Alignment, BlueprintData, CrystalData, MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

use crate::diff_test_support::{APPLY_TOLERANCE, assert_node_diff_roundtrip, evaluate_pin};
use crate::structure_equivalence::assert_structures_equivalent;

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

/// A Blueprint value (no atoms) — its diff pin must yield an empty diff (§2.3).
fn blueprint_value() -> NetworkResult {
    NetworkResult::Blueprint(BlueprintData {
        structure: Structure::diamond(),
        geo_tree_root: GeoNode::sphere(DVec3::ZERO, 5.0),
        alignment: Alignment::Aligned,
        alignment_reason: None,
    })
}

/// A small bonded, bent molecule whose atoms are off any single axis, so both a
/// translation and a rotation move every atom (exercises real diff content).
fn sample_molecule() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(2.5, 0.5, 0.0));
    let c = s.add_atom(8, DVec3::new(3.0, 2.0, 0.5));
    s.add_bond(a, b, BOND_SINGLE);
    s.add_bond(b, c, BOND_SINGLE);
    s
}

/// Sets text properties on a node's data (movement parameters).
fn set_props(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    props: Vec<(&str, TextValue)>,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    let map: HashMap<String, TextValue> =
        props.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    node.data.set_text_properties(&map).unwrap();
}

/// Adds a movement node wired to `input_id` at pin 0 and returns its id.
fn add_movement(designer: &mut StructureDesigner, node_type: &str, input_id: u64) -> u64 {
    let id = designer.add_node(node_type, DVec2::new(200.0, 0.0));
    designer.connect_nodes(input_id, 0, id, 0);
    id
}

/// Asserts pin 1 is an empty diff and pin 0 preserves the given phase predicate.
fn assert_empty_diff_pin(
    designer: &StructureDesigner,
    net: &str,
    node_id: u64,
    pin0_ok: impl Fn(&NetworkResult) -> bool,
) {
    let pin0 = evaluate_pin(designer, net, node_id, 0);
    assert!(
        pin0_ok(&pin0),
        "pin 0 phase unexpected: {:?}",
        pin0.infer_data_type()
    );
    match evaluate_pin(designer, net, node_id, 1) {
        NetworkResult::Molecule(m) => {
            assert!(m.atoms.is_diff(), "pin 1 must be a diff");
            assert_eq!(
                m.atoms.get_num_of_atoms(),
                0,
                "non-atomic input must yield an empty diff"
            );
        }
        other => panic!("expected Molecule diff, got {:?}", other.infer_data_type()),
    }
}

// ============================================================================
// Test 1: mandatory §3.0 roundtrip per node, on each accepted atomic phase
// ============================================================================

#[test]
fn free_move_diff_roundtrip_molecule() {
    let net = "test";
    let input = sample_molecule();
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(input.clone()));
    let mv = add_movement(&mut designer, "free_move", value_id);
    set_props(
        &mut designer,
        net,
        mv,
        vec![("translation", TextValue::Vec3(DVec3::new(5.0, -3.0, 2.0)))],
    );
    assert_node_diff_roundtrip(&designer, net, mv, &input);
}

#[test]
fn free_rot_diff_roundtrip_molecule() {
    let net = "test";
    let input = sample_molecule();
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(input.clone()));
    let rot = add_movement(&mut designer, "free_rot", value_id);
    set_props(
        &mut designer,
        net,
        rot,
        vec![
            ("angle_degrees", TextValue::Float(37.0)),
            ("rot_axis", TextValue::Vec3(DVec3::new(0.0, 0.0, 1.0))),
            ("pivot_point", TextValue::Vec3(DVec3::ZERO)),
        ],
    );
    assert_node_diff_roundtrip(&designer, net, rot, &input);
}

#[test]
fn structure_move_diff_roundtrip_crystal() {
    let net = "test";
    let input = sample_molecule();
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, crystal_value(input.clone()));
    let mv = add_movement(&mut designer, "structure_move", value_id);
    set_props(
        &mut designer,
        net,
        mv,
        vec![("translation", TextValue::IVec3(IVec3::new(1, 2, 0)))],
    );
    assert_node_diff_roundtrip(&designer, net, mv, &input);
}

#[test]
fn structure_rot_diff_roundtrip_crystal() {
    let net = "test";
    let input = sample_molecule();
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, crystal_value(input.clone()));
    let rot = add_movement(&mut designer, "structure_rot", value_id);
    // axis 0, step 1 on the cubic diamond cell is a genuine symmetry rotation.
    set_props(
        &mut designer,
        net,
        rot,
        vec![
            ("axis_index", TextValue::Int(0)),
            ("step", TextValue::Int(1)),
            ("pivot_point", TextValue::IVec3(IVec3::ZERO)),
        ],
    );
    assert_node_diff_roundtrip(&designer, net, rot, &input);
}

// ============================================================================
// Test 2: Blueprint input ⇒ empty diff on pin 1, pin 0 unchanged
// ============================================================================

#[test]
fn free_move_blueprint_empty_diff() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, blueprint_value());
    let mv = add_movement(&mut designer, "free_move", value_id);
    set_props(
        &mut designer,
        net,
        mv,
        vec![("translation", TextValue::Vec3(DVec3::new(4.0, 0.0, 0.0)))],
    );
    assert_empty_diff_pin(&designer, net, mv, |r| {
        matches!(r, NetworkResult::Blueprint(_))
    });
}

#[test]
fn free_rot_blueprint_empty_diff() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, blueprint_value());
    let rot = add_movement(&mut designer, "free_rot", value_id);
    set_props(
        &mut designer,
        net,
        rot,
        vec![
            ("angle_degrees", TextValue::Float(45.0)),
            ("rot_axis", TextValue::Vec3(DVec3::new(0.0, 0.0, 1.0))),
        ],
    );
    assert_empty_diff_pin(&designer, net, rot, |r| {
        matches!(r, NetworkResult::Blueprint(_))
    });
}

#[test]
fn structure_move_blueprint_empty_diff() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, blueprint_value());
    let mv = add_movement(&mut designer, "structure_move", value_id);
    set_props(
        &mut designer,
        net,
        mv,
        vec![("translation", TextValue::IVec3(IVec3::new(1, 0, 0)))],
    );
    assert_empty_diff_pin(&designer, net, mv, |r| {
        matches!(r, NetworkResult::Blueprint(_))
    });
}

#[test]
fn structure_rot_blueprint_empty_diff() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, blueprint_value());
    let rot = add_movement(&mut designer, "structure_rot", value_id);
    set_props(
        &mut designer,
        net,
        rot,
        vec![
            ("axis_index", TextValue::Int(0)),
            ("step", TextValue::Int(1)),
        ],
    );
    assert_empty_diff_pin(&designer, net, rot, |r| {
        matches!(r, NetworkResult::Blueprint(_))
    });
}

// ============================================================================
// Test 3: free_rot about a non-origin pivot roundtrips
// ============================================================================

#[test]
fn free_rot_non_origin_pivot_roundtrip() {
    let net = "test";
    let input = sample_molecule();
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(input.clone()));
    let rot = add_movement(&mut designer, "free_rot", value_id);
    set_props(
        &mut designer,
        net,
        rot,
        vec![
            ("angle_degrees", TextValue::Float(90.0)),
            ("rot_axis", TextValue::Vec3(DVec3::new(0.0, 1.0, 0.0))),
            ("pivot_point", TextValue::Vec3(DVec3::new(3.0, 0.0, -1.0))),
        ],
    );
    assert_node_diff_roundtrip(&designer, net, rot, &input);
}

// ============================================================================
// Test 4: composability — free_move diff ∘ relax diff ≡ sequential application
// ============================================================================

#[test]
fn free_move_then_relax_diffs_compose() {
    let net = "test";
    // A stretched C–C pair that both translates and relaxes.
    let mut input = AtomicStructure::new();
    let a = input.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = input.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    input.add_bond(a, b, BOND_SINGLE);

    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(input.clone()));
    let mv = add_movement(&mut designer, "free_move", value_id);
    set_props(
        &mut designer,
        net,
        mv,
        vec![("translation", TextValue::Vec3(DVec3::new(10.0, 5.0, -4.0)))],
    );
    // relax reads free_move's pin 0 (the moved molecule).
    let relax = designer.add_node("relax", DVec2::new(400.0, 0.0));
    designer.connect_nodes(mv, 0, relax, 0);

    let d1 = match evaluate_pin(&designer, net, mv, 1) {
        NetworkResult::Molecule(m) => m.atoms,
        other => panic!("free_move pin1: {:?}", other.infer_data_type()),
    };
    let d2 = match evaluate_pin(&designer, net, relax, 1) {
        NetworkResult::Molecule(m) => m.atoms,
        other => panic!("relax pin1: {:?}", other.infer_data_type()),
    };
    // The real chained output (move then relax).
    let sequential = match evaluate_pin(&designer, net, relax, 0) {
        NetworkResult::Molecule(m) => m.atoms,
        other => panic!("relax pin0: {:?}", other.infer_data_type()),
    };

    let composed = compose_two_diffs(&d1, &d2, APPLY_TOLERANCE).composed;
    let applied = apply_diff(&input, &composed, APPLY_TOLERANCE).result;
    assert_structures_equivalent(&applied, &sequential, 1e-6);
}

// ============================================================================
// Test 5: free_rot degenerate (zero) axis ⇒ pin 0 input unchanged, pin 1 empty
// ============================================================================

#[test]
fn free_rot_degenerate_axis_two_pins() {
    let net = "test";
    let input = sample_molecule();
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, molecule_value(input.clone()));
    let rot = add_movement(&mut designer, "free_rot", value_id);
    // Zero axis: the mutation is skipped; the node must still return two pins.
    set_props(
        &mut designer,
        net,
        rot,
        vec![
            ("angle_degrees", TextValue::Float(90.0)),
            ("rot_axis", TextValue::Vec3(DVec3::ZERO)),
        ],
    );

    // Pin 0 is the input unchanged.
    match evaluate_pin(&designer, net, rot, 0) {
        NetworkResult::Molecule(m) => {
            assert_structures_equivalent(&m.atoms, &input, 1e-9);
        }
        other => panic!(
            "pin 0: expected Molecule, got {:?}",
            other.infer_data_type()
        ),
    }

    // Pin 1 is an empty diff, not None/Error.
    match evaluate_pin(&designer, net, rot, 1) {
        NetworkResult::Molecule(m) => {
            assert!(m.atoms.is_diff(), "pin 1 must be a diff");
            assert_eq!(
                m.atoms.get_num_of_atoms(),
                0,
                "degenerate axis ⇒ empty diff"
            );
        }
        other => panic!(
            "pin 1: expected Molecule diff, got {:?}",
            other.infer_data_type()
        ),
    }
}
