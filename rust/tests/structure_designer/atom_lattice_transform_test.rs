use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    AtomicStructure, DELETED_SITE_ATOMIC_NUMBER,
};
use rust_lib_flutter_cad::crystolecule::crystolecule_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::collections::HashMap;

// ============================================================================
// Helper Functions
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn add_atomic_value_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    position: DVec2,
    structure: AtomicStructure,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let value_data = Box::new(ValueData {
        value: NetworkResult::Atomic(structure),
    });
    network.add_node("value", position, 0, value_data)
}

fn add_unit_cell_value_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    position: DVec2,
    unit_cell: UnitCellStruct,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let value_data = Box::new(ValueData {
        value: NetworkResult::LatticeVecs(unit_cell),
    });
    network.add_node("value", position, 0, value_data)
}

fn evaluate_to_atomic(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> AtomicStructure {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];

    let result = evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context);

    match result {
        NetworkResult::Atomic(s) => s,
        NetworkResult::Error(e) => panic!("Expected Atomic result, got Error: {}", e),
        _ => panic!("Expected Atomic result, got unexpected type"),
    }
}

fn set_text_property(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    key: &str,
    value: rust_lib_flutter_cad::structure_designer::text_format::TextValue,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    let mut props = HashMap::new();
    props.insert(key.to_string(), value);
    node.data.set_text_properties(&props).unwrap();
}

// ============================================================================
// Test: atom_lmove_basic
// ============================================================================

#[test]
fn atom_lmove_basic() {
    let network_name = "test_atom_lmove_basic";
    let mut designer = setup_designer_with_network(network_name);

    let a = DIAMOND_UNIT_CELL_SIZE_ANGSTROM;

    // Create an AtomicStructure with an atom at the origin
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    // Create a cubic diamond UnitCell
    let unit_cell = UnitCellStruct::cubic_diamond();

    let structure_node_id =
        add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let unit_cell_node_id = add_unit_cell_value_node(
        &mut designer,
        network_name,
        DVec2::new(0.0, 200.0),
        unit_cell,
    );

    // Add atom_lmove node
    let lattice_move_id = designer.add_node("atom_lmove", DVec2::new(200.0, 0.0));

    // Connect: structure -> pin 0 (molecule), unit_cell -> pin 3 (unit_cell)
    designer.connect_nodes(structure_node_id, 0, lattice_move_id, 0);
    designer.connect_nodes(unit_cell_node_id, 0, lattice_move_id, 3);

    // Set translation to (1, 0, 0)
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
    set_text_property(
        &mut designer,
        network_name,
        lattice_move_id,
        "translation",
        TextValue::IVec3(glam::IVec3::new(1, 0, 0)),
    );

    // Evaluate
    let result = evaluate_to_atomic(&designer, network_name, lattice_move_id);

    // The atom should have moved by one unit cell along X
    assert_eq!(result.get_num_of_atoms(), 1);
    let atom = result.atoms_values().next().unwrap();
    assert!(
        atom.position.distance(DVec3::new(a, 0.0, 0.0)) < 0.001,
        "Expected atom at ({}, 0, 0), got ({}, {}, {})",
        a,
        atom.position.x,
        atom.position.y,
        atom.position.z
    );
}

// ============================================================================
// Test: atom_lmove_subdivision
// ============================================================================

