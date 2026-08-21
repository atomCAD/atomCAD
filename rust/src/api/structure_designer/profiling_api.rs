//! Flutter-facing surface of the always-on refresh phase breakdown — Phase 1
//! of `doc/design_eval_profiling.md` (D6, D8a).
//!
//! The authoritative types live in
//! `atomcad_structure_designer::refresh_profile`; these are their Dart-facing
//! twins, per the FRB rule that a `pub use` re-export from a lower crate is
//! invisible to codegen (see `rust/AGENTS.md`). Adding this module to
//! `flutter_rust_bridge.yaml`'s `rust_input` is part of the same change — a
//! missing entry is not a build error, it silently produces an opaque handle
//! on the Dart side.
//!
//! Both getters **read without draining** (D6): a profile is a snapshot the UI
//! re-renders, so polling it must not empty it.

use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance,
    with_mut_cad_instance_or,
};
use atomcad_structure_designer::evaluator::eval_profiler::{
    EvalProfile, NodeProfileRecord, NodeTypeProfileRecord, SelfCheckViolation,
};
use atomcad_structure_designer::refresh_profile::{CsgCacheDelta, RefreshProfile};
use atomcad_structure_designer::structure_designer_changes::RefreshMode;

/// Nanoseconds to milliseconds. The per-node profiler accumulates in `u64`
/// nanoseconds (no float rounding across ~10^3 additions) and reports in the
/// same millisecond unit as every other number in the panel.
fn ns_to_ms(ns: u64) -> f64 {
    ns as f64 / 1_000_000.0
}

/// Flutter-facing mirror of [`RefreshMode`]. Which of the three refresh paths
/// produced a profile row — the tag that makes a 40 ms drag tick and a 1.8 s
/// node activation distinguishable at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum APIRefreshMode {
    Lightweight,
    Partial,
    Full,
}

impl From<RefreshMode> for APIRefreshMode {
    fn from(mode: RefreshMode) -> Self {
        match mode {
            RefreshMode::Lightweight => APIRefreshMode::Lightweight,
            RefreshMode::Partial => APIRefreshMode::Partial,
            RefreshMode::Full => APIRefreshMode::Full,
        }
    }
}

/// Flutter-facing mirror of [`RefreshProfile`]: one refresh broken into its
/// phases, all in milliseconds.
///
/// `evalMs` and `backgroundMs` are `null` — never `0.0` — when the refresh ran
/// no such phase at all. A lightweight refresh enters no evaluation pass and
/// skips the background mesh rebuild; rendering either as `0.00` would read as
/// "that phase is free", which is precisely the wrong conclusion (D6/D8a).
#[derive(Debug, Clone)]
pub struct APIRefreshProfile {
    pub mode: APIRefreshMode,
    /// Evaluation of the displayed roots. `None` on a lightweight refresh.
    pub eval_ms: Option<f64>,
    /// Scene-dependent node data refresh.
    pub scene_dependent_ms: f64,
    /// Gadget rebuild + tessellatable.
    pub gadget_ms: f64,
    /// Scene tessellation.
    pub tessellate_ms: f64,
    /// CPU-side mesh upload to the renderer.
    pub gpu_upload_ms: f64,
    /// Background coordinate-system mesh rebuild. `None` when skipped.
    pub background_ms: Option<f64>,
    /// Wall time of the whole Rust-side refresh. The Dart-side stopwatch
    /// brackets this one, and the difference is the FFI overhead (D7).
    pub total_ms: f64,
    /// How many refreshes this row represents — always 1 for the "last
    /// refresh" reading, and > 1 for a history row that coalesced a burst of
    /// consecutive lightweight ticks (in which case the timings above are
    /// means over the burst).
    pub count: u32,
    /// Worst `total_ms` among the refreshes coalesced into this row.
    pub max_total_ms: f64,
    /// CSG conversion-cache activity this refresh caused (D12). Shown *beside*
    /// the phase totals, never folded into node time — the time itself is
    /// charged to the node that triggered the conversion, and what these
    /// counters add is why two otherwise identical refreshes differ.
    pub csg_cache: APICsgCacheCounts,
    /// Whether this refresh's evaluation pass was profiled per node. The table
    /// itself is fetched separately by [`get_last_eval_profile`] — a history
    /// row carries the flag only, so listing 20 rows does not marshal 20
    /// tables across the FFI boundary.
    pub has_node_stats: bool,
}

/// Flutter-facing mirror of [`CsgCacheDelta`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct APICsgCacheCounts {
    pub mesh_hits: u64,
    pub mesh_misses: u64,
    pub sketch_hits: u64,
    pub sketch_misses: u64,
}

