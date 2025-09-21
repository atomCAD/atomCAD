use std::fs;
use std::io::{self, Read};
use std::path::Path;
use serde::{Serialize, Deserialize};
use serde_json;
use glam::f64::DVec2;
use crate::common::serialization_utils::dvec2_serializer;
use crate::structure_designer::data_type::DataType;
use super::super::node_type::{NodeType, Parameter};
use super::super::node_network::{NodeNetwork, Node, Argument};
use super::super::node_type_registry::NodeTypeRegistry;
use super::super::node_data::NodeData;
use super::super::node_data::NoData;
use super::super::node_network::NodeDisplayType;

// The current version of the serialization format
const SERIALIZATION_VERSION: u32 = 1;

/// Serializable version of Parameter struct for JSON serialization
#[derive(Serialize, Deserialize)]
pub struct SerializableParameter {
    pub name: String,
    pub data_type: String,
}

/// Serializable version of NodeType struct for JSON serialization
#[derive(Serialize, Deserialize)]
pub struct SerializableNodeType {
    pub name: String,
    pub parameters: Vec<SerializableParameter>,
    pub output_type: String,
}

/// Serializable version of Node without trait objects for JSON serialization
#[derive(Serialize, Deserialize)]
pub struct SerializableNode {
    pub id: u64,
    pub node_type_name: String,
    #[serde(with = "dvec2_serializer")]
    pub position: DVec2,
    pub arguments: Vec<Argument>,
    // Use a string type tag and direct JSON value for the polymorphic data
    pub data_type: String,
    pub data: serde_json::Value, // Store as native JSON value instead of a string for better readability
}

/// Serializable version of NodeNetwork for JSON serialization
#[derive(Serialize, Deserialize)]
pub struct SerializableNodeNetwork {
    pub next_node_id: u64,
    pub node_type: SerializableNodeType,
    pub nodes: Vec<SerializableNode>, // Store as vec instead of HashMap
    pub return_node_id: Option<u64>,
    pub displayed_node_ids: Vec<(u64, NodeDisplayType)>, // Store as vec instead of HashSet
}

/// Container for serializing all node networks in the NodeTypeRegistry
#[derive(Serialize, Deserialize)]
pub struct SerializableNodeTypeRegistryNetworks {
    pub node_networks: Vec<(String, SerializableNodeNetwork)>,
    pub version: u32, // For future compatibility
}

/// Converts a NodeType to its serializable counterpart
pub fn node_type_to_serializable(node_type: &NodeType) -> SerializableNodeType {
    let serializable_parameters = node_type.parameters
        .iter()
        .map(|param| SerializableParameter {
            name: param.name.clone(),
            data_type: param.data_type.to_string(),
        })
        .collect();
    
    SerializableNodeType {
        name: node_type.name.clone(),
        parameters: serializable_parameters,
        output_type: node_type.output_type.to_string(),
    }
}

/// Converts a SerializableNodeType back to a NodeType
/// 
/// # Returns
/// * `io::Result<NodeType>` - The converted NodeType or an error if conversion fails
pub fn serializable_to_node_type(serializable: &SerializableNodeType) -> io::Result<NodeType> {
    // Parse the output type using the helper function
    let output_type = DataType::from_string(&serializable.output_type)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Invalid output type: {}", e)))?;
    
    // Create parameters from the serializable parameters
    let parameters = serializable.parameters
        .iter()
        .map(|serializable_param| {
            // Parse the data type using the helper function
            let data_type = DataType::from_string(&serializable_param.data_type)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Invalid parameter data type: {}", e)))?;
            
            Ok(Parameter {
                name: serializable_param.name.clone(),
                data_type,
            })
        })
        .collect::<io::Result<Vec<Parameter>>>()?;
    
    // Create the NodeType with a default node_data_creator
    Ok(NodeType {
        name: serializable.name.clone(),
        parameters,
        output_type,
        node_data_creator: || Box::new(NoData {}), // Default, will be replaced with actual data
        node_data_saver: crate::structure_designer::node_type::no_data_saver,
        node_data_loader: crate::structure_designer::node_type::no_data_loader,
    })
}

