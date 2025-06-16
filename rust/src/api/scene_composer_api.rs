use crate::api::api_common::refresh_renderer;
use crate::api::api_common::with_mut_cad_instance;
use crate::api::api_common::with_cad_instance;
use crate::api::api_common::with_mut_cad_instance_or;
use crate::api::api_common::with_cad_instance_or;
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
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.scene_composer.undo();
        refresh_renderer(instance, false);
        result
      },
      false // Default return value if CAD_INSTANCE is None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn scene_composer_redo() -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.scene_composer.redo();
        refresh_renderer(instance, false);
        result
      },
      false // Default return value if CAD_INSTANCE is None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_scene_composer_view() -> Option<SceneComposerView> {
  unsafe {
    with_cad_instance(|cad_instance| {
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
  
      scene_composer_view
    })
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_selected_frame_transform() -> Option<APITransform> {
  unsafe {
    with_cad_instance_or(
      |instance| {
        instance.scene_composer.model.get_selected_frame_transform()
          .map(|transform| to_api_transform(&transform))
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_selected_frame_transform(transform: APITransform) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.scene_composer.set_selected_frame_transform(from_api_transform(&transform));
      refresh_renderer(instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn scene_composer_rename_cluster(cluster_id: u64, new_name: String) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.scene_composer.model.model.rename_cluster(cluster_id, &new_name);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn import_xyz(file_path: &str) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.scene_composer.import_xyz(file_path).unwrap();
      refresh_renderer(cad_instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn export_xyz(file_path: &str) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| cad_instance.scene_composer.export_xyz(file_path).is_ok(),
      false // Default return value if CAD_INSTANCE is None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_cluster_by_ray(ray_start: APIVec3, ray_dir: APIVec3, select_modifier: SelectModifier) -> Option<u64> {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        // First, run the function
        let selected_cluster = instance.scene_composer.select_cluster_by_ray(
          &from_api_vec3(&ray_start),
          &from_api_vec3(&ray_dir),
          select_modifier);
        // Then refresh the renderer
        refresh_renderer(instance, false);
        // Return the result (Option<u64>)
        selected_cluster
      },
      None // Default value if CAD_INSTANCE is None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_cluster_by_id(cluster_id: u64, select_modifier: SelectModifier) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.scene_composer.select_cluster_by_id(cluster_id, select_modifier);
      refresh_renderer(instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn translate_along_local_axis(axis_index: u32, translation: f64) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.scene_composer.translate_along_local_axis(axis_index, translation);
      refresh_renderer(instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn rotate_around_local_axis(axis_index: u32, angle_degrees: f64) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.scene_composer.rotate_around_local_axis(axis_index, angle_degrees);
      refresh_renderer(instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn is_frame_locked_to_atoms() -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| instance.scene_composer.model.is_frame_locked_to_atoms(),
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_frame_locked_to_atoms(locked: bool) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.scene_composer.set_frame_locked_to_atoms(locked);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_align_atom_by_ray(ray_start: APIVec3, ray_dir: APIVec3) -> Option<u64> {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let ray_start_dvec3 = from_api_vec3(&ray_start);
        let ray_dir_dvec3 = from_api_vec3(&ray_dir);
        let ret = cad_instance.scene_composer.select_align_atom_by_ray(&ray_start_dvec3, &ray_dir_dvec3);
        refresh_renderer(cad_instance, false);
        ret
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_active_scene_composer_tool(tool: APISceneComposerTool) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.scene_composer.set_active_tool(tool);
      refresh_renderer(instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_align_tool_state_text() -> String {
  let _start_time = Instant::now();
  
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| cad_instance.scene_composer.get_align_tool_state_text(),
      String::new()
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_distance_tool_state_text() -> String {
  let _start_time = Instant::now();
  
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| cad_instance.scene_composer.get_distance_tool_state_text(),
      String::new()
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_atom_info_atom_by_ray(ray_start: APIVec3, ray_dir: APIVec3) -> Option<u64> {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let ray_start_dvec3 = from_api_vec3(&ray_start);
        let ray_dir_dvec3 = from_api_vec3(&ray_dir);
        let ret = cad_instance.scene_composer.select_atom_info_atom_by_ray(&ray_start_dvec3, &ray_dir_dvec3);
        refresh_renderer(cad_instance, false);
        ret
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_scene_composer_atom_info() -> Option<AtomView> {
  unsafe {
    with_cad_instance_or(
      |instance| {
        // Get the atom ID
        let atom_id = match instance.scene_composer.get_atom_info_atom_id() {
          Some(id) => id,
          None => return None
        };

        // Get the atom from the model
        let atom = match instance.scene_composer.model.model.get_atom(atom_id) {
          Some(a) => a,
          None => return None
        };
        
        // Get the cluster from the model
        let cluster = match instance.scene_composer.model.model.get_cluster(atom.cluster_id) {
          Some(c) => c,
          None => return None
        };

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
        
        // Return the AtomView
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
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_distance_atom_by_ray(ray_start: APIVec3, ray_dir: APIVec3) -> Option<u64> {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let ray_start_dvec3 = from_api_vec3(&ray_start);
        let ray_dir_dvec3 = from_api_vec3(&ray_dir);
        let ret = cad_instance.scene_composer.select_distance_atom_by_ray(&ray_start_dvec3, &ray_dir_dvec3);
        refresh_renderer(cad_instance, false);
        ret
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn scene_composer_new_model() {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.scene_composer.new_model();
      refresh_renderer(cad_instance, false);
    });
  }
}
