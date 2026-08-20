//! Phase 2 of `doc/design_eval_profiling.md`: the opt-in per-node profiler.
//!
//! **No wall-clock assertions live here**, per the design's testing
//! conventions. "Self time is under half the total" is flaky on any machine
//! and worthless on a loaded one. What is asserted is *structure*: evaluation
//! counts, node identities, the self-vs-total relations the child-accumulator
//! scheme guarantees, and — the two that matter most — that the RAII guard is
//! released on every exit path and that the eager-HOF context split does not
//! swallow body records.
//!
//! The profiler is driven the way the application drives it: through
//! `StructureDesigner::refresh`, which is the only production caller that
//! arms `with_eval_context` and hands back a `RefreshSubPhases` carrying the
//! table. Reaching into `eval_profiler::install` directly would test the
//! module and not the wiring.

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::evaluator::eval_profiler::{EvalProfile, NodeProfileRecord};
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkStackElement, node_profile_key,
};
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_network::{Argument, IncomingWire, NodeRef, SourcePin};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::expr::{ExprData, ExprParameter};
use atomcad_structure_designer::nodes::fold::FoldData;
use atomcad_structure_designer::nodes::int::IntData;
use atomcad_structure_designer::nodes::range::RangeData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::structure_designer_changes::{
    RefreshMode, StructureDesignerChanges,
};
use glam::f64::DVec2;
use std::sync::Arc;

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

/// Add an `expr` node to the *top-level* network.
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

/// Add an `expr` node into an HOF's inline zone body.
fn add_expr_to_body(
    designer: &mut StructureDesigner,
    network_name: &str,
    hof_node_id: u64,
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
    let body = registry
        .node_networks
        .get_mut(network_name)
        .unwrap()
        .nodes
        .get_mut(&hof_node_id)
        .unwrap()
        .zone_mut()
        .expect("HOF node missing zone");
    let expr_id = body.add_node(
        "expr",
        DVec2::new(50.0, 0.0),
        num_params,
        Box::new(expr_data),
    );

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
            .get_mut(&expr_id)
            .unwrap(),
        true,
    );
    expr_id
}

fn wire_zone_input_pin_to_body_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    hof_node_id: u64,
    zone_input_pin: usize,
    body_node_id: u64,
    body_param_index: usize,
) {
    let body = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap()
        .nodes
        .get_mut(&hof_node_id)
        .unwrap()
        .zone_mut()
        .unwrap();
    body.nodes.get_mut(&body_node_id).unwrap().arguments[body_param_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: hof_node_id,
            source_pin: SourcePin::ZoneInput {
                pin_index: zone_input_pin,
            },
            source_scope_depth: 1,
        });
}

fn wire_body_node_to_zone_output(
    designer: &mut StructureDesigner,
    network_name: &str,
    hof_node_id: u64,
    body_node_id: u64,
) {
    let hof_node = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap()
        .nodes
        .get_mut(&hof_node_id)
        .unwrap();
    if hof_node.zone_output_arguments.is_empty() {
        hof_node.zone_output_arguments.push(Argument::new());
    }
    hof_node.zone_output_arguments[0]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: body_node_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
        });
    // These helpers poke the registry directly, so re-validate by hand — the
    // stale "zone-output has no incoming wire" error is blocking and would
    // cone-poison the HOF before its `eval` ever ran.
    designer.validate_active_network();
}

/// Push a raw wire, bypassing `can_connect_nodes` (hand-authored-file style).
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
    network.nodes.get_mut(&dest).unwrap().arguments[param_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: source,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
        });
}

/// Make `node_id` the network's **only** displayed root.
///
/// Load-bearing, not tidiness: the display policy shows freshly added nodes, so
/// a network built node-by-node ends up with every node displayed — and a
/// refresh then evaluates each of them as its own root. A 34-node chain that
/// way is 595 evaluations rather than 34, and every count in this file would be
/// measuring the fixture instead of the evaluator.
fn display_only(designer: &mut StructureDesigner, network_name: &str, node_id: u64) {
    designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap()
        .displayed_nodes
        .clear();
    designer.set_node_display(node_id, true);
}

fn full_refresh(designer: &mut StructureDesigner) -> Option<Arc<EvalProfile>> {
    let changes = StructureDesignerChanges {
        mode: RefreshMode::Full,
        ..Default::default()
    };
    designer.refresh(&changes).node_stats
}

