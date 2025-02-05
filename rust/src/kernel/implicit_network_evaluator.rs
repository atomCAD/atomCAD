use glam::i32::IVec3;
use glam::f32::Vec3;
use super::surface_point_cloud::SurfacePointCloud;
use super::node::NodeNetwork;

// TODO: these will not be constant, will be set by the user
const NETWORK_EVAL_VOLUME_MIN: IVec3 = IVec3::new(-16, -16, -16);
const NETWORK_EVAL_VOLUME_MAX: IVec3 = IVec3::new(16, 16, 16);

pub struct ImplicitNetworkEvaluator {
}

/*
 * Node network evaluator that uses implicits for geometry modeling.
 * A node network evaluator is able to generate displayable representation for a node in a node network. This evaluator
 * does this by treating the abstract operators (nodes) in the node network as implicit geometry functions. 
 * Currently this is the only network evaluator in our codebase, but it should be possible to create other evaluators
 * (like evaluator based on polygon meshes or evaluator based on voxels.)
 * On a discussion of the distinction of the abstract shape algebra and a concrete implementation see:
 * /doc/crystal/shapre_algebra.md
 * TODO: probably should be refactored into an Evaluator and an ImplicitGeometry evaluator,
 * as nodes related to atomic representation is not specific to implicits. 
 */
impl ImplicitNetworkEvaluator {
  // Creates the display representation that will be displayed for the given node
  // Currently creates it from scratch, no caching is used.
  // TODO: Currently just supports geometry nodes and creates SurfacePointCloud. Should be refaactored
  // to be able to support generating atomic models too
  pub fn generate_displayable(&self, network: &NodeNetwork, node_id: u64) -> SurfacePointCloud {

    let point_cloud = SurfacePointCloud::new();

    // Iterate over voxel grid
    for x in NETWORK_EVAL_VOLUME_MIN.x..NETWORK_EVAL_VOLUME_MAX.x {
      for y in NETWORK_EVAL_VOLUME_MIN.y..NETWORK_EVAL_VOLUME_MAX.y {
        for z in NETWORK_EVAL_VOLUME_MIN.z..NETWORK_EVAL_VOLUME_MAX.z {
          let corner_points = [
            Vec3::new(x as f32, y as f32, z as f32),
            Vec3::new((x + 1) as f32, y as f32, z as f32),
            Vec3::new(x as f32, (y + 1) as f32, z as f32),
            Vec3::new(x as f32, y as f32, (z + 1) as f32),
            Vec3::new((x + 1) as f32, (y + 1) as f32, z as f32),
            Vec3::new((x + 1) as f32, y as f32, (z + 1) as f32),
            Vec3::new(x as f32, (y + 1) as f32, (z + 1) as f32),
            Vec3::new((x + 1) as f32, (y + 1) as f32, (z + 1) as f32),
          ];

          // TODO: Optimize this by caching parts of the SDF, because this way
          // each corner is sampled 8 times! 
          let signs: Vec<f32> = corner_points.iter().map(|&p| sdf(p)).collect();
          if signs.iter().any(|&s| s > 0.0) && signs.iter().any(|&s| s < 0.0) {
            point_cloud.points.push(
              SurfacePoint {
                position: corner_points[0] + 0.5,
                normal: Vec3::new(0.0, 1.0, 0.0),
              }
            ); // Start at cell center
          }
        }
      }
    }

    return point_cloud;
  }
}
