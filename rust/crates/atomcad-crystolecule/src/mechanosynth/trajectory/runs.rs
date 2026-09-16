//! The **run** structure of a build script: which `tip` steps a tool performs
//! without going home in between.
//!
//! A tool's run is a maximal sequence of its own `tip` steps in which every step
//! between two consecutive ones is `spontaneous`. A `bulk` step or another
//! tool's `tip` step ends a run. Within a run the tool never returns to park: it
//! ascends after each reaction, flies to the next visit's standoff, waits there
//! through whatever settles the crystal does, and descends when its next step
//! comes.
//!
//! **Script and library only** — no scene, no coordinates. Run membership is a
//! function of the sequence, so it is recomputed on every evaluation and a
//! reordered script joins whatever run it now sits in, which is exactly why
//! nothing about it is stored on a step.
//!
//! One consequence is load-bearing: **at most one tool is away from park at any
//! step**, because a second tool's run cannot begin inside the first's — its
//! `tip` step would end that run. So the sweep that cleared a standoff saw every
//! other tool at park, which is where they are.
//!
//! Design doc: `doc/design_mechanosynth_trajectory.md` §A tool leaves park once
//! per run.

use crate::mechanosynth::schema::{BuildScript, Method, OpLibrary};

/// What a script's steps do to the tools, step by step.
///
/// Indices throughout are **0-based step indices** into `script.steps`, the way
/// `steps[k]` is step number `k + 1` in every message.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Runs {
    /// The tool type of a `tip` step, `None` for every other kind.
    tool_type: Vec<Option<String>>,
    /// The previous `tip` step of the same run, for a `tip` step that is not its
    /// run's first.
    previous: Vec<Option<usize>>,
    /// The next `tip` step of the same run, for a `tip` step that is not its
    /// run's last.
    next: Vec<Option<usize>>,
    /// For a `spontaneous` step that a run spans, the two visits it lies
    /// between.
    spans: Vec<Option<(usize, usize)>>,
}

impl Runs {
    /// The tool type of step `k`, if it is a `tip` step of a known operation.
    pub fn tool_type(&self, k: usize) -> Option<&str> {
        self.tool_type.get(k).and_then(Option::as_deref)
    }

    /// The previous visit of the run step `k` belongs to. `None` when `k` opens
    /// its run — the tool flies in from park.
    pub fn previous_visit(&self, k: usize) -> Option<usize> {
        self.previous.get(k).copied().flatten()
    }

    /// The next visit of the run step `k` belongs to. `None` when `k` closes its
    /// run — the tool goes home.
    pub fn next_visit(&self, k: usize) -> Option<usize> {
        self.next.get(k).copied().flatten()
    }

    /// The two visits a `spontaneous` step lies between, when one run spans it.
    /// `None` for a settle outside any run, which nothing hovers over.
    pub fn spanned_by(&self, k: usize) -> Option<(usize, usize)> {
        self.spans.get(k).copied().flatten()
    }

    /// The first visit of the run step `k` belongs to — `k` itself when it opens
    /// one.
    pub fn run_start(&self, k: usize) -> usize {
        let mut first = k;
        while let Some(previous) = self.previous_visit(first) {
            first = previous;
        }
        first
    }
}

/// The run structure of `script` against `library`.
///
/// A step naming an operation the library does not have ends whatever run is
/// open: the engine cannot say which tool it would use, and
/// `validate_script_ops` is what reports the missing name.
pub fn runs(script: &BuildScript, library: &OpLibrary) -> Runs {
    let count = script.steps.len();
    let mut runs = Runs {
        tool_type: vec![None; count],
        previous: vec![None; count],
        next: vec![None; count],
        spans: vec![None; count],
    };

    // The last visit a run could still continue from, and the settles seen since
    // it — which become the hover steps the moment the run does continue.
    let mut open: Option<(usize, String)> = None;
    let mut settles: Vec<usize> = Vec::new();

    for (k, step) in script.steps.iter().enumerate() {
        let op = library.get(&step.op);
        let method = op.map(|op| op.method);
        match method {
            Some(Method::Spontaneous) => {
                if open.is_some() {
                    settles.push(k);
                }
            }
            Some(Method::Tip) => {
                let tool_type = op
                    .and_then(|op| op.tool.as_ref())
                    .map(|side| side.tool_type.clone());
                runs.tool_type[k] = tool_type.clone();

                if let (Some((previous, previous_type)), Some(tool_type)) = (&open, &tool_type)
                    && previous_type == tool_type
                {
                    let previous = *previous;
                    runs.previous[k] = Some(previous);
                    runs.next[previous] = Some(k);
                    for settle in &settles {
                        runs.spans[*settle] = Some((previous, k));
                    }
                }
                open = tool_type.map(|tool_type| (k, tool_type));
                settles.clear();
            }
            // A bulk step is an exposure the instrument retracts from, and an
            // unknown operation says nothing about which tool it uses; both end
            // the run.
            _ => {
                open = None;
                settles.clear();
            }
        }
    }

    runs
}
