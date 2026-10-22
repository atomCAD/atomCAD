//! The structural edits of a `record_construct` (`doc/design_wire_declared_inputs.md`,
//! Phases 2 and 3): switching its schema (which also switches between named
//! and anonymous mode), editing an anonymous node's field list, and promoting
//! an anonymous node to a new named def ("Name this record…").
//!
//! The first two can drop wires — a switch drops the wires of the fields the new
//! schema lacks, a field delete drops that field's wire — and the wires go
//! when the pins are re-derived, not when the data is written. A
//! `SetNodeDataCommand` restores the data only, so its undo would bring a
//! field back without its wire. Each edit is therefore one
//! `NetworkSnapshotCommand`, like the open-port drop. Promotion drops no
//! wire, but it changes the registry and the network together, so it is a
//! `CompositeCommand` of `AddRecordTypeDefCommand` and that snapshot.
//!
//! This file is the node's own (`record_node_hook_test.rs` lists it): the
//! downcasts below reach `record_construct` alone and are not a record-node
//! list.

use crate::data_type::DataType;
use crate::node_type_registry::RecordTypeDef;
use crate::nodes::record_construct::RecordConstructData;
use crate::structure_designer::StructureDesigner;
use crate::undo::commands::add_record_type_def::AddRecordTypeDefCommand;
use crate::undo::commands::composite::CompositeCommand;
use std::collections::HashSet;

/// The undo-history label of a schema change.
pub const SET_RECORD_SCHEMA_DESCRIPTION: &str = "Set record schema";
/// The undo-history label of an edit of an anonymous record's field list.
pub const EDIT_RECORD_FIELDS_DESCRIPTION: &str = "Edit record fields";
/// The undo-history label of promoting an anonymous record to a named def.
pub const NAME_RECORD_DESCRIPTION: &str = "Name record";

