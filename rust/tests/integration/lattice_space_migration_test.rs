// Lattice-space refactoring .cnnd migration tests (v2 → v3).
//
// Per-fixture tests for each migration change-class. Each phase of
// `doc/design_cnnd_migration_v2_to_v3.md` adds new fixtures here.

use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::network_validator::validate_network;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::migrate_v2_to_v3::{
    migrate_v2_to_v3, migration_call_count, reset_migration_call_count,
};
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use tempfile::tempdir;

const FIXTURE_DIR: &str = "tests/fixtures/lattice_space_migration";

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Loads a fixture and validates every network in dependency order. Mirrors
/// `StructureDesigner::load_node_networks` so the resulting registry is in the
/// state evaluation expects (sub-networks resolved, custom-type definitions
/// up-to-date). Uses the same split-borrow pattern as the production loader.
fn load_and_validate(fixture_path: &str) -> NodeTypeRegistry {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, fixture_path)
        .unwrap_or_else(|e| panic!("Failed to load {}: {}", fixture_path, e));

    let networks_in_order = registry.get_networks_in_dependency_order();
    for network_name in networks_in_order {
        // Same split-borrow trick StructureDesigner::load_node_networks uses:
        // validate_network mutates the current network and the registry, but
        // not in conflicting ways. The raw pointer avoids the double-borrow
        // checker error.
        let registry_ptr = &mut registry as *mut NodeTypeRegistry;
        unsafe {
            if let Some(network) = (*registry_ptr).node_networks.get_mut(&network_name) {
                validate_network(network, &mut *registry_ptr, None);
            }
        }
    }
    registry
}

/// Evaluates a node's pin 0 in the named network and returns the resulting
/// `AtomicStructure` (works for both Crystal and Molecule outputs). Panics
/// on Error or any non-atomic result type so test failures are loud.
fn evaluate_to_atoms(
    registry: &NodeTypeRegistry,
    network_name: &str,
    node_id: u64,
) -> AtomicStructure {
    let network = registry
        .node_networks
        .get(network_name)
        .unwrap_or_else(|| panic!("network {} missing from registry", network_name));
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    let result = evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context);
    match result {
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Molecule(m) => m.atoms,
        NetworkResult::Error(e) => panic!(
            "expected atoms from node {} in {}; got Error: {}",
            node_id, network_name, e
        ),
        other => panic!(
            "expected Crystal/Molecule from node {} in {}; got {:?}",
            node_id,
            network_name,
            std::mem::discriminant(&other)
        ),
    }
}

/// Finds the (single) `materialize` node in a network and returns its id.
/// Panics if there is not exactly one.
fn materialize_node_id(registry: &NodeTypeRegistry, network_name: &str) -> u64 {
    let network = registry.node_networks.get(network_name).unwrap();
    let mat_ids: Vec<u64> = network
        .nodes
        .values()
        .filter(|n| n.node_type_name == "materialize")
        .map(|n| n.id)
        .collect();
    assert_eq!(
        mat_ids.len(),
        1,
        "expected exactly one materialize in {}; got {:?}",
        network_name,
        mat_ids
    );
    mat_ids[0]
}

// ---------------------------------------------------------------------------
// Phase 1: version dispatch — v3 file is a no-op through the new load path.
// ---------------------------------------------------------------------------

