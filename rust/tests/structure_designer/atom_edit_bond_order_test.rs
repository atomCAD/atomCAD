use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::APIAtomEditTool;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_AROMATIC, BOND_DATIVE, BOND_DELETED, BOND_DOUBLE, BOND_METALLIC, BOND_QUADRUPLE,
    BOND_SINGLE, BOND_TRIPLE,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure::{AtomicStructure, BondReference};
use rust_lib_flutter_cad::crystolecule::atomic_structure_diff::apply_diff;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AddBondInteractionState, AddBondMoveResult, AddBondToolState, AtomEditData, AtomEditTool,
    cycle_bond_order,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// =============================================================================
// Helper: set up a StructureDesigner with an atom_edit node in diff view
// =============================================================================

fn setup_atom_edit_diff_view() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    let node_id = designer.add_node("atom_edit", DVec2::ZERO);
    designer.select_node(node_id);

    // Set output_diff = true for diff view
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

/// Access the AtomEditData on the selected node.
fn get_atom_edit_data(designer: &StructureDesigner) -> &AtomEditData {
    let network = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let node_id = network.active_node_id.unwrap();
    let data = network.get_node_network_data(node_id).unwrap();
    data.as_any_ref().downcast_ref::<AtomEditData>().unwrap()
}

/// Access the AtomEditData mutably on the selected node.
fn get_atom_edit_data_mut(designer: &mut StructureDesigner) -> &mut AtomEditData {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node_id = network.active_node_id.unwrap();
    let data = network.get_node_network_data_mut(node_id).unwrap();
    data.as_any_mut().downcast_mut::<AtomEditData>().unwrap()
}

// =============================================================================
// cycle_bond_order tests
// =============================================================================

#[test]
fn test_cycle_bond_order_single_to_double() {
    assert_eq!(cycle_bond_order(BOND_SINGLE), BOND_DOUBLE);
}

#[test]
fn test_cycle_bond_order_double_to_triple() {
    assert_eq!(cycle_bond_order(BOND_DOUBLE), BOND_TRIPLE);
}

#[test]
fn test_cycle_bond_order_triple_to_single() {
    assert_eq!(cycle_bond_order(BOND_TRIPLE), BOND_SINGLE);
}

#[test]
fn test_cycle_bond_order_quadruple_to_single() {
    assert_eq!(cycle_bond_order(BOND_QUADRUPLE), BOND_SINGLE);
}

#[test]
fn test_cycle_bond_order_aromatic_to_single() {
    assert_eq!(cycle_bond_order(BOND_AROMATIC), BOND_SINGLE);
}

#[test]
fn test_cycle_bond_order_dative_to_single() {
    assert_eq!(cycle_bond_order(BOND_DATIVE), BOND_SINGLE);
}

#[test]
fn test_cycle_bond_order_metallic_to_single() {
    assert_eq!(cycle_bond_order(BOND_METALLIC), BOND_SINGLE);
}

#[test]
fn test_cycle_bond_order_deleted_to_single() {
    // Bond order 0 (BOND_DELETED) also cycles to single via the catch-all arm
    assert_eq!(cycle_bond_order(BOND_DELETED), BOND_SINGLE);
}

#[test]
fn test_cycle_bond_order_full_cycle() {
    // Verify the complete common-order cycle: single → double → triple → single
    let mut order = BOND_SINGLE;
    order = cycle_bond_order(order);
    assert_eq!(order, BOND_DOUBLE);
    order = cycle_bond_order(order);
    assert_eq!(order, BOND_TRIPLE);
    order = cycle_bond_order(order);
    assert_eq!(order, BOND_SINGLE);
}

// =============================================================================
// Bond creation with all 7 orders (via AtomEditData directly)
// =============================================================================

#[test]
fn test_add_bond_single_order() {
    let mut data = AtomEditData::new();
    let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
    let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));

    data.add_bond_in_diff(id_a, id_b, BOND_SINGLE);

    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds.len(), 1);
    assert_eq!(atom.bonds[0].bond_order(), BOND_SINGLE);
    assert_eq!(atom.bonds[0].other_atom_id(), id_b);
}

#[test]
fn test_add_bond_double_order() {
    let mut data = AtomEditData::new();
    let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
    let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));

    data.add_bond_in_diff(id_a, id_b, BOND_DOUBLE);

    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds[0].bond_order(), BOND_DOUBLE);
}