//  &node.data.as_any_ref().downcast_ref::<HalfSpaceData>().unwrap();


/// Converts a Node to a SerializableNode, handling the polymorphic NodeData
/// 
/// # Returns
/// * `io::Result<SerializableNode>` - The serializable node or an error if serialization fails
pub fn node_to_serializable(id: u64, node: &mut Node, built_in_node_types: &std::collections::HashMap<String, crate::structure_designer::node_type::NodeType>, design_dir: Option<&str>) -> io::Result<SerializableNode> {
    // Handle the polymorphic node data based on its type
    let node_type_name = node.node_type_name.clone();
    
    // Convert the node data to a JSON value using the built-in node types
    let (data_type, json_data) = if let Some(node_type) = built_in_node_types.get(&node_type_name) {
        let json_data = (node_type.node_data_saver)(node.data.as_mut(), design_dir)?;
        (node_type_name.clone(), json_data)
    } else {
        // Fallback for unknown types
        ("no_data".to_string(), serde_json::json!({}))
    };
    
    // Create the serializable node
    Ok(SerializableNode {
        id,
        node_type_name: node.node_type_name.clone(),
        position: node.position,
        arguments: node.arguments.clone(),
        data_type,
        data: json_data,
    })
}

/// Creates a Node instance from a SerializableNode
/// 
/// # Returns
/// * `io::Result<Node>` - The deserialized Node or an error if deserialization fails
pub fn serializable_to_node(serializable: &SerializableNode, built_in_node_types: &std::collections::HashMap<String, crate::structure_designer::node_type::NodeType>, design_dir: Option<&str>) -> io::Result<Node> {
    // Create the node data using the built-in node types
    let data: Box<dyn NodeData> = if let Some(node_type) = built_in_node_types.get(&serializable.data_type) {
        (node_type.node_data_loader)(&serializable.data, design_dir)?
    } else {
        // Default to NoData for unknown types
        Box::new(NoData {})
    };
    
    // Create the Node instance
    Ok(Node {
        id: serializable.id,
        node_type_name: serializable.node_type_name.clone(),
        position: serializable.position,
        arguments: serializable.arguments.clone(),
        data,
        custom_node_type: None,
    })
}

/// Converts a NodeNetwork to a SerializableNodeNetwork
/// 
/// # Returns
/// * `io::Result<SerializableNodeNetwork>` - The serializable network or an error if serialization fails
pub fn node_network_to_serializable(network: &mut NodeNetwork, built_in_node_types: &std::collections::HashMap<String, crate::structure_designer::node_type::NodeType>, design_dir: Option<&str>) -> io::Result<SerializableNodeNetwork> {
    // Convert each node to a SerializableNode
    let mut serializable_nodes = Vec::new();
    
    for (id, node) in &mut network.nodes {
        let serializable_node = node_to_serializable(*id, node, built_in_node_types, design_dir)?;
        serializable_nodes.push(serializable_node);
    }
    
    // Convert displayed_node_ids from HashMap to Vec of tuples
    let displayed_node_ids: Vec<(u64, NodeDisplayType)> = network.displayed_node_ids.iter().map(|(key, value)| (*key, *value)).collect();
    
    // Create a serializable version of the node type
    let serializable_node_type = node_type_to_serializable(&network.node_type);
    
    // Create the serializable network
    Ok(SerializableNodeNetwork {
        next_node_id: network.next_node_id,
        node_type: serializable_node_type,
        nodes: serializable_nodes,
        return_node_id: network.return_node_id,
        displayed_node_ids,
    })
}

