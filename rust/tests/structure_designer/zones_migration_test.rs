// v4 → v5 migration tests.
//
// Phases:
// - Phase 1: scaffolding + version-bump dispatch.
// - Phase 2: detection + skip-with-warning (Phase 2 was a stepping stone — the
//   skip-with-warning branch survives, the ClosureWrap-as-warning branch is
//   replaced by the real transformation in Phase 3).
// - Phase 3: closure-wrapping transformation for the trailing-extras case.
//
// Pattern modelled after `tests/integration/iterator_migration_test.rs`.

use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_network::{NodeNetwork, SourcePin};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::serialization::migrate_v4_to_v5::{
    migration_call_count, reset_migration_call_count,
};
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::load_node_networks_from_file;

const FIXTURE_DIR: &str = "tests/fixtures/zones_migration";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn find_node_id_by_type(network: &NodeNetwork, node_type_name: &str) -> u64 {
    let ids: Vec<u64> = network
        .nodes
        .values()
        .filter(|n| n.node_type_name == node_type_name)
        .map(|n| n.id)
        .collect();
    assert_eq!(
        ids.len(),
        1,
        "expected exactly one '{}' node; got {:?}",
        node_type_name,
        ids
    );
    ids[0]
}

fn find_node_ids_by_type(network: &NodeNetwork, node_type_name: &str) -> Vec<u64> {
    let mut ids: Vec<u64> = network
        .nodes
        .values()
        .filter(|n| n.node_type_name == node_type_name)
        .map(|n| n.id)
        .collect();
    ids.sort();
    ids
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
// with no HOFs round-trips unchanged through the pre-pass.
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

    let main = registry
        .node_networks
        .get("Main")
        .expect("Main network missing after migration");

    assert_eq!(
        main.nodes.len(),
        2,
        "no-HOF v4 fixture must round-trip with its original node count"
    );
}

// ---------------------------------------------------------------------------
// Phase 2: the skip-with-warning branch survives — `bad_wired_after_unwired`
// and `bad_too_few_inputs` still load and never synthesise a closure.
// ---------------------------------------------------------------------------

#[test]
fn test_bad_wired_after_unwired_loads_without_synthesis() {
    reset_migration_call_count();

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/bad_wired_after_unwired.cnnd", FIXTURE_DIR),
    )
    .expect("bad_wired_after_unwired v4 fixture failed to load");

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
fn test_bad_too_few_inputs_loads_without_synthesis() {
    reset_migration_call_count();

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/bad_too_few_inputs.cnnd", FIXTURE_DIR),
    )
    .expect("bad_too_few_inputs v4 fixture failed to load");

    let main = registry
        .node_networks
        .get("Main")
        .expect("Main network missing after migration");
    assert!(
        !main.nodes.values().any(|n| n.node_type_name == "closure"),
        "Skip branch must never synthesize a closure node"
    );
}

// ---------------------------------------------------------------------------
// Phase 3: closure synthesis.
// ---------------------------------------------------------------------------

