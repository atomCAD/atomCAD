use super::rename_helpers::apply_rename_core;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing network rename.
#[derive(Debug)]
pub struct RenameNetworkCommand {
    pub old_name: String,
    pub new_name: String,
}

impl UndoCommand for RenameNetworkCommand {
    fn description(&self) -> &str {
        "Rename network"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        apply_rename_core(
            ctx.node_type_registry,
            ctx.active_network_name,
            &self.new_name,
            &self.old_name,
        );
    }

    fn redo(&self, ctx: &mut UndoContext) {
        apply_rename_core(
            ctx.node_type_registry,
            ctx.active_network_name,
            &self.old_name,
            &self.new_name,
        );
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
