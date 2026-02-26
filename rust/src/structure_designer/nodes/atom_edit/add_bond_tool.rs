//! AddBond tool: drag-to-bond interaction with configurable bond order.
//!
//! State machine: Idle → Pending (pointer down on atom) → Dragging (drag
//! threshold exceeded) → bond creation on pointer_up over target, or cancel.
//!
//! pointer_move performs a lightweight ray-cast only (no evaluation, no
//! tessellation) and returns `AddBondMoveResult` for Flutter's 2D rubber-band
//! overlay. Bond is created with the order stored in `AddBondToolState`.

use super::atom_edit_data::*;
use super::types::*;
use crate::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization;
use crate::crystolecule::atomic_structure::HitTestResult;
use crate::crystolecule::atomic_structure_diff::AtomSource;
use crate::display::atomic_tessellator::{BAS_STICK_RADIUS, get_displayed_atom_radius};
use crate::display::preferences as display_prefs;
use crate::structure_designer::structure_designer::StructureDesigner;
use glam::f64::{DVec2, DVec3};

/// Result of `add_bond_pointer_move`. Contains all info Flutter needs to draw
/// the rubber-band preview line as a 2D overlay.
#[derive(Debug, Clone)]
pub struct AddBondMoveResult {
    /// True if we are in the Dragging state (rubber-band should be drawn).
    pub is_dragging: bool,
    /// World position of the source atom (start of the rubber-band).
    pub source_atom_pos: Option<DVec3>,
    /// World position of the preview end point (cursor ray–plane intersection
    /// or snapped target atom position).
    pub preview_end_pos: Option<DVec3>,
    /// True if the cursor is hovering over a valid snap target atom.
    pub snapped_to_atom: bool,
    /// Current bond order setting, for visual styling of the preview line.
    pub bond_order: u8,
}

impl Default for AddBondMoveResult {
    fn default() -> Self {
        Self {
            is_dragging: false,
            source_atom_pos: None,
            preview_end_pos: None,
            snapped_to_atom: false,
            bond_order: 1,
        }
    }
}

// =============================================================================
// Private helpers
// =============================================================================

/// Set the AddBond interaction state. Uses transient accessor (no
/// mark_node_data_changed) since interaction_state is not serialized.
fn set_add_bond_interaction_state(
    structure_designer: &mut StructureDesigner,
    new_state: AddBondInteractionState,
) {
    if let Some(data) = get_atom_edit_data_mut_transient(structure_designer) {
        if let AtomEditTool::AddBond(ref mut bond_state) = data.active_tool {
            bond_state.interaction_state = new_state;
        }
    }
}

/// Perform a hit test for atoms only (bonds and empty space are ignored).
/// Returns `(result_atom_id, atom_position)` if an atom was hit.
fn hit_test_atom_only(
    structure_designer: &StructureDesigner,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
) -> Option<(u32, DVec3)> {
    let result_structure = structure_designer.get_atomic_structure_from_selected_node()?;

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

    match result_structure.hit_test(
        ray_origin,
        ray_direction,
        visualization,
        |atom| get_displayed_atom_radius(atom, &display_visualization),
        BAS_STICK_RADIUS,
    ) {
        HitTestResult::Atom(id, _) => {
            let pos = result_structure.get_atom(id)?.position;
            Some((id, pos))
        }
        _ => None,
    }
}

/// Resolve a result atom ID to a diff atom ID, promoting base-passthrough
/// atoms to diff identity entries as needed. Returns `None` on failure.
///
/// `atom_info` is `(atomic_number, position)` for the hit atom.
fn resolve_to_diff_id(
    structure_designer: &mut StructureDesigner,
    result_atom_id: u32,
    atom_info: (i16, DVec3),
    is_diff_view: bool,
) -> Option<u32> {
    if is_diff_view {
        // In diff view, the hit ID is already a diff atom ID.
        return Some(result_atom_id);
    }

    // Result view: look up provenance.
    let atom_source = {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let eval_cache = eval_cache.downcast_ref::<AtomEditEvalCache>()?;
        eval_cache.provenance.sources.get(&result_atom_id)?.clone()
    };

    let atom_edit_data = get_selected_atom_edit_data_mut(structure_designer)?;
    match &atom_source {
        AtomSource::BasePassthrough(_) => {
            Some(atom_edit_data.diff.add_atom(atom_info.0, atom_info.1))
        }
        AtomSource::DiffMatchedBase { diff_id, .. } | AtomSource::DiffAdded(diff_id) => {
            Some(*diff_id)
        }
    }
}

// =============================================================================
// Public event handlers
// =============================================================================

/// Reset the AddBond tool interaction state to Idle. Called on pointer cancel
/// or when switching away from the AddBond tool mid-interaction.
pub fn add_bond_reset_interaction(structure_designer: &mut StructureDesigner) {
    set_add_bond_interaction_state(structure_designer, AddBondInteractionState::Idle);
}

