//! The two structural edits of a `record_construct` (`doc/design_wire_declared_inputs.md`,
//! Phase 2): switching its schema (which also switches between named and
//! anonymous mode) and editing an anonymous node's field list.
//!
//! Both can drop wires — a switch drops the wires of the fields the new
//! schema lacks, a field delete drops that field's wire — and the wires go
//! when the pins are re-derived, not when the data is written. A
//! `SetNodeDataCommand` restores the data only, so its undo would bring a
//! field back without its wire. Each edit is therefore one
//! `NetworkSnapshotCommand`, like the open-port drop.
//!
//! This file is the node's own (`record_node_hook_test.rs` lists it): the
//! downcasts below reach `record_construct` alone and are not a record-node
//! list.

use crate::data_type::DataType;
use crate::nodes::record_construct::RecordConstructData;
use crate::structure_designer::StructureDesigner;
use std::collections::HashSet;

/// The undo-history label of a schema change.
pub const SET_RECORD_SCHEMA_DESCRIPTION: &str = "Set record schema";
/// The undo-history label of an edit of an anonymous record's field list.
pub const EDIT_RECORD_FIELDS_DESCRIPTION: &str = "Edit record fields";

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

    /// The shared tail: snapshot, install `data`, re-derive the pins (which
    /// keeps every wire whose pin survives, by id then name, and drops the
    /// rest), reflow neighbours if the node grew, then revalidate and push one
    /// `NetworkSnapshotCommand` labelled `description`.
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
        self.finish_declared_input_edit(scope_path, node_id, network_name, before, description);
        Ok(())
    }
}
