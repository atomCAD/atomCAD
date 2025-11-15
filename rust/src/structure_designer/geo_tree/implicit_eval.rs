use super::{GeoNode, GeoNodeKind};
use glam::f64::{DVec2, DVec3};
use crate::util::transform::Transform;
use crate::structure_designer::implicit_eval::implicit_geometry::ImplicitGeometry2D;
use crate::structure_designer::implicit_eval::implicit_geometry::{ImplicitGeometry3D, BATCH_SIZE};

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
    match &self.kind {
      GeoNodeKind::HalfPlane { point1, point2 } => {
        Self::half_plane_implicit_eval(*point1, *point2, sample_point)
      }
      GeoNodeKind::Circle { center, radius } => {
        Self::circle_implicit_eval(*center, *radius, sample_point)
      }
      GeoNodeKind::Polygon { vertices } => {
        Self::polygon_implicit_eval(vertices, sample_point)
      }
      GeoNodeKind::Union2D { shapes } => {
        Self::union_2d_implicit_eval(shapes, sample_point)
      }
      GeoNodeKind::Intersection2D { shapes } => {
        Self::intersection_2d_implicit_eval(shapes, sample_point)
      }
      GeoNodeKind::Difference2D { base, sub } => {
        Self::difference_2d_implicit_eval(base, sub, sample_point)
      }
      // 3D shapes should use implicit_eval_3d instead
      _ => panic!("3D shapes should be evaluated using implicit_eval_3d")
    }
  }

  fn implicit_eval_2d_batch(&self, sample_points: &[DVec2; BATCH_SIZE], results: &mut [f64; BATCH_SIZE]) {
    match &self.kind {
      GeoNodeKind::HalfPlane { point1, point2 } => {
        Self::half_plane_implicit_eval_batch(*point1, *point2, sample_points, results)
      }
      GeoNodeKind::Circle { center, radius } => {
        Self::circle_implicit_eval_batch(*center, *radius, sample_points, results)
      }
      GeoNodeKind::Union2D { shapes } => {
        Self::union_2d_implicit_eval_batch(shapes, sample_points, results)
      }
      GeoNodeKind::Intersection2D { shapes } => {
        Self::intersection_2d_implicit_eval_batch(shapes, sample_points, results)
      }
      GeoNodeKind::Difference2D { base, sub } => {
        Self::difference_2d_implicit_eval_batch(base, sub, sample_points, results)
      }
      // For all other node types, use naive implementation for now
      _ => {
        for i in 0..BATCH_SIZE {
          results[i] = self.implicit_eval_2d(&sample_points[i]);
        }
      }
    }
  }

  fn is2d(&self) -> bool {
    match &self.kind {
      GeoNodeKind::HalfPlane { .. } => true,
      GeoNodeKind::Circle { .. } => true,
      GeoNodeKind::Polygon { .. } => true,
      GeoNodeKind::Union2D { .. } => true,
      GeoNodeKind::Intersection2D { .. } => true,
      GeoNodeKind::Difference2D { .. } => true,
      _ => false
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
    match &self.kind {
      GeoNodeKind::HalfSpace { normal, center} => {
        Self::half_space_implicit_eval(*normal, *center, sample_point)
      }
      GeoNodeKind::Sphere { center, radius } => {
        Self::sphere_implicit_eval(*center, *radius, sample_point)
      }
      GeoNodeKind::Extrude { height, direction, shape } => {
        Self::extrude_implicit_eval(*height, *direction, shape, sample_point)
      }
      GeoNodeKind::Transform { transform, shape } => {
        Self::transform_implicit_eval(transform, shape, sample_point)
      }
      GeoNodeKind::Union3D { shapes } => {
        Self::union_3d_implicit_eval(shapes, sample_point)
      }
      GeoNodeKind::Intersection3D { shapes } => {
        Self::intersection_3d_implicit_eval(shapes, sample_point)
      }
      GeoNodeKind::Difference3D { base, sub } => {
        Self::difference_3d_implicit_eval(base, sub, sample_point)
      }
      // 2D shapes should use implicit_eval_2d instead
      _ => panic!("2D shapes should be evaluated using implicit_eval_2d")
    }
  }

  fn implicit_eval_3d_batch(&self, sample_points: &[DVec3; BATCH_SIZE], results: &mut [f64; BATCH_SIZE]) {
    match &self.kind {
      GeoNodeKind::HalfSpace { normal, center } => {
        Self::half_space_implicit_eval_batch(*normal, *center, sample_points, results)
      }
      GeoNodeKind::Sphere { center, radius } => {
        Self::sphere_implicit_eval_batch(*center, *radius, sample_points, results)
      }
      GeoNodeKind::Extrude { height, direction, shape } => {
        Self::extrude_implicit_eval_batch(*height, *direction, shape, sample_points, results)
      }
      GeoNodeKind::Transform { transform, shape } => {
        Self::transform_implicit_eval_batch(transform, shape, sample_points, results)
      }
      GeoNodeKind::Union3D { shapes } => {
        Self::union_3d_implicit_eval_batch(shapes, sample_points, results)
      }
      GeoNodeKind::Intersection3D { shapes } => {
        Self::intersection_3d_implicit_eval_batch(shapes, sample_points, results)
      }
      GeoNodeKind::Difference3D { base, sub } => {
        Self::difference_3d_implicit_eval_batch(base, sub, sample_points, results)
      }
      // For all other node types, use naive implementation for now
      _ => {
        for i in 0..BATCH_SIZE {
          results[i] = self.implicit_eval_3d(&sample_points[i]);
        }
      }
    }
  }

  fn is3d(&self) -> bool {
    match &self.kind {
      GeoNodeKind::HalfSpace { .. } => true,
      GeoNodeKind::Sphere { .. } => true,
      GeoNodeKind::Extrude { .. } => true,
      GeoNodeKind::Transform { .. } => true,
      GeoNodeKind::Union3D { .. } => true,
      GeoNodeKind::Intersection3D { .. } => true,
      GeoNodeKind::Difference3D { .. } => true,
      _ => false
    }
  }
}

