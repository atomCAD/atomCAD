//! Historical: this file once held a regression test
//! (`test_map_with_expr_function_evaluates_without_panic`) for a crash in
//! `FunctionEvaluator::new` that fired when an `expr` node's function pin
//! was wired into `map.f`. The crash was rooted in `map.f` itself —
//! `FunctionEvaluator::new` cloned the function node's data into a
//! throw-away network but never repopulated the `custom_node_type` cache,
//! so `get_node_type_for_node` returned the empty base parameter list.
//!
//! Phase 4 of `doc/design_zones.md` removed `map.f` (and the function-pin
//! plumbing for `map` overall), so the original failure mode is no longer
//! reachable from `map`. The same `FunctionEvaluator` path is still
//! exercised by `filter` / `fold` / `foreach` tests in Phase 4 (and by the
//! end-to-end FE-walker propagation tests in `execute_flag_test.rs`).
//!
//! Kept as a documentation breadcrumb until Phase 5 retires
//! `FunctionEvaluator` entirely.
