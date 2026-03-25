// Phase 0: Backward-compatibility fixture tests for multi-output pin migration.
//
// These tests load frozen .cnnd files created with the old serialization format
// (before any multi-output changes) and verify they load correctly.
// They establish a baseline so that after Phase 1+ changes, we can verify
// the old format still loads correctly (migration).

use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_network::NodeDisplayType;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use tempfile::tempdir;

const FIXTURE_DIR: &str = "tests/fixtures/multi_output_migration";

// ---------------------------------------------------------------------------
// Fixture 1: old_builtin_only.cnnd
// A network with only built-in nodes (sphere, cuboid, union).
// Uses old `output_type` field on node_type. Has `displayed_node_ids` but no
// `displayed_output_pins`.
// ---------------------------------------------------------------------------

#[test]
fn test_load_old_builtin_only() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/old_builtin_only.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load old_builtin_only.cnnd");

    assert_eq!(load_result.first_network_name, "Main");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Verify nodes loaded correctly
    assert_eq!(
        network.nodes.len(),
        3,
        "Expected 3 nodes (sphere, cuboid, union)"
    );

    // Verify built-in node types resolve from registry
    for (_id, node) in &network.nodes {
        let node_type = registry
            .get_node_type(&node.node_type_name)
            .unwrap_or_else(|| panic!("Node type '{}' not found in registry", node.node_type_name));
        // All these are geometry-producing nodes
        assert_ne!(
            *node_type.output_type(),
            DataType::None,
            "Node type '{}' should have a valid output type",
            node.node_type_name
        );
    }

    // Verify the union node has wired inputs from sphere and cuboid
    let union_node = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "union")
        .expect("Union node missing");
    // union's first argument (shapes) should have 2 wired inputs
    assert!(
        !union_node.arguments.is_empty(),
        "Union node should have arguments"
    );
    assert_eq!(
        union_node.arguments[0].argument_output_pins.len(),
        2,
        "Union shapes param should have 2 wired inputs"
    );

    // Verify network output_type (from old serialized field)
    assert_eq!(*network.node_type.output_type(), DataType::Geometry);

    // Verify displayed_nodes loaded (old format, no per-pin info)
    assert_eq!(
        network.displayed_nodes.len(),
        1,
        "Expected 1 displayed node"
    );
    // The union/result node should be displayed as Normal
    let return_node_id = network
        .return_node_id
        .expect("return_node_id should be set");
    assert_eq!(
        network.get_node_display_type(return_node_id),
        Some(NodeDisplayType::Normal),
        "Return node should be displayed as Normal"
    );
}

#[test]
fn test_roundtrip_old_builtin_only() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/old_builtin_only.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load");

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().join("roundtrip.cnnd");

    save_node_networks_to_file(
        &mut registry,
        &temp_path,
        load_result.direct_editing_mode,
        &load_result.cli_access_rules,
    )
    .expect("Failed to save");

    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");

    let net1 = registry.node_networks.get("Main").unwrap();
    let net2 = registry2.node_networks.get("Main").unwrap();
    assert_eq!(net1.nodes.len(), net2.nodes.len());
    assert_eq!(net1.next_node_id, net2.next_node_id);
    assert_eq!(net1.return_node_id, net2.return_node_id);
    assert_eq!(net1.displayed_nodes.len(), net2.displayed_nodes.len());
    assert_eq!(net1.node_type.output_type(), net2.node_type.output_type());
}

// ---------------------------------------------------------------------------
// Fixture 2: old_custom_network.cnnd
// Two networks: "my_shape" defines a custom node type (with old
// `"output_type": "Geometry"` on its node_type), "Main" uses it as a node.
// ---------------------------------------------------------------------------

#[test]
fn test_load_old_custom_network() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/old_custom_network.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load old_custom_network.cnnd");

    assert_eq!(load_result.first_network_name, "Main");

    // Both networks should exist
    assert!(
        registry.node_networks.contains_key("Main"),
        "Main network missing"
    );
    assert!(
        registry.node_networks.contains_key("my_shape"),
        "my_shape network missing"
    );

    // Verify custom network's output_type was loaded from old format
    let my_shape = registry.node_networks.get("my_shape").unwrap();
    assert_eq!(
        *my_shape.node_type.output_type(),
        DataType::Geometry,
        "Custom network output_type should be Geometry (migrated from old output_type string)"
    );

    // Verify custom network has a parameter
    assert_eq!(
        my_shape.node_type.parameters.len(),
        1,
        "my_shape should have 1 parameter"
    );
    assert_eq!(my_shape.node_type.parameters[0].name, "size");

    // Verify Main uses the custom node type
    let main = registry.node_networks.get("Main").unwrap();
    assert_eq!(main.nodes.len(), 1);
    let custom_node = main.nodes.values().next().unwrap();
    assert_eq!(custom_node.node_type_name, "my_shape");

    // Verify the custom node type is available in the registry (as a network-based type)
    let custom_type = registry.get_node_type("my_shape");
    assert!(
        custom_type.is_some(),
        "my_shape should be available as a node type in registry"
    );
    assert_eq!(*custom_type.unwrap().output_type(), DataType::Geometry);
}

