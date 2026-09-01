//! Network editor for the AI assistant integration.
//!
//! This module provides the `NetworkEditor` struct which applies text format
//! edit commands to a `NodeNetwork`, enabling AI assistants to modify node
//! networks programmatically.
//!
//! # Supported Operations
//!
//! - **Create nodes:** `name = type { prop: value }`
//! - **Update nodes:** Same syntax as create, with existing name
//! - **Delete nodes:** `delete name`
//! - **Set output:** `output name`
//! - **Wire connections:** `prop: other_node` or `prop: @func_node`
//!
//! # Example Input
//!
//! ```text
//! sphere1 = sphere { center: (0, 0, 0), radius: 5 }
//! box1 = cuboid { min_corner: (0, 0, 0), extent: (10, 10, 10) }
//! union1 = union { shapes: [sphere1, box1] }
//! output union1
//! ```

use serde::Serialize;
use std::collections::HashMap;

use crate::node_network::{Argument, ArgumentKind, NodeNetwork, Wire};
use crate::node_type_registry::NodeTypeRegistry;
use crate::nodes::comment::{ANCHOR_PROPERTY, CommentAnchor, CommentData, WireAnchor};
use crate::nodes::parameter::ParameterData;
use crate::text_format::TextValue;
use crate::text_format::auto_layout;
use crate::text_format::{Parser, PropertyValue, Statement};

/// Result of an edit operation.
#[derive(Debug, Clone, Serialize)]
pub struct EditResult {
    /// Whether the edit operation succeeded overall.
    pub success: bool,
    /// Names of nodes that were created.
    pub nodes_created: Vec<String>,
    /// Names of nodes that were updated.
    pub nodes_updated: Vec<String>,
    /// Names of nodes that were deleted.
    pub nodes_deleted: Vec<String>,
    /// Descriptions of wire connections that were made.
    pub connections_made: Vec<String>,
    /// Network description if it was set.
    pub description_set: Option<String>,
    /// Network summary if it was set.
    pub summary_set: Option<String>,
    /// Network output node if it was set.
    pub output_set: Option<String>,
    /// Error messages encountered during editing.
    pub errors: Vec<String>,
    /// Warning messages (non-fatal issues).
    pub warnings: Vec<String>,
}

impl EditResult {
    fn new() -> Self {
        Self {
            success: true,
            nodes_created: Vec::new(),
            nodes_updated: Vec::new(),
            nodes_deleted: Vec::new(),
            connections_made: Vec::new(),
            description_set: None,
            summary_set: None,
            output_set: None,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn add_error(&mut self, error: impl Into<String>) {
        self.success = false;
        self.errors.push(error.into());
    }

    fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }
}

/// A source reference extracted from a property value.
#[derive(Debug, Clone)]
struct SourceRef {
    name: String,
    is_function_ref: bool,
    /// Output pin name for multi-output nodes (e.g., "diff" in `atom_edit.diff`)
    pin_name: Option<String>,
}

/// Pending connection to be made after all nodes are created.
#[derive(Debug, Clone)]
struct PendingConnection {
    dest_node_name: String,
    param_name: String,
    source_refs: Vec<SourceRef>,
}

/// One comment anchor target, still in name form.
#[derive(Debug, Clone)]
enum PendingAnchorTarget {
    /// `on: mybox`
    Node(String),
    /// `on: mybox -> union.a`. The source output pin is deliberately not
    /// carried: a wire is identified by its destination slot plus its source
    /// *node*, so a pin qualifier on the way in is decoration.
    Wire {
        source: String,
        dest: String,
        dest_param: String,
    },
}

/// A comment's `on:` anchors, resolved after the connection pass — names only
/// map to ids once every node exists, and a wire anchor additionally needs the
/// wire itself, which that pass is what creates.
#[derive(Debug, Clone)]
struct PendingAnchors {
    comment_name: String,
    targets: Vec<PendingAnchorTarget>,
}

/// Edits a node network based on text format commands.
pub struct NetworkEditor<'a> {
    network: &'a mut NodeNetwork,
    registry: &'a NodeTypeRegistry,
    /// Maps text names to node IDs (existing + newly created)
    name_to_id: HashMap<String, u64>,
    /// Maps node IDs to text names (reverse lookup)
    id_to_name: HashMap<u64, String>,
    /// Counter for placeholder positioning
    new_node_count: usize,
    /// Pending connections to be made in second pass
    pending_connections: Vec<PendingConnection>,
    /// Nodes that should be visible after edit
    visible_nodes: Vec<String>,
    /// Comment anchors to resolve after the connection pass
    pending_anchors: Vec<PendingAnchors>,
    /// Result tracking
    result: EditResult,
}

