use crate::api::api_common::from_api_vec3;
use crate::api::api_common::refresh_structure_designer_auto;
use crate::api::api_common::with_mut_cad_instance;
use crate::api::api_common::with_mut_cad_instance_or;
use crate::api::api_common::{from_api_transform, from_api_vec2};
use crate::api::common_api_types::APITransform;
use crate::api::common_api_types::APIVec2;
use crate::api::common_api_types::APIVec3;
use crate::api::common_api_types::SelectModifier;
use crate::api::structure_designer::structure_designer_api_types::APIAddBondMoveResult;
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

// --- AddBond tool pointer event API ---

/// Pointer down in AddBond tool. Returns whether an atom was hit.
/// Triggers one refresh if an atom is hit (to show source atom highlight).
#[flutter_rust_bridge::frb(sync)]
pub fn add_bond_pointer_down(
    screen_pos: APIVec2,
    ray_origin: APIVec3,
    ray_direction: APIVec3,
) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = atom_edit::add_bond_pointer_down(
                    &mut cad_instance.structure_designer,
                    from_api_vec2(&screen_pos),
                    &from_api_vec3(&ray_origin),
                    &from_api_vec3(&ray_direction),
                );
                if result {
                    refresh_structure_designer_auto(cad_instance);
                }
                result
            },
            false,
        )
    }
}

/// Pointer move in AddBond tool. Returns preview state for rubber-band rendering.
/// NO refresh, NO evaluation — only a ray-cast hit test.
#[flutter_rust_bridge::frb(sync)]
pub fn add_bond_pointer_move(
    screen_pos: APIVec2,
    ray_origin: APIVec3,
    ray_direction: APIVec3,
) -> APIAddBondMoveResult {
    let no_op = APIAddBondMoveResult {
        is_dragging: false,
        source_atom_x: 0.0,
        source_atom_y: 0.0,
        source_atom_z: 0.0,
        has_source_pos: false,
        preview_end_x: 0.0,
        preview_end_y: 0.0,
        preview_end_z: 0.0,
        has_preview_end: false,
        snapped_to_atom: false,
        bond_order: 1,
    };

    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = atom_edit::add_bond_pointer_move(
                    &mut cad_instance.structure_designer,
                    from_api_vec2(&screen_pos),
                    &from_api_vec3(&ray_origin),
                    &from_api_vec3(&ray_direction),
                );
                // Convert from internal AddBondMoveResult to API type
                APIAddBondMoveResult {
                    is_dragging: result.is_dragging,
                    source_atom_x: result.source_atom_pos.map_or(0.0, |p| p.x),
                    source_atom_y: result.source_atom_pos.map_or(0.0, |p| p.y),
                    source_atom_z: result.source_atom_pos.map_or(0.0, |p| p.z),
                    has_source_pos: result.source_atom_pos.is_some(),
                    preview_end_x: result.preview_end_pos.map_or(0.0, |p| p.x),
                    preview_end_y: result.preview_end_pos.map_or(0.0, |p| p.y),
                    preview_end_z: result.preview_end_pos.map_or(0.0, |p| p.z),
                    has_preview_end: result.preview_end_pos.is_some(),
                    snapped_to_atom: result.snapped_to_atom,
                    bond_order: result.bond_order,
                }
            },
            no_op,
        )
    }
}

/// Pointer up in AddBond tool. Creates bond if released on valid target.
/// Triggers one refresh to show the new bond (or remove source highlight on cancel).
#[flutter_rust_bridge::frb(sync)]
pub fn add_bond_pointer_up(ray_origin: APIVec3, ray_direction: APIVec3) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = atom_edit::add_bond_pointer_up(
                    &mut cad_instance.structure_designer,
                    &from_api_vec3(&ray_origin),
                    &from_api_vec3(&ray_direction),
                );
                refresh_structure_designer_auto(cad_instance);
                result
            },
            false,
        )
    }
}

/// Cancel AddBond tool interaction (reset to Idle).
#[flutter_rust_bridge::frb(sync)]
pub fn add_bond_pointer_cancel() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            atom_edit::add_bond_reset_interaction(&mut cad_instance.structure_designer);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Set the bond order for the AddBond tool (1-7).
#[flutter_rust_bridge::frb(sync)]
pub fn set_add_bond_order(order: u8) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            atom_edit::set_add_bond_order(&mut cad_instance.structure_designer, order);
        });
    }
}

