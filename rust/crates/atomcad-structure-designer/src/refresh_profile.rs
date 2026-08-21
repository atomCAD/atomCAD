//! Always-on breakdown of a refresh into its phases — Phase 1 of
//! `doc/design_eval_profiling.md` (D1, D6).
//!
//! A refresh of a real design can take seconds and nothing in the application
//! says *where* the time went. One `Instant::now()` per phase boundary — a
//! handful per refresh — is not measurable against a refresh measured in
//! milliseconds, so this clock has no off switch (D1): a regression surfaces
//! during ordinary work rather than only during a deliberate profiling
//! session.
//!
//! **Division of labour.** Each layer times what it owns and nothing else:
//! [`StructureDesigner::refresh`](crate::structure_designer::StructureDesigner::refresh)
//! returns the three sub-phases it can see ([`RefreshSubPhases`]) rather than
//! reaching outward — this crate may not reference `api/` — and the API layer
//! (`rust/src/api/api_common.rs`), the only place that sees tessellation and
//! GPU upload as well, assembles the [`RefreshProfile`] and hands it to
//! [`RefreshProfileHistory::record`].
//!
//! The Dart-side view-building / FFI-marshalling phase is timed in Dart (D7);
//! the gap between that stopwatch and [`RefreshProfile::total_ms`] is itself a
//! measurement — the FFI and serialization overhead nothing else reports.
//!
//! Reports live here for the session only: nothing touches `.cnnd` or
//! preferences, so there is no undo command and no file-format change.

use crate::evaluator::eval_memo::MemoCounts;
use crate::evaluator::eval_profiler::EvalProfile;
use crate::structure_designer_changes::RefreshMode;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

/// How many refresh rows the history ring retains (D6).
pub const REFRESH_PROFILE_HISTORY_CAPACITY: usize = 20;

/// Milliseconds elapsed since `start`. The one place the `Instant` → `f64`
/// conversion lives, so every phase reports in the same unit.
#[inline]
pub fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// The refresh sub-phases visible from inside `StructureDesigner::refresh`.
///
/// `eval_ms` is `None` — deliberately not `0.0` — for a refresh that runs no
/// evaluation pass at all. A `Lightweight` refresh never enters
/// `with_eval_context`, and rendering that as `0.00` would read as "evaluation
/// is free", which is the single most misleading thing this measurement could
/// say (D6).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RefreshSubPhases {
    /// Time inside the displayed-roots evaluation pass, or `None` when the
    /// refresh ran no pass.
    pub eval_ms: Option<f64>,
    /// Time inside `refresh_scene_dependent_node_data`.
    pub scene_dependent_ms: f64,
    /// Time spent rebuilding the gadget and its tessellatable.
    pub gadget_ms: f64,
    /// Per-node evaluation breakdown, `Some` only when the opt-in profiler was
    /// switched on for this pass (Phase 2, D1/D2). `Arc`-shared because the
    /// history ring clones each row, and a table of ~10³ records should not be
    /// deep-copied once per refresh.
    pub node_stats: Option<Arc<EvalProfile>>,
    /// CSG conversion-cache hits and misses **caused by this refresh** (D12).
    /// Reported next to the phase totals rather than folded into node time:
    /// the time itself is charged to the node that triggered the conversion,
    /// and what the counters add is *why* two otherwise identical refreshes
    /// differ.
    pub csg_cache: CsgCacheDelta,
    /// What the per-pass evaluation memo did
    /// (`doc/design_eval_memoization.md` D10), including whether it was
    /// switched on at all.
    ///
    /// **Harvested, not queried.** Unlike the CSG cache the memo does not exist
    /// when the panel renders — it is created and dropped inside one pass (D1)
    /// — so the counters have to be taken at the same seam
    /// `with_eval_context` takes the `EvalProfile`. A stats API that read a
    /// live memo would return zeroes every time it was called.
    ///
    /// Always-on, like the phase clock and unlike the per-node profiler: a few
    /// increments and one `max` per insert are unmeasurable, and someone
    /// chasing a memory number should not have to distort the time numbers to
    /// see it.
    pub memo: MemoCounts,
}

/// CSG conversion-cache activity over one refresh — the difference between two
/// `NetworkEvaluator::get_csg_cache_stats` readings (D12).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CsgCacheDelta {
    pub mesh_hits: u64,
    pub mesh_misses: u64,
    pub sketch_hits: u64,
    pub sketch_misses: u64,
}

