use crate::structure_designer::node_network::NodeNetwork;
use crate::api::structure_designer::structure_designer_preferences::{NodeDisplayPreferences, NodeDisplayPolicy};
use crate::structure_designer::node_network::NodeDisplayType;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;

pub struct NodeDisplayPolicyResolver {  
}

impl NodeDisplayPolicyResolver {
  pub fn new() -> Self {
    Self {}
  }
  
  /// Conditionally adds a node display type change to the changes map
  /// Only adds an entry if the new display type is different from the current one
  /// 
  /// # Parameters
  /// * `node_network` - The node network containing current display information
  /// * `node_id` - The ID of the node to potentially update
  /// * `new_display` - The new display type to set (Some for display, None for hide)
  /// * `changes` - The map to store changes that need to be applied
  fn add_display_change_if_needed(
    &self,
    node_network: &NodeNetwork,
    node_id: u64,
    new_display: Option<NodeDisplayType>,
    changes: &mut HashMap<u64, Option<NodeDisplayType>>
  ) {
    let current_display = node_network.get_node_display_type(node_id);
    if current_display != new_display {
      changes.insert(node_id, new_display);
    }
  }
  
  /// Applies the frontier policy to an island, identifying and displaying only frontier nodes
  /// 
  /// # Parameters
  /// * `node_network` - The node network being processed
  /// * `island` - The set of node IDs in the current island
  /// * `reverse_connections` - Map of reverse connections
  /// * `changes` - Output map to store display type changes
  fn apply_frontier_policy(
    &self,
    node_network: &NodeNetwork,
    island: &HashSet<u64>,
    reverse_connections: &HashMap<u64, HashSet<u64>>,
    changes: &mut HashMap<u64, Option<NodeDisplayType>>
  ) {
    for &node_id in island {
      // A node is considered a frontier node if it has no incoming connections
      // (i.e., it's not found in reverse_connections or its entry is empty)
      let is_frontier = !reverse_connections.contains_key(&node_id) || 
                        reverse_connections[&node_id].is_empty();
      
      // Set display type to Normal for frontier nodes, None for others
      let new_display = if is_frontier {
        Some(NodeDisplayType::Normal)
      } else {
        None
      };
      
      self.add_display_change_if_needed(node_network, node_id, new_display, changes);
    }
  }
  
  /// Calculates a map of reverse connections in the node network
  /// 
  /// # Parameters
  /// * `node_network` - The node network to analyze
  /// 
  /// # Returns
  /// A HashMap where keys are node IDs and values are sets of node IDs that connect to the key node
  fn calculate_reverse_connections(&self, node_network: &NodeNetwork) -> HashMap<u64, HashSet<u64>> {
    let mut reverse_connections: HashMap<u64, HashSet<u64>> = HashMap::new();
    
    // Build the reverse connection map
    for (&node_id, node) in &node_network.nodes {
      for arg in &node.arguments {
        for &target_node_id in &arg.argument_node_ids {
          // Add an entry that node_id connects to target_node_id
          reverse_connections
            .entry(target_node_id)
            .or_insert_with(HashSet::new)
            .insert(node_id);
        }
      }
    }
    
    reverse_connections
  }
  
