use super::GeoNode;
use glam::f64::{DVec2, DVec3};
use crate::util::transform::Transform;
use crate::structure_designer::implicit_eval::implicit_geometry::ImplicitGeometry2D;
use crate::structure_designer::implicit_eval::implicit_geometry::ImplicitGeometry3D;

impl ImplicitGeometry2D for GeoNode {
  fn get_gradient_2d(
    &self,
    sample_point: &DVec2,
  ) -> (DVec2, f64) {
    let epsilon: f64 = 0.001; // Small value for finite difference approximation

    let value = self.implicit_eval_2d(sample_point);
    let gradient = DVec2::new(
      (self.implicit_eval_2d(&(sample_point + DVec2::new(epsilon, 0.0))) - value) / epsilon,
      (self.implicit_eval_2d(&(sample_point + DVec2::new(0.0, epsilon))) - value) / epsilon,
    );
    (gradient, value)
  }

  fn implicit_eval_2d(&self, sample_point: &DVec2) -> f64 {
    match self {
      GeoNode::HalfPlane { point1, point2 } => {
        Self::half_plane_implicit_eval(*point1, *point2, sample_point)
      }
      GeoNode::Circle { center, radius } => {
        Self::circle_implicit_eval(*center, *radius, sample_point)
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
}

impl ImplicitGeometry3D for GeoNode {
  // Calculate gradient using one sided differences
  // This is faster than using central differences but potentially less accurate
  // It also returns the value at the sampled point, so that the value can be reused. 
  fn get_gradient(
    &self,
    sample_point: &DVec3
  ) -> (DVec3, f64) {
    let epsilon: f64 = 0.001; // Small value for finite difference approximation

    let value = self.implicit_eval_3d(sample_point);
    let gradient = DVec3::new(
      (self.implicit_eval_3d(&(sample_point + DVec3::new(epsilon, 0.0, 0.0))) - value) / epsilon,
      (self.implicit_eval_3d(&(sample_point + DVec3::new(0.0, epsilon, 0.0))) - value) / epsilon,
      (self.implicit_eval_3d(&(sample_point + DVec3::new(0.0, 0.0, epsilon))) - value) / epsilon
    );
    (gradient, value)
  }

  fn implicit_eval_3d(&self, sample_point: &DVec3) -> f64 {
    match self {
      GeoNode::HalfSpace { normal, center} => {
        Self::half_space_implicit_eval(*normal, *center, sample_point)
      }
      GeoNode::Sphere { center, radius } => {
        Self::sphere_implicit_eval(*center, *radius, sample_point)
      }
      GeoNode::Extrude { height, direction, shape } => {
        Self::extrude_implicit_eval(*height, *direction, shape, sample_point)
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
}

impl GeoNode {
  fn half_space_implicit_eval(normal: DVec3, center: DVec3, sample_point: &DVec3) -> f64 {
    // Calculate the signed distance from the point to the plane defined by the normal and center point
    return normal.dot(*sample_point - center);
  }

  fn half_plane_implicit_eval(point1: DVec2, point2: DVec2, sample_point: &DVec2) -> f64 {
    // Points are already in double precision
    
    // Calculate line direction and normal
    let dir_vector = point2 - point1;
    let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
    
    // Calculate signed distance from sample_point to the line
    // Formula: distance = normal·(sample_point - point1)
    normal.dot(*sample_point - point1)
  }

  fn circle_implicit_eval(center: DVec2, radius: f64, sample_point: &DVec2) -> f64 {
    (sample_point - center).length() - radius
  }

  fn sphere_implicit_eval(center: DVec3, radius: f64, sample_point: &DVec3) -> f64 {
    (sample_point - center).length() - radius
  }

  fn polygon_implicit_eval(vertices: &Vec<DVec2>, sample_point: &DVec2) -> f64 {
    // Vertices are already in double precision
    let vertices_dvec2 = vertices;
    
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
    let is_inside = Self::is_point_inside_polygon(sample_point, vertices_dvec2);
    
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

  fn extrude_implicit_eval(height: f64, direction: DVec3, shape: &Box<GeoNode>, sample_point: &DVec3) -> f64 {
    // Calculate Y bounds constraint (extrusion is along Y axis from 0 to height)
    let height_y = direction.y * height;
    let y_val = f64::max(-sample_point.y, sample_point.y - height_y);
    
    // Evaluate the 2D shape in the XZ plane

    let sample_horizontal_displacement = DVec2::new(direction.x, direction.z) * sample_point.y / direction.y;

    let sample_point_2d = DVec2::new(sample_point.x, sample_point.z) + sample_horizontal_displacement;
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
