//! Test helpers shared by more than one test binary.
//!
//! Before `doc/design_rust_crate_split.md`, everything here was a `#[path]`-included
//! module under `rust/tests/test_support/`. Test binaries in different *packages*
//! cannot include each other's files, so the helpers became a real crate, used
//! as a `[dev-dependencies]` entry (D5.2).
//!
//! Two rules for anything added here:
//!
//! - This crate is only ever a dev-dependency. It depends on
//!   `atomcad-crystolecule`, which dev-depends back on it; that cycle is legal
//!   *because* the edge back is a dev edge.
//! - Paths to `rust/tests/fixtures/` must go through [`fixture_path`], and
//!   paths to the repository's `samples/` through [`sample_path`] — never
//!   through a caller-side `CARGO_MANIFEST_DIR` or a relative `"tests/…"` /
//!   `"../samples/…"` string, both of which are anchored to the *package*
//!   root and mean something different in every crate.

pub mod fixtures;
pub mod structure_equivalence;

pub use fixtures::{fixture_path, fixture_path_str, fixtures_root, sample_path, sample_path_str};
pub use structure_equivalence::assert_structures_equivalent;
