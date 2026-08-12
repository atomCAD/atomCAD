//! Phase 3 of the error-management redesign (`doc/design_error_management.md`
//! D3 + D5): cone-scoped validation blocking.
//!
//! - **Skip-and-synthesize:** a blocking validation error attributed to a node
//!   makes the evaluator skip the node's `eval` and synthesize a
//!   `NetworkResult::Error` from the validation text — only the node and its
//!   downstream cone go dark, independent nodes keep evaluating.
//! - **`valid` = interface residue:** `NodeNetwork::valid` now means "free of
//!   interface-level errors and unattributed blocking errors"; node-attributed
//!   blocking errors no longer flip it, so custom networks with localized
//!   breakage stay usable from their parents.
//! - **Wire-cycle rule + re-entrancy backstop:** cycles (including
//!   capture-threaded cross-scope ones) are flagged blocking on every member;
//!   an escaped cycle terminates via the evaluator's `eval_in_progress` guard
//!   instead of hanging.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_network::{
    IncomingWire, SourcePin, ValidationError, has_interface_residue, node_poison_message,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
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

fn evaluate_node(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

/// Add an `expr` node with the given expression/parameters to the active
/// network of `network_name`, with its custom-node-type cache populated (the
/// debug invariants assert on an unpopulated cache).
fn add_expr(
    designer: &mut StructureDesigner,
    network_name: &str,
    expression: &str,
    parameters: Vec<(&str, DataType)>,
) -> u64 {
    let expr_params: Vec<ExprParameter> = parameters
        .into_iter()
        .map(|(name, dt)| ExprParameter {
            id: None,
            name: name.to_string(),
            data_type: dt,
            data_type_str: None,
        })
        .collect();
    let num_params = expr_params.len();
    let mut expr_data = ExprData {
        parameters: expr_params,
        expression: expression.to_string(),
        expr: None,
        error: None,
        output_type: None,
    };
    let _ = expr_data.parse_and_validate(0);

    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let expr_id = network.add_node("expr", DVec2::ZERO, num_params, Box::new(expr_data));
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        registry
            .node_networks
            .get_mut(network_name)
            .unwrap()
            .nodes
            .get_mut(&expr_id)
            .unwrap(),
        true,
    );
    expr_id
}

/// Push a raw wire (hand-authored-file style, bypassing `can_connect_nodes`)
/// from `source` pin 0 into `dest`'s `param_index`-th argument.
fn push_wire(
    designer: &mut StructureDesigner,
    network_name: &str,
    source: u64,
    dest: u64,
    param_index: usize,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let dest_node = network.nodes.get_mut(&dest).unwrap();
    dest_node.arguments[param_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: source,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
        });
}

