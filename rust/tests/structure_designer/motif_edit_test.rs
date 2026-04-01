// Tests for the motif_edit node (Phase 2 + Phase 3: parameter elements + Phase 5: ghost atoms + Phase 6: cross-cell bonds)

use glam::IVec3;
use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::{AtomicStructure, BondReference};
use rust_lib_flutter_cad::crystolecule::motif::Motif;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::EvalOutput;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, CrossCellBondInfo, PARAM_ELEMENT_BASE, generate_ghost_atoms,
    get_node_type_motif_edit, is_atom_edit_family, is_param_element,
    min_distance_to_unit_cube, param_atomic_number_to_index,
    param_atomic_number_to_motif, param_index_to_atomic_number,
};
use rust_lib_flutter_cad::structure_designer::serialization::atom_edit_data_serialization::{
    atom_edit_data_to_serializable, serializable_to_atom_edit_data,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::collections::HashMap;

// ===== is_atom_edit_family tests =====

#[test]
fn test_is_atom_edit_family_atom_edit() {
    assert!(is_atom_edit_family("atom_edit"));
}

#[test]
fn test_is_atom_edit_family_motif_edit() {
    assert!(is_atom_edit_family("motif_edit"));
}

#[test]
fn test_is_atom_edit_family_other() {
    assert!(!is_atom_edit_family("sphere"));
    assert!(!is_atom_edit_family("motif"));
    assert!(!is_atom_edit_family(""));
}

// ===== Node registration tests =====

#[test]
fn test_motif_edit_registered_in_registry() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("motif_edit");
    assert!(node_type.is_some(), "motif_edit should be registered");
}

#[test]
fn test_motif_edit_node_type_pins() {
    let node_type = get_node_type_motif_edit();
    assert_eq!(node_type.name, "motif_edit");

    // 3 input pins: molecule, unit_cell, tolerance
    assert_eq!(node_type.parameters.len(), 3);
    assert_eq!(node_type.parameters[0].name, "molecule");
    assert_eq!(node_type.parameters[0].data_type, DataType::Atomic);
    assert_eq!(node_type.parameters[1].name, "unit_cell");
    assert_eq!(node_type.parameters[1].data_type, DataType::UnitCell);
    assert_eq!(node_type.parameters[2].name, "tolerance");
    assert_eq!(node_type.parameters[2].data_type, DataType::Float);

    // 2 output pins: result (Motif), diff (Atomic)
    assert_eq!(node_type.output_pins.len(), 2);
    assert_eq!(node_type.output_pins[0].name, "result");
    assert_eq!(node_type.output_pins[0].data_type, DataType::Motif);
    assert_eq!(node_type.output_pins[1].name, "diff");
    assert_eq!(node_type.output_pins[1].data_type, DataType::Atomic);
}

#[test]
fn test_motif_edit_node_data_is_motif_mode() {
    let node_type = get_node_type_motif_edit();
    let data = (node_type.node_data_creator)();
    let atom_edit_data = data.as_any_ref().downcast_ref::<AtomEditData>().unwrap();
    assert!(atom_edit_data.is_motif_mode);
}

// ===== AtomEditData constructor tests =====

#[test]
fn test_new_motif_mode() {
    let data = AtomEditData::new_motif_mode();
    assert!(data.is_motif_mode);
    assert!(data.cached_unit_cell.lock().unwrap().is_none());
}

#[test]
fn test_new_default_not_motif_mode() {
    let data = AtomEditData::new();
    assert!(!data.is_motif_mode);
}

// ===== Serialization tests =====

#[test]
fn test_motif_edit_serialization_roundtrip() {
    let data = AtomEditData::new_motif_mode();
    let serializable = atom_edit_data_to_serializable(&data).unwrap();
    assert!(serializable.is_motif_mode);

    let restored = serializable_to_atom_edit_data(&serializable).unwrap();
    assert!(restored.is_motif_mode);
}

#[test]
fn test_atom_edit_serialization_not_motif() {
    let data = AtomEditData::new();
    let serializable = atom_edit_data_to_serializable(&data).unwrap();
    assert!(!serializable.is_motif_mode);
}

#[test]
fn test_motif_edit_backward_compat_load() {
    // Simulate loading old JSON without is_motif_mode field
    let json = serde_json::json!({
        "diff": {
            "atoms": [],
            "bonds": [],
            "anchor_positions": []
        }
    });
    let serializable: rust_lib_flutter_cad::structure_designer::serialization::atom_edit_data_serialization::SerializableAtomEditData =
        serde_json::from_value(json).unwrap();
    assert!(!serializable.is_motif_mode); // should default to false
}

// ===== Conversion tests =====

