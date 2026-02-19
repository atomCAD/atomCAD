use super::atom_edit_data::*;
use super::operations::drag_selected_by_delta;
use super::selection::*;
use super::types::*;
use crate::api::common_api_types::SelectModifier;
use crate::api::structure_designer::structure_designer_api_types::{
    PointerDownResult, PointerDownResultKind, PointerMoveResult, PointerMoveResultKind,
    PointerUpResult,
};
use crate::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization;
use crate::crystolecule::atomic_structure::HitTestResult;
use crate::crystolecule::atomic_structure_diff::AtomSource;
use crate::display::atomic_tessellator::{BAS_STICK_RADIUS, get_displayed_atom_radius};
use crate::display::preferences as display_prefs;
use crate::structure_designer::structure_designer::StructureDesigner;
use glam::f64::{DMat4, DVec2, DVec3};

// =============================================================================
// Private helpers
// =============================================================================

/// Set the interaction state on the active Default tool. Uses transient accessor
/// (no mark_node_data_changed) since interaction_state is not serialized.
fn set_interaction_state(
    structure_designer: &mut StructureDesigner,
    state: DefaultToolInteractionState,
) {
    if let Some(data) = get_atom_edit_data_mut_transient(structure_designer) {
        if let AtomEditTool::Default(ref mut default_state) = data.active_tool {
            default_state.interaction_state = state;
        }
    }
}

/// Intersect a ray with a plane. Returns the intersection point, or None if
/// the ray is parallel to the plane.
///
/// Plane defined by: dot(point - plane_point, plane_normal) = 0
fn ray_plane_intersect(
    ray_origin: &DVec3,
    ray_direction: &DVec3,
    plane_normal: &DVec3,
    plane_point: &DVec3,
) -> Option<DVec3> {
    let denom = ray_direction.dot(*plane_normal);
    if denom.abs() < 1e-10 {
        return None; // Ray parallel to plane
    }
    let t = (*plane_point - *ray_origin).dot(*plane_normal) / denom;
    Some(*ray_origin + *ray_direction * t)
}

/// Compute an LTWH rectangle from two corner points (handles any drag direction).
fn screen_rect_from_corners(a: DVec2, b: DVec2) -> (f64, f64, f64, f64) {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let w = (a.x - b.x).abs();
    let h = (a.y - b.y).abs();
    (x, y, w, h)
}

// =============================================================================
// Public event handlers
// =============================================================================

/// Reset the Default tool interaction state to Idle. Called on pointer cancel or when
/// switching away from the Default tool mid-interaction.
/// If a drag was in progress, the atoms remain at their current positions (already moved
/// incrementally) and the node is marked as data-changed so the next refresh commits them.
pub fn default_tool_reset_interaction(structure_designer: &mut StructureDesigner) {
    let was_dragging = {
        let data = match get_active_atom_edit_data(structure_designer) {
            Some(d) => d,
            None => return,
        };
        match &data.active_tool {
            AtomEditTool::Default(state) => {
                matches!(
                    state.interaction_state,
                    DefaultToolInteractionState::ScreenPlaneDragging { .. }
                )
            }
            _ => false,
        }
    };

    set_interaction_state(structure_designer, DefaultToolInteractionState::Idle);

    // If we were mid-drag, mark node data changed so the next refresh commits the positions
    if was_dragging {
        if let Some(node_id) = structure_designer.get_selected_node_id_with_type("atom_edit") {
            structure_designer.mark_node_data_changed(node_id);
        }
    }
}

