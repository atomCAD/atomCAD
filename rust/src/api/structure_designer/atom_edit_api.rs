use crate::api::api_common::from_api_transform;
use crate::api::api_common::from_api_vec3;
use crate::api::api_common::refresh_structure_designer_auto;
use crate::api::api_common::with_mut_cad_instance;
use crate::api::api_common::with_mut_cad_instance_or;
use crate::api::common_api_types::APITransform;
use crate::api::common_api_types::APIVec3;
use crate::api::common_api_types::SelectModifier;
use crate::api::structure_designer::structure_designer_api_types::APIAtomEditTool;
use crate::api::structure_designer::structure_designer_api_types::APIMinimizeFreezeMode;
use crate::structure_designer::nodes::atom_edit::atom_edit;
use crate::structure_designer::nodes::atom_edit::atom_edit::MinimizeFreezeMode;

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_select_by_ray(
    ray_start: APIVec3,
    ray_dir: APIVec3,
    select_modifier: SelectModifier,
) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let ray_start_vec3 = from_api_vec3(&ray_start);
                let ray_dir_vec3 = from_api_vec3(&ray_dir);
                let result = atom_edit::select_atom_or_bond_by_ray(
                    &mut cad_instance.structure_designer,
                    &ray_start_vec3,
                    &ray_dir_vec3,
                    select_modifier,
                );
                refresh_structure_designer_auto(cad_instance);
                result
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_add_atom_by_ray(
    atomic_number: i16,
    plane_normal: APIVec3,
    ray_start: APIVec3,
    ray_dir: APIVec3,
) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let plane_normal_vec3 = from_api_vec3(&plane_normal);
            let ray_start_vec3 = from_api_vec3(&ray_start);
            let ray_dir_vec3 = from_api_vec3(&ray_dir);
            atom_edit::add_atom_by_ray(
                &mut cad_instance.structure_designer,
                atomic_number,
                &plane_normal_vec3,
                &ray_start_vec3,
                &ray_dir_vec3,
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_draw_bond_by_ray(ray_start: APIVec3, ray_dir: APIVec3) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let ray_start_dvec3 = from_api_vec3(&ray_start);
            let ray_dir_dvec3 = from_api_vec3(&ray_dir);
            atom_edit::draw_bond_by_ray(
                &mut cad_instance.structure_designer,
                &ray_start_dvec3,
                &ray_dir_dvec3,
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_delete_selected() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            atom_edit::delete_selected_atoms_and_bonds(&mut cad_instance.structure_designer);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_replace_selected(atomic_number: i16) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            atom_edit::replace_selected_atoms(&mut cad_instance.structure_designer, atomic_number);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_transform_selected(abs_transform: APITransform) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let transform = from_api_transform(&abs_transform);
            atom_edit::transform_selected(&mut cad_instance.structure_designer, &transform);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_toggle_output_diff() -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                if let Some(atom_edit_data) =
                    atom_edit::get_selected_atom_edit_data_mut(&mut cad_instance.structure_designer)
                {
                    atom_edit_data.output_diff = !atom_edit_data.output_diff;
                    refresh_structure_designer_auto(cad_instance);
                    true
                } else {
                    false
                }
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_toggle_show_anchor_arrows() -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                if let Some(atom_edit_data) =
                    atom_edit::get_selected_atom_edit_data_mut(&mut cad_instance.structure_designer)
                {
                    atom_edit_data.show_anchor_arrows = !atom_edit_data.show_anchor_arrows;
                    refresh_structure_designer_auto(cad_instance);
                    true
                } else {
                    false
                }
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_toggle_include_base_bonds_in_diff() -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                if let Some(atom_edit_data) =
                    atom_edit::get_selected_atom_edit_data_mut(&mut cad_instance.structure_designer)
                {
                    atom_edit_data.include_base_bonds_in_diff =
                        !atom_edit_data.include_base_bonds_in_diff;
                    refresh_structure_designer_auto(cad_instance);
                    true
                } else {
                    false
                }
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_active_atom_edit_tool() -> Option<APIAtomEditTool> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| match atom_edit::get_active_atom_edit_data(
                &cad_instance.structure_designer,
            ) {
                Some(atom_edit_data) => Some(atom_edit_data.get_active_tool()),
                None => None,
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_active_atom_edit_tool(tool: APIAtomEditTool) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                if let Some(atom_edit_data) =
                    atom_edit::get_selected_atom_edit_data_mut(&mut cad_instance.structure_designer)
                {
                    atom_edit_data.set_active_tool(tool);
                    refresh_structure_designer_auto(cad_instance);
                    true
                } else {
                    false
                }
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_atom_edit_default_data(replacement_atomic_number: i16) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                if let Some(atom_edit_data) =
                    atom_edit::get_selected_atom_edit_data_mut(&mut cad_instance.structure_designer)
                {
                    let result =
                        atom_edit_data.set_default_tool_atomic_number(replacement_atomic_number);
                    refresh_structure_designer_auto(cad_instance);
                    result
                } else {
                    false
                }
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_atom_edit_add_atom_data(atomic_number: i16) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                if let Some(atom_edit_data) =
                    atom_edit::get_selected_atom_edit_data_mut(&mut cad_instance.structure_designer)
                {
                    let result = atom_edit_data.set_add_atom_tool_atomic_number(atomic_number);
                    refresh_structure_designer_auto(cad_instance);
                    result
                } else {
                    false
                }
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_minimize(freeze_mode: APIMinimizeFreezeMode) -> String {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let internal_mode = match freeze_mode {
                    APIMinimizeFreezeMode::FreezeBase => MinimizeFreezeMode::FreezeBase,
                    APIMinimizeFreezeMode::FreeAll => MinimizeFreezeMode::FreeAll,
                    APIMinimizeFreezeMode::FreeSelected => MinimizeFreezeMode::FreeSelected,
                };
                let result = atom_edit::minimize_atom_edit(
                    &mut cad_instance.structure_designer,
                    internal_mode,
                );
                refresh_structure_designer_auto(cad_instance);
                match result {
                    Ok(message) => message,
                    Err(error) => format!("Error: {}", error),
                }
            },
            "Error: no active instance".to_string(),
        )
    }
}
