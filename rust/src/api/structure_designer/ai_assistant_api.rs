//! AI Assistant API for node network text format serialization and editing.
//!
//! This module provides the public API functions for the AI assistant integration,
//! allowing external tools to query and modify node networks through a text format.
//!
//! # Overview
//!
//! The API provides two main operations:
//! - **Query**: Serialize the active node network to text format
//! - **Edit**: Parse text format commands and apply changes to the network
//!
//! # Example Usage (via HTTP server)
//!
//! ```text
//! # Query the network
//! GET http://localhost:19847/query
//!
//! # Edit the network
//! POST http://localhost:19847/edit
//! Content-Type: text/plain
//!
//! sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }
//! output sphere1
//! ```

use crate::api::api_common::{
    refresh_structure_designer_auto,
    with_cad_instance_or,
    with_mut_cad_instance_or,
};
use crate::structure_designer::text_format::{
    serialize_network,
    edit_network as text_edit_network,
    EditResult,
    describe_node_type,
    truncate_description,
};

// =============================================================================
// FFI Functions (exposed to Flutter via flutter_rust_bridge)
// =============================================================================

/// Query the active node network and return its text format representation.
///
/// This function serializes the currently active node network to the text format
/// used by AI assistants for understanding and editing the network.
///
/// # Returns
/// A string containing the text format representation of the network.
/// If no network is active, returns an error message starting with "#".
///
/// # Example Output
/// ```text
/// sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }
/// cuboid1 = cuboid { min_corner: (-5, -5, -5), extent: (10, 10, 10) }
/// union1 = union { shapes: [sphere1, cuboid1] }
/// output union1
/// ```
#[flutter_rust_bridge::frb(sync)]
pub fn ai_query_network() -> String {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let structure_designer = &cad_instance.structure_designer;

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

                // Serialize the network with its name
                serialize_network(network, &structure_designer.node_type_registry, Some(network_name))
            },
            "# Error: Could not access structure designer\n".to_string()
        )
    }
}

/// Edit the active node network using text format commands.
///
/// This function parses text format commands and applies changes to the currently
/// active node network. Changes may include creating nodes, updating properties,
/// making wire connections, and deleting nodes.
///
/// # Arguments
/// * `code` - The edit commands in text format
/// * `replace` - If true, replace entire network; if false, incremental merge
///
/// # Returns
/// A JSON string containing the `EditResult` with details of what was changed:
/// - `success` - Whether the operation succeeded
/// - `nodes_created` - Names of newly created nodes
/// - `nodes_updated` - Names of modified nodes
/// - `nodes_deleted` - Names of deleted nodes
/// - `connections_made` - Descriptions of wire connections
/// - `errors` - Error messages if any
/// - `warnings` - Warning messages if any
///
/// # Example Input
/// ```text
/// sphere1 = sphere { center: (0, 0, 0), radius: 10, visible: true }
/// output sphere1
/// ```
#[flutter_rust_bridge::frb(sync)]
pub fn ai_edit_network(code: String, replace: bool) -> String {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let structure_designer = &mut cad_instance.structure_designer;

                // Get the active network name
                let network_name = match &structure_designer.active_node_network_name {
                    Some(name) => name.clone(),
                    None => {
                        return serde_json::to_string(&EditResult {
                            success: false,
                            nodes_created: vec![],
                            nodes_updated: vec![],
                            nodes_deleted: vec![],
                            connections_made: vec![],
                            errors: vec!["No active node network".to_string()],
                            warnings: vec![],
                        }).unwrap_or_else(|_| r#"{"success":false,"errors":["No active node network"]}"#.to_string());
                    }
                };

                // Temporarily remove the network from the registry to avoid borrow conflicts.
                // This is necessary because text_edit_network needs:
                // - &mut NodeNetwork (the network we're editing)
                // - &NodeTypeRegistry (for looking up node types)
                // And the network lives inside the registry's node_networks HashMap.
                let mut network = match structure_designer.node_type_registry.node_networks.remove(&network_name) {
                    Some(network) => network,
                    None => {
                        return serde_json::to_string(&EditResult {
                            success: false,
                            nodes_created: vec![],
                            nodes_updated: vec![],
                            nodes_deleted: vec![],
                            connections_made: vec![],
                            errors: vec![format!("Network '{}' not found", network_name)],
                            warnings: vec![],
                        }).unwrap_or_else(|_| r#"{"success":false,"errors":["Network not found"]}"#.to_string());
                    }
                };

                // Apply the edit commands
                let result = text_edit_network(&mut network, &structure_designer.node_type_registry, &code, replace);

                // Put the network back into the registry
                structure_designer.node_type_registry.node_networks.insert(network_name.clone(), network);

                // Validate the edited network to update output_type and repair arguments
                // This is necessary because text_edit_network doesn't update network.node_type.output_type
                // when the return node is set via "output <node>" statement.
                // Without this, custom nodes using this network won't render correctly.
                {
                    use crate::structure_designer::network_validator::validate_network;
                    let registry_ptr = &mut structure_designer.node_type_registry as *mut crate::structure_designer::node_type_registry::NodeTypeRegistry;
                    unsafe {
                        if let Some(network) = (*registry_ptr).node_networks.get_mut(&network_name) {
                            validate_network(network, &mut *registry_ptr, None);
                        }
                    }
                }

                // Mark that a full refresh is needed since the network was edited directly
                // (bypassing StructureDesigner change tracking)
                cad_instance.structure_designer.mark_full_refresh();

                // Trigger a refresh after editing
                refresh_structure_designer_auto(cad_instance);

                // Return the result as JSON
                serde_json::to_string(&result).unwrap_or_else(|e| {
                    format!(r#"{{"success":false,"errors":["Failed to serialize result: {}"]}}"#, e)
                })
            },
            r#"{"success":false,"errors":["Could not access structure designer"]}"#.to_string()
        )
    }
}

