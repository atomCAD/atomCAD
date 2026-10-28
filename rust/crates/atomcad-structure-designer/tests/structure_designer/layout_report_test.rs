//! The layout report and `--dry-run` (`doc/design_deliberate_node_placement.md`
//! A3, Phase 3).
//!
//! The report is the incremental pass's own before/after diff with a cause
//! attached to every move, plus the findings: what this edit created and
//! nothing repaired. These tests go through the real choke point,
//! [`StructureDesigner::ai_text_edit`], and read the JSON the AI reads.
//!
//! Most findings cannot be produced without layout statements (Phase 5): the
//! pass itself never leaves a new overlap behind, which is what the oracle's
//! invariant 1 asserts everywhere. So the overlap finding is tested on
//! [`scope_findings`] directly, against a drawing moved by hand; the backward
//! wire, which a cycle produces through an ordinary edit, end to end.

use glam::DVec2;
use serde_json::Value;

use atomcad_structure_designer::layout::{
    FindingKind, MoveCause, measure_scope, rendered_node_size, scope_findings,
};
use atomcad_structure_designer::node_network::NodeNetwork;
use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::serialization::node_networks_serialization::serialize_registry_to_string;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::snapshot_node_positions;

use super::layout_test_support::p;

// ============================================================================
// Harness
// ============================================================================

/// A designer with one empty active network called `main`, with the
/// preferences these tests depend on pinned (see `layout_ai_edit_test.rs`).
fn designer() -> StructureDesigner {
    let mut sd = StructureDesigner::new();
    sd.preferences.node_display_preferences.display_policy = NodeDisplayPolicy::Manual;
    sd.preferences
        .layout_preferences
        .respect_hand_moved_in_reflow = false;
    sd.add_node_network("main");
    sd.set_active_node_network_name(Some("main".to_string()));
    sd
}

fn edit(sd: &mut StructureDesigner, code: &str) -> Value {
    let outcome = sd.ai_text_edit(code, false);
    assert!(
        outcome.result.success,
        "edit failed: {:?}",
        outcome.result.errors
    );
    serde_json::from_str(&outcome.to_json()).expect("the result is JSON")
}

fn network<'a>(sd: &'a StructureDesigner, name: &str) -> &'a NodeNetwork {
    sd.node_type_registry
        .node_networks
        .get(name)
        .expect("the network exists")
}

