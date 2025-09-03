use super::GeoNode;
use glam::f64::{DVec2, DVec3};
use glam::i32::{IVec2, IVec3};
use crate::util::transform::Transform;
use crate::structure_designer::utils::half_space_utils::implicit_eval_half_space_calc;

impl GeoNode {
  pub fn implicit_eval_3d(&self, sample_point: &DVec3) -> f64 {
    match self {
      GeoNode::HalfSpace { miller_index, center, shift } => {
        Self::half_space_implicit_eval(*miller_index, *center, *shift, sample_point)
      }
      GeoNode::Sphere { center, radius } => {
        Self::sphere_implicit_eval(*center, *radius, sample_point)
      }
      GeoNode::Cuboid { min_corner, extent } => {
        Self::cuboid_implicit_eval(*min_corner, *extent, sample_point)
      }
      GeoNode::Extrude { height, shape } => {
        Self::extrude_implicit_eval(*height, shape, sample_point)
      }
      GeoNode::Transform { transform, shape } => {
        Self::transform_implicit_eval(transform, shape, sample_point)
      }
      GeoNode::Union3D { shapes } => {
        Self::union_3d_implicit_eval(shapes, sample_point)
      }
      GeoNode::Intersection3D { shapes } => {
        Self::intersection_3d_implicit_eval(shapes, sample_point)
      }
      GeoNode::Difference3D { base, sub } => {
        Self::difference_3d_implicit_eval(base, sub, sample_point)
      }
      // 2D shapes should use implicit_eval_2d instead
      _ => panic!("2D shapes should be evaluated using implicit_eval_2d")
    }
  }

  pub fn implicit_eval_2d(&self, sample_point: &DVec2) -> f64 {
    match self {
      GeoNode::HalfPlane { point1, point2 } => {
        Self::half_plane_implicit_eval(*point1, *point2, sample_point)
      }
      GeoNode::Circle { center, radius } => {
        Self::circle_implicit_eval(*center, *radius, sample_point)
      }
      GeoNode::Rect { min_corner, extent } => {
        Self::rect_implicit_eval(*min_corner, *extent, sample_point)
      }
      GeoNode::Polygon { vertices } => {
        Self::polygon_implicit_eval(vertices, sample_point)
      }
      GeoNode::Union2D { shapes } => {
        Self::union_2d_implicit_eval(shapes, sample_point)
      }
      GeoNode::Intersection2D { shapes } => {
        Self::intersection_2d_implicit_eval(shapes, sample_point)
      }
      GeoNode::Difference2D { base, sub } => {
        Self::difference_2d_implicit_eval(base, sub, sample_point)
      }
      // 3D shapes should use implicit_eval_3d instead
      _ => panic!("3D shapes should be evaluated using implicit_eval_3d")
    }
  }

  fn half_space_implicit_eval(miller_index: IVec3, center: IVec3, shift: i32, sample_point: &DVec3) -> f64 {
    implicit_eval_half_space_calc(&miller_index, &center, shift, sample_point)
  }

  fn half_plane_implicit_eval(point1: IVec2, point2: IVec2, sample_point: &DVec2) -> f64 {
    // Convert points to double precision for calculations
    let point1 = point1.as_dvec2();
    let point2 = point2.as_dvec2();
    
    // Calculate line direction and normal
    let dir_vector = point2 - point1;
    let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
    
    // Calculate signed distance from sample_point to the line
    // Formula: distance = normal·(sample_point - point1)
    normal.dot(*sample_point - point1)
  }

  fn circle_implicit_eval(center: IVec2, radius: i32, sample_point: &DVec2) -> f64 {
    let center_f64 = DVec2::new(center.x as f64, center.y as f64);
    (sample_point - center_f64).length() - (radius as f64)
  }

  fn sphere_implicit_eval(center: IVec3, radius: i32, sample_point: &DVec3) -> f64 {
    let center_f64 = DVec3::new(center.x as f64, center.y as f64, center.z as f64);
    (sample_point - center_f64).length() - (radius as f64)
  }