impl<'a> NetworkEditor<'a> {
    /// Create a new editor for the given network.
    pub fn new(network: &'a mut NodeNetwork, registry: &'a NodeTypeRegistry) -> Self {
        Self {
            network,
            registry,
            name_to_id: HashMap::new(),
            id_to_name: HashMap::new(),
            new_node_count: 0,
            pending_connections: Vec::new(),
            visible_nodes: Vec::new(),
            pending_anchors: Vec::new(),
            result: EditResult::new(),
        }
    }

    /// Apply edit commands to the network.
    ///
    /// # Arguments
    /// * `code` - The edit commands in text format
    /// * `replace` - If true, replace entire network; if false, incremental merge
    ///
    /// # Returns
    /// An `EditResult` describing what was changed.
    pub fn apply(mut self, code: &str, replace: bool) -> EditResult {
        // Step 1: Parse input
        let statements = match Parser::parse(code) {
            Ok(stmts) => stmts,
            Err(e) => {
                self.result.add_error(format!("Parse error: {}", e));
                return self.result;
            }
        };

        // Step 2: If replace mode, clear the network
        if replace {
            self.clear_network();
        } else {
            // Build name map from existing network
            self.build_existing_name_map();
        }

        // Step 3: First pass - create/update nodes, collect connections
        for stmt in &statements {
            if let Err(e) = self.process_statement_first_pass(stmt) {
                self.result.add_error(e);
            }
        }

        // Step 4: Second pass - wire connections
        self.wire_pending_connections();

        // Step 4b: Comment anchors, which need the wires the previous step made
        self.apply_pending_anchors();

        // Step 5: Process visibility
        self.apply_visibility();

        // Step 6: Process delete and output statements
        for stmt in &statements {
            if let Err(e) = self.process_statement_second_pass(stmt) {
                self.result.add_error(e);
            }
        }

        // Ensure custom node types are updated for any nodes that need them
        self.registry
            .initialize_custom_node_types_for_network(self.network);

        self.result
    }

    /// Clear the entire network (for replace mode).
    fn clear_network(&mut self) {
        // Get all node IDs
        let node_ids: Vec<u64> = self.network.nodes.keys().copied().collect();

        // Remove all nodes
        for node_id in node_ids {
            self.network.nodes.remove(&node_id);
        }

        // Clear other state
        self.network.return_node_id = None;
        self.network.displayed_nodes.clear();
        self.network.selected_node_ids.clear();
        self.network.active_node_id = None;
        self.network.selected_wires.clear();

        // Clear name maps
        self.name_to_id.clear();
        self.id_to_name.clear();
    }

    /// Build name→id mapping from existing network.
    ///
    /// Since all nodes now have persistent names assigned at creation time,
    /// this simply iterates through nodes and uses their custom_name directly.
    fn build_existing_name_map(&mut self) {
        self.name_to_id.clear();
        self.id_to_name.clear();

        for (&node_id, node) in &self.network.nodes {
            if let Some(ref name) = node.custom_name {
                self.name_to_id.insert(name.clone(), node_id);
                self.id_to_name.insert(node_id, name.clone());
            }
        }
    }

    /// Process a statement in the first pass (create/update nodes, collect connections).
    fn process_statement_first_pass(&mut self, stmt: &Statement) -> Result<(), String> {
        match stmt {
            Statement::Assignment {
                name,
                node_type,
                properties,
            } => self.process_assignment(name, node_type, properties),
            Statement::Description { text } => self.process_description(text),
            Statement::Summary { text } => self.process_summary(text),
            Statement::Comment(_) => Ok(()), // Skip comments
            Statement::Output { .. } | Statement::Delete { .. } => Ok(()), // Handled in second pass
        }
    }

    /// Process a statement in the second pass (delete, output).
    fn process_statement_second_pass(&mut self, stmt: &Statement) -> Result<(), String> {
        match stmt {
            Statement::Delete { node_name } => self.process_delete(node_name),
            Statement::Output { node_name } => self.process_output(node_name),
            _ => Ok(()), // Already handled in first pass
        }
    }

    /// Process an assignment statement (create or update node).
    fn process_assignment(
        &mut self,
        name: &str,
        node_type_name: &str,
        properties: &[(String, PropertyValue)],
    ) -> Result<(), String> {
        // Check if node already exists
        let node_id = if let Some(&existing_id) = self.name_to_id.get(name) {
            // Update existing node
            self.update_node(existing_id, name, properties)?;
            existing_id
        } else {
            // Create new node
            self.create_node(name, node_type_name, properties)?
        };

        // Collect pending connections and visibility
        self.collect_connections(name, node_id, properties);

        Ok(())
    }

