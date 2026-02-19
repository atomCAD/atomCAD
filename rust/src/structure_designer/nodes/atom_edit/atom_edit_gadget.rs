use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::display::gadget::Gadget;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput};
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::utils::xyz_gadget_utils;
use glam::f64::{DQuat, DVec3};
use std::cell::{Cell, RefCell};

use super::atom_edit::AtomEditData;

/// Gadget for the atom_edit Default tool that displays an XYZ translation gizmo
/// at the selection centroid. Dragging an axis arrow moves all selected atoms
/// along that axis.
///
/// The gadget is always world-aligned (not rotated), matching `AtomMoveGadget`.
///
/// During drag, `sync_data()` applies the displacement to all selected atoms:
/// - Diff atoms are repositioned absolutely (original_pos + total_delta).
/// - Base atoms are converted to diff atoms on the first non-zero sync.
#[derive(Clone)]
pub struct AtomEditSelectionGadget {
    /// Current centroid position (updated during drag).
    pub center: DVec3,
    /// Centroid at gadget creation time.
    pub original_center: DVec3,
    /// Currently dragged axis handle (0=X, 1=Y, 2=Z), or None.
    pub dragged_handle: Option<i32>,
    /// Axis offset at drag start.
    pub start_drag_offset: f64,
    /// Center position at drag start.
    pub start_drag_center: DVec3,
    /// Snapshot of selected diff atom positions at gadget creation: (diff_id, position).
    diff_atom_positions: Vec<(u32, DVec3)>,
    /// Base atoms to convert on first sync: (base_id, atomic_number, position).
    base_atoms_info: Vec<(u32, i16, DVec3)>,
    /// Whether base atoms have been converted to diff atoms.
    base_converted: Cell<bool>,
    /// Diff atom positions added after base conversion: (new_diff_id, original_position).
    converted_positions: RefCell<Vec<(u32, DVec3)>>,
}

impl AtomEditSelectionGadget {
    pub fn new(
        center: DVec3,
        diff_atom_positions: Vec<(u32, DVec3)>,
        base_atoms_info: Vec<(u32, i16, DVec3)>,
    ) -> Self {
        Self {
            center,
            original_center: center,
            dragged_handle: None,
            start_drag_offset: 0.0,
            start_drag_center: center,
            diff_atom_positions,
            base_atoms_info,
            base_converted: Cell::new(false),
            converted_positions: RefCell::new(Vec::new()),
        }
    }

    /// Applies drag offset along the given axis, updating the center position.
    /// Returns true if successful (and the drag start should be reset).
    fn apply_drag_offset(&mut self, axis_index: i32, offset_delta: f64) -> bool {
        if !(0..=2).contains(&axis_index) {
            return false;
        }

        let axis_direction = match xyz_gadget_utils::get_local_axis_direction(
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            axis_index,
        ) {
            Some(dir) => dir,
            None => return false,
        };

        let movement_vector = axis_direction * offset_delta;
        self.center = self.start_drag_center + movement_vector;
        true
    }
}

impl Tessellatable for AtomEditSelectionGadget {
    fn tessellate(&self, output: &mut TessellationOutput) {
        let output_mesh: &mut Mesh = &mut output.mesh;
        xyz_gadget_utils::tessellate_xyz_gadget(
            output_mesh,
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            &self.center,
            false, // No rotation handles (translation only)
        );
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for AtomEditSelectionGadget {
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        xyz_gadget_utils::xyz_gadget_hit_test(
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            &self.center,
            &ray_origin,
            &ray_direction,
            false, // No rotation handles
        )
    }

    fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        self.dragged_handle = Some(handle_index);
        self.start_drag_offset = xyz_gadget_utils::get_dragged_axis_offset(
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            &self.center,
            handle_index,
            &ray_origin,
            &ray_direction,
        );
        self.start_drag_center = self.center;
    }

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        let current_offset = xyz_gadget_utils::get_dragged_axis_offset(
            &UnitCellStruct::cubic_diamond(),
            DQuat::IDENTITY,
            &self.center,
            handle_index,
            &ray_origin,
            &ray_direction,
        );
        let offset_delta = current_offset - self.start_drag_offset;
        if self.apply_drag_offset(handle_index, offset_delta) {
            self.start_drag(handle_index, ray_origin, ray_direction);
        }
    }

    fn end_drag(&mut self) {
        self.dragged_handle = None;
    }
}

impl NodeNetworkGadget for AtomEditSelectionGadget {
    fn sync_data(&self, data: &mut dyn NodeData) {
        let atom_edit_data = match data.as_any_mut().downcast_mut::<AtomEditData>() {
            Some(d) => d,
            None => return,
        };

        let total_delta = self.center - self.original_center;

        // Convert base atoms to diff atoms once (on first non-zero delta).
        // Must happen before moving diff atoms so that the newly converted
        // atoms are placed at the correct position and don't get double-moved.
        if !self.base_converted.get()
            && !self.base_atoms_info.is_empty()
            && total_delta.length_squared() > 1e-15
        {
            let mut converted = self.converted_positions.borrow_mut();
            for &(base_id, atomic_number, position) in &self.base_atoms_info {
                let target = position + total_delta;
                let new_diff_id = atom_edit_data.diff.add_atom(atomic_number, target);
                atom_edit_data
                    .diff
                    .set_anchor_position(new_diff_id, position);
                atom_edit_data
                    .selection
                    .selected_base_atoms
                    .remove(&base_id);
                atom_edit_data
                    .selection
                    .selected_diff_atoms
                    .insert(new_diff_id);
                converted.push((new_diff_id, position));
            }
            self.base_converted.set(true);
        }

        // Apply absolute positions to pre-existing diff atoms.
        for &(diff_id, original_pos) in &self.diff_atom_positions {
            let target = original_pos + total_delta;
            if !atom_edit_data.diff.has_anchor_position(diff_id)
                && atom_edit_data.diff.get_atom(diff_id).is_some()
            {
                atom_edit_data
                    .diff
                    .set_anchor_position(diff_id, original_pos);
            }
            atom_edit_data.diff.set_atom_position(diff_id, target);
        }

        // Apply absolute positions to converted (formerly base) atoms.
        {
            let converted = self.converted_positions.borrow();
            for &(diff_id, original_pos) in converted.iter() {
                let target = original_pos + total_delta;
                atom_edit_data.diff.set_atom_position(diff_id, target);
            }
        }

        // Update selection transform to reflect the new centroid.
        if let Some(ref mut transform) = atom_edit_data.selection.selection_transform {
            transform.translation = self.center;
        }
        atom_edit_data.selection.clear_bonds();
    }

    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }
}
