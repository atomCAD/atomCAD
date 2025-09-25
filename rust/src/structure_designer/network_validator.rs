use crate::structure_designer::node_network::{NodeNetwork, ValidationError, Argument};
use crate::structure_designer::nodes::parameter::ParameterData;
use crate::structure_designer::node_type::Parameter;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use std::collections::HashMap;
use crate::structure_designer::data_type::DataType;   

#[derive(Debug, Clone)]
pub struct NetworkValidationResult {
    pub valid: bool,
    pub interface_changed: bool,
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
                    Some(*node_id)
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
                format!("Duplicate parameter name '{}'", 
                    param_data.param_name),
                Some(*node_id)
            ));
            return false;
        } else {
            param_names.insert(param_data.param_name.clone(), *node_id);
        }
    }
    
    // Validate sort_order uniqueness
    let mut sort_orders: HashMap<i32, u64> = HashMap::new();
    for (node_id, param_data) in &parameter_nodes {
        if let Some(_existing_node_id) = sort_orders.get(&param_data.sort_order) {
            network.validation_errors.push(ValidationError::new(
                format!("Duplicate sort order {}", 
                    param_data.sort_order),
                Some(*node_id)
            ));
            return false;
        } else {
            sort_orders.insert(param_data.sort_order, *node_id);
        }
    }
    
    // Sort parameter nodes by sort_order
    parameter_nodes.sort_by_key(|(_, param_data)| param_data.sort_order);
    
    // Recreate the parameters array based on sort order
    network.node_type.parameters = parameter_nodes.iter().map(|(_, param_data)| {
        Parameter {
            name: param_data.param_name.clone(),
            data_type: param_data.data_type.clone(),
        }
    }).collect();
    
    // Update param_index for each parameter node
    // Collect node IDs and their new indices to avoid borrowing conflicts
    let param_updates: Vec<(u64, usize)> = parameter_nodes.iter().enumerate()
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
    // Collect current parameter nodes for comparison
    let mut current_params: Vec<&ParameterData> = Vec::new();
    
    for node in network.nodes.values() {
        if node.node_type_name == "parameter" {
            if let Some(param_data) = (*node.data).as_any_ref().downcast_ref::<ParameterData>() {
                current_params.push(param_data);
            }
        }
    }
    
    // Sort by sort_order for proper comparison
    current_params.sort_by_key(|param| param.sort_order);
    
    // Check if the interface changed by comparing with existing parameters
    if network.node_type.parameters.len() != current_params.len() {
        return true;
    }
    
    current_params.iter().enumerate().any(|(index, param_data)| {
        if let Some(existing_param) = network.node_type.parameters.get(index) {
            existing_param.name != param_data.param_name ||
            existing_param.data_type != param_data.data_type
        } else {
            true
        }
    })
}

