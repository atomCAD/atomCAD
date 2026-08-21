//! The per-pass evaluation memo — Phase 3 of
//! `doc/design_eval_memoization.md` (D1–D5, D7–D10).
//!
//! The evaluator is *demand-driven but not sharing*: a node reached twice in
//! one pass is computed twice, so a diamond re-runs its apex per consuming wire
//! and every displayed root re-walks its whole upstream cone. This module is
//! the missing half — a table from **evaluation environment** to the complete
//! [`EvalOutput`] a node produced in it, alive for exactly one refresh pass.
//!
//! ## What it keys on, and why that is sufficient
//!
//! The key is [`EvalEnvKey`], computed by
//! [`eval_env_key`](crate::evaluator::network_evaluator::eval_env_key) — the
//! same function the profiler's redundancy counters use, which is why the
//! panel's prediction and the memo's behaviour agree by construction. The
//! argument that "same environment ⇒ same result" lives in
//! `doc/design_eval_memoization.md` §"The evaluation environment" and is not
//! restated here; in one line, `NodeData::eval`'s six arguments are a closed
//! input surface, three of them vary (`network_stack`, `node_id`, `decorate`),
//! and the only two live context reads not determined by the stack change
//! exactly once per closure invocation — which is what
//! [`NetworkStackElement::env_epoch`] numbers.
//!
//! ## Why a thread-local rather than a field on `NetworkEvaluationContext`
//!
//! The rule in `evaluator/AGENTS.md`: per-pass state belongs either in a pass
//! thread-local or in **both** `fresh_inner_for_eager_body` and
//! `drain_inner_context`, never on the context alone. A context-owned memo
//! would hand every eager-HOF body (`apply`, `fold`, `foreach`) a fresh empty
//! table that `drain_inner_context` then discards — bodies would memoize
//! nothing, and it would read as a tuning problem rather than a wiring bug.
//! Sharing one table across the split is sound precisely because `env_epoch`
//! is in the key.
//!
//! `StructureDesigner::with_eval_context` owns the lifetime, installing at the
//! start of a pass and taking the counters back at the end — the same seam and
//! the same discipline as [`crate::evaluator::eval_profiler`].
//!
//! ## The three deliberate exclusions
//!
//! - **Iterators** (D4/D6 R4). A stored [`Walker`](crate::evaluator::iterator_walker::Walker)
//!   pins its `ZoneClosure` for the whole pass while buying almost nothing (a
//!   `map`'s `eval` only *builds* the walker; the work is in `next()`). The
//!   test is [`EvalOutput::contains_iterator`], which recurses through `Array`
//!   and `Record` and asks `display_results` as well — not the profiler's
//!   top-level `RecordFlags::produced_iterator` flag. It lives inside
//!   [`insert`] so no call site can forget it.
//! - **`evaluate`'s custom-network arm** (D2). That arm forwards a single
//!   `output_pin_index` to the child's return node and gets one
//!   `NetworkResult` back, so it never holds the *complete* output the key
//!   (which omits the pin index) promises. Its call site calls
//!   [`note_declined`] instead.
//! - **Results produced under the re-entrancy backstop** (D9). With a cycle
//!   `A → B → A` the inner and outer evaluations of `A` share a byte-identical
//!   environment and return different results — the one case where the key is
//!   genuinely insufficient. Those arms return before reaching [`insert`].
//!
//! ## Epoch-scoped eviction (D3)
//!
//! An entry created inside body invocation *N* is keyed to *N* and can never be
//! reached again once that invocation pops, so it is retired there rather than
//! left for the LRU to notice: without it a 10⁵-element `map` accumulates 10⁵
//! generations of dead entries. [`retire_epoch`] is called from
//! `zone_closure::run_closure_once`, the single place a body invocation ends.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use atomcad_util::memory_bounded_lru_cache::MemoryBoundedLruCache;
use atomcad_util::memory_size_estimator::MemorySizeEstimator;

use crate::evaluator::network_evaluator::{EvalEnvKey, NetworkStackElement};
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::EvalOutput;

