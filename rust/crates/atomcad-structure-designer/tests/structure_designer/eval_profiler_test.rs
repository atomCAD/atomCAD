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
use atomcad_structure_designer::evaluator::eval_profiler::{
    EvalProfile, NodeLocation, NodeProfileRecord, RecordFlags, SelfCheckKeyMode,
};
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkStackElement, eval_env_key, node_profile_key,
};
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_network::{Argument, IncomingWire, NodeRef, SourcePin};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::closure::{ClosureData, ClosureKind};
use atomcad_structure_designer::nodes::collect::CollectData;
use atomcad_structure_designer::nodes::expr::{ExprData, ExprParameter};
use atomcad_structure_designer::nodes::fold::FoldData;
use atomcad_structure_designer::nodes::int::IntData;
use atomcad_structure_designer::nodes::map::MapData;
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
    // **The memo is pinned off for this whole file.** The profiler's redundancy
    // numbers are *defined* against the un-memoized evaluator — `evaluations`
    // is what a pass costs when nothing is shared, and the diamond-apex-twice
    // counts below are the measurement `doc/design_eval_memoization.md` was
    // built on. With the memo on (the product default since its Phase 3) those
    // counts collapse, which is the memo working and not the profiler
    // miscounting. The memo's own effect on these columns is asserted in
    // `eval_memo_test.rs`, where it is the subject rather than the environment.
    designer.eval_memo_enabled = false;
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
        NetworkStackElement::root(&root),
        NetworkStackElement::body_static(&body_a, 12),
    ];
    let stack_b = vec![
        NetworkStackElement::root(&root),
        NetworkStackElement::body_static(&body_b, 12),
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
        NetworkStackElement::root(&root),
        NetworkStackElement::body_static(&body_a, 13),
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

    let as_instance_7 = vec![NetworkStackElement::instance(&network, 7)];
    let as_instance_9 = vec![NetworkStackElement::instance(&network, 9)];
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

// ============================================================================
// Phase 3 — the environment key and redundancy (D9, D10, D11)
// ============================================================================
//
// The distinction every test below turns on: **an evaluation is redundant only
// if it ran in an environment that had already been evaluated.** A diamond apex
// pulled twice is one environment and one wasted evaluation; a body node run
// once per element over three elements is three environments and no waste at
// all. Counting evaluations alone cannot tell those apart, which is why a
// static count could never produce a trustworthy ratio (design doc,
// §Motivation).

/// Refresh with the profiler **and** the D11 self-check armed, under the given
/// key mode.
fn self_checked_full_refresh(
    designer: &mut StructureDesigner,
    mode: SelfCheckKeyMode,
) -> Arc<EvalProfile> {
    designer.eval_profiling_enabled = true;
    designer.eval_self_check_enabled = true;
    designer.eval_self_check_key_mode = mode;
    full_refresh(designer).expect("a profiled full refresh must produce a table")
}

/// The single record for a node inside a body (non-empty scope path).
fn body_record_for(profile: &EvalProfile, node_id: u64) -> &NodeProfileRecord {
    let matches: Vec<_> = profile
        .records()
        .iter()
        .filter(|r| r.location.node_id == node_id && !r.location.scope_path.is_empty())
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one body record for #{node_id}; recorded: {:?}",
        profile
            .records()
            .iter()
            .map(|r| r.location.label.as_str())
            .collect::<Vec<_>>()
    );
    matches[0]
}

/// The canonical redundancy shape: two consumers pull one apex within one
/// environment. Two lookups, **one** distinct environment — so exactly one of
/// the two evaluations was avoidable, and `wasted_ns` says so.
#[test]
fn a_diamond_reports_two_lookups_in_one_environment() {
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
    let apex_record = record_for(&profile, apex);

    assert_eq!(apex_record.lookups, 2);
    assert_eq!(
        apex_record.distinct_envs, 1,
        "one environment, pulled twice"
    );
    assert_eq!(
        apex_record.evaluations, 2,
        "no memo yet, so every lookup evaluated"
    );
    assert_eq!(apex_record.redundancy_factor(), 2.0);
    assert!(
        !apex_record.flags.uncacheable(),
        "a plain expr node is exactly what the memo would cache"
    );

    for id in [left, right, sink] {
        let record = record_for(&profile, id);
        assert_eq!(record.lookups, 1);
        assert_eq!(record.distinct_envs, 1);
        assert_eq!(record.wasted_ns(), 0, "#{id} has nothing to save");
    }
}

