//! What `StructureDesigner::ai_text_edit` records — Phase 1 of
//! `doc/design_ai_edit_history.md`.
//!
//! The AI edit surface used to leave no record of itself: after a session of AI
//! editing the maintainer could see the current network and nothing else. These
//! tests pin what one entry contains, and in particular the three things a
//! naive recording would get wrong:
//!
//! - **two verdicts, not one** (D8) — `applied` is the editor's own, `success`
//!   folds in a *whole-network* validation verdict, and `applied && !success`
//!   is the ordinary "my edit landed, the network is broken elsewhere";
//! - **text snapshots, not a delta** (D2) — a `--replace` mints fresh node ids
//!   for everything, so only the text is comparable across one;
//! - **the layout record is about every scope** — since Phase 5 of
//!   `doc/design_incremental_layout.md` an applied edit always runs the
//!   incremental pass, bodies included, and the interesting assertion has
//!   flipped: the moved list is expected to be *empty*.
//!
//! The ring buffer and its caps are `ai_edit_log_test.rs`.

use atomcad_structure_designer::ai_edit_log::LayoutPath;
use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::snapshot_node_positions;
use glam::DVec2;

// ============================================================================
// Helpers
// ============================================================================

/// A designer with one empty active network called `main`.
///
/// `StructureDesigner::new()` loads the **real** user preferences, so every
/// preference this file depends on is pinned here rather than inherited from
/// whichever machine runs the suite.
fn designer() -> StructureDesigner {
    let mut sd = StructureDesigner::new();
    sd.preferences.node_display_preferences.display_policy = NodeDisplayPolicy::Manual;
    sd.add_node_network("main");
    sd.set_active_node_network_name(Some("main".to_string()));
    sd
}

/// Apply a script and assert the editor accepted it.
fn edit(sd: &mut StructureDesigner, code: &str) {
    let outcome = sd.ai_text_edit(code, false);
    assert!(
        outcome.result.success,
        "edit should have succeeded, errors: {:?}",
        outcome.result.errors
    );
}

const SPHERE: &str = "sphere1 = sphere { radius: 5 }\noutput sphere1\n";

/// The design doc's canonical `map`: a range, a captured constant and a
/// two-node body.
const MAP_WITH_BODY: &str = r#"
r = range { start: 0, step: 1, count: 5 }
scale = int { value: 3 }
m1 = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, b: ^scale, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    e = expr { a: d, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }
    output e
  }
}
output m1
"#;

// ============================================================================
// A merge edit
// ============================================================================

#[test]
fn a_merge_edit_records_both_snapshots_and_the_code_verbatim() {
    let mut sd = designer();
    edit(&mut sd, SPHERE);

    assert_eq!(sd.ai_edit_log.len(), 1);
    let rec = sd.ai_edit_log.last().unwrap();

    // Exactly what was submitted, byte for byte — no normalization, because a
    // summary written now cannot answer a question thought of later (D8).
    assert_eq!(rec.code, SPHERE);
    assert!(!rec.replace);
    assert_eq!(rec.network_name, "main");

    assert!(rec.applied);
    assert!(rec.success);
    assert_eq!(rec.nodes_created, vec!["sphere1".to_string()]);
    assert!(rec.errors.is_empty());

    // The snapshots are the AI text format — what `query` would have returned
    // at each moment — not `.cnnd`.
    assert!(
        rec.before_text.contains("# Empty network"),
        "before: {}",
        rec.before_text
    );
    assert!(
        rec.after_text.contains("sphere1 = sphere"),
        "after: {}",
        rec.after_text
    );
    assert!(rec.before_complete && rec.after_complete);
    assert!(!rec.diverged);
}

#[test]
fn the_three_easily_dropped_result_fields_survive_into_the_record() {
    let mut sd = designer();
    edit(
        &mut sd,
        concat!(
            "description \"a test network\"\n",
            "summary \"tests\"\n",
            "sphere1 = sphere { radius: 5 }\n",
            "output sphere1\n"
        ),
    );

    let rec = sd.ai_edit_log.last().unwrap();
    assert_eq!(rec.description_set.as_deref(), Some("a test network"));
    assert_eq!(rec.summary_set.as_deref(), Some("tests"));
    // `output_set` is the only trace a re-pointed return node leaves.
    assert_eq!(rec.output_set.as_deref(), Some("sphere1"));
}

// ============================================================================
// A replace edit (D2)
// ============================================================================

