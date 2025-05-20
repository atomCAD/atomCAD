use crate::structure_designer::nodes::stamp;
use crate::api::api_common::CAD_INSTANCE;
use crate::api::api_common::from_api_vec3;
use crate::api::api_common::refresh_renderer;
use crate::api::common_api_types::APIVec3;

#[flutter_rust_bridge::frb(sync)]
pub fn add_or_select_stamp_placement_by_ray(ray_start: APIVec3, ray_dir: APIVec3) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      let ray_start_dvec3 = from_api_vec3(&ray_start);
      let ray_dir_dvec3 = from_api_vec3(&ray_dir);
      stamp::add_or_select_stamp_placement_by_ray(&mut instance.structure_designer, &ray_start_dvec3, &ray_dir_dvec3);
      refresh_renderer(instance, false);
    }
  }
}
