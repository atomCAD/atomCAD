use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing text-based network edits.
///
/// Text edits can make arbitrary changes to a network. Rather than decomposing
/// into fine-grained commands, we store before/after snapshots of the entire network.
pub struct TextEditNetworkCommand {
    pub network_name: String,
    /// Serialized network state before the text edit
    pub before_snapshot: SerializableNodeNetwork,
    /// Serialized network state after the text edit
    pub after_snapshot: SerializableNodeNetwork,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug
impl std::fmt::Debug for TextEditNetworkCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextEditNetworkCommand")
            .field("network_name", &self.network_name)
            .finish()
    }
}

impl TextEditNetworkCommand {
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

impl UndoCommand for TextEditNetworkCommand {
    fn description(&self) -> &str {
        "Text edit network"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        Self::restore_network(ctx, &self.network_name, &self.before_snapshot);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        Self::restore_network(ctx, &self.network_name, &self.after_snapshot);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