/// **The test that proves the epoch works.** A lazily-driven `map` body runs
/// once per element, and each run is a *different* environment — the zone-input
/// frame and the captures `Arc` are rebuilt per invocation. Without
/// `env_epoch` the three invocations would push a byte-identical body frame,
/// report `distinct_envs = 1`, and inflate this design's own business case by
/// claiming 3x redundancy where there is none.
#[test]
fn a_map_body_over_three_elements_reports_three_environments() {
    let (mut designer, _map_id, body_expr) = build_map_over_three();
    let profile = profiled_full_refresh(&mut designer);

    let record = body_record_for(&profile, body_expr);
    assert_eq!(record.lookups, 3, "one body run per element");
    assert_eq!(
        record.distinct_envs, 3,
        "each invocation is its own environment — this is not redundancy"
    );
    assert_eq!(record.redundancy_factor(), 1.0);
    assert_eq!(
        record.wasted_ns(),
        0,
        "a memo could save nothing here, and the column must not pretend otherwise"
    );
}

/// The eager path to the same conclusion, and the one that would catch a
/// missing `next_env_epoch` carry: a `fold` body runs against a
/// `fresh_inner_for_eager_body` context. If that context restarted its counter
/// at 1 the epochs would still differ *within* the body, so this test alone
/// would pass — which is why the monotonicity test below checks the counter
/// directly rather than only through a fixture.
#[test]
fn a_fold_body_over_three_elements_reports_three_environments() {
    let (mut designer, _fold_id, expr_id) = build_fold_over_three();
    let profile = profiled_full_refresh(&mut designer);

    let record = body_record_for(&profile, expr_id);
    assert_eq!(record.lookups, 3);
    assert_eq!(record.distinct_envs, 3);
}

/// Two displayed nodes **inside one 0-ary closure body** both pull a third body
/// node. Each displayed root descends into the body separately, and that
/// descent allocates **no** epoch — so the two pulls share an environment and
/// the redundancy is real.
///
/// With a fresh epoch per descent this would read `distinct_envs = 2`, hiding
/// exactly the cross-root redundancy the memo exists to collect. That is what
/// makes this the test for `generate_scene_scoped`'s push.
#[test]
fn two_displayed_body_nodes_share_the_body_environment() {
    let mut designer = setup_designer_with_network("main");

    let closure = designer.add_node("closure", DVec2::ZERO);
    designer.set_node_network_data_scoped(
        &[],
        closure,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int],
            param_names: vec![],
            custom_label: None,
        }),
    );
    let body = [closure];

    let source = add_expr_to_body(&mut designer, "main", closure, "7", vec![]);
    let left = add_expr_to_body(
        &mut designer,
        "main",
        closure,
        "a + 1",
        vec![("a", DataType::Int)],
    );
    let right = add_expr_to_body(
        &mut designer,
        "main",
        closure,
        "a + 2",
        vec![("a", DataType::Int)],
    );
    designer.connect_nodes_scoped(&body, source, 0, left, 0);
    designer.connect_nodes_scoped(&body, source, 0, right, 0);
    designer.connect_zone_output_wire(&body, left, 0, 0);
    designer.validate_active_network();
    assert!(
        designer.get_active_node_network().unwrap().valid,
        "the fixture must be valid, else nothing evaluates"
    );

    // Display exactly the two consumers, inside the body.
    designer
        .node_type_registry
        .node_networks
        .get_mut("main")
        .unwrap()
        .displayed_nodes
        .clear();
    for id in [left, right] {
        designer.set_node_display_scoped(&body, id, true);
    }
    designer.set_node_display_scoped(&body, source, false);

    let profile = profiled_full_refresh(&mut designer);
    let record = body_record_for(&profile, source);

    assert_eq!(
        record.lookups, 2,
        "both displayed body roots pull the shared source"
    );
    assert_eq!(
        record.distinct_envs, 1,
        "the scene descent must not allocate an epoch — with one, the two \
         descents would look like different environments and the memo's case \
         would vanish"
    );
}

