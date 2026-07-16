// Integration test crate for the `display` module.

#[path = "display/poly_mesh_tessellator_test.rs"]
mod poly_mesh_tessellator_test;

#[path = "display/csg_to_poly_mesh_test.rs"]
mod csg_to_poly_mesh_test;

#[path = "display/atomic_impostor_alpha_test.rs"]
mod atomic_impostor_alpha_test;

#[path = "display/atomic_color_test.rs"]
mod atomic_color_test;

#[path = "display/atomic_render_style_test.rs"]
mod atomic_render_style_test;

#[path = "display/atom_label_test.rs"]
mod atom_label_test;
