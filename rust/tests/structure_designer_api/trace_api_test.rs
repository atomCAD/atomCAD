//! Output-driven editing, Phase 2 (`doc/design_output_driven_editing.md`): the
//! Dart-facing side of the right-click trace.
//!
//! - **Flattening** of the engine's `TraceNode` tree into pre-order
//!   `APITraceRow`s with a `depth`.
//! - **The two-step pick** (`trace_viewport_pick` → `trace_viewport_hit`) and
//!   the left-click / hover functions that share its overlap rule, through the
//!   view builders the FFI wrappers delegate to. The wrappers themselves need
//!   the GPU-backed `CADInstance` and are one call each.
//!
//! Networks are built in code: a `value` holding a one-atom molecule feeding a
//! `tag` (a node v1 never traces, so the rows are the fallback's: `tag`
//! "possibly contributed", the value "source").

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::visualization::AtomicStructureVisualization;
use atomcad_structure_designer::evaluator::network_result::{MoleculeData, NetworkResult};
use atomcad_structure_designer::node_network::NodeRef;
use atomcad_structure_designer::nodes::value::add_value_node;
use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::trace::{
    Contribution, RowContribution, TraceNode, TraceReport, TraceRow,
};
use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::{
    APIContributionKind, APITracePick, APITraceResult, APIViewportPickResult,
};
use rust_lib_flutter_cad::api::structure_designer::view_builders::{
    flatten_trace_report, hovered_atom_info_view, trace_hit_view, trace_pick_view,
    viewport_pick_view,
};

const NET: &str = "test";

// ============================================================================
// Flattening
// ============================================================================

fn row(network: &str, node_id: u64, contribution: RowContribution) -> TraceRow {
    TraceRow {
        frame: Vec::new(),
        node: NodeRef::top(node_id),
        network: network.to_string(),
        contribution,
    }
}

fn leaf(network: &str, node_id: u64, contribution: RowContribution) -> TraceNode {
    TraceNode {
        row: row(network, node_id, contribution),
        children: Vec::new(),
    }
}

/// `main` holds instance 1 of `outer`; `outer` holds instance 7 of `inner`.
fn nested_report(truncated: bool) -> TraceReport {
    let filled = RowContribution::Traced(Contribution::Created("filled"));
    let inner_instance = TraceNode {
        row: row("outer", 7, filled.clone()),
        children: vec![leaf("inner", 3, filled.clone())],
    };
    let outer_instance = TraceNode {
        row: row("main", 1, filled.clone()),
        children: vec![
            inner_instance,
            leaf("outer", 2, RowContribution::Traced(Contribution::Passed)),
        ],
    };
    TraceReport {
        rows: vec![
            outer_instance,
            leaf("main", 9, RowContribution::Unknown),
            leaf(
                "main",
                4,
                RowContribution::Traced(Contribution::Modified("moved")),
            ),
        ],
        truncated,
        stats: Default::default(),
    }
}

fn summary(result: &APITraceResult) -> Vec<String> {
    result
        .rows
        .iter()
        .map(|r| {
            format!(
                "{}{} @{} #{} {}",
                "  ".repeat(r.depth as usize),
                r.node_label,
                r.host_network,
                r.node_id,
                r.label
            )
        })
        .collect()
}

#[test]
fn flattening_keeps_walk_order_and_lists_each_instance_before_its_children() {
    let result = flatten_trace_report(&nested_report(false), |row| {
        format!("n{}", row.node.node_id)
    });
    assert_eq!(
        summary(&result),
        vec![
            "n1 @main #1 filled",
            "  n7 @outer #7 filled",
            "    n3 @inner #3 filled",
            "  n2 @outer #2 passed",
            "n9 @main #9 possibly contributed",
            "n4 @main #4 moved",
        ]
    );
    let kinds: Vec<_> = result.rows.iter().map(|r| &r.contribution).collect();
    assert!(matches!(kinds[0], APIContributionKind::Created));
    assert!(matches!(kinds[3], APIContributionKind::Passed));
    assert!(matches!(kinds[4], APIContributionKind::Unknown));
    assert!(matches!(kinds[5], APIContributionKind::Modified));
    assert!(result.rows.iter().all(|r| r.scope_path.is_empty()));
    assert!(!result.truncated);
}