/// The decision the whole feature rests on: a `--replace` rebuilds every node
/// with a fresh id, so an id-keyed delta would report "everything deleted,
/// everything created". The *text* is unchanged, because names and positions
/// are carried across.
#[test]
fn a_replace_edit_records_the_flag_and_two_comparable_snapshots() {
    let mut sd = designer();
    edit(&mut sd, SPHERE);

    let outcome = sd.ai_text_edit(SPHERE, true);
    assert!(outcome.result.success, "{:?}", outcome.result.errors);

    assert_eq!(sd.ai_edit_log.len(), 2);
    let rec = sd.ai_edit_log.last().unwrap();
    assert!(rec.replace);
    assert_eq!(
        rec.before_text, rec.after_text,
        "a --replace of an unchanged script must be a no-op in the text"
    );
    assert!(!rec.diverged);
}

#[test]
fn a_replace_of_a_body_bearing_script_is_also_a_text_no_op() {
    let mut sd = designer();
    edit(&mut sd, MAP_WITH_BODY);

    let outcome = sd.ai_text_edit(MAP_WITH_BODY, true);
    assert!(outcome.result.success, "{:?}", outcome.result.errors);

    let rec = sd.ai_edit_log.last().unwrap();
    assert_eq!(rec.before_text, rec.after_text);
}

// ============================================================================
// The three rejection paths (D1)
// ============================================================================

#[test]
fn a_write_locked_edit_records_a_failed_entry_with_empty_snapshots() {
    let mut sd = designer();
    sd.set_cli_access("main", false);

    let outcome = sd.ai_text_edit(SPHERE, false);
    assert!(!outcome.result.success);
    // Nothing was touched, so nothing needs redrawing.
    assert!(!outcome.needs_refresh);

    let rec = sd.ai_edit_log.last().unwrap();
    assert!(!rec.applied);
    assert!(!rec.success);
    assert_eq!(rec.code, SPHERE);
    assert!(rec.before_text.is_empty() && rec.after_text.is_empty());
    assert_eq!(rec.errors.len(), 1);
    assert!(
        rec.errors[0].contains("locked"),
        "expected the lock message, got {:?}",
        rec.errors
    );
}

#[test]
fn an_edit_with_no_active_network_records_a_failed_entry() {
    let mut sd = designer();
    sd.set_active_node_network_name(None);

    let outcome = sd.ai_text_edit(SPHERE, false);
    assert!(!outcome.result.success);

    let rec = sd.ai_edit_log.last().unwrap();
    assert!(!rec.applied);
    assert_eq!(rec.network_name, "");
    assert_eq!(rec.errors, vec!["No active node network".to_string()]);
}

// ============================================================================
// Broken scripts
// ============================================================================

#[test]
fn a_syntactically_broken_script_records_its_errors_and_changes_nothing() {
    let mut sd = designer();
    edit(&mut sd, SPHERE);

    let outcome = sd.ai_text_edit("sphere1 = sphere { radius: ", false);
    assert!(!outcome.result.success);

    let rec = sd.ai_edit_log.last().unwrap();
    assert!(!rec.applied);
    assert!(!rec.errors.is_empty());
    // A parse error rejects the whole script, so the state is untouched — and
    // the snapshots say so rather than being empty.
    assert_eq!(rec.before_text, rec.after_text);
    assert!(rec.before_text.contains("sphere1 = sphere"));
}

/// "…and whatever partial state resulted": an unknown node type fails the edit
/// but the statements around it still land, and the snapshot pair is what shows
/// that.
#[test]
fn a_partially_applied_script_records_the_state_it_actually_left() {
    let mut sd = designer();

    let outcome = sd.ai_text_edit(
        "good = sphere { radius: 5 }\nbad = no_such_node_type { }\n",
        false,
    );
    assert!(!outcome.result.success);

    let rec = sd.ai_edit_log.last().unwrap();
    assert!(!rec.applied);
    assert!(!rec.errors.is_empty());
    assert!(
        rec.after_text.contains("good = sphere"),
        "the partial effect must be visible in the after-snapshot: {}",
        rec.after_text
    );
    assert_ne!(rec.before_text, rec.after_text);
}

// ============================================================================
// The two verdicts (D8)
// ============================================================================

/// The ordinary two-step of creating a `map` and then filling its body: an HOF
/// with no body and no wired `f:` is blocking-invalid by construction, so the
/// first step lands cleanly *and* leaves the network invalid.
#[test]
fn an_hof_with_no_body_records_applied_but_not_success() {
    let mut sd = designer();
    let outcome = sd.ai_text_edit("m1 = map { input_type: Int, output_type: Int }", false);

    let rec = sd.ai_edit_log.last().unwrap();
    assert!(rec.applied, "the statement parsed and landed");
    assert!(!rec.success, "but the network does not validate");
    assert!(!rec.errors.is_empty());
    // The reported result carries the same pair of verdicts.
    assert!(!outcome.result.success);
    assert_eq!(rec.nodes_created, vec!["m1".to_string()]);
}

