use crate::structure_designer::node_network::FUNCTION_PIN_INDEX;
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
            if let Some(dest_node) = network.nodes.get_mut(&self.wire.dest_node_id)
                && let Some(arg) = dest_node.arguments.get_mut(self.wire.dest_param_index)
            {
                arg.remove_source(self.wire.source_node_id);
            }

            // Restore the replaced wire if there was one
            if let Some(replaced) = &self.replaced_wire
                && let Some(dest_node) = network.nodes.get_mut(&replaced.dest_node_id)
                && let Some(arg) = dest_node.arguments.get_mut(replaced.dest_param_index)
            {
                arg.set_source(replaced.source_node_id, replaced.source_output_pin_index);
            }
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            // If not multi-valued, clear existing connections on this pin first
            if self.replaced_wire.is_some()
                && let Some(dest_node) = network.nodes.get_mut(&self.wire.dest_node_id)
                && let Some(arg) = dest_node.arguments.get_mut(self.wire.dest_param_index)
            {
                arg.clear();
            }

            // Re-establish the wire
            if let Some(dest_node) = network.nodes.get_mut(&self.wire.dest_node_id)
                && let Some(arg) = dest_node.arguments.get_mut(self.wire.dest_param_index)
            {
                arg.set_source(self.wire.source_node_id, self.wire.source_output_pin_index);
            }
        }
    }

    /// Ordinarily just the dest node's data changed (it gained/lost an input).
    ///
    /// A **function pin** (`-1`) wire is the exception: connecting/disconnecting
    /// it *toggles the source node's `function_pin_consumed` state*, which is
    /// type-visible (the consumer's derived `apply`/`map` layouts) and
    /// validation-visible (the role rules in
    /// `doc/design_function_pin_roles.md`, which are gated on consumption). The
    /// `NodeDataChanged` arm's conditional revalidation cannot cover this: it
    /// asks whether the listed nodes are consumed **after** the undo, so the
    /// leg that *removes* consumption always reads as "not consumed" and skips
    /// the very revalidation it needs — listing the source node alongside the
    /// dest does not help. `Full` re-validates unconditionally, matching what
    /// the forward connect/delete paths do on this same condition.
    fn refresh_mode(&self) -> UndoRefreshMode {
        if self.wire.source_output_pin_index == FUNCTION_PIN_INDEX {
            return UndoRefreshMode::Full;
        }
        UndoRefreshMode::NodeDataChanged(vec![self.wire.dest_node_id])
    }
}
