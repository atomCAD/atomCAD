use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing network deletion.
///
/// Stores the serialized network snapshot as a `SerializableNodeNetwork` so it
/// can be restored on undo.
pub struct DeleteNetworkCommand {
    pub network_name: String,
    /// Full serialized network for restoration
    pub network_snapshot: SerializableNodeNetwork,
    /// The active network before deletion (restored on undo)
    pub active_network_before: Option<String>,
    /// The active network after deletion (restored on redo)
    pub active_network_after: Option<String>,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug
impl std::fmt::Debug for DeleteNetworkCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeleteNetworkCommand")
            .field("network_name", &self.network_name)
            .field("active_network_before", &self.active_network_before)
            .field("active_network_after", &self.active_network_after)
            .finish()
    }
}

impl UndoCommand for DeleteNetworkCommand {
    fn description(&self) -> &str {
        "Delete network"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Deserialize and re-add the network
        if let Ok(network) = serializable_to_node_network(
            &self.network_snapshot,
            &ctx.node_type_registry.built_in_node_types,
            None,
        ) {
            ctx.node_type_registry.add_node_network(network);
        }

        // Restore active network to what it was before deletion
        *ctx.active_network_name = self.active_network_before.clone();
    }

    fn redo(&self, ctx: &mut UndoContext) {
        // Re-delete the network
        ctx.node_type_registry
            .node_networks
            .remove(&self.network_name);

        // Restore active network to what it was after deletion
        *ctx.active_network_name = self.active_network_after.clone();
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
