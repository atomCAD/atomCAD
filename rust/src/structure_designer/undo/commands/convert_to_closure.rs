use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing a top-level **Convert to Closure** operation
/// (*Network → Closure*).
///
/// The conversion mutates exactly one network (the host that held the instance)
/// — the custom-network definition `N` is left untouched — so, like
/// [`super::inline_node::InlineNodeCommand`], it stores before/after snapshots
/// of that whole network rather than fine-grained deltas. Body-scoped conversion
/// (non-empty `scope_path`) uses `EditZoneBodyCommand` instead (a later phase);
/// see `doc/design_closure_network_conversion.md` §"Undo (Network → Closure)".
pub struct ConvertToClosureCommand {
    pub network_name: String,
    /// Serialized host-network state before the conversion.
    pub before_snapshot: SerializableNodeNetwork,
    /// Serialized host-network state after the conversion.
    pub after_snapshot: SerializableNodeNetwork,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug.
impl std::fmt::Debug for ConvertToClosureCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConvertToClosureCommand")
            .field("network_name", &self.network_name)
            .finish()
    }
}

impl ConvertToClosureCommand {
    /// Replace a network in the registry with one deserialized from a snapshot.
    ///
    /// Unlike [`super::inline_node::InlineNodeCommand`] (whose content lands at
    /// top level), this conversion buries a copied body inside the new `closure`
    /// node, so the deserialized network's **body-node** caches must be
    /// repopulated — otherwise a restored closure body's `expr` (etc.) has an
    /// empty parameter list and the next evaluation panics. Mirrors
    /// `EditZoneBodyCommand::restore`.
    fn restore_network(ctx: &mut UndoContext, name: &str, snapshot: &SerializableNodeNetwork) {
        if let Ok(mut network) = serializable_to_node_network(
            snapshot,
            &ctx.node_type_registry.built_in_node_types,
            None,
        ) {
            ctx.node_type_registry
                .initialize_custom_node_types_for_network(&mut network);
            ctx.node_type_registry
                .node_networks
                .insert(name.to_string(), network);
        }
    }
}

impl UndoCommand for ConvertToClosureCommand {
    fn description(&self) -> &str {
        "Convert to closure"
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
