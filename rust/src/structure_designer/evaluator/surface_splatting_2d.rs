use crate::common::surface_point_cloud::SurfacePointCloud2D;
use crate::structure_designer::structure_designer_scene::StructureDesignerScene;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::implicit_evaluator::NodeEvaluator;
use crate::structure_designer::common_constants;
use lru::LruCache;
use glam::i32::IVec2;
use glam::Vec3Swizzles;
use crate::util::box_subdivision::subdivide_rect;
use crate::common::surface_point_cloud::SurfacePoint2D;

const SS_2D_SAMPLES_PER_UNIT: i32 = 4;

pub fn generate_2d_point_cloud_scene(node_evaluator: &NodeEvaluator, context: &mut NetworkEvaluationContext) -> StructureDesignerScene {
  let mut point_cloud = SurfacePointCloud2D::new();
  let cache_size = (common_constants::IMPLICIT_VOLUME_MAX.z - common_constants::IMPLICIT_VOLUME_MIN.z + 1) *
  (common_constants::IMPLICIT_VOLUME_MAX.x - common_constants::IMPLICIT_VOLUME_MIN.x + 1) *
  SS_2D_SAMPLES_PER_UNIT * SS_2D_SAMPLES_PER_UNIT;

  let mut eval_cache = LruCache::new(std::num::NonZeroUsize::new(cache_size as usize).unwrap());

  process_rect_for_point_cloud(
      &node_evaluator,
      &(common_constants::IMPLICIT_VOLUME_MIN.xz() * SS_2D_SAMPLES_PER_UNIT),
      &((common_constants::IMPLICIT_VOLUME_MAX.xz() - common_constants::IMPLICIT_VOLUME_MIN.xz()) * SS_2D_SAMPLES_PER_UNIT),
      &mut eval_cache,
      &mut point_cloud);

  let mut scene = StructureDesignerScene::new();
  scene.surface_point_cloud_2ds.push(point_cloud);
  
  // Copy any collected errors to the scene
  scene.node_errors = context.node_errors.clone();
  
  scene
}

fn process_rect_for_point_cloud(
  node_evaluator: &NodeEvaluator,
  start_pos: &IVec2,
  size: &IVec2,
  eval_cache: &mut LruCache<IVec2, f64>,
  point_cloud: &mut SurfacePointCloud2D,) {

let spu = SS_2D_SAMPLES_PER_UNIT as f64;
let epsilon = 0.001;

// Calculate the center point of the rect
let center_point = (start_pos.as_dvec2() + size.as_dvec2() / 2.0) / spu;

// Evaluate SDF at the center point using NodeEvaluator's eval_2d method
let sdf_value = node_evaluator.eval_2d(&center_point);

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
                    node_evaluator,
                    &cell_pos,
                    eval_cache,
                    point_cloud
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
        node_evaluator,
        &sub_start,
        &sub_size,
        eval_cache,
        point_cloud
    );
}
}


fn process_2d_cell_for_point_cloud(
node_evaluator: &NodeEvaluator,
int_pos: &IVec2,
eval_cache: &mut LruCache<IVec2, f64>,
point_cloud: &mut SurfacePointCloud2D) {
  let spu = SS_2D_SAMPLES_PER_UNIT as f64;

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
      let value = node_evaluator.eval_2d(&p);
      //println!("Evaluating point: {:?}, value: {}", ip, value);
      eval_cache.put(*ip, value);
      value
    }
  }).collect();

  if values.iter().any(|&v| v >= 0.0) && values.iter().any(|&v| v < 0.0) {
      let center_point = (corner_points[0].as_dvec2() + 0.5) / spu;
      let gradient_val = node_evaluator.get_gradient_2d(&center_point);
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
          position: (center_point - step) * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
          normal: gradient.normalize(),
        }
      );
  }
}
