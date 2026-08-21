//! Opt-in per-node evaluation profiler — Phase 2 of
//! `doc/design_eval_profiling.md` (D1, D3, D4, D5).
//!
//! Where `refresh_profile` answers "which *phase* of the refresh was slow?",
//! this answers "which node, which node type, how many times, and how much of
//! that was its own work rather than its dependencies'".
//!
//! ## Why this is opt-in and the phase clock is not (D1)
//!
//! A phase boundary costs one `Instant::now()` per refresh; a node boundary
//! costs two clock reads and a hash-map update *per evaluation*. That is
//! microseconds at ~10³ evaluations per pass but not inside a `map` body over
//! 10⁵ elements, and a profiler that inflates the numbers it reports is worse
//! than useless. Switched off, the cost is one thread-local `bool` read
//! ([`ENABLED`]) per evaluation and **no guard at all**.
//!
//! ## Why the state is a thread-local and not a context field (D4/D6)
//!
//! Two independent reasons, either sufficient:
//!
//! - *Borrowck.* A guard holding `&mut NetworkEvaluationContext` for the
//!   duration of a frame would freeze the one thing the whole function body
//!   needs — `context` is threaded into every recursive `evaluate` call.
//! - *The eager-HOF context split.* `apply` / `fold` / `foreach` evaluate their
//!   bodies against a `fresh_inner_for_eager_body` context whose
//!   `drain_inner_context` merges **`print_buffer` and nothing else**. With a
//!   context-owned accumulator stack the body's `total` would never reach the
//!   HOF's child accumulator, so the HOF would be charged the entire body cost
//!   as *self* time — a wrong row, not a missing one — and the body's own
//!   records would vanish. A thread-local is per *pass*, which is the correct
//!   scope: it spans every context a pass constructs, and a new eager-body call
//!   site cannot drop what it never held.
//!
//! `StructureDesigner::with_eval_context` is the single owner of the lifetime:
//! it [`install`]s a profile at the start of a pass and [`take`]s it back at
//! the end, at the same seam as the existing `print_buffer` drain and with the
//! same "regardless of how the closure returned" discipline.
//!
//! ## Two attribution artifacts that are documented, not fixed (D4)
//!
//! - **Lazy iterators shift time to the consumer.** A `map` body node runs when
//!   `collect` pulls it, so its time nests under `collect` and `map`'s own
//!   total looks near-zero.
//! - **A custom-node instance has ~zero self time.** It delegates to its
//!   network's return node, so its *total* covers the subnetwork while its
//!   *self* is bookkeeping only. That is the useful reading, and the panel says
//!   so.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::evaluator::network_evaluator::{
    EvalEnvKey, NetworkStackElement, NodeProfileKey, eval_env_key,
};
use crate::evaluator::network_result::NetworkResult;

/// Where a profiled node lives, captured **once on vacant insert** — never on
/// the hot update path, which would clone a `Vec` per evaluation (D5).
///
/// The aggregation key and this struct answer different questions and both are
/// needed: a key cannot be displayed (it is a hash) and cannot be navigated to.
/// Conflating them is what D5 rejects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLocation {
    /// The network the node actually **lives in** — what click-to-jump
    /// activates. Read off the deepest registry-owned frame, so a node inside a
    /// custom network names that network rather than the caller's; the jump
    /// crosses the boundary the same way the error-navigation jump lands on a
    /// root cause in another network.
    pub host_network: String,
    /// The node's HOF-body chain **within `host_network`**. Together with
    /// `host_network` and `node_id` this is exactly the address the Find Usages
    /// / error-navigation jump already consumes, so click-to-jump needs no new
    /// navigation machinery.
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    /// Human-readable address: `"main/fold#12/add#3 (mysum)"`.
    pub label: String,
    /// The node's type name — the roll-up key of the "By node type" table.
    pub node_type_name: String,
    /// Whether `(host_network, scope_path, node_id)` is an address the canvas
    /// navigation can actually reach.
    ///
    /// True for everything with a home frame, custom-network internals
    /// included. It is false only for a body evaluated off a **body-only**
    /// stack (a lazy `map`/`filter` step) whose fallback scope path turns out
    /// to contain a custom-network instance hop — an id no `Node.zone` walk can
    /// follow. Such rows are still measured and still roll up by type; the
    /// panel just renders them non-clickable rather than offering a jump that
    /// lands nowhere.
    pub navigable: bool,
}

