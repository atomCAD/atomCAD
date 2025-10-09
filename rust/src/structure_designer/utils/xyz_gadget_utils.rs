use glam::f64::DVec3;
use glam::f64::DQuat;
use glam::Vec3;
use crate::renderer::tessellator::tessellator;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;
use crate::util::hit_test_utils::{arrow_hit_test, get_closest_point_on_first_ray, cylinder_hit_test};

pub const AXIS_CYLINDER_LENGTH: f64 = 6.0;
pub const AXIS_CYLINDER_RADIUS: f64 = 0.12;
pub const AXIS_CONE_RADIUS: f64 = 0.2;
pub const AXIS_DIVISIONS: u32 = 16;
pub const AXIS_CONE_LENGTH: f64 = 0.5;
pub const AXIS_CONE_OFFSET: f64 = 0.1;

// Rotation handle constants
pub const ROTATION_HANDLE_RADIUS: f64 = 0.5;
pub const ROTATION_HANDLE_LENGTH: f64 = 1.4;
pub const ROTATION_HANDLE_OFFSET: f64 = 5.0;
pub const ROTATION_SENSITIVITY: f64 = 0.2; // radians per unit offset delta

pub fn tessellate_xyz_gadget(output_mesh: &mut Mesh, unit_cell: &UnitCellStruct, rotation_quat: DQuat, pos: &DVec3, include_rotation_handles: bool) {
  let x_axis_dir = rotation_quat.mul_vec3(unit_cell.a.normalize());
  let y_axis_dir = rotation_quat.mul_vec3(unit_cell.b.normalize());
  let z_axis_dir = rotation_quat.mul_vec3(unit_cell.c.normalize());

  if include_rotation_handles {
    tessellate_rotation_handle(output_mesh, &pos, &x_axis_dir, &Vec3::new(1.0, 0.0, 0.0));
    tessellate_rotation_handle(output_mesh, &pos, &y_axis_dir, &Vec3::new(0.0, 1.0, 0.0));
    tessellate_rotation_handle(output_mesh, &pos, &z_axis_dir, &Vec3::new(0.0, 0.0, 1.0));
    
    // Tessellate axis arrows starting from negative ROTATION_HANDLE_OFFSET with extended length
    let axis_start_offset = -ROTATION_HANDLE_OFFSET;
    let extended_length = AXIS_CYLINDER_LENGTH + ROTATION_HANDLE_OFFSET;
    tessellate_axis_arrow(output_mesh, &pos, &x_axis_dir, &Vec3::new(1.0, 0.0, 0.0), axis_start_offset, extended_length);
    tessellate_axis_arrow(output_mesh, &pos, &y_axis_dir, &Vec3::new(0.0, 1.0, 0.0), axis_start_offset, extended_length);
    tessellate_axis_arrow(output_mesh, &pos, &z_axis_dir, &Vec3::new(0.0, 0.0, 1.0), axis_start_offset, extended_length);
  } else {
    tessellate_axis_arrow(output_mesh, &pos, &x_axis_dir, &Vec3::new(1.0, 0.0, 0.0), 0.0, AXIS_CYLINDER_LENGTH);
    tessellate_axis_arrow(output_mesh, &pos, &y_axis_dir, &Vec3::new(0.0, 1.0, 0.0), 0.0, AXIS_CYLINDER_LENGTH);
    tessellate_axis_arrow(output_mesh, &pos, &z_axis_dir, &Vec3::new(0.0, 0.0, 1.0), 0.0, AXIS_CYLINDER_LENGTH);
  }
}

pub fn tessellate_axis_arrow(output_mesh: &mut Mesh, start_pos: &DVec3, axis_dir: &DVec3, albedo: &Vec3, start_offset: f64, cylinder_length: f64) {
  let offset_start_pos = *start_pos + *axis_dir * start_offset;
  tessellator::tessellate_arrow(
    output_mesh,
    &offset_start_pos,
    axis_dir,
    AXIS_CYLINDER_RADIUS,
    AXIS_CONE_RADIUS,
    AXIS_DIVISIONS,
    cylinder_length,
    AXIS_CONE_LENGTH,
    AXIS_CONE_OFFSET,
    &Material::new(albedo, 0.4, 0.8),
);
}

pub fn tessellate_rotation_handle(output_mesh: &mut Mesh, start_pos: &DVec3, axis_dir: &DVec3, albedo: &Vec3) {
  let handle_center = *start_pos - *axis_dir * ROTATION_HANDLE_OFFSET;
  let handle_top = handle_center + *axis_dir * (ROTATION_HANDLE_LENGTH * 0.5);
  let handle_bottom = handle_center - *axis_dir * (ROTATION_HANDLE_LENGTH * 0.5);
  
  let material = Material::new(albedo, 0.4, 0.8);
  tessellator::tessellate_cylinder(
    output_mesh,
    &handle_top,
    &handle_bottom,
    ROTATION_HANDLE_RADIUS,
    AXIS_DIVISIONS,
    &material,
    true, // include_top_and_bottom
    Some(&material),  // top_material
    Some(&material)   // bottom_material
  );
}

