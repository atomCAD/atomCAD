//! The ring buffer and the derived flags of `ai_edit_log` — Phase 1 of
//! `doc/design_ai_edit_history.md`.
//!
//! These exercise [`AiEditLog`] and [`LayoutOutcome`] directly, with records
//! built by hand: the caps (D10) and the path-keyed layout diff (D9) are
//! properties of the log itself, and driving them through real edits would take
//! a 200-edit session and a 32 MB network to reach. The end-to-end assembly —
//! that `ai_text_edit` fills a record with the right things — is
//! `ai_edit_history_test.rs`.

use atomcad_structure_designer::ai_edit_log::{
    AI_EDIT_LOG_MAX_ENTRIES, AiEditLog, AiEditRecord, LayoutOutcome, LayoutPath, count_nodes_deep,
    snapshot_is_complete,
};
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::node_network::NodeNetwork;
use atomcad_structure_designer::node_type::NodeTypeCategory;
use atomcad_structure_designer::node_type::{NodeType, OutputPinDefinition};
use atomcad_structure_designer::text_format::PositionSnapshot;
use glam::DVec2;

// ============================================================================
// Helpers
// ============================================================================

/// A record that "reached the network", with the two snapshots given.
fn record(network: &str, before: &str, after: &str) -> AiEditRecord {
    AiEditRecord::applied_edit(
        network.to_string(),
        String::new(),
        false,
        true,
        before.to_string(),
        after.to_string(),
        LayoutOutcome::default(),
    )
}

/// An empty network, only ever used for `LayoutOutcome::compute`'s node count.
fn empty_network() -> NodeNetwork {
    NodeNetwork::new(NodeType {
        name: "test".to_string(),
        description: String::new(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::Blueprint),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(atomcad_structure_designer::node_data::NoData {}),
        node_data_saver: atomcad_structure_designer::node_type::no_data_saver,
        node_data_loader: atomcad_structure_designer::node_type::no_data_loader,
    })
}

fn positions(entries: &[(&[&str], DVec2)]) -> PositionSnapshot {
    entries
        .iter()
        .map(|(path, pos)| (path.iter().map(|s| s.to_string()).collect(), *pos))
        .collect()
}

// ============================================================================
// Sequence numbers and the caps (D10)
// ============================================================================

#[test]
fn seq_is_monotonic_from_zero() {
    let mut log = AiEditLog::new();
    for _ in 0..3 {
        log.push(record("main", "a", "a"));
    }
    let seqs: Vec<u64> = log.records().map(|r| r.seq).collect();
    assert_eq!(seqs, vec![0, 1, 2]);
}

#[test]
fn the_entry_cap_evicts_oldest_first() {
    let mut log = AiEditLog::new();
    for _ in 0..AI_EDIT_LOG_MAX_ENTRIES + 50 {
        log.push(record("main", "a", "a"));
    }

    assert_eq!(log.len(), AI_EDIT_LOG_MAX_ENTRIES);
    // The oldest 50 are gone; `seq` keeps counting so the survivors still name
    // the edits they were.
    assert_eq!(log.records().next().unwrap().seq, 50);
    assert_eq!(
        log.last().unwrap().seq,
        (AI_EDIT_LOG_MAX_ENTRIES + 50 - 1) as u64
    );
}

#[test]
fn the_byte_cap_evicts_on_a_large_snapshot() {
    let mut log = AiEditLog::new();
    for _ in 0..5 {
        log.push(record("main", "small", "small"));
    }
    assert_eq!(log.len(), 5);

    // One snapshot past the byte cap on its own. Everything before it goes; the
    // record just pushed is never the one evicted, however large.
    let huge = "x".repeat(33 * 1024 * 1024);
    log.push(record("main", &huge, ""));

    assert_eq!(log.len(), 1);
    assert_eq!(log.last().unwrap().seq, 5);
}

