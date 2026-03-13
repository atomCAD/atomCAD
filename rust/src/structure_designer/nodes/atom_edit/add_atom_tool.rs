use super::atom_edit_data::{
    get_active_atom_edit_data, get_atom_edit_data_mut_transient, get_selected_atom_edit_data_mut,
};
use super::types::*;
use crate::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization;
use crate::crystolecule::atomic_structure::HitTestResult;
use crate::crystolecule::atomic_structure_diff::AtomSource;
use crate::crystolecule::guided_placement::{
    BondLengthMode, BondMode, GuideDot, GuidedPlacementMode, Hybridization,
    compute_guided_placement, compute_ring_preview_positions, cone_half_angle_for_ring,
    ray_ring_nearest_point, ray_sphere_nearest_point,
};
use crate::display::atomic_tessellator::{BAS_STICK_RADIUS, get_displayed_atom_radius};
use crate::display::preferences as display_prefs;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::util::hit_test_utils;
use glam::f64::DVec3;

/// Hit radius for guide dot spheres (Angstroms).
const GUIDE_DOT_HIT_RADIUS: f64 = 0.3;

/// Add an atom at the ray-plane intersection point.
///
/// The plane passes through the closest atom to the ray (or at a default distance).
pub fn add_atom_by_ray(
    structure_designer: &mut StructureDesigner,
    atomic_number: i16,
    plane_normal: &DVec3,
    ray_start: &DVec3,
    ray_dir: &DVec3,
) {
    // Phase 1: Calculate position (immutable borrow)
    let position = {
        let atomic_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(structure) => structure,
            None => return,
        };

        let closest_atom_position = atomic_structure.find_closest_atom_to_ray(ray_start, ray_dir);
        let default_distance = 5.0;
        let plane_distance = match closest_atom_position {
            Some(atom_pos) => plane_normal.dot(atom_pos),
            None => plane_normal.dot(*ray_start) + default_distance,
        };

        let denominator = plane_normal.dot(*ray_dir);
        if denominator.abs() < 1e-6 {
            return;
        }

        let t = (plane_distance - plane_normal.dot(*ray_start)) / denominator;
        if t < 0.0 {
            return;
        }

        *ray_start + *ray_dir * t
    };

    // Phase 2: Add atom to diff
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    atom_edit_data.add_atom_to_diff(atomic_number, position);
}

/// Result of attempting to start guided placement.
#[derive(Debug)]
pub enum GuidedPlacementStartResult {
    /// No atom was hit by the ray.
    NoAtomHit,
    /// The hit atom is saturated (no remaining bonding slots).
    AtomSaturated {
        /// True when geometric max > covalent max (atom has lone pairs / empty orbitals
        /// that could be used with dative bond mode).
        has_additional_capacity: bool,
        /// True when has_additional_capacity is true but the anchor-new element pair
        /// is incompatible for dative bonding (no valid donor-acceptor relationship).
        /// When true, the "switch to dative mode" message should NOT be shown.
        dative_incompatible: bool,
    },
    /// Guided placement started successfully.
    Started {
        guide_count: usize,
        anchor_atom_id: u32,
    },
}

