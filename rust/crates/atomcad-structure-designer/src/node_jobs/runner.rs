//! [`JobRunner`]: the running jobs, their control blocks and result slots,
//! and the thread pool they run on.

use super::{JOB_ALREADY_RUNNING, JobResult, JobStatus, JobTarget, JobWork};
use crate::document_set::DocumentId;
use atomcad_util::job_control::JobControl;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, OnceLock};

/// The job pool size the application uses (D4): every core but one, so the
/// UI thread keeps one, and at least one.
pub fn default_job_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .saturating_sub(1)
        .max(1)
}

/// How a job's worker ended. Decided by the runner, never by parsing a
/// message.
pub enum FinishedResult {
    /// A result to install. An `Ok` that races a cancel is still a valid
    /// result and lands here.
    Ok(Box<dyn JobResult>),
    /// The work returned an error while its control was cancelled — whatever
    /// the text says.
    Cancelled(String),
    /// The work returned an error without a cancel, or panicked
    /// (`"internal error: …"`).
    Failed(String),
}

/// A job taken out of the runner by [`JobRunner::take_finished`].
pub struct FinishedJob {
    pub id: u64,
    pub target: JobTarget,
    pub label: String,
    pub result: FinishedResult,
}

enum JobSlot {
    Running,
    Finished(FinishedResult),
}

struct JobEntry {
    id: u64,
    target: JobTarget,
    label: String,
    control: Arc<JobControl>,
    slot: Arc<Mutex<JobSlot>>,
}

impl JobEntry {
    fn slot(&self) -> std::sync::MutexGuard<'_, JobSlot> {
        self.slot.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The session's jobs (owned by `DocumentSet`, D8) and the pool they run on.
///
/// The runner **owns its pool** rather than sharing a process-wide static: a
/// test builds a runner with a small private pool and cannot interfere with
/// another test's jobs. The pool is built on the first [`start`](Self::start),
/// so a runner that never starts a job spawns no threads.
///
/// A job's entry stays until its worker has returned *and* the entry is taken
/// — a cancelled job included — so a second job on the same target is refused
/// until then, and two runs on one node never overlap (D7).
pub struct JobRunner {
    threads: usize,
    pool: OnceLock<rayon::ThreadPool>,
    next_id: u64,
    jobs: Vec<JobEntry>,
}

impl JobRunner {
    /// `threads` for the runner's own pool; the application passes
    /// [`default_job_threads`], tests 1 or 2.
    pub fn new(threads: usize) -> Self {
        Self {
            threads: threads.max(1),
            pool: OnceLock::new(),
            next_id: 1,
            jobs: Vec::new(),
        }
    }

    /// Whether the pool has been built (it is, on the first `start`).
    pub fn pool_built(&self) -> bool {
        self.pool.get().is_some()
    }

    /// Starts `work` on the runner's pool and returns the job's id. Refuses a
    /// second job on the same target (D7). A `par_iter` inside the work runs
    /// on the same pool (D4). A panic inside the work is caught and reported
    /// as a failure.
    pub fn start(&mut self, target: JobTarget, work: Box<dyn JobWork>) -> Result<u64, String> {
        if self.jobs.iter().any(|j| j.target == target) {
            return Err(JOB_ALREADY_RUNNING.to_string());
        }
        let id = self.next_id;
        self.next_id += 1;
        let control = Arc::new(JobControl::new());
        let slot = Arc::new(Mutex::new(JobSlot::Running));
        self.jobs.push(JobEntry {
            id,
            target,
            label: work.label(),
            control: control.clone(),
            slot: slot.clone(),
        });

        let threads = self.threads;
        let pool = self.pool.get_or_init(|| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .thread_name(|i| format!("atomcad-job-{i}"))
                .build()
                .expect("build the node-job thread pool")
        });
        pool.spawn(move || {
            let finished = match catch_unwind(AssertUnwindSafe(|| work.run(Some(&control)))) {
                Ok(Ok(result)) => FinishedResult::Ok(result),
                Ok(Err(message)) if control.is_cancelled() => FinishedResult::Cancelled(message),
                Ok(Err(message)) => FinishedResult::Failed(message),
                Err(panic) => FinishedResult::Failed(format!(
                    "internal error: {}",
                    panic_message(panic.as_ref())
                )),
            };
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = JobSlot::Finished(finished);
        });
        Ok(id)
    }

    /// Asks job `id` to stop. Its entry stays until its worker returns.
    pub fn cancel(&self, id: u64) {
        if let Some(job) = self.jobs.iter().find(|j| j.id == id) {
            job.control.cancel();
        }
    }

    /// Cancels every job of `document_id` (D8: a close, an in-place load).
    pub fn cancel_document(&self, document_id: DocumentId) {
        for job in self
            .jobs
            .iter()
            .filter(|j| j.target.document_id == document_id)
        {
            job.control.cancel();
        }
    }

    /// Jobs whose worker has not returned yet, cancelling ones included.
    pub fn statuses(&self) -> Vec<JobStatus> {
        self.jobs
            .iter()
            .filter(|j| matches!(*j.slot(), JobSlot::Running))
            .map(|j| JobStatus {
                job_id: j.id,
                target: j.target.clone(),
                label: j.label.clone(),
                progress: j.control.snapshot(),
                cancelling: j.control.is_cancelled(),
            })
            .collect()
    }

    /// Successfully finished jobs still waiting in their slot (D11).
    pub fn pending_installs(&self) -> usize {
        self.jobs
            .iter()
            .filter(|j| matches!(*j.slot(), JobSlot::Finished(FinishedResult::Ok(_))))
            .count()
    }

    /// Removes and returns the finished jobs, except successful ones whose
    /// target is `deferred` (D11), which stay in place for a later call.
    pub fn take_finished(&mut self, deferred: impl Fn(&JobTarget) -> bool) -> Vec<FinishedJob> {
        let mut taken = Vec::new();
        let mut kept = Vec::with_capacity(self.jobs.len());
        for job in self.jobs.drain(..) {
            let result = {
                let mut slot = job.slot();
                match &*slot {
                    JobSlot::Running => None,
                    JobSlot::Finished(FinishedResult::Ok(_)) if deferred(&job.target) => None,
                    JobSlot::Finished(_) => match std::mem::replace(&mut *slot, JobSlot::Running) {
                        JobSlot::Finished(result) => Some(result),
                        JobSlot::Running => unreachable!("matched as finished"),
                    },
                }
            };
            match result {
                Some(result) => taken.push(FinishedJob {
                    id: job.id,
                    target: job.target,
                    label: job.label,
                    result,
                }),
                None => kept.push(job),
            }
        }
        self.jobs = kept;
        taken
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "the job panicked".to_string()
    }
}