impl StructureDesigner {
    /// The data of the `record_construct` node `node_id` in the scope, or
    /// `None` when there is no such node or it is another node type.
    pub fn record_construct_data(
        &self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Option<&RecordConstructData> {
        self.get_scope_network(scope_path)?
            .nodes
            .get(&node_id)?
            .data
            .as_any_ref()
            .downcast_ref::<RecordConstructData>()
    }

    /// Point a `record_construct` at the def `schema`, or at none (`""`,
    /// anonymous mode) — one undo step, "Set record schema". Local fields are
    /// cleared; wires and literals carry over by field name where the new
    /// schema has a same-named field, and the rest are dropped. Setting the
    /// current schema changes nothing and pushes no step.
    pub fn set_record_construct_schema(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        schema: &str,
    ) -> Result<(), String> {
        self.ensure_active_editable()?;
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or_else(|| "No active network".to_string())?;
        let current = self
            .record_construct_data(scope_path, node_id)
            .ok_or_else(|| "Not a record_construct node".to_string())?;
        if current.schema == schema {
            return Ok(());
        }
        let kept_literal_names: HashSet<String> = self
            .node_type_registry
            .lookup_record_type_def(schema)
            .map(|def| def.fields.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default();
        let mut data = current.clone();
        data.set_schema(schema.to_string(), &kept_literal_names);
        self.replace_record_construct_data(
            scope_path,
            node_id,
            &network_name,
            data,
            SET_RECORD_SCHEMA_DESCRIPTION,
        )
    }

    /// Replace an anonymous `record_construct`'s field list — one undo step,
    /// "Edit record fields". `fields` are the names and types in authored
    /// order; ids are kept by name, then position
    /// ([`RecordConstructData::set_local_fields`]), so a rename or a reorder
    /// keeps every wire and a delete drops only its own. Refused, with nothing
    /// changed, on a named-mode node, a read-only network, an invalid or
    /// duplicate name, or a type a field cannot have. An edit that changes
    /// nothing pushes no step.
    pub fn set_record_construct_fields(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        fields: Vec<(String, DataType)>,
    ) -> Result<(), String> {
        self.ensure_active_editable()?;
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or_else(|| "No active network".to_string())?;
        let current = self
            .record_construct_data(scope_path, node_id)
            .ok_or_else(|| "Not a record_construct node".to_string())?;
        let mut data = current.clone();
        data.set_local_fields(fields)?;
        if data.fields == current.fields && data.literal_values == current.literal_values {
            return Ok(());
        }
        self.replace_record_construct_data(
            scope_path,
            node_id,
            &network_name,
            data,
            EDIT_RECORD_FIELDS_DESCRIPTION,
        )
    }

    /// Every record def whose fields are **exactly** the anonymous node's:
    /// the same names with equal types, no `Optional` field and no extra
    /// field. These are the defs "Name this record…" offers to reuse instead
    /// of creating one. A wider def is never listed: its extra required
    /// fields would be unwired and the record would become `None`. User and
    /// linked defs are listed, built-in ones are not; sorted by name. Empty
    /// when the node is not an anonymous `record_construct` or has no fields
    /// (which promotion refuses too).
    pub fn matching_record_defs_for(&self, scope_path: &[u64], node_id: u64) -> Vec<String> {
        let Some(data) = self.record_construct_data(scope_path, node_id) else {
            return Vec::new();
        };
        if !data.is_anonymous() || data.fields.is_empty() {
            return Vec::new();
        }
        let mut names: Vec<String> = self
            .node_type_registry
            .record_type_defs
            .values()
            .filter(|def| {
                def.fields.len() == data.fields.len()
                    && def.fields.iter().all(|def_field| {
                        !def_field.data_type.is_optional()
                            && data.fields.iter().any(|local| {
                                local.name == def_field.name
                                    && local.data_type == def_field.data_type
                            })
                    })
            })
            .map(|def| def.name.clone())
            .collect();
        names.sort();
        names
    }

    /// "Name this record…": create the def `def_name` from an anonymous
    /// `record_construct`'s fields (authored order, same types, no hints,
    /// fresh `FieldId`s) and point the node at it — one undo step, "Name
    /// record". The pins re-derive with the def's `FieldId`s, none of which
    /// can equal an anonymous pin id, so every wire carries over by name;
    /// literals are keyed by name and stay. Downstream nodes are not touched:
    /// the named type has exactly the anonymous shape, so their wires stay
    /// valid.
    ///
    /// Refused, with nothing changed and no undo step, when the network is
    /// read-only, the node is not an anonymous `record_construct` or has no
    /// fields, or `def_name` is invalid, inside a linked library or taken.
    pub fn promote_record_construct(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        def_name: &str,
    ) -> Result<(), String> {
        self.ensure_active_editable()?;
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or_else(|| "No active network".to_string())?;
        let current = self
            .record_construct_data(scope_path, node_id)
            .ok_or_else(|| "Not a record_construct node".to_string())?;
        if !current.is_anonymous() {
            return Err(format!("The record is already named '{}'", current.schema));
        }
        if current.fields.is_empty() {
            return Err("A record with no fields cannot be named".to_string());
        }
        crate::identifier::is_valid_user_name(def_name)
            .map_err(|reason| format!("Name '{}' is invalid: {}", def_name, reason))?;
        if crate::library_links::name_conflicts_with_mount(&self.node_type_registry, def_name) {
            return Err(format!(
                "Name '{}' is inside a linked library, which is read-only",
                def_name
            ));
        }
        if self.node_type_registry.name_is_taken(def_name) {
            return Err(format!("Name '{}' is already taken", def_name));
        }

        let mut def = RecordTypeDef::from_named_fields(
            def_name,
            current
                .fields
                .iter()
                .map(|f| (f.name.clone(), f.data_type.clone()))
                .collect(),
        );
        self.node_type_registry.claim_field_ids(&mut def);
        let mut data = current.clone();
        data.schema = def_name.to_string();
        data.fields.clear();

        let before = self
            .snapshot_network(&network_name)
            .ok_or_else(|| "Failed to snapshot network".to_string())?;
        let pruned_folders = self.node_type_registry.ancestor_folders_present(def_name);
        // The non-pushing registry call. It validates the def (a cycle through
        // a field type, say) before anything has changed.
        self.node_type_registry
            .add_record_type_def(def.clone())
            .map_err(|e| e.to_string())?;
        self.install_record_construct_data(scope_path, node_id, data)?;
        if let Some(network_command) = self.finish_network_edit(
            scope_path,
            node_id,
            &network_name,
            before,
            NAME_RECORD_DESCRIPTION,
        ) {
            self.push_command(CompositeCommand {
                commands: vec![
                    Box::new(AddRecordTypeDefCommand {
                        def,
                        pruned_folders,
                    }),
                    Box::new(network_command),
                ],
                description: NAME_RECORD_DESCRIPTION.to_string(),
            });
        }
        Ok(())
    }

    /// The shared tail of the schema switch and the field-list edit: snapshot,
    /// install `data`, then revalidate and push one `NetworkSnapshotCommand`
    /// labelled `description`.
    fn replace_record_construct_data(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        network_name: &str,
        data: RecordConstructData,
        description: &str,
    ) -> Result<(), String> {
        let before = self
            .snapshot_network(network_name)
            .ok_or_else(|| "Failed to snapshot network".to_string())?;
        self.install_record_construct_data(scope_path, node_id, data)?;
        self.finish_declared_input_edit(scope_path, node_id, network_name, before, description);
        Ok(())
    }

    /// Install `data` on the node, re-derive its pins (which keeps every wire
    /// whose pin survives, by id then name, and drops the rest) and reflow
    /// neighbours if the node grew (a named node shows its schema as a
    /// subtitle, an anonymous one does not).
    fn install_record_construct_data(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        data: RecordConstructData,
    ) -> Result<(), String> {
        let old_sizes = self.capture_footprint_chain(scope_path, node_id);
        {
            let node = self
                .get_scope_network_mut(scope_path)
                .and_then(|network| network.nodes.get_mut(&node_id))
                .ok_or_else(|| "Node not found".to_string())?;
            node.data = Box::new(data);
        }
        self.rederive_node_type(scope_path, node_id);
        self.reflow_for_footprint_change(scope_path, node_id, &old_sizes);
        Ok(())
    }
}