/// Refresh with the profiler armed and return the table it produced.
fn profiled_full_refresh(designer: &mut StructureDesigner) -> Arc<EvalProfile> {
    designer.eval_profiling_enabled = true;
    full_refresh(designer).expect("a profiled full refresh must produce a table")
}

/// Every record whose label names the given node type.
fn records_of_type<'a>(
    profile: &'a EvalProfile,
    node_type_name: &str,
) -> Vec<&'a NodeProfileRecord> {
    profile
        .records()
        .iter()
        .filter(|r| r.location.node_type_name == node_type_name)
        .collect()
}

/// The single record for a top-level node of the active network.
///
/// Filters on the host network as well as the id: `next_node_id` is
/// **per-network**, so `#1` exists in every network a fixture defines and an
/// id-only lookup silently returns whichever one was recorded first.
fn record_for(profile: &EvalProfile, node_id: u64) -> &NodeProfileRecord {
    profile
        .records()
        .iter()
        .find(|r| {
            r.location.node_id == node_id
                && r.location.scope_path.is_empty()
                && r.location.host_network == "main"
        })
        .unwrap_or_else(|| {
            panic!(
                "no record for top-level node {node_id}; recorded: {:?}",
                profile
                    .records()
                    .iter()
                    .map(|r| r.location.label.as_str())
                    .collect::<Vec<_>>()
            )
        })
}

// ============================================================================
// Evaluation counts (D3, D5)
// ============================================================================

/// The canonical fan-out shape: `apex` feeds two consumers that both feed the
/// displayed sink, so the evaluator — which memoizes nothing — runs `apex`
/// **twice** in one pass. This count *is* the redundancy Phase 3 will measure,
/// so it is the first thing the profiler has to get right.
#[test]
fn diamond_records_the_apex_twice() {
    let mut designer = setup_designer_with_network("main");

    let apex = add_expr(&mut designer, "main", "7", vec![]);
    let left = add_expr(&mut designer, "main", "a + 1", vec![("a", DataType::Int)]);
    let right = add_expr(&mut designer, "main", "a + 2", vec![("a", DataType::Int)]);
    let sink = add_expr(
        &mut designer,
        "main",
        "a + b",
        vec![("a", DataType::Int), ("b", DataType::Int)],
    );
    push_wire(&mut designer, "main", apex, left, 0);
    push_wire(&mut designer, "main", apex, right, 0);
    push_wire(&mut designer, "main", left, sink, 0);
    push_wire(&mut designer, "main", right, sink, 1);
    designer.validate_active_network();
    display_only(&mut designer, "main", sink);

    let profile = profiled_full_refresh(&mut designer);

    assert_eq!(
        record_for(&profile, apex).evaluations,
        2,
        "the diamond apex is pulled once per consumer"
    );
    assert_eq!(record_for(&profile, left).evaluations, 1);
    assert_eq!(record_for(&profile, right).evaluations, 1);
    assert_eq!(record_for(&profile, sink).evaluations, 1);
}

/// A straight chain has no fan-out, so every node is evaluated exactly once —
/// the control case for the diamond above. A profiler that double-counted
/// (e.g. by hooking both `evaluate` and `evaluate_all_outputs` on a path where
/// one delegates to the other) would fail here, not there.
#[test]
fn chain_records_each_node_once() {
    let mut designer = setup_designer_with_network("main");

    let a = add_expr(&mut designer, "main", "1", vec![]);
    let b = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)]);
    let c = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)]);
    push_wire(&mut designer, "main", a, b, 0);
    push_wire(&mut designer, "main", b, c, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", c);

    let profile = profiled_full_refresh(&mut designer);

    for id in [a, b, c] {
        assert_eq!(
            record_for(&profile, id).evaluations,
            1,
            "chain node {id} should be evaluated exactly once"
        );
    }
}

