use glam::i32::IVec3;
use glam::f32::Vec3;
use glam::f64::DQuat;
use glam::f64::DVec3;
use super::node_network_gadget::NodeNetworkGadget;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::renderer::tessellator::tessellator;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::util::hit_test_utils::sphere_hit_test;
use crate::util::hit_test_utils::cylinder_hit_test;
use crate::util::hit_test_utils::get_closest_point_on_first_ray;
use crate::util::hit_test_utils::get_point_distance_to_ray;
use crate::structure_designer::node_data::half_space_data::HalfSpaceData;
use crate::structure_designer::node_data::node_data::NodeData;
use crate::structure_designer::common_constants;
use std::collections::HashSet;
use crate::common::gadget::Gadget;

pub const MAX_MILLER_INDEX: f64 = 6.0;
pub const GADGET_LENGTH: f64 = 6.0;
pub const AXIS_RADIUS: f64 = 0.1;
pub const AXIS_DIVISIONS: u32 = 16;
pub const CENTER_SPHERE_RADIUS: f64 = 0.35;
pub const CENTER_SPHERE_HORIZONTAL_DIVISIONS: u32 = 16;
pub const CENTER_SPHERE_VERTICAL_DIVISIONS: u32 = 32;

pub const DIRECTION_HANDLE_RADIUS: f64 = 0.5;
pub const DIRECTION_HANDLE_DIVISIONS: u32 = 16;
pub const DIRECTION_HANDLE_LENGTH: f64 = 0.6;

pub const SHIFT_HANDLE_RADIUS: f64 = 0.4;
pub const SHIFT_HANDLE_DIVISIONS: u32 = 16;
pub const SHIFT_HANDLE_LENGTH: f64 = 1.2;

#[derive(Clone)]
pub struct HalfSpaceGadget {
    pub miller_index: IVec3,
    pub shift: i32,
    pub dir: DVec3, // normalized
    pub shift_handle_offset: f64,
    pub is_dragging: bool,
}

impl Tessellatable for HalfSpaceGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {
        let direction_handle_center = self.dir * GADGET_LENGTH;

        // axis of the gadget
        tessellator::tessellate_cylinder(
          output_mesh,
          &(self.dir * f64::min(self.shift_handle_offset, 0.0)),
          &(self.dir * f64::max(self.shift_handle_offset, GADGET_LENGTH)),
          AXIS_RADIUS,
          AXIS_DIVISIONS,
          &Material::new(&Vec3::new(0.95, 0.93, 0.88), 0.4, 0.8), 
          false);

        // center sphere
        tessellator::tessellate_sphere(
            output_mesh,
            &DVec3::new(0.0, 0.0, 0.0),
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

        if self.is_dragging {
            let plane_normal = self.miller_index.as_dvec3().normalize();
            let plane_rotator = DQuat::from_rotation_arc(DVec3::Y, plane_normal);

            let roughness: f32 = 1.0;
            let metallic: f32 = 0.0;
            let outside_material = Material::new(&Vec3::new(0.5, 0.5, 0.5), roughness, metallic);
            let inside_material = Material::new(&Vec3::new(0.5, 0.5, 0.5), roughness, metallic);
            let side_material = Material::new(&Vec3::new(0.5, 0.5, 0.5), roughness, metallic);      

            let thickness = 0.05;

            let plane_offset = ((self.shift as f64) / self.miller_index.as_dvec3().length()) * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);

            // A grid representing the plane
            tessellator::tessellate_grid(
                output_mesh,
                &(plane_normal * plane_offset),
                &plane_rotator,
                thickness,
                40.0,
                40.0,
                0.05,
                1.0,
                &outside_material,
                &inside_material,
                &side_material);
        }

        self.tessellate_lattice_points(output_mesh);     
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for HalfSpaceGadget {
    // Returns the index of the handle that was hit, or None if no handle was hit
    // handle 0: shift handle
    // handle 1: direction handle
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
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

    fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        self.is_dragging = true;
    }

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        if handle_index == 0 {
            // Shift handle drag
            let t = get_closest_point_on_first_ray(
                &DVec3::new(0.0, 0.0, 0.0),
                &self.dir,
                &ray_origin,
                &ray_direction);
            self.shift_handle_offset = t;
            self.shift = self.offset_to_quantized_shift(self.shift_handle_offset);
        }
        else if handle_index == 1 {
            // Direction handle drag
            if let Some(t) = sphere_hit_test(
                &DVec3::new(0.0, 0.0, 0.0),
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
        self.is_dragging = false;
        self.dir = self.miller_index.as_dvec3().normalize();
        self.shift_handle_offset = ((self.shift as f64) / self.miller_index.as_dvec3().length()) * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64)
    }
}