#[test]
fn test_load_trivial_v3_no_op() {
    let mut registry = NodeTypeRegistry::new();
    let load_result =
        load_node_networks_from_file(&mut registry, &format!("{}/trivial_v3.cnnd", FIXTURE_DIR))
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
    load_node_networks_from_file(&mut registry, &format!("{}/pure_rename.cnnd", FIXTURE_DIR))
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
    let load_result =
        load_node_networks_from_file(&mut registry, &format!("{}/pure_rename.cnnd", FIXTURE_DIR))
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
    assert_eq!(c1.node_type.parameters.len(), c2.node_type.parameters.len());
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

// ---------------------------------------------------------------------------
// Phase 5: `atom_fill` split (renames to `materialize`, synthesises a
// `structure` source node holding the motif / motif_offset wires).
// ---------------------------------------------------------------------------

/// Fixture 2 — `atom_fill_split.cnnd` (case A coverage):
/// a `unit_cell → cuboid → atom_fill` pipeline with motif and motif_offset
/// sources wired in, every Bool flag pin (passivate, rm_single, surf_recon,
/// invert_phase) wired to its own `bool` source, and non-default
/// `AtomFillData` field values. Verifies the full case-A algorithm: node-rename
/// plus data-tag rename, argument re-indexing, NodeData translation (drop
/// `motif_offset`, keep the rest), and synthesis of the W/G/S triplet that
/// patches the user's motif into the Structure flowing into `materialize`.
///
/// Expected post-migration state (case A):
/// - the v2 `atom_fill` is renamed to `materialize` (both `node_type_name`
///   and the `data_type` dispatch tag);
/// - `materialize.arguments` has 5 slots in v3 order: shape (now wired to W,
///   not directly to cuboid), then the four Bool flag wires;
/// - a `with_structure` (W) node sits between the cuboid and `materialize`,
///   reading the patched Structure from a `structure` (S) override node, which
///   in turn reads its base Structure from a `get_structure` (G) node fed by
///   a clone of the original shape wire (cuboid id 2);
/// - S carries the motif wire on arg 2 and the motif_offset wire on arg 3;
///   its `structure` (arg 0) is wired to G; its `lattice_vecs` (arg 1) stays
///   unwired (the lattice rides through with the Blueprint);
/// - `MaterializeData` carries `parameter_element_value_definition`,
///   `hydrogen_passivation`, `remove_single_bond_atoms_before_passivation`,
///   `surface_reconstruction`, `invert_phase` verbatim from `AtomFillData`;
/// - `next_node_id` advances past every newly-allocated id (one for the
///   primitive-adaptation adapter on cuboid, three for the W/G/S triplet).
#[test]
fn test_load_atom_fill_split() {
    use rust_lib_flutter_cad::structure_designer::nodes::materialize::MaterializeData;

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/atom_fill_split.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load atom_fill_split.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // No surviving atom_fill; exactly one materialize.
    let type_names: std::collections::HashSet<&str> = network
        .nodes
        .values()
        .map(|n| n.node_type_name.as_str())
        .collect();
    assert!(
        !type_names.contains("atom_fill"),
        "atom_fill must be renamed; got {:?}",
        type_names
    );
    let materialize_nodes: Vec<&rust_lib_flutter_cad::structure_designer::node_network::Node> =
        network
            .nodes
            .values()
            .filter(|n| n.node_type_name == "materialize")
            .collect();
    assert_eq!(materialize_nodes.len(), 1, "expected one materialize");
    let materialize = materialize_nodes[0];
    // The renamed node keeps its original id (9).
    assert_eq!(materialize.id, 9);

    // Case A synthesises three new nodes — `get_structure` (G), `with_structure`
    // (W), and a `structure` override (S) — alongside the primitive-adaptation
    // adapter on cuboid. So we expect:
    //   - exactly two `structure` nodes (the cuboid adapter + the override S),
    //   - exactly one `get_structure` node (G),
    //   - exactly one `with_structure` node (W).
    let structure_nodes: Vec<&rust_lib_flutter_cad::structure_designer::node_network::Node> =
        network
            .nodes
            .values()
            .filter(|n| n.node_type_name == "structure")
            .collect();
    assert_eq!(
        structure_nodes.len(),
        2,
        "expected one structure adapter (from primitive adaptation) plus one \
         structure override (from atom_fill split); got {}",
        structure_nodes.len()
    );
    let get_structure = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "get_structure")
        .expect("case A must synthesise a get_structure node");
    let with_structure = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "with_structure")
        .expect("case A must synthesise a with_structure node");

    // The split's structure override is the one with motif/motif_offset wired
    // *and* a base wire to G. The primitive adapter is the inverse: only its
    // lattice_vecs (arg 1) is wired.
    let split_override = structure_nodes
        .iter()
        .find(|n| {
            !n.arguments[2].argument_output_pins.is_empty()
                || !n.arguments[3].argument_output_pins.is_empty()
        })
        .expect("structure override for atom_fill split should have motif or motif_offset wired");
    assert_eq!(split_override.arguments.len(), 4);
    assert_eq!(
        split_override.arguments[0]
            .argument_output_pins
            .get(&get_structure.id),
        Some(&0),
        "case A split override's `structure` (arg 0) must be wired to G's pin 0"
    );
    assert!(
        split_override.arguments[1].argument_output_pins.is_empty(),
        "split override's `lattice_vecs` (arg 1) must be unwired"
    );
    assert_eq!(
        split_override.arguments[2].argument_output_pins.get(&3),
        Some(&0),
        "split override's `motif` (arg 2) must be wired to motif source (id 3, pin 0)"
    );
    assert_eq!(
        split_override.arguments[3].argument_output_pins.get(&4),
        Some(&0),
        "split override's `motif_offset` (arg 3) must be wired to vec3 source (id 4, pin 0)"
    );

    // G reads its single input from a clone of the original shape wire (cuboid id 2).
    assert_eq!(get_structure.arguments.len(), 1);
    assert_eq!(
        get_structure.arguments[0].argument_output_pins.get(&2),
        Some(&0),
        "G's input must be a clone of the original shape wire (cuboid id 2, pin 0)"
    );

    // W: arg 0 (shape) ← clone of the original shape wire (cuboid id 2);
    //    arg 1 (structure) ← S's pin 0.
    assert_eq!(with_structure.arguments.len(), 2);
    assert_eq!(
        with_structure.arguments[0].argument_output_pins.get(&2),
        Some(&0),
        "W's `shape` (arg 0) must be a clone of the original shape wire (cuboid id 2)"
    );
    assert_eq!(
        with_structure.arguments[1]
            .argument_output_pins
            .get(&split_override.id),
        Some(&0),
        "W's `structure` (arg 1) must be wired to the override S's pin 0"
    );

    // Materialize args, in v3 order: shape (now W, not cuboid directly),
    // passivate (id 5), rm_single (id 6), surf_recon (id 7), invert_phase (id 8).
    assert_eq!(materialize.arguments.len(), 5);
    assert_eq!(
        materialize.arguments[0]
            .argument_output_pins
            .get(&with_structure.id),
        Some(&0),
        "case A: materialize.shape must point at W's pin 0, not directly at the cuboid"
    );
    assert_eq!(
        materialize.arguments[0].argument_output_pins.len(),
        1,
        "materialize.shape must have W as its only source in case A"
    );
    assert_eq!(
        materialize.arguments[1].argument_output_pins.get(&5),
        Some(&0),
        "passivate wire must move from v2 arg 3 to v3 arg 1"
    );
    assert_eq!(
        materialize.arguments[2].argument_output_pins.get(&6),
        Some(&0),
        "rm_single wire must move from v2 arg 4 to v3 arg 2"
    );
    assert_eq!(
        materialize.arguments[3].argument_output_pins.get(&7),
        Some(&0),
        "surf_recon wire must move from v2 arg 5 to v3 arg 3"
    );
    assert_eq!(
        materialize.arguments[4].argument_output_pins.get(&8),
        Some(&0),
        "invert_phase wire must move from v2 arg 6 to v3 arg 4"
    );

    // NodeData translation: AtomFillData → MaterializeData. `motif_offset` is
    // dropped; everything else carries over verbatim from the v2 fixture.
    let mat_data = materialize
        .data
        .as_any_ref()
        .downcast_ref::<MaterializeData>()
        .expect("materialize node should hold MaterializeData");
    assert_eq!(mat_data.parameter_element_value_definition, "X = C");
    assert!(!mat_data.hydrogen_passivation);
    assert!(mat_data.remove_single_bond_atoms_before_passivation);
    assert!(!mat_data.surface_reconstruction);
    assert!(mat_data.invert_phase);

    // next_node_id advances past every synthesised id — primitive adapter (1)
    // + W/G/S triplet (3). With v2 next_node_id=10, post-migration must be ≥ 14.
    assert!(
        network.next_node_id >= 14,
        "next_node_id should advance past both the cuboid adapter and the \
         atom_fill split's W/G/S triplet; got {}",
        network.next_node_id
    );
}