impl From<CsgCacheDelta> for APICsgCacheCounts {
    fn from(delta: CsgCacheDelta) -> Self {
        Self {
            mesh_hits: delta.mesh_hits,
            mesh_misses: delta.mesh_misses,
            sketch_hits: delta.sketch_hits,
            sketch_misses: delta.sketch_misses,
        }
    }
}

impl From<&RefreshProfile> for APIRefreshProfile {
    fn from(profile: &RefreshProfile) -> Self {
        Self {
            mode: profile.mode.into(),
            eval_ms: profile.eval_ms,
            scene_dependent_ms: profile.scene_dependent_ms,
            gadget_ms: profile.gadget_ms,
            tessellate_ms: profile.tessellate_ms,
            gpu_upload_ms: profile.gpu_upload_ms,
            background_ms: profile.background_ms,
            total_ms: profile.total_ms,
            count: profile.count,
            max_total_ms: profile.max_total_ms,
            csg_cache: profile.csg_cache.into(),
            has_node_stats: profile.node_stats.is_some(),
        }
    }
}

/// One row of the profiler panel's **By node** table: an aggregate over every
/// evaluation of one node in one home network during one pass.
///
/// `lookups` and `wasted_ms` are Phase 3 fields that sit **alongside**
/// `evaluations` rather than re-interpreting it (D8b). Before the evaluation
/// memo lands `lookups == evaluations` exactly; afterwards the difference is the
/// memo's hit count, and a column that quietly changed meaning at that point is
/// how a regression would hide.
#[derive(Debug, Clone)]
pub struct APINodeProfileRecord {
    /// Human-readable address: `"main/fold#12/add#3 (mysum)"`.
    pub label: String,
    pub node_type_name: String,
    /// Click-to-jump target — the network the node lives in. For a node inside
    /// a custom network this is that network, not the one being profiled.
    pub host_network: String,
    /// Click-to-jump target — the HOF-body chain within `host_network`.
    pub scope_path: Vec<u64>,
    /// Click-to-jump target — the node.
    pub node_id: u64,
    /// False only for a lazily-evaluated HOF body whose address could not be
    /// pinned down (see `NodeLocation::navigable`). Such a row is still
    /// measured and still rolls up by type; the panel just renders it
    /// non-clickable rather than offering a jump that lands nowhere.
    pub navigable: bool,
    pub evaluations: u64,
    /// Times a result for this node was **requested**. Equals `evaluations`
    /// until the memo lands.
    pub lookups: u64,
    /// How many distinct evaluation environments those requests spanned (D9).
    /// The denominator that makes the redundancy factor honest: a `map` body
    /// node run once per element over 3 elements has 3 distinct environments
    /// and is not redundant at all.
    pub distinct_envs: u64,
    /// `lookups / distinct_envs` — the per-node redundancy factor. `1.0` means
    /// every request was a genuinely different environment.
    pub redundancy_factor: f64,
    /// Self time a perfect memo would avoid, in ms. **The actionable column.**
    pub wasted_ms: f64,
    /// The node produced an iterator, which `doc/design_eval_memoization.md` D4
    /// deliberately does not cache — so its `wasted_ms` is not an available
    /// saving.
    pub produced_iterator: bool,
    /// The re-entrancy backstop fired on this node (a wire cycle escaped
    /// validation); D9 there forbids memoizing under it, so again `wasted_ms`
    /// is not collectable.
    pub under_reentrancy_backstop: bool,
    /// A custom-network instance requested through `evaluate`'s single-pin arm,
    /// which D2 forbids the memo from inserting from. Such a row shows
    /// `evaluations == lookups` permanently with the memo working perfectly;
    /// unflagged, every subnetwork in every design would read as a memo bug.
    pub subnetwork: bool,
    /// The memo held this node's entry earlier in the pass and the LRU dropped
    /// it, so the work was redone (D6). Distinguishes memory pressure from a
    /// correctness bug.
    pub evicted: bool,
    /// Time in this node's own `eval`, with its dependencies' time subtracted.
    pub self_ms: f64,
    /// Wall time including everything this node pulled. A custom-node instance
    /// legitimately shows ~zero `self_ms` against a large `total_ms`: it
    /// delegates to its network's return node.
    pub total_ms: f64,
}

impl From<&NodeProfileRecord> for APINodeProfileRecord {
    fn from(record: &NodeProfileRecord) -> Self {
        Self {
            label: record.location.label.clone(),
            node_type_name: record.location.node_type_name.clone(),
            host_network: record.location.host_network.clone(),
            scope_path: record.location.scope_path.clone(),
            node_id: record.location.node_id,
            navigable: record.location.navigable,
            evaluations: record.evaluations,
            lookups: record.lookups,
            distinct_envs: record.distinct_envs,
            redundancy_factor: record.redundancy_factor(),
            wasted_ms: ns_to_ms(record.wasted_ns()),
            produced_iterator: record.flags.produced_iterator,
            under_reentrancy_backstop: record.flags.under_reentrancy_backstop,
            subnetwork: record.flags.subnetwork,
            evicted: record.flags.evicted,
            self_ms: ns_to_ms(record.self_ns),
            total_ms: ns_to_ms(record.total_ns),
        }
    }
}

