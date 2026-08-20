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

use crate::api::api_common::with_cad_instance_or;
use atomcad_structure_designer::refresh_profile::RefreshProfile;
use atomcad_structure_designer::structure_designer_changes::RefreshMode;

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