/// Start guided placement by ray-casting to find an anchor atom.
///
/// Phase 1: Hit test the result structure, compute guided placement (immutable borrows).
/// Phase 2: Promote base atom if needed, store state (mutable borrow).
pub fn start_guided_placement(
    structure_designer: &mut StructureDesigner,
    ray_start: &DVec3,
    ray_dir: &DVec3,
    atomic_number: i16,
    hybridization_override: Option<Hybridization>,
    bond_mode: BondMode,
    bond_length_mode: BondLengthMode,
) -> GuidedPlacementStartResult {
    // Phase 1: Hit test, resolve provenance, and compute guided placement (immutable)
    let is_diff_view = match get_active_atom_edit_data(structure_designer) {
        Some(data) => data.output_diff,
        None => return GuidedPlacementStartResult::NoAtomHit,
    };

    // Gather: atom source, hit atom info (atomic_number, position), guided placement result,
    // and merge targets for each guide dot (FixedDots mode only).
    let (atom_source, hit_atom_info, placement_result, merge_targets) = {
        let result_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(s) => s,
            None => return GuidedPlacementStartResult::NoAtomHit,
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

        let result_atom_id = match result_structure.hit_test(
            ray_start,
            ray_dir,
            visualization,
            |atom| get_displayed_atom_radius(atom, &display_visualization),
            BAS_STICK_RADIUS,
        ) {
            HitTestResult::Atom(id, _) => id,
            _ => return GuidedPlacementStartResult::NoAtomHit,
        };

        // Compute guided placement on the result structure (has all bonds from apply_diff)
        let placement = compute_guided_placement(
            result_structure,
            result_atom_id,
            atomic_number,
            hybridization_override,
            bond_mode,
            bond_length_mode,
        );

        // Compute merge targets for FixedDots guide dots.
        // For each dot, check if an existing atom overlaps within MERGE_TOLERANCE.
        let merge_targets = if let GuidedPlacementMode::FixedDots { guide_dots } = &placement.mode
        {
            let eval_cache_for_merge = if !is_diff_view {
                structure_designer
                    .get_selected_node_eval_cache()
                    .and_then(|c| c.downcast_ref::<AtomEditEvalCache>())
            } else {
                None
            };
            compute_merge_targets(
                result_structure,
                guide_dots,
                result_atom_id,
                is_diff_view,
                eval_cache_for_merge,
            )
        } else {
            Vec::new()
        };

        if is_diff_view {
            let atom = match result_structure.get_atom(result_atom_id) {
                Some(a) => (a.atomic_number, a.position),
                None => return GuidedPlacementStartResult::NoAtomHit,
            };
            (None, (result_atom_id, atom), placement, merge_targets)
        } else {
            let eval_cache = match structure_designer.get_selected_node_eval_cache() {
                Some(cache) => cache,
                None => return GuidedPlacementStartResult::NoAtomHit,
            };
            let eval_cache = match eval_cache.downcast_ref::<AtomEditEvalCache>() {
                Some(cache) => cache,
                None => return GuidedPlacementStartResult::NoAtomHit,
            };

            let source = match eval_cache.provenance.sources.get(&result_atom_id) {
                Some(s) => s.clone(),
                None => return GuidedPlacementStartResult::NoAtomHit,
            };

            let atom = match result_structure.get_atom(result_atom_id) {
                Some(a) => (a.atomic_number, a.position),
                None => return GuidedPlacementStartResult::NoAtomHit,
            };

            (Some(source), (result_atom_id, atom), placement, merge_targets)
        }
    };

    // Phase 2: Resolve to diff atom ID, store state (mutable)
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return GuidedPlacementStartResult::NoAtomHit,
    };

    // Check saturation before promoting
    if placement_result.remaining_slots == 0 {
        if let AtomEditTool::AddAtom(state) = &mut atom_edit_data.active_tool {
            *state = AddAtomToolState::Idle;
        }
        return GuidedPlacementStartResult::AtomSaturated {
            has_additional_capacity: placement_result.has_additional_geometric_capacity,
            dative_incompatible: placement_result.dative_incompatible,
        };
    }

    // Check if we have something to show (guide dots, free sphere, or free ring)
    let is_free_sphere = placement_result.mode.is_free_sphere();
    let is_free_ring = placement_result.mode.is_free_ring();
    let guide_dots_empty = placement_result.guide_dots().is_empty();
    if !is_free_sphere && !is_free_ring && guide_dots_empty {
        // No guide dots computed — stay in Idle
        if let AtomEditTool::AddAtom(state) = &mut atom_edit_data.active_tool {
            *state = AddAtomToolState::Idle;
        }
        return GuidedPlacementStartResult::NoAtomHit;
    }

    // Resolve to diff atom ID (promote base atom if needed)
    let diff_atom_id = if is_diff_view {
        hit_atom_info.0
    } else {
        match &atom_source {
            Some(AtomSource::BasePassthrough(_)) => {
                atom_edit_data.add_atom_recorded(hit_atom_info.1.0, hit_atom_info.1.1)
            }
            Some(AtomSource::DiffMatchedBase { diff_id, .. })
            | Some(AtomSource::DiffAdded(diff_id)) => *diff_id,
            None => return GuidedPlacementStartResult::NoAtomHit,
        }
    };

    // Enter guided placement mode based on the placement result
    let is_dative = placement_result.is_dative_bond;
    let guide_count = match &placement_result.mode {
        GuidedPlacementMode::FixedDots { guide_dots } => {
            let count = guide_dots.len();
            if let AtomEditTool::AddAtom(state) = &mut atom_edit_data.active_tool {
                *state = AddAtomToolState::GuidedPlacement {
                    anchor_atom_id: diff_atom_id,
                    guide_dots: guide_dots.clone(),
                    bond_distance: placement_result.bond_distance,
                    is_dative_bond: is_dative,
                    merge_targets,
                };
            }
            count
        }
        GuidedPlacementMode::FreeSphere { center, radius, .. } => {
            if let AtomEditTool::AddAtom(state) = &mut atom_edit_data.active_tool {
                *state = AddAtomToolState::GuidedFreeSphere {
                    anchor_atom_id: diff_atom_id,
                    center: *center,
                    radius: *radius,
                    preview_position: None,
                    is_dative_bond: is_dative,
                };
            }
            0 // No fixed guide dots; sphere is interactive
        }
        GuidedPlacementMode::FreeRing {
            ring_center,
            ring_normal,
            ring_radius,
            bond_distance,
            anchor_pos,
            num_ring_dots,
            ..
        } => {
            if let AtomEditTool::AddAtom(state) = &mut atom_edit_data.active_tool {
                *state = AddAtomToolState::GuidedFreeRing {
                    anchor_atom_id: diff_atom_id,
                    ring_center: *ring_center,
                    ring_normal: *ring_normal,
                    ring_radius: *ring_radius,
                    bond_distance: *bond_distance,
                    anchor_pos: *anchor_pos,
                    num_ring_dots: *num_ring_dots,
                    preview_positions: None,
                    is_dative_bond: is_dative,
                };
            }
            0 // No fixed guide dots; ring is interactive
        }
    };

    GuidedPlacementStartResult::Started {
        guide_count,
        anchor_atom_id: diff_atom_id,
    }
}

