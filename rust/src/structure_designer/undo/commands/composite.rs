use crate::structure_designer::undo::{
    UndoCommand, UndoContext, UndoRefreshMode, combine_refresh_modes,
};

/// Bundles N child commands into a single undo step.
///
/// The "safety valve for future compound operations" foreshadowed by
/// `undo/AGENTS.md`. Used when one user-visible edit must record several
/// commands that undo/redo as one step — e.g. a structural edit plus the
/// `MoveNodesCommand`s that reflowed neighbours out of the grown node's way
/// (see `doc/design_reflow_on_footprint_change.md`).
///
/// `undo` runs children in **reverse**, `redo` in **forward** order — the
/// standard composite convention. In practice `MoveNodesCommand` sets
/// *absolute* positions, so its order relative to the primary command is
/// immaterial; reverse-on-undo is kept for correctness with any future
/// order-dependent child.
///
/// A composite with a single child should never be constructed — callers push
/// the bare child when reflow produced no moves.
#[derive(Debug)]
pub struct CompositeCommand {
    pub commands: Vec<Box<dyn UndoCommand>>,
    pub description: String,
}

impl UndoCommand for CompositeCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        for command in self.commands.iter().rev() {
            command.undo(ctx);
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        for command in &self.commands {
            command.redo(ctx);
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        // Strongest child wins: Full > NodeDataChanged(∪ ids) > Lightweight.
        combine_refresh_modes(self.commands.iter().map(|c| c.refresh_mode()))
    }
}
