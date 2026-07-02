use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing `zip_with` lane-list edits (add / remove /
/// retype — `doc/design_zip_with.md` Phase 3).
///
/// Lane edits are not pure node-data edits: removal and retype also drop or
/// remap wires (the removed lane's external wire, body wires including nested
/// bodies, retype-incompatible body wires), and the generic node-data snapshot
/// captures only the `node_data_saver` blob — no `arguments`, no zone body.
/// So, like `TextEditNetworkCommand`, this stores before/after snapshots of
/// the owning **top-level** network — bodies travel inside their HOF nodes, so
/// body-internal zip nodes are covered.
pub struct ZipWithLaneEditCommand {
    pub network_name: String,
    /// Serialized network state before the lane edit
    pub before_snapshot: SerializableNodeNetwork,
    /// Serialized network state after the lane edit
    pub after_snapshot: SerializableNodeNetwork,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug
impl std::fmt::Debug for ZipWithLaneEditCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZipWithLaneEditCommand")
            .field("network_name", &self.network_name)
            .finish()
    }
}

impl ZipWithLaneEditCommand {
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

impl UndoCommand for ZipWithLaneEditCommand {
    fn description(&self) -> &str {
        "Edit zip_with lanes"
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
