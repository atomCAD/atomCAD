//! Node jobs: an explicit, expensive action on one node, run off the UI thread
//! (`doc/design_background_node_jobs.md`).
//!
//! A job has three halves, and only the middle one leaves the UI thread:
//!
//! 1. **prepare** — `NodeData::prepare_job`, on the UI thread inside an
//!    ordinary API call: guards, evaluates the node's inputs through
//!    [`JobInputs`], and returns a [`JobWork`] that owns everything it needs;
//! 2. **run** — [`JobWork::run`], on the [`JobRunner`]'s own pool: pure
//!    computation on owned data, reporting into a `JobControl`;
//! 3. **install** — [`JobResult::install`], back on the UI thread during a
//!    poll (`DocumentSet::poll_jobs`): writes the result into the target node.
//!
//! **The worker never sees the designer** (D2), so nothing is shared with the
//! UI thread but the job's control block and its result slot, and no lock
//! around `CAD_INSTANCE` is needed. `JobWork: Send + 'static` is what the
//! compiler checks this by: the work cannot borrow a node, only own copies.
//!
//! **Install is unconditionally safe because the result is fingerprint-keyed**
//! (D3). Whatever was edited while the job ran, installing can only *miss*:
//! the node outputs a stored result only while its current inputs hash to the
//! fingerprint the result was computed from. A node that cannot fingerprint
//! its inputs must not have a job.
//!
//! Job state — running, progress, outcome — is session state owned by
//! `DocumentSet` (D8) and is never node data (D10): not saved, not undoable.

pub mod designer_ops;
pub mod document_ops;
pub mod inputs;
pub mod runner;

pub use inputs::JobInputs;
pub use runner::{FinishedJob, FinishedResult, JobRunner, default_job_threads};

use crate::document_set::DocumentId;
use crate::node_data::NodeData;
use atomcad_util::job_control::{JobControl, JobProgress};

/// The refusal of a second job on a node that has one (D7). Generic, like the
/// layer.
pub const JOB_ALREADY_RUNNING: &str = "A job is already running on this node";

/// Where a job's result goes. Identity is by document + network + scope + id;
/// a target that no longer resolves (or resolves to another node type) drops
/// the result. Thanks to D3 a wrong-but-same-type node can only miss.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobTarget {
    pub document_id: DocumentId,
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
}

/// The worker half: owned inputs, no access to the designer.
pub trait JobWork: Send + 'static {
    /// What the job is called in the UI ("Chemisorption search").
    fn label(&self) -> String;

    /// Does the work. `control` is `None` on the blocking path (D9); the work
    /// reports progress into it and stops early, with an `Err`, once it is
    /// cancelled.
    fn run(self: Box<Self>, control: Option<&JobControl>) -> Result<Box<dyn JobResult>, String>;
}

/// The UI-thread half: writes the result into the target node.
pub trait JobResult: Send + 'static {
    /// `data` is the target node's data; downcast it. Returns the summary a
    /// caller reports (one line per fact — the CLI prints it, the GUI shows
    /// it in a snackbar). `Err` when the node is not of the type the result
    /// belongs to.
    fn install(self: Box<Self>, data: &mut dyn NodeData) -> Result<String, String>;
}

/// A job whose worker has not returned yet, cancelling ones included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobStatus {
    pub job_id: u64,
    pub target: JobTarget,
    pub label: String,
    /// `done` / `total` (`None` = indeterminate) / `phase`.
    pub progress: JobProgress,
    /// Cancel was asked for; the worker has not returned yet.
    pub cancelling: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobOutcomeKind {
    /// The result was installed into its node.
    Finished,
    /// The job was cancelled and returned without a result.
    Cancelled,
    /// The work returned an error, or panicked.
    Failed,
    /// There was a result, or would have been, but nowhere to put it: the
    /// document was closed, the node is gone or of another type, the network
    /// became read-only.
    Dropped,
}

/// How a job ended, reported by exactly one poll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobOutcome {
    pub job_id: u64,
    pub target: JobTarget,
    pub label: String,
    pub kind: JobOutcomeKind,
    /// The install summary, the error, or why the result was dropped.
    pub message: String,
}

/// What one `DocumentSet::poll_jobs` saw and did.
#[derive(Debug, Default)]
pub struct JobPoll {
    pub running: Vec<JobStatus>,
    pub finished: Vec<JobOutcome>,
    /// Successfully finished jobs held back by an open interaction (D11).
    /// Non-zero keeps the caller polling, so a deferred result is not
    /// stranded.
    pub pending_installs: usize,
    /// An install touched the active designer; the caller refreshes once.
    pub active_changed: bool,
}