/// The case a single glyph would misreport: the *previous* edit left the
/// network blocking-invalid, so a later, entirely clean edit still records
/// `success: false`. `applied` is what says the AI did its job.
#[test]
fn an_edit_on_an_already_invalid_network_is_applied_but_not_successful() {
    let mut sd = designer();
    sd.ai_text_edit("m1 = map { input_type: Int, output_type: Int }", false);

    let outcome = sd.ai_text_edit("s = sphere { radius: 5 }", false);
    assert!(!outcome.result.success);

    let rec = sd.ai_edit_log.last().unwrap();
    assert!(rec.applied);
    assert!(!rec.success);
    assert_eq!(rec.nodes_created, vec!["s".to_string()]);
    // The two are distinguishable in the row, which is the whole point of
    // recording both.
    assert_ne!(rec.applied, rec.success);
}

// ============================================================================
// Divergence (D7)
// ============================================================================

#[test]
fn two_consecutive_ai_edits_do_not_diverge() {
    let mut sd = designer();
    edit(&mut sd, SPHERE);
    edit(&mut sd, "cuboid1 = cuboid { extent: (2, 2, 2) }\n");

    assert!(!sd.ai_edit_log.last().unwrap().diverged);
}

#[test]
fn a_gui_edit_between_two_ai_edits_sets_diverged() {
    let mut sd = designer();
    edit(&mut sd, SPHERE);

    // A human drops a node on the canvas.
    sd.add_node("cuboid", DVec2::new(100.0, 100.0));

    edit(&mut sd, "s2 = sphere { radius: 7 }\n");
    let rec = sd.ai_edit_log.last().unwrap();
    assert!(rec.diverged);
    assert!(!rec.diverged_by_undo);
}

/// The endpoint that was missed on the first pass: `node-policy` reaches
/// `set_preferences`, which reapplies the display policy and rewrites the
/// active network's `displayed_nodes` — and `visible:` **is** in the text
/// format.
#[test]
fn a_node_policy_change_between_two_edits_sets_diverged() {
    let mut sd = designer();
    edit(&mut sd, SPHERE);

    let mut prefs = sd.preferences.clone();
    prefs.node_display_preferences.display_policy = NodeDisplayPolicy::PreferFrontier;
    sd.set_preferences(prefs);

    edit(&mut sd, "cuboid1 = cuboid { extent: (2, 2, 2) }\n");
    assert!(sd.ai_edit_log.last().unwrap().diverged);
}

/// Undo is now the likeliest source of divergence, and the marker should say so
/// rather than report an unknown outside change.
#[test]
fn an_undo_of_an_ai_edit_is_named_as_such_on_the_next_entry() {
    let mut sd = designer();
    edit(&mut sd, SPHERE);
    edit(&mut sd, "cuboid1 = cuboid { extent: (2, 2, 2) }\n");

    assert!(sd.undo(), "the AI edit should be one undo step");

    edit(&mut sd, "s2 = sphere { radius: 7 }\n");
    let rec = sd.ai_edit_log.last().unwrap();
    assert!(rec.diverged);
    assert!(rec.diverged_by_undo);
}

/// D6: the log is not a view of document state. Undoing an edit does not remove
/// its entry — an entry that was later undone is still a thing that happened.
#[test]
fn undo_does_not_rewrite_the_log() {
    let mut sd = designer();
    edit(&mut sd, SPHERE);
    let before = sd.ai_edit_log.len();

    assert!(sd.undo());
    assert_eq!(sd.ai_edit_log.len(), before);
}

// ============================================================================
// Layout (D9)
// ============================================================================

/// The property the whole incremental design exists to buy, read off the log:
/// an edit that adds a node reports the pass that ran and an **empty** moved
/// list. Before Phase 5 this same edit reflowed the network and rewrote every
/// position in it.
#[test]
fn an_added_node_moves_nothing_that_was_already_there() {
    let mut sd = designer();
    edit(
        &mut sd,
        "a = sphere { radius: 5 }\nb = cuboid { extent: (2, 2, 2) }\n",
    );
    edit(&mut sd, "c = sphere { radius: 9 }\n");

    let rec = sd.ai_edit_log.last().unwrap();
    assert_eq!(rec.layout.path, LayoutPath::Incremental);
    assert!(
        rec.layout.moved.is_empty(),
        "existing nodes must not move: {:?}",
        rec.layout.moved
    );
    assert_eq!(rec.layout.max_displacement, 0.0);
    assert_eq!(rec.layout.node_count, 3);
}