/// A **capture cone** pulled under one enclosing environment is one
/// environment, not one per pull. Capture pre-evaluation pushes the body so
/// `source_scope_depth` walks resolve in the parent scope, but it runs *before*
/// any captures exist and is not an invocation — so it keeps epoch 0.
///
/// A fresh epoch there would pin capture redundancy at 1.0 forever and make
/// every capture cone permanently uncacheable, which is the opposite of what
/// the measurement is for. Here the `fold` is pulled twice (fan-out), so its
/// captures are pre-evaluated twice against an identical enclosing stack.
#[test]
fn a_capture_cone_pulled_twice_shares_one_environment() {
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

    // The captured node: a top-level constant read from inside the body.
    let captured = add_expr(&mut designer, "main", "10", vec![]);

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

    let body_expr = add_expr_to_body(
        &mut designer,
        "main",
        fold_id,
        "acc + elem + cap",
        vec![
            ("acc", DataType::Int),
            ("elem", DataType::Int),
            ("cap", DataType::Int),
        ],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 0, body_expr, 0);
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 1, body_expr, 1);
    // The capture wire: depth 1 up to the top-level `captured` node.
    {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&fold_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        body.nodes.get_mut(&body_expr).unwrap().arguments[2]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: captured,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 1,
            });
    }
    wire_body_node_to_zone_output(&mut designer, "main", fold_id, body_expr);

    // Fan the fold out into two consumers of one displayed sink, so `fold.eval`
    // — and therefore capture pre-evaluation — runs twice.
    let left = add_expr(&mut designer, "main", "a + 1", vec![("a", DataType::Int)]);
    let right = add_expr(&mut designer, "main", "a + 2", vec![("a", DataType::Int)]);
    let sink = add_expr(
        &mut designer,
        "main",
        "a + b",
        vec![("a", DataType::Int), ("b", DataType::Int)],
    );
    push_wire(&mut designer, "main", fold_id, left, 0);
    push_wire(&mut designer, "main", fold_id, right, 0);
    push_wire(&mut designer, "main", left, sink, 0);
    push_wire(&mut designer, "main", right, sink, 1);
    designer.validate_active_network();
    display_only(&mut designer, "main", sink);

    let profile = profiled_full_refresh(&mut designer);
    let record = profile
        .records()
        .iter()
        .find(|r| r.location.node_id == captured)
        .expect("the captured node must be recorded");

    assert_eq!(
        record.lookups, 2,
        "the fold is pulled twice, so its captures are pre-evaluated twice"
    );
    assert_eq!(
        record.distinct_envs, 1,
        "capture pre-evaluation is not an invocation and must keep epoch 0"
    );
}

/// Epoch allocation is monotonic across the **whole pass**, including epochs
/// handed out inside an eager HOF body. The body runs against a
/// `fresh_inner_for_eager_body` context, and without the explicit carry-and-
/// merge that context would restart at 1 and re-issue numbers the outer context
/// had already spent — producing two genuinely different environments with
/// equal keys. Silent today; a wrong *result* once the memo keys on it.
#[test]
fn epoch_allocation_is_monotonic_across_an_eager_body_drain() {
    let mut outer = NetworkEvaluationContext::new();
    let first = outer.alloc_env_epoch();
    let second = outer.alloc_env_epoch();
    assert!(second > first, "epochs are handed out strictly increasing");

    let mut inner = outer.fresh_inner_for_eager_body();
    assert_eq!(
        inner.peek_next_env_epoch(),
        outer.peek_next_env_epoch(),
        "the body context must continue the pass's numbering, not restart it"
    );
    let inner_epochs: Vec<u64> = (0..3).map(|_| inner.alloc_env_epoch()).collect();
    assert!(inner_epochs.iter().all(|e| *e > second));

    outer.drain_inner_context(inner);
    let after = outer.alloc_env_epoch();
    assert!(
        after > *inner_epochs.last().unwrap(),
        "an epoch spent inside the body was re-issued after the drain: {} <= {}",
        after,
        inner_epochs.last().unwrap()
    );
}