impl CsgCacheDelta {
    /// `after - before`, saturating: the cache can be cleared mid-refresh,
    /// which would otherwise underflow.
    pub fn between(
        before: &atomcad_geo_tree::csg_cache::CacheStats,
        after: &atomcad_geo_tree::csg_cache::CacheStats,
    ) -> Self {
        Self {
            mesh_hits: after.mesh_hits.saturating_sub(before.mesh_hits),
            mesh_misses: after.mesh_misses.saturating_sub(before.mesh_misses),
            sketch_hits: after.sketch_hits.saturating_sub(before.sketch_hits),
            sketch_misses: after.sketch_misses.saturating_sub(before.sketch_misses),
        }
    }

    /// Total lookups the refresh made.
    pub fn lookups(&self) -> u64 {
        self.mesh_hits + self.mesh_misses + self.sketch_hits + self.sketch_misses
    }
}

impl RefreshSubPhases {
    /// Sub-phases for a refresh that bailed out before doing any work (no
    /// active network). Every phase is zero and no evaluation pass ran.
    pub fn nothing_ran() -> Self {
        Self::default()
    }
}

/// One row of the refresh phase breakdown.
///
/// A `Lightweight` row may represent **several** coalesced refreshes: one
/// gadget drag emits hundreds of ticks, and letting each take a ring slot
/// would flush the whole history of the interesting refreshes before the user
/// can look at it (D6). For a coalesced row the timing fields carry the
/// **mean** over `count` refreshes, and `max_total_ms` carries the worst one.
#[derive(Debug, Clone, PartialEq)]
pub struct RefreshProfile {
    pub mode: RefreshMode,
    /// `None` when this refresh ran no evaluation pass — see
    /// [`RefreshSubPhases::eval_ms`].
    pub eval_ms: Option<f64>,
    pub scene_dependent_ms: f64,
    pub gadget_ms: f64,
    pub tessellate_ms: f64,
    pub gpu_upload_ms: f64,
    /// `None` on a lightweight refresh, which skips the background mesh
    /// rebuild entirely.
    pub background_ms: Option<f64>,
    /// Wall time of the whole refresh, measured by the API layer. Always at
    /// least the sum of the sub-phases — the remainder is the un-attributed
    /// bookkeeping between them (scene assembly, the eval-error harvest,
    /// preference conversion).
    pub total_ms: f64,
    /// How many refreshes this row represents. `1` except for a coalesced run
    /// of consecutive `Lightweight` ticks.
    pub count: u32,
    /// Largest `total_ms` among the refreshes coalesced into this row.
    pub max_total_ms: f64,
    /// Per-node breakdown of this refresh's evaluation pass, `Some` only when
    /// the opt-in profiler was on. Never carried by a coalesced lightweight
    /// row: a lightweight refresh runs no pass at all.
    pub node_stats: Option<Arc<EvalProfile>>,
    /// CSG conversion-cache hits/misses this refresh caused (D12).
    pub csg_cache: CsgCacheDelta,
    /// Evaluation-memo activity this refresh caused, and whether the memo was
    /// on. The `enabled` flag is what makes the D10 A/B comparison readable in
    /// the history ring: two rows, one memo-off and one memo-on, each carrying
    /// its own numbers.
    pub memo: MemoCounts,
}

impl RefreshProfile {
    /// Assembles a single-refresh row from the per-layer measurements.
    pub fn new(
        mode: RefreshMode,
        sub_phases: RefreshSubPhases,
        tessellate_ms: f64,
        gpu_upload_ms: f64,
        background_ms: Option<f64>,
        total_ms: f64,
    ) -> Self {
        Self {
            mode,
            eval_ms: sub_phases.eval_ms,
            scene_dependent_ms: sub_phases.scene_dependent_ms,
            gadget_ms: sub_phases.gadget_ms,
            tessellate_ms,
            gpu_upload_ms,
            background_ms,
            total_ms,
            count: 1,
            max_total_ms: total_ms,
            node_stats: sub_phases.node_stats,
            csg_cache: sub_phases.csg_cache,
            memo: sub_phases.memo,
        }
    }

    /// Sum of the sub-phases this row attributes. Never exceeds `total_ms`.
    pub fn attributed_ms(&self) -> f64 {
        self.eval_ms.unwrap_or(0.0)
            + self.scene_dependent_ms
            + self.gadget_ms
            + self.tessellate_ms
            + self.gpu_upload_ms
            + self.background_ms.unwrap_or(0.0)
    }