/// The counters the record has carried an empty slot for since its own Phase 1
/// are populated now: what the edit *asked* layout to do, beside the list of
/// what layout then did.
#[test]
fn the_edits_delta_is_recorded_beside_the_moves() {
    let mut sd = designer();
    edit(
        &mut sd,
        concat!(
            "a = sphere { radius: 5 }\n",
            "b = cuboid { extent: (2, 2, 2) }\n",
            "u = union { shapes: [a, b] }\n",
            "output u\n"
        ),
    );

    edit(&mut sd, "c = sphere { radius: 9 }\n");

    let rec = sd.ai_edit_log.last().unwrap();
    assert_eq!(rec.layout.path, LayoutPath::Incremental);
    let delta = rec.layout.delta.expect("the incremental pass records one");
    assert_eq!(delta.nodes_added, 1, "`c`, and nothing else");
    assert_eq!(delta.nodes_removed, 0);
    assert_eq!(delta.wires_added, 0);
    assert_eq!(rec.layout.node_count, 4);
}

/// A body is a first-class scope for the incremental pass, so a rebuild inside
/// one is laid out like any other edit — a renamed body node fails the identity
/// match, is classified `added`, and is placed as its own block.
#[test]
fn a_body_rebuild_is_laid_out_in_the_body() {
    let mut sd = designer();
    edit(&mut sd, MAP_WITH_BODY);

    // Rename the body's first node, which makes it a new node placed by the
    // creation-time placer rather than by any layout pass — the distinction
    // `path` exists to keep straight.
    edit(
        &mut sd,
        r#"
m1 = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    renamed = expr { a: $element, b: ^scale, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    e = expr { a: renamed, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }
    output e
  }
}
"#,
    );

    let rec = sd.ai_edit_log.last().unwrap();
    assert_eq!(rec.layout.path, LayoutPath::Incremental);
    let delta = rec.layout.delta.expect("the incremental pass records one");
    assert_eq!(
        delta.nodes_added, 1,
        "the renamed body node fails the identity match and counts as added"
    );
    // `node_count` counts bodies too: r, scale, m1 and the two body nodes.
    assert_eq!(rec.layout.node_count, 5);
}

/// A node added to the *root* scope must not disturb a body — the delta is per
/// scope, and the body's is empty. (Before Phase 5 the same assertion held for
/// the opposite reason: no layout pass ever reached a body at all.)
#[test]
fn a_root_scope_addition_leaves_a_body_alone() {
    let mut sd = designer();
    edit(&mut sd, MAP_WITH_BODY);

    edit(
        &mut sd,
        "extra = int { value: 7 }
",
    );

    let rec = sd.ai_edit_log.last().unwrap();
    assert_eq!(rec.layout.path, LayoutPath::Incremental);
    assert!(
        rec.layout.moved.is_empty(),
        "nothing in any scope should have moved: {:?}",
        rec.layout
            .moved
            .iter()
            .map(|m| m.path_string())
            .collect::<Vec<_>>()
    );
    // The count reaches into bodies.
    assert_eq!(rec.layout.node_count, 6);
}

/// The bare-name collision D9 exists to prevent, at the source: the promoted
/// identity walk keys every body node by its **path**, so `m1/d` and `m2/d`
/// stay apart. A bare-name map would merge them into one entry and the log
/// would name the wrong node as moved, or none.
#[test]
fn the_shared_identity_walk_keys_body_nodes_by_path() {
    let mut sd = designer();
    edit(
        &mut sd,
        concat!(
            "r = range { start: 0, step: 1, count: 3 }
",
            "m1 = map { xs: r, input_type: Int, output_type: Int, body { d = int { value: 1 } output d } }
",
            "m2 = map { xs: r, input_type: Int, output_type: Int, body { d = int { value: 2 } output d } }
"
        ),
    );

    let network = sd.node_type_registry.node_networks.get("main").unwrap();
    let snapshot = snapshot_node_positions(network, &sd.node_type_registry);

    let mut paths: Vec<String> = snapshot.keys().map(|p| p.join("/")).collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![
            "m1".to_string(),
            "m1/d".to_string(),
            "m2".to_string(),
            "m2/d".to_string(),
            "r".to_string(),
        ]
    );
    // Same name, two scopes, two entries that track independently: displace
    // one and only its key changes. A bare-name map would have had a single
    // `d` and would now report the wrong position for both.
    let m1 = node_id(&sd, "m1");
    let network = sd.node_type_registry.node_networks.get_mut("main").unwrap();
    let body = network.nodes.get_mut(&m1).unwrap().zone_mut().unwrap();
    for node in body.nodes.values_mut() {
        node.position += DVec2::new(40.0, 0.0);
    }

    let moved = snapshot_node_positions(
        sd.node_type_registry.node_networks.get("main").unwrap(),
        &sd.node_type_registry,
    );
    let m1_d = vec!["m1".to_string(), "d".to_string()];
    let m2_d = vec!["m2".to_string(), "d".to_string()];
    assert_eq!(
        moved[&m1_d].position,
        snapshot[&m1_d].position + DVec2::new(40.0, 0.0)
    );
    assert_eq!(moved[&m2_d].position, snapshot[&m2_d].position);
}

