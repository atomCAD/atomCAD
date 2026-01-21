//! Node type introspection for generating human-readable documentation.
//!
//! This module provides functions to describe node types in detail, including
//! their parameters, default values, and output types. Used by the AI assistant
//! CLI to provide dynamic node documentation.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::structure_designer::data_type::DataType;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;

/// Returns true if a data type can only be provided via wire connection (no literal representation).
fn is_wire_only_type(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Geometry
            | DataType::Geometry2D
            | DataType::Atomic
            | DataType::Motif
            | DataType::UnitCell
            | DataType::DrawingPlane
            | DataType::Array(_)
            | DataType::Function(_)
    )
}

/// Describe a specific node type in detail.
///
/// Returns a human-readable description of the node type including:
/// - Name, category, and description
/// - Inputs with types, default values, and access modifiers (wire-only, literal-only)
/// - Output type
///
/// # Terminology
///
/// - **wire-only**: This input can only be connected to another node's output.
///   There is no text literal representation for this type (e.g., Geometry, Atomic, Motif).
/// - **literal-only**: This input can only be set as a literal value in the text format.
///   It has no input pin and cannot be connected to other nodes.
/// - Inputs without either marker can be set as a literal OR wired to another node.
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
/// Node: atom_fill
/// Category: AtomicStructure
/// Description: Converts a 3D geometry into an atomic structure...
///
/// Inputs:
///   shape          : Geometry  [required, wire-only]
///   motif          : Motif     [default: cubic zincblende, wire-only]
///   m_offset       : Vec3      [default: (0.0, 0.0, 0.0)]
///   passivate      : Bool      [default: true]
///   element_values : String    [default: "", literal-only]
///
/// Output: Atomic
/// ```
pub fn describe_node_type(node_type_name: &str, registry: &NodeTypeRegistry) -> String {
    // Look up the node type
    let node_type = match registry.get_node_type(node_type_name) {
        Some(nt) => nt,
        None => return format!("# Node type '{}' not found\n", node_type_name),
    };

    // Create a default instance to get text properties and parameter metadata
    let default_data = (node_type.node_data_creator)();
    let text_props = default_data.get_text_properties();
    let param_metadata = default_data.get_parameter_metadata();

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

    // Collect literal-only properties (in text_props but not in parameters)
    let literal_only_props: Vec<_> = text_props
        .iter()
        .filter(|(name, _)| !param_names.contains(name.as_str()))
        .collect();

    // Find max name length for alignment (considering both parameters and literal-only props)
    let max_param_name_len = node_type
        .parameters
        .iter()
        .map(|p| p.name.len())
        .max()
        .unwrap_or(0);
    let max_literal_name_len = literal_only_props
        .iter()
        .map(|(name, _)| name.len())
        .max()
        .unwrap_or(0);
    let max_name_len = max_param_name_len.max(max_literal_name_len);

    // Find max type length for alignment
    let max_param_type_len = node_type
        .parameters
        .iter()
        .map(|p| p.data_type.to_string().len())
        .max()
        .unwrap_or(0);
    let max_literal_type_len = literal_only_props
        .iter()
        .map(|(_, value)| value.inferred_data_type().to_string().len())
        .max()
        .unwrap_or(0);
    let max_type_len = max_param_type_len.max(max_literal_type_len);

    let mut output = String::new();

    // Header
    writeln!(output, "Node: {}", node_type.name).unwrap();
    writeln!(output, "Category: {:?}", node_type.category).unwrap();
    writeln!(output, "Description: {}", node_type.description).unwrap();
    writeln!(output).unwrap();

    // Inputs section (unified parameters and literal-only properties)
    let has_inputs = !node_type.parameters.is_empty() || !literal_only_props.is_empty();
    if has_inputs {
        writeln!(output, "Inputs:").unwrap();

        // Process parameters (wirable inputs)
        for param in &node_type.parameters {
            let type_str = param.data_type.to_string();
            let wire_only = is_wire_only_type(&param.data_type);

            // Determine default info from: 1) stored property, 2) parameter metadata, 3) fallback
            let default_info = if let Some((_, default_val)) = prop_map.get(&param.name) {
                // Pattern A: Property-backed default
                format!("default: {}", default_val)
            } else if let Some((is_required, default_desc)) = param_metadata.get(&param.name) {
                // Pattern B or C: from metadata
                if *is_required {
                    "required".to_string()
                } else {
                    match default_desc {
                        Some(desc) => format!("default: {}", desc),
                        None => "has default".to_string(),
                    }
                }
            } else {
                // Fallback: assume required if no property and no metadata
                "required".to_string()
            };

            // Build the info string with optional wire-only marker
            let info_str = if wire_only {
                format!("[{}, wire-only]", default_info)
            } else {
                format!("[{}]", default_info)
            };

            writeln!(
                output,
                "  {:name_width$} : {:type_width$}  {}",
                param.name,
                type_str,
                info_str,
                name_width = max_name_len,
                type_width = max_type_len
            )
            .unwrap();
        }

        // Process literal-only properties (not in parameters)
        for (name, value) in &literal_only_props {
            let type_str = value.inferred_data_type().to_string();
            let value_str = value.to_text();
            writeln!(
                output,
                "  {:name_width$} : {:type_width$}  [default: {}, literal-only]",
                name,
                type_str,
                value_str,
                name_width = max_name_len,
                type_width = max_type_len
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