/// Two *instances* of one custom network aggregate under the same node
/// identity: the home frame is identified by the network's address, and a
/// registry frame's own `node_id` is deliberately not part of that identity
/// (D5). The table answers "how expensive is this node", not "how expensive
/// was this call site".
#[test]
fn two_instances_of_one_custom_network_share_a_record() {
    let mut designer = setup_designer_with_network("main");
    designer.add_node_network("helper");

    // helper: a single `expr` returning a constant, marked as the return node.
    designer.set_active_node_network_name(Some("helper".to_string()));
    let inner = add_expr(&mut designer, "helper", "3", vec![]);
    designer.set_return_node_id(Some(inner));
    designer.validate_active_network();

    designer.set_active_node_network_name(Some("main".to_string()));
    let a = designer.add_node("helper", DVec2::ZERO);
    let b = designer.add_node("helper", DVec2::new(0.0, 100.0));
    let sink = add_expr(
        &mut designer,
        "main",
        "x + y",
        vec![("x", DataType::Int), ("y", DataType::Int)],
    );
    push_wire(&mut designer, "main", a, sink, 0);
    push_wire(&mut designer, "main", b, sink, 1);
    designer.validate_active_network();
    display_only(&mut designer, "main", sink);

    let profile = profiled_full_refresh(&mut designer);

    // The two instance nodes are distinct nodes in `main` and get their own
    // rows; the node *inside* `helper` gets exactly one row carrying both
    // evaluations.
    let inner_records: Vec<_> = profile
        .records()
        .iter()
        .filter(|r| r.location.node_id == inner && r.location.host_network == "helper")
        .collect();
    assert_eq!(
        inner_records.len(),
        1,
        "the custom network's internal node must aggregate into ONE row across \
         both instances; got {:?}",
        profile
            .records()
            .iter()
            .map(|r| r.location.label.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        inner_records[0].evaluations, 2,
        "one row, two evaluations — one per instance"
    );
}

// ============================================================================
// Self-vs-total invariants (D4)
// ============================================================================

/// The two relations the child-accumulator scheme guarantees, asserted as
/// relations between recorded numbers rather than as thresholds: no record can
/// spend more time in its own `eval` than in the whole evaluation, and the
/// summed totals of a node's children cannot exceed its own total.
#[test]
fn self_time_never_exceeds_total_and_children_fit_inside_the_parent() {
    let mut designer = setup_designer_with_network("main");

    let a = add_expr(&mut designer, "main", "1", vec![]);
    let b = add_expr(&mut designer, "main", "2", vec![]);
    let sink = add_expr(
        &mut designer,
        "main",
        "x + y",
        vec![("x", DataType::Int), ("y", DataType::Int)],
    );
    push_wire(&mut designer, "main", a, sink, 0);
    push_wire(&mut designer, "main", b, sink, 1);
    designer.validate_active_network();
    display_only(&mut designer, "main", sink);

    let profile = profiled_full_refresh(&mut designer);

    for record in profile.records() {
        assert!(
            record.self_ns <= record.total_ns,
            "{}: self_ns {} > total_ns {}",
            record.location.label,
            record.self_ns,
            record.total_ns
        );
    }

    let children_total = record_for(&profile, a).total_ns + record_for(&profile, b).total_ns;
    let sink_record = record_for(&profile, sink);
    assert!(
        children_total <= sink_record.total_ns,
        "the sink's two inputs summed to {children_total} ns but the sink's own \
         total is {} ns — the child accumulator is not nesting",
        sink_record.total_ns
    );
}

// ============================================================================
// Guard release (D4) — the two exits that bypass the tail of `evaluate`
// ============================================================================

/// A wire cycle trips the re-entrancy backstop, which returns from the middle
/// of `evaluate`. A leaked guard frame there would silently corrupt every
/// ancestor's self time, so the pass must end with an empty accumulator stack.
#[test]
fn a_wire_cycle_leaves_no_leaked_guard_frame() {
    let mut designer = setup_designer_with_network("main");

    let a = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)]);
    let b = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)]);
    // a → b → a: hand-authored cycle, exactly what the backstop exists for.
    push_wire(&mut designer, "main", a, b, 0);
    push_wire(&mut designer, "main", b, a, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", b);

    let profile = profiled_full_refresh(&mut designer);

    assert_eq!(
        profile.live_frame_count(),
        0,
        "the child-accumulator stack must be empty at end of pass"
    );
}