#[test]
fn flattening_maps_wired_upstream_and_trace_stops_here() {
    let report = TraceReport {
        rows: vec![
            leaf("main", 1, RowContribution::Opaque),
            leaf("main", 2, RowContribution::Unreached),
        ],
        truncated: false,
        stats: Default::default(),
    };
    let result = flatten_trace_report(&report, |row| format!("n{}", row.node.node_id));
    assert_eq!(
        summary(&result),
        vec!["n1 @main #1 trace stops here", "n2 @main #2 wired upstream"]
    );
    // "trace stops here" reads and dims like an unknown row.
    assert!(matches!(
        result.rows[0].contribution,
        APIContributionKind::Unknown
    ));
    assert!(matches!(
        result.rows[1].contribution,
        APIContributionKind::Unreached
    ));
}

#[test]
fn flattening_carries_the_row_cap() {
    let result = flatten_trace_report(&nested_report(true), |_| String::new());
    assert!(result.truncated);
}

// ============================================================================
// Networks
// ============================================================================

fn setup() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.preferences.node_display_preferences.display_policy = NodeDisplayPolicy::Manual;
    designer
        .preferences
        .atomic_structure_visualization_preferences
        .visualization = AtomicStructureVisualization::BallAndStick;
    designer.add_node_network(NET);
    designer.set_active_node_network_name(Some(NET.to_string()));
    designer
}

fn name(designer: &mut StructureDesigner, node_id: u64, label: &str) {
    designer
        .node_type_registry
        .node_networks
        .get_mut(NET)
        .unwrap()
        .nodes
        .get_mut(&node_id)
        .unwrap()
        .custom_name = Some(label.to_string());
}

/// `src_{suffix}` (a one-carbon molecule at `at`) → `tag_{suffix}` (displayed).
/// Returns (value id, tag id).
fn add_chain(designer: &mut StructureDesigner, suffix: &str, at: DVec3) -> (u64, u64) {
    let mut atoms = AtomicStructure::new();
    atoms.add_atom(6, at);
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(NET)
        .unwrap();
    let value = add_value_node(
        network,
        DVec2::ZERO,
        NetworkResult::Molecule(MoleculeData {
            atoms,
            geo_tree_root: None,
        }),
    );
    name(designer, value, &format!("src_{suffix}"));
    let tag = designer.add_node("tag", DVec2::new(200.0, 0.0));
    name(designer, tag, &format!("tag_{suffix}"));
    designer.connect_nodes(value, 0, tag, 0);
    designer.set_node_display(value, false);
    designer.set_node_display(tag, true);
    (value, tag)
}

