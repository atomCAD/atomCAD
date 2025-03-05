use glam::i32::IVec3;
use glam::f32::Vec3;
use glam::f32::Quat;
use super::gadget::Gadget;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::kernel::implicit_network_evaluator::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use crate::renderer::tessellator::tessellator;
use crate::util::hit_test_utils::sphere_hit_test;
use crate::util::hit_test_utils::cylinder_hit_test;
use crate::util::hit_test_utils::get_closest_point_on_first_ray;
use crate::util::hit_test_utils::get_point_distance_to_ray;
use std::collections::HashSet;

pub const MAX_MILLER_INDEX: f32 = 6.0;
pub const GADGET_LENGTH: f32 = 6.0;
pub const AXIS_RADIUS: f32 = 0.1;
pub const AXIS_DIVISIONS: u32 = 16;
pub const CENTER_SPHERE_RADIUS: f32 = 0.35;
pub const CENTER_SPHERE_HORIZONTAL_DIVISIONS: u32 = 16;
pub const CENTER_SPHERE_VERTICAL_DIVISIONS: u32 = 32;

pub const DIRECTION_HANDLE_RADIUS: f32 = 0.5;
pub const DIRECTION_HANDLE_DIVISIONS: u32 = 16;
pub const DIRECTION_HANDLE_LENGTH: f32 = 0.6;

pub const SHIFT_HANDLE_RADIUS: f32 = 0.4;
pub const SHIFT_HANDLE_DIVISIONS: u32 = 16;
pub const SHIFT_HANDLE_LENGTH: f32 = 1.2;

#[derive(Clone)]
pub struct HalfSpaceGadget {
    pub miller_index: IVec3,
    pub shift: i32,
    pub dir: Vec3, // normalized
    pub shift_handle_offset: f32,
}

impl Gadget for HalfSpaceGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {
        let direction_handle_center = self.dir * GADGET_LENGTH;

        // axis of the gadget
        tessellator::tessellate_cylinder(
          output_mesh,
          &(self.dir * f32::min(self.shift_handle_offset, 0.0)),
          &(self.dir * f32::max(self.shift_handle_offset, GADGET_LENGTH)),
          AXIS_RADIUS,
          AXIS_DIVISIONS,
          &Material::new(&Vec3::new(0.95, 0.93, 0.88), 0.4, 0.8), 
          false);

        // center sphere
        tessellator::tessellate_sphere(
            output_mesh,
            &Vec3::new(0.0, 0.0, 0.0),
            CENTER_SPHERE_RADIUS,
            CENTER_SPHERE_HORIZONTAL_DIVISIONS, // number sections when dividing by horizontal lines
            CENTER_SPHERE_VERTICAL_DIVISIONS, // number of sections when dividing by vertical lines
            &Material::new(&Vec3::new(0.95, 0.0, 0.0), 0.3, 0.0));

        
        let shift_handle_center = self.dir * self.shift_handle_offset;

        // shift handle
        tessellator::tessellate_cylinder(
            output_mesh,
            &(shift_handle_center - self.dir * 0.5 * SHIFT_HANDLE_LENGTH),
            &(shift_handle_center + self.dir * 0.5 * SHIFT_HANDLE_LENGTH),
            SHIFT_HANDLE_RADIUS,
            SHIFT_HANDLE_DIVISIONS,
            &Material::new(&Vec3::new(1.0, 1.0, 0.0), 0.3, 0.0), 
            true);
      
        // direction handle
        tessellator::tessellate_cylinder(
            output_mesh,
            &(direction_handle_center - self.dir * 0.5 * DIRECTION_HANDLE_LENGTH),
            &(direction_handle_center + self.dir * 0.5 * DIRECTION_HANDLE_LENGTH),
            DIRECTION_HANDLE_RADIUS,
            DIRECTION_HANDLE_DIVISIONS,
            &Material::new(&Vec3::new(0.0, 0.0, 0.95), 0.3, 0.0), 
            true);

        let plane_normal = self.miller_index.as_vec3().normalize();
        let plane_rotator = Quat::from_rotation_arc(Vec3::Y, plane_normal);

        let roughness: f32 = 0.5;
        let metallic: f32 = 0.0;
        let outside_material = Material::new(&Vec3::new(0.0, 0.0, 1.0), roughness, metallic);
        let inside_material = Material::new(&Vec3::new(1.0, 0.0, 0.0), roughness, metallic);
        let side_material = Material::new(&Vec3::new(0.5, 0.5, 0.5), roughness, metallic);      

        let thickness = 0.06;

        let plane_offset = ((self.shift as f32) / self.miller_index.as_vec3().length()) * (DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f32);