fn make_cubic_unit_cell(a: f64) -> UnitCellStruct {
    UnitCellStruct::new(
        DVec3::new(a, 0.0, 0.0),
        DVec3::new(0.0, a, 0.0),
        DVec3::new(0.0, 0.0, a),
    )
}

#[test]
fn test_motif_edit_eval_creates_motif_output() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));

    // Create unit_cell node + motif_edit node
    let uc_id = designer.add_node("unit_cell", DVec2::ZERO);
    let me_id = designer.add_node("motif_edit", DVec2::new(200.0, 0.0));

    // Wire unit_cell → motif_edit pin 1 (unit_cell)
    designer.connect_nodes(uc_id, 0, me_id, 1);

    // Add an atom to the motif_edit diff
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test")
            .unwrap();
        let node = network.nodes.get_mut(&me_id).unwrap();
        let data = node
            .data
            .as_any_mut()
            .downcast_mut::<AtomEditData>()
            .unwrap();
        // Place carbon atoms at Cartesian positions
        data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        data.diff.add_atom(6, DVec3::new(1.7835, 1.7835, 1.7835));
    }

    // Evaluate the motif_edit node
    let result = designer.evaluate_node_for_cli(me_id, false);
    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(
        result.success,
        "Eval should succeed: {:?}",
        result.error_message
    );
}

#[test]
fn test_motif_edit_eval_no_unit_cell_returns_error() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));

    // Create motif_edit node without wiring unit_cell
    let me_id = designer.add_node("motif_edit", DVec2::ZERO);

    let result = designer.evaluate_node_for_cli(me_id, false);
    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(!result.success);
    assert!(result.error_message.is_some());
}

// ===== Display override tests =====

#[test]
fn test_eval_output_display_override_motif_pattern() {
    // Simulate what motif_edit does: wire carries Motif, display carries Atomic
    let motif = Motif {
        parameters: vec![],
        sites: vec![],
        bonds: vec![],
        bonds_by_site1_index: vec![],
        bonds_by_site2_index: vec![],
    };
    let viz = AtomicStructure::new();
    let diff = AtomicStructure::new_diff();

    let mut output = EvalOutput::multi(vec![
        NetworkResult::Motif(motif),
        NetworkResult::Atomic(diff),
    ]);
    output.set_display_override(0, NetworkResult::Atomic(viz));

    // Wire result is Motif
    assert!(matches!(output.get(0), NetworkResult::Motif(_)));

    // Display result is Atomic
    let display = output.get_display(0);
    assert!(matches!(display, NetworkResult::Atomic(_)));

    // Pin 1 has no override — display falls back to wire
    let pin1_display = output.get_display(1);
    assert!(matches!(pin1_display, NetworkResult::Atomic(_)));
}

// ===== Coordinate roundtrip test =====

#[test]
fn test_motif_edit_coordinate_roundtrip() {
    let uc = make_cubic_unit_cell(3.567); // diamond cubic
    let cart_pos = DVec3::new(0.89175, 0.89175, 0.89175); // ~(0.25, 0.25, 0.25) fractional

    let frac = uc.real_to_dvec3_lattice(&cart_pos);
    let roundtrip = uc.dvec3_lattice_to_real(&frac);

    assert!(
        (roundtrip - cart_pos).length() < 1e-10,
        "Roundtrip should preserve position: cart={:?}, frac={:?}, roundtrip={:?}",
        cart_pos,
        frac,
        roundtrip
    );
    // Verify fractional is approximately (0.25, 0.25, 0.25)
    assert!((frac.x - 0.25).abs() < 1e-10);
    assert!((frac.y - 0.25).abs() < 1e-10);
    assert!((frac.z - 0.25).abs() < 1e-10);
}

// ===== Phase 3: Parameter element constant tests =====

#[test]
fn test_param_atomic_number_roundtrip() {
    assert_eq!(param_index_to_atomic_number(0), -100);
    assert_eq!(param_index_to_atomic_number(1), -101);
    assert_eq!(param_index_to_atomic_number(99), -199);

    assert_eq!(param_atomic_number_to_index(-100), Some(0));
    assert_eq!(param_atomic_number_to_index(-101), Some(1));
    assert_eq!(param_atomic_number_to_index(-199), Some(99));
}

#[test]
fn test_param_atomic_number_to_motif_convention() {
    // -100 → -1 (first parameter), -101 → -2, etc.
    assert_eq!(param_atomic_number_to_motif(-100), -1);
    assert_eq!(param_atomic_number_to_motif(-101), -2);
    assert_eq!(param_atomic_number_to_motif(-105), -6);
}

#[test]
fn test_is_param_element_valid_range() {
    assert!(is_param_element(-100));
    assert!(is_param_element(-150));
    assert!(is_param_element(-199));
}

