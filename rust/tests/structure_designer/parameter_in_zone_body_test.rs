//! Issue #417 — `parameter` nodes are not allowed inside a zone body.
//!
//! A `parameter` node declares an input pin of the enclosing *network*; a zone
//! body (an HOF body or a `closure` body) has no interface — its inputs are
//! zone-input pins and captures. These tests cover the four enforcement layers:
//!
//! 1. authoring refusal (`add_node_scoped`, `paste_at_position_scoped`,
//!    `duplicate_node_scoped`),
//! 2. the `allowed_in_zone_body` flag the add-node popup filters on,
//! 3. the validator backstop (non-blocking, for hand-authored / legacy files),
//! 4. the `ParameterData::eval` guard (localized error instead of a wrong
//!    argument read or an out-of-bounds panic).

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_network::{Argument, IncomingWire, SourcePin};
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, allowed_in_zone_body,
};
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::nodes::fold::FoldData;
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn set_node_data(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    data: Box<dyn NodeData>,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = data;
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
}

/// Force a `parameter` node into an HOF/closure body, bypassing every authoring
/// refusal — this is the hand-authored / pre-#417 `.cnnd` state the validator
/// and the eval guard exist to handle.
fn force_parameter_into_body(
    designer: &mut StructureDesigner,
    network_name: &str,
    hof_node_id: u64,
    param_name: &str,
) -> u64 {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let hof_node = network.nodes.get_mut(&hof_node_id).unwrap();
    let body = hof_node.zone_mut().expect("HOF node missing zone");

    let param_data = ParameterData {
        param_id: Some(0),
        param_index: 0,
        param_name: param_name.to_string(),
        data_type: DataType::Int,
        sort_order: 0,
        data_type_str: None,
        error: None,
    };
    let param_id = body.add_node("parameter", DVec2::new(20.0, 0.0), 1, Box::new(param_data));

    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        registry
            .node_networks
            .get_mut(network_name)
            .unwrap()
            .nodes
            .get_mut(&hof_node_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&param_id)
            .unwrap(),
        true,
    );

    param_id
}

/// Wire a body node into the HOF's zone-output pin at `pin_index`.
fn wire_body_node_to_zone_output(
    designer: &mut StructureDesigner,
    network_name: &str,
    hof_node_id: u64,
    pin_index: usize,
    body_node_id: u64,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let hof_node = network.nodes.get_mut(&hof_node_id).unwrap();
    while hof_node.zone_output_arguments.len() <= pin_index {
        hof_node.zone_output_arguments.push(Argument::new());
    }
    hof_node.zone_output_arguments[pin_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: body_node_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
        });
}

fn evaluate_node(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn drain_iter(designer: &StructureDesigner, result: NetworkResult) -> Vec<NetworkResult> {
    let mut walker = match result {
        NetworkResult::Iterator(w) => w,
        other => panic!(
            "expected NetworkResult::Iterator, got {}",
            other.to_display_string()
        ),
    };
    let evaluator = NetworkEvaluator::new();
    let registry = &designer.node_type_registry;
    let mut context = NetworkEvaluationContext::new();
    let mut out = Vec::new();
    while out.len() < 64 {
        match walker.next(&evaluator, registry, &mut context) {
            None => return out,
            Some(v) => out.push(v),
        }
    }
    panic!("drain exceeded cap");
}

/// A `map` node in `main` fed by a 3-element `range`, with an empty body.
fn map_over_range(designer: &mut StructureDesigner) -> u64 {
    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        }),
    );
    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
    set_node_data(
        designer,
        "main",
        map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, map_id, 0);
    map_id
}

// ============================================================================
// The predicate itself
// ============================================================================

#[test]
fn only_parameter_is_disallowed_in_zone_body() {
    assert!(!allowed_in_zone_body("parameter"));
    for name in ["expr", "int", "map", "closure", "apply", "sphere"] {
        assert!(allowed_in_zone_body(name), "{} should be allowed", name);
    }
}