#[test]
fn test_phase3_simple_map_with_capture_synthesises_closure() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/simple_map_with_capture.cnnd", FIXTURE_DIR),
    )
    .expect("simple_map_with_capture v4 fixture failed to load");

    let main = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Original 5 nodes minus the orphaned `vec3` source plus 1 synthesised
    // closure = 5 nodes. The `vec3` was rewritten into the closure body and
    // had no non-`-1` consumers, so cleanup deleted it.
    let closure_ids = find_node_ids_by_type(main, "closure");
    assert_eq!(
        closure_ids.len(),
        1,
        "exactly one closure must be synthesised; got {:?}",
        main.nodes
            .values()
            .map(|n| n.node_type_name.as_str())
            .collect::<Vec<_>>()
    );
    let closure_id = closure_ids[0];

    // `vec3` source was rewritten into the body and has no non-`-1` consumer
    // remaining → cleanup deleted it from the parent network.
    assert!(
        !main.nodes.values().any(|n| n.node_type_name == "vec3"),
        "orphaned vec3 source must be deleted by cleanup_orphan_sources"
    );

    // The map's `f` argument now reads pin 0 of the closure (not `-1` of the
    // original source).
    let map_id = find_node_id_by_type(main, "map");
    let map_node = main.nodes.get(&map_id).unwrap();
    let f_arg = &map_node.arguments[1];
    assert_eq!(f_arg.incoming_wires.len(), 1, "map.f must have one wire");
    let f_wire = &f_arg.incoming_wires[0];
    assert_eq!(f_wire.source_node_id, closure_id);
    assert_eq!(f_wire.source_pin, SourcePin::NodeOutput { pin_index: 0 });
    assert_eq!(f_wire.source_scope_depth, 0);

    // The closure carries the right kind + type_args (read back via the
    // typed ClosureData).
    let closure_node = main.nodes.get(&closure_id).unwrap();
    let closure_data = closure_node
        .data
        .as_any_ref()
        .downcast_ref::<ClosureData>()
        .expect("closure node's data must be ClosureData");
    assert_eq!(closure_data.kind, ClosureKind::Map);
    // type_args[0] = input_type = Float; type_args[1] = output_type = Vec3.
    assert_eq!(
        closure_data.type_args[0].to_string(),
        "Float",
        "Map.type_args[0] must be the input type"
    );
    assert_eq!(
        closure_data.type_args[1].to_string(),
        "Vec3",
        "Map.type_args[1] must be the output type"
    );

    // The closure has a body with exactly one node (the cloned vec3) at id 1.
    let body = closure_node
        .zone
        .as_ref()
        .expect("closure must own a body");
    assert_eq!(body.nodes.len(), 1, "body must contain exactly one node");
    let clone = body
        .nodes
        .get(&1)
        .expect("body's only node must have id 1");
    assert_eq!(clone.node_type_name, "vec3");

    // Main's partial-application convention is parameters-first, captures-
    // last. With map's K=1, the body clone's first pin (args[0] = vec3.x) is
    // the parameter (depth=1 ZoneInput pin 0 reading the closure's zone-
    // input); args[1] = vec3.y and args[2] = vec3.z are captures (depth=1
    // NodeOutput) reaching the parent network's fx (id 1) and fy (id 2).
    let param_x = &clone.arguments[0].incoming_wires[0];
    assert_eq!(
        param_x.source_node_id, closure_id,
        "parameter wire must reach the closure's ZoneInput"
    );
    assert_eq!(param_x.source_pin, SourcePin::ZoneInput { pin_index: 0 });
    assert_eq!(param_x.source_scope_depth, 1);

    let capture_y = &clone.arguments[1].incoming_wires[0];
    assert_eq!(capture_y.source_node_id, 1, "first capture must read fx");
    assert_eq!(
        capture_y.source_pin,
        SourcePin::NodeOutput { pin_index: 0 }
    );
    assert_eq!(capture_y.source_scope_depth, 1);

    let capture_z = &clone.arguments[2].incoming_wires[0];
    assert_eq!(capture_z.source_node_id, 2, "second capture must read fy");
    assert_eq!(
        capture_z.source_pin,
        SourcePin::NodeOutput { pin_index: 0 }
    );
    assert_eq!(capture_z.source_scope_depth, 1);

    // The closure's zone-output reads body-local node 1 pin 0.
    assert_eq!(closure_node.zone_output_arguments.len(), 1);
    let zo = &closure_node.zone_output_arguments[0].incoming_wires[0];
    assert_eq!(zo.source_node_id, 1);
    assert_eq!(zo.source_pin, SourcePin::NodeOutput { pin_index: 0 });
    assert_eq!(zo.source_scope_depth, 0);

    // Post-migration network validates clean.
    assert!(
        main.valid,
        "post-migration network must validate; errors={:?}",
        main.validation_errors
            .iter()
            .map(|e| e.error_text.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_phase3_simple_filter_with_capture_synthesises_closure() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/simple_filter_with_capture.cnnd", FIXTURE_DIR),
    )
    .expect("simple_filter_with_capture v4 fixture failed to load");

    let main = registry.node_networks.get("Main").unwrap();
    let closure_ids = find_node_ids_by_type(main, "closure");
    assert_eq!(closure_ids.len(), 1);

    let closure_data = main
        .nodes
        .get(&closure_ids[0])
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ClosureData>()
        .expect("closure node must carry ClosureData");
    assert_eq!(closure_data.kind, ClosureKind::Filter);
    assert!(main.valid, "filter migration network must validate");
}