/// Ceiling on the set of keys the memo remembers having inserted.
///
/// That set exists only to tell an *evicted* miss from a never-seen one (the
/// `evicted` row flag, D10). It is trimmed continuously by [`retire_epoch`], so
/// on a realistic pass it tracks the live entry count; the ceiling bounds the
/// pathological case where a pass produces millions of top-level environments,
/// where a diagnostic must not be the reason a session runs out of memory.
pub const MAX_TRACKED_INSERTED_KEYS: usize = 1_000_000;

/// What one memo pass did — harvested into `RefreshProfile` before the table is
/// dropped (D10).
///
/// There is deliberately no `get_memo_stats()`: unlike the CSG cache the memo
/// does not exist when the panel renders, so a live-read API would return
/// zeroes every time it was called.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoCounts {
    /// Whether the memo was switched on for the pass (D10). Every other field
    /// is zero when it was not, and `false` here is what distinguishes that
    /// from "on, but this design has no sharing to exploit".
    pub enabled: bool,
    /// Entries held at the high-water mark. Compare against
    /// `EvalProfile::total_distinct_envs`, which *predicts* it: the memo's peak
    /// is lower by whatever D3 retired and the LRU evicted, never higher. A
    /// large unexplained gap means the memo and the profiler are keying on
    /// different things.
    pub peak_entries: usize,
    /// Bytes held at the high-water mark, by the D6 size estimator's reckoning.
    /// The number Phase 5 would tune against.
    pub peak_bytes: usize,
    /// Entries still held when the pass ended.
    pub end_entries: usize,
    /// Bytes still held when the pass ended. A peak far above this means D3's
    /// epoch eviction is doing its job.
    pub end_bytes: usize,
    /// The configured budget (D11), so a peak is readable without fetching the
    /// preference separately.
    pub budget_bytes: usize,
    /// Requests served from the table.
    pub hits: u64,
    /// Requests that had to be computed.
    pub misses: u64,
    /// Of those misses, the ones whose key the memo had held earlier — i.e.
    /// work redone because the budget was too small. Backs the `evicted` row
    /// flag, without which memory pressure is indistinguishable from a
    /// correctness bug.
    pub evicted_misses: u64,
    /// Entries the LRU dropped because the budget was exceeded.
    pub lru_evictions: u64,
    /// Entries retired because their body iteration ended (D3). Kept separate
    /// from `lru_evictions` on purpose: both remove entries and both show up as
    /// later misses, but an epoch drop is the design working and an LRU
    /// eviction is the budget being too small. Collapsed into one number, the
    /// single signal Phase 5 fires on becomes unreadable.
    pub epoch_drops: u64,
    /// The deliberate exclusions firing (D2's subnetwork arm, D4's iterators,
    /// D9's re-entrancy), as a total.
    pub declined_inserts: u64,
    /// Time inside [`insert`], which is dominated by the D6 R2 size estimator's
    /// recursive walk over the value (and over anything it evicts).
    ///
    /// The optional counter D10 asks for, under an honest name: the estimator
    /// runs only on insert and eviction, never on a lookup, so if this ever
    /// became significant it would erode the win invisibly — charged to the
    /// eval phase and attributable to nothing. A single accumulated duration
    /// rules that out at a glance.
    pub insert_ns: u64,
    /// The inserted-key set hit [`MAX_TRACKED_INSERTED_KEYS`], so
    /// [`Self::evicted_misses`] (and the `evicted` row flag it backs) is a
    /// floor for the rest of the pass. Reported rather than silently capped: a
    /// diagnostic that quietly stops counting reads as "nothing happened".
    pub inserted_tracking_truncated: bool,
}

/// The live table. Never leaves this module — the outside world sees
/// [`MemoCounts`].
struct EvalMemo {
    cache: MemoryBoundedLruCache<EvalEnvKey, EvalOutput>,
    /// Keys inserted under each **live** body invocation, so [`retire_epoch`]
    /// can drop a whole iteration's entries in one step (D3).
    epoch_keys: HashMap<u64, Vec<EvalEnvKey>>,
    /// Keys the memo has held and not epoch-retired. Membership on a miss means
    /// the LRU dropped the entry.
    inserted: HashSet<EvalEnvKey>,
    /// Set once [`MAX_TRACKED_INSERTED_KEYS`] is reached; from then on
    /// `evicted_misses` under-reports rather than growing without bound.
    inserted_truncated: bool,
    counts: MemoCounts,
}