/// The add-node popup filters on this flag; it must be `false` exactly for
/// `parameter` and `true` for everything else the registry publishes.
#[test]
fn node_type_views_expose_allowed_in_zone_body() {
    let designer = setup_designer_with_network("main");
    let categories = designer.node_type_registry.get_node_type_views();

    let mut saw_parameter = false;
    for category in &categories {
        for view in &category.nodes {
            if view.name == "parameter" {
                saw_parameter = true;
                assert!(
                    !view.allowed_in_zone_body,
                    "`parameter` must be flagged as body-disallowed"
                );
            } else {
                assert!(
                    view.allowed_in_zone_body,
                    "`{}` must be allowed in a zone body",
                    view.name
                );
            }
        }
    }
    assert!(
        saw_parameter,
        "`parameter` missing from the node type views"
    );
}

// ============================================================================
// Authoring refusals
// ============================================================================

#[test]
fn add_node_scoped_refuses_parameter_in_body() {
    let mut designer = setup_designer_with_network("main");
    let map_id = map_over_range(&mut designer);

    let refused = designer.add_node_scoped(&[map_id], "parameter", DVec2::new(30.0, 0.0), None);
    assert_eq!(refused, 0, "adding a `parameter` to a body must be refused");

    // Nothing landed in the body.
    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert!(body.nodes.is_empty());

    // A normal node still adds fine through the same path.
    let ok = designer.add_node_scoped(&[map_id], "int", DVec2::new(30.0, 0.0), None);
    assert_ne!(ok, 0);
}

#[test]
fn add_node_top_level_still_allows_parameter() {
    let mut designer = setup_designer_with_network("main");
    let param_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    assert_ne!(param_id, 0, "top-level `parameter` must still be addable");
}

#[test]
fn paste_into_body_drops_parameter_nodes() {
    let mut designer = setup_designer_with_network("main");
    let map_id = map_over_range(&mut designer);

    // Copy a top-level selection containing a `parameter` and an `int`.
    let param_id = designer.add_node("parameter", DVec2::new(0.0, 200.0));
    let int_id = designer.add_node("int", DVec2::new(0.0, 300.0));
    set_node_data(
        &mut designer,
        "main",
        int_id,
        Box::new(IntData { value: 7 }),
    );
    designer.select_nodes(vec![param_id, int_id]);
    assert!(designer.copy_selection());

    let new_ids = designer.paste_at_position_scoped(&[map_id], DVec2::new(40.0, 20.0));
    assert_eq!(new_ids.len(), 1, "only the `int` should have pasted");

    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert_eq!(body.nodes.len(), 1);
    assert!(
        body.nodes.values().all(|n| n.node_type_name != "parameter"),
        "no `parameter` may land in a body"
    );
}

#[test]
fn duplicate_in_body_refuses_a_legacy_parameter() {
    let mut designer = setup_designer_with_network("main");
    let map_id = map_over_range(&mut designer);
    let param_id = force_parameter_into_body(&mut designer, "main", map_id, "legacy");

    let duplicated = designer.duplicate_node_scoped(&[map_id], param_id);
    assert_eq!(duplicated, 0, "duplicating a body `parameter` must refuse");
    assert_eq!(
        designer.get_scope_network(&[map_id]).unwrap().nodes.len(),
        1
    );
}

// ============================================================================
// Validator backstop
// ============================================================================

#[test]
fn validator_flags_body_parameter_without_blocking() {
    let mut designer = setup_designer_with_network("main");
    let map_id = map_over_range(&mut designer);
    let param_id = force_parameter_into_body(&mut designer, "main", map_id, "legacy");
    wire_body_node_to_zone_output(&mut designer, "main", map_id, 0, param_id);

    designer.validate_active_network();

    let network = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    let body = network.nodes.get(&map_id).unwrap().zone.as_ref().unwrap();

    let err = body
        .validation_errors
        .iter()
        .find(|e| e.node_id == Some(param_id))
        .expect("expected a validation error on the body `parameter`");
    assert!(
        err.error_text.contains("not allowed inside a zone body"),
        "unexpected message: {}",
        err.error_text
    );
    assert!(!err.blocking, "the rule must be non-blocking");

    // Non-blocking ⇒ neither the body nor the network is invalidated, so the
    // rest of the design keeps evaluating.
    assert!(body.valid, "body must stay valid");
    assert!(network.valid, "network must stay valid");
}

