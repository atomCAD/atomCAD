use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing the creation of an empty folder marker.
/// Folders carry no wires or evaluation state, so this only mutates the
/// registry's `folders` set. See `doc/design_empty_folders.md`.
#[derive(Debug)]
pub struct AddFolderCommand {
    /// The full dot-delimited folder path that was created.
    pub path: String,
    /// Ancestor empty-folder markers that were pruned (absorbed) when this
    /// folder was created — restored on undo so the prior tree reappears.
    pub pruned_ancestors: Vec<String>,
}

impl UndoCommand for AddFolderCommand {
    fn description(&self) -> &str {
        "Add folder"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        ctx.node_type_registry.folders.remove(&self.path);
        for a in &self.pruned_ancestors {
            ctx.node_type_registry.folders.insert(a.clone());
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        for a in &self.pruned_ancestors {
            ctx.node_type_registry.folders.remove(a);
        }
        ctx.node_type_registry.folders.insert(self.path.clone());
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        // Folders don't affect evaluation; the Flutter side re-fetches the
        // folder list on every refresh regardless of mode.
        UndoRefreshMode::Lightweight
    }
}
