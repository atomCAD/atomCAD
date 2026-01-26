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

use std::collections::HashMap;
use serde::Serialize;

use crate::structure_designer::node_network::{NodeNetwork, Argument};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::text_format::{Parser, Statement, PropertyValue};
use crate::structure_designer::text_format::TextValue;
use crate::structure_designer::text_format::auto_layout;

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

/// Pending connection to be made after all nodes are created.
#[derive(Debug, Clone)]
struct PendingConnection {
    dest_node_name: String,
    param_name: String,
    source_refs: Vec<(String, bool)>, // (source_name, is_function_ref)
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

        // Step 5: Process visibility
        self.apply_visibility();

        // Step 6: Process delete and output statements
        for stmt in &statements {
            if let Err(e) = self.process_statement_second_pass(stmt) {
                self.result.add_error(e);
            }
        }

        // Ensure custom node types are updated for any nodes that need them
        self.registry.initialize_custom_node_types_for_network(self.network);

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
        self.network.displayed_node_ids.clear();
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
            Statement::Assignment { name, node_type, properties } => {
                self.process_assignment(name, node_type, properties)
            }
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
        // Look up node type
        let node_type = self.registry.get_node_type(node_type_name)
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
        let node_id = self.network.add_node(node_type_name, position, num_params, node_data);

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
            PropertyValue::NodeRef(name) => {
                if let Some(&node_id) = self.name_to_id.get(name) {
                    connections.push((node_id, 0)); // Regular output pin
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
            PropertyValue::Literal(_) => {}
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
            PropertyValue::NodeRef(_) | PropertyValue::FunctionRef(_) => None,
        }
    }

    /// Apply literal properties to a node's data.
    fn apply_literal_properties(
        &mut self,
        node_id: u64,
        properties: &[(String, PropertyValue)],
    ) -> Result<(), String> {
        // Get valid parameter names for this node type (for validation)
        let (valid_params, node_type_name): (Vec<String>, String) = self
            .network
            .nodes
            .get(&node_id)
            .and_then(|node| {
                self.registry
                    .get_node_type_for_node(node)
                    .map(|node_type| {
                        let params = node_type
                            .parameters
                            .iter()
                            .map(|p| p.name.clone())
                            .collect();
                        (params, node.node_type_name.clone())
                    })
            })
            .unwrap_or_else(|| (Vec::new(), String::new()));

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
            // Skip special properties
            if prop_name == "visible" {
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
        if !literal_props.is_empty() {
            if let Some(node) = self.network.nodes.get_mut(&node_id) {
                node.data.set_text_properties(&literal_props)
                    .map_err(|e| format!("Error setting properties: {}", e))?;
            }
        }

        Ok(())
    }

    /// Collect pending connections from properties.
    fn collect_connections(
        &mut self,
        dest_node_name: &str,
        _node_id: u64,
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

            // Collect connection references
            let source_refs = self.extract_source_refs(prop_value);
            if !source_refs.is_empty() {
                self.pending_connections.push(PendingConnection {
                    dest_node_name: dest_node_name.to_string(),
                    param_name: prop_name.clone(),
                    source_refs,
                });
            }
        }
    }

    /// Extract source node references from a property value.
    fn extract_source_refs(&self, prop_value: &PropertyValue) -> Vec<(String, bool)> {
        match prop_value {
            PropertyValue::NodeRef(name) => vec![(name.clone(), false)],
            PropertyValue::FunctionRef(name) => vec![(name.clone(), true)],
            PropertyValue::Array(items) => {
                items.iter()
                    .flat_map(|item| self.extract_source_refs(item))
                    .collect()
            }
            PropertyValue::Literal(_) => vec![],
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
        let dest_node_id = *self.name_to_id.get(&conn.dest_node_name)
            .ok_or_else(|| format!("Destination node '{}' not found", conn.dest_node_name))?;

        // Get destination node's parameter index
        let (param_index, is_multi) = self.get_param_index(dest_node_id, &conn.param_name)?;

        // Get destination node for modification
        let dest_node = self.network.nodes.get_mut(&dest_node_id)
            .ok_or_else(|| format!("Destination node '{}' not found in network", conn.dest_node_name))?;

        // Ensure arguments vector is large enough
        while dest_node.arguments.len() <= param_index {
            dest_node.arguments.push(Argument::new());
        }

        // Clear existing connections for this parameter if it's not multi
        if !is_multi {
            dest_node.arguments[param_index].argument_output_pins.clear();
        }

        // Wire each source
        for (source_name, is_function_ref) in &conn.source_refs {
            let source_node_id = *self.name_to_id.get(source_name)
                .ok_or_else(|| format!("Source node '{}' not found", source_name))?;

            // Determine output pin index (-1 for function pin, 0 for regular)
            let output_pin_index = if *is_function_ref { -1 } else { 0 };

            // Add the connection
            // We need to re-borrow dest_node since we released it above
            if let Some(dest_node) = self.network.nodes.get_mut(&dest_node_id) {
                dest_node.arguments[param_index]
                    .argument_output_pins
                    .insert(source_node_id, output_pin_index);
            }

            let ref_type = if *is_function_ref { "@" } else { "" };
            self.result.connections_made.push(format!(
                "{}.{} <- {}{}",
                conn.dest_node_name, conn.param_name, ref_type, source_name
            ));
        }

        Ok(())
    }

    /// Get the parameter index for a parameter name.
    /// Returns (index, is_multi) where is_multi is true if the parameter accepts multiple inputs.
    fn get_param_index(&self, node_id: u64, param_name: &str) -> Result<(usize, bool), String> {
        let node = self.network.nodes.get(&node_id)
            .ok_or_else(|| "Node not found".to_string())?;

        let node_type = self.registry.get_node_type_for_node(node)
            .ok_or_else(|| format!("Node type '{}' not found", node.node_type_name))?;

        for (index, param) in node_type.parameters.iter().enumerate() {
            if param.name == param_name {
                // Multi-input parameters have array data types
                let is_multi = param.data_type.is_array();
                return Ok((index, is_multi));
            }
        }

        Err(format!("Parameter '{}' not found on node type '{}'", param_name, node.node_type_name))
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
        let node_id = *self.name_to_id.get(node_name)
            .ok_or_else(|| format!("Cannot delete '{}': node not found", node_name))?;

        // Remove all wires connected to this node (both incoming and outgoing)
        self.remove_wires_for_node(node_id);

        // Remove from displayed nodes
        self.network.displayed_node_ids.remove(&node_id);

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
                argument.argument_output_pins.remove(&node_id);
            }
        }

        // Incoming wires are automatically removed when the node is removed
    }

    /// Process an output statement.
    fn process_output(&mut self, node_name: &str) -> Result<(), String> {
        let node_id = *self.name_to_id.get(node_name)
            .ok_or_else(|| format!("Cannot set output to '{}': node not found", node_name))?;

        self.network.set_return_node(node_id);
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
