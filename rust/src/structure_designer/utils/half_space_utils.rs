use crate::common::csg_types::CSG;
use glam::i32::IVec3;
use glam::f32::Vec3;
use glam::DQuat;
use glam::DVec3;
use crate::common::csg_utils::dvec3_to_point3;
use crate::common::csg_utils::dvec3_to_vector3;
use csgrs::polygon::Polygon;
use csgrs::vertex::Vertex;
use crate::util::hit_test_utils::get_closest_point_on_first_ray;
use crate::structure_designer::common_constants;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::renderer::tessellator::tessellator;
use std::collections::HashSet;

pub const CENTER_SPHERE_RADIUS: f64 = 0.25;
pub const CENTER_SPHERE_HORIZONTAL_DIVISIONS: u32 = 16;
pub const CENTER_SPHERE_VERTICAL_DIVISIONS: u32 = 16;

// Constants for shift drag handle
pub const SHIFT_HANDLE_ACCESSIBILITY_OFFSET: f64 = 3.0;
pub const SHIFT_HANDLE_AXIS_RADIUS: f64 = 0.1;
pub const SHIFT_HANDLE_CYLINDER_RADIUS: f64 = 0.3;
pub const SHIFT_HANDLE_CYLINDER_LENGTH: f64 = 1.0;
pub const SHIFT_HANDLE_DIVISIONS: u32 = 16;

// Constants for miller index disc visualization
pub const MILLER_INDEX_DISC_DISTANCE: f64 = 5.0; // Distance from center to place discs
pub const MILLER_INDEX_DISC_RADIUS: f64 = 0.5;   // Radius of each disc
pub const MILLER_INDEX_DISC_THICKNESS: f64 = 0.06; // Thickness of each disc
pub const MILLER_INDEX_DISC_DIVISIONS: u32 = 16;  // Number of divisions for disc cylinder

/// Visualization type for the half space
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HalfSpaceVisualization {
    /// Visualize as a plane (square)
    Plane,
    /// Visualize as a cuboid
    Cuboid,
}

/// Calculate the vector by which to shift along miller index normal direction,
/// with magnitude based on the miller index d-spacing
pub fn calculate_shift_vector(miller_index: &IVec3, shift: f64) -> DVec3 {
  let float_miller = miller_index.as_dvec3();
  let miller_magnitude = float_miller.length();
  
  // Avoid division by zero
  if miller_magnitude <= 0.0 {
    return DVec3::ZERO;
  }
  
  // Calculate the d-spacing (interplanar spacing) based on Miller indices
  // Formula: d = 1 / √(h² + k² + l²) in normalized space where unit cell = 1
  let d_spacing = 1.0 / miller_magnitude;
  
  // Calculate the normalized direction vector
  let normalized_dir = float_miller / miller_magnitude;
  
  // Calculate shift distance along normal direction
  let shift_distance = shift * d_spacing;
  
  // Return the shift vector
  normalized_dir * shift_distance
}

// Calculate the continuous shift value of a half space based on its centerm miller index and
// a mouse ray. The handle offset is the distance from the plane center to the handle.
// Useful dragging a half plane shift handle.
pub fn get_dragged_shift(miller_index: &IVec3, center: &IVec3, ray_origin: &DVec3, ray_direction: &DVec3, handle_offset: f64) -> f64 {
    let center_pos = center.as_dvec3() * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);

    let float_miller = miller_index.as_dvec3();
    let miller_magnitude = float_miller.length();
            
    // Avoid division by zero
    if miller_magnitude <= 0.0 {
        return 0.0;
    }
   
    // Calculate the normalized direction vector
    let normal_dir = float_miller / miller_magnitude;

    // Find where on the 'normal ray' the mouse ray is closest
    let distance_along_normal = get_closest_point_on_first_ray(
        &center_pos,
        &normal_dir,
        &ray_origin,
        &ray_direction
    );
      
    // Convert the world distance to cell-space distance
    let cell_space_distance = (distance_along_normal - handle_offset) / (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);

    // (* miller_magnitude) is equivalent to (/ d-spacing)
    return cell_space_distance * miller_magnitude;
}

