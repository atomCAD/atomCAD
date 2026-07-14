//! Phase 5 of `doc/design_xray_node.md` — back-to-front sort for the merged
//! transparent impostor mesh.
//!
//! `sorted_transparent_indices` is a pure, GPU-free function: given per-quad
//! sort centers and a view matrix, it returns an index buffer whose quads are
//! ordered farthest-first in view space, preserving each quad's `0-1-2 / 0-2-3`
//! winding. These tests lock the ordering (perspective, orthographic, rotated
//! camera), the winding/permutation invariants, and the degenerate inputs.

use glam::f32::{Mat4, Vec3};
use rust_lib_flutter_cad::renderer::transparent_sort::sorted_transparent_indices;

/// A right-handed look-at view matrix, matching `Camera::build_view_matrix`.
fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    Mat4::look_at_rh(eye, target, up)
}

/// The quad index whose vertices start at `4 * quad` — recovered from the first
/// index of each 6-index group.
fn quad_order(indices: &[u32]) -> Vec<u32> {
    assert_eq!(indices.len() % 6, 0, "index buffer must be 6 per quad");
    indices.chunks(6).map(|chunk| chunk[0] / 4).collect()
}

/// Every quad's 6 indices must keep the `0-1-2 / 0-2-3` winding relative to its
/// own 4-vertex base.
fn assert_winding(indices: &[u32]) {
    for chunk in indices.chunks(6) {
        let base = chunk[0];
        assert_eq!(base % 4, 0, "quad base must be a multiple of 4");
        assert_eq!(
            chunk,
            [base, base + 1, base + 2, base + 2, base + 3, base],
            "winding drifted for quad based at {base}"
        );
    }
}

/// The output must be a permutation of the input quads: every quad appears
/// exactly once, index count preserved.
fn assert_permutation(indices: &[u32], num_quads: usize) {
    assert_eq!(indices.len(), num_quads * 6);
    let mut quads = quad_order(indices);
    quads.sort_unstable();
    assert_eq!(quads, (0..num_quads as u32).collect::<Vec<_>>());
}

/// Three quads at increasing distance from a perspective camera looking down
/// -Y come back farthest-first (the farthest quad's indices lead).
#[test]
fn perspective_orders_farthest_first() {
    // Camera at y = -10 looking toward +Y (down -view-z). Larger world y is
    // farther from the camera.
    let view = look_at(
        Vec3::new(0.0, -10.0, 0.0),
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::Z,
    );
    // near → far by world y.
    let centers = vec![
        Vec3::new(0.0, 0.0, 0.0),  // quad 0 (nearest)
        Vec3::new(0.0, 5.0, 0.0),  // quad 1
        Vec3::new(0.0, 10.0, 0.0), // quad 2 (farthest)
    ];

    let indices = sorted_transparent_indices(&centers, &view);
    assert_eq!(
        quad_order(&indices),
        vec![2, 1, 0],
        "farthest quad must draw first"
    );
    assert_winding(&indices);
    assert_permutation(&indices, centers.len());
}

/// The same ordering holds under an orthographic-style view; the sort key is
/// view-space z, so world position along the view direction is what matters.
#[test]
fn orthographic_orders_farthest_first() {
    // Camera on +Z looking toward the origin (down -view-z). Larger world z is
    // nearer; smaller (more negative) z is farther.
    let view = look_at(Vec3::new(0.0, 0.0, 20.0), Vec3::new(0.0, 0.0, 0.0), Vec3::Y);
    let centers = vec![
        Vec3::new(0.0, 0.0, 8.0),  // quad 0 (nearest)
        Vec3::new(0.0, 0.0, -4.0), // quad 1
        Vec3::new(0.0, 0.0, -9.0), // quad 2 (farthest)
    ];

    let indices = sorted_transparent_indices(&centers, &view);
    assert_eq!(quad_order(&indices), vec![2, 1, 0]);
    assert_winding(&indices);
    assert_permutation(&indices, centers.len());
}

/// A rotated (oblique) camera: the key is view-space z, not any single world
/// axis. Quads farther along the view direction still lead.
#[test]
fn rotated_camera_uses_view_space_depth() {
    let eye = Vec3::new(10.0, 10.0, 10.0);
    let view = look_at(eye, Vec3::ZERO, Vec3::Z);

    // Points along the eye→origin direction at decreasing distance from the eye.
    let dir = (Vec3::ZERO - eye).normalize();
    let far = eye + dir * 2.0; // closest to the eye is *smallest* t; farthest is largest t past origin
    let mid = eye + dir * 12.0;
    let near_origin = eye + dir * 20.0;

    // far/mid/near_origin are increasing distance along the ray from the eye,
    // so near_origin is the farthest in front and must draw first.
    let centers = vec![far, mid, near_origin];

    let indices = sorted_transparent_indices(&centers, &view);
    // Distances from the eye: far (2) < mid (12) < near_origin (20), and all
    // are in front of the camera, so farthest-first is quad 2, 1, 0.
    assert_eq!(quad_order(&indices), vec![2, 1, 0]);
    assert_winding(&indices);
    assert_permutation(&indices, centers.len());
}

/// Empty mesh → empty index buffer, no panic.
#[test]
fn empty_mesh_yields_no_indices() {
    let view = Mat4::IDENTITY;
    let indices = sorted_transparent_indices(&[], &view);
    assert!(indices.is_empty());
}

/// Single quad → its 6 indices in canonical winding.
#[test]
fn single_quad_is_identity() {
    let view = look_at(Vec3::new(0.0, -5.0, 0.0), Vec3::ZERO, Vec3::Z);
    let centers = vec![Vec3::new(1.0, 2.0, 3.0)];
    let indices = sorted_transparent_indices(&centers, &view);
    assert_eq!(indices, vec![0, 1, 2, 2, 3, 0]);
}