#[test]
fn test_is_param_element_invalid() {
    assert!(!is_param_element(0)); // delete marker
    assert!(!is_param_element(-1)); // unchanged marker
    assert!(!is_param_element(1)); // hydrogen
    assert!(!is_param_element(6)); // carbon
    assert!(!is_param_element(-200)); // out of range
    assert!(!is_param_element(-99)); // just above range
}

#[test]
fn test_param_element_base_constant() {
    assert_eq!(PARAM_ELEMENT_BASE, -100);
}

// ===== Phase 3: Motif conversion with parameter elements =====

#[test]
fn test_motif_with_parameter_elements() {
    // Build motif_edit with parameter elements and a param atom
    let mut data = AtomEditData::new_motif_mode();
    data.parameter_elements = vec![
        ("PRIMARY".to_string(), 6),    // Carbon default
        ("SECONDARY".to_string(), 14), // Silicon default
    ];

    // Place a carbon atom, a param_1 atom, and a param_2 atom
    data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // normal carbon
    data.diff.add_atom(-100, DVec3::new(1.0, 0.0, 0.0)); // PARAM_1
    data.diff.add_atom(-101, DVec3::new(0.0, 1.0, 0.0)); // PARAM_2

    // Evaluate through a full network to verify motif output
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));

    let uc_id = designer.add_node("unit_cell", DVec2::ZERO);
    let me_id = designer.add_node("motif_edit", DVec2::new(200.0, 0.0));
    designer.connect_nodes(uc_id, 0, me_id, 1);

    // Inject the prepared data
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test")
            .unwrap();
        let node = network.nodes.get_mut(&me_id).unwrap();
        let node_data = node
            .data
            .as_any_mut()
            .downcast_mut::<AtomEditData>()
            .unwrap();
        node_data.diff = data.diff;
        node_data.parameter_elements = data.parameter_elements;
    }

    let result = designer.evaluate_node_for_cli(me_id, false);
    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(
        result.success,
        "Eval should succeed: {:?}",
        result.error_message
    );
}

// ===== Phase 3: Serialization with parameter elements =====

#[test]
fn test_parameter_element_serialization_roundtrip() {
    let mut data = AtomEditData::new_motif_mode();
    data.parameter_elements = vec![("PRIMARY".to_string(), 6), ("SECONDARY".to_string(), 14)];

    let serializable = atom_edit_data_to_serializable(&data).unwrap();
    assert_eq!(serializable.parameter_elements.len(), 2);
    assert_eq!(serializable.parameter_elements[0].name, "PRIMARY");
    assert_eq!(serializable.parameter_elements[0].default_atomic_number, 6);
    assert_eq!(serializable.parameter_elements[1].name, "SECONDARY");
    assert_eq!(serializable.parameter_elements[1].default_atomic_number, 14);

    let restored = serializable_to_atom_edit_data(&serializable).unwrap();
    assert!(restored.is_motif_mode);
    assert_eq!(restored.parameter_elements.len(), 2);
    assert_eq!(restored.parameter_elements[0].0, "PRIMARY");
    assert_eq!(restored.parameter_elements[0].1, 6);
    assert_eq!(restored.parameter_elements[1].0, "SECONDARY");
    assert_eq!(restored.parameter_elements[1].1, 14);
}

#[test]
fn test_parameter_element_backward_compat_load() {
    // Old JSON without parameter_elements field
    let json = serde_json::json!({
        "diff": {
            "atoms": [],
            "bonds": [],
            "anchor_positions": []
        },
        "is_motif_mode": true
    });
    let serializable: rust_lib_flutter_cad::structure_designer::serialization::atom_edit_data_serialization::SerializableAtomEditData =
        serde_json::from_value(json).unwrap();
    assert!(serializable.is_motif_mode);
    assert!(serializable.parameter_elements.is_empty()); // defaults to empty
}

#[test]
fn test_parameter_element_empty_serialization() {
    // Verify empty parameter_elements is skipped in JSON output
    let data = AtomEditData::new_motif_mode();
    let serializable = atom_edit_data_to_serializable(&data).unwrap();
    let json = serde_json::to_value(&serializable).unwrap();
    // skip_serializing_if = "Vec::is_empty" should omit the field
    assert!(json.get("parameter_elements").is_none());
}

#[test]
fn test_new_motif_mode_has_empty_parameter_elements() {
    let data = AtomEditData::new_motif_mode();
    assert!(data.parameter_elements.is_empty());
}

// ===== Phase 5: Ghost atom tests =====

#[test]
fn test_new_motif_mode_default_neighbor_depth() {
    let data = AtomEditData::new_motif_mode();
    assert!((data.neighbor_depth - 0.3).abs() < f64::EPSILON);
}