  fn rect_implicit_eval(min_corner: IVec2, extent: IVec2, sample_point: &DVec2) -> f64 {
    let max_corner = min_corner + extent;
    let x_val = f64::max((min_corner.x as f64) - sample_point.x, sample_point.x - (max_corner.x as f64));
    let y_val = f64::max((min_corner.y as f64) - sample_point.y, sample_point.y - (max_corner.y as f64));
    
    f64::max(x_val, y_val)
  }

  fn cuboid_implicit_eval(min_corner: IVec3, extent: IVec3, sample_point: &DVec3) -> f64 {
    let max_corner = min_corner + extent;
    let x_val = f64::max((min_corner.x as f64) - sample_point.x, sample_point.x - (max_corner.x as f64));
    let y_val = f64::max((min_corner.y as f64) - sample_point.y, sample_point.y - (max_corner.y as f64));
    let z_val = f64::max((min_corner.z as f64) - sample_point.z, sample_point.z - (max_corner.z as f64));
    
    f64::max(f64::max(x_val, y_val), z_val)
  }

  fn polygon_implicit_eval(vertices: &Vec<IVec2>, sample_point: &DVec2) -> f64 {
    // Convert vertices to double precision for calculations
    let vertices_dvec2: Vec<DVec2> = vertices.iter()
        .map(|v| v.as_dvec2())
        .collect();
    
    // Handle degenerate case - not enough vertices for a polygon
    if vertices_dvec2.len() < 3 {
        return f64::MAX;
    }
    
    // Calculate minimum distance to any line segment (absolute value of SDF)
    let mut min_distance = f64::MAX;
    for i in 0..vertices_dvec2.len() {
        let j = (i + 1) % vertices_dvec2.len();
        let distance = Self::point_to_line_segment_distance(
            sample_point, 
            &vertices_dvec2[i], 
            &vertices_dvec2[j]
        );
        min_distance = min_distance.min(distance);
    }
    
    // Determine sign using ray casting
    let is_inside = Self::is_point_inside_polygon(sample_point, &vertices_dvec2);
    
    // Apply sign: negative inside, positive outside
    if is_inside {
        -min_distance
    } else {
        min_distance
    }
  }

  /// Calculates the minimum distance from a point to a line segment
  fn point_to_line_segment_distance(point: &DVec2, line_start: &DVec2, line_end: &DVec2) -> f64 {
    let line_vector = *line_end - *line_start;
    let line_length_squared = line_vector.length_squared();
    
    // Handle degenerate case where line segment is actually a point
    if line_length_squared < 1e-10 {
        return (*point - *line_start).length();
    }
    
    // Calculate projection of point onto line
    let t = f64::max(0.0, f64::min(1.0, (*point - *line_start).dot(line_vector) / line_length_squared));
    
    // Calculate closest point on the line segment
    let closest_point = *line_start + line_vector * t;
    
    // Return distance from point to closest point on line segment
    (*point - closest_point).length()
  }

  /// Check if a point is inside a polygon using ray casting algorithm
  fn is_point_inside_polygon(point: &DVec2, vertices: &Vec<DVec2>) -> bool {
    let num_vertices = vertices.len();
    if num_vertices < 3 {
        return false; // Not a proper polygon
    }
    
    // Cast a ray from the point in the positive X direction
    // and count intersections with polygon edges
    let mut intersections = 0;
    
    for i in 0..num_vertices {
        let j = (i + 1) % num_vertices;
        if Self::line_segment_intersects_ray(point, &vertices[i], &vertices[j]) {
            intersections += 1;
        }
    }
    
    // If number of intersections is odd, point is inside the polygon
    intersections % 2 == 1
  }

