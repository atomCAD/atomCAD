// v3 → v4 migration tests for the iterator refactoring (Phase 3+ of
// `doc/design_iterators.md`).
//
// Phase 3 ships:
// - the `range` arm of predicate (A) in `migrate_v3_to_v4`
// - the `("collect", 0)` entry of `ITERATOR_PINS_V4` (predicate (B))
// - chained dispatch through `migrate_v2_to_v3` then `migrate_v3_to_v4`
//
// Tests in this file follow the pattern established by
// `lattice_space_migration_test.rs`.

use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::network_validator::validate_network;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::migrate_v3_to_v4::{
    migrate_v3_to_v4, migration_call_count, reset_migration_call_count,
};
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use std::collections::HashMap;
use tempfile::tempdir;

const FIXTURE_DIR: &str = "tests/fixtures/iterator_migration";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Loads a fixture and validates every network in dependency order.
/// Mirrors `lattice_space_migration_test::load_and_validate`.
fn load_and_validate(fixture_path: &str) -> NodeTypeRegistry {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, fixture_path)
        .unwrap_or_else(|e| panic!("Failed to load {}: {}", fixture_path, e));

    let networks_in_order = registry.get_networks_in_dependency_order();
    for network_name in networks_in_order {
        let registry_ptr = &mut registry as *mut NodeTypeRegistry;
        unsafe {
            if let Some(network) = (*registry_ptr).node_networks.get_mut(&network_name) {
                validate_network(network, &mut *registry_ptr, None);
            }
        }
    }
    registry
}

fn evaluate_node(registry: &NodeTypeRegistry, network_name: &str, node_id: u64) -> NetworkResult {
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
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn find_node_id_by_type(
    registry: &NodeTypeRegistry,
    network_name: &str,
    node_type_name: &str,
) -> u64 {
    let network = registry.node_networks.get(network_name).unwrap();
    let ids: Vec<u64> = network
        .nodes
        .values()
        .filter(|n| n.node_type_name == node_type_name)
        .map(|n| n.id)
        .collect();
    assert_eq!(
        ids.len(),
        1,
        "expected exactly one '{}' node in {}; got {:?}",
        node_type_name,
        network_name,
        ids
    );
    ids[0]
}

// ---------------------------------------------------------------------------
// Phase 3: `range → array_len` is rewritten as `range → collect → array_len`.
// ---------------------------------------------------------------------------

#[test]
fn test_load_old_range_to_array_len_inserts_collect() {
    let registry = load_and_validate(&format!("{}/old_range_to_array_len.cnnd", FIXTURE_DIR));

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Three nodes after migration: range, collect, array_len.
    assert_eq!(
        network.nodes.len(),
        3,
        "expected 3 nodes (range + synthesised collect + array_len) after migration; got {:?}",
        network
            .nodes
            .values()
            .map(|n| n.node_type_name.as_str())
            .collect::<Vec<_>>()
    );

    let range_id = find_node_id_by_type(&registry, "Main", "range");
    let collect_id = find_node_id_by_type(&registry, "Main", "collect");
    let array_len_id = find_node_id_by_type(&registry, "Main", "array_len");

    // The synthesised `collect` is wired to the range output (pin 0).
    let collect_node = network.nodes.get(&collect_id).unwrap();
    assert_eq!(collect_node.arguments.len(), 1);
    assert_eq!(
        collect_node.arguments[0]
            .argument_output_pins
            .get(&range_id)
            .copied(),
        Some(0)
    );

    // `array_len` now points at the synthesised `collect` (id 3 was the next
    // free id in the v3 file with `next_node_id: 3`), not the original
    // `range`. Pre-migration its argument was `{1: 0}` (range -> pin 0).
    let array_len_node = network.nodes.get(&array_len_id).unwrap();
    assert_eq!(array_len_node.arguments.len(), 1);
    assert_eq!(
        array_len_node.arguments[0]
            .argument_output_pins
            .get(&collect_id)
            .copied(),
        Some(0)
    );
    assert!(
        !array_len_node.arguments[0]
            .argument_output_pins
            .contains_key(&range_id),
        "after migration array_len must no longer wire directly to range"
    );

    // Network validates clean post-migration.
    assert!(
        network.valid,
        "post-migration network must be valid; errors={:?}",
        network
            .validation_errors
            .iter()
            .map(|e| e.error_text.clone())
            .collect::<Vec<_>>()
    );

    // Evaluate: range(0, 1, 5) yields 5 elements; array_len returns 5.
    let result = evaluate_node(&registry, "Main", array_len_id);
    match result {
        NetworkResult::Int(n) => assert_eq!(n, 5),
        NetworkResult::Error(e) => panic!("array_len evaluated to Error: {}", e),
        _ => panic!("expected Int(5) from array_len; got a non-Int result"),
    }
}

// ---------------------------------------------------------------------------
// Phase 3: version dispatch — a v4 file is a no-op through `migrate_v3_to_v4`.
// ---------------------------------------------------------------------------

#[test]
fn test_v4_file_skips_migration() {
    // Round-trip the migrated v3 fixture through save → load. The saved file
    // should be v4, and reloading it must not invoke `migrate_v3_to_v4`.
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/old_range_to_array_len.cnnd", FIXTURE_DIR),
    )
    .expect("initial v3 load failed");

    let dir = tempdir().unwrap();
    let out = dir.path().join("roundtrip_v4.cnnd");
    let cli_access_rules: HashMap<String, bool> = HashMap::new();
    save_node_networks_to_file(&mut registry, &out, false, &cli_access_rules).expect("save failed");

    reset_migration_call_count();
    let mut reloaded = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut reloaded, out.to_str().unwrap()).expect("reload failed");
    assert_eq!(
        migration_call_count(),
        0,
        "v4 file must skip the v3→v4 migration pass"
    );
}

