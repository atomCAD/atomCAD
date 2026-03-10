use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};
use glam::f64::DVec2;

/// Command for undoing/redoing node movement.
///
/// A single drag operation produces one MoveNodesCommand (via begin/end grouping),
/// not one per intermediate position.
#[derive(Debug)]
pub struct MoveNodesCommand {
    pub network_name: String,
    /// (node_id, old_position, new_position)
    pub moves: Vec<(u64, DVec2, DVec2)>,
    pub description: String,
}

impl UndoCommand for MoveNodesCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            for &(node_id, old_pos, _new_pos) in &self.moves {
                if let Some(node) = network.nodes.get_mut(&node_id) {
                    node.position = old_pos;
                }
            }
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            for &(node_id, _old_pos, new_pos) in &self.moves {
                if let Some(node) = network.nodes.get_mut(&node_id) {
                    node.position = new_pos;
                }
            }
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Lightweight
    }
}
