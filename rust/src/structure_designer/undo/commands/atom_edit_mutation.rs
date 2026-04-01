use crate::crystolecule::atomic_structure::{AtomicStructure, BondReference};
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::nodes::atom_edit::diff_recorder::{
    AtomDelta, BondDelta, CrossCellBondDelta,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing atom_edit diff mutations.
///
/// Stores the ordered list of atom and bond deltas produced by a recording session.
/// Hybridization overrides are now captured as atom flag changes in AtomDelta.
///
/// Undo reverses them; redo re-applies them. Both use non-recording AtomicStructure
/// methods directly to avoid producing new deltas.
#[derive(Debug)]
pub struct AtomEditMutationCommand {
    pub description: String,
    pub network_name: String,
    pub node_id: u64,
    pub atom_deltas: Vec<AtomDelta>,
    pub bond_deltas: Vec<BondDelta>,
    pub cross_cell_bond_deltas: Vec<CrossCellBondDelta>,
}

impl UndoCommand for AtomEditMutationCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        if let Some(data) = get_atom_edit_data_mut(ctx, &self.network_name, self.node_id) {
            apply_undo(data, &self.atom_deltas, &self.bond_deltas);
            apply_cross_cell_bond_undo(data, &self.cross_cell_bond_deltas);
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        if let Some(data) = get_atom_edit_data_mut(ctx, &self.network_name, self.node_id) {
            apply_redo(data, &self.atom_deltas, &self.bond_deltas);
            apply_cross_cell_bond_redo(data, &self.cross_cell_bond_deltas);
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::NodeDataChanged(vec![self.node_id])
    }
}

fn get_atom_edit_data_mut<'a>(
    ctx: &'a mut UndoContext,
    network_name: &str,
    node_id: u64,
) -> Option<&'a mut crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData> {
    let network = ctx.network_mut(network_name)?;
    let node = network.nodes.get_mut(&node_id)?;
    node.data.as_mut().as_any_mut().downcast_mut()
}

/// Undo: apply deltas in reverse, restoring each delta's `before` state.
///
/// Three-pass structure to maintain the invariant that bonds are only
/// added/removed while both endpoint atoms exist:
/// 1. Remove added bonds (reverse order)
/// 2. Restore atoms (reverse order)
/// 3. Re-add removed bonds (reverse order)
fn apply_undo(
    data: &mut crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData,
    atom_deltas: &[AtomDelta],
    bond_deltas: &[BondDelta],
) {
    // Pass 1: Undo bond additions and order changes (atoms still exist).
    for delta in bond_deltas.iter().rev() {
        match (delta.old_order, delta.new_order) {
            (None, Some(_)) => {
                // Bond was added → remove it
                data.diff.delete_bond(&BondReference {
                    atom_id1: delta.atom_id1,
                    atom_id2: delta.atom_id2,
                });
            }
            (Some(old), Some(_)) => {
                // Bond order changed → restore old order
                data.diff
                    .add_bond_checked(delta.atom_id1, delta.atom_id2, old);
            }
            _ => {} // Removals handled in pass 3
        }
    }

    // Pass 2: Restore atoms.
    for delta in atom_deltas.iter().rev() {
        match (&delta.before, &delta.after) {
            (None, Some(_)) => {
                // Atom was added → delete it
                data.diff.delete_atom(delta.atom_id);
                data.diff.remove_anchor_position(delta.atom_id);
            }
            (Some(state), None) => {
                // Atom was removed → re-add with original ID
                data.diff
                    .add_atom_with_id(delta.atom_id, state.atomic_number, state.position);
                if let Some(anchor) = state.anchor {
                    data.diff.set_anchor_position(delta.atom_id, anchor);
                }
                restore_flags(&mut data.diff, delta.atom_id, state.flags);
            }
            (Some(before), Some(_after)) => {
                // Atom was modified → restore before state
                data.diff
                    .set_atomic_number(delta.atom_id, before.atomic_number);
                data.diff.set_atom_position(delta.atom_id, before.position);
                match before.anchor {
                    Some(anchor) => data.diff.set_anchor_position(delta.atom_id, anchor),
                    None => data.diff.remove_anchor_position(delta.atom_id),
                }
                restore_flags(&mut data.diff, delta.atom_id, before.flags);
            }
            (None, None) => {}
        }
    }

    // Pass 3: Re-add removed bonds (atoms they reference now exist).
    for delta in bond_deltas.iter().rev() {
        if let (Some(order), None) = (delta.old_order, delta.new_order) {
            // Bond was removed → re-add it
            data.diff.add_bond(delta.atom_id1, delta.atom_id2, order);
        }
    }

    data.selection.clear();
    data.clear_input_cache();
}

