// Lattice-space refactoring .cnnd migration tests (v2 → v3).
//
// Per-fixture tests for each migration change-class. Each phase of
// `doc/design_cnnd_migration_v2_to_v3.md` adds new fixtures here.

use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::network_validator::validate_network;
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

// ---------------------------------------------------------------------------
// Phase 3: deleted-node drop (`atom_trans`, `lattice_symop`).
// ---------------------------------------------------------------------------

/// Fixture 6 — `atom_trans_present.cnnd`:
/// two parallel chains covering both deleted v2 node types:
/// 1. `parameter(Atomic) → atom_trans → atom_move` (tests atom_trans drop);
/// 2. `cuboid → lattice_symop → lattice_move` (tests lattice_symop drop).
///
/// In each chain the deleted node sits between an upstream source and a
/// downstream consumer with a polymorphic output (so the disconnected input
/// surfaces as an unresolved-output validation error). The downstream
/// consumers are also both subject to node renames (`atom_move → free_move`,
/// `lattice_move → structure_move`), exercising the rename pass's
/// interaction with the deleted-node drop.
///
/// Expected post-migration state:
/// - the two deleted nodes disappear;
/// - each downstream node keeps its argument slot but with an empty
///   `argument_output_pins` map (wire to the deleted node removed);
/// - `displayed_nodes` no longer references the deleted ids;
/// - `validate_network` flags both downstream consumers as invalid.
#[test]
fn test_load_atom_trans_and_lattice_symop_dropped() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/atom_trans_present.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load atom_trans_present.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Deleted node types must be gone.
    let type_names: std::collections::HashSet<&str> = network
        .nodes
        .values()
        .map(|n| n.node_type_name.as_str())
        .collect();
    assert!(
        !type_names.contains("atom_trans"),
        "atom_trans must be dropped; got {:?}",
        type_names
    );
    assert!(
        !type_names.contains("lattice_symop"),
        "lattice_symop must be dropped; got {:?}",
        type_names
    );
    // The surviving nodes are the two upstream sources, the two downstream
    // consumers (renamed), and nothing else.
    assert_eq!(
        network.nodes.len(),
        4,
        "expected 4 surviving nodes (parameter, free_move, cuboid, structure_move); got {}",
        network.nodes.len()
    );

    // Downstream wire disconnection: free_move's arg 0 (previously wired to
    // atom_trans=id 2) must be empty now.
    let free_move = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "free_move")
        .expect("free_move node missing");
    assert!(
        free_move.arguments[0].argument_output_pins.is_empty(),
        "free_move's input wire to atom_trans should be disconnected; got {:?}",
        free_move.arguments[0].argument_output_pins
    );

    // Same for structure_move: its input (previously wired to lattice_symop=id 5)
    // must be empty.
    let structure_move = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "structure_move")
        .expect("structure_move node missing");
    assert!(
        structure_move.arguments[0].argument_output_pins.is_empty(),
        "structure_move's input wire to lattice_symop should be disconnected; got {:?}",
        structure_move.arguments[0].argument_output_pins
    );

    // lattice_symop's id (5) was in `displayed_node_ids`; the deleted-node drop
    // must strip it so the display map doesn't reference a missing node.
    assert!(
        !network.displayed_nodes.contains_key(&5),
        "displayed_nodes should not reference deleted lattice_symop (id 5)"
    );

    // Validate the loaded network and assert the resulting dangling inputs are
    // surfaced as errors. This is the user-visible signal the design calls for:
    // "this old node was removed, please replace it."
    let free_move_id = free_move.id;
    let structure_move_id = structure_move.id;
    let mut network = registry.node_networks.remove("Main").unwrap();
    validate_network(&mut network, &mut registry, None);
    assert!(
        !network.valid,
        "network should be invalid after the migration dropped nodes with polymorphic downstream consumers"
    );
    let error_nodes: std::collections::HashSet<u64> = network
        .validation_errors
        .iter()
        .filter_map(|e| e.node_id)
        .collect();
    // Either consumer can short-circuit the validator (it returns on the first
    // failing node); assert that at least one of them produced the error, and
    // that neither of the deleted node ids somehow survived as an error source.
    assert!(
        error_nodes.contains(&free_move_id) || error_nodes.contains(&structure_move_id),
        "at least one dangling consumer (free_move={}, structure_move={}) should have a validation error; got errors on {:?}",
        free_move_id,
        structure_move_id,
        error_nodes
    );
    assert!(
        !error_nodes.contains(&2) && !error_nodes.contains(&5),
        "deleted node ids must not appear in validation errors; got {:?}",
        error_nodes
    );
}

#[test]
fn test_roundtrip_atom_trans_and_lattice_symop_dropped() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/atom_trans_present.cnnd", FIXTURE_DIR),
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
        saved.contains("\"version\": 3"),
        "saved file should be tagged version: 3"
    );
    assert!(
        !saved.contains("\"atom_trans\""),
        "saved file must not contain the deleted `atom_trans` node type"
    );
    assert!(
        !saved.contains("\"lattice_symop\""),
        "saved file must not contain the deleted `lattice_symop` node type"
    );

    // Reload-after-save is byte-idempotent and stays on the v3 no-op path.
    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");
    let n1 = registry.node_networks.get("Main").unwrap();
    let n2 = registry2.node_networks.get("Main").unwrap();
    assert_eq!(n1.nodes.len(), n2.nodes.len());

    // And the parameter node kept its renamed Molecule type through the trip.
    let param = n2
        .nodes
        .values()
        .find(|n| n.node_type_name == "parameter")
        .expect("parameter missing after reload");
    let custom_type = param
        .custom_node_type
        .as_ref()
        .expect("parameter custom_node_type missing");
    assert_eq!(*custom_type.output_type(), DataType::Molecule);
}