/// Hit test guide dots against a ray. Returns the index of the closest hit dot.
pub fn hit_test_guide_dots(
    ray_start: &DVec3,
    ray_dir: &DVec3,
    guide_dots: &[GuideDot],
) -> Option<usize> {
    let mut closest: Option<(usize, f64)> = None;
    for (i, dot) in guide_dots.iter().enumerate() {
        if let Some(distance) =
            hit_test_utils::sphere_hit_test(&dot.position, GUIDE_DOT_HIT_RADIUS, ray_start, ray_dir)
        {
            if closest.is_none() || distance < closest.unwrap().1 {
                closest = Some((i, distance));
            }
        }
    }
    closest.map(|(i, _)| i)
}

/// Attempt to place an atom at a guide dot hit by the ray (FixedDots mode),
/// or at the preview position (FreeSphere / FreeRing mode).
///
/// If the placement position overlaps an existing atom within `MERGE_TOLERANCE`:
/// - Same element: merge (bond anchor to existing atom, no new atom created).
/// - Different element: replace existing atom's element, then bond anchor to it.
///
/// Returns true if an atom was placed or merged, false if no valid target was found.
pub fn place_guided_atom(
    structure_designer: &mut StructureDesigner,
    ray_start: &DVec3,
    ray_dir: &DVec3,
) -> bool {
    // Phase 1: Extract state, determine placement position, and detect merge target
    let is_diff_view = match get_active_atom_edit_data(structure_designer) {
        Some(data) => data.output_diff,
        None => return false,
    };

    let placement_info = {
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return false,
        };

        match &atom_edit_data.active_tool {
            AtomEditTool::AddAtom(AddAtomToolState::GuidedPlacement {
                anchor_atom_id,
                guide_dots,
                is_dative_bond,
                merge_targets,
                ..
            }) => {
                let dot_index = match hit_test_guide_dots(ray_start, ray_dir, guide_dots) {
                    Some(i) => i,
                    None => return false,
                };
                let position = guide_dots[dot_index].position;
                let merge_target = merge_targets
                    .get(dot_index)
                    .and_then(|mt| mt.clone());
                Some((*anchor_atom_id, position, *is_dative_bond, merge_target))
            }
            AtomEditTool::AddAtom(AddAtomToolState::GuidedFreeSphere {
                anchor_atom_id,
                center,
                radius,
                is_dative_bond,
                ..
            }) => {
                let dative = *is_dative_bond;
                let anchor = *anchor_atom_id;
                ray_sphere_nearest_point(ray_start, ray_dir, center, *radius)
                    .map(|hit_pos| (anchor, hit_pos, dative, None))
            }
            AtomEditTool::AddAtom(AddAtomToolState::GuidedFreeRing {
                anchor_atom_id,
                ring_center,
                ring_normal,
                ring_radius,
                bond_distance,
                anchor_pos,
                num_ring_dots,
                is_dative_bond,
                ..
            }) => {
                let dative = *is_dative_bond;
                let anchor = *anchor_atom_id;
                let cone_half_angle = cone_half_angle_for_ring(*num_ring_dots);
                ray_ring_nearest_point(ray_start, ray_dir, ring_center, ring_normal, *ring_radius)
                    .map(|point_on_ring| {
                        let positions = compute_ring_preview_positions(
                            *ring_center,
                            *ring_normal,
                            *ring_radius,
                            *anchor_pos,
                            *bond_distance,
                            point_on_ring,
                            *num_ring_dots,
                            cone_half_angle,
                        );
                        (anchor, positions[0], dative, None)
                    })
            }
            _ => None,
        }
    };

    let (anchor_atom_id, position, is_dative_bond, pre_computed_merge) = match placement_info {
        Some(info) => info,
        None => return false,
    };

    // For FreeSphere/FreeRing, detect merge target at placement time (not pre-computed).
    // For FixedDots, the merge target was pre-computed in start_guided_placement.
    // We need to resolve the anchor_atom_id from diff space back to result space for exclusion.
    let merge_target = if pre_computed_merge.is_some() {
        pre_computed_merge
    } else {
        // Check result structure for nearby atoms at the placement position.
        let result_structure = structure_designer.get_atomic_structure_from_selected_node();
        match result_structure {
            Some(rs) => {
                let eval_cache = if !is_diff_view {
                    structure_designer
                        .get_selected_node_eval_cache()
                        .and_then(|c| c.downcast_ref::<AtomEditEvalCache>())
                } else {
                    None
                };
                // In diff view, anchor_atom_id IS the result atom ID.
                // In result view, we need to find the result atom ID from the diff atom ID.
                let anchor_result_id = if is_diff_view {
                    anchor_atom_id
                } else {
                    eval_cache
                        .and_then(|c| c.provenance.diff_to_result.get(&anchor_atom_id).copied())
                        .unwrap_or(anchor_atom_id)
                };
                find_merge_target_at_position(
                    rs,
                    &position,
                    anchor_result_id,
                    is_diff_view,
                    eval_cache,
                )
            }
            None => None,
        }
    };

    // Phase 2: Add/merge atom and bond in diff
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return false,
    };

    let atomic_number = atom_edit_data.selected_atomic_number;
    let bond_order = if is_dative_bond {
        crate::crystolecule::atomic_structure::inline_bond::BOND_DATIVE
    } else {
        crate::crystolecule::atomic_structure::inline_bond::BOND_SINGLE
    };

    if let Some(target) = merge_target {
        // Resolve merge target to diff atom ID
        let target_diff_id = resolve_to_diff_id(atom_edit_data, &target);
        if let Some(diff_id) = target_diff_id {
            // Different element → replace
            if target.atomic_number != atomic_number {
                atom_edit_data.set_atomic_number_recorded(diff_id, atomic_number);
            }
            // Bond anchor to the existing/replaced atom
            atom_edit_data.add_bond_in_diff(anchor_atom_id, diff_id, bond_order);
        }
    } else {
        // No overlap — standard behavior: create new atom + bond
        let new_atom_id = atom_edit_data.add_atom_to_diff(atomic_number, position);
        atom_edit_data.add_bond_in_diff(anchor_atom_id, new_atom_id, bond_order);
    }

    // Transition back to Idle
    if let AtomEditTool::AddAtom(state) = &mut atom_edit_data.active_tool {
        *state = AddAtomToolState::Idle;
    }

    true
}

