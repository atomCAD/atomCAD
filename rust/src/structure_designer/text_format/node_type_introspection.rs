//! Node type introspection for generating human-readable documentation.
//!
//! This module provides functions to describe node types in detail, including
//! their parameters, default values, and output types. Used by the AI assistant
//! CLI to provide dynamic node documentation.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::structure_designer::node_type_registry::NodeTypeRegistry;

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
/// * `registry` - The node type registry to look up the node type
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
pub fn describe_node_type(node_type_name: &str, registry: &NodeTypeRegistry) -> String {
    // Look up the node type
    let node_type = match registry.get_node_type(node_type_name) {
        Some(nt) => nt,
        None => return format!("# Node type '{}' not found\n", node_type_name),
    };

    // Create a default instance to get text properties
    let default_data = (node_type.node_data_creator)();
    let text_props = default_data.get_text_properties();

    // Build a map of property name -> (inferred type, formatted value)
    let prop_map: HashMap<String, (String, String)> = text_props
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                (value.inferred_data_type().to_string(), value.to_text()),
            )
        })
        .collect();

    // Get parameter names as a set for quick lookup
    let param_names: HashSet<&str> = node_type
        .parameters
        .iter()
        .map(|p| p.name.as_str())
        .collect();

    // Find max parameter name length for alignment
    let max_param_len = node_type
        .parameters
        .iter()
        .map(|p| p.name.len())
        .max()
        .unwrap_or(0);

    // Find max type length for alignment
    let max_type_len = node_type
        .parameters
        .iter()
        .map(|p| p.data_type.to_string().len())
        .max()
        .unwrap_or(0);

    let mut output = String::new();

    // Header
    writeln!(output, "Node: {}", node_type.name).unwrap();
    writeln!(output, "Category: {:?}", node_type.category).unwrap();
    writeln!(output, "Description: {}", node_type.description).unwrap();
    writeln!(output).unwrap();

    // Parameters section
    if !node_type.parameters.is_empty() {
        writeln!(output, "Parameters (input pins):").unwrap();

        for param in &node_type.parameters {
            let type_str = param.data_type.to_string();
            let default_info = if let Some((_, default_val)) = prop_map.get(&param.name) {
                format!("[default: {}]", default_val)
            } else {
                "[no default - wire only]".to_string()
            };

            writeln!(
                output,
                "  {:name_width$} : {:type_width$}  {}",
                param.name,
                type_str,
                default_info,
                name_width = max_param_len,
                type_width = max_type_len
            )
            .unwrap();
        }

        writeln!(output).unwrap();
    }

    // Properties section (stored-only, not in parameters)
    let stored_only: Vec<_> = text_props
        .iter()
        .filter(|(name, _)| !param_names.contains(name.as_str()))
        .collect();

    if !stored_only.is_empty() {
        writeln!(output, "Properties (not wirable):").unwrap();

        let max_prop_len = stored_only.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
        let max_prop_type_len = stored_only
            .iter()
            .map(|(_, value)| value.inferred_data_type().to_string().len())
            .max()
            .unwrap_or(0);

        for (name, value) in stored_only {
            let type_str = value.inferred_data_type().to_string();
            let value_str = value.to_text();
            writeln!(
                output,
                "  {:name_width$} : {:type_width$}  [default: {}]",
                name,
                type_str,
                value_str,
                name_width = max_prop_len,
                type_width = max_prop_type_len
            )
            .unwrap();
        }

        writeln!(output).unwrap();
    }

    // Output type
    writeln!(output, "Output: {}", node_type.output_type.to_string()).unwrap();

    output
}

/// Truncate a description for display in verbose node listing.
///
/// Algorithm:
/// 1. Take first line only (split on newline)
/// 2. If > 150 chars, find first ". " and truncate there
/// 3. If no ". " found within 150 chars, truncate at word boundary and add "..."
pub fn truncate_description(description: &str) -> String {
    const MAX_LEN: usize = 150;

    // Step 1: Take first line only
    let first_line = description.lines().next().unwrap_or("");

    // If short enough, return as-is
    if first_line.len() <= MAX_LEN {
        return first_line.to_string();
    }

    // Step 2: Try to find first sentence ending (". ") within limit
    if let Some(period_pos) = first_line[..MAX_LEN].find(". ") {
        return first_line[..=period_pos].to_string();
    }

    // Also check for period at end of the truncation range
    if first_line.as_bytes().get(MAX_LEN - 1) == Some(&b'.') {
        return first_line[..MAX_LEN].to_string();
    }

    // Step 3: Truncate at word boundary and add "..."
    let truncated = &first_line[..MAX_LEN];
    if let Some(last_space) = truncated.rfind(' ') {
        format!("{}...", &truncated[..last_space])
    } else {
        // No space found, just truncate
        format!("{}...", truncated)
    }
}
