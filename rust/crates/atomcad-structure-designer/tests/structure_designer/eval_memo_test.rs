//! Phase 3 of `doc/design_eval_memoization.md`: the per-pass evaluation memo.
//!
//! The counterpart of `eval_profiler_test.rs`, which pins the memo **off** so
//! its redundancy numbers keep measuring the un-memoized evaluator. Here the
//! memo is the subject, and almost every assertion is of the form "`lookups`
//! stayed the same and `evaluations` fell" — the two columns the profiler kept
//! separate precisely so this change would be visible rather than silent.
//!
//! Everything is driven through `StructureDesigner::refresh`, the way the
//! application drives it. Reaching into `eval_memo::install` directly would
//! test the module and not the wiring — and the wiring (`with_eval_context`
//! owning the lifetime, the table living in a thread-local rather than on the
//! context) is where the two failure modes this phase is exposed to live.
//!
//! No wall-clock assertions: the win is asserted as *evaluation counts*, which
//! are machine-independent, and the millisecond claim in the design's
//! acceptance criterion is the maintainer's manual walkthrough.

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::evaluator::eval_memo::MemoCounts;
use atomcad_structure_designer::evaluator::eval_profiler::{EvalProfile, NodeProfileRecord};
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_network::{Argument, IncomingWire, NodeRef, SourcePin};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
use atomcad_structure_designer::nodes::closure::{ClosureData, ClosureKind};
use atomcad_structure_designer::nodes::collect::CollectData;
use atomcad_structure_designer::nodes::expr::{ExprData, ExprParameter};
use atomcad_structure_designer::nodes::fold::FoldData;
use atomcad_structure_designer::nodes::int::IntData;
use atomcad_structure_designer::nodes::lattice_vecs::LatticeVecsData;
use atomcad_structure_designer::nodes::map::MapData;
use atomcad_structure_designer::nodes::print::PrintData;
use atomcad_structure_designer::nodes::range::RangeData;
use atomcad_structure_designer::nodes::string::StringData;
use atomcad_structure_designer::preferences::MemoryPreferences;
use atomcad_structure_designer::refresh_profile::{RefreshProfile, RefreshSubPhases};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::structure_designer_changes::{
    RefreshMode, StructureDesignerChanges,
};
use glam::f64::DVec2;
use std::collections::HashSet;
use std::sync::Arc;

// ============================================================================
// Helpers
// ============================================================================

/// A designer with the memo in its **product default** state (on).
fn setup(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    // `StructureDesigner::new()` loads the real user preferences file, so pin
    // the budget rather than inheriting whatever this machine happens to have
    // set — a small persisted budget would turn "did the memo hit?" into a
    // measurement of the maintainer's preferences.
    let mut prefs = designer.preferences.clone();
    prefs.memory_preferences.eval_memo_cache_mb = 1024;
    designer.set_preferences(prefs);
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

/// Wire one body node to another body node (both inside `hof_node_id`'s body).
fn push_body_wire(
    designer: &mut StructureDesigner,
    network_name: &str,
    hof_node_id: u64,
    source: u64,
    dest: u64,
    param_index: usize,
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
    body.nodes.get_mut(&dest).unwrap().arguments[param_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: source,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
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
    designer.validate_active_network();
}

/// Push a raw wire, bypassing `can_connect_nodes` (hand-authored-file style).
fn push_wire_from_pin(
    designer: &mut StructureDesigner,
    network_name: &str,
    source: u64,
    source_pin: i32,
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
            source_pin: SourcePin::NodeOutput {
                pin_index: source_pin,
            },
            source_scope_depth: 0,
        });
}

fn push_wire(
    designer: &mut StructureDesigner,
    network_name: &str,
    source: u64,
    dest: u64,
    param_index: usize,
) {
    push_wire_from_pin(designer, network_name, source, 0, dest, param_index);
}

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

fn display_also(designer: &mut StructureDesigner, node_id: u64) {
    designer.set_node_display(node_id, true);
}

fn refresh(designer: &mut StructureDesigner) -> RefreshSubPhases {
    let changes = StructureDesignerChanges {
        mode: RefreshMode::Full,
        ..Default::default()
    };
    designer.refresh(&changes)
}

/// Refresh with the profiler armed and return the table it produced.
fn profiled_refresh(designer: &mut StructureDesigner) -> Arc<EvalProfile> {
    designer.eval_profiling_enabled = true;
    refresh(designer)
        .node_stats
        .expect("a profiled full refresh must produce a table")
}

/// Refresh with the profiler armed and return both the table and the memo
/// counters, which are always on.
fn profiled_refresh_with_counts(
    designer: &mut StructureDesigner,
) -> (Arc<EvalProfile>, MemoCounts) {
    designer.eval_profiling_enabled = true;
    let sub_phases = refresh(designer);
    (
        sub_phases
            .node_stats
            .expect("a profiled full refresh must produce a table"),
        sub_phases.memo,
    )
}

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

/// The single record for a node evaluated inside an HOF body.
fn body_record_for(profile: &EvalProfile, node_id: u64) -> &NodeProfileRecord {
    let matches: Vec<_> = profile
        .records()
        .iter()
        .filter(|r| r.location.node_id == node_id && !r.location.scope_path.is_empty())
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one body record for #{node_id}; got {:?}",
        matches
            .iter()
            .map(|r| r.location.label.as_str())
            .collect::<Vec<_>>()
    );
    matches[0]
}

/// The hover strings the pass produced, as a sorted, comparable snapshot. This
/// is the A/B comparison's observable: it covers every node's every pin.
fn output_string_snapshot(designer: &StructureDesigner) -> Vec<(String, Vec<String>)> {
    let mut rows: Vec<(String, Vec<String>)> = designer
        .last_generated_structure_designer_scene
        .get_all_node_output_strings()
        .into_iter()
        .map(|(node_ref, strings)| {
            (
                format!("{:?}/{}", node_ref.scope_path, node_ref.node_id),
                strings,
            )
        })
        .collect();
    rows.sort();
    rows
}

/// The two `atom_edit` side channels the UI reads back out of band: the input
/// its `eval` cached, and the subtitle its stored data renders.
fn atom_edit_side_channels(
    designer: &StructureDesigner,
    node_id: u64,
) -> (Option<usize>, Option<String>) {
    let node = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&node_id)
        .unwrap();
    let data = node
        .data
        .as_any_ref()
        .downcast_ref::<AtomEditData>()
        .expect("atom_edit node data");
    (
        data.get_cached_input().map(|s| s.get_num_of_atoms()),
        data.get_subtitle(&HashSet::new()),
    )
}

