use crate::api::api_common::from_api_vec3;
use crate::api::api_common::refresh_structure_designer_auto;
use crate::api::api_common::with_mut_cad_instance;
use crate::api::api_common::with_mut_cad_instance_or;
use crate::api::api_common::{from_api_transform, from_api_vec2};
use crate::api::common_api_types::APITransform;
use crate::api::common_api_types::APIVec2;
use crate::api::common_api_types::APIVec3;
use crate::api::common_api_types::SelectModifier;
use crate::api::structure_designer::structure_designer_api_types::APIAtomEditTool;
use crate::api::structure_designer::structure_designer_api_types::APIMinimizeFreezeMode;
use crate::api::structure_designer::structure_designer_api_types::PointerDownResult;
use crate::api::structure_designer::structure_designer_api_types::PointerDownResultKind;
use crate::api::structure_designer::structure_designer_api_types::PointerMoveResult;
use crate::api::structure_designer::structure_designer_api_types::PointerMoveResultKind;
use crate::api::structure_designer::structure_designer_api_types::PointerUpResult;
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
pub fn atom_edit_toggle_show_gadget() -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                if let Some(atom_edit_data) =
                    atom_edit::get_selected_atom_edit_data_mut(&mut cad_instance.structure_designer)
                {
                    if let atom_edit::AtomEditTool::Default(ref mut state) =
                        atom_edit_data.active_tool
                    {
                        state.show_gadget = !state.show_gadget;
                        refresh_structure_designer_auto(cad_instance);
                        return true;
                    }
                    false
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

// --- Guided atom placement API ---

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_start_guided_placement(
    ray_start: APIVec3,
    ray_dir: APIVec3,
    atomic_number: i16,
    bond_length_mode: crate::api::structure_designer::structure_designer_api_types::APIBondLengthMode,
) -> crate::api::structure_designer::structure_designer_api_types::GuidedPlacementApiResult {
    use crate::api::structure_designer::structure_designer_api_types::{
        APIBondLengthMode, GuidedPlacementApiResult,
    };
    use crate::crystolecule::guided_placement::BondLengthMode;

    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let ray_start_vec3 = from_api_vec3(&ray_start);
                let ray_dir_vec3 = from_api_vec3(&ray_dir);
                let mode = match bond_length_mode {
                    APIBondLengthMode::Crystal => BondLengthMode::Crystal,
                    APIBondLengthMode::Uff => BondLengthMode::Uff,
                };
                let result = atom_edit::start_guided_placement(
                    &mut cad_instance.structure_designer,
                    &ray_start_vec3,
                    &ray_dir_vec3,
                    atomic_number,
                    mode,
                );
                refresh_structure_designer_auto(cad_instance);
                match result {
                    atom_edit::GuidedPlacementStartResult::NoAtomHit => {
                        GuidedPlacementApiResult::NoAtomHit
                    }
                    atom_edit::GuidedPlacementStartResult::AtomSaturated {
                        has_additional_capacity,
                    } => GuidedPlacementApiResult::AtomSaturated {
                        has_additional_capacity,
                    },
                    atom_edit::GuidedPlacementStartResult::Started {
                        guide_count,
                        anchor_atom_id,
                    } => GuidedPlacementApiResult::GuidedPlacementStarted {
                        guide_count: guide_count as i32,
                        anchor_atom_id: anchor_atom_id as i32,
                    },
                }
            },
            GuidedPlacementApiResult::NoAtomHit,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_place_guided_atom(ray_start: APIVec3, ray_dir: APIVec3) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let ray_start_vec3 = from_api_vec3(&ray_start);
                let ray_dir_vec3 = from_api_vec3(&ray_dir);
                let result = atom_edit::place_guided_atom(
                    &mut cad_instance.structure_designer,
                    &ray_start_vec3,
                    &ray_dir_vec3,
                );
                refresh_structure_designer_auto(cad_instance);
                result
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_cancel_guided_placement() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            atom_edit::cancel_guided_placement(&mut cad_instance.structure_designer);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_is_in_guided_placement() -> bool {
    use crate::api::api_common::with_cad_instance_or;
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                atom_edit::is_in_guided_placement(&cad_instance.structure_designer)
            },
            false,
        )
    }
}

