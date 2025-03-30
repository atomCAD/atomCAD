use crate::common::gadget::Gadget;
use crate::util::transform::Transform;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator;
use glam::f64::DVec3;
use glam::f32::Vec3;
use crate::renderer::mesh::Material;
use crate::util::hit_test_utils;

pub const GADGET_LENGTH: f64 = 6.0;
pub const AXIS_RADIUS: f64 = 0.1;
pub const AXIS_DIVISIONS: u32 = 16;
pub const AXIS_CONE_RADIUS: f64 = 0.3;
pub const AXIS_ARROW_CONE_LENGTH: f64 = 0.6;
pub const AXIS_ARROW_CONE_OFFSET: f64 = 0.1;

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
    // Returns the index of the handle that was hit, or None if no handle was hit
    // handle 0: x axis translate handle
    // handle 1: y axis translate handle
    // handle 2: z axis translate handle
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        let x_axis_dir = self.transform.rotation.mul_vec3(DVec3::new(1.0, 0.0, 0.0));
        let y_axis_dir = self.transform.rotation.mul_vec3(DVec3::new(0.0, 1.0, 0.0));
        let z_axis_dir = self.transform.rotation.mul_vec3(DVec3::new(0.0, 0.0, 1.0));
        
        if self.axis_arrow_hit_test(&x_axis_dir, &ray_origin, &ray_direction) {
            return Some(0);
        }
        if self.axis_arrow_hit_test(&y_axis_dir, &ray_origin, &ray_direction) {
            return Some(1);
        }
        if self.axis_arrow_hit_test(&z_axis_dir, &ray_origin, &ray_direction) {
            return Some(2);
        }
        None
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
        tessellator::tessellate_arrow(
            output_mesh,
            &self.transform.translation,
            axis_dir,
            AXIS_RADIUS,
            AXIS_CONE_RADIUS,
            AXIS_DIVISIONS,
            GADGET_LENGTH,
            AXIS_ARROW_CONE_LENGTH,
            AXIS_ARROW_CONE_OFFSET,
            &Material::new(albedo, 0.4, 0.8));
    }

    fn axis_arrow_hit_test(
        &self,
        axis_dir: &DVec3,
        ray_origin: &DVec3,
        ray_direction: &DVec3) -> bool {
        match hit_test_utils::arrow_hit_test(
            &self.transform.translation,
            axis_dir,
            AXIS_RADIUS,
            AXIS_CONE_RADIUS,
            GADGET_LENGTH,
            AXIS_ARROW_CONE_LENGTH,
            AXIS_ARROW_CONE_OFFSET,
            ray_origin,
            ray_direction
        ) {
            Some(_) => true,
            None => false
        }
    }
}
