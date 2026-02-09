use crate::renderer::tessellator::tessellator::Tessellatable;
use glam::f64::DVec3;

pub trait Gadget: Tessellatable {
    // Returns the index of the handle that was hit, or None if no handle was hit.
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32>;

    // Start dragging the handle with the given index.
    fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3);

    // Drag the handle with the given index.
    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3);

    // End dragging the handle.
    fn end_drag(&mut self);
}
