use crate::structure_designer::node_network::NodeDisplayState;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing toggling an output pin's display state.
///
/// Stores the full `NodeDisplayState` before and after the change, so that
/// undo/redo restores the exact pin set and display type atomically.
#[derive(Debug)]
pub struct SetOutputPinDisplayCommand {
    pub network_name: String,
    pub node_id: u64,
    pub old_display_state: Option<NodeDisplayState>,
    pub new_display_state: Option<NodeDisplayState>,
    pub description: String,
}

impl UndoCommand for SetOutputPinDisplayCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            match &self.old_display_state {
                Some(state) => {
                    network.displayed_nodes.insert(self.node_id, state.clone());
                }
                None => {
                    network.displayed_nodes.remove(&self.node_id);
                }
            }
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            match &self.new_display_state {
                Some(state) => {
                    network.displayed_nodes.insert(self.node_id, state.clone());
                }
                None => {
                    network.displayed_nodes.remove(&self.node_id);
                }
            }
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Lightweight
    }
}