// ============================================================================
// The environment key itself (D9)
// ============================================================================

/// **The address is not an input to the key.** This key is retained — across
/// the pass, and afterwards by the memo — while both kinds of body network get
/// dropped mid-pass (`zone_closure` pushes a locally constructed body; closure
/// bodies are `Arc`s), so a reused allocation must not silently merge two
/// environments. That is precisely the argument `eval_frame_key`'s doc comment
/// makes for its *own* address hashing ("a spurious collision needs two **live**
/// frames") and the one claim this key may not borrow.
#[test]
fn the_body_network_address_is_not_an_input_to_the_env_key() {
    let mut designer = setup_designer_with_network("main");
    designer.add_node_network("other");
    let root = designer.node_type_registry.node_networks["main"].clone();
    let body_a = designer.node_type_registry.node_networks["other"].clone();
    let body_b = designer.node_type_registry.node_networks["other"].clone();
    assert_ne!(
        &body_a as *const _, &body_b as *const _,
        "the fixture needs two distinct allocations"
    );

    let stack_a = vec![
        NetworkStackElement::root(&root),
        NetworkStackElement::body_invocation(&body_a, 12, 7),
    ];
    let stack_b = vec![
        NetworkStackElement::root(&root),
        NetworkStackElement::body_invocation(&body_b, 12, 7),
    ];
    assert_eq!(
        eval_env_key(&stack_a, 5, false),
        eval_env_key(&stack_b, 5, false),
        "same owner, same epoch, different allocation — one environment"
    );

    // Same allocation, different owner id → different environment. `node_id`
    // counters are per-network, which is why the owner alone is not enough and
    // the enclosing frames are hashed too.
    let stack_c = vec![
        NetworkStackElement::root(&root),
        NetworkStackElement::body_invocation(&body_a, 13, 7),
    ];
    assert_ne!(
        eval_env_key(&stack_a, 5, false),
        eval_env_key(&stack_c, 5, false)
    );
}

/// **The environment key is 128 bits wide, and actually uses them.**
///
/// It is the one identity key whose collision serves a *wrong value* rather
/// than raising an error or merging a table row, which is why it is wider than
/// `eval_frame_key` and `node_profile_key`. A future refactor that quietly
/// narrows it back — a single digest cast to `u128`, say — leaves the high half
/// zero and every other test still passing, so this asserts the property
/// directly. Deterministic, not probabilistic: `DefaultHasher` is fixed-seed.
#[test]
fn the_env_key_is_a_full_width_128_bit_digest() {
    let designer = setup_designer_with_network("main");
    let network = designer.node_type_registry.node_networks["main"].clone();
    let stack = vec![NetworkStackElement::root(&network)];

    let key = eval_env_key(&stack, 3, false);
    assert_ne!(
        key >> 64,
        0,
        "the high 64 bits are unused — the key has been narrowed back to one digest"
    );
    assert_ne!(key as u64, 0);
    assert_eq!(
        key,
        eval_env_key(&stack, 3, false),
        "the key must be stable within a pass, or a memo keyed on it would miss          every hit"
    );
}