pub fn create_half_space_geo(miller_index: &IVec3, center: &IVec3, shift: i32, visualization: HalfSpaceVisualization) -> CSG {
  let dir = miller_index.as_dvec3().normalize();
  let center_pos = center.as_dvec3();

  // Apply the shift along the normal direction
  let shift_vector = calculate_shift_vector(miller_index, shift as f64);
  let shifted_center = center_pos + shift_vector;

  let normal = dvec3_to_vector3(dir);
  let rotation = DQuat::from_rotation_arc(DVec3::Y, dir);

  let width = 40.0;
  let height = 40.0;
  let depth = 40.0; // Depth for cuboid visualization

  let start_x = -width * 0.5;
  let start_z = -height * 0.5;
  let end_x = width * 0.5;
  let end_z = height * 0.5;

  // Front face vertices (at y=0)
  let v1 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(start_x, 0.0, start_z)));
  let v2 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(start_x, 0.0, end_z)));
  let v3 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(end_x, 0.0, end_z)));
  let v4 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(end_x, 0.0, start_z)));

  // Create polygons based on the visualization type
  let polygons = match visualization {
    HalfSpaceVisualization::Plane => {
      // Single plane representation (original)
      vec![
        Polygon::new(
            vec![
                Vertex::new(v1, normal),
                Vertex::new(v2, normal),
                Vertex::new(v3, normal),
                Vertex::new(v4, normal),
            ], None
        ),
      ]
    },
    HalfSpaceVisualization::Cuboid => {
      // Back face vertices (at y=-depth)
      let v5 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(start_x, -depth, start_z)));
      let v6 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(start_x, -depth, end_z)));
      let v7 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(end_x, -depth, end_z)));
      let v8 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(end_x, -depth, start_z)));

      // Calculate normals for each face
      let front_normal = normal;
      let back_normal = dvec3_to_vector3(-dir);
      let left_normal = dvec3_to_vector3(rotation.mul_vec3(DVec3::new(-1.0, 0.0, 0.0)));
      let right_normal = dvec3_to_vector3(rotation.mul_vec3(DVec3::new(1.0, 0.0, 0.0)));
      let top_normal = dvec3_to_vector3(rotation.mul_vec3(DVec3::new(0.0, 0.0, 1.0)));
      let bottom_normal = dvec3_to_vector3(rotation.mul_vec3(DVec3::new(0.0, 0.0, -1.0)));

      vec![
        // Front face (original plane)
        Polygon::new(
            vec![
                Vertex::new(v1, front_normal),
                Vertex::new(v2, front_normal),
                Vertex::new(v3, front_normal),
                Vertex::new(v4, front_normal),
            ], None
        ),
        // Back face
        Polygon::new(
            vec![
                Vertex::new(v8, back_normal),
                Vertex::new(v7, back_normal),
                Vertex::new(v6, back_normal),
                Vertex::new(v5, back_normal),
            ], None
        ),
        // Left face
        Polygon::new(
            vec![
                Vertex::new(v5, left_normal),
                Vertex::new(v6, left_normal),
                Vertex::new(v2, left_normal),
                Vertex::new(v1, left_normal),
            ], None
        ),
        // Right face
        Polygon::new(
            vec![
                Vertex::new(v4, right_normal),
                Vertex::new(v3, right_normal),
                Vertex::new(v7, right_normal),
                Vertex::new(v8, right_normal),
            ], None
        ),
        // Top face
        Polygon::new(
            vec![
                Vertex::new(v2, top_normal),
                Vertex::new(v6, top_normal),
                Vertex::new(v7, top_normal),
                Vertex::new(v3, top_normal),
            ], None
        ),
        // Bottom face
        Polygon::new(
            vec![
                Vertex::new(v1, bottom_normal),
                Vertex::new(v4, bottom_normal),
                Vertex::new(v8, bottom_normal),
                Vertex::new(v5, bottom_normal),
            ], None
        ),
      ]
    },
  };

  return CSG::from_polygons(&polygons)
    .translate(shifted_center.x, shifted_center.y, shifted_center.z);
}

