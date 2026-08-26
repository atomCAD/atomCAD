// Integration test crate for the `renderer` module.
// Camera math is GPU-free, so it can be unit-tested here.

#[path = "renderer/camera_test.rs"]
mod camera_test;

#[path = "renderer/label_atlas_test.rs"]
mod label_atlas_test;

#[path = "renderer/label_mesh_test.rs"]
mod label_mesh_test;

#[path = "renderer/transparent_impostor_mesh_test.rs"]
mod transparent_impostor_mesh_test;

#[path = "renderer/transparent_sort_test.rs"]
mod transparent_sort_test;

// Vertex alpha and the transparent-surface component sort
// (doc/design_isosurface_node.md P4).
#[path = "renderer/surface_alpha_test.rs"]
mod surface_alpha_test;