    /// Create a new node.
    fn create_node(
        &mut self,
        name: &str,
        node_type_name: &str,
        properties: &[(String, PropertyValue)],
    ) -> Result<u64, String> {
        // Validate the user-provided name before using it as a custom_name.
        if let Err(reason) = crate::identifier::is_valid_user_name(name) {
            return Err(format!("Invalid node name '{}': {}", name, reason));
        }
        // Look up node type
        let node_type = self
            .registry
            .get_node_type(node_type_name)
            .ok_or_else(|| format!("Unknown node type: '{}'", node_type_name))?;

        // Create node data using the factory
        let node_data = (node_type.node_data_creator)();

        // Extract input connections from properties for smart layout positioning
        let input_connections = self.extract_input_connections_for_layout(properties);

        // Calculate position using smart auto-layout
        let position = auto_layout::calculate_new_node_position(
            self.network,
            self.registry,
            node_type_name,
            &input_connections,
        );
        self.new_node_count += 1;

        // Add node to network
        let num_params = node_type.parameters.len();
        let node_id = self
            .network
            .add_node(node_type_name, position, num_params, node_data);

        // Assign param_id for parameter nodes (for wire preservation across renames)
        if node_type_name == "parameter" {
            let param_id = self.network.next_param_id;
            self.network.next_param_id += 1;
            if let Some(node) = self.network.nodes.get_mut(&node_id)
                && let Some(param_data) = node.data.as_any_mut().downcast_mut::<ParameterData>()
            {
                param_data.param_id = Some(param_id);
            }
        }

        // Set node as NOT displayed by default (will be set if visible: true)
        self.network.set_node_display(node_id, false);

        // Update name maps
        self.name_to_id.insert(name.to_string(), node_id);
        self.id_to_name.insert(node_id, name.to_string());

        // Store the user-specified custom name on the node for persistence
        if let Some(node) = self.network.nodes.get_mut(&node_id) {
            node.custom_name = Some(name.to_string());
        }

        // Apply literal properties
        self.apply_literal_properties(node_id, properties)?;

        // Initialize custom node type cache (for expr, parameter nodes, etc.)
        if let Some(node) = self.network.nodes.get_mut(&node_id) {
            self.registry.populate_custom_node_type_cache(node, true);
        }

        self.result.nodes_created.push(name.to_string());
        Ok(node_id)
    }

    /// Extract source node IDs from properties for layout positioning.
    ///
    /// Returns a list of (source_node_id, output_pin_index) for each connection
    /// reference found in the properties. Only returns connections to nodes that
    /// already exist in the name map.
    fn extract_input_connections_for_layout(
        &self,
        properties: &[(String, PropertyValue)],
    ) -> Vec<(u64, i32)> {
        let mut connections = Vec::new();

        for (_prop_name, prop_value) in properties {
            self.collect_source_refs_for_layout(prop_value, &mut connections);
        }

        connections
    }

    /// Recursively collect source node references from a property value.
    fn collect_source_refs_for_layout(
        &self,
        prop_value: &PropertyValue,
        connections: &mut Vec<(u64, i32)>,
    ) {
        match prop_value {
            PropertyValue::NodeRef(name, _pin_name) => {
                if let Some(&node_id) = self.name_to_id.get(name) {
                    connections.push((node_id, 0)); // Regular output pin (pin index doesn't matter for layout)
                }
            }
            PropertyValue::FunctionRef(name) => {
                if let Some(&node_id) = self.name_to_id.get(name) {
                    connections.push((node_id, -1)); // Function pin
                }
            }
            PropertyValue::Array(items) => {
                for item in items {
                    self.collect_source_refs_for_layout(item, connections);
                }
            }
            // An anchor is documentation, never a dependency edge (D7), so it
            // must not pull a comment towards its target during layout either.
            PropertyValue::WireRef { .. } | PropertyValue::Literal(_) => {}
        }
    }

    /// Update an existing node.
    fn update_node(
        &mut self,
        node_id: u64,
        name: &str,
        properties: &[(String, PropertyValue)],
    ) -> Result<(), String> {
        // Apply literal properties
        self.apply_literal_properties(node_id, properties)?;

        // Re-initialize custom node type cache in case properties changed
        if let Some(node) = self.network.nodes.get_mut(&node_id) {
            self.registry.populate_custom_node_type_cache(node, true);
        }

        self.result.nodes_updated.push(name.to_string());
        Ok(())
    }