#[test]
fn test_add_bond_triple_order() {
    let mut data = AtomEditData::new();
    let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
    let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));

    data.add_bond_in_diff(id_a, id_b, BOND_TRIPLE);

    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds[0].bond_order(), BOND_TRIPLE);
}

#[test]
fn test_add_bond_quadruple_order() {
    let mut data = AtomEditData::new();
    let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
    let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));

    data.add_bond_in_diff(id_a, id_b, BOND_QUADRUPLE);

    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds[0].bond_order(), BOND_QUADRUPLE);
}

#[test]
fn test_add_bond_aromatic_order() {
    let mut data = AtomEditData::new();
    let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
    let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));

    data.add_bond_in_diff(id_a, id_b, BOND_AROMATIC);

    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds[0].bond_order(), BOND_AROMATIC);
}

#[test]
fn test_add_bond_dative_order() {
    let mut data = AtomEditData::new();
    let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
    let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));

    data.add_bond_in_diff(id_a, id_b, BOND_DATIVE);

    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds[0].bond_order(), BOND_DATIVE);
}

#[test]
fn test_add_bond_metallic_order() {
    let mut data = AtomEditData::new();
    let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
    let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));

    data.add_bond_in_diff(id_a, id_b, BOND_METALLIC);

    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds[0].bond_order(), BOND_METALLIC);
}

// =============================================================================
// Bond order overwrite (add_bond_in_diff on existing bond changes order)
// =============================================================================

#[test]
fn test_overwrite_bond_order() {
    let mut data = AtomEditData::new();
    let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
    let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));

    // Create single bond
    data.add_bond_in_diff(id_a, id_b, BOND_SINGLE);
    assert_eq!(
        data.diff.get_atom(id_a).unwrap().bonds[0].bond_order(),
        BOND_SINGLE
    );

    // Overwrite with double bond
    data.add_bond_in_diff(id_a, id_b, BOND_DOUBLE);
    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds.len(), 1); // Still one bond, not duplicated
    assert_eq!(atom.bonds[0].bond_order(), BOND_DOUBLE);
}

#[test]
fn test_overwrite_bond_order_all_types() {
    let mut data = AtomEditData::new();
    let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
    let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));

    // Cycle through all bond orders, overwriting each time
    for order in [
        BOND_SINGLE,
        BOND_DOUBLE,
        BOND_TRIPLE,
        BOND_QUADRUPLE,
        BOND_AROMATIC,
        BOND_DATIVE,
        BOND_METALLIC,
    ] {
        data.add_bond_in_diff(id_a, id_b, order);
        let atom = data.diff.get_atom(id_a).unwrap();
        assert_eq!(
            atom.bonds.len(),
            1,
            "Bond duplicated on order change to {order}"
        );
        assert_eq!(atom.bonds[0].bond_order(), order, "Expected order {order}");
    }
}

// =============================================================================
// Bond creation clears bond selection
// =============================================================================

#[test]
fn test_add_bond_in_diff_clears_bond_selection() {
    let mut data = AtomEditData::new();
    let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
    let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));

    // Pre-populate selection with a bond
    data.selection.selected_bonds.insert(BondReference {
        atom_id1: 100,
        atom_id2: 200,
    });
    assert!(!data.selection.selected_bonds.is_empty());

    data.add_bond_in_diff(id_a, id_b, BOND_SINGLE);

    assert!(data.selection.selected_bonds.is_empty());
}

// =============================================================================
// change_selected_bonds_order via StructureDesigner (diff view)
// =============================================================================

#[test]
fn test_change_selected_bonds_order_diff_view_single_bond() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::change_selected_bonds_order;

    let mut designer = setup_atom_edit_diff_view();

    // Add atoms and a single bond
    let (id_a, id_b) = {
        let data = get_atom_edit_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));
        data.add_bond_in_diff(id_a, id_b, BOND_SINGLE);
        (id_a, id_b)
    };

    // Select the bond
    {
        let data = get_atom_edit_data_mut(&mut designer);
        data.selection.selected_bonds.insert(BondReference {
            atom_id1: id_a,
            atom_id2: id_b,
        });
    }

    // Change to double
    change_selected_bonds_order(&mut designer, BOND_DOUBLE);

    let data = get_atom_edit_data(&designer);
    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds[0].bond_order(), BOND_DOUBLE);
}