/// A `lattice_vecs` node carrying a plain orthorhombic cell — a real
/// `LatticeVecs` value whose three unpacked vectors are pairwise different,
/// which is what makes "pin 0 and pin 1 did not collapse" a meaningful
/// assertion.
fn add_lattice_vecs(designer: &mut StructureDesigner, network_name: &str) -> u64 {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let id = network.add_node(
        "lattice_vecs",
        DVec2::ZERO,
        0,
        Box::new(LatticeVecsData {
            cell_length_a: 1.0,
            cell_length_b: 2.0,
            cell_length_c: 3.0,
            cell_angle_alpha: 90.0,
            cell_angle_beta: 90.0,
            cell_angle_gamma: 90.0,
        }),
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
            .get_mut(&id)
            .unwrap(),
        true,
    );
    id
}

/// A diamond: `apex` feeds two consumers that both feed the displayed sink.
fn build_diamond() -> (StructureDesigner, u64, u64) {
    let mut designer = setup("main");
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
    (designer, apex, sink)
}

// ============================================================================
// Sharing, and its limits
// ============================================================================

/// **The headline.** The diamond apex is *requested* twice and *evaluated*
/// once, and the sink — which adds the two consumers' results — still gets the
/// right answer, so both consumers were served the same value.
#[test]
fn a_diamond_evaluates_its_apex_once() {
    let (mut designer, apex, sink) = build_diamond();
    let (profile, memo) = profiled_refresh_with_counts(&mut designer);

    let apex_record = record_for(&profile, apex);
    assert_eq!(apex_record.lookups, 2, "demand is unchanged by the memo");
    assert_eq!(apex_record.distinct_envs, 1);
    assert_eq!(
        apex_record.evaluations, 1,
        "the second request must be served from the memo"
    );
    assert!(memo.enabled);
    assert!(memo.hits >= 1, "the second apex pull is a hit");

    // (7+1) + (7+2) = 17 — the consumers received equal, correct values.
    let strings = designer
        .last_generated_structure_designer_scene
        .get_node_output_strings(&[], sink)
        .expect("the sink must have a hover value");
    assert_eq!(strings[0], "17");
}