/// The epoch, `decorate` and the instance id are all *in* the key — the three
/// separations `node_profile_key` deliberately does not make.
#[test]
fn the_env_key_separates_epochs_decorate_and_instances() {
    let mut designer = setup_designer_with_network("main");
    designer.add_node_network("other");
    let root = designer.node_type_registry.node_networks["main"].clone();
    let body = designer.node_type_registry.node_networks["other"].clone();

    let base = vec![
        NetworkStackElement::root(&root),
        NetworkStackElement::body_invocation(&body, 12, 7),
    ];
    let next_epoch = vec![
        NetworkStackElement::root(&root),
        NetworkStackElement::body_invocation(&body, 12, 8),
    ];
    assert_ne!(
        eval_env_key(&base, 5, false),
        eval_env_key(&next_epoch, 5, false),
        "two invocations of one body are two environments — the whole point of \
         the epoch"
    );

    assert_ne!(
        eval_env_key(&base, 5, false),
        eval_env_key(&base, 5, true),
        "`decorate` genuinely changes results, so it is in the key"
    );

    // Two instances of one custom network: one row in the profiler's table
    // (`node_profile_key`), two environments here — their arguments come from
    // different call sites.
    let as_instance_7 = vec![NetworkStackElement::instance(&root, 7)];
    let as_instance_9 = vec![NetworkStackElement::instance(&root, 9)];
    assert_eq!(
        node_profile_key(&as_instance_7, 3),
        node_profile_key(&as_instance_9, 3)
    );
    assert_ne!(
        eval_env_key(&as_instance_7, 3, false),
        eval_env_key(&as_instance_9, 3, false)
    );
}

// ============================================================================
// The Wasted column (D10)
// ============================================================================

fn synthetic_record(
    lookups: u64,
    evaluations: u64,
    distinct_envs: u64,
    self_ns: u64,
) -> NodeProfileRecord {
    NodeProfileRecord {
        location: NodeLocation {
            host_network: "main".to_string(),
            scope_path: Vec::new(),
            node_id: 1,
            label: "main/expr#1".to_string(),
            node_type_name: "expr".to_string(),
            navigable: true,
        },
        lookups,
        evaluations,
        distinct_envs,
        self_ns,
        total_ns: self_ns,
        flags: RecordFlags::default(),
    }
}

/// `wasted_ns = self_ns x (lookups - distinct_envs) / evaluations`, including
/// the **post-memo** case `evaluations < lookups`. Dividing by `evaluations`
/// rather than by `lookups` is what keeps the column meaningful after the memo
/// lands: `self_ns` accumulates over actual evaluations, so `self_ns /
/// evaluations` is the mean cost of computing the node once.
#[test]
fn wasted_ns_arithmetic_survives_the_memo() {
    // Pre-memo: 4 requests, 1 environment, 4 evaluations of 100 ns each.
    // Three of the four were avoidable.
    let pre = synthetic_record(4, 4, 1, 400);
    assert_eq!(pre.wasted_ns(), 300);
    assert_eq!(pre.redundancy_factor(), 4.0);

    // Post-memo, same node: still 4 requests over 1 environment, but only one
    // evaluation ran. The projected saving has been *collected*, so what is
    // still "wasted" is three times the one evaluation's cost — the same 300 ns
    // the memo is now avoiding, which is how the acceptance criterion is read.
    let post = synthetic_record(4, 1, 1, 100);
    assert_eq!(post.wasted_ns(), 300);

    // No redundancy at all: as many environments as requests.
    let honest = synthetic_record(3, 3, 3, 300);
    assert_eq!(honest.wasted_ns(), 0);
    assert_eq!(honest.redundancy_factor(), 1.0);

    // Degenerate rows must not divide by zero.
    assert_eq!(synthetic_record(0, 0, 0, 0).wasted_ns(), 0);
    assert_eq!(synthetic_record(0, 0, 0, 0).redundancy_factor(), 1.0);
}

/// A row the memo would decline to cache is **counted but flagged**, so its
/// `wasted_ns` is never read as an available saving — and is excluded from the
/// pass's projected total (D10).
#[test]
fn an_iterator_producer_is_flagged_as_uncacheable() {
    let (mut designer, map_id, _body_expr) = build_map_over_three();
    let profile = profiled_full_refresh(&mut designer);

    let map_record = record_for(&profile, map_id);
    assert!(
        map_record.flags.produced_iterator,
        "`map` yields a walker, which the memo deliberately does not store"
    );
    assert!(map_record.flags.uncacheable());

    let flagged_waste: u64 = profile
        .records()
        .iter()
        .filter(|r| r.flags.uncacheable())
        .map(|r| r.wasted_ns())
        .sum();
    assert_eq!(
        profile.projected_saving_ns(),
        profile.records().iter().map(|r| r.wasted_ns()).sum::<u64>() - flagged_waste,
        "the projected saving must exclude every flagged row"
    );
}

