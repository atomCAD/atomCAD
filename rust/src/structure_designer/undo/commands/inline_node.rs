use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing a top-level **Inline a Custom Node** operation.
///
/// Inlining mutates exactly one network (the parent that held the instance), so
/// — like [`super::text_edit_network::TextEditNetworkCommand`] — it stores
/// before/after snapshots of that whole network rather than fine-grained deltas.
/// Body-scoped inlining (non-empty `scope_path`) uses `EditZoneBodyCommand`
/// instead; see `doc/design_inline_custom_node.md` §"Undo".
pub struct InlineNodeCommand {
    pub network_name: String,
    /// Serialized parent-network state before the inline.
    pub before_snapshot: SerializableNodeNetwork,
    /// Serialized parent-network state after the inline.
    pub after_snapshot: SerializableNodeNetwork,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug.
impl std::fmt::Debug for InlineNodeCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InlineNodeCommand")
            .field("network_name", &self.network_name)
            .finish()
    }
}

impl InlineNodeCommand {
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

impl UndoCommand for InlineNodeCommand {
    fn description(&self) -> &str {
        "Inline custom node"
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