/// D1's cross-root sharing: the largest single win in the design's
/// measurement. Two displayed roots share an upstream cone, and the shared node
/// is evaluated **once for the whole pass**, not once per root.
///
/// This is what the per-pass lifetime buys, and what a memo cleared by
/// `generate_scene_scoped`'s per-root scratch reset would throw away.
#[test]
fn a_shared_upstream_node_evaluates_once_across_two_displayed_roots() {
    let mut designer = setup("main");
    let shared = add_expr(&mut designer, "main", "7", vec![]);
    let root_a = add_expr(&mut designer, "main", "a + 1", vec![("a", DataType::Int)]);
    let root_b = add_expr(&mut designer, "main", "a + 2", vec![("a", DataType::Int)]);
    push_wire(&mut designer, "main", shared, root_a, 0);
    push_wire(&mut designer, "main", shared, root_b, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", root_a);
    display_also(&mut designer, root_b);

    let profile = profiled_refresh(&mut designer);

    let shared_record = record_for(&profile, shared);
    assert_eq!(shared_record.lookups, 2, "one pull per displayed root");
    assert_eq!(shared_record.distinct_envs, 1);
    assert_eq!(
        shared_record.evaluations, 1,
        "the memo must survive the per-root scratch reset (D1)"
    );
}

/// D2's free win: `evaluate` already computed every pin and threw all but one
/// away, so a two-output node consumed on two pins ran `eval` twice. Keying the
/// *whole* `EvalOutput` under one environment removes that without any
/// pin-index bookkeeping.
#[test]
fn a_two_output_node_consumed_on_both_pins_evaluates_once() {
    let mut designer = setup("main");
    let cell = add_lattice_vecs(&mut designer, "main");
    let unpack = designer.add_node("lattice_vecs_unpack", DVec2::new(200.0, 0.0));
    push_wire(&mut designer, "main", cell, unpack, 0);
    // One consumer per pin, each its own displayed root, so the two pulls are
    // unambiguously two separate requests for the same node.
    let sink_a = add_expr(&mut designer, "main", "a", vec![("a", DataType::Vec3)]);
    let sink_b = add_expr(&mut designer, "main", "b", vec![("b", DataType::Vec3)]);
    push_wire_from_pin(&mut designer, "main", unpack, 0, sink_a, 0);
    push_wire_from_pin(&mut designer, "main", unpack, 1, sink_b, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", sink_a);
    display_also(&mut designer, sink_b);

    let profile = profiled_refresh(&mut designer);

    let record = record_for(&profile, unpack);
    assert_eq!(record.lookups, 2, "one pull per consumed pin");
    assert_eq!(
        record.evaluations, 1,
        "the key omits the pin index, so one entry serves both pins"
    );

    // ...and the two pins still carry *different* values, which is the half a
    // truncated entry would break.
    let scene = &designer.last_generated_structure_designer_scene;
    let a = scene.get_node_output_strings(&[], sink_a).expect("pin a")[0].clone();
    let b = scene.get_node_output_strings(&[], sink_b).expect("pin b")[0].clone();
    assert_ne!(a, b, "pin 0 and pin 1 must not collapse onto one value");
}

/// **D2's insert rule, and the trap it exists for.** `evaluate`'s
/// custom-network arm holds only *one* pin's result; storing that under a key
/// that claims to be a complete output would serve a truncated output to the
/// next request for a different pin.
///
/// So: both pins must come back correct, and the instance row must carry the
/// `subnetwork` flag that keeps it out of the acceptance criterion's offender
/// count.
#[test]
fn a_two_output_custom_network_instance_returns_both_pins() {
    let mut designer = setup("main");
    designer.add_node_network("helper");

    designer.set_active_node_network_name(Some("helper".to_string()));
    let helper_cell = add_lattice_vecs(&mut designer, "helper");
    let inner = designer.add_node("lattice_vecs_unpack", DVec2::new(200.0, 0.0));
    push_wire(&mut designer, "helper", helper_cell, inner, 0);
    designer.set_return_node_id(Some(inner));
    designer.validate_active_network();

    designer.set_active_node_network_name(Some("main".to_string()));
    let instance = designer.add_node("helper", DVec2::ZERO);
    let sink_a = add_expr(&mut designer, "main", "a", vec![("a", DataType::Vec3)]);
    let sink_b = add_expr(&mut designer, "main", "b", vec![("b", DataType::Vec3)]);
    push_wire_from_pin(&mut designer, "main", instance, 0, sink_a, 0);
    push_wire_from_pin(&mut designer, "main", instance, 1, sink_b, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", sink_a);
    display_also(&mut designer, sink_b);

    let profile = profiled_refresh(&mut designer);

    // The two pins of a default `lattice_vecs_unpack` are the a and b vectors
    // of a diamond cell — distinct values, so a truncated entry would show up
    // as one of these being the other (or `None`).
    let a = designer
        .last_generated_structure_designer_scene
        .get_node_output_strings(&[], sink_a)
        .expect("sink_a hover value")[0]
        .clone();
    let b = designer
        .last_generated_structure_designer_scene
        .get_node_output_strings(&[], sink_b)
        .expect("sink_b hover value")[0]
        .clone();
    assert_ne!(a, b, "pin 0 and pin 1 must not collapse onto one value");
    assert!(!a.contains("error"), "pin 0 came back as: {a}");
    assert!(!b.contains("error"), "pin 1 came back as: {b}");

    let instance_record = record_for(&profile, instance);
    assert!(
        instance_record.flags.subnetwork,
        "an instance pulled through `evaluate`'s single-pin arm must be flagged, \
         or every subnetwork in every design reads as a memo bug (D10)"
    );
    // This row *does* re-evaluate within one environment — two pins, two
    // single-pin pulls, neither insertable — which is exactly why the flag has
    // to keep it out of the acceptance criterion's population.
    assert!(instance_record.evaluations > instance_record.distinct_envs);
    assert_eq!(
        profile.unmemoized_offender_count(),
        0,
        "a flagged row must be excluded from the offender count; offenders: {:?}",
        profile
            .records()
            .iter()
            .filter(|r| !r.flags.uncacheable() && r.evaluations > r.distinct_envs)
            .map(|r| r.location.label.as_str())
            .collect::<Vec<_>>()
    );
}

/// Two *instances* of one custom network are two environments — their
/// arguments come from different call sites — so the node inside must be
/// evaluated twice. A memo that shared them would be returning one call site's
/// answer for another's.
#[test]
fn two_instances_of_one_custom_network_do_not_share() {
    let mut designer = setup("main");
    designer.add_node_network("helper");

    designer.set_active_node_network_name(Some("helper".to_string()));
    let param = designer.add_node("parameter", DVec2::ZERO);
    let inner = add_expr(
        &mut designer,
        "helper",
        "x * 10",
        vec![("x", DataType::Int)],
    );
    push_wire(&mut designer, "helper", param, inner, 0);
    designer.set_return_node_id(Some(inner));
    designer.validate_active_network();

    designer.set_active_node_network_name(Some("main".to_string()));
    let one = add_expr(&mut designer, "main", "1", vec![]);
    let two = add_expr(&mut designer, "main", "2", vec![]);
    let a = designer.add_node("helper", DVec2::ZERO);
    let b = designer.add_node("helper", DVec2::new(0.0, 100.0));
    push_wire(&mut designer, "main", one, a, 0);
    push_wire(&mut designer, "main", two, b, 0);
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

    let profile = profiled_refresh(&mut designer);

    // 1*10 + 2*10 = 30. A shared entry would give 20 or 40.
    let strings = designer
        .last_generated_structure_designer_scene
        .get_node_output_strings(&[], sink)
        .expect("sink hover value");
    assert_eq!(
        strings[0], "30",
        "the two instances must not share a result"
    );

    let inner_records: Vec<_> = profile
        .records()
        .iter()
        .filter(|r| r.location.node_id == inner && r.location.host_network == "helper")
        .collect();
    assert_eq!(inner_records.len(), 1);
    assert_eq!(
        inner_records[0].distinct_envs, 2,
        "two call sites are two environments"
    );
    assert_eq!(inner_records[0].evaluations, 2);
}

/// The `NodeRef`-is-not-a-frame-identity hazard, which for the memo is a
/// **wrong value** rather than the spurious error it was for the re-entrancy
/// guard: a custom network's `parameter` resolves its argument by a stack
/// excursion that pops the network frame while the instance's eval scope stays
/// pushed, and per-network `next_node_id` counters let a parent node and a
/// child node share an id.
///
/// The fixture makes that collision concrete — both networks' first node is
/// `#1` — and asserts the arithmetic still comes out right.
#[test]
fn a_parameter_excursion_does_not_cross_contaminate_same_id_nodes() {
    let mut designer = setup("main");
    designer.add_node_network("helper");

    designer.set_active_node_network_name(Some("helper".to_string()));
    let child_param = designer.add_node("parameter", DVec2::ZERO);
    let child_expr = add_expr(
        &mut designer,
        "helper",
        "x + 100",
        vec![("x", DataType::Int)],
    );
    push_wire(&mut designer, "helper", child_param, child_expr, 0);
    designer.set_return_node_id(Some(child_expr));
    designer.validate_active_network();

    designer.set_active_node_network_name(Some("main".to_string()));
    let parent_source = add_expr(&mut designer, "main", "5", vec![]);
    let instance = designer.add_node("helper", DVec2::new(200.0, 0.0));
    push_wire(&mut designer, "main", parent_source, instance, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", instance);

    let _ = profiled_refresh(&mut designer);

    let strings = designer
        .last_generated_structure_designer_scene
        .get_node_output_strings(&[], instance)
        .expect("instance hover value");
    assert_eq!(
        strings[0], "105",
        "the parent's `{parent_source}` and the child's same-numbered node must \
         not share a memo entry"
    );
}

/// **The test that proves the table is in the pass thread-local.** A diamond
/// *inside* a `fold` body shares its apex within each iteration.
///
/// With the memo parked on `NetworkEvaluationContext` the eager HOF's
/// `fresh_inner_for_eager_body` context would hand the body a fresh empty table
/// that `drain_inner_context` then discards — the body would memoize nothing,
/// and the symptom would look like a tuning problem rather than a wiring bug
/// (D7).
#[test]
fn a_diamond_inside_a_fold_body_is_memoized_per_iteration() {
    let mut designer = setup("main");

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

    // body: apex = elem + 1; left = apex + 1; right = apex + 2;
    //       sink = acc + left + right
    let apex = add_expr_to_body(
        &mut designer,
        "main",
        fold_id,
        "elem + 1",
        vec![("elem", DataType::Int)],
    );
    let left = add_expr_to_body(
        &mut designer,
        "main",
        fold_id,
        "a + 1",
        vec![("a", DataType::Int)],
    );
    let right = add_expr_to_body(
        &mut designer,
        "main",
        fold_id,
        "a + 2",
        vec![("a", DataType::Int)],
    );
    let sink = add_expr_to_body(
        &mut designer,
        "main",
        fold_id,
        "acc + l + r",
        vec![
            ("acc", DataType::Int),
            ("l", DataType::Int),
            ("r", DataType::Int),
        ],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 1, apex, 0);
    push_body_wire(&mut designer, "main", fold_id, apex, left, 0);
    push_body_wire(&mut designer, "main", fold_id, apex, right, 0);
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 0, sink, 0);
    push_body_wire(&mut designer, "main", fold_id, left, sink, 1);
    push_body_wire(&mut designer, "main", fold_id, right, sink, 2);
    wire_body_node_to_zone_output(&mut designer, "main", fold_id, sink);
    display_only(&mut designer, "main", fold_id);

    let profile = profiled_refresh(&mut designer);

    let apex_record = body_record_for(&profile, apex);
    assert_eq!(
        apex_record.lookups, 6,
        "two consumers over three iterations"
    );
    assert_eq!(
        apex_record.distinct_envs, 3,
        "one environment per iteration — the epoch in the key"
    );
    assert_eq!(
        apex_record.evaluations, 3,
        "memoized within each iteration, never across them; a context-owned \
         table would report 6 here"
    );
}

/// `apply` takes the same `fresh_inner_for_eager_body` path as `fold`, so it
/// gets the same test: a diamond inside the invoked closure's body shares its
/// apex within the invocation. Repeating it is cheap and the alternative is
/// trusting that two call sites of one helper stayed the same.
#[test]
fn a_diamond_inside_an_applied_closure_body_is_memoized() {
    let mut designer = setup("main");

    let closure_id = designer.add_node("closure", DVec2::ZERO);
    set_node_data(
        &mut designer,
        "main",
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int, DataType::Int],
            param_names: vec!["x".to_string()],
            custom_label: None,
        }),
    );

    // body: apex = x + 1; left = apex + 1; right = apex + 2; out = left + right
    let apex = add_expr_to_body(
        &mut designer,
        "main",
        closure_id,
        "x + 1",
        vec![("x", DataType::Int)],
    );
    let left = add_expr_to_body(
        &mut designer,
        "main",
        closure_id,
        "a + 1",
        vec![("a", DataType::Int)],
    );
    let right = add_expr_to_body(
        &mut designer,
        "main",
        closure_id,
        "a + 2",
        vec![("a", DataType::Int)],
    );
    let out = add_expr_to_body(
        &mut designer,
        "main",
        closure_id,
        "l + r",
        vec![("l", DataType::Int), ("r", DataType::Int)],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", closure_id, 0, apex, 0);
    push_body_wire(&mut designer, "main", closure_id, apex, left, 0);
    push_body_wire(&mut designer, "main", closure_id, apex, right, 0);
    push_body_wire(&mut designer, "main", closure_id, left, out, 0);
    push_body_wire(&mut designer, "main", closure_id, right, out, 1);
    wire_body_node_to_zone_output(&mut designer, "main", closure_id, out);

    let arg = add_expr(&mut designer, "main", "5", vec![]);
    let apply_id = designer.add_node("apply", DVec2::new(400.0, 0.0));
    designer.connect_nodes(closure_id, 0, apply_id, 0);
    designer.connect_nodes(arg, 0, apply_id, 1);
    designer.validate_active_network();
    display_only(&mut designer, "main", apply_id);

    let profile = profiled_refresh(&mut designer);

    let apex_record = body_record_for(&profile, apex);
    assert_eq!(apex_record.lookups, 2, "two consumers inside the body");
    assert_eq!(apex_record.distinct_envs, 1, "one invocation");
    assert_eq!(
        apex_record.evaluations, 1,
        "an `apply` body must see the same memo as the rest of the pass — a \
         context-owned table would be discarded by `drain_inner_context`"
    );

    // (5+1+1) + (5+1+2) = 15 — the body still computes the right answer.
    let strings = designer
        .last_generated_structure_designer_scene
        .get_node_output_strings(&[], apply_id)
        .expect("apply hover value");
    assert_eq!(strings[0], "15");
}

/// **The `-1` function pin is not a projection of the node's `eval` output**,
/// and the environment key does not carry the pin index — so if that arm
/// consulted or populated the memo, a node used both as a value and as a
/// function value would serve one for the other.
///
/// Here one `expr` is consumed both ways: as a wire value by a displayed sink,
/// and as a function value by a `map`'s `f` pin.
#[test]
fn a_function_pin_and_a_value_pin_of_one_node_do_not_share_a_memo_entry() {
    let mut designer = setup("main");

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

    // The dual-role node: `double` has one Int input and an Int output.
    let double = add_expr(&mut designer, "main", "n * 2", vec![("n", DataType::Int)]);

    let map_id = designer.add_node("map", DVec2::new(300.0, 0.0));
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
    // Wire `double`'s **function pin** (-1) into the map's `f` pin.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        let f_pin_index = network
            .nodes
            .get(&map_id)
            .unwrap()
            .arguments
            .len()
            .saturating_sub(1);
        network.nodes.get_mut(&map_id).unwrap().arguments[f_pin_index]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: double,
                source_pin: SourcePin::NodeOutput { pin_index: -1 },
                source_scope_depth: 0,
            });
    }

    let collect_id = designer.add_node("collect", DVec2::new(500.0, 0.0));
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
    // ...and display `double` itself, so the same node is *also* requested as
    // an ordinary value in the same pass.
    display_also(&mut designer, double);

    refresh(&mut designer);

    let scene = &designer.last_generated_structure_designer_scene;
    let collected = scene
        .get_node_output_strings(&[], collect_id)
        .expect("collect hover value")[0]
        .clone();
    assert!(
        collected.contains('2') && collected.contains('4') && collected.contains('6'),
        "the function-pin closure must still double each element; got {collected}"
    );
}