fn blocking_errors_for(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> Vec<String> {
    designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap()
        .validation_errors
        .iter()
        .filter(|e| e.blocking && e.node_id == Some(node_id))
        .map(|e| e.error_text.clone())
        .collect()
}

// ============================================================================
// D3: skip-and-synthesize
// ============================================================================

/// The headline scenario: a lone `relax` (unresolved polymorphic output →
/// blocking validation error) must NOT blank the network. Independent nodes
/// keep evaluating; the `relax` node's output is the synthesized validation
/// text — proving its `eval` was never entered (the eval path would produce a
/// different, "missing input"-style message).
#[test]
fn lone_relax_poisons_only_itself() {
    let mut designer = setup_designer_with_network("main");
    let int_id = designer.add_node("int", DVec2::ZERO);
    let relax_id = designer.add_node("relax", DVec2::new(200.0, 0.0));
    designer.validate_active_network();

    // The polymorphic-output rule flags the bare relax, blocking...
    let relax_errors = blocking_errors_for(&designer, "main", relax_id);
    assert_eq!(
        relax_errors.len(),
        1,
        "bare relax carries one blocking error"
    );
    assert!(
        relax_errors[0].contains("could not be resolved"),
        "unexpected error text: {}",
        relax_errors[0]
    );
    // ...but the network stays valid (the error is node-attributed).
    assert!(
        designer.node_type_registry.node_networks["main"].valid,
        "a node-attributed blocking error must not invalidate the network"
    );

    // Independent node evaluates untouched.
    assert!(matches!(
        evaluate_node(&designer, "main", int_id),
        NetworkResult::Int(_)
    ));

    // The poisoned node's output is the synthesized validation text, verbatim.
    match evaluate_node(&designer, "main", relax_id) {
        NetworkResult::Error(text) => assert_eq!(
            text, relax_errors[0],
            "synthesized output must be the validation text (eval was skipped)"
        ),
        other => panic!("expected Error, got {}", other.to_display_string()),
    }
}

/// A type-mismatch destination is skip-and-synthesized: its `eval` is never
/// entered (the panic class this rule historically protected against), and the
/// synthesized output is exactly the validation text.
#[test]
fn type_mismatch_dest_eval_is_skipped() {
    let mut designer = setup_designer_with_network("main");
    let bool_id = designer.add_node("bool", DVec2::ZERO);
    // The expr's `x` pin expects a Float; wire a Bool into it by hand (the UI
    // refuses this connection, a hand-authored file doesn't).
    let expr_id = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Float)]);
    push_wire(&mut designer, "main", bool_id, expr_id, 0);
    designer.validate_active_network();

    let errors = blocking_errors_for(&designer, "main", expr_id);
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].contains("Data type mismatch"),
        "unexpected error text: {}",
        errors[0]
    );
    assert!(designer.node_type_registry.node_networks["main"].valid);

    match evaluate_node(&designer, "main", expr_id) {
        NetworkResult::Error(text) => assert_eq!(text, errors[0]),
        other => panic!("expected Error, got {}", other.to_display_string()),
    }
}

/// Downstream of a poisoned node, the synthesized error propagates through the
/// ordinary chaining machinery (`error in {pin} input (from …)`).
#[test]
fn poison_propagates_via_normal_chaining() {
    let mut designer = setup_designer_with_network("main");
    let relax_id = designer.add_node("relax", DVec2::ZERO);
    // expr consuming the relax output — the wire is type-invalid too, but the
    // relax's own poison fires first on the source side.
    let expr_id = add_expr(&mut designer, "main", "x", vec![("x", DataType::Float)]);
    push_wire(&mut designer, "main", relax_id, expr_id, 0);
    designer.validate_active_network();

    match evaluate_node(&designer, "main", expr_id) {
        NetworkResult::Error(text) => {
            assert!(
                text.contains("error in") && text.contains("could not be resolved"),
                "expected a chained error carrying the root text, got: {}",
                text
            );
        }
        other => panic!("expected Error, got {}", other.to_display_string()),
    }
}

// ============================================================================
// D5: cross-network semantics through the redefined `valid`
// ============================================================================

/// A custom network with a cone-scoped error OUTSIDE its return cone stays
/// `valid`; instances evaluate it normally and are completely unaffected.
#[test]
fn child_error_outside_return_cone_leaves_instances_untouched() {
    let mut designer = setup_designer_with_network("child");
    let int_id = designer.add_node("int", DVec2::ZERO);
    designer.set_return_node_id(Some(int_id));
    // A bare relax on a branch that feeds nothing.
    let _relax_id = designer.add_node("relax", DVec2::new(0.0, 200.0));
    designer.validate_active_network();
    assert!(
        designer.node_type_registry.node_networks["child"].valid,
        "child must stay valid — its blocking error is node-attributed"
    );

    designer.add_node_network("parent");
    designer.set_active_node_network_name(Some("parent".to_string()));
    let instance_id = designer.add_node("child", DVec2::ZERO);
    designer.validate_active_network();

    // No "References invalid node network" error appears in the parent.
    let parent = &designer.node_type_registry.node_networks["parent"];
    assert!(
        !parent
            .validation_errors
            .iter()
            .any(|e| e.error_text.contains("References invalid")),
        "parent must not flag the child as invalid"
    );
    assert!(parent.valid);

    // The instance evaluates normally.
    assert!(matches!(
        evaluate_node(&designer, "parent", instance_id),
        NetworkResult::Int(_)
    ));
}

