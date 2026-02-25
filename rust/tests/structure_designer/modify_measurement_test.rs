use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AngleMoveChoice, AtomEditData, DihedralMoveChoice, DistanceMoveChoice, SelectionProvenance,
    modify_angle, modify_dihedral, modify_distance,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// =============================================================================
// Test helpers
// =============================================================================

/// Create a StructureDesigner with a single atom_edit node in diff view.
/// Returns the designer (node is already selected and active).
fn setup_atom_edit_diff_view() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    let node_id = designer.add_node("atom_edit", DVec2::ZERO);
    designer.select_node(node_id);

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test")
            .unwrap();
        let data = network
            .get_node_network_data_mut(node_id)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<AtomEditData>()
            .unwrap();
        data.output_diff = true;
    }

    designer
}

/// Evaluate the node network so the scene (and result structure) is populated.
fn refresh(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

/// Get immutable access to the AtomEditData.
fn get_data(designer: &StructureDesigner) -> &AtomEditData {
    let network = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let node_id = network.active_node_id.unwrap();
    let data = network.get_node_network_data(node_id).unwrap();
    data.as_any_ref().downcast_ref::<AtomEditData>().unwrap()
}

/// Get mutable access to the AtomEditData.
fn get_data_mut(designer: &mut StructureDesigner) -> &mut AtomEditData {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node_id = network.active_node_id.unwrap();
    let data = network.get_node_network_data_mut(node_id).unwrap();
    data.as_any_mut().downcast_mut::<AtomEditData>().unwrap()
}

/// Select diff atoms by their IDs. Adds to selection and tracks order.
fn select_diff_atoms(designer: &mut StructureDesigner, ids: &[u32]) {
    let data = get_data_mut(designer);
    for &id in ids {
        data.selection.selected_diff_atoms.insert(id);
        data.selection.track_selected(SelectionProvenance::Diff, id);
    }
}

// =============================================================================
// Distance modification tests
// =============================================================================

#[test]
fn test_modify_distance_increase() {
    let mut designer = setup_atom_edit_diff_view();

    // Add two bonded carbon atoms along X axis, distance = 1.5
    let (id0, id1) = {
        let data = get_data_mut(&mut designer);
        let id0 = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id1 = data.diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
        data.diff.add_bond_checked(id0, id1, 1);
        (id0, id1)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id0, id1]);

    // Increase distance to 2.0 by moving the second atom
    let result = modify_distance(&mut designer, 2.0, DistanceMoveChoice::Second, false);
    assert!(result.is_ok(), "modify_distance failed: {:?}", result);

    let data = get_data(&designer);
    let atom0 = data.diff.get_atom(id0).unwrap();
    let atom1 = data.diff.get_atom(id1).unwrap();

    // Atom 0 should not have moved
    assert!(
        (atom0.position - DVec3::new(0.0, 0.0, 0.0)).length() < 1e-10,
        "Fixed atom moved: {:?}",
        atom0.position
    );
    // Atom 1 should be at x=2.0
    assert!(
        (atom1.position - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-10,
        "Moving atom at wrong position: {:?}",
        atom1.position
    );
}

#[test]
fn test_modify_distance_decrease() {
    let mut designer = setup_atom_edit_diff_view();

    let (id0, id1) = {
        let data = get_data_mut(&mut designer);
        let id0 = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id1 = data.diff.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
        data.diff.add_bond_checked(id0, id1, 1);
        (id0, id1)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id0, id1]);

    let result = modify_distance(&mut designer, 1.0, DistanceMoveChoice::Second, false);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let atom1 = data.diff.get_atom(id1).unwrap();
    assert!(
        (atom1.position - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-10,
        "Expected (1, 0, 0), got {:?}",
        atom1.position
    );
}

#[test]
fn test_modify_distance_move_first_atom() {
    let mut designer = setup_atom_edit_diff_view();

    let (id0, id1) = {
        let data = get_data_mut(&mut designer);
        let id0 = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id1 = data.diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
        data.diff.add_bond_checked(id0, id1, 1);
        (id0, id1)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id0, id1]);

    // Move the first atom to increase distance to 2.0
    let result = modify_distance(&mut designer, 2.0, DistanceMoveChoice::First, false);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let atom0 = data.diff.get_atom(id0).unwrap();
    let atom1 = data.diff.get_atom(id1).unwrap();

    // Atom 1 should stay at 1.5
    assert!(
        (atom1.position - DVec3::new(1.5, 0.0, 0.0)).length() < 1e-10,
        "Fixed atom moved: {:?}",
        atom1.position
    );
    // Atom 0 should move to -0.5 (away from atom 1)
    assert!(
        (atom0.position - DVec3::new(-0.5, 0.0, 0.0)).length() < 1e-10,
        "Moving atom at wrong position: {:?}",
        atom0.position
    );
}

