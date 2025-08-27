use crate::structure_designer::node_network::{NodeNetwork, ValidationError};
use crate::structure_designer::nodes::parameter::ParameterData;
use crate::structure_designer::node_type::Parameter;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use std::collections::HashMap;

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
        if let Some(existing_node_id) = param_names.get(&param_data.param_name) {
            network.validation_errors.push(ValidationError::new(
                format!("Duplicate parameter name '{}' found in nodes {} and {}", 
                    param_data.param_name, existing_node_id, node_id),
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
        if let Some(existing_node_id) = sort_orders.get(&param_data.sort_order) {
            network.validation_errors.push(ValidationError::new(
                format!("Duplicate sort order {} found in nodes {} and {}", 
                    param_data.sort_order, existing_node_id, node_id),
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
            data_type: param_data.data_type,
            multi: param_data.multi,
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
            existing_param.data_type != param_data.data_type ||
            existing_param.multi != param_data.multi
        } else {
            true
        }
    })
}

fn validate_wires(network: &mut NodeNetwork, node_type_registry: &NodeTypeRegistry) -> bool {
    // Iterate through all nodes and their arguments to validate wires
    for (dest_node_id, dest_node) in &network.nodes {
        // Get the destination node type to access parameter information
        let dest_node_type = match node_type_registry.get_node_type(&dest_node.node_type_name) {
            Some(node_type) => node_type,
            None => {
                network.validation_errors.push(ValidationError::new(
                    format!("Unknown node type '{}'", dest_node.node_type_name),
                    Some(*dest_node_id)
                ));
                return false;
            }
        };
        
        // Validate each argument (input pin) of the destination node
        for (arg_index, argument) in dest_node.arguments.iter().enumerate() {
            // Get parameter information for this argument
            let parameter = match dest_node_type.parameters.get(arg_index) {
                Some(param) => param,
                None => {
                    network.validation_errors.push(ValidationError::new(
                        format!("Node '{}' has more arguments than parameters defined in its type", dest_node.node_type_name),
                        Some(*dest_node_id)
                    ));
                    return false;
                }
            };
            
            // Validate non-multi input pins have at most one connection
            if !parameter.multi && argument.argument_node_ids.len() > 1 {
                network.validation_errors.push(ValidationError::new(
                    format!("Non-multi parameter '{}' of node {} has {} connections, but only 1 is allowed", 
                        parameter.name, dest_node_id, argument.argument_node_ids.len()),
                    Some(*dest_node_id)
                ));
                return false;
            }
            
            // Validate data types for each connected source node
            for source_node_id in &argument.argument_node_ids {
                // Get the source node
                let source_node = match network.nodes.get(source_node_id) {
                    Some(node) => node,
                    None => {
                        network.validation_errors.push(ValidationError::new(
                            format!("Wire references non-existent source node {}", source_node_id),
                            Some(*dest_node_id)
                        ));
                        return false;
                    }
                };
                
                // Get the source node type to access its output type
                let source_node_type = match node_type_registry.get_node_type(&source_node.node_type_name) {
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
                if source_node_type.output_type != parameter.data_type {
                    network.validation_errors.push(ValidationError::new(
                        format!("Data type mismatch: source node {} outputs {:?}, but destination parameter '{}' expects {:?}", 
                            source_node_id, source_node_type.output_type, parameter.name, parameter.data_type),
                        Some(*dest_node_id)
                    ));
                    return false;
                }
            }
        }
    }
    
    true
}

pub fn validate_network(network: &mut NodeNetwork, node_type_registry: &NodeTypeRegistry) -> NetworkValidationResult {
    // Clear previous validation state
    network.valid = true;
    network.validation_errors.clear();
    
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
    
    // TODO: Add other validation functions here in future steps
    
    NetworkValidationResult {
        valid: network.valid,
        interface_changed,
    }
}