/// One row of the **By node type** table — a roll-up of every
/// [`APINodeProfileRecord`] sharing a type name. Both tables come from one map,
/// so their totals agree by construction.
#[derive(Debug, Clone)]
pub struct APINodeTypeProfileRecord {
    pub node_type_name: String,
    /// How many distinct nodes of this type were evaluated.
    pub nodes: u64,
    pub evaluations: u64,
    pub self_ms: f64,
    pub total_ms: f64,
}

impl From<&NodeTypeProfileRecord> for APINodeTypeProfileRecord {
    fn from(record: &NodeTypeProfileRecord) -> Self {
        Self {
            node_type_name: record.node_type_name.clone(),
            nodes: record.nodes,
            evaluations: record.evaluations,
            self_ms: ns_to_ms(record.self_ns),
            total_ms: ns_to_ms(record.total_ns),
        }
    }
}

/// The per-node breakdown of one profiled evaluation pass.
#[derive(Debug, Clone)]
pub struct APIEvalProfile {
    pub total_evaluations: u64,
    /// Summed self time over every record — the figure to compare against the
    /// refresh's `evalMs` phase.
    pub total_self_ms: f64,
    /// Total result requests (Phase 3). Equal to `total_evaluations` until the
    /// memo lands.
    pub total_lookups: u64,
    /// Distinct evaluation environments the pass visited — and, since the key
    /// carries the node and `decorate` too, the **number of entries a perfect
    /// memo would hold at peak**. The memo's memory question, measured before a
    /// line of it is written.
    pub total_distinct_envs: u64,
    /// `total_lookups / total_distinct_envs`. Shown next to — never instead of
    /// — the per-node breakdown: a pass that is globally 2.5x can be 11x on
    /// `materialize` and 1.0 on body nodes, and only the breakdown says where a
    /// memo would pay (D10).
    pub redundancy_factor: f64,
    /// Summed `wasted_ms` over the rows the memo would actually cache — rows
    /// flagged uncacheable are excluded rather than inflating the projection.
    pub projected_saving_ms: f64,
    /// Environment tracking stopped at its ceiling, so `total_distinct_envs` is
    /// a floor and the redundancy numbers are upper bounds. Reported rather than
    /// silently capped.
    pub envs_truncated: bool,
    /// Whether the D11 equal-key/equal-result self-check ran for this pass.
    pub self_check_ran: bool,
    /// Self-check sampling hit its ceiling, so a clean result covers only the
    /// environments seen before that point rather than the whole pass.
    pub self_check_truncated: bool,
    /// What it found. Empty is the expected outcome; a non-empty list means the
    /// environment key is missing an input — a wrong number now, and a wrong
    /// result once the memo keys on it.
    pub self_check_violations: Vec<APISelfCheckViolation>,
    pub by_node: Vec<APINodeProfileRecord>,
    pub by_node_type: Vec<APINodeTypeProfileRecord>,
}

/// Flutter-facing mirror of [`SelfCheckViolation`]: two evaluations that shared
/// an environment key produced different results.
#[derive(Debug, Clone)]
pub struct APISelfCheckViolation {
    pub label: String,
    pub first: String,
    pub later: String,
}

impl From<&SelfCheckViolation> for APISelfCheckViolation {
    fn from(violation: &SelfCheckViolation) -> Self {
        Self {
            label: violation.label.clone(),
            first: violation.first.clone(),
            later: violation.later.clone(),
        }
    }
}

impl From<&EvalProfile> for APIEvalProfile {
    fn from(profile: &EvalProfile) -> Self {
        Self {
            total_evaluations: profile.total_evaluations(),
            total_self_ms: ns_to_ms(profile.total_self_ns()),
            total_lookups: profile.total_lookups(),
            total_distinct_envs: profile.total_distinct_envs(),
            redundancy_factor: profile.redundancy_factor(),
            projected_saving_ms: ns_to_ms(profile.projected_saving_ns()),
            envs_truncated: profile.envs_truncated(),
            self_check_ran: profile.self_check_ran(),
            self_check_truncated: profile.self_check_truncated(),
            self_check_violations: profile
                .self_check_violations()
                .iter()
                .map(APISelfCheckViolation::from)
                .collect(),
            by_node: profile
                .records()
                .iter()
                .map(APINodeProfileRecord::from)
                .collect(),
            by_node_type: profile
                .by_node_type()
                .iter()
                .map(APINodeTypeProfileRecord::from)
                .collect(),
        }
    }
}