#[test]
fn test_modify_distance_diagonal_axis() {
    let mut designer = setup_atom_edit_diff_view();

    // Atoms along a diagonal
    let (id0, id1) = {
        let data = get_data_mut(&mut designer);
        let id0 = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id1 = data.diff.add_atom(6, DVec3::new(1.0, 1.0, 1.0));
        data.diff.add_bond_checked(id0, id1, 1);
        (id0, id1)
    };

    let original_distance = DVec3::new(1.0, 1.0, 1.0).length();
    let target = 2.0 * original_distance; // Double the distance

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id0, id1]);

    let result = modify_distance(&mut designer, target, DistanceMoveChoice::Second, false);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let atom1 = data.diff.get_atom(id1).unwrap();
    // Should be at (2, 2, 2)
    assert!(
        (atom1.position - DVec3::new(2.0, 2.0, 2.0)).length() < 1e-10,
        "Expected (2, 2, 2), got {:?}",
        atom1.position
    );
}

#[test]
fn test_modify_distance_with_fragment() {
    let mut designer = setup_atom_edit_diff_view();

    // Chain: A—B—C (select A and B, move B with fragment → C should also move)
    let (id_a, id_b, id_c) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
        let id_c = data.diff.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
        data.diff.add_bond_checked(id_a, id_b, 1);
        data.diff.add_bond_checked(id_b, id_c, 1);
        (id_a, id_b, id_c)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_b]);

    // Move B (with fragment) to increase A—B distance to 2.0
    let result = modify_distance(&mut designer, 2.0, DistanceMoveChoice::Second, true);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let atom_a = data.diff.get_atom(id_a).unwrap();
    let atom_b = data.diff.get_atom(id_b).unwrap();
    let atom_c = data.diff.get_atom(id_c).unwrap();

    // A stays at origin
    assert!(
        (atom_a.position - DVec3::new(0.0, 0.0, 0.0)).length() < 1e-10,
        "A should not move"
    );
    // B moves from 1.5 to 2.0 (delta = +0.5)
    assert!(
        (atom_b.position - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-10,
        "B at wrong position: {:?}",
        atom_b.position
    );
    // C should also shift by +0.5 (fragment following)
    assert!(
        (atom_c.position - DVec3::new(3.5, 0.0, 0.0)).length() < 1e-10,
        "C (fragment) at wrong position: {:?}",
        atom_c.position
    );
}

#[test]
fn test_modify_distance_without_fragment() {
    let mut designer = setup_atom_edit_diff_view();

    // Same chain: A—B—C, but fragment disabled
    let (id_a, id_b, id_c) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
        let id_c = data.diff.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
        data.diff.add_bond_checked(id_a, id_b, 1);
        data.diff.add_bond_checked(id_b, id_c, 1);
        (id_a, id_b, id_c)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_b]);

    // Move B (without fragment) to increase A—B distance to 2.0
    let result = modify_distance(&mut designer, 2.0, DistanceMoveChoice::Second, false);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let atom_b = data.diff.get_atom(id_b).unwrap();
    let atom_c = data.diff.get_atom(id_c).unwrap();

    // B moves
    assert!(
        (atom_b.position - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-10,
        "B at wrong position: {:?}",
        atom_b.position
    );
    // C should NOT move (fragment disabled)
    assert!(
        (atom_c.position - DVec3::new(3.0, 0.0, 0.0)).length() < 1e-10,
        "C should not move when fragment is disabled: {:?}",
        atom_c.position
    );
}

