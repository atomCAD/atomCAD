//! Node jobs, Phase 2 (`doc/design_background_node_jobs.md`): the runner and
//! the session side, driven by [`GatedWork`] — no chemistry, no sleeps. The
//! real `chemisorb` job is tested in `chemisorb_node_test.rs`.

use super::node_jobs_support::*;
use atomcad_structure_designer::document_set::{DocumentId, DocumentSet};
use atomcad_structure_designer::node_jobs::{
    FinishedResult, JOB_ALREADY_RUNNING, JobOutcome, JobOutcomeKind, JobPoll, JobResult, JobRunner,
    JobTarget, JobWork,
};
use atomcad_structure_designer::nodes::int::IntData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::structure_designer_changes::RefreshMode;
use atomcad_util::job_control::{JobControl, JobProgress};
use glam::f64::DVec2;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

// ---------------------------------------------------------------------------
// JobRunner
// ---------------------------------------------------------------------------

fn target(node_id: u64) -> JobTarget {
    JobTarget {
        document_id: DocumentId(1),
        network_name: "main".to_string(),
        scope_path: Vec::new(),
        node_id,
    }
}

/// Waits until no job of `runner` is running.
fn wait_idle(runner: &JobRunner) {
    wait_until("every worker has returned", || runner.statuses().is_empty());
}

#[test]
fn a_status_mirrors_the_control_while_the_job_runs() {
    let mut runner = JobRunner::new(1);
    let (gate, work) = gated(0);
    let id = runner
        .start(target(1), Box::new(work.with_progress("Relaxing", 37, 121)))
        .unwrap();
    let expected = JobProgress {
        done: 37,
        total: Some(121),
        phase: "Relaxing".to_string(),
    };
    wait_until("the progress is reported", || {
        runner.statuses().first().map(|s| &s.progress) == Some(&expected)
    });
    let status = &runner.statuses()[0];
    assert_eq!(status.job_id, id);
    assert_eq!(status.target, target(1));
    assert_eq!(status.label, GATED_LABEL);
    assert!(!status.cancelling);

    gate.release(Release::Ok);
    wait_idle(&runner);
}

#[test]
fn one_job_per_target_until_the_cancelled_one_has_returned_and_been_taken() {
    let mut runner = JobRunner::new(2);
    // Ignoring the cancel keeps the worker blocked until the test releases
    // it, so the "cancelling" state cannot be missed.
    let (gate, work) = gated(0);
    let first = runner
        .start(target(1), Box::new(work.ignoring_cancel()))
        .unwrap();

    let (_g, again) = gated(0);
    assert_eq!(
        runner.start(target(1), Box::new(again)).unwrap_err(),
        JOB_ALREADY_RUNNING
    );
    let (other_gate, other) = gated(0);
    runner
        .start(target(2), Box::new(other))
        .expect("another target is accepted");

    runner.cancel(first);
    let status = runner
        .statuses()
        .into_iter()
        .find(|s| s.job_id == first)
        .expect("a cancelled job is listed until its worker returns");
    assert!(status.cancelling);
    let (_g, again) = gated(0);
    assert!(runner.start(target(1), Box::new(again)).is_err());

    gate.release(Release::Err("cancelled".to_string()));
    wait_until("the cancelled worker returns", || {
        runner.statuses().iter().all(|s| s.job_id != first)
    });
    let (_g, again) = gated(0);
    assert!(
        runner.start(target(1), Box::new(again)).is_err(),
        "still refused until the entry is taken"
    );
    let taken = runner.take_finished(|_| false);
    assert_eq!(taken.len(), 1);
    assert!(matches!(&taken[0].result, FinishedResult::Cancelled(m) if m == "cancelled"));

    let (again_gate, again) = gated(0);
    runner
        .start(target(1), Box::new(again))
        .expect("the target is free again");

    other_gate.release(Release::Ok);
    again_gate.release(Release::Ok);
    wait_idle(&runner);
}

