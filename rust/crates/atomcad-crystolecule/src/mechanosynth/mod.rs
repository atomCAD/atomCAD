//! Replaying mechanosynthetic build sequences.
//!
//! A **build script** is an ordered list of **steps**; each step names an
//! **operation** from a library and gives a rigid transform placing the
//! operation's local frame into workpiece coordinates. An operation is a
//! before/after pair of small atom lists — comparing them by pattern id *is*
//! the rewrite; there is no separate diff syntax. Replaying the first `k` steps
//! onto a base structure yields the workpiece after `k` reactions, which is
//! what makes scrubbing `k` show a structure being built.
//!
//! Three properties are load-bearing and easy to erode:
//!
//! - **Coordinates, not graphs.** Matching is nearest-atom-within-tolerance on
//!   position and element. No subgraph isomorphism, no chemical perception.
//!   What the coordinates find, the pattern's **bonds** and **bond counts**
//!   then verify: within a pattern the bond list is closed-world, and a
//!   `before` atom may state its own bond count as `deg`. Neither searches; both
//!   are O(1) checks on atoms already found. See
//!   `doc/design_mechanosynth_pattern_checks.md`.
//! - **Ideal geometry.** Added atoms land exactly where the operation says.
//!   Nothing here relaxes anything, so coordinates stay ideal, tolerances stay
//!   tight, and every intermediate state is deterministic. Wire `relax`
//!   downstream if a settled geometry is wanted.
//! - **Files are machine-written.** Both JSON files come from generators that
//!   already know every coordinate; this module only verifies and applies.
//!   Human ergonomics is not a design driver here.
//!
//! Independent of the `apply_diff` / `atom_composediff` machinery on purpose:
//! that solves the more general problem of anchoring arbitrary diffs across
//! bases. Nothing is shared beyond [`AtomicStructure`].
//!
//! Design doc: `design_mechanosynth_node.md` (external, in the mechanosynth
//! working folder).
//!
//! [`AtomicStructure`]: crate::atomic_structure::AtomicStructure

pub mod apply;
pub mod compare;
mod fit;
pub mod parse;
pub mod place;
pub mod pose;
pub mod scene;
pub mod schema;
pub mod trajectory;

pub use apply::{
    Contact, HighlightTags, StepEffect, apply_step, describe_nearest, replay, resolve_tolerance,
    step_contact, steps_applied,
};
pub use compare::{Mismatch, compare_structures, describe_mismatches};
pub use parse::{
    load_build_script, load_library, parse_build_script, parse_library, validate_script_ops,
};
pub use place::{
    Applicability, Candidate, EXACT_FIT_RESIDUAL, GhostAtom, GhostBond, GhostBondKind, GhostKind,
    NEAR_MISS_FACTOR, PlaceStats, RESIDUAL_RANK_EPSILON, Refusal, ToolReadiness, applicable_ops,
    applicable_ops_where, place, place_with_stats, preview_atoms, preview_bonds,
};
pub use pose::{ToolPose, tool_pose};
pub use scene::{
    LandingPlan, Participant, Scene, SceneEffect, StepPlan, ToolBinding, apply_step_in_scene,
    build_scene, event_indices, match_step_in_scene, replay_scene, replay_scene_partial,
    replay_steps,
};
pub use schema::{
    APEX_FRAME_TAG, BUILD_FORMAT, BondMismatch, BuildScript, CLASH_BLOCK, CLASH_SEARCH_RADIUS,
    CLOSE_PAIR_WARNING_FACTOR, Clash, DEFAULT_ANCHORS, DEFAULT_DURATION, DEFAULT_TOLERANCE,
    DegreeMismatch, FRAME_COPLANAR_EPSILON, FrameAtom, LIBRARY_FORMAT, MAX_PATTERN_DEGREE,
    MechanosynthError, Method, NO_LAYER, NO_SITE, NoMatch, ORIGIN_PATTERN_ATOM_ID, OpLibrary,
    Operation, PATTERN_POSITION_EPSILON, Pattern, PatternAtom, PatternBond, PatternElement,
    Reaction, Step, ToolSide, ToolType,
};
pub use trajectory::{
    Approach, CAGE_CYLINDER_LENGTH, CAGE_MERIDIANS, CLEAR_MARGIN, Envelope, Landing, Leg,
    PATH_SAMPLE, PATH_SAMPLE_ANGLE, PathContact, PathScan, Pose, REACTION, REACTION_DEPARTURE,
    REACTION_LANDING, Runs, STANDOFF_HEIGHT, SWEEP_DIRECTIONS, SWEEP_REFINEMENTS, ToolMotion,
    Visit, apply_tool_pose, approach_direction, cage_apex, obstacles_for, plan_landing,
    playable_steps, reaction_pose, replay_scene_at, runs, standoff_pose, sweep_directions,
    tool_envelope_cages,
};