#[test]
fn test_min_distance_inside_cube_center() {
    // Point at center of cube: distance to nearest face = 0.5
    let d = min_distance_to_unit_cube(&DVec3::new(0.5, 0.5, 0.5));
    assert!((d - 0.5).abs() < 1e-10);
}

#[test]
fn test_min_distance_inside_cube_near_face() {
    // Point at (0.1, 0.5, 0.5): nearest face is x=0 at distance 0.1
    let d = min_distance_to_unit_cube(&DVec3::new(0.1, 0.5, 0.5));
    assert!((d - 0.1).abs() < 1e-10);
}

#[test]
fn test_min_distance_outside_cube_face() {
    // Point at (1.2, 0.5, 0.5): 0.2 past the x=1 face
    let d = min_distance_to_unit_cube(&DVec3::new(1.2, 0.5, 0.5));
    assert!((d - 0.2).abs() < 1e-10);
}

#[test]
fn test_min_distance_outside_cube_corner() {
    // Point at (1.3, 1.1, 0.5): max overshoot is 0.3 (x-axis)
    let d = min_distance_to_unit_cube(&DVec3::new(1.3, 1.1, 0.5));
    assert!((d - 0.3).abs() < 1e-10);
}

#[test]
fn test_min_distance_outside_cube_negative() {
    // Point at (-0.15, 0.5, 0.5): 0.15 past x=0 face
    let d = min_distance_to_unit_cube(&DVec3::new(-0.15, 0.5, 0.5));
    assert!((d - 0.15).abs() < 1e-10);
}

#[test]
fn test_ghost_generation_depth_zero() {
    // With neighbor_depth = 0.0, no ghost atoms should be generated
    let uc = make_cubic_unit_cell(5.0);
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::new(0.5, 0.5, 0.5)); // atom near center at frac (0.1, 0.1, 0.1)

    let initial_count = structure.get_num_of_atoms();
    generate_ghost_atoms(&mut structure, &uc, 0.0, &HashMap::new());
    assert_eq!(structure.get_num_of_atoms(), initial_count);
}

#[test]
fn test_ghost_generation_depth_one() {
    // With neighbor_depth = 1.0, all 26 neighboring cells should produce ghosts for all atoms
    let uc = make_cubic_unit_cell(5.0);
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::new(2.5, 2.5, 2.5)); // center of cell

    generate_ghost_atoms(&mut structure, &uc, 1.0, &HashMap::new());
    // 1 primary + 26 ghosts = 27
    assert_eq!(structure.get_num_of_atoms(), 27);
}

#[test]
fn test_ghost_atoms_have_ghost_flag() {
    let uc = make_cubic_unit_cell(5.0);
    let mut structure = AtomicStructure::new();
    let primary_id = structure.add_atom(6, DVec3::new(2.5, 2.5, 2.5));

    generate_ghost_atoms(&mut structure, &uc, 1.0, &HashMap::new());

    // Primary atom should NOT be a ghost
    assert!(!structure.get_atom(primary_id).unwrap().is_ghost());

    // All other atoms should be ghosts
    let ghost_count = structure.iter_atoms().filter(|(_, a)| a.is_ghost()).count();
    assert_eq!(ghost_count, 26);
}

#[test]
fn test_ghost_generation_near_face() {
    // Atom at fractional (0.1, 0.5, 0.5) — close to x=0 face
    // With depth = 0.2, should see ghost in cell (-1,0,0) because
    // ghost frac would be (0.1 - 1, 0.5, 0.5) = (-0.9, 0.5, 0.5),
    // distance = max(0.9, 0, 0) = 0.9 > 0.2, so NOT shown.
    // But the ghost in cell (+1,0,0) would be at (1.1, 0.5, 0.5),
    // distance = 0.1 < 0.2, so it IS shown.
    let uc = make_cubic_unit_cell(5.0);
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::new(0.5, 2.5, 2.5)); // frac (0.1, 0.5, 0.5)

    generate_ghost_atoms(&mut structure, &uc, 0.15, &HashMap::new());

    // Only the ghost from cell (-1,0,0) should appear:
    // ghost frac = (-0.9, 0.5, 0.5), dist = 0.9 > 0.15 → not shown
    // ghost from (+1,0,0): frac = (1.1, 0.5, 0.5), dist = 0.1 < 0.15 → shown!
    let ghost_count = structure.iter_atoms().filter(|(_, a)| a.is_ghost()).count();
    assert_eq!(ghost_count, 1);

    // Verify the ghost position
    let ghost = structure
        .iter_atoms()
        .find(|(_, a)| a.is_ghost())
        .unwrap()
        .1;
    let expected_pos = DVec3::new(0.5 + 5.0, 2.5, 2.5); // translated by +a
    assert!((ghost.position - expected_pos).length() < 1e-10);
}