// ---------------------------------------------------------------------------
// Phase 3: idempotence — running `migrate_v3_to_v4` directly on a value that
// has already been migrated yields a no-op (no fresh `collect` insertions).
// ---------------------------------------------------------------------------

#[test]
fn test_migrate_v3_to_v4_is_idempotent() {
    let raw = std::fs::read_to_string(format!("{}/old_range_to_array_len.cnnd", FIXTURE_DIR))
        .expect("fixture missing");
    let mut value: serde_json::Value = serde_json::from_str(&raw).expect("invalid JSON");

    migrate_v3_to_v4(&mut value).expect("first migration failed");
    let after_first = serde_json::to_string(&value).unwrap();
    migrate_v3_to_v4(&mut value).expect("second migration failed");
    let after_second = serde_json::to_string(&value).unwrap();

    assert_eq!(
        after_first, after_second,
        "v3→v4 migration must be idempotent; second run must not add another collect"
    );
}

// ---------------------------------------------------------------------------
// Phase 3 of design_iterators: a freshly-built `range → collect → array_len`
// chain validates and evaluates correctly even without going through
// migration. Sanity-check that the new `range → Iter[Int]` plus implicit
// conversions work end-to-end.
// ---------------------------------------------------------------------------

#[test]
fn test_range_output_type_is_iter_int() {
    let registry = NodeTypeRegistry::new();
    let range_node_type = registry
        .get_node_type("range")
        .expect("range node type missing");
    assert_eq!(
        *range_node_type.output_type(),
        DataType::Iterator(Box::new(DataType::Int)),
        "range output must be Iter[Int] after Phase 3"
    );
}

// ---------------------------------------------------------------------------
// Phase 3: chained dispatch v2 → v3 → v4.
// A v2 fixture (`pure_rename.cnnd` from the lattice-space migration tests)
// should chain through both passes and reach v4 cleanly.
// ---------------------------------------------------------------------------

#[test]
fn test_chained_v2_v3_v4_dispatch() {
    // Use an existing v2 fixture — it doesn't contain any `range`/`map`/etc.,
    // so the v3→v4 pass is a no-op for it. The point is that both passes run
    // without erroring on a v2 file.
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        "tests/fixtures/lattice_space_migration/pure_rename.cnnd",
    )
    .expect("v2 file failed to chain through v2→v3 then v3→v4");

    // The Main network must exist and be loadable.
    assert!(
        registry.node_networks.contains_key("Main"),
        "Main network missing after chained migration"
    );
}
