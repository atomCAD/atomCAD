use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use serde::{Serialize, Deserialize};
use glam::f64::DVec2;

use crate::structure_designer::node_type::{DataType, NodeType, Parameter, data_type_to_str, str_to_data_type};
use crate::structure_designer::node_network::{NodeNetwork, Node, Argument, Wire};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::node_data::node_data::NodeData;
use crate::structure_designer::node_data::no_data::NoData;
use crate::structure_designer::node_data::sphere_data::SphereData;
use crate::structure_designer::node_data::cuboid_data::CuboidData;
use crate::structure_designer::node_data::half_space_data::HalfSpaceData;
use crate::structure_designer::node_data::geo_trans_data::GeoTransData;
use crate::structure_designer::node_data::atom_trans_data::AtomTransData;
use crate::structure_designer::node_data::parameter_data::ParameterData;

// The current version of the serialization format
const SERIALIZATION_VERSION: u32 = 1;

/// Serializable version of Parameter struct for JSON serialization
#[derive(Serialize, Deserialize)]
pub struct SerializableParameter {
    pub name: String,
    pub data_type: String,
    pub multi: bool,
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
    pub position: DVec2,
    pub arguments: Vec<Argument>,
    // We'll use a string type tag and JSON value for the polymorphic data
    pub data_type: String,
    pub data_json: String,
}

/// Serializable version of NodeNetwork for JSON serialization
#[derive(Serialize, Deserialize)]
pub struct SerializableNodeNetwork {
    pub next_node_id: u64,
    pub node_type: SerializableNodeType,
    pub nodes: Vec<SerializableNode>, // Store as vec instead of HashMap
    pub return_node_id: Option<u64>,
    pub displayed_node_ids: Vec<u64>, // Store as vec instead of HashSet
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
            data_type: data_type_to_str(&param.data_type),
            multi: param.multi,
        })
        .collect();
    
    SerializableNodeType {
        name: node_type.name.clone(),
        parameters: serializable_parameters,
        output_type: data_type_to_str(&node_type.output_type),
    }
}

/// Converts a SerializableNodeType back to a NodeType
/// 
/// # Returns
/// * `io::Result<NodeType>` - The converted NodeType or an error if conversion fails
pub fn serializable_to_node_type(serializable: &SerializableNodeType) -> io::Result<NodeType> {
    // Parse the output type using the helper function
    let output_type = str_to_data_type(&serializable.output_type)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid output type"))?;
    
    // Create parameters from the serializable parameters
    let parameters = serializable.parameters
        .iter()
        .map(|serializable_param| {
            // Parse the data type using the helper function
            let data_type = str_to_data_type(&serializable_param.data_type)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid parameter data type"))?;
            
            Ok(Parameter {
                name: serializable_param.name.clone(),
                data_type,
                multi: serializable_param.multi,
            })
        })
        .collect::<io::Result<Vec<Parameter>>>()?;
    
    // Create the NodeType with a default node_data_creator
    Ok(NodeType {
        name: serializable.name.clone(),
        parameters,
        output_type,
        node_data_creator: || Box::new(NoData {}), // Default, will be replaced with actual data
    })
}

/// Converts a Node to a SerializableNode, handling the polymorphic NodeData
/// 
/// # Returns
/// * `io::Result<SerializableNode>` - The serializable node or an error if serialization fails
pub fn node_to_serializable(id: u64, node: &Node) -> io::Result<SerializableNode> {
    // Handle the polymorphic node data based on its type
    let node_type_name = node.node_type_name.clone();
    
    // Convert the node data to a JSON string based on type
    let (data_type, data_json) = match node_type_name.as_str() {
        "cuboid" => {
            if let Some(data) = node.data.as_any().downcast_ref::<CuboidData>() {
                ("cuboid".to_string(), serde_json::to_string(data)?)
            } else {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Data type mismatch for cuboid"));
            }
        },
        "sphere" => {
            if let Some(data) = node.data.as_any().downcast_ref::<SphereData>() {
                ("sphere".to_string(), serde_json::to_string(data)?)
            } else {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Data type mismatch for sphere"));
            }
        },
        "half_space" => {
            if let Some(data) = node.data.as_any().downcast_ref::<HalfSpaceData>() {
                ("half_space".to_string(), serde_json::to_string(data)?)
            } else {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Data type mismatch for half_space"));
            }
        },
        "geo_trans" => {
            if let Some(data) = node.data.as_any().downcast_ref::<GeoTransData>() {
                ("geo_trans".to_string(), serde_json::to_string(data)?)
            } else {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Data type mismatch for geo_trans"));
            }
        },
        "atom_trans" => {
            if let Some(data) = node.data.as_any().downcast_ref::<AtomTransData>() {
                ("atom_trans".to_string(), serde_json::to_string(data)?)
            } else {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Data type mismatch for atom_trans"));
            }
        },
        "parameter" => {
            if let Some(data) = node.data.as_any().downcast_ref::<ParameterData>() {
                ("parameter".to_string(), serde_json::to_string(data)?)
            } else {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Data type mismatch for parameter"));
            }
        },
        _ => {
            // For nodes with NoData or other types we don't specifically handle
            ("no_data".to_string(), "{}".to_string())
        }
    };
    
    // Create the serializable node
    Ok(SerializableNode {
        id,
        node_type_name: node.node_type_name.clone(),
        position: node.position,
        arguments: node.arguments.clone(),
        data_type,
        data_json,
    })
}