/// A cone-scoped error INSIDE the child's return cone surfaces per call site:
/// the instance's output is a chained error naming the child; the parent's
/// other nodes are untouched. Fixing the child heals the instance with no
/// parent-side re-validation (the eval gates read the child's live state).
#[test]
fn child_error_inside_return_cone_chains_out_of_instance() {
    let mut designer = setup_designer_with_network("child");
    let relax_id = designer.add_node("relax", DVec2::ZERO);
    let int_id = designer.add_node("int", DVec2::new(0.0, 200.0));
    designer.set_return_node_id(Some(relax_id));
    designer.validate_active_network();
    assert!(designer.node_type_registry.node_networks["child"].valid);

    designer.add_node_network("parent");
    designer.set_active_node_network_name(Some("parent".to_string()));
    let instance_id = designer.add_node("child", DVec2::ZERO);
    let independent_id = designer.add_node("int", DVec2::new(0.0, 200.0));
    designer.validate_active_network();

    match evaluate_node(&designer, "parent", instance_id) {
        NetworkResult::Error(text) => {
            assert!(
                text.contains("Error in child:") && text.contains("could not be resolved"),
                "expected chained child error, got: {}",
                text
            );
        }
        other => panic!("expected Error, got {}", other.to_display_string()),
    }
    // Independent parent node unaffected.
    assert!(matches!(
        evaluate_node(&designer, "parent", independent_id),
        NetworkResult::Int(_)
    ));

    // Transition: fix the child (return the int instead), re-validate the
    // child only — the instance heals because the evaluator reads the child's
    // live validation state, not a parent-side cached verdict.
    designer.set_active_node_network_name(Some("child".to_string()));
    designer.set_return_node_id(Some(int_id));
    designer.validate_active_network();
    assert!(matches!(
        evaluate_node(&designer, "parent", instance_id),
        NetworkResult::Int(_)
    ));
}

/// Malformed `parameter` nodes are the interface residue: the network itself
/// still refuses evaluation wholesale, and instances of it become poisoned
/// nodes in their (otherwise fully working) parents.
#[test]
fn malformed_parameters_keep_whole_network_refusal() {
    let mut designer = setup_designer_with_network("child");
    let p1 = designer.add_node("parameter", DVec2::ZERO);
    let p2 = designer.add_node("parameter", DVec2::new(0.0, 100.0));
    for id in [p1, p2] {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("child")
            .unwrap();
        let node = network.nodes.get_mut(&id).unwrap();
        let pd = node
            .data
            .as_any_mut()
            .downcast_mut::<ParameterData>()
            .unwrap();
        pd.param_name = "p".to_string();
    }
    let int_id = designer.add_node("int", DVec2::new(0.0, 200.0));
    designer.set_return_node_id(Some(int_id));
    designer.validate_active_network();

    let child = &designer.node_type_registry.node_networks["child"];
    assert!(
        !child.valid,
        "duplicate parameter names are interface residue — network refuses evaluation"
    );
    assert!(has_interface_residue(&child.validation_errors));
    assert!(
        child
            .validation_errors
            .iter()
            .any(|e| e.interface && e.node_id == Some(p2)),
        "the duplicate-name error is interface-level and node-attributed"
    );

    // The parent flags the instance ("References invalid node network"),
    // which poisons it, but the parent itself stays valid and working.
    designer.add_node_network("parent");
    designer.set_active_node_network_name(Some("parent".to_string()));
    let instance_id = designer.add_node("child", DVec2::ZERO);
    let independent_id = designer.add_node("int", DVec2::new(0.0, 200.0));
    designer.validate_active_network();

    let parent = &designer.node_type_registry.node_networks["parent"];
    assert!(parent.valid);
    let instance_errors = blocking_errors_for(&designer, "parent", instance_id);
    assert!(
        instance_errors
            .iter()
            .any(|t| t.contains("References invalid node network")),
        "instance of a residue-invalid child must be flagged: {:?}",
        instance_errors
    );
    match evaluate_node(&designer, "parent", instance_id) {
        NetworkResult::Error(text) => {
            assert!(text.contains("References invalid node network"))
        }
        other => panic!("expected Error, got {}", other.to_display_string()),
    }
    assert!(matches!(
        evaluate_node(&designer, "parent", independent_id),
        NetworkResult::Int(_)
    ));
}