#[test]
fn test_phase3_simple_fold_with_capture_synthesises_closure() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/simple_fold_with_capture.cnnd", FIXTURE_DIR),
    )
    .expect("simple_fold_with_capture v4 fixture failed to load");

    let main = registry.node_networks.get("Main").unwrap();
    let closure_ids = find_node_ids_by_type(main, "closure");
    assert_eq!(closure_ids.len(), 1);

    let closure_data = main
        .nodes
        .get(&closure_ids[0])
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ClosureData>()
        .expect("closure node must carry ClosureData");
    assert_eq!(closure_data.kind, ClosureKind::Fold);
    assert!(main.valid, "fold migration network must validate");
}

#[test]
fn test_phase3_simple_foreach_with_capture_synthesises_closure() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/simple_foreach_with_capture.cnnd", FIXTURE_DIR),
    )
    .expect("simple_foreach_with_capture v4 fixture failed to load");

    let main = registry.node_networks.get("Main").unwrap();
    let closure_ids = find_node_ids_by_type(main, "closure");
    assert_eq!(closure_ids.len(), 1);

    let closure_data = main
        .nodes
        .get(&closure_ids[0])
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ClosureData>()
        .expect("closure node must carry ClosureData");
    assert_eq!(closure_data.kind, ClosureKind::Foreach);
    assert!(main.valid, "foreach migration network must validate");
}

#[test]
fn test_phase3_no_extras_preserves_wire() {
    // A `map.f` whose source has exactly K (=1) free inputs falls in `NoOp` —
    // the function-pin synthesizer on the zones branch handles it directly,
    // so the migration must leave the wire untouched.
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/no_extras_preserved.cnnd", FIXTURE_DIR),
    )
    .expect("no_extras_preserved v4 fixture failed to load");

    let main = registry.node_networks.get("Main").unwrap();
    assert!(
        !main.nodes.values().any(|n| n.node_type_name == "closure"),
        "NoOp branch must never synthesize a closure node"
    );

    // The map's `f` argument still reads pin `-1` of the original source.
    let map_id = find_node_id_by_type(main, "map");
    let map_node = main.nodes.get(&map_id).unwrap();
    let f_wire = &map_node.arguments[1].incoming_wires[0];
    assert_eq!(f_wire.source_pin, SourcePin::NodeOutput { pin_index: -1 });
}

#[test]
fn test_phase3_fanout_creates_two_closures() {
    // One source's `-1` pin feeding two HOFs.f → two independent closures,
    // each with its own body clone of the source.
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/fanout_creates_two_closures.cnnd", FIXTURE_DIR),
    )
    .expect("fanout_creates_two_closures v4 fixture failed to load");

    let main = registry.node_networks.get("Main").unwrap();
    let closure_ids = find_node_ids_by_type(main, "closure");
    assert_eq!(
        closure_ids.len(),
        2,
        "fanout must synthesise one closure per consumer; got {} closures",
        closure_ids.len()
    );
    // Each closure has its own body with exactly one node.
    for cid in &closure_ids {
        let n = main.nodes.get(cid).unwrap();
        let body = n.zone.as_ref().expect("closure must own a body");
        assert_eq!(body.nodes.len(), 1);
    }
}

#[test]
fn test_phase3_source_cleanup_fanout_deletes_orphan() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/source_cleanup_fanout.cnnd", FIXTURE_DIR),
    )
    .expect("source_cleanup_fanout v4 fixture failed to load");

    let main = registry.node_networks.get("Main").unwrap();
    // The `vec3` source had only `-1` consumers; after rewriting both, it
    // is orphaned and cleanup deletes it.
    assert!(
        !main.nodes.values().any(|n| n.node_type_name == "vec3"),
        "orphan source must be deleted from the parent network"
    );
    // Two closures synthesised.
    let closure_ids = find_node_ids_by_type(main, "closure");
    assert_eq!(closure_ids.len(), 2);
}

