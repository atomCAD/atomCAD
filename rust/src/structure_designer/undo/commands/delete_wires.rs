use crate::structure_designer::node_network::FUNCTION_PIN_INDEX;
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
                if let Some(dest_node) = network.nodes.get_mut(&wire.dest_node_id)
                    && let Some(arg) = dest_node.arguments.get_mut(wire.dest_param_index)
                {
                    arg.set_source(wire.source_node_id, wire.source_output_pin_index);
                }
            }
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        // Remove the wires
        if let Some(network) = ctx.network_mut(&self.network_name) {
            for wire in &self.deleted_wires {
                if let Some(dest_node) = network.nodes.get_mut(&wire.dest_node_id)
                    && let Some(arg) = dest_node.arguments.get_mut(wire.dest_param_index)
                {
                    arg.remove_source(wire.source_node_id);
                }
            }
        }
    }

    /// Each dest node's data changed — except when a **function pin** (`-1`)
    /// wire is among the deleted ones, which toggles its source node's
    /// consumption and needs an unconditional revalidate. See
    /// `ConnectWireCommand::refresh_mode` for why the `NodeDataChanged` arm's
    /// conditional revalidation cannot cover that case.
    fn refresh_mode(&self) -> UndoRefreshMode {
        if self
            .deleted_wires
            .iter()
            .any(|w| w.source_output_pin_index == FUNCTION_PIN_INDEX)
        {
            return UndoRefreshMode::Full;
        }
        let node_ids: Vec<u64> = self.deleted_wires.iter().map(|w| w.dest_node_id).collect();
        UndoRefreshMode::NodeDataChanged(node_ids)
    }
}
