//! Network serializer for the AI assistant integration.
//!
//! This module provides the `NetworkSerializer` struct which converts a `NodeNetwork`
//! to the human-readable text format suitable for AI assistant consumption.
//!
//! # Example Output
//!
//! ```text
//! sphere1 = sphere { center: (0, 0, 0), radius: 5 }
//! box1 = cuboid { min_corner: (0, 0, 0), extent: (10, 10, 10) }
//! union1 = union { shapes: [sphere1, box1] }
//! output union1
//! ```

use std::collections::{HashMap, HashSet};
use crate::structure_designer::node_network::NodeNetwork;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;

/// Serializes a node network to text format.
pub struct NetworkSerializer<'a> {
    network: &'a NodeNetwork,
    registry: &'a NodeTypeRegistry,
    /// Maps node IDs to generated names
    node_names: HashMap<u64, String>,
    /// Counter per node type for name generation
    type_counters: HashMap<String, u32>,
}

impl<'a> NetworkSerializer<'a> {
    /// Create a new serializer for the given network.
    pub fn new(network: &'a NodeNetwork, registry: &'a NodeTypeRegistry) -> Self {
        Self {
            network,
            registry,
            node_names: HashMap::new(),
            type_counters: HashMap::new(),
        }
    }

    /// Serialize the network to text format.
    pub fn serialize(&mut self) -> String {
        // Handle empty network
        if self.network.nodes.is_empty() {
            return "# Empty network\n".to_string();
        }

        // Step 1: Topological sort
        let sorted_ids = match self.topological_sort() {
            Ok(ids) => ids,
            Err(cycle_error) => {
                return format!("# Error: {}\n", cycle_error);
            }
        };

        // Step 2: Generate names for all nodes
        self.generate_names(&sorted_ids);

        // Step 3: Serialize each node
        let mut output = String::new();
        for node_id in &sorted_ids {
            let node_line = self.serialize_node(*node_id);
            output.push_str(&node_line);
            output.push('\n');
        }

        // Step 4: Output statement
        if let Some(return_node_id) = self.network.return_node_id {
            if let Some(return_name) = self.node_names.get(&return_node_id) {
                output.push_str(&format!("output {}\n", return_name));
            }
        }

        output
    }

    /// Perform topological sort of nodes (dependencies before dependents).
    /// Returns an error if a cycle is detected.
    fn topological_sort(&self) -> Result<Vec<u64>, String> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut temp_mark = HashSet::new();

        // Get all node IDs sorted for deterministic output
        let mut node_ids: Vec<u64> = self.network.nodes.keys().copied().collect();
        node_ids.sort();

        // Visit all nodes in sorted order
        for node_id in node_ids {
            if !visited.contains(&node_id) {
                self.dfs_visit(node_id, &mut result, &mut visited, &mut temp_mark)?;
            }
        }

