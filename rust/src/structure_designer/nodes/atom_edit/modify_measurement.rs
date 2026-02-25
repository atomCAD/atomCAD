use std::collections::HashSet;

use glam::f64::{DQuat, DVec3};

use super::atom_edit_data::*;
use super::measurement::{MeasurementResult, SelectedAtomInfo, compute_measurement};
use super::types::*;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_structure::fragment::compute_moving_fragment;
use crate::crystolecule::atomic_structure_diff::AtomSource;
use crate::structure_designer::structure_designer::StructureDesigner;

// =============================================================================
// Public enums for move choice
// =============================================================================

/// Which atom to move in a 2-atom distance modification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistanceMoveChoice {
    /// Move the first atom in the measurement (atoms\[0\]).
    First,
    /// Move the second atom in the measurement (atoms\[1\]).
    Second,
}

/// Which arm to move in a 3-atom angle modification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AngleMoveChoice {
    /// Move arm A (the first non-vertex atom).
    ArmA,
    /// Move arm B (the second non-vertex atom).
    ArmB,
}

/// Which end to rotate in a 4-atom dihedral modification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DihedralMoveChoice {
    /// Rotate the A-side (chain\[0\] end).
    ASide,
    /// Rotate the D-side (chain\[3\] end).
    DSide,
}

// =============================================================================
// Modify distance
// =============================================================================

/// Modify the distance between two selected atoms by translating the chosen atom
/// (and optionally its connected fragment) along the bond axis.
///
/// Operates on the result structure and writes changes back through the diff.
pub fn modify_distance(
    structure_designer: &mut StructureDesigner,
    target_distance: f64,
    move_choice: DistanceMoveChoice,
    move_fragment: bool,
) -> Result<(), String> {
    if target_distance < 0.1 {
        return Err("Distance must be at least 0.1 Å".to_string());
    }

    // Phase 1: Gather — immutable borrows to collect all needed data
    let gathered = gather_measurement_data(structure_designer)?;
    let GatheredData {
        selected_atoms,
        result_structure,
        measurement,
        atom_provenance,
    } = gathered;

    let current_distance = match measurement {
        MeasurementResult::Distance { distance } => distance,
        _ => return Err("Expected distance measurement (2 atoms selected)".to_string()),
    };

    // Phase 2: Compute — determine which atoms move and by how much
    let (moving_idx, fixed_idx) = match move_choice {
        DistanceMoveChoice::First => (0, 1),
        DistanceMoveChoice::Second => (1, 0),
    };

    let moving_pos = selected_atoms[moving_idx].position;
    let fixed_pos = selected_atoms[fixed_idx].position;
    let moving_result_id = selected_atoms[moving_idx].result_atom_id;
    let fixed_result_id = selected_atoms[fixed_idx].result_atom_id;

    let axis = (moving_pos - fixed_pos).normalize_or_zero();
    if axis == DVec3::ZERO {
        return Err("Atoms are at the same position — cannot determine axis".to_string());
    }

    let delta = (target_distance - current_distance) * axis;

    // Determine which atoms to move
    let atoms_to_move = if move_fragment {
        compute_moving_fragment(&result_structure, moving_result_id, fixed_result_id)
    } else {
        let mut set = HashSet::new();
        set.insert(moving_result_id);
        set
    };

    // Compute new positions for all moving atoms
    let position_updates: Vec<(u32, DVec3)> = atoms_to_move
        .iter()
        .filter_map(|&result_id| {
            let atom = result_structure.get_atom(result_id)?;
            Some((result_id, atom.position + delta))
        })
        .collect();

    // Phase 3: Mutate — apply position changes through the diff
    apply_position_updates(structure_designer, &position_updates, &atom_provenance)
}

// =============================================================================
// Modify angle
// =============================================================================