// ============================================================================
// Bodies and iterators
// ============================================================================

/// The epoch doing its job in the other direction: three elements are three
/// environments, so the body still runs three times. A memo that shared across
/// iterations would return element 1's result for element 2 — the silent wrong
/// value this design's key exists to prevent.
#[test]
fn a_map_over_three_elements_still_evaluates_its_body_three_times() {
    let (mut designer, _map_id, body_expr, collect_id) = build_map_over(3);
    let profile = profiled_refresh(&mut designer);

    let record = body_record_for(&profile, body_expr);
    assert_eq!(record.lookups, 3);
    assert_eq!(record.distinct_envs, 3);
    assert_eq!(record.evaluations, 3, "iterations must not share");

    let strings = designer
        .last_generated_structure_designer_scene
        .get_node_output_strings(&[], collect_id)
        .expect("collect hover value");
    assert!(
        strings[0].contains('2') && strings[0].contains('3') && strings[0].contains('4'),
        "map over 1..3 with `elem + 1` should be [2, 3, 4]; got {}",
        strings[0]
    );
}

/// **Epoch-scoped eviction (D3), as a relation rather than a threshold.** A
/// body's entries die with their iteration, so the peak entry count must not
/// grow with the element count — otherwise a 10^5-element `map` accumulates
/// 10^5 generations before the LRU notices.
#[test]
fn a_large_map_does_not_grow_the_memos_peak_entry_count() {
    let (mut designer, _, _, _) = build_map_over(10);
    let small = refresh(&mut designer).memo;

    let (mut designer, _, _, _) = build_map_over(1000);
    let large = refresh(&mut designer).memo;

    assert!(small.peak_entries > 0, "the small map memoized something");
    assert!(
        large.peak_entries <= small.peak_entries * 3,
        "peak entries grew from {} (10 elements) to {} (1000 elements) — the \
         per-epoch retire is not firing",
        small.peak_entries,
        large.peak_entries
    );
    assert!(
        large.epoch_drops >= 900,
        "a 1000-element map should retire roughly one generation per element; \
         got {} epoch drops",
        large.epoch_drops
    );
    assert_eq!(
        large.lru_evictions, 0,
        "a tiny design must not be hitting the 1 GB budget — an eviction here \
         means the two removal kinds are being conflated"
    );
}

