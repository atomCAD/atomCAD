//! Selection factoring module for converting node selections into reusable subnetworks.
//!
//! This module provides functionality to analyze a selection of nodes and factor them
//! into a new subnetwork (custom node type). The selection must be a "single-output subset"
//! meaning at most one wire exits the selection to nodes outside it.

use std::collections::{HashMap, HashSet};
use glam::f64::DVec2;

use super::data_type::DataType;
use super::node_network::{Argument, Node, NodeDisplayType, NodeNetwork};
use super::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use super::node_type_registry::NodeTypeRegistry;
use super::node_data::CustomNodeData;
use super::nodes::parameter::ParameterData;
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;

/// Information about an external input wire (wire coming INTO the selection from OUTSIDE)
#[derive(Debug, Clone)]
pub struct ExternalInput {
    /// Node ID outside the selection that provides input
    pub source_node_id: u64,
    /// Output pin index of the source node
    pub source_output_pin_index: i32,
    /// Node ID inside the selection that receives the input
    pub destination_node_id: u64,
    /// Parameter index on the destination node
    pub destination_param_index: usize,
    /// Data type of the input
    pub data_type: DataType,
    /// Suggested parameter name based on source node
    pub suggested_name: String,
}

/// Information about an external output wire (wire going OUT OF the selection to OUTSIDE)
#[derive(Debug, Clone)]
pub struct ExternalOutput {
    /// Node ID inside the selection that provides the output
    pub source_node_id: u64,
    /// Output pin index of the source node
    pub source_output_pin_index: i32,
    /// Node ID outside the selection that receives the output
    pub destination_node_id: u64,
    /// Parameter index on the destination node
    pub destination_param_index: usize,
}

/// Result of analyzing a selection for factoring
#[derive(Debug, Clone)]
pub struct SelectionAnalysis {
    /// Nodes inside the selection
    pub selected_ids: HashSet<u64>,
    /// Wires coming INTO the selection from OUTSIDE
    /// Sorted by destination node Y-coordinate (for parameter ordering)
    pub external_inputs: Vec<ExternalInput>,
    /// Wire going OUT OF the selection to OUTSIDE (0 or 1 for valid selection)
    pub external_output: Option<ExternalOutput>,
    /// Whether the selection is valid for factoring
    pub is_valid: bool,
    /// If not valid, the reason why
    pub invalid_reason: Option<String>,
    /// Bounding box of selection (min, max)
    pub bounding_box: (DVec2, DVec2),
}

/// Creates an invalid SelectionAnalysis with the given reason
fn invalid(reason: &str) -> SelectionAnalysis {
    SelectionAnalysis {
        selected_ids: HashSet::new(),
        external_inputs: Vec::new(),
        external_output: None,
        is_valid: false,
        invalid_reason: Some(reason.to_string()),
        bounding_box: (DVec2::ZERO, DVec2::ZERO),
    }
}

/// Gets the output type of a node's output pin
fn get_output_type(
    network: &NodeNetwork,
    node_id: u64,
    pin_index: i32,
    registry: &NodeTypeRegistry,
) -> DataType {
    if let Some(node) = network.nodes.get(&node_id) {
        if let Some(node_type) = registry.get_node_type_for_node(node) {
            return node_type.get_output_pin_type(pin_index);
        }
    }
    DataType::None
}

/// Generates a suggested parameter name from the source node and destination parameter.
/// Format: {source_node_name}_{destination_param_name}
/// For function pins (pin_index == -1), appends "_fn" instead of param name.
fn generate_param_name(
    source_node_id: u64,
    source_pin_index: i32,
    dest_node_id: u64,
    dest_param_index: usize,
    nodes: &HashMap<u64, Node>,
    registry: &NodeTypeRegistry,
) -> String {
    // Get source node name
    let source_name = if let Some(node) = nodes.get(&source_node_id) {
        node.custom_name.as_ref()
            .unwrap_or(&node.node_type_name)
            .clone()
    } else {
        "input".to_string()
    };

    // For function pins, just append "_fn"
    if source_pin_index == -1 {
        return format!("{}_fn", source_name);
    }

    // Get destination parameter name from the node type
    let dest_param_name = if let Some(dest_node) = nodes.get(&dest_node_id) {
        if let Some(node_type) = registry.get_node_type_for_node(dest_node) {
            node_type.parameters.get(dest_param_index)
                .map(|p| p.name.clone())
        } else {
            None
        }
    } else {
        None
    };

    // Combine source name with destination parameter name
    match dest_param_name {
        Some(param_name) => format!("{}_{}", source_name, param_name),
        None => source_name,
    }
}