/// Why a node's `wasted_ns` is **not** an available saving: the memo declines
/// to cache some results on purpose (D10). Counted like everything else, but
/// flagged so a big number in the Redundancy tab is not read as free money.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordFlags {
    /// The node produced a `NetworkResult::Iterator` on at least one pin.
    /// `doc/design_eval_memoization.md` D4 excludes those from the memo for
    /// **memory** reasons, not correctness: a stored walker pins its
    /// `ZoneClosure` — possibly an `Arc<Vec<NetworkResult>>` over a large source
    /// array — for the whole pass, while memoizing it buys almost nothing (a
    /// `map`'s `eval` only *builds* the walker; the work is in `next()`).
    pub produced_iterator: bool,
    /// The re-entrancy backstop fired on this node — a wire cycle escaped
    /// validation and re-entered it. `doc/design_eval_memoization.md` D9 must
    /// never store a result produced under it: with `A -> B -> A` the inner and
    /// outer evaluations of `A` share a byte-identical environment and return
    /// different results, which is the one case where the key is genuinely
    /// insufficient.
    pub under_reentrancy_backstop: bool,
    /// The node is a **custom-network instance**, and at least one request for
    /// it went through `evaluate`'s single-pin custom-network arm.
    /// `doc/design_eval_memoization.md` D2 forbids inserting from that arm — it
    /// forwards one `output_pin_index` to the child's return node and never
    /// holds the complete `EvalOutput` the key promises — so such a row can
    /// legitimately show `evaluations == lookups` with the memo working
    /// perfectly. Unflagged, *every subnetwork instance in every design would
    /// read as a memo bug*. That the cost is usually small (the expensive work
    /// re-enters and hits at the child's return node) is why it is flagged
    /// rather than fixed.
    pub subnetwork: bool,
    /// A request for this node missed on a key the memo had held earlier in the
    /// pass: the LRU dropped the entry and the work was redone (D6/D10).
    /// Without this, memory pressure is indistinguishable from a correctness
    /// bug, and Phase 5's trigger has no signal to fire on.
    pub evicted: bool,
}

impl RecordFlags {
    /// Whether this row's `wasted_ns` overstates the achievable saving — the
    /// four reasons a row can legitimately re-evaluate in one environment.
    ///
    /// The first two are properties of the *result* (an iterator must not be
    /// stored; a result produced under the cycle backstop must not be), the
    /// last two of the *pass* (an arm that cannot insert; a budget that could
    /// not hold what it inserted).
    pub fn uncacheable(&self) -> bool {
        self.produced_iterator || self.under_reentrancy_backstop || self.subnetwork || self.evicted
    }
}

/// One row of the per-node table: an aggregate over every evaluation of one
/// `(home frame, node)` pair during one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeProfileRecord {
    pub location: NodeLocation,
    /// Times a result for this node was **requested** (Phase 3, D10).
    ///
    /// Before the memo lands this equals [`evaluations`](Self::evaluations)
    /// exactly — every request runs `eval`. Afterwards the difference *is* the
    /// memo's hit count, which is why the two are separate fields rather than
    /// one column that quietly changes meaning (D8b).
    pub lookups: u64,
    /// Times `NodeData::eval` actually ran for this node.
    pub evaluations: u64,
    /// How many distinct **evaluation environments** (`eval_env_key`) this node
    /// was requested in during the pass (D9/D10).
    ///
    /// This is the denominator that makes the redundancy factor honest: a `map`
    /// body node evaluated once per element over 3 elements runs in 3
    /// *different* environments and is not redundant at all, while a diamond's
    /// apex evaluated twice in one environment is redundant exactly once.
    pub distinct_envs: u64,
    /// Time in this node's own `eval`, with time spent evaluating its upstream
    /// dependencies subtracted (D4).
    pub self_ns: u64,
    /// Wall time of the whole evaluation including everything it pulled.
    pub total_ns: u64,
    /// Why this row's `wasted_ns` may not be collectable — see [`RecordFlags`].
    pub flags: RecordFlags,
}

impl NodeProfileRecord {
    /// **The actionable column** (D10): the self time a perfect memo would
    /// avoid, in nanoseconds.
    ///
    /// `self_ns * (lookups - distinct_envs) / evaluations`. The division by
    /// `evaluations` — not by `lookups` — is what keeps the number meaningful
    /// after the memo lands: `self_ns` accumulates over actual evaluations, so
    /// `self_ns / evaluations` is the mean cost of *computing* the node once,
    /// and `lookups - distinct_envs` is how many of those computations were
    /// avoidable.
    ///
    /// **Collapses to ~0 once the memo is working, and not for the reason the
    /// design first gave.** `lookups` was expected to hold steady because it
    /// measures demand — but demand is itself *generated* by re-evaluation: a
    /// consumer served from the memo never runs `eval`, so it never re-issues
    /// its own downstream pulls, and the collapse compounds down the cone. On
    /// the design's own measurement `materialize#8` went from 12 lookups to 1.
    ///
    /// So this column reads the redundancy that **remains**, not the saving
    /// that was realized. What the memo actually did is read off `MemoCounts`
    /// (hits over requests) and by comparing two history-ring rows, which is
    /// why `doc/design_eval_memoization.md` D10's `Wasted` → `Saved` relabel
    /// was rejected: it would have printed a zero next to the word "Saved".
    ///
    /// The acceptance criterion is stated over `evaluations == distinct_envs`
    /// — see [`EvalProfile::unmemoized_offender_count`].
    pub fn wasted_ns(&self) -> u64 {
        if self.evaluations == 0 {
            return 0;
        }
        let avoidable = self.lookups.saturating_sub(self.distinct_envs);
        ((self.self_ns as u128 * avoidable as u128) / self.evaluations as u128) as u64
    }

    /// Requests per distinct environment — the per-node redundancy factor.
    /// `1.0` means every request was a genuinely different environment.
    ///
    /// Reported **per node, never only globally** (D10): a pass that is
    /// globally 2.5x but 11x on `materialize` and 1.0 on body nodes is the
    /// realistic shape, and only the breakdown says where a memo would pay.
    pub fn redundancy_factor(&self) -> f64 {
        if self.distinct_envs == 0 {
            return 1.0;
        }
        self.lookups as f64 / self.distinct_envs as f64
    }
}