#[test]
fn test_phase3_source_cleanup_preserved_when_non_minus_one_consumer() {
    // The source has both a `-1` consumer (the HOF.f) and a regular consumer
    // (some other node reading pin 0). Cleanup preserves the source.
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/source_cleanup_preserved.cnnd", FIXTURE_DIR),
    )
    .expect("source_cleanup_preserved v4 fixture failed to load");

    let main = registry.node_networks.get("Main").unwrap();
    // The source (`expr` named "fn") is preserved because the `doubler` expr
    // still consumes its pin 0.
    let fn_preserved = main
        .nodes
        .values()
        .any(|n| n.custom_name.as_deref() == Some("fn"));
    assert!(
        fn_preserved,
        "source with a non-`-1` consumer must NOT be deleted by cleanup; \
         post-migration nodes: {:?}",
        main.nodes
            .values()
            .map(|n| (n.id, n.node_type_name.as_str(), n.custom_name.clone()))
            .collect::<Vec<_>>()
    );
    // Exactly one closure was still synthesised.
    let closure_ids = find_node_ids_by_type(main, "closure");
    assert_eq!(closure_ids.len(), 1);
}

// ---------------------------------------------------------------------------
// Phase 4: deferred Phase-3 fixtures — the more involved test cases listed
// in the design doc's §"Test fixtures" table. These exercise interaction
// with neighbouring features (custom subnetworks, nested networks, chained
// migrations) and pin down behaviour for Open Questions §1.
// ---------------------------------------------------------------------------

#[test]
fn test_phase4_hof_source_for_hof_f_loads() {
    // Open Question §1: HOF.f wires whose source is itself an HOF. The
    // migration treats them uniformly — the inner `map` is cloned into the
    // closure body. The resulting nested-HOF construct's *validation*
    // outcome is the open question; the design-doc directive is that the
    // file must at least **load** and the migration produces a closure (no
    // panic).
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/hof_source_for_hof_f.cnnd", FIXTURE_DIR),
    )
    .expect("hof_source_for_hof_f v4 fixture failed to load");

    let main = registry.node_networks.get("Main").unwrap();
    let closure_ids = find_node_ids_by_type(main, "closure");
    assert_eq!(
        closure_ids.len(),
        1,
        "outer_map.f rewriting must synthesise exactly one closure node, \
         even though its source is itself an HOF"
    );

    // The closure body contains a clone of the inner `map` (node_type_name = "map").
    let closure_node = main.nodes.get(&closure_ids[0]).unwrap();
    let body = closure_node
        .zone
        .as_ref()
        .expect("closure must own a body");
    assert_eq!(body.nodes.len(), 1, "body must contain exactly one clone");
    let clone = body
        .nodes
        .get(&1)
        .expect("body's only node must have id 1");
    assert_eq!(
        clone.node_type_name, "map",
        "the cloned source must be the inner map node"
    );
}

#[test]
fn test_phase4_custom_subnetwork_instance_source_loads() {
    // The HOF.f's source is an instance of a custom subnetwork. The
    // migration treats the instance like any other node — clones it into
    // the closure body verbatim (same `node_type_name`, `data: {}`).
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/custom_subnetwork_instance_source.cnnd", FIXTURE_DIR),
    )
    .expect("custom_subnetwork_instance_source v4 fixture failed to load");

    let main = registry.node_networks.get("Main").unwrap();
    let closure_ids = find_node_ids_by_type(main, "closure");
    assert_eq!(closure_ids.len(), 1);

    let closure_node = main.nodes.get(&closure_ids[0]).unwrap();
    let body = closure_node
        .zone
        .as_ref()
        .expect("closure must own a body");
    let clone = body.nodes.get(&1).expect("body must contain clone at id 1");
    assert_eq!(
        clone.node_type_name, "ScaleBy",
        "the cloned source must carry the custom subnetwork's type name verbatim"
    );
}

#[test]
fn test_phase4_nested_custom_network_migrates_inner_hof() {
    // An HOF lives inside a custom subnetwork's definition (not in the
    // top-level Main). The migration iterates *every* entry in
    // `root["node_networks"]`, so the inner HOF is rewritten the same way
    // as a top-level one.
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/nested_custom_network.cnnd", FIXTURE_DIR),
    )
    .expect("nested_custom_network v4 fixture failed to load");

    // The closure was synthesised inside the `DoMap` subnetwork, not in Main.
    let do_map = registry
        .node_networks
        .get("DoMap")
        .expect("DoMap network missing after migration");
    let closure_ids = find_node_ids_by_type(do_map, "closure");
    assert_eq!(
        closure_ids.len(),
        1,
        "exactly one closure must be synthesised inside the nested subnetwork"
    );

    // Main has only its `DoMap` instance node — no spurious closure added.
    let main = registry.node_networks.get("Main").unwrap();
    assert!(
        !main.nodes.values().any(|n| n.node_type_name == "closure"),
        "Main must not gain a closure node — the rewrite happens inside DoMap"
    );
}

