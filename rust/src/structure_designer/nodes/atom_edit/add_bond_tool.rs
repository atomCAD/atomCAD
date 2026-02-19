use super::atom_edit_data::*;
use super::types::*;
use crate::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization;
use crate::crystolecule::atomic_structure::HitTestResult;
use crate::crystolecule::atomic_structure_diff::AtomSource;
use crate::display::atomic_tessellator::{BAS_STICK_RADIUS, get_displayed_atom_radius};
use crate::display::preferences as display_prefs;
use crate::structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec3;

/// Draw a bond by clicking on atoms (two-click workflow).
///
/// First click stores the atom, second click creates the bond.
/// Clicking the same atom again cancels the pending bond.
pub fn draw_bond_by_ray(
    structure_designer: &mut StructureDesigner,
    ray_start: &DVec3,
    ray_dir: &DVec3,
) {
    // Phase 1: Hit test and gather info (immutable borrows)
    let is_diff_view = match get_active_atom_edit_data(structure_designer) {
        Some(data) => data.output_diff,
        None => return,
    };

    let (atom_source, atom_info) = {
        let result_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(s) => s,
            None => return,
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
            _ => return,
        };

        if is_diff_view {
            // In diff view, atom IDs are diff-native — no provenance needed
            let atom = match result_structure.get_atom(result_atom_id) {
                Some(a) => (a.atomic_number, a.position),
                None => return,
            };
            (None, (result_atom_id, atom))
        } else {
            let eval_cache = match structure_designer.get_selected_node_eval_cache() {
                Some(cache) => cache,
                None => return,
            };
            let eval_cache = match eval_cache.downcast_ref::<AtomEditEvalCache>() {
                Some(cache) => cache,
                None => return,
            };

            let source = match eval_cache.provenance.sources.get(&result_atom_id) {
                Some(s) => s.clone(),
                None => return,
            };

            let atom = match result_structure.get_atom(result_atom_id) {
                Some(a) => (a.atomic_number, a.position),
                None => return,
            };

            (Some(source), (result_atom_id, atom))
        }
    };

    // Phase 2: Resolve to diff atom ID and handle bond workflow
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    // Resolve to diff atom ID
    let diff_atom_id = if is_diff_view {
        // In diff view, the hit ID is already a diff atom ID
        atom_info.0
    } else {
        // In result view, map through provenance (add identity entry for base atoms)
        match &atom_source {
            Some(AtomSource::BasePassthrough(_)) => {
                atom_edit_data.diff.add_atom(atom_info.1.0, atom_info.1.1)
            }
            Some(AtomSource::DiffMatchedBase { diff_id, .. })
            | Some(AtomSource::DiffAdded(diff_id)) => *diff_id,
            None => return,
        }
    };

    // Get current last_atom_id (copies the value, ending the immutable borrow)
    let last_atom_id = if let AtomEditTool::AddBond(state) = &atom_edit_data.active_tool {
        state.last_atom_id
    } else {
        return;
    };

    match last_atom_id {
        Some(last_id) => {
            if last_id == diff_atom_id {
                // Same atom clicked again → cancel pending bond
                if let AtomEditTool::AddBond(state) = &mut atom_edit_data.active_tool {
                    state.last_atom_id = None;
                }
            } else {
                // Create bond between last atom and current atom
                atom_edit_data.add_bond_in_diff(last_id, diff_atom_id, 1);
                // Update last_atom_id for continuous bonding
                if let AtomEditTool::AddBond(state) = &mut atom_edit_data.active_tool {
                    state.last_atom_id = Some(diff_atom_id);
                }
            }
        }
        None => {
            // First click: store this atom
            if let AtomEditTool::AddBond(state) = &mut atom_edit_data.active_tool {
                state.last_atom_id = Some(diff_atom_id);
            }
        }
    }
}
