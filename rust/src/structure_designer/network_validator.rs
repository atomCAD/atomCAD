use crate::structure_designer::data_type::DataType;
use crate::structure_designer::node_network::{Argument, NodeNetwork, ValidationError};
use crate::structure_designer::node_type::{OutputPinDefinition, Parameter, PinOutputType};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::nodes::parameter::ParameterData;
use std::cmp::Ordering;
use std::collections::HashMap;

/// Per-validation-run cache of resolved concrete output types, keyed by
/// `(node_id, output_pin_index)`. A `None` entry means "we tried to resolve
/// and failed" (unresolved — treated as disconnected downstream).
#[derive(Default)]
pub struct ValidationContext {
    resolved_outputs: HashMap<(u64, i32), Option<DataType>>,
}

impl ValidationContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve (with memoization) the concrete output type of `(node_id, pin_index)`.
    pub fn resolve(
        &mut self,
        network: &NodeNetwork,
        registry: &NodeTypeRegistry,
        node_id: u64,
        output_pin_index: i32,
    ) -> Option<DataType> {
        if let Some(cached) = self.resolved_outputs.get(&(node_id, output_pin_index)) {
            return cached.clone();
        }
        // Insert a tentative None to guard against infinite recursion on malformed
        // cyclic graphs; real cycles should be rejected elsewhere.
        self.resolved_outputs
            .insert((node_id, output_pin_index), None);
        let node = network.nodes.get(&node_id)?;
        let resolved = registry.resolve_output_type(node, network, output_pin_index);
        self.resolved_outputs
            .insert((node_id, output_pin_index), resolved.clone());
        resolved
    }
}

#[derive(Debug, Clone)]
pub struct NetworkValidationResult {
    pub valid: bool,
    pub interface_changed: bool,
}

/// Compares two parameters for deterministic sorting.
/// Primary sort key: sort_order (ascending)
/// Secondary sort key: node_id (ascending)
fn compare_parameters(
    node_id_a: u64,
    param_data_a: &ParameterData,
    node_id_b: u64,
    param_data_b: &ParameterData,
) -> Ordering {
    param_data_a
        .sort_order
        .cmp(&param_data_b.sort_order)
        .then_with(|| node_id_a.cmp(&node_id_b))
}

/// Repairs call sites when a network's parameter interface changes.
/// This function updates all nodes that use the given network as their type,
/// preserving argument connections based on parameter IDs (primary) or names (fallback).
fn repair_call_sites_for_network(
    network_name: &str,
    old_parameters: &[Parameter],
    new_parameters: &[Parameter],
    node_type_registry: &mut NodeTypeRegistry,
) {
    // Build mapping: parameter_id -> old_index (primary matching strategy)
    let old_param_id_map: HashMap<u64, usize> = old_parameters
        .iter()
        .enumerate()
        .filter_map(|(idx, param)| param.id.map(|id| (id, idx)))
        .collect();

    // Build mapping: parameter_name -> old_index (fallback for backwards compatibility)
    let old_param_name_map: HashMap<&str, usize> = old_parameters
        .iter()
        .enumerate()
        .map(|(idx, param)| (param.name.as_str(), idx))
        .collect();

    // Find all parent networks that use this network
    let parent_network_names = node_type_registry.find_parent_networks(network_name);

    // Update each parent network's call sites
    for parent_name in parent_network_names {
        if let Some(parent_network) = node_type_registry.node_networks.get_mut(&parent_name) {
            // Find all nodes in parent that use our network
            let mut nodes_to_update: Vec<(u64, Vec<Argument>)> = Vec::new();

            for (node_id, node) in &parent_network.nodes {
                if node.node_type_name == network_name {
                    // This node needs argument updates
                    let mut new_arguments = Vec::with_capacity(new_parameters.len());

                    // For each new parameter, try to preserve old argument
                    for new_param in new_parameters {
                        let old_idx = {
                            // First try ID-based matching (handles renames)
                            if let Some(new_id) = new_param.id {
                                if let Some(&idx) = old_param_id_map.get(&new_id) {
                                    Some(idx)
                                } else {
                                    // Fall back to name-based matching
                                    old_param_name_map.get(new_param.name.as_str()).copied()
                                }
                            } else {
                                // No ID, use name-based matching (backwards compatibility)
                                old_param_name_map.get(new_param.name.as_str()).copied()
                            }
                        };

                        if let Some(old_idx) = old_idx {
                            // Parameter existed before - preserve its argument if within bounds
                            if old_idx < node.arguments.len() {
                                new_arguments.push(node.arguments[old_idx].clone());
                            } else {
                                // Shouldn't happen, but handle gracefully
                                new_arguments.push(Argument::new());
                            }
                        } else {
                            // New parameter - create empty argument
                            new_arguments.push(Argument::new());
                        }
                    }

                    nodes_to_update.push((*node_id, new_arguments));
                }
            }

            // Apply updates
            for (node_id, new_arguments) in nodes_to_update {
                if let Some(node) = parent_network.nodes.get_mut(&node_id) {
                    node.arguments = new_arguments;
                }
            }
        }
    }
}