/// D4/D6 R4: an `Array` whose **elements** are iterators is not stored.
///
/// This is the case that distinguishes the memo's recursive value-level
/// predicate from the profiler's `RecordFlags::produced_iterator`, which is
/// top-level only: `collect` here produces an `Array`, not an `Iterator`, so
/// the flat check would wave it through and the memo would pin three
/// walkers — and their source arrays — for the whole pass.
#[test]
fn an_array_of_iterators_is_not_stored() {
    let mut designer = setup("main");

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

    // A `map` whose body *returns a stream*: the output element type is
    // `Iter[Int]`, so the map yields `Iter[Iter[Int]]`.
    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Iterator(Box::new(DataType::Int)),
        }),
    );
    designer.connect_nodes(range_id, 0, map_id, 0);

    let body_range = {
        let registry = &mut designer.node_type_registry;
        let body = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        body.add_node(
            "range",
            DVec2::new(50.0, 0.0),
            0,
            Box::new(RangeData {
                start: 0,
                step: 1,
                count: 2,
            }),
        )
    };
    wire_body_node_to_zone_output(&mut designer, "main", map_id, body_range);

    // `collect` turns that into `Array[Iter[Int]]` — an array of walkers.
    let collect_id = designer.add_node("collect", DVec2::new(400.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        collect_id,
        Box::new(CollectData {
            element_type: DataType::Iterator(Box::new(DataType::Int)),
            limit: None,
            offset: 0,
        }),
    );
    designer.connect_nodes(map_id, 0, collect_id, 0);

    // Two consumers, so a stored entry would show up as a hit.
    let left = add_expr(
        &mut designer,
        "main",
        "1",
        vec![(
            "a",
            DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int)))),
        )],
    );
    let right = add_expr(
        &mut designer,
        "main",
        "2",
        vec![(
            "a",
            DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int)))),
        )],
    );
    push_wire(&mut designer, "main", collect_id, left, 0);
    push_wire(&mut designer, "main", collect_id, right, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", left);
    display_also(&mut designer, right);

    let (profile, memo) = profiled_refresh_with_counts(&mut designer);

    let record = record_for(&profile, collect_id);
    assert_eq!(
        record.evaluations, record.lookups,
        "an array carrying walkers must be recomputed on every request, never \
         stored — a stored walker pins its `ZoneClosure` for the whole pass (D4)"
    );
    assert!(
        record.lookups >= 2,
        "the fixture must actually request the node twice"
    );
    assert!(
        memo.declined_inserts >= 1,
        "the recursive iterator predicate must have declined at least once"
    );
}

// ============================================================================
// Effects and side channels
// ============================================================================