/// Size of one stored value, for the byte budget (D6). A plain `fn` because
/// that is what [`MemoryBoundedLruCache`] takes.
fn estimate_eval_output(output: &EvalOutput) -> usize {
    output.estimate_memory_bytes()
}

impl EvalMemo {
    fn new(budget_bytes: usize) -> Self {
        Self {
            cache: MemoryBoundedLruCache::new(budget_bytes, estimate_eval_output),
            epoch_keys: HashMap::new(),
            inserted: HashSet::new(),
            inserted_truncated: false,
            counts: MemoCounts {
                enabled: true,
                budget_bytes,
                ..MemoCounts::default()
            },
        }
    }

    /// Fold the cache's own instrumentation into the counters. Called once, at
    /// [`take`] time, because `MemoryBoundedLruCache` tracks the peaks itself.
    fn finish(mut self) -> MemoCounts {
        self.counts.peak_entries = self.cache.peak_len();
        self.counts.peak_bytes = self.cache.peak_memory_bytes();
        self.counts.end_entries = self.cache.len();
        self.counts.end_bytes = self.cache.current_memory_bytes();
        self.counts.lru_evictions = self.cache.lru_eviction_count();
        self.counts.inserted_tracking_truncated = self.inserted_truncated;
        self.counts
    }
}

thread_local! {
    /// The off-path fast check, separate from [`MEMO`] so a pass with the memo
    /// switched off costs one `Cell` read per evaluation rather than a
    /// `RefCell` borrow — the same split the profiler uses.
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    /// The live table for the current pass, or `None` when the memo is off.
    /// Owned by `StructureDesigner::with_eval_context`.
    static MEMO: RefCell<Option<EvalMemo>> = const { RefCell::new(None) };
}

/// Installs a fresh memo for the pass about to run, or clears any previous one
/// when `budget_bytes` is `None` (memo switched off).
///
/// Called only from `StructureDesigner::with_eval_context`, which pairs it with
/// [`take`] on every exit path.
pub fn install(budget_bytes: Option<usize>) {
    ENABLED.set(budget_bytes.is_some());
    MEMO.with_borrow_mut(|slot| *slot = budget_bytes.map(EvalMemo::new));
}

/// Drops the table and returns what it did. `None` when the memo was off — the
/// caller renders that as [`MemoCounts::enabled`] `== false` rather than as a
/// row of zeroes with no explanation.
pub fn take() -> Option<MemoCounts> {
    ENABLED.set(false);
    MEMO.with_borrow_mut(|slot| slot.take())
        .map(EvalMemo::finish)
}

/// Whether a memo is installed. The hot-path check.
#[inline]
pub fn is_enabled() -> bool {
    ENABLED.get()
}

/// Ask the memo for `key`, returning the whole stored output on a hit.
///
/// `evicted` is set to `true` on a miss whose key the memo had held earlier in
/// the pass — i.e. work about to be redone because the LRU dropped it. That is
/// the `evicted` row flag (D10), and without it memory pressure is
/// indistinguishable from a correctness bug.
///
/// An out-parameter rather than a two-variant enum on purpose: `NetworkResult`
/// is ~1.3 KB and `EvalOutput` holds several, and both seams are recursive
/// frames that debug builds run close to the thread stack limit in (see the
/// STACK-SIZE WARNING on `NetworkEvaluator::evaluate_all_outputs`). An
/// `Option<T>` returns through the caller's own return slot; a wrapper enum
/// would cost a second one per frame.
///
/// A miss is counted here, so every call must be a genuine request the
/// evaluator is about to serve one way or the other.
pub fn lookup(key: EvalEnvKey, evicted: &mut bool) -> Option<EvalOutput> {
    MEMO.with_borrow_mut(|slot| {
        let memo = slot.as_mut()?;
        if let Some(output) = memo.cache.get(&key) {
            memo.counts.hits += 1;
            return Some(output.clone());
        }
        memo.counts.misses += 1;
        if memo.inserted.contains(&key) {
            *evicted = true;
            memo.counts.evicted_misses += 1;
        }
        None
    })
}