// ============================================================================
// The equal-key ⇒ equal-result self-check (D11)
// ============================================================================

/// The check runs clean on the fixtures above. Note this only means anything
/// while there is **no memo**: once one serves the second request from the
/// first result there is no second computation to compare and the check passes
/// vacuously.
#[test]
fn the_self_check_runs_clean_on_real_passes() {
    let (mut designer, _map_id, _body_expr) = build_map_over_three();
    let profile = self_checked_full_refresh(&mut designer, SelfCheckKeyMode::Full);
    assert!(profile.self_check_ran());
    assert_eq!(
        profile.self_check_violations(),
        &[],
        "equal keys produced different results on a `map` pass"
    );

    let (mut designer, _fold_id, _expr_id) = build_fold_over_three();
    let profile = self_checked_full_refresh(&mut designer, SelfCheckKeyMode::Full);
    assert_eq!(profile.self_check_violations(), &[]);
}

/// **The check has teeth.** `decorate` is one of the three varying inputs of
/// `NodeData::eval`, and a selected node that also feeds another displayed node
/// is evaluated both ways in one pass. Under the real key those are two
/// environments and the check is silent; under a key with `decorate` dropped
/// they collide, and the check must say so — otherwise a green result on a real
/// design would prove nothing.
#[test]
fn the_self_check_catches_a_key_with_decorate_omitted() {
    fn build() -> (StructureDesigner, u64) {
        let mut designer = setup_designer_with_network("main");
        let upstream = designer.add_node("atom_edit", DVec2::ZERO);
        let downstream = designer.add_node("atom_edit", DVec2::new(200.0, 0.0));
        designer.connect_nodes(upstream, 0, downstream, 0);
        designer.validate_active_network();
        designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .displayed_nodes
            .clear();
        designer.set_node_display(upstream, true);
        designer.set_node_display(downstream, true);
        // Selection is what makes `decorate` true for `upstream`'s own scene
        // evaluation — and only for that one.
        designer.select_node(upstream);
        (designer, upstream)
    }

    let (mut designer, upstream) = build();
    let full = self_checked_full_refresh(&mut designer, SelfCheckKeyMode::Full);
    assert_eq!(
        record_for(&full, upstream).distinct_envs,
        2,
        "the selected node is evaluated decorated as its own root and \
         undecorated as an input — two environments"
    );
    assert_eq!(
        full.self_check_violations(),
        &[],
        "the real key separates them, so nothing is wrong"
    );

    let (mut designer, _upstream) = build();
    let weakened = self_checked_full_refresh(&mut designer, SelfCheckKeyMode::OmitDecorate);
    assert!(
        !weakened.self_check_violations().is_empty(),
        "a key missing `decorate` must be caught; a check that cannot fail \
         proves nothing"
    );
}

// ============================================================================
// Phase 3 fixtures
// ============================================================================

/// `range(1..4) -> map(elem + 1) -> collect`, with the `collect` displayed so
/// the lazy walker is actually drained. Returns `(designer, map_id,
/// body_expr_id)`.
fn build_map_over_three() -> (StructureDesigner, u64, u64) {
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

    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, map_id, 0);

    let body_expr = add_expr_to_body(
        &mut designer,
        "main",
        map_id,
        "elem + 1",
        vec![("elem", DataType::Int)],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", map_id, 0, body_expr, 0);
    wire_body_node_to_zone_output(&mut designer, "main", map_id, body_expr);

    let collect_id = designer.add_node("collect", DVec2::new(400.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        collect_id,
        Box::new(CollectData {
            element_type: DataType::Int,
            limit: None,
            offset: 0,
        }),
    );
    designer.connect_nodes(map_id, 0, collect_id, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", collect_id);

    (designer, map_id, body_expr)
}