/// Which key the D11 self-check groups results under.
///
/// [`Full`](Self::Full) is the only mode any production path uses: it is the
/// real environment key, and the check asks whether equal keys really do imply
/// equal results. The weakened mode exists so the check itself can be shown to
/// **fail** on a key that is missing an input — "a check that can't fail proves
/// nothing" (D11) — and is reachable only from a test.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelfCheckKeyMode {
    /// The real `eval_env_key`.
    #[default]
    Full,
    /// The key with `decorate` dropped. `decorate` genuinely changes results
    /// (the selected node's own scene evaluation decorates atoms; every other
    /// consumer of it does not), so a pass in which a selected node also feeds
    /// another displayed node must report a violation under this mode — and
    /// none under `Full`. That pair is the check's regression test.
    OmitDecorate,
}

/// One equal-key-different-result finding from the D11 self-check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfCheckViolation {
    /// Label of the node that produced two different results under one
    /// environment key.
    pub label: String,
    /// Summary of the result recorded the first time the key was seen.
    pub first: String,
    /// Summary of the differing later result.
    pub later: String,
}

/// The live accumulator **and** the finished report — one type, no separate
/// `EvalProfiler`. While a pass runs it lives in [`PROFILE`]; when the pass
/// ends `with_eval_context` takes it out and it becomes an immutable snapshot
/// hanging off the pass's `RefreshProfile` row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvalProfile {
    /// Records in first-seen order. A `Vec` rather than the map's values
    /// because the RAII guard carries a `u32` index into it — a plain integer
    /// with no borrow, which is what keeps the guard cheap enough to add a
    /// frame to a recursion that already runs near the debug-build stack limit
    /// (D4).
    records: Vec<NodeProfileRecord>,
    /// Aggregation key → index into `records` (D5).
    index: HashMap<NodeProfileKey, u32>,
    /// Child-accumulator stack: one entry per live guard, holding the summed
    /// `total_ns` of the evaluations that completed beneath it. Must be empty
    /// at end of pass — a leaked frame silently corrupts every ancestor's self
    /// time, which is why release is by `Drop` and not by hand at each of the
    /// two hook functions' many early exits.
    child_acc: Vec<u64>,
    /// The pass's root network — the fallback host for a record whose stack
    /// has no registry-owned frame to read one from (see `build_location`).
    root_network_name: String,
    /// Per record, the set of environment keys it was requested in — the
    /// working state behind `NodeProfileRecord::distinct_envs`. Parallel to
    /// `records` rather than a field on it: the record type is the panel's row,
    /// and a set of hashes is not a column.
    record_envs: Vec<HashSet<EvalEnvKey>>,
    /// Every environment key the pass saw. Its size is the **would-be memo peak
    /// entry count** — one entry per `(environment, node, decorate)`, which is
    /// exactly what `doc/design_eval_memoization.md` D2 would store — so the
    /// memo's memory question is answered by measurement before a line of it is
    /// written.
    all_envs: HashSet<EvalEnvKey>,
    /// Set when environment tracking hit [`MAX_TRACKED_ENVS`] and stopped
    /// recording new keys. Surfaced in the panel rather than silently capping:
    /// a truncated pass under-reports `distinct_envs`, which *over*-reports
    /// redundancy, and a bound nobody is told about reads as coverage.
    envs_truncated: bool,
    /// D11 self-check state: environment key -> summary of the first result
    /// recorded under it. `None` unless the check is armed.
    self_check_results: Option<HashMap<EvalEnvKey, String>>,
    /// Findings of the D11 self-check. Empty is the expected outcome; a
    /// non-empty list means the environment key is missing an input — a wrong
    /// *number* here, and a wrong *result* once the memo keys on it.
    self_check_violations: Vec<SelfCheckViolation>,
    /// Set when self-check sampling stopped at [`MAX_SELF_CHECK_SAMPLES`].
    /// A truncated run checked only the environments it had already seen, so a
    /// clean result covers less than the whole pass — reported, never silent.
    self_check_truncated: bool,
    /// Which key the self-check groups by. Never anything but
    /// [`SelfCheckKeyMode::Full`] outside the check's own regression test.
    self_check_key_mode: SelfCheckKeyMode,
}

/// Ceiling on tracked environment keys. Reaching it stops key recording for the
/// rest of the pass and raises `envs_truncated`.
///
/// Environment tracking is inherently O(distinct environments), and a `map` over
/// 10^5 elements produces 10^5 of them; an opt-in profiler may be slow but must
/// not be the reason a session runs out of memory. At 16 bytes a key
/// ([`EvalEnvKey`] is 128-bit) plus hash-set overhead this bound is well under
/// 100 MB.
pub const MAX_TRACKED_ENVS: usize = 1_000_000;

/// Ceiling on retained self-check samples.
///
/// Lower than [`MAX_TRACKED_ENVS`] because an entry here is a summary *string*
/// rather than a `u64`: at a hundred-odd bytes apiece this bound is a few tens
/// of MB, where a million would be hundreds. The check runs in release builds
/// now, so this is a real bound on a real design rather than a theoretical one.
pub const MAX_SELF_CHECK_SAMPLES: usize = 200_000;

impl EvalProfile {
    /// Every recorded node, in first-seen order. The "By node" table sorts a
    /// copy of this by self time; the "By node type" table rolls it up.
    pub fn records(&self) -> &[NodeProfileRecord] {
        &self.records
    }