/// Handle mouse-down for the Default tool. Performs hit test and enters pending state.
///
/// Hit test priority: gadget → atom → bond → empty (per design Section 6).
pub fn default_tool_pointer_down(
    structure_designer: &mut StructureDesigner,
    screen_pos: DVec2,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
    select_modifier: SelectModifier,
) -> PointerDownResult {
    // Test gadget FIRST — gadget handles have priority over atoms/bonds.
    if let Some(handle_index) = structure_designer.gadget_hit_test(*ray_origin, *ray_direction) {
        set_interaction_state(structure_designer, DefaultToolInteractionState::Idle);
        return PointerDownResult {
            kind: PointerDownResultKind::GadgetHit,
            gadget_handle_index: handle_index,
        };
    }

    // Phase 1: Hit test and gather info (immutable borrows)
    let (hit_result, is_diff_view, was_selected) = {
        let result_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(s) => s,
            None => {
                set_interaction_state(structure_designer, DefaultToolInteractionState::Idle);
                return PointerDownResult {
                    kind: PointerDownResultKind::StartedOnEmpty,
                    gadget_handle_index: -1,
                };
            }
        };

        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => {
                set_interaction_state(structure_designer, DefaultToolInteractionState::Idle);
                return PointerDownResult {
                    kind: PointerDownResultKind::StartedOnEmpty,
                    gadget_handle_index: -1,
                };
            }
        };

        let is_diff = atom_edit_data.output_diff;

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

        let hit = result_structure.hit_test(
            ray_origin,
            ray_direction,
            visualization,
            |atom| get_displayed_atom_radius(atom, &display_visualization),
            BAS_STICK_RADIUS,
        );

        // Check was_selected for atom hits
        let was_sel = match &hit {
            HitTestResult::Atom(atom_id, _) => {
                if is_diff {
                    atom_edit_data
                        .selection
                        .selected_diff_atoms
                        .contains(atom_id)
                } else {
                    // Check via provenance
                    let eval_cache = structure_designer.get_selected_node_eval_cache();
                    if let Some(cache) = eval_cache {
                        if let Some(cache) = cache.downcast_ref::<AtomEditEvalCache>() {
                            match cache.provenance.sources.get(atom_id) {
                                Some(AtomSource::BasePassthrough(base_id)) => atom_edit_data
                                    .selection
                                    .selected_base_atoms
                                    .contains(base_id),
                                Some(AtomSource::DiffMatchedBase { diff_id, .. }) => atom_edit_data
                                    .selection
                                    .selected_diff_atoms
                                    .contains(diff_id),
                                Some(AtomSource::DiffAdded(diff_id)) => atom_edit_data
                                    .selection
                                    .selected_diff_atoms
                                    .contains(diff_id),
                                None => false,
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            }
            _ => false,
        };

        (hit, is_diff, was_sel)
    };

    // Phase 2: Set interaction state (transient — no mark_node_data_changed)
    match hit_result {
        HitTestResult::Atom(atom_id, _) => {
            set_interaction_state(
                structure_designer,
                DefaultToolInteractionState::PendingAtom {
                    hit_atom_id: atom_id,
                    is_diff_view,
                    was_selected,
                    mouse_down_screen: screen_pos,
                    select_modifier,
                },
            );
            PointerDownResult {
                kind: PointerDownResultKind::StartedOnAtom,
                gadget_handle_index: -1,
            }
        }
        HitTestResult::Bond(bond_ref, _) => {
            set_interaction_state(
                structure_designer,
                DefaultToolInteractionState::PendingBond {
                    bond_reference: bond_ref,
                    mouse_down_screen: screen_pos,
                },
            );
            PointerDownResult {
                kind: PointerDownResultKind::StartedOnBond,
                gadget_handle_index: -1,
            }
        }
        HitTestResult::None => {
            set_interaction_state(
                structure_designer,
                DefaultToolInteractionState::PendingMarquee {
                    mouse_down_screen: screen_pos,
                },
            );
            PointerDownResult {
                kind: PointerDownResultKind::StartedOnEmpty,
                gadget_handle_index: -1,
            }
        }
    }
}

/// Handle mouse-move for the Default tool. Checks drag threshold and updates
/// active drag state (marquee rectangle or screen-plane atom drag).
///
/// `camera_forward` is the camera's forward direction (eye → target, normalized).
/// It's used as the constraint plane normal for screen-plane dragging.
pub fn default_tool_pointer_move(
    structure_designer: &mut StructureDesigner,
    screen_pos: DVec2,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
    _viewport_width: f64,
    _viewport_height: f64,
    camera_forward: &DVec3,
) -> PointerMoveResult {
    let no_op = PointerMoveResult {
        kind: PointerMoveResultKind::StillPending,
        marquee_rect_x: 0.0,
        marquee_rect_y: 0.0,
        marquee_rect_w: 0.0,
        marquee_rect_h: 0.0,
    };

    let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
        Some(data) => data,
        None => return no_op,
    };

    // Determine what transition is needed based on current state
    enum MoveAction {
        None,
        StartDrag {
            hit_atom_id: u32,
            is_diff_view: bool,
            was_selected: bool,
            select_modifier: SelectModifier,
        },
        ContinueDrag {
            plane_normal: DVec3,
            plane_point: DVec3,
            start_world_pos: DVec3,
            last_world_pos: DVec3,
        },
        ThresholdExceededOnMarquee {
            start_screen: DVec2,
        },
        UpdateMarquee {
            start_screen: DVec2,
        },
    }

    let action = match &atom_edit_data.active_tool {
        AtomEditTool::Default(state) => match &state.interaction_state {
            DefaultToolInteractionState::PendingAtom {
                mouse_down_screen,
                hit_atom_id,
                is_diff_view,
                was_selected,
                select_modifier,
            } => {
                if screen_pos.distance(*mouse_down_screen) > DRAG_THRESHOLD {
                    MoveAction::StartDrag {
                        hit_atom_id: *hit_atom_id,
                        is_diff_view: *is_diff_view,
                        was_selected: *was_selected,
                        select_modifier: select_modifier.clone(),
                    }
                } else {
                    MoveAction::None
                }
            }
            DefaultToolInteractionState::PendingMarquee {
                mouse_down_screen, ..
            } => {
                if screen_pos.distance(*mouse_down_screen) > DRAG_THRESHOLD {
                    MoveAction::ThresholdExceededOnMarquee {
                        start_screen: *mouse_down_screen,
                    }
                } else {
                    MoveAction::None
                }
            }
            DefaultToolInteractionState::MarqueeActive { start_screen, .. } => {
                MoveAction::UpdateMarquee {
                    start_screen: *start_screen,
                }
            }
            DefaultToolInteractionState::ScreenPlaneDragging {
                plane_normal,
                plane_point,
                start_world_pos,
                last_world_pos,
            } => MoveAction::ContinueDrag {
                plane_normal: *plane_normal,
                plane_point: *plane_point,
                start_world_pos: *start_world_pos,
                last_world_pos: *last_world_pos,
            },
            // Bonds are not draggable; threshold doesn't change behavior
            DefaultToolInteractionState::PendingBond { .. } | DefaultToolInteractionState::Idle => {
                MoveAction::None
            }
        },
        _ => MoveAction::None,
    };

    match action {
        MoveAction::None => no_op,
        MoveAction::StartDrag {
            hit_atom_id,
            is_diff_view,
            was_selected,
            select_modifier,
        } => {
            // If the atom was not selected, apply tentative selection first
            if !was_selected {
                if is_diff_view {
                    select_diff_atom_directly(structure_designer, hit_atom_id, select_modifier);
                } else {
                    select_result_atom(structure_designer, hit_atom_id, select_modifier);
                }
            }

            // Compute the constraint plane: camera-parallel, through selection centroid
            let plane_normal = *camera_forward;
            let plane_point = match get_active_atom_edit_data(structure_designer) {
                Some(data) => match &data.selection.selection_transform {
                    Some(t) => t.translation,
                    None => return no_op,
                },
                None => return no_op,
            };

            // Intersect the current ray with the constraint plane to get start_world_pos
            let start_world_pos =
                match ray_plane_intersect(ray_origin, ray_direction, &plane_normal, &plane_point) {
                    Some(pos) => pos,
                    None => return no_op, // Ray parallel to plane
                };

            set_interaction_state(
                structure_designer,
                DefaultToolInteractionState::ScreenPlaneDragging {
                    plane_normal,
                    plane_point,
                    start_world_pos,
                    last_world_pos: start_world_pos,
                },
            );

            PointerMoveResult {
                kind: PointerMoveResultKind::Dragging,
                marquee_rect_x: 0.0,
                marquee_rect_y: 0.0,
                marquee_rect_w: 0.0,
                marquee_rect_h: 0.0,
            }
        }
        MoveAction::ContinueDrag {
            plane_normal,
            plane_point,
            start_world_pos,
            last_world_pos,
        } => {
            // Intersect the current ray with the constraint plane
            let current_world_pos =
                match ray_plane_intersect(ray_origin, ray_direction, &plane_normal, &plane_point) {
                    Some(pos) => pos,
                    None => return no_op, // Ray parallel to plane
                };

            // Compute incremental delta from last frame
            let delta = current_world_pos - last_world_pos;

            if delta.length_squared() > 0.0 {
                // Apply delta to all selected atoms
                drag_selected_by_delta(structure_designer, delta);

                // Update the last_world_pos in the interaction state
                set_interaction_state(
                    structure_designer,
                    DefaultToolInteractionState::ScreenPlaneDragging {
                        plane_normal,
                        plane_point,
                        start_world_pos,
                        last_world_pos: current_world_pos,
                    },
                );
            }

            PointerMoveResult {
                kind: PointerMoveResultKind::Dragging,
                marquee_rect_x: 0.0,
                marquee_rect_y: 0.0,
                marquee_rect_w: 0.0,
                marquee_rect_h: 0.0,
            }
        }
        MoveAction::ThresholdExceededOnMarquee { start_screen } => {
            set_interaction_state(
                structure_designer,
                DefaultToolInteractionState::MarqueeActive {
                    start_screen,
                    current_screen: screen_pos,
                },
            );
            let rect = screen_rect_from_corners(start_screen, screen_pos);
            PointerMoveResult {
                kind: PointerMoveResultKind::MarqueeUpdated,
                marquee_rect_x: rect.0,
                marquee_rect_y: rect.1,
                marquee_rect_w: rect.2,
                marquee_rect_h: rect.3,
            }
        }
        MoveAction::UpdateMarquee { start_screen } => {
            set_interaction_state(
                structure_designer,
                DefaultToolInteractionState::MarqueeActive {
                    start_screen,
                    current_screen: screen_pos,
                },
            );
            let rect = screen_rect_from_corners(start_screen, screen_pos);
            PointerMoveResult {
                kind: PointerMoveResultKind::MarqueeUpdated,
                marquee_rect_x: rect.0,
                marquee_rect_y: rect.1,
                marquee_rect_w: rect.2,
                marquee_rect_h: rect.3,
            }
        }
    }
}

/// Handle mouse-up for the Default tool. Commits click-select, marquee selection,
/// or clears selection.
///
/// The `view_proj` matrix is needed for marquee selection (projecting atoms to screen
/// coordinates). It comes from `Camera::build_view_projection_matrix()` and is passed
/// through from the API layer which has access to the full `CadInstance`.
#[allow(clippy::too_many_arguments)]
pub fn default_tool_pointer_up(
    structure_designer: &mut StructureDesigner,
    _screen_pos: DVec2,
    _ray_origin: &DVec3,
    _ray_direction: &DVec3,
    select_modifier: SelectModifier,
    viewport_width: f64,
    viewport_height: f64,
    view_proj: &DMat4,
) -> PointerUpResult {
    // Read the current interaction state (owned copy of the data we need)
    let pending_info = {
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return PointerUpResult::NothingHappened,
        };
        match &atom_edit_data.active_tool {
            AtomEditTool::Default(state) => match &state.interaction_state {
                DefaultToolInteractionState::PendingAtom {
                    hit_atom_id,
                    is_diff_view,
                    was_selected,
                    ..
                } => Some(PendingClickInfo::Atom {
                    atom_id: *hit_atom_id,
                    is_diff_view: *is_diff_view,
                    was_selected: *was_selected,
                }),
                DefaultToolInteractionState::PendingBond { bond_reference, .. } => {
                    Some(PendingClickInfo::Bond {
                        bond_reference: bond_reference.clone(),
                    })
                }
                DefaultToolInteractionState::PendingMarquee { .. } => Some(PendingClickInfo::Empty),
                DefaultToolInteractionState::MarqueeActive {
                    start_screen,
                    current_screen,
                } => Some(PendingClickInfo::Marquee {
                    start_screen: *start_screen,
                    end_screen: *current_screen,
                }),
                DefaultToolInteractionState::ScreenPlaneDragging { .. } => {
                    Some(PendingClickInfo::DragCompleted)
                }
                DefaultToolInteractionState::Idle => None,
            },
            _ => None,
        }
    };

    // Reset to Idle
    set_interaction_state(structure_designer, DefaultToolInteractionState::Idle);

    match pending_info {
        Some(PendingClickInfo::Atom {
            atom_id,
            is_diff_view,
            was_selected,
        }) => {
            // Click on an already-selected atom with Replace modifier: keep selection unchanged
            if was_selected && matches!(select_modifier, SelectModifier::Replace) {
                return PointerUpResult::SelectionChanged;
            }
            // Click on an already-selected atom with Expand modifier: already selected, no-op
            if was_selected && matches!(select_modifier, SelectModifier::Expand) {
                return PointerUpResult::SelectionChanged;
            }
            // All other cases: delegate to existing selection functions
            if is_diff_view {
                select_diff_atom_directly(structure_designer, atom_id, select_modifier);
            } else {
                select_result_atom(structure_designer, atom_id, select_modifier);
            }
            PointerUpResult::SelectionChanged
        }
        Some(PendingClickInfo::Bond { bond_reference }) => {
            select_result_bond(structure_designer, &bond_reference, select_modifier);
            PointerUpResult::SelectionChanged
        }
        Some(PendingClickInfo::Marquee {
            start_screen,
            end_screen,
        }) => {
            // Compute the screen-space AABB from the two marquee corners
            let screen_min = DVec2::new(
                start_screen.x.min(end_screen.x),
                start_screen.y.min(end_screen.y),
            );
            let screen_max = DVec2::new(
                start_screen.x.max(end_screen.x),
                start_screen.y.max(end_screen.y),
            );
            let changed = select_atoms_in_screen_rect(
                structure_designer,
                view_proj,
                screen_min,
                screen_max,
                viewport_width,
                viewport_height,
                &select_modifier,
            );
            if changed {
                PointerUpResult::MarqueeCommitted
            } else {
                // Empty marquee with Replace: selection was cleared in select_atoms_in_screen_rect
                // Empty marquee with Expand/Toggle: nothing happened
                if matches!(select_modifier, SelectModifier::Replace) {
                    PointerUpResult::MarqueeCommitted
                } else {
                    PointerUpResult::NothingHappened
                }
            }
        }
        Some(PendingClickInfo::DragCompleted) => {
            // Screen-plane drag finished. Atoms are already at their new positions
            // (updated incrementally during drag). The state has been reset to Idle.
            // mark_node_data_changed was already called by drag_selected_by_delta,
            // and refresh_structure_designer_auto will be called by the API layer.
            // The full refresh on commit re-evaluates downstream nodes.
            PointerUpResult::DragCommitted
        }
        Some(PendingClickInfo::Empty) => {
            // Click on empty space: clear selection (Replace) or no-op (Expand/Toggle)
            if matches!(select_modifier, SelectModifier::Replace) {
                let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
                    Some(data) => data,
                    None => return PointerUpResult::NothingHappened,
                };
                if atom_edit_data.selection.is_empty() {
                    return PointerUpResult::NothingHappened;
                }
                atom_edit_data.selection.clear();
                PointerUpResult::SelectionChanged
            } else {
                PointerUpResult::NothingHappened
            }
        }
        None => PointerUpResult::NothingHappened,
    }
}
