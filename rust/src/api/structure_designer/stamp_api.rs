use crate::structure_designer::nodes::stamp;
use crate::api::api_common::from_api_vec3;
use crate::api::api_common::refresh_renderer;
use crate::api::common_api_types::APIVec3;
use crate::api::structure_designer::structure_designer_api_types::APIStampView;
use crate::structure_designer::nodes::stamp::StampData;
use crate::api::api_common::to_api_ivec3;
use crate::api::api_common::with_mut_cad_instance;
use crate::api::api_common::with_cad_instance_or;

#[flutter_rust_bridge::frb(sync)]
pub fn add_or_select_stamp_placement_by_ray(ray_start: APIVec3, ray_dir: APIVec3) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let ray_start_dvec3 = from_api_vec3(&ray_start);
      let ray_dir_dvec3 = from_api_vec3(&ray_dir);
      stamp::add_or_select_stamp_placement_by_ray(&mut cad_instance.structure_designer, &ray_start_dvec3, &ray_dir_dvec3);
      refresh_renderer(cad_instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_stamp_view(node_id: u64) -> Option<APIStampView> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        
        let stamp_data = match node_data.as_any_ref().downcast_ref::<StampData>() {
          Some(data) => data,
          None => return None,
        };
        
        // Get the selected stamp placement if it exists
        let selected_stamp_placement = match stamp_data.selected_stamp_placement {
          Some(index) if index < stamp_data.stamp_placements.len() => {
            // Convert StampPlacement to APIStampPlacement
            let stamp_placement = &stamp_data.stamp_placements[index];
            Some(crate::api::structure_designer::structure_designer_api_types::APIStampPlacement {
              position: to_api_ivec3(&stamp_placement.position),
              rotation: stamp_placement.rotation,
            })
          },
          _ => None,
        };
        
        Some(APIStampView {
          selected_stamp_placement,
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_stamp_rotation(node_id: u64, rotation: i32) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      stamp::set_rotation(&mut cad_instance.structure_designer, node_id, rotation);
      refresh_renderer(cad_instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn delete_selected_stamp_placement(node_id: u64) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      stamp::delete_selected_stamp_placement(&mut cad_instance.structure_designer, node_id);
      refresh_renderer(cad_instance, false);
    });
  }
}