fn place(sd: &mut StructureDesigner, name: &str, x: f64, y: f64) {
    let network = sd
        .node_type_registry
        .node_networks
        .get_mut("main")
        .expect("the active network");
    let id = network
        .nodes
        .values()
        .find(|node| node.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named `{name}`"))
        .id;
    network.nodes.get_mut(&id).expect("just found").position = DVec2::new(x, y);
}

fn position(sd: &StructureDesigner, name: &str) -> DVec2 {
    network(sd, "main")
        .nodes
        .values()
        .find(|node| node.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named `{name}`"))
        .position
}

fn size(sd: &StructureDesigner, name: &str) -> DVec2 {
    let node = network(sd, "main")
        .nodes
        .values()
        .find(|node| node.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named `{name}`"));
    rendered_node_size(node, &sd.node_type_registry)
}

/// The text a save would write: "bit-identical" for a whole design.
fn saved_text(sd: &mut StructureDesigner) -> String {
    let rules = sd.cli_access_rules.clone();
    let mode = sd.direct_editing_mode;
    serialize_registry_to_string(&mut sd.node_type_registry, None, mode, &rules).expect("serialize")
}

const EXPR_X: &str = r#"expression: "x + 1", parameters: [{ name: "x", data_type: Int }]"#;

/// `a -> b`, with `b` exactly one gap right of `a`: no room between them.
fn a_chain(sd: &mut StructureDesigner) {
    edit(
        sd,
        &format!("a = int {{ value: 1 }}\nb = expr {{ x: a, {EXPR_X} }}\n"),
    );
    place(sd, "a", 100.0, 100.0);
    let right = 100.0 + size(sd, "a").x;
    place(sd, "b", right + 50.0, 100.0);
}

// ============================================================================
// The `layout` key and its presence rule
// ============================================================================

/// An edit that moves nothing that existed says nothing about layout, even
/// though it placed a new node.
#[test]
fn an_edit_that_moves_no_existing_node_has_no_layout_key() {
    let mut sd = designer();
    a_chain(&mut sd);

    let json = edit(&mut sd, "c = int { value: 9 }\n");

    assert!(
        json.get("layout").is_none(),
        "nothing moved, nothing to say: {json}"
    );
}

/// A node inserted between two that have no room between them shifts the
/// consumer: the report names it, with cause `auto` and the new node it made
/// room for, and gives the new node's final box.
#[test]
fn a_shifted_existing_node_is_reported_with_its_cause() {
    let mut sd = designer();
    a_chain(&mut sd);
    let b_before = position(&sd, "b");

    let json = edit(
        &mut sd,
        &format!("n = expr {{ x: a, {EXPR_X} }}\nb = expr {{ x: n, {EXPR_X} }}\n"),
    );

    let b_after = position(&sd, "b");
    assert!(b_after.x > b_before.x, "the premise: `b` made room");

    let layout = &json["layout"];
    let moved = layout["moved"].as_array().expect("a moved list");
    assert_eq!(moved.len(), 1, "only `b` moved: {layout}");
    assert_eq!(moved[0]["node"], "b");
    assert_eq!(moved[0]["cause"], "auto");
    assert_eq!(
        moved[0]["from"],
        serde_json::json!([b_before.x.round() as i64, b_before.y.round() as i64])
    );
    assert_eq!(
        moved[0]["to"],
        serde_json::json!([b_after.x.round() as i64, b_after.y.round() as i64])
    );
    assert!(
        moved[0]["why"].as_str().unwrap().contains("new node n"),
        "the reason names what it made room for: {}",
        moved[0]["why"]
    );

    let placed = layout["auto_placed"].as_array().expect("the new node");
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0]["node"], "n");
    let (n_pos, n_size) = (position(&sd, "n"), size(&sd, "n"));
    assert_eq!(
        placed[0]["box"],
        serde_json::json!([
            n_pos.x.round() as i64,
            n_pos.y.round() as i64,
            n_size.x.round() as i64,
            n_size.y.round() as i64
        ])
    );
    assert_eq!(layout["findings"], serde_json::json!([]));
    assert_eq!(layout["warnings"], serde_json::json!([]));
}

/// A node that grows in place pushes its neighbour, and the report says
/// `grow`, naming the node that grew.
#[test]
fn a_node_pushed_by_a_growth_is_reported_as_grow() {
    let mut sd = designer();
    edit(
        &mut sd,
        &format!("a = int {{ value: 1 }}\nb = expr {{ x: a, {EXPR_X} }}\nc = int {{ value: 2 }}\n"),
    );
    place(&mut sd, "a", 100.0, 100.0);
    place(&mut sd, "b", 400.0, 100.0);
    let below_b = 100.0 + size(&sd, "b").y + 30.0;
    place(&mut sd, "c", 400.0, below_b);

    // A second parameter is a second pin: `b` grows downward into `c`.
    let json = edit(
        &mut sd,
        r#"b = expr { x: a, y: a, expression: "x + y", parameters: [{ name: "x", data_type: Int }, { name: "y", data_type: Int }] }
"#,
    );

    assert!(
        position(&sd, "c").y > below_b,
        "the premise: `c` was pushed"
    );
    let moved = json["layout"]["moved"].as_array().expect("a moved list");
    let c = moved
        .iter()
        .find(|m| m["node"] == "c")
        .expect("`c` is reported");
    assert_eq!(c["cause"], "grow");
    assert_eq!(c["why"], "made room for b as it grew");
}

/// The log's moved list is the same diff, and now carries the same causes —
/// the History panel's Layout tab reads them from there.
#[test]
fn the_edit_log_carries_the_reported_causes() {
    let mut sd = designer();
    a_chain(&mut sd);

    edit(
        &mut sd,
        &format!("n = expr {{ x: a, {EXPR_X} }}\nb = expr {{ x: n, {EXPR_X} }}\n"),
    );

    let record = sd.ai_edit_log.last().expect("logged");
    let b = record
        .layout
        .moved
        .iter()
        .find(|m| m.path == p(&["b"]))
        .expect("`b` is in the log's moved list");
    assert_eq!(b.cause, Some(MoveCause::Auto));
    assert!(b.why.contains("new node n"), "{}", b.why);
}

// ============================================================================
// Findings
// ============================================================================

/// A new overlap is a finding; one that was already there is the baseline.
#[test]
fn a_new_overlap_is_found_and_a_pre_existing_one_is_not() {
    let mut sd = designer();
    edit(
        &mut sd,
        "a = int { value: 1 }\nb = int { value: 2 }\nx = int { value: 3 }\ny = int { value: 4 }\n",
    );
    place(&mut sd, "a", 0.0, 0.0);
    place(&mut sd, "b", 600.0, 0.0);
    // `x` and `y` overlap before the "edit" and keep overlapping after it.
    place(&mut sd, "x", 0.0, 400.0);
    place(&mut sd, "y", 40.0, 420.0);
    let snapshot = snapshot_node_positions(network(&sd, "main"), &sd.node_type_registry);

    // The "edit": `b` lands 20 px into `a`'s right edge.
    let a_width = size(&sd, "a").x;
    place(&mut sd, "b", a_width - 20.0, 10.0);

    let scope = network(&sd, "main");
    let sizes = measure_scope(scope, &sd.node_type_registry);
    let findings = scope_findings(scope, &sizes, &[], &snapshot, &[]);

    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].kind, FindingKind::Overlap);
    assert_eq!(findings[0].nodes, vec![p(&["a"]), p(&["b"])]);
    let overlap_h = (size(&sd, "a").y - 10.0).round() as i64;
    assert_eq!(
        findings[0].message,
        format!("a overlaps b by 20×{overlap_h} px")
    );
}

