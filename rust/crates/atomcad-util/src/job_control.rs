//! Progress and cancellation for one long computation, shared between the
//! thread that runs it and the thread that watches it
//! (`doc/design_background_node_jobs.md`).
//!
//! It names nothing domain-specific, so the code that does the work (a
//! chemisorption search) can report into it without knowing who started it
//! or why.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Progress and cancellation for one long computation. Cheap to poll; every
/// field is an atomic, except the phase text.
///
/// All atomics use `Relaxed` ordering: the values are advisory. A result
/// computed under a control crosses threads by some other synchronized route
/// (a `Mutex`, a channel, a join), which carries the happens-before edge.
#[derive(Debug, Default)]
pub struct JobControl {
    cancel: AtomicBool,
    done: AtomicU64,
    /// 0 = unknown (indeterminate).
    total: AtomicU64,
    /// "Planning", "Relaxing", …
    phase: Mutex<String>,
}

/// A point-in-time copy of a [`JobControl`]'s progress.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobProgress {
    pub done: u64,
    /// `None` while the amount of work is not known yet.
    pub total: Option<u64>,
    pub phase: String,
}

impl JobControl {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks the computation to stop. Sticky: there is no un-cancel.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// The number of units the whole computation is; 0 means unknown.
    pub fn set_total(&self, total: u64) {
        self.total.store(total, Ordering::Relaxed);
    }

    /// `n` more units are done.
    pub fn advance(&self, n: u64) {
        self.done.fetch_add(n, Ordering::Relaxed);
    }

    pub fn set_phase(&self, phase: &str) {
        let mut current = self.phase.lock().unwrap_or_else(|e| e.into_inner());
        current.clear();
        current.push_str(phase);
    }

    pub fn snapshot(&self) -> JobProgress {
        let total = self.total.load(Ordering::Relaxed);
        JobProgress {
            done: self.done.load(Ordering::Relaxed),
            total: (total != 0).then_some(total),
            phase: self.phase.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        }
    }
}
