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

// Node network thumbnails: bounds, framing, downsample, PNG and the change
// comparison (doc/design_network_thumbnails.md, Phase 1).
#[path = "renderer/thumbnail_test.rs"]
mod thumbnail_test;

// *File > Export viewport image*: the size rule.
#[path = "renderer/image_export_test.rs"]
mod image_export_test;

// Screen-space lines, Phase 1: the extended camera uniform
// (doc/design_screen_space_lines.md D3).
#[path = "renderer/camera_uniform_test.rs"]
mod camera_uniform_test;

// The render target's size, pixel scale and sample count, and the live
// target's quality settings (doc/design_viewport_antialiasing.md D1, D8;
// doc/design_screen_space_lines.md D2).
#[path = "renderer/render_target_test.rs"]
mod render_target_test;

// Every shader parses and validates; every `CameraUniform` in WGSL matches the
// Rust struct (naga, doc/design_screen_space_lines.md Testing layer 2).
#[path = "renderer/shader_test.rs"]
mod shader_test;

// Screen-space lines, Phase 2: the instanced `LineMesh`, the line blend state
// and the Rust twins of `line_mesh.wgsl` (doc/design_screen_space_lines.md
// D1, D4, D6, D7).
#[path = "renderer/line_mesh_test.rs"]
mod line_mesh_test;

#[path = "renderer/line_geometry_test.rs"]
mod line_geometry_test;

// Screen-space lines, Phase 4: grid level of detail and the distance fade
// (doc/design_screen_space_lines.md D9).
#[path = "renderer/line_lod_test.rs"]
mod line_lod_test;

// Viewport anti-aliasing, Phase 2: the Rust twins of the impostor shaders'
// per-sample coverage (doc/design_viewport_antialiasing.md D4, D6).
#[path = "renderer/impostor_coverage_test.rs"]
mod impostor_coverage_test;

// Viewport anti-aliasing, Phase 3: when the still-camera refined frame is
// drawn, and that every mutating `Renderer` method invalidates it
// (doc/design_viewport_antialiasing.md D5).
#[path = "renderer/live_refinement_test.rs"]
mod live_refinement_test;