#[test]
fn atom_lmove_subdivision() {
    let network_name = "test_atom_lmove_subdivision";
    let mut designer = setup_designer_with_network(network_name);

    let a = DIAMOND_UNIT_CELL_SIZE_ANGSTROM;

    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let unit_cell = UnitCellStruct::cubic_diamond();

    let structure_node_id =
        add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let unit_cell_node_id = add_unit_cell_value_node(
        &mut designer,
        network_name,
        DVec2::new(0.0, 200.0),
        unit_cell,
    );

    let lattice_move_id = designer.add_node("atom_lmove", DVec2::new(200.0, 0.0));

    designer.connect_nodes(structure_node_id, 0, lattice_move_id, 0);
    designer.connect_nodes(unit_cell_node_id, 0, lattice_move_id, 3);

    // Set translation (1, 0, 0) with subdivision = 2
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
    set_text_property(
        &mut designer,
        network_name,
        lattice_move_id,
        "translation",
        TextValue::IVec3(glam::IVec3::new(1, 0, 0)),
    );
    set_text_property(
        &mut designer,
        network_name,
        lattice_move_id,
        "subdivision",
        TextValue::Int(2),
    );

    let result = evaluate_to_atomic(&designer, network_name, lattice_move_id);

    // With subdivision = 2, the atom should move by half a unit cell
    assert_eq!(result.get_num_of_atoms(), 1);
    let atom = result.atoms_values().next().unwrap();
    let expected_x = a / 2.0;
    assert!(
        atom.position.distance(DVec3::new(expected_x, 0.0, 0.0)) < 0.001,
        "Expected atom at ({}, 0, 0), got ({}, {}, {})",
        expected_x,
        atom.position.x,
        atom.position.y,
        atom.position.z
    );
}

// ============================================================================
// Test: atom_lmove_diff_preserves_anchors
// ============================================================================

#[test]
fn atom_lmove_diff_preserves_anchors() {
    let network_name = "test_atom_lmove_anchors";
    let mut designer = setup_designer_with_network(network_name);

    let a = DIAMOND_UNIT_CELL_SIZE_ANGSTROM;

    // Create a diff structure with atom at (0,0,0), anchor at (0,0,0)
    let mut diff = AtomicStructure::new_diff();
    let atom_id = diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    diff.set_anchor_position(atom_id, DVec3::new(0.0, 0.0, 0.0));

    let unit_cell = UnitCellStruct::cubic_diamond();

    let diff_node_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, diff);
    let unit_cell_node_id = add_unit_cell_value_node(
        &mut designer,
        network_name,
        DVec2::new(0.0, 200.0),
        unit_cell,
    );

    let lattice_move_id = designer.add_node("atom_lmove", DVec2::new(200.0, 0.0));

    designer.connect_nodes(diff_node_id, 0, lattice_move_id, 0);
    designer.connect_nodes(unit_cell_node_id, 0, lattice_move_id, 3);

    // Set translation (2, 0, 0)
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
    set_text_property(
        &mut designer,
        network_name,
        lattice_move_id,
        "translation",
        TextValue::IVec3(glam::IVec3::new(2, 0, 0)),
    );

    let result = evaluate_to_atomic(&designer, network_name, lattice_move_id);

    assert_eq!(result.get_num_of_atoms(), 1);

    // Check atom position moved to (2 * a, 0, 0)
    let atom = result.atoms_values().next().unwrap();
    let expected_pos = DVec3::new(2.0 * a, 0.0, 0.0);
    assert!(
        atom.position.distance(expected_pos) < 0.001,
        "Expected atom at {:?}, got {:?}",
        expected_pos,
        atom.position
    );

    // Check anchor position also moved to (2 * a, 0, 0) (via Phase 1 fix)
    let anchor = result.anchor_position(atom.id);
    assert!(
        anchor.is_some(),
        "Anchor should still exist after transform"
    );
    let anchor_pos = anchor.unwrap();
    assert!(
        anchor_pos.distance(expected_pos) < 0.001,
        "Expected anchor at {:?}, got {:?}",
        expected_pos,
        anchor_pos
    );
}

// ============================================================================
// Test: atom_lrot_basic
// ============================================================================