/// Update the preview position for free sphere guided placement.
/// Returns true if the preview changed (needs re-render).
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_guided_placement_pointer_move(
    ray_start: APIVec3,
    ray_dir: APIVec3,
) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let ray_start_vec3 = from_api_vec3(&ray_start);
                let ray_dir_vec3 = from_api_vec3(&ray_dir);
                let changed = atom_edit::guided_placement_pointer_move(
                    &mut cad_instance.structure_designer,
                    &ray_start_vec3,
                    &ray_dir_vec3,
                );
                if changed {
                    refresh_structure_designer_auto(cad_instance);
                }
                changed
            },
            false,
        )
    }
}

// --- Default tool pointer event API ---

#[flutter_rust_bridge::frb(sync)]
pub fn default_tool_pointer_cancel() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            atom_edit::default_tool_reset_interaction(&mut cad_instance.structure_designer);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn default_tool_pointer_down(
    screen_pos: APIVec2,
    ray_origin: APIVec3,
    ray_direction: APIVec3,
    select_modifier: SelectModifier,
) -> PointerDownResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                atom_edit::default_tool_pointer_down(
                    &mut cad_instance.structure_designer,
                    from_api_vec2(&screen_pos),
                    &from_api_vec3(&ray_origin),
                    &from_api_vec3(&ray_direction),
                    select_modifier,
                )
            },
            PointerDownResult {
                kind: PointerDownResultKind::StartedOnEmpty,
                gadget_handle_index: -1,
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn default_tool_pointer_move(
    screen_pos: APIVec2,
    ray_origin: APIVec3,
    ray_direction: APIVec3,
    viewport_width: f64,
    viewport_height: f64,
) -> PointerMoveResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let camera = &cad_instance.renderer.camera;
                let camera_forward = (camera.target - camera.eye).normalize();
                let result = atom_edit::default_tool_pointer_move(
                    &mut cad_instance.structure_designer,
                    from_api_vec2(&screen_pos),
                    &from_api_vec3(&ray_origin),
                    &from_api_vec3(&ray_direction),
                    viewport_width,
                    viewport_height,
                    &camera_forward,
                );
                // During drag, re-evaluate the atom_edit node so atom positions
                // update visually, but skip downstream dependents for performance.
                if matches!(result.kind, PointerMoveResultKind::Dragging) {
                    cad_instance.structure_designer.mark_skip_downstream();
                    refresh_structure_designer_auto(cad_instance);
                }
                result
            },
            PointerMoveResult {
                kind: PointerMoveResultKind::StillPending,
                marquee_rect_x: 0.0,
                marquee_rect_y: 0.0,
                marquee_rect_w: 0.0,
                marquee_rect_h: 0.0,
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn default_tool_pointer_up(
    screen_pos: APIVec2,
    ray_origin: APIVec3,
    ray_direction: APIVec3,
    select_modifier: SelectModifier,
    viewport_width: f64,
    viewport_height: f64,
) -> PointerUpResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let view_proj = cad_instance.renderer.camera.build_view_projection_matrix();
                let result = atom_edit::default_tool_pointer_up(
                    &mut cad_instance.structure_designer,
                    from_api_vec2(&screen_pos),
                    &from_api_vec3(&ray_origin),
                    &from_api_vec3(&ray_direction),
                    select_modifier,
                    viewport_width,
                    viewport_height,
                    &view_proj,
                );
                // Refresh after selection change (re-evaluates decorations)
                if !matches!(result, PointerUpResult::NothingHappened) {
                    refresh_structure_designer_auto(cad_instance);
                }
                result
            },
            PointerUpResult::NothingHappened,
        )
    }
}
