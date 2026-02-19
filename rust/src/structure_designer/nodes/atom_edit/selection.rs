use super::atom_edit_data::*;
use super::types::*;
use crate::api::common_api_types::SelectModifier;
use crate::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization;
use crate::crystolecule::atomic_structure::HitTestResult;
use crate::crystolecule::atomic_structure_diff::AtomSource;
use crate::display::atomic_tessellator::{BAS_STICK_RADIUS, get_displayed_atom_radius};
use crate::display::preferences as display_prefs;
use crate::structure_designer::structure_designer::StructureDesigner;
use glam::f64::{DMat4, DVec2, DVec3, DVec4};
use std::collections::HashMap;

use crate::crystolecule::atomic_structure::BondReference;

/// Select an atom or bond by ray hit test.
///
/// Returns true if something was hit, false otherwise.
pub fn select_atom_or_bond_by_ray(
    structure_designer: &mut StructureDesigner,
    ray_start: &DVec3,
    ray_dir: &DVec3,
    select_modifier: SelectModifier,
) -> bool {
    // Phase 1: Hit test (immutable borrow)
    let hit_result = {
        let result_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(s) => s,
            None => return false,
        };

        let visualization = &structure_designer
            .preferences
            .atomic_structure_visualization_preferences
            .visualization;
        let display_visualization = match visualization {
            AtomicStructureVisualization::BallAndStick => {
                display_prefs::AtomicStructureVisualization::BallAndStick
            }
            AtomicStructureVisualization::SpaceFilling => {
                display_prefs::AtomicStructureVisualization::SpaceFilling
            }
        };

        result_structure.hit_test(
            ray_start,
            ray_dir,
            visualization,
            |atom| get_displayed_atom_radius(atom, &display_visualization),
            BAS_STICK_RADIUS,
        )
    };

    // In diff view, atom IDs from the hit test are diff-native IDs — no provenance needed
    let is_diff_view = match get_active_atom_edit_data(structure_designer) {
        Some(data) => data.output_diff,
        None => false,
    };

    match hit_result {
        HitTestResult::Atom(atom_id, _distance) => {
            if is_diff_view {
                select_diff_atom_directly(structure_designer, atom_id, select_modifier)
            } else {
                select_result_atom(structure_designer, atom_id, select_modifier)
            }
        }
        HitTestResult::Bond(bond_reference, _distance) => {
            select_result_bond(structure_designer, &bond_reference, select_modifier)
        }
        HitTestResult::None => false,
    }
}

/// Select an atom by its result atom ID, using provenance to categorize it.
pub(super) fn select_result_atom(
    structure_designer: &mut StructureDesigner,
    result_atom_id: u32,
    select_modifier: SelectModifier,
) -> bool {
    // Phase 1: Gather info (immutable borrows)
    let (atom_source, clicked_position, mut position_map) = {
        let eval_cache = match structure_designer.get_selected_node_eval_cache() {
            Some(cache) => cache,
            None => return false,
        };
        let eval_cache = match eval_cache.downcast_ref::<AtomEditEvalCache>() {
            Some(cache) => cache,
            None => return false,
        };
        let result_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(s) => s,
            None => return false,
        };
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return false,
        };

        let atom_source = match eval_cache.provenance.sources.get(&result_atom_id) {
            Some(s) => s.clone(),
            None => return false,
        };
        let clicked_pos = match result_structure.get_atom(result_atom_id) {
            Some(a) => a.position,
            None => return false,
        };

        // Pre-collect positions for currently selected atoms (needed for transform calculation)
        let mut sel_positions: HashMap<(bool, u32), DVec3> = HashMap::new();
        for &base_id in &atom_edit_data.selection.selected_base_atoms {
            if let Some(&res_id) = eval_cache.provenance.base_to_result.get(&base_id) {
                if let Some(atom) = result_structure.get_atom(res_id) {
                    sel_positions.insert((false, base_id), atom.position);
                }
            }
        }
        for &diff_id in &atom_edit_data.selection.selected_diff_atoms {
            if let Some(&res_id) = eval_cache.provenance.diff_to_result.get(&diff_id) {
                if let Some(atom) = result_structure.get_atom(res_id) {
                    sel_positions.insert((true, diff_id), atom.position);
                }
            }
        }

        (atom_source, clicked_pos, sel_positions)
    };

    // Add clicked atom to position map (may not be there if newly selected)
    match &atom_source {
        AtomSource::BasePassthrough(base_id) => {
            position_map.insert((false, *base_id), clicked_position);
        }
        AtomSource::DiffMatchedBase { diff_id, base_id } => {
            position_map.insert((true, *diff_id), clicked_position);
            // Clean up stale base entry if present
            position_map.remove(&(false, *base_id));
        }
        AtomSource::DiffAdded(diff_id) => {
            position_map.insert((true, *diff_id), clicked_position);
        }
    }

    // Phase 2: Mutate selection
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return false,
    };

    // Handle Replace modifier (clear all first)
    if matches!(select_modifier, SelectModifier::Replace) {
        atom_edit_data.selection.clear();
    }

    // Add/toggle in appropriate selection set based on provenance
    match &atom_source {
        AtomSource::BasePassthrough(base_id) => {
            apply_modifier_to_set(
                &mut atom_edit_data.selection.selected_base_atoms,
                *base_id,
                &select_modifier,
            );
        }
        AtomSource::DiffMatchedBase { diff_id, base_id } => {
            // Clean up: remove from base selection if present (atom is now in diff)
            atom_edit_data.selection.selected_base_atoms.remove(base_id);
            apply_modifier_to_set(
                &mut atom_edit_data.selection.selected_diff_atoms,
                *diff_id,
                &select_modifier,
            );
        }
        AtomSource::DiffAdded(diff_id) => {
            apply_modifier_to_set(
                &mut atom_edit_data.selection.selected_diff_atoms,
                *diff_id,
                &select_modifier,
            );
        }
    }

    // Recalculate selection transform from positions
    let positions: Vec<DVec3> = atom_edit_data
        .selection
        .selected_base_atoms
        .iter()
        .filter_map(|&id| position_map.get(&(false, id)).copied())
        .chain(
            atom_edit_data
                .selection
                .selected_diff_atoms
                .iter()
                .filter_map(|&id| position_map.get(&(true, id)).copied()),
        )
        .collect();

    atom_edit_data.selection.selection_transform = calc_transform_from_positions(&positions);

    true
}

