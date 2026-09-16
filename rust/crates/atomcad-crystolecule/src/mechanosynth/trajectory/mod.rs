//! Tool trajectories: where a tool is at every point of a step, and whether it
//! could get there.
//!
//! Milestone 1 made a step a **discrete change** — the scene before it and the
//! scene after it, and nothing in between, with every tool at its parked pose.
//! This module makes a step a **process in time**: every step owns a unit
//! interval of *step time*, `time ∈ [0, 1]`, and the scene is defined at every
//! point of it.
//!
//! Two layers, with one rule between them — **the engine measures, the generator
//! refuses, the node reports**:
//!
//! - the **feasibility** layer ([`envelope`], [`landing`]) holds the envelope,
//!   the sweep, the obstacles, the landing and its verdict, and the containment
//!   check. It never fails a *step*; it does fail a *binding*, with
//!   `ToolOutsideEnvelope`.
//! - the **presentation** layer ([`runs`], [`path`]) holds runs, poses, flights,
//!   the hover, the scan and the pose at a step time. It cannot fail at all —
//!   the scan and a blocked approach are reports.
//!
//! The feasibility layer is reached by *applying a step*
//! ([`apply_step_in_scene`](crate::mechanosynth::apply_step_in_scene) returns
//! its landing on the effect), so a replayer, an editor's block replay and a
//! sequence generator all get the same answer from the same code — and only the
//! generator turns a blocked landing into a refusal.
//!
//! Design doc: `doc/design_mechanosynth_trajectory.md`.

pub mod envelope;
pub mod landing;
pub mod path;
pub mod runs;

pub use envelope::{
    Approach, CLEAR_MARGIN, Envelope, SWEEP_DIRECTIONS, SWEEP_REFINEMENTS, approach_direction,
    sweep_directions,
};
pub use landing::{Landing, MIN_STANDOFF, STANDOFF_TILT_CAP, obstacles_for, plan_landing};
pub use path::{
    PATH_SAMPLE, PATH_SAMPLE_ANGLE, PathContact, PathScan, Pose, REACTION, REACTION_DEPARTURE,
    REACTION_LANDING, ToolMotion, Visit, apply_tool_pose, arriving_pose, reaction_pose,
    replay_scene_at, standoff_pose,
};
pub use runs::{Runs, runs};

pub(super) use landing::check_containment;
