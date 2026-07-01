use crate::structure_designer::node_type_registry::RecordTypeDef;
use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, node_network_to_serializable,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing the deletion of a record type def.
///
/// `StructureDesigner::delete_record_type_def` blocks deletion while any entity
/// still references the def, so a successful delete never disconnects a wire or
/// dangles another def — the command only has to remove/re-insert the def (and
/// track the active-record-def selection). No per-network snapshot is needed.
pub struct DeleteRecordTypeDefCommand {
    pub def: RecordTypeDef,
    /// Whether the deleted def was the active record def at delete time, so
    /// redo can clear and undo can restore the schema-editor selection
    /// (parity with the network active-name handling). See
    /// `doc/design_hierarchical_records.md` §8.
    pub was_active: bool,
}

impl std::fmt::Debug for DeleteRecordTypeDefCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeleteRecordTypeDefCommand")
            .field("def", &self.def.name)
            .finish()
    }
}

impl UndoCommand for DeleteRecordTypeDefCommand {
    fn description(&self) -> &str {
        "Delete record type def"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Restore the def. Nothing referenced it at delete time (the delete was
        // gated on that), so no network needs repair — restoring the def alone
        // reproduces the pre-delete state.
        ctx.node_type_registry
            .record_type_defs
            .insert(self.def.name.clone(), self.def.clone());

        // Restore the active record def if the deleted def was active.
        if self.was_active {
            *ctx.active_record_def_name = Some(self.def.name.clone());
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        ctx.node_type_registry
            .record_type_defs
            .remove(&self.def.name);

        // Clear the active record def if it was the deleted def.
        if self.was_active && ctx.active_record_def_name.as_deref() == Some(self.def.name.as_str())
        {
            *ctx.active_record_def_name = None;
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}

/// Helper used by `StructureDesigner::delete_record_type_def` to snapshot
/// every affected network *before* repair runs. The "affected" set is every
/// network in the registry — references to record defs can be deeply nested
/// inside data types, so a conservative full snapshot is the simplest correct
/// thing. (For typical projects this is a small handful of networks.)
pub fn snapshot_all_networks_for_record_def_change(
    registry: &mut crate::structure_designer::node_type_registry::NodeTypeRegistry,
) -> Vec<(String, SerializableNodeNetwork)> {
    let names: Vec<String> = registry.node_networks.keys().cloned().collect();
    let mut snapshots = Vec::with_capacity(names.len());
    let built_in_types = &registry.built_in_node_types;
    for name in names {
        // Take ownership briefly so we can borrow built_in_types immutably and
        // network mutably (the serializer needs mutable access to refresh some
        // node-data caches before saving).
        if let Some(mut network) = registry.node_networks.remove(&name) {
            if let Ok(snap) = node_network_to_serializable(&mut network, built_in_types, None) {
                snapshots.push((name.clone(), snap));
            }
            registry.node_networks.insert(name, network);
        }
    }
    snapshots
}