/// Change the order of all selected bonds (in Default tool).
#[flutter_rust_bridge::frb(sync)]
pub fn change_selected_bonds_order(new_order: u8) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            atom_edit::change_selected_bonds_order(&mut cad_instance.structure_designer, new_order);
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
pub fn atom_edit_toggle_error_on_stale_entries() -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                if let Some(atom_edit_data) =
                    atom_edit::get_selected_atom_edit_data_mut(&mut cad_instance.structure_designer)
                {
                    atom_edit_data.error_on_stale_entries = !atom_edit_data.error_on_stale_entries;
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
pub fn set_atom_edit_selected_element(atomic_number: i16) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            if let Some(atom_edit_data) =
                atom_edit::get_selected_atom_edit_data_mut(&mut cad_instance.structure_designer)
            {
                atom_edit_data.set_selected_element(atomic_number);
                refresh_structure_designer_auto(cad_instance);
            }
        });
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
    hybridization_override: crate::api::structure_designer::structure_designer_api_types::APIHybridization,
    bond_mode: crate::api::structure_designer::structure_designer_api_types::APIBondMode,
    bond_length_mode: crate::api::structure_designer::structure_designer_api_types::APIBondLengthMode,
) -> crate::api::structure_designer::structure_designer_api_types::GuidedPlacementApiResult {
    use crate::api::structure_designer::structure_designer_api_types::{
        APIBondLengthMode, APIBondMode, APIHybridization, GuidedPlacementApiResult,
    };
    use crate::crystolecule::guided_placement::{BondLengthMode, BondMode, Hybridization};

    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let ray_start_vec3 = from_api_vec3(&ray_start);
                let ray_dir_vec3 = from_api_vec3(&ray_dir);
                let hyb_override = match hybridization_override {
                    APIHybridization::Auto => None,
                    APIHybridization::Sp3 => Some(Hybridization::Sp3),
                    APIHybridization::Sp2 => Some(Hybridization::Sp2),
                    APIHybridization::Sp1 => Some(Hybridization::Sp1),
                };
                let bond_mode_internal = match bond_mode {
                    APIBondMode::Covalent => BondMode::Covalent,
                    APIBondMode::Dative => BondMode::Dative,
                };
                let length_mode = match bond_length_mode {
                    APIBondLengthMode::Crystal => BondLengthMode::Crystal,
                    APIBondLengthMode::Uff => BondLengthMode::Uff,
                };
                let result = atom_edit::start_guided_placement(
                    &mut cad_instance.structure_designer,
                    &ray_start_vec3,
                    &ray_dir_vec3,
                    atomic_number,
                    hyb_override,
                    bond_mode_internal,
                    length_mode,
                );
                refresh_structure_designer_auto(cad_instance);
                match result {
                    atom_edit::GuidedPlacementStartResult::NoAtomHit => {
                        GuidedPlacementApiResult::NoAtomHit
                    }
                    atom_edit::GuidedPlacementStartResult::AtomSaturated {
                        has_additional_capacity,
                        dative_incompatible,
                    } => GuidedPlacementApiResult::AtomSaturated {
                        has_additional_capacity,
                        dative_incompatible,
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
            |cad_instance| atom_edit::is_in_guided_placement(&cad_instance.structure_designer),
            false,
        )
    }
}

/// Update the preview position for free sphere guided placement.
/// Returns true if the preview changed (needs re-render).
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_guided_placement_pointer_move(ray_start: APIVec3, ray_dir: APIVec3) -> bool {
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

// --- Modify measurement API ---

/// Modify the distance between two selected atoms.
/// `move_first`: true = move atom1, false = move atom2.
/// Returns an error message string on failure, empty string on success.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_modify_distance(
    target_distance: f64,
    move_first: bool,
    move_fragment: bool,
) -> String {
    use crate::structure_designer::nodes::atom_edit::atom_edit::{
        DistanceMoveChoice, modify_distance,
    };

    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let move_choice = if move_first {
                    DistanceMoveChoice::First
                } else {
                    DistanceMoveChoice::Second
                };
                let result = modify_distance(
                    &mut cad_instance.structure_designer,
                    target_distance,
                    move_choice,
                    move_fragment,
                );
                refresh_structure_designer_auto(cad_instance);
                match result {
                    Ok(()) => String::new(),
                    Err(e) => e,
                }
            },
            "Error: no active instance".to_string(),
        )
    }
}

