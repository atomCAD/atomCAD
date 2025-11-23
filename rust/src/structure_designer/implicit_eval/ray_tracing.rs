use glam::f64::DVec3;
use crate::structure_designer::implicit_eval::implicit_geometry::ImplicitGeometry3D;

// traces a ray into the given implicit geometries and returns the distance of the closest intersection if any.
pub fn raytrace_geometries(geometries: &[&dyn ImplicitGeometry3D], ray_origin: &DVec3, ray_direction: &DVec3, world_scale: f64) -> Option<f64> {  
  let mut min_distance: Option<f64> = None;
  
  for geometry in geometries {
    // Raytrace the current geometry node
    if let Some(distance) = raytrace_geometry(*geometry, ray_origin, ray_direction, world_scale) {
      // Update minimum distance if this is the first hit or closer than previous hits
      min_distance = match min_distance {
        None => Some(distance),
        Some(current_min) if distance < current_min => Some(distance),
        _ => min_distance,
      };
    }
  }  
  min_distance
} 

// traces a ray into the given implicit geometry and returns the distance of the intersection if any.
pub fn raytrace_geometry(geometry: &dyn ImplicitGeometry3D, ray_origin: &DVec3, ray_direction: &DVec3, world_scale: f64) -> Option<f64> {
  // Early return if the geometry is not 3D
  if !geometry.is3d() {
    return None;
  }
  
  // Constants for ray marching algorithm
  const MAX_STEPS: usize = 100;
  const MAX_DISTANCE: f64 = 5000.0;
  const SURFACE_THRESHOLD: f64 = 0.01;
    
  let normalized_dir = ray_direction.normalize();
  let mut current_distance: f64 = 0.0;

  // Perform ray marching
  for _ in 0..MAX_STEPS {
    // Calculate current position along the ray
    let current_pos = *ray_origin + normalized_dir * current_distance;
      
    // Scale the position by dividing by DIAMOND_UNIT_CELL_SIZE_ANGSTROM to match the scale used in rendering
    let scaled_pos = current_pos / world_scale;
      
    // Evaluate SDF at the scaled position
    let sdf_value = geometry.implicit_eval_3d(&scaled_pos);

    // If we're close enough to the surface, return the distance
    if sdf_value.abs() < SURFACE_THRESHOLD {
      return Some(current_distance);
    }
      
    // If we've gone too far, give up
    if current_distance > MAX_DISTANCE {
      return None;
    }
      
    // Step forward by the SDF value - this is safe because
    // the absolute value of the gradient of an SDF cannot be bigger than 1
    // This means the SDF value tells us how far we can safely march without missing the surface
    // We need to scale the SDF value back to world space by multiplying by world_scale
    current_distance += sdf_value * world_scale;
  }

  // No intersection found within the maximum number of steps
  None
}