#[test]
fn test_change_selected_bonds_order_diff_view_all_orders() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::change_selected_bonds_order;

    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_b) = {
        let data = get_atom_edit_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));
        data.add_bond_in_diff(id_a, id_b, BOND_SINGLE);
        (id_a, id_b)
    };

    for order in [
        BOND_SINGLE,
        BOND_DOUBLE,
        BOND_TRIPLE,
        BOND_QUADRUPLE,
        BOND_AROMATIC,
        BOND_DATIVE,
        BOND_METALLIC,
    ] {
        // Re-select the bond (add_bond_in_diff clears selection)
        {
            let data = get_atom_edit_data_mut(&mut designer);
            data.selection.selected_bonds.insert(BondReference {
                atom_id1: id_a,
                atom_id2: id_b,
            });
        }

        change_selected_bonds_order(&mut designer, order);

        let data = get_atom_edit_data(&designer);
        let atom = data.diff.get_atom(id_a).unwrap();
        assert_eq!(
            atom.bonds[0].bond_order(),
            order,
            "Expected bond order {order} after change_selected_bonds_order"
        );
    }
}

#[test]
fn test_change_selected_bonds_order_rejects_order_zero() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::change_selected_bonds_order;

    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_b) = {
        let data = get_atom_edit_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));
        data.add_bond_in_diff(id_a, id_b, BOND_SINGLE);
        (id_a, id_b)
    };

    {
        let data = get_atom_edit_data_mut(&mut designer);
        data.selection.selected_bonds.insert(BondReference {
            atom_id1: id_a,
            atom_id2: id_b,
        });
    }

    // Order 0 (BOND_DELETED) should be rejected — bond stays single
    change_selected_bonds_order(&mut designer, 0);

    let data = get_atom_edit_data(&designer);
    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds[0].bond_order(), BOND_SINGLE);
}

#[test]
fn test_change_selected_bonds_order_rejects_order_above_7() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::change_selected_bonds_order;

    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_b) = {
        let data = get_atom_edit_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));
        data.add_bond_in_diff(id_a, id_b, BOND_SINGLE);
        (id_a, id_b)
    };

    {
        let data = get_atom_edit_data_mut(&mut designer);
        data.selection.selected_bonds.insert(BondReference {
            atom_id1: id_a,
            atom_id2: id_b,
        });
    }

    // Order 8 should be rejected — bond stays single
    change_selected_bonds_order(&mut designer, 8);

    let data = get_atom_edit_data(&designer);
    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds[0].bond_order(), BOND_SINGLE);
}

#[test]
fn test_change_selected_bonds_order_multiple_bonds() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::change_selected_bonds_order;

    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_b, id_c) = {
        let data = get_atom_edit_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));
        let id_c = data.diff.add_atom(6, glam::f64::DVec3::new(3.0, 0.0, 0.0));
        data.add_bond_in_diff(id_a, id_b, BOND_SINGLE);
        data.add_bond_in_diff(id_b, id_c, BOND_SINGLE);
        (id_a, id_b, id_c)
    };

    // Select both bonds
    {
        let data = get_atom_edit_data_mut(&mut designer);
        data.selection.selected_bonds.insert(BondReference {
            atom_id1: id_a,
            atom_id2: id_b,
        });
        data.selection.selected_bonds.insert(BondReference {
            atom_id1: id_b,
            atom_id2: id_c,
        });
    }

    change_selected_bonds_order(&mut designer, BOND_TRIPLE);

    let data = get_atom_edit_data(&designer);

    // Check bond A-B
    let atom_a = data.diff.get_atom(id_a).unwrap();
    let bond_ab = atom_a
        .bonds
        .iter()
        .find(|b| b.other_atom_id() == id_b)
        .unwrap();
    assert_eq!(bond_ab.bond_order(), BOND_TRIPLE);

    // Check bond B-C
    let atom_b = data.diff.get_atom(id_b).unwrap();
    let bond_bc = atom_b
        .bonds
        .iter()
        .find(|b| b.other_atom_id() == id_c)
        .unwrap();
    assert_eq!(bond_bc.bond_order(), BOND_TRIPLE);
}