/// The central `Unit`-skip rule returns before the dispatch too. `export_atoms`
/// is `Unit`-returning, so on an ordinary display pass its `eval` never runs —
/// but the frame still has to be released.
#[test]
fn a_unit_skipped_effect_node_leaves_no_leaked_guard_frame() {
    let mut designer = setup_designer_with_network("main");

    let effect = designer.add_node("export_atoms", DVec2::ZERO);
    designer.validate_active_network();
    display_only(&mut designer, "main", effect);

    let profile = profiled_full_refresh(&mut designer);

    assert_eq!(
        profile.live_frame_count(),
        0,
        "the child-accumulator stack must be empty at end of pass"
    );
    // The skip path is still an evaluation from the profiler's point of view:
    // it resolves every output pin's type, which is real work worth attributing.
    assert_eq!(
        records_of_type(&profile, "export_atoms").len(),
        1,
        "a Unit-skipped node still gets a row"
    );
}

// ============================================================================
// The eager-HOF context split (D4/D6) — the bug this design exists to prevent
// ============================================================================

/// `fold` evaluates its body against a `fresh_inner_for_eager_body` context
/// whose `drain_inner_context` merges **`print_buffer` and nothing else**. With
/// a context-owned profiler these body records would not exist at all.
#[test]
fn fold_body_nodes_are_recorded_at_all() {
    let (mut designer, fold_id, expr_id) = build_fold_over_three();
    let profile = profiled_full_refresh(&mut designer);

    let body_records: Vec<_> = profile
        .records()
        .iter()
        .filter(|r| r.location.node_id == expr_id && !r.location.scope_path.is_empty())
        .collect();
    assert_eq!(
        body_records.len(),
        1,
        "the fold body's expr must produce a record; recorded: {:?}",
        profile
            .records()
            .iter()
            .map(|r| r.location.label.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        body_records[0].evaluations, 3,
        "one body evaluation per element"
    );
    assert!(
        body_records[0].location.scope_path.contains(&fold_id),
        "a body record's scope path must name the HOF that owns the body"
    );
}

/// The other half of the same bug: with a context-owned accumulator stack the
/// body's time would never reach the HOF's child accumulator and `fold` would
/// be charged the **entire body cost as self time**. Stated as a relation
/// between two recorded numbers, never as a ratio.
#[test]
fn fold_body_time_is_subtracted_from_the_fold() {
    let (mut designer, fold_id, expr_id) = build_fold_over_three();
    let profile = profiled_full_refresh(&mut designer);

    let fold_record = record_for(&profile, fold_id);
    let body_total: u64 = profile
        .records()
        .iter()
        .filter(|r| r.location.node_id == expr_id && !r.location.scope_path.is_empty())
        .map(|r| r.total_ns)
        .sum();

    assert!(
        fold_record.total_ns - fold_record.self_ns >= body_total,
        "fold total {} − self {} = {} must cover the body's {} ns; the eager-body \
         context split is losing the body's time",
        fold_record.total_ns,
        fold_record.self_ns,
        fold_record.total_ns - fold_record.self_ns,
        body_total
    );
}

/// `range(1..4) → fold(acc + element, init = 0)`, with a displayed sink so a
/// refresh actually evaluates it. Returns `(designer, fold_id, body_expr_id)`.
fn build_fold_over_three() -> (StructureDesigner, u64, u64) {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::ZERO);
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

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        fold_id,
        "acc + elem",
        vec![("acc", DataType::Int), ("elem", DataType::Int)],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 0, expr_id, 0);
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 1, expr_id, 1);
    wire_body_node_to_zone_output(&mut designer, "main", fold_id, expr_id);

    display_only(&mut designer, "main", fold_id);
    (designer, fold_id, expr_id)
}

// ============================================================================
// The toggle (D1/D2) and non-interference
// ============================================================================

/// With the toggle off there is no table and no accumulation — the release
/// build the maintainer runs must pay one thread-local `bool` read and nothing
/// else.
#[test]
fn the_toggle_is_honoured() {
    let mut designer = setup_designer_with_network("main");
    let node = add_expr(&mut designer, "main", "1", vec![]);
    designer.validate_active_network();
    display_only(&mut designer, "main", node);

    assert!(
        !designer.eval_profiling_enabled,
        "the profiler is off by default — it is not a persisted preference"
    );
    assert!(
        full_refresh(&mut designer).is_none(),
        "an unprofiled refresh must report no node table"
    );

    designer.eval_profiling_enabled = true;
    assert!(full_refresh(&mut designer).is_some());

    designer.eval_profiling_enabled = false;
    assert!(
        full_refresh(&mut designer).is_none(),
        "switching the toggle back off must stop accumulation immediately"
    );
}