    /// Total `NodeData::eval` invocations the pass made.
    pub fn total_evaluations(&self) -> u64 {
        self.records.iter().map(|r| r.evaluations).sum()
    }

    /// Summed self time over every record. Equal to the summed *total* time of
    /// the outermost evaluations, which is the useful sanity check against the
    /// refresh's `eval_ms` phase.
    pub fn total_self_ns(&self) -> u64 {
        self.records.iter().map(|r| r.self_ns).sum()
    }

    /// Roll-up by node type (D5: "both tables come from one map"), returned in
    /// no particular order — the UI sorts.
    pub fn by_node_type(&self) -> Vec<NodeTypeProfileRecord> {
        let mut by_type: HashMap<&str, NodeTypeProfileRecord> = HashMap::new();
        for record in &self.records {
            let entry = by_type
                .entry(record.location.node_type_name.as_str())
                .or_insert_with(|| NodeTypeProfileRecord {
                    node_type_name: record.location.node_type_name.clone(),
                    nodes: 0,
                    evaluations: 0,
                    self_ns: 0,
                    total_ns: 0,
                });
            entry.nodes += 1;
            entry.evaluations += record.evaluations;
            entry.self_ns += record.self_ns;
            entry.total_ns += record.total_ns;
        }
        by_type.into_values().collect()
    }

    /// Depth of the child-accumulator stack. Zero everywhere outside an
    /// evaluation; the guard-release tests assert it is zero at end of pass.
    pub fn live_frame_count(&self) -> usize {
        self.child_acc.len()
    }

    /// Total result *requests* the pass made. Equal to
    /// [`Self::total_evaluations`] until the memo lands; afterwards the
    /// difference is the memo's hit count (D10).
    pub fn total_lookups(&self) -> u64 {
        self.records.iter().map(|r| r.lookups).sum()
    }

    /// How many distinct evaluation environments the pass visited — and, since
    /// the key already carries the node id and `decorate`, the **number of
    /// entries a perfect memo would hold at peak**.
    pub fn total_distinct_envs(&self) -> u64 {
        self.all_envs.len() as u64
    }

    /// Pass-level redundancy factor: requests per distinct environment.
    ///
    /// Never the *only* number reported (D10). A pass that is globally 2.5x can
    /// be 11x on `materialize` and 1.0 on body nodes, and only the per-node
    /// breakdown says where a memo would pay.
    pub fn redundancy_factor(&self) -> f64 {
        let distinct = self.total_distinct_envs();
        if distinct == 0 {
            return 1.0;
        }
        self.total_lookups() as f64 / distinct as f64
    }

    /// Summed `wasted_ns` over the records the memo would actually cache. Rows
    /// flagged uncacheable (D10) are excluded rather than quietly inflating the
    /// projected saving.
    pub fn projected_saving_ns(&self) -> u64 {
        self.records
            .iter()
            .filter(|r| !r.flags.uncacheable())
            .map(|r| r.wasted_ns())
            .sum()
    }

    /// Whether environment tracking stopped early at [`MAX_TRACKED_ENVS`]. A
    /// truncated pass under-counts `distinct_envs`, so its redundancy numbers
    /// are upper bounds; the panel says so.
    pub fn envs_truncated(&self) -> bool {
        self.envs_truncated
    }

    /// Whether the D11 equal-key/equal-result self-check ran for this pass.
    pub fn self_check_ran(&self) -> bool {
        self.self_check_results.is_some()
    }

    /// Whether self-check sampling hit [`MAX_SELF_CHECK_SAMPLES`] and stopped
    /// taking new environments. A clean result from a truncated run covers only
    /// part of the pass.
    pub fn self_check_truncated(&self) -> bool {
        self.self_check_truncated
    }

    /// What the self-check found. Empty is the expected — and, so far, the
    /// observed — outcome.
    pub fn self_check_violations(&self) -> &[SelfCheckViolation] {
        &self.self_check_violations
    }

    /// **The acceptance criterion of `doc/design_eval_memoization.md`, as one
    /// number**: rows that re-evaluated within a single environment without a
    /// flag excusing it.
    ///
    /// With the memo on this reads zero. Computed here rather than in the panel
    /// so the criterion is testable without a UI, and so the population it is
    /// computed over — *unflagged* rows only — has exactly one definition.
    pub fn unmemoized_offender_count(&self) -> usize {
        self.records
            .iter()
            .filter(|record| !record.flags.uncacheable())
            .filter(|record| record.evaluations > record.distinct_envs)
            .count()
    }
}

/// One row of the "By node type" table — a roll-up of every
/// [`NodeProfileRecord`] sharing a type name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeTypeProfileRecord {
    pub node_type_name: String,
    /// How many distinct nodes of this type were evaluated.
    pub nodes: u64,
    pub evaluations: u64,
    pub self_ns: u64,
    pub total_ns: u64,
}