        Ok(result)
    }

    /// Depth-first search visit for topological sort.
    fn dfs_visit(
        &self,
        node_id: u64,
        result: &mut Vec<u64>,
        visited: &mut HashSet<u64>,
        temp_mark: &mut HashSet<u64>,
    ) -> Result<(), String> {
        // Cycle detection
        if temp_mark.contains(&node_id) {
            let node = self.network.nodes.get(&node_id);
            let node_type = node.map(|n| n.node_type_name.as_str()).unwrap_or("unknown");
            return Err(format!("Cycle detected at node {} (type: {})", node_id, node_type));
        }

        // Already fully visited
        if visited.contains(&node_id) {
            return Ok(());
        }

        // Mark temporarily (for cycle detection)
        temp_mark.insert(node_id);

        // Visit dependencies first (nodes that this node depends on)
        if let Some(node) = self.network.nodes.get(&node_id) {
            for argument in &node.arguments {
                // Sort dependency node IDs for deterministic output
                let mut dep_ids: Vec<u64> = argument.argument_output_pins.keys().copied().collect();
                dep_ids.sort();
                for source_node_id in dep_ids {
                    self.dfs_visit(source_node_id, result, visited, temp_mark)?;
                }
            }
        }

        // Remove temp mark, add permanent mark
        temp_mark.remove(&node_id);
        visited.insert(node_id);

        // Add to result (post-order)
        result.push(node_id);

        Ok(())
    }

    /// Generate names for all nodes in the given order.
    ///
    /// Prefers user-specified custom_name if available. Handles name collisions
    /// by falling back to auto-generated names when necessary.
    fn generate_names(&mut self, sorted_ids: &[u64]) {
        // First pass: collect all valid custom names and reserve them
        let mut used_names: HashSet<String> = HashSet::new();
        let mut nodes_with_valid_custom_names: HashSet<u64> = HashSet::new();

        for &node_id in sorted_ids {
            if let Some(node) = self.network.nodes.get(&node_id) {
                if let Some(ref custom_name) = node.custom_name {
                    // Check if this custom name is already used by another node
                    if !used_names.contains(custom_name) {
                        used_names.insert(custom_name.clone());
                        nodes_with_valid_custom_names.insert(node_id);
                    }
                    // If custom_name is already used, this node will get an auto-generated name
                }
            }
        }

        // Second pass: assign names to all nodes
        for &node_id in sorted_ids {
            if let Some(node) = self.network.nodes.get(&node_id) {
                let name = if nodes_with_valid_custom_names.contains(&node_id) {
                    // Use the custom name (we know it's valid and unique)
                    node.custom_name.clone().unwrap()
                } else {
                    // Generate a unique name avoiding collisions with custom names
                    let node_type = &node.node_type_name;
                    loop {
                        let counter = self.type_counters
                            .entry(node_type.clone())
                            .or_insert(0);
                        *counter += 1;
                        let generated_name = format!("{}{}", node_type, counter);

                        // Check if this generated name conflicts with a reserved custom name
                        if !used_names.contains(&generated_name) {
                            used_names.insert(generated_name.clone());
                            break generated_name;
                        }
                        // If it conflicts, loop and try the next counter
                    }
                };

                self.node_names.insert(node_id, name);
            }
        }
    }

    /// Get the generated name for a node.
    fn get_node_name(&self, node_id: u64) -> Option<&str> {
        self.node_names.get(&node_id).map(|s| s.as_str())
    }

    /// Serialize a single node to text format.
    fn serialize_node(&self, node_id: u64) -> String {
        let node = match self.network.nodes.get(&node_id) {
            Some(n) => n,
            None => return format!("# Error: node {} not found", node_id),
        };

        let node_name = match self.get_node_name(node_id) {
            Some(name) => name.to_string(),
            None => return format!("# Error: no name for node {}", node_id),
        };

        // Get the node type to access parameter information
        let node_type = self.registry.get_node_type_for_node(node);

        // Collect all properties (stored values + connections)
        let mut properties: Vec<(String, String)> = Vec::new();

        // Track which parameters have connections
        let mut connected_params: HashSet<String> = HashSet::new();

        // First pass: gather connections
        if let Some(nt) = node_type {
            for (arg_index, argument) in node.arguments.iter().enumerate() {
                if !argument.argument_output_pins.is_empty() && arg_index < nt.parameters.len() {
                    let param_name = &nt.parameters[arg_index].name;
                    connected_params.insert(param_name.clone());

                    // Check if this is a multi-input parameter
                    let is_multi = argument.argument_output_pins.len() > 1;

                    if is_multi {
                        // Multi-input: format as array of references
                        // Sort by source node ID for deterministic output
                        let mut entries: Vec<_> = argument.argument_output_pins.iter().collect();
                        entries.sort_by_key(|(id, _)| **id);
                        let refs: Vec<String> = entries.iter()
                            .filter_map(|(source_id, pin_index)| {
                                let source_name = self.get_node_name(**source_id)?;
                                Some(self.format_reference(source_name, **pin_index))
                            })
                            .collect();
                        properties.push((param_name.clone(), format!("[{}]", refs.join(", "))));
                    } else {
                        // Single input: format as direct reference
                        if let Some((&source_id, &pin_index)) = argument.argument_output_pins.iter().next() {
                            if let Some(source_name) = self.get_node_name(source_id) {
                                properties.push((
                                    param_name.clone(),
                                    self.format_reference(source_name, pin_index),
                                ));
                            }
                        }
                    }
                }
            }
        }

        // Second pass: add stored properties (skip those with connections)
        let text_props = node.data.get_text_properties();
        for (prop_name, prop_value) in text_props {
            // Only include properties that don't have connections
            if !connected_params.contains(&prop_name) {
                properties.push((prop_name, prop_value.to_text()));
            }
        }

        // Third pass: add visibility property (only if visible, since invisible is the default)
        if self.network.displayed_node_ids.contains_key(&node_id) {
            properties.push(("visible".to_string(), "true".to_string()));
        }

        // Format the node
        if properties.is_empty() {
            format!("{} = {}", node_name, node.node_type_name)
        } else {
            let props_str: Vec<String> = properties.iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect();
            format!("{} = {} {{ {} }}", node_name, node.node_type_name, props_str.join(", "))
        }
    }

    /// Format a node reference, handling function pin references with @ prefix.
    fn format_reference(&self, source_name: &str, pin_index: i32) -> String {
        if pin_index == -1 {
            // Function pin reference
            format!("@{}", source_name)
        } else {
            // Regular output reference
            source_name.to_string()
        }
    }
}

/// Serialize a node network to text format.
///
/// This is the main entry point for network serialization.
///
/// # Arguments
/// * `network` - The node network to serialize
/// * `registry` - The node type registry for looking up parameter information
///
/// # Returns
/// A string containing the text format representation of the network.
///
/// # Example
/// ```rust,ignore
/// let text = serialize_network(&network, &registry);
/// println!("{}", text);
/// // Output:
/// // sphere1 = sphere { center: (0, 0, 0), radius: 5 }
/// // output sphere1
/// ```
pub fn serialize_network(network: &NodeNetwork, registry: &NodeTypeRegistry) -> String {
    let mut serializer = NetworkSerializer::new(network, registry);
    serializer.serialize()
}