#[test]
fn atom_lrot_basic() {
    let network_name = "test_atom_lrot_basic";
    let mut designer = setup_designer_with_network(network_name);

    let a = DIAMOND_UNIT_CELL_SIZE_ANGSTROM;

    // Create an AtomicStructure with atom at (a, a, 0) — off-axis so rotation is visible
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::new(a, a, 0.0));

    let unit_cell = UnitCellStruct::cubic_diamond();

    let structure_node_id =
        add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let unit_cell_node_id = add_unit_cell_value_node(
        &mut designer,
        network_name,
        DVec2::new(0.0, 200.0),
        unit_cell,
    );

    // Add atom_lrot node
    let lattice_rot_id = designer.add_node("atom_lrot", DVec2::new(200.0, 0.0));

    // Connect: structure -> pin 0 (molecule), unit_cell -> pin 4 (unit_cell)
    designer.connect_nodes(structure_node_id, 0, lattice_rot_id, 0);
    designer.connect_nodes(unit_cell_node_id, 0, lattice_rot_id, 4);

    // Set axis_index=0, step=1, pivot=(0,0,0)
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
    set_text_property(
        &mut designer,
        network_name,
        lattice_rot_id,
        "axis_index",
        TextValue::Int(0),
    );
    set_text_property(
        &mut designer,
        network_name,
        lattice_rot_id,
        "step",
        TextValue::Int(1),
    );

    let result = evaluate_to_atomic(&designer, network_name, lattice_rot_id);

    assert_eq!(result.get_num_of_atoms(), 1);

    let atom = result.atoms_values().next().unwrap();
    let original_pos = DVec3::new(a, a, 0.0);
    let original_distance = original_pos.length();

    // After rotation, the atom should be at the same distance from origin
    let distance_from_origin = atom.position.length();
    assert!(
        (distance_from_origin - original_distance).abs() < 0.001,
        "Atom should be at distance {} from origin, got {}",
        original_distance,
        distance_from_origin
    );

    // Verify the atom has actually moved (not still at original position)
    assert!(
        atom.position.distance(original_pos) > 0.001,
        "Atom should have moved from its original position ({:?}), but is at ({:?})",
        original_pos,
        atom.position
    );
}

// ============================================================================
// Test: atom_lmove_then_apply_diff (full integration)
// ============================================================================

#[test]
fn atom_lmove_then_apply_diff() {
    let network_name = "test_atom_lmove_apply_diff";
    let mut designer = setup_designer_with_network(network_name);

    let a = DIAMOND_UNIT_CELL_SIZE_ANGSTROM;

    // Create a base structure with atoms at (0,0,0) and (2a,0,0)
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    base.add_atom(6, DVec3::new(2.0 * a, 0.0, 0.0));

    // Create a diff with a delete marker at (0,0,0)
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));

    let unit_cell = UnitCellStruct::cubic_diamond();

    let base_node_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, base);
    let diff_node_id =
        add_atomic_value_node(&mut designer, network_name, DVec2::new(0.0, 100.0), diff);
    let unit_cell_node_id = add_unit_cell_value_node(
        &mut designer,
        network_name,
        DVec2::new(0.0, 200.0),
        unit_cell,
    );

    // Add atom_lmove and apply_diff nodes
    let lattice_move_id = designer.add_node("atom_lmove", DVec2::new(100.0, 100.0));
    let apply_diff_id = designer.add_node("apply_diff", DVec2::new(300.0, 50.0));

    // Wire: diff -> atom_lmove -> apply_diff pin 1
    designer.connect_nodes(diff_node_id, 0, lattice_move_id, 0);
    designer.connect_nodes(unit_cell_node_id, 0, lattice_move_id, 3);
    designer.connect_nodes(lattice_move_id, 0, apply_diff_id, 1);
    // Wire: base -> apply_diff pin 0
    designer.connect_nodes(base_node_id, 0, apply_diff_id, 0);

    // Set lattice_move translation to (2, 0, 0) — moves delete marker to (2a, 0, 0)
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
    set_text_property(
        &mut designer,
        network_name,
        lattice_move_id,
        "translation",
        TextValue::IVec3(glam::IVec3::new(2, 0, 0)),
    );

    // Evaluate
    let result = evaluate_to_atomic(&designer, network_name, apply_diff_id);

    // The delete marker was at (0,0,0) and moved by (2,0,0) in lattice coords → (2a,0,0).
    // So the atom at (2a,0,0) should be deleted, not the one at (0,0,0).
    assert_eq!(
        result.get_num_of_atoms(),
        1,
        "Should have 1 atom (2 - 1 deleted)"
    );

    // Atom at (0,0,0) should still exist
    let has_origin = result
        .atoms_values()
        .any(|a| a.position.distance(DVec3::ZERO) < 0.01);
    assert!(has_origin, "Atom at origin should still exist");

    // Atom at (2a,0,0) should be deleted
    let has_2a = result.atoms_values().any(|a| {
        a.position
            .distance(DVec3::new(2.0 * DIAMOND_UNIT_CELL_SIZE_ANGSTROM, 0.0, 0.0))
            < 0.01
    });
    assert!(
        !has_2a,
        "Atom at (2a,0,0) should be deleted by the moved delete marker"
    );
}