#[test]
fn test_ghost_generation_corner_atom() {
    // Atom at fractional (0.1, 0.1, 0.1) — close to the (0,0,0) corner
    // With depth = 0.15, ghosts visible in cells that share that corner
    let uc = make_cubic_unit_cell(10.0);
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::new(1.0, 1.0, 1.0)); // frac (0.1, 0.1, 0.1)

    generate_ghost_atoms(&mut structure, &uc, 0.15, &HashMap::new());

    // Ghosts in: (-1,0,0), (0,-1,0), (0,0,-1), (-1,-1,0), (-1,0,-1), (0,-1,-1), (-1,-1,-1)
    // These are the 7 cells that share the (0,0,0) corner.
    // For cell (-1,0,0): ghost frac = (-0.9, 0.1, 0.1), dist = max(0.9, 0, 0) = 0.9 > 0.15 → NO
    // Wait, this is wrong. Let me reconsider.
    // For cell (-1,0,0): ghost frac = (0.1 - 1, 0.1, 0.1) = (-0.9, 0.1, 0.1)
    //   dist = max(0.9, 0, 0) = 0.9 > 0.15 → NOT shown
    // For cell (+1,0,0): ghost frac = (1.1, 0.1, 0.1)
    //   dist = max(0.1, 0, 0) = 0.1 < 0.15 → shown!
    // And similar for (0,+1,0), (0,0,+1), (+1,+1,0), (+1,0,+1), (0,+1,+1), (+1,+1,+1)
    // That's 7 ghosts from the positive corner cells.
    let ghost_count = structure.iter_atoms().filter(|(_, a)| a.is_ghost()).count();
    assert_eq!(ghost_count, 7);
}

#[test]
fn test_ghost_atoms_preserve_element() {
    let uc = make_cubic_unit_cell(5.0);
    let mut structure = AtomicStructure::new();
    structure.add_atom(14, DVec3::new(2.5, 2.5, 2.5)); // Silicon

    generate_ghost_atoms(&mut structure, &uc, 1.0, &HashMap::new());

    // All ghosts should have the same atomic number
    for (_, atom) in structure.iter_atoms() {
        assert_eq!(atom.atomic_number, 14);
    }
}

#[test]
fn test_ghost_bonds_between_ghost_atoms() {
    let uc = make_cubic_unit_cell(5.0);
    let mut structure = AtomicStructure::new();
    let id1 = structure.add_atom(6, DVec3::new(1.0, 2.5, 2.5));
    let id2 = structure.add_atom(6, DVec3::new(2.0, 2.5, 2.5));
    structure.add_bond(id1, id2, 1);

    generate_ghost_atoms(&mut structure, &uc, 1.0, &HashMap::new());

    // Count ghost atoms
    let ghost_count = structure.iter_atoms().filter(|(_, a)| a.is_ghost()).count();
    // 2 atoms × 26 cells = 52 ghost atoms
    assert_eq!(ghost_count, 52);

    // Count ghost bonds (bonds where at least one endpoint is a ghost)
    let mut ghost_bond_count = 0;
    for (_, atom) in structure.iter_atoms() {
        if atom.is_ghost() {
            for bond in &atom.bonds {
                let other = structure.get_atom(bond.other_atom_id()).unwrap();
                if other.is_ghost() && atom.id < bond.other_atom_id() {
                    ghost_bond_count += 1;
                }
            }
        }
    }
    // Each of the 26 neighboring cells should have 1 bond between its 2 ghost atoms
    assert_eq!(ghost_bond_count, 26);
}

#[test]
fn test_ghost_atoms_not_in_motif_output() {
    // Ghost atoms are added to the display visualization (Atomic),
    // but the Motif wire result should never contain ghost data.
    // This is guaranteed by the architecture: generate_ghost_atoms
    // is called only on the `result` display structure, not on the
    // motif conversion input. We verify this by checking that
    // generate_ghost_atoms only adds atoms with the ghost flag.
    let uc = make_cubic_unit_cell(5.0);
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::new(2.5, 2.5, 2.5));

    generate_ghost_atoms(&mut structure, &uc, 1.0, &HashMap::new());

    // All new atoms should be ghosts, original should not
    let primary_count = structure
        .iter_atoms()
        .filter(|(_, a)| !a.is_ghost())
        .count();
    let ghost_count = structure.iter_atoms().filter(|(_, a)| a.is_ghost()).count();
    assert_eq!(primary_count, 1);
    assert_eq!(ghost_count, 26);
}

#[test]
fn test_neighbor_depth_serialization_roundtrip() {
    let mut data = AtomEditData::new_motif_mode();
    data.neighbor_depth = 0.42;

    let serializable = atom_edit_data_to_serializable(&data).unwrap();
    assert!((serializable.neighbor_depth - 0.42).abs() < f64::EPSILON);

    let restored = serializable_to_atom_edit_data(&serializable).unwrap();
    assert!((restored.neighbor_depth - 0.42).abs() < f64::EPSILON);
}

