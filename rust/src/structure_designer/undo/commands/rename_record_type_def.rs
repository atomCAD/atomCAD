use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing a record type def rename. Rename has no wire-
/// disconnection side effects (every reference resolves through the registry
/// to the same schema, just under a new name), so undo just renames in the
/// other direction. Both directions are valid since the rename walker rewrites
/// every embedded `Named(N)` reference symmetrically.
#[derive(Debug)]
pub struct RenameRecordTypeDefCommand {
    pub old_name: String,
    pub new_name: String,
}

impl UndoCommand for RenameRecordTypeDefCommand {
    fn description(&self) -> &str {
        "Rename record type def"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Rename new -> old. The registry-level call validates against
        // existing names and walks every reference. If validation fails (it
        // shouldn't — the original add already passed), silently no-op rather
        // than panic in the undo path.
        let _ = ctx
            .node_type_registry
            .rename_record_type_def(&self.new_name, &self.old_name);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        let _ = ctx
            .node_type_registry
            .rename_record_type_def(&self.old_name, &self.new_name);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
