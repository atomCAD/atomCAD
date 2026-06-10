use crate::structure_designer::node_type_registry::RecordTypeDef;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing the addition of a record type def. Reversal is
/// a simple removal — adding a def has no cross-network side effects (every
/// reference resolves through the registry on every lookup), so undo just
/// drops it from `record_type_defs`.
#[derive(Debug)]
pub struct AddRecordTypeDefCommand {
    /// Full def captured at add time so redo is byte-identical.
    pub def: RecordTypeDef,
    /// Ancestor empty-folder markers absorbed (pruned) when this def was
    /// created; restored on undo. See `doc/design_empty_folders.md`.
    pub pruned_folders: Vec<String>,
}

impl UndoCommand for AddRecordTypeDefCommand {
    fn description(&self) -> &str {
        "Add record type def"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        ctx.node_type_registry
            .record_type_defs
            .remove(&self.def.name);
        // Restore any empty-folder markers this def's creation absorbed.
        for f in &self.pruned_folders {
            ctx.node_type_registry.folders.insert(f.clone());
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        // Re-absorb the same ancestor markers (this insert bypasses the
        // auto-pruning `add_record_type_def`, so prune explicitly).
        for f in &self.pruned_folders {
            ctx.node_type_registry.folders.remove(f);
        }
        ctx.node_type_registry
            .record_type_defs
            .insert(self.def.name.clone(), self.def.clone());
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
