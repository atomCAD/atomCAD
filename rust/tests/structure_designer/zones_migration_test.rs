// Load-and-evaluate regressions for legacy main-branch function-pin files.
//
// History: the `zones` branch briefly carried a `migrate_v4_to_v5` pre-pass
// that rewrote main's "node `-1` pin feeding an HOF `f` pin, with some inputs
// wired as captures" idiom into a synthesized `closure` node. That migration
// was deleted (see `doc/design_node_function_pin_captures.md`, Phase 2): the
// function-pin synthesizer (`build_node_function_closure`) now reproduces the
// capture/parameter partition at evaluation time, so a v4 main-branch file
// loads and evaluates **directly** with no structural rewrite.
//
// These tests therefore no longer assert "a closure node was synthesized".
// Instead they load each (formerly migration-) fixture and assert the network
// evaluates to the hand-computed reference value — proving the synthesizer
// reproduces main's semantics end-to-end. The fixtures still live under
// `tests/fixtures/zones_migration/` and are still stamped `version: 4` (or 3),
// exercising the version dispatch's no-transform bump to 5.
//
// Pattern modelled after `tests/integration/iterator_migration_test.rs`.

use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::load_node_networks_from_file;

const FIXTURE_DIR: &str = "tests/fixtures/zones_migration";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load(fixture: &str) -> NodeTypeRegistry {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, &format!("{}/{}", FIXTURE_DIR, fixture))
        .unwrap_or_else(|e| panic!("fixture {} failed to load: {}", fixture, e));
    registry
}

fn error_texts(network: &NodeNetwork) -> Vec<String> {
    network
        .validation_errors
        .iter()
        .map(|e| e.error_text.clone())
        .collect()
}

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

