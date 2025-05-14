use crate::structure_designer::nodes::edit_atom::edit_atom;
use crate::api::api_common::refresh_renderer;
use crate::api::api_common::CAD_INSTANCE;
use crate::api::common_api_types::APIVec3;
use crate::api::common_api_types::SelectModifier;
use crate::api::common_api_types::APITransform;
use crate::api::api_common::from_api_vec3;
use crate::api::api_common::from_api_transform;
use crate::api::structure_designer::structure_designer_api_types::APIEditAtomTool;

#[flutter_rust_bridge::frb(sync)]
pub fn is_edit_atom_active() -> bool {
  unsafe {
    if let Some(instance) = &CAD_INSTANCE {
      edit_atom::is_edit_atom_active(&instance.structure_designer)
    } else {
      false
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_atom_or_bond_by_ray(ray_start: APIVec3, ray_dir: APIVec3, select_modifier: SelectModifier) -> bool {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      let ray_start_vec3 = from_api_vec3(&ray_start);
      let ray_dir_vec3 = from_api_vec3(&ray_dir);
      let result = edit_atom::select_atom_or_bond_by_ray(&mut instance.structure_designer, &ray_start_vec3, &ray_dir_vec3, select_modifier);
      refresh_renderer(instance, false);
      return result;
    }
    false
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn delete_selected_atoms_and_bonds() {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      edit_atom::delete_selected_atoms_and_bonds(&mut instance.structure_designer);
      refresh_renderer(instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_atom_by_ray(atomic_number: i32, plane_normal: APIVec3, ray_start: APIVec3, ray_dir: APIVec3) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      let plane_normal_vec3 = from_api_vec3(&plane_normal);
      let ray_start_vec3 = from_api_vec3(&ray_start);
      let ray_dir_vec3 = from_api_vec3(&ray_dir);
      edit_atom::add_atom_by_ray(&mut instance.structure_designer, atomic_number, &plane_normal_vec3, &ray_start_vec3, &ray_dir_vec3);
      refresh_renderer(instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn replace_selected_atoms(atomic_number: i32) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      edit_atom::replace_selected_atoms(&mut instance.structure_designer, atomic_number);
      refresh_renderer(instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn edit_atom_undo() {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      edit_atom::edit_atom_undo(&mut instance.structure_designer);
      refresh_renderer(instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn edit_atom_redo() {
  unsafe {
    if let Some(cad_instance) = &mut CAD_INSTANCE {
      edit_atom::edit_atom_redo(&mut cad_instance.structure_designer);
      refresh_renderer(cad_instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn transform_selected(abs_transform: APITransform) {
  unsafe {
    if let Some(cad_instance) = &mut CAD_INSTANCE {
      // Convert APITransform to Transform using the existing helper function
      let transform = from_api_transform(&abs_transform);
      
      edit_atom::transform_selected(&mut cad_instance.structure_designer, &transform);
      refresh_renderer(cad_instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn draw_bond_by_ray(ray_start: APIVec3, ray_dir: APIVec3) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      let ray_start_dvec3 = from_api_vec3(&ray_start);
      let ray_dir_dvec3 = from_api_vec3(&ray_dir);
      edit_atom::draw_bond_by_ray(&mut instance.structure_designer, &ray_start_dvec3, &ray_dir_dvec3);
      refresh_renderer(instance, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_active_edit_atom_tool() -> Option<APIEditAtomTool> {
  unsafe {
    let cad_instance = CAD_INSTANCE.as_ref()?;
    
    // Get the edit atom data and return its active tool
    let edit_atom_data = edit_atom::get_active_edit_atom_data(&cad_instance.structure_designer)?;
    Some(edit_atom_data.get_active_tool())
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_active_edit_atom_tool(tool: APIEditAtomTool) -> bool {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      // Get the edit atom data and set its active tool
      if let Some(edit_atom_data) = edit_atom::get_active_edit_atom_data_mut(&mut instance.structure_designer) {
        edit_atom_data.set_active_tool(tool);
        refresh_renderer(instance, false);
        return true;
      }
    }
    false
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_edit_atom_default_data(replacement_atomic_number: i32) -> bool {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      if let Some(edit_atom_data) = edit_atom::get_active_edit_atom_data_mut(&mut instance.structure_designer) {
        let result = edit_atom_data.set_default_tool_atomic_number(replacement_atomic_number);
        refresh_renderer(instance, false);
        return result;
      }
    }
    false
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_edit_atom_add_atom_data(atomic_number: i32) -> bool {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      if let Some(edit_atom_data) = edit_atom::get_active_edit_atom_data_mut(&mut instance.structure_designer) {
        let result = edit_atom_data.set_add_atom_tool_atomic_number(atomic_number);
        refresh_renderer(instance, false);
        return result;
      }
    }
    false
  }
}