/// Select an atom directly in diff view (no provenance needed).
///
/// In diff view, the displayed structure IS the diff, so atom IDs from the hit test
/// are diff atom IDs. All selected atoms go into `selected_diff_atoms`.
pub(super) fn select_diff_atom_directly(
    structure_designer: &mut StructureDesigner,
    diff_atom_id: u32,
    select_modifier: SelectModifier,
) -> bool {
    // Phase 1: Gather positions (immutable borrow)
    let (clicked_position, mut position_map) = {
        let displayed_structure = match structure_designer.get_atomic_structure_from_selected_node()
        {
            Some(s) => s,
            None => return false,
        };
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return false,
        };

        let clicked_pos = match displayed_structure.get_atom(diff_atom_id) {
            Some(a) => a.position,
            None => return false,
        };

        // Collect positions for currently selected diff atoms
        let mut sel_positions: HashMap<u32, DVec3> = HashMap::new();
        for &id in &atom_edit_data.selection.selected_diff_atoms {
            if let Some(atom) = displayed_structure.get_atom(id) {
                sel_positions.insert(id, atom.position);
            }
        }

        (clicked_pos, sel_positions)
    };

    position_map.insert(diff_atom_id, clicked_position);

    // Phase 2: Mutate selection
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return false,
    };

    if matches!(select_modifier, SelectModifier::Replace) {
        atom_edit_data.selection.clear();
    }

    apply_modifier_to_set(
        &mut atom_edit_data.selection.selected_diff_atoms,
        diff_atom_id,
        &select_modifier,
    );

    // Recalculate selection transform from diff atom positions
    let positions: Vec<DVec3> = atom_edit_data
        .selection
        .selected_diff_atoms
        .iter()
        .filter_map(|&id| position_map.get(&id).copied())
        .collect();

    atom_edit_data.selection.selection_transform = calc_transform_from_positions(&positions);

    true
}

/// Select a bond by its reference in result space.
pub(super) fn select_result_bond(
    structure_designer: &mut StructureDesigner,
    bond_reference: &BondReference,
    select_modifier: SelectModifier,
) -> bool {
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return false,
    };

    if matches!(select_modifier, SelectModifier::Replace) {
        atom_edit_data.selection.clear();
    }

    match select_modifier {
        SelectModifier::Replace | SelectModifier::Expand => {
            atom_edit_data
                .selection
                .selected_bonds
                .insert(bond_reference.clone());
        }
        SelectModifier::Toggle => {
            if !atom_edit_data
                .selection
                .selected_bonds
                .remove(bond_reference)
            {
                atom_edit_data
                    .selection
                    .selected_bonds
                    .insert(bond_reference.clone());
            }
        }
    }

    true
}

/// Project a world-space position to screen coordinates using a view-projection matrix.
/// Returns `None` if the point is behind the camera (w <= 0 in clip space).
pub(super) fn project_to_screen(
    world_pos: DVec3,
    view_proj: &DMat4,
    viewport_width: f64,
    viewport_height: f64,
) -> Option<DVec2> {
    let clip = *view_proj * DVec4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = DVec3::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
    // NDC → screen: x in [-1,1] → [0, viewport_width], y in [-1,1] → [0, viewport_height]
    // Note: NDC y points up, screen y points down, so we flip y.
    let screen_x = (ndc.x + 1.0) * 0.5 * viewport_width;
    let screen_y = (1.0 - ndc.y) * 0.5 * viewport_height;
    Some(DVec2::new(screen_x, screen_y))
}

