//! Root-package harness for the `structure_designer` tests that reach *up* into
//! `api/`.
//!
//! A member crate cannot depend on the root crate, so a test naming anything in
//! `rust_lib_flutter_cad::api` has to live here even though its subject lives in
//! `atomcad-structure-designer` — **a test's home is decided by what it imports,
//! not by what it is about** (`doc/design_rust_crate_split.md` D5.1a; the
//! precedent is `tests/renderer_api.rs` from Phase 3). The other ~160 files are
//! at `crates/atomcad-structure-designer/tests/structure_designer/`.
//!
//! Two of these are *splits*, not whole files: `function_pin_api_test.rs` and
//! `node_type_views_zone_body_test.rs` carry the one api-touching test out of a
//! file whose remainder is pure domain.

#[path = "structure_designer_api/atom_edit_add_atom_marker_test.rs"]
mod atom_edit_add_atom_marker_test;

#[path = "structure_designer_api/atom_edit_bond_order_test.rs"]
mod atom_edit_bond_order_test;

#[path = "structure_designer_api/chain_hygiene_test.rs"]
mod chain_hygiene_test;

#[path = "structure_designer_api/data_type_test.rs"]
mod data_type_test;

#[path = "structure_designer_api/drag_adapter_test.rs"]
mod drag_adapter_test;

#[path = "structure_designer_api/error_origins_test.rs"]
mod error_origins_test;

#[path = "structure_designer_api/eval_error_snapshot_test.rs"]
mod eval_error_snapshot_test;

#[path = "structure_designer_api/function_pin_api_test.rs"]
mod function_pin_api_test;

#[path = "structure_designer_api/node_type_registry_test.rs"]
mod node_type_registry_test;

#[path = "structure_designer_api/node_type_views_zone_body_test.rs"]
mod node_type_views_zone_body_test;

#[path = "structure_designer_api/scoped_validation_errors_test.rs"]
mod scoped_validation_errors_test;
