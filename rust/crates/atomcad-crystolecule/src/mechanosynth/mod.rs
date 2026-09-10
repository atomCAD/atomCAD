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
//!   position and element. No subgraph isomorphism, no bond-pattern matching,
//!   no chemical perception — and matching deliberately ignores bonds, so a
//!   `before` pattern's bonds exist only to express deletions and order
//!   changes.
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
pub mod parse;
pub mod schema;

pub use apply::{HighlightTags, StepEffect, apply_step, replay, resolve_tolerance, steps_applied};
pub use compare::{Mismatch, compare_structures, describe_mismatches};
pub use parse::{
    load_build_script, load_library, parse_build_script, parse_library, validate_script_ops,
};
pub use schema::{
    BUILD_FORMAT, BuildScript, DEFAULT_TOLERANCE, LIBRARY_FORMAT, MechanosynthError, NO_LAYER,
    NO_SITE, OpLibrary, Operation, PATTERN_POSITION_EPSILON, Pattern, PatternAtom, PatternBond,
    PatternElement, Step,
};