// =============================================================================
// change_bond_order via StructureDesigner (diff view)
// =============================================================================

#[test]
fn test_change_bond_order_diff_view() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::change_bond_order;

    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_b) = {
        let data = get_atom_edit_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));
        data.add_bond_in_diff(id_a, id_b, BOND_SINGLE);
        (id_a, id_b)
    };

    let bond_ref = BondReference {
        atom_id1: id_a,
        atom_id2: id_b,
    };
    change_bond_order(&mut designer, &bond_ref, BOND_TRIPLE);

    let data = get_atom_edit_data(&designer);
    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds[0].bond_order(), BOND_TRIPLE);
}

#[test]
fn test_change_bond_order_rejects_zero() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::change_bond_order;

    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_b) = {
        let data = get_atom_edit_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));
        data.add_bond_in_diff(id_a, id_b, BOND_SINGLE);
        (id_a, id_b)
    };

    let bond_ref = BondReference {
        atom_id1: id_a,
        atom_id2: id_b,
    };
    change_bond_order(&mut designer, &bond_ref, 0);

    // Bond should remain unchanged
    let data = get_atom_edit_data(&designer);
    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds[0].bond_order(), BOND_SINGLE);
}

#[test]
fn test_change_bond_order_rejects_above_7() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::change_bond_order;

    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_b) = {
        let data = get_atom_edit_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));
        data.add_bond_in_diff(id_a, id_b, BOND_DOUBLE);
        (id_a, id_b)
    };

    let bond_ref = BondReference {
        atom_id1: id_a,
        atom_id2: id_b,
    };
    change_bond_order(&mut designer, &bond_ref, 8);

    let data = get_atom_edit_data(&designer);
    let atom = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom.bonds[0].bond_order(), BOND_DOUBLE);
}

// =============================================================================
// AddBondMoveResult default
// =============================================================================

#[test]
fn test_add_bond_move_result_default() {
    let result = AddBondMoveResult::default();
    assert!(!result.is_dragging);
    assert!(result.source_atom_pos.is_none());
    assert!(result.preview_end_pos.is_none());
    assert!(!result.snapped_to_atom);
    assert_eq!(result.bond_order, BOND_SINGLE);
}

// =============================================================================
// AddBondInteractionState default
// =============================================================================

#[test]
fn test_add_bond_interaction_state_default_is_idle() {
    let state = AddBondInteractionState::default();
    assert!(matches!(state, AddBondInteractionState::Idle));
}

// =============================================================================
// AddBondToolState initialization
// =============================================================================

#[test]
fn test_add_bond_tool_state_default_bond_order() {
    // When switching to the AddBond tool, bond_order should default to BOND_SINGLE
    let mut data = AtomEditData::new();
    data.set_active_tool(APIAtomEditTool::AddBond);

    match &data.active_tool {
        AtomEditTool::AddBond(state) => {
            assert_eq!(state.bond_order, BOND_SINGLE);
            assert!(matches!(
                state.interaction_state,
                AddBondInteractionState::Idle
            ));
        }
        _ => panic!("Expected AddBond tool"),
    }
}

// =============================================================================
// set_add_bond_order via StructureDesigner
// =============================================================================

#[test]
fn test_set_add_bond_order_valid() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::set_add_bond_order;

    let mut designer = setup_atom_edit_diff_view();

    // Switch to AddBond tool
    {
        let data = get_atom_edit_data_mut(&mut designer);
        data.set_active_tool(APIAtomEditTool::AddBond);
    }

    for order in 1u8..=7 {
        set_add_bond_order(&mut designer, order);

        let data = get_atom_edit_data(&designer);
        match &data.active_tool {
            AtomEditTool::AddBond(state) => {
                assert_eq!(state.bond_order, order, "Expected bond order {order}");
            }
            _ => panic!("Expected AddBond tool"),
        }
    }
}

