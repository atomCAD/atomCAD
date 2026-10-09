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
//! Legs 4 and later are the **local phase** (`local`): `evaluate` relaxes
//! the three-leg hypotheses as parents, then searches each unbonded foot's
//! sites within `reach` of its relaxed position, level by level. It replaced
//! an earlier all-at-once engine that took one pose and relaxed every
//! combination of sites within reach of it, so it covered only the
//! orientations near that pose.
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
//! - A hypothesis is relaxed only as a candidate or as a local-phase parent
//!   (a three-leg or deeper state the next leg is searched from); a two-leg
//!   step on the way to three legs is never relaxed as a step (§4.8).
//! - **The local phase keeps positions, never structures**, and only two
//!   levels at a time; nothing of it outlives `evaluate`. A local state's
//!   geometry depends on its parent's relaxation, so its change set is
//!   deduplicated *after* relaxing (lowest strain wins), and `replay`
//!   rebuilds any row through its canonical parents.
//! - **The debug view (`debug`) stores nothing either.** An item's view is
//!   rebuilt from the tree on demand: by geometry alone when it is posed or
//!   seated, by `replay` when it is relaxed and not a kept candidate. Its
//!   "next foot" steps are a grouping of the tree's rows, not rows of their
//!   own: the tree is what the search recorded and nothing else.

pub mod config;
pub mod debug;
pub mod evaluate;
pub mod local;
pub mod plan;
pub mod setup;
pub mod tree;

pub use config::SequentialSearch;
pub use debug::{
    ChildVerdict, DebugForm, DebugItem, DebugShapes, DebugView, Marks, RowForms, SHAPE_ALPHA,
    SHAPE_COLOR, SHAPE_LEVEL, Shape, Shell, child_verdict, debug_view, find_item, item_ancestors,
    item_children, item_label, item_of_row, item_parent, item_path, needs_relaxation, next_feet,
    relaxation, root_view, row_forms, shown_row, step_legs, step_near_misses, tree_of,
};
pub use evaluate::{Candidate, RelaxedRow, SearchReport, SearchStats, evaluate, search};
pub use local::{LevelStats, Replayed, replay, replay_start};
pub use plan::{GEOMETRIC_LEGS, Hypothesis, PlanStats, SequentialPlan, plan};
pub use setup::{
    CLASH_FRACTION, Foot, LOCAL_UP_RADIUS, Leg, MIRROR_MARGIN, Mirror, NEAR_MISS_BAND, ROUNDING,
    Seating, Setup, Site, Step, THETA_STEP_DEG, collinear_axis, rest_length, triangle_quality,
};
pub use tree::{NO_ROW, NearMiss, Row, RowKind, SearchTree};
