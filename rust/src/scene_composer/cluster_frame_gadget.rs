use crate::common::gadget::Gadget;
use crate::util::transform::Transform;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator;
use glam::f64::DVec3;
use glam::f32::Vec3;
use crate::renderer::mesh::Material;

pub const GADGET_LENGTH: f64 = 6.0;
pub const AXIS_RADIUS: f64 = 0.1;
pub const AXIS_DIVISIONS: u32 = 16;

#[derive(Clone)]
pub struct ClusterFrameGadget {
    pub transform: Transform,
    pub last_synced_transform: Transform,
    pub frame_locked_to_atoms: bool,
}

impl Tessellatable for ClusterFrameGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {
        let x_axis_dir = self.transform.rotation.mul_vec3(DVec3::new(1.0, 0.0, 0.0));
        let y_axis_dir = self.transform.rotation.mul_vec3(DVec3::new(0.0, 1.0, 0.0));
        let z_axis_dir = self.transform.rotation.mul_vec3(DVec3::new(0.0, 0.0, 1.0));

        self.tessellate_axis_arrow(output_mesh, &x_axis_dir, &Vec3::new(1.0, 0.0, 0.0));
        self.tessellate_axis_arrow(output_mesh, &y_axis_dir, &Vec3::new(0.0, 1.0, 0.0));
        self.tessellate_axis_arrow(output_mesh, &z_axis_dir, &Vec3::new(0.0, 0.0, 1.0));
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}


impl Gadget for ClusterFrameGadget {
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        todo!();
    }

    fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        todo!();
    }

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        todo!();
    }

    fn end_drag(&mut self) {
        todo!();
    }
}

impl ClusterFrameGadget {
    fn tessellate_axis_arrow(&self, output_mesh: &mut Mesh, axis_dir: &DVec3, albedo: &Vec3) {
        tessellator::tessellate_cylinder(
            output_mesh,
            &(self.transform.translation + axis_dir * GADGET_LENGTH),
            &self.transform.translation,
            AXIS_RADIUS,
            AXIS_DIVISIONS,
            &Material::new(albedo, 0.4, 0.8), 
            true);        
    }
}