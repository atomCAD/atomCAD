//! The session side of node jobs: `DocumentSet` owns the jobs (D8), starts
//! them on the active document, and routes finished ones back to their
//! document — active or parked — when polled.
//!
//! The cancellation hooks of D8 are not here but in `document_set.rs`, inside
//! the two functions every close and every in-place replacement goes through
//! (`drop_parked`, `renumber_active`), so a new close path cannot forget them.

use super::{FinishedResult, JobOutcome, JobOutcomeKind, JobPoll};
use crate::document_set::{DocumentId, DocumentSet};
use crate::structure_designer::StructureDesigner;
use std::collections::HashSet;

/// The reason a job's result goes nowhere when its document is gone.
pub const DOCUMENT_CLOSED: &str = "the document was closed";

impl DocumentSet {
    /// `prepare_node_job` on the active designer, then starts the work. The
    /// target carries the active document's id.
    pub fn start_job(
        &mut self,
        active: &mut StructureDesigner,
        scope_path: &[u64],
        node_id: u64,
    ) -> Result<u64, String> {
        let (target, work) = active.prepare_node_job(scope_path, node_id)?;
        debug_assert_eq!(target.document_id, self.active_id());
        self.jobs.start(target, work)
    }

    pub fn cancel_job(&self, job_id: u64) {
        self.jobs.cancel(job_id);
    }

    /// The install site. Takes the finished jobs and reports each exactly
    /// once:
    ///
    /// - a successful job is **deferred** — left in its slot for the next
    ///   poll — while `defer_installs` is set (Flutter's half of D11) or its
    ///   document reports an open interaction (Rust's half);
    /// - a job whose document is no longer open is `Dropped` ("the document
    ///   was closed"), whatever it returned;
    /// - otherwise a cancelled or failed job is reported as such, and a
    ///   successful one is installed into its designer: `Finished` with the
    ///   summary, or `Dropped` with the reason the install refused.
    ///
    /// Does not refresh: `active_changed` tells the caller to. A result
    /// installed into a parked designer is evaluated by the full refresh its
    /// activation performs.
    pub fn poll_jobs(&mut self, active: &mut StructureDesigner, defer_installs: bool) -> JobPoll {
        let active_id = self.active_id();
        let busy: HashSet<DocumentId> = self
            .order()
            .iter()
            .copied()
            .filter(|id| {
                let designer = if *id == active_id {
                    Some(&*active)
                } else {
                    self.parked(*id)
                };
                designer.is_some_and(|d| d.open_interaction().is_some())
            })
            .collect();
        let finished = self
            .jobs
            .take_finished(|t| defer_installs || busy.contains(&t.document_id));

        let mut poll = JobPoll::default();
        for job in finished {
            let id = job.target.document_id;
            let open = id == active_id || self.parked(id).is_some();
            let (kind, message) = if !open {
                (JobOutcomeKind::Dropped, DOCUMENT_CLOSED.to_string())
            } else {
                match job.result {
                    FinishedResult::Cancelled(message) => (JobOutcomeKind::Cancelled, message),
                    FinishedResult::Failed(message) => (JobOutcomeKind::Failed, message),
                    FinishedResult::Ok(result) => {
                        let designer = if id == active_id {
                            &mut *active
                        } else {
                            self.parked_mut(id).expect("an open parked document")
                        };
                        match designer.install_job_result(&job.target, result) {
                            Ok(summary) => {
                                poll.active_changed |= id == active_id;
                                (JobOutcomeKind::Finished, summary)
                            }
                            Err(reason) => (JobOutcomeKind::Dropped, reason),
                        }
                    }
                }
            };
            poll.finished.push(JobOutcome {
                job_id: job.id,
                target: job.target,
                label: job.label,
                kind,
                message,
            });
        }
        poll.running = self.jobs.statuses();
        poll.pending_installs = self.jobs.pending_installs();
        poll
    }
}
