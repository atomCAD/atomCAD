use crate::structure_designer::undo::snapshot::WireSnapshot;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing a wire connection.
#[derive(Debug)]
pub struct ConnectWireCommand {
    pub network_name: String,
    pub wire: WireSnapshot,
    /// If the destination pin was not multi-valued, connecting may have
    /// replaced an existing wire. Store the replaced wire for undo.
    pub replaced_wire: Option<WireSnapshot>,
}

impl UndoCommand for ConnectWireCommand {
    fn description(&self) -> &str {
        "Connect wire"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            // Remove the wire we added
            if let Some(dest_node) = network.nodes.get_mut(&self.wire.dest_node_id) {
                if let Some(arg) = dest_node.arguments.get_mut(self.wire.dest_param_index) {
                    arg.argument_output_pins.remove(&self.wire.source_node_id);
                }
            }

            // Restore the replaced wire if there was one
            if let Some(replaced) = &self.replaced_wire {
                if let Some(dest_node) = network.nodes.get_mut(&replaced.dest_node_id) {
                    if let Some(arg) = dest_node.arguments.get_mut(replaced.dest_param_index) {
                        arg.argument_output_pins
                            .insert(replaced.source_node_id, replaced.source_output_pin_index);
                    }
                }
            }
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            // If not multi-valued, clear existing connections on this pin first
            if self.replaced_wire.is_some() {
                if let Some(dest_node) = network.nodes.get_mut(&self.wire.dest_node_id) {
                    if let Some(arg) = dest_node.arguments.get_mut(self.wire.dest_param_index) {
                        arg.argument_output_pins.clear();
                    }
                }
            }

            // Re-establish the wire
            if let Some(dest_node) = network.nodes.get_mut(&self.wire.dest_node_id) {
                if let Some(arg) = dest_node.arguments.get_mut(self.wire.dest_param_index) {
                    arg.argument_output_pins
                        .insert(self.wire.source_node_id, self.wire.source_output_pin_index);
                }
            }
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::NodeDataChanged(vec![self.wire.dest_node_id])
    }
}
