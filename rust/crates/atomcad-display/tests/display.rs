// Integration test crate for `atomcad-display`.
//
// `atomic_impostor_alpha_test` and `atom_label_test` are NOT here: they drive
// `tessellate_scene_content`, which Phase 5 of `doc/design_rust_crate_split.md`
// (D7) moved up into `structure_designer`. A test's home is decided by what it
// imports, so they live in `rust/tests/structure_designer/` now.

#[path = "display/poly_mesh_tessellator_test.rs"]
mod poly_mesh_tessellator_test;

#[path = "display/csg_to_poly_mesh_test.rs"]
mod csg_to_poly_mesh_test;

#[path = "display/atomic_color_test.rs"]
mod atomic_color_test;

#[path = "display/atomic_render_style_test.rs"]
mod atomic_render_style_test;

// Isosurface extraction (doc/design_isosurface_node.md P2). Tables A-E of that
// document's test plan, sharing fixtures and invariant helpers.
#[path = "display/isosurface_common.rs"]
mod isosurface_common;

#[path = "display/isosurface_case_table_test.rs"]
mod isosurface_case_table_test;

#[path = "display/isosurface_fuzz_test.rs"]
mod isosurface_fuzz_test;

#[path = "display/isosurface_extract_test.rs"]
mod isosurface_extract_test;

#[path = "display/isosurface_lattice_test.rs"]
mod isosurface_lattice_test;

#[path = "display/isosurface_snapshot_test.rs"]
mod isosurface_snapshot_test;

#[path = "display/isosurface_tessellator_test.rs"]
mod isosurface_tessellator_test;

// The surface-restricted colour distribution and the two fits
// (doc/design_isosurface_level.md Part 4).
#[path = "display/isosurface_surface_distribution_test.rs"]
mod isosurface_surface_distribution_test;
