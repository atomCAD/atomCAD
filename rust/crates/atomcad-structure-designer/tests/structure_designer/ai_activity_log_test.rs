//! The activity half of the session log — Phase 5 of
//! `doc/design_ai_edit_history.md`.
//!
//! Phase 5 widened the log from "every AI *edit*" to "every CLI request", so
//! that the timeline shows what the AI **looked at** before it edited. Three
//! properties carry that, and each is easy to break without noticing:
//!
//! - the two rings **share one sequence counter**, which is the only thing that
//!   makes an interleaved timeline exactly ordered (two requests routinely land
//!   in the same millisecond);
//! - an activity entry takes **no divergence flags** — it holds no snapshot to
//!   compare and a `query` between two edits is not an outside change, so the
//!   D7 detection must still read edit records only;
//! - it is **lightweight by construction**: its own generous cap, no snapshots,
//!   and bounded strings.
//!
//! The recording *hook* is in Dart (`lib/ai_assistant/http_server.dart`), which
//! is the only layer that knows a request happened; what is testable here is
//! everything it hands over.

use atomcad_structure_designer::ai_edit_log::{
    AI_ACTIVITY_LOG_MAX_ENTRIES, AiActivityRecord, AiEditLog, AiEditRecord, LayoutOutcome,
};

// ============================================================================
// Helpers
// ============================================================================