#[test]
fn clear_empties_the_ring_but_not_the_sequence() {
    let mut log = AiEditLog::new();
    log.push(record("main", "a", "a"));
    log.push(record("main", "a", "b"));
    log.clear();

    assert!(log.is_empty());
    assert_eq!(log.snapshot_bytes(), 0);

    // `seq` identifies an edit within the session; reusing numbers after a
    // clear would make an exported log ambiguous.
    log.push(record("main", "b", "c"));
    assert_eq!(log.last().unwrap().seq, 2);
}

#[test]
fn the_version_advances_on_every_mutation() {
    let mut log = AiEditLog::new();
    let start = log.version();

    log.push(record("main", "a", "a"));
    let after_push = log.version();
    assert!(after_push > start);

    log.set_session_label("Opus 5 / skill v3".to_string());
    assert!(log.version() > after_push);
    assert_eq!(log.session_label(), "Opus 5 / skill v3");

    let before_clear = log.version();
    log.clear();
    assert!(log.version() > before_clear);
}

// ============================================================================
// Divergence (D7)
// ============================================================================

#[test]
fn consecutive_entries_that_line_up_do_not_diverge() {
    let mut log = AiEditLog::new();
    log.push(record("main", "v0", "v1"));
    log.push(record("main", "v1", "v2"));

    assert!(!log.last().unwrap().diverged);
}

#[test]
fn a_gap_between_entries_diverges() {
    let mut log = AiEditLog::new();
    log.push(record("main", "v0", "v1"));
    // Something changed the network between the two edits.
    log.push(record("main", "v1-plus-a-gui-edit", "v2"));

    let last = log.last().unwrap();
    assert!(last.diverged);
    // Nothing said an undo explained it.
    assert!(!last.diverged_by_undo);
}

#[test]
fn divergence_is_keyed_per_network() {
    let mut log = AiEditLog::new();
    log.push(record("main", "m0", "m1"));
    // An edit to a *different* network in between must not trip the marker —
    // this is why `/networks/activate` cannot cause a false positive.
    log.push(record("other", "o0", "o1"));
    log.push(record("main", "m1", "m2"));

    assert!(!log.last().unwrap().diverged);
}

#[test]
fn an_undo_between_edits_words_the_marker_differently() {
    let mut log = AiEditLog::new();
    log.push(record("main", "v0", "v1"));
    log.note_ai_edit_undo();
    log.push(record("main", "v0", "v1-again"));

    let last = log.last().unwrap();
    assert!(last.diverged);
    assert!(last.diverged_by_undo);
}

#[test]
fn an_undo_that_left_no_gap_does_not_claim_one() {
    let mut log = AiEditLog::new();
    log.push(record("main", "v0", "v1"));
    log.note_ai_edit_undo();
    // Undone and redone, say: the state the next edit finds is the state the
    // last one left. `diverged_by_undo` only ever qualifies a real gap.
    log.push(record("main", "v1", "v2"));

    let last = log.last().unwrap();
    assert!(!last.diverged);
    assert!(!last.diverged_by_undo);
}

#[test]
fn the_undo_note_is_consumed_by_one_entry() {
    let mut log = AiEditLog::new();
    log.push(record("main", "v0", "v1"));
    log.note_ai_edit_undo();
    log.push(record("main", "v0", "v1-again"));
    // A later gap with no undo behind it gets the general wording.
    log.push(record("main", "something-else", "v2"));

    assert!(log.last().unwrap().diverged);
    assert!(!log.last().unwrap().diverged_by_undo);
}

#[test]
fn a_rejected_entry_takes_no_part_in_divergence() {
    let mut log = AiEditLog::new();
    log.push(record("main", "v0", "v1"));
    // A write-locked or no-such-network edit says nothing about the state the
    // network was left in, so it must neither diverge nor make the next entry
    // diverge against its empty snapshots.
    log.push(AiEditRecord::rejected(
        "main".to_string(),
        "s = sphere { radius: 5 }".to_string(),
        false,
        vec!["Write access to 'main' is locked.".to_string()],
    ));
    assert!(!log.records().nth(1).unwrap().diverged);

    log.push(record("main", "v1", "v2"));
    assert!(!log.last().unwrap().diverged);
}

// ============================================================================
// Snapshot completeness (D2)
// ============================================================================

