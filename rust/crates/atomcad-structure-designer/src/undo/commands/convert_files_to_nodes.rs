use crate::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing **Convert to nodes** on a `mechanosynth` node —
/// the one-shot migration from the deprecated `ops_file` / `build_file`
/// properties to wired `ops_library` / `build_script` nodes
/// (`doc/design_mechanosynth_editor.md` Phase 1).
///
/// Like [`super::inline_node::InlineNodeCommand`] it is graph surgery that
/// adds nodes, adds wires and rewrites node data in one go, so it stores
/// before/after snapshots of the whole network rather than fine-grained
/// deltas. Body-scoped conversion uses `EditZoneBodyCommand` instead, exactly
/// as inlining does.
pub struct ConvertFilesToNodesCommand {
    pub network_name: String,
    pub before_snapshot: SerializableNodeNetwork,
    pub after_snapshot: SerializableNodeNetwork,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug.
impl std::fmt::Debug for ConvertFilesToNodesCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConvertFilesToNodesCommand")
            .field("network_name", &self.network_name)
            .finish()
    }
}

impl ConvertFilesToNodesCommand {
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

impl UndoCommand for ConvertFilesToNodesCommand {
    fn description(&self) -> &str {
        "Convert files to nodes"
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