  /// Finds all node islands in the network that contain any of the specified dirty nodes
  /// 
  /// # Parameters
  /// * `node_network` - The node network to find islands in
  /// * `dirty_node_ids` - Only islands containing these nodes will be returned
  /// * `reverse_connections` - Pre-calculated map of node IDs to nodes that point to them
  /// 
  /// # Returns
  /// A vector of HashSets, where each HashSet contains the node IDs in one island
  fn find_islands_with_dirty_nodes(
    &self,
    node_network: &NodeNetwork,
    dirty_node_ids: &HashSet<u64>,
    reverse_connections: &HashMap<u64, HashSet<u64>>,
  ) -> Vec<HashSet<u64>> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    
    // Check each dirty node
    for &dirty_node_id in dirty_node_ids {
      // Skip if we've already visited this node as part of a previous island
      if visited.contains(&dirty_node_id) || !node_network.nodes.contains_key(&dirty_node_id) {
        continue;
      }
      
      // Perform a breadth-first search from this dirty node to find its island
      let mut island = HashSet::new();
      let mut queue = VecDeque::new();
      queue.push_back(dirty_node_id);
      island.insert(dirty_node_id);
      visited.insert(dirty_node_id);
      
      while let Some(current_node_id) = queue.pop_front() {
        if let Some(current_node) = node_network.nodes.get(&current_node_id) {
          // Add all connected nodes to the island
          
          // Forward connections: nodes this node connects to (via arguments)
          for arg in &current_node.arguments {
            for &connected_node_id in &arg.argument_node_ids {
              if !visited.contains(&connected_node_id) && node_network.nodes.contains_key(&connected_node_id) {
                queue.push_back(connected_node_id);
                island.insert(connected_node_id);
                visited.insert(connected_node_id);
              }
            }
          }
          
          // Reverse connections: nodes that connect to this node (using our pre-calculated map)
          if let Some(incoming_connections) = reverse_connections.get(&current_node_id) {
            for &incoming_node_id in incoming_connections {
              if !visited.contains(&incoming_node_id) {
                queue.push_back(incoming_node_id);
                island.insert(incoming_node_id);
                visited.insert(incoming_node_id);
              }
            }
          }
        }
      }
      
      // Add the completed island to our results
      result.push(island);
    }
    
    result
  }

  /*
   * Resolves the node display policy.
   * 
   * # Parameters
   * * `node_network` - The node network to resolve the node display policy on
   * * `node_display_preferences` - The node display preferences to use
   * * `dirty_node_ids` - Only the node islands
   * that contain dirty nodes are recalculated
   * 
   * # Returns
   * The node ids for which the display type needs to be changed.
   */
  pub fn resolve(
    &self,
    node_network: &NodeNetwork,
    node_display_preferences: &NodeDisplayPreferences,
    dirty_node_ids: &HashSet<u64>,
  ) -> HashMap<u64, Option<NodeDisplayType>> {
    // If policy is Manual, do nothing
    if node_display_preferences.display_policy == NodeDisplayPolicy::Manual {
      return HashMap::new();
    }
    
    // Calculate reverse connections map for the node network
    let reverse_connections = self.calculate_reverse_connections(node_network);
    
    // Find islands containing dirty nodes
    let islands = self.find_islands_with_dirty_nodes(node_network, dirty_node_ids, &reverse_connections);
    
    // Create a map to store the display type changes
    let mut changes = HashMap::new();
    
    // Process each island according to the display policy
    for island in islands {
      match node_display_preferences.display_policy {
        // PreferFrontier: Display only frontier nodes
        NodeDisplayPolicy::PreferFrontier => {
          self.apply_frontier_policy(node_network, &island, &reverse_connections, &mut changes);
        },
        
        // PreferSelected: Display selected node if in island, otherwise fallback to frontier nodes
        NodeDisplayPolicy::PreferSelected => {
          let selected_in_island = node_network.selected_node_id
            .filter(|&selected_id| island.contains(&selected_id));
          
          if let Some(selected_id) = selected_in_island {
            // Selected node is in this island: show only that node
            for &node_id in &island {
              // Set display type based on whether it's the selected node
              let new_display = if node_id == selected_id {
                Some(NodeDisplayType::Normal)
              } else {
                None
              };
              self.add_display_change_if_needed(node_network, node_id, new_display, &mut changes);
            }
          } else {
            // No selected node in island: fallback to frontier policy
            self.apply_frontier_policy(node_network, &island, &reverse_connections, &mut changes);
          }
        },
        
        // Manual policy is already handled earlier
        NodeDisplayPolicy::Manual => unreachable!(),
      }
    }
    
    changes
  }
}