#[test]
fn the_serializers_error_comment_marks_a_snapshot_incomplete() {
    assert!(snapshot_is_complete("# Network: main\n\ns = sphere { }\n"));
    // A wire cycle is the serializer's only failure channel, and it is silent
    // apart from this comment.
    assert!(!snapshot_is_complete(
        "# Network: main\n\n# Error: Cycle detected involving node 'a'\n"
    ));
}

#[test]
fn a_records_completeness_flags_come_from_its_snapshots() {
    let good = record("main", "s = sphere { }", "s = sphere { radius: 5 }");
    assert!(good.before_complete && good.after_complete);

    let broken = record("main", "s = sphere { }", "# Error: Cycle detected");
    assert!(broken.before_complete);
    assert!(!broken.after_complete);
}

// ============================================================================
// LayoutOutcome (D9)
// ============================================================================

#[test]
fn only_nodes_that_moved_are_reported() {
    let before = positions(&[
        (&["a"], DVec2::new(0.0, 0.0)),
        (&["b"], DVec2::new(10.0, 0.0)),
    ]);
    let after = positions(&[
        (&["a"], DVec2::new(0.0, 0.0)),
        (&["b"], DVec2::new(10.0, 30.0)),
        // Created by this edit: new, not moved.
        (&["c"], DVec2::new(50.0, 50.0)),
    ]);

    let outcome = LayoutOutcome::compute(LayoutPath::FullReflow, &empty_network(), &before, &after);

    assert_eq!(outcome.moved.len(), 1);
    assert_eq!(outcome.moved[0].path_string(), "b");
    assert_eq!(outcome.max_displacement, 30.0);
    assert_eq!(outcome.path, LayoutPath::FullReflow);
    // Not populated until `design_incremental_layout.md` lands.
    assert!(outcome.delta.is_none());
}

#[test]
fn a_node_that_only_existed_before_is_not_a_move() {
    let before = positions(&[(&["gone"], DVec2::new(0.0, 0.0))]);
    let after = positions(&[]);

    let outcome = LayoutOutcome::compute(LayoutPath::None, &empty_network(), &before, &after);
    assert!(outcome.moved.is_empty());
    assert_eq!(outcome.max_displacement, 0.0);
}

/// The bare-name collision D9 exists to prevent: `m1/d` and `m2/d` are
/// different nodes, and a name-keyed map would merge them into one entry —
/// reporting the wrong node as moved, or none.
#[test]
fn body_nodes_with_the_same_name_stay_separate() {
    let before = positions(&[
        (&["m1", "d"], DVec2::new(0.0, 0.0)),
        (&["m2", "d"], DVec2::new(0.0, 0.0)),
    ]);
    let after = positions(&[
        (&["m1", "d"], DVec2::new(5.0, 0.0)),
        (&["m2", "d"], DVec2::new(0.0, 9.0)),
    ]);

    let outcome = LayoutOutcome::compute(LayoutPath::None, &empty_network(), &before, &after);

    let paths: Vec<String> = outcome.moved.iter().map(|m| m.path_string()).collect();
    assert_eq!(paths, vec!["m1/d".to_string(), "m2/d".to_string()]);
    assert_eq!(outcome.max_displacement, 9.0);
}

#[test]
fn moved_nodes_are_sorted_by_path() {
    let before = positions(&[
        (&["z"], DVec2::ZERO),
        (&["a"], DVec2::ZERO),
        (&["m", "b"], DVec2::ZERO),
    ]);
    let after = positions(&[
        (&["z"], DVec2::new(1.0, 0.0)),
        (&["a"], DVec2::new(1.0, 0.0)),
        (&["m", "b"], DVec2::new(1.0, 0.0)),
    ]);

    let outcome = LayoutOutcome::compute(LayoutPath::None, &empty_network(), &before, &after);
    let paths: Vec<String> = outcome.moved.iter().map(|m| m.path_string()).collect();
    assert_eq!(
        paths,
        vec!["a".to_string(), "m/b".to_string(), "z".to_string()]
    );
}

#[test]
fn an_empty_network_counts_zero_nodes() {
    assert_eq!(count_nodes_deep(&empty_network()), 0);
}