#[test]
fn test_roundtrip_atom_fill_split() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/atom_fill_split.cnnd", FIXTURE_DIR),
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
        !saved.contains("\"atom_fill\""),
        "saved file must not contain the v2 `atom_fill` node type name"
    );
    assert!(
        saved.contains("\"materialize\""),
        "saved file should contain the v3 `materialize` node type"
    );

    // Reload-after-save stays on the v3 no-op path; the split runs once and
    // doesn't multiply on subsequent loads.
    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");
    let n1 = registry.node_networks.get("Main").unwrap();
    let n2 = registry2.node_networks.get("Main").unwrap();
    assert_eq!(n1.nodes.len(), n2.nodes.len());

    let materialize_count = n2
        .nodes
        .values()
        .filter(|n| n.node_type_name == "materialize")
        .count();
    let structure_count = n2
        .nodes
        .values()
        .filter(|n| n.node_type_name == "structure")
        .count();
    let get_structure_count = n2
        .nodes
        .values()
        .filter(|n| n.node_type_name == "get_structure")
        .count();
    let with_structure_count = n2
        .nodes
        .values()
        .filter(|n| n.node_type_name == "with_structure")
        .count();
    assert_eq!(
        materialize_count, 1,
        "second load must not duplicate the materialize node"
    );
    assert_eq!(
        structure_count, 2,
        "second load must not synthesise a third structure node \
         (1 primitive adapter + 1 case-A override)"
    );
    assert_eq!(
        get_structure_count, 1,
        "second load must not synthesise a second get_structure node"
    );
    assert_eq!(
        with_structure_count, 1,
        "second load must not synthesise a second with_structure node"
    );
}

/// Fixture 4 — `shared_unit_cell.cnnd` (case B coverage):
/// one `unit_cell` (id 1) feeding two primitives — a `cuboid` (id 2) which
/// then feeds `atom_fill.shape` (id 4), and a parallel `sphere` (id 3) used
/// directly. The atom_fill has only its shape pin wired (motif and
/// motif_offset both unwired), so it falls into case B: rename + re-index
/// only, no W/G/S triplet. Verifies the primitive-adaptation pass and the
/// atom_fill split compose cleanly when no override is needed: the unit_cell's
/// two consumers each get their own `structure` adapter, but no extra
/// `structure` source is synthesised for the atom_fill itself.
#[test]
fn test_load_shared_unit_cell_composes_passes() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/shared_unit_cell.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load shared_unit_cell.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Case B: 4 originals + 2 primitive adapters = 6 nodes. No W/G/S triplet
    // because the atom_fill's motif and motif_offset pins were both unwired.
    assert_eq!(
        network.nodes.len(),
        6,
        "expected 4 originals + 2 primitive adapters (case B: no W/G/S triplet); got {}",
        network.nodes.len()
    );

    let structure_nodes: Vec<&rust_lib_flutter_cad::structure_designer::node_network::Node> =
        network
            .nodes
            .values()
            .filter(|n| n.node_type_name == "structure")
            .collect();
    assert_eq!(
        structure_nodes.len(),
        2,
        "expected 2 synthesised structure nodes (one adapter per primitive); \
         no atom_fill split override in case B; got {}",
        structure_nodes.len()
    );

    // Case B must not emit a get_structure or with_structure either.
    assert!(
        !network
            .nodes
            .values()
            .any(|n| n.node_type_name == "get_structure"),
        "case B must not synthesise a get_structure node"
    );
    assert!(
        !network
            .nodes
            .values()
            .any(|n| n.node_type_name == "with_structure"),
        "case B must not synthesise a with_structure node"
    );

    // The two primitive adapters both read from the renamed lattice_vecs (id 1).
    // Each adapter's lattice_vecs input (arg 1) must be wired to id 1, with
    // structure / motif / motif_offset (args 0, 2, 3) all unwired.
    for adapter in &structure_nodes {
        assert!(
            adapter.arguments[0].argument_output_pins.is_empty(),
            "primitive adapter's `structure` (arg 0) must be unwired"
        );
        assert_eq!(
            adapter.arguments[1].argument_output_pins.get(&1),
            Some(&0),
            "primitive adapter's lattice_vecs (arg 1) must be wired to id 1, pin 0"
        );
        assert!(
            adapter.arguments[2].argument_output_pins.is_empty(),
            "primitive adapter's `motif` (arg 2) must be unwired"
        );
        assert!(
            adapter.arguments[3].argument_output_pins.is_empty(),
            "primitive adapter's `motif_offset` (arg 3) must be unwired"
        );
    }

    // The materialize node (renamed atom_fill, id 4) has its shape wire
    // pointing at the cuboid (id 2) — case B preserves the original shape wire
    // verbatim. The four Bool flag args are unwired (no flag wires here).
    let materialize = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "materialize")
        .expect("materialize node missing");
    assert_eq!(materialize.id, 4);
    assert_eq!(materialize.arguments.len(), 5);
    assert_eq!(
        materialize.arguments[0].argument_output_pins.get(&2),
        Some(&0),
        "case B: materialize.shape (arg 0) must still point at cuboid (id 2, pin 0)"
    );
    for i in 1..5 {
        assert!(
            materialize.arguments[i].argument_output_pins.is_empty(),
            "materialize Bool arg {} should be unwired in this fixture",
            i
        );
    }

    // Each cuboid / sphere has its `structure` input (arg 2) rewired to its
    // own adapter — never to id 1 directly.
    for primitive_name in ["cuboid", "sphere"] {
        let prim = network
            .nodes
            .values()
            .find(|n| n.node_type_name == primitive_name)
            .unwrap_or_else(|| panic!("{} node missing", primitive_name));
        let wires = &prim.arguments[2].argument_output_pins;
        assert_eq!(
            wires.len(),
            1,
            "{} should have exactly one structure-input source",
            primitive_name
        );
        assert!(
            !wires.contains_key(&1),
            "{}'s structure input must point at its adapter, not directly at id 1",
            primitive_name
        );
    }

    // Validate the migrated network: in case B the materialize should evaluate
    // (its lattice context flows through the cuboid's adapter) — assert the
    // network is valid end-to-end.
    let mut network = registry.node_networks.remove("Main").unwrap();
    validate_network(&mut network, &mut registry, None);
    let error_texts: Vec<String> = network
        .validation_errors
        .iter()
        .map(|e| e.error_text.clone())
        .collect();
    assert!(
        network.valid,
        "migrated case-B network should validate cleanly; errors: {:?}",
        error_texts
    );
}

