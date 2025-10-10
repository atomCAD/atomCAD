use csgrs::traits::CSG;
use glam::f64::{DVec2, DVec3, DQuat};
use nalgebra::{Point3, Vector3};
use crate::common::csg_types::CSGMesh;
use crate::common::csg_types::CSGSketch;
use super::GeoNode;
use crate::util::transform::Transform;
use crate::common::csg_utils::dvec3_to_point3;
use crate::common::csg_utils::dvec3_to_vector3;
use csgrs::mesh::polygon::Polygon;
use csgrs::mesh::vertex::Vertex;

impl GeoNode {
  pub fn to_csg_mesh(&self) -> Option<CSGMesh> {
    self.internal_to_csg_mesh(true)
  }

  pub fn to_csg_sketch(&self) -> Option<CSGSketch> {
    match self {
      GeoNode::HalfPlane { point1, point2 } => {
        Some(Self::half_plane_to_csg(*point1, *point2))
      }
      GeoNode::Circle { center, radius } => {
        Some(Self::circle_to_csg(*center, *radius))
      }
      GeoNode::Polygon { vertices } => {
        Some(Self::polygon_to_csg(vertices))
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
      _ => None
    }    
  }

  fn internal_to_csg_mesh(&self, is_root: bool) -> Option<CSGMesh> {
    match self {
      GeoNode::HalfSpace { normal, center} => {
        Some(Self::half_space_to_csg(*normal, *center, is_root))
      }
      GeoNode::Sphere { center, radius } => {
        Some(Self::sphere_to_csg(*center, *radius))
      }
      GeoNode::Extrude { height, direction, shape } => {
        Self::extrude_to_csg(*height, *direction, shape)
      }
      GeoNode::Transform { transform, shape } => {
        Self::transform_to_csg(transform, shape)
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
      _ => None
    }
  }

  fn half_space_to_csg(normal: DVec3, center: DVec3, is_root: bool) -> CSGMesh {
    create_half_space_geo(
          &normal,
          &center,
          is_root)
  }

  fn half_plane_to_csg(point1: DVec2, point2: DVec2) -> CSGSketch {
    // Calculate direction vector from point1 to point2
    let dir_vector = point2 - point1;
    let dir = dir_vector.normalize();
    let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
    
    let center_pos = point1 + dir_vector * 0.5;
  
    let width = 400.0;
    let height = 400.0;
  
    let tr = center_pos - dir * width * 0.5 - normal * height;

    CSGSketch::rectangle(width, height, None)
    .rotate(0.0, 0.0, dir.y.atan2(dir.x).to_degrees())
    .translate(tr.x, tr.y, 0.0)
  }

  fn circle_to_csg(center: DVec2, radius: f64) -> CSGSketch {
    CSGSketch::circle(
      radius,
      32,
      None
    )
    .translate(center.x, center.y, 0.0)
  }

  fn sphere_to_csg(center: DVec3, radius: f64) -> CSGMesh {
    CSGMesh::sphere(
      radius,
      24,
      12,
      None
    )
      .translate(center.x, center.y, center.z)
  }

  fn polygon_to_csg(vertices: &Vec<DVec2>) -> CSGSketch {
    let mut points: Vec<[f64; 2]> = Vec::new();
  
    for i in 0..vertices.len() {
        points.push([vertices[i].x, vertices[i].y]);
    }
  
    CSGSketch::polygon(&points, None)
  }

  fn extrude_to_csg(height: f64, direction: DVec3, shape: &Box<GeoNode>) -> Option<CSGMesh> {
      // Calculate the extrusion vector by multiplying height with normalized direction
      let extrusion_vector = dvec3_to_vector3(direction * height);
      
      // Convert atomCAD extrusion vector (XZ plane, Y up) to csgrs (XY plane, Z up)
      // This requires a -90° rotation around X-axis: (x, y, z) -> (x, z, -y)
      let csgrs_extrusion = Vector3::new(extrusion_vector.x, extrusion_vector.z, -extrusion_vector.y);

      // Use the new extrude_vector method instead of the old extrude method
      let sketch = shape.to_csg_sketch()?;
      let mut extruded = sketch.extrude_vector(csgrs_extrusion);

      // Convert result back from csgrs coordinate system to atomCAD
      // This requires a +90° rotation around X-axis: (x, y, z) -> (x, -z, y)
      for polygon in &mut extruded.polygons {        
        for vertex in &mut polygon.vertices {
            let csgrs_pos = vertex.pos;
            vertex.pos = Point3::new(csgrs_pos.x, -csgrs_pos.z, csgrs_pos.y);

            let csgrs_normal = vertex.normal;
            vertex.normal = Vector3::new(csgrs_normal.x, -csgrs_normal.z, csgrs_normal.y);
        }
        
        // Also fix the polygon's plane definition with the same rotation
        let plane_a = polygon.plane.point_a;
        let plane_b = polygon.plane.point_b; 
        let plane_c = polygon.plane.point_c;
        
        polygon.plane.point_a = Point3::new(plane_a.x, -plane_a.z, plane_a.y);
        polygon.plane.point_b = Point3::new(plane_b.x, -plane_b.z, plane_b.y);
        polygon.plane.point_c = Point3::new(plane_c.x, -plane_c.z, plane_c.y);
      }

      Some(extruded.inverse())
  }

  fn transform_to_csg(transform: &Transform, shape: &Box<GeoNode>) -> Option<CSGMesh> {
    // TODO: Implement transform to CSG conversion
    let euler_extrinsic_zyx = transform.rotation.to_euler(glam::EulerRot::ZYX);
    let mesh = shape.internal_to_csg_mesh(false)?;
    Some(mesh
      .rotate(
        euler_extrinsic_zyx.2.to_degrees(), 
        euler_extrinsic_zyx.1.to_degrees(), 
        euler_extrinsic_zyx.0.to_degrees()
      )
      .translate(transform.translation.x, transform.translation.y, transform.translation.z))
  }

  fn union_2d_to_csg(shapes: &Vec<GeoNode>) -> Option<CSGSketch> {
    if shapes.is_empty() {
      return Some(CSGSketch::new());
    }
    
    let mut result = shapes[0].to_csg_sketch()?;
    for shape in shapes.iter().skip(1) {
      result = result.union(&shape.to_csg_sketch()?);
    }
    Some(result)
  }

  fn intersection_2d_to_csg(shapes: &Vec<GeoNode>) -> Option<CSGSketch> {
    if shapes.is_empty() {
      return Some(CSGSketch::new());
    }
    
    let mut result = shapes[0].to_csg_sketch()?;
    for shape in shapes.iter().skip(1) {
      result = result.intersection(&shape.to_csg_sketch()?);
    }
    Some(result)
  }

  fn difference_2d_to_csg(base: &Box<GeoNode>, sub: &Box<GeoNode>) -> Option<CSGSketch> {
    let base_csg = base.to_csg_sketch()?;
    let sub_csg = sub.to_csg_sketch()?;
    Some(base_csg.difference(&sub_csg))
  }

  fn union_3d_to_csg(shapes: &Vec<GeoNode>) -> Option<CSGMesh> {
    if shapes.is_empty() {
      return Some(CSGMesh::new());
    }
    
    let mut result = shapes[0].internal_to_csg_mesh(false)?;
    for shape in shapes.iter().skip(1) {
      result = result.union(&shape.internal_to_csg_mesh(false)?);
    }
    Some(result)
  }

  fn intersection_3d_to_csg(shapes: &Vec<GeoNode>) -> Option<CSGMesh> {
    if shapes.is_empty() {
      return Some(CSGMesh::new());
    }
    
    let mut result = shapes[0].internal_to_csg_mesh(false)?;
    for shape in shapes.iter().skip(1) {
      let shape_mesh = shape.internal_to_csg_mesh(false)?;
      result = result.intersection(&shape_mesh);
    }
    Some(result)
  }

  fn difference_3d_to_csg(base: &Box<GeoNode>, sub: &Box<GeoNode>) -> Option<CSGMesh> {
    let base_csg = base.internal_to_csg_mesh(false)?;
    let sub_csg = sub.internal_to_csg_mesh(false)?;
    Some(base_csg.difference(&sub_csg))
  }
}

pub fn create_half_space_geo(normal: &DVec3, center_pos: &DVec3, is_root: bool) -> CSGMesh {
  let na_normal = dvec3_to_vector3(*normal);
  let rotation = DQuat::from_rotation_arc(DVec3::Y, *normal);

  let width : f64 = if is_root { 100.0 } else { 400.0 };
  let height : f64 = if is_root { 100.0 } else { 400.0 };

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
  let polygons = 
      vec![
        Polygon::new(
            vec![
                Vertex::new(v1, na_normal),
                Vertex::new(v2, na_normal),
                Vertex::new(v3, na_normal),
                Vertex::new(v4, na_normal),
            ], None
        ),
      ];

  return CSGMesh::from_polygons(&polygons, None)
    .translate(center_pos.x, center_pos.y, center_pos.z);
}

