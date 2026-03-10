use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing setting the return node.
#[derive(Debug)]
pub struct SetReturnNodeCommand {
    pub network_name: String,
    pub old_return_node_id: Option<u64>,
    pub new_return_node_id: Option<u64>,
    pub description: String,
}

impl UndoCommand for SetReturnNodeCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            network.return_node_id = self.old_return_node_id;
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            network.return_node_id = self.new_return_node_id;
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