fn validate_wires(network: &mut NodeNetwork, node_type_registry: &NodeTypeRegistry) -> bool {
    // First pass: collect nodes that need argument count fixes
    let mut nodes_to_fix = Vec::new();
    
    for (dest_node_id, dest_node) in &network.nodes {
        // Check if this node references a node network and validate its validity
        if let Some(referenced_network) = node_type_registry.node_networks.get(&dest_node.node_type_name) {
            if !referenced_network.valid {
                network.validation_errors.push(ValidationError::new(
                    format!("References invalid node network '{}'", dest_node.node_type_name),
                    Some(*dest_node_id)
                ));
                return false;
            }
        }
        
        // Get the destination node type to access parameter information
        let dest_node_type = match node_type_registry.get_node_type_for_node(&dest_node) {
            Some(node_type) => node_type,
            None => {
                network.validation_errors.push(ValidationError::new(
                    format!("Unknown node type '{}'", dest_node.node_type_name),
                    Some(*dest_node_id)
                ));
                return false;
            }
        };
        
        // Check if argument count needs fixing
        let expected_param_count = dest_node_type.parameters.len();
        let current_arg_count = dest_node.arguments.len();
        
        if current_arg_count != expected_param_count {
            nodes_to_fix.push((*dest_node_id, expected_param_count, current_arg_count));
        }
    }
    
    // Second pass: apply argument count fixes
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
    
    // Third pass: validate wires after fixes
    for (dest_node_id, dest_node) in &network.nodes {
        let dest_node_type = node_type_registry.get_node_type_for_node(dest_node).unwrap();
        
        // Now validate each argument (input pin) of the destination node
        // Re-get the node reference after potential modification
        let dest_node = network.nodes.get(dest_node_id).unwrap();
        for (arg_index, argument) in dest_node.arguments.iter().enumerate() {
            // Get parameter information for this argument
            let parameter = &dest_node_type.parameters[arg_index];
            
            // Validate non-multi input pins have at most one connection
            if !parameter.data_type.is_array() && argument.argument_output_pins.len() > 1 {
                network.validation_errors.push(ValidationError::new(
                        format!("Non-multi parameter '{}' has {} connections, but only 1 is allowed", 
                        parameter.name, argument.argument_output_pins.len()),
                    Some(*dest_node_id)
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
                            Some(*dest_node_id)
                        ));
                        return false;
                    }
                };
                
                // Check if this source node references a node network and validate its validity
                if let Some(referenced_network) = node_type_registry.node_networks.get(&source_node.node_type_name) {
                    if !referenced_network.valid {
                        network.validation_errors.push(ValidationError::new(
                            format!("Source node references invalid node network '{}'", source_node.node_type_name),
                            Some(*source_node_id)
                        ));
                        return false;
                    }
                }
                
                // Get the source node type to access its output type
                let _source_node_type = match node_type_registry.get_node_type_for_node(&source_node) {
                    Some(node_type) => node_type,
                    None => {
                        network.validation_errors.push(ValidationError::new(
                            format!("Unknown source node type '{}'", source_node.node_type_name),
                            Some(*source_node_id)
                        ));
                        return false;
                    }
                };
                
                // Validate data type compatibility
                if node_type_registry.get_node_type_for_node(source_node).unwrap().get_output_pin_type(*output_pin_index) != node_type_registry.get_node_param_data_type(dest_node, arg_index) {
                    network.validation_errors.push(ValidationError::new(
                        format!("Data type mismatch: input expects {:?}, but source outputs {:?}", 
                            parameter.data_type, node_type_registry.get_node_type_for_node(source_node).unwrap().output_type),
                        Some(*dest_node_id)
                    ));
                    return false;
                }
            }
        }
    }
    
    true
}

pub fn validate_network(network: &mut NodeNetwork, node_type_registry: &NodeTypeRegistry, initial_errors: Option<Vec<crate::structure_designer::node_network::ValidationError>>) -> NetworkValidationResult {
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
    
    // Validate parameters
    if !validate_parameters(network) {
        network.valid = false;
        return NetworkValidationResult {
            valid: false,
            interface_changed,
        };
    }
    
    // Validate wires
    if !validate_wires(network, node_type_registry) {
        network.valid = false;
        return NetworkValidationResult {
            valid: false,
            interface_changed,
        };
    }
    
    // Update the network's output type based on return node
    let output_type_changed = update_network_output_type(network, node_type_registry);
    
    NetworkValidationResult {
        valid: network.valid,
        interface_changed: interface_changed || output_type_changed,
    }
}

fn update_network_output_type(network: &mut NodeNetwork, node_type_registry: &NodeTypeRegistry) -> bool {
    let old_output_type = network.node_type.output_type.clone();
    
    // Determine the new output type based on return_node_id
    let new_output_type = if let Some(return_node_id) = network.return_node_id {
        // Get the return node
        if let Some(return_node) = network.nodes.get(&return_node_id) {
            // Get the node type to find its output type
            node_type_registry.get_node_type_for_node(return_node).unwrap().output_type.clone()
        } else {
            // Return node doesn't exist, set to None
            DataType::None
        }
    } else {
        // No return node, output type is None
        DataType::None
    };
    
    // Update the network's output type
    network.node_type.output_type = new_output_type.clone();
    
    // Return true if the output type changed
    old_output_type != new_output_type
}