/// Redo: apply deltas in forward order, restoring each delta's `after` state.
///
/// Same three-pass structure, mirrored for forward application:
/// 1. Remove bonds being removed (forward order)
/// 2. Apply atom deltas (forward order)
/// 3. Add new bonds (forward order)
fn apply_redo(
    data: &mut crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData,
    atom_deltas: &[AtomDelta],
    bond_deltas: &[BondDelta],
) {
    // Pass 1: Remove bonds and apply order changes (atoms still exist).
    for delta in bond_deltas.iter() {
        match (delta.old_order, delta.new_order) {
            (Some(_), None) => {
                // Bond was removed → remove it
                data.diff.delete_bond(&BondReference {
                    atom_id1: delta.atom_id1,
                    atom_id2: delta.atom_id2,
                });
            }
            (Some(_), Some(new)) => {
                // Bond order changed → apply new order
                data.diff
                    .add_bond_checked(delta.atom_id1, delta.atom_id2, new);
            }
            _ => {} // Additions handled in pass 3
        }
    }

    // Pass 2: Apply atom deltas.
    for delta in atom_deltas.iter() {
        match (&delta.before, &delta.after) {
            (None, Some(state)) => {
                // Atom was added → re-add with original ID
                data.diff
                    .add_atom_with_id(delta.atom_id, state.atomic_number, state.position);
                if let Some(anchor) = state.anchor {
                    data.diff.set_anchor_position(delta.atom_id, anchor);
                }
                restore_flags(&mut data.diff, delta.atom_id, state.flags);
            }
            (Some(_), None) => {
                // Atom was removed → delete it
                data.diff.delete_atom(delta.atom_id);
                data.diff.remove_anchor_position(delta.atom_id);
            }
            (Some(_before), Some(after)) => {
                // Atom was modified → apply after state
                data.diff
                    .set_atomic_number(delta.atom_id, after.atomic_number);
                data.diff.set_atom_position(delta.atom_id, after.position);
                match after.anchor {
                    Some(anchor) => data.diff.set_anchor_position(delta.atom_id, anchor),
                    None => data.diff.remove_anchor_position(delta.atom_id),
                }
                restore_flags(&mut data.diff, delta.atom_id, after.flags);
            }
            (None, None) => {}
        }
    }

    // Pass 3: Add new bonds (atoms they reference now exist).
    for delta in bond_deltas.iter() {
        if let (None, Some(order)) = (delta.old_order, delta.new_order) {
            // Bond was added → add it
            data.diff.add_bond(delta.atom_id1, delta.atom_id2, order);
        }
    }

    data.selection.clear();
    data.clear_input_cache();
}

/// Restore atom flags (frozen, hybridization, passivation) from a saved state.
/// Uses the public per-flag setters on AtomicStructure.
fn restore_flags(diff: &mut AtomicStructure, atom_id: u32, flags: u16) {
    let frozen = (flags & (1 << 2)) != 0;
    let h_passivation = (flags & (1 << 1)) != 0;
    let hybridization = ((flags >> 3) & 0b11) as u8;
    diff.set_atom_frozen(atom_id, frozen);
    diff.set_atom_hydrogen_passivation(atom_id, h_passivation);
    diff.set_atom_hybridization_override(atom_id, hybridization);
}

/// Undo cross-cell bond metadata changes: restore old_offset for each delta.
fn apply_cross_cell_bond_undo(
    data: &mut crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData,
    deltas: &[CrossCellBondDelta],
) {
    for delta in deltas.iter().rev() {
        match delta.old_offset {
            Some(offset) => {
                data.cross_cell_bonds.insert(delta.bond_ref.clone(), offset);
            }
            None => {
                data.cross_cell_bonds.remove(&delta.bond_ref);
            }
        }
    }
}

/// Redo cross-cell bond metadata changes: apply new_offset for each delta.
fn apply_cross_cell_bond_redo(
    data: &mut crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData,
    deltas: &[CrossCellBondDelta],
) {
    for delta in deltas.iter() {
        match delta.new_offset {
            Some(offset) => {
                data.cross_cell_bonds.insert(delta.bond_ref.clone(), offset);
            }
            None => {
                data.cross_cell_bonds.remove(&delta.bond_ref);
            }
        }
    }
}
