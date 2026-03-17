use crate::structure_designer::undo::snapshot::WireSnapshot;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing deletion of wires (when only wires are selected, not nodes).
#[derive(Debug)]
pub struct DeleteWiresCommand {
    pub network_name: String,
    pub deleted_wires: Vec<WireSnapshot>,
}

impl UndoCommand for DeleteWiresCommand {
    fn description(&self) -> &str {
        "Delete wires"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Re-add the wires
        if let Some(network) = ctx.network_mut(&self.network_name) {
            for wire in &self.deleted_wires {
                if let Some(dest_node) = network.nodes.get_mut(&wire.dest_node_id) {
                    if let Some(arg) = dest_node.arguments.get_mut(wire.dest_param_index) {
                        arg.argument_output_pins
                            .insert(wire.source_node_id, wire.source_output_pin_index);
                    }
                }
            }
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        // Remove the wires
        if let Some(network) = ctx.network_mut(&self.network_name) {
            for wire in &self.deleted_wires {
                if let Some(dest_node) = network.nodes.get_mut(&wire.dest_node_id) {
                    if let Some(arg) = dest_node.arguments.get_mut(wire.dest_param_index) {
                        arg.argument_output_pins.remove(&wire.source_node_id);
                    }
                }
            }
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        let node_ids: Vec<u64> = self.deleted_wires.iter().map(|w| w.dest_node_id).collect();
        UndoRefreshMode::NodeDataChanged(node_ids)
    }
}
