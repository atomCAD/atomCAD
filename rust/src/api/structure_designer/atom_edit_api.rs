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
use crate::api::structure_designer::structure_designer_api_types::DragFrozenStatus;
use crate::api::structure_designer::structure_designer_api_types::PointerDownResult;
use crate::api::structure_designer::structure_designer_api_types::PointerDownResultKind;
use crate::api::structure_designer::structure_designer_api_types::PointerMoveResult;
use crate::api::structure_designer::structure_designer_api_types::PointerMoveResultKind;
use crate::api::structure_designer::structure_designer_api_types::PointerUpResult;
use crate::structure_designer::nodes::atom_edit::atom_edit;
use crate::structure_designer::nodes::atom_edit::atom_edit::{AtomEditData, MinimizeFreezeMode};
use crate::structure_designer::nodes::atom_edit::operations::BaseAtomPromotionInfo;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::undo::commands::atom_edit_toggle_flag::{
    AtomEditFlag, AtomEditToggleFlagCommand,
};

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
    hybridization_override: crate::api::structure_designer::structure_designer_api_types::APIHybridization,
) {
    use crate::api::structure_designer::structure_designer_api_types::APIHybridization;
    use crate::crystolecule::guided_placement::Hybridization;

    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let plane_normal_vec3 = from_api_vec3(&plane_normal);
            let ray_start_vec3 = from_api_vec3(&ray_start);
            let ray_dir_vec3 = from_api_vec3(&ray_dir);
            let hyb_override = match hybridization_override {
                APIHybridization::Auto => None,
                APIHybridization::Sp3 => Some(Hybridization::Sp3),
                APIHybridization::Sp2 => Some(Hybridization::Sp2),
                APIHybridization::Sp1 => Some(Hybridization::Sp1),
            };
            atom_edit::with_atom_edit_undo(
                &mut cad_instance.structure_designer,
                "Add atom",
                |sd| {
                    atom_edit::add_atom_by_ray(
                        sd,
                        atomic_number,
                        &plane_normal_vec3,
                        &ray_start_vec3,
                        &ray_dir_vec3,
                        hyb_override,
                    );
                },
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Add an atom at an exact position (typed in by the user).
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_add_atom_at_position(
    atomic_number: i16,
    position: APIVec3,
    hybridization_override: crate::api::structure_designer::structure_designer_api_types::APIHybridization,
) {
    use crate::api::structure_designer::structure_designer_api_types::APIHybridization;
    use crate::crystolecule::atomic_structure::atom::{
        HYBRIDIZATION_AUTO, HYBRIDIZATION_SP1, HYBRIDIZATION_SP2, HYBRIDIZATION_SP3,
    };
    use crate::crystolecule::guided_placement::Hybridization;

    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let pos = from_api_vec3(&position);
            let hyb_override = match hybridization_override {
                APIHybridization::Auto => None,
                APIHybridization::Sp3 => Some(Hybridization::Sp3),
                APIHybridization::Sp2 => Some(Hybridization::Sp2),
                APIHybridization::Sp1 => Some(Hybridization::Sp1),
            };
            atom_edit::with_atom_edit_undo(
                &mut cad_instance.structure_designer,
                "Add atom at position",
                |sd| {
                    let atom_edit_data =
                        match atom_edit::get_selected_atom_edit_data_mut(sd) {
                            Some(data) => data,
                            None => return,
                        };
                    let new_atom_id =
                        atom_edit_data.add_atom_to_diff(atomic_number, pos);
                    let hyb_flag = match hyb_override {
                        None => HYBRIDIZATION_AUTO,
                        Some(Hybridization::Sp3) => HYBRIDIZATION_SP3,
                        Some(Hybridization::Sp2) => HYBRIDIZATION_SP2,
                        Some(Hybridization::Sp1) => HYBRIDIZATION_SP1,
                    };
                    if hyb_flag != HYBRIDIZATION_AUTO {
                        atom_edit_data
                            .set_hybridization_override_recorded(new_atom_id, hyb_flag);
                    }
                },
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
                let ray_origin_vec3 = from_api_vec3(&ray_origin);
                let ray_dir_vec3 = from_api_vec3(&ray_direction);
                let mut result = false;
                atom_edit::with_atom_edit_undo(
                    &mut cad_instance.structure_designer,
                    "Add bond",
                    |sd| {
                        result =
                            atom_edit::add_bond_pointer_up(sd, &ray_origin_vec3, &ray_dir_vec3);
                    },
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
            atom_edit::with_atom_edit_undo(
                &mut cad_instance.structure_designer,
                "Change bond order",
                |sd| {
                    atom_edit::change_selected_bonds_order(sd, new_order);
                },
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_delete_selected() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            atom_edit::with_atom_edit_undo(
                &mut cad_instance.structure_designer,
                "Delete atoms",
                |sd| {
                    atom_edit::delete_selected_atoms_and_bonds(sd);
                },
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_replace_selected(atomic_number: i16) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            atom_edit::with_atom_edit_undo(
                &mut cad_instance.structure_designer,
                "Replace atoms",
                |sd| {
                    atom_edit::replace_selected_atoms(sd, atomic_number);
                },
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_transform_selected(abs_transform: APITransform) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let transform = from_api_transform(&abs_transform);
            atom_edit::with_atom_edit_undo(
                &mut cad_instance.structure_designer,
                "Move atoms",
                |sd| {
                    atom_edit::transform_selected(sd, &transform);
                },
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Helper: toggle a boolean flag on AtomEditData and push an undo command.
fn toggle_atom_edit_flag(
    sd: &mut StructureDesigner,
    flag: AtomEditFlag,
    description: &str,
    accessor: fn(&mut AtomEditData) -> &mut bool,
) {
    let (network_name, node_id) = match atom_edit::get_atom_edit_node_info_pub(sd) {
        Some(info) => info,
        None => return,
    };
    if let Some(data) = atom_edit::get_selected_atom_edit_data_mut(sd) {
        let field = accessor(data);
        let old_value = *field;
        let new_value = !old_value;
        *field = new_value;
        sd.push_command(AtomEditToggleFlagCommand {
            description: description.to_string(),
            network_name,
            node_id,
            flag,
            old_value,
            new_value,
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_toggle_show_anchor_arrows() -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                toggle_atom_edit_flag(
                    &mut cad_instance.structure_designer,
                    AtomEditFlag::ShowAnchorArrows,
                    "Toggle anchor arrows",
                    |d| &mut d.show_anchor_arrows,
                );
                refresh_structure_designer_auto(cad_instance);
                true
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
                toggle_atom_edit_flag(
                    &mut cad_instance.structure_designer,
                    AtomEditFlag::IncludeBaseBondsInDiff,
                    "Toggle base bonds in diff",
                    |d| &mut d.include_base_bonds_in_diff,
                );
                refresh_structure_designer_auto(cad_instance);
                true
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
                toggle_atom_edit_flag(
                    &mut cad_instance.structure_designer,
                    AtomEditFlag::ErrorOnStaleEntries,
                    "Toggle error on stale entries",
                    |d| &mut d.error_on_stale_entries,
                );
                refresh_structure_designer_auto(cad_instance);
                true
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_toggle_continuous_minimization() -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                toggle_atom_edit_flag(
                    &mut cad_instance.structure_designer,
                    AtomEditFlag::ContinuousMinimization,
                    "Toggle continuous minimization",
                    |d| &mut d.continuous_minimization,
                );
                refresh_structure_designer_auto(cad_instance);
                true
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_set_tolerance(value: f64) -> bool {
    use crate::structure_designer::undo::commands::atom_edit_set_tolerance::AtomEditSetToleranceCommand;
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let (network_name, node_id) =
                    match atom_edit::get_atom_edit_node_info_pub(&cad_instance.structure_designer) {
                        Some(info) => info,
                        None => return false,
                    };
                if let Some(data) = atom_edit::get_selected_atom_edit_data_mut(
                    &mut cad_instance.structure_designer,
                ) {
                    let old_value = data.tolerance;
                    if (old_value - value).abs() < f64::EPSILON {
                        return true; // No change
                    }
                    data.tolerance = value;
                    cad_instance
                        .structure_designer
                        .push_command(AtomEditSetToleranceCommand {
                            network_name,
                            node_id,
                            old_value,
                            new_value: value,
                        });
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
                let mut result = Err("No active instance".to_string());
                atom_edit::with_atom_edit_undo(
                    &mut cad_instance.structure_designer,
                    "Minimize structure",
                    |sd| {
                        result = atom_edit::minimize_atom_edit(sd, internal_mode);
                    },
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

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_add_hydrogen(selected_only: bool) -> String {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let mut result = Err("No active instance".to_string());
                atom_edit::with_atom_edit_undo(
                    &mut cad_instance.structure_designer,
                    "Add hydrogen",
                    |sd| {
                        result = atom_edit::add_hydrogen_atom_edit(sd, selected_only);
                    },
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

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_remove_hydrogen(selected_only: bool) -> String {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let mut result = Err("No active instance".to_string());
                atom_edit::with_atom_edit_undo(
                    &mut cad_instance.structure_designer,
                    "Remove hydrogen",
                    |sd| {
                        result = atom_edit::remove_hydrogen_atom_edit(sd, selected_only);
                    },
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
                let mut result = false;
                atom_edit::with_atom_edit_undo(
                    &mut cad_instance.structure_designer,
                    "Place atom",
                    |sd| {
                        result = atom_edit::place_guided_atom(sd, &ray_start_vec3, &ray_dir_vec3);
                    },
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
                frozen_drag_status: DragFrozenStatus::NoneFrozen,
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
                let mut result = Err("No active instance".to_string());
                atom_edit::with_atom_edit_undo(
                    &mut cad_instance.structure_designer,
                    "Modify distance",
                    |sd| {
                        result = modify_distance(sd, target_distance, move_choice, move_fragment);
                    },
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
                let mut result = Err("No active instance".to_string());
                atom_edit::with_atom_edit_undo(
                    &mut cad_instance.structure_designer,
                    "Modify angle",
                    |sd| {
                        result = modify_angle(sd, target_angle_degrees, move_choice, move_fragment);
                    },
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
                let mut result = Err("No active instance".to_string());
                atom_edit::with_atom_edit_undo(
                    &mut cad_instance.structure_designer,
                    "Modify dihedral",
                    |sd| {
                        result =
                            modify_dihedral(sd, target_angle_degrees, move_choice, move_fragment);
                    },
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

// --- Frozen atom API ---

/// Gather promotion info for selected base atoms (Phase 1 of borrow split).
fn gather_selected_base_promotion_info(sd: &StructureDesigner) -> Vec<BaseAtomPromotionInfo> {
    use crate::structure_designer::nodes::atom_edit::operations::gather_base_atom_promotion_info;
    let data = match atom_edit::get_active_atom_edit_data(sd) {
        Some(d) => d,
        None => return Vec::new(),
    };
    if data.selection.selected_base_atoms.is_empty() || sd.is_selected_node_in_diff_view() {
        return Vec::new();
    }
    gather_base_atom_promotion_info(sd, &data.selection.selected_base_atoms)
}

/// Sets the frozen flag on all currently selected atoms (additive).
/// Base atoms are promoted to diff first.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_selection_to_frozen() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let sd = &mut cad_instance.structure_designer;
            // Phase 1: gather base atom promotion info (immutable borrow)
            let base_info = gather_selected_base_promotion_info(sd);
            // Phase 2: promote and set flags (mutable borrow via with_atom_edit_undo)
            atom_edit::with_atom_edit_undo(sd, "Freeze selection", |sd| {
                if let Some(data) = atom_edit::get_selected_atom_edit_data_mut(sd) {
                    // Promote base atoms to diff
                    for info in &base_info {
                        let diff_id = if let Some(existing_id) = info.existing_diff_id {
                            data.set_atomic_number_recorded(existing_id, info.atomic_number);
                            data.set_anchor_recorded(existing_id, info.position);
                            existing_id
                        } else {
                            let new_id = data.add_atom_recorded(info.atomic_number, info.position);
                            data.set_anchor_recorded(new_id, info.position);
                            new_id
                        };
                        data.promote_base_atom_metadata(info.flags, diff_id);
                        data.selection.selected_base_atoms.remove(&info.base_id);
                        data.selection.selected_diff_atoms.insert(diff_id);
                        data.set_frozen_recorded(diff_id, true);
                    }
                    // Set frozen on already-diff atoms
                    for &diff_id in &data.selection.selected_diff_atoms.clone() {
                        data.set_frozen_recorded(diff_id, true);
                    }
                }
            });
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Clears the frozen flag on all currently selected atoms.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_selection_to_unfrozen() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let sd = &mut cad_instance.structure_designer;
            atom_edit::with_atom_edit_undo(sd, "Unfreeze selection", |sd| {
                if let Some(data) = atom_edit::get_selected_atom_edit_data_mut(sd) {
                    // Only diff atoms can have frozen flag set (base atoms were promoted)
                    for &diff_id in &data.selection.selected_diff_atoms.clone() {
                        data.set_frozen_recorded(diff_id, false);
                    }
                }
            });
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Replaces the current selection with the set of frozen diff atoms.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_frozen_to_selection() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            if let Some(atom_edit_data) =
                atom_edit::get_selected_atom_edit_data_mut(&mut cad_instance.structure_designer)
            {
                // Frozen atoms are now always diff atoms (promoted on freeze)
                let frozen_diff: std::collections::HashSet<u32> = atom_edit_data
                    .diff
                    .iter_atoms()
                    .filter(|(_, a)| a.is_frozen())
                    .map(|(_, a)| a.id)
                    .collect();
                atom_edit_data.selection.selected_base_atoms.clear();
                atom_edit_data.selection.selected_diff_atoms = frozen_diff;
                atom_edit_data.selection.selected_bonds.clear();
                atom_edit_data.selection.selection_transform = None;
                refresh_structure_designer_auto(cad_instance);
            }
        });
    }
}

/// Clears the frozen flag from all diff atoms.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_clear_frozen() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let sd = &mut cad_instance.structure_designer;
            atom_edit::with_atom_edit_undo(sd, "Clear frozen atoms", |sd| {
                if let Some(data) = atom_edit::get_selected_atom_edit_data_mut(sd) {
                    let frozen_ids: Vec<u32> = data
                        .diff
                        .iter_atoms()
                        .filter(|(_, a)| a.is_frozen())
                        .map(|(_, a)| a.id)
                        .collect();
                    for diff_id in frozen_ids {
                        data.set_frozen_recorded(diff_id, false);
                    }
                }
            });
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Returns true if any diff atom has the frozen flag set.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_has_frozen_atoms() -> bool {
    use crate::api::api_common::with_cad_instance_or;
    unsafe {
        with_cad_instance_or(
            |cad_instance| match atom_edit::get_active_atom_edit_data(
                &cad_instance.structure_designer,
            ) {
                Some(data) => data.diff.iter_atoms().any(|(_, a)| a.is_frozen()),
                None => false,
            },
            false,
        )
    }
}

// --- Hybridization override API ---

/// Sets the hybridization override on all currently selected atoms.
/// `Auto` removes the override (restoring bond-based inference).
/// Base atoms are promoted to diff first.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_set_hybridization_override(
    hybridization: crate::api::structure_designer::structure_designer_api_types::APIHybridization,
) {
    use crate::api::structure_designer::structure_designer_api_types::APIHybridization;
    use crate::crystolecule::atomic_structure::atom::{
        HYBRIDIZATION_AUTO, HYBRIDIZATION_SP1, HYBRIDIZATION_SP2, HYBRIDIZATION_SP3,
    };

    let value: u8 = match hybridization {
        APIHybridization::Auto => HYBRIDIZATION_AUTO,
        APIHybridization::Sp3 => HYBRIDIZATION_SP3,
        APIHybridization::Sp2 => HYBRIDIZATION_SP2,
        APIHybridization::Sp1 => HYBRIDIZATION_SP1,
    };

    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let sd = &mut cad_instance.structure_designer;
            // Phase 1: gather base atom promotion info
            let base_info = gather_selected_base_promotion_info(sd);
            // Phase 2: promote and set hybridization
            atom_edit::with_atom_edit_undo(
                sd,
                &format!("Set hybridization to {:?}", hybridization),
                |sd| {
                    if let Some(data) = atom_edit::get_selected_atom_edit_data_mut(sd) {
                        // Promote base atoms to diff
                        for info in &base_info {
                            let diff_id = if let Some(existing_id) = info.existing_diff_id {
                                data.set_atomic_number_recorded(existing_id, info.atomic_number);
                                data.set_anchor_recorded(existing_id, info.position);
                                existing_id
                            } else {
                                let new_id =
                                    data.add_atom_recorded(info.atomic_number, info.position);
                                data.set_anchor_recorded(new_id, info.position);
                                new_id
                            };
                            data.promote_base_atom_metadata(info.flags, diff_id);
                            data.selection.selected_base_atoms.remove(&info.base_id);
                            data.selection.selected_diff_atoms.insert(diff_id);
                            data.set_hybridization_override_recorded(diff_id, value);
                        }
                        // Set hybridization on already-diff atoms
                        for &diff_id in &data.selection.selected_diff_atoms.clone() {
                            data.set_hybridization_override_recorded(diff_id, value);
                        }
                    }
                },
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Returns the common hybridization override of the currently selected atoms.
/// Returns -1 if no atom_edit is active or no atoms are selected.
/// Returns 0 (Auto), 1 (Sp3), 2 (Sp2), or 3 (Sp1) if all selected atoms agree.
/// Returns -2 if selected atoms have differing overrides (mixed state).
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_get_selected_hybridization() -> i8 {
    use crate::api::api_common::with_cad_instance_or;
    use crate::crystolecule::atomic_structure::atom::HYBRIDIZATION_AUTO;

    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let data =
                    match atom_edit::get_active_atom_edit_data(&cad_instance.structure_designer) {
                        Some(d) => d,
                        None => return -1,
                    };

                let diff_selected = &data.selection.selected_diff_atoms;
                // Base atoms without a diff entry have Auto hybridization (not overridden)
                let base_selected = &data.selection.selected_base_atoms;

                if base_selected.is_empty() && diff_selected.is_empty() {
                    return -1;
                }

                let mut common: Option<u8> = None;

                // Base atoms: Auto hybridization (overrides require promotion to diff)
                for &_base_id in base_selected {
                    match common {
                        None => common = Some(HYBRIDIZATION_AUTO),
                        Some(c) if c != HYBRIDIZATION_AUTO => return -2,
                        _ => {}
                    }
                }

                for &diff_id in diff_selected {
                    let val = data
                        .diff
                        .get_atom(diff_id)
                        .map_or(HYBRIDIZATION_AUTO, |a| a.hybridization_override());
                    match common {
                        None => common = Some(val),
                        Some(c) if c != val => return -2,
                        _ => {}
                    }
                }

                common.unwrap_or(HYBRIDIZATION_AUTO) as i8
            },
            -1,
        )
    }
}

/// Returns the common *inferred* hybridization of the currently selected atoms.
///
/// This always returns the bond-order-based inference (ignoring overrides), so the
/// UI can show "Auto (sp3)" etc. when the override is Auto.
///
/// Returns -1 if no atom_edit is active, no atoms are selected, or the result
/// structure is unavailable.
/// Returns 1 (Sp3), 2 (Sp2), or 3 (Sp1) if all selected atoms agree.
/// Returns -2 if selected atoms have differing inferred hybridizations (mixed).
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_get_selected_inferred_hybridization() -> i8 {
    use crate::api::api_common::with_cad_instance_or;
    use crate::crystolecule::guided_placement::{Hybridization, detect_hybridization};
    use crate::structure_designer::nodes::atom_edit::atom_edit::{
        AtomEditEvalCache, SelectionProvenance,
    };

    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let data =
                    match atom_edit::get_active_atom_edit_data(&cad_instance.structure_designer) {
                        Some(d) => d,
                        None => return -1,
                    };

                if data.selection.selected_base_atoms.is_empty()
                    && data.selection.selected_diff_atoms.is_empty()
                {
                    return -1;
                }

                let result_structure = match cad_instance
                    .structure_designer
                    .get_atomic_structure_from_selected_node()
                {
                    Some(s) => s,
                    None => return -1,
                };

                let eval_cache = cad_instance
                    .structure_designer
                    .get_selected_node_eval_cache()
                    .and_then(|cache| cache.downcast_ref::<AtomEditEvalCache>());

                let hyb_to_u8 = |h: Hybridization| -> u8 {
                    match h {
                        Hybridization::Sp3 => 1,
                        Hybridization::Sp2 => 2,
                        Hybridization::Sp1 => 3,
                    }
                };

                let mut common: Option<u8> = None;

                // In diff view, selected atom IDs are already result-space IDs.
                if cad_instance
                    .structure_designer
                    .is_selected_node_in_diff_view()
                {
                    for &diff_id in &data.selection.selected_diff_atoms {
                        let val = hyb_to_u8(detect_hybridization(result_structure, diff_id, None));
                        match common {
                            None => common = Some(val),
                            Some(c) if c != val => return -2,
                            _ => {}
                        }
                    }
                } else {
                    // Result view: resolve through provenance.
                    let cache = match eval_cache {
                        Some(c) => c,
                        None => return -1,
                    };
                    for &(prov, id) in &data.selection.selection_order {
                        let result_id = match prov {
                            SelectionProvenance::Base => {
                                cache.provenance.base_to_result.get(&id).copied()
                            }
                            SelectionProvenance::Diff => {
                                cache.provenance.diff_to_result.get(&id).copied()
                            }
                        };
                        if let Some(result_id) = result_id {
                            let val =
                                hyb_to_u8(detect_hybridization(result_structure, result_id, None));
                            match common {
                                None => common = Some(val),
                                Some(c) if c != val => return -2,
                                _ => {}
                            }
                        }
                    }
                }

                common.unwrap_or(0) as i8
            },
            -1,
        )
    }
}
