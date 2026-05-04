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
}

impl UndoCommand for AddRecordTypeDefCommand {
    fn description(&self) -> &str {
        "Add record type def"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        ctx.node_type_registry
            .record_type_defs
            .remove(&self.def.name);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        ctx.node_type_registry
            .record_type_defs
            .insert(self.def.name.clone(), self.def.clone());
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
