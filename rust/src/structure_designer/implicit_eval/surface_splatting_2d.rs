use crate::common::surface_point_cloud::SurfacePointCloud2D;
use crate::structure_designer::implicit_eval::implicit_geometry::ImplicitGeometry2D;
use crate::structure_designer::structure_designer_scene::StructureDesignerScene;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::common_constants;
use lru::LruCache;
use glam::i32::IVec2;
use glam::Vec3Swizzles;
use crate::util::box_subdivision::subdivide_rect;
use crate::common::surface_point_cloud::SurfacePoint2D;
use crate::api::structure_designer::structure_designer_preferences::GeometryVisualizationPreferences;

pub fn generate_2d_point_cloud(
  geometry: &dyn ImplicitGeometry2D,
  context: &mut NetworkEvaluationContext,
  geometry_visualization_preferences: &GeometryVisualizationPreferences
) -> SurfacePointCloud2D {
  let mut point_cloud = SurfacePointCloud2D::new();
  let cache_size = common_constants::MAX_EVAL_CACHE_SIZE;

  let mut eval_cache = LruCache::new(std::num::NonZeroUsize::new(cache_size as usize).unwrap());

  process_rect_for_point_cloud(
      geometry,
      &(common_constants::REAL_IMPLICIT_VOLUME_MIN.round().as_ivec3().xy() * geometry_visualization_preferences.samples_per_unit_cell),
      &((common_constants::REAL_IMPLICIT_VOLUME_MAX.round().as_ivec3().xy() - common_constants::REAL_IMPLICIT_VOLUME_MIN.round().as_ivec3().xy()) * geometry_visualization_preferences.samples_per_unit_cell),
      &mut eval_cache,
      &mut point_cloud,
      geometry_visualization_preferences);

  point_cloud
}

fn process_rect_for_point_cloud(
  geometry: &dyn ImplicitGeometry2D,
  start_pos: &IVec2,
  size: &IVec2,
  eval_cache: &mut LruCache<IVec2, f64>,
  point_cloud: &mut SurfacePointCloud2D,
  geometry_visualization_preferences: &GeometryVisualizationPreferences) {

  let spu = geometry_visualization_preferences.samples_per_unit_cell as f64;
  let epsilon = 0.001;

  // Calculate the center point of the rect
  let center_point = (start_pos.as_dvec2() + size.as_dvec2() / 2.0) / spu;

  // Evaluate SDF at the center point using NodeEvaluator's eval_2d method
  let sdf_value = geometry.implicit_eval_2d(&center_point);

  let half_diagonal = size.as_dvec2().length() / spu / 2.0;

  // If absolute SDF value is greater than half diagonal, there's no surface in this rect
  if sdf_value.abs() > half_diagonal + epsilon {
    return;
  }

  // Determine if we should subdivide in each dimension (size >= 4)
  let should_subdivide_x = size.x >= 4;
  let should_subdivide_y = size.y >= 4;

  // If we can't subdivide in any direction, process each cell individually
  if !should_subdivide_x && !should_subdivide_y {
    // Process each cell within the rect
    for x in 0..size.x {
        for y in 0..size.y {
                let cell_pos = IVec2::new(
                    start_pos.x + x,
                    start_pos.y + y,
                );
                process_2d_cell_for_point_cloud(
                    geometry,
                    &cell_pos,
                    eval_cache,
                    point_cloud,
                    geometry_visualization_preferences
                );
        }
    }
    return;
  }

  // Otherwise, subdivide the rect and recursively process each subdivision
  let subdivisions = subdivide_rect(
    start_pos,
    size,
    should_subdivide_x,
    should_subdivide_y,
  );

  // Process each subdivision recursively
  for (sub_start, sub_size) in subdivisions {
    process_rect_for_point_cloud(
        geometry,
        &sub_start,
        &sub_size,
        eval_cache,
        point_cloud,
        geometry_visualization_preferences
    );
  }
}

fn process_2d_cell_for_point_cloud(
  geometry: &dyn ImplicitGeometry2D,
  int_pos: &IVec2,
  eval_cache: &mut LruCache<IVec2, f64>,
  point_cloud: &mut SurfacePointCloud2D,
  geometry_visualization_preferences: &GeometryVisualizationPreferences) {
  let spu = geometry_visualization_preferences.samples_per_unit_cell as f64;

  // Define the corner points for the current square
  let corner_points = [
      IVec2::new(int_pos.x, int_pos.y),
      IVec2::new(int_pos.x + 1, int_pos.y),
      IVec2::new(int_pos.x, int_pos.y + 1),
      IVec2::new(int_pos.x + 1, int_pos.y + 1),
  ];

  // Evaluate corner points using cache
  let values: Vec<f64> = corner_points.iter().map(|ip| {
    if let Some(&cached_value) = eval_cache.get(ip) {
      cached_value
    } else {
      let p = ip.as_dvec2() / spu;
      let value = geometry.implicit_eval_2d(&p);
      //println!("Evaluating point: {:?}, value: {}", ip, value);
      eval_cache.put(*ip, value);
      value
    }
  }).collect();

  if values.iter().any(|&v| v >= 0.0) && values.iter().any(|&v| v < 0.0) {
      let center_point = (corner_points[0].as_dvec2() + 0.5) / spu;
      let gradient_val = geometry.get_gradient_2d(&center_point);
      let gradient = gradient_val.0;
      let value = gradient_val.1;
      let gradient_magnitude_sq = gradient.length_squared();
      // Avoid division by very small numbers
      let step = if gradient_magnitude_sq > 1e-10 {
          value * gradient / gradient_magnitude_sq
      } else {
          value * gradient // Fallback to SDF assumption if gradient is nearly zero
      };
      point_cloud.points.push(
        SurfacePoint2D {
          position: (center_point - step),
          normal: gradient.normalize(),
        }
      );
  }
}
