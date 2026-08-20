//! Phase 1 of `doc/design_eval_profiling.md`: refresh phase timing.
//!
//! **No wall-clock assertions live here.** "Eval took under 200 ms" is flaky
//! on any machine and worthless on a loaded one. What is asserted is
//! *structure*: which sub-phases a refresh mode reports at all, that no
//! measurement is negative, that the parts never exceed the whole, and that
//! the history ring keeps and coalesces the rows the design says it should.
//!
//! The full `RefreshProfile` is assembled one layer up, in
//! `rust/src/api/api_common.rs`, because that is the only place that sees
//! tessellation and GPU upload as well. That layer cannot be exercised from a
//! test — it needs a live `CADInstance` with a wgpu renderer — so the
//! assembly is covered here in two halves: the sub-phases
//! `StructureDesigner::refresh` really returns, and the row/ring arithmetic
//! the API layer feeds them into.

use atomcad_structure_designer::refresh_profile::{
    REFRESH_PROFILE_HISTORY_CAPACITY, RefreshProfile, RefreshProfileHistory, RefreshSubPhases,
};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::structure_designer_changes::{
    RefreshMode, StructureDesignerChanges,
};

// ============================================================================
// Helpers
// ============================================================================

/// A designer with one active, empty network — enough for every refresh mode
/// to run its real code path.
fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    designer
}

/// A synthetic row, so the ring tests do not depend on how long anything
/// actually took.
fn row(mode: RefreshMode, total_ms: f64) -> RefreshProfile {
    let sub_phases = RefreshSubPhases {
        eval_ms: match mode {
            RefreshMode::Lightweight => None,
            _ => Some(total_ms * 0.5),
        },
        scene_dependent_ms: total_ms * 0.1,
        gadget_ms: total_ms * 0.1,
        ..Default::default()
    };
    RefreshProfile::new(
        mode,
        sub_phases,
        total_ms * 0.1,
        total_ms * 0.05,
        match mode {
            RefreshMode::Lightweight => None,
            _ => Some(total_ms * 0.05),
        },
        total_ms,
    )
}

/// Every timing a sub-phase set reports is finite and non-negative.
fn assert_sub_phases_sane(sub_phases: &RefreshSubPhases, context: &str) {
    if let Some(eval_ms) = sub_phases.eval_ms {
        assert!(
            eval_ms >= 0.0 && eval_ms.is_finite(),
            "{context}: eval_ms must be a non-negative finite number, got {eval_ms}"
        );
    }
    assert!(
        sub_phases.scene_dependent_ms >= 0.0 && sub_phases.scene_dependent_ms.is_finite(),
        "{context}: scene_dependent_ms must be non-negative and finite, got {}",
        sub_phases.scene_dependent_ms
    );
    assert!(
        sub_phases.gadget_ms >= 0.0 && sub_phases.gadget_ms.is_finite(),
        "{context}: gadget_ms must be non-negative and finite, got {}",
        sub_phases.gadget_ms
    );
}

// ============================================================================
// Sub-phases per refresh mode
// ============================================================================

/// A full and a partial refresh each report an evaluation phase; a lightweight
/// refresh reports **none at all**.
///
/// The `None` is the point, not an implementation detail: a lightweight
/// refresh never enters `with_eval_context` (the other `with_eval_context`
/// callers are the CLI and Execute paths, not refreshes), so reporting a small
/// number there would let the strip and the panel misread a drag tick as cheap
/// evaluation (D6).
#[test]
fn eval_sub_phase_is_absent_only_for_lightweight_refreshes() {
    let mut designer = setup_designer();

    let full = designer.refresh(&StructureDesignerChanges::full());
    assert!(
        full.eval_ms.is_some(),
        "a full refresh runs an evaluation pass and must report it"
    );
    assert_sub_phases_sane(&full, "full");

    let partial = designer.refresh(&StructureDesignerChanges::new());
    assert!(
        partial.eval_ms.is_some(),
        "a partial refresh has an evaluation phase even when it finds no work; \
         it must report Some(0.0), not None"
    );
    assert_sub_phases_sane(&partial, "partial");

    let lightweight = designer.refresh(&StructureDesignerChanges::lightweight());
    assert_eq!(
        lightweight.eval_ms, None,
        "a lightweight refresh runs no evaluation pass at all — it must report \
         None, never a small number that reads as free evaluation"
    );
    assert_sub_phases_sane(&lightweight, "lightweight");
}