/// Creates a Node instance from a SerializableNode
/// 
/// # Returns
/// * `io::Result<Node>` - The deserialized Node or an error if deserialization fails
pub fn serializable_to_node(serializable: &SerializableNode) -> io::Result<Node> {
    // Create the node data based on data_type
    let node_data: Box<dyn NodeData> = match serializable.data_type.as_str() {
        "cuboid" => {
            let cuboid_data: CuboidData = serde_json::from_str(&serializable.data_json)?;
            Box::new(cuboid_data)
        },
        "sphere" => {
            let sphere_data: SphereData = serde_json::from_str(&serializable.data_json)?;
            Box::new(sphere_data)
        },
        "half_space" => {
            let half_space_data: HalfSpaceData = serde_json::from_str(&serializable.data_json)?;
            Box::new(half_space_data)
        },
        "geo_trans" => {
            let geo_trans_data: GeoTransData = serde_json::from_str(&serializable.data_json)?;
            Box::new(geo_trans_data)
        },
        "atom_trans" => {
            let atom_trans_data: AtomTransData = serde_json::from_str(&serializable.data_json)?;
            Box::new(atom_trans_data)
        },
        "parameter" => {
            let parameter_data: ParameterData = serde_json::from_str(&serializable.data_json)?;
            Box::new(parameter_data)
        },
        _ => {
            // Default to NoData for unrecognized types
            Box::new(NoData {})
        }
    };
    
    // Create the Node instance
    Ok(Node {
        id: serializable.id,
        node_type_name: serializable.node_type_name.clone(),
        position: serializable.position,
        arguments: serializable.arguments.clone(),
        data: node_data,
    })
}

/// Converts a NodeNetwork to a SerializableNodeNetwork
/// 
/// # Returns
/// * `io::Result<SerializableNodeNetwork>` - The serializable network or an error if serialization fails
pub fn node_network_to_serializable(network: &NodeNetwork) -> io::Result<SerializableNodeNetwork> {
    // Convert each node to a SerializableNode
    let mut serializable_nodes = Vec::new();
    
    for (id, node) in &network.nodes {
        let serializable_node = node_to_serializable(*id, node)?;
        serializable_nodes.push(serializable_node);
    }
    
    // Convert displayed_node_ids from HashSet to Vec
    let displayed_node_ids: Vec<u64> = network.displayed_node_ids.iter().cloned().collect();
    
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
pub fn serializable_to_node_network(serializable: &SerializableNodeNetwork) -> io::Result<NodeNetwork> {
    // Create the node type from the serializable node type
    let node_type = serializable_to_node_type(&serializable.node_type)?;
    
    // Create a new network
    let mut network = NodeNetwork::new(node_type);
    
    // Set next_node_id and return_node_id
    network.next_node_id = serializable.next_node_id;
    network.return_node_id = serializable.return_node_id;
    
    // Convert displayed_node_ids from Vec to HashSet
    network.displayed_node_ids = serializable.displayed_node_ids.iter().cloned().collect();
    
    // Process each node
    for serializable_node in &serializable.nodes {
        let node = serializable_to_node(serializable_node)?;
        network.nodes.insert(node.id, node);
    }
    
    Ok(network)
}

/// Saves all node networks from a NodeTypeRegistry to a JSON file
/// 
/// # Parameters
/// * `registry` - The NodeTypeRegistry to save
/// * `path` - The file path to save to
/// 
/// # Returns
/// * `io::Result<()>` - Ok if the save operation was successful, Err otherwise
pub fn save_node_networks_to_file<P: AsRef<Path>>(registry: &NodeTypeRegistry, path: P) -> io::Result<()> {
    // Convert the node networks to a serializable format
    let mut serializable_networks = Vec::new();
    
    for (name, network) in &registry.node_networks {
        let serializable_network = node_network_to_serializable(network)?;
        serializable_networks.push((name.clone(), serializable_network));
    }
    
    // Create the container with version information
    let serializable_registry = SerializableNodeTypeRegistryNetworks {
        node_networks: serializable_networks,
        version: SERIALIZATION_VERSION,
    };
    
    // Serialize to JSON
    let json_data = serde_json::to_string_pretty(&serializable_registry)?;
    
    // Write to file
    let mut file = fs::File::create(path)?;
    file.write_all(json_data.as_bytes())?;
    
    Ok(())
}

/// Loads node networks from a JSON file into a NodeTypeRegistry
/// 
/// # Parameters
/// * `registry` - The NodeTypeRegistry to load into
/// * `path` - The file path to load from
/// 
/// # Returns
/// * `io::Result<()>` - Ok if the load operation was successful, Err otherwise
pub fn load_node_networks_from_file<P: AsRef<Path>>(registry: &mut NodeTypeRegistry, path: P) -> io::Result<()> {
    // Read the file content
    let mut file = fs::File::open(path)?;
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
    
    // Process each network
    for (name, serializable_network) in serializable_registry.node_networks {
        let network = serializable_to_node_network(&serializable_network)?;
        registry.node_networks.insert(name, network);
    }
    
    Ok(())
}