thread_local! {
    /// The off-path fast check. Separate from [`PROFILE`] so a pass with
    /// profiling off costs one `Cell` read per evaluation rather than a
    /// `RefCell` borrow (D1).
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    /// Whether the D11 self-check is armed for the current pass, and under
    /// which key. A second `Cell` for the same reason [`ENABLED`] is one: the
    /// check's own hook runs once per evaluation and must cost nothing when it
    /// is off, which is its normal state.
    static SELF_CHECK: Cell<Option<SelfCheckKeyMode>> = const { Cell::new(None) };
    /// The live accumulator for the current pass, or `None` when profiling is
    /// off. Owned by `StructureDesigner::with_eval_context`.
    static PROFILE: RefCell<Option<EvalProfile>> = const { RefCell::new(None) };
}

/// Installs a fresh profile for the pass that is about to run, or clears any
/// previous one when `profile` is `None` (profiling off).
///
/// Called only from `StructureDesigner::with_eval_context`, which pairs it with
/// [`take`] on every exit path.
pub fn install(profile: Option<EvalProfile>) {
    ENABLED.set(profile.is_some());
    SELF_CHECK.set(profile.as_ref().and_then(|p| {
        p.self_check_results
            .is_some()
            .then_some(p.self_check_key_mode)
    }));
    PROFILE.with_borrow_mut(|slot| *slot = profile);
}

/// A profile with the D11 equal-key/equal-result self-check armed.
///
/// **Available in every build, gated at runtime** — D11 specifies a
/// `debug_assertions` gate, and that is the one part of it this implementation
/// does not follow. `flutter run` loads the **release** DLL, so a compile-gated
/// check would be missing from the only build the maintainer runs against real
/// designs, which is precisely the failure mode D2 introduces the runtime
/// profiler toggle to avoid. The gate's other justification disappeared when
/// violations became *recorded* rather than asserted: `debug_assert!` compiles
/// out in release, a recorded finding does not.
///
/// What remains is cost, and the runtime toggle already controls it: the check
/// is off by default, does nothing unless per-node profiling is also on, and
/// when armed adds one result summary per evaluation plus one retained summary
/// per distinct environment (bounded by [`MAX_SELF_CHECK_SAMPLES`]).
///
/// **The check only means anything with the memo disabled** (D11). Once the memo
/// serves the second request from the first result there is no second
/// computation to compare, and the check passes vacuously.
///
/// That is enforced by a **hard gate**, not by forcing the memo off for the
/// pass: `StructureDesigner::try_set_eval_self_check_enabled` refuses to arm the
/// check while `eval_memo_enabled` is on, and `set_eval_memo_enabled(true)`
/// disarms an armed check. Auto-forcing would make one switch have two effects
/// and the second one invisible — the pass's *Self*, *Total* and *Phases*
/// numbers would silently become memo-off numbers, sitting in the same history
/// ring as comparable ones (`doc/design_eval_memoization.md` D10).
pub fn profile_with_self_check() -> EvalProfile {
    profile_with_self_check_mode(SelfCheckKeyMode::Full)
}

/// [`profile_with_self_check`] with an explicit key mode. Only the check's own
/// regression test passes anything but [`SelfCheckKeyMode::Full`].
pub fn profile_with_self_check_mode(mode: SelfCheckKeyMode) -> EvalProfile {
    EvalProfile {
        self_check_results: Some(HashMap::new()),
        self_check_key_mode: mode,
        ..EvalProfile::default()
    }
}

/// Takes the finished profile back out at end of pass. Returns `None` when
/// profiling was off.
pub fn take() -> Option<EvalProfile> {
    ENABLED.set(false);
    SELF_CHECK.set(None);
    let profile = PROFILE.with_borrow_mut(|slot| slot.take());
    debug_assert!(
        profile.as_ref().is_none_or(|p| p.child_acc.is_empty()),
        "eval profiler: {} guard frame(s) leaked — an early return in \
         `evaluate` / `evaluate_all_outputs` bypassed the RAII guard",
        profile.as_ref().map_or(0, |p| p.child_acc.len())
    );
    profile
}

/// Whether a profile is currently installed. The hot-path check.
#[inline]
pub fn is_enabled() -> bool {
    ENABLED.get()
}

/// Records the network this pass is rooted at, for the location of every
/// record the pass produces. Called at the top of each displayed-root
/// evaluation; a pass covers exactly one active network, so the repeated writes
/// all carry the same name. A no-op when profiling is off.
pub fn note_root_network(name: &str) {
    if !is_enabled() {
        return;
    }
    PROFILE.with_borrow_mut(|slot| {
        if let Some(profile) = slot.as_mut()
            && profile.root_network_name != name
        {
            profile.root_network_name = name.to_string();
        }
    });
}

/// The RAII bookkeeping for one node evaluation (D4).
///
/// **Release must be by `Drop`, not by hand.** Both hook functions have many
/// early exits — the poison check, the cycle guard, the central `Unit`-skip
/// rule, the invalid-network and missing-return-node bails — and a leaked frame
/// corrupts every ancestor's self time *silently*. That is the opposite trade
/// from the `eval_in_progress` bracket next to it, whose manual cleanup is
/// deliberate: a leak there produces a caught error, not silent corruption.
///
/// The struct is a plain `Instant` + `u32` — no borrows, no closure wrapper, no
/// `Box` — so it respects the STACK-SIZE WARNING on `evaluate_all_outputs`, and
/// it is **not constructed at all** when profiling is off.
pub struct NodeEvalGuard {
    start: Instant,
    record_index: u32,
    /// The environment key the D11 self-check groups this evaluation under
    /// (D9). Carried on the guard so [`NodeEvalGuard::note_results`] can feed
    /// the check without re-hashing the stack — by the time the results exist
    /// the stack slice is no longer in scope at every call site.
    ///
    /// The real `eval_env_key` in every mode but the deliberately-weakened one,
    /// and unread when the check is off. The counters do not need it: `begin`
    /// folds the real key into the record's environment set there and then.
    self_check_key: EvalEnvKey,
}

