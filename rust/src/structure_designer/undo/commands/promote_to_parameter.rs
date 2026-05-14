use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

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

impl PromoteToParameterCommand {
    fn restore_network(ctx: &mut UndoContext, name: &str, snapshot: &SerializableNodeNetwork) {
        if let Ok(network) = serializable_to_node_network(
            snapshot,
            &ctx.node_type_registry.built_in_node_types,
            None,
        ) {
            ctx.node_type_registry
                .node_networks
                .insert(name.to_string(), network);
        }
    }
}

impl UndoCommand for PromoteToParameterCommand {
    fn description(&self) -> &str {
        "Promote to parameter"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        Self::restore_network(ctx, &self.network_name, &self.network_before);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        Self::restore_network(ctx, &self.network_name, &self.network_after);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
