use glam::i32::IVec3;
use glam::f32::Vec3;
use super::gadget::Gadget;
use crate::renderer::mesh::Mesh;
use crate::kernel::implicit_network_evaluator::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use crate::renderer::tessellator::tessellator;

pub const HALF_SPACE_DIR_MANIPULATION_CELL_SIZE: f32 = 1.0;
pub const AXIS_RADIUS: f32 = 0.2;
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

impl Gadget for HalfSpaceGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {
        let gadget_dir = self.dir;
        let gadget_normal = gadget_dir.normalize();
        let gadget_shift = self.shift;
        let gadget_miller_index = self.miller_index.as_vec3();
        let gadget_offset = gadget_shift / gadget_miller_index.length();
        let gadget_diamond_cell_size = DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f32;
      
        let gadget_start_point = gadget_normal * gadget_offset * gadget_diamond_cell_size;
        let gadget_end_point = gadget_start_point + gadget_dir * HALF_SPACE_DIR_MANIPULATION_CELL_SIZE;
      
        tessellator::tessellate_cylinder(
          output_mesh,
          &gadget_start_point,
          &gadget_end_point,
          AXIS_RADIUS,
          AXIS_DIVISIONS,
          &Vec3::new(0.95, 0.93, 0.88),
          0.4, 0.8, false);
      
          tessellator::tessellate_sphere(
              output_mesh,
              &gadget_start_point,
              SHIFT_HANDLE_RADIUS,
              SHIFT_HANDLE_HORIZONTAL_DIVISIONS, // number sections when dividing by horizontal lines
              SHIFT_HANDLE_VERTICAL_DIVISIONS,
              &Vec3::new(0.95, 0.0, 0.0), // number of sections when dividing by vertical lines
              0.3, 0.0);
      
          tessellator::tessellate_cylinder(
              output_mesh,
              &(gadget_end_point - gadget_normal * DIRECTION_HANDLE_LENGTH),
              &(gadget_end_point + gadget_normal * DIRECTION_HANDLE_LENGTH),
              DIRECTION_HANDLE_RADIUS,
              DIRECTION_HANDLE_DIVISIONS,
              &Vec3::new(0.0, 0.0, 0.95), // number of sections when dividing by vertical lines
              0.3, 0.0, true);
    }
    
    fn hit_test(&self, ray_origin: Vec3, ray_direction: Vec3) -> Option<f32> {
        // Implement hit testing logic
        None // placeholder
    }
    
    fn clone_box(&self) -> Box<dyn Gadget> {
        Box::new(self.clone())
    }
}