/// `execute_node` inherits the relaxed gate: a partially-broken network still
/// executes clean cones normally, and executing a poisoned cone yields the
/// synthesized error result rather than a whole-network refusal.
#[test]
fn execute_node_on_partially_broken_network() {
    let mut designer = setup_designer_with_network("main");
    let int_id = designer.add_node("int", DVec2::ZERO);
    let relax_id = designer.add_node("relax", DVec2::new(200.0, 0.0));
    designer.validate_active_network();

    let clean = designer
        .execute_node("main", &[], int_id)
        .expect("execute on a clean cone must not be refused");
    assert!(clean.ok, "clean cone executes normally: {:?}", clean.error);

    let poisoned = designer
        .execute_node("main", &[], relax_id)
        .expect("execute on a poisoned cone must not be refused at the gate");
    assert!(!poisoned.ok);
    assert!(
        poisoned
            .error
            .as_deref()
            .unwrap_or("")
            .contains("could not be resolved"),
        "expected the synthesized validation text, got {:?}",
        poisoned.error
    );
}

// ============================================================================
// Wire cycles: validation rule
// ============================================================================

/// A plain intra-scope cycle authored through the ORDINARY connect path
/// (`can_connect_nodes` does type checks only): both members are flagged
/// blocking, evaluation terminates with the cycle text, independents render.
#[test]
fn wire_cycle_flagged_and_evaluation_terminates() {
    let mut designer = setup_designer_with_network("main");
    let e1 = add_expr(&mut designer, "main", "x", vec![("x", DataType::Float)]);
    let e2 = add_expr(&mut designer, "main", "x", vec![("x", DataType::Float)]);
    let independent_id = designer.add_node("int", DVec2::new(0.0, 300.0));
    designer.connect_nodes(e1, 0, e2, 0);
    designer.connect_nodes(e2, 0, e1, 0);
    designer.validate_active_network();

    for id in [e1, e2] {
        let errors = blocking_errors_for(&designer, "main", id);
        assert!(
            errors.iter().any(|t| t.contains("Wire cycle detected")),
            "cycle member {} must carry the cycle error, got {:?}",
            id,
            errors
        );
    }
    assert!(designer.node_type_registry.node_networks["main"].valid);

    // Terminates (poisoned members are never evaluated), no hang/overflow.
    match evaluate_node(&designer, "main", e1) {
        NetworkResult::Error(text) => assert!(text.contains("Wire cycle detected")),
        other => panic!("expected Error, got {}", other.to_display_string()),
    }
    assert!(matches!(
        evaluate_node(&designer, "main", independent_id),
        NetworkResult::Int(_)
    ));
}

/// A self-loop (node wired to itself) is a cycle of one.
#[test]
fn self_loop_is_flagged() {
    let mut designer = setup_designer_with_network("main");
    let e1 = add_expr(&mut designer, "main", "x", vec![("x", DataType::Float)]);
    push_wire(&mut designer, "main", e1, e1, 0);
    designer.validate_active_network();

    let errors = blocking_errors_for(&designer, "main", e1);
    assert!(
        errors.iter().any(|t| t.contains("Wire cycle detected")),
        "self-loop must be flagged, got {:?}",
        errors
    );
}

/// A capture-threaded cycle: X → captured into an HOF body → zone-output wire
/// → HOF output → regular wire → X. Invisible to a scope-local DFS; the
/// projected capture edge ("H depends on X") closes the loop.
#[test]
fn capture_threaded_cycle_through_hof_body() {
    let mut designer = setup_designer_with_network("main");
    let map_id = designer.add_node("map", DVec2::ZERO);
    // X consumes the map's output (types are irrelevant to the cycle rule).
    let collect_id = designer.add_node("collect", DVec2::new(200.0, 0.0));
    designer.connect_nodes(map_id, 0, collect_id, 0);
    // Body node capturing X at depth 1, wired to the map's zone output.
    let body_node = designer.add_node_scoped(&[map_id], "collect", DVec2::ZERO, None);
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        let map_node = network.nodes.get_mut(&map_id).unwrap();
        let body = map_node.zone_mut().unwrap();
        body.nodes.get_mut(&body_node).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: collect_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 1,
            });
        use rust_lib_flutter_cad::structure_designer::node_network::Argument;
        let map_node = network.nodes.get_mut(&map_id).unwrap();
        if map_node.zone_output_arguments.is_empty() {
            map_node.zone_output_arguments.push(Argument::new());
        }
        map_node.zone_output_arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: body_node,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }
    designer.validate_active_network();

    for id in [map_id, collect_id] {
        let errors = blocking_errors_for(&designer, "main", id);
        assert!(
            errors.iter().any(|t| t.contains("Wire cycle detected")),
            "cycle member {} must carry the cycle error, got {:?}",
            id,
            errors
        );
    }
    // Evaluation of the cycle terminates.
    assert!(matches!(
        evaluate_node(&designer, "main", collect_id),
        NetworkResult::Error(_)
    ));
}