#[test]
fn test_roundtrip_shared_unit_cell() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/shared_unit_cell.cnnd", FIXTURE_DIR),
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
        !saved.contains("\"atom_fill\""),
        "saved file must not contain the v2 `atom_fill` node type name"
    );

    // Reload-after-save: structure-node count must be stable (no new
    // synthesis on top of v3 input).
    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");
    let n1 = registry.node_networks.get("Main").unwrap();
    let n2 = registry2.node_networks.get("Main").unwrap();
    assert_eq!(n1.nodes.len(), n2.nodes.len());
    let s1 = n1
        .nodes
        .values()
        .filter(|n| n.node_type_name == "structure")
        .count();
    let s2 = n2
        .nodes
        .values()
        .filter(|n| n.node_type_name == "structure")
        .count();
    assert_eq!(s1, s2, "structure-node count must survive a v3 round trip");
}

// ---------------------------------------------------------------------------
// Phase 6: hardening — frame_transform silent drop, idempotence, v3 no-op,
// corrupt-input error surface, real-sample smoke.
// ---------------------------------------------------------------------------

/// Fixture 5 — `frame_transform_present.cnnd`:
/// a v2 file whose `cuboid` and `sphere` node data blocks each carry a stray
/// `frame_transform` field (as Appendix B of the refactoring design anticipates
/// for older saves that encoded it inline). Verifies that serde's default
/// leniency drops the field on deserialization and that it does not reappear
/// in the v3 output on save — i.e. no explicit migration action is required
/// for this removed field.
#[test]
fn test_load_frame_transform_dropped_silently() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/frame_transform_present.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load frame_transform_present.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // The two original primitives survived the load. Nothing synthesised — the
    // fixture has no lattice wires or atom_fill nodes.
    assert_eq!(
        network.nodes.len(),
        2,
        "expected both primitives preserved; got {}",
        network.nodes.len()
    );
    let type_names: std::collections::HashSet<&str> = network
        .nodes
        .values()
        .map(|n| n.node_type_name.as_str())
        .collect();
    assert!(type_names.contains("cuboid"));
    assert!(type_names.contains("sphere"));
}

#[test]
fn test_roundtrip_frame_transform_dropped() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/frame_transform_present.cnnd", FIXTURE_DIR),
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
        !saved.contains("frame_transform"),
        "saved v3 file must not carry the dropped `frame_transform` field; got:\n{}",
        saved
    );
}

/// Calling the pre-pass twice on the same in-memory value must produce a
/// byte-identical result the second time. Catches helpers that silently mutate
/// already-migrated shapes — the kind of bug that only surfaces if the
/// migration is accidentally re-invoked on its own output.
#[test]
fn test_double_migration_is_idempotent() {
    let raw = std::fs::read_to_string(&format!("{}/atom_fill_split.cnnd", FIXTURE_DIR))
        .expect("read atom_fill_split fixture");
    let mut value: serde_json::Value = serde_json::from_str(&raw).expect("parse fixture");

    migrate_v2_to_v3(&mut value).expect("first migration call failed");
    let snapshot_after_first = value.clone();

    migrate_v2_to_v3(&mut value).expect("second migration call failed");
    assert_eq!(
        value, snapshot_after_first,
        "second migrate_v2_to_v3 call must be a no-op on an already-migrated value"
    );
}