#[test]
fn test_modify_distance_target_equals_current_is_noop() {
    let mut designer = setup_atom_edit_diff_view();

    let (id0, id1) = {
        let data = get_data_mut(&mut designer);
        let id0 = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id1 = data.diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
        data.diff.add_bond_checked(id0, id1, 1);
        (id0, id1)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id0, id1]);

    // Target = current distance
    let result = modify_distance(&mut designer, 1.5, DistanceMoveChoice::Second, false);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let atom1 = data.diff.get_atom(id1).unwrap();
    assert!(
        (atom1.position - DVec3::new(1.5, 0.0, 0.0)).length() < 1e-10,
        "Atom should not move when target equals current"
    );
}

#[test]
fn test_modify_distance_rejects_too_small() {
    let mut designer = setup_atom_edit_diff_view();

    let (id0, id1) = {
        let data = get_data_mut(&mut designer);
        let id0 = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id1 = data.diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
        (id0, id1)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id0, id1]);

    let result = modify_distance(&mut designer, 0.05, DistanceMoveChoice::Second, false);
    assert!(result.is_err());
}

// =============================================================================
// Angle modification tests
// =============================================================================

#[test]
fn test_modify_angle_increase() {
    let mut designer = setup_atom_edit_diff_view();

    // Right-angle: A at origin, vertex at (1,0,0), B at (1,1,0). Angle = 90°.
    let (id_a, id_v, id_b) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_v = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(1.0, 1.0, 0.0));
        data.diff.add_bond_checked(id_a, id_v, 1);
        data.diff.add_bond_checked(id_v, id_b, 1);
        (id_a, id_v, id_b)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_v, id_b]);

    // Increase angle to 120° by moving arm B
    let result = modify_angle(&mut designer, 120.0, AngleMoveChoice::ArmB, false);
    assert!(result.is_ok(), "modify_angle failed: {:?}", result);

    let data = get_data(&designer);
    let atom_v = data.diff.get_atom(id_v).unwrap();
    let atom_b = data.diff.get_atom(id_b).unwrap();

    // Vertex should not move
    assert!(
        (atom_v.position - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-10,
        "Vertex moved"
    );

    // Verify the new angle is 120°
    let v = atom_v.position;
    let atom_a = data.diff.get_atom(id_a).unwrap();
    let va = (atom_a.position - v).normalize();
    let vb = (atom_b.position - v).normalize();
    let measured_angle = va.dot(vb).clamp(-1.0, 1.0).acos().to_degrees();
    assert!(
        (measured_angle - 120.0).abs() < 0.1,
        "Expected 120°, got {:.2}°",
        measured_angle
    );

    // Distance from vertex to B should be preserved (1.0)
    let dist = atom_b.position.distance(v);
    assert!(
        (dist - 1.0).abs() < 1e-10,
        "Distance from vertex to arm changed: {dist}"
    );
}

#[test]
fn test_modify_angle_move_arm_a() {
    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_v, id_b) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_v = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(1.0, 1.0, 0.0));
        data.diff.add_bond_checked(id_a, id_v, 1);
        data.diff.add_bond_checked(id_v, id_b, 1);
        (id_a, id_v, id_b)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_v, id_b]);

    // Move arm A instead of arm B
    let result = modify_angle(&mut designer, 60.0, AngleMoveChoice::ArmA, false);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let atom_v = data.diff.get_atom(id_v).unwrap();
    let atom_a = data.diff.get_atom(id_a).unwrap();
    let atom_b = data.diff.get_atom(id_b).unwrap();

    // B should not move
    assert!(
        (atom_b.position - DVec3::new(1.0, 1.0, 0.0)).length() < 1e-10,
        "Arm B moved when arm A was chosen"
    );

    // Verify the new angle is 60°
    let v = atom_v.position;
    let va = (atom_a.position - v).normalize();
    let vb = (atom_b.position - v).normalize();
    let measured_angle = va.dot(vb).clamp(-1.0, 1.0).acos().to_degrees();
    assert!(
        (measured_angle - 60.0).abs() < 0.1,
        "Expected 60°, got {:.2}°",
        measured_angle
    );
}