impl NodeEvalGuard {
    /// Record what this evaluation produced: the memo-exclusion flags (D10) and
    /// the D11 self-check sample.
    ///
    /// Called from the two hook functions once the results exist — deliberately
    /// **not** from `Drop`, which cannot see them. Costs one thread-local borrow
    /// per evaluation on top of the guard's own, and only when profiling is on;
    /// the self-check's expensive half (`to_display_string`) runs only when the
    /// check is additionally armed.
    ///
    /// `full_output` says whether `results` is the node's **complete**
    /// `EvalOutput` or a single projected pin. It gates the self-check and
    /// nothing else, and the distinction is load-bearing rather than fussy: the
    /// environment key deliberately excludes the output pin index (the memo's
    /// value is the whole `EvalOutput` — `doc/design_eval_memoization.md` D2),
    /// so comparing one pin's projection against another's under one key would
    /// report a violation on every two-output node consumed on both pins. The
    /// flags are safe either way — they only ever get OR-ed in.
    pub fn note_results(&self, results: &[NetworkResult], full_output: bool) {
        let produced_iterator = results
            .iter()
            .any(|r| matches!(r, NetworkResult::Iterator(_)));
        let summary = (full_output && SELF_CHECK.get().is_some()).then(|| result_summary(results));
        if !produced_iterator && summary.is_none() {
            return;
        }
        PROFILE.with_borrow_mut(|slot| {
            let Some(profile) = slot.as_mut() else {
                return;
            };
            let record = &mut profile.records[self.record_index as usize];
            record.flags.produced_iterator |= produced_iterator;
            let label = record.location.label.clone();
            let Some(summary) = summary else {
                return;
            };
            let Some(seen) = profile.self_check_results.as_mut() else {
                return;
            };
            match seen.get(&self.self_check_key) {
                None if seen.len() >= MAX_SELF_CHECK_SAMPLES => {
                    // Stop retaining new environments, but keep comparing the
                    // ones already sampled: a partial check still catches a
                    // wrong key on everything it saw before the ceiling.
                    profile.self_check_truncated = true;
                }
                None => {
                    seen.insert(self.self_check_key, summary);
                }
                Some(first) if *first != summary => {
                    // Equal key, different result: the key is missing an input.
                    // Recorded rather than panicked so one pass reports *every*
                    // offender — a panic in the middle of a refresh would show
                    // the first one and lose the rest, and this check exists to
                    // be run against real designs.
                    profile.self_check_violations.push(SelfCheckViolation {
                        label,
                        first: first.clone(),
                        later: summary,
                    });
                }
                Some(_) => {}
            }
        });
    }

    /// Flag this row as a **custom-network instance** whose request went
    /// through `evaluate`'s single-pin arm, which
    /// `doc/design_eval_memoization.md` D2 forbids inserting from. See
    /// [`RecordFlags::subnetwork`] for why an unflagged row would read as a
    /// memo bug.
    pub fn note_subnetwork(&self) {
        self.set_flag(|flags| flags.subnetwork = true);
    }

    /// Flag this row as having been recomputed after the memo's LRU dropped an
    /// entry it had held (D6/D10).
    pub fn note_evicted(&self) {
        self.set_flag(|flags| flags.evicted = true);
    }

    fn set_flag(&self, apply: impl FnOnce(&mut RecordFlags)) {
        PROFILE.with_borrow_mut(|slot| {
            if let Some(profile) = slot.as_mut() {
                apply(&mut profile.records[self.record_index as usize].flags);
            }
        });
    }
}

/// The self-check's notion of "same result" (D11).
///
/// `NetworkResult` equality is neither universally cheap nor even defined for
/// every variant — `Function` carries a `Box<dyn NodeData>`, `Iterator` a live
/// walker — so this compares display strings plus, where they exist, atom and
/// bond counts. **A weak check that runs beats a perfect one that does not**:
/// it is strong enough to have caught the `decorate` omission and a `NodeRef`
/// key collision, which is what it is for.
///
/// Arrays are capped so a 10^5-element result does not turn the check into the
/// dominant cost of the pass; two arrays agreeing on their first elements and
/// differing later is a shape no known evaluator bug produces.
fn result_summary(results: &[NetworkResult]) -> String {
    const SUMMARY_ARRAY_CAP: usize = 32;
    let mut out = String::new();
    for result in results {
        out.push_str(&result.to_display_string_capped(SUMMARY_ARRAY_CAP));
        match result {
            NetworkResult::Crystal(data) => out.push_str(&atomic_summary(&data.atoms)),
            NetworkResult::Molecule(data) => out.push_str(&atomic_summary(&data.atoms)),
            _ => {}
        }
        out.push(';');
    }
    out
}