/// A refresh with no active network bails out before doing anything, and says
/// so rather than reporting an evaluation phase that never ran.
#[test]
fn refresh_without_active_network_reports_nothing_ran() {
    let mut designer = StructureDesigner::new();

    let sub_phases = designer.refresh(&StructureDesignerChanges::full());

    assert_eq!(sub_phases, RefreshSubPhases::nothing_ran());
    assert_eq!(sub_phases.eval_ms, None);
}

/// The sub-phases a real refresh reports never sum past the wall time of the
/// call that produced them. A relation between two recorded numbers, not a
/// threshold — stable on any machine.
#[test]
fn sub_phases_never_exceed_the_refresh_that_produced_them() {
    let mut designer = setup_designer();
    // Warm the paths once so the measured call isn't dominated by first-touch
    // lazy initialization. (The assertion below holds either way; this just
    // keeps the numbers meaningful when a failure has to be read.)
    designer.refresh(&StructureDesignerChanges::full());

    for changes in [
        StructureDesignerChanges::full(),
        StructureDesignerChanges::new(),
        StructureDesignerChanges::lightweight(),
    ] {
        let mode = changes.mode;
        let start = std::time::Instant::now();
        let sub_phases = designer.refresh(&changes);
        let wall_ms = start.elapsed().as_secs_f64() * 1000.0;

        assert_sub_phases_sane(&sub_phases, &format!("{mode:?}"));

        // Assembled the way the API layer assembles it, minus the phases only
        // that layer can see (they would only add to `total_ms`).
        let profile = RefreshProfile::new(mode, sub_phases, 0.0, 0.0, None, wall_ms);
        assert!(
            profile.attributed_ms() <= profile.total_ms,
            "{mode:?}: attributed sub-phases ({}) exceeded the refresh total ({})",
            profile.attributed_ms(),
            profile.total_ms
        );
    }
}

// ============================================================================
// History ring (D6)
// ============================================================================

/// The ring keeps the last `REFRESH_PROFILE_HISTORY_CAPACITY` rows and drops
/// the oldest — 25 refreshes in, the last 20 out.
#[test]
fn ring_retains_only_the_most_recent_rows() {
    let mut history = RefreshProfileHistory::new();

    for i in 0..25 {
        history.record(row(RefreshMode::Full, i as f64));
    }

    assert_eq!(history.len(), REFRESH_PROFILE_HISTORY_CAPACITY);
    let totals: Vec<f64> = history.rows().map(|row| row.total_ms).collect();
    assert_eq!(
        totals,
        (5..25).map(|i| i as f64).collect::<Vec<f64>>(),
        "the ring must drop the oldest rows, keeping the newest 20"
    );
}

/// A burst of lightweight ticks — one gadget drag — collapses into a single
/// row carrying the burst count, and does not evict the `Full`/`Partial` rows
/// around it. Without this, a drag flushes the whole history of the
/// interesting refreshes before the user can open the panel.
#[test]
fn lightweight_burst_coalesces_into_one_counted_row() {
    let mut history = RefreshProfileHistory::new();

    history.record(row(RefreshMode::Full, 1000.0));
    history.record(row(RefreshMode::Partial, 500.0));
    for _ in 0..200 {
        history.record(row(RefreshMode::Lightweight, 40.0));
    }
    history.record(row(RefreshMode::Partial, 500.0));

    let modes: Vec<RefreshMode> = history.rows().map(|row| row.mode).collect();
    assert_eq!(
        modes,
        vec![
            RefreshMode::Full,
            RefreshMode::Partial,
            RefreshMode::Lightweight,
            RefreshMode::Partial,
        ],
        "200 lightweight ticks must occupy exactly one row and evict nothing"
    );

    let coalesced = history.rows().nth(2).expect("the coalesced row");
    assert_eq!(coalesced.count, 200, "the row must carry the burst count");
    assert_eq!(
        coalesced.eval_ms, None,
        "coalescing lightweight rows must not invent an evaluation phase"
    );
    // Identical samples: the mean is the sample, and so is the max.
    assert!((coalesced.total_ms - 40.0).abs() < 1e-9);
    assert!((coalesced.max_total_ms - 40.0).abs() < 1e-9);
}