/// Set the bond order for the AddBond tool.
pub fn set_add_bond_order(structure_designer: &mut StructureDesigner, order: u8) {
    if order == 0 || order > 7 {
        return; // Reject invalid orders
    }
    if let Some(data) = get_atom_edit_data_mut_transient(structure_designer) {
        if let AtomEditTool::AddBond(ref mut state) = data.active_tool {
            state.bond_order = order;
        }
    }
}

/// Handle pointer-down in the AddBond tool. Performs a hit test for atoms.
/// Returns `true` if an atom was hit (entered Pending state).
///
/// Triggers one refresh if an atom is hit (to show source atom highlight).
pub fn add_bond_pointer_down(
    structure_designer: &mut StructureDesigner,
    screen_pos: DVec2,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
) -> bool {
    // Phase 1: Hit test (immutable borrows)
    let is_diff_view = match get_active_atom_edit_data(structure_designer) {
        Some(data) => data.output_diff,
        None => return false,
    };

    let hit = hit_test_atom_only(structure_designer, ray_origin, ray_direction);

    let (result_atom_id, _atom_pos) = match hit {
        Some(pair) => pair,
        None => {
            // Click on empty space or bond — no-op in AddBond tool.
            set_add_bond_interaction_state(structure_designer, AddBondInteractionState::Idle);
            return false;
        }
    };

    // Phase 2: Resolve to diff atom ID for provenance handling.
    // We need the atom's info for potential identity entry promotion.
    let atom_info = {
        let result_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(s) => s,
            None => return false,
        };
        match result_structure.get_atom(result_atom_id) {
            Some(a) => (a.atomic_number, a.position),
            None => return false,
        }
    };

    // For the Pending state, we store the hit atom's result-space ID and resolve
    // to diff ID later (on transition to Dragging or on pointer_up), to avoid
    // promoting base atoms that the user never actually drags from.
    // However, we need to know the diff atom ID for the source highlight.
    // We resolve eagerly — matching the old draw_bond_by_ray pattern.
    let diff_atom_id =
        match resolve_to_diff_id(structure_designer, result_atom_id, atom_info, is_diff_view) {
            Some(id) => id,
            None => return false,
        };

    set_add_bond_interaction_state(
        structure_designer,
        AddBondInteractionState::Pending {
            hit_atom_id: diff_atom_id,
            is_diff_view,
            mouse_down_screen: screen_pos,
        },
    );

    true
}

/// Handle pointer-move in the AddBond tool. Checks drag threshold and performs
/// ray-cast snap test for target atoms.
///
/// This function does NOT trigger evaluation or tessellation — it only performs
/// a lightweight ray-cast hit test and returns preview data for Flutter's 2D
/// overlay rendering.
pub fn add_bond_pointer_move(
    structure_designer: &mut StructureDesigner,
    screen_pos: DVec2,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
) -> AddBondMoveResult {
    let no_op = AddBondMoveResult::default();

    // Read current state
    let (current_state_kind, bond_order) = {
        let data = match get_active_atom_edit_data(structure_designer) {
            Some(d) => d,
            None => return no_op,
        };
        let state = match &data.active_tool {
            AtomEditTool::AddBond(s) => s,
            _ => return no_op,
        };
        let kind = match &state.interaction_state {
            AddBondInteractionState::Idle => return no_op,
            AddBondInteractionState::Pending {
                hit_atom_id,
                mouse_down_screen,
                ..
            } => PendingOrDragging::Pending {
                hit_atom_id: *hit_atom_id,
                mouse_down_screen: *mouse_down_screen,
            },
            AddBondInteractionState::Dragging { source_atom_id, .. } => {
                PendingOrDragging::Dragging {
                    source_atom_id: *source_atom_id,
                }
            }
        };
        (kind, state.bond_order)
    };

    match current_state_kind {
        PendingOrDragging::Pending {
            hit_atom_id,
            mouse_down_screen,
        } => {
            // Check drag threshold
            if screen_pos.distance(mouse_down_screen) <= DRAG_THRESHOLD {
                return no_op; // Still pending, below threshold
            }

            // Threshold exceeded → transition to Dragging
            let snap = hit_test_atom_only(structure_designer, ray_origin, ray_direction);
            let preview_target = snap
                .filter(|(id, _)| *id != hit_atom_id) // Don't snap to self
                .map(|(id, _)| id);

            set_add_bond_interaction_state(
                structure_designer,
                AddBondInteractionState::Dragging {
                    source_atom_id: hit_atom_id,
                    preview_target,
                },
            );

            // Get source atom position for the rubber-band start point
            let source_pos = get_atom_world_position(structure_designer, hit_atom_id);
            let end_pos = snap
                .filter(|(id, _)| *id != hit_atom_id)
                .map(|(_, pos)| pos)
                .or_else(|| ray_default_end_pos(ray_origin, ray_direction, &source_pos));

            AddBondMoveResult {
                is_dragging: true,
                source_atom_pos: source_pos,
                preview_end_pos: end_pos,
                snapped_to_atom: preview_target.is_some(),
                bond_order,
            }
        }
        PendingOrDragging::Dragging { source_atom_id } => {
            // Already dragging — update snap target
            let snap = hit_test_atom_only(structure_designer, ray_origin, ray_direction);
            let new_target = snap
                .filter(|(id, _)| *id != source_atom_id) // Don't snap to self
                .map(|(id, _)| id);

            set_add_bond_interaction_state(
                structure_designer,
                AddBondInteractionState::Dragging {
                    source_atom_id,
                    preview_target: new_target,
                },
            );

            let source_pos = get_atom_world_position(structure_designer, source_atom_id);
            let end_pos = snap
                .filter(|(id, _)| *id != source_atom_id)
                .map(|(_, pos)| pos)
                .or_else(|| ray_default_end_pos(ray_origin, ray_direction, &source_pos));

            AddBondMoveResult {
                is_dragging: true,
                source_atom_pos: source_pos,
                preview_end_pos: end_pos,
                snapped_to_atom: new_target.is_some(),
                bond_order,
            }
        }
    }
}