/// Atom/bond counts plus a **decorator fingerprint**.
///
/// The decorator half is not padding: `decorate` is one of the three varying
/// inputs the environment key carries, and every node that reads it —
/// `atom_edit`, `edit_atom` — expresses the difference *only* through decorator
/// state (selection marks, `from_selected_node`, guide visuals). Atom counts and
/// display strings are identical either way, so without this the check could not
/// see the very omission D11 names as the thing it would have caught.
fn atomic_summary(atoms: &atomcad_crystolecule::atomic_structure::AtomicStructure) -> String {
    let decorator = atoms.decorator();
    format!(
        "|atoms={},bonds={},sel={},marks={},labels={}",
        atoms.get_num_of_atoms(),
        atoms.get_num_of_bonds(),
        decorator.from_selected_node as u8,
        decorator.atom_display_states.len(),
        decorator.atom_label.len(),
    )
}

/// Flag the node's record as having tripped the re-entrancy backstop (D10).
///
/// Called from the two hook functions' cycle arms, which return *before*
/// opening a profiler frame — so this looks the record up by key rather than
/// through a guard. In a genuine cycle the record already exists: the outer
/// evaluation of the same node opened it before recursing. When it does not
/// (a shape no cycle can produce), the call is a no-op rather than a phantom
/// zero-evaluation row.
pub fn note_reentrancy_backstop(network_stack: &[NetworkStackElement], node_id: u64) {
    if !is_enabled() {
        return;
    }
    let key = crate::evaluator::network_evaluator::node_profile_key(network_stack, node_id);
    PROFILE.with_borrow_mut(|slot| {
        let Some(profile) = slot.as_mut() else {
            return;
        };
        if let Some(index) = profile.index.get(&key).copied() {
            profile.records[index as usize]
                .flags
                .under_reentrancy_backstop = true;
        }
    });
}

impl Drop for NodeEvalGuard {
    fn drop(&mut self) {
        let total_ns = self.start.elapsed().as_nanos() as u64;
        let record_index = self.record_index;
        PROFILE.with_borrow_mut(|slot| {
            let Some(profile) = slot.as_mut() else {
                // The pass ended under us. Impossible in the evaluator (the
                // take happens after `f` returns, so every guard is already
                // dropped), but dropping the sample beats panicking in `Drop`.
                return;
            };
            let children_ns = profile.child_acc.pop().unwrap_or(0);
            let record = &mut profile.records[record_index as usize];
            record.evaluations += 1;
            // `saturating_sub` rather than an assert: on a coarse or
            // non-monotonic-across-cores clock a child can measure marginally
            // longer than its parent, and the invariant the tables rely on is
            // `self_ns <= total_ns`, which this preserves.
            record.self_ns += total_ns.saturating_sub(children_ns);
            record.total_ns += total_ns;
            // Charge the whole subtree to the parent, so *its* self time
            // excludes everything it pulled. This is the one line that makes
            // "time spent evaluating upstream dependencies is not charged to
            // the consumer" true.
            if let Some(parent) = profile.child_acc.last_mut() {
                *parent += total_ns;
            }
        });
    }
}

/// Opens a profiling frame for one evaluation of `node_id` against
/// `network_stack`, or returns `None` when profiling is off.
///
/// The returned guard must be bound to a local (`let _guard = …`) so it lives
/// for the whole evaluation; binding it to `_` would drop it immediately and
/// charge the node nothing.
pub fn begin(
    network_stack: &[NetworkStackElement],
    node_id: u64,
    scope_path: &[u64],
    decorate: bool,
) -> Option<NodeEvalGuard> {
    if !is_enabled() {
        return None;
    }
    let key = crate::evaluator::network_evaluator::node_profile_key(network_stack, node_id);
    // O(stack depth), and gated on the toggle for exactly that reason (D9).
    // When the memo lands it consults the key on every evaluation and this
    // becomes unconditional — which is why the function lives in the evaluator
    // next to `eval_frame_key` rather than in this module.
    let env_key = eval_env_key(network_stack, node_id, decorate);
    let self_check_key = match SELF_CHECK.get() {
        Some(SelfCheckKeyMode::OmitDecorate) => eval_env_key(network_stack, node_id, false),
        _ => env_key,
    };
    let record_index = count_lookup(network_stack, node_id, scope_path, key, env_key, true)?;
    Some(NodeEvalGuard {
        start: Instant::now(),
        record_index,
        self_check_key,
    })
}

/// Count a request that the **evaluation memo served from a previous
/// evaluation** (`doc/design_eval_memoization.md` D10).
///
/// A hit is a `lookup` and an environment visit, but *not* an `evaluation`: it
/// must therefore not open a [`NodeEvalGuard`], whose `Drop` is what increments
/// `evaluations` and pops a child-accumulator frame. The divergence between the
/// two columns is the memo's hit count, which is the whole reason they were
/// separate fields before there was a memo.
///
/// The clone the hit pays for lands in the *consumer's* self time, because no
/// frame is opened for the producer. That is the honest attribution: with the
/// memo on, serving the value is the consumer's cost, not a second evaluation
/// of the producer.
pub fn note_memo_hit(
    network_stack: &[NetworkStackElement],
    node_id: u64,
    scope_path: &[u64],
    decorate: bool,
) {
    if !is_enabled() {
        return;
    }
    let key = crate::evaluator::network_evaluator::node_profile_key(network_stack, node_id);
    let env_key = eval_env_key(network_stack, node_id, decorate);
    count_lookup(network_stack, node_id, scope_path, key, env_key, false);
}