/// Coalescing means *mean and max*, not sum: a fast tick after a slow one
/// leaves a mean between them and a max equal to the slow one.
#[test]
fn coalesced_row_carries_mean_and_max() {
    let mut history = RefreshProfileHistory::new();

    history.record(row(RefreshMode::Lightweight, 100.0));
    history.record(row(RefreshMode::Lightweight, 50.0));
    history.record(row(RefreshMode::Lightweight, 30.0));

    let coalesced = history.rows().next().expect("the coalesced row");
    assert_eq!(coalesced.count, 3);
    assert!(
        (coalesced.total_ms - 60.0).abs() < 1e-9,
        "expected the mean of 100/50/30, got {}",
        coalesced.total_ms
    );
    assert!((coalesced.max_total_ms - 100.0).abs() < 1e-9);
    assert!(
        coalesced.attributed_ms() <= coalesced.total_ms,
        "a coalesced row must stay additive: means of parts sum to at most the \
         mean of the totals"
    );
}

/// Non-consecutive lightweight rows do not merge — only a run does.
#[test]
fn lightweight_rows_split_by_another_mode_do_not_merge() {
    let mut history = RefreshProfileHistory::new();

    history.record(row(RefreshMode::Lightweight, 40.0));
    history.record(row(RefreshMode::Partial, 500.0));
    history.record(row(RefreshMode::Lightweight, 40.0));

    let counts: Vec<u32> = history.rows().map(|row| row.count).collect();
    assert_eq!(counts, vec![1, 1, 1]);
    assert_eq!(history.len(), 3);
}

/// The "last refresh" reading is the actual last refresh, never a coalesced
/// aggregate — that is what the always-on strip shows.
#[test]
fn last_is_the_uncoalesced_most_recent_refresh() {
    let mut history = RefreshProfileHistory::new();

    history.record(row(RefreshMode::Lightweight, 100.0));
    history.record(row(RefreshMode::Lightweight, 20.0));

    let last = history.last().expect("a refresh was recorded");
    assert_eq!(last.count, 1, "the last reading must not be an aggregate");
    assert!((last.total_ms - 20.0).abs() < 1e-9);
    // …while the ring row it folded into is the aggregate.
    assert_eq!(history.rows().next().unwrap().count, 2);
}

/// The readers read; they do not drain. A panel that polls must not empty the
/// history it is rendering.
#[test]
fn reading_the_history_does_not_drain_it() {
    let mut history = RefreshProfileHistory::new();
    history.record(row(RefreshMode::Full, 1000.0));
    history.record(row(RefreshMode::Partial, 500.0));

    let first_last = history.last().cloned();
    let first_rows: Vec<RefreshProfile> = history.rows().cloned().collect();
    let second_last = history.last().cloned();
    let second_rows: Vec<RefreshProfile> = history.rows().cloned().collect();

    assert_eq!(first_last, second_last);
    assert_eq!(first_rows, second_rows);
    assert_eq!(history.len(), 2);
}

/// An empty history reports nothing rather than a zeroed row that would render
/// as a real (and very fast) refresh.
#[test]
fn empty_history_has_no_last_reading() {
    let history = RefreshProfileHistory::new();
    assert!(history.is_empty());
    assert!(history.last().is_none());
}

/// A refresh driven through the designer lands in its own history — the field
/// the API layer records into and the FFI getters read from is wired up.
#[test]
fn recorded_profiles_land_on_the_designer() {
    let mut designer = setup_designer();

    let sub_phases = designer.refresh(&StructureDesignerChanges::full());
    designer.record_refresh_profile(RefreshProfile::new(
        RefreshMode::Full,
        sub_phases,
        0.0,
        0.0,
        None,
        1.0,
    ));

    let last = designer
        .refresh_profiles
        .last()
        .expect("the recorded profile");
    assert_eq!(last.mode, RefreshMode::Full);
    assert!(last.eval_ms.is_some());
}