#[test]
fn take_finished_honours_the_deferral_and_pending_installs_counts_it() {
    let mut runner = JobRunner::new(2);
    let (g1, w1) = gated(0);
    let (g2, w2) = gated(0);
    runner.start(target(1), Box::new(w1)).unwrap();
    runner.start(target(2), Box::new(w2)).unwrap();
    g1.release(Release::Ok);
    g2.release(Release::Err("broken".to_string()));
    wait_idle(&runner);

    assert_eq!(runner.pending_installs(), 1, "only the success counts");
    let taken = runner.take_finished(|t| t.node_id == 1);
    assert_eq!(taken.len(), 1, "a failure is never deferred");
    assert!(matches!(&taken[0].result, FinishedResult::Failed(m) if m == "broken"));
    assert_eq!(runner.pending_installs(), 1, "the deferred success stays");

    let taken = runner.take_finished(|_| false);
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].target, target(1));
    assert!(matches!(taken[0].result, FinishedResult::Ok(_)));
    assert_eq!(runner.pending_installs(), 0);
    assert!(runner.take_finished(|_| false).is_empty());
}

#[test]
fn a_panicking_job_fails_and_the_runner_keeps_working() {
    let mut runner = JobRunner::new(1);
    let (gate, work) = gated(0);
    runner.start(target(1), Box::new(work)).unwrap();
    gate.release(Release::Panic);
    wait_idle(&runner);
    let taken = runner.take_finished(|_| false);
    match &taken[0].result {
        FinishedResult::Failed(m) => {
            assert!(m.starts_with("internal error"), "{m}");
            assert!(m.contains("on purpose"), "{m}");
        }
        _ => panic!("a panic is a failure"),
    }

    let (gate, work) = gated(0);
    runner.start(target(1), Box::new(work)).unwrap();
    gate.release(Release::Ok);
    wait_idle(&runner);
    assert!(matches!(
        runner.take_finished(|_| false)[0].result,
        FinishedResult::Ok(_)
    ));
}

#[test]
fn the_pool_is_built_on_the_first_start() {
    let mut runner = JobRunner::new(1);
    assert!(!runner.pool_built());
    let (gate, work) = gated(0);
    runner.start(target(1), Box::new(work)).unwrap();
    assert!(runner.pool_built());
    gate.release(Release::Ok);
    wait_idle(&runner);
}

/// Records the rayon pool size the work runs in.
struct ThreadCount(Arc<AtomicUsize>);

impl JobWork for ThreadCount {
    fn label(&self) -> String {
        "count".to_string()
    }

    fn run(self: Box<Self>, _control: Option<&JobControl>) -> Result<Box<dyn JobResult>, String> {
        self.0.store(rayon::current_num_threads(), Ordering::SeqCst);
        Err("done".to_string())
    }
}

/// D4: a `par_iter` inside a job uses the job pool, not the global one.
#[test]
fn code_inside_a_job_sees_the_job_pool() {
    let mut runner = JobRunner::new(2);
    let seen = Arc::new(AtomicUsize::new(0));
    runner
        .start(target(1), Box::new(ThreadCount(seen.clone())))
        .unwrap();
    wait_idle(&runner);
    assert_eq!(seen.load(Ordering::SeqCst), 2);
}

// ---------------------------------------------------------------------------
// DocumentSet::poll_jobs: routing, deferral, outcome kinds
// ---------------------------------------------------------------------------

/// A document whose networks `names` each hold one `int` node, all with the
/// same id; the first network is active.
fn designer_with_ints(names: &[&str]) -> (StructureDesigner, u64) {
    let mut d = StructureDesigner::new();
    let mut id = None;
    for name in names {
        d.add_node_network(name);
        d.set_active_node_network_name(Some(name.to_string()));
        let node = d.add_node("int", DVec2::ZERO);
        assert_eq!(*id.get_or_insert(node), node, "same id in every network");
    }
    d.set_active_node_network_name(Some(names[0].to_string()));
    (d, id.unwrap())
}

fn refresh(d: &mut StructureDesigner) {
    let changes = d.get_pending_changes();
    d.refresh(&changes);
}