/// List all available node network names.
///
/// This function returns a list of all node networks available in the current design.
/// Useful for AI assistants to discover what networks can be queried.
///
/// # Returns
/// A vector of network names, or empty vector if no networks exist.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_list_networks() -> Vec<String> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance.structure_designer.node_type_registry.get_node_network_names()
            },
            vec![]
        )
    }
}

/// Get information about the currently active node network.
///
/// # Returns
/// A tuple of (network_name, node_count, has_output) if a network is active,
/// or None if no network is active.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_get_active_network_info() -> Option<(String, usize, bool)> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let structure_designer = &cad_instance.structure_designer;
                let network_name = structure_designer.active_node_network_name.as_ref()?;
                let network = structure_designer.node_type_registry.node_networks.get(network_name)?;

                Some((
                    network_name.clone(),
                    network.nodes.len(),
                    network.return_node_id.is_some(),
                ))
            },
            None
        )
    }
}

/// List all available node types in human-readable text format.
///
/// Returns node types grouped by category, with optional filtering.
///
/// # Arguments
/// * `category` - Optional category filter (e.g., "Geometry3D", "AtomicStructure")
/// * `verbose` - If true, include descriptions; if false, show only names (compact)
///
/// # Returns
/// Human-readable text listing all node types.
///
/// # Example Output (compact, verbose=false)
/// ```text
/// === Geometry3D ===
///   cuboid, sphere, half_space, union, intersect, diff, ...
/// ```
///
/// # Example Output (verbose=true)
/// ```text
/// === Geometry3D ===
///   `cuboid`  - Outputs a cuboid with integer corner and extent
///   `sphere`  - Outputs a sphere with integer center and radius
///   ...
/// ```
/// Note: In verbose mode, descriptions are truncated to first line/sentence (max ~150 chars).
#[flutter_rust_bridge::frb(sync)]
pub fn ai_list_node_types(category: Option<String>, verbose: bool) -> String {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let category_views = cad_instance
                    .structure_designer
                    .node_type_registry
                    .get_node_type_views();

                format_node_type_list(&category_views, category.as_deref(), verbose)
            },
            "# Error: Could not access structure designer\n".to_string(),
        )
    }
}