        // A grid representing the plane
        tessellator::tessellate_grid(
            output_mesh,
            &(plane_normal * (plane_offset - thickness * 0.5)),
            &plane_rotator,
            thickness,
            40.0,
            40.0,
            0.1,
            1.0,
            &outside_material,
            &inside_material,
            &side_material);

    }

    // Returns the index of the handle that was hit, or None if no handle was hit
    // handle 0: shift handle
    // handle 1: direction handle
    fn hit_test(&self, ray_origin: Vec3, ray_direction: Vec3) -> Option<i32> {
        // Test shift handle
        let shift_handle_center = self.dir * self.shift_handle_offset;
        let shift_handle_start = shift_handle_center - self.dir * 0.5 * SHIFT_HANDLE_LENGTH;
        let shift_handle_end = shift_handle_center + self.dir * 0.5 * SHIFT_HANDLE_LENGTH;

        if let Some(_t) = cylinder_hit_test(
            &shift_handle_end,
            &shift_handle_start,
            SHIFT_HANDLE_RADIUS,
            &ray_origin,
            &ray_direction
        ) {
            return Some(0); // Shift handle hit
        }

        let direction_handle_center = self.dir * GADGET_LENGTH;
   
        // Test direction handle (cylinder centered at gadget_end_point)
        let direction_handle_start = direction_handle_center - self.dir * 0.5 * DIRECTION_HANDLE_LENGTH;
        let direction_handle_end = direction_handle_center + self.dir * 0.5 * DIRECTION_HANDLE_LENGTH;
        
        if let Some(_t) = cylinder_hit_test(
            &direction_handle_end,
            &direction_handle_start,
            DIRECTION_HANDLE_RADIUS,
            &ray_origin,
            &ray_direction
        ) {
            return Some(1); // Direction handle hit
        }

        None // No handle was hit
    }

    fn clone_box(&self) -> Box<dyn Gadget> {
        Box::new(self.clone())
    }

    fn start_drag(&mut self, handle_index: i32, ray_origin: Vec3, ray_direction: Vec3) {

    }

    fn drag(&mut self, handle_index: i32, ray_origin: Vec3, ray_direction: Vec3) {
        if handle_index == 0 {
            // Shift handle drag
            let t = get_closest_point_on_first_ray(
                &Vec3::new(0.0, 0.0, 0.0),
                &self.dir,
                &ray_origin,
                &ray_direction);
            self.shift_handle_offset = t;
            self.shift = self.offset_to_quantized_shift(self.shift_handle_offset);
        }
        else if handle_index == 1 {
            // Direction handle drag
            if let Some(t) = sphere_hit_test(
                &Vec3::new(0.0, 0.0, 0.0),
                GADGET_LENGTH,
                &ray_origin,
                &ray_direction
            ) {
                let new_end_point = ray_origin + ray_direction * t;
                self.dir = new_end_point.normalize();
                self.miller_index = self.quantize_dir(&self.dir);
                self.shift = self.offset_to_quantized_shift(self.shift_handle_offset);
            }
        }
    }

    fn end_drag(&mut self) {
        self.dir = self.miller_index.as_vec3().normalize();
        self.shift_handle_offset = ((self.shift as f32) / self.miller_index.as_vec3().length()) * (DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f32)
    }

}

impl HalfSpaceGadget {

    pub fn new(miller_index: &IVec3, shift: i32) -> Self {
        let ret = Self {
            miller_index: *miller_index,
            shift,
            dir: miller_index.as_vec3().normalize(),
            shift_handle_offset: ((shift as f32) / miller_index.as_vec3().length()) * (DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f32)
        };

        return ret;
    }

    // Returns a miller index
    fn quantize_dir(&self, dir: &Vec3) -> IVec3 {
        let mut candidate_points: HashSet<IVec3> = HashSet::new();
        let mut t = MAX_MILLER_INDEX * 0.5;
        while t <= MAX_MILLER_INDEX {
            let p = dir * t;

            // Calculate floor and ceiling for each component to get unit cell corners
            let x_floor = p.x.floor() as i32;
            let y_floor = p.y.floor() as i32;
            let z_floor = p.z.floor() as i32;
            let x_ceil = p.x.ceil() as i32;
            let y_ceil = p.y.ceil() as i32;
            let z_ceil = p.z.ceil() as i32;
            
            // Add all 8 corners of the unit cell to candidate_points
            candidate_points.insert(IVec3::new(x_floor, y_floor, z_floor));
            candidate_points.insert(IVec3::new(x_floor, y_floor, z_ceil));
            candidate_points.insert(IVec3::new(x_floor, y_ceil, z_floor));
            candidate_points.insert(IVec3::new(x_floor, y_ceil, z_ceil));
            candidate_points.insert(IVec3::new(x_ceil, y_floor, z_floor));
            candidate_points.insert(IVec3::new(x_ceil, y_floor, z_ceil));
            candidate_points.insert(IVec3::new(x_ceil, y_ceil, z_floor));
            candidate_points.insert(IVec3::new(x_ceil, y_ceil, z_ceil));
            
            t += 0.5;
        }
                
        let mut closest_point = None;
        let mut min_distance = f32::MAX;

        for point in &candidate_points {
            let distance = get_point_distance_to_ray(&Vec3::ZERO, dir, &point.as_vec3());
            if distance < min_distance {
                min_distance = distance;
                closest_point = Some(*point);
            }
        }

        closest_point.unwrap_or(IVec3::new(1, 2, 3))
    }

    fn offset_to_quantized_shift(&self, offset: f32) -> i32 {
        let shift = offset * (self.miller_index.as_vec3().length()) / (DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f32);
        return shift.round() as i32;
    }
}
