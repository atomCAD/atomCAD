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
// Phase 4: `map → array_at` is rewritten as `map → collect → array_at`.
// `range → map.xs` stays direct because `map.xs` accepts `Iter[T]` natively.
// ---------------------------------------------------------------------------

#[test]
fn test_load_old_map_to_array_at_inserts_collect() {
    let registry = load_and_validate(&format!("{}/old_map_to_array_at.cnnd", FIXTURE_DIR));

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Five nodes after migration: range, map, int, array_at, collect.
    assert_eq!(
        network.nodes.len(),
        5,
        "expected 5 nodes (4 originals + synthesised collect); got {:?}",
        network
            .nodes
            .values()
            .map(|n| n.node_type_name.as_str())
            .collect::<Vec<_>>()
    );

    let map_id = find_node_id_by_type(&registry, "Main", "map");
    let array_at_id = find_node_id_by_type(&registry, "Main", "array_at");
    let collect_id = find_node_id_by_type(&registry, "Main", "collect");

    // Synthesised collect wires range/map → collect → array_at.
    let collect_node = network.nodes.get(&collect_id).unwrap();
    assert_eq!(collect_node.arguments.len(), 1);
    assert_eq!(
        collect_node.arguments[0]
            .argument_output_pins
            .get(&map_id)
            .copied(),
        Some(0),
        "collect must be wired to map's pin 0"
    );

    // array_at's `array` arg now points at collect, not map.
    let array_at_node = network.nodes.get(&array_at_id).unwrap();
    assert_eq!(
        array_at_node.arguments[0]
            .argument_output_pins
            .get(&collect_id)
            .copied(),
        Some(0),
        "array_at.array must point at the synthesised collect"
    );
    assert!(
        !array_at_node.arguments[0]
            .argument_output_pins
            .contains_key(&map_id),
        "array_at must no longer wire directly to map"
    );

    // range → map.xs stays direct: map.xs is now Iter[Int], so the wire is
    // identity-valid; no collect inserted on it.
    let map_node = network.nodes.get(&map_id).unwrap();
    let range_id = find_node_id_by_type(&registry, "Main", "range");
    assert_eq!(
        map_node.arguments[0]
            .argument_output_pins
            .get(&range_id)
            .copied(),
        Some(0),
        "range → map.xs wire must be preserved unchanged (Iter[Int] → Iter[Int] identity)"
    );
}

// ---------------------------------------------------------------------------
// Phase 4: `filter → array_concat` is rewritten as
// `filter → collect → array_concat`. `range → filter.xs` stays direct.
// ---------------------------------------------------------------------------

#[test]
fn test_load_old_filter_to_array_concat_inserts_collect() {
    let registry = load_and_validate(&format!("{}/old_filter_to_array_concat.cnnd", FIXTURE_DIR));

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    assert_eq!(
        network.nodes.len(),
        4,
        "expected 4 nodes (3 originals + synthesised collect); got {:?}",
        network
            .nodes
            .values()
            .map(|n| n.node_type_name.as_str())
            .collect::<Vec<_>>()
    );

    let filter_id = find_node_id_by_type(&registry, "Main", "filter");
    let array_concat_id = find_node_id_by_type(&registry, "Main", "array_concat");
    let collect_id = find_node_id_by_type(&registry, "Main", "collect");

    let collect_node = network.nodes.get(&collect_id).unwrap();
    assert_eq!(
        collect_node.arguments[0]
            .argument_output_pins
            .get(&filter_id)
            .copied(),
        Some(0),
        "collect must be wired to filter's pin 0"
    );

    let array_concat_node = network.nodes.get(&array_concat_id).unwrap();
    assert_eq!(
        array_concat_node.arguments[0]
            .argument_output_pins
            .get(&collect_id)
            .copied(),
        Some(0),
        "array_concat.a must point at the synthesised collect"
    );
}

// ---------------------------------------------------------------------------
// Phase 4: fan-out from one `map` source pin to two non-iterator consumers
// synthesises **one collect per consumer** (per-consumer adapters, not a
// shared collect) — matches v2→v3's policy and the design doc's
// "Fan-out from one source pin to multiple non-iterator destinations" rule.
// ---------------------------------------------------------------------------

