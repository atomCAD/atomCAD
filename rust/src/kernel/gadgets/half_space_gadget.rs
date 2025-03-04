use glam::i32::IVec3;
use glam::f32::Vec3;
use super::gadget::Gadget;
use crate::renderer::mesh::Mesh;
use crate::kernel::implicit_network_evaluator::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use crate::renderer::tessellator::tessellator;
use crate::util::hit_test_utils::sphere_hit_test;
use crate::util::hit_test_utils::cylinder_hit_test;
use crate::util::hit_test_utils::get_closest_point_on_first_ray;

pub const HALF_SPACE_DIR_MANIPULATION_CELL_SIZE: f32 = 1.0;
pub const AXIS_RADIUS: f32 = 0.1;
pub const AXIS_DIVISIONS: u32 = 16;
pub const SHIFT_HANDLE_RADIUS: f32 = 0.5;
pub const SHIFT_HANDLE_HORIZONTAL_DIVISIONS: u32 = 16;
pub const SHIFT_HANDLE_VERTICAL_DIVISIONS: u32 = 32;

pub const DIRECTION_HANDLE_RADIUS: f32 = 0.5;
pub const DIRECTION_HANDLE_DIVISIONS: u32 = 16;
pub const DIRECTION_HANDLE_LENGTH: f32 = 1.0;

#[derive(Clone)]
pub struct HalfSpaceGadget {
    pub dir: Vec3,
    pub miller_index: IVec3,
    pub int_shift: i32,
    pub shift: f32,
}

struct HalfSpaceGadgetCalculated {
    start_point: Vec3,
    end_point: Vec3,
    normal: Vec3,
    quantized_start_point: Vec3,
    quantized_end_point: Vec3,
}

impl Gadget for HalfSpaceGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {
        let calculated = self.calculate_gadget();
      
        // axis of the gadget
        tessellator::tessellate_cylinder(
          output_mesh,
          &calculated.start_point,
          &calculated.end_point,
          AXIS_RADIUS,
          AXIS_DIVISIONS,
          &Vec3::new(0.95, 0.93, 0.88),
          0.4, 0.8, false);
      
        // shift handle
        tessellator::tessellate_sphere(
            output_mesh,
            &calculated.start_point,
            SHIFT_HANDLE_RADIUS,
            SHIFT_HANDLE_HORIZONTAL_DIVISIONS, // number sections when dividing by horizontal lines
            SHIFT_HANDLE_VERTICAL_DIVISIONS,
            &Vec3::new(0.95, 0.0, 0.0), // number of sections when dividing by vertical lines
            0.3, 0.0);
      
        // direction handle
        tessellator::tessellate_cylinder(
            output_mesh,
            &(calculated.end_point - calculated.normal * 0.5 * DIRECTION_HANDLE_LENGTH),
            &(calculated.end_point + calculated.normal * 0.5 * DIRECTION_HANDLE_LENGTH),
            DIRECTION_HANDLE_RADIUS,
            DIRECTION_HANDLE_DIVISIONS,
            &Vec3::new(0.0, 0.0, 0.95), // number of sections when dividing by vertical lines
            0.3, 0.0, true);

        // Axis representing the quantized shift and miller index
        tessellator::tessellate_cylinder(
            output_mesh,
            &calculated.quantized_start_point,
            &calculated.quantized_end_point,
            AXIS_RADIUS,
            AXIS_DIVISIONS,
            &Vec3::new(1.0, 1.0, 1.0),
            0.3, 0.0, true);

    }

    // Returns the index of the handle that was hit, or None if no handle was hit
    // handle 0: shift handle
    // handle 1: direction handle
    fn hit_test(&self, ray_origin: Vec3, ray_direction: Vec3) -> Option<i32> {
        let calculated = self.calculate_gadget();
        
        // Test shift handle (sphere at gadget_start_point)
        if let Some(_t) = sphere_hit_test(
            &calculated.start_point,
            SHIFT_HANDLE_RADIUS,
            &ray_origin,
            &ray_direction
        ) {
            return Some(0); // Shift handle hit
        }
        
        // Test direction handle (cylinder centered at gadget_end_point)
        let direction_handle_start = calculated.end_point - calculated.normal * DIRECTION_HANDLE_LENGTH;
        let direction_handle_end = calculated.end_point + calculated.normal * DIRECTION_HANDLE_LENGTH;
        
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
        let calculated = self.calculate_gadget();
        
        if handle_index == 0 {
            // Shift handle drag
            let dt = get_closest_point_on_first_ray(
                &calculated.start_point,
                &calculated.normal,
                &ray_origin,
                &ray_direction);
            self.shift += dt * (self.miller_index.as_vec3().length()) / (DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f32);
        }
        else if handle_index == 1 {
            // Direction handle drag
            if let Some(t) = sphere_hit_test(
                &calculated.start_point,
                self.dir.length(),
                &ray_origin,
                &ray_direction
            ) {
                let new_end_point = ray_origin + ray_direction * t;
                self.dir = (new_end_point - calculated.start_point).normalize() * 6.0; // TODO: implement this correctly
            }
        }
    }

    fn end_drag(&mut self) {

    }

}

impl HalfSpaceGadget {
    fn calculate_gadget(&self) -> HalfSpaceGadgetCalculated {
        let gadget_dir = self.dir;
        let gadget_normal = gadget_dir.normalize();
        let gadget_shift = self.shift;
        let gadget_miller_index = self.miller_index.as_vec3();
        let gadget_offset = gadget_shift / gadget_miller_index.length();
        let gadget_diamond_cell_size = DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f32;

        let gadget_start_point = gadget_normal * gadget_offset * gadget_diamond_cell_size;

        let quantized_normal = gadget_miller_index.normalize();
        let quantized_shift = self.int_shift as f32;
        let quantized_offset = quantized_shift / gadget_miller_index.length();

        let quantized_start_point = quantized_normal * quantized_offset * gadget_diamond_cell_size; 
        let quantized_end_point = quantized_start_point + quantized_normal * (gadget_dir.length() + 2.0) * HALF_SPACE_DIR_MANIPULATION_CELL_SIZE;    

        return HalfSpaceGadgetCalculated {
            start_point: gadget_start_point,
            end_point: gadget_start_point + gadget_dir * HALF_SPACE_DIR_MANIPULATION_CELL_SIZE,
            normal: gadget_normal,
            quantized_start_point,
            quantized_end_point,
        }       
    }
}