fn validate_parameters(network: &mut NodeNetwork) -> bool {
    // Collect all parameter nodes
    let mut parameter_nodes: Vec<(u64, &ParameterData)> = Vec::new();

    for (node_id, node) in &network.nodes {
        if node.node_type_name == "parameter" {
            // Cast node data to ParameterData
            if let Some(param_data) = (*node.data).as_any_ref().downcast_ref::<ParameterData>() {
                parameter_nodes.push((*node_id, param_data));
            } else {
                network.validation_errors.push(ValidationError::new(
                    "Parameter node has invalid data type".to_string(),
                    Some(*node_id),
                ));
                return false;
            }
        }
    }

    // Validate param_name uniqueness
    let mut param_names: HashMap<String, u64> = HashMap::new();
    for (node_id, param_data) in &parameter_nodes {
        if let Some(_existing_node_id) = param_names.get(&param_data.param_name) {
            network.validation_errors.push(ValidationError::new(
                format!("Duplicate parameter name '{}'", param_data.param_name),
                Some(*node_id),
            ));
            return false;
        } else {
            param_names.insert(param_data.param_name.clone(), *node_id);
        }
    }

    // Reject abstract parameter types: abstract types may only appear as declared
    // input-pin types on built-in polymorphic nodes, not on user-declared parameter pins.
    for (node_id, param_data) in &parameter_nodes {
        if contains_abstract(&param_data.data_type) {
            network.validation_errors.push(ValidationError::new(
                format!(
                    "Parameter '{}' has abstract type {:?}; abstract phase types are not allowed on parameter pins",
                    param_data.param_name, param_data.data_type
                ),
                Some(*node_id),
            ));
            return false;
        }
    }

    // Sort parameter nodes by sort_order (primary) and node_id (secondary)
    // This ensures deterministic ordering even when multiple parameters have the same sort_order
    parameter_nodes.sort_by(|(node_id_a, param_data_a), (node_id_b, param_data_b)| {
        compare_parameters(*node_id_a, param_data_a, *node_id_b, param_data_b)
    });

    // Recreate the parameters array based on sort order, propagating IDs for wire preservation
    network.node_type.parameters = parameter_nodes
        .iter()
        .map(|(_, param_data)| {
            Parameter {
                id: param_data.param_id, // Propagate ID for wire preservation across renames
                name: param_data.param_name.clone(),
                data_type: param_data.data_type.clone(),
            }
        })
        .collect();

    // Update param_index for each parameter node
    // Collect node IDs and their new indices to avoid borrowing conflicts
    let param_updates: Vec<(u64, usize)> = parameter_nodes
        .iter()
        .enumerate()
        .map(|(index, (node_id, _))| (*node_id, index))
        .collect();

    for (node_id, new_index) in param_updates {
        if let Some(node) = network.nodes.get_mut(&node_id) {
            if let Some(param_data) = (*node.data).as_any_mut().downcast_mut::<ParameterData>() {
                param_data.param_index = new_index;
            }
        }
    }

    true
}

fn check_interface_changed(network: &NodeNetwork) -> bool {
    // Collect current parameter nodes with their IDs for deterministic sorting
    let mut current_params_with_ids: Vec<(u64, &ParameterData)> = Vec::new();

    for (node_id, node) in &network.nodes {
        if node.node_type_name == "parameter" {
            if let Some(param_data) = (*node.data).as_any_ref().downcast_ref::<ParameterData>() {
                current_params_with_ids.push((*node_id, param_data));
            }
        }
    }

    // Sort by sort_order (primary) and node_id (secondary) for deterministic comparison
    current_params_with_ids.sort_by(|(node_id_a, param_data_a), (node_id_b, param_data_b)| {
        compare_parameters(*node_id_a, param_data_a, *node_id_b, param_data_b)
    });

    // Check if the interface changed by comparing with existing parameters
    if network.node_type.parameters.len() != current_params_with_ids.len() {
        return true;
    }

    current_params_with_ids
        .iter()
        .enumerate()
        .any(|(index, (_, param_data))| {
            if let Some(existing_param) = network.node_type.parameters.get(index) {
                existing_param.name != param_data.param_name
                    || existing_param.data_type != param_data.data_type
            } else {
                true
            }
        })
}

