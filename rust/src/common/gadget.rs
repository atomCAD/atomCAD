use crate::renderer::tessellator::tessellator::Tessellatable;
use glam::f64::DVec3;

pub trait Gadget: Tessellatable {
    // Returns the index of the handle.
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32>;
    fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3);
    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3);
    fn end_drag(&mut self);
}
