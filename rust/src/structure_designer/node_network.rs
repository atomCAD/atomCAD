use glam::f64::DVec2;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::collections::HashSet;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::structure_designer::StructureDesigner;

use super::data_type::DataType;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeDisplayType {
  Normal,
  Ghost,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ValidationError {
  pub error_text: String,
  pub node_id: Option<u64>,
}

impl ValidationError {
  pub fn new(error_text: String, node_id: Option<u64>) -> Self {
    Self {
      error_text,
      node_id,
    }
  }
} 

#[derive(Clone, Serialize, Deserialize)]
pub struct Argument {
  // As parameters can have the 'multiple' flag set we need to represent multiple argument output pins here.
  // In argument_output_pins the key is node id of the output pin,
  // and the value is the output pin index.
  // The output pin index can have the following values:
  // -1: the 'function pin' of the node
  // 0: the normal output pin of the node
  // Later if we will support multiple regular output pins we will be able to use the positive
  // output pin indices.
  pub argument_output_pins: HashMap<u64, i32>,
}

impl Argument {

  pub fn new() -> Self {
    Self {
      argument_output_pins: HashMap::new(),
    }
  }

  /// Returns Some(node_id) for one of the nodes in argument_output_pins if not empty,
  /// otherwise returns None
  pub fn get_node_id(&self) -> Option<u64> {
    self.argument_output_pins.keys().next().copied()
  }

  /// Returns Some((node_id, output_pin_index)) for one of the nodes in argument_output_pins if not empty,
  /// otherwise returns None
  pub fn get_node_id_and_pin(&self) -> Option<(u64, i32)> {
    self.argument_output_pins.iter().next().map(|(&node_id, &pin_index)| (node_id, pin_index))
  }

  pub fn is_empty(&self) -> bool {
    self.argument_output_pins.is_empty()
  }
}

#[derive(Serialize, Deserialize)]
pub struct Wire {
    pub source_node_id: u64,
    pub source_output_pin_index: i32,
    pub destination_node_id: u64,
    pub destination_argument_index: usize,
}

pub struct Node {
  pub id: u64,
  pub node_type_name: String,
  pub position: DVec2,
  pub arguments: Vec<Argument>,
  pub data: Box<dyn NodeData>,
  pub custom_node_type: Option<NodeType>,
}

impl Node {
  /// Sets the custom node type and intelligently preserves existing argument connections
  /// when parameter names match between old and new node types
  pub fn set_custom_node_type(&mut self, custom_node_type: Option<NodeType>, refresh_args: bool) {
    if let Some(ref new_node_type) = custom_node_type {
      // Check if we can preserve existing arguments
      let can_preserve = if let Some(ref old_node_type) = self.custom_node_type {
        // Check if parameters have same names in same order
        old_node_type.parameters.len() == new_node_type.parameters.len() &&
        old_node_type.parameters.iter()
          .zip(new_node_type.parameters.iter())
          .all(|(old_param, new_param)| old_param.name == new_param.name)
      } else {
        false
      };

      if (!refresh_args) || can_preserve {
        // Parameters match exactly, keep existing arguments
        // (no changes to self.arguments)
      } else {
        // Parameters changed, need to rebuild arguments array
        let mut new_arguments = vec![Argument::new(); new_node_type.parameters.len()];
        
        // Try to preserve connections for matching parameter names
        if let Some(ref old_node_type) = self.custom_node_type {
          for (new_index, new_param) in new_node_type.parameters.iter().enumerate() {
            // Find matching parameter name in old node type
            if let Some(old_index) = old_node_type.parameters.iter()
              .position(|old_param| old_param.name == new_param.name) {
              // Copy argument connections from old position to new position
              if old_index < self.arguments.len() {
                new_arguments[new_index] = self.arguments[old_index].clone();
              }
            }
          }
        }
        
        self.arguments = new_arguments;
      }
    }
    self.custom_node_type = custom_node_type;
  }
}



/*
 * A node network is a network of nodes used by users to create geometries and atomic structures.
 * A node network can also be an implementation of a non-built-in node type.
 * In this case it might or might not have parameters.
 */
pub struct NodeNetwork {
  pub next_node_id: u64,
  pub node_type: NodeType, // This is the node type when this node network is used as a node in another network. (analog to a function header in programming)
  pub nodes: HashMap<u64, Node>,
  pub return_node_id: Option<u64>, // Only node networks with a return node can be used as a node (a.k.a can be called)
  pub displayed_node_ids: HashMap<u64, NodeDisplayType>, // Map of nodes that are currently displayed with their display type (Normal or Ghost)
  pub selected_node_id: Option<u64>, // Currently selected node, if any
  pub selected_wire: Option<Wire>, // Currently selected wire
  pub valid: bool, // Whether the node network is valid and can be evaluated
  pub validation_errors: Vec<ValidationError>, // List of validation errors if any
}

impl NodeNetwork {
  /// Builds a reverse dependency map (downstream connections)
  /// 
  /// For each node, this returns the set of nodes that depend on it
  /// (i.e., nodes that have this node as an input in their arguments)
  /// 
  /// # Returns
  /// A HashMap where:
  /// - Key: source node ID
  /// - Value: HashSet of node IDs that have the key node as an input
  /// 
  /// # Example
  /// If node B depends on node A (A → B), then the map will contain:
  /// - Key: A, Value: {B}
  pub fn build_reverse_dependency_map(&self) -> HashMap<u64, HashSet<u64>> {
    let mut reverse_map: HashMap<u64, HashSet<u64>> = HashMap::new();
    
    for (&node_id, node) in &self.nodes {
      for arg in &node.arguments {
        for (&source_node_id, &_output_pin_index) in &arg.argument_output_pins {
          // node_id depends on source_node_id
          // So source_node_id has node_id as a downstream dependent
          reverse_map
            .entry(source_node_id)
            .or_insert_with(HashSet::new)
            .insert(node_id);
        }
      }
    }
    
    reverse_map
  }

  /// Returns a HashSet of all node IDs that are directly connected to the given node
  /// This includes both nodes that provide input to this node and nodes that receive output from this node
  pub fn get_connected_node_ids(&self, node_id: u64) -> HashSet<u64> {
    let mut connected_ids = HashSet::new();
    
    // Check if the node exists
    if !self.nodes.contains_key(&node_id) {
      return connected_ids; // Return empty set if node doesn't exist
    }
    
    // Get nodes that provide input to this node (input connections)
    if let Some(node) = self.nodes.get(&node_id) {
      for argument in &node.arguments {
        // Add all node IDs that provide input to this node
        connected_ids.extend(argument.argument_output_pins.keys());
      }
    }
    
    // Get nodes that receive output from this node (output connections)
    for (other_id, other_node) in &self.nodes {
      // Skip the node itself
      if *other_id == node_id {
        continue;
      }
      
      // Check if any of this node's arguments reference the given node
      for argument in &other_node.arguments {
        if argument.argument_output_pins.contains_key(&node_id) {
          connected_ids.insert(*other_id);
          break; // No need to check other arguments of this node
        }
      }
    }
    
    connected_ids
  }

  pub fn new(node_type: NodeType) -> Self {
    let ret = Self {
      next_node_id: 1,
      node_type,
      nodes: HashMap::new(),
      return_node_id: None,
      displayed_node_ids: HashMap::new(),
      selected_node_id: None,
      selected_wire: None,
      valid: true,
      validation_errors: Vec::new(),
    };

    return ret;
  }

  pub fn add_node(&mut self, node_type_name: &str, position: DVec2, num_of_parameters: usize, node_data: Box<dyn NodeData>) -> u64 {
    let node_id = self.next_node_id;
    let mut arguments: Vec<Argument> = Vec::new();
    for _i in 0..num_of_parameters {
      arguments.push(Argument::new());
    }

    let node = Node {
      id: node_id,
      node_type_name: node_type_name.to_string(),
      position,
      arguments,
      data: node_data,
      custom_node_type: None,
    };
    
    self.next_node_id += 1;
    self.nodes.insert(node_id, node);
    self.set_node_display(node_id, true);
    return node_id;
  }

  pub fn move_node(&mut self, node_id: u64, position: DVec2) {
    if let Some(node) = self.nodes.get_mut(&node_id) {
      node.position = position;
    }
  }

  pub fn can_connect_nodes(&self, source_node_id: u64, source_output_pin_index: i32, dest_node_id: u64, dest_param_index: usize, node_type_registry: &crate::structure_designer::node_type_registry::NodeTypeRegistry) -> bool {
    // Check if both nodes exist
    let source_node = match self.nodes.get(&source_node_id) {
      Some(node) => node,
      None => return false,
    };
    
    let dest_node = match self.nodes.get(&dest_node_id) {
      Some(node) => node,
      None => return false,
    };
    
    // Check if the destination parameter index is valid
    if dest_param_index >= dest_node.arguments.len() {
      return false;
    }
    
    // Get the expected input type for the destination parameter
    let dest_param_type = node_type_registry.get_node_param_data_type(dest_node, dest_param_index);

    // Get the output type of the source node
    let source_output_type = &node_type_registry.get_node_type_for_node(source_node).unwrap().get_output_pin_type(source_output_pin_index);

    // Check if the data types are compatible using conversion rules
    DataType::can_be_converted_to(source_output_type, &dest_param_type)
  }

  pub fn connect_nodes(&mut self, source_node_id: u64, source_output_pin_index: i32, dest_node_id: u64, dest_param_index: usize, dest_param_is_multi: bool) {
    if let Some(dest_node) = self.nodes.get_mut(&dest_node_id) {
      let argument = &mut dest_node.arguments[dest_param_index];
      // In case of single parameters we need to disconnect the existing parameter first
      if (!dest_param_is_multi) && (!argument.is_empty()) {
        argument.argument_output_pins.clear();
      }
      argument.argument_output_pins.insert(source_node_id, source_output_pin_index);
    }
  }

  pub fn set_node_network_data(&mut self, node_id: u64, data: Box<dyn NodeData>) {
    if let Some(node) = self.nodes.get_mut(&node_id) {
      node.data = data;
    }
  }

  pub fn get_node_network_data(&self, node_id: u64) -> Option<&dyn NodeData> {
    self.nodes.get(&node_id).map(|node| node.data.as_ref())
  }

  pub fn get_node_network_data_mut(&mut self, node_id: u64) -> Option<&mut dyn NodeData> {
    self.nodes.get_mut(&node_id).map(|node| node.data.as_mut())
  }

  pub fn set_node_display(&mut self, node_id: u64, is_displayed: bool) {
    if self.nodes.contains_key(&node_id) {
      if is_displayed {
        self.displayed_node_ids.insert(node_id, NodeDisplayType::Normal);
      } else {
        self.displayed_node_ids.remove(&node_id);
      }
    }
  }
  
  /// Sets a node to be displayed with the specified display type, or hides it if display_type is None
  pub fn set_node_display_type(&mut self, node_id: u64, display_type: Option<NodeDisplayType>) {
    if self.nodes.contains_key(&node_id) {
      match display_type {
        Some(display_type) => {
          self.displayed_node_ids.insert(node_id, display_type);
        },
        None => {
          self.displayed_node_ids.remove(&node_id);
        }
      }
    }
  }
  
  /// Check if a node is currently displayed
  pub fn is_node_displayed(&self, node_id: u64) -> bool {
    self.displayed_node_ids.contains_key(&node_id)
  }
  
  /// Get the display type of a node if it is displayed
  pub fn get_node_display_type(&self, node_id: u64) -> Option<NodeDisplayType> {
    self.displayed_node_ids.get(&node_id).copied()
  }

  /// Selects a node and clears any existing wire selection.
  /// Returns true if the node exists and was selected, false otherwise.
  pub fn select_node(&mut self, node_id: u64) -> bool {
    if self.nodes.contains_key(&node_id) {
      self.selected_wire = None;
      self.selected_node_id = Some(node_id);
      true
    } else {
      false
    }
  }

  /// Selects a wire and clears any existing node selection.
  /// Returns true if both nodes exist and the wire was selected, false otherwise.
  pub fn select_wire(&mut self, source_node_id: u64, source_output_pin_index: i32, destination_node_id: u64, destination_argument_index: usize) -> bool {
    if self.nodes.contains_key(&source_node_id) && self.nodes.contains_key(&destination_node_id) {
      self.selected_node_id = None;
      self.selected_wire = Some(Wire {
        source_node_id,
        source_output_pin_index,
        destination_node_id,
        destination_argument_index,
      });
      true
    } else {
      false
    }
  }

  /// Clears any existing selection (both node and wire).
  pub fn clear_selection(&mut self) {
    self.selected_node_id = None;
    self.selected_wire = None;
  }

  pub fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
    if let Some(node_id) = self.selected_node_id {
      let node = self.nodes.get(&node_id).unwrap();
      return node.data.provide_gadget(structure_designer);
    }
    None
  }

  pub fn delete_selected(&mut self) {
    // Handle selected node
    if let Some(node_id) = self.selected_node_id {
      // First remove any references to this node from all other nodes' arguments
      let nodes_to_process: Vec<u64> = self.nodes.keys().cloned().collect();
      for other_node_id in nodes_to_process {
        if let Some(node) = self.nodes.get_mut(&other_node_id) {
          for argument in node.arguments.iter_mut() {
            argument.argument_output_pins.remove(&node_id);
          }
        }
      }

      // If this was the return node, clear that reference
      if self.return_node_id == Some(node_id) {
        self.return_node_id = None;
      }

      // Remove from displayed nodes if present
      self.displayed_node_ids.remove(&node_id);

      // Remove the node itself
      self.nodes.remove(&node_id);
      self.selected_node_id = None;
    }
    // Handle selected wire
    else if let Some(wire) = self.selected_wire.take() {
      if let Some(dest_node) = self.nodes.get_mut(&wire.destination_node_id) {
        if let Some(argument) = dest_node.arguments.get_mut(wire.destination_argument_index) {
          argument.argument_output_pins.remove(&wire.source_node_id);
        }
      }
    }
  }

  /// Sets a node as the return node for this network
  /// 
  /// # Parameters
  /// * `node_id` - The ID of the node to set as the return node
  /// 
  /// # Returns
  /// Returns true if the node exists and was set as the return node, false otherwise.
  pub fn set_return_node(&mut self, node_id: u64) -> bool {
    if self.nodes.contains_key(&node_id) {
      // Set this node as the return node
      self.return_node_id = Some(node_id);
      
      true
    } else {
      false
    }
  }

  /// Duplicates a node with all its data and arguments
  /// 
  /// # Parameters
  /// * `node_id` - The ID of the node to duplicate
  /// 
  /// # Returns
  /// Returns Some(new_node_id) if the node was successfully duplicated, None if the node doesn't exist.
  pub fn duplicate_node(&mut self, node_id: u64) -> Option<u64> {
    // Check if the node exists
    let original_node = self.nodes.get(&node_id)?;
    
    // Generate new node ID
    let new_node_id = self.next_node_id;
    self.next_node_id += 1;
    
    // Clone the node data using the clone_box method
    let cloned_data = original_node.data.clone_box();
    
    // Clone the arguments (connections)
    let cloned_arguments = original_node.arguments.clone();
    
    // Calculate new position (180 units to the right)
    let new_position = DVec2::new(original_node.position.x + 180.0, original_node.position.y);
    
    // Create the duplicated node
    let duplicated_node = Node {
      id: new_node_id,
      node_type_name: original_node.node_type_name.clone(),
      position: new_position,
      arguments: cloned_arguments,
      data: cloned_data,
      custom_node_type: original_node.custom_node_type.clone(),
    };
    
    // Insert the duplicated node into the network
    self.nodes.insert(new_node_id, duplicated_node);
    
    Some(new_node_id)
  }
}
