use super::rename_helpers::apply_rename_core;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing namespace rename (batch rename of all networks under a prefix).
#[derive(Debug)]
pub struct RenameNamespaceCommand {
    /// List of (old_name, new_name) pairs for all affected networks.
    pub renames: Vec<(String, String)>,
}

impl UndoCommand for RenameNamespaceCommand {
    fn description(&self) -> &str {
        "Rename namespace"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        for (old_name, new_name) in &self.renames {
            apply_rename_core(
                ctx.node_type_registry,
                ctx.active_network_name,
                new_name,
                old_name,
            );
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        for (old_name, new_name) in &self.renames {
            apply_rename_core(
                ctx.node_type_registry,
                ctx.active_network_name,
                old_name,
                new_name,
            );
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
