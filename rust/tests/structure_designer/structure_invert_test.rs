//! Node-level tests for `structure_invert` — point inversion of a
//! structure-bound object through a (possibly fractional) lattice pivot.
//!
//! Uses the `value`-node harness and the shared `assert_node_diff_roundtrip`
//! helper from `diff_test_support.rs` (same pattern as
//! `movement_diff_output_test.rs`).

use glam::f64::{DVec2, DVec3};
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::crystolecule_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    Alignment, BlueprintData, CrystalData, MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

use crate::diff_test_support::{assert_node_diff_roundtrip, evaluate_pin};

// ============================================================================
// Helpers (mirroring movement_diff_output_test.rs)
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

fn crystal_value(structure: AtomicStructure) -> NetworkResult {
    NetworkResult::Crystal(CrystalData {
        structure: Structure::diamond(),
        atoms: structure,
        geo_tree_root: None,
        alignment: Alignment::Aligned,
        alignment_reason: None,
    })
}

fn blueprint_value() -> NetworkResult {
    NetworkResult::Blueprint(BlueprintData {
        structure: Structure::diamond(),
        geo_tree_root: GeoNode::sphere(DVec3::ZERO, 5.0),
        alignment: Alignment::Aligned,
        alignment_reason: None,
    })
}

fn sample_molecule() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(2.5, 0.5, 0.0));
    let c = s.add_atom(8, DVec3::new(3.0, 2.0, 0.5));
    s.add_bond(a, b, BOND_SINGLE);
    s.add_bond(b, c, BOND_SINGLE);
    s
}

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

fn add_invert(designer: &mut StructureDesigner, input_id: u64) -> u64 {
    let id = designer.add_node("structure_invert", DVec2::new(200.0, 0.0));
    designer.connect_nodes(input_id, 0, id, 0);
    id
}

fn pin0_crystal_alignment(
    designer: &StructureDesigner,
    net: &str,
    node_id: u64,
) -> (Alignment, Option<String>) {
    match evaluate_pin(designer, net, node_id, 0) {
        NetworkResult::Crystal(c) => (c.alignment, c.alignment_reason),
        other => panic!("pin 0: expected Crystal, got {:?}", other.infer_data_type()),
    }
}

// ============================================================================
// Registration
// ============================================================================

#[test]
fn structure_invert_registration() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("structure_invert").unwrap();
    assert_eq!(node_type.name, "structure_invert");
    assert!(node_type.public);
    assert_eq!(node_type.parameters.len(), 3);
    assert_eq!(node_type.parameters[0].name, "input");
    assert_eq!(node_type.parameters[1].name, "pivot_point");
    assert_eq!(node_type.parameters[2].name, "subdivision");
    assert_eq!(node_type.output_pins.len(), 2);
    assert_eq!(node_type.output_pins[0].name, "result");
    assert_eq!(node_type.output_pins[1].name, "diff");
}

// ============================================================================
// Atom motion + diff roundtrip
// ============================================================================

#[test]
fn structure_invert_maps_atom_positions() {
    let net = "test";
    let input = sample_molecule();
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, crystal_value(input.clone()));
    let inv = add_invert(&mut designer, value_id);
    // Bond-center pivot (1,1,1)/8 on the diamond cell.
    set_props(
        &mut designer,
        net,
        inv,
        vec![
            ("pivot_point", TextValue::IVec3(IVec3::new(1, 1, 1))),
            ("subdivision", TextValue::Int(8)),
        ],
    );

    let pivot_real = DVec3::splat(DIAMOND_UNIT_CELL_SIZE_ANGSTROM / 8.0);
    match evaluate_pin(&designer, net, inv, 0) {
        NetworkResult::Crystal(c) => {
            for id in input.atom_ids() {
                let expected = 2.0 * pivot_real - input.get_atom(*id).unwrap().position;
                let actual = c.atoms.get_atom(*id).unwrap().position;
                assert!(
                    actual.distance(expected) < 1e-9,
                    "atom {} expected {:?}, got {:?}",
                    id,
                    expected,
                    actual
                );
            }
        }
        other => panic!("pin 0: expected Crystal, got {:?}", other.infer_data_type()),
    }
}