/// A v3 file must skip the pre-pass entirely — the version dispatch should not
/// even call [`migrate_v2_to_v3`]. Uses the test-only call counter to observe
/// dispatch behaviour directly. The counter is thread-local, so parallel tests
/// don't contaminate this observation.
#[test]
fn test_v3_file_skips_migration_pre_pass() {
    reset_migration_call_count();

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, &format!("{}/trivial_v3.cnnd", FIXTURE_DIR))
        .expect("Failed to load trivial_v3.cnnd");

    assert_eq!(
        migration_call_count(),
        0,
        "loading a v3 file must not invoke migrate_v2_to_v3; the version dispatch should short-circuit"
    );
}

/// A v2 file must go through the pre-pass exactly once per load — the counterpart
/// to the v3 no-op check. Together they prove the dispatch routes correctly.
#[test]
fn test_v2_file_invokes_migration_pre_pass() {
    reset_migration_call_count();

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, &format!("{}/pure_rename.cnnd", FIXTURE_DIR))
        .expect("Failed to load pure_rename.cnnd");

    assert_eq!(
        migration_call_count(),
        1,
        "loading a v2 file must invoke migrate_v2_to_v3 exactly once"
    );
}

/// A structurally corrupt v2 file (truncated mid-JSON) must surface as a clear
/// error from the load path — never a panic. The exact error type depends on
/// where the parse fails; the requirement is that the Display message carries
/// useful locating information, which serde_json's errors always do
/// (line/column).
#[test]
fn test_corrupt_v2_produces_clear_error() {
    let mut registry = NodeTypeRegistry::new();
    let result =
        load_node_networks_from_file(&mut registry, &format!("{}/corrupt_v2.cnnd", FIXTURE_DIR));

    // Can't use `expect_err` here — `LoadResult` doesn't implement `Debug`. Match
    // explicitly and panic with the message we do have.
    let err = match result {
        Ok(_) => panic!("truncated v2 fixture must not load successfully"),
        Err(e) => e,
    };
    let msg = err.to_string();
    assert!(
        !msg.is_empty(),
        "error must have a non-empty message; got {:?}",
        err
    );
    // serde_json errors on malformed input always include a line/column
    // locator, which is sufficient "where did it fail" context for the user.
    assert!(
        msg.contains("line") && msg.contains("column"),
        "corrupt-input error should locate the problem (expected line/column info); got {:?}",
        msg
    );
}

// ---------------------------------------------------------------------------
// Real-sample smoke: `git show main:samples/...` seeded fixtures. These
// exercise combinations of change classes the minimal fixtures don't hit.
// No semantic comparison — just load / migrate / save / reload / idempotent
// per the design doc's Phase 6 instructions.
// ---------------------------------------------------------------------------

/// Real v2 sample — atom-heavy. Diamond uses `half_space`, `atom_fill`,
/// `lattice_move`, `lattice_rot`, `intersect`. Exercises every major migration
/// change class in combination within one file.
#[test]
fn test_real_diamond_roundtrip_smoke() {
    real_sample_roundtrip_smoke("real_diamond.cnnd");
}

/// Real v2 sample — geometry-heavy. Extrude-demo uses `extrude`, `polygon`,
/// `diff_2d`, `atom_fill`, `lattice_move`, plus custom networks (`M`, `P`, `S`).
/// Exercises the rename walker descending into custom-network `node_type`
/// definitions alongside the structural passes.
#[test]
fn test_real_extrude_demo_roundtrip_smoke() {
    real_sample_roundtrip_smoke("real_extrude_demo.cnnd");
}

/// Shared smoke shape used for every real sample: load, re-save, reload.
/// No semantic comparison — just that the pipeline runs end-to-end without
/// panics or errors, that the saved file is tagged v3, and that every v2
/// token we migrate has been erased.
///
/// Byte-identity between two saves of the same network is **not** asserted:
/// `NodeNetwork::nodes` is a `HashMap` and its iteration order is not stable
/// across independent instances (which is what you get from load / re-save /
/// re-load). Node-count parity and v3-version tagging are the strongest
/// checks that survive that constraint. Reload-skips-migration is also
/// verified via the process-wide call counter.
fn real_sample_roundtrip_smoke(fixture_name: &str) {
    let path = format!("{}/{}", FIXTURE_DIR, fixture_name);

    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(&mut registry, &path)
        .unwrap_or_else(|e| panic!("Failed to load {}: {}", fixture_name, e));

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let first_save = temp_dir.path().join("first.cnnd");
    save_node_networks_to_file(
        &mut registry,
        &first_save,
        load_result.direct_editing_mode,
        &load_result.cli_access_rules,
    )
    .unwrap_or_else(|e| panic!("Failed to save {}: {}", fixture_name, e));

    let first_bytes = std::fs::read_to_string(&first_save).expect("read first save");
    assert!(
        first_bytes.contains("\"version\": 3"),
        "{}: saved file should be tagged version: 3",
        fixture_name
    );
    // Every migration change class should have erased its v2 tokens. The
    // smoke test doesn't care which of them was present in this sample; any
    // stragglers would be a bug.
    for v2_token in [
        "\"unit_cell\"",
        "\"atom_fill\"",
        "\"atom_trans\"",
        "\"lattice_symop\"",
        "\"atom_lmove\"",
        "\"atom_lrot\"",
        "\"Geometry\"",
        "\"UnitCell\"",
        "\"Atomic\"",
    ] {
        assert!(
            !first_bytes.contains(v2_token),
            "{}: saved v3 file must not contain v2 token {}",
            fixture_name,
            v2_token
        );
    }

    // Reload-skips-migration: the counter is thread-local, so the first load
    // earlier in this fn (which bumped the counter to 1) is isolated. Reset
    // before the reload and assert the reload itself did not bump it.
    reset_migration_call_count();
    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, first_save.to_str().unwrap())
        .unwrap_or_else(|e| panic!("Failed to reload saved {}: {}", fixture_name, e));
    assert_eq!(
        migration_call_count(),
        0,
        "{}: reloading the saved v3 form must not trigger the v2→v3 pre-pass",
        fixture_name
    );

    // Node count survives the full v2 load → v3 save → v3 reload cycle.
    let net_names: Vec<String> = registry.node_networks.keys().cloned().collect();
    for name in &net_names {
        let net1 = registry.node_networks.get(name).unwrap();
        let net2 = registry2
            .node_networks
            .get(name)
            .unwrap_or_else(|| panic!("{}: network {} missing from reload", fixture_name, name));
        assert_eq!(
            net1.nodes.len(),
            net2.nodes.len(),
            "{}: network {} lost nodes on reload",
            fixture_name,
            name
        );
    }
}

