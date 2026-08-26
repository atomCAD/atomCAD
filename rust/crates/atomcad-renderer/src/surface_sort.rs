//! Back-to-front component order for the merged transparent surface mesh
//! (Phase 4 of `doc/design_isosurface_node.md`).
//!
//! Unlike [`crate::transparent_sort`], which rewrites the index buffer, this
//! sort reorders **draw calls**: components are already contiguous index
//! ranges, so ordering them costs a permutation of a single-digit-length list
//! and no GPU upload at all. It therefore runs per moved frame, in `render`,
//! rather than at tessellation time — a sort done once at tessellation looks
//! right from the original viewpoint and wrong from every other.
//!
//! **Known limit: nested components.** Two concentric shells have coincident
//! centroids, so their relative order is arbitrary and half the time wrong.
//! That needs a field with an interior extremum inside a closed shell, and the
//! fix is a per-component depth range rather than a centroid; not worth
//! building until something produces one.

use crate::transparent_surface_mesh::SurfaceComponentRange;
use glam::f32::Mat4;

/// Indices into `components`, ordered farthest-first for the given view matrix.
///
/// The key is the centroid's view-space z: in a right-handed view matrix the
/// camera looks down `-z`, so more-negative z is farther away and must be
/// drawn first. `total_cmp` gives a total order even with NaN, so the result is
/// deterministic and never panics.
pub fn sorted_component_order(components: &[SurfaceComponentRange], view: &Mat4) -> Vec<usize> {
    let mut order: Vec<(usize, f32)> = components
        .iter()
        .enumerate()
        .map(|(i, component)| (i, view.transform_point3(component.centroid).z))
        .collect();
    order.sort_by(|a, b| a.1.total_cmp(&b.1));
    order.into_iter().map(|(i, _)| i).collect()
}