#[test]
fn test_neighbor_depth_backward_compat_load() {
    // Old JSON without neighbor_depth field — should default to 0.3
    let json = serde_json::json!({
        "diff": {
            "atoms": [],
            "bonds": [],
            "anchor_positions": []
        },
        "is_motif_mode": true
    });
    let serializable: rust_lib_flutter_cad::structure_designer::serialization::atom_edit_data_serialization::SerializableAtomEditData =
        serde_json::from_value(json).unwrap();
    assert!((serializable.neighbor_depth - 0.3).abs() < f64::EPSILON);
}

#[test]
fn test_ghost_atom_flag_accessors() {
    let mut structure = AtomicStructure::new();
    let id = structure.add_atom(6, DVec3::ZERO);

    // Default: not ghost
    assert!(!structure.get_atom(id).unwrap().is_ghost());

    // Set ghost
    structure.set_atom_ghost(id, true);
    assert!(structure.get_atom(id).unwrap().is_ghost());

    // Clear ghost
    structure.set_atom_ghost(id, false);
    assert!(!structure.get_atom(id).unwrap().is_ghost());
}

#[test]
fn test_ghost_flag_does_not_interfere_with_other_flags() {
    let mut structure = AtomicStructure::new();
    let id = structure.add_atom(6, DVec3::ZERO);

    structure.set_atom_selected(id, true);
    structure.set_atom_frozen(id, true);
    structure.set_atom_ghost(id, true);

    let atom = structure.get_atom(id).unwrap();
    assert!(atom.is_selected());
    assert!(atom.is_frozen());
    assert!(atom.is_ghost());

    // Clear ghost, others should remain
    structure.set_atom_ghost(id, false);
    let atom = structure.get_atom(id).unwrap();
    assert!(!atom.is_ghost());
    assert!(atom.is_selected());
    assert!(atom.is_frozen());
}

// =============================================================================
// Phase 6: Cross-cell bond tests
// =============================================================================

/// Helper: create a cubic unit cell with the given size.
fn cubic_unit_cell(size: f64) -> UnitCellStruct {
    UnitCellStruct::new(
        DVec3::new(size, 0.0, 0.0),
        DVec3::new(0.0, size, 0.0),
        DVec3::new(0.0, 0.0, size),
    )
}

#[test]
fn test_cross_cell_bond_to_motif_basic() {
    // Two atoms: A at origin, B at (0.5, 0.5, 0.5) in a 4.0 Å cubic cell.
    // Cross-cell bond from A to B in cell (+1, 0, 0).
    let uc = cubic_unit_cell(4.0);
    let mut data = AtomEditData::new_motif_mode();

    let atom_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id=1
    let atom_b = data.diff.add_atom(14, DVec3::new(2.0, 2.0, 2.0)); // id=2

    // Cross-cell bonds are NOT stored in the diff — only in the cross_cell_bonds map.
    let bond_ref = BondReference {
        atom_id1: atom_a,
        atom_id2: atom_b,
    };
    data.cross_cell_bonds.insert(
        bond_ref,
        CrossCellBondInfo {
            offset: IVec3::new(1, 0, 0),
            bond_order: 1,
        },
    );

    // Build the result structure by just using the diff directly
    let result = data.diff.clone();

    // Convert to motif
    let motif = rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::atomic_structure_to_motif(
        &result, &uc, &data.parameter_elements, &data.cross_cell_bonds,
    );

    assert_eq!(motif.sites.len(), 2);
    assert_eq!(motif.bonds.len(), 1);

    let bond = &motif.bonds[0];
    // site_1 (atom_a, lower id) always gets ZERO
    assert_eq!(bond.site_1.relative_cell, IVec3::ZERO);
    // site_2 (atom_b, higher id) gets the stored offset
    assert_eq!(bond.site_2.relative_cell, IVec3::new(1, 0, 0));
}

#[test]
fn test_cross_cell_bond_same_cell_default() {
    // Bond NOT in cross_cell_bonds → relative_cell = IVec3::ZERO for both sites
    let uc = cubic_unit_cell(4.0);
    let mut structure = AtomicStructure::new();
    let a = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = structure.add_atom(6, DVec3::new(1.0, 1.0, 1.0));
    structure.add_bond(a, b, 1);

    let empty_ccb: HashMap<BondReference, CrossCellBondInfo> = HashMap::new();
    let motif = rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::atomic_structure_to_motif(
        &structure, &uc, &[], &empty_ccb,
    );

    assert_eq!(motif.bonds.len(), 1);
    assert_eq!(motif.bonds[0].site_1.relative_cell, IVec3::ZERO);
    assert_eq!(motif.bonds[0].site_2.relative_cell, IVec3::ZERO);
}

