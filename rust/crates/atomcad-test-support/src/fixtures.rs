//! Resolver for the shared test fixture tree at `rust/tests/fixtures/`
//! (`doc/design_rust_crate_split.md`, D5.3).
//!
//! The fixtures are read from three different packages — `atomcad-crystolecule`
//! (the CIF corpus), `atomcad-structure-designer`, and the root crate's
//! `tests/integration/` (the `.cnnd` migration corpus) — and they deliberately
//! stay in **one** place: duplicating them per crate would fork the migration
//! corpus, which is exactly the drift the corpus exists to catch.
//!
//! Both addressing styles the tests used before the split are anchored to the
//! *package* root and therefore break on the move:
//!
//! - `format!("{}/tests/fixtures/…", env!("CARGO_MANIFEST_DIR"))` —
//!   `CARGO_MANIFEST_DIR` becomes the crate directory.
//! - `std::fs::read_to_string("tests/fixtures/…")` — `cargo test` runs with the
//!   package root as the working directory.
//!
//! [`fixture_path`] fixes both: its `CARGO_MANIFEST_DIR` expands at compile time
//! of *this* crate, so the answer is the same no matter who calls it, and the
//! `../..` hop lives in exactly one line.

use std::path::{Path, PathBuf};

/// Absolute path of `rust/tests/fixtures/<rel>`.
///
/// ```text
/// let cif = std::fs::read_to_string(fixture_path("cif/diamond.cif")).unwrap();
/// ```
pub fn fixture_path(rel: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).join(rel)
}

/// [`fixture_path`] as a `String`, for the many loaders that take `&str`
/// (`load_node_networks`, `load_cif`, …) and cannot accept a `Path`.
pub fn fixture_path_str(rel: &str) -> String {
    fixture_path(rel).to_string_lossy().into_owned()
}

/// `rust/tests/fixtures/` itself — for the few tests that walk the tree instead
/// of naming a single file (the `.cnnd` validation corpus).
pub fn fixtures_root() -> PathBuf {
    fixture_path("")
}