#[test]
fn test_modify_angle_with_fragment() {
    let mut designer = setup_atom_edit_diff_view();

    // Chain: A—V—B—C, modify angle at V, B arm should drag C along
    let (id_a, id_v, id_b, id_c) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_v = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(1.0, 1.0, 0.0));
        let id_c = data.diff.add_atom(6, DVec3::new(1.0, 2.0, 0.0));
        data.diff.add_bond_checked(id_a, id_v, 1);
        data.diff.add_bond_checked(id_v, id_b, 1);
        data.diff.add_bond_checked(id_b, id_c, 1);
        (id_a, id_v, id_b, id_c)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_v, id_b]);

    // Move arm B with fragment following — C should rotate too
    let result = modify_angle(&mut designer, 120.0, AngleMoveChoice::ArmB, true);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let atom_c = data.diff.get_atom(id_c).unwrap();

    // C should have moved (not still at (1, 2, 0))
    assert!(
        (atom_c.position - DVec3::new(1.0, 2.0, 0.0)).length() > 0.1,
        "C should have moved with the fragment, but is at {:?}",
        atom_c.position
    );
}

#[test]
fn test_modify_angle_without_fragment() {
    let mut designer = setup_atom_edit_diff_view();

    // Same chain as above, but without fragment following
    let (id_a, id_v, id_b, id_c) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_v = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(1.0, 1.0, 0.0));
        let id_c = data.diff.add_atom(6, DVec3::new(1.0, 2.0, 0.0));
        data.diff.add_bond_checked(id_a, id_v, 1);
        data.diff.add_bond_checked(id_v, id_b, 1);
        data.diff.add_bond_checked(id_b, id_c, 1);
        (id_a, id_v, id_b, id_c)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_v, id_b]);

    let result = modify_angle(&mut designer, 120.0, AngleMoveChoice::ArmB, false);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let atom_c = data.diff.get_atom(id_c).unwrap();

    // C should NOT have moved (fragment disabled)
    assert!(
        (atom_c.position - DVec3::new(1.0, 2.0, 0.0)).length() < 1e-10,
        "C should not move when fragment is disabled: {:?}",
        atom_c.position
    );
}

#[test]
fn test_modify_angle_target_equals_current() {
    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_v, id_b) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_v = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(1.0, 1.0, 0.0));
        data.diff.add_bond_checked(id_a, id_v, 1);
        data.diff.add_bond_checked(id_v, id_b, 1);
        (id_a, id_v, id_b)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_v, id_b]);

    // Target = current angle (90°)
    let result = modify_angle(&mut designer, 90.0, AngleMoveChoice::ArmB, false);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let atom_b = data.diff.get_atom(id_b).unwrap();
    assert!(
        (atom_b.position - DVec3::new(1.0, 1.0, 0.0)).length() < 1e-10,
        "Atom should not move when target equals current"
    );
}

#[test]
fn test_modify_angle_rejects_out_of_range() {
    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_v, id_b) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_v = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(1.0, 1.0, 0.0));
        (id_a, id_v, id_b)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_v, id_b]);

    assert!(modify_angle(&mut designer, -10.0, AngleMoveChoice::ArmB, false).is_err());
    assert!(modify_angle(&mut designer, 200.0, AngleMoveChoice::ArmB, false).is_err());
}

