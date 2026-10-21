use crate::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Generic whole-network snapshot command: one undo step for any edit of a
/// network whose fallout reaches beyond one node's data blob — a variadic-pin
/// list change that drops or remaps external/body wires, a declared input
/// appended together with its wire, a retype that revalidation re-derives.
///
/// The generic node-data snapshot (`SetNodeDataCommand`) captures only the
/// `node_data_saver` blob — no `arguments`, no zone body — so it cannot restore
/// that wire fallout. Like `TextEditNetworkCommand`, this instead stores
/// before/after snapshots of the owning **top-level** network; bodies travel
/// inside their HOF nodes, so body-internal edits are covered, and redo never
/// depends on re-deriving a pin before re-adding its wire.
///
/// Shared by `zip_with` lane edits (`doc/design_zip_with.md` Phase 3),
/// `switch` case edits (`doc/design_switch_node.md` Phase 2) and the declared
/// input operations (`declared_input_ops.rs`, "Add input from wire" / "Take
/// type from wire"); `description` distinguishes them in the undo history.
pub struct NetworkSnapshotCommand {
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
impl std::fmt::Debug for NetworkSnapshotCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkSnapshotCommand")
            .field("network_name", &self.network_name)
            .field("description", &self.description)
            .finish()
    }
}

impl NetworkSnapshotCommand {
    /// Replace a network in the registry with one deserialized from a snapshot.
    fn restore_network(ctx: &mut UndoContext, name: &str, snapshot: &SerializableNodeNetwork) {
        if let Ok(network) = serializable_to_node_network(
            snapshot,
            &ctx.node_type_registry.built_in_node_types,
            None,
        ) {
            ctx.node_type_registry
                .replace_network_keeping_thumbnail(name.to_string(), network);
        }
    }
}

impl UndoCommand for NetworkSnapshotCommand {
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