/// Creates a NodeNetwork from a SerializableNodeNetwork
/// 
/// # Returns
/// * `io::Result<NodeNetwork>` - The deserialized network or an error if deserialization fails
pub fn serializable_to_node_network(serializable: &SerializableNodeNetwork, built_in_node_types: &std::collections::HashMap<String, crate::structure_designer::node_type::NodeType>, design_dir: Option<&str>) -> io::Result<NodeNetwork> {
    // Create the node type from the serializable node type
    let node_type = serializable_to_node_type(&serializable.node_type)?;
    
    // Create a new network
    let mut network = NodeNetwork::new(node_type);
    
    // Set next_node_id and return_node_id
    network.next_node_id = serializable.next_node_id;
    network.return_node_id = serializable.return_node_id;
    
    // Convert displayed_node_ids from Vec of tuples to HashMap without taking ownership
    network.displayed_node_ids = serializable.displayed_node_ids.iter().map(|(id, display_type)| (*id, *display_type)).collect();
    
    // Process each node
    for serializable_node in &serializable.nodes {
        let node = serializable_to_node(serializable_node, built_in_node_types, design_dir)?;
        network.nodes.insert(node.id, node);
    }
    
    Ok(network)
}

/// Saves node networks from a NodeTypeRegistry to a JSON file
/// 
/// # Parameters
/// * `registry` - The NodeTypeRegistry to save
/// * `file_path` - Path to the output JSON file
/// 
/// # Returns
/// * `io::Result<()>` - Success or an error if saving fails
pub fn save_node_networks_to_file(registry: &mut NodeTypeRegistry, file_path: &Path) -> io::Result<()> {
    // Extract design directory early
    let design_dir = file_path.parent().and_then(|p| p.to_str());
    
    // Convert the node networks to a serializable format
    let mut serializable_networks = Vec::new();
    
    for (name, network) in &mut registry.node_networks {
        let serializable_network = node_network_to_serializable(network, &registry.built_in_node_types, design_dir)?;
        serializable_networks.push((name.clone(), serializable_network));
    }
    
    // Create the container with version information
    let serializable_registry = SerializableNodeTypeRegistryNetworks {
        node_networks: serializable_networks,
        version: SERIALIZATION_VERSION,
    };
    
    // Serialize to JSON
    let json_data = serde_json::to_string_pretty(&serializable_registry)?;
    
    // Create the parent directory if it doesn't exist
    if let Some(parent) = Path::new(file_path).parent() {
        fs::create_dir_all(parent)?;
    }
    
    // Write to file
    fs::write(file_path, json_data)?;

    registry.design_file_name = Some(file_path.to_string_lossy().to_string());

    Ok(())
}

/// Loads node networks from a JSON file into a NodeTypeRegistry
/// 
/// # Parameters
/// * `registry` - The NodeTypeRegistry to load into
/// * `file_path` - The file path to load from as a string
/// 
/// # Returns
/// * `io::Result<()>` - Ok if the load operation was successful, Err otherwise
pub fn load_node_networks_from_file(registry: &mut NodeTypeRegistry, file_path: &str) -> io::Result<()> {
    // Extract design directory early
    let design_dir = std::path::Path::new(file_path).parent().and_then(|p| p.to_str());
    
    // Read the file content
    let mut file = fs::File::open(file_path)?;
    let mut json_data = String::new();
    file.read_to_string(&mut json_data)?;
    
    // Deserialize from JSON
    let serializable_registry: SerializableNodeTypeRegistryNetworks = serde_json::from_str(&json_data)?;
    
    // Check version for potential compatibility handling in the future
    let version = serializable_registry.version;
    if version > SERIALIZATION_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData, 
            format!("Unsupported version: {}", version)
        ));
    }
    
    registry.node_networks.clear();

    // Process each network
    for (name, serializable_network) in serializable_registry.node_networks {
        let mut network = serializable_to_node_network(&serializable_network, &registry.built_in_node_types, design_dir)?;
        registry.initialize_custom_node_types_for_network(&mut network);
        registry.node_networks.insert(name, network);
    }
    
    // Set the design file name after successful load
    registry.design_file_name = Some(file_path.to_string());
    
    Ok(())
}
