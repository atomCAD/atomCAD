use glam::f64::DVec3;
use glam::f64::DQuat;
use glam::Vec3;
use crate::renderer::tessellator::tessellator;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::util::hit_test_utils::{arrow_hit_test, get_closest_point_on_first_ray};

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

pub fn xyz_gadget_hit_test(
    rotation_quat: DQuat,
    pos: &DVec3,
    ray_origin: &DVec3,
    ray_direction: &DVec3
) -> Option<i32> {
    let x_axis_dir = rotation_quat.mul_vec3(DVec3::new(1.0, 0.0, 0.0));
    let y_axis_dir = rotation_quat.mul_vec3(DVec3::new(0.0, 1.0, 0.0));
    let z_axis_dir = rotation_quat.mul_vec3(DVec3::new(0.0, 0.0, 1.0));

    // Test hit against X axis arrow
    let x_hit = arrow_hit_test(
        pos,
        &x_axis_dir,
        AXIS_CYLINDER_RADIUS,
        AXIS_CONE_RADIUS,
        AXIS_CYLINDER_LENGTH,
        AXIS_CONE_LENGTH,
        AXIS_CONE_OFFSET,
        ray_origin,
        ray_direction
    );

    // Test hit against Y axis arrow
    let y_hit = arrow_hit_test(
        pos,
        &y_axis_dir,
        AXIS_CYLINDER_RADIUS,
        AXIS_CONE_RADIUS,
        AXIS_CYLINDER_LENGTH,
        AXIS_CONE_LENGTH,
        AXIS_CONE_OFFSET,
        ray_origin,
        ray_direction
    );

    // Test hit against Z axis arrow
    let z_hit = arrow_hit_test(
        pos,
        &z_axis_dir,
        AXIS_CYLINDER_RADIUS,
        AXIS_CONE_RADIUS,
        AXIS_CYLINDER_LENGTH,
        AXIS_CONE_LENGTH,
        AXIS_CONE_OFFSET,
        ray_origin,
        ray_direction
    );

    // Find the closest hit and return its axis index
    let mut closest_distance: Option<f64> = None;
    let mut closest_axis: Option<i32> = None;
    
    let hits = [(x_hit, 0), (y_hit, 1), (z_hit, 2)];
    
    for (hit, axis_index) in hits.iter() {
        if let Some(distance) = hit {
            match closest_distance {
                None => {
                    closest_distance = Some(*distance);
                    closest_axis = Some(*axis_index);
                },
                Some(current_closest) => {
                    if *distance < current_closest {
                        closest_distance = Some(*distance);
                        closest_axis = Some(*axis_index);
                    }
                }
            }
        }
    }
    
    closest_axis
}

pub fn get_dragged_axis_offset(
    rotation_quat: DQuat,
    pos: &DVec3,
    dragged_axis_index: i32,
    ray_origin: &DVec3,
    ray_direction: &DVec3
) -> f64 {
    // Get the axis direction based on the dragged axis index
    let axis_dir = match dragged_axis_index {
        0 => rotation_quat.mul_vec3(DVec3::new(1.0, 0.0, 0.0)), // X axis
        1 => rotation_quat.mul_vec3(DVec3::new(0.0, 1.0, 0.0)), // Y axis
        2 => rotation_quat.mul_vec3(DVec3::new(0.0, 0.0, 1.0)), // Z axis
        _ => DVec3::new(1.0, 0.0, 0.0), // Default to X axis for invalid input
    };

    // Use get_closest_point_on_first_ray to find the offset along the axis
    // The first ray is along the axis direction starting from the gadget position
    // The second ray is the mouse ray
    get_closest_point_on_first_ray(
        pos,           // axis ray origin (gadget position)
        &axis_dir,     // axis ray direction
        ray_origin,    // mouse ray origin
        ray_direction  // mouse ray direction
    )
}

pub fn get_local_axis_direction(rotation_quat: DQuat, axis_index: i32) -> Option<DVec3> {
    match axis_index {
        0 => Some(rotation_quat.mul_vec3(DVec3::new(1.0, 0.0, 0.0))), // X axis
        1 => Some(rotation_quat.mul_vec3(DVec3::new(0.0, 1.0, 0.0))), // Y axis
        2 => Some(rotation_quat.mul_vec3(DVec3::new(0.0, 0.0, 1.0))), // Z axis
        _ => None, // Invalid axis index
    }
}