/// An edit record that "reached the network", with the two snapshots given.
fn edit(network: &str, before: &str, after: &str) -> AiEditRecord {
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

fn get(path: &str) -> AiActivityRecord {
    AiActivityRecord::new(
        "GET".to_string(),
        path.to_string(),
        String::new(),
        String::new(),
        200,
        3,
    )
}

// ============================================================================
// One timeline
// ============================================================================

/// The property the whole merged view rests on. A timestamp-ordered merge would
/// tie constantly — a scripted session issues several requests per millisecond
/// — and a tie renders "the AI queried, then edited" in either order.
#[test]
fn edits_and_activity_share_one_sequence_counter() {
    let mut log = AiEditLog::new();
    log.push(edit("main", "v0", "v1"));
    log.push_activity(get("/query"));
    log.push_activity(get("/screenshot"));
    log.push(edit("main", "v1", "v2"));

    let edit_seqs: Vec<u64> = log.records().map(|record| record.seq).collect();
    let activity_seqs: Vec<u64> = log.activity().map(|record| record.seq).collect();

    assert_eq!(edit_seqs, vec![0, 3]);
    assert_eq!(activity_seqs, vec![1, 2]);
}

/// Sequence numbers stay unique across the two rings, which is what lets the
/// panel key a selection and an export name an entry unambiguously.
#[test]
fn no_sequence_number_is_used_twice() {
    let mut log = AiEditLog::new();
    for _ in 0..5 {
        log.push(edit("main", "v0", "v0"));
        log.push_activity(get("/query"));
    }

    let mut seqs: Vec<u64> = log
        .records()
        .map(|record| record.seq)
        .chain(log.activity().map(|record| record.seq))
        .collect();
    let count = seqs.len();
    seqs.sort_unstable();
    seqs.dedup();
    assert_eq!(seqs.len(), count);
}

/// D7's divergence comparison reads edit records only. A `query` between two
/// edits changes nothing, and flagging it would turn the marker — whose job is
/// "the human, or something else, touched this network" — into noise on every
/// session that reads before it writes.
#[test]
fn activity_between_two_edits_does_not_diverge_them() {
    let mut log = AiEditLog::new();
    log.push(edit("main", "v0", "v1"));
    log.push_activity(get("/query"));
    log.push_activity(get("/evaluate"));
    log.push(edit("main", "v1", "v2"));

    let diverged: Vec<bool> = log.records().map(|record| record.diverged).collect();
    assert_eq!(diverged, vec![false, false]);
}

/// …and it does not *mask* one either: the comparison still reaches past the
/// activity entries to the previous edit's `after_text`.
#[test]
fn activity_does_not_mask_a_real_divergence() {
    let mut log = AiEditLog::new();
    log.push(edit("main", "v0", "v1"));
    log.push_activity(get("/query"));
    // Something changed the network between the two edits — a GUI edit, a load.
    log.push(edit("main", "edited by hand", "v2"));

    let diverged: Vec<bool> = log.records().map(|record| record.diverged).collect();
    assert_eq!(diverged, vec![false, true]);
}

// ============================================================================
// Caps and bounds
// ============================================================================

#[test]
fn the_activity_ring_evicts_oldest_first() {
    let mut log = AiEditLog::new();
    for _ in 0..(AI_ACTIVITY_LOG_MAX_ENTRIES + 10) {
        log.push_activity(get("/query"));
    }

    assert_eq!(log.activity_len(), AI_ACTIVITY_LOG_MAX_ENTRIES);
    assert_eq!(log.activity().next().unwrap().seq, 10);
}

/// Nothing stops a caller putting a whole script in a query parameter, and a
/// "lightweight" entry that can grow without bound is not lightweight.
#[test]
fn an_overlong_query_is_truncated_with_an_ellipsis() {
    let record = AiActivityRecord::new(
        "GET".to_string(),
        "/evaluate".to_string(),
        "x".repeat(5_000),
        String::new(),
        200,
        1,
    );

    assert!(record.query.chars().count() < 600);
    assert!(record.query.ends_with('…'));
}

/// Truncation counts **characters**, not bytes: a byte slice through a network
/// name or a file path with an accented character would panic.
#[test]
fn truncation_does_not_split_a_multibyte_character() {
    let record = AiActivityRecord::new(
        "POST".to_string(),
        "/load".to_string(),
        String::new(),
        "é".repeat(5_000),
        200,
        1,
    );

    assert!(record.detail.ends_with('…'));
    assert!(record.detail.chars().all(|c| c == 'é' || c == '…'));
}

// ============================================================================
// The record itself
// ============================================================================

#[test]
fn the_request_line_includes_a_query_only_when_there_is_one() {
    assert_eq!(get("/query").request_line(), "GET /query");

    let with_query = AiActivityRecord::new(
        "GET".to_string(),
        "/nodes".to_string(),
        "category=Geometry3D".to_string(),
        String::new(),
        200,
        1,
    );
    assert_eq!(with_query.request_line(), "GET /nodes?category=Geometry3D");
}

#[test]
fn ok_covers_the_success_and_redirect_ranges() {
    let status = |code: u16| {
        AiActivityRecord::new(
            "GET".to_string(),
            "/query".to_string(),
            String::new(),
            String::new(),
            code,
            1,
        )
        .ok()
    };

    assert!(status(200));
    assert!(status(304));
    assert!(!status(400));
    assert!(!status(500));
}

// ============================================================================
// The client label (open question 5)
// ============================================================================

/// The label is announced by the transport once per request and stamped by the
/// log, because an edit record is built deep inside the domain crate, which
/// knows nothing about HTTP headers.
#[test]
fn the_client_label_is_stamped_on_both_kinds_of_record() {
    let mut log = AiEditLog::new();
    log.set_client_label("Opus 5 / skill v3".to_string());
    log.push(edit("main", "v0", "v1"));
    log.push_activity(get("/query"));

    assert_eq!(
        log.records().next().unwrap().client_label,
        "Opus 5 / skill v3"
    );
    assert_eq!(
        log.activity().next().unwrap().client_label,
        "Opus 5 / skill v3"
    );
}

/// A record keeps the label that was current when it was pushed, so a session
/// in which two models edited in turn stays readable.
#[test]
fn a_record_keeps_the_label_current_when_it_was_pushed() {
    let mut log = AiEditLog::new();
    log.set_client_label("model A".to_string());
    log.push(edit("main", "v0", "v1"));
    log.set_client_label("model B".to_string());
    log.push(edit("main", "v1", "v2"));

    let labels: Vec<&str> = log
        .records()
        .map(|record| record.client_label.as_str())
        .collect();
    assert_eq!(labels, vec!["model A", "model B"]);
}

#[test]
fn client_labels_are_distinct_and_in_first_seen_order() {
    let mut log = AiEditLog::new();
    log.set_client_label("model A".to_string());
    log.push(edit("main", "v0", "v1"));
    log.push_activity(get("/query"));
    log.set_client_label("model B".to_string());
    log.push_activity(get("/query"));
    log.set_client_label("model A".to_string());
    log.push(edit("main", "v1", "v2"));

    assert_eq!(log.client_labels(), vec!["model A", "model B"]);
}

#[test]
fn an_unidentified_caller_contributes_no_label() {
    let mut log = AiEditLog::new();
    log.push(edit("main", "v0", "v1"));
    log.push_activity(get("/query"));

    assert!(log.client_labels().is_empty());
    assert!(log.records().next().unwrap().client_label.is_empty());
}

// ============================================================================
// Version and clear
// ============================================================================

/// The panel's refresh gate is a `u64` compare (D12), so an activity entry has
/// to bump it or the timeline would only update when an edit happened to
/// follow.
#[test]
fn pushing_activity_bumps_the_version() {
    let mut log = AiEditLog::new();
    let before = log.version();
    log.push_activity(get("/query"));

    assert_ne!(log.version(), before);
}

/// …but announcing a client label does not. It is invisible until a record
/// carries it, and bumping per request would make the refresh gate fire on a
/// header.
#[test]
fn setting_the_client_label_does_not_bump_the_version() {
    let mut log = AiEditLog::new();
    let before = log.version();
    log.set_client_label("Opus 5".to_string());

    assert_eq!(log.version(), before);
    assert_eq!(log.client_label(), "Opus 5");
}

#[test]
fn clear_drops_the_activity_too() {
    let mut log = AiEditLog::new();
    log.push(edit("main", "v0", "v1"));
    log.push_activity(get("/query"));
    log.clear();

    assert!(log.is_empty());
    assert_eq!(log.activity_len(), 0);
    assert!(log.client_labels().is_empty());
}

/// `seq` keeps counting across a clear — for both rings, since they share the
/// counter — so an export taken after one is still unambiguous.
#[test]
fn clear_does_not_reset_the_shared_sequence() {
    let mut log = AiEditLog::new();
    log.push_activity(get("/query"));
    log.push(edit("main", "v0", "v1"));
    log.clear();
    log.push_activity(get("/query"));

    assert_eq!(log.activity().next().unwrap().seq, 2);
}
