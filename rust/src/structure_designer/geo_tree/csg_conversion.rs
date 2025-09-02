use glam::i32::{IVec2, IVec3};
use glam::f64::DVec2;
use crate::common::csg_types::CSG;
use super::GeoNode;
use crate::util::transform::Transform;
use crate::structure_designer::utils::half_space_utils::create_half_space_geo;
use crate::structure_designer::utils::half_space_utils::HalfSpaceVisualization;

impl GeoNode {
  pub fn to_csg(&self) -> CSG {
    self.internal_to_csg(true)
  }

  fn internal_to_csg(&self, is_root: bool) -> CSG {
    match self {
      GeoNode::HalfSpace { miller_index, center, shift } => {
        Self::half_space_to_csg(*miller_index, *center, *shift, is_root)
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
      GeoNode::Rect { min_corner, extent } => {
        Self::rect_to_csg(*min_corner, *extent)
      }
      GeoNode::Cuboid { min_corner, extent } => {
        Self::cuboid_to_csg(*min_corner, *extent)
      }
      GeoNode::Polygon { vertices } => {
        Self::polygon_to_csg(vertices)
      }
      GeoNode::Extrude { height, shape } => {
        Self::extrude_to_csg(*height, shape)
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

  fn half_space_to_csg(miller_index: IVec3, center: IVec3, shift: i32, is_root: bool) -> CSG {
    create_half_space_geo(
          &miller_index,
          &center,
          shift,
          if is_root { HalfSpaceVisualization::Plane } else { HalfSpaceVisualization::Cuboid })
  }

  fn half_plane_to_csg(point1: IVec2, point2: IVec2) -> CSG {
    let real_point1 = point1.as_dvec2();

    // Calculate direction vector from point1 to point2
    let dir_vector = point2.as_dvec2() - real_point1;
    let dir = dir_vector.normalize();
    let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
    
    let center_pos = real_point1 + dir_vector * 0.5;
  
    let width = 100.0;
    let height = 100.0;
  
    let tr = center_pos - dir * width * 0.5 - normal * height;

    CSG::square(width, height, None)
    .rotate(0.0, 0.0, dir.y.atan2(dir.x).to_degrees())
    .translate(tr.x, tr.y, 0.0)
  }

  fn circle_to_csg(center: IVec2, radius: i32) -> CSG {
    let real_center = center.as_dvec2();
  
    CSG::circle(
      radius as f64,
      32,
      None
    )
    .translate(real_center.x, real_center.y, 0.0)
  }

  fn sphere_to_csg(center: IVec3, radius: i32) -> CSG {
    let real_center = center.as_dvec3();
    CSG::sphere(
      radius as f64,
      24,
      12,
      None
    )
      .translate(real_center.x, real_center.y, real_center.z)
  }

  fn rect_to_csg(min_corner: IVec2, extent: IVec2) -> CSG {
    let real_min_corner = min_corner.as_dvec2();
    let real_extent = extent.as_dvec2();
  
    CSG::square(real_extent.x, real_extent.y, None)
      .translate(real_min_corner.x, real_min_corner.y, 0.0)
  }

  fn cuboid_to_csg(min_corner: IVec3, extent: IVec3) -> CSG {
    let real_min_corner = min_corner.as_dvec3();
    let real_extent = extent.as_dvec3();
  
    CSG::cube(real_extent.x, real_extent.y, real_extent.z, None)
      .translate(real_min_corner.x, real_min_corner.y, real_min_corner.z)
  }

  fn polygon_to_csg(vertices: &Vec<IVec2>) -> CSG {
    let mut points: Vec<[f64; 2]> = Vec::new();
  
    for i in 0..vertices.len() {
        points.push([vertices[i].x as f64, vertices[i].y as f64]);
    }
  
    CSG::polygon(&points, None)
  }

  fn extrude_to_csg(height: i32, shape: &Box<GeoNode>) -> CSG {
      let mut extruded = shape.internal_to_csg(false).extrude(height as f64);

      // swap y and z coordinates
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