#[test]
fn test_set_add_bond_order_rejects_zero() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::set_add_bond_order;

    let mut designer = setup_atom_edit_diff_view();

    {
        let data = get_atom_edit_data_mut(&mut designer);
        data.set_active_tool(APIAtomEditTool::AddBond);
    }

    // Set to a valid order first
    set_add_bond_order(&mut designer, BOND_DOUBLE);

    // Try to set order 0 — should be rejected
    set_add_bond_order(&mut designer, 0);

    let data = get_atom_edit_data(&designer);
    match &data.active_tool {
        AtomEditTool::AddBond(state) => {
            assert_eq!(state.bond_order, BOND_DOUBLE); // Unchanged
        }
        _ => panic!("Expected AddBond tool"),
    }
}

#[test]
fn test_set_add_bond_order_rejects_above_7() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::set_add_bond_order;

    let mut designer = setup_atom_edit_diff_view();

    {
        let data = get_atom_edit_data_mut(&mut designer);
        data.set_active_tool(APIAtomEditTool::AddBond);
    }

    set_add_bond_order(&mut designer, BOND_TRIPLE);

    // Try to set order 8 — should be rejected
    set_add_bond_order(&mut designer, 8);

    let data = get_atom_edit_data(&designer);
    match &data.active_tool {
        AtomEditTool::AddBond(state) => {
            assert_eq!(state.bond_order, BOND_TRIPLE); // Unchanged
        }
        _ => panic!("Expected AddBond tool"),
    }
}

// =============================================================================
// add_bond_reset_interaction
// =============================================================================

#[test]
fn test_add_bond_reset_interaction() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::add_bond_reset_interaction;

    let mut designer = setup_atom_edit_diff_view();

    // Switch to AddBond tool and manually set a non-Idle state
    {
        let data = get_atom_edit_data_mut(&mut designer);
        data.active_tool = AtomEditTool::AddBond(AddBondToolState {
            bond_order: BOND_DOUBLE,
            interaction_state: AddBondInteractionState::Dragging {
                source_atom_id: 42,
                preview_target: Some(99),
            },
            last_atom_id: None,
        });
    }

    add_bond_reset_interaction(&mut designer);

    let data = get_atom_edit_data(&designer);
    match &data.active_tool {
        AtomEditTool::AddBond(state) => {
            assert!(matches!(
                state.interaction_state,
                AddBondInteractionState::Idle
            ));
            // Bond order should be preserved
            assert_eq!(state.bond_order, BOND_DOUBLE);
        }
        _ => panic!("Expected AddBond tool"),
    }
}

// =============================================================================
// Bond order cycling integration: click-to-cycle in Default tool (diff view)
//
// The Default tool's pointer_up PendingBond path looks up the bond order
// from the structure, calls cycle_bond_order, then change_bond_order.
// We test the composed behavior by calling change_bond_order with cycled
// orders, since the actual pointer_up requires ray-cast hit testing.
// =============================================================================

#[test]
fn test_bond_order_cycle_integration_diff_view() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::change_bond_order;

    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_b) = {
        let data = get_atom_edit_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));
        data.add_bond_in_diff(id_a, id_b, BOND_SINGLE);
        (id_a, id_b)
    };

    let bond_ref = BondReference {
        atom_id1: id_a,
        atom_id2: id_b,
    };

    // Simulate the Default tool cycle: read order, cycle, apply
    // Cycle 1: single → double
    let current = get_atom_edit_data(&designer)
        .diff
        .get_atom(id_a)
        .unwrap()
        .bonds[0]
        .bond_order();
    change_bond_order(&mut designer, &bond_ref, cycle_bond_order(current));
    assert_eq!(
        get_atom_edit_data(&designer)
            .diff
            .get_atom(id_a)
            .unwrap()
            .bonds[0]
            .bond_order(),
        BOND_DOUBLE
    );

    // Cycle 2: double → triple
    let current = get_atom_edit_data(&designer)
        .diff
        .get_atom(id_a)
        .unwrap()
        .bonds[0]
        .bond_order();
    change_bond_order(&mut designer, &bond_ref, cycle_bond_order(current));
    assert_eq!(
        get_atom_edit_data(&designer)
            .diff
            .get_atom(id_a)
            .unwrap()
            .bonds[0]
            .bond_order(),
        BOND_TRIPLE
    );

    // Cycle 3: triple → single
    let current = get_atom_edit_data(&designer)
        .diff
        .get_atom(id_a)
        .unwrap()
        .bonds[0]
        .bond_order();
    change_bond_order(&mut designer, &bond_ref, cycle_bond_order(current));
    assert_eq!(
        get_atom_edit_data(&designer)
            .diff
            .get_atom(id_a)
            .unwrap()
            .bonds[0]
            .bond_order(),
        BOND_SINGLE
    );
}