/// **D5, a deliberate semantic change.** A `print` node with fan-out 2 printed
/// twice per display pass; memoized it prints once. That is the more correct
/// semantics — "one evaluation per pass" — and the expectation is updated
/// rather than worked around.
#[test]
fn print_with_fan_out_fires_once_per_pass() {
    let mut designer = setup("main");

    let text_id = designer.add_node("string", DVec2::ZERO);
    set_node_data(
        &mut designer,
        "main",
        text_id,
        Box::new(StringData {
            value: "hello".to_string(),
        }),
    );
    let print_id = designer.add_node("print", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        print_id,
        Box::new(PrintData {
            execute_only: false,
        }),
    );
    push_wire(&mut designer, "main", text_id, print_id, 0);
    let left = add_expr(&mut designer, "main", "1", vec![("a", DataType::String)]);
    let right = add_expr(&mut designer, "main", "2", vec![("a", DataType::String)]);
    push_wire(&mut designer, "main", print_id, left, 0);
    push_wire(&mut designer, "main", print_id, right, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", left);
    display_also(&mut designer, right);

    designer.clear_print_log();
    refresh(&mut designer);

    assert_eq!(
        designer.take_print_log().len(),
        1,
        "with the memo on, one evaluation means one print"
    );
}

/// D8: origin links are recorded in `evaluate_arg`, **outside** the memo seam,
/// so a second consumer of a cached upstream `Error` must still get its
/// `consumer -> source` link. The design says this *should* be automatic, which
/// is the reason to assert it rather than reason about it.
#[test]
fn origin_links_survive_a_memo_hit() {
    let mut designer = setup("main");

    // A deliberately broken source: an `expr` referencing an unwired parameter
    // errors at evaluation time.
    let broken = add_expr(&mut designer, "main", "1 / 0", vec![]);
    let left = add_expr(&mut designer, "main", "a", vec![("a", DataType::Int)]);
    let right = add_expr(&mut designer, "main", "a", vec![("a", DataType::Int)]);
    push_wire(&mut designer, "main", broken, left, 0);
    push_wire(&mut designer, "main", broken, right, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", left);
    display_also(&mut designer, right);

    refresh(&mut designer);

    let origins = designer
        .last_generated_structure_designer_scene
        .get_all_node_error_origins();
    // Both consumers, not just the first: if the one served from the memo lost
    // its link, the seam ate it.
    for consumer in [left, right] {
        let links = origins
            .get(&NodeRef::top(consumer))
            .unwrap_or_else(|| panic!("consumer #{consumer} recorded no origin links at all"));
        assert!(
            links.iter().any(|l| l.source_ref.node_id == broken),
            "consumer #{consumer} lost its origin link to the cached error"
        );
    }
}

/// **D5's easiest-to-miss hazard.** `selected_node_eval_cache` is written from
/// inside sixteen nodes' `eval`, taken into the root's `NodeSceneData`, and read
/// back by the gadget layer — so a memo hit, which skips `eval`, would skip the
/// write and leave an empty gadget cache, invisible to any result-comparing
/// test.
///
/// It is safe only via an invariant stated nowhere in the selection code that
/// upholds it: `decorate` is `true` **only** for a displayed root's own
/// evaluation, every selection path keeps `active_node_id` inside
/// `selected_node_ids`, and so the active node's own root evaluation is the
/// unique `decorate = true` evaluation of that node in the pass — and can never
/// be served from the memo.
///
/// Here the gadget-bearing `atom_edit` is *also* upstream of a second displayed
/// node, which is the arrangement that would otherwise cache it first with
/// `decorate = false` and starve its own root.
#[test]
fn a_gadget_bearing_node_upstream_of_another_root_keeps_its_eval_cache() {
    let mut designer = setup("main");

    // A bare `atom_edit` — its output pin resolves to `Molecule` with nothing
    // wired in, which is all this test needs and avoids a fixture that has to
    // hand-build a typed source.
    let edit = designer.add_node("atom_edit", DVec2::ZERO);
    let downstream = designer.add_node("atom_union", DVec2::new(400.0, 0.0));
    push_wire(&mut designer, "main", edit, downstream, 0);
    designer.validate_active_network();

    // Both displayed, and the `atom_edit` selected — which also makes it the
    // active node, the arrangement the invariant rests on.
    designer.select_node(edit);
    display_only(&mut designer, "main", downstream);
    display_also(&mut designer, edit);

    designer.eval_memo_enabled = false;
    refresh(&mut designer);
    assert!(
        designer.get_selected_node_eval_cache().is_some(),
        "CONTROL: the fixture must produce an eval cache with the memo off"
    );

    designer.eval_memo_enabled = true;
    refresh(&mut designer);

    assert!(
        designer.get_selected_node_eval_cache().is_some(),
        "the active node's own root evaluation must never be served from the \
         memo, or the gadget layer silently loses its cache (D5)"
    );
}

/// D5's other half: the interior-mutability fields a node writes from `eval`
/// and the UI reads back. With the memo on the **first** write wins where the
/// last one used to, so the two must agree — asserted by running the same
/// design both ways and comparing what the UI would show.
///
/// `cached_input` and `get_subtitle` are the two `atom_edit` reads back. Of the
/// other four fields in the same family, `cached_unit_cell` (atom_edit),
/// `available_parameters` (materialize / motif_sub), `last_report`
/// (patch_latticefill) and `available_tags` (tag) are all pure functions of the
/// evaluation's inputs and stored data — none reads `decorate` — so first-write
/// and last-write agree by construction. (`last_stats`, listed alongside them
/// in the design, turns out never to be written at all.)
#[test]
fn atom_edit_side_channels_agree_with_the_memo_on_and_off() {
    let mut designer = setup("main");

    let edit = designer.add_node("atom_edit", DVec2::ZERO);
    // Fan-out 2, so with the memo on the second consumer is served a hit.
    let left = designer.add_node("atom_union", DVec2::new(400.0, 0.0));
    let right = designer.add_node("atom_union", DVec2::new(400.0, 200.0));
    push_wire(&mut designer, "main", edit, left, 0);
    push_wire(&mut designer, "main", edit, right, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", left);
    display_also(&mut designer, right);

    designer.eval_memo_enabled = false;
    refresh(&mut designer);
    let (without_input, without_subtitle) = atom_edit_side_channels(&designer, edit);

    designer.eval_memo_enabled = true;
    refresh(&mut designer);
    let (with_input, with_subtitle) = atom_edit_side_channels(&designer, edit);

    assert_eq!(
        without_input, with_input,
        "`get_cached_input()` must not depend on how many times `eval` ran"
    );
    assert_eq!(without_subtitle, with_subtitle);
}

// ============================================================================
// Cycles, the switch, and the criterion
// ============================================================================

/// D9: a synthesized cycle error is never inserted, and the enclosing
/// evaluation of the same environment still inserts its own real result — so a
/// non-cyclic consumer downstream of the cycle sees the real value, not the
/// inner backstop error.
#[test]
fn a_cycle_error_is_never_stored() {
    let mut designer = setup("main");

    let a = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)]);
    let b = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)]);
    push_wire(&mut designer, "main", a, b, 0);
    push_wire(&mut designer, "main", b, a, 0);
    // Two independent consumers of `b`, so the second one exercises the memo.
    let left = add_expr(&mut designer, "main", "a", vec![("a", DataType::Int)]);
    let right = add_expr(&mut designer, "main", "a", vec![("a", DataType::Int)]);
    push_wire(&mut designer, "main", b, left, 0);
    push_wire(&mut designer, "main", b, right, 0);
    designer.validate_active_network();
    display_only(&mut designer, "main", left);
    display_also(&mut designer, right);

    let (profile, _memo) = profiled_refresh_with_counts(&mut designer);

    assert_eq!(
        profile.live_frame_count(),
        0,
        "the memo's early return must not leak a profiler frame"
    );
    // Both consumers see the *same* value — whatever the cycle resolves to —
    // rather than one seeing a real result and the other the inner backstop
    // error, which is what storing the inner error would produce.
    let scene = &designer.last_generated_structure_designer_scene;
    let l = scene
        .get_node_output_strings(&[], left)
        .map(|s| s[0].clone());
    let r = scene
        .get_node_output_strings(&[], right)
        .map(|s| s[0].clone());
    assert_eq!(l, r, "the two consumers of a cyclic node must agree");
}