/// Select atoms whose screen-space projections fall inside a marquee rectangle.
///
/// Returns true if the selection changed.
pub(super) fn select_atoms_in_screen_rect(
    structure_designer: &mut StructureDesigner,
    view_proj: &DMat4,
    screen_min: DVec2,
    screen_max: DVec2,
    viewport_width: f64,
    viewport_height: f64,
    select_modifier: &SelectModifier,
) -> bool {
    #[derive(Clone)]
    enum SelectTarget {
        Base(u32),
        Diff(u32),
    }

    // Phase 1: Gather atom projections and provenance (immutable borrows)
    let (atoms_to_select, position_map, is_diff_view) = {
        let result_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(s) => s,
            None => return false,
        };
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return false,
        };
        let is_diff = atom_edit_data.output_diff;

        // Collect result atom IDs whose projections are inside the rectangle
        let mut inside_atom_ids: Vec<u32> = Vec::new();
        let mut pos_map: HashMap<(bool, u32), DVec3> = HashMap::new();

        for (&atom_id, atom) in result_structure.iter_atoms() {
            if let Some(screen_pos) =
                project_to_screen(atom.position, view_proj, viewport_width, viewport_height)
            {
                if screen_pos.x >= screen_min.x
                    && screen_pos.x <= screen_max.x
                    && screen_pos.y >= screen_min.y
                    && screen_pos.y <= screen_max.y
                {
                    inside_atom_ids.push(atom_id);
                }
            }
        }

        // Resolve provenance for each hit atom
        let mut targets: Vec<SelectTarget> = Vec::new();

        if is_diff {
            // Diff view: atom IDs are diff atom IDs directly
            for &atom_id in &inside_atom_ids {
                if let Some(atom) = result_structure.get_atom(atom_id) {
                    pos_map.insert((true, atom_id), atom.position);
                }
                targets.push(SelectTarget::Diff(atom_id));
            }
        } else {
            // Result view: resolve provenance
            let eval_cache = structure_designer.get_selected_node_eval_cache();
            if let Some(cache) = eval_cache {
                if let Some(cache) = cache.downcast_ref::<AtomEditEvalCache>() {
                    for &result_id in &inside_atom_ids {
                        if let Some(source) = cache.provenance.sources.get(&result_id) {
                            if let Some(atom) = result_structure.get_atom(result_id) {
                                match source {
                                    AtomSource::BasePassthrough(base_id) => {
                                        pos_map.insert((false, *base_id), atom.position);
                                        targets.push(SelectTarget::Base(*base_id));
                                    }
                                    AtomSource::DiffMatchedBase { diff_id, .. } => {
                                        pos_map.insert((true, *diff_id), atom.position);
                                        targets.push(SelectTarget::Diff(*diff_id));
                                    }
                                    AtomSource::DiffAdded(diff_id) => {
                                        pos_map.insert((true, *diff_id), atom.position);
                                        targets.push(SelectTarget::Diff(*diff_id));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        (targets, pos_map, is_diff)
    };

    // Phase 2: Mutate selection
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return false,
    };

    let was_empty_before = atom_edit_data.selection.is_empty();

    // Apply modifier: Replace clears first, Expand/Toggle preserve
    if matches!(select_modifier, SelectModifier::Replace) {
        atom_edit_data.selection.clear();
    }

    if is_diff_view {
        // Diff view: all targets are diff atoms
        for target in &atoms_to_select {
            #[allow(irrefutable_let_patterns)]
            if let SelectTarget::Diff(diff_id) = target {
                apply_modifier_to_set(
                    &mut atom_edit_data.selection.selected_diff_atoms,
                    *diff_id,
                    select_modifier,
                );
            }
        }
    } else {
        for target in &atoms_to_select {
            match target {
                SelectTarget::Base(base_id) => {
                    apply_modifier_to_set(
                        &mut atom_edit_data.selection.selected_base_atoms,
                        *base_id,
                        select_modifier,
                    );
                }
                SelectTarget::Diff(diff_id) => {
                    apply_modifier_to_set(
                        &mut atom_edit_data.selection.selected_diff_atoms,
                        *diff_id,
                        select_modifier,
                    );
                }
            }
        }
    }

    // Recalculate selection transform from positions
    let positions: Vec<DVec3> = atom_edit_data
        .selection
        .selected_base_atoms
        .iter()
        .filter_map(|&id| position_map.get(&(false, id)).copied())
        .chain(
            atom_edit_data
                .selection
                .selected_diff_atoms
                .iter()
                .filter_map(|&id| position_map.get(&(true, id)).copied()),
        )
        .collect();

    atom_edit_data.selection.selection_transform = calc_transform_from_positions(&positions);

    // Selection changed if we had atoms to select or if we cleared (Replace with empty rect)
    !atoms_to_select.is_empty() || (was_empty_before != atom_edit_data.selection.is_empty())
}