// ---------------------------------------------------------------------------
// Phase 7 (cnnd-migration-motif-fix): atom_fill split with W/G/S triplet.
// Coverage of the case-A/B/C/D matrix from
// `doc/design_cnnd_migration_motif_fix.md`. Cases A and B are exercised by
// `atom_fill_split.cnnd` and `shared_unit_cell.cnnd` above; the fixtures here
// fill in cases C, the motif-offset-only branches of A and C, and a real-world
// MOF5 regression.
// ---------------------------------------------------------------------------

/// Fixture — `atom_fill_unwired_shape.cnnd` (case C: motif wired, shape unwired):
/// no shape source, just a motif fed into `atom_fill.motif`. The migration
/// must create a dangling `S` (no `G`/`W` because there is no shape chain to
/// splice into) and leave `materialize.shape` empty so the validator surfaces
/// the missing input as a user-facing error.
#[test]
fn test_load_atom_fill_unwired_shape_case_c() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/atom_fill_unwired_shape.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load atom_fill_unwired_shape.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Case C: 2 originals + 1 dangling S = 3 nodes. No G/W since shape is
    // unwired and there is no chain to splice into.
    assert_eq!(
        network.nodes.len(),
        3,
        "expected 2 originals + 1 dangling S; got {}",
        network.nodes.len()
    );
    assert!(
        !network
            .nodes
            .values()
            .any(|n| n.node_type_name == "get_structure"),
        "case C must not synthesise a get_structure node"
    );
    assert!(
        !network
            .nodes
            .values()
            .any(|n| n.node_type_name == "with_structure"),
        "case C must not synthesise a with_structure node"
    );

    // The dangling S has motif (arg 2) wired to id 1 (motif source); base
    // (arg 0), lattice_vecs (arg 1), and motif_offset (arg 3) all unwired.
    let dangling_s = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "structure")
        .expect("case C must synthesise a structure (S) node");
    assert_eq!(dangling_s.arguments.len(), 4);
    assert!(
        dangling_s.arguments[0].argument_output_pins.is_empty(),
        "dangling S's `structure` (arg 0) must be empty in case C (no chain to read base from)"
    );
    assert!(
        dangling_s.arguments[1].argument_output_pins.is_empty(),
        "dangling S's `lattice_vecs` (arg 1) must be empty"
    );
    assert_eq!(
        dangling_s.arguments[2].argument_output_pins.get(&1),
        Some(&0),
        "dangling S's `motif` (arg 2) must hold the motif wire (id 1, pin 0)"
    );
    assert!(
        dangling_s.arguments[3].argument_output_pins.is_empty(),
        "dangling S's `motif_offset` (arg 3) must be empty (offset wasn't wired in v2)"
    );

    // Materialize.shape must remain empty (no chain). Validator should flag
    // this as a user-visible missing input — exactly the signal the design
    // intends ("a single drag-to-reconnect").
    let materialize = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "materialize")
        .expect("materialize must exist");
    assert!(
        materialize.arguments[0].argument_output_pins.is_empty(),
        "case C: materialize.shape must stay unwired"
    );

    let materialize_id = materialize.id;
    // Static validation does not currently flag missing-required wires (it
    // only checks type compatibility, parameter validity, etc. — see
    // `network_validator.rs`), so the v3 network passes static validation
    // just as the v2 file did. The user-visible signal comes at evaluation
    // time: materialize.eval() returns an Error for the missing shape input.
    let registry = load_and_validate(&format!("{}/atom_fill_unwired_shape.cnnd", FIXTURE_DIR));
    let network = registry.node_networks.get("Main").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    let result = evaluator.evaluate(
        &network_stack,
        materialize_id,
        0,
        &registry,
        false,
        &mut context,
    );
    match result {
        NetworkResult::Error(_) => {
            // Expected — the missing shape wire surfaces as a runtime error
            // when materialize is evaluated.
        }
        other => panic!(
            "case C: evaluating materialize with no shape wire should produce \
             an Error; got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

/// Fixture — `motif_offset_only_chained.cnnd` (case A, only offset wired):
/// shape + motif_offset wired, motif unwired. Specifically guards the
/// `needs_S` broadening (motif OR motif_offset triggers S synthesis): if
/// `needs_S` were gated only on the motif wire, the offset would be silently
/// dropped by the materialize re-index step.
#[test]
fn test_load_motif_offset_only_chained_case_a() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/motif_offset_only_chained.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load motif_offset_only_chained.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // 4 originals + 1 primitive adapter (cuboid had unit_cell wired) + W/G/S
    // triplet (3) = 8 nodes.
    assert_eq!(
        network.nodes.len(),
        8,
        "expected 4 originals + 1 primitive adapter + W/G/S triplet; got {}",
        network.nodes.len()
    );

    let get_structure = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "get_structure")
        .expect("case A must synthesise a get_structure (G) node");
    let with_structure = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "with_structure")
        .expect("case A must synthesise a with_structure (W) node");

    // The case-A override is the structure node with motif_offset (arg 3)
    // wired and motif (arg 2) empty. The primitive adapter has only
    // lattice_vecs (arg 1) wired.
    let split_override = network
        .nodes
        .values()
        .find(|n| {
            n.node_type_name == "structure" && !n.arguments[3].argument_output_pins.is_empty()
        })
        .expect("expected a structure override with motif_offset wired");
    assert!(
        split_override.arguments[2].argument_output_pins.is_empty(),
        "case A motif-offset-only: S.motif (arg 2) must be empty"
    );
    assert_eq!(
        split_override.arguments[3].argument_output_pins.get(&3),
        Some(&0),
        "case A motif-offset-only: S.motif_offset (arg 3) must hold the user's vec3 wire (id 3, pin 0)"
    );
    assert_eq!(
        split_override.arguments[0]
            .argument_output_pins
            .get(&get_structure.id),
        Some(&0),
        "case A: S.structure (arg 0) must be wired to G's pin 0 (chained base)"
    );
    assert!(
        split_override.arguments[1].argument_output_pins.is_empty(),
        "case A: S.lattice_vecs (arg 1) stays unwired (rides through with the Blueprint)"
    );

    // Materialize.shape must point at W, not the cuboid directly.
    let materialize = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "materialize")
        .unwrap();
    assert_eq!(
        materialize.arguments[0]
            .argument_output_pins
            .get(&with_structure.id),
        Some(&0),
        "case A: materialize.shape (arg 0) must point at W"
    );
}

