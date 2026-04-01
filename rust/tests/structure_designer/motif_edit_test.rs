// Tests for the motif_edit node (Phase 2 + Phase 3: parameter elements)

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::motif::Motif;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::EvalOutput;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, PARAM_ELEMENT_BASE, get_node_type_motif_edit, is_atom_edit_family,
    is_param_element, param_atomic_number_to_index, param_atomic_number_to_motif,
    param_index_to_atomic_number,
};
use rust_lib_flutter_cad::structure_designer::serialization::atom_edit_data_serialization::{
    atom_edit_data_to_serializable, serializable_to_atom_edit_data,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

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