// ---------------------------------------------------------------------------
// Phase 4: primitive input adaptation (`LatticeVecs` wire → `structure` adapter).
// ---------------------------------------------------------------------------

/// Fixture 3 — `primitive_with_lattice.cnnd`:
/// a Main network with two primitives covering both branches of the Phase 4
/// decision tree:
/// 1. `unit_cell → cuboid.unit_cell` (wired pin) — verifies adapter synthesis;
/// 2. `sphere` with its `unit_cell` pin unwired — verifies no-op, new
///    `structure` input falls back to the diamond default.
///
/// Expected post-migration state:
/// - `unit_cell` is renamed to `lattice_vecs`;
/// - a synthesized `structure` node sits between `lattice_vecs` and `cuboid`,
///   with the old wire routed to its `lattice_vecs` input (arg 1);
/// - `cuboid`'s `structure` input (arg 2) points at the adapter's output 0;
/// - `sphere` is untouched (no adapter synthesized);
/// - `next_node_id` advances to cover the new adapter's id;
/// - the network validates cleanly (Blueprint flows to the return node).
#[test]
fn test_load_primitive_with_lattice_adapter_synthesised() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/primitive_with_lattice.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load primitive_with_lattice.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Original 3 + 1 synthesized adapter = 4 nodes.
    assert_eq!(
        network.nodes.len(),
        4,
        "expected 3 original + 1 synthesized structure adapter; got {}",
        network.nodes.len()
    );

    // `unit_cell` renamed; exactly one synthesized `structure` adapter.
    let structure_adapters: Vec<&rust_lib_flutter_cad::structure_designer::node_network::Node> =
        network
            .nodes
            .values()
            .filter(|n| n.node_type_name == "structure")
            .collect();
    assert_eq!(
        structure_adapters.len(),
        1,
        "expected exactly one synthesized structure adapter; got {}",
        structure_adapters.len()
    );
    let adapter = structure_adapters[0];

    // Adapter must be the id allocated from next_node_id (10 → allocated to adapter).
    assert_eq!(
        adapter.id, 10,
        "adapter should use the id pulled from next_node_id"
    );
    // The adapter's 4 arguments: only arg 1 (lattice_vecs) is wired — to the
    // renamed `lattice_vecs` node (id 1). The other three are unwired so the
    // primitive still gets diamond defaults for motif / motif_offset / base.
    assert_eq!(adapter.arguments.len(), 4);
    assert!(adapter.arguments[0].argument_output_pins.is_empty());
    assert_eq!(
        adapter.arguments[1].argument_output_pins.get(&1),
        Some(&0),
        "adapter's lattice_vecs input (arg 1) must be wired to the renamed unit_cell node (id 1, pin 0)"
    );
    assert!(adapter.arguments[2].argument_output_pins.is_empty());
    assert!(adapter.arguments[3].argument_output_pins.is_empty());

    // The cuboid's structure input (arg 2) now points at the adapter's output 0.
    let cuboid = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "cuboid")
        .expect("cuboid missing");
    assert_eq!(
        cuboid.arguments[2].argument_output_pins.get(&adapter.id),
        Some(&0),
        "cuboid's structure input (arg 2) should be wired to adapter's output 0"
    );
    assert_eq!(
        cuboid.arguments[2].argument_output_pins.len(),
        1,
        "cuboid's structure pin must have the adapter as its only source"
    );

    // The sphere — whose unit_cell pin was unwired in v2 — has no adapter.
    // Its structure input (arg 2) stays empty; diamond defaults apply.
    let sphere = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "sphere")
        .expect("sphere missing");
    assert!(
        sphere.arguments[2].argument_output_pins.is_empty(),
        "sphere had no v2 unit_cell wire; its structure pin must stay unwired (no adapter synthesized)"
    );

    // next_node_id advanced past the allocated id so future nodes don't clash.
    assert!(
        network.next_node_id > adapter.id,
        "next_node_id ({}) should be greater than the adapter's id ({})",
        network.next_node_id,
        adapter.id
    );

    // The migrated network validates cleanly: cuboid outputs Blueprint to the
    // return node and no pin is left dangling.
    let mut network = registry.node_networks.remove("Main").unwrap();
    validate_network(&mut network, &mut registry, None);
    let error_texts: Vec<String> = network
        .validation_errors
        .iter()
        .map(|e| e.error_text.clone())
        .collect();
    assert!(
        network.valid,
        "migrated network should validate cleanly; errors: {:?}",
        error_texts
    );
}

#[test]
fn test_roundtrip_primitive_with_lattice() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/primitive_with_lattice.cnnd", FIXTURE_DIR),
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
        saved.contains("\"version\": 3"),
        "saved file should be tagged version: 3"
    );
    assert!(
        !saved.contains("\"unit_cell\""),
        "saved file must not contain the renamed-away `unit_cell` node type"
    );
    // The adapter round-trips as a real `structure` node.
    assert!(
        saved.contains("\"structure\""),
        "saved file should contain the synthesized `structure` node type"
    );

    // Reload-after-save stays on the v3 no-op path and preserves the adapter.
    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");
    let n1 = registry.node_networks.get("Main").unwrap();
    let n2 = registry2.node_networks.get("Main").unwrap();
    assert_eq!(n1.nodes.len(), n2.nodes.len());

    // Exactly one structure adapter survives the round trip — no second pass
    // can sneak an extra one in on reload.
    let adapter_count = n2
        .nodes
        .values()
        .filter(|n| n.node_type_name == "structure")
        .count();
    assert_eq!(
        adapter_count, 1,
        "second load must not synthesize a second adapter; got {}",
        adapter_count
    );
}
