use crate::structure_designer::nodes::anchor;
use crate::api::api_common::from_api_vec3;
use crate::api::api_common::refresh_renderer;
use crate::api::api_common::with_mut_cad_instance;
use crate::api::common_api_types::APIVec3;

#[flutter_rust_bridge::frb(sync)]
pub fn select_anchor_atom_by_ray(ray_start: APIVec3, ray_dir: APIVec3) {
  unsafe {
    with_mut_cad_instance(|instance| {
      let ray_start_dvec3 = from_api_vec3(&ray_start);
      let ray_dir_dvec3 = from_api_vec3(&ray_dir);
      anchor::select_anchor_atom_by_ray(&mut instance.structure_designer, &ray_start_dvec3, &ray_dir_dvec3);
      refresh_renderer(instance, false);
    });
  }
}