#[test]
fn test_specialized_order_enters_cycle_at_single() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::change_bond_order;

    let mut designer = setup_atom_edit_diff_view();

    let (id_a, id_b) = {
        let data = get_atom_edit_data_mut(&mut designer);
        let id_a = data.diff.add_atom(6, glam::f64::DVec3::new(0.0, 0.0, 0.0));
        let id_b = data.diff.add_atom(6, glam::f64::DVec3::new(1.5, 0.0, 0.0));
        data.add_bond_in_diff(id_a, id_b, BOND_AROMATIC); // Start with specialized order
        (id_a, id_b)
    };

    let bond_ref = BondReference {
        atom_id1: id_a,
        atom_id2: id_b,
    };

    // Cycling from aromatic should go to single
    let current = get_atom_edit_data(&designer)
        .diff
        .get_atom(id_a)
        .unwrap()
        .bonds[0]
        .bond_order();
    assert_eq!(current, BOND_AROMATIC);
    change_bond_order(&mut designer, &bond_ref, cycle_bond_order(current));
    assert_eq!(
        get_atom_edit_data(&designer)
            .diff
            .get_atom(id_a)
            .unwrap()
            .bonds[0]
            .bond_order(),
        BOND_SINGLE
    );
}

#[test]
fn test_all_specialized_orders_cycle_to_single() {
    for specialized_order in [BOND_QUADRUPLE, BOND_AROMATIC, BOND_DATIVE, BOND_METALLIC] {
        assert_eq!(
            cycle_bond_order(specialized_order),
            BOND_SINGLE,
            "Specialized order {specialized_order} should cycle to BOND_SINGLE"
        );
    }
}

// =============================================================================
// Regression: bond order change re-selects bond using fresh apply_diff provenance
//
// Before the fix, changing a bond's order between base-passthrough atoms would
// save/restore result-space bond references. After re-evaluation, those IDs
// were stale (because promoted atoms get different result IDs), causing a
// different bond to appear selected.
//
// The fix: after writing the bond, re-run apply_diff with the cached input to
// get the new provenance, then set selected_bonds with the correct result IDs.
// =============================================================================

#[test]
fn test_bond_order_change_resolves_selection_via_fresh_apply_diff() {
    // Simulate what the operations do: promote base atoms, write bond,
    // then resolve the bond selection using a fresh apply_diff.
    let mut base = AtomicStructure::new();
    let base_a = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let base_b = base.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    base.add_bond_checked(base_a, base_b, BOND_SINGLE);

    let mut data = AtomEditData::new();

    // Promote both endpoints to diff (identity entries) and write double bond
    let diff_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let diff_b = data.diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    data.add_bond_in_diff(diff_a, diff_b, BOND_DOUBLE);

    // Bond selection is empty after add_bond_in_diff
    assert!(data.selection.selected_bonds.is_empty());

    // Resolve via fresh apply_diff (same as the fix does)
    let fresh = apply_diff(&base, &data.diff, data.tolerance);
    let result_a = *fresh.provenance.diff_to_result.get(&diff_a).unwrap();
    let result_b = *fresh.provenance.diff_to_result.get(&diff_b).unwrap();
    data.selection.selected_bonds.insert(BondReference {
        atom_id1: result_a,
        atom_id2: result_b,
    });

    // Verify the selected bond points to the correct atoms
    assert_eq!(data.selection.selected_bonds.len(), 1);
    let selected = data.selection.selected_bonds.iter().next().unwrap();
    let atom_a = fresh.result.get_atom(selected.atom_id1).unwrap();
    let atom_b = fresh.result.get_atom(selected.atom_id2).unwrap();
    assert!((atom_a.position - DVec3::new(0.0, 0.0, 0.0)).length() < 1e-10);
    assert!((atom_b.position - DVec3::new(1.5, 0.0, 0.0)).length() < 1e-10);

    // Verify the bond is double
    let bond = atom_a
        .bonds
        .iter()
        .find(|b| b.other_atom_id() == selected.atom_id2)
        .expect("Bond should exist");
    assert_eq!(bond.bond_order(), BOND_DOUBLE);
}
