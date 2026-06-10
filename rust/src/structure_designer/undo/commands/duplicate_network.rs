use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing node-network duplication.
///
/// Duplication is a *shallow* copy: the new network's full content — including
/// every inline HOF / closure zone body, recursively — is stored as a
/// `SerializableNodeNetwork` snapshot, while references to *other* named
/// networks remain references (the referenced networks are not themselves
/// duplicated). On undo the copy is removed; on redo it is re-materialized from
/// the snapshot. Mirrors `DeleteNetworkCommand` (inverted) and `AddNetworkCommand`
/// (folder restoration).
pub struct DuplicateNetworkCommand {
    /// The name of the newly created copy.
    pub network_name: String,
    /// Full serialized copy for re-creation on redo.
    pub network_snapshot: SerializableNodeNetwork,
    /// The active network before duplication (restored on undo).
    pub previous_active_network: Option<String>,
    /// Ancestor empty-folder markers absorbed (pruned) when the copy was
    /// created; restored on undo so any empty folder it filled reappears.
    /// See `doc/design_empty_folders.md`.
    pub pruned_folders: Vec<String>,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug.
impl std::fmt::Debug for DuplicateNetworkCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DuplicateNetworkCommand")
            .field("network_name", &self.network_name)
            .field("previous_active_network", &self.previous_active_network)
            .finish()
    }
}

impl UndoCommand for DuplicateNetworkCommand {
    fn description(&self) -> &str {
        "Duplicate network"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Remove the copy.
        ctx.node_type_registry
            .node_networks
            .remove(&self.network_name);

        // Restore any empty-folder markers the copy's creation absorbed.
        for f in &self.pruned_folders {
            ctx.node_type_registry.folders.insert(f.clone());
        }

        // Restore previous active network.
        *ctx.active_network_name = self.previous_active_network.clone();
    }

    fn redo(&self, ctx: &mut UndoContext) {
        // Deserialize and re-add the copy. Re-run
        // `initialize_custom_node_types_for_network` so per-node custom-type
        // caches (incl. nodes inside HOF / closure bodies) are repopulated —
        // mirrors `FactorSelectionCommand::restore_network`.
        if let Ok(mut network) = serializable_to_node_network(
            &self.network_snapshot,
            &ctx.node_type_registry.built_in_node_types,
            None,
        ) {
            ctx.node_type_registry
                .initialize_custom_node_types_for_network(&mut network);
            ctx.node_type_registry.add_node_network(network);
        }

        // Switch active to the new copy.
        *ctx.active_network_name = Some(self.network_name.clone());
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