pub fn implicit_eval_half_space_calc(
  miller_index: &IVec3, center: &IVec3, shift: i32,
  sample_point: &DVec3) -> f64 {
  let float_miller = miller_index.as_dvec3();
  let miller_magnitude = float_miller.length();
  let center_pos = center.as_dvec3();
  
  // Apply the shift along the normal direction, using the common function
  let shift_vector = calculate_shift_vector(miller_index, shift as f64);
  let shifted_center = center_pos + shift_vector;
  
  // Calculate the signed distance from the point to the plane defined by the normal (miller_index) and shifted center point
  return float_miller.dot(*sample_point - shifted_center) / miller_magnitude;
}

/// Tessellates discs representing each possible miller index
/// These discs are positioned at a fixed distance from the center in the direction of each miller index
/// The current miller index disc is highlighted with a yellowish-orange color
pub fn tessellate_miller_indices_discs(
    output_mesh: &mut Mesh,
    center_pos: &DVec3,
    miller_index: &IVec3,
    possible_miller_indices: &HashSet<IVec3>,
    max_miller_index: i32,
) {
    // Material for regular discs - blue color
    let disc_material = Material::new(&Vec3::new(0.0, 0.3, 0.9), 0.3, 0.0);
        
    // Material for the current miller index disc - yellowish orange color
    let current_disc_material = Material::new(&Vec3::new(1.0, 0.6, 0.0), 0.3, 0.0);
        
    // Create a red material for the inside/bottom face of regular discs
    let red_material = Material::new(&Vec3::new(0.95, 0.0, 0.0), 0.3, 0.0);

    // Get the simplified version of the current miller index for comparison
    let simplified_current_miller = simplify_miller_index(*miller_index);

    // Iterate through all possible miller indices
    for miller_index in possible_miller_indices {
        // Get the normalized direction for this miller index
        let direction = miller_index.as_dvec3().normalize();
            
        // Calculate the position for the disc
        let disc_center = *center_pos + direction * MILLER_INDEX_DISC_DISTANCE;
            
        // Calculate start and end points for the disc (thin cylinder)
        let disc_start = disc_center - direction * (MILLER_INDEX_DISC_THICKNESS * 0.5);
        let disc_end = disc_center + direction * (MILLER_INDEX_DISC_THICKNESS * 0.5);
            
        // Get the dynamic disc radius based on the max miller index
        let disc_radius = get_miller_index_disc_radius(max_miller_index);
            
        // Check if this is the current miller index (compare simplified forms)
        let is_current = *miller_index == simplified_current_miller;
            
        // Choose material based on whether this is the current miller index
        let material = if is_current {
            &current_disc_material
        } else {
            &disc_material
        };

        // Tessellate the disc
        tessellator::tessellate_cylinder(
            output_mesh,
            &disc_start,
            &disc_end,
            disc_radius,
            MILLER_INDEX_DISC_DIVISIONS,
            material,
            true, // Cap the ends
            // If current disc, use the same orange material for top face
            // Otherwise use red material for inside/bottom face
            if is_current { Some(material) } else { Some(&red_material) },
            None,
        );
    }
}

pub fn simplify_miller_index(miller_index: IVec3) -> IVec3 {
    // Get absolute values for checking divisibility
    let abs_x = miller_index.x.abs();
    let abs_y = miller_index.y.abs();
    let abs_z = miller_index.z.abs();

    // Set max_divisor to the maximum of the absolute values of the components
    // This is an optimization as we don't need to check divisors larger than the largest component
    let max_divisor = abs_x.max(abs_y).max(abs_z);
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

pub fn get_miller_index_disc_radius(max_miller_index: i32) -> f64 {
    let divisor = f64::max(max_miller_index as f64 - 1.0, 1.0);
    MILLER_INDEX_DISC_RADIUS / divisor
}