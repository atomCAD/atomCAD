use glam::f64::DVec3;
use glam::f64::DQuat;
use glam::Vec3;
use crate::renderer::tessellator::tessellator;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;

pub const AXIS_CYLINDER_LENGTH: f64 = 6.0;
pub const AXIS_CYLINDER_RADIUS: f64 = 0.1;
pub const AXIS_CONE_RADIUS: f64 = 0.2;
pub const AXIS_DIVISIONS: u32 = 16;
pub const AXIS_CONE_LENGTH: f64 = 0.5;
pub const AXIS_CONE_OFFSET: f64 = 0.1;

pub fn tessellate_xyz_gadget(output_mesh: &mut Mesh, rotation_quat: DQuat, pos: &DVec3) {        
  let x_axis_dir = rotation_quat.mul_vec3(DVec3::new(1.0, 0.0, 0.0));
  let y_axis_dir = rotation_quat.mul_vec3(DVec3::new(0.0, 1.0, 0.0));
  let z_axis_dir = rotation_quat.mul_vec3(DVec3::new(0.0, 0.0, 1.0));

  tessellate_axis_arrow(output_mesh, &pos, &x_axis_dir, &Vec3::new(1.0, 0.0, 0.0));
  tessellate_axis_arrow(output_mesh, &pos, &y_axis_dir, &Vec3::new(0.0, 1.0, 0.0));
  tessellate_axis_arrow(output_mesh, &pos, &z_axis_dir, &Vec3::new(0.0, 0.0, 1.0));
}

pub fn tessellate_axis_arrow(output_mesh: &mut Mesh, start_pos: &DVec3, axis_dir: &DVec3, albedo: &Vec3) {
  tessellator::tessellate_arrow(
    output_mesh,
    start_pos,
    axis_dir,
    AXIS_CYLINDER_RADIUS,
    AXIS_CONE_RADIUS,
    AXIS_DIVISIONS,
    AXIS_CYLINDER_LENGTH,
    AXIS_CONE_LENGTH,
    AXIS_CONE_OFFSET,
    &Material::new(albedo, 0.4, 0.8),
);
}