/// A profiler that perturbs the pass it measures is worse than none. The same
/// fixture, evaluated with the toggle off and then on, must produce identical
/// node output strings.
#[test]
fn profiling_does_not_change_results() {
    fn output_strings(profiling: bool) -> Vec<String> {
        let (mut designer, fold_id, _) = build_fold_over_three();
        designer.eval_profiling_enabled = profiling;
        full_refresh(&mut designer);
        let scene = &designer.last_generated_structure_designer_scene;
        let mut strings: Vec<String> = scene
            .node_data
            .get(&NodeRef::top(fold_id))
            .map(|data| {
                data.node_output_strings
                    .values()
                    .flat_map(|pins| pins.iter().cloned())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();
        strings.sort();
        strings
    }

    let without = output_strings(false);
    let with = output_strings(true);
    assert!(
        !without.is_empty(),
        "fixture produced no output strings — the test would be vacuous"
    );
    assert_eq!(
        without, with,
        "arming the profiler changed the pass's results"
    );
}

// ============================================================================
// NodeLocation round-trip (D5) — click-to-jump has to land somewhere
// ============================================================================

/// A record for a node **inside a zone body** must carry a `scope_path` +
/// `node_id` that the same scope walk the canvas navigation uses resolves back
/// to that node. Without this the panel's click-to-jump lands nowhere and only
/// manual testing would notice.
#[test]
fn a_body_record_addresses_a_node_the_navigation_can_resolve() {
    let (mut designer, fold_id, expr_id) = build_fold_over_three();
    let profile = profiled_full_refresh(&mut designer);

    let record = profile
        .records()
        .iter()
        .find(|r| r.location.node_id == expr_id && !r.location.scope_path.is_empty())
        .expect("body record missing");

    assert_eq!(
        record.location.host_network, "main",
        "the jump target names the network to activate"
    );
    assert!(
        record.location.navigable,
        "a plain zone-body node is reachable by the scope walk"
    );

    // The same walk the canvas navigation performs: activate the host network,
    // resolve the scope chain, look the node up in the body it lands in.
    let address = record.location.clone();
    designer.set_active_node_network_name(Some(address.host_network.clone()));
    let scope_network = designer
        .get_scope_network(&address.scope_path)
        .expect("the record's scope path must resolve to a body");
    assert!(
        scope_network.nodes.contains_key(&address.node_id),
        "the record's address ({:?}, #{}) does not resolve — click-to-jump would \
         land nowhere",
        address.scope_path,
        address.node_id
    );
    assert_eq!(record.location.scope_path, vec![fold_id]);
    assert!(
        record.location.label.starts_with("main/fold#"),
        "unexpected label: {}",
        record.location.label
    );
}

// ============================================================================
// The aggregation key itself (D5)
// ============================================================================

/// A **body** frame's identity is `(identity of the frame below, owner node
/// id)` and must not hash the body network's address: this key is retained past
/// the life of its frames, and both kinds of body network get dropped mid-pass.
/// Two body frames over different allocations with the same owner id therefore
/// hash **equal**, while two over the same allocation with different owner ids
/// hash **differently**.
#[test]
fn the_body_network_address_is_not_an_input_to_the_key() {
    let mut designer = setup_designer_with_network("main");
    designer.add_node_network("other");
    let root = designer.node_type_registry.node_networks["main"].clone();
    // Two *separately allocated* body networks standing in for the two body
    // networks a real pass constructs and drops.
    let body_a = designer.node_type_registry.node_networks["other"].clone();
    let body_b = designer.node_type_registry.node_networks["other"].clone();
    assert_ne!(
        &body_a as *const _, &body_b as *const _,
        "the fixture needs two distinct allocations"
    );

    let stack_a = vec![
        NetworkStackElement {
            is_zone_body: false,
            node_network: &root,
            node_id: 0,
        },
        NetworkStackElement {
            is_zone_body: true,
            node_network: &body_a,
            node_id: 12,
        },
    ];
    let stack_b = vec![
        NetworkStackElement {
            is_zone_body: false,
            node_network: &root,
            node_id: 0,
        },
        NetworkStackElement {
            is_zone_body: true,
            node_network: &body_b,
            node_id: 12,
        },
    ];
    assert_eq!(
        node_profile_key(&stack_a, 5),
        node_profile_key(&stack_b, 5),
        "two allocations of the same body must share a key — otherwise a \
         re-allocated body silently splits a table row"
    );

    // Same allocation, different owner id → different key. `node_id` counters
    // are per-network, so this is what keeps two HOFs' bodies apart.
    let stack_c = vec![
        NetworkStackElement {
            is_zone_body: false,
            node_network: &root,
            node_id: 0,
        },
        NetworkStackElement {
            is_zone_body: true,
            node_network: &body_a,
            node_id: 13,
        },
    ];
    assert_ne!(node_profile_key(&stack_a, 5), node_profile_key(&stack_c, 5));
}

/// A registry frame's own `node_id` is not part of its identity — that is what
/// makes two instances of one custom network aggregate (the end-to-end version
/// is `two_instances_of_one_custom_network_share_a_record`).
#[test]
fn a_registry_frames_node_id_is_not_part_of_its_identity() {
    let designer = setup_designer_with_network("main");
    let network = designer.node_type_registry.node_networks["main"].clone();

    let as_instance_7 = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: &network,
        node_id: 7,
    }];
    let as_instance_9 = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: &network,
        node_id: 9,
    }];
    assert_eq!(
        node_profile_key(&as_instance_7, 3),
        node_profile_key(&as_instance_9, 3)
    );
}