/// Repairs argument counts in the network to match parameter counts.
/// This ensures all nodes have the correct number of arguments for their type.
fn repair_network_arguments(network: &mut NodeNetwork, node_type_registry: &NodeTypeRegistry) {
    let mut nodes_to_fix = Vec::new();

    // Collect nodes that need argument count adjustments
    for (dest_node_id, dest_node) in &network.nodes {
        if let Some(dest_node_type) = node_type_registry.get_node_type_for_node(dest_node) {
            let expected_param_count = dest_node_type.parameters.len();
            let current_arg_count = dest_node.arguments.len();

            if current_arg_count != expected_param_count {
                nodes_to_fix.push((*dest_node_id, expected_param_count, current_arg_count));
            }
        }
    }

    // Apply argument count fixes
    for (node_id, expected_count, current_count) in nodes_to_fix {
        let dest_node_mut = network.nodes.get_mut(&node_id).unwrap();

        if current_count < expected_count {
            // Add empty arguments when too few
            for _ in current_count..expected_count {
                dest_node_mut.arguments.push(Argument::new());
            }
        } else {
            // Remove excess arguments when too many
            dest_node_mut.arguments.truncate(expected_count);
        }
    }
}

/// Removes wire connections that reference output pins that no longer exist on the source node.
/// This handles the case where a custom network's return node changes from multi-output to
/// single-output, leaving dangling wires to pins that were removed.
fn repair_output_pin_wires(network: &mut NodeNetwork, node_type_registry: &NodeTypeRegistry) {
    // First pass: build a map of node_id -> output_pin_count for all nodes
    let pin_counts: HashMap<u64, usize> = network
        .nodes
        .iter()
        .filter_map(|(&node_id, node)| {
            node_type_registry
                .get_node_type_for_node(node)
                .map(|nt| (node_id, nt.output_pin_count()))
        })
        .collect();

    // Second pass: remove wires to non-existent output pins
    for node in network.nodes.values_mut() {
        for argument in node.arguments.iter_mut() {
            argument
                .argument_output_pins
                .retain(|source_node_id, output_pin_index| {
                    if *output_pin_index == -1 {
                        return true; // Function pin is always valid
                    }
                    if let Some(&count) = pin_counts.get(source_node_id) {
                        (*output_pin_index as usize) < count
                    } else {
                        true // Unknown source — let validate_wires catch it
                    }
                });
        }
    }
}

/// Returns true if `t` is itself abstract or contains an abstract type inside
/// an `Array[..]` wrapper. Used for guards on user-declared type fields
/// (parameter pins, sequence element_type) where abstract is always invalid.
fn contains_abstract(t: &DataType) -> bool {
    match t {
        _ if t.is_abstract() => true,
        DataType::Array(inner) => contains_abstract(inner),
        _ => false,
    }
}