impl GeoNode {
  fn half_space_implicit_eval(normal: DVec3, center: DVec3, sample_point: &DVec3) -> f64 {
    // Calculate the signed distance from the point to the plane defined by the normal and center point
    return normal.dot(*sample_point - center);
  }

  fn half_space_implicit_eval_batch(
    normal: DVec3, 
    center: DVec3, 
    sample_points: &[DVec3; BATCH_SIZE], 
    results: &mut [f64; BATCH_SIZE]
  ) {
    // Calculate signed distance from each point to the plane defined by normal and center
    // This is highly optimizable since it's just dot products
    for i in 0..BATCH_SIZE {
      results[i] = normal.dot(sample_points[i] - center);
    }
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

  fn half_plane_implicit_eval_batch(
    point1: DVec2, 
    point2: DVec2, 
    sample_points: &[DVec2; BATCH_SIZE], 
    results: &mut [f64; BATCH_SIZE]
  ) {
    // Pre-calculate line direction and normal (same for all points)
    let dir_vector = point2 - point1;
    let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
    
    // Calculate signed distance from each point to the line
    // This is highly optimizable since it's just dot products
    for i in 0..BATCH_SIZE {
      results[i] = normal.dot(sample_points[i] - point1);
    }
  }

  fn circle_implicit_eval(center: DVec2, radius: f64, sample_point: &DVec2) -> f64 {
    (sample_point - center).length() - radius
  }

  fn circle_implicit_eval_batch(
    center: DVec2, 
    radius: f64, 
    sample_points: &[DVec2; BATCH_SIZE], 
    results: &mut [f64; BATCH_SIZE]
  ) {
    // Calculate distance from each point to circle center, then subtract radius
    // This is highly optimizable since it's just vector operations
    for i in 0..BATCH_SIZE {
      results[i] = (sample_points[i] - center).length() - radius;
    }
  }

  fn sphere_implicit_eval(center: DVec3, radius: f64, sample_point: &DVec3) -> f64 {
    (sample_point - center).length() - radius
  }

  fn sphere_implicit_eval_batch(
    center: DVec3, 
    radius: f64, 
    sample_points: &[DVec3; BATCH_SIZE], 
    results: &mut [f64; BATCH_SIZE]
  ) {
    // Calculate distance from each point to sphere center, then subtract radius
    // This is highly optimizable since it's just vector operations
    for i in 0..BATCH_SIZE {
      results[i] = (sample_points[i] - center).length() - radius;
    }
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
    // Calculate Z bounds constraint (extrusion is along Z axis from 0 to height)
    let height_z = direction.z * height;
    let z_val = f64::max(-sample_point.z, sample_point.z - height_z);
    
    // Evaluate the 2D shape in the XY plane

    let sample_horizontal_displacement = DVec2::new(direction.x, direction.y) * sample_point.z / direction.z;

    let sample_point_2d = DVec2::new(sample_point.x, sample_point.y) - sample_horizontal_displacement;
    let input_val = shape.implicit_eval_2d(&sample_point_2d);
    
    // Return the maximum of Z constraint and 2D shape evaluation
    f64::max(z_val, input_val)
  }

  fn extrude_implicit_eval_batch(
    height: f64, 
    direction: DVec3, 
    shape: &Box<GeoNode>, 
    sample_points: &[DVec3; BATCH_SIZE], 
    results: &mut [f64; BATCH_SIZE]
  ) {
    // Pre-calculate constants
    let height_z = direction.z * height;
    let horizontal_direction = DVec2::new(direction.x, direction.y);
    let inv_direction_z = 1.0 / direction.z;
    
    // Prepare 2D sample points for batch evaluation
    let mut sample_points_2d = [DVec2::ZERO; BATCH_SIZE];
    let mut z_constraints = [0.0; BATCH_SIZE];
    
    for i in 0..BATCH_SIZE {
      // Calculate Z bounds constraint
      z_constraints[i] = f64::max(-sample_points[i].z, sample_points[i].z - height_z);
      
      // Project 3D point to 2D for shape evaluation
      let sample_horizontal_displacement = horizontal_direction * sample_points[i].z * inv_direction_z;
      sample_points_2d[i] = DVec2::new(sample_points[i].x, sample_points[i].y) - sample_horizontal_displacement;
    }
    
    // Evaluate 2D shape in batch
    let mut shape_results = [0.0; BATCH_SIZE];
    shape.implicit_eval_2d_batch(&sample_points_2d, &mut shape_results);
    
    // Combine Z constraint and 2D shape evaluation
    for i in 0..BATCH_SIZE {
      results[i] = f64::max(z_constraints[i], shape_results[i]);
    }
  }

  fn transform_implicit_eval(transform: &Transform, shape: &Box<GeoNode>, sample_point: &DVec3) -> f64 {
    // Apply inverse transform to the sample point to get the point in the shape's local space
    let inverse_transform = transform.inverse();
    let transformed_point = inverse_transform.apply_to_position(sample_point);
    
    // Evaluate the shape at the transformed point
    shape.implicit_eval_3d(&transformed_point)
  }

  fn transform_implicit_eval_batch(
    transform: &Transform, 
    shape: &Box<GeoNode>, 
    sample_points: &[DVec3; BATCH_SIZE], 
    results: &mut [f64; BATCH_SIZE]
  ) {
    // Apply inverse transform to all sample points to get points in the shape's local space
    let inverse_transform = transform.inverse();
    let mut transformed_points = [DVec3::ZERO; BATCH_SIZE];
    
    for i in 0..BATCH_SIZE {
      transformed_points[i] = inverse_transform.apply_to_position(&sample_points[i]);
    }
    
    // Evaluate the shape at all transformed points using batch evaluation
    shape.implicit_eval_3d_batch(&transformed_points, results);
  }

  fn union_2d_implicit_eval(shapes: &Vec<GeoNode>, sample_point: &DVec2) -> f64 {
    shapes.iter().map(|shape| {
      shape.implicit_eval_2d(sample_point)
    }).reduce(f64::min).unwrap_or(f64::MAX)
  }

  fn union_2d_implicit_eval_batch(
    shapes: &Vec<GeoNode>, 
    sample_points: &[DVec2; BATCH_SIZE], 
    results: &mut [f64; BATCH_SIZE]
  ) {
    if shapes.is_empty() {
      // No shapes - fill with maximum distance (outside everything)
      results.fill(f64::MAX);
      return;
    }

    // Initialize results with the first shape's evaluation
    shapes[0].implicit_eval_2d_batch(sample_points, results);

    // For each additional shape, evaluate in batch and take minimum with current results
    let mut shape_results = [0.0; BATCH_SIZE];
    for shape in shapes.iter().skip(1) {
      shape.implicit_eval_2d_batch(sample_points, &mut shape_results);
      
      // Take minimum of current results and this shape's results (union operation)
      for i in 0..BATCH_SIZE {
        results[i] = results[i].min(shape_results[i]);
      }
    }
  }

  fn union_3d_implicit_eval(shapes: &Vec<GeoNode>, sample_point: &DVec3) -> f64 {
    shapes.iter().map(|shape| {
      shape.implicit_eval_3d(sample_point)
    }).reduce(f64::min).unwrap_or(f64::MAX)
  }

  fn union_3d_implicit_eval_batch(
    shapes: &Vec<GeoNode>, 
    sample_points: &[DVec3; BATCH_SIZE], 
    results: &mut [f64; BATCH_SIZE]
  ) {
    if shapes.is_empty() {
      // No shapes - fill with maximum distance (outside everything)
      results.fill(f64::MAX);
      return;
    }

    // Initialize results with the first shape's evaluation
    shapes[0].implicit_eval_3d_batch(sample_points, results);

    // For each additional shape, evaluate in batch and take minimum with current results
    let mut shape_results = [0.0; BATCH_SIZE];
    for shape in shapes.iter().skip(1) {
      shape.implicit_eval_3d_batch(sample_points, &mut shape_results);
      
      // Take minimum of current results and this shape's results (union operation)
      for i in 0..BATCH_SIZE {
        results[i] = results[i].min(shape_results[i]);
      }
    }
  }

  fn intersection_2d_implicit_eval(shapes: &Vec<GeoNode>, sample_point: &DVec2) -> f64 {
    shapes.iter().map(|shape| {
      shape.implicit_eval_2d(sample_point)
    }).reduce(f64::max).unwrap_or(f64::MIN)
  }

  fn intersection_2d_implicit_eval_batch(
    shapes: &Vec<GeoNode>, 
    sample_points: &[DVec2; BATCH_SIZE], 
    results: &mut [f64; BATCH_SIZE]
  ) {
    if shapes.is_empty() {
      // No shapes - fill with minimum distance (inside everything)
      results.fill(f64::MIN);
      return;
    }

    // Initialize results with the first shape's evaluation
    shapes[0].implicit_eval_2d_batch(sample_points, results);

    // For each additional shape, evaluate in batch and take maximum with current results
    let mut shape_results = [0.0; BATCH_SIZE];
    for shape in shapes.iter().skip(1) {
      shape.implicit_eval_2d_batch(sample_points, &mut shape_results);
      
      // Take maximum of current results and this shape's results (intersection operation)
      for i in 0..BATCH_SIZE {
        results[i] = results[i].max(shape_results[i]);
      }
    }
  }

  fn intersection_3d_implicit_eval(shapes: &Vec<GeoNode>, sample_point: &DVec3) -> f64 {
    shapes.iter().map(|shape| {
      shape.implicit_eval_3d(sample_point)
    }).reduce(f64::max).unwrap_or(f64::MIN)
  }

  fn intersection_3d_implicit_eval_batch(
    shapes: &Vec<GeoNode>, 
    sample_points: &[DVec3; BATCH_SIZE], 
    results: &mut [f64; BATCH_SIZE]
  ) {
    if shapes.is_empty() {
      // No shapes - fill with minimum distance (inside everything)
      results.fill(f64::MIN);
      return;
    }

    // Initialize results with the first shape's evaluation
    shapes[0].implicit_eval_3d_batch(sample_points, results);

    // For each additional shape, evaluate in batch and take maximum with current results
    let mut shape_results = [0.0; BATCH_SIZE];
    for shape in shapes.iter().skip(1) {
      shape.implicit_eval_3d_batch(sample_points, &mut shape_results);
      
      // Take maximum of current results and this shape's results (intersection operation)
      for i in 0..BATCH_SIZE {
        results[i] = results[i].max(shape_results[i]);
      }
    }
  }

  fn difference_2d_implicit_eval(base: &Box<GeoNode>, sub: &Box<GeoNode>, sample_point: &DVec2) -> f64 {
    let ubase = base.implicit_eval_2d(sample_point);
    let usub = sub.implicit_eval_2d(sample_point);
    
    f64::max(ubase, -usub)
  }

  fn difference_2d_implicit_eval_batch(
    base: &Box<GeoNode>, 
    sub: &Box<GeoNode>, 
    sample_points: &[DVec2; BATCH_SIZE], 
    results: &mut [f64; BATCH_SIZE]
  ) {
    // Evaluate base shape in batch
    base.implicit_eval_2d_batch(sample_points, results);
    
    // Evaluate subtracted shape in batch
    let mut sub_results = [0.0; BATCH_SIZE];
    sub.implicit_eval_2d_batch(sample_points, &mut sub_results);
    
    // Apply difference operation: max(base, -sub) for each point
    for i in 0..BATCH_SIZE {
      results[i] = results[i].max(-sub_results[i]);
    }
  }

  fn difference_3d_implicit_eval(base: &Box<GeoNode>, sub: &Box<GeoNode>, sample_point: &DVec3) -> f64 {
    let ubase = base.implicit_eval_3d(sample_point);
    let usub = sub.implicit_eval_3d(sample_point);
    
    f64::max(ubase, -usub)
  }

  fn difference_3d_implicit_eval_batch(
    base: &Box<GeoNode>, 
    sub: &Box<GeoNode>, 
    sample_points: &[DVec3; BATCH_SIZE], 
    results: &mut [f64; BATCH_SIZE]
  ) {
    // Evaluate base shape in batch
    base.implicit_eval_3d_batch(sample_points, results);
    
    // Evaluate subtracted shape in batch
    let mut sub_results = [0.0; BATCH_SIZE];
    sub.implicit_eval_3d_batch(sample_points, &mut sub_results);
    
    // Apply difference operation: max(base, -sub) for each point
    for i in 0..BATCH_SIZE {
      results[i] = results[i].max(-sub_results[i]);
    }
  }
}
