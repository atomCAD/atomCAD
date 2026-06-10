use crate::structure_designer::node_type_registry::RecordTypeDef;
use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing namespace deletion — a batch delete of every
/// user type (networks AND user record defs) under a prefix.
pub struct DeleteNamespaceCommand {
    /// Snapshots of all deleted networks (for undo restoration).
    pub network_snapshots: Vec<(String, SerializableNodeNetwork)>,
    /// Snapshots of all deleted user record defs (for undo restoration).
    pub record_snapshots: Vec<(String, RecordTypeDef)>,
    pub active_network_before: Option<String>,
    pub active_network_after: Option<String>,
    pub active_record_def_before: Option<String>,
    pub active_record_def_after: Option<String>,
    /// Empty-folder markers removed by this delete (the folder itself if empty,
    /// plus any empty subfolders under the prefix); restored on undo. See
    /// `doc/design_empty_folders.md`.
    pub folder_markers: Vec<String>,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug.
impl std::fmt::Debug for DeleteNamespaceCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeleteNamespaceCommand")
            .field(
                "network_names",
                &self
                    .network_snapshots
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>(),
            )
            .field(
                "record_names",
                &self
                    .record_snapshots
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>(),
            )
            .field("active_network_before", &self.active_network_before)
            .field("active_network_after", &self.active_network_after)
            .finish()
    }
}

impl UndoCommand for DeleteNamespaceCommand {
    fn description(&self) -> &str {
        "Delete namespace"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Restore deleted record defs first so any references resolve.
        for (name, def) in &self.record_snapshots {
            ctx.node_type_registry
                .record_type_defs
                .insert(name.clone(), def.clone());
        }
        // Restore all deleted networks from snapshots.
        for (_name, snapshot) in &self.network_snapshots {
            if let Ok(network) = serializable_to_node_network(
                snapshot,
                &ctx.node_type_registry.built_in_node_types,
                None,
            ) {
                ctx.node_type_registry.add_node_network(network);
            }
        }
        // Re-introducing a `Named` target means wires that were disconnected on
        // delete must be re-validated and record-node pin layouts refreshed
        // (Helper 2 — the `Full` refresh does not do this).
        if !self.record_snapshots.is_empty() {
            ctx.node_type_registry.repair_all_networks();
        }
        // Restore removed empty-folder markers.
        for m in &self.folder_markers {
            ctx.node_type_registry.folders.insert(m.clone());
        }
        *ctx.active_network_name = self.active_network_before.clone();
        *ctx.active_record_def_name = self.active_record_def_before.clone();
    }

    fn redo(&self, ctx: &mut UndoContext) {
        // Re-delete all networks and record defs.
        for (name, _snapshot) in &self.network_snapshots {
            ctx.node_type_registry.node_networks.remove(name);
        }
        for (name, _def) in &self.record_snapshots {
            ctx.node_type_registry.record_type_defs.remove(name);
        }
        if !self.record_snapshots.is_empty() {
            ctx.node_type_registry.repair_all_networks();
        }
        // Re-remove the empty-folder markers.
        for m in &self.folder_markers {
            ctx.node_type_registry.folders.remove(m);
        }
        *ctx.active_network_name = self.active_network_after.clone();
        *ctx.active_record_def_name = self.active_record_def_after.clone();
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