#[test]
fn test_modify_angle_collinear_atoms() {
    let mut designer = setup_atom_edit_diff_view();

    // Collinear: A(0,0,0)—V(1,0,0)—B(2,0,0), angle = 180°
    let (id_a, id_v, id_b) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_v = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
        data.diff.add_bond_checked(id_a, id_v, 1);
        data.diff.add_bond_checked(id_v, id_b, 1);
        (id_a, id_v, id_b)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_v, id_b]);

    // Modify to 90° — should work even though cross product is zero
    let result = modify_angle(&mut designer, 90.0, AngleMoveChoice::ArmB, false);
    assert!(
        result.is_ok(),
        "Collinear angle modification failed: {:?}",
        result
    );

    let data = get_data(&designer);
    let atom_v = data.diff.get_atom(id_v).unwrap();
    let atom_a = data.diff.get_atom(id_a).unwrap();
    let atom_b = data.diff.get_atom(id_b).unwrap();

    let v = atom_v.position;
    let va = (atom_a.position - v).normalize();
    let vb = (atom_b.position - v).normalize();
    let measured_angle = va.dot(vb).clamp(-1.0, 1.0).acos().to_degrees();
    assert!(
        (measured_angle - 90.0).abs() < 0.1,
        "Expected 90°, got {:.2}°",
        measured_angle
    );
}

// =============================================================================
// Dihedral modification tests
// =============================================================================

#[test]
fn test_modify_dihedral_basic() {
    let mut designer = setup_atom_edit_diff_view();

    // Chain A—B—C—D with a 90° dihedral
    // A above the XZ plane, B at origin, C along X, D along Z from C
    let (id_a, id_b, id_c, id_d) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 1.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_c = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
        let id_d = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 1.0));
        data.diff.add_bond_checked(id_a, id_b, 1);
        data.diff.add_bond_checked(id_b, id_c, 1);
        data.diff.add_bond_checked(id_c, id_d, 1);
        (id_a, id_b, id_c, id_d)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_b, id_c, id_d]);

    // Rotate D-side to make dihedral = 0° (eclipsed)
    let result = modify_dihedral(&mut designer, 0.0, DihedralMoveChoice::DSide, false);
    assert!(result.is_ok(), "modify_dihedral failed: {:?}", result);

    // Verify the dihedral is now ~0°
    let data = get_data(&designer);
    let a = data.diff.get_atom(id_a).unwrap().position;
    let b = data.diff.get_atom(id_b).unwrap().position;
    let c = data.diff.get_atom(id_c).unwrap().position;
    let d = data.diff.get_atom(id_d).unwrap().position;

    let measured = compute_dihedral_angle(a, b, c, d);
    assert!(
        measured.abs() < 1.0,
        "Expected ~0° dihedral, got {:.2}°",
        measured
    );
}

#[test]
fn test_modify_dihedral_to_180() {
    let mut designer = setup_atom_edit_diff_view();

    // Start with eclipsed (0°), set to anti-periplanar (180°)
    let (id_a, id_b, id_c, id_d) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 1.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_c = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
        let id_d = data.diff.add_atom(6, DVec3::new(1.0, 1.0, 0.0)); // eclipsed
        data.diff.add_bond_checked(id_a, id_b, 1);
        data.diff.add_bond_checked(id_b, id_c, 1);
        data.diff.add_bond_checked(id_c, id_d, 1);
        (id_a, id_b, id_c, id_d)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_b, id_c, id_d]);

    let result = modify_dihedral(&mut designer, 180.0, DihedralMoveChoice::DSide, false);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let a = data.diff.get_atom(id_a).unwrap().position;
    let b = data.diff.get_atom(id_b).unwrap().position;
    let c = data.diff.get_atom(id_c).unwrap().position;
    let d = data.diff.get_atom(id_d).unwrap().position;

    let measured = compute_dihedral_angle(a, b, c, d);
    assert!(
        (measured.abs() - 180.0).abs() < 1.0,
        "Expected ~180° dihedral, got {:.2}°",
        measured
    );
}

#[test]
fn test_modify_dihedral_move_a_side() {
    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_b, id_c, id_d) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 1.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_c = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
        let id_d = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 1.0));
        data.diff.add_bond_checked(id_a, id_b, 1);
        data.diff.add_bond_checked(id_b, id_c, 1);
        data.diff.add_bond_checked(id_c, id_d, 1);
        (id_a, id_b, id_c, id_d)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_b, id_c, id_d]);

    // Move A-side instead of D-side
    let result = modify_dihedral(&mut designer, 0.0, DihedralMoveChoice::ASide, false);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let d_pos = data.diff.get_atom(id_d).unwrap().position;
    // D should not have moved
    assert!(
        (d_pos - DVec3::new(1.0, 0.0, 1.0)).length() < 1e-10,
        "D should not move when A-side is chosen: {:?}",
        d_pos
    );
}