fn int_value(d: &StructureDesigner, network: &str, node: u64) -> i32 {
    d.node_type_registry.node_networks[network].nodes[&node]
        .data
        .as_any_ref()
        .downcast_ref::<IntData>()
        .unwrap()
        .value
}

/// A `DocumentSet` with a small job pool, and the active slot.
struct Session {
    set: DocumentSet,
    active: StructureDesigner,
}

impl Session {
    fn new(active: StructureDesigner) -> Self {
        let mut active = active;
        let mut set = DocumentSet::new(&mut active);
        set.jobs = JobRunner::new(2);
        Session { set, active }
    }

    fn target(&self, document_id: DocumentId, network: &str, node_id: u64) -> JobTarget {
        JobTarget {
            document_id,
            network_name: network.to_string(),
            scope_path: Vec::new(),
            node_id,
        }
    }

    fn start(&mut self, target: JobTarget, work: GatedWork) -> u64 {
        self.set.jobs.start(target, Box::new(work)).unwrap()
    }

    fn poll(&mut self, defer: bool) -> JobPoll {
        self.set.poll_jobs(&mut self.active, defer)
    }

    fn wait_idle(&self) {
        wait_idle(&self.set.jobs);
    }

    fn outcome(&mut self) -> JobOutcome {
        let poll = self.poll(false);
        assert_eq!(poll.finished.len(), 1, "{:?}", poll.finished);
        poll.finished.into_iter().next().unwrap()
    }
}

#[test]
fn an_install_into_the_active_document_asks_for_a_refresh_and_one_into_a_parked_one_does_not() {
    let (d1, node) = designer_with_ints(&["main"]);
    let mut s = Session::new(d1);
    let first = s.set.active_id();
    let (d2, node2) = designer_with_ints(&["main"]);
    let second = s.set.insert(d2, first);
    assert_eq!(node, node2);
    refresh(s.set.parked_mut(second).unwrap());

    let (g_active, w_active) = gated(11);
    let (g_parked, w_parked) = gated(22);
    s.start(s.target(first, "main", node), w_active);
    s.start(s.target(second, "main", node), w_parked);
    g_parked.release(Release::Ok);
    s.wait_until_pending(1);

    let poll = s.poll(false);
    assert_eq!(poll.finished.len(), 1);
    assert_eq!(poll.finished[0].kind, JobOutcomeKind::Finished);
    assert_eq!(poll.finished[0].message, GATED_SUMMARY);
    assert!(
        !poll.active_changed,
        "a parked install needs no refresh now"
    );
    assert_eq!(int_value(s.set.parked(second).unwrap(), "main", node), 22);
    assert_eq!(int_value(&s.active, "main", node), 0);

    g_active.release(Release::Ok);
    s.wait_idle();
    let poll = s.poll(false);
    assert!(poll.active_changed);
    assert_eq!(int_value(&s.active, "main", node), 11);

    // Activating the parked document evaluates what was installed: the swap
    // marks a full refresh (its install alone marked only the node).
    assert!(s.set.parked(second).unwrap().get_pending_changes().mode != RefreshMode::Full);
    s.set.activate(&mut s.active, second).unwrap();
    assert_eq!(s.active.get_pending_changes().mode, RefreshMode::Full);
    assert_eq!(int_value(&s.active, "main", node), 22);
}

impl Session {
    fn wait_until_pending(&self, n: usize) {
        wait_until("a result waits in its slot", || {
            self.set.jobs.pending_installs() == n
        });
    }
}

#[test]
fn closing_a_document_cancels_its_job_and_the_late_outcome_is_dropped() {
    for returns_ok in [false, true] {
        let (d1, node) = designer_with_ints(&["main"]);
        let mut s = Session::new(d1);
        let first = s.set.active_id();
        let (d2, _) = designer_with_ints(&["main"]);
        let second = s.set.insert(d2, first);

        let (gate, work) = gated(5);
        s.start(s.target(second, "main", node), work.ignoring_cancel());
        s.set.close(&mut s.active, second).unwrap();
        let status = &s.poll(false).running[0];
        assert!(status.cancelling, "closing cancels (drop_parked)");

        gate.release(if returns_ok {
            Release::Ok
        } else {
            Release::Err("cancelled".to_string())
        });
        s.wait_idle();
        let outcome = s.outcome();
        assert_eq!(outcome.kind, JobOutcomeKind::Dropped);
        assert_eq!(outcome.message, "the document was closed");
        assert_eq!(gate.installs(), 0);
    }
}