    /// Folds `other` into this row, turning every timing field into the
    /// running mean over the enlarged `count` and tracking the worst total.
    fn coalesce(&mut self, other: &RefreshProfile) {
        let n = self.count.saturating_add(1);
        self.count = n;
        self.eval_ms = merge_mean_opt(self.eval_ms, other.eval_ms, n);
        self.scene_dependent_ms = merge_mean(self.scene_dependent_ms, other.scene_dependent_ms, n);
        self.gadget_ms = merge_mean(self.gadget_ms, other.gadget_ms, n);
        self.tessellate_ms = merge_mean(self.tessellate_ms, other.tessellate_ms, n);
        self.gpu_upload_ms = merge_mean(self.gpu_upload_ms, other.gpu_upload_ms, n);
        self.background_ms = merge_mean_opt(self.background_ms, other.background_ms, n);
        self.total_ms = merge_mean(self.total_ms, other.total_ms, n);
        self.max_total_ms = self.max_total_ms.max(other.max_total_ms);
        // Cache counters are *sums*, not means: the question a coalesced drag
        // row answers is "how many conversions did the whole drag cost", and a
        // per-tick mean of a mostly-zero counter says nothing.
        self.csg_cache.mesh_hits += other.csg_cache.mesh_hits;
        self.csg_cache.mesh_misses += other.csg_cache.mesh_misses;
        self.csg_cache.sketch_hits += other.csg_cache.sketch_hits;
        self.csg_cache.sketch_misses += other.csg_cache.sketch_misses;
        // `node_stats` and `memo` are deliberately untouched: only lightweight
        // rows coalesce and a lightweight refresh runs no evaluation pass, so
        // neither side ever carries either.
    }
}

/// Running mean: `mean` is the mean of `n - 1` samples, `sample` is the nth.
fn merge_mean(mean: f64, sample: f64, n: u32) -> f64 {
    mean + (sample - mean) / (n as f64)
}

/// [`merge_mean`] over optionals. Consecutive `Lightweight` rows — the only
/// rows that ever coalesce — carry `None` on both optional fields, so the
/// mixed arms exist only so a future caller cannot produce a nonsensical row:
/// a missing sample counts as the zero it measured.
fn merge_mean_opt(mean: Option<f64>, sample: Option<f64>, n: u32) -> Option<f64> {
    match (mean, sample) {
        (None, None) => None,
        (Some(mean), Some(sample)) => Some(merge_mean(mean, sample, n)),
        (Some(mean), None) => Some(merge_mean(mean, 0.0, n)),
        (None, Some(sample)) => Some(sample / n as f64),
    }
}

/// The session's refresh history: a bounded ring plus the un-coalesced last
/// refresh.
///
/// Both are **read, not drained** — a profile is a snapshot the UI re-renders,
/// and draining would empty the panel on every unrelated poll (D6).
///
/// The two are separate on purpose. `last` is the actual most recent refresh,
/// which is what the always-on status strip shows; `rows` is the history, in
/// which a burst of `Lightweight` ticks collapses into one counted row so a
/// drag cannot evict the `Full`/`Partial` refreshes the user wants to compare.
#[derive(Debug, Default)]
pub struct RefreshProfileHistory {
    rows: VecDeque<RefreshProfile>,
    last: Option<RefreshProfile>,
    last_node_stats: Option<Arc<EvalProfile>>,
}

impl RefreshProfileHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one refresh. Consecutive `Lightweight` refreshes fold into the
    /// trailing row instead of taking a slot of their own.
    pub fn record(&mut self, profile: RefreshProfile) {
        if profile.mode == RefreshMode::Lightweight
            && let Some(back) = self.rows.back_mut()
            && back.mode == RefreshMode::Lightweight
        {
            back.coalesce(&profile);
        } else {
            self.rows.push_back(profile.clone());
            while self.rows.len() > REFRESH_PROFILE_HISTORY_CAPACITY {
                self.rows.pop_front();
            }
        }
        if profile.node_stats.is_some() {
            self.last_node_stats = profile.node_stats.clone();
        }
        self.last = Some(profile);
    }

    /// The most recent **profiled** pass's per-node table, which is not
    /// necessarily the most recent refresh's: an unrelated lightweight tick
    /// (or a refresh taken with the profiler switched off) must not blank the
    /// panel. A profile is a snapshot, and it stays on screen until another
    /// profiled pass replaces it.
    pub fn last_node_stats(&self) -> Option<&Arc<EvalProfile>> {
        self.last_node_stats.as_ref()
    }

    /// The most recent refresh, never coalesced.
    pub fn last(&self) -> Option<&RefreshProfile> {
        self.last.as_ref()
    }

    /// The history ring, oldest first.
    pub fn rows(&self) -> impl Iterator<Item = &RefreshProfile> {
        self.rows.iter()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.last = None;
        self.last_node_stats = None;
    }
}