#[test]
fn test_modify_dihedral_with_fragment() {
    let mut designer = setup_atom_edit_diff_view();

    // Chain: A—B—C—D—E, modify dihedral A-B-C-D, D-side with fragment → E moves too
    let (id_a, id_b, id_c, id_d, id_e) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 1.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_c = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
        let id_d = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 1.0));
        let id_e = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 2.0));
        data.diff.add_bond_checked(id_a, id_b, 1);
        data.diff.add_bond_checked(id_b, id_c, 1);
        data.diff.add_bond_checked(id_c, id_d, 1);
        data.diff.add_bond_checked(id_d, id_e, 1);
        (id_a, id_b, id_c, id_d, id_e)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_b, id_c, id_d]);

    let result = modify_dihedral(&mut designer, 0.0, DihedralMoveChoice::DSide, true);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let e_pos = data.diff.get_atom(id_e).unwrap().position;

    // E should have moved (fragment following)
    assert!(
        (e_pos - DVec3::new(1.0, 0.0, 2.0)).length() > 0.1,
        "E should have moved with the fragment, but is at {:?}",
        e_pos
    );
}

#[test]
fn test_modify_dihedral_without_fragment() {
    let mut designer = setup_atom_edit_diff_view();

    // Same chain as above, but without fragment
    let (id_a, id_b, id_c, id_d, id_e) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 1.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_c = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
        let id_d = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 1.0));
        let id_e = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 2.0));
        data.diff.add_bond_checked(id_a, id_b, 1);
        data.diff.add_bond_checked(id_b, id_c, 1);
        data.diff.add_bond_checked(id_c, id_d, 1);
        data.diff.add_bond_checked(id_d, id_e, 1);
        (id_a, id_b, id_c, id_d, id_e)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_b, id_c, id_d]);

    let result = modify_dihedral(&mut designer, 0.0, DihedralMoveChoice::DSide, false);
    assert!(result.is_ok());

    let data = get_data(&designer);
    let e_pos = data.diff.get_atom(id_e).unwrap().position;

    // E should NOT have moved
    assert!(
        (e_pos - DVec3::new(1.0, 0.0, 2.0)).length() < 1e-10,
        "E should not move when fragment is disabled: {:?}",
        e_pos
    );
}

#[test]
fn test_modify_dihedral_rejects_out_of_range() {
    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_b, id_c, id_d) = {
        let data = get_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, DVec3::new(0.0, 1.0, 0.0));
        let id_b = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let id_c = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
        let id_d = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 1.0));
        (id_a, id_b, id_c, id_d)
    };

    refresh(&mut designer);
    select_diff_atoms(&mut designer, &[id_a, id_b, id_c, id_d]);

    assert!(modify_dihedral(&mut designer, -200.0, DihedralMoveChoice::DSide, false).is_err());
    assert!(modify_dihedral(&mut designer, 200.0, DihedralMoveChoice::DSide, false).is_err());
}

// =============================================================================
// Dihedral angle computation helper (for verification)
// =============================================================================

/// Compute the dihedral angle A-B-C-D in degrees, matching the measurement system.
fn compute_dihedral_angle(a: DVec3, b: DVec3, c: DVec3, d: DVec3) -> f64 {
    let b1 = b - a;
    let b2 = c - b;
    let b3 = d - c;

    let n1 = b1.cross(b2);
    let n2 = b2.cross(b3);

    if n1.length_squared() < 1e-20 || n2.length_squared() < 1e-20 {
        return 0.0;
    }

    let n1 = n1.normalize();
    let n2 = n2.normalize();
    let m1 = n1.cross(b2.normalize());
    let x = n1.dot(n2);
    let y = m1.dot(n2);
    (-y).atan2(x).to_degrees()
}