pub fn rotation_handle_hit_test(
    start_pos: &DVec3,
    axis_dir: &DVec3,
    ray_origin: &DVec3,
    ray_direction: &DVec3
) -> Option<f64> {
    let handle_center = *start_pos - *axis_dir * ROTATION_HANDLE_OFFSET;
    let handle_top = handle_center + *axis_dir * (ROTATION_HANDLE_LENGTH * 0.5);
    let handle_bottom = handle_center - *axis_dir * (ROTATION_HANDLE_LENGTH * 0.5);
    
    cylinder_hit_test(
        &handle_top,
        &handle_bottom,
        ROTATION_HANDLE_RADIUS,
        ray_origin,
        ray_direction
    )
}

pub fn axis_arrow_hit_test(
    start_pos: &DVec3,
    axis_dir: &DVec3,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
    start_offset: f64,
    cylinder_length: f64
) -> Option<f64> {
    let offset_start_pos = *start_pos + *axis_dir * start_offset;
    arrow_hit_test(
        &offset_start_pos,
        axis_dir,
        AXIS_CYLINDER_RADIUS,
        AXIS_CONE_RADIUS,
        cylinder_length,
        AXIS_CONE_LENGTH,
        AXIS_CONE_OFFSET,
        ray_origin,
        ray_direction
    )
}

pub fn xyz_gadget_hit_test(
    unit_cell: &UnitCellStruct,
    rotation_quat: DQuat,
    pos: &DVec3,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
    include_rotation_handles: bool
) -> Option<i32> {
    let x_axis_dir = rotation_quat.mul_vec3(unit_cell.a.normalize());
    let y_axis_dir = rotation_quat.mul_vec3(unit_cell.b.normalize());
    let z_axis_dir = rotation_quat.mul_vec3(unit_cell.c.normalize());

    // Test hit against axis arrows (parameters depend on whether rotation handles are enabled)
    let (axis_start_offset, axis_cylinder_length) = if include_rotation_handles {
        (-ROTATION_HANDLE_OFFSET, AXIS_CYLINDER_LENGTH + ROTATION_HANDLE_OFFSET)
    } else {
        (0.0, AXIS_CYLINDER_LENGTH)
    };

    let x_hit = axis_arrow_hit_test(
        pos,
        &x_axis_dir,
        ray_origin,
        ray_direction,
        axis_start_offset,
        axis_cylinder_length
    );

    let y_hit = axis_arrow_hit_test(
        pos,
        &y_axis_dir,
        ray_origin,
        ray_direction,
        axis_start_offset,
        axis_cylinder_length
    );

    let z_hit = axis_arrow_hit_test(
        pos,
        &z_axis_dir,
        ray_origin,
        ray_direction,
        axis_start_offset,
        axis_cylinder_length
    );

    // Test rotation handles if enabled
    let mut rotation_hits = [None, None, None];
    if include_rotation_handles {
        rotation_hits[0] = rotation_handle_hit_test(&pos, &x_axis_dir, ray_origin, ray_direction);
        rotation_hits[1] = rotation_handle_hit_test(&pos, &y_axis_dir, ray_origin, ray_direction);
        rotation_hits[2] = rotation_handle_hit_test(&pos, &z_axis_dir, ray_origin, ray_direction);
    }

    // Find the closest hit and return its axis index
    // Rotation handles have priority over axis handles (checked first)
    let mut closest_distance: Option<f64> = None;
    let mut closest_axis: Option<i32> = None;
    
    // Check rotation handle hits first (indices 3, 4, 5 for x, y, z rotation handles)
    if include_rotation_handles {
        let rotation_hit_data = [(rotation_hits[0], 3), (rotation_hits[1], 4), (rotation_hits[2], 5)];
        
        for (hit, axis_index) in rotation_hit_data.iter() {
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
    }
    
    // Then check axis hits (indices 0, 1, 2 for x, y, z axes)
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
    unit_cell: &UnitCellStruct,
    rotation_quat: DQuat,
    pos: &DVec3,
    dragged_axis_index: i32,
    ray_origin: &DVec3,
    ray_direction: &DVec3
) -> f64 {
    // Get the axis direction based on the dragged axis index
    // Rotation handles (3, 4, 5) map to the same axes as translation handles (0, 1, 2)
    let axis_dir = match dragged_axis_index {
        0 | 3 => rotation_quat.mul_vec3(unit_cell.a.normalize()), // X axis
        1 | 4 => rotation_quat.mul_vec3(unit_cell.b.normalize()), // Y axis
        2 | 5 => rotation_quat.mul_vec3(unit_cell.c.normalize()), // Z axis
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

pub fn get_local_axis_direction(unit_cell: &UnitCellStruct, rotation_quat: DQuat, axis_index: i32) -> Option<DVec3> {
    match axis_index {
        0 => Some(rotation_quat.mul_vec3(unit_cell.a.normalize())), // X axis
        1 => Some(rotation_quat.mul_vec3(unit_cell.b.normalize())), // Y axis
        2 => Some(rotation_quat.mul_vec3(unit_cell.c.normalize())), // Z axis
        _ => None, // Invalid axis index
    }
}