/// Deduplicates external inputs, keeping one entry per unique (source_node_id, pin_index)
fn deduplicate_external_inputs(inputs: Vec<ExternalInput>) -> Vec<ExternalInput> {
    let mut seen: HashSet<(u64, i32)> = HashSet::new();
    inputs.into_iter()
        .filter(|input| seen.insert((input.source_node_id, input.source_output_pin_index)))
        .collect()
}

/// Calculates the bounding box of the selected nodes
fn calculate_bounding_box(
    selected_ids: &HashSet<u64>,
    nodes: &HashMap<u64, Node>,
) -> (DVec2, DVec2) {
    let mut min = DVec2::new(f64::MAX, f64::MAX);
    let mut max = DVec2::new(f64::MIN, f64::MIN);

    for &node_id in selected_ids {
        if let Some(node) = nodes.get(&node_id) {
            min = DVec2::new(min.x.min(node.position.x), min.y.min(node.position.y));
            max = DVec2::new(max.x.max(node.position.x), max.y.max(node.position.y));
        }
    }

    if min.x > max.x || min.y > max.y {
        // No valid positions found
        return (DVec2::ZERO, DVec2::ZERO);
    }

    (min, max)
}

/// Calculates the center point of a bounding box
fn calculate_center(bounding_box: &(DVec2, DVec2)) -> DVec2 {
    (bounding_box.0 + bounding_box.1) / 2.0
}