fn refresh(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

const RAY_ORIGIN: DVec3 = DVec3::new(0.0, 0.0, -50.0);
const RAY_DIRECTION: DVec3 = DVec3::new(0.0, 0.0, 1.0);

fn rows(result: &APITraceResult) -> Vec<String> {
    result
        .rows
        .iter()
        .map(|r| format!("{} {}", r.node_label, r.label))
        .collect()
}

// ============================================================================
// trace_viewport_pick / trace_viewport_hit
// ============================================================================

#[test]
fn a_ray_through_a_known_atom_is_traced_straight_away() {
    let mut designer = setup();
    let (value, tag) = add_chain(&mut designer, "a", DVec3::ZERO);
    refresh(&mut designer);

    let APITracePick::Traced(result) = trace_pick_view(&mut designer, &RAY_ORIGIN, &RAY_DIRECTION)
    else {
        panic!("one displayed node under the ray must be traced straight away");
    };
    assert_eq!(
        rows(&result),
        vec!["tag_a tagged", "src_a source"]
    );
    assert_eq!(result.rows[0].node_id, tag);
    assert_eq!(result.rows[1].node_id, value);
    assert!(
        result
            .rows
            .iter()
            .all(|r| r.host_network == NET && r.depth == 0)
    );
}

#[test]
fn a_ray_that_misses_everything_is_no_hit() {
    let mut designer = setup();
    add_chain(&mut designer, "a", DVec3::ZERO);
    refresh(&mut designer);

    let pick = trace_pick_view(
        &mut designer,
        &DVec3::new(100.0, 100.0, -50.0),
        &RAY_DIRECTION,
    );
    assert!(matches!(pick, APITracePick::NoHit));
}

#[test]
fn overlapping_outputs_ask_even_when_one_of_them_is_the_active_node() {
    let mut designer = setup();
    let (_, tag_a) = add_chain(&mut designer, "a", DVec3::ZERO);
    let (_, tag_b) = add_chain(&mut designer, "b", DVec3::new(0.0, 0.0, 0.05));
    designer.select_node(tag_a);
    refresh(&mut designer);

    // Left-click takes the active node among the overlap as already chosen…
    assert!(matches!(
        viewport_pick_view(&designer, &RAY_ORIGIN, &RAY_DIRECTION),
        APIViewportPickResult::ActiveNodeHit
    ));
    // …the trace never does.
    let APITracePick::Disambiguation { candidates } =
        trace_pick_view(&mut designer, &RAY_ORIGIN, &RAY_DIRECTION)
    else {
        panic!("overlapping outputs must be offered as candidates");
    };
    let ids: Vec<u64> = candidates.iter().map(|c| c.node_id).collect();
    assert_eq!(ids, vec![tag_a, tag_b]);
    assert_eq!(candidates[0].node_name, "tag_a");
}

#[test]
fn trace_viewport_hit_traces_the_chosen_candidate() {
    let mut designer = setup();
    let (_, tag_a) = add_chain(&mut designer, "a", DVec3::ZERO);
    let (_, tag_b) = add_chain(&mut designer, "b", DVec3::new(0.0, 0.0, 0.05));
    refresh(&mut designer);

    let a = trace_hit_view(&mut designer, &RAY_ORIGIN, &RAY_DIRECTION, tag_a);
    assert_eq!(rows(&a), vec!["tag_a tagged", "src_a source"]);
    let b = trace_hit_view(&mut designer, &RAY_ORIGIN, &RAY_DIRECTION, tag_b);
    assert_eq!(rows(&b), vec!["tag_b tagged", "src_b source"]);
}

#[test]
fn trace_viewport_hit_on_a_node_the_ray_misses_is_empty() {
    let mut designer = setup();
    let (_, tag_a) = add_chain(&mut designer, "a", DVec3::ZERO);
    let (_, tag_b) = add_chain(&mut designer, "b", DVec3::new(30.0, 0.0, 0.0));
    refresh(&mut designer);

    assert!(
        !trace_hit_view(&mut designer, &RAY_ORIGIN, &RAY_DIRECTION, tag_a)
            .rows
            .is_empty()
    );
    assert!(
        trace_hit_view(&mut designer, &RAY_ORIGIN, &RAY_DIRECTION, tag_b)
            .rows
            .is_empty()
    );
}

#[test]
fn gadgets_are_ignored_by_the_trace() {
    // A selected `free_move` shows a translation gizmo at the origin; the atom
    // sits behind it along the ray.
    let mut designer = setup();
    let (_, tag) = add_chain(&mut designer, "a", DVec3::new(0.0, 0.0, 10.0));
    let mover = designer.add_node("free_move", DVec2::new(400.0, 0.0));
    name(&mut designer, mover, "mover");
    designer.connect_nodes(tag, 0, mover, 0);
    designer.set_node_display(tag, false);
    designer.set_node_display(mover, true);
    designer.select_node(mover);
    refresh(&mut designer);

    assert!(
        designer
            .gadget_hit_test(RAY_ORIGIN, RAY_DIRECTION)
            .is_some(),
        "precondition: the ray must hit a gizmo handle in front of the atom"
    );
    // Left-click hands a gadget hit to the gadget…
    assert!(matches!(
        viewport_pick_view(&designer, &RAY_ORIGIN, &RAY_DIRECTION),
        APIViewportPickResult::ActiveNodeHit
    ));
    // …the trace looks through it at the atom.
    let APITracePick::Traced(result) = trace_pick_view(&mut designer, &RAY_ORIGIN, &RAY_DIRECTION)
    else {
        panic!("the atom behind the gizmo must be traced");
    };
    assert_eq!(
        rows(&result),
        vec!["mover moved", "tag_a tagged", "src_a source"]
    );
}

// ============================================================================
// viewport_pick and query_hovered_atom_info, through the API
// ============================================================================

#[test]
fn viewport_pick_no_hit() {
    let mut designer = setup();
    add_chain(&mut designer, "a", DVec3::ZERO);
    refresh(&mut designer);
    let miss = DVec3::new(100.0, 100.0, -50.0);
    assert!(matches!(
        viewport_pick_view(&designer, &miss, &RAY_DIRECTION),
        APIViewportPickResult::NoHit
    ));
}

#[test]
fn viewport_pick_one_hit_activates_it_unless_it_is_active() {
    let mut designer = setup();
    let (_, tag) = add_chain(&mut designer, "a", DVec3::ZERO);
    refresh(&mut designer);
    match viewport_pick_view(&designer, &RAY_ORIGIN, &RAY_DIRECTION) {
        APIViewportPickResult::ActivateNode { node_id, node_name } => {
            assert_eq!(node_id, tag);
            assert_eq!(node_name, "tag_a");
        }
        _ => panic!("one non-active hit must activate"),
    }
    designer.select_node(tag);
    assert!(matches!(
        viewport_pick_view(&designer, &RAY_ORIGIN, &RAY_DIRECTION),
        APIViewportPickResult::ActiveNodeHit
    ));
}

#[test]
fn viewport_pick_overlap_and_distance() {
    let mut designer = setup();
    let (_, tag_a) = add_chain(&mut designer, "a", DVec3::ZERO);
    let (_, tag_b) = add_chain(&mut designer, "b", DVec3::new(0.0, 0.0, 0.05));
    let (_, _far) = add_chain(&mut designer, "far", DVec3::new(0.0, 0.0, 10.0));
    refresh(&mut designer);
    let APIViewportPickResult::Disambiguation { candidates } =
        viewport_pick_view(&designer, &RAY_ORIGIN, &RAY_DIRECTION)
    else {
        panic!("two hits within the overlap epsilon must disambiguate");
    };
    let ids: Vec<u64> = candidates.iter().map(|c| c.node_id).collect();
    assert_eq!(ids, vec![tag_a, tag_b], "the far hit is not a candidate");
}

#[test]
fn hovered_atom_info_names_the_node_and_its_overlaps() {
    let mut designer = setup();
    add_chain(&mut designer, "a", DVec3::ZERO);
    add_chain(&mut designer, "b", DVec3::new(0.0, 0.0, 0.05));
    add_chain(&mut designer, "far", DVec3::new(0.0, 0.0, 10.0));
    refresh(&mut designer);
    let info = hovered_atom_info_view(&designer, &RAY_ORIGIN, &RAY_DIRECTION)
        .expect("the ray hits an atom");
    assert_eq!(info.symbol, "C");
    assert_eq!(info.node_name, "tag_a");
    assert_eq!(info.overlapping_node_names, vec!["tag_b".to_string()]);

    let miss = DVec3::new(100.0, 100.0, -50.0);
    assert!(hovered_atom_info_view(&designer, &miss, &RAY_DIRECTION).is_none());
}
