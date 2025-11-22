use crate::structure_designer::nodes::edit_atom::edit_atom;
use crate::api::api_common::refresh_structure_designer_auto;
use crate::api::common_api_types::APIVec3;
use crate::api::common_api_types::SelectModifier;
use crate::api::common_api_types::APITransform;
use crate::api::api_common::from_api_vec3;
use crate::api::api_common::from_api_transform;
use crate::api::structure_designer::structure_designer_api_types::APIEditAtomTool;
use crate::api::api_common::with_mut_cad_instance;
use crate::api::api_common::with_mut_cad_instance_or;
use crate::api::api_common::with_cad_instance_or;

#[flutter_rust_bridge::frb(sync)]
pub fn select_atom_or_bond_by_ray(ray_start: APIVec3, ray_dir: APIVec3, select_modifier: SelectModifier) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let ray_start_vec3 = from_api_vec3(&ray_start);
        let ray_dir_vec3 = from_api_vec3(&ray_dir);
        let result = edit_atom::select_atom_or_bond_by_ray(&mut cad_instance.structure_designer, &ray_start_vec3, &ray_dir_vec3, select_modifier);
        // Edit atom operations modify the edit_atom node's internal state
        refresh_structure_designer_auto(cad_instance);
        result
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn delete_selected_atoms_and_bonds() {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      edit_atom::delete_selected_atoms_and_bonds(&mut cad_instance.structure_designer);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_atom_by_ray(atomic_number: i16, plane_normal: APIVec3, ray_start: APIVec3, ray_dir: APIVec3) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let plane_normal_vec3 = from_api_vec3(&plane_normal);
      let ray_start_vec3 = from_api_vec3(&ray_start);
      let ray_dir_vec3 = from_api_vec3(&ray_dir);
      edit_atom::add_atom_by_ray(&mut cad_instance.structure_designer, atomic_number, &plane_normal_vec3, &ray_start_vec3, &ray_dir_vec3);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn replace_selected_atoms(atomic_number: i16) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      edit_atom::replace_selected_atoms(&mut cad_instance.structure_designer, atomic_number);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn edit_atom_undo() {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      edit_atom::edit_atom_undo(&mut cad_instance.structure_designer);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn edit_atom_redo() {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      edit_atom::edit_atom_redo(&mut cad_instance.structure_designer);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn transform_selected(abs_transform: APITransform) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      // Convert APITransform to Transform using the existing helper function
      let transform = from_api_transform(&abs_transform);
      
      edit_atom::transform_selected(&mut cad_instance.structure_designer, &transform);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn draw_bond_by_ray(ray_start: APIVec3, ray_dir: APIVec3) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let ray_start_dvec3 = from_api_vec3(&ray_start);
      let ray_dir_dvec3 = from_api_vec3(&ray_dir);
      edit_atom::draw_bond_by_ray(&mut cad_instance.structure_designer, &ray_start_dvec3, &ray_dir_dvec3);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_active_edit_atom_tool() -> Option<APIEditAtomTool> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        // Get the edit atom data and return its active tool
        match edit_atom::get_active_edit_atom_data(&cad_instance.structure_designer) {
          Some(edit_atom_data) => Some(edit_atom_data.get_active_tool()),
          None => None,
        }
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_active_edit_atom_tool(tool: APIEditAtomTool) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        // Get the edit atom data and set its active tool
        if let Some(edit_atom_data) = edit_atom::get_selected_edit_atom_data_mut(&mut cad_instance.structure_designer) {
          edit_atom_data.set_active_tool(tool);
          // Edit atom operations modify the edit_atom node's internal state
          refresh_structure_designer_auto(cad_instance);
          true
        } else {
          false
        }
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_edit_atom_default_data(replacement_atomic_number: i16) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        if let Some(edit_atom_data) = edit_atom::get_selected_edit_atom_data_mut(&mut cad_instance.structure_designer) {
          let result = edit_atom_data.set_default_tool_atomic_number(replacement_atomic_number);
          // Edit atom operations modify the edit_atom node's internal state
          refresh_structure_designer_auto(cad_instance);
          result
        } else {
          false
        }
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_edit_atom_add_atom_data(atomic_number: i16) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        if let Some(edit_atom_data) = edit_atom::get_selected_edit_atom_data_mut(&mut cad_instance.structure_designer) {
          let result = edit_atom_data.set_add_atom_tool_atomic_number(atomic_number);
          // Edit atom operations modify the edit_atom node's internal state
          refresh_structure_designer_auto(cad_instance);
          result
        } else {
          false
        }
      },
      false
    )
  }
}
