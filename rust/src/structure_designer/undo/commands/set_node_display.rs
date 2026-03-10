use crate::structure_designer::node_network::NodeDisplayType;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing toggling a node's display state.
#[derive(Debug)]
pub struct SetNodeDisplayCommand {
    pub network_name: String,
    pub node_id: u64,
    pub old_display_type: Option<NodeDisplayType>,
    pub new_display_type: Option<NodeDisplayType>,
    pub description: String,
}

impl UndoCommand for SetNodeDisplayCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            match self.old_display_type {
                Some(dt) => {
                    network.displayed_node_ids.insert(self.node_id, dt);
                }
                None => {
                    network.displayed_node_ids.remove(&self.node_id);
                }
            }
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            match self.new_display_type {
                Some(dt) => {
                    network.displayed_node_ids.insert(self.node_id, dt);
                }
                None => {
                    network.displayed_node_ids.remove(&self.node_id);
                }
            }
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Lightweight
    }
}
