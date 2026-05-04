use crate::structure_designer::node_type_registry::RecordTypeDef;
use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, node_network_to_serializable, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing the deletion of a record type def.
///
/// On delete: the def is removed and `repair_node_network` is called on every
/// affected network so wires whose source/dest type became dangling are
/// disconnected. Undo restores the def *and* every affected network back to
/// its pre-delete shape (so the disconnected wires reappear).
pub struct DeleteRecordTypeDefCommand {
    pub def: RecordTypeDef,
    /// Snapshot of every network affected by the delete (i.e., every network
    /// whose `repair_node_network` ran). Stored as `SerializableNodeNetwork`
    /// because `NodeNetwork` doesn't derive `Clone` cheaply and the
    /// serialization round-trip is the existing canonical "full snapshot".
    pub affected_network_snapshots: Vec<(String, SerializableNodeNetwork)>,
}

impl std::fmt::Debug for DeleteRecordTypeDefCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeleteRecordTypeDefCommand")
            .field("def", &self.def.name)
            .field(
                "affected_networks",
                &self
                    .affected_network_snapshots
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl UndoCommand for DeleteRecordTypeDefCommand {
    fn description(&self) -> &str {
        "Delete record type def"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Restore the def first so any references resolve correctly.
        ctx.node_type_registry
            .record_type_defs
            .insert(self.def.name.clone(), self.def.clone());

        // Restore every affected network from its snapshot.
        for (name, snapshot) in &self.affected_network_snapshots {
            if let Ok(network) = serializable_to_node_network(
                snapshot,
                &ctx.node_type_registry.built_in_node_types,
                None,
            ) {
                ctx.node_type_registry
                    .node_networks
                    .insert(name.clone(), network);
            }
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        // Re-delete the def, then re-run repair on every previously-affected
        // network so wires depending on the now-dangling reference are
        // re-disconnected.
        ctx.node_type_registry
            .record_type_defs
            .remove(&self.def.name);

        // Snapshot built-ins out before mutating networks (avoid borrow conflict).
        let network_names: Vec<String> = self
            .affected_network_snapshots
            .iter()
            .map(|(n, _)| n.clone())
            .collect();

        for name in network_names {
            if let Some(mut network) = ctx.node_type_registry.node_networks.remove(&name) {
                ctx.node_type_registry.repair_node_network(&mut network);
                ctx.node_type_registry
                    .node_networks
                    .insert(name, network);
            }
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