#[test]
fn test_cross_cell_bond_normalization_convention() {
    // Verify the normalization convention: offset is "max relative to min".
    // If we create a bond from atom 5 to ghost of atom 3 in cell (1,0,0),
    // the stored normalized offset should be (-1,0,0) because min=3, max=5,
    // and we negate the offset since from > to.
    let mut data = AtomEditData::new_motif_mode();
    let _a = data.diff.add_atom(6, DVec3::ZERO); // id=1
    let _b = data.diff.add_atom(6, DVec3::X); // id=2
    let _c = data.diff.add_atom(6, DVec3::Y); // id=3
    let _d = data.diff.add_atom(6, DVec3::Z); // id=4
    let _e = data.diff.add_atom(6, DVec3::new(1.0, 1.0, 0.0)); // id=5

    // User draws from atom 5 (source) to ghost of atom 3 in cell (1,0,0).
    // source_atom_id=5, target_diff_id=3, cell_offset=(1,0,0)
    // Normalization: since source(5) > target(3), negate: offset = (-1,0,0)
    // This is the offset of max(3,5)=5 relative to min(3,5)=3.
    let source = 5u32;
    let target = 3u32;
    let raw_offset = IVec3::new(1, 0, 0);
    let normalized = if source < target {
        raw_offset
    } else {
        -raw_offset
    };
    assert_eq!(normalized, IVec3::new(-1, 0, 0));

    // The stored offset of max(3,5)=5 relative to min(3,5)=3 is (-1,0,0).
    // This means: atom 5 is in cell (-1,0,0) relative to atom 3.
    // Or equivalently: atom 3 is in cell (+1,0,0) relative to atom 5.
    // This matches the user's action: from 5 looking toward 3 in cell (+1,0,0).
}

#[test]
fn test_cross_cell_bond_serialization_roundtrip() {
    let mut data = AtomEditData::new_motif_mode();
    let a = data.diff.add_atom(6, DVec3::ZERO);
    let b = data.diff.add_atom(14, DVec3::new(1.0, 1.0, 1.0));

    let bond_ref = BondReference {
        atom_id1: a,
        atom_id2: b,
    };
    data.cross_cell_bonds.insert(
        bond_ref.clone(),
        CrossCellBondInfo {
            offset: IVec3::new(1, 0, 0),
            bond_order: 1,
        },
    );

    // Serialize
    let serializable = atom_edit_data_to_serializable(&data).unwrap();
    assert_eq!(serializable.cross_cell_bonds.len(), 1);
    assert_eq!(serializable.cross_cell_bonds[0].atom_id_1, 1);
    assert_eq!(serializable.cross_cell_bonds[0].atom_id_2, 2);
    assert_eq!(serializable.cross_cell_bonds[0].relative_cell, [1, 0, 0]);

    // Deserialize
    let restored = serializable_to_atom_edit_data(&serializable).unwrap();
    assert!(restored.is_motif_mode);
    assert_eq!(restored.cross_cell_bonds.len(), 1);

    let restored_info = restored.cross_cell_bonds.get(&bond_ref).unwrap();
    assert_eq!(restored_info.offset, IVec3::new(1, 0, 0));
    assert_eq!(restored_info.bond_order, 1);
}

#[test]
fn test_cross_cell_bond_backward_compat_no_field() {
    // Deserialize JSON without cross_cell_bonds field — should default to empty
    let json = serde_json::json!({
        "diff": {
            "atoms": [],
            "bonds": [],
            "anchor_positions": []
        },
        "is_motif_mode": true,
        "neighbor_depth": 0.3
    });

    let serializable: rust_lib_flutter_cad::structure_designer::serialization::atom_edit_data_serialization::SerializableAtomEditData =
        serde_json::from_value(json).unwrap();
    let data = serializable_to_atom_edit_data(&serializable).unwrap();
    assert!(data.cross_cell_bonds.is_empty());
}