// ============================================================================
// Stack budget (D4)
// ============================================================================

/// The guard adds a frame to the recursion the STACK-SIZE WARNING on
/// `evaluate_all_outputs` is about, and deep node chains run close to the
/// debug-build thread stack limit. This mirrors `tag_test`'s 33-node chain with
/// profiling **on**: if the added frame blows the budget, it overflows here.
#[test]
fn a_deep_chain_survives_with_profiling_on() {
    let mut designer = setup_designer_with_network("main");

    let mut previous = add_expr(&mut designer, "main", "0", vec![]);
    for _ in 0..33 {
        let next = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)]);
        push_wire(&mut designer, "main", previous, next, 0);
        previous = next;
    }
    designer.validate_active_network();
    display_only(&mut designer, "main", previous);

    let profile = profiled_full_refresh(&mut designer);
    assert_eq!(profile.records().len(), 34, "one record per chain node");
    assert_eq!(profile.total_evaluations(), 34);
    assert_eq!(profile.live_frame_count(), 0);
}

// ============================================================================
// Roll-up by node type (D5) — both tables come from one map
// ============================================================================

/// The "By node type" table is a roll-up of the same records, so its totals
/// agree with the per-node table by construction. Asserting that keeps a future
/// second traversal from drifting.
#[test]
fn the_by_type_rollup_agrees_with_the_per_node_table() {
    let mut designer = setup_designer_with_network("main");
    let a = add_expr(&mut designer, "main", "1", vec![]);
    let b = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)]);
    push_wire(&mut designer, "main", a, b, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", b);

    let profile = profiled_full_refresh(&mut designer);
    let by_type = profile.by_node_type();

    let expr_row = by_type
        .iter()
        .find(|r| r.node_type_name == "expr")
        .expect("expr roll-up missing");
    assert_eq!(expr_row.nodes, 2, "two distinct expr nodes");
    assert_eq!(
        expr_row.evaluations,
        records_of_type(&profile, "expr")
            .iter()
            .map(|r| r.evaluations)
            .sum::<u64>()
    );
    assert_eq!(
        by_type.iter().map(|r| r.evaluations).sum::<u64>(),
        profile.total_evaluations(),
        "the roll-up must account for every evaluation"
    );
    assert_eq!(
        by_type.iter().map(|r| r.self_ns).sum::<u64>(),
        profile.total_self_ns()
    );
}

/// A pass that is **not** a refresh — an Execute action or a CLI run — also goes
/// through `with_eval_context` and leaves its table parked on the designer. A
/// later refresh must not pick that up and report it as its own, which is
/// exactly what a partial refresh with nothing to re-evaluate would otherwise
/// do (it never enters `with_eval_context` to overwrite it).
#[test]
fn a_refresh_does_not_inherit_a_non_refresh_passs_table() {
    let mut designer = setup_designer_with_network("main");
    let node = add_expr(&mut designer, "main", "1", vec![]);
    designer.validate_active_network();
    display_only(&mut designer, "main", node);

    // Stand in for an Execute pass: a profiled `with_eval_context` that is not
    // a refresh.
    designer.eval_profiling_enabled = true;
    designer.with_eval_context(true, |_evaluator, _registry, _prefs, _context| {});
    designer.eval_profiling_enabled = false;

    // A partial refresh with no tracked changes re-evaluates nothing.
    let sub_phases = designer.refresh(&StructureDesignerChanges {
        mode: RefreshMode::Partial,
        ..Default::default()
    });
    assert!(
        sub_phases.node_stats.is_none(),
        "the refresh reported a table it did not produce"
    );
}

