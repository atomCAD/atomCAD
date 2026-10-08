//! The sequential chemisorption search: orientation coverage by building each
//! binding leg by leg (`design_chemisorption_sequential.md`, in the external
//! mechanosynth working folder).
//!
//! The pose fixes only where leg 1 lands (`anchor_reach`). Leg 2 is any site
//! on a spherical shell around leg 1's site, leg 3 any site on the ring where
//! the shells around both bonded sites overlap. The shells come from the
//! triangle inequality: a foot is `d` from another foot and each foot is its
//! bond length `b` from its site, so the two sites are `d ± (b₁ + b₂)` apart.
//! That is **exact at tolerance 0 for every bond direction**, so `tolerance`
//! is only the molecule's own flex. Orientations are never sampled; each site
//! choice fixes its own, and the adsorbate is placed by a Kabsch fit of its
//! bonded feet onto points a bond length above their sites (seating).
//!
//! **Phase 1 of the design: the geometric phase only** (legs 1–3). Legs 4 and
//! later (the local phase, relaxing three-leg parents and searching within
//! `reach` of the relaxed feet) are Phase 2; `PlanStats::local_phase` says
//! when one would follow. This module sits beside the original all-at-once
//! engine until the `chemisorb` node moves over (Phase 3).
//!
//! Rules that are easy to erode:
//!
//! - **The shell test is exact at tolerance 0.** It compares site spacing
//!   with foot spacing plus the bond lengths, and nothing else. Never replace
//!   the ring by a distance from a circle on the site axis: moving the foot
//!   circle onto the sites shifts it by up to a bond length, and the
//!   tolerance would silently absorb the error.
//! - **The search is site-centred.** It reads atom positions and distances;
//!   the local up is used for seating and the mirror check only, never in a
//!   test that admits or rejects a site.
//! - **The start geometry is a function of the change set** (bonds plus
//!   transfers), so dedupe by change set is exact. Seating sorts the legs
//!   before fitting, so the binding order cannot leak into it.
//! - **A transfer is fixed when its leg is added**: the moved atom goes to
//!   the nearest site with valence left, never the foot's own site, within
//!   `reach` site to site; no such site drops the leg. Two binding orders
//!   that send it to different sites are two hypotheses.
//! - **`plan` never relaxes.** The mirror check, seating and the clash check
//!   are geometry and run in `plan`.
//! - Only candidates are relaxed in the geometric phase; a two-leg step on
//!   the way to three legs is never relaxed as a step (§4.8).

pub mod config;
pub mod evaluate;
pub mod plan;
pub mod setup;
pub mod tree;

pub use config::SequentialSearch;
pub use evaluate::{Candidate, RelaxedRow, SearchReport, SearchStats, evaluate, search};
pub use plan::{GEOMETRIC_LEGS, Hypothesis, PlanStats, SequentialPlan, plan};
pub use setup::{
    CLASH_FRACTION, Foot, LOCAL_UP_RADIUS, Leg, MIRROR_MARGIN, Mirror, NEAR_MISS_BAND, ROUNDING,
    Seating, Setup, Site, Step, THETA_STEP_DEG, collinear_axis, rest_length, triangle_quality,
};
pub use tree::{NO_ROW, NearMiss, Row, RowKind, SearchTree};