  /// Checks if a line segment from point1 to point2 intersects with a ray
  /// cast from test_point in the positive X direction
  fn line_segment_intersects_ray(test_point: &DVec2, point1: &DVec2, point2: &DVec2) -> bool {
    // Early exclusion: both endpoints are above or below the ray
    if (point1.y > test_point.y && point2.y > test_point.y) || 
       (point1.y < test_point.y && point2.y < test_point.y) {
        return false;
    }
    
    // Early exclusion: both endpoints are to the left of the test point
    if point1.x < test_point.x && point2.x < test_point.x {
        return false;
    }
    
    // Calculate intersection point of line segment with horizontal ray
    if (point1.y - test_point.y).abs() < 1e-10 || (point2.y - test_point.y).abs() < 1e-10 {
        // One endpoint is on the ray - special case
        // Count intersection only if the endpoint is the lower one
        if (point1.y - test_point.y).abs() < 1e-10 {
            return point1.y > point2.y && point1.x >= test_point.x;
        } else {
            return point2.y > point1.y && point2.x >= test_point.x;
        }
    } else {
        // Normal case - check if ray intersects line segment
        let t = (test_point.y - point1.y) / (point2.y - point1.y);
        if t >= 0.0 && t <= 1.0 {
            let x_intersect = point1.x + t * (point2.x - point1.x);
            return x_intersect >= test_point.x;
        }
    }
    
    false
  }

  fn extrude_implicit_eval(height: i32, shape: &Box<GeoNode>, sample_point: &DVec3) -> f64 {
    // Calculate Y bounds constraint (extrusion is along Y axis from 0 to height)
    let y_val = f64::max(-sample_point.y, sample_point.y - (height as f64));
    
    // Evaluate the 2D shape in the XZ plane
    let sample_point_2d = DVec2::new(sample_point.x, sample_point.z);
    let input_val = shape.implicit_eval_2d(&sample_point_2d);
    
    // Return the maximum of Y constraint and 2D shape evaluation
    f64::max(y_val, input_val)
  }

  fn transform_implicit_eval(transform: &Transform, shape: &Box<GeoNode>, sample_point: &DVec3) -> f64 {
    // Apply inverse transform to the sample point to get the point in the shape's local space
    let inverse_transform = transform.inverse();
    let transformed_point = inverse_transform.apply_to_position(sample_point);
    
    // Evaluate the shape at the transformed point
    shape.implicit_eval_3d(&transformed_point)
  }

  fn union_2d_implicit_eval(shapes: &Vec<GeoNode>, sample_point: &DVec2) -> f64 {
    shapes.iter().map(|shape| {
      shape.implicit_eval_2d(sample_point)
    }).reduce(f64::min).unwrap_or(f64::MAX)
  }

  fn union_3d_implicit_eval(shapes: &Vec<GeoNode>, sample_point: &DVec3) -> f64 {
    shapes.iter().map(|shape| {
      shape.implicit_eval_3d(sample_point)
    }).reduce(f64::min).unwrap_or(f64::MAX)
  }

  fn intersection_2d_implicit_eval(shapes: &Vec<GeoNode>, sample_point: &DVec2) -> f64 {
    shapes.iter().map(|shape| {
      shape.implicit_eval_2d(sample_point)
    }).reduce(f64::max).unwrap_or(f64::MIN)
  }

  fn intersection_3d_implicit_eval(shapes: &Vec<GeoNode>, sample_point: &DVec3) -> f64 {
    shapes.iter().map(|shape| {
      shape.implicit_eval_3d(sample_point)
    }).reduce(f64::max).unwrap_or(f64::MIN)
  }

  fn difference_2d_implicit_eval(base: &Box<GeoNode>, sub: &Box<GeoNode>, sample_point: &DVec2) -> f64 {
    let ubase = base.implicit_eval_2d(sample_point);
    let usub = sub.implicit_eval_2d(sample_point);
    
    f64::max(ubase, -usub)
  }

  fn difference_3d_implicit_eval(base: &Box<GeoNode>, sub: &Box<GeoNode>, sample_point: &DVec3) -> f64 {
    let ubase = base.implicit_eval_3d(sample_point);
    let usub = sub.implicit_eval_3d(sample_point);
    
    f64::max(ubase, -usub)
  }
}