/// [`lookup`] projected onto one output pin, for `evaluate`'s single-pin seam.
///
/// The projection happens **inside** this function rather than at the call site
/// so no `EvalOutput`-sized temporary lands in `evaluate`'s frame.
///
/// `pin` must be `>= 0`. The `-1` **function pin** is a different value
/// entirely — a synthesized `NetworkResult::Function`, not a projection of the
/// node's `eval` output — and the key does not distinguish it, so that arm
/// neither reads nor writes the memo.
pub fn lookup_pin(key: EvalEnvKey, pin: i32, evicted: &mut bool) -> Option<NetworkResult> {
    debug_assert!(pin >= 0, "the -1 function pin must not consult the memo");
    MEMO.with_borrow_mut(|slot| {
        let memo = slot.as_mut()?;
        if let Some(output) = memo.cache.get(&key) {
            memo.counts.hits += 1;
            return Some(output.get(pin));
        }
        memo.counts.misses += 1;
        if memo.inserted.contains(&key) {
            *evicted = true;
            memo.counts.evicted_misses += 1;
        }
        None
    })
}

/// Store `output` as the result of `key`, unless it carries a lazy walker.
///
/// `owning_epoch` is the innermost live body invocation the evaluation ran
/// under ([`owning_epoch`]), or `0` outside any body: the entry is retired with
/// that invocation (D3).
///
/// The iterator test lives here rather than at the call sites so it cannot be
/// forgotten by a third one — D4's exclusion is about memory, and a stored
/// walker pinning its `ZoneClosure` for the pass is exactly the hazard.
pub fn insert(key: EvalEnvKey, owning_epoch: u64, output: &EvalOutput) {
    MEMO.with_borrow_mut(|slot| {
        let Some(memo) = slot.as_mut() else {
            return;
        };
        if output.contains_iterator() {
            memo.counts.declined_inserts += 1;
            return;
        }
        let started = Instant::now();
        memo.cache.insert(key, output.clone());
        memo.counts.insert_ns += started.elapsed().as_nanos() as u64;
        if owning_epoch != 0 {
            memo.epoch_keys.entry(owning_epoch).or_default().push(key);
        }
        if memo.inserted.len() < MAX_TRACKED_INSERTED_KEYS {
            memo.inserted.insert(key);
        } else {
            memo.inserted_truncated = true;
        }
    });
}

/// Record that an insert was deliberately skipped — D2's custom-network arm of
/// `evaluate` and D9's cycle arms. Purely a counter: the skip itself is the
/// call site returning without calling [`insert`].
pub fn note_declined() {
    MEMO.with_borrow_mut(|slot| {
        if let Some(memo) = slot.as_mut() {
            memo.counts.declined_inserts += 1;
        }
    });
}

/// Drop every entry created under body invocation `epoch` (D3).
///
/// Called from `zone_closure::run_closure_once` once the body frame has popped.
/// Those entries are unreachable from that moment — epochs are allocated from a
/// monotonic per-pass counter and never reused — so this is a pure
/// memory reclaim, not a correctness measure.
pub fn retire_epoch(epoch: u64) {
    if !is_enabled() || epoch == 0 {
        return;
    }
    MEMO.with_borrow_mut(|slot| {
        let Some(memo) = slot.as_mut() else {
            return;
        };
        let Some(keys) = memo.epoch_keys.remove(&epoch) else {
            return;
        };
        for key in keys {
            // `pop`, not an eviction: `MemoryBoundedLruCache` counts only
            // budget-driven removals, which is what keeps "the budget is too
            // small" readable next to "a `map` finished an element".
            if memo.cache.pop(&key).is_some() {
                memo.counts.epoch_drops += 1;
            }
            memo.inserted.remove(&key);
        }
    });
}

/// The innermost live body invocation on `network_stack`, or `0` when the
/// evaluation is not inside one.
///
/// Epochs are handed out by a monotonic per-pass counter at `run_closure_once`
/// only, and a nested invocation therefore always carries a *later* epoch than
/// the one enclosing it — so the deepest non-zero frame is both the innermost
/// invocation and the one whose pop makes the entry unreachable.
pub fn owning_epoch(network_stack: &[NetworkStackElement]) -> u64 {
    network_stack
        .iter()
        .rev()
        .find_map(|frame| (frame.env_epoch != 0).then_some(frame.env_epoch))
        .unwrap_or(0)
}