/// Modify the angle at the vertex of three selected atoms.
/// `move_arm_a`: true = move arm A, false = move arm B.
/// Returns an error message string on failure, empty string on success.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_modify_angle(
    target_angle_degrees: f64,
    move_arm_a: bool,
    move_fragment: bool,
) -> String {
    use crate::structure_designer::nodes::atom_edit::atom_edit::{AngleMoveChoice, modify_angle};

    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let move_choice = if move_arm_a {
                    AngleMoveChoice::ArmA
                } else {
                    AngleMoveChoice::ArmB
                };
                let result = modify_angle(
                    &mut cad_instance.structure_designer,
                    target_angle_degrees,
                    move_choice,
                    move_fragment,
                );
                refresh_structure_designer_auto(cad_instance);
                match result {
                    Ok(()) => String::new(),
                    Err(e) => e,
                }
            },
            "Error: no active instance".to_string(),
        )
    }
}

/// Modify the dihedral angle of four selected atoms.
/// `move_a_side`: true = rotate A-side, false = rotate D-side.
/// Returns an error message string on failure, empty string on success.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_modify_dihedral(
    target_angle_degrees: f64,
    move_a_side: bool,
    move_fragment: bool,
) -> String {
    use crate::structure_designer::nodes::atom_edit::atom_edit::{
        DihedralMoveChoice, modify_dihedral,
    };

    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let move_choice = if move_a_side {
                    DihedralMoveChoice::ASide
                } else {
                    DihedralMoveChoice::DSide
                };
                let result = modify_dihedral(
                    &mut cad_instance.structure_designer,
                    target_angle_degrees,
                    move_choice,
                    move_fragment,
                );
                refresh_structure_designer_auto(cad_instance);
                match result {
                    Ok(()) => String::new(),
                    Err(e) => e,
                }
            },
            "Error: no active instance".to_string(),
        )
    }
}

// --- Measurement mark API ---

/// Mark a result-space atom for highlighting while the modify measurement dialog is open.
/// Triggers a refresh to render the yellow crosshair.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_set_measurement_mark(result_atom_id: u32) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            if let Some(atom_edit_data) =
                atom_edit::get_selected_atom_edit_data_mut(&mut cad_instance.structure_designer)
            {
                atom_edit_data.measurement_marked_atom_id = Some(result_atom_id);
            }
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Clear the measurement mark (when the dialog closes).
/// Triggers a refresh to remove the crosshair.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_clear_measurement_mark() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            if let Some(atom_edit_data) =
                atom_edit::get_selected_atom_edit_data_mut(&mut cad_instance.structure_designer)
            {
                atom_edit_data.measurement_marked_atom_id = None;
            }
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Get the default (equilibrium) bond length for the two selected atoms.
/// Returns None if atoms are not bonded or if UFF typing fails.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_get_default_bond_length(
    bond_length_mode: crate::api::structure_designer::structure_designer_api_types::APIBondLengthMode,
) -> Option<f64> {
    use crate::api::api_common::with_cad_instance_or;
    use crate::api::structure_designer::structure_designer_api_types::APIBondLengthMode;
    use crate::crystolecule::guided_placement::BondLengthMode;
    use crate::structure_designer::nodes::atom_edit::atom_edit::compute_default_bond_length;

    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let mode = match bond_length_mode {
                    APIBondLengthMode::Crystal => BondLengthMode::Crystal,
                    APIBondLengthMode::Uff => BondLengthMode::Uff,
                };
                compute_default_bond_length(&cad_instance.structure_designer, mode)
            },
            None,
        )
    }
}

/// Get the default (equilibrium) angle for the three selected atoms.
/// Returns None if UFF typing fails for the vertex atom.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_get_default_angle() -> Option<f64> {
    use crate::api::api_common::with_cad_instance_or;
    use crate::structure_designer::nodes::atom_edit::atom_edit::compute_default_angle;

    unsafe {
        with_cad_instance_or(
            |cad_instance| compute_default_angle(&cad_instance.structure_designer),
            None,
        )
    }
}
