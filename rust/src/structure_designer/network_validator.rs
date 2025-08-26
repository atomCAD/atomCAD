use crate::structure_designer::node_network::{NodeNetwork, ValidationError};
use crate::structure_designer::nodes::parameter::ParameterData;
use crate::structure_designer::node_type::Parameter;
use std::collections::HashMap;

pub struct NetworkValidationResult {
    pub valid: bool,
    pub interface_changed: bool,
}

pub fn validate_network(network: &mut NodeNetwork) -> NetworkValidationResult {
    // Clear previous validation state
    network.valid = true;
    network.validation_errors.clear();
    
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
                network.valid = false;
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
            network.valid = false;
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
            network.valid = false;
        } else {
            sort_orders.insert(param_data.sort_order, *node_id);
        }
    }
    
    // If validation failed, return early
    if !network.valid {
        return NetworkValidationResult {
            valid: false,
            interface_changed: false,
        };
    }
    
    // Sort parameter nodes by sort_order
    parameter_nodes.sort_by_key(|(_, param_data)| param_data.sort_order);
    
    // Check if the interface changed by comparing with existing parameters
    let interface_changed = if network.node_type.parameters.len() != parameter_nodes.len() {
        true
    } else {
        parameter_nodes.iter().enumerate().any(|(index, (_, param_data))| {
            if let Some(existing_param) = network.node_type.parameters.get(index) {
                existing_param.name != param_data.param_name ||
                existing_param.data_type != param_data.data_type ||
                existing_param.multi != param_data.multi
            } else {
                true
            }
        })
    };
    
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
    
    NetworkValidationResult {
        valid: network.valid,
        interface_changed,
    }
}

