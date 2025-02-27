use crate::renderer::mesh::Mesh;
use crate::kernel::gadget_state::HalfSpaceGadgetState;
use crate::kernel::implicit_network_evaluator::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use super::tessellator;
use glam::f32::Vec3;
use glam::f32::Quat;

pub const HALF_SPACE_DIR_MANIPULATION_CELL_SIZE: f32 = 1.0;
pub const AXIS_RADIUS: f32 = 0.2;
pub const AXIS_DIVISIONS: u32 = 16;
pub const SHIFT_HANDLE_RADIUS: f32 = 0.5;
pub const SHIFT_HANDLE_HORIZONTAL_DIVISIONS: u32 = 16;
pub const SHIFT_HANDLE_VERTICAL_DIVISIONS: u32 = 32;

pub const DIRECTION_HANDLE_RADIUS: f32 = 0.5;
pub const DIRECTION_HANDLE_DIVISIONS: u32 = 16;
pub const DIRECTION_HANDLE_LENGTH: f32 = 1.0;

pub fn tessellate_half_space_gadget(output_mesh: &mut Mesh, half_space_gadget_state: &HalfSpaceGadgetState) {
  let gadget_dir = half_space_gadget_state.int_dir.as_vec3();
  let gadget_normal = gadget_dir.normalize();
  let gadget_shift = half_space_gadget_state.int_shift as f32;
  let gadget_miller_index = half_space_gadget_state.miller_index.as_vec3();
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