/// Modify the angle at the vertex by rotating the chosen arm atom
/// (and optionally its connected fragment) around an axis through the vertex.
pub fn modify_angle(
    structure_designer: &mut StructureDesigner,
    target_angle_degrees: f64,
    move_choice: AngleMoveChoice,
    move_fragment: bool,
) -> Result<(), String> {
    if !(0.0..=180.0).contains(&target_angle_degrees) {
        return Err("Angle must be between 0° and 180°".to_string());
    }

    // Phase 1: Gather
    let gathered = gather_measurement_data(structure_designer)?;
    let GatheredData {
        selected_atoms,
        result_structure,
        measurement,
        atom_provenance,
    } = gathered;

    let (current_angle, vertex_index) = match measurement {
        MeasurementResult::Angle {
            angle_degrees,
            vertex_index,
        } => (angle_degrees, vertex_index),
        _ => return Err("Expected angle measurement (3 atoms selected)".to_string()),
    };

    // Phase 2: Compute
    // Determine arm indices (non-vertex atoms)
    let (arm_a_idx, arm_b_idx) = match vertex_index {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };

    let (moving_idx, _fixed_arm_idx) = match move_choice {
        AngleMoveChoice::ArmA => (arm_a_idx, arm_b_idx),
        AngleMoveChoice::ArmB => (arm_b_idx, arm_a_idx),
    };

    let vertex_pos = selected_atoms[vertex_index].position;
    let moving_pos = selected_atoms[moving_idx].position;
    let fixed_arm_pos = selected_atoms[_fixed_arm_idx].position;
    let moving_result_id = selected_atoms[moving_idx].result_atom_id;
    let vertex_result_id = selected_atoms[vertex_index].result_atom_id;

    // Rotation axis = cross product of the two arm vectors from vertex.
    // v_to_fixed × v_to_moving gives an axis where positive rotation
    // increases the angle (moves the arm away from the fixed arm).
    let v_to_moving = moving_pos - vertex_pos;
    let v_to_fixed = fixed_arm_pos - vertex_pos;
    let cross = v_to_fixed.cross(v_to_moving);

    let rotation_axis = if cross.length_squared() < 1e-20 {
        // Collinear: pick any perpendicular axis
        arbitrary_perpendicular(v_to_moving)
    } else {
        cross.normalize()
    };

    let rotation_angle_rad = (target_angle_degrees - current_angle).to_radians();

    // Determine which atoms to move
    let atoms_to_move = if move_fragment {
        compute_moving_fragment(&result_structure, moving_result_id, vertex_result_id)
    } else {
        let mut set = HashSet::new();
        set.insert(moving_result_id);
        set
    };

    // Compute new positions
    let position_updates: Vec<(u32, DVec3)> = atoms_to_move
        .iter()
        .filter_map(|&result_id| {
            let atom = result_structure.get_atom(result_id)?;
            let new_pos = rotate_point_around_axis(
                atom.position,
                vertex_pos,
                rotation_axis,
                rotation_angle_rad,
            );
            Some((result_id, new_pos))
        })
        .collect();

    // Phase 3: Mutate
    apply_position_updates(structure_designer, &position_updates, &atom_provenance)
}

// =============================================================================
// Modify dihedral
// =============================================================================

/// Modify the dihedral angle by rotating the chosen end
/// (and optionally its connected fragment) around the B-C axis.
pub fn modify_dihedral(
    structure_designer: &mut StructureDesigner,
    target_angle_degrees: f64,
    move_choice: DihedralMoveChoice,
    move_fragment: bool,
) -> Result<(), String> {
    if !(-180.0..=180.0).contains(&target_angle_degrees) {
        return Err("Dihedral angle must be between -180° and 180°".to_string());
    }

    // Phase 1: Gather
    let gathered = gather_measurement_data(structure_designer)?;
    let GatheredData {
        selected_atoms,
        result_structure,
        measurement,
        atom_provenance,
    } = gathered;

    let (current_angle, chain) = match measurement {
        MeasurementResult::Dihedral {
            angle_degrees,
            chain,
        } => (angle_degrees, chain),
        _ => return Err("Expected dihedral measurement (4 atoms selected)".to_string()),
    };

    // Phase 2: Compute
    let b_pos = selected_atoms[chain[1]].position;
    let c_pos = selected_atoms[chain[2]].position;

    let bc_axis = (c_pos - b_pos).normalize_or_zero();
    if bc_axis == DVec3::ZERO {
        return Err("Center atoms overlap — cannot define rotation axis".to_string());
    }

    let rotation_angle_rad = (target_angle_degrees - current_angle).to_radians();

    // Determine moving end and reference end
    let (moving_end_idx, ref_end_idx) = match move_choice {
        DihedralMoveChoice::ASide => (chain[0], chain[3]),
        DihedralMoveChoice::DSide => (chain[3], chain[0]),
    };

    let moving_result_id = selected_atoms[moving_end_idx].result_atom_id;
    let ref_result_id = selected_atoms[ref_end_idx].result_atom_id;

    // Determine which atoms to move
    let atoms_to_move = if move_fragment {
        compute_moving_fragment(&result_structure, moving_result_id, ref_result_id)
    } else {
        let mut set = HashSet::new();
        set.insert(moving_result_id);
        set
    };

    // Compute new positions — rotate around B-C axis through point B
    let position_updates: Vec<(u32, DVec3)> = atoms_to_move
        .iter()
        .filter_map(|&result_id| {
            let atom = result_structure.get_atom(result_id)?;
            let new_pos =
                rotate_point_around_axis(atom.position, b_pos, bc_axis, rotation_angle_rad);
            Some((result_id, new_pos))
        })
        .collect();

    // Phase 3: Mutate
    apply_position_updates(structure_designer, &position_updates, &atom_provenance)
}

