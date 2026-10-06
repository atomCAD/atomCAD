//! Test doubles for node jobs (`doc/design_background_node_jobs.md`, "Testing
//! strategy"): the test, not the scheduler, decides when a job finishes.
//!
//! [`GatedWork`] reports the progress it is given, then blocks until the test
//! releases it through its [`Gate`] with an outcome — `Ok`, `Err` or a panic —
//! returning `Err("cancelled")` early if its control is cancelled while it
//! waits (unless built with [`GatedWork::ignoring_cancel`]). Its result,
//! [`GatedResult`], installs into any node: it counts the install and, on an
//! `int` node, writes its mark into the value, so a test can see *which* node
//! it landed on.
//!
//! Every wait is bounded ([`wait_until`]), so a broken runner fails the test
//! instead of hanging the suite.

use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_jobs::{JobResult, JobWork};
use atomcad_structure_designer::nodes::int::IntData;
use atomcad_util::job_control::{JobControl, JobProgress};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

pub const GATED_LABEL: &str = "Gated job";
pub const GATED_SUMMARY: &str = "gated summary";

/// How the test releases a [`GatedWork`].
pub enum Release {
    Ok,
    Err(String),
    Panic,
}

/// The test's end of a [`GatedWork`].
pub struct Gate {
    tx: Sender<Release>,
    installs: Arc<AtomicUsize>,
}

impl Gate {
    pub fn release(&self, outcome: Release) {
        // The worker may have returned already (cancelled); that is fine.
        let _ = self.tx.send(outcome);
    }

    /// How many times this job's result was installed.
    pub fn installs(&self) -> usize {
        self.installs.load(Ordering::SeqCst)
    }
}

pub struct GatedWork {
    rx: Receiver<Release>,
    progress: Option<JobProgress>,
    honour_cancel: bool,
    mark: i32,
    installs: Arc<AtomicUsize>,
}

/// A gated job and its gate. `mark` is what its result writes into an `int`
/// node.
pub fn gated(mark: i32) -> (Gate, GatedWork) {
    let (tx, rx) = mpsc::channel();
    let installs = Arc::new(AtomicUsize::new(0));
    (
        Gate {
            tx,
            installs: installs.clone(),
        },
        GatedWork {
            rx,
            progress: None,
            honour_cancel: true,
            mark,
            installs,
        },
    )
}

impl GatedWork {
    /// Reports this progress on its control before it blocks.
    pub fn with_progress(mut self, phase: &str, done: u64, total: u64) -> Self {
        self.progress = Some(JobProgress {
            done,
            total: Some(total),
            phase: phase.to_string(),
        });
        self
    }

    /// Keeps waiting for the gate after a cancel, so a test can return `Ok`
    /// from a cancelled job.
    pub fn ignoring_cancel(mut self) -> Self {
        self.honour_cancel = false;
        self
    }
}

impl JobWork for GatedWork {
    fn label(&self) -> String {
        GATED_LABEL.to_string()
    }

    fn run(self: Box<Self>, control: Option<&JobControl>) -> Result<Box<dyn JobResult>, String> {
        if let (Some(control), Some(p)) = (control, &self.progress) {
            control.set_phase(&p.phase);
            control.set_total(p.total.unwrap_or(0));
            control.advance(p.done);
        }
        loop {
            match self.rx.recv_timeout(Duration::from_millis(2)) {
                Ok(Release::Ok) => {
                    return Ok(Box::new(GatedResult {
                        mark: self.mark,
                        installs: self.installs.clone(),
                    }));
                }
                Ok(Release::Err(message)) => return Err(message),
                Ok(Release::Panic) => panic!("gated work panicked on purpose"),
                Err(RecvTimeoutError::Timeout) => {
                    if self.honour_cancel && control.is_some_and(JobControl::is_cancelled) {
                        return Err("cancelled".to_string());
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return Err("gate dropped".to_string()),
            }
        }
    }
}

pub struct GatedResult {
    mark: i32,
    installs: Arc<AtomicUsize>,
}

impl JobResult for GatedResult {
    fn install(self: Box<Self>, data: &mut dyn NodeData) -> Result<String, String> {
        if let Some(int) = data.as_any_mut().downcast_mut::<IntData>() {
            int.value = self.mark;
        }
        self.installs.fetch_add(1, Ordering::SeqCst);
        Ok(GATED_SUMMARY.to_string())
    }
}

/// Polls `cond` until it holds, failing with `what` after 10 s.
pub fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        std::thread::yield_now();
    }
}