// ============================================================================
// Test: lattice_move_geometry_mode_unchanged (regression)
// ============================================================================

#[test]
fn lattice_move_geometry_mode_unchanged() {
    // Verify the lattice_move node is still registered and has the correct properties
    let registry = NodeTypeRegistry::new();

    let lattice_move_type = registry.get_node_type("lattice_move").unwrap();
    assert_eq!(lattice_move_type.name, "lattice_move");
    assert!(lattice_move_type.public);
    assert_eq!(lattice_move_type.parameters.len(), 3);
    assert_eq!(lattice_move_type.parameters[0].name, "shape");

    let lattice_rot_type = registry.get_node_type("lattice_rot").unwrap();
    assert_eq!(lattice_rot_type.name, "lattice_rot");
    assert!(lattice_rot_type.public);
    assert_eq!(lattice_rot_type.parameters.len(), 4);
    assert_eq!(lattice_rot_type.parameters[0].name, "shape");
}

// ============================================================================
// Test: node snapshot tests for atom_lmove and atom_lrot
// ============================================================================

#[test]
fn atom_lmove_node_snapshot() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("atom_lmove").unwrap();

    assert_eq!(node_type.name, "atom_lmove");
    assert!(node_type.public);
    assert_eq!(node_type.parameters.len(), 4);
    assert_eq!(node_type.parameters[0].name, "molecule");
    assert_eq!(node_type.parameters[1].name, "translation");
    assert_eq!(node_type.parameters[2].name, "subdivision");
    assert_eq!(node_type.parameters[3].name, "unit_cell");

    // Verify default data
    let data = (node_type.node_data_creator)();
    let props = data.get_text_properties();
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
    let translation_prop = props.iter().find(|(k, _)| k == "translation").unwrap();
    assert!(matches!(&translation_prop.1, TextValue::IVec3(v) if *v == glam::IVec3::ZERO));
    let subdivision_prop = props.iter().find(|(k, _)| k == "subdivision").unwrap();
    assert!(matches!(&subdivision_prop.1, TextValue::Int(1)));
}

#[test]
fn atom_lrot_node_snapshot() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("atom_lrot").unwrap();

    assert_eq!(node_type.name, "atom_lrot");
    assert!(node_type.public);
    assert_eq!(node_type.parameters.len(), 5);
    assert_eq!(node_type.parameters[0].name, "molecule");
    assert_eq!(node_type.parameters[1].name, "axis_index");
    assert_eq!(node_type.parameters[2].name, "step");
    assert_eq!(node_type.parameters[3].name, "pivot_point");
    assert_eq!(node_type.parameters[4].name, "unit_cell");

    // Verify default data
    let data = (node_type.node_data_creator)();
    let props = data.get_text_properties();
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
    let step_prop = props.iter().find(|(k, _)| k == "step").unwrap();
    assert!(matches!(&step_prop.1, TextValue::Int(0)));
    let pivot_prop = props.iter().find(|(k, _)| k == "pivot_point").unwrap();
    assert!(matches!(&pivot_prop.1, TextValue::IVec3(v) if *v == glam::IVec3::ZERO));
}