// =============================================================================
// Internal helpers
// =============================================================================

/// Data gathered during Phase 1 (immutable borrows).
struct GatheredData {
    selected_atoms: Vec<SelectedAtomInfo>,
    result_structure: AtomicStructure,
    measurement: MeasurementResult,
    /// For each result atom ID: (provenance, provenance_id, atomic_number, position).
    /// Used to promote base atoms to diff and to call move_in_diff.
    atom_provenance: Vec<AtomProvenanceEntry>,
}

/// Provenance info for a single result atom.
struct AtomProvenanceEntry {
    result_id: u32,
    /// diff atom ID, if this atom already exists in the diff.
    diff_id: Option<u32>,
    /// (atomic_number, position) for base pass-through atoms that need promotion.
    identity: Option<(i16, DVec3)>,
}

/// Phase 1: Gather all data needed for measurement modification.
///
/// Builds the selected atom info list, computes the measurement, and collects
/// provenance information for all atoms in the result structure.
fn gather_measurement_data(structure_designer: &StructureDesigner) -> Result<GatheredData, String> {
    let atom_edit_data = get_active_atom_edit_data(structure_designer)
        .ok_or_else(|| "No active atom_edit node".to_string())?;

    let result_structure = structure_designer
        .get_atomic_structure_from_selected_node()
        .ok_or_else(|| "No result structure available".to_string())?;

    // Build selected atom infos — resolve through provenance
    let total_selected = atom_edit_data.selection.selected_base_atoms.len()
        + atom_edit_data.selection.selected_diff_atoms.len();
    if !(2..=4).contains(&total_selected) {
        return Err(format!(
            "Expected 2-4 selected atoms, got {}",
            total_selected
        ));
    }

    let eval_cache = if !atom_edit_data.output_diff {
        let cache = structure_designer
            .get_selected_node_eval_cache()
            .ok_or_else(|| "No eval cache available".to_string())?;
        Some(
            cache
                .downcast_ref::<AtomEditEvalCache>()
                .ok_or_else(|| "Eval cache is not AtomEditEvalCache".to_string())?,
        )
    } else {
        None
    };

    // Use selection_order to build selected_atoms in a deterministic order.
    // This ensures that atoms[0]/atoms[1] in the measurement correspond to
    // the order the user selected them, making MoveChoice predictable.
    let mut selected_atoms: Vec<SelectedAtomInfo> = Vec::with_capacity(total_selected);

    if atom_edit_data.output_diff {
        // Diff view: diff atom IDs ARE the output atom IDs.
        // Use selection_order for deterministic ordering.
        for &(prov, id) in &atom_edit_data.selection.selection_order {
            if prov == SelectionProvenance::Diff
                && atom_edit_data.selection.selected_diff_atoms.contains(&id)
            {
                if let Some(atom) = result_structure.get_atom(id) {
                    selected_atoms.push(SelectedAtomInfo {
                        result_atom_id: id,
                        position: atom.position,
                    });
                }
            }
        }
    } else {
        let cache = eval_cache.as_ref().unwrap();
        // Use selection_order for deterministic ordering.
        for &(prov, id) in &atom_edit_data.selection.selection_order {
            match prov {
                SelectionProvenance::Base => {
                    if atom_edit_data.selection.selected_base_atoms.contains(&id) {
                        if let Some(&result_id) = cache.provenance.base_to_result.get(&id) {
                            if let Some(atom) = result_structure.get_atom(result_id) {
                                selected_atoms.push(SelectedAtomInfo {
                                    result_atom_id: result_id,
                                    position: atom.position,
                                });
                            }
                        }
                    }
                }
                SelectionProvenance::Diff => {
                    if atom_edit_data.selection.selected_diff_atoms.contains(&id) {
                        if let Some(&result_id) = cache.provenance.diff_to_result.get(&id) {
                            if let Some(atom) = result_structure.get_atom(result_id) {
                                selected_atoms.push(SelectedAtomInfo {
                                    result_atom_id: result_id,
                                    position: atom.position,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    if !(2..=4).contains(&selected_atoms.len()) {
        return Err(format!(
            "Could not resolve {} selected atoms to result positions",
            total_selected
        ));
    }

    // Compute the measurement
    let measurement = compute_measurement(&selected_atoms, result_structure)
        .ok_or_else(|| "Failed to compute measurement".to_string())?;

    // Collect provenance info for ALL atoms in the result structure
    // (we need this to map result IDs back to diff IDs for the mutation phase)
    let mut atom_provenance = Vec::new();
    if atom_edit_data.output_diff {
        // In diff view, all result IDs are diff IDs
        for (&atom_id, _atom) in result_structure.iter_atoms() {
            atom_provenance.push(AtomProvenanceEntry {
                result_id: atom_id,
                diff_id: Some(atom_id),
                identity: None,
            });
        }
    } else {
        let cache = eval_cache.as_ref().unwrap();
        for (&atom_id, atom) in result_structure.iter_atoms() {
            let source = cache.provenance.sources.get(&atom_id);
            let (diff_id, identity) = match source {
                Some(AtomSource::BasePassthrough(_)) => {
                    (None, Some((atom.atomic_number, atom.position)))
                }
                Some(AtomSource::DiffMatchedBase { diff_id, .. }) => (Some(*diff_id), None),
                Some(AtomSource::DiffAdded(diff_id)) => (Some(*diff_id), None),
                None => (None, Some((atom.atomic_number, atom.position))),
            };
            atom_provenance.push(AtomProvenanceEntry {
                result_id: atom_id,
                diff_id,
                identity,
            });
        }
    }

    // Clone the result structure since we need it after the borrows end
    let result_structure_owned = result_structure.clone();

    Ok(GatheredData {
        selected_atoms,
        result_structure: result_structure_owned,
        measurement,
        atom_provenance,
    })
}

/// Phase 3: Apply computed position updates through the diff system.
///
/// For each (result_id, new_position):
/// - If the atom already has a diff ID, call `move_in_diff`.
/// - If it's a base pass-through, promote it to diff first (add with anchor), then move.
fn apply_position_updates(
    structure_designer: &mut StructureDesigner,
    position_updates: &[(u32, DVec3)],
    atom_provenance: &[AtomProvenanceEntry],
) -> Result<(), String> {
    let atom_edit_data = get_selected_atom_edit_data_mut(structure_designer)
        .ok_or_else(|| "Cannot get mutable atom_edit data".to_string())?;

    // Build a lookup from result_id to provenance entry
    let provenance_lookup: rustc_hash::FxHashMap<u32, &AtomProvenanceEntry> = atom_provenance
        .iter()
        .map(|entry| (entry.result_id, entry))
        .collect();

    for &(result_id, new_position) in position_updates {
        let entry = match provenance_lookup.get(&result_id) {
            Some(e) => e,
            None => continue, // Atom not in provenance (shouldn't happen)
        };

        if let Some(diff_id) = entry.diff_id {
            // Atom already in diff — just move it
            atom_edit_data.move_in_diff(diff_id, new_position);
        } else if let Some((atomic_number, old_position)) = entry.identity {
            // Base pass-through atom — promote to diff with anchor, then move
            let new_diff_id = atom_edit_data.diff.add_atom(atomic_number, old_position);
            atom_edit_data
                .diff
                .set_anchor_position(new_diff_id, old_position);
            atom_edit_data.move_in_diff(new_diff_id, new_position);
        }
    }

    Ok(())
}

/// Rotate a point around an axis through a center point.
fn rotate_point_around_axis(point: DVec3, center: DVec3, axis: DVec3, angle_rad: f64) -> DVec3 {
    let q = DQuat::from_axis_angle(axis, angle_rad);
    center + q * (point - center)
}

/// Find an arbitrary vector perpendicular to the given vector.
/// Used when the cross product is degenerate (collinear atoms).
fn arbitrary_perpendicular(v: DVec3) -> DVec3 {
    let candidate = if v.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    v.cross(candidate).normalize()
}
