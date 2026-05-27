// v4 → v5 migration tests (Phase 1: scaffolding only).
//
// Phase 1 ships:
// - the version bump from 4 to 5 in `node_networks_serialization.rs`
// - the chained-dispatch entry `if version < 5 { migrate_v4_to_v5(...) }`
// - the `migrate_v4_to_v5` module itself as an inert pre-pass (no per-network work)
//
// Tests in this file assert only the dispatch behaviour. Phase 2 will add
// detection + skip-with-warning tests; Phase 3 will add the closure-synthesis
// tests against the `simple_*_with_capture.cnnd` fixtures.
//
// Pattern modelled after `tests/integration/iterator_migration_test.rs`.

use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::migrate_v4_to_v5::{
    migration_call_count, reset_migration_call_count,
};
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::load_node_networks_from_file;

const FIXTURE_DIR: &str = "tests/fixtures/zones_migration";

// ---------------------------------------------------------------------------
// Phase 1: a v5 file skips the migration pass entirely (`version < 5` guard).
// ---------------------------------------------------------------------------

#[test]
fn test_v5_file_skips_migration() {
    reset_migration_call_count();

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, &format!("{}/already_v5.cnnd", FIXTURE_DIR))
        .expect("v5 fixture failed to load");

    assert_eq!(
        migration_call_count(),
        0,
        "a v5-stamped file must skip the v4→v5 migration pass"
    );

    // Sanity: the network still loads.
    assert!(
        registry.node_networks.contains_key("Main"),
        "Main network missing after load"
    );
}

// ---------------------------------------------------------------------------
// Phase 1: a v4 file triggers the migration pass exactly once, and a file
// with no HOFs round-trips unchanged through the inert pre-pass (Phase 1
// touches nothing per network — every node and wire survives identically).
// ---------------------------------------------------------------------------

#[test]
fn test_v4_file_triggers_migration() {
    reset_migration_call_count();

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, &format!("{}/v4_no_hofs.cnnd", FIXTURE_DIR))
        .expect("v4 fixture failed to load");

    assert_eq!(
        migration_call_count(),
        1,
        "a v4-stamped file must invoke the v4→v5 migration pass exactly once"
    );

    // Round-trips unchanged: the fixture has no HOFs, so the inert Phase 1
    // pre-pass touches nothing. Both nodes survive with their original ids
    // and types.
    let main = registry
        .node_networks
        .get("Main")
        .expect("Main network missing after migration");

    assert_eq!(
        main.nodes.len(),
        2,
        "no-HOF v4 fixture must round-trip with its original node count"
    );
    assert_eq!(
        main.nodes
            .get(&1)
            .expect("node 1 missing after migration")
            .node_type_name,
        "sphere",
        "node 1 must remain a sphere"
    );
    assert_eq!(
        main.nodes
            .get(&2)
            .expect("node 2 missing after migration")
            .node_type_name,
        "cuboid",
        "node 2 must remain a cuboid"
    );
}

// ---------------------------------------------------------------------------
// Phase 2: legacy HOF.f wires fall into one of three buckets — `NoOp`,
// `ClosureWrap`, or `Skip`. Phase 2 emits warnings for `ClosureWrap` and
// `Skip` and never mutates the JSON; Phase 3 will replace the `ClosureWrap`
// warning with the real closure-wrapping transformation.
//
// All three fixtures here load without crashing; the file remains
// post-migration-broken on the wires that fall into `ClosureWrap` or `Skip`
// (the user fixes those interactively or via Phase 3 once it lands). These
// tests stay valid in Phase 3 — the skip-with-warning branch is permanent;
// only the `simple_*_with_capture` assertion will be replaced.
// ---------------------------------------------------------------------------

#[test]
fn test_phase2_simple_map_with_capture_loads_without_crashing() {
    reset_migration_call_count();

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/simple_map_with_capture.cnnd", FIXTURE_DIR),
    )
    .expect("simple_map_with_capture v4 fixture failed to load");

    assert_eq!(
        migration_call_count(),
        1,
        "v4 file must invoke the v4→v5 migration pass exactly once"
    );

    // Phase 2 leaves the JSON untouched — the `map` node still has its
    // original argument count, and the network still contains exactly the
    // five hand-authored nodes (no closure synthesized yet). Phase 3 will
    // replace this assertion with the full migration check.
    let main = registry
        .node_networks
        .get("Main")
        .expect("Main network missing after migration");
    assert_eq!(
        main.nodes.len(),
        5,
        "Phase 2 must not synthesize anything — fixture should keep its 5 originals"
    );
    assert!(
        main.nodes.values().any(|n| n.node_type_name == "map"),
        "map node must still be present after Phase 2"
    );
    assert!(
        !main.nodes.values().any(|n| n.node_type_name == "closure"),
        "Phase 2 must not synthesize a closure node"
    );
}

#[test]
fn test_phase2_bad_wired_after_unwired_loads_without_crashing() {
    reset_migration_call_count();

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/bad_wired_after_unwired.cnnd", FIXTURE_DIR),
    )
    .expect("bad_wired_after_unwired v4 fixture failed to load");

    assert_eq!(
        migration_call_count(),
        1,
        "v4 file must invoke the v4→v5 migration pass exactly once"
    );

    // No mutation in Phase 2; this same assertion is permanent (the Skip
    // branch never synthesizes anything, even in Phase 3).
    let main = registry
        .node_networks
        .get("Main")
        .expect("Main network missing after migration");
    assert!(
        !main.nodes.values().any(|n| n.node_type_name == "closure"),
        "Skip branch must never synthesize a closure node"
    );
}

#[test]
fn test_phase2_bad_too_few_inputs_loads_without_crashing() {
    reset_migration_call_count();

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/bad_too_few_inputs.cnnd", FIXTURE_DIR),
    )
    .expect("bad_too_few_inputs v4 fixture failed to load");

    assert_eq!(
        migration_call_count(),
        1,
        "v4 file must invoke the v4→v5 migration pass exactly once"
    );

    let main = registry
        .node_networks
        .get("Main")
        .expect("Main network missing after migration");
    assert!(
        !main.nodes.values().any(|n| n.node_type_name == "closure"),
        "Skip branch must never synthesize a closure node"
    );
}