#[test]
fn test_load_old_double_fanout_inserts_one_collect_per_consumer() {
    let registry = load_and_validate(&format!("{}/old_double_fanout.cnnd", FIXTURE_DIR));

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Original 5 + 2 synthesised collects = 7.
    assert_eq!(
        network.nodes.len(),
        7,
        "expected 7 nodes (5 originals + 2 synthesised collects, one per consumer); got {:?}",
        network
            .nodes
            .values()
            .map(|n| n.node_type_name.as_str())
            .collect::<Vec<_>>()
    );

    // Two collects.
    let collect_ids: Vec<u64> = network
        .nodes
        .values()
        .filter(|n| n.node_type_name == "collect")
        .map(|n| n.id)
        .collect();
    assert_eq!(
        collect_ids.len(),
        2,
        "expected exactly two collect nodes (one per fan-out consumer); got {}",
        collect_ids.len()
    );

    let map_id = find_node_id_by_type(&registry, "Main", "map");
    let array_at_id = find_node_id_by_type(&registry, "Main", "array_at");
    let array_len_id = find_node_id_by_type(&registry, "Main", "array_len");

    // Both collects must be wired to map's pin 0.
    for &cid in &collect_ids {
        let cnode = network.nodes.get(&cid).unwrap();
        assert_eq!(
            cnode.arguments[0]
                .argument_output_pins
                .get(&map_id)
                .copied(),
            Some(0),
            "collect {} must be wired to map's pin 0",
            cid
        );
    }

    // array_at and array_len each point at *some* collect; the consumers must
    // not share the same collect.
    let at_target = *array_at_node_array_target(network, array_at_id, &collect_ids);
    let len_target = *array_len_node_array_target(network, array_len_id, &collect_ids);
    assert_ne!(
        at_target, len_target,
        "fan-out must produce a per-consumer collect; got the same collect for both consumers"
    );
}

fn array_at_node_array_target<'a>(
    network: &'a rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork,
    array_at_id: u64,
    collect_ids: &'a [u64],
) -> &'a u64 {
    let node = network.nodes.get(&array_at_id).unwrap();
    let pins = &node.arguments[0].argument_output_pins;
    collect_ids
        .iter()
        .find(|cid| pins.contains_key(cid))
        .expect("array_at.array must point at one of the synthesised collects")
}

fn array_len_node_array_target<'a>(
    network: &'a rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork,
    array_len_id: u64,
    collect_ids: &'a [u64],
) -> &'a u64 {
    let node = network.nodes.get(&array_len_id).unwrap();
    let pins = &node.arguments[0].argument_output_pins;
    collect_ids
        .iter()
        .find(|cid| pins.contains_key(cid))
        .expect("array_len.array must point at one of the synthesised collects")
}

// ---------------------------------------------------------------------------
// Phase 4: predicate (B) recognises `map.xs` and `filter.xs` as iterator
// pins, so a v3 file with `map → map` (or `filter → filter`) is loaded
// without inserting any collect.
// ---------------------------------------------------------------------------

#[test]
fn test_iter_to_iter_xs_pin_skips_collect_insertion() {
    // We don't ship a dedicated fixture for this; reuse `old_map_to_array_at`
    // which has a `range → map.xs` wire in it. Because `map.xs` is in
    // `ITERATOR_PINS_V4` after Phase 4, no collect should sit between range
    // and map.
    let registry = load_and_validate(&format!("{}/old_map_to_array_at.cnnd", FIXTURE_DIR));
    let network = registry.node_networks.get("Main").unwrap();

    // No collect feeds map.xs; map.xs feeds direct from range.
    let map_id = find_node_id_by_type(&registry, "Main", "map");
    let range_id = find_node_id_by_type(&registry, "Main", "range");
    let map_node = network.nodes.get(&map_id).unwrap();
    let xs_pins = &map_node.arguments[0].argument_output_pins;
    assert_eq!(xs_pins.len(), 1, "map.xs must have exactly one source");
    assert!(
        xs_pins.contains_key(&range_id),
        "map.xs must wire directly to range, not via a collect"
    );
}

// ---------------------------------------------------------------------------
// Phase 4: idempotence on the new fixtures (re-running `migrate_v3_to_v4`
// directly on a value that has already been migrated must be a no-op).
// ---------------------------------------------------------------------------

#[test]
fn test_migrate_v3_to_v4_idempotent_on_map_fanout() {
    let raw = std::fs::read_to_string(format!("{}/old_double_fanout.cnnd", FIXTURE_DIR))
        .expect("fixture missing");
    let mut value: serde_json::Value = serde_json::from_str(&raw).expect("invalid JSON");

    migrate_v3_to_v4(&mut value).expect("first migration failed");
    let after_first = serde_json::to_string(&value).unwrap();
    migrate_v3_to_v4(&mut value).expect("second migration failed");
    let after_second = serde_json::to_string(&value).unwrap();

    assert_eq!(
        after_first, after_second,
        "v3→v4 migration must be idempotent on a fan-out fixture"
    );
}

// ---------------------------------------------------------------------------
// Phase 4: declared output type of `map` and `filter` is `Iter[T]`.
// ---------------------------------------------------------------------------

#[test]
fn test_map_output_type_is_iter() {
    let registry = NodeTypeRegistry::new();
    let map_node_type = registry
        .get_node_type("map")
        .expect("map node type missing");
    assert_eq!(
        *map_node_type.output_type(),
        DataType::Iterator(Box::new(DataType::Float)),
        "map's default output must be Iter[Float] after Phase 4"
    );
}

#[test]
fn test_filter_output_type_is_iter() {
    let registry = NodeTypeRegistry::new();
    let filter_node_type = registry
        .get_node_type("filter")
        .expect("filter node type missing");
    assert_eq!(
        *filter_node_type.output_type(),
        DataType::Iterator(Box::new(DataType::Float)),
        "filter's default output must be Iter[Float] after Phase 4"
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