#[test]
fn test_roundtrip_old_custom_network() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/old_custom_network.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load");

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().join("roundtrip.cnnd");

    save_node_networks_to_file(
        &mut registry,
        &temp_path,
        load_result.direct_editing_mode,
        &load_result.cli_access_rules,
    )
    .expect("Failed to save");

    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");

    // Both networks survive roundtrip
    assert!(registry2.node_networks.contains_key("Main"));
    assert!(registry2.node_networks.contains_key("my_shape"));

    let shape1 = registry.node_networks.get("my_shape").unwrap();
    let shape2 = registry2.node_networks.get("my_shape").unwrap();
    assert_eq!(
        shape1.node_type.output_type(),
        shape2.node_type.output_type()
    );
    assert_eq!(
        shape1.node_type.parameters.len(),
        shape2.node_type.parameters.len()
    );
    assert_eq!(shape1.nodes.len(), shape2.nodes.len());
}

// ---------------------------------------------------------------------------
// Fixture 3: old_atom_edit_output_diff_false.cnnd
// A network with atom_edit node where output_diff is false (default).
// ---------------------------------------------------------------------------

#[test]
fn test_load_old_atom_edit_output_diff_false() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/old_atom_edit_output_diff_false.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load old_atom_edit_output_diff_false.cnnd");

    let network = registry.node_networks.get("Main").unwrap();

    // Find the atom_edit node
    let atom_edit_node = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "atom_edit")
        .expect("atom_edit node missing");

    // Verify output_diff is false
    let atom_edit_data = atom_edit_node
        .data
        .as_any_ref()
        .downcast_ref::<AtomEditData>()
        .expect("Failed to downcast to AtomEditData");
    assert!(!atom_edit_data.output_diff, "output_diff should be false");

    // Verify network output_type is Atomic (atom_edit's output)
    assert_eq!(*network.node_type.output_type(), DataType::Atomic);

    // Verify the atom_edit node is displayed
    let atom_edit_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == "atom_edit")
        .map(|(id, _)| *id)
        .unwrap();
    assert_eq!(
        network.get_node_display_type(atom_edit_id),
        Some(NodeDisplayType::Normal),
        "atom_edit node should be displayed"
    );

    // Verify wiring: atom_edit is connected to atom_fill
    assert!(
        !atom_edit_node.arguments.is_empty(),
        "atom_edit should have arguments"
    );
    assert_eq!(
        atom_edit_node.arguments[0].argument_output_pins.len(),
        1,
        "atom_edit's molecule input should be wired"
    );
}

// ---------------------------------------------------------------------------
// Fixture 4: old_atom_edit_output_diff_true.cnnd
// A network with atom_edit node where output_diff is true.
// ---------------------------------------------------------------------------

#[test]
fn test_load_old_atom_edit_output_diff_true() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/old_atom_edit_output_diff_true.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load old_atom_edit_output_diff_true.cnnd");

    let network = registry.node_networks.get("Main").unwrap();

    // Find the atom_edit node
    let atom_edit_node = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "atom_edit")
        .expect("atom_edit node missing");

    // Verify output_diff is true
    let atom_edit_data = atom_edit_node
        .data
        .as_any_ref()
        .downcast_ref::<AtomEditData>()
        .expect("Failed to downcast to AtomEditData");
    assert!(atom_edit_data.output_diff, "output_diff should be true");

    // Verify the atom_edit node is displayed
    let atom_edit_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == "atom_edit")
        .map(|(id, _)| *id)
        .unwrap();
    assert_eq!(
        network.get_node_display_type(atom_edit_id),
        Some(NodeDisplayType::Normal),
    );
}

// ---------------------------------------------------------------------------
// Phase 3 migration tests: output_diff is migrated to displayed_pins in NodeDisplayState.
// ---------------------------------------------------------------------------

#[test]
fn test_migrate_atom_edit_output_diff_true_to_displayed_pins() {
    // Loading old file with output_diff: true should migrate to displayed_pins: {1}
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/old_atom_edit_output_diff_true.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load");

    let network = registry.node_networks.get("Main").unwrap();
    let atom_edit_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == "atom_edit")
        .map(|(id, _)| *id)
        .unwrap();

    let display_state = network.displayed_nodes.get(&atom_edit_id).unwrap();
    assert_eq!(
        display_state.displayed_pins,
        std::collections::HashSet::from([1]),
        "output_diff: true should migrate to displayed_pins: {{1}}"
    );
}

#[test]
fn test_migrate_atom_edit_output_diff_false_to_displayed_pins() {
    // Loading old file with output_diff: false should keep default displayed_pins: {0}
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/old_atom_edit_output_diff_false.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load");

    let network = registry.node_networks.get("Main").unwrap();
    let atom_edit_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == "atom_edit")
        .map(|(id, _)| *id)
        .unwrap();

    let display_state = network.displayed_nodes.get(&atom_edit_id).unwrap();
    assert_eq!(
        display_state.displayed_pins,
        std::collections::HashSet::from([0]),
        "output_diff: false should keep default displayed_pins: {{0}}"
    );
}