/// **A node inside a custom network is navigable, and its address names that
/// network.**
///
/// The jump activates a network by *name* and routinely crosses network
/// boundaries (that is how error navigation lands on a root cause elsewhere),
/// so there is nothing special to do here — as long as the recorded address is
/// **home-relative**. Recording it relative to the pass's root instead yields
/// `(root, [instance_id], node_id)`, which no `Node.zone` walk can follow, and
/// the row has to be greyed out for no reason. That is the regression this
/// guards.
#[test]
fn a_custom_network_internal_is_navigable_in_its_own_network() {
    let mut designer = setup_designer_with_network("main");
    designer.add_node_network("helper");

    designer.set_active_node_network_name(Some("helper".to_string()));
    let inner = add_expr(&mut designer, "helper", "3", vec![]);
    designer.set_return_node_id(Some(inner));
    designer.validate_active_network();

    designer.set_active_node_network_name(Some("main".to_string()));
    let instance = designer.add_node("helper", DVec2::ZERO);
    let sink = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)]);
    push_wire(&mut designer, "main", instance, sink, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", sink);

    let profile = profiled_full_refresh(&mut designer);
    let record = profile
        .records()
        .iter()
        .find(|r| r.location.node_id == inner && r.location.node_type_name == "expr")
        .filter(|r| r.location.host_network == "helper")
        .expect("the custom network's internal node must be recorded under `helper`");

    assert!(
        record.location.navigable,
        "a node inside a custom network has exactly one canvas position — in \
         that network — and must be jumpable"
    );
    assert!(
        record.location.scope_path.is_empty(),
        "the address is relative to its own network, so the instance id must \
         NOT appear in the scope path; got {:?}",
        record.location.scope_path
    );
    assert!(
        record.location.label.starts_with("helper/"),
        "the label already named the home network: {}",
        record.location.label
    );

    // The jump resolves: activate the host network, walk the scope chain.
    designer.set_active_node_network_name(Some(record.location.host_network.clone()));
    let scope_network = designer
        .get_scope_network(&record.location.scope_path)
        .expect("host network must resolve");
    assert!(scope_network.nodes.contains_key(&record.location.node_id));
}

/// A caller-side node pulled through a custom network's `parameter` is
/// evaluated during a **stack excursion**: the network-stack frame is popped
/// while the instance's eval scope stays pushed (`nodes/parameter.rs`). An
/// address read off `eval_scope_path` would therefore carry an instance id the
/// node does not live under, and the click would land nowhere. The network
/// stack is excursion-correct, which is why the address is derived from it.
#[test]
fn a_node_reached_through_a_parameter_excursion_keeps_its_own_address() {
    let mut designer = setup_designer_with_network("main");
    designer.add_node_network("helper");

    // helper: parameter → expr(return), so evaluating the instance pulls the
    // caller's wire through the parameter excursion.
    designer.set_active_node_network_name(Some("helper".to_string()));
    let param = designer.add_node("parameter", DVec2::ZERO);
    let inner = add_expr(&mut designer, "helper", "x + 1", vec![("x", DataType::Int)]);
    push_wire(&mut designer, "helper", param, inner, 0);
    designer.set_return_node_id(Some(inner));
    designer.validate_active_network();

    designer.set_active_node_network_name(Some("main".to_string()));
    let feeder = add_expr(&mut designer, "main", "7", vec![]);
    let instance = designer.add_node("helper", DVec2::new(200.0, 0.0));
    push_wire(&mut designer, "main", feeder, instance, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", instance);

    let profile = profiled_full_refresh(&mut designer);
    let record = record_for(&profile, feeder);

    assert_eq!(
        record.location.host_network, "main",
        "the feeder lives in `main`, whatever scope the excursion left pushed"
    );
    assert!(
        record.location.scope_path.is_empty(),
        "a top-level node's address must not pick up the instance id the \
         excursion left on the eval scope path; got {:?}",
        record.location.scope_path
    );
    assert!(record.location.navigable);
}