fn validate_wires(
    network: &mut NodeNetwork,
    node_type_registry: &NodeTypeRegistry,
    ctx: &mut ValidationContext,
) -> bool {
    // Validate wires - pure checking, no repairs
    for (dest_node_id, dest_node) in &network.nodes {
        // Check if this node references a node network and validate its validity
        if let Some(referenced_network) = node_type_registry
            .node_networks
            .get(&dest_node.node_type_name)
        {
            if !referenced_network.valid {
                network.validation_errors.push(ValidationError::new(
                    format!(
                        "References invalid node network '{}'",
                        dest_node.node_type_name
                    ),
                    Some(*dest_node_id),
                ));
                return false;
            }
        }

        // Get the destination node type to access parameter information
        let dest_node_type = match node_type_registry.get_node_type_for_node(dest_node) {
            Some(node_type) => node_type,
            None => {
                network.validation_errors.push(ValidationError::new(
                    format!("Unknown node type '{}'", dest_node.node_type_name),
                    Some(*dest_node_id),
                ));
                return false;
            }
        };

        // Validate argument count matches parameter count
        // (This should always pass after repair phase)
        if dest_node.arguments.len() != dest_node_type.parameters.len() {
            network.validation_errors.push(ValidationError::new(
                format!(
                    "Node has {} arguments but type expects {} parameters",
                    dest_node.arguments.len(),
                    dest_node_type.parameters.len()
                ),
                Some(*dest_node_id),
            ));
            return false;
        }

        // Validate each argument (input pin) of the destination node
        for (arg_index, argument) in dest_node.arguments.iter().enumerate() {
            // Get parameter information for this argument
            let parameter = &dest_node_type.parameters[arg_index];

            // Validate non-multi input pins have at most one connection
            if !parameter.data_type.is_array() && argument.argument_output_pins.len() > 1 {
                network.validation_errors.push(ValidationError::new(
                    format!(
                        "Non-multi parameter '{}' has {} connections, but only 1 is allowed",
                        parameter.name,
                        argument.argument_output_pins.len()
                    ),
                    Some(*dest_node_id),
                ));
                return false;
            }

            // Validate data types for each connected source node
            for (source_node_id, output_pin_index) in &argument.argument_output_pins {
                // Get the source node
                let source_node = match network.nodes.get(source_node_id) {
                    Some(node) => node,
                    None => {
                        network.validation_errors.push(ValidationError::new(
                            "Wire references non-existent source node".to_string(),
                            Some(*dest_node_id),
                        ));
                        return false;
                    }
                };

                // Check if this source node references a node network and validate its validity
                if let Some(referenced_network) = node_type_registry
                    .node_networks
                    .get(&source_node.node_type_name)
                {
                    if !referenced_network.valid {
                        network.validation_errors.push(ValidationError::new(
                            format!(
                                "Source node references invalid node network '{}'",
                                source_node.node_type_name
                            ),
                            Some(*source_node_id),
                        ));
                        return false;
                    }
                }

                // Get the source node type to access its output type
                let _source_node_type = match node_type_registry.get_node_type_for_node(source_node)
                {
                    Some(node_type) => node_type,
                    None => {
                        network.validation_errors.push(ValidationError::new(
                            format!("Unknown source node type '{}'", source_node.node_type_name),
                            Some(*source_node_id),
                        ));
                        return false;
                    }
                };

                // Validate data type compatibility using the resolved concrete
                // source type. If resolution fails (unresolved polymorphic
                // output upstream), treat the wire as disconnected — the
                // upstream node itself is flagged invalid below.
                let source_data_type = match ctx.resolve(
                    network,
                    node_type_registry,
                    *source_node_id,
                    *output_pin_index,
                ) {
                    Some(t) => t,
                    None => continue,
                };
                let dest_data_type =
                    node_type_registry.get_node_param_data_type(dest_node, arg_index);
                if !DataType::can_be_converted_to(&source_data_type, &dest_data_type) {
                    network.validation_errors.push(ValidationError::new(
                        format!(
                            "Data type mismatch: input expects {:?}, but source outputs {:?}",
                            parameter.data_type, source_data_type
                        ),
                        Some(*dest_node_id),
                    ));
                    return false;
                }
            }

            // Note: a direct "abstract input pin unconnected → invalid" check
            // is subsumed by the polymorphic-output-unresolved check below
            // once a node's outputs are migrated to `SameAsInput` /
            // `SameAsArrayElements`. Not-yet-migrated nodes still declare
            // `Fixed(Atomic)` on their outputs, and enforcing the rule on
            // their abstract input pins directly would flag existing valid
            // graphs invalid before migration lands. The uniform rule is
            // applied via the output-resolution check below.
        }

        // Polymorphic output pins must resolve to a concrete type. If any
        // output is unresolved, the node is flagged invalid. This is the
        // uniform rule that covers both single-input SameAsInput pins
        // (disconnected input) and SameAsArrayElements pins (mixed phases,
        // empty arrays, upstream unresolved).
        for pin_index_usize in 0..dest_node_type.output_pin_count() {
            let pin_index = pin_index_usize as i32;
            let pin = &dest_node_type.output_pins[pin_index_usize];
            let is_polymorphic = !matches!(pin.data_type, PinOutputType::Fixed(_));
            if !is_polymorphic {
                continue;
            }
            if ctx
                .resolve(network, node_type_registry, *dest_node_id, pin_index)
                .is_none()
            {
                network.validation_errors.push(ValidationError::new(
                    format!(
                        "Output pin '{}' ({}) could not be resolved to a concrete type",
                        pin.name, pin.data_type
                    ),
                    Some(*dest_node_id),
                ));
                return false;
            }
        }
    }

    true
}