#[test]
fn validator_leaves_top_level_parameters_alone() {
    let mut designer = setup_designer_with_network("main");
    designer.add_node("parameter", DVec2::new(0.0, 0.0));

    designer.validate_active_network();

    let network = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert!(
        !network
            .validation_errors
            .iter()
            .any(|e| e.error_text.contains("not allowed inside a zone body")),
        "a top-level parameter must not trip the zone-body rule"
    );
}

// ============================================================================
// Eval guard
// ============================================================================

/// Lazy path (`map`): the body runs on a body-only stack, which used to take
/// the "evaluated in isolation" branch and quietly behave as a constant.
#[test]
fn body_parameter_evaluates_to_error_in_lazy_map() {
    let mut designer = setup_designer_with_network("main");
    let map_id = map_over_range(&mut designer);
    let param_id = force_parameter_into_body(&mut designer, "main", map_id, "legacy");
    wire_body_node_to_zone_output(&mut designer, "main", map_id, 0, param_id);

    let elements = drain_iter(&designer, evaluate_node(&designer, "main", map_id));
    // The walker surfaces the body's error and stops, so only the first element
    // materializes — what matters is that no element is a bogus value.
    assert!(!elements.is_empty(), "expected at least one element");
    for element in elements {
        match element {
            NetworkResult::Error(msg) => assert!(
                msg.contains("not allowed inside a zone body"),
                "unexpected error: {}",
                msg
            ),
            other => panic!("expected Error, got {}", other.to_display_string()),
        }
    }
}

/// Eager path (`fold`): the body runs on the real network stack, so the frame
/// below the parameter is the `fold` node itself — the lookup used to read
/// `fold`'s own arguments by a stale `param_index`.
#[test]
fn body_parameter_evaluates_to_error_in_eager_fold() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 1,
            step: 1,
            count: 3,
        }),
    );
    let init_id = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_node_data(
        &mut designer,
        "main",
        init_id,
        Box::new(IntData { value: 0 }),
    );

    let fold_id = designer.add_node("fold", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        fold_id,
        Box::new(FoldData {
            element_type: DataType::Int,
            accumulator_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, fold_id, 0);
    designer.connect_nodes(init_id, 0, fold_id, 1);

    let param_id = force_parameter_into_body(&mut designer, "main", fold_id, "legacy");
    wire_body_node_to_zone_output(&mut designer, "main", fold_id, 0, param_id);

    match evaluate_node(&designer, "main", fold_id) {
        NetworkResult::Error(msg) => assert!(
            msg.contains("not allowed inside a zone body"),
            "unexpected error: {}",
            msg
        ),
        other => panic!("expected Error, got {}", other.to_display_string()),
    }
}

/// Regression: a `closure` declares **no input pins**, so the pre-#417 argument
/// lookup (`parent_node.arguments[param_index]`) panicked outright when a body
/// `parameter` was reached through a real stack. Evaluating the closure's body
/// through `apply` must return a clean error instead.
#[test]
fn body_parameter_in_closure_does_not_panic() {
    let mut designer = setup_designer_with_network("main");

    let closure_id = designer.add_node("closure", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int],
            param_names: vec![],
            custom_label: None,
        }),
    );

    let param_id = force_parameter_into_body(&mut designer, "main", closure_id, "legacy");
    wire_body_node_to_zone_output(&mut designer, "main", closure_id, 0, param_id);

    let apply_id = designer.add_node("apply", DVec2::new(300.0, 0.0));
    designer.connect_nodes(closure_id, 0, apply_id, 0);

    match evaluate_node(&designer, "main", apply_id) {
        NetworkResult::Error(msg) => assert!(
            msg.contains("not allowed inside a zone body"),
            "unexpected error: {}",
            msg
        ),
        other => panic!("expected Error, got {}", other.to_display_string()),
    }
}
