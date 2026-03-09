use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing factor-selection-into-subnetwork operations.
///
/// Factoring creates a new subnetwork from selected nodes and replaces them
/// with a single custom node. Both the source network and the new subnetwork
/// are snapshotted for full restoration on undo.
pub struct FactorSelectionCommand {
    pub source_network_name: String,
    pub subnetwork_name: String,
    /// Snapshot of the source network before factoring
    pub source_network_before: SerializableNodeNetwork,
    /// Snapshot of the source network after factoring
    pub source_network_after: SerializableNodeNetwork,
    /// The newly created subnetwork
    pub subnetwork_snapshot: SerializableNodeNetwork,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug
impl std::fmt::Debug for FactorSelectionCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FactorSelectionCommand")
            .field("source_network_name", &self.source_network_name)
            .field("subnetwork_name", &self.subnetwork_name)
            .finish()
    }
}

impl FactorSelectionCommand {
    /// Replace a network in the registry with one deserialized from a snapshot.
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

impl UndoCommand for FactorSelectionCommand {
    fn description(&self) -> &str {
        "Factor selection into subnetwork"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Remove the subnetwork
        ctx.node_type_registry
            .node_networks
            .remove(&self.subnetwork_name);

        // Restore the source network to its pre-factoring state
        Self::restore_network(ctx, &self.source_network_name, &self.source_network_before);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        // Re-add the subnetwork
        Self::restore_network(ctx, &self.subnetwork_name, &self.subnetwork_snapshot);

        // Restore the source network to its post-factoring state
        Self::restore_network(ctx, &self.source_network_name, &self.source_network_after);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