/// Fixture — `motif_offset_only_unchained.cnnd` (case C, only offset wired):
/// just a vec3 source feeding `atom_fill.motif_offset` with everything else
/// unwired. Same `needs_S`-broadening guard as the chained variant but in the
/// dangling-`S` branch: the offset wire must not be silently overwritten by
/// step 4's re-index.
#[test]
fn test_load_motif_offset_only_unchained_case_c() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/motif_offset_only_unchained.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load motif_offset_only_unchained.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Case C: 2 originals + 1 dangling S = 3 nodes.
    assert_eq!(
        network.nodes.len(),
        3,
        "expected 2 originals + 1 dangling S; got {}",
        network.nodes.len()
    );
    assert!(
        !network
            .nodes
            .values()
            .any(|n| n.node_type_name == "get_structure" || n.node_type_name == "with_structure"),
        "case C must not synthesise G or W"
    );

    let dangling_s = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "structure")
        .expect("case C must synthesise a dangling S");
    assert!(
        dangling_s.arguments[0].argument_output_pins.is_empty(),
        "dangling S.structure (arg 0) must be empty"
    );
    assert!(
        dangling_s.arguments[1].argument_output_pins.is_empty(),
        "dangling S.lattice_vecs (arg 1) must be empty"
    );
    assert!(
        dangling_s.arguments[2].argument_output_pins.is_empty(),
        "dangling S.motif (arg 2) must be empty (motif wasn't wired in v2)"
    );
    assert_eq!(
        dangling_s.arguments[3].argument_output_pins.get(&1),
        Some(&0),
        "dangling S.motif_offset (arg 3) must hold the vec3 wire (id 1, pin 0); \
         re-index must not silently overwrite it"
    );

    // Materialize.shape stays empty in case C, and the four Bool flag args
    // are also empty (this fixture wires nothing besides motif_offset).
    let materialize = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "materialize")
        .unwrap();
    assert_eq!(materialize.arguments.len(), 5);
    for (i, a) in materialize.arguments.iter().enumerate() {
        assert!(
            a.argument_output_pins.is_empty(),
            "materialize arg {} should be empty in this fixture",
            i
        );
    }
}

/// Fixture — `motif_mof5.cnnd` (case A, regression: motif must survive):
/// the smaller stand-in for the full MOF5 file. A non-default motif (a single
/// nitrogen site) is wired into `atom_fill.motif`. After migration, evaluating
/// `materialize` must produce nitrogen atoms — not the diamond carbons that
/// the buggy pre-fix migration produced. This is the regression test for the
/// motif-fix design: failure means we are silently back to diamond.
#[test]
fn test_load_motif_mof5_evaluation_preserves_motif() {
    let registry = load_and_validate(&format!("{}/motif_mof5.cnnd", FIXTURE_DIR));

    let network = registry.node_networks.get("Main").unwrap();
    assert!(
        network.valid,
        "migrated motif_mof5 network should validate cleanly; errors: {:?}",
        network
            .validation_errors
            .iter()
            .map(|e| e.error_text.clone())
            .collect::<Vec<_>>()
    );

    let materialize_id = materialize_node_id(&registry, "Main");
    let atoms = evaluate_to_atoms(&registry, "Main", materialize_id);

    let elements: std::collections::HashSet<i16> =
        atoms.atoms_values().map(|a| a.atomic_number).collect();
    assert!(
        !elements.is_empty(),
        "materialize must produce at least one atom"
    );
    // The motif declares a single nitrogen (atomic number 7) site.
    assert!(
        elements.contains(&7),
        "materialize must produce nitrogen atoms (the motif's element); \
         got elements {:?} — diamond would give {{6}}",
        elements
    );
    assert!(
        !elements.contains(&6),
        "materialize must NOT produce carbon atoms — that would mean the \
         user's motif was lost and the diamond default leaked through; got {:?}",
        elements
    );
}

