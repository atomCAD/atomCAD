//! JSON and Markdown export of the AI edit log — Phase 4 of
//! `doc/design_ai_edit_history.md`.
//!
//! Export is how a session leaves the process (D10), so what these assert is
//! **completeness of the JSON form** rather than its prettiness: every field a
//! downstream reader might want has to be there, because the whole point of not
//! summarising at record time is that a question can be thought of later. The
//! Markdown form is the opposite bargain — readable, deliberately without the
//! snapshots — and the tests say so.

use atomcad_structure_designer::ai_edit_export::{
    EXPORT_FORMAT_ID, EXPORT_FORMAT_VERSION, export_json, export_markdown,
};
use atomcad_structure_designer::ai_edit_log::{
    AiEditLog, AiEditRecord, LayoutOutcome, LayoutPath, MovedNode,
};
use glam::DVec2;

// ============================================================================
// Helpers
// ============================================================================

fn applied(network: &str, code: &str, before: &str, after: &str) -> AiEditRecord {
    AiEditRecord::applied_edit(
        network.to_string(),
        code.to_string(),
        false,
        true,
        before.to_string(),
        after.to_string(),
        LayoutOutcome::default(),
    )
}

fn parse(json: &str) -> serde_json::Value {
    serde_json::from_str(json).expect("export must be valid JSON")
}

// ============================================================================
// JSON — the canonical form
// ============================================================================

#[test]
fn an_empty_log_still_exports_a_well_formed_document() {
    let document = parse(&export_json(&AiEditLog::new()));

    assert_eq!(document["format"], EXPORT_FORMAT_ID);
    assert_eq!(document["version"], EXPORT_FORMAT_VERSION);
    assert_eq!(document["entry_count"], 0);
    assert_eq!(document["entries"].as_array().unwrap().len(), 0);
}

#[test]
fn json_carries_the_session_label() {
    let mut log = AiEditLog::new();
    log.set_session_label("Opus 5 / skill v3".to_string());
    log.push(applied("main", "a = sphere { radius: 5.0 }", "v0", "v1"));

    let document = parse(&export_json(&log));
    assert_eq!(document["session_label"], "Opus 5 / skill v3");
}

/// The submitted script and both snapshots verbatim: this is what makes an
/// exported entry replayable through `edit --replace` and diffable outside the
/// application.
#[test]
fn json_carries_the_code_and_both_snapshots_verbatim() {
    let mut log = AiEditLog::new();
    let code = "a = sphere { radius: 5.0 }\nb = union { shapes: [a] }";
    log.push(applied(
        "beam",
        code,
        "# Network: beam\n",
        "# Network: beam\na = …\n",
    ));

    let document = parse(&export_json(&log));
    let entry = &document["entries"][0];

    assert_eq!(entry["code"], code);
    assert_eq!(entry["before_text"], "# Network: beam\n");
    assert_eq!(entry["after_text"], "# Network: beam\na = …\n");
    assert_eq!(entry["network_name"], "beam");
}

/// Both verdicts, separately (D8). A single `success` would answer "is the
/// network valid right now" rather than "did this edit work", and the two
/// diverge exactly when the log is most useful — mid-repair.
#[test]
fn json_keeps_the_two_verdicts_apart() {
    let mut log = AiEditLog::new();
    let mut record = applied("main", "m1 = map { }", "v0", "v1");
    record.success = false; // applied cleanly, network invalid elsewhere
    log.push(record);

    let entry = &parse(&export_json(&log))["entries"][0];
    assert_eq!(entry["applied"], true);
    assert_eq!(entry["success"], false);
}

