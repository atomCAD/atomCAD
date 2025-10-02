use glam::f64::{DVec2, DVec3, DQuat};
use crate::common::csg_types::CSG;
use super::GeoNode;
use crate::util::transform::Transform;
use crate::structure_designer::utils::half_space_utils::HalfSpaceVisualization;
use crate::common::csg_utils::dvec3_to_point3;
use crate::common::csg_utils::dvec3_to_vector3;
use csgrs::polygon::Polygon;
use csgrs::vertex::Vertex;

impl GeoNode {
  pub fn to_csg(&self) -> CSG {
    self.internal_to_csg(true)
  }

  fn internal_to_csg(&self, is_root: bool) -> CSG {
    match self {
      GeoNode::HalfSpace { normal, center} => {
        Self::half_space_to_csg(*normal, *center, is_root)
      }
      GeoNode::HalfPlane { point1, point2 } => {
        Self::half_plane_to_csg(*point1, *point2)
      }
      GeoNode::Circle { center, radius } => {
        Self::circle_to_csg(*center, *radius)
      }
      GeoNode::Sphere { center, radius } => {
        Self::sphere_to_csg(*center, *radius)
      }
      GeoNode::Polygon { vertices } => {
        Self::polygon_to_csg(vertices)
      }
      GeoNode::Extrude { height, direction, shape } => {
        Self::extrude_to_csg(*height, *direction, shape)
      }
      GeoNode::Transform { transform, shape } => {
        Self::transform_to_csg(transform, shape)
      }
      GeoNode::Union2D { shapes } => {
        Self::union_2d_to_csg(shapes)
      }
      GeoNode::Intersection2D { shapes } => {
        Self::intersection_2d_to_csg(shapes)
      }
      GeoNode::Difference2D { base, sub } => {
        Self::difference_2d_to_csg(base, sub)
      }
      GeoNode::Union3D { shapes } => {
        Self::union_3d_to_csg(shapes)
      }
      GeoNode::Intersection3D { shapes } => {
        Self::intersection_3d_to_csg(shapes)
      }
      GeoNode::Difference3D { base, sub } => {
        Self::difference_3d_to_csg(base, sub)
      }
    }
  }

  fn half_space_to_csg(normal: DVec3, center: DVec3, is_root: bool) -> CSG {
    create_half_space_geo(
          &normal,
          &center,
          if is_root { HalfSpaceVisualization::Plane } else { HalfSpaceVisualization::Cuboid })
  }

  fn half_plane_to_csg(point1: DVec2, point2: DVec2) -> CSG {
    // Calculate direction vector from point1 to point2
    let dir_vector = point2 - point1;
    let dir = dir_vector.normalize();
    let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
    
    let center_pos = point1 + dir_vector * 0.5;
  
    let width = 100.0;
    let height = 100.0;
  
    let tr = center_pos - dir * width * 0.5 - normal * height;

    CSG::square(width, height, None)
    .rotate(0.0, 0.0, dir.y.atan2(dir.x).to_degrees())
    .translate(tr.x, tr.y, 0.0)
  }

  fn circle_to_csg(center: DVec2, radius: f64) -> CSG {
    CSG::circle(
      radius,
      32,
      None
    )
    .translate(center.x, center.y, 0.0)
  }

  fn sphere_to_csg(center: DVec3, radius: f64) -> CSG {
    CSG::sphere(
      radius,
      24,
      12,
      None
    )
      .translate(center.x, center.y, center.z)
  }

  fn polygon_to_csg(vertices: &Vec<DVec2>) -> CSG {
    let mut points: Vec<[f64; 2]> = Vec::new();
  
    for i in 0..vertices.len() {
        points.push([vertices[i].x, vertices[i].y]);
    }
  
    CSG::polygon(&points, None)
  }