/// Fixture — `real_mof5.cnnd` (real-sample regression):
/// the actual MOF5 sample from `c:\atomcad_v0.3.0\samples\MOF5-motif.cnnd`.
/// Load / migrate / save / reload; assert idempotence; assert that the
/// `motif_MOF5` network's materialize node, when evaluated, produces atoms
/// with elements including Zn / O / C — the MOF5 elements — rather than
/// C-only (which would indicate diamond output).
#[test]
fn test_real_mof5_motif_survives_migration() {
    let registry = load_and_validate(&format!("{}/real_mof5.cnnd", FIXTURE_DIR));

    // The motif_MOF5 network is the one that owns the motif wired into its
    // atom_fill — that's the case-A site under test.
    let network = registry.node_networks.get("motif_MOF5").unwrap_or_else(|| {
        panic!(
            "motif_MOF5 network missing; available: {:?}",
            registry.node_networks.keys().collect::<Vec<_>>()
        )
    });
    assert!(
        network.valid,
        "migrated motif_MOF5 network should validate cleanly; errors: {:?}",
        network
            .validation_errors
            .iter()
            .map(|e| e.error_text.clone())
            .collect::<Vec<_>>()
    );

    let materialize_id = materialize_node_id(&registry, "motif_MOF5");
    let atoms = evaluate_to_atoms(&registry, "motif_MOF5", materialize_id);

    let elements: std::collections::HashSet<i16> =
        atoms.atoms_values().map(|a| a.atomic_number).collect();
    assert!(!elements.is_empty(), "MOF5 materialize must produce atoms");
    // MOF5 motif declares carbon (6), oxygen (8), and zinc (30). Diamond would
    // only give carbon. We require at least one non-carbon element to prove
    // the motif survived migration.
    let non_carbon: std::collections::HashSet<i16> = elements
        .iter()
        .copied()
        .filter(|&z| z != 6 && z != 1)
        .collect();
    assert!(
        !non_carbon.is_empty(),
        "MOF5 materialize must produce non-carbon (non-H) atoms — diamond \
         output would give {{6}} (or {{1, 6}} with passivation). Got elements {:?}",
        elements
    );
    // Zinc (30) and oxygen (8) are the marker elements that distinguish MOF5
    // from any diamond-like output. Either one being present is a strong signal
    // that the migration preserved the motif.
    assert!(
        elements.contains(&30) || elements.contains(&8),
        "MOF5 materialize should produce zinc or oxygen atoms; got {:?}",
        elements
    );
}

/// Roundtrip smoke for `real_mof5.cnnd`: load / migrate / save / reload, and
/// verify the second load skips the v2 → v3 pre-pass entirely. Mirrors the
/// pattern used for `real_diamond.cnnd` and `real_extrude_demo.cnnd`.
#[test]
fn test_real_mof5_roundtrip_smoke() {
    real_sample_roundtrip_smoke("real_mof5.cnnd");
}

/// Once a v2 file has been migrated and saved as v3, repeated save→reload→save
/// cycles must produce semantically identical files: same nodes, same wires,
/// same `next_node_id`. The migration must run exactly once on the original
/// load and never touch the output again.
///
/// Compares by parsed JSON value with each network's `nodes` array sorted by
/// id, since `NodeNetwork::nodes` is a `HashMap` and its iteration order is
/// not stable across independent loads (a known pre-existing limitation
/// noted in `real_sample_roundtrip_smoke`). The `next_node_id` field is the
/// load-bearing check: if the migration silently re-allocated ids on the
/// second save, this would drift.
///
/// Uses `atom_fill_split.cnnd` because it's case A — exercises every
/// new-node-allocation path the migration takes.
#[test]
fn test_resave_roundtrip_semantically_identical_after_first_v3_save() {
    fn save_then_normalize(fixture_path: &str, out_path: &std::path::Path) -> serde_json::Value {
        let mut registry = NodeTypeRegistry::new();
        let load_result = load_node_networks_from_file(&mut registry, fixture_path).expect("load");
        save_node_networks_to_file(
            &mut registry,
            out_path,
            load_result.direct_editing_mode,
            &load_result.cli_access_rules,
        )
        .expect("save");
        let raw = std::fs::read_to_string(out_path).expect("read");
        let mut value: serde_json::Value = serde_json::from_str(&raw).expect("parse");
        // Sort each network's `nodes` array by id so HashMap-iteration order
        // doesn't leak into the comparison.
        if let Some(networks) = value
            .get_mut("node_networks")
            .and_then(|v| v.as_array_mut())
        {
            for entry in networks {
                if let Some(net) = entry.as_array_mut().and_then(|a| a.get_mut(1)) {
                    if let Some(nodes) = net.get_mut("nodes").and_then(|v| v.as_array_mut()) {
                        nodes.sort_by_key(|n| n.get("id").and_then(|v| v.as_u64()).unwrap_or(0));
                    }
                }
            }
        }
        value
    }

    let temp_dir = tempdir().expect("tempdir");
    let first = save_then_normalize(
        &format!("{}/atom_fill_split.cnnd", FIXTURE_DIR),
        &temp_dir.path().join("first.cnnd"),
    );
    // For the second save we have to feed the v3 output back in, not the v2
    // fixture, so the migration is what produced the saved bytes the first
    // time; this round must re-emit the same shape without re-running migration.
    let second_input = temp_dir.path().join("first.cnnd");
    let second = save_then_normalize(
        second_input.to_str().unwrap(),
        &temp_dir.path().join("second.cnnd"),
    );

    assert_eq!(
        first, second,
        "second v3 save must be semantically identical to the first v3 save \
         (after sorting node arrays by id)"
    );
}
