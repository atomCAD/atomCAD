//! Bottom of the atomCAD backend dependency DAG: small, dependency-free
//! helpers (integer matrices, axis-aligned boxes, transforms, hit tests,
//! serde adapters, caching primitives) shared by every crate above it.
//!
//! This crate must never depend on another `atomcad-*` crate — that is the
//! whole point of it being the bottom of the graph (see
//! `doc/design_rust_crate_split.md`).

pub mod as_any;
pub mod box_subdivision;
pub mod daabox;
pub mod hit_test_utils;
pub mod imat2;
pub mod imat3;
pub mod mat_utils;
pub mod memory_bounded_lru_cache;
pub mod memory_size_estimator;
pub mod path_utils;
pub mod serialization_utils;
pub mod timer;
pub mod transform;
pub mod unique_3d_points;
