use glam::i32::IVec3;
use lru::LruCache;
use crate::crystolecule::surface_point_cloud::SurfacePointCloud;
use crate::structure_designer::common_constants;
use crate::structure_designer::implicit_eval::implicit_geometry::ImplicitGeometry3D;
use crate::util::box_subdivision::subdivide_box;
use crate::crystolecule::surface_point_cloud::SurfacePoint;
use crate::structure_designer::structure_designer_scene::StructureDesignerScene;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::api::structure_designer::structure_designer_preferences::GeometryVisualizationPreferences;

pub fn generate_point_cloud(
  geometry: &dyn ImplicitGeometry3D,
  context: &mut NetworkEvaluationContext,
  geometry_visualization_preferences: &GeometryVisualizationPreferences) -> SurfacePointCloud {
  let mut point_cloud = SurfacePointCloud::new();
  let cache_size = common_constants::MAX_EVAL_CACHE_SIZE;

  let mut eval_cache = LruCache::new(std::num::NonZeroUsize::new(cache_size as usize).unwrap());

  process_box_for_point_cloud(
      geometry,
      &(common_constants::REAL_IMPLICIT_VOLUME_MIN.round().as_ivec3() * geometry_visualization_preferences.samples_per_unit_cell),
      &((common_constants::REAL_IMPLICIT_VOLUME_MAX.round().as_ivec3() - common_constants::REAL_IMPLICIT_VOLUME_MIN.round().as_ivec3()) * geometry_visualization_preferences.samples_per_unit_cell),
      &mut eval_cache,
      &mut point_cloud,
      geometry_visualization_preferences);

  point_cloud
}

fn process_box_for_point_cloud(
  geometry: &dyn ImplicitGeometry3D,
  start_pos: &IVec3,
  size: &IVec3,
  eval_cache: &mut LruCache<IVec3, f64>,
  point_cloud: &mut SurfacePointCloud,
  geometry_visualization_preferences: &GeometryVisualizationPreferences) {

  let spu = geometry_visualization_preferences.samples_per_unit_cell as f64;
  let epsilon = 0.001;

  // Calculate the center point of the box
  let center_point = (start_pos.as_dvec3() + size.as_dvec3() / 2.0) / spu;

  // Evaluate SDF at the center point
  let sdf_value = geometry.implicit_eval_3d(&center_point);

  let half_diagonal = size.as_dvec3().length() / spu / 2.0;

  // If absolute SDF value is greater than half diagonal, there's no surface in this box
  if sdf_value.abs() > half_diagonal + epsilon {
    return;
  }

  // Determine if we should subdivide in each dimension (size >= 4)
  let should_subdivide_x = size.x >= 4;
  let should_subdivide_y = size.y >= 4;
  let should_subdivide_z = size.z >= 4;

  // If we can't subdivide in any direction, process each cell individually
  if !should_subdivide_x && !should_subdivide_y && !should_subdivide_z {
    // Process each cell within the box
    for x in 0..size.x {
        for y in 0..size.y {
            for z in 0..size.z {
                let cell_pos = IVec3::new(
                    start_pos.x + x,
                    start_pos.y + y,
                    start_pos.z + z
                );
                process_cell_for_point_cloud(
                    geometry,
                    &cell_pos,
                    eval_cache,
                    point_cloud,
                    geometry_visualization_preferences,
                );
            }
        }
    }
    return;
  }

  // Otherwise, subdivide the box and recursively process each subdivision
  let subdivisions = subdivide_box(
    start_pos,
    size,
    should_subdivide_x,
    should_subdivide_y,
    should_subdivide_z
  );

  // Process each subdivision recursively
  for (sub_start, sub_size) in subdivisions {
    process_box_for_point_cloud(
        geometry,
        &sub_start,
        &sub_size,
        eval_cache,
        point_cloud,
        geometry_visualization_preferences,
    );
  }
}

fn process_cell_for_point_cloud(
  geometry: &dyn ImplicitGeometry3D,
  int_pos: &IVec3,
  eval_cache: &mut LruCache<IVec3, f64>,
  point_cloud: &mut SurfacePointCloud,
  geometry_visualization_preferences: &GeometryVisualizationPreferences) {
    let spu = geometry_visualization_preferences.samples_per_unit_cell as f64;

    // Define the corner points for the current cube
    let corner_points = [
        IVec3::new(int_pos.x, int_pos.y, int_pos.z),
        IVec3::new(int_pos.x + 1, int_pos.y, int_pos.z),
        IVec3::new(int_pos.x, int_pos.y + 1, int_pos.z),
        IVec3::new(int_pos.x, int_pos.y, int_pos.z + 1),
        IVec3::new(int_pos.x + 1, int_pos.y + 1, int_pos.z),
        IVec3::new(int_pos.x + 1, int_pos.y, int_pos.z + 1),
        IVec3::new(int_pos.x, int_pos.y + 1, int_pos.z + 1),
        IVec3::new(int_pos.x + 1, int_pos.y + 1, int_pos.z + 1),
    ];

    // Evaluate corner points using cache
    let values: Vec<f64> = corner_points.iter().map(|ip| {
      if let Some(&cached_value) = eval_cache.get(ip) {
        cached_value
      } else {
        let p = ip.as_dvec3() / spu;
        let value = geometry.implicit_eval_3d(&p);
        //println!("Evaluating point: {:?}, value: {}", ip, value);
        eval_cache.put(*ip, value);
        value
      }
    }).collect();

    if values.iter().any(|&v| v >= 0.0) && values.iter().any(|&v| v < 0.0) {
        let center_point = (corner_points[0].as_dvec3() + 0.5) / spu;
        let gradient_val = geometry.get_gradient(&center_point);
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
          SurfacePoint {
            position: (center_point - step),
            normal: gradient.normalize(),
          }
        );
    }
}
