use crate::structure_designer::node_type_registry::{RecordField, RecordFieldEdit};
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
///
/// The command round-trips field **identity** (R2 of
/// `doc/design_record_field_identity.md`): it stores the exact pre-update
/// `RecordField` list (with `FieldId`s) plus `next_field_id`, and the
/// identity-aware [`RecordFieldEdit`] list applied. Undo restores the field
/// list/counter verbatim; redo replays the same id-aware edit (so renames and
/// preserved ids reproduce exactly) and re-keys `record_construct` literals.
pub struct UpdateRecordTypeDefCommand {
    pub name: String,
    /// Exact pre-update field list (with `FieldId`s), restored verbatim on undo.
    pub old_fields: Vec<RecordField>,
    /// Pre-update allocator floor, restored verbatim on undo.
    pub old_next_field_id: u64,
    /// The identity-aware edit applied, replayed on redo. Redo recomputes the
    /// literal-rename map from this edit (undo restores literals via the network
    /// snapshots).
    pub new_edits: Vec<RecordFieldEdit>,
    /// Snapshot of every network before the update, so undo can restore the
    /// disconnected wires and the pre-rename literal keys. See
    /// `DeleteRecordTypeDefCommand` for the same pattern.
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
        // Restore the def's exact pre-update field list and allocator floor
        // (ids round-trip verbatim — R2).
        if let Some(def) = ctx.node_type_registry.record_type_defs.get_mut(&self.name) {
            def.fields = self.old_fields.clone();
            def.next_field_id = self.old_next_field_id;
        }
        // Restore each affected network from its pre-update snapshot — this also
        // restores `record_construct` literal keys to their pre-rename names.
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
        // Base state is the restored-old def (set by the preceding undo). Replay
        // the identity-aware edit so field ids / renames reproduce exactly,
        // re-key literals, then repair every network so wires re-preserve /
        // -disconnect identically to the original update.
        if let Ok(renames) = ctx
            .node_type_registry
            .update_record_type_def_with_edits(&self.name, self.new_edits.clone())
        {
            ctx.node_type_registry
                .rekey_record_construct_literals(&self.name, &renames);
        }
        ctx.node_type_registry.repair_all_networks();
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