/// Moving a node *off* an overlap it already had, onto nothing, finds
/// nothing; so does an edit that moves nothing.
#[test]
fn moving_without_creating_an_overlap_finds_nothing() {
    let mut sd = designer();
    edit(&mut sd, "a = int { value: 1 }\nb = int { value: 2 }\n");
    place(&mut sd, "a", 0.0, 0.0);
    place(&mut sd, "b", 20.0, 20.0);
    let snapshot = snapshot_node_positions(network(&sd, "main"), &sd.node_type_registry);

    let scope = network(&sd, "main");
    let sizes = measure_scope(scope, &sd.node_type_registry);
    assert!(scope_findings(scope, &sizes, &[], &snapshot, &[]).is_empty());

    place(&mut sd, "b", 900.0, 900.0);
    let scope = network(&sd, "main");
    let sizes = measure_scope(scope, &sd.node_type_registry);
    assert!(scope_findings(scope, &sizes, &[], &snapshot, &[]).is_empty());
}

/// A wire this edit created inside a cycle cannot be put in order by any
/// shift, so Step 6 leaves it — and the report says so.
#[test]
fn a_backward_wire_inside_a_new_cycle_is_a_finding() {
    let mut sd = designer();
    edit(
        &mut sd,
        &format!(
            "s = int {{ value: 1 }}\na = expr {{ x: s, {EXPR_X} }}\nb = expr {{ x: a, {EXPR_X} }}\n"
        ),
    );
    place(&mut sd, "s", 0.0, 0.0);
    place(&mut sd, "a", 300.0, 0.0);
    place(&mut sd, "b", 600.0, 0.0);

    // `b -> a` closes the cycle `a -> b -> a`. The network no longer
    // validates, but the edit applied, so its layout ran and reports.
    let outcome = sd.ai_text_edit(&format!("a = expr {{ x: b, {EXPR_X} }}\n"), false);
    assert!(
        sd.ai_edit_log.last().expect("logged").applied,
        "{:?}",
        outcome.result.errors
    );
    let json: Value = serde_json::from_str(&outcome.to_json()).unwrap();

    let findings = json["layout"]["findings"].as_array().expect("findings");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0]["kind"], "backward_wire");
    assert_eq!(findings[0]["nodes"], serde_json::json!(["b", "a"]));
    let message = findings[0]["message"].as_str().unwrap();
    assert!(
        message.starts_with("wire b -> a runs backward"),
        "{message}"
    );
    assert!(message.contains("a cycle"), "{message}");
    assert_eq!(position(&sd, "a"), DVec2::new(300.0, 0.0), "nothing moved");
}

