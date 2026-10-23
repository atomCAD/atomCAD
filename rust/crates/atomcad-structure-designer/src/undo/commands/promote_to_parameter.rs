use crate::serialization::node_networks_serialization::SerializableNodeNetwork;
use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing the "Promote to Parameter" operation.
///
/// The operation may insert a parameter node, rewire downstream consumers,
/// re-point the network's return node, and bump `next_node_id` / `next_param_id`.
/// Snapshotting the full network keeps the undo/redo atomic regardless of which
/// of those side effects fired.
pub struct PromoteToParameterCommand {
    pub network_name: String,
    pub network_before: SerializableNodeNetwork,
    pub network_after: SerializableNodeNetwork,
}

impl std::fmt::Debug for PromoteToParameterCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromoteToParameterCommand")
            .field("network_name", &self.network_name)
            .finish()
    }
}

impl UndoCommand for PromoteToParameterCommand {
    fn description(&self) -> &str {
        "Promote to parameter"
    }

    /// Keeps the live interface
    /// ([`UndoContext::restore_network_keeping_interface`]), so the refresh's
    /// validation sees the parameter come and go and repairs the callers: an
    /// undo takes the promoted parameter's slot away from every instance.
    /// Nothing is captured for the callers, since a promote only adds a
    /// parameter, and the slot it adds is empty.
    fn undo(&self, ctx: &mut UndoContext) {
        ctx.restore_network_keeping_interface(&self.network_name, &self.network_before);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        ctx.restore_network_keeping_interface(&self.network_name, &self.network_after);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
