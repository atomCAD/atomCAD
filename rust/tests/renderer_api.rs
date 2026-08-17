// Root-package harness for the renderer tests that cannot live in
// `atomcad-renderer`, because they reach *up* into `api` / `crystolecule`
// (`doc/design_rust_crate_split.md`, D5 and D5.1a's `structure_designer_api`
// precedent). The GPU-free renderer tests themselves are in
// `crates/atomcad-renderer/tests/renderer.rs`.

#[path = "renderer_api/camera_axis_resolution_test.rs"]
mod camera_axis_resolution_test;