/// **The A/B, run once as a test.** The switch exists so a suspected memo bug
/// can be confirmed in one click; executing the comparison automatically is
/// what stops that guarantee from rotting.
#[test]
fn memo_off_and_memo_on_produce_identical_output_strings() {
    let (mut designer, _, _) = build_diamond();

    designer.eval_memo_enabled = false;
    refresh(&mut designer);
    let without = output_string_snapshot(&designer);

    designer.eval_memo_enabled = true;
    refresh(&mut designer);
    let with = output_string_snapshot(&designer);

    assert_eq!(
        without, with,
        "the memo changed an observable value — that is the bug the switch \
         exists to find"
    );
    assert!(!without.is_empty(), "the fixture produced no output at all");
}

/// The same A/B over the shapes with the most moving parts: an HOF body and a
/// subnetwork, the two the reference guide's cost model now makes promises
/// about.
#[test]
fn memo_off_and_memo_on_agree_on_a_map_body() {
    let (mut designer, _, _, _) = build_map_over(4);

    designer.eval_memo_enabled = false;
    refresh(&mut designer);
    let without = output_string_snapshot(&designer);

    designer.eval_memo_enabled = true;
    refresh(&mut designer);
    let with = output_string_snapshot(&designer);

    assert_eq!(without, with);
}

/// D10's gate, both halves. Without the second, a pass can run memoized with
/// the check silently armed and report a vacuous green — the exact outcome the
/// gate exists to prevent.
#[test]
fn the_self_check_gate_works_in_both_directions() {
    let mut designer = setup("main");

    assert!(
        designer.eval_memo_enabled,
        "the memo defaults **on** — it is the product's behaviour"
    );
    assert!(
        !designer.try_set_eval_self_check_enabled(true),
        "arming the self-check must be refused while the memo is on"
    );
    assert!(!designer.eval_self_check_enabled);

    assert!(!designer.set_eval_memo_enabled(false));
    assert!(
        designer.try_set_eval_self_check_enabled(true),
        "with the memo off the check arms"
    );
    assert!(designer.eval_self_check_enabled);

    assert!(
        designer.set_eval_memo_enabled(true),
        "switching the memo on must report that it disarmed the check"
    );
    assert!(
        !designer.eval_self_check_enabled,
        "the memo is the product's behaviour; a diagnostic must not block it"
    );
}

/// The acceptance criterion, as the single number the panel will show in Phase
/// 4: with the memo on, no unflagged row re-evaluates within one environment.
#[test]
fn a_memoized_pass_reports_no_unflagged_offenders() {
    let (mut designer, _, _) = build_diamond();
    let profile = profiled_refresh(&mut designer);
    assert_eq!(
        profile.unmemoized_offender_count(),
        0,
        "offending rows: {:?}",
        profile
            .records()
            .iter()
            .filter(|r| !r.flags.uncacheable() && r.evaluations > r.distinct_envs)
            .map(|r| r.location.label.as_str())
            .collect::<Vec<_>>()
    );
}

/// The same criterion on the more demanding fixture — an HOF body plus a lazy
/// walker — where the only rows allowed to offend are the flagged ones.
#[test]
fn a_memoized_map_pass_reports_no_unflagged_offenders() {
    let (mut designer, _, _, _) = build_map_over(3);
    let profile = profiled_refresh(&mut designer);
    assert_eq!(
        profile.unmemoized_offender_count(),
        0,
        "offending rows: {:?}",
        profile
            .records()
            .iter()
            .filter(|r| !r.flags.uncacheable() && r.evaluations > r.distinct_envs)
            .map(|r| r.location.label.as_str())
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Counters and the switch's plumbing
// ============================================================================

/// The counters have to be **harvested** before the table is dropped (D10):
/// unlike the CSG cache the memo does not exist when anything could query it.
#[test]
fn memo_counters_survive_into_the_refresh_profile() {
    let (mut designer, _, _) = build_diamond();
    let memo = refresh(&mut designer).memo;

    assert!(memo.enabled);
    assert_eq!(
        memo.budget_bytes,
        MemoryPreferences::clamped_bytes(1024),
        "the budget the pass actually ran with, from the Memory preference"
    );
    assert!(memo.hits >= 1);
    assert!(memo.misses >= 1);
    assert!(memo.peak_entries >= 1);
    assert!(
        memo.peak_bytes >= memo.end_bytes,
        "the high-water mark cannot be below the ending size"
    );
    assert_eq!(memo.lru_evictions, 0, "1 GB is not tight for four exprs");
}

/// With the memo off the row says so, and says nothing else — `enabled: false`
/// is what distinguishes "switched off" from "on, but nothing to share".
#[test]
fn a_memo_off_pass_reports_a_disabled_row() {
    let (mut designer, _, _) = build_diamond();
    designer.eval_memo_enabled = false;
    let memo = refresh(&mut designer).memo;

    assert!(!memo.enabled);
    assert_eq!(memo.hits, 0);
    assert_eq!(memo.misses, 0);
    assert_eq!(memo.peak_entries, 0);
}

/// A budget too small to hold the pass forces recomputation, and that has to be
/// reported as an **LRU eviction** and an `evicted` row flag rather than as an
/// unexplained re-evaluation (D6/D10). Phase 5's trigger fires on exactly this
/// signal.
#[test]
fn a_tiny_budget_evicts_and_says_so() {
    let (mut designer, apex, _) = build_diamond();
    let mut prefs = designer.preferences.clone();
    // Below the floor on purpose: `clamped_bytes` lifts it to 1 MB, which is
    // still far too small once the estimator counts a handful of results — and
    // `insert` admits an over-budget value into an empty cache, so the memo
    // degrades to a pass-through rather than failing.
    prefs.memory_preferences.eval_memo_cache_mb = 0;
    designer.set_preferences(prefs);

    // A payload comfortably larger than the clamped 1 MB floor: an array of
    // 200_000 integers, requested by two consumers.
    let mut designer2 = setup("main");
    let range_id = designer2.add_node("range", DVec2::ZERO);
    set_node_data(
        &mut designer2,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 200_000,
        }),
    );
    let collect_id = designer2.add_node("collect", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer2,
        "main",
        collect_id,
        Box::new(CollectData {
            element_type: DataType::Int,
            limit: None,
            offset: 0,
        }),
    );
    designer2.connect_nodes(range_id, 0, collect_id, 0);
    let left = add_expr(
        &mut designer2,
        "main",
        "1",
        vec![("a", DataType::Array(Box::new(DataType::Int)))],
    );
    let right = add_expr(
        &mut designer2,
        "main",
        "2",
        vec![("a", DataType::Array(Box::new(DataType::Int)))],
    );
    push_wire(&mut designer2, "main", collect_id, left, 0);
    push_wire(&mut designer2, "main", collect_id, right, 0);
    designer2.validate_active_network();
    display_only(&mut designer2, "main", left);
    display_also(&mut designer2, right);

    let mut prefs = designer2.preferences.clone();
    prefs.memory_preferences.eval_memo_cache_mb = 0;
    designer2.set_preferences(prefs);

    let (profile, memo) = profiled_refresh_with_counts(&mut designer2);

    assert!(
        memo.lru_evictions >= 1,
        "a 1 MB budget must evict a multi-megabyte array; counts were {memo:?}"
    );
    assert!(
        memo.evicted_misses >= 1,
        "the recomputation must be attributed to eviction, not to a cold key"
    );
    let record = record_for(&profile, collect_id);
    assert!(
        record.flags.evicted,
        "the row must carry `evicted`, or memory pressure is indistinguishable \
         from a correctness bug"
    );
    assert!(
        record.flags.uncacheable(),
        "an evicted row must be excluded from the acceptance criterion's count"
    );
    // The unused first fixture keeps the "tiny budget on an ordinary design is
    // harmless" half of the claim honest: it still refreshes without error.
    let _ = refresh(&mut designer).memo;
    let strings = designer
        .last_generated_structure_designer_scene
        .get_node_output_strings(&[], apex);
    assert!(
        strings.is_some(),
        "a tiny budget degrades, it does not break"
    );
}

