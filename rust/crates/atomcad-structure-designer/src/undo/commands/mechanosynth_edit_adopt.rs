use crate::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing **Adopt these into the block** on a
/// `mechanosynth_edit` node — the one-shot copy of the wired prefix into the
/// authored block, which also disconnects the `steps` pin
/// (`doc/design_mechanosynth_editor.md` §"Adopting the prefix").
///
/// Like [`super::convert_files_to_nodes::ConvertFilesToNodesCommand`] it
/// rewrites node data *and* removes a wire in one go, so it stores
/// before/after snapshots of the whole network rather than fine-grained
/// deltas — [`super::mechanosynth_edit_block::MechanosynthEditBlockCommand`]
/// covers the block alone and cannot restore the wire. Body-scoped adoption
/// uses `EditZoneBodyCommand` instead, exactly as the conversion does.
pub struct MechanosynthEditAdoptCommand {
    pub network_name: String,
    pub before_snapshot: SerializableNodeNetwork,
    pub after_snapshot: SerializableNodeNetwork,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug.
impl std::fmt::Debug for MechanosynthEditAdoptCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MechanosynthEditAdoptCommand")
            .field("network_name", &self.network_name)
            .finish()
    }
}

impl MechanosynthEditAdoptCommand {
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

impl UndoCommand for MechanosynthEditAdoptCommand {
    fn description(&self) -> &str {
        "Adopt steps into the block"
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