/// Evaluate one pin of one node in a named network. `execute` toggles the
/// effect-node Execute pass (needed for `foreach`, whose body fires only under
/// Execute).
fn evaluate_node(
    registry: &NodeTypeRegistry,
    network_name: &str,
    node_id: u64,
    execute: bool,
) -> NetworkResult {
    let network = registry
        .node_networks
        .get(network_name)
        .unwrap_or_else(|| panic!("network {} missing from registry", network_name));
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    context.execute = execute;
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

/// Drain an `Iter[T]` result into a Vec of elements. Bodies are pre-frozen at
/// the producing HOF's eval, so a fresh body-only context is sufficient.
fn drain(registry: &NodeTypeRegistry, result: NetworkResult) -> Vec<NetworkResult> {
    let mut walker = match result {
        NetworkResult::Iterator(w) => w,
        NetworkResult::Error(e) => panic!("expected Iterator, got Error: {}", e),
        other => panic!("expected Iterator, got {}", other.to_display_string()),
    };
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let mut out = Vec::new();
    while out.len() < 4096 {
        match walker.next(&evaluator, registry, &mut context) {
            None => return out,
            Some(v) => out.push(v),
        }
    }
    panic!("drain exceeded cap of 4096 elements");
}

fn as_floats(values: Vec<NetworkResult>) -> Vec<f64> {
    values
        .into_iter()
        .map(|r| match r {
            NetworkResult::Float(v) => v,
            NetworkResult::Error(e) => panic!("expected Float element, got Error: {}", e),
            other => panic!("expected Float element, got {}", other.to_display_string()),
        })
        .collect()
}

fn as_vec3s(values: Vec<NetworkResult>) -> Vec<(f64, f64, f64)> {
    values
        .into_iter()
        .map(|r| match r {
            NetworkResult::Vec3(v) => (v.x, v.y, v.z),
            NetworkResult::Error(e) => panic!("expected Vec3 element, got Error: {}", e),
            other => panic!("expected Vec3 element, got {}", other.to_display_string()),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Version dispatch: v4/v5 files load with no transform.
// ---------------------------------------------------------------------------

#[test]
fn test_v5_file_loads() {
    let registry = load("already_v5.cnnd");
    assert!(
        registry.node_networks.contains_key("Main"),
        "Main network missing after loading a v5 file"
    );
}

#[test]
fn test_v4_no_hofs_loads_unchanged() {
    let registry = load("v4_no_hofs.cnnd");
    let main = registry
        .node_networks
        .get("Main")
        .expect("Main network missing after load");
    assert_eq!(
        main.nodes.len(),
        2,
        "no-HOF v4 fixture must load with its original node count (no transform)"
    );
}

// ---------------------------------------------------------------------------
// The four HOFs: a node `-1` pin with mixed wired (capture) / unwired
// (parameter) inputs feeds the HOF's `f` pin and evaluates with the captures
// frozen. These are the canonical "main files just work" regressions.
// ---------------------------------------------------------------------------

#[test]
fn test_simple_map_with_capture_evaluates() {
    // vec3(x: param, y: capture=fx=1.0, z: capture=fy=2.0); vec3.-1 → map.f.
    // map over range(0,1,3) = [0,1,2] → [(0,1,2),(1,1,2),(2,1,2)].
    let registry = load("simple_map_with_capture.cnnd");
    let main = registry.node_networks.get("Main").unwrap();
    assert!(main.valid, "errors={:?}", error_texts(main));

    let map_id = find_node_id_by_type(main, "map");
    let result = evaluate_node(&registry, "Main", map_id, false);
    assert_eq!(
        as_vec3s(drain(&registry, result)),
        vec![(0.0, 1.0, 2.0), (1.0, 1.0, 2.0), (2.0, 1.0, 2.0)]
    );
}

#[test]
fn test_simple_filter_with_capture_evaluates() {
    // pred(x: param, t: capture=threshold=2.0) = x > t; pred.-1 → filter.f.
    // filter over range(0,1,5) = [0,1,2,3,4] → [3, 4]. The filter declares
    // `element_type: Float` (output `Iter[Float]`), and its source `range`
    // yields `Iter[Int]`. With lazy `Iter[Int] → Iter[Float]` element
    // conversion now implemented (open question #2 of
    // `doc/design_iterators.md`), the source is converted at the filter's
    // input pin, so the filter correctly emits `Float` elements matching its
    // declared output type — previously this read was a silent no-op
    // passthrough that emitted `Int` despite the `Iter[Float]` declaration.
    let registry = load("simple_filter_with_capture.cnnd");
    let main = registry.node_networks.get("Main").unwrap();
    assert!(main.valid, "errors={:?}", error_texts(main));

    let filter_id = find_node_id_by_type(main, "filter");
    let result = evaluate_node(&registry, "Main", filter_id, false);
    assert_eq!(as_floats(drain(&registry, result)), vec![3.0, 4.0]);
}

#[test]
fn test_simple_fold_with_capture_evaluates() {
    // step(acc, x, s: capture=scale=0.5) = acc + s*x; step.-1 → fold.f.
    // fold init=0 over [0,1,2,3,4] → 0 + 0.5*(0+1+2+3+4) = 5.0.
    let registry = load("simple_fold_with_capture.cnnd");
    let main = registry.node_networks.get("Main").unwrap();
    assert!(main.valid, "errors={:?}", error_texts(main));

    let fold_id = find_node_id_by_type(main, "fold");
    match evaluate_node(&registry, "Main", fold_id, false) {
        NetworkResult::Float(v) => assert!((v - 5.0).abs() < 1e-9, "expected 5.0, got {}", v),
        other => panic!("expected Float(5.0), got {}", other.to_display_string()),
    }
}

#[test]
fn test_simple_foreach_with_capture_evaluates() {
    // side(x, s: capture=scale=2.0) = s*x; side.-1 → foreach.f. foreach is an
    // effect node (returns Unit). It loads/validates, and under an Execute pass
    // the body fires once per element (range(0,1,3) = [0,1,2]) without error.
    let registry = load("simple_foreach_with_capture.cnnd");
    let main = registry.node_networks.get("Main").unwrap();
    assert!(main.valid, "errors={:?}", error_texts(main));

    let foreach_id = find_node_id_by_type(main, "foreach");
    match evaluate_node(&registry, "Main", foreach_id, true) {
        NetworkResult::Unit => {}
        NetworkResult::Error(e) => panic!("foreach evaluated to Error: {}", e),
        other => panic!(
            "expected Unit from foreach, got {}",
            other.to_display_string()
        ),
    }
}

// ---------------------------------------------------------------------------
// No-capture source: every input unwired → function of all params. With the
// arity matching the HOF exactly, the HOF produces a fully-applied stream
// (this is the former `NoOp` migration case — now just an ordinary load).
// ---------------------------------------------------------------------------

#[test]
fn test_no_extras_source_evaluates() {
    // expr fn(x) = x*2 with x unwired; fn.-1 → map.f. map over [0,1,2] →
    // [0,2,4].
    let registry = load("no_extras_preserved.cnnd");
    let main = registry.node_networks.get("Main").unwrap();
    assert!(main.valid, "errors={:?}", error_texts(main));

    let map_id = find_node_id_by_type(main, "map");
    let result = evaluate_node(&registry, "Main", map_id, false);
    assert_eq!(as_floats(drain(&registry, result)), vec![0.0, 2.0, 4.0]);
}

// ---------------------------------------------------------------------------
// Non-prefix capture: captures need not be a trailing suffix. The migration
// classified "wired pin before unwired pin" as a `Skip`; the new model treats
// it as an ordinary, valid capture at an arbitrary pin position. (This fixture
// was formerly `bad_wired_after_unwired.cnnd`.)
// ---------------------------------------------------------------------------

#[test]
fn test_nonprefix_capture_evaluates() {
    // vec3(x: capture=fx=1.0, y: param, z: capture=fz=3.0); vec3.-1 → map.f.
    // unwired = [y] → function (Float) → Vec3 with x=1, z=3 frozen. map over
    // range(0,1,3) = [0,1,2] → [(1,0,3),(1,1,3),(1,2,3)].
    let registry = load("nonprefix_capture.cnnd");
    let main = registry.node_networks.get("Main").unwrap();
    assert!(main.valid, "errors={:?}", error_texts(main));

    let map_id = find_node_id_by_type(main, "map");
    let result = evaluate_node(&registry, "Main", map_id, false);
    assert_eq!(
        as_vec3s(drain(&registry, result)),
        vec![(1.0, 0.0, 3.0), (1.0, 1.0, 3.0), (1.0, 2.0, 3.0)]
    );
}

// ---------------------------------------------------------------------------
// The source is a custom-subnetwork instance with one input wired as a
// capture — exercises the synthesizer over a non-built-in source node.
// ---------------------------------------------------------------------------

#[test]
fn test_custom_subnetwork_instance_source_evaluates() {
    // ScaleBy(x, k) = k*x. Instance "s" has x unwired (param), k captured
    // (k_val=3.0). s.-1 → map.f. map over range(0,1,4) = [0,1,2,3] →
    // [0,3,6,9].
    let registry = load("custom_subnetwork_instance_source.cnnd");
    let main = registry.node_networks.get("Main").unwrap();
    assert!(main.valid, "errors={:?}", error_texts(main));

    let map_id = find_node_id_by_type(main, "map");
    let result = evaluate_node(&registry, "Main", map_id, false);
    assert_eq!(
        as_floats(drain(&registry, result)),
        vec![0.0, 3.0, 6.0, 9.0]
    );
}

// ---------------------------------------------------------------------------
// The capture pattern living inside a custom subnetwork definition (DoMap),
// invoked from Main via an instance node — the synthesizer runs through the
// recursive custom-network evaluation path.
// ---------------------------------------------------------------------------

#[test]
fn test_nested_custom_network_evaluates() {
    // DoMap reproduces the simple_map_with_capture pattern internally and
    // returns Iter[Vec3]; Main just instantiates DoMap. Evaluating Main's
    // instance node → [(0,1,2),(1,1,2),(2,1,2)].
    let registry = load("nested_custom_network.cnnd");
    let main = registry.node_networks.get("Main").unwrap();
    assert!(main.valid, "errors={:?}", error_texts(main));
    let do_map = registry.node_networks.get("DoMap").unwrap();
    assert!(do_map.valid, "errors={:?}", error_texts(do_map));

    let instance_id = main.return_node_id.expect("Main must have a return node");
    let result = evaluate_node(&registry, "Main", instance_id, false);
    assert_eq!(
        as_vec3s(drain(&registry, result)),
        vec![(0.0, 1.0, 2.0), (1.0, 1.0, 2.0), (2.0, 1.0, 2.0)]
    );
}

// ---------------------------------------------------------------------------
// End-to-end chain: range → filter → fold, both HOFs using the legacy
// capture idiom. The headline "a real main-branch file just works" regression.
// ---------------------------------------------------------------------------

#[test]
fn test_real_filter_then_fold_evaluates() {
    // range(0,1,5) = [0,1,2,3,4]; filter(x > threshold=2) = [3,4];
    // fold(init=0, acc + scale*x where scale=10) = 0 + 30 + 40 = 70.
    let registry = load("real_filter_then_fold.cnnd");
    let main = registry.node_networks.get("Main").unwrap();
    assert!(main.valid, "errors={:?}", error_texts(main));

    let fold_id = main.return_node_id.expect("Main must have a return node");
    match evaluate_node(&registry, "Main", fold_id, false) {
        NetworkResult::Int(n) => assert_eq!(n, 70, "fold should compute 0 + 10*3 + 10*4 = 70"),
        NetworkResult::Error(e) => panic!("fold evaluated to Error: {}", e),
        other => panic!("expected Int(70), got {}", other.to_display_string()),
    }
}

// ---------------------------------------------------------------------------
// Chained dispatch: a v3 file runs v3→v4 (inserts a `collect`) and then is
// bumped to v5 with no transform; the capture idiom still evaluates.
// ---------------------------------------------------------------------------

#[test]
fn test_v3_chained_through_evaluates() {
    // fn(x: param, k: capture=2.0) = k*x; fn.-1 → map.f. map over r1 =
    // range(0,1,4) = [0,1,2,3] → [0,2,4,6]. Separately, r2 → array_len gets a
    // `collect` inserted by the v3→v4 pass.
    let registry = load("v3_chained_through.cnnd");
    let main = registry.node_networks.get("Main").unwrap();
    assert!(main.valid, "errors={:?}", error_texts(main));

    // v3→v4 must still insert exactly one collect on the range→array_len wire.
    let collect_count = main
        .nodes
        .values()
        .filter(|n| n.node_type_name == "collect")
        .count();
    assert_eq!(
        collect_count, 1,
        "v3→v4 must insert exactly one collect node on the range→array_len wire"
    );

    let map_id = find_node_id_by_type(main, "map");
    let result = evaluate_node(&registry, "Main", map_id, false);
    assert_eq!(
        as_floats(drain(&registry, result)),
        vec![0.0, 2.0, 4.0, 6.0]
    );
}

// ---------------------------------------------------------------------------
// Edge cases that must at least load without panicking (Open Question §1 and
// the ill-typed-source hardening check from the design doc).
// ---------------------------------------------------------------------------

#[test]
fn test_hof_source_for_hof_f_loads() {
    // outer_map.f's source is itself an HOF (inner_map). The construct loads;
    // its evaluation/validation outcome is Open Question §1 — we only require
    // a clean load (no panic).
    let registry = load("hof_source_for_hof_f.cnnd");
    assert!(registry.node_networks.contains_key("Main"));
}

#[test]
fn test_type_mismatch_source_loads_without_panic() {
    // The source expr expects Vec3 params but map.input_type is Float, so the
    // synthesized function is type-incompatible with map.f. The file must
    // still load — repair disconnects the incompatible wire / validation flags
    // it, rather than panicking.
    let registry = load("type_mismatch_source.cnnd");
    assert!(registry.node_networks.contains_key("Main"));
}