#[test]
fn test_phase4_type_mismatch_source_loads_without_panic() {
    // The source `expr` expects Vec3 parameters but the `map`'s `input_type`
    // is Float — so the closure's `ZoneInput` pin (Float) is type-
    // incompatible with the body clone's `x` parameter (Vec3). The migration
    // still produces structurally valid output; `repair_node_network` then
    // disconnects the now-incompatible wire rather than panicking.
    //
    // This is the design-doc Phase 4 hardening check: "validate that repair
    // disconnects incompatible wires cleanly rather than panicking."
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/type_mismatch_source.cnnd", FIXTURE_DIR),
    )
    .expect("type_mismatch_source fixture must load even with mismatched body types");

    // Structural check: the closure was still synthesised; load did not bail.
    let main = registry.node_networks.get("Main").unwrap();
    let closure_ids = find_node_ids_by_type(main, "closure");
    assert_eq!(
        closure_ids.len(),
        1,
        "migration must still synthesise a closure even when body types are mismatched"
    );
}

#[test]
fn test_phase4_real_filter_then_fold_evaluates() {
    // End-to-end regression: a representative real-world chain
    // `range → filter → fold` where both HOFs use the legacy
    // trailing-extras `f`-wire pattern. After v4→v5 migration the network
    // must load, validate, and produce the expected fold result.
    //
    // range(0,1,5) → [0,1,2,3,4]
    // filter(x > threshold=2) → [3, 4]
    // fold(init=0, acc + scale*x where scale=10) → 0 + 30 + 40 = 70
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/real_filter_then_fold.cnnd", FIXTURE_DIR),
    )
    .expect("real_filter_then_fold v4 fixture failed to load");

    let main = registry.node_networks.get("Main").unwrap();
    // Two closures synthesised (filter.f + fold.f).
    let closure_ids = find_node_ids_by_type(main, "closure");
    assert_eq!(
        closure_ids.len(),
        2,
        "expected two closures (one per HOF.f); got {:?}",
        main.nodes
            .values()
            .map(|n| (n.id, n.node_type_name.as_str()))
            .collect::<Vec<_>>()
    );

    // The fold node is the return node; evaluate it.
    let fold_id = main.return_node_id.expect("Main must have a return node");
    let result = evaluate_node(&registry, "Main", fold_id);
    match result {
        NetworkResult::Int(n) => assert_eq!(
            n, 70,
            "fold should compute 0 + scale*3 + scale*4 = 70 (scale=10)"
        ),
        NetworkResult::Error(e) => panic!("fold evaluated to Error: {}", e),
        other => panic!(
            "expected Int(70) from fold; got {}",
            other.to_display_string()
        ),
    }
}

#[test]
fn test_phase4_v3_chained_through_runs_both_migrations() {
    // A v3 file that chains v3→v4 (inserts a `collect`) and v4→v5
    // (synthesises a `closure` for the legacy HOF.f-with-extras pattern).
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/v3_chained_through.cnnd", FIXTURE_DIR),
    )
    .expect("v3_chained_through v3 fixture failed to load");

    let main = registry.node_networks.get("Main").unwrap();
    // v3→v4 must have synthesised a `collect` on the range→array_len wire.
    let collect_ids = find_node_ids_by_type(main, "collect");
    assert_eq!(
        collect_ids.len(),
        1,
        "v3→v4 must insert exactly one collect node on the range→array_len wire"
    );
    // v4→v5 must have synthesised a `closure` on map.f's source.
    let closure_ids = find_node_ids_by_type(main, "closure");
    assert_eq!(
        closure_ids.len(),
        1,
        "v4→v5 must synthesise exactly one closure for the legacy HOF.f pattern"
    );
}