/// Nested-body variant: the capture sits two body levels deep
/// (`source_scope_depth == 2`), so the projection must walk the whole body
/// subtree, not just the immediate body.
#[test]
fn capture_threaded_cycle_through_nested_body() {
    let mut designer = setup_designer_with_network("main");
    let outer_map = designer.add_node("map", DVec2::ZERO);
    let collect_id = designer.add_node("collect", DVec2::new(200.0, 0.0));
    designer.connect_nodes(outer_map, 0, collect_id, 0);
    let inner_map = designer.add_node_scoped(&[outer_map], "map", DVec2::ZERO, None);
    let inner_body_node =
        designer.add_node_scoped(&[outer_map, inner_map], "collect", DVec2::ZERO, None);
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        let outer_body = network
            .nodes
            .get_mut(&outer_map)
            .unwrap()
            .zone_mut()
            .unwrap();
        let inner_body = outer_body
            .nodes
            .get_mut(&inner_map)
            .unwrap()
            .zone_mut()
            .unwrap();
        inner_body
            .nodes
            .get_mut(&inner_body_node)
            .unwrap()
            .arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: collect_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 2,
            });
    }
    designer.validate_active_network();

    for id in [outer_map, collect_id] {
        let errors = blocking_errors_for(&designer, "main", id);
        assert!(
            errors.iter().any(|t| t.contains("Wire cycle detected")),
            "cycle member {} must carry the cycle error, got {:?}",
            id,
            errors
        );
    }
}

/// `closure`-node variant: closures freeze captures at the closure node's own
/// eval, so the projected dependency sits on the closure node — a cycle
/// `closure body captures X; X ← closure.out` is flagged on both.
#[test]
fn capture_threaded_cycle_through_closure() {
    let mut designer = setup_designer_with_network("main");
    let closure_id = designer.add_node("closure", DVec2::ZERO);
    let apply_id = designer.add_node("apply", DVec2::new(200.0, 0.0));
    designer.connect_nodes(closure_id, 0, apply_id, 0);
    let body_node = designer.add_node_scoped(&[closure_id], "collect", DVec2::ZERO, None);
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        let body = network
            .nodes
            .get_mut(&closure_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        body.nodes.get_mut(&body_node).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: apply_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 1,
            });
    }
    designer.validate_active_network();

    for id in [closure_id, apply_id] {
        let errors = blocking_errors_for(&designer, "main", id);
        assert!(
            errors.iter().any(|t| t.contains("Wire cycle detected")),
            "cycle member {} must carry the cycle error, got {:?}",
            id,
            errors
        );
    }
}

// ============================================================================
// Evaluator re-entrancy backstop
// ============================================================================

/// An escaped cycle (validation never ran — simulating a hand-authored file
/// slipping past) terminates via the `eval_in_progress` guard with a
/// localized error instead of hanging or overflowing the stack.
#[test]
fn re_entrancy_guard_catches_escaped_cycle() {
    let mut designer = setup_designer_with_network("main");
    let e1 = add_expr(&mut designer, "main", "x", vec![("x", DataType::Float)]);
    let e2 = add_expr(&mut designer, "main", "x", vec![("x", DataType::Float)]);
    push_wire(&mut designer, "main", e2, e1, 0);
    push_wire(&mut designer, "main", e1, e2, 0);
    // Deliberately do NOT validate — and drop any state a previous validate
    // left, so no poison entry protects the evaluator.
    designer
        .node_type_registry
        .node_networks
        .get_mut("main")
        .unwrap()
        .validation_errors
        .clear();

    match evaluate_node(&designer, "main", e1) {
        NetworkResult::Error(text) => assert!(
            text.contains("evaluation cycle detected"),
            "expected the guard's localized error, got: {}",
            text
        ),
        other => panic!("expected Error, got {}", other.to_display_string()),
    }
}

