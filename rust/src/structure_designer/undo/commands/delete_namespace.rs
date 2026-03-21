use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing namespace deletion (batch delete of all networks under a prefix).
pub struct DeleteNamespaceCommand {
    /// Snapshots of all deleted networks (for undo restoration).
    pub network_snapshots: Vec<(String, SerializableNodeNetwork)>,
    pub active_network_before: Option<String>,
    pub active_network_after: Option<String>,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug
impl std::fmt::Debug for DeleteNamespaceCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeleteNamespaceCommand")
            .field(
                "network_names",
                &self
                    .network_snapshots
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>(),
            )
            .field("active_network_before", &self.active_network_before)
            .field("active_network_after", &self.active_network_after)
            .finish()
    }
}

impl UndoCommand for DeleteNamespaceCommand {
    fn description(&self) -> &str {
        "Delete namespace"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Restore all deleted networks from snapshots
        for (_name, snapshot) in &self.network_snapshots {
            if let Ok(network) = serializable_to_node_network(
                snapshot,
                &ctx.node_type_registry.built_in_node_types,
                None,
            ) {
                ctx.node_type_registry.add_node_network(network);
            }
        }
        *ctx.active_network_name = self.active_network_before.clone();
    }

    fn redo(&self, ctx: &mut UndoContext) {
        // Re-delete all networks
        for (name, _snapshot) in &self.network_snapshots {
            ctx.node_type_registry.node_networks.remove(name);
        }
        *ctx.active_network_name = self.active_network_after.clone();
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
