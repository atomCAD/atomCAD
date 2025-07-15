use crate::common::csg_types::CSG;
use glam::i32::IVec3;
use glam::DQuat;
use glam::DVec3;
use crate::common::csg_utils::dvec3_to_point3;
use crate::common::csg_utils::dvec3_to_vector3;
use csgrs::polygon::Polygon;
use csgrs::vertex::Vertex;

/// Visualization type for the half space
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HalfSpaceVisualization {
    /// Visualize as a plane (square)
    Plane,
    /// Visualize as a cuboid
    Cuboid,
}

pub fn create_half_space_geo(miller_index: &IVec3, center: &IVec3, shift: i32, visualization: HalfSpaceVisualization) -> CSG {
  let dir = miller_index.as_dvec3().normalize();
  let center_pos = center.as_dvec3();

  // Calculate the d-spacing (interplanar spacing) based on Miller indices
  // Formula: d = 1 / √(h² + k² + l²) in normalized space where unit cell = 1
  let miller_length = miller_index.as_dvec3().length();
  let d_spacing = if miller_length > 0.0 {
    1.0 / miller_length
  } else {
    1.0 // Default to 1.0 if Miller indices are all zero
  };

  // Apply the shift along the normal direction, using d-spacing as the unit
  let shift_distance = shift as f64 * d_spacing;
  let shifted_center = center_pos + dir * shift_distance;

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
  
  // Calculate the d-spacing (interplanar spacing) based on Miller indices
  // Formula: d = 1 / √(h² + k² + l²) in normalized space where unit cell = 1
  let d_spacing = if miller_magnitude > 0.0 {
    1.0 / miller_magnitude
  } else {
    1.0 // Default to 1.0 if Miller indices are all zero
  };
  
  // Apply the shift along the normal direction, using d-spacing as the unit
  let normalized_dir = float_miller / miller_magnitude;
  let shift_distance = shift as f64 * d_spacing;
  let shifted_center = center_pos + normalized_dir * shift_distance;
  
  // Calculate the signed distance from the point to the plane defined by the normal (miller_index) and shifted center point
  return float_miller.dot(*sample_point - shifted_center) / miller_magnitude;
}