#[test]
fn closing_the_active_document_cancels_its_job() {
    let (d1, node) = designer_with_ints(&["main"]);
    let mut s = Session::new(d1);
    let first = s.set.active_id();
    let (d2, _) = designer_with_ints(&["main"]);
    s.set.insert(d2, first);
    let (_gate, work) = gated(5);
    s.start(s.target(first, "main", node), work);
    s.set.close(&mut s.active, first).unwrap();
    s.wait_idle();
    let outcome = s.outcome();
    assert_eq!(outcome.kind, JobOutcomeKind::Dropped);
    assert_eq!(outcome.message, "the document was closed");
}

#[test]
fn an_in_place_replacement_cancels_the_job() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("saved.cnnd");
    for load in [false, true] {
        let (d1, node) = designer_with_ints(&["main"]);
        let mut s = Session::new(d1);
        s.active
            .save_node_networks_as(path.to_str().unwrap())
            .unwrap();
        let first = s.set.active_id();
        let (_gate, work) = gated(5);
        s.start(s.target(first, "main", node), work);
        if load {
            s.set
                .load_in_place(&mut s.active, path.to_str().unwrap())
                .unwrap();
        } else {
            s.set.new_project_in_place(&mut s.active, true);
        }
        assert_ne!(s.set.active_id(), first);
        s.wait_idle();
        let outcome = s.outcome();
        assert_eq!(outcome.kind, JobOutcomeKind::Dropped, "load: {load}");
        assert_eq!(outcome.message, "the document was closed");
    }
}

#[test]
fn an_open_interaction_or_defer_installs_holds_a_success_but_not_a_failure() {
    let (d1, node) = designer_with_ints(&["main"]);
    let mut s = Session::new(d1);
    let doc = s.set.active_id();

    // Rust's side: a node drag begun through the existing API.
    let (gate, work) = gated(7);
    s.start(s.target(doc, "main", node), work);
    gate.release(Release::Ok);
    s.wait_until_pending(1);
    s.active.select_node(node);
    s.active.begin_move_nodes();
    assert!(s.active.open_interaction().is_some());
    let poll = s.poll(false);
    assert!(poll.finished.is_empty());
    assert_eq!(poll.pending_installs, 1);
    assert_eq!(int_value(&s.active, "main", node), 0);
    s.active.end_move_nodes();
    let outcome = s.outcome();
    assert_eq!(outcome.kind, JobOutcomeKind::Finished);
    assert_eq!(int_value(&s.active, "main", node), 7);

    // Flutter's side: `defer_installs`.
    let (gate, work) = gated(8);
    s.start(s.target(doc, "main", node), work);
    gate.release(Release::Ok);
    s.wait_until_pending(1);
    let poll = s.poll(true);
    assert!(poll.finished.is_empty());
    assert_eq!(poll.pending_installs, 1);
    assert_eq!(s.outcome().kind, JobOutcomeKind::Finished);
    assert_eq!(int_value(&s.active, "main", node), 8);

    // A failure has nothing to install and is reported at once.
    let (gate, work) = gated(9);
    s.start(s.target(doc, "main", node), work);
    gate.release(Release::Err("broken".to_string()));
    s.wait_idle();
    let poll = s.poll(true);
    assert_eq!(poll.finished.len(), 1);
    assert_eq!(poll.finished[0].kind, JobOutcomeKind::Failed);
    assert_eq!(poll.finished[0].message, "broken");
    assert_eq!(poll.pending_installs, 0);
}

