use crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Whether a frozen atom ID refers to a base atom or a diff atom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrozenProvenance {
    Base,
    Diff,
}

/// Delta representing changes to the frozen atom sets.
#[derive(Debug, Clone)]
pub struct FrozenDelta {
    /// (provenance, atom_id) pairs added to frozen sets.
    pub added: Vec<(FrozenProvenance, u32)>,
    /// (provenance, atom_id) pairs removed from frozen sets.
    pub removed: Vec<(FrozenProvenance, u32)>,
}

/// Command for undoing/redoing freeze/unfreeze operations.
#[derive(Debug)]
pub struct AtomEditFrozenChangeCommand {
    pub description: String,
    pub network_name: String,
    pub node_id: u64,
    pub delta: FrozenDelta,
}

impl UndoCommand for AtomEditFrozenChangeCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        if let Some(data) = get_atom_edit_data_mut(ctx, &self.network_name, self.node_id) {
            // Remove what was added
            for &(prov, atom_id) in &self.delta.added {
                match prov {
                    FrozenProvenance::Base => {
                        data.frozen_base_atoms.remove(&atom_id);
                    }
                    FrozenProvenance::Diff => {
                        data.frozen_diff_atoms.remove(&atom_id);
                    }
                }
            }
            // Re-add what was removed
            for &(prov, atom_id) in &self.delta.removed {
                match prov {
                    FrozenProvenance::Base => {
                        data.frozen_base_atoms.insert(atom_id);
                    }
                    FrozenProvenance::Diff => {
                        data.frozen_diff_atoms.insert(atom_id);
                    }
                }
            }
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        if let Some(data) = get_atom_edit_data_mut(ctx, &self.network_name, self.node_id) {
            // Add what was added
            for &(prov, atom_id) in &self.delta.added {
                match prov {
                    FrozenProvenance::Base => {
                        data.frozen_base_atoms.insert(atom_id);
                    }
                    FrozenProvenance::Diff => {
                        data.frozen_diff_atoms.insert(atom_id);
                    }
                }
            }
            // Remove what was removed
            for &(prov, atom_id) in &self.delta.removed {
                match prov {
                    FrozenProvenance::Base => {
                        data.frozen_base_atoms.remove(&atom_id);
                    }
                    FrozenProvenance::Diff => {
                        data.frozen_diff_atoms.remove(&atom_id);
                    }
                }
            }
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
) -> Option<&'a mut AtomEditData> {
    let network = ctx.network_mut(network_name)?;
    let node = network.nodes.get_mut(&node_id)?;
    node.data.as_mut().as_any_mut().downcast_mut()
}