/// Resolve a MergeTarget to a diff atom ID, promoting from base if needed.
fn resolve_to_diff_id(
    atom_edit_data: &mut super::atom_edit_data::AtomEditData,
    target: &MergeTarget,
) -> Option<u32> {
    match &target.atom_source {
        AtomSource::BasePassthrough(_) => {
            // Promote base atom to diff: add with same element + position
            Some(atom_edit_data.add_atom_recorded(target.atomic_number, target.position))
        }
        AtomSource::DiffMatchedBase { diff_id, .. } | AtomSource::DiffAdded(diff_id) => {
            Some(*diff_id)
        }
    }
}

/// Compute merge targets for each guide dot in FixedDots mode.
/// Returns a Vec parallel to guide_dots: `Some(target)` if overlapping, `None` otherwise.
fn compute_merge_targets(
    result_structure: &crate::crystolecule::atomic_structure::AtomicStructure,
    guide_dots: &[GuideDot],
    anchor_result_atom_id: u32,
    is_diff_view: bool,
    eval_cache: Option<&AtomEditEvalCache>,
) -> Vec<Option<MergeTarget>> {
    guide_dots
        .iter()
        .map(|dot| {
            find_merge_target_at_position(
                result_structure,
                &dot.position,
                anchor_result_atom_id,
                is_diff_view,
                eval_cache,
            )
        })
        .collect()
}