/// Heavy legitimate re-evaluation must never trip the guard: a diamond
/// (same node evaluated twice sequentially in one pass) and one custom
/// network used by two instances (distinct scope paths → distinct NodeRefs).
#[test]
fn re_entrancy_guard_ignores_legitimate_re_evaluation() {
    // Diamond: x feeds two exprs which feed a two-input expr.
    let mut designer = setup_designer_with_network("main");
    let x = designer.add_node("int", DVec2::ZERO);
    let y = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)]);
    let z = add_expr(&mut designer, "main", "x + 2", vec![("x", DataType::Int)]);
    let w = add_expr(
        &mut designer,
        "main",
        "a + b",
        vec![("a", DataType::Int), ("b", DataType::Int)],
    );
    designer.connect_nodes(x, 0, y, 0);
    designer.connect_nodes(x, 0, z, 0);
    designer.connect_nodes(y, 0, w, 0);
    designer.connect_nodes(z, 0, w, 1);
    designer.validate_active_network();
    match evaluate_node(&designer, "main", w) {
        NetworkResult::Int(v) => assert_eq!(v, 3),
        other => panic!("diamond must evaluate, got {}", other.to_display_string()),
    }

    // Two instances of one custom network in a single expression.
    let mut designer = setup_designer_with_network("child");
    let int_id = designer.add_node("int", DVec2::ZERO);
    designer.set_return_node_id(Some(int_id));
    designer.validate_active_network();
    designer.add_node_network("parent");
    designer.set_active_node_network_name(Some("parent".to_string()));
    let i1 = designer.add_node("child", DVec2::ZERO);
    let i2 = designer.add_node("child", DVec2::new(0.0, 100.0));
    let sum = add_expr(
        &mut designer,
        "parent",
        "a + b",
        vec![("a", DataType::Int), ("b", DataType::Int)],
    );
    designer.connect_nodes(i1, 0, sum, 0);
    designer.connect_nodes(i2, 0, sum, 1);
    designer.validate_active_network();
    match evaluate_node(&designer, "parent", sum) {
        NetworkResult::Int(v) => assert_eq!(v, 0),
        other => panic!(
            "two-instance sum must evaluate, got {}",
            other.to_display_string()
        ),
    }
}

// ============================================================================
// Serde compatibility
// ============================================================================

/// The new `interface` field round-trips, and legacy serialized errors
/// (without `interface`, or even without `blocking`) keep loading with the
/// backward-compatible defaults.
#[test]
fn validation_error_serde_roundtrip_and_legacy_default() {
    let e = ValidationError::interface_error("broken interface".to_string(), Some(7));
    let json = serde_json::to_string(&e).unwrap();
    let back: ValidationError = serde_json::from_str(&json).unwrap();
    assert!(back.interface);
    assert!(back.blocking);
    assert_eq!(back.node_id, Some(7));

    let legacy: ValidationError =
        serde_json::from_str(r#"{"error_text":"old","node_id":3}"#).unwrap();
    assert!(legacy.blocking, "legacy errors default to blocking");
    assert!(!legacy.interface, "legacy errors default to non-interface");

    // The predicate helpers agree with the D5 classification.
    assert!(!has_interface_residue(&[ValidationError::new(
        "attributed".to_string(),
        Some(1)
    )]));
    assert!(has_interface_residue(&[ValidationError::new(
        "unattributed".to_string(),
        None
    )]));
    assert!(has_interface_residue(&[e]));
    assert_eq!(
        node_poison_message(
            &[
                ValidationError::new("first".to_string(), Some(1)),
                ValidationError::new("second".to_string(), Some(1)),
                ValidationError::warning("advisory".to_string(), Some(1)),
                ValidationError::new("other node".to_string(), Some(2)),
            ],
            1
        )
        .as_deref(),
        Some("first\nsecond"),
        "poison text joins the node's blocking texts only"
    );
}
