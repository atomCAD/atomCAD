use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing a record type def rename. Rename has no wire-
/// disconnection side effects (every reference resolves through the registry
/// to the same schema, just under a new name), so undo just renames in the
/// other direction. Both directions go through the infallible
/// `rename_record_type_def_unchecked` (Helper 1) — the target name was just
/// vacated by the symmetric rename, so no validation is needed and no `Err` is
/// silently dropped. After each direction the record-node pin layouts are
/// repaired (Helper 2 — the `Full` refresh does not) and the active record def
/// is remapped so the schema-editor selection follows the rename across
/// undo/redo. See `doc/design_hierarchical_records.md` (fixes #1, #3).
#[derive(Debug)]
pub struct RenameRecordTypeDefCommand {
    pub old_name: String,
    pub new_name: String,
}

impl RenameRecordTypeDefCommand {
    fn apply(&self, ctx: &mut UndoContext, from: &str, to: &str) {
        ctx.node_type_registry
            .rename_record_type_def_unchecked(from, to);
        ctx.node_type_registry.repair_all_networks();
        if ctx.active_record_def_name.as_deref() == Some(from) {
            *ctx.active_record_def_name = Some(to.to_string());
        }
    }
}

impl UndoCommand for RenameRecordTypeDefCommand {
    fn description(&self) -> &str {
        "Rename record type def"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, &self.new_name, &self.old_name);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, &self.old_name, &self.new_name);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