// ============================================================================
// Reading the result (Phase 4)
// ============================================================================

/// **The A/B pair, as the history ring sees it.** Two rows, one taken with the
/// memo off and one with it on, have to be distinguishable — the whole
/// comparison workflow the switch exists for is unreadable if they are not.
///
/// The flag rides on the row rather than being a "current state" reading for
/// exactly this reason: by the time you look at the ring, the switch says
/// whatever it says now, which tells you nothing about the row you are
/// comparing against.
#[test]
fn the_history_ring_distinguishes_a_memo_off_row_from_a_memo_on_row() {
    let (mut designer, _, _) = build_diamond();

    designer.eval_memo_enabled = false;
    let off = refresh(&mut designer);
    designer.record_refresh_profile(RefreshProfile::new(
        RefreshMode::Full,
        off,
        0.0,
        0.0,
        None,
        1.0,
    ));

    designer.eval_memo_enabled = true;
    let on = refresh(&mut designer);
    designer.record_refresh_profile(RefreshProfile::new(
        RefreshMode::Full,
        on,
        0.0,
        0.0,
        None,
        1.0,
    ));

    let rows: Vec<_> = designer.refresh_profiles.rows().collect();
    assert_eq!(rows.len(), 2);
    assert!(
        !rows[0].memo.enabled,
        "the first row was taken with the memo off"
    );
    assert!(rows[1].memo.enabled);
    assert!(
        rows[1].memo.hits > 0,
        "the memo-on row must carry its own counters, not the switch's state"
    );
    assert_eq!(
        rows[0].memo.hits, 0,
        "a memo-off row reports no activity, which is not the same as zero hits \
         with the memo on"
    );
}

/// **A free check on the key.** The profiler predicts the memo's peak entry
/// count from the same `eval_env_key` the memo stores under, so the two must
/// agree closely — and the memo's actual peak can only ever be *lower* (by
/// whatever D3 retired and the LRU evicted), never higher.
///
/// A peak above the prediction would mean the memo and the profiler are keying
/// on different things, which is the failure mode the frame-identity rules
/// exist to prevent — caught here without needing the self-check.
#[test]
fn the_memos_actual_peak_never_exceeds_the_profilers_prediction() {
    for (label, mut designer) in [
        ("diamond", build_diamond().0),
        ("map over 8", build_map_over(8).0),
    ] {
        designer.eval_profiling_enabled = true;
        let sub_phases = refresh(&mut designer);
        let profile = sub_phases
            .node_stats
            .expect("a profiled full refresh must produce a table");
        let memo = sub_phases.memo;

        assert!(
            memo.peak_entries > 0,
            "{label}: the memo held nothing at all"
        );
        assert!(
            memo.peak_entries as u64 <= profile.total_distinct_envs(),
            "{label}: the memo peaked at {} entries but the profiler predicted \
             at most {} — the two are keying on different things",
            memo.peak_entries,
            profile.total_distinct_envs(),
        );
    }
}

/// The Redundancy footnote reads one way with the memo on and another with it
/// off, and the number it leans on differs: with the memo on it is the count of
/// **unexplained repeats** (zero when the memo is working), with it off it is
/// the **projected saving**. Asserted on the data rather than the string, which
/// is the panel's business.
#[test]
fn the_footnotes_two_readings_come_from_two_different_numbers() {
    let (mut designer, _, _) = build_diamond();

    designer.eval_memo_enabled = false;
    let profile = profiled_refresh(&mut designer);
    assert!(
        profile.projected_saving_ns() > 0,
        "with the memo off there is a saving left on the table to report"
    );
    assert!(
        profile.unmemoized_offender_count() > 0,
        "with the memo off, rows legitimately re-evaluate within one \
         environment — which is why the offender count is only meaningful, and \
         only shown, with the memo on"
    );

    designer.eval_memo_enabled = true;
    let profile = profiled_refresh(&mut designer);
    assert_eq!(profile.unmemoized_offender_count(), 0);
}

// ============================================================================
// Fixtures
// ============================================================================

/// `range(1..n) -> map(elem + 1) -> collect`, with `collect` the only displayed
/// root. Returns `(designer, map_id, body_expr_id, collect_id)`.
fn build_map_over(count: i32) -> (StructureDesigner, u64, u64, u64) {
    let mut designer = setup("main");

    let range_id = designer.add_node("range", DVec2::ZERO);
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 1,
            step: 1,
            count,
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

    (designer, map_id, body_expr, collect_id)
}
