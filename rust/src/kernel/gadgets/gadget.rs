use glam::f32::Vec3;
use crate::renderer::mesh::Mesh;

pub trait Gadget {
    fn tessellate(&self, output_mesh: &mut Mesh);
    fn hit_test(&self, ray_origin: Vec3, ray_direction: Vec3) -> Option<f32>;
    //fn handle_mouse_drag(&mut self, ray_origin: Vec3, ray_direction: Vec3, previous_ray_origin: Vec3, previous_ray_direction: Vec3);
    fn clone_box(&self) -> Box<dyn Gadget>;
}

