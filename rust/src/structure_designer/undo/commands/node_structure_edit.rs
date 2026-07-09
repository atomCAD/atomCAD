use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing a **structural node edit** whose fallout reaches
/// beyond the edited node's own data blob — a variadic-pin-list change that
/// drops or remaps external/body wires, a retype that revalidation prunes, etc.
///
/// The generic node-data snapshot (`SetNodeDataCommand`) captures only the
/// `node_data_saver` blob — no `arguments`, no zone body — so it cannot restore
/// that wire fallout. Like `TextEditNetworkCommand`, this instead stores
/// before/after snapshots of the owning **top-level** network; bodies travel
/// inside their HOF nodes, so body-internal edits are covered.
///
/// Shared by `zip_with` lane edits (`doc/design_zip_with.md` Phase 3) and
/// `switch` case edits (`doc/design_switch_node.md` Phase 2); `description`
/// distinguishes them in the undo history.
pub struct NodeStructureEditCommand {
    pub network_name: String,
    /// Human-readable label shown in the undo history (e.g. "Edit switch
    /// cases", "Edit zip_with lanes").
    pub description: String,
    /// Serialized network state before the edit.
    pub before_snapshot: SerializableNodeNetwork,
    /// Serialized network state after the edit.
    pub after_snapshot: SerializableNodeNetwork,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug.
impl std::fmt::Debug for NodeStructureEditCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeStructureEditCommand")
            .field("network_name", &self.network_name)
            .field("description", &self.description)
            .finish()
    }
}

impl NodeStructureEditCommand {
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

impl UndoCommand for NodeStructureEditCommand {
    fn description(&self) -> &str {
        &self.description
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