#[test]
fn outcome_kinds_are_decided_by_the_cancel_flag_not_the_message() {
    let (d1, node) = designer_with_ints(&["main"]);
    let mut s = Session::new(d1);
    let doc = s.set.active_id();

    // `Err` after a cancel → Cancelled, whatever the text.
    let (gate, work) = gated(1);
    let id = s.start(s.target(doc, "main", node), work.ignoring_cancel());
    s.set.cancel_job(id);
    gate.release(Release::Err("something else".to_string()));
    s.wait_idle();
    let outcome = s.outcome();
    assert_eq!(outcome.kind, JobOutcomeKind::Cancelled);
    assert_eq!(outcome.message, "something else");
    assert_eq!(outcome.job_id, id);

    // `Err` without a cancel → Failed, even if it says "cancelled".
    let (gate, work) = gated(2);
    s.start(s.target(doc, "main", node), work);
    gate.release(Release::Err("cancelled".to_string()));
    s.wait_idle();
    assert_eq!(s.outcome().kind, JobOutcomeKind::Failed);

    // `Ok` after a cancel is a valid result and installs.
    let (gate, work) = gated(3);
    let id = s.start(s.target(doc, "main", node), work.ignoring_cancel());
    s.set.cancel_job(id);
    gate.release(Release::Ok);
    s.wait_idle();
    assert_eq!(s.outcome().kind, JobOutcomeKind::Finished);
    assert_eq!(int_value(&s.active, "main", node), 3);
}

#[test]
fn every_outcome_is_delivered_by_exactly_one_poll() {
    let (d1, node) = designer_with_ints(&["main"]);
    let mut s = Session::new(d1);
    let doc = s.set.active_id();
    let (gate, work) = gated(1);
    s.start(s.target(doc, "main", node), work);
    let (gate2, work2) = gated(1);
    s.start(s.target(doc, "main", node + 1000), work2);
    gate.release(Release::Ok);
    gate2.release(Release::Ok);
    s.wait_idle();

    let first = s.poll(false);
    assert_eq!(first.finished.len(), 2);
    let kinds: Vec<_> = first.finished.iter().map(|o| o.kind).collect();
    assert!(
        kinds.contains(&JobOutcomeKind::Dropped),
        "no node {}",
        node + 1000
    );
    let dropped = first
        .finished
        .iter()
        .find(|o| o.kind == JobOutcomeKind::Dropped)
        .unwrap();
    assert_eq!(dropped.message, "the node no longer exists");
    assert!(kinds.contains(&JobOutcomeKind::Finished));
    let second = s.poll(false);
    assert!(second.finished.is_empty() && second.running.is_empty());
    assert_eq!(gate.installs(), 1);
}

#[test]
fn jobs_on_the_same_node_id_in_two_networks_route_to_their_own_nodes() {
    let (d1, node) = designer_with_ints(&["a", "b"]);
    let mut s = Session::new(d1);
    refresh(&mut s.active);
    let doc = s.set.active_id();
    let (ga, wa) = gated(100);
    let (gb, wb) = gated(200);
    s.start(s.target(doc, "a", node), wa);
    s.start(s.target(doc, "b", node), wb.with_progress("Relaxing", 1, 2));

    let running = s.poll(false).running;
    assert_eq!(
        running.len(),
        2,
        "same id, different networks: both accepted"
    );
    let networks: Vec<_> = running
        .iter()
        .map(|r| r.target.network_name.as_str())
        .collect();
    assert_eq!(networks, ["a", "b"]);

    ga.release(Release::Ok);
    gb.release(Release::Ok);
    s.wait_idle();
    let poll = s.poll(false);
    assert_eq!(poll.finished.len(), 2);
    assert!(poll.active_changed);
    assert_eq!(int_value(&s.active, "a", node), 100);
    assert_eq!(int_value(&s.active, "b", node), 200);
    // The install into "b", which is not the network shown, asks for a full
    // refresh: it may be used in "a" as a custom node.
    assert_eq!(s.active.get_pending_changes().mode, RefreshMode::Full);
}