/// The most recent refresh, never coalesced — what the always-on status strip
/// shows. `None` before the first refresh of the session.
#[flutter_rust_bridge::frb(sync)]
pub fn get_last_refresh_profile() -> Option<APIRefreshProfile> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .refresh_profiles
                    .last()
                    .map(APIRefreshProfile::from)
            },
            None,
        )
    }
}

/// The bounded refresh history, oldest first. Consecutive lightweight rows are
/// already coalesced, so a gadget drag appears as one row with a count rather
/// than flushing the interesting refreshes out of the ring (D6).
#[flutter_rust_bridge::frb(sync)]
pub fn get_refresh_profile_history() -> Vec<APIRefreshProfile> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .refresh_profiles
                    .rows()
                    .map(APIRefreshProfile::from)
                    .collect()
            },
            Vec::new(),
        )
    }
}

/// Whether the opt-in per-node profiler is currently armed. Session state, not
/// a persisted preference: leaving it on across sessions would silently skew
/// later measurements (D2).
#[flutter_rust_bridge::frb(sync)]
pub fn get_eval_profiling_enabled() -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.eval_profiling_enabled,
            false,
        )
    }
}

/// Arms or disarms the per-node profiler. Takes effect on the next evaluation
/// pass; the previously collected table stays readable until another profiled
/// pass replaces it.
#[flutter_rust_bridge::frb(sync)]
pub fn set_eval_profiling_enabled(enabled: bool) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.structure_designer.eval_profiling_enabled = enabled;
        });
    }
}

/// Whether the self-check is armed. Needs per-node profiling on as well — the
/// check lives in the profile.
#[flutter_rust_bridge::frb(sync)]
pub fn get_eval_self_check_enabled() -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.eval_self_check_enabled,
            false,
        )
    }
}

/// Arms or disarms the self-check. Session state like the profiler toggle, and
/// for the same reason (D2): a check left on across sessions would quietly tax
/// every later measurement.
///
/// **Returns `false` when the request was refused** because the evaluation memo
/// is on (`doc/design_eval_memoization.md` D10's hard gate). Once a memo serves
/// the second request *from* the first result there is no second computation to
/// compare and the check passes vacuously, so arming it under a memo would
/// report a green that means nothing. The UI turns a `false` into an
/// explanation pointing at the memo switch rather than a silent no-op.
#[flutter_rust_bridge::frb(sync)]
pub fn set_eval_self_check_enabled(enabled: bool) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .try_set_eval_self_check_enabled(enabled)
            },
            false,
        )
    }
}

/// Whether the per-pass evaluation memo is on. **Defaults to `true`** — it is
/// the product's behaviour, unlike the profiler's opt-in toggle (D10).
#[flutter_rust_bridge::frb(sync)]
pub fn get_eval_memo_enabled() -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.eval_memo_enabled,
            true,
        )
    }
}

/// Switches the evaluation memo on or off and forces one **full** refresh, so
/// the effect is visible immediately and the A/B comparison is between two
/// comparable passes (D10).
///
/// The refresh is not a convenience: a per-pass memo only shows its effect on
/// the *next* pass, and comparing a memo-off partial against a memo-on full
/// measures nothing.
///
/// Returns `true` when an armed self-check had to be disarmed to let the memo
/// run, so the panel can say so rather than leaving the user's diagnostic
/// silently switched off.
#[flutter_rust_bridge::frb(sync)]
pub fn set_eval_memo_enabled(enabled: bool) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let disarmed = cad_instance
                    .structure_designer
                    .set_eval_memo_enabled(enabled);
                cad_instance.structure_designer.mark_full_refresh();
                refresh_structure_designer_auto(cad_instance);
                disarmed
            },
            false,
        )
    }
}

/// The most recent **profiled** pass's per-node table — read, never drained
/// (D6). `None` until a pass has run with profiling armed.
///
/// Deliberately not "the last refresh's": an unrelated lightweight tick, or a
/// refresh taken after the toggle went off, must not blank the panel.
#[flutter_rust_bridge::frb(sync)]
pub fn get_last_eval_profile() -> Option<APIEvalProfile> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .refresh_profiles
                    .last_node_stats()
                    .map(|profile| APIEvalProfile::from(profile.as_ref()))
            },
            None,
        )
    }
}

/// Arms the profiler and forces one **full** refresh, so successive readings
/// are comparable (D8b).
///
/// Without it the panel shows whatever partial refresh happened to run last,
/// and two measurements taken a minute apart measure different amounts of
/// work. The toggle is left **on** afterwards rather than restored, so the
/// *View* menu keeps telling the truth about what the next refresh will do.
#[flutter_rust_bridge::frb(sync)]
pub fn profile_full_refresh() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.structure_designer.eval_profiling_enabled = true;
            cad_instance.structure_designer.mark_full_refresh();
            refresh_structure_designer_auto(cad_instance);
        });
    }
}
