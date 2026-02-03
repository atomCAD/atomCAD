use glam::f64::DVec2;
use serde::{Serialize, Deserialize};
use std::cmp::max;
use std::collections::HashMap;
use std::collections::HashSet;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::structure_designer::StructureDesigner;

use super::data_type::DataType;
use super::node_layout;

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

impl Default for Argument {
    fn default() -> Self {
        Self::new()
    }
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

#[derive(Clone, Serialize, Deserialize)]
pub struct Wire {
    pub source_node_id: u64,
    pub source_output_pin_index: i32,
    pub destination_node_id: u64,
    pub destination_argument_index: usize,
}

impl PartialEq for Wire {
  fn eq(&self, other: &Self) -> bool {
    self.source_node_id == other.source_node_id
      && self.source_output_pin_index == other.source_output_pin_index
      && self.destination_node_id == other.destination_node_id
      && self.destination_argument_index == other.destination_argument_index
  }
}

impl Eq for Wire {}

pub struct Node {
  pub id: u64,
  pub node_type_name: String,
  /// User-specified name for this node (e.g., "mybox" from "mybox = cuboid {...}").
  /// If None, the node will be named using auto-generated names like "cuboid1".
  pub custom_name: Option<String>,
  pub position: DVec2,
  pub arguments: Vec<Argument>,
  pub data: Box<dyn NodeData>,
  pub custom_node_type: Option<NodeType>,
}

impl Node {
  /// Sets the custom node type and intelligently preserves existing argument connections
  /// when parameter IDs (primary) or names (fallback) match between old and new node types
  pub fn set_custom_node_type(&mut self, custom_node_type: Option<NodeType>, refresh_args: bool) {
    if let Some(ref new_node_type) = custom_node_type {
      // Check if we can preserve existing arguments (same parameters in same order)
      let can_preserve = if let Some(ref old_node_type) = self.custom_node_type {
        // Check if parameters match by ID (if both have IDs) or by name
        old_node_type.parameters.len() == new_node_type.parameters.len() &&
        old_node_type.parameters.iter()
          .zip(new_node_type.parameters.iter())
          .all(|(old_param, new_param)| {
            // Match by ID if both have IDs, otherwise by name
            match (old_param.id, new_param.id) {
              (Some(old_id), Some(new_id)) => old_id == new_id,
              _ => old_param.name == new_param.name,
            }
          })
      } else {
        false
      };

      if (!refresh_args) || can_preserve {
        // Parameters match exactly, keep existing arguments
        // (no changes to self.arguments)
      } else {
        // Parameters changed, need to rebuild arguments array
        let mut new_arguments = vec![Argument::new(); new_node_type.parameters.len()];

        // Try to preserve connections using ID-based matching (primary) or name-based (fallback)
        if let Some(ref old_node_type) = self.custom_node_type {
          // Build ID map for old parameters
          let old_id_map: std::collections::HashMap<u64, usize> = old_node_type.parameters.iter()
            .enumerate()
            .filter_map(|(idx, p)| p.id.map(|id| (id, idx)))
            .collect();

          for (new_index, new_param) in new_node_type.parameters.iter().enumerate() {
            // First try ID-based matching (handles renames)
            let old_index = if let Some(new_id) = new_param.id {
              if let Some(&idx) = old_id_map.get(&new_id) {
                Some(idx)
              } else {
                // Fall back to name-based matching
                old_node_type.parameters.iter()
                  .position(|old_param| old_param.name == new_param.name)
              }
            } else {
              // No ID, use name-based matching (backwards compatibility)
              old_node_type.parameters.iter()
                .position(|old_param| old_param.name == new_param.name)
            };

            // Copy argument connections from old position to new position
            if let Some(old_idx) = old_index {
              if old_idx < self.arguments.len() {
                new_arguments[new_index] = self.arguments[old_idx].clone();
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
  pub next_param_id: u64,  // Counter for generating unique parameter IDs within this network
  pub node_type: NodeType, // This is the node type when this node network is used as a node in another network. (analog to a function header in programming)
  pub nodes: HashMap<u64, Node>,
  pub return_node_id: Option<u64>, // Only node networks with a return node can be used as a node (a.k.a can be called)
  pub displayed_node_ids: HashMap<u64, NodeDisplayType>, // Map of nodes that are currently displayed with their display type (Normal or Ghost)
  pub selected_node_ids: HashSet<u64>, // All selected nodes (multi-selection)
  pub active_node_id: Option<u64>, // Active node (for properties panel/gadget) - the last selected node
  pub selected_wires: Vec<Wire>, // All selected wires (multi-selection)
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
            .or_default()
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
    

    Self {
      next_node_id: 1,
      next_param_id: 1,  // Start parameter IDs at 1
      node_type,
      nodes: HashMap::new(),
      return_node_id: None,
      displayed_node_ids: HashMap::new(),
      selected_node_ids: HashSet::new(),
      active_node_id: None,
      selected_wires: Vec::new(),
      valid: true,
      validation_errors: Vec::new(),
    }
  }

  /// Generate a unique display name for a new node of the given type.
  ///
  /// Scans existing nodes to find the highest counter used for this type,
  /// then returns `{type}{max+1}`. Names are never reused even if nodes
  /// are deleted, ensuring stability for external references.
  pub fn generate_unique_display_name(&self, node_type: &str) -> String {
    let mut max_counter = 0;
    for node in self.nodes.values() {
      if let Some(ref name) = node.custom_name {
        if let Some(num_str) = name.strip_prefix(node_type) {
          if let Ok(num) = num_str.parse::<u32>() {
            max_counter = max_counter.max(num);
          }
        }
      }
    }
    format!("{}{}", node_type, max_counter + 1)
  }

  pub fn add_node(&mut self, node_type_name: &str, position: DVec2, num_of_parameters: usize, node_data: Box<dyn NodeData>) -> u64 {
    let node_id = self.next_node_id;
    let display_name = self.generate_unique_display_name(node_type_name);
    let mut arguments: Vec<Argument> = Vec::new();
    for _i in 0..num_of_parameters {
      arguments.push(Argument::new());
    }

    let node = Node {
      id: node_id,
      node_type_name: node_type_name.to_string(),
      custom_name: Some(display_name),
      position,
      arguments,
      data: node_data,
      custom_node_type: None,
    };

    self.next_node_id += 1;
    self.nodes.insert(node_id, node);
    self.set_node_display(node_id, true);
    node_id
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

  // ===== NODE SELECTION =====

  /// Select a single node (clears existing selection including wires)
  /// Returns true if the node exists and was selected, false otherwise.
  pub fn select_node(&mut self, node_id: u64) -> bool {
    if self.nodes.contains_key(&node_id) {
      self.selected_wires.clear();
      self.selected_node_ids.clear();
      self.selected_node_ids.insert(node_id);
      self.active_node_id = Some(node_id);
      true
    } else {
      false
    }
  }

  /// Toggle node in selection (for Ctrl+click)
  /// Returns true if the node exists, false otherwise.
  /// Does not clear wire selection to allow mixed node+wire selections.
  pub fn toggle_node_selection(&mut self, node_id: u64) -> bool {
    if !self.nodes.contains_key(&node_id) {
      return false;
    }
    if self.selected_node_ids.contains(&node_id) {
      self.selected_node_ids.remove(&node_id);
      // Update active node if we removed it
      if self.active_node_id == Some(node_id) {
        self.active_node_id = self.selected_node_ids.iter().next().copied();
      }
    } else {
      self.selected_node_ids.insert(node_id);
      self.active_node_id = Some(node_id);
    }
    true
  }

  /// Add node to selection (for Shift+click)
  /// Returns true if the node exists, false otherwise.
  /// Does not clear wire selection to allow mixed node+wire selections.
  pub fn add_node_to_selection(&mut self, node_id: u64) -> bool {
    if !self.nodes.contains_key(&node_id) {
      return false;
    }
    self.selected_node_ids.insert(node_id);
    self.active_node_id = Some(node_id);
    true
  }

  /// Select multiple nodes (for rectangle selection)
  /// Returns true if at least one node was selected, false otherwise.
  pub fn select_nodes(&mut self, node_ids: Vec<u64>) -> bool {
    self.selected_wires.clear();
    self.selected_node_ids.clear();
    for id in &node_ids {
      if self.nodes.contains_key(id) {
        self.selected_node_ids.insert(*id);
      }
    }
    // Set active to last node in list (or none if empty)
    self.active_node_id = node_ids.last().copied()
      .filter(|id| self.selected_node_ids.contains(id));
    !self.selected_node_ids.is_empty()
  }

  /// Toggle multiple nodes in selection (for Ctrl+rectangle)
  pub fn toggle_nodes_selection(&mut self, node_ids: Vec<u64>) {
    self.selected_wires.clear();
    for id in node_ids {
      if self.nodes.contains_key(&id) {
        if self.selected_node_ids.contains(&id) {
          self.selected_node_ids.remove(&id);
        } else {
          self.selected_node_ids.insert(id);
          self.active_node_id = Some(id);
        }
      }
    }
    // Update active node if removed
    if let Some(active) = self.active_node_id {
      if !self.selected_node_ids.contains(&active) {
        self.active_node_id = self.selected_node_ids.iter().next().copied();
      }
    }
  }

  /// Add multiple nodes to selection (for Shift+rectangle)
  pub fn add_nodes_to_selection(&mut self, node_ids: Vec<u64>) {
    self.selected_wires.clear();
    for id in &node_ids {
      if self.nodes.contains_key(id) {
        self.selected_node_ids.insert(*id);
      }
    }
    // Set active to last node in list (if valid)
    if let Some(last_id) = node_ids.last() {
      if self.selected_node_ids.contains(last_id) {
        self.active_node_id = Some(*last_id);
      }
    }
  }

  /// Check if a node is selected
  pub fn is_node_selected(&self, node_id: u64) -> bool {
    self.selected_node_ids.contains(&node_id)
  }

  /// Check if a node is the active node
  pub fn is_node_active(&self, node_id: u64) -> bool {
    self.active_node_id == Some(node_id)
  }

  /// Get all selected node IDs
  pub fn get_selected_node_ids(&self) -> &HashSet<u64> {
    &self.selected_node_ids
  }

  // ===== WIRE SELECTION =====

  /// Select a single wire (clears existing selection including nodes)
  /// Returns true if both nodes exist and the wire was selected, false otherwise.
  pub fn select_wire(&mut self, source_node_id: u64, source_output_pin_index: i32, destination_node_id: u64, destination_argument_index: usize) -> bool {
    if self.nodes.contains_key(&source_node_id) && self.nodes.contains_key(&destination_node_id) {
      self.selected_node_ids.clear();
      self.active_node_id = None;
      self.selected_wires.clear();
      self.selected_wires.push(Wire {
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

  /// Toggle wire in selection (for Ctrl+click)
  /// Returns true if both nodes exist, false otherwise.
  /// Does not clear node selection to allow mixed node+wire selections.
  pub fn toggle_wire_selection(&mut self, source_node_id: u64, source_output_pin_index: i32, destination_node_id: u64, destination_argument_index: usize) -> bool {
    if !self.nodes.contains_key(&source_node_id) || !self.nodes.contains_key(&destination_node_id) {
      return false;
    }
    
    let wire = Wire {
      source_node_id,
      source_output_pin_index,
      destination_node_id,
      destination_argument_index,
    };
    
    // Check if wire already selected
    if let Some(idx) = self.selected_wires.iter().position(|w| *w == wire) {
      self.selected_wires.remove(idx);
    } else {
      self.selected_wires.push(wire);
    }
    true
  }

  /// Add wire to selection (for Shift+click)
  /// Returns true if both nodes exist, false otherwise.
  /// Does not clear node selection to allow mixed node+wire selections.
  pub fn add_wire_to_selection(&mut self, source_node_id: u64, source_output_pin_index: i32, destination_node_id: u64, destination_argument_index: usize) -> bool {
    if !self.nodes.contains_key(&source_node_id) || !self.nodes.contains_key(&destination_node_id) {
      return false;
    }
    
    let wire = Wire {
      source_node_id,
      source_output_pin_index,
      destination_node_id,
      destination_argument_index,
    };
    
    // Only add if not already selected
    if !self.selected_wires.contains(&wire) {
      self.selected_wires.push(wire);
    }
    true
  }

  /// Check if a wire is selected
  pub fn is_wire_selected(&self, source_node_id: u64, source_output_pin_index: i32, destination_node_id: u64, destination_argument_index: usize) -> bool {
    let wire = Wire {
      source_node_id,
      source_output_pin_index,
      destination_node_id,
      destination_argument_index,
    };
    self.selected_wires.contains(&wire)
  }

  /// Get all selected wires
  pub fn get_selected_wires(&self) -> &Vec<Wire> {
    &self.selected_wires
  }

  /// Select multiple wires (replaces current selection)
  pub fn select_wires(&mut self, wires: Vec<Wire>) {
    self.selected_node_ids.clear();
    self.active_node_id = None;
    self.selected_wires.clear();
    for wire in wires {
      if self.nodes.contains_key(&wire.source_node_id) && self.nodes.contains_key(&wire.destination_node_id) && !self.selected_wires.contains(&wire) {
        self.selected_wires.push(wire);
      }
    }
  }

  /// Add multiple wires to selection (for Shift+rectangle)
  pub fn add_wires_to_selection(&mut self, wires: Vec<Wire>) {
    self.selected_node_ids.clear();
    self.active_node_id = None;
    for wire in wires {
      if self.nodes.contains_key(&wire.source_node_id) && self.nodes.contains_key(&wire.destination_node_id) && !self.selected_wires.contains(&wire) {
        self.selected_wires.push(wire);
      }
    }
  }

  /// Toggle multiple wires in selection (for Ctrl+rectangle)
  pub fn toggle_wires_selection(&mut self, wires: Vec<Wire>) {
    self.selected_node_ids.clear();
    self.active_node_id = None;
    for wire in wires {
      if self.nodes.contains_key(&wire.source_node_id) && self.nodes.contains_key(&wire.destination_node_id) {
        if let Some(idx) = self.selected_wires.iter().position(|w| *w == wire) {
          self.selected_wires.remove(idx);
        } else {
          self.selected_wires.push(wire);
        }
      }
    }
  }

  /// Select nodes and wires together (for rectangle selection)
  /// Clears existing selection and adds both nodes and wires.
  pub fn select_nodes_and_wires(&mut self, node_ids: Vec<u64>, wires: Vec<Wire>) {
    self.selected_node_ids.clear();
    self.selected_wires.clear();
    self.active_node_id = None;

    // Add nodes
    for id in &node_ids {
      if self.nodes.contains_key(id) {
        self.selected_node_ids.insert(*id);
      }
    }
    // Set active to last node in list (if valid)
    if let Some(last_id) = node_ids.last() {
      if self.selected_node_ids.contains(last_id) {
        self.active_node_id = Some(*last_id);
      }
    }

    // Add wires
    for wire in wires {
      if self.nodes.contains_key(&wire.source_node_id) && self.nodes.contains_key(&wire.destination_node_id) && !self.selected_wires.contains(&wire) {
        self.selected_wires.push(wire);
      }
    }
  }

  /// Add nodes and wires to existing selection (for Shift+rectangle)
  pub fn add_nodes_and_wires_to_selection(&mut self, node_ids: Vec<u64>, wires: Vec<Wire>) {
    // Add nodes without clearing existing selection
    for id in &node_ids {
      if self.nodes.contains_key(id) {
        self.selected_node_ids.insert(*id);
      }
    }
    // Set active to last node in list (if valid)
    if let Some(last_id) = node_ids.last() {
      if self.selected_node_ids.contains(last_id) {
        self.active_node_id = Some(*last_id);
      }
    }

    // Add wires without clearing existing selection
    for wire in wires {
      if self.nodes.contains_key(&wire.source_node_id) && self.nodes.contains_key(&wire.destination_node_id) && !self.selected_wires.contains(&wire) {
        self.selected_wires.push(wire);
      }
    }
  }

  /// Toggle nodes and wires in selection (for Ctrl+rectangle)
  pub fn toggle_nodes_and_wires_selection(&mut self, node_ids: Vec<u64>, wires: Vec<Wire>) {
    // Toggle nodes
    for id in node_ids {
      if self.nodes.contains_key(&id) {
        if self.selected_node_ids.contains(&id) {
          self.selected_node_ids.remove(&id);
        } else {
          self.selected_node_ids.insert(id);
          self.active_node_id = Some(id);
        }
      }
    }
    // Update active node if removed
    if let Some(active) = self.active_node_id {
      if !self.selected_node_ids.contains(&active) {
        self.active_node_id = self.selected_node_ids.iter().next().copied();
      }
    }

    // Toggle wires
    for wire in wires {
      if self.nodes.contains_key(&wire.source_node_id) && self.nodes.contains_key(&wire.destination_node_id) {
        if let Some(idx) = self.selected_wires.iter().position(|w| *w == wire) {
          self.selected_wires.remove(idx);
        } else {
          self.selected_wires.push(wire);
        }
      }
    }
  }

  // ===== COMMON SELECTION =====

  /// Clears any existing selection (both nodes and wires).
  pub fn clear_selection(&mut self) {
    self.selected_node_ids.clear();
    self.active_node_id = None;
    self.selected_wires.clear();
  }

  /// Provides gadget for the active node (used for property panels)
  pub fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
    if let Some(node_id) = self.active_node_id {
      let node = self.nodes.get(&node_id).unwrap();
      return node.data.provide_gadget(structure_designer);
    }
    None
  }

  /// Delete all selected nodes and wires
  pub fn delete_selected(&mut self) {
    // Handle selected nodes (delete all selected)
    if !self.selected_node_ids.is_empty() {
      let selected_ids: Vec<u64> = self.selected_node_ids.iter().cloned().collect();
      
      for node_id in selected_ids {
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
      }
      
      self.selected_node_ids.clear();
      self.active_node_id = None;
    }
    // Handle selected wires (delete all selected)
    else if !self.selected_wires.is_empty() {
      let wires_to_delete: Vec<Wire> = self.selected_wires.drain(..).collect();
      
      for wire in wires_to_delete {
        if let Some(dest_node) = self.nodes.get_mut(&wire.destination_node_id) {
          if let Some(argument) = dest_node.arguments.get_mut(wire.destination_argument_index) {
            argument.argument_output_pins.remove(&wire.source_node_id);
          }
        }
      }
    }
  }

  /// Move all selected nodes by delta
  pub fn move_selected_nodes(&mut self, delta: DVec2) {
    for &node_id in &self.selected_node_ids.clone() {
      if let Some(node) = self.nodes.get_mut(&node_id) {
        node.position += delta;
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

    // Clone the node type name for display name generation
    let node_type_name = original_node.node_type_name.clone();

    // Use node_layout module for consistent size estimation across the codebase.
    // The subtitle parameter is set to true as most nodes display a subtitle.
    let vert_offset = node_layout::duplicate_node_vertical_offset(
        max(cloned_arguments.len(), 1),
        true, // has_subtitle - assume yes for conservative spacing
    );
    let new_position = DVec2::new(original_node.position.x, original_node.position.y + vert_offset);

    // Clone the custom node type
    let custom_node_type = original_node.custom_node_type.clone();

    // Generate a unique display name for the duplicated node
    let display_name = self.generate_unique_display_name(&node_type_name);

    // Create the duplicated node
    let duplicated_node = Node {
      id: new_node_id,
      node_type_name,
      custom_name: Some(display_name),
      position: new_position,
      arguments: cloned_arguments,
      data: cloned_data,
      custom_node_type,
    };

    // Insert the duplicated node into the network
    self.nodes.insert(new_node_id, duplicated_node);

    Some(new_node_id)
  }
}
















