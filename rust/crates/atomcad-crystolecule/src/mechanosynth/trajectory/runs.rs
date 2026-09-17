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

/// The steps a **playback** stops on, 0-based, ascending.
///
/// Playing a build one step at a time gives every step the same dwell, and most
/// steps of a real script do not deserve one: a `spontaneous` settle holds its
/// tool perfectly still while the crystal relaxes, so a run that should read as
/// one movement — descend, react, lift, fly, descend — is chopped up by seconds
/// of stillness. This is the list that fixes it, and the rule has two
/// consequences from one sentence:
///
/// > A `tip` step is playable. A maximal block of consecutive non-`tip` steps
/// > is **one** playable step — its last — if the block contains a `bulk` step,
/// > and **no** playable step at all if the block is nothing but settles.
///
/// So a settle between two visits disappears from the playback, and a phase of
/// `bulk` exposures with their settles between them plays as a single beat
/// landing on its last step. Nothing is *dropped*: a player that jumps to step
/// `m` is asking for the first `m − 1` steps applied, so the scene it lands on
/// is exactly the one the skipped steps produced.
///
/// **Skipping a settle is pose-continuous**, which is what makes this a pacing
/// decision and not an animation one. `path.rs` poses a `spontaneous` step that
/// a run spans as a static `ToolMotion::Hover` at the *next* visit's standoff —
/// the pose the previous visit's flight ends on at `time 1.0`, and the one the
/// next visit begins from at `time 0.0`. The tool does not move across the
/// jump; only the workpiece does, at once, which is what a relaxation looks
/// like when you are not watching it. A `bulk` step parks every tool, so its
/// block is continuous for the trivial reason.
///
/// **Only what can be proved skippable is skipped.** The method is the
/// *operation's*, so a step naming an operation the library does not define has
/// no known method: it stays playable **and** ends whatever block is open,
/// exactly as it ends a run in [`runs`]. A script played against an empty or
/// unwired library is therefore every step, never none — playback degrades to
/// the un-skipped walk rather than silently dropping steps someone asked to
/// see.
///
/// The list can be shorter than the script and can be empty (a script of
/// nothing but settles). Neither is a failure: what covers the tail is the
/// player's own rule that a run ends on the script's last step whether or not
/// it is playable.
///
/// Design doc: `doc/design_mechanosynth_trajectory.md` §What playing skips.
pub fn playable_steps(script: &BuildScript, library: &OpLibrary) -> Vec<usize> {
    let method_at = |k: usize| {
        script
            .steps
            .get(k)
            .and_then(|step| library.get(&step.op))
            .map(|op| op.method)
    };

    let count = script.steps.len();
    let mut playable = Vec::new();
    let mut k = 0;
    while k < count {
        match method_at(k) {
            // A visit, or a step whose kind cannot be established. Both stop the
            // playback on themselves.
            Some(Method::Tip) | None => {
                playable.push(k);
                k += 1;
            }
            // A block of gating steps. It is walked to its end first, because
            // whether it is worth a beat at all is a property of the block
            // rather than of any step in it.
            Some(_) => {
                let mut has_bulk = false;
                while let Some(method @ (Method::Bulk | Method::Spontaneous)) = method_at(k) {
                    has_bulk |= method == Method::Bulk;
                    k += 1;
                }
                if has_bulk {
                    playable.push(k - 1);
                }
            }
        }
    }
    playable
}