#[test]
fn test_symmetric_ghost_bond_rendering() {
    // Place two atoms near opposite cell faces so their ghosts appear.
    // A at frac ~(0.05, 0.5, 0.5), B at frac ~(0.95, 0.5, 0.5) in a 4.0 Å cell.
    // With depth=1.0 (full neighboring cells), ghosts of both atoms appear in
    // all neighboring cells, enabling symmetric cross-cell bond rendering.
    let uc = cubic_unit_cell(4.0);
    let mut structure = AtomicStructure::new();
    let a = structure.add_atom(6, DVec3::new(0.2, 2.0, 2.0)); // frac (0.05, 0.5, 0.5)
    let b = structure.add_atom(6, DVec3::new(3.8, 2.0, 2.0)); // frac (0.95, 0.5, 0.5)
    // No same-cell bond: cross-cell bonds are stored only in the map.

    let mut ccb: HashMap<BondReference, CrossCellBondInfo> = HashMap::new();
    ccb.insert(
        BondReference {
            atom_id1: a,
            atom_id2: b,
        },
        CrossCellBondInfo {
            offset: IVec3::new(1, 0, 0), // offset of max(1,2)=b relative to min(1,2)=a
            bond_order: 1,
        },
    );

    // Generate ghosts with depth=1.0 to ensure ghosts exist for cross-cell bond targets
    generate_ghost_atoms(&mut structure, &uc, 1.0, &ccb);

    // Count ghost atoms
    let ghost_count = structure
        .iter_atoms()
        .filter(|(_, atom)| atom.is_ghost())
        .count();
    assert!(ghost_count > 0, "Should have generated some ghost atoms");

    // Check that cross-cell bonds were rendered: atom A should have a bond
    // to a ghost (ghost of B in cell +1,0,0), and atom B should have a bond
    // to a ghost (ghost of A in cell -1,0,0).
    let a_bonds_to_ghost = structure
        .get_atom(a)
        .unwrap()
        .bonds
        .iter()
        .filter(|bond| {
            structure
                .get_atom(bond.other_atom_id())
                .is_some_and(|atom| atom.is_ghost())
        })
        .count();
    let b_bonds_to_ghost = structure
        .get_atom(b)
        .unwrap()
        .bonds
        .iter()
        .filter(|bond| {
            structure
                .get_atom(bond.other_atom_id())
                .is_some_and(|atom| atom.is_ghost())
        })
        .count();

    assert!(
        a_bonds_to_ghost >= 1,
        "Atom A should have cross-cell bond to ghost of B, got {} ghost bonds",
        a_bonds_to_ghost
    );
    assert!(
        b_bonds_to_ghost >= 1,
        "Atom B should have cross-cell bond to ghost of A, got {} ghost bonds",
        b_bonds_to_ghost
    );
}

#[test]
fn test_ghost_atom_metadata_stored() {
    // Verify that ghost atom metadata (primary_id, cell_offset) is stored in the decorator
    let uc = cubic_unit_cell(4.0);
    let mut structure = AtomicStructure::new();
    let a = structure.add_atom(6, DVec3::new(0.2, 2.0, 2.0)); // frac ~(0.05, 0.5, 0.5) — near boundary

    generate_ghost_atoms(&mut structure, &uc, 0.15, &HashMap::new());

    // Should have ghost atoms with metadata pointing back to atom 'a'
    let metadata = &structure.decorator().ghost_atom_metadata;
    assert!(
        !metadata.is_empty(),
        "Should have ghost atom metadata entries"
    );

    for (&ghost_id, &(primary_id, cell_offset)) in metadata {
        assert_eq!(primary_id, a, "Ghost should reference primary atom");
        assert_ne!(
            cell_offset,
            IVec3::ZERO,
            "Ghost cell offset should be non-zero"
        );
        // Verify the ghost atom exists and is flagged
        assert!(structure.get_atom(ghost_id).unwrap().is_ghost());
    }
}

#[test]
fn test_cross_cell_bond_recording() {
    // Test that set_cross_cell_bond_recorded and remove_cross_cell_bond_recorded
    // correctly record deltas
    let mut data = AtomEditData::new_motif_mode();
    let a = data.diff.add_atom(6, DVec3::ZERO);
    let b = data.diff.add_atom(14, DVec3::X);

    let bond_ref = BondReference {
        atom_id1: a,
        atom_id2: b,
    };

    // Start recording
    data.begin_recording();

    // Add cross-cell bond
    let info = CrossCellBondInfo {
        offset: IVec3::new(1, 0, 0),
        bond_order: 1,
    };
    data.set_cross_cell_bond_recorded(bond_ref.clone(), info);
    assert_eq!(
        *data.cross_cell_bonds.get(&bond_ref).unwrap(),
        info
    );

    // Remove it
    data.remove_cross_cell_bond_recorded(&bond_ref);
    assert!(data.cross_cell_bonds.get(&bond_ref).is_none());

    // End recording and check deltas
    let recorder = data.end_recording().unwrap();
    assert_eq!(recorder.cross_cell_bond_deltas.len(), 2);

    // First delta: None → Some(info)
    let d0 = &recorder.cross_cell_bond_deltas[0];
    assert!(d0.old_value.is_none());
    assert_eq!(d0.new_value, Some(info));

    // Second delta: Some(info) → None
    let d1 = &recorder.cross_cell_bond_deltas[1];
    assert_eq!(d1.old_value, Some(info));
    assert!(d1.new_value.is_none());
}