    /// Recursively converts a PropertyValue to a TextValue if all nested values are literals.
    /// Returns None if any nested value is a NodeRef or FunctionRef (these are handled in the connection pass).
    fn property_value_to_text_value(pv: &PropertyValue) -> Option<TextValue> {
        match pv {
            PropertyValue::Literal(tv) => Some(tv.clone()),
            PropertyValue::Array(items) => {
                let converted: Option<Vec<TextValue>> = items
                    .iter()
                    .map(Self::property_value_to_text_value)
                    .collect();
                converted.map(TextValue::Array)
            }
            PropertyValue::NodeRef(..)
            | PropertyValue::FunctionRef(_)
            | PropertyValue::WireRef { .. } => None,
        }
    }

    /// Apply literal properties to a node's data.
    fn apply_literal_properties(
        &mut self,
        node_id: u64,
        properties: &[(String, PropertyValue)],
    ) -> Result<(), String> {
        // Get valid parameter names for this node type (for validation), plus
        // the subset that is array-typed — those are the multi-input pins,
        // whose only literal form is the empty array.
        let (valid_params, multi_params, node_type_name): (
            Vec<String>,
            std::collections::HashSet<String>,
            String,
        ) = self
            .network
            .nodes
            .get(&node_id)
            .and_then(|node| {
                self.registry.get_node_type_for_node(node).map(|node_type| {
                    let params: Vec<String> = node_type
                        .parameters
                        .iter()
                        .map(|p| p.name.clone())
                        .collect();
                    let multi = node_type
                        .parameters
                        .iter()
                        .filter(|p| p.data_type.is_array())
                        .map(|p| p.name.clone())
                        .collect();
                    (params, multi, node.node_type_name.clone())
                })
            })
            .unwrap_or_else(|| (Vec::new(), std::collections::HashSet::new(), String::new()));

        // Get text property names (for literal-only properties that aren't in parameters)
        let text_prop_names: std::collections::HashSet<String> = self
            .network
            .nodes
            .get(&node_id)
            .map(|node| {
                node.data
                    .get_text_properties()
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default();

        // Check if this is a custom node (user-defined node network)
        // Custom nodes can accept literal values for ALL their parameters
        let is_custom_node = self.registry.is_custom_node_type(&node_type_name);

        // Collect literal properties into a HashMap
        let mut literal_props: HashMap<String, TextValue> = HashMap::new();

        for (prop_name, prop_value) in properties {
            // Skip special properties. Neither is `NodeData` state: `visible`
            // lives in `NodeNetwork.displayed_nodes` and `on` (comment anchors)
            // is a list of node ids, so both are handled in the
            // connection-collection pass, which can see the whole network.
            if prop_name == "visible" || prop_name == ANCHOR_PROPERTY {
                continue;
            }

            // `shapes: []` on an array pin is an instruction for the connection
            // pass ("no inbound wires"), not a stored value. Array pins have no
            // stored-value backing, so without this it would be reported as an
            // ignored wire-only literal — a warning on the one spelling that
            // does exactly what the user asked. A pin that *does* back its
            // array with a text property keeps the normal path and stores `[]`.
            if multi_params.contains(prop_name)
                && !text_prop_names.contains(prop_name)
                && matches!(prop_value, PropertyValue::Array(items) if items.is_empty())
            {
                continue;
            }

            // Try to convert PropertyValue to TextValue (handles literals and arrays of literals)
            if let Some(text_value) = Self::property_value_to_text_value(prop_value) {
                // Warn about unknown properties (only for values we're actually applying)
                // A property is "known" if it's either a wirable parameter OR a text-only property
                if !valid_params.is_empty()
                    && !valid_params.contains(prop_name)
                    && !text_prop_names.contains(prop_name)
                {
                    self.result.add_warning(format!(
                        "Unknown property '{}' on node type '{}'",
                        prop_name, node_type_name
                    ));
                }
                // Warn if trying to set a literal on a wire-only parameter
                // (a parameter that exists but has no text property backing)
                // BUT: Custom nodes can accept literals for all parameters
                else if !valid_params.is_empty()
                    && valid_params.contains(prop_name)
                    && !text_prop_names.contains(prop_name)
                    && !is_custom_node
                {
                    self.result.add_warning(format!(
                        "Parameter '{}' on '{}' is wire-only; literal value ignored (connect a node instead)",
                        prop_name, node_type_name
                    ));
                    continue; // Don't add to literal_props since it will be ignored anyway
                }
                literal_props.insert(prop_name.clone(), text_value);
            }
            // Skip NodeRef, FunctionRef, and arrays containing them - handled in connection pass
        }

        // Apply to node data
        if !literal_props.is_empty()
            && let Some(node) = self.network.nodes.get_mut(&node_id)
        {
            // A `zip_with` lane-list shrink through the text path must also
            // disconnect body wires referencing the dropped tail indices —
            // including nested depth ≥ 2 wires that validation rule 3 only
            // flags red and `repair_zone_body` deliberately skips. The data
            // struct cannot reach the body, so the cleanup runs here, at
            // mutation time (`doc/design_zip_with.md` Phase 3). Text edits
            // are positional, so a tail drop is the only shrink shape.
            let old_zip_lane_count = zip_with_lane_count(node);
            node.data
                .set_text_properties(&literal_props)
                .map_err(|e| format!("Error setting properties: {}", e))?;
            if let (Some(old_count), Some(new_count)) =
                (old_zip_lane_count, zip_with_lane_count(node))
                && new_count < old_count
            {
                crate::nodes::zip_with::disconnect_zip_body_wires_to_dropped_lanes(node, new_count);
            }
        }

        Ok(())
    }

    /// Collect pending connections from properties.
    fn collect_connections(
        &mut self,
        dest_node_name: &str,
        node_id: u64,
        properties: &[(String, PropertyValue)],
    ) {
        for (prop_name, prop_value) in properties {
            // Handle visibility
            if prop_name == "visible" {
                if let PropertyValue::Literal(TextValue::Bool(true)) = prop_value {
                    self.visible_nodes.push(dest_node_name.to_string());
                }
                continue;
            }

            // Handle comment anchors. Like `visible`, this has to be taken out
            // before the generic reference handling below, which would
            // otherwise try to wire `on` to a parameter that does not exist.
            if prop_name == ANCHOR_PROPERTY {
                self.collect_anchors(dest_node_name, node_id, prop_value);
                continue;
            }

            // Collect connection references. A property that names no source
            // is still an assignment of that pin — `radius: 5.0` says the pin
            // is a stored value now, not a wire — so it is queued too, with an
            // empty source list, and `wire_connection` clears the pin. Queuing
            // nothing here is what used to let a literal silently coexist with
            // a live wire: the literal landed in the node's data, the wire kept
            // winning at evaluation, and the serializer hid the dead value.
            let source_refs = self.extract_source_refs(prop_value);
            if !source_refs.is_empty()
                || self.property_disconnects_pin(node_id, prop_name, prop_value)
            {
                self.pending_connections.push(PendingConnection {
                    dest_node_name: dest_node_name.to_string(),
                    param_name: prop_name.clone(),
                    source_refs,
                });
            }
        }
    }

    /// Whether a source-free property should disconnect the pin it names.
    ///
    /// Only a property that is *entirely* literal counts: a `WireRef` outside
    /// `on:` names no value at all, and an array that still mentions nodes goes
    /// down the ordinary wiring path. Beyond that there are two pins a literal
    /// must **not** clear:
    ///
    /// - a name that is not a wirable parameter (a text-only property such as
    ///   `polygon.vertices`) — there is no pin to clear;
    /// - a wire-only scalar pin, whose literal `apply_literal_properties`
    ///   rejects with a warning — clearing its wire would leave it with
    ///   neither a wire nor a stored value.
    fn property_disconnects_pin(
        &self,
        node_id: u64,
        prop_name: &str,
        prop_value: &PropertyValue,
    ) -> bool {
        if Self::property_value_to_text_value(prop_value).is_none() {
            return false;
        }
        let Ok((_, is_multi)) = self.get_param_index(node_id, prop_name) else {
            return false;
        };
        if is_multi {
            // An array pin's literal form *is* its wire list, so `[]` — and
            // only `[]` — means "no inbound wires". A non-empty literal array
            // on such a pin is a type error the literal pass already warns
            // about; it must not silently drop the wires as well.
            return matches!(prop_value, PropertyValue::Array(items) if items.is_empty());
        }
        self.pin_accepts_stored_value(node_id, prop_name)
    }

    /// Whether a pin on this node can hold a stored literal value, i.e. the
    /// node's data exposes a text property of that name — or the node is a
    /// custom node type, which accepts literals for all of its parameters.
    fn pin_accepts_stored_value(&self, node_id: u64, prop_name: &str) -> bool {
        let Some(node) = self.network.nodes.get(&node_id) else {
            return false;
        };
        if self.registry.is_custom_node_type(&node.node_type_name) {
            return true;
        }
        node.data
            .get_text_properties()
            .iter()
            .any(|(name, _)| name == prop_name)
    }

    /// Extract source node references from a property value.
    #[allow(clippy::only_used_in_recursion)]
    fn extract_source_refs(&self, prop_value: &PropertyValue) -> Vec<SourceRef> {
        match prop_value {
            PropertyValue::NodeRef(name, pin_name) => vec![SourceRef {
                name: name.clone(),
                is_function_ref: false,
                pin_name: pin_name.clone(),
            }],
            PropertyValue::FunctionRef(name) => vec![SourceRef {
                name: name.clone(),
                is_function_ref: true,
                pin_name: None,
            }],
            PropertyValue::Array(items) => items
                .iter()
                .flat_map(|item| self.extract_source_refs(item))
                .collect(),
            // A wire reference names an existing wire; it never creates one.
            PropertyValue::WireRef { .. } | PropertyValue::Literal(_) => vec![],
        }
    }

    /// Wire all pending connections.
    fn wire_pending_connections(&mut self) {
        let connections = std::mem::take(&mut self.pending_connections);

        for conn in connections {
            if let Err(e) = self.wire_connection(&conn) {
                self.result.add_warning(format!(
                    "Connection warning for {}.{}: {}",
                    conn.dest_node_name, conn.param_name, e
                ));
            }
        }
    }

    /// Wire a single connection.
    fn wire_connection(&mut self, conn: &PendingConnection) -> Result<(), String> {
        // Resolve destination node
        let dest_node_id = *self
            .name_to_id
            .get(&conn.dest_node_name)
            .ok_or_else(|| format!("Destination node '{}' not found", conn.dest_node_name))?;

        // Get destination node's parameter index
        let (param_index, _is_multi) = self.get_param_index(dest_node_id, &conn.param_name)?;

        // Get destination node for modification
        let dest_node = self.network.nodes.get_mut(&dest_node_id).ok_or_else(|| {
            format!(
                "Destination node '{}' not found in network",
                conn.dest_node_name
            )
        })?;

        // Ensure arguments vector is large enough
        while dest_node.arguments.len() <= param_index {
            dest_node.arguments.push(Argument::new());
        }

        // Mentioning a property assigns that pin's whole inbound wire set:
        // what the statement names replaces what was there. This clears for
        // array pins too, which is what lets `shapes: [a, b, c]` shrink to
        // `shapes: [a]` — appending only, as this used to do for multi pins,
        // made an array pin able to grow but never shrink. An empty source
        // list (a literal, or `[]`) therefore disconnects the pin; see
        // `property_disconnects_pin`.
        let removed_wires = dest_node.arguments[param_index].incoming_wires.len();
        dest_node.arguments[param_index].clear();
        if conn.source_refs.is_empty() && removed_wires > 0 {
            self.result.connections_made.push(format!(
                "{}.{} disconnected ({} wire{} removed)",
                conn.dest_node_name,
                conn.param_name,
                removed_wires,
                if removed_wires == 1 { "" } else { "s" }
            ));
        }

        // Wire each source
        for source_ref in &conn.source_refs {
            let source_node_id = *self
                .name_to_id
                .get(&source_ref.name)
                .ok_or_else(|| format!("Source node '{}' not found", source_ref.name))?;

            // Determine output pin index
            let output_pin_index = if source_ref.is_function_ref {
                -1
            } else if let Some(ref pin_name) = source_ref.pin_name {
                // Resolve pin name to pin index using the source node's type
                self.resolve_output_pin_index(source_node_id, pin_name)?
            } else {
                0
            };

            // Add the connection
            // We need to re-borrow dest_node since we released it above
            if let Some(dest_node) = self.network.nodes.get_mut(&dest_node_id) {
                dest_node.arguments[param_index].set_source(source_node_id, output_pin_index);
            }

            let ref_type = if source_ref.is_function_ref { "@" } else { "" };
            let pin_suffix = source_ref
                .pin_name
                .as_ref()
                .map(|p| format!(".{}", p))
                .unwrap_or_default();
            self.result.connections_made.push(format!(
                "{}.{} <- {}{}{}",
                conn.dest_node_name, conn.param_name, ref_type, source_ref.name, pin_suffix
            ));
        }

        Ok(())
    }

    /// Resolve an output pin name to its index on a source node.
    fn resolve_output_pin_index(&self, source_node_id: u64, pin_name: &str) -> Result<i32, String> {
        let source_node = self
            .network
            .nodes
            .get(&source_node_id)
            .ok_or_else(|| "Source node not found".to_string())?;

        let node_type = self
            .registry
            .get_node_type_for_node(source_node)
            .ok_or_else(|| {
                format!(
                    "Node type '{}' not found for pin resolution",
                    source_node.node_type_name
                )
            })?;

        for (index, pin_def) in node_type.output_pins.iter().enumerate() {
            if pin_def.name == pin_name {
                return Ok(index as i32);
            }
        }

        Err(format!(
            "Output pin '{}' not found on node type '{}' (available pins: {})",
            pin_name,
            source_node.node_type_name,
            node_type
                .output_pins
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    /// Get the parameter index for a parameter name.
    /// Returns (index, is_multi) where is_multi is true if the parameter accepts multiple inputs.
    fn get_param_index(&self, node_id: u64, param_name: &str) -> Result<(usize, bool), String> {
        let node = self
            .network
            .nodes
            .get(&node_id)
            .ok_or_else(|| "Node not found".to_string())?;

        let node_type = self
            .registry
            .get_node_type_for_node(node)
            .ok_or_else(|| format!("Node type '{}' not found", node.node_type_name))?;

        for (index, param) in node_type.parameters.iter().enumerate() {
            if param.name == param_name {
                // Multi-input parameters have array data types
                let is_multi = param.data_type.is_array();
                return Ok((index, is_multi));
            }
        }

        Err(format!(
            "Parameter '{}' not found on node type '{}'",
            param_name, node.node_type_name
        ))
    }

    /// Collect a comment's `on:` anchors for the post-wiring pass.
    ///
    /// Accepts a single reference or an array of them; anything else warns and
    /// is skipped, matching how unknown properties are reported.
    fn collect_anchors(&mut self, comment_name: &str, node_id: u64, prop_value: &PropertyValue) {
        let is_comment = self
            .network
            .nodes
            .get(&node_id)
            .is_some_and(|node| node.data.as_any_ref().is::<CommentData>());
        if !is_comment {
            self.result.add_warning(format!(
                "'{}' is only supported on comment nodes; ignored on '{}'",
                ANCHOR_PROPERTY, comment_name
            ));
            return;
        }

        let items: Vec<&PropertyValue> = match prop_value {
            PropertyValue::Array(items) => items.iter().collect(),
            single => vec![single],
        };

        let mut targets = Vec::new();
        for item in items {
            match item {
                PropertyValue::NodeRef(name, None) => {
                    targets.push(PendingAnchorTarget::Node(name.clone()))
                }
                PropertyValue::NodeRef(name, Some(pin)) => self.result.add_warning(format!(
                    "Comment anchor '{}.{}' on '{}' dropped: a node anchor takes no output pin",
                    name, pin, comment_name
                )),
                PropertyValue::WireRef {
                    source,
                    dest,
                    dest_param,
                    ..
                } => targets.push(PendingAnchorTarget::Wire {
                    source: source.clone(),
                    dest: dest.clone(),
                    dest_param: dest_param.clone(),
                }),
                _ => self.result.add_warning(format!(
                    "Comment anchor on '{}' dropped: expected a node reference or a wire reference (`source -> dest.param`)",
                    comment_name
                )),
            }
        }

        // Pushed even when empty: `on: []` is how a text edit clears anchors.
        self.pending_anchors.push(PendingAnchors {
            comment_name: comment_name.to_string(),
            targets,
        });
    }

    /// Resolve collected comment anchors to ids and store them on the comments.
    ///
    /// Runs after wiring, so a wire anchor can be checked against the wire it
    /// names. An anchor that does not resolve warns and is dropped rather than
    /// re-pointed at whatever is nearby (D6).
    fn apply_pending_anchors(&mut self) {
        let pending = std::mem::take(&mut self.pending_anchors);

        for entry in pending {
            let Some(&comment_id) = self.name_to_id.get(&entry.comment_name) else {
                self.result.add_warning(format!(
                    "Comment '{}' not found; its anchors were dropped",
                    entry.comment_name
                ));
                continue;
            };

            let mut anchors = Vec::new();
            for target in &entry.targets {
                match self.resolve_pending_anchor(target) {
                    Ok(anchor) => anchors.push(anchor),
                    Err(e) => self.result.add_warning(format!(
                        "Comment anchor on '{}' dropped: {}",
                        entry.comment_name, e
                    )),
                }
            }

            if let Some(node) = self.network.nodes.get_mut(&comment_id)
                && let Some(data) = node.data.as_any_mut().downcast_mut::<CommentData>()
            {
                data.anchors = anchors;
            }
        }
    }

    /// Turn one name-form anchor target into a `CommentAnchor`.
    fn resolve_pending_anchor(
        &self,
        target: &PendingAnchorTarget,
    ) -> Result<CommentAnchor, String> {
        match target {
            PendingAnchorTarget::Node(name) => {
                let node_id = *self
                    .name_to_id
                    .get(name)
                    .ok_or_else(|| format!("unknown node '{}'", name))?;
                Ok(CommentAnchor::Node(node_id))
            }
            PendingAnchorTarget::Wire {
                source,
                dest,
                dest_param,
            } => {
                let source_id = *self
                    .name_to_id
                    .get(source)
                    .ok_or_else(|| format!("unknown source node '{}'", source))?;
                let dest_id = *self
                    .name_to_id
                    .get(dest)
                    .ok_or_else(|| format!("unknown destination node '{}'", dest))?;
                let (param_index, _) = self.get_param_index(dest_id, dest_param)?;
                let dest_node = self
                    .network
                    .nodes
                    .get(&dest_id)
                    .ok_or_else(|| format!("destination node '{}' not found", dest))?;
                let incoming = dest_node
                    .arguments
                    .get(param_index)
                    .and_then(|arg| {
                        arg.incoming_wires
                            .iter()
                            .find(|w| w.source_node_id == source_id)
                    })
                    .ok_or_else(|| {
                        format!("no wire from '{}' to '{}.{}'", source, dest, dest_param)
                    })?;
                let wire = Wire {
                    source_node_id: incoming.source_node_id,
                    source_pin: incoming.source_pin,
                    source_scope_depth: incoming.source_scope_depth,
                    destination_node_id: dest_id,
                    destination_argument_index: param_index,
                    destination_argument_kind: ArgumentKind::External,
                };
                Ok(CommentAnchor::Wire(WireAnchor::from_wire(
                    &wire,
                    self.network,
                )))
            }
        }
    }

    /// Apply visibility settings to nodes.
    fn apply_visibility(&mut self) {
        let visible_nodes = std::mem::take(&mut self.visible_nodes);

        for node_name in visible_nodes {
            if let Some(&node_id) = self.name_to_id.get(&node_name) {
                self.network.set_node_display(node_id, true);
            }
        }
    }

    /// Process a delete statement.
    fn process_delete(&mut self, node_name: &str) -> Result<(), String> {
        let node_id = *self
            .name_to_id
            .get(node_name)
            .ok_or_else(|| format!("Cannot delete '{}': node not found", node_name))?;

        // Remove all wires connected to this node (both incoming and outgoing)
        self.remove_wires_for_node(node_id);

        // Remove from displayed nodes
        self.network.displayed_nodes.remove(&node_id);

        // Clear return node if this was it
        if self.network.return_node_id == Some(node_id) {
            self.network.return_node_id = None;
        }

        // Remove the node
        self.network.nodes.remove(&node_id);

        // Update name maps
        self.name_to_id.remove(node_name);
        self.id_to_name.remove(&node_id);

        self.result.nodes_deleted.push(node_name.to_string());
        Ok(())
    }

    /// Remove all wires connected to a node.
    fn remove_wires_for_node(&mut self, node_id: u64) {
        // Remove outgoing wires (where this node is a source)
        for node in self.network.nodes.values_mut() {
            for argument in node.arguments.iter_mut() {
                argument.remove_source(node_id);
            }
        }

        // Incoming wires are automatically removed when the node is removed
    }

    /// Process an output statement.
    fn process_output(&mut self, node_name: &str) -> Result<(), String> {
        let node_id = *self
            .name_to_id
            .get(node_name)
            .ok_or_else(|| format!("Cannot set output to '{}': node not found", node_name))?;

        self.network.set_return_node(node_id);
        self.result.output_set = Some(node_name.to_string());
        Ok(())
    }

    /// Process a description statement.
    fn process_description(&mut self, text: &str) -> Result<(), String> {
        self.network.node_type.description = text.to_string();
        self.result.description_set = Some(text.to_string());
        Ok(())
    }

    /// Process a summary statement.
    fn process_summary(&mut self, text: &str) -> Result<(), String> {
        self.network.node_type.summary = Some(text.to_string());
        self.result.summary_set = Some(text.to_string());
        Ok(())
    }
}

/// Apply edit commands to a node network.
///
/// This is the main entry point for network editing.
///
/// # Arguments
/// * `network` - The node network to edit
/// * `registry` - The node type registry for looking up node types
/// * `code` - The edit commands in text format
/// * `replace` - If true, replace entire network; if false, incremental merge
///
/// # Returns
/// An `EditResult` describing what was changed.
///
/// # Example
/// ```rust,ignore
/// let result = edit_network(&mut network, &registry, r#"
///     sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }
///     output sphere1
/// "#, true);
///
/// if result.success {
///     println!("Created {} nodes", result.nodes_created.len());
/// }
/// ```
pub fn edit_network(
    network: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    code: &str,
    replace: bool,
) -> EditResult {
    let editor = NetworkEditor::new(network, registry);
    editor.apply(code, replace)
}

/// The current lane count of a `zip_with` node, `None` for any other node.
/// Used to detect a lane-list shrink across a `set_text_properties` call so
/// body wires to the dropped tail indices can be disconnected at mutation
/// time (`doc/design_zip_with.md` Phase 3).
fn zip_with_lane_count(node: &crate::node_network::Node) -> Option<usize> {
    if node.node_type_name != "zip_with" {
        return None;
    }
    node.data
        .as_any_ref()
        .downcast_ref::<crate::nodes::zip_with::ZipWithData>()
        .map(|d| d.lanes.len())
}