/// The three `EditResult` fields it is easy to drop on the floor (D8):
/// `output_set` is how "the AI re-pointed the network's return node" is visible
/// at all.
#[test]
fn json_carries_the_description_summary_and_output_assignments() {
    let mut log = AiEditLog::new();
    let mut record = applied("main", "", "v0", "v1");
    record.description_set = Some("A cantilever beam".to_string());
    record.summary_set = Some("beam".to_string());
    record.output_set = Some("u1".to_string());
    record.nodes_created = vec!["m1/tip".to_string()];
    record.nodes_updated = vec!["u1".to_string()];
    record.nodes_deleted = vec!["old".to_string()];
    record.connections_made = vec!["a.out -> u1.shapes".to_string()];
    record.errors = vec!["no such node: q".to_string()];
    record.warnings = vec!["unused node: z".to_string()];
    log.push(record);

    let entry = &parse(&export_json(&log))["entries"][0];
    assert_eq!(entry["description_set"], "A cantilever beam");
    assert_eq!(entry["summary_set"], "beam");
    assert_eq!(entry["output_set"], "u1");
    assert_eq!(entry["nodes_created"][0], "m1/tip");
    assert_eq!(entry["nodes_updated"][0], "u1");
    assert_eq!(entry["nodes_deleted"][0], "old");
    assert_eq!(entry["connections_made"][0], "a.out -> u1.shapes");
    assert_eq!(entry["errors"][0], "no such node: q");
    assert_eq!(entry["warnings"][0], "unused node: z");
}

/// The layout outcome is the field that cannot be reconstructed from anything
/// else — neither the text diff nor the `EditResult` mentions positions (D9).
/// Moved nodes are keyed by **path**, so `m1/d` survives the round trip as one
/// string rather than collapsing into a bare `d`.
#[test]
fn json_carries_the_layout_outcome_with_path_keyed_moves() {
    let mut log = AiEditLog::new();
    let mut record = applied("main", "", "v0", "v1");
    record.layout = LayoutOutcome {
        path: LayoutPath::FullReflow,
        node_count: 12,
        moved: vec![MovedNode {
            path: vec!["m1".to_string(), "d".to_string()],
            before: DVec2::new(10.0, 20.0),
            after: DVec2::new(10.0, 50.0),
        }],
        max_displacement: 30.0,
        delta: None,
    };
    log.push(record);

    let layout = &parse(&export_json(&log))["entries"][0]["layout"];
    assert_eq!(layout["path"], "full_reflow");
    assert_eq!(layout["node_count"], 12);
    assert_eq!(layout["max_displacement"], 30.0);
    assert_eq!(layout["moved"][0]["path"], "m1/d");
    assert_eq!(layout["moved"][0]["before"][1], 20.0);
    assert_eq!(layout["moved"][0]["after"][1], 50.0);
    assert_eq!(layout["moved"][0]["displacement"], 30.0);
}

/// `LayoutPath` is exported snake-cased, not `Debug`-formatted: a reader
/// outside Rust should not be parsing a Rust type name.
#[test]
fn the_layout_path_spelling_is_stable() {
    for (path, expected) in [
        (LayoutPath::None, "none"),
        (LayoutPath::FullReflow, "full_reflow"),
        (LayoutPath::Incremental, "incremental"),
    ] {
        let mut log = AiEditLog::new();
        let mut record = applied("main", "", "v0", "v1");
        record.layout.path = path;
        log.push(record);

        assert_eq!(
            parse(&export_json(&log))["entries"][0]["layout"]["path"],
            expected
        );
    }
}

/// A rejected edit is in the log (D1) and must be in the export too — it is the
/// kind of AI-facing feedback the log exists to surface.
#[test]
fn json_includes_rejected_edits() {
    let mut log = AiEditLog::new();
    log.push(AiEditRecord::rejected(
        String::new(),
        "a = sphere { radius: 5.0 }".to_string(),
        true,
        vec!["network is locked against CLI writes".to_string()],
    ));

    let entry = &parse(&export_json(&log))["entries"][0];
    assert_eq!(entry["applied"], false);
    assert_eq!(entry["replace"], true);
    assert_eq!(entry["network_name"], "");
    assert_eq!(entry["before_text"], "");
    assert_eq!(entry["errors"][0], "network is locked against CLI writes");
}