#[test]
fn structure_invert_diff_roundtrip_crystal() {
    let net = "test";
    let input = sample_molecule();
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, crystal_value(input.clone()));
    let inv = add_invert(&mut designer, value_id);
    set_props(
        &mut designer,
        net,
        inv,
        vec![
            ("pivot_point", TextValue::IVec3(IVec3::new(1, 1, 1))),
            ("subdivision", TextValue::Int(8)),
        ],
    );
    assert_node_diff_roundtrip(&designer, net, inv, &input);
}

// ============================================================================
// Blueprint input ⇒ empty diff, geometry inverted
// ============================================================================

#[test]
fn structure_invert_blueprint_empty_diff_and_inverted_geometry() {
    use rust_lib_flutter_cad::geo_tree::implicit_geometry::ImplicitGeometry3D;

    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, blueprint_value());
    let inv = add_invert(&mut designer, value_id);
    set_props(
        &mut designer,
        net,
        inv,
        vec![
            ("pivot_point", TextValue::IVec3(IVec3::new(2, 0, 0))),
            ("subdivision", TextValue::Int(1)),
        ],
    );

    match evaluate_pin(&designer, net, inv, 0) {
        NetworkResult::Blueprint(bp) => {
            // The input sphere is centered at the origin; the pivot is at
            // 2·a Å along x, so the inverted sphere is centered at 4·a along x.
            let expected_center = DVec3::new(4.0 * DIAMOND_UNIT_CELL_SIZE_ANGSTROM, 0.0, 0.0);
            assert!(bp.geo_tree_root.implicit_eval_3d(&expected_center) < -4.9);
            assert!(bp.geo_tree_root.implicit_eval_3d(&DVec3::ZERO) > 0.0);
        }
        other => panic!(
            "pin 0: expected Blueprint, got {:?}",
            other.infer_data_type()
        ),
    }

    match evaluate_pin(&designer, net, inv, 1) {
        NetworkResult::Molecule(m) => {
            assert!(m.atoms.is_diff(), "pin 1 must be a diff");
            assert_eq!(m.atoms.get_num_of_atoms(), 0, "Blueprint ⇒ empty diff");
        }
        other => panic!("expected Molecule diff, got {:?}", other.infer_data_type()),
    }
}

// ============================================================================
// Alignment semantics
// ============================================================================

#[test]
fn bond_center_pivot_keeps_alignment() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, crystal_value(sample_molecule()));
    let inv = add_invert(&mut designer, value_id);
    set_props(
        &mut designer,
        net,
        inv,
        vec![
            ("pivot_point", TextValue::IVec3(IVec3::new(1, 1, 1))),
            ("subdivision", TextValue::Int(8)),
        ],
    );
    let (alignment, reason) = pin0_crystal_alignment(&designer, net, inv);
    assert_eq!(alignment, Alignment::Aligned, "reason: {:?}", reason);
}

#[test]
fn lattice_point_pivot_worsens_to_motif_unaligned() {
    // Inversion through a lattice point preserves the diamond lattice but not
    // the motif (the inversion centers are at bond midpoints).
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, crystal_value(sample_molecule()));
    let inv = add_invert(&mut designer, value_id);
    set_props(
        &mut designer,
        net,
        inv,
        vec![
            ("pivot_point", TextValue::IVec3(IVec3::ZERO)),
            ("subdivision", TextValue::Int(1)),
        ],
    );
    let (alignment, _) = pin0_crystal_alignment(&designer, net, inv);
    assert_eq!(alignment, Alignment::MotifUnaligned);
}

#[test]
fn non_half_lattice_pivot_worsens_to_lattice_unaligned() {
    // 2·(1,1,1)/3 is not a lattice vector, so the lattice itself is not
    // preserved — the worst alignment level wins.
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, crystal_value(sample_molecule()));
    let inv = add_invert(&mut designer, value_id);
    set_props(
        &mut designer,
        net,
        inv,
        vec![
            ("pivot_point", TextValue::IVec3(IVec3::new(1, 1, 1))),
            ("subdivision", TextValue::Int(3)),
        ],
    );
    let (alignment, _) = pin0_crystal_alignment(&designer, net, inv);
    assert_eq!(alignment, Alignment::LatticeUnaligned);
}
