//! Phase 3 of `doc/design_background_node_jobs.md` — the node-job FFI surface.
//!
//! Routing, deferral and outcome classification are `DocumentSet`'s and are
//! tested in the crate (`node_jobs_test.rs`); the `#[frb(sync)]` wrappers touch
//! `CAD_INSTANCE` and stay untested. What is tested here is what crosses the
//! boundary: a `JobPoll` converts with every field intact, and the CLI's `run`
//! — now generic — says what it said when it was `chemisorb`'s alone.

use super::chemisorb_api_test::network;
use atomcad_structure_designer::chemisorb_ops::{ChemisorbRunSummary, format_run_result};
use atomcad_structure_designer::document_set::DocumentId;
use atomcad_structure_designer::node_jobs::{
    JobOutcome, JobOutcomeKind, JobPoll, JobStatus, JobTarget,
};
use atomcad_util::job_control::JobProgress;
use rust_lib_flutter_cad::api::structure_designer::node_jobs_api::{
    resolve_node_identifier, run_node_job_named,
};
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::{
    APINodeJobOutcomeKind, APINodeJobPoll,
};

fn target(document: u64, network: &str, scope_path: Vec<u64>, node_id: u64) -> JobTarget {
    JobTarget {
        document_id: DocumentId(document),
        network_name: network.to_string(),
        scope_path,
        node_id,
    }
}

fn outcome(job_id: u64, kind: JobOutcomeKind, message: &str) -> JobOutcome {
    JobOutcome {
        job_id,
        target: target(3, "other", vec![], 40 + job_id),
        label: "Chemisorption search".to_string(),
        kind,
        message: message.to_string(),
    }
}

#[test]
fn a_poll_converts_with_every_field() {
    let poll = JobPoll {
        running: vec![
            JobStatus {
                job_id: 7,
                target: target(2, "main", vec![], 5),
                label: "Chemisorption search".to_string(),
                progress: JobProgress {
                    done: 0,
                    total: None,
                    phase: "Planning".to_string(),
                },
                cancelling: false,
            },
            JobStatus {
                job_id: 8,
                target: target(4, "sub", vec![11, 12], 6),
                label: "Other job".to_string(),
                progress: JobProgress {
                    done: 37,
                    total: Some(121),
                    phase: "Relaxing".to_string(),
                },
                cancelling: true,
            },
        ],
        finished: vec![
            outcome(1, JobOutcomeKind::Finished, "Relaxed 2 hypotheses"),
            outcome(2, JobOutcomeKind::Cancelled, "search cancelled"),
            outcome(3, JobOutcomeKind::Failed, "unknown tag"),
            outcome(4, JobOutcomeKind::Dropped, "the node no longer exists"),
        ],
        pending_installs: 2,
        active_changed: true,
    };

    let api = APINodeJobPoll::from(poll);
    assert_eq!(api.pending_installs, 2);
    assert!(api.active_changed);

    assert_eq!(api.running.len(), 2);
    let s = &api.running[0];
    assert_eq!(
        (s.job_id, s.document_id, s.network_name.as_str(), s.node_id),
        (7, 2, "main", 5)
    );
    assert!(s.scope_path.is_empty());
    assert_eq!(s.label, "Chemisorption search");
    assert_eq!((s.phase.as_str(), s.done, s.total), ("Planning", 0, None));
    assert!(!s.cancelling);
    let s = &api.running[1];
    assert_eq!(
        (s.job_id, s.document_id, s.network_name.as_str(), s.node_id),
        (8, 4, "sub", 6)
    );
    assert_eq!(s.scope_path, vec![11, 12]);
    assert_eq!(s.label, "Other job");
    assert_eq!(
        (s.phase.as_str(), s.done, s.total),
        ("Relaxing", 37, Some(121))
    );
    assert!(s.cancelling);

    let kinds: Vec<_> = api.finished.iter().map(|o| o.kind).collect();
    assert_eq!(
        kinds,
        vec![
            APINodeJobOutcomeKind::Finished,
            APINodeJobOutcomeKind::Cancelled,
            APINodeJobOutcomeKind::Failed,
            APINodeJobOutcomeKind::Dropped,
        ]
    );
    let o = &api.finished[3];
    assert_eq!(
        (o.job_id, o.document_id, o.network_name.as_str(), o.node_id),
        (4, 3, "other", 44)
    );
    assert_eq!(o.label, "Chemisorption search");
    assert_eq!(o.message, "the node no longer exists");
    assert_eq!(api.finished[0].message, "Relaxed 2 hypotheses");
}

#[test]
fn an_empty_poll_converts_to_an_idle_one() {
    let api = APINodeJobPoll::from(JobPoll::default());
    assert!(api.running.is_empty() && api.finished.is_empty());
    assert_eq!(api.pending_installs, 0);
    assert!(!api.active_changed);
}

/// The `Relaxed … in <t> s;` timing differs between two runs.
fn without_seconds(text: &str) -> String {
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    let (head, tail) = first.split_once(" in ").expect("'in <t> s'");
    let tail = tail.split_once(" s;").expect("'<t> s;'").1;
    format!("{head} in _ s;{tail}\n{rest}")
}

#[test]
fn run_by_name_gives_the_text_run_chemisorb_node_gave() {
    let (mut designer, node) = network();
    designer.rename_node(&[], node, "mount").expect("rename");
    assert_eq!(resolve_node_identifier(&designer, "mount"), Some(node));
    assert_eq!(
        resolve_node_identifier(&designer, &node.to_string()),
        Some(node)
    );
    assert_eq!(resolve_node_identifier(&designer, "nope"), None);

    // What `run_chemisorb_node` printed: the typed Run, formatted.
    let (mut reference, reference_node) = network();
    let typed = reference.run_chemisorb(&[], reference_node).unwrap();
    let expected = format_run_result(&typed);

    let text = run_node_job_named(&mut designer, "mount").unwrap();
    assert_eq!(without_seconds(&text), without_seconds(&expected));
    assert!(text.starts_with("Relaxed 2 hypotheses in "), "{text}");
    assert!(text.contains("2 listed"), "{text}");
    assert!(text.contains("formed 1× O–Si"), "{text}");
    assert!(!text.contains("NOT exhaustive"), "{text}");

    // By id too.
    let text = run_node_job_named(&mut designer, &node.to_string()).unwrap();
    assert_eq!(without_seconds(&text), without_seconds(&expected));

    let truncated = ChemisorbRunSummary {
        truncated: true,
        best_strain: None,
        ..typed
    };
    let text = format_run_result(&truncated);
    assert!(text.contains("No candidate listed."), "{text}");
    assert!(text.contains("NOT exhaustive"), "{text}");
}

#[test]
fn run_by_name_fails_on_an_unknown_name_and_on_a_node_without_a_job() {
    let (mut designer, _) = network();
    assert_eq!(
        run_node_job_named(&mut designer, "nope"),
        Err("Node not found: nope".to_string())
    );
    // Node 1 is the adsorbate's `value` node: it has no run action.
    assert_eq!(
        run_node_job_named(&mut designer, "1"),
        Err("Node 1 has no run action".to_string())
    );
}
