use crate::common::gadget::Gadget;
use crate::util::transform::Transform;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator;
use glam::f64::DVec3;
use glam::f64::DQuat;
use glam::f32::Vec3;
use crate::renderer::mesh::Material;
use crate::util::hit_test_utils;

pub const GADGET_OFFSET: f64 = -1.0;
pub const GADGET_LENGTH: f64 = 6.0;
pub const AXIS_RADIUS: f64 = 0.1;
pub const AXIS_DIVISIONS: u32 = 16;
pub const AXIS_CONE_RADIUS: f64 = 0.3;
pub const AXIS_ARROW_CONE_LENGTH: f64 = 0.6;
pub const AXIS_OVERLAP: f64 = 0.1;
pub const ROTATOR_CYLINDER_RADIUS: f64 = 0.3;
pub const ROTATOR_CYLINDER_LENGTH: f64 = 0.6;

#[derive(Clone)]
pub struct ClusterFrameGadget {
    pub transform: Transform,
    pub last_synced_transform: Transform,
    pub frame_locked_to_atoms: bool,
    pub dragging_offset: f64, // used during dragging
    pub drag_start_rotation: DQuat,
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

        if self.axis_rotator_hit_test(&x_axis_dir, &ray_origin, &ray_direction) {
            return Some(3);
        }

        if self.axis_rotator_hit_test(&y_axis_dir, &ray_origin, &ray_direction) {
            return Some(4);
        }

        if self.axis_rotator_hit_test(&z_axis_dir, &ray_origin, &ray_direction) {
            return Some(5);
        }

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
        self.drag_start_rotation = self.transform.rotation;
        let axis_dir = self.get_axis_dir(handle_index);
        self.dragging_offset = hit_test_utils::get_closest_point_on_first_ray(
            &self.transform.translation,
            &axis_dir,
            &ray_origin,
            &ray_direction);
    }

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        let axis_dir = self.get_axis_dir(handle_index);
        let offset = hit_test_utils::get_closest_point_on_first_ray(
            &self.transform.translation,
            &axis_dir,
            &ray_origin,
            &ray_direction);
        if handle_index == 3 || handle_index == 4 || handle_index == 5 {
            println!("o: {}", offset - self.dragging_offset);
            println!("axis_dir: {}", axis_dir);
            let q = DQuat::from_axis_angle(axis_dir, offset - self.dragging_offset);
            println!("q: {}", q);
            self.transform.rotation =  DQuat::from_axis_angle(axis_dir, offset - self.dragging_offset).mul_quat(self.drag_start_rotation).normalize();
        } else {
            self.transform.translation += axis_dir * (offset - self.dragging_offset);
        }
    }

    fn end_drag(&mut self) {
        self.dragging_offset = 0.0; // not really necessary to clear it, we do it for debugging purposes
    }
}

impl ClusterFrameGadget {
    fn tessellate_axis_arrow(&self, output_mesh: &mut Mesh, axis_dir: &DVec3, albedo: &Vec3) {

        let material = Material::new(albedo, 0.4, 0.8);

        tessellator::tessellate_cylinder(
            output_mesh,
            &(self.transform.translation + axis_dir * GADGET_OFFSET),
            &(self.transform.translation + axis_dir * (GADGET_OFFSET - ROTATOR_CYLINDER_LENGTH)),
            ROTATOR_CYLINDER_RADIUS,
            AXIS_DIVISIONS,
            &material,
            true);

        tessellator::tessellate_arrow(
            output_mesh,
            &(self.transform.translation + axis_dir * GADGET_OFFSET),
            axis_dir,
            AXIS_RADIUS,
            AXIS_CONE_RADIUS,
            AXIS_DIVISIONS,
            GADGET_LENGTH,
            AXIS_ARROW_CONE_LENGTH,
            AXIS_OVERLAP,
            &material);
    }

    fn axis_arrow_hit_test(
        &self,
        axis_dir: &DVec3,
        ray_origin: &DVec3,
        ray_direction: &DVec3) -> bool {
        match hit_test_utils::arrow_hit_test(
            &(self.transform.translation + axis_dir * GADGET_OFFSET),
            axis_dir,
            AXIS_RADIUS,
            AXIS_CONE_RADIUS,
            GADGET_LENGTH,
            AXIS_ARROW_CONE_LENGTH,
            AXIS_OVERLAP,
            ray_origin,
            ray_direction
        ) {
            Some(_) => true,
            None => false
        }
    }

    fn axis_rotator_hit_test(
        &self,
        axis_dir: &DVec3,
        ray_origin: &DVec3,
        ray_direction: &DVec3) -> bool {
        match hit_test_utils::cylinder_hit_test(
            &(self.transform.translation + axis_dir * GADGET_OFFSET),
            &(self.transform.translation + axis_dir * (GADGET_OFFSET - ROTATOR_CYLINDER_LENGTH)),
            ROTATOR_CYLINDER_RADIUS,
            ray_origin,
            ray_direction
        ) {
            Some(_) => true,
            None => false
        }
    }

    fn get_axis_dir(&self, handle_index: i32) -> DVec3 {
        return self.drag_start_rotation.mul_vec3(match handle_index {
            0 => DVec3::new(1.0, 0.0, 0.0),
            1 => DVec3::new(0.0, 1.0, 0.0),
            2 => DVec3::new(0.0, 0.0, 1.0),
            3 => DVec3::new(1.0, 0.0, 0.0),
            4 => DVec3::new(0.0, 1.0, 0.0),
            5 => DVec3::new(0.0, 0.0, 1.0),
            _ => DVec3::new(0.0, 0.0, 0.0)
        });
    }
}
