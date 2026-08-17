//! Back-to-front depth sort for the merged transparent impostor mesh
//! (Phase 5 of `doc/design_xray_node.md`).
//!
//! The transparent impostor pipeline draws with depth *test* on but depth
//! *write* off, so correct alpha compositing depends entirely on the draw
//! order: farther quads must be drawn before nearer ones. This module turns
//! the mesh's per-quad sort centers into a full index buffer whose quads are
//! ordered farthest-first, ready for `queue.write_buffer` onto the existing
//! index buffer (the permutation never changes the buffer size).

use glam::f32::{Mat4, Vec3};

/// Back-to-front quad order for alpha blending. Returns a full index buffer
/// (6 indices per quad, `0-1-2 / 0-2-3` winding within each quad, matching the
/// emission order in `TransparentImpostorMesh::add_quad_indices`), with quads
/// ordered by ascending view-space z — i.e. farthest first.
///
/// The i-th sort center in `quad_centers` corresponds to the quad whose four
/// vertices occupy vertex-buffer slots `4*i .. 4*i + 4`, so the returned
/// indices reference those vertices directly. The view-space z of the center
/// is the sort key: in a right-handed view matrix the camera looks down `-z`,
/// so more-negative z is farther away. This key is exact for orthographic
/// projection and a solid per-quad approximation for perspective (§Renderer).
pub fn sorted_transparent_indices(quad_centers: &[Vec3], view: &Mat4) -> Vec<u32> {
    // Pair each quad with its view-space depth, then order farthest-first.
    let mut order: Vec<(usize, f32)> = quad_centers
        .iter()
        .enumerate()
        .map(|(i, center)| (i, view.transform_point3(*center).z))
        .collect();
    // Ascending view-space z = most negative (farthest) first. `total_cmp`
    // gives a total order even in the presence of NaN, so the sort is
    // deterministic and never panics.
    order.sort_by(|a, b| a.1.total_cmp(&b.1));

    let mut indices = Vec::with_capacity(quad_centers.len() * 6);
    for (quad_index, _) in order {
        let base = (quad_index * 4) as u32;
        // Same winding as the CPU mesh emits, just re-ordered per quad.
        indices.push(base);
        indices.push(base + 1);
        indices.push(base + 2);
        indices.push(base + 2);
        indices.push(base + 3);
        indices.push(base);
    }
    indices
}