/// Handle pointer-up in the AddBond tool. Creates a bond if released on a
/// valid target atom, otherwise cancels.
///
/// Returns `true` if a bond was created.
pub fn add_bond_pointer_up(
    structure_designer: &mut StructureDesigner,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
) -> bool {
    // Read current state
    let (source_atom_id, bond_order, is_diff_view) = {
        let data = match get_active_atom_edit_data(structure_designer) {
            Some(d) => d,
            None => return false,
        };
        let state = match &data.active_tool {
            AtomEditTool::AddBond(s) => s,
            _ => return false,
        };
        let is_diff = data.output_diff;
        match &state.interaction_state {
            AddBondInteractionState::Dragging { source_atom_id, .. } => {
                (*source_atom_id, state.bond_order, is_diff)
            }
            AddBondInteractionState::Pending { .. } => {
                // Pointer up without exceeding drag threshold = click.
                // In the new drag-to-bond design, a click does nothing.
                set_add_bond_interaction_state(structure_designer, AddBondInteractionState::Idle);
                return false;
            }
            AddBondInteractionState::Idle => return false,
        }
    };

    // Reset to Idle first — we'll create the bond below
    set_add_bond_interaction_state(structure_designer, AddBondInteractionState::Idle);

    // Hit test for the target atom at the release position
    let hit = hit_test_atom_only(structure_designer, ray_origin, ray_direction);
    let (target_result_id, _target_pos) = match hit {
        Some(pair) if pair.0 != source_atom_id => pair,
        _ => return false, // Released on empty, bond, or same atom → cancel
    };

    // Resolve target to diff ID
    let target_atom_info = {
        let result_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(s) => s,
            None => return false,
        };
        match result_structure.get_atom(target_result_id) {
            Some(a) => (a.atomic_number, a.position),
            None => return false,
        }
    };

    let target_diff_id = match resolve_to_diff_id(
        structure_designer,
        target_result_id,
        target_atom_info,
        is_diff_view,
    ) {
        Some(id) => id,
        None => return false,
    };

    // Create the bond
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return false,
    };
    atom_edit_data.add_bond_in_diff(source_atom_id, target_diff_id, bond_order);

    true
}

// =============================================================================
// Helpers
// =============================================================================

/// Internal enum for extracting Pending/Dragging state without holding borrows.
enum PendingOrDragging {
    Pending {
        hit_atom_id: u32,
        mouse_down_screen: DVec2,
    },
    Dragging {
        source_atom_id: u32,
    },
}

/// Look up the world position of a diff atom by searching the result structure.
/// Returns `None` if the atom cannot be found.
fn get_atom_world_position(
    structure_designer: &StructureDesigner,
    diff_atom_id: u32,
) -> Option<DVec3> {
    let result_structure = structure_designer.get_atomic_structure_from_selected_node()?;

    // First try: the diff_atom_id may be a result atom ID directly (diff view)
    if let Some(atom) = result_structure.get_atom(diff_atom_id) {
        return Some(atom.position);
    }

    // Second try: look up via provenance (result view)
    let eval_cache = structure_designer.get_selected_node_eval_cache()?;
    let eval_cache = eval_cache.downcast_ref::<AtomEditEvalCache>()?;
    let result_id = eval_cache.provenance.diff_to_result.get(&diff_atom_id)?;
    let atom = result_structure.get_atom(*result_id)?;
    Some(atom.position)
}

/// Compute a default end position for the rubber-band line when not snapped to
/// an atom. Projects the ray a fixed distance from the source atom.
fn ray_default_end_pos(
    ray_origin: &DVec3,
    ray_direction: &DVec3,
    source_pos: &Option<DVec3>,
) -> Option<DVec3> {
    let source = (*source_pos)?;
    // Project the source atom position onto the ray to find the closest point,
    // then use that distance as the projection distance for the cursor end.
    let t = (*ray_direction).dot(source - *ray_origin);
    let t = t.max(0.1); // Clamp to avoid negative/zero distances
    Some(*ray_origin + *ray_direction * t)
}
