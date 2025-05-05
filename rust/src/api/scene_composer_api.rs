use crate::api::api_common::refresh_renderer;
use crate::api::api_common::CAD_INSTANCE;
use crate::api::scene_composer_api_types::SceneComposerView;
use crate::api::scene_composer_api_types::ClusterView;
use crate::api::common_api_types::APITransform;
use crate::api::api_common::to_api_transform;
use crate::api::api_common::from_api_transform;
use crate::api::common_api_types::APIVec3;
use crate::api::common_api_types::SelectModifier;
use crate::api::scene_composer_api_types::APISceneComposerTool;
use std::time::Instant;
use crate::api::api_common::from_api_vec3;
use crate::api::api_common::to_api_vec3;
use crate::api::scene_composer_api_types::AtomView;

#[flutter_rust_bridge::frb(sync)]
pub fn scene_composer_undo() -> bool {
  unsafe {
    let instance = CAD_INSTANCE.as_mut().unwrap();
    let result = instance.scene_composer.undo();
    refresh_renderer(instance, false);
    result
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn scene_composer_redo() -> bool {
  unsafe {
    let instance = CAD_INSTANCE.as_mut().unwrap();
    let result = instance.scene_composer.redo();
    refresh_renderer(instance, false);
    result
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_scene_composer_view() -> Option<SceneComposerView> {
  unsafe {
    let cad_instance = CAD_INSTANCE.as_ref()?;

    let mut scene_composer_view = SceneComposerView {
      clusters: Vec::new(),
      active_tool: cad_instance.scene_composer.model.get_active_tool(),
      available_tools: cad_instance.scene_composer.get_available_tools(),
      is_undo_available: cad_instance.scene_composer.is_undo_available(),
      is_redo_available: cad_instance.scene_composer.is_redo_available(),
    };

    for cluster in cad_instance.scene_composer.model.model.clusters.values() {
      scene_composer_view.clusters.push(ClusterView {
        id: cluster.id,
        name: cluster.name.clone(),
        selected: cluster.selected,
      });
    }

    Some(scene_composer_view)
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_selected_frame_transform() -> Option<APITransform> {
  unsafe {
    let instance = CAD_INSTANCE.as_ref()?;
    let transform = instance.scene_composer.model.get_selected_frame_transform()?;
    Some(to_api_transform(&transform))
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_selected_frame_transform(transform: APITransform) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.scene_composer.set_selected_frame_transform(from_api_transform(&transform));
      refresh_renderer(instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn scene_composer_rename_cluster(cluster_id: u64, new_name: String) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.scene_composer.model.model.rename_cluster(cluster_id, &new_name);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn import_xyz(file_path: &str) {
  unsafe {
    if let Some(cad_instance) = &mut CAD_INSTANCE {
      cad_instance.scene_composer.import_xyz(file_path).unwrap();
      refresh_renderer(cad_instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn export_xyz(file_path: &str) -> bool {
  unsafe {
    if let Some(cad_instance) = &mut CAD_INSTANCE {
      cad_instance.scene_composer.export_xyz(file_path).is_ok()
    } else {
      false
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_cluster_by_ray(ray_start: APIVec3, ray_dir: APIVec3, select_modifier: SelectModifier) -> Option<u64> {
  unsafe {
    let instance = CAD_INSTANCE.as_mut()?;
    let selected_cluster = instance.scene_composer.select_cluster_by_ray(
      &from_api_vec3(&ray_start),
      &from_api_vec3(&ray_dir),
      select_modifier);
    refresh_renderer(instance, false);
    selected_cluster
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_cluster_by_id(cluster_id: u64, select_modifier: SelectModifier) {
  unsafe {
    let instance = CAD_INSTANCE.as_mut().unwrap();
    instance.scene_composer.select_cluster_by_id(cluster_id, select_modifier);
    refresh_renderer(instance, false);
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn translate_along_local_axis(axis_index: u32, translation: f64) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.scene_composer.translate_along_local_axis(axis_index, translation);
      refresh_renderer(instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn rotate_around_local_axis(axis_index: u32, angle_degrees: f64) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.scene_composer.rotate_around_local_axis(axis_index, angle_degrees);
      refresh_renderer(instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn is_frame_locked_to_atoms() -> bool {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      return instance.scene_composer.model.is_frame_locked_to_atoms();
    }
  }
  
  false
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_frame_locked_to_atoms(locked: bool) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.scene_composer.set_frame_locked_to_atoms(locked);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_align_atom_by_ray(ray_start: APIVec3, ray_dir: APIVec3) -> Option<u64> {
  unsafe {
    if let Some(ref mut cad_instance) = CAD_INSTANCE {
      let ray_start_dvec3 = from_api_vec3(&ray_start);
      let ray_dir_dvec3 = from_api_vec3(&ray_dir);
      let ret = cad_instance.scene_composer.select_align_atom_by_ray(&ray_start_dvec3, &ray_dir_dvec3);
      refresh_renderer(cad_instance, false);
      return ret;
    }
  }
  None
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_active_scene_composer_tool(tool: APISceneComposerTool) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.scene_composer.set_active_tool(tool);
      refresh_renderer(instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_align_tool_state_text() -> String {
  let start_time = Instant::now();
  
  let result = unsafe {
    if let Some(ref mut cad_instance) = CAD_INSTANCE {
      cad_instance.scene_composer.get_align_tool_state_text()
    } else {
      String::new()
    }
  };
  
  result
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_distance_tool_state_text() -> String {
  let start_time = Instant::now();
  
  let result = unsafe {
    if let Some(ref mut cad_instance) = CAD_INSTANCE {
      cad_instance.scene_composer.get_distance_tool_state_text()
    } else {
      String::new()
    }
  };

  result
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_atom_info_atom_by_ray(ray_start: APIVec3, ray_dir: APIVec3) -> Option<u64> {
  unsafe {
    if let Some(ref mut cad_instance) = CAD_INSTANCE {
      let ray_start_dvec3 = from_api_vec3(&ray_start);
      let ray_dir_dvec3 = from_api_vec3(&ray_dir);
      let ret = cad_instance.scene_composer.select_atom_info_atom_by_ray(&ray_start_dvec3, &ray_dir_dvec3);
      refresh_renderer(cad_instance, false);
      return ret;
    }
  }
  None
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_scene_composer_atom_info() -> Option<AtomView> {
  unsafe {
    let instance = CAD_INSTANCE.as_ref()?;

    let atom_id = instance.scene_composer.get_atom_info_atom_id()?;

    // Get the atom from the model
    let atom = instance.scene_composer.model.model.get_atom(atom_id)?;
    
    // Get the cluster from the model
    let cluster = instance.scene_composer.model.model.get_cluster(atom.cluster_id)?;

    // Extract atom information from ATOM_INFO hashmap or use defaults
    let atom_info = crate::common::common_constants::ATOM_INFO.get(&atom.atomic_number);
    let (symbol, element_name, covalent_radius) = atom_info.map_or_else(
        || (
            crate::common::common_constants::DEFAULT_ATOM_INFO.symbol.clone(),
            crate::common::common_constants::DEFAULT_ATOM_INFO.element_name.clone(),
            crate::common::common_constants::DEFAULT_ATOM_INFO.radius
        ),
        |info| (
            info.symbol.clone(),
            info.element_name.clone(),
            info.radius
        )
    );
    
    // Convert the atom to an AtomView
    Some(AtomView {
      id: atom.id,
      atomic_number: atom.atomic_number,
      symbol: symbol.to_string(),
      cluster_id: atom.cluster_id,
      cluster_name: cluster.name.clone(),
      position: to_api_vec3(&atom.position),
      element_name: element_name,
      covalent_radius: covalent_radius,
    })
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_distance_atom_by_ray(ray_start: APIVec3, ray_dir: APIVec3) -> Option<u64> {
  unsafe {
    if let Some(ref mut cad_instance) = CAD_INSTANCE {
      let ray_start_dvec3 = from_api_vec3(&ray_start);
      let ray_dir_dvec3 = from_api_vec3(&ray_dir);
      let ret = cad_instance.scene_composer.select_distance_atom_by_ray(&ray_start_dvec3, &ray_dir_dvec3);
      refresh_renderer(cad_instance, false);
      return ret;
    }
  }
  None
}

#[flutter_rust_bridge::frb(sync)]
pub fn scene_composer_new_model() {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.scene_composer.new_model();
      refresh_renderer(instance, false);
    }
  }
}