/// Format node type views into human-readable text.
fn format_node_type_list(
    category_views: &[crate::api::structure_designer::structure_designer_api_types::APINodeCategoryView],
    category_filter: Option<&str>,
    verbose: bool,
) -> String {
    use std::fmt::Write;

    let mut output = String::new();

    // Map category filter string to enum variant
    let filter_category = category_filter.and_then(|s| {
        use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
        match s.to_lowercase().as_str() {
            "annotation" => Some(NodeTypeCategory::Annotation),
            "mathandprogramming" | "math" | "programming" => {
                Some(NodeTypeCategory::MathAndProgramming)
            }
            "geometry2d" | "2d" => Some(NodeTypeCategory::Geometry2D),
            "geometry3d" | "3d" => Some(NodeTypeCategory::Geometry3D),
            "atomicstructure" | "atomic" => Some(NodeTypeCategory::AtomicStructure),
            "otherbuiltin" | "other" => Some(NodeTypeCategory::OtherBuiltin),
            "custom" => Some(NodeTypeCategory::Custom),
            _ => None,
        }
    });

    // Check if filter was invalid
    if category_filter.is_some() && filter_category.is_none() {
        return format!(
            "# Unknown category: '{}'\n# Valid categories: Annotation, MathAndProgramming, Geometry2D, Geometry3D, AtomicStructure, OtherBuiltin, Custom\n",
            category_filter.unwrap()
        );
    }

    let mut has_output = false;
    for category_view in category_views {
        // Apply category filter if specified
        if let Some(ref filter) = filter_category {
            if &category_view.category != filter {
                continue;
            }
        }

        if category_view.nodes.is_empty() {
            continue;
        }

        has_output = true;

        // Write category header
        let category_name = format!("{:?}", category_view.category);
        writeln!(output, "=== {} ===", category_name).unwrap();

        if verbose {
            // Verbose mode: one node per line with description
            // Add 2 for backticks around name
            let max_name_len = category_view
                .nodes
                .iter()
                .map(|n| n.name.len() + 2)
                .max()
                .unwrap_or(0);

            for node in &category_view.nodes {
                let truncated_desc = truncate_description(&node.description);
                writeln!(
                    output,
                    "  {:width$} - {}",
                    format!("`{}`", node.name),
                    truncated_desc,
                    width = max_name_len
                )
                .unwrap();
            }
        } else {
            // Compact mode: comma-separated names
            let names: Vec<&str> = category_view.nodes.iter().map(|n| n.name.as_str()).collect();
            writeln!(output, "  {}", names.join(", ")).unwrap();
        }

        writeln!(output).unwrap();
    }

    if !has_output {
        if let Some(filter) = category_filter {
            return format!("# No nodes found in category '{}'\n", filter);
        }
        return "# No node types available\n".to_string();
    }

    output
}

/// Describe a specific node type in detail.
///
/// Returns a human-readable description of the node type including:
/// - Name, category, and description
/// - Parameters (input pins) with types and default values
/// - Properties that are stored but not wirable (stored-only)
/// - Output type
///
/// # Arguments
/// * `node_type_name` - The name of the node type to describe
///
/// # Returns
/// Human-readable text describing the node, or an error message if not found.
///
/// # Example Output
/// ```text
/// Node: sphere
/// Category: Geometry3D
/// Description: Outputs a sphere with integer center coordinates and integer radius.
///
/// Parameters (input pins):
///   center    : IVec3     [default: (0, 0, 0)]
///   radius    : Int       [default: 1]
///   unit_cell : UnitCell  [no default - wire only]
///
/// Output: Geometry
/// ```
#[flutter_rust_bridge::frb(sync)]
pub fn ai_describe_node_type(node_type_name: String) -> String {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let registry = &cad_instance.structure_designer.node_type_registry;
                describe_node_type(&node_type_name, registry)
            },
            "# Error: Could not access structure designer\n".to_string(),
        )
    }
}