impl NodeNetworkGadget for HalfSpaceGadget {
    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }

    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(half_space_data) = data.as_any_mut().downcast_mut::<HalfSpaceData>() {
            half_space_data.miller_index = self.miller_index;
            half_space_data.shift = self.shift;
        }
    }
}

impl HalfSpaceGadget {

    pub fn new(miller_index: &IVec3, shift: i32) -> Self {
        let ret = Self {
            miller_index: *miller_index,
            shift,
            dir: miller_index.as_dvec3().normalize(),
            shift_handle_offset: ((shift as f64) / miller_index.as_dvec3().length()) * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64),
            is_dragging: false
        };

        return ret;
    }

    fn tessellate_lattice_points(&self, output_mesh: &mut Mesh) {

        let float_miller = self.miller_index.as_dvec3();
        let miller_magnitude = float_miller.length();
        let shift_float = self.shift as f64;

        let regular_cube_size = DVec3::new(0.1, 0.1, 0.1);
        let regular_cube_material = Material::new(&Vec3::new(0.6, 0.6, 0.6), 0.5, 0.0);

        let highlighted_cube_size = DVec3::new(0.2, 0.2, 0.2);
        let highlighted_cube_material = Material::new(&Vec3::new(1.0, 1.0, 0.2), 0.3, 0.0);

        // Iterate over voxel grid
        for x in common_constants::IMPLICIT_VOLUME_MIN.x..common_constants::IMPLICIT_VOLUME_MAX.x {
            for y in common_constants::IMPLICIT_VOLUME_MIN.y..common_constants::IMPLICIT_VOLUME_MAX.y {
                for z in common_constants::IMPLICIT_VOLUME_MIN.z..common_constants::IMPLICIT_VOLUME_MAX.z {
                    let sample_point = DVec3::new(x as f64, y as f64, z as f64);
                    let distance = (float_miller.dot(sample_point) - shift_float).abs() / miller_magnitude;

                    if distance < 0.01 {
                        let lattice_point = sample_point * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
                        let highlighted = distance < 0.01;

                        let cube_size = if highlighted {
                            &highlighted_cube_size
                        } else {
                            &regular_cube_size
                        };

                        let cube_material = if highlighted {
                            &highlighted_cube_material
                        } else {
                            &regular_cube_material
                        };

                        tessellator::tessellate_cuboid(
                            output_mesh,
                            &lattice_point,
                            cube_size,
                        &DQuat::IDENTITY,
                        cube_material, cube_material, cube_material);
                    } 
                }
            }
        }
    }

    // Returns a miller index
    fn quantize_dir(&self, dir: &DVec3) -> IVec3 {
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
        let mut min_distance = f64::MAX;

        for point in &candidate_points {
            let distance = get_point_distance_to_ray(&DVec3::ZERO, dir, &point.as_dvec3());
            if distance < min_distance {
                min_distance = distance;
                closest_point = Some(*point);
            }
        }

        self.simplify_miller_index(closest_point.unwrap_or(IVec3::new(1, 0, 0)))
    }

    fn simplify_miller_index(&self, miller_index: IVec3) -> IVec3 {
        // Get absolute values for checking divisibility
        let abs_x = miller_index.x.abs();
        let abs_y = miller_index.y.abs();
        let abs_z = miller_index.z.abs();

        // Try divisions from MAX_MILLER_INDEX down to 2
        let max_divisor = MAX_MILLER_INDEX.ceil() as i32;
        for divisor in (2..=max_divisor).rev() {
            // Check if all components are divisible by the divisor
            if abs_x % divisor == 0 && abs_y % divisor == 0 && abs_z % divisor == 0 {
                return IVec3::new(
                    miller_index.x / divisor,
                    miller_index.y / divisor,
                    miller_index.z / divisor,
                );
            }
        }

        // If no common divisor found, return the original miller index
        miller_index
    }

    fn offset_to_quantized_shift(&self, offset: f64) -> i32 {
        let shift = offset * (self.miller_index.as_dvec3().length()) / (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
        return shift.round() as i32;
    }
}
