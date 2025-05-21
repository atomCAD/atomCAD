use crate::structure_designer::nodes::stamp;
use crate::api::api_common::CAD_INSTANCE;
use crate::api::api_common::from_api_vec3;
use crate::api::api_common::refresh_renderer;
use crate::api::common_api_types::APIVec3;
use crate::api::structure_designer::structure_designer_api_types::APIStampView;
use crate::structure_designer::nodes::stamp::StampData;
use crate::api::api_common::to_api_ivec3;

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

#[flutter_rust_bridge::frb(sync)]
pub fn get_stamp_view(node_id: u64) -> Option<APIStampView> {
  unsafe {
    let cad_instance = CAD_INSTANCE.as_ref()?;
    let node_data = cad_instance.structure_designer.get_node_network_data(node_id)?;
    let stamp_data = node_data.as_any_ref().downcast_ref::<StampData>()?;
    
    // Get the selected stamp placement if it exists
    let selected_stamp_placement = match stamp_data.selected_stamp_placement {
      Some(index) if index < stamp_data.stamp_placements.len() => {
        // Convert StampPlacement to APIStampPlacement
        let stamp_placement = &stamp_data.stamp_placements[index];
        Some(crate::api::structure_designer::structure_designer_api_types::APIStampPlacement {
          position: to_api_ivec3(&stamp_placement.position),
          x_dir: stamp_placement.x_dir,
          y_dir: stamp_placement.y_dir,
        })
      },
      _ => None,
    };
    
    return Some(APIStampView {
      selected_stamp_placement,
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_stamp_x_dir(node_id: u64, x_dir: i32) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      stamp::set_x_dir(&mut instance.structure_designer, node_id, x_dir);
      refresh_renderer(instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_stamp_y_dir(node_id: u64, y_dir: i32) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      stamp::set_y_dir(&mut instance.structure_designer, node_id, y_dir);
      refresh_renderer(instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn delete_selected_stamp_placement(node_id: u64) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      stamp::delete_selected_stamp_placement(&mut instance.structure_designer, node_id);
      refresh_renderer(instance, false);
    }
  }
}