// ============================================================================
// --dry-run
// ============================================================================

/// The preview's whole contract: what it shows is what the edit then does,
/// and it leaves nothing behind.
#[test]
fn a_dry_run_reports_exactly_what_the_edit_then_does() {
    let mut sd = designer();
    a_chain(&mut sd);
    let code = format!("n = expr {{ x: a, {EXPR_X} }}\nb = expr {{ x: n, {EXPR_X} }}\n");

    let text_before = saved_text(&mut sd);
    let history_before = sd.undo_stack.history_len();
    let log_before = sd.ai_edit_log.len();
    sd.set_dirty(false);

    let preview = sd.ai_text_edit_with_options(&code, false, true);
    assert!(preview.dry_run && !preview.needs_refresh);
    assert_eq!(saved_text(&mut sd), text_before, "the design is untouched");
    assert_eq!(sd.undo_stack.history_len(), history_before, "no undo step");
    assert_eq!(sd.ai_edit_log.len(), log_before, "no log entry");
    assert!(!sd.is_dirty(), "no dirty flag");

    let real = sd.ai_text_edit(&code, false);
    let mut preview_json: Value = serde_json::from_str(&preview.to_json()).unwrap();
    let real_json: Value = serde_json::from_str(&real.to_json()).unwrap();
    assert_eq!(preview_json["dry_run"], true);
    assert!(real_json.get("dry_run").is_none());
    preview_json.as_object_mut().unwrap().remove("dry_run");
    assert_eq!(preview_json, real_json);
    assert!(
        real_json["layout"]["moved"].is_array(),
        "the case is worth previewing"
    );
}

/// The parameter case: deleting a parameter drops its callers' wires in
/// *another* network, outside the edited one. The preview must put those
/// back too, exactly as Ctrl+Z would.
#[test]
fn a_dry_run_that_removes_a_called_parameter_restores_the_callers() {
    let mut sd = designer();
    sd.add_node_network("inner");
    sd.set_active_node_network_name(Some("inner".to_string()));
    edit(
        &mut sd,
        r#"p = parameter { param_name: "p", data_type: Int, sort_order: 0 }
q = parameter { param_name: "q", data_type: Int, sort_order: 1 }
e = expr { a: p, b: q, expression: "a + b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
output e
"#,
    );
    sd.set_active_node_network_name(Some("main".to_string()));
    edit(
        &mut sd,
        "x = int { value: 1 }\ny = int { value: 2 }\nc = inner { p: x, q: y }\n",
    );
    sd.validate_active_network();
    sd.set_active_node_network_name(Some("inner".to_string()));
    sd.validate_active_network();

    let text_before = saved_text(&mut sd);
    let history_before = sd.undo_stack.history_len();
    let log_before = sd.ai_edit_log.len();
    sd.set_dirty(false);

    let preview = sd.ai_text_edit_with_options("delete p\n", false, true);
    assert!(preview.result.success, "{:?}", preview.result.errors);

    assert_eq!(
        saved_text(&mut sd),
        text_before,
        "the callers' wires came back"
    );
    assert_eq!(sd.undo_stack.history_len(), history_before);
    assert_eq!(sd.ai_edit_log.len(), log_before);
    assert!(!sd.is_dirty());
}

/// A dry run that never reaches the network is not logged either.
#[test]
fn a_rejected_dry_run_is_not_logged() {
    let mut sd = StructureDesigner::new();
    sd.set_active_node_network_name(None);
    let log_before = sd.ai_edit_log.len();

    let outcome = sd.ai_text_edit_with_options("a = int { value: 1 }\n", false, true);

    assert!(!outcome.result.success);
    assert!(outcome.dry_run);
    assert_eq!(sd.ai_edit_log.len(), log_before);
    let json: Value = serde_json::from_str(&outcome.to_json()).unwrap();
    assert_eq!(json["dry_run"], true);
    assert!(json.get("layout").is_none());
}