#[test]
fn json_flags_an_incomplete_snapshot() {
    let mut log = AiEditLog::new();
    log.push(applied(
        "main",
        "",
        "a = sphere { radius: 5.0 }\n# Error: Cycle detected in the node network\n",
        "v1",
    ));

    let entry = &parse(&export_json(&log))["entries"][0];
    assert_eq!(entry["before_complete"], false);
    assert_eq!(entry["after_complete"], true);
}

#[test]
fn json_entries_are_in_log_order() {
    let mut log = AiEditLog::new();
    log.push(applied("main", "first", "v0", "v1"));
    log.push(applied("main", "second", "v1", "v2"));
    log.push(applied("main", "third", "v2", "v3"));

    let document = parse(&export_json(&log));
    let seqs: Vec<u64> = document["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["seq"].as_u64().unwrap())
        .collect();
    assert_eq!(seqs, vec![0, 1, 2]);
    assert_eq!(document["entry_count"], 3);
}

// ============================================================================
// Markdown — the readable form
// ============================================================================

#[test]
fn markdown_names_the_session_and_the_entries() {
    let mut log = AiEditLog::new();
    log.set_session_label("Opus 5 / skill v3".to_string());
    log.push(applied("beam", "a = sphere { radius: 5.0 }", "v0", "v1"));

    let text = export_markdown(&log);
    assert!(text.starts_with("# AI edit history"));
    assert!(text.contains("**Session:** Opus 5 / skill v3"));
    assert!(text.contains("**Entries:** 1"));
    assert!(text.contains("## #0 — beam"));
    assert!(text.contains("a = sphere { radius: 5.0 }"));
}

/// The bargain the Markdown form makes: readable in a chat message, therefore
/// without two whole network serializations per entry. The JSON form is the one
/// that keeps everything.
#[test]
fn markdown_omits_the_snapshots() {
    let mut log = AiEditLog::new();
    log.push(applied(
        "beam",
        "a = sphere { radius: 5.0 }",
        "SNAPSHOT-BEFORE-MARKER",
        "SNAPSHOT-AFTER-MARKER",
    ));

    let text = export_markdown(&log);
    assert!(!text.contains("SNAPSHOT-BEFORE-MARKER"));
    assert!(!text.contains("SNAPSHOT-AFTER-MARKER"));
}

#[test]
fn markdown_states_both_verdicts_and_the_replace_mode() {
    let mut log = AiEditLog::new();
    let mut record = AiEditRecord::applied_edit(
        "beam".to_string(),
        "m1 = map { }".to_string(),
        true,
        true,
        "v0".to_string(),
        "v1".to_string(),
        LayoutOutcome::default(),
    );
    record.success = false;
    log.push(record);

    let text = export_markdown(&log);
    assert!(text.contains("(replace)"));
    assert!(text.contains("applied: true | validates: false"));
}

#[test]
fn markdown_reports_divergence_errors_and_warnings() {
    let mut log = AiEditLog::new();
    log.push(applied("main", "", "v0", "v1"));
    let mut record = applied("main", "", "v1-plus-a-gui-edit", "v2");
    record.errors = vec!["no such node: q".to_string()];
    record.warnings = vec!["unused node: z".to_string()];
    log.push(record);

    let text = export_markdown(&log);
    assert!(text.contains("diverged: the network changed outside `edit`"));
    assert!(text.contains("- error: no such node: q"));
    assert!(text.contains("- warning: unused node: z"));
}

#[test]
fn markdown_names_a_rejected_edit_without_a_network() {
    let mut log = AiEditLog::new();
    log.push(AiEditRecord::rejected(
        String::new(),
        "a = sphere { radius: 5.0 }".to_string(),
        false,
        vec!["no active network".to_string()],
    ));

    let text = export_markdown(&log);
    assert!(text.contains("(no active network)"));
    assert!(text.contains("- error: no active network"));
}

#[test]
fn markdown_flags_an_incomplete_snapshot() {
    let mut log = AiEditLog::new();
    log.push(applied(
        "main",
        "",
        "# Error: Cycle detected in the node network\n",
        "v1",
    ));

    assert!(export_markdown(&log).contains("**incomplete snapshot**"));
}
