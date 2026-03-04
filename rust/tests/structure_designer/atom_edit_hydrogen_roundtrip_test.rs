/// Regression test for hydrogen depassivation → passivation in the same atom_edit node.
///
/// Bug: `add_hydrogen_atom_edit` stored base parent info keyed by result_id but
/// looked it up by base_id. When the base structure has gaps in atom IDs (from
/// deleted atoms during lattice fill), base_id != result_id for atoms after the
/// gap. This caused hydrogens to be bonded to the wrong carbon atoms.
///
/// The bug only manifested when both operations happened in the same atom_edit
/// node because separate nodes produce fresh structures with contiguous IDs.
use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    add_hydrogen_atom_edit, remove_hydrogen_atom_edit,
};
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Helpers
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

/// Trigger a full refresh on the designer to populate the scene (eval caches, etc.)
fn do_full_refresh(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

/// Get the atomic structure output for the selected node from the last scene.
fn get_selected_atomic_structure(designer: &StructureDesigner) -> &AtomicStructure {
    designer
        .get_atomic_structure_from_selected_node()
        .expect("No atomic structure from selected node")
}

/// Build a base structure with intentional ID gaps (simulating lone atom removal
/// during lattice fill) followed by hydrogen atoms.
///
/// Layout: a chain of 3 sp3 carbons, with a gap before C2 and C3.
///
///   C1(id=1) ------ C2(id=3) ------ C3(id=4)
///        (id=2 deleted = gap)
///
/// Each carbon gets hydrogen atoms to fill remaining valence:
///   C1: 3 H (one bond to C2)
///   C2: 2 H (two bonds to C1 and C3)
///   C3: 3 H (one bond to C2)
///
/// This creates the condition where after H removal:
///   result_id=1 → BasePassthrough(1) [C1]   (match)
///   result_id=2 → BasePassthrough(3) [C2]   (MISMATCH: base_id=3 != result_id=2)
///   result_id=3 → BasePassthrough(4) [C3]   (MISMATCH: base_id=4 != result_id=3)
///
/// The buggy code stored parent info keyed by result_id, so looking up
/// base_id=3 found the entry for result_id=3 (which is C3, not C2).
fn build_base_with_gaps() -> AtomicStructure {
    let mut s = AtomicStructure::new();

    // C1 at origin
    let c1 = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id=1

    // Dummy atom that will be deleted to create a gap
    let dummy = s.add_atom(6, DVec3::new(99.0, 99.0, 99.0)); // id=2

    // C2 at ~1.54 A along X
    let c2 = s.add_atom(6, DVec3::new(1.54, 0.0, 0.0)); // id=3

    // C3 at ~3.08 A along X
    let c3 = s.add_atom(6, DVec3::new(3.08, 0.0, 0.0)); // id=4

    // Create the C-C backbone
    s.add_bond(c1, c2, BOND_SINGLE);
    s.add_bond(c2, c3, BOND_SINGLE);

    // Delete dummy to create gap at id=2
    s.delete_atom(dummy);

    // Add H atoms for C1 (3 H, since it has 1 C-C bond → needs 3 more)
    let h_positions_c1 = [
        DVec3::new(-0.51, 0.89, 0.0),
        DVec3::new(-0.51, -0.45, 0.77),
        DVec3::new(-0.51, -0.45, -0.77),
    ];
    for pos in &h_positions_c1 {
        let h = s.add_atom(1, *pos);
        s.add_bond(c1, h, BOND_SINGLE);
    }

    // Add H atoms for C2 (2 H, since it has 2 C-C bonds → needs 2 more)
    let h_positions_c2 = [
        DVec3::new(1.54, 0.89, 0.51),
        DVec3::new(1.54, -0.89, 0.51),
    ];
    for pos in &h_positions_c2 {
        let h = s.add_atom(1, *pos);
        s.add_bond(c2, h, BOND_SINGLE);
    }

    // Add H atoms for C3 (3 H, since it has 1 C-C bond → needs 3 more)
    let h_positions_c3 = [
        DVec3::new(3.59, 0.89, 0.0),
        DVec3::new(3.59, -0.45, 0.77),
        DVec3::new(3.59, -0.45, -0.77),
    ];
    for pos in &h_positions_c3 {
        let h = s.add_atom(1, *pos);
        s.add_bond(c3, h, BOND_SINGLE);
    }

    s
}

// ============================================================================
// Tests
// ============================================================================

/// Regression test: remove all H then re-add H in the same atom_edit node.
///
/// With the bug, hydrogens after the ID gap would be bonded to wrong carbons
/// (shifted by the number of gaps) because add_hydrogen_atom_edit's HashMap
/// was keyed by result_id but looked up by base_id.
#[test]
fn test_depassivation_then_passivation_same_atom_edit_bonds_correct_parents() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    // Build base structure with ID gaps
    let base = build_base_with_gaps();

    // Verify the base has the expected gaps
    assert!(base.get_atom(2).is_none(), "Atom id=2 should be deleted (gap)");
    assert!(base.get_atom(1).is_some(), "C1 at id=1");
    assert!(base.get_atom(3).is_some(), "C2 at id=3");
    assert!(base.get_atom(4).is_some(), "C3 at id=4");

    // Record carbon positions from the base
    let c1_pos = base.get_atom(1).unwrap().position;
    let c2_pos = base.get_atom(3).unwrap().position;
    let c3_pos = base.get_atom(4).unwrap().position;

    // Set up node network: value → atom_edit
    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, base);
    let atom_edit_id = designer.add_node("atom_edit", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, atom_edit_id, 0);
    designer.select_node(atom_edit_id);

    // Initial refresh to populate eval cache
    do_full_refresh(&mut designer);

    // Verify initial state: 3 C + 8 H = 11 atoms
    {
        let result = get_selected_atomic_structure(&designer);
        let total = result.atom_ids().count();
        assert_eq!(total, 11, "Initial: expected 11 atoms (3C + 8H), got {}", total);
    }

    // Step 1: Remove all hydrogen atoms
    let remove_msg = remove_hydrogen_atom_edit(&mut designer, false)
        .expect("remove_hydrogen_atom_edit failed");
    assert!(
        remove_msg.contains("8"),
        "Expected to remove 8 hydrogens, got: {}",
        remove_msg
    );

    // Refresh to re-evaluate with the delete markers
    do_full_refresh(&mut designer);

    // Verify: only 3 carbon atoms remain
    {
        let result = get_selected_atomic_structure(&designer);
        let total = result.atom_ids().count();
        assert_eq!(total, 3, "After removal: expected 3 atoms (3C), got {}", total);
    }

    // Step 2: Re-add hydrogen atoms
    let add_msg =
        add_hydrogen_atom_edit(&mut designer, false).expect("add_hydrogen_atom_edit failed");
    assert!(
        add_msg.contains("8"),
        "Expected to add 8 hydrogens, got: {}",
        add_msg
    );

    // Final refresh to evaluate with new H atoms
    do_full_refresh(&mut designer);

    // Step 3: Verify every hydrogen is bonded to the correct carbon
    let result = get_selected_atomic_structure(&designer);
    let total = result.atom_ids().count();
    assert_eq!(
        total, 11,
        "Final: expected 11 atoms (3C + 8H), got {}",
        total
    );

    // For each hydrogen atom, check that its bonded carbon is the nearest carbon
    for &h_id in result.atom_ids().copied().collect::<Vec<_>>().iter() {
        let h_atom = result.get_atom(h_id).unwrap();
        if h_atom.atomic_number != 1 {
            continue;
        }

        // Get the carbon this H is bonded to
        let bonded_c_id = h_atom
            .bonds
            .iter()
            .filter(|b| !b.is_delete_marker())
            .find_map(|b| {
                let neighbor = result.get_atom(b.other_atom_id())?;
                if neighbor.atomic_number == 6 {
                    Some(b.other_atom_id())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("H atom {} has no bonded carbon", h_id));

        let bonded_c_pos = result.get_atom(bonded_c_id).unwrap().position;
        let h_pos = h_atom.position;

        // Find the nearest carbon to this hydrogen
        let c_positions = [c1_pos, c2_pos, c3_pos];
        let nearest_c_pos = c_positions
            .iter()
            .min_by(|a, b| {
                h_pos
                    .distance_squared(**a)
                    .partial_cmp(&h_pos.distance_squared(**b))
                    .unwrap()
            })
            .unwrap();

        // The bonded carbon should be the nearest carbon
        let dist_to_bonded = h_pos.distance(bonded_c_pos);
        let dist_to_nearest = h_pos.distance(*nearest_c_pos);

        assert!(
            (dist_to_bonded - dist_to_nearest).abs() < 0.01,
            "H atom at {:?} is bonded to carbon at {:?} (dist={:.3}), \
             but nearest carbon is at {:?} (dist={:.3}). \
             The hydrogen is bonded to the wrong carbon!",
            h_pos,
            bonded_c_pos,
            dist_to_bonded,
            nearest_c_pos,
            dist_to_nearest,
        );
    }
}