/// The bookkeeping shared by [`begin`] and [`note_memo_hit`]: find or create the
/// row, count the lookup and the environment, and — only for a real evaluation
/// — push the child-accumulator frame the guard's `Drop` will pop.
fn count_lookup(
    network_stack: &[NetworkStackElement],
    node_id: u64,
    scope_path: &[u64],
    key: NodeProfileKey,
    env_key: EvalEnvKey,
    open_frame: bool,
) -> Option<u32> {
    PROFILE.with_borrow_mut(|slot| {
        let profile = slot.as_mut()?;
        let record_index = match profile.index.get(&key) {
            Some(index) => *index,
            None => {
                // Vacant insert is the **only** place a location is built: it
                // clones a `Vec` and formats a `String`, which must never
                // happen once per evaluation (D5).
                let location = build_location(
                    network_stack,
                    node_id,
                    scope_path,
                    &profile.root_network_name,
                );
                let index = profile.records.len() as u32;
                profile.records.push(NodeProfileRecord {
                    location,
                    lookups: 0,
                    evaluations: 0,
                    distinct_envs: 0,
                    self_ns: 0,
                    total_ns: 0,
                    flags: RecordFlags::default(),
                });
                profile.record_envs.push(HashSet::new());
                profile.index.insert(key, index);
                index
            }
        };
        // A lookup is counted here and an evaluation on release (`Drop`), so
        // the two fields are already measuring different things before the memo
        // makes them diverge.
        profile.records[record_index as usize].lookups += 1;
        if profile.all_envs.len() < MAX_TRACKED_ENVS {
            profile.all_envs.insert(env_key);
            if profile.record_envs[record_index as usize].insert(env_key) {
                profile.records[record_index as usize].distinct_envs += 1;
            }
        } else {
            profile.envs_truncated = true;
        }
        if open_frame {
            profile.child_acc.push(0);
        }
        Some(record_index)
    })
}

/// Builds the display/navigation half of a record (D5). Cold path — vacant
/// insert only.
fn build_location(
    network_stack: &[NetworkStackElement],
    node_id: u64,
    scope_path: &[u64],
    root_network_name: &str,
) -> NodeLocation {
    let node = network_stack
        .last()
        .and_then(|frame| frame.node_network.nodes.get(&node_id));
    let node_type_name = node.map_or_else(|| "?".to_string(), |n| n.node_type_name.clone());

    // The deepest **registry-owned** frame: the network the node's home body
    // chain hangs off. `None` when the stack is body-only, which is how the
    // lazy walkers call `run_closure_once`.
    let home = network_stack.iter().rposition(|frame| !frame.is_zone_body);

    let mut label = match home {
        Some(index) => network_stack[index].node_network.node_type.name.clone(),
        None => root_network_name.to_string(),
    };
    if label.is_empty() {
        label.push('?');
    }
    // One segment per body frame above the home network, naming the node that
    // *owns* the body (`fold#12`) rather than the anonymous body itself. The
    // owner lives in the frame below, which is missing for a body-only stack —
    // there the segment degenerates to `#12`.
    let body_start = home.map_or(0, |index| index + 1);
    for index in body_start..network_stack.len() {
        let owner_id = network_stack[index].node_id;
        let owner_type = index
            .checked_sub(1)
            .and_then(|below| network_stack[below].node_network.nodes.get(&owner_id))
            .map(|owner| owner.node_type_name.as_str())
            .unwrap_or("");
        label.push_str(&format!("/{}#{}", owner_type, owner_id));
    }
    label.push_str(&format!("/{}#{}", node_type_name, node_id));
    if let Some(custom_name) = node.and_then(|n| n.custom_name.as_ref()) {
        label.push_str(&format!(" ({})", custom_name));
    }

    // The jump address is **home-relative**, derived from the same frame the
    // label and the aggregation key are: the network the node actually lives
    // in, plus the body chain above it. A node inside a custom network is
    // therefore navigable — the jump activates *that* network by name, exactly
    // as the error-navigation jump lands on a root cause across a network
    // boundary.
    //
    // Deriving it from the network stack rather than from `eval_scope_path` is
    // also what makes it survive a **parameter stack excursion**: a custom
    // network's `parameter` resolves its argument by popping the network-stack
    // frame while the instance's eval scope stays pushed
    // (`nodes/parameter.rs`), so a caller-side node evaluated through one would
    // otherwise record a scope path carrying an instance id it does not live
    // under.
    let (host_network, address_scope_path, navigable) = match home {
        Some(index) => {
            let host = network_stack[index].node_network.node_type.name.clone();
            let path = network_stack[index + 1..]
                .iter()
                .map(|frame| frame.node_id)
                .collect();
            let named = !host.is_empty();
            (host, path, named)
        }
        None => {
            // Body-only stack — how the lazy walkers call `run_closure_once`.
            // There is no home frame to read, so fall back to the pass's root
            // network plus the eval scope path, which *is* maintained on the
            // real scene context for these bodies. That is exact only when the
            // whole path is body hops: an instance hop in it would put an id
            // there that no `Node.zone` walk can follow, and the two lengths
            // agreeing is precisely the check for that.
            let exact = scope_path.len() == network_stack.len();
            (root_network_name.to_string(), scope_path.to_vec(), exact)
        }
    };

    NodeLocation {
        host_network,
        scope_path: address_scope_path,
        node_id,
        label,
        node_type_name,
        navigable,
    }
}
