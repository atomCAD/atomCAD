//! AI Assistant API for node network text format serialization and editing.
//!
//! This module provides the public API functions for the AI assistant integration,
//! allowing external tools to query and modify node networks through a text format.
//!
//! # Overview
//!
//! The API provides two main operations:
//! - **Query**: Serialize the active node network to text format
//! - **Edit**: Parse text format commands and apply changes to the network (Phase 4)
//!
//! # Example Usage
//!
//! ```rust,ignore
//! use crate::api::structure_designer::ai_assistant_api::query_network;
//!
//! let text = query_network(&structure_designer);
//! // Returns text like:
//! // sphere1 = sphere { center: (0, 0, 0), radius: 5 }
//! // output sphere1
//! ```

use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::serialize_network;

/// Serializes the active node network to text format.
///
/// This function returns a human-readable text representation of the currently
/// active node network, suitable for AI assistant consumption and editing.
///
/// # Arguments
/// * `structure_designer` - The structure designer instance containing the network
///
/// # Returns
/// A string containing the text format representation of the active network.
/// If no network is active, returns an appropriate message.
///
/// # Text Format
///
/// The output follows this format:
/// ```text
/// # Node definitions (in topological order)
/// name = type { property: value, property: value }
///
/// # Connections use node names
/// union1 = union { shapes: [sphere1, box1] }
///
/// # Function pin references use @ prefix
/// map1 = map { f: @pattern }
///
/// # Output declaration
/// output final_node
/// ```
///
/// # Example
/// ```rust,ignore
/// let text = query_network(&designer);
/// println!("{}", text);
/// // Output:
/// // sphere1 = sphere { center: (0, 0, 0), radius: 5 }
/// // cuboid1 = cuboid { min_corner: (-5, -5, -5), extent: (10, 10, 10) }
/// // union1 = union { shapes: [sphere1, cuboid1] }
/// // output union1
/// ```
pub fn query_network(structure_designer: &StructureDesigner) -> String {
    // Get the active network name
    let network_name = match &structure_designer.active_node_network_name {
        Some(name) => name,
        None => return "# No active node network\n".to_string(),
    };

    // Get the network from the registry
    let network = match structure_designer.node_type_registry.node_networks.get(network_name) {
        Some(network) => network,
        None => return format!("# Network '{}' not found\n", network_name),
    };

    // Serialize the network
    serialize_network(network, &structure_designer.node_type_registry)
}

/// Query a specific node network by name.
///
/// This function allows querying a specific network by name, rather than
/// the currently active network.
///
/// # Arguments
/// * `structure_designer` - The structure designer instance
/// * `network_name` - The name of the network to query
///
/// # Returns
/// A string containing the text format representation of the specified network.
/// Returns an error message if the network is not found.
pub fn query_network_by_name(structure_designer: &StructureDesigner, network_name: &str) -> String {
    // Get the network from the registry
    let network = match structure_designer.node_type_registry.node_networks.get(network_name) {
        Some(network) => network,
        None => return format!("# Network '{}' not found\n", network_name),
    };

    // Serialize the network
    serialize_network(network, &structure_designer.node_type_registry)
}

/// Returns a list of all available node network names.
///
/// This can be used by AI assistants to discover what networks are available
/// for querying.
///
/// # Arguments
/// * `structure_designer` - The structure designer instance
///
/// # Returns
/// A vector of network names in alphabetical order.
pub fn list_networks(structure_designer: &StructureDesigner) -> Vec<String> {
    structure_designer.node_type_registry.get_node_network_names()
}

/// Returns information about the active network.
///
/// # Arguments
/// * `structure_designer` - The structure designer instance
///
/// # Returns
/// A tuple of (network_name, node_count, has_output) if a network is active,
/// or None if no network is active.
pub fn get_active_network_info(structure_designer: &StructureDesigner) -> Option<(String, usize, bool)> {
    let network_name = structure_designer.active_node_network_name.as_ref()?;
    let network = structure_designer.node_type_registry.node_networks.get(network_name)?;

    Some((
        network_name.clone(),
        network.nodes.len(),
        network.return_node_id.is_some(),
    ))
}
