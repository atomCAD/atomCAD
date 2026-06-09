use crate::structure_designer::data_type::DataType;
use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing a record type def field-list update.
///
/// On update, the registry's def is replaced with the new field list and
/// `repair_node_network` runs on every affected network so wires whose source
/// type no longer satisfies the new field type are disconnected. Undo restores
/// the def *and* the affected networks back to their pre-update shape.
pub struct UpdateRecordTypeDefCommand {
    pub name: String,
    pub old_fields: Vec<(String, DataType)>,
    pub new_fields: Vec<(String, DataType)>,
    /// Snapshot of every network before the update, so undo can restore the
    /// disconnected wires. See `DeleteRecordTypeDefCommand` for the same
    /// pattern.
    pub network_snapshots_before: Vec<(String, SerializableNodeNetwork)>,
}

impl std::fmt::Debug for UpdateRecordTypeDefCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateRecordTypeDefCommand")
            .field("name", &self.name)
            .field(
                "affected_networks",
                &self
                    .network_snapshots_before
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl UndoCommand for UpdateRecordTypeDefCommand {
    fn description(&self) -> &str {
        "Update record type def"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Restore the def's old field list.
        if let Some(def) = ctx.node_type_registry.record_type_defs.get_mut(&self.name) {
            def.fields = self.old_fields.clone();
        }
        // Restore each affected network from its pre-update snapshot.
        for (network_name, snapshot) in &self.network_snapshots_before {
            if let Ok(network) = serializable_to_node_network(
                snapshot,
                &ctx.node_type_registry.built_in_node_types,
                None,
            ) {
                ctx.node_type_registry
                    .node_networks
                    .insert(network_name.clone(), network);
            }
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        // Apply the new field list and re-run repair on every affected
        // network so wires re-disconnect identically to the original update.
        if let Some(def) = ctx.node_type_registry.record_type_defs.get_mut(&self.name) {
            def.fields = self.new_fields.clone();
        }
        // Helper 2 — repair every network so record-node pin layouts re-derive
        // and now-incompatible wires re-disconnect (matches the forward update).
        ctx.node_type_registry.repair_all_networks();
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