/// Analyzes the current selection in a network for factoring eligibility.
/// Returns information about external inputs/outputs and validity.
pub fn analyze_selection_for_factoring(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> SelectionAnalysis {
    // 1. Check minimum node count
    if network.selected_node_ids.is_empty() {
        return invalid("Select at least 1 node");
    }

    // 2. Check for Parameter nodes in selection
    for &node_id in &network.selected_node_ids {
        if let Some(node) = network.nodes.get(&node_id) {
            if node.node_type_name == "parameter" {
                return invalid("Selection contains Parameter nodes");
            }
        }
    }

    // 3. Find external inputs and outputs
    let mut external_inputs = Vec::new();
    let mut external_outputs = Vec::new();

    for &node_id in &network.selected_node_ids {
        let node = match network.nodes.get(&node_id) {
            Some(n) => n,
            None => continue,
        };

        // Check each argument for external inputs
        for (param_idx, arg) in node.arguments.iter().enumerate() {
            for (&source_id, &pin_idx) in &arg.argument_output_pins {
                if !network.selected_node_ids.contains(&source_id) {
                    // This is an external input
                    let data_type = get_output_type(network, source_id, pin_idx, registry);
                    let suggested_name = generate_param_name(
                        source_id,
                        pin_idx,
                        node_id,
                        param_idx,
                        &network.nodes,
                        registry,
                    );
                    external_inputs.push(ExternalInput {
                        source_node_id: source_id,
                        source_output_pin_index: pin_idx,
                        destination_node_id: node_id,
                        destination_param_index: param_idx,
                        data_type,
                        suggested_name,
                    });
                }
            }
        }
    }

    // Check all nodes for outputs going outside selection
    for (&other_id, other_node) in &network.nodes {
        if network.selected_node_ids.contains(&other_id) {
            continue; // Skip nodes in selection
        }

        for (param_idx, arg) in other_node.arguments.iter().enumerate() {
            for (&source_id, &pin_idx) in &arg.argument_output_pins {
                if network.selected_node_ids.contains(&source_id) {
                    // This is an external output
                    external_outputs.push(ExternalOutput {
                        source_node_id: source_id,
                        source_output_pin_index: pin_idx,
                        destination_node_id: other_id,
                        destination_param_index: param_idx,
                    });
                }
            }
        }
    }

    // 4. Validate single-output constraint
    if external_outputs.len() > 1 {
        return invalid("Selection has multiple output wires");
    }

    // 5. Sort external inputs by destination node Y-coordinate
    external_inputs.sort_by(|a, b| {
        let y_a = network.nodes.get(&a.destination_node_id).map(|n| n.position.y).unwrap_or(0.0);
        let y_b = network.nodes.get(&b.destination_node_id).map(|n| n.position.y).unwrap_or(0.0);
        y_a.partial_cmp(&y_b).unwrap_or(std::cmp::Ordering::Equal)
    });

    // 6. Deduplicate inputs (same source may connect to multiple destinations)
    let external_inputs = deduplicate_external_inputs(external_inputs);

    // 7. Make suggested names unique
    let external_inputs = make_names_unique(external_inputs);

    // 8. Calculate bounding box
    let bounding_box = calculate_bounding_box(&network.selected_node_ids, &network.nodes);

    SelectionAnalysis {
        selected_ids: network.selected_node_ids.clone(),
        external_inputs,
        external_output: external_outputs.into_iter().next(),
        is_valid: true,
        invalid_reason: None,
        bounding_box,
    }
}

/// Make suggested parameter names unique by appending _2, _3, etc. for duplicates
fn make_names_unique(mut inputs: Vec<ExternalInput>) -> Vec<ExternalInput> {
    let mut name_counts: HashMap<String, usize> = HashMap::new();

    for input in inputs.iter_mut() {
        let base_name = input.suggested_name.clone();
        let count = name_counts.entry(base_name.clone()).or_insert(0);
        *count += 1;

        if *count > 1 {
            input.suggested_name = format!("{}_{}", base_name, count);
        }
    }

    // Second pass: if a name was duplicated, rename the first occurrence too
    let mut final_counts: HashMap<String, usize> = HashMap::new();
    for input in inputs.iter() {
        *final_counts.entry(input.suggested_name.clone()).or_insert(0) += 1;
    }

    // If we have duplicates due to the naming above, handle it
    let mut seen_names: HashSet<String> = HashSet::new();
    for input in inputs.iter_mut() {
        let original = input.suggested_name.clone();
        let mut name = original.clone();
        let mut counter = 1;
        while seen_names.contains(&name) {
            counter += 1;
            // Strip any existing suffix like _2, _3 and re-add
            let base = if let Some(idx) = original.rfind('_') {
                if original[idx+1..].parse::<usize>().is_ok() {
                    original[..idx].to_string()
                } else {
                    original.clone()
                }
            } else {
                original.clone()
            };
            name = format!("{}_{}", base, counter);
        }
        seen_names.insert(name.clone());
        input.suggested_name = name;
    }

    inputs
}

/// Creates a new NodeNetwork from the selected nodes.
/// The new network contains Parameter nodes for external inputs and has the
/// appropriate return node set if there's an external output.
pub fn create_subnetwork_from_selection(
    source_network: &NodeNetwork,
    analysis: &SelectionAnalysis,
    subnetwork_name: &str,
    param_names: &[String],
    registry: &NodeTypeRegistry,
) -> NodeNetwork {
    // 1. Determine output type
    let output_type = if let Some(ref output) = analysis.external_output {
        get_output_type(source_network, output.source_node_id, output.source_output_pin_index, registry)
    } else {
        DataType::None
    };

    // 2. Create parameters from external inputs
    let parameters: Vec<Parameter> = analysis.external_inputs.iter()
        .enumerate()
        .map(|(i, input)| Parameter {
            id: Some(i as u64 + 1),
            name: param_names[i].clone(),
            data_type: input.data_type.clone(),
        })
        .collect();

    // 3. Create NodeType for the subnetwork
    let node_type = NodeType {
        name: subnetwork_name.to_string(),
        description: "Custom node factored from selection".to_string(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters,
        output_type,
        public: true,
        node_data_creator: || Box::new(CustomNodeData::default()),
        node_data_saver: generic_node_data_saver::<CustomNodeData>,
        node_data_loader: generic_node_data_loader::<CustomNodeData>,
    };

    // 4. Create new NodeNetwork
    let mut new_network = NodeNetwork::new(node_type);

    // 5. Build ID mapping and copy nodes
    let mut id_mapping: HashMap<u64, u64> = HashMap::new();
    let center = calculate_center(&analysis.bounding_box);

    for &old_id in &analysis.selected_ids {
        let old_node = match source_network.nodes.get(&old_id) {
            Some(n) => n,
            None => continue,
        };

        let new_id = new_network.next_node_id;
        new_network.next_node_id += 1;
        id_mapping.insert(old_id, new_id);

        // Create new node with adjusted position
        let new_position = old_node.position - center;
        let new_node = Node {
            id: new_id,
            node_type_name: old_node.node_type_name.clone(),
            custom_name: old_node.custom_name.clone(),
            position: new_position,
            arguments: old_node.arguments.clone(), // Will rewire below
            data: old_node.data.clone_box(),
            custom_node_type: old_node.custom_node_type.clone(),
        };
        new_network.nodes.insert(new_id, new_node);

        // Inherit display status
        if let Some(display_type) = source_network.get_node_display_type(old_id) {
            new_network.set_node_display_type(new_id, Some(display_type));
        }
    }

    // 6. Create Parameter nodes for external inputs
    // Build map of (source_id, pin) -> param_index for wiring
    let mut source_to_param_index: HashMap<(u64, i32), usize> = HashMap::new();
    for (i, input) in analysis.external_inputs.iter().enumerate() {
        source_to_param_index.insert((input.source_node_id, input.source_output_pin_index), i);
    }

    let mut param_node_ids: HashMap<usize, u64> = HashMap::new(); // param_index -> param node id

    for (i, input) in analysis.external_inputs.iter().enumerate() {
        let param_id = new_network.next_node_id;
        new_network.next_node_id += 1;

        // Position parameter nodes on the left
        let y_offset = i as f64 * 80.0;
        let param_position = DVec2::new(-300.0, y_offset - (analysis.external_inputs.len() as f64 * 40.0));

        let param_data = ParameterData {
            param_id: Some(new_network.next_param_id),
            param_index: i,
            param_name: param_names[i].clone(),
            data_type: input.data_type.clone(),
            sort_order: i as i32,
            data_type_str: None,
            error: None,
        };
        new_network.next_param_id += 1;

        let param_node = Node {
            id: param_id,
            node_type_name: "parameter".to_string(),
            custom_name: Some(param_names[i].clone()),
            position: param_position,
            arguments: vec![Argument::new()], // Default input
            data: Box::new(param_data),
            custom_node_type: None,
        };

        new_network.nodes.insert(param_id, param_node);
        param_node_ids.insert(i, param_id);
    }

    // 7. Rewire internal connections
    for &new_id in id_mapping.values() {
        let node = new_network.nodes.get_mut(&new_id).unwrap();
        for arg in &mut node.arguments {
            let mut new_pins: HashMap<u64, i32> = HashMap::new();

            for (&source_id, &pin_idx) in &arg.argument_output_pins {
                if let Some(&mapped_id) = id_mapping.get(&source_id) {
                    // Internal connection - use mapped ID
                    new_pins.insert(mapped_id, pin_idx);
                } else {
                    // External input - connect to parameter node
                    let key = (source_id, pin_idx);
                    if let Some(&param_index) = source_to_param_index.get(&key) {
                        if let Some(&param_id) = param_node_ids.get(&param_index) {
                            new_pins.insert(param_id, 0); // Parameter output is pin 0
                        }
                    }
                }
            }

            arg.argument_output_pins = new_pins;
        }
    }

    // 8. Set return node if there's an external output
    if let Some(ref output) = analysis.external_output {
        if let Some(&new_return_id) = id_mapping.get(&output.source_node_id) {
            new_network.return_node_id = Some(new_return_id);
        }
    }

    // Initialize custom node types for all nodes in the new network
    registry.initialize_custom_node_types_for_network(&mut new_network);

    new_network
}

/// Replaces the selected nodes with a single custom node instance.
/// Wires up the custom node's inputs and output, then removes the original nodes.
/// Returns the ID of the newly created custom node.
pub fn replace_selection_with_custom_node(
    network: &mut NodeNetwork,
    analysis: &SelectionAnalysis,
    subnetwork_name: &str,
    num_params: usize,
) -> u64 {
    // 1. Calculate position (center of selection)
    let center = calculate_center(&analysis.bounding_box);

    // 2. Determine display type for new node
    let display_type = if let Some(ref output) = analysis.external_output {
        network.get_node_display_type(output.source_node_id)
    } else {
        Some(NodeDisplayType::Normal) // Default to visible
    };

    // 3. Create custom node
    let new_node_id = network.add_node(
        subnetwork_name,
        center,
        num_params,
        Box::new(CustomNodeData::default()),
    );

    // 4. Set display type
    network.set_node_display_type(new_node_id, display_type);

    // 5. Wire inputs to custom node
    // Build map of (source_id, pin) -> param_index
    let mut input_map: HashMap<(u64, i32), usize> = HashMap::new();
    for (i, input) in analysis.external_inputs.iter().enumerate() {
        input_map.entry((input.source_node_id, input.source_output_pin_index))
            .or_insert(i);
    }

    for input in &analysis.external_inputs {
        let param_idx = input_map[&(input.source_node_id, input.source_output_pin_index)];
        network.connect_nodes(
            input.source_node_id,
            input.source_output_pin_index,
            new_node_id,
            param_idx,
            false, // not multi
        );
    }

    // 6. Wire custom node output (if any)
    if let Some(ref output) = analysis.external_output {
        network.connect_nodes(
            new_node_id,
            0, // Main output pin
            output.destination_node_id,
            output.destination_param_index,
            false,
        );
    }

    // 7. Remove selected nodes
    for &node_id in &analysis.selected_ids {
        // Remove from other nodes' arguments
        let nodes_to_process: Vec<u64> = network.nodes.keys().cloned().collect();
        for other_id in nodes_to_process {
            if let Some(node) = network.nodes.get_mut(&other_id) {
                for arg in &mut node.arguments {
                    arg.argument_output_pins.remove(&node_id);
                }
            }
        }

        // Remove from displayed nodes
        network.displayed_node_ids.remove(&node_id);

        // Remove the node
        network.nodes.remove(&node_id);
    }

    // 8. Clear selection and select new node
    network.clear_selection();
    network.select_node(new_node_id);

    new_node_id
}