  fn extrude_to_csg(height: f64, direction: DVec3, shape: &Box<GeoNode>) -> CSG {
      // Calculate the extrusion vector by multiplying height with normalized direction
      let extrusion_vector = dvec3_to_vector3(direction * height);
      
      // Use the new extrude_vector method instead of the old extrude method
      let mut extruded = shape.internal_to_csg(false).extrude_vector(extrusion_vector);

      // swap y and z coordinates to match atomCAD coordinate system
      for polygon in &mut extruded.polygons {        
        for vertex in &mut polygon.vertices {
            let tmp = vertex.pos.y;
            vertex.pos.y = vertex.pos.z;
            vertex.pos.z = tmp;

            let tmp_norm = vertex.normal.y;
            vertex.normal.y = vertex.normal.z;
            vertex.normal.z = tmp_norm;
        }
      }

      extruded.inverse()
  }

  fn transform_to_csg(transform: &Transform, shape: &Box<GeoNode>) -> CSG {
    // TODO: Implement transform to CSG conversion
    let euler_extrinsic_zyx = transform.rotation.to_euler(glam::EulerRot::ZYX);
    shape.internal_to_csg(false)
      .rotate(
        euler_extrinsic_zyx.2.to_degrees(), 
        euler_extrinsic_zyx.1.to_degrees(), 
        euler_extrinsic_zyx.0.to_degrees()
      )
      .translate(transform.translation.x, transform.translation.y, transform.translation.z)
  }

  fn union_2d_to_csg(shapes: &Vec<GeoNode>) -> CSG {
    if shapes.is_empty() {
      return CSG::new();
    }
    
    let mut result = shapes[0].internal_to_csg(false);
    for shape in shapes.iter().skip(1) {
      result = result.union(&shape.internal_to_csg(false));
    }
    result
  }

  fn intersection_2d_to_csg(shapes: &Vec<GeoNode>) -> CSG {
    if shapes.is_empty() {
      return CSG::new();
    }
    
    let mut result = shapes[0].internal_to_csg(false);
    for shape in shapes.iter().skip(1) {
      result = result.intersection(&shape.internal_to_csg(false));
    }
    result
  }

  fn difference_2d_to_csg(base: &Box<GeoNode>, sub: &Box<GeoNode>) -> CSG {
    let base_csg = base.internal_to_csg(false);
    let sub_csg = sub.internal_to_csg(false);
    base_csg.difference(&sub_csg)
  }

  fn union_3d_to_csg(shapes: &Vec<GeoNode>) -> CSG {
    if shapes.is_empty() {
      return CSG::new();
    }
    
    let mut result = shapes[0].internal_to_csg(false);
    for shape in shapes.iter().skip(1) {
      result = result.union(&shape.internal_to_csg(false));
    }
    result
  }

  fn intersection_3d_to_csg(shapes: &Vec<GeoNode>) -> CSG {
    if shapes.is_empty() {
      return CSG::new();
    }
    
    let mut result = shapes[0].internal_to_csg(false);
    for shape in shapes.iter().skip(1) {
      result = result.intersection(&shape.internal_to_csg(false));
    }
    result
  }

  fn difference_3d_to_csg(base: &Box<GeoNode>, sub: &Box<GeoNode>) -> CSG {
    let base_csg = base.internal_to_csg(false);
    let sub_csg = sub.internal_to_csg(false);
    base_csg.difference(&sub_csg)
  }
}


pub fn create_half_space_geo(normal: &DVec3, center_pos: &DVec3, visualization: HalfSpaceVisualization) -> CSG {
  let na_normal = dvec3_to_vector3(*normal);
  let rotation = DQuat::from_rotation_arc(DVec3::Y, *normal);

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
                Vertex::new(v1, na_normal),
                Vertex::new(v2, na_normal),
                Vertex::new(v3, na_normal),
                Vertex::new(v4, na_normal),
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
      let front_normal = na_normal;
      let back_normal = dvec3_to_vector3(-normal);
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
    .translate(center_pos.x, center_pos.y, center_pos.z);
}
