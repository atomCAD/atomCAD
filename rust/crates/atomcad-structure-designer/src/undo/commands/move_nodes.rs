use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};
use glam::f64::DVec2;

/// Command for undoing/redoing node movement.
///
/// A single drag operation produces one MoveNodesCommand (via begin/end grouping),
/// not one per intermediate position.
#[derive(Debug)]
pub struct MoveNodesCommand {
    pub network_name: String,
    /// Scope of the body the moved nodes live in (empty = top-level network).
    /// Resolved via `ctx.network_in_scope_mut` so body-scope drags undo/redo
    /// against the right nested network.
    pub scope_path: Vec<u64>,
    /// (node_id, old_position, new_position)
    pub moves: Vec<(u64, DVec2, DVec2)>,
    pub description: String,
    /// `(node_id, hand_moved before this command)` for every node whose
    /// `hand_moved` flag this command sets. Populated **only** by the hand-drag
    /// path (`StructureDesigner::end_move_nodes`); every programmatic move —
    /// reflow, inlining, auto-layout — leaves it empty and so leaves the flags
    /// alone.
    ///
    /// The flag rides inside the existing move command rather than getting its
    /// own undo entry (`doc/design_incremental_layout.md`, "Uses of
    /// `hand_moved`"), so one Ctrl+Z takes back both the position and the
    /// "a human placed this" claim.
    pub hand_moved_before: Vec<(u64, bool)>,
}

impl UndoCommand for MoveNodesCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_in_scope_mut(&self.network_name, &self.scope_path) {
            for &(node_id, old_pos, _new_pos) in &self.moves {
                if let Some(node) = network.nodes.get_mut(&node_id) {
                    node.position = old_pos;
                }
            }
            for &(node_id, was_hand_moved) in &self.hand_moved_before {
                if let Some(node) = network.nodes.get_mut(&node_id) {
                    node.hand_moved = was_hand_moved;
                }
            }
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_in_scope_mut(&self.network_name, &self.scope_path) {
            for &(node_id, _old_pos, new_pos) in &self.moves {
                if let Some(node) = network.nodes.get_mut(&node_id) {
                    node.position = new_pos;
                }
            }
            for &(node_id, _was_hand_moved) in &self.hand_moved_before {
                if let Some(node) = network.nodes.get_mut(&node_id) {
                    node.hand_moved = true;
                }
            }
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Lightweight
    }
}
