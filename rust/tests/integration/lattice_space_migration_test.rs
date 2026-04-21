// Lattice-space refactoring .cnnd migration tests (v2 → v3).
//
// Per-fixture tests for each migration change-class. Each phase of
// `doc/design_cnnd_migration_v2_to_v3.md` adds new fixtures here.

use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use tempfile::tempdir;

const FIXTURE_DIR: &str = "tests/fixtures/lattice_space_migration";

// ---------------------------------------------------------------------------
// Phase 1: version dispatch — v3 file is a no-op through the new load path.
// ---------------------------------------------------------------------------

#[test]
fn test_load_trivial_v3_no_op() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/trivial_v3.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load trivial_v3.cnnd");

    assert_eq!(load_result.first_network_name, "Main");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // A v3 file with a single sphere node loads exactly as written.
    assert_eq!(network.nodes.len(), 1);
    let only = network.nodes.values().next().unwrap();
    assert_eq!(only.node_type_name, "sphere");
    assert_eq!(network.return_node_id, Some(1));
}

// ---------------------------------------------------------------------------
// Phase 2: string renames (DataType + node type).
// ---------------------------------------------------------------------------

/// Fixture 1 — `pure_rename.cnnd`:
/// a Main network using `unit_cell`, `atom_move`, and a `parameter` node whose
/// `data.data_type` is `"UnitCell"`. No structural changes needed post-v3 beyond
/// the string rewrites. Verifies both rename tables (DataType + node type) and
/// that the `data_type` dispatch tag is rewritten in lock-step with
/// `node_type_name`.
#[test]
fn test_load_pure_rename() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/pure_rename.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load pure_rename.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Node-type rename: `unit_cell` → `lattice_vecs`, `atom_move` → `free_move`.
    let type_names: std::collections::HashSet<&str> = network
        .nodes
        .values()
        .map(|n| n.node_type_name.as_str())
        .collect();
    assert!(
        type_names.contains("lattice_vecs"),
        "expected unit_cell → lattice_vecs; got {:?}",
        type_names
    );
    assert!(
        type_names.contains("free_move"),
        "expected atom_move → free_move; got {:?}",
        type_names
    );
    assert!(
        !type_names.contains("unit_cell") && !type_names.contains("atom_move"),
        "old node type names must not survive the rename; got {:?}",
        type_names
    );

    // DataType rename inside ParameterData: `"UnitCell"` → `"LatticeVecs"`. The parameter
    // node's custom type is computed from its data_type so the pin type shows through.
    let param_node = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "parameter")
        .expect("parameter node missing");
    let custom_type = param_node
        .custom_node_type
        .as_ref()
        .expect("parameter node should have a resolved custom node type");
    assert_eq!(
        *custom_type.output_type(),
        DataType::LatticeVecs,
        "parameter node's output should be LatticeVecs after v2→v3 rename"
    );
}

#[test]
fn test_roundtrip_pure_rename() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/pure_rename.cnnd", FIXTURE_DIR),
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

    // Re-save is v3: second load goes through the v3 no-op path.
    let saved = std::fs::read_to_string(&temp_path).expect("read saved file");
    assert!(
        saved.contains("\"version\": 3"),
        "saved file should be tagged version: 3"
    );
    assert!(
        !saved.contains("\"unit_cell\""),
        "saved file must not contain the old `unit_cell` node type name"
    );
    assert!(
        !saved.contains("\"atom_move\""),
        "saved file must not contain the old `atom_move` node type name"
    );

    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");

    let net1 = registry.node_networks.get("Main").unwrap();
    let net2 = registry2.node_networks.get("Main").unwrap();
    assert_eq!(net1.nodes.len(), net2.nodes.len());
    assert_eq!(net1.return_node_id, net2.return_node_id);
}

/// Fixture 7 — `custom_network.cnnd`:
/// the rename walker must reach the parameter/output-pin type strings inside
/// each custom network's embedded `node_type`, not just the top-level Main
/// network. Also verifies the `[Atomic]` (bracketed) Display form rewrites to
/// `[Molecule]` rather than the abstract `HasAtoms`.
#[test]
fn test_load_custom_network_rename() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/custom_network.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load custom_network.cnnd");

    let my_custom = registry
        .node_networks
        .get("my_custom")
        .expect("my_custom network missing");

    // Custom-network output_type: `Geometry` → `Blueprint`.
    assert_eq!(
        *my_custom.node_type.output_type(),
        DataType::Blueprint,
        "custom-network output_type should be Blueprint after rename"
    );

    // Custom-network parameter types: `UnitCell` → `LatticeVecs`, `[Atomic]` → `[Molecule]`.
    let param_types: Vec<(String, DataType)> = my_custom
        .node_type
        .parameters
        .iter()
        .map(|p| (p.name.clone(), p.data_type.clone()))
        .collect();
    let expected: Vec<(String, DataType)> = vec![
        ("uc".to_string(), DataType::LatticeVecs),
        (
            "motifs".to_string(),
            DataType::Array(Box::new(DataType::Molecule)),
        ),
    ];
    assert_eq!(param_types, expected);

    // The registry must expose the custom type with the renamed output.
    let custom_type = registry
        .get_node_type("my_custom")
        .expect("my_custom should be registered as a node type");
    assert_eq!(*custom_type.output_type(), DataType::Blueprint);
}

#[test]
fn test_roundtrip_custom_network_rename() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/custom_network.cnnd", FIXTURE_DIR),
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

    let saved = std::fs::read_to_string(&temp_path).expect("read saved file");
    assert!(
        !saved.contains("\"UnitCell\""),
        "saved file must not contain v2 `UnitCell` DataType string"
    );
    assert!(
        !saved.contains("\"Geometry\""),
        "saved file must not contain v2 `Geometry` DataType string"
    );
    assert!(
        !saved.contains("\"[Atomic]\""),
        "saved file must not contain v2 `[Atomic]` DataType string"
    );

    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");

    let c1 = registry.node_networks.get("my_custom").unwrap();
    let c2 = registry2.node_networks.get("my_custom").unwrap();
    assert_eq!(c1.node_type.output_type(), c2.node_type.output_type());
    assert_eq!(
        c1.node_type.parameters.len(),
        c2.node_type.parameters.len()
    );
}