// ============================================================================
// A truncated snapshot (D2)
// ============================================================================

/// A wire cycle is the serializer's only failure channel and it fails
/// *silently*: the root scope drops every remaining statement and the footer,
/// leaving a `# Error:` comment. Unflagged, that would make the diff report a
/// mass deletion that never happened — and the text is not valid
/// `edit --replace` input either, since it no longer describes the network.
#[test]
fn a_wire_cycle_records_an_incomplete_before_snapshot() {
    let mut sd = designer();
    edit(
        &mut sd,
        "a = union { shapes: [] }
b = union { shapes: [] }
",
    );

    let a = node_id(&sd, "a");
    let b = node_id(&sd, "b");
    sd.connect_nodes(a, 0, b, 0);
    sd.connect_nodes(b, 0, a, 0);

    sd.ai_text_edit(
        "c = sphere { radius: 1 }
",
        false,
    );

    let rec = sd.ai_edit_log.last().unwrap();
    assert!(!rec.before_complete, "before: {:?}", rec.before_text);
    assert!(rec.before_text.contains("# Error:"));
}

/// Top-level id of a node by its name.
fn node_id(sd: &StructureDesigner, name: &str) -> u64 {
    let network = sd.node_type_registry.node_networks.get("main").unwrap();
    *network
        .nodes
        .iter()
        .find(|(_, node)| node.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named {}", name))
        .0
}

// ============================================================================
// The log is session state, not document state (D10)
// ============================================================================

#[test]
fn the_log_is_absent_from_a_saved_cnnd_and_survives_a_load() {
    let mut sd = designer();
    edit(&mut sd, SPHERE);

    let dir = std::env::temp_dir().join("atomcad_ai_edit_history_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("log_not_persisted.cnnd");
    let path_str = path.to_str().unwrap();
    sd.save_node_networks_as(path_str).unwrap();

    // Nothing about the log reaches the file — not the submitted script, not
    // the snapshots.
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(
        !saved.contains("sphere1 = sphere"),
        "the AI text-format snapshot must not be written to .cnnd"
    );

    // …and it is a *session* log, so a load does not forget it (open question 3
    // recommends exactly this).
    sd.load_node_networks(path_str).unwrap();
    assert_eq!(sd.ai_edit_log.len(), 1);
    assert_eq!(sd.ai_edit_log.last().unwrap().code, SPHERE);

    let _ = std::fs::remove_file(&path);
}

// ============================================================================
// A `--replace` that creates nothing still changed the network
// ============================================================================

/// The editor clears the network before a single statement runs, so a
/// `--replace` whose script then creates nothing has wiped the drawing while
/// reporting every change list empty. A comment-only script is exactly what a
/// `query` printout cut short at its first blank line amounts to, and that
/// combination emptied a 135-node network with Undo greyed out and no dirty
/// flag. Both gates key on the text pair now, not on the change lists alone.
#[test]
fn a_replace_that_creates_nothing_is_still_dirty_and_undoable() {
    let mut sd = designer();
    edit(&mut sd, SPHERE);
    sd.set_dirty(false);

    let outcome = sd.ai_text_edit("# Network: main\n", true);
    assert!(outcome.result.success, "{:?}", outcome.result.errors);
    assert!(
        outcome.result.nodes_created.is_empty() && outcome.result.nodes_deleted.is_empty(),
        "the editor reports nothing, which is the whole problem"
    );
    assert!(
        sd.node_type_registry.node_networks["main"].nodes.is_empty(),
        "and yet the network was cleared"
    );

    assert!(sd.is_dirty(), "an emptied network is a changed document");
    assert!(sd.undo(), "and one undo step");
    assert_eq!(
        sd.node_type_registry.node_networks["main"].nodes.len(),
        1,
        "undo brings the sphere back"
    );
}