/// Check if an existing atom overlaps a given position within MERGE_TOLERANCE.
/// Excludes the anchor atom. Returns the closest overlapping atom as a MergeTarget.
fn find_merge_target_at_position(
    result_structure: &crate::crystolecule::atomic_structure::AtomicStructure,
    position: &DVec3,
    anchor_result_atom_id: u32,
    is_diff_view: bool,
    eval_cache: Option<&AtomEditEvalCache>,
) -> Option<MergeTarget> {
    let nearby_ids = result_structure.get_atoms_in_radius(position, MERGE_TOLERANCE);
    let mut best: Option<(f64, MergeTarget)> = None;
    for atom_id in nearby_ids {
        // Skip the anchor atom itself
        if atom_id == anchor_result_atom_id {
            continue;
        }
        let atom = match result_structure.get_atom(atom_id) {
            Some(a) => a,
            None => continue,
        };
        // Skip delete markers
        if atom.is_delete_marker() {
            continue;
        }
        let dist = atom.position.distance(*position);
        if dist > MERGE_TOLERANCE {
            continue;
        }
        // Resolve provenance
        let atom_source = if is_diff_view {
            // In diff view, atom IDs are diff-native — treat as DiffAdded
            AtomSource::DiffAdded(atom_id)
        } else {
            match eval_cache.and_then(|c| c.provenance.sources.get(&atom_id)) {
                Some(source) => source.clone(),
                None => continue,
            }
        };
        let candidate = MergeTarget {
            result_atom_id: atom_id,
            atomic_number: atom.atomic_number,
            position: atom.position,
            atom_source,
        };
        if best.is_none() || dist < best.as_ref().unwrap().0 {
            best = Some((dist, candidate));
        }
    }
    best.map(|(_, target)| target)
}

/// Update the preview position for FreeSphere or FreeRing mode based on cursor ray.
///
/// Returns true if the preview position changed (needs re-render).
pub fn guided_placement_pointer_move(
    structure_designer: &mut StructureDesigner,
    ray_start: &DVec3,
    ray_dir: &DVec3,
) -> bool {
    let atom_edit_data = match get_atom_edit_data_mut_transient(structure_designer) {
        Some(data) => data,
        None => return false,
    };

    if let AtomEditTool::AddAtom(AddAtomToolState::GuidedFreeSphere {
        center,
        radius,
        preview_position,
        ..
    }) = &mut atom_edit_data.active_tool
    {
        let new_pos = ray_sphere_nearest_point(ray_start, ray_dir, center, *radius);
        if new_pos != *preview_position {
            *preview_position = new_pos;
            return true;
        }
    }

    if let AtomEditTool::AddAtom(AddAtomToolState::GuidedFreeRing {
        ring_center,
        ring_normal,
        ring_radius,
        bond_distance,
        anchor_pos,
        num_ring_dots,
        preview_positions,
        ..
    }) = &mut atom_edit_data.active_tool
    {
        let n_dots = *num_ring_dots;
        let half_angle = cone_half_angle_for_ring(n_dots);
        let new_positions =
            ray_ring_nearest_point(ray_start, ray_dir, ring_center, ring_normal, *ring_radius).map(
                |point_on_ring| {
                    compute_ring_preview_positions(
                        *ring_center,
                        *ring_normal,
                        *ring_radius,
                        *anchor_pos,
                        *bond_distance,
                        point_on_ring,
                        n_dots,
                        half_angle,
                    )
                },
            );
        if new_positions != *preview_positions {
            *preview_positions = new_positions;
            return true;
        }
    }
    false
}

/// Cancel guided placement and return to Idle state.
pub fn cancel_guided_placement(structure_designer: &mut StructureDesigner) {
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    if let AtomEditTool::AddAtom(state) = &mut atom_edit_data.active_tool {
        *state = AddAtomToolState::Idle;
    }
}

/// Check if the tool is currently in guided placement mode (FixedDots, FreeSphere, or FreeRing).
pub fn is_in_guided_placement(structure_designer: &StructureDesigner) -> bool {
    match get_active_atom_edit_data(structure_designer) {
        Some(data) => matches!(
            &data.active_tool,
            AtomEditTool::AddAtom(AddAtomToolState::GuidedPlacement { .. })
                | AtomEditTool::AddAtom(AddAtomToolState::GuidedFreeSphere { .. })
                | AtomEditTool::AddAtom(AddAtomToolState::GuidedFreeRing { .. })
        ),
        None => false,
    }
}