pub fn validate_network(
    network: &mut NodeNetwork,
    node_type_registry: &mut NodeTypeRegistry,
    initial_errors: Option<Vec<crate::structure_designer::node_network::ValidationError>>,
) -> NetworkValidationResult {
    // Clear previous validation state
    network.valid = true;
    network.validation_errors.clear();

    // Add initial errors first if provided
    if let Some(errors) = initial_errors {
        for error in errors {
            network.validation_errors.push(error);
            network.valid = false;
        }
    }

    // Check if interface changed before validation (to detect changes)
    let interface_changed = check_interface_changed(network);

    // Store old parameters before updating them
    let old_parameters = network.node_type.parameters.clone();

    // Validate parameters (this updates parameter order and indices)
    if !validate_parameters(network) {
        network.valid = false;
        return NetworkValidationResult {
            valid: false,
            interface_changed,
        };
    }

    // REPAIR PHASE: Update call sites if interface changed
    if interface_changed {
        let new_parameters = network.node_type.parameters.clone();
        let network_name = network.node_type.name.clone();
        repair_call_sites_for_network(
            &network_name,
            &old_parameters,
            &new_parameters,
            node_type_registry,
        );
    }

    // REPAIR PHASE: Ensure argument counts match parameter counts in this network
    repair_network_arguments(network, node_type_registry);

    // REPAIR PHASE: Remove wires to output pins that no longer exist
    repair_output_pin_wires(network, node_type_registry);

    // VALIDATION PHASE: Check wire validity and resolve polymorphic output pins.
    let mut ctx = ValidationContext::new();
    let wires_valid = validate_wires(network, node_type_registry, &mut ctx);
    if !wires_valid {
        network.valid = false;
    }

    // Update the network's output type based on return node, using resolved
    // concrete types for any polymorphic pins on the return node. This runs
    // even when wires are invalid so the enclosing network can still see this
    // network's interface shape (e.g. to repair call-sites). Pins that cannot
    // be resolved fall back to DataType::None.
    let output_type_changed = update_network_output_type(network, node_type_registry, &mut ctx);

    NetworkValidationResult {
        valid: network.valid,
        interface_changed: interface_changed || output_type_changed,
    }
}

fn update_network_output_type(
    network: &mut NodeNetwork,
    node_type_registry: &NodeTypeRegistry,
    ctx: &mut ValidationContext,
) -> bool {
    let old_output_pins = network.node_type.output_pins.clone();

    // Determine the new output pins based on return_node_id. Substitute
    // `Fixed(<concrete>)` for each pin by resolving polymorphic pins against
    // the validation cache. Custom-network parameter pins are concrete
    // (enforced in `validate_parameters`), so resolution always succeeds in a
    // valid graph; unresolved pins fall back to DataType::None, which is
    // consistent with how unresolved outputs were treated previously.
    let new_output_pins = if let Some(return_node_id) = network.return_node_id {
        if let Some(return_node) = network.nodes.get(&return_node_id) {
            let return_node_type = node_type_registry
                .get_node_type_for_node(return_node)
                .unwrap();
            let mut pins = Vec::with_capacity(return_node_type.output_pins.len());
            for (pin_idx, pin) in return_node_type.output_pins.iter().enumerate() {
                // Preserve `Fixed` pins as-is so their declared types (even
                // abstract ones on not-yet-migrated nodes) reach the
                // enclosing network unchanged. For polymorphic pins,
                // substitute the resolved concrete type; if resolution fails
                // fall back to DataType::None.
                let data_type = match &pin.data_type {
                    PinOutputType::Fixed(_) => pin.data_type.clone(),
                    _ => PinOutputType::Fixed(
                        ctx.resolve(network, node_type_registry, return_node_id, pin_idx as i32)
                            .unwrap_or(DataType::None),
                    ),
                };
                pins.push(OutputPinDefinition {
                    name: pin.name.clone(),
                    data_type,
                });
            }
            pins
        } else {
            // Return node doesn't exist, set to None
            OutputPinDefinition::single(DataType::None)
        }
    } else {
        // No return node, output type is None
        OutputPinDefinition::single(DataType::None)
    };

    // Update the network's output pins
    network.node_type.output_pins = new_output_pins.clone();

    // Check if output pins changed (count or types)
    let changed = old_output_pins.len() != new_output_pins.len()
        || old_output_pins
            .iter()
            .zip(new_output_pins.iter())
            .any(|(old, new)| old.name != new.name || old.data_type != new.data_type);

    changed
}
