use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_structure_utils::calc_selection_transform;
use glam::f64::DVec3;
use glam::f64::DVec2;
use super::node_type_registry::NodeTypeRegistry;
use super::node_network::NodeNetwork;
use super::node_type::NodeType;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_data::CustomNodeData;
use crate::structure_designer::node_type::{generic_node_data_saver, generic_node_data_loader};
use super::evaluator::network_evaluator::{NetworkEvaluator, NetworkStackElement, NetworkEvaluationContext};
use super::evaluator::network_result::NetworkResult;
use crate::structure_designer::structure_designer_scene::StructureDesignerScene;
use super::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::serialization::node_networks_serialization;
use crate::structure_designer::nodes::edit_atom::edit_atom::get_selected_edit_atom_data_mut;
use crate::structure_designer::nodes::comment::CommentData;
use crate::api::structure_designer::structure_designer_preferences::{StructureDesignerPreferences, AtomicStructureVisualization};
use crate::api::structure_designer::structure_designer_api_types::APINodeEvaluationResult;
use crate::display::atomic_tessellator::{get_displayed_atom_radius, BAS_STICK_RADIUS};
use super::node_display_policy_resolver::NodeDisplayPolicyResolver;
use super::node_networks_import_manager::NodeNetworksImportManager;
use super::network_validator::{validate_network, NetworkValidationResult};
use super::navigation_history::NavigationHistory;
use std::collections::{HashSet, HashMap};
use crate::structure_designer::implicit_eval::ray_tracing::raytrace_geometries;
use crate::geo_tree::implicit_geometry::ImplicitGeometry3D;
use crate::crystolecule::io::xyz_saver::save_xyz;
use crate::crystolecule::io::mol_exporter::save_mol_v3000;
use crate::structure_designer::data_type::DataType;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use super::structure_designer_changes::{StructureDesignerChanges, RefreshMode};
use crate::structure_designer::node_dependency_analysis::compute_downstream_dependents;
use super::camera_settings::CameraSettings;

pub struct StructureDesigner {
  pub node_type_registry: NodeTypeRegistry,
  pub network_evaluator: NetworkEvaluator,
  pub gadget: Option<Box<dyn NodeNetworkGadget>>,
  pub active_node_network_name: Option<String>,
  pub last_generated_structure_designer_scene: StructureDesignerScene,
  pub preferences: StructureDesignerPreferences,
  pub node_display_policy_resolver: NodeDisplayPolicyResolver,
  pub import_manager: NodeNetworksImportManager,
  pub is_dirty: bool,
  pub file_path: Option<String>,
  // Tracks pending changes since last refresh to determine what needs to be refreshed
  pending_changes: StructureDesignerChanges,
  // Temporary storage for CLI parameters during evaluation (used in headless mode)
  pub cli_top_level_parameters: Option<HashMap<String, NetworkResult>>,
  // Navigation history for back/forward functionality
  navigation_history: NavigationHistory,
}

impl Default for StructureDesigner {
    fn default() -> Self {
        Self::new()
    }
}

impl StructureDesigner {

  pub fn new() -> Self {

    let node_type_registry = NodeTypeRegistry::new();
    let network_evaluator = NetworkEvaluator::new();
    let node_display_policy_resolver = NodeDisplayPolicyResolver::new();

    Self {
      node_type_registry,
      network_evaluator,
      gadget: None,
      active_node_network_name: None,
      last_generated_structure_designer_scene: StructureDesignerScene::new(),
      preferences: StructureDesignerPreferences::new(),
      node_display_policy_resolver,
      import_manager: NodeNetworksImportManager::new(),
      is_dirty: false,
      file_path: None,
      pending_changes: StructureDesignerChanges::default(),
      cli_top_level_parameters: None,
      navigation_history: NavigationHistory::new(),
    }
  }
}

impl StructureDesigner {

  // Returns the first atomic structure generated from a selected node, if any
  pub fn get_atomic_structure_from_selected_node(&self) -> Option<&AtomicStructure> {
    use crate::structure_designer::structure_designer_scene::NodeOutput;
    // Find the first atomic structure with from_selected_node = true
    for node_data in self.last_generated_structure_designer_scene.node_data.values() {
      if let NodeOutput::Atomic(atomic_structure) = &node_data.output {
        if atomic_structure.decorator().from_selected_node {
          return Some(atomic_structure);
        }
      }
    }
    None
  }

  /// Gets the eval cache for the currently active node (used for gadget creation)
  /// Returns None if no node is active or the active node has no eval cache
  pub fn get_selected_node_eval_cache(&self) -> Option<&Box<dyn std::any::Any>> {
    let network_name = self.active_node_network_name.as_ref()?;
    let network = self.node_type_registry.node_networks.get(network_name)?;
    let active_node_id = network.active_node_id?;
    self.last_generated_structure_designer_scene.get_node_eval_cache(active_node_id)
  }

  /// Helper method to get the active node ID of a node of a specific type
  /// 
  /// Returns None if:
  /// - There is no active node network
  /// - No node is active in the active network
  /// - The active node has a different type name than the needed node type name
  pub fn get_selected_node_id_with_type(&self,  needed_node_type_name: &str) -> Option<u64> {
    // Get active node network name
    let network_name = self.active_node_network_name.as_ref()?;
    
    // Get the active node network
    let network = self.node_type_registry.node_networks.get(network_name)?;
    
    // Get the active node ID
    let active_node_id = network.active_node_id?;
    
    // Get the active node's type name
    let node_type_name = network.nodes.get(&active_node_id)?.node_type_name.as_str();
    
    // Check if the node is with the needed node type name
    if node_type_name != needed_node_type_name {
      return None;
    }

    Some(active_node_id)
  }

  // Returns true if the active node is displayed and has the needed node type name
  pub fn is_node_type_active(&self, needed_node_type_name: &str) -> bool {
    // Check if active_node_network_name exists
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return false,
    };
    
    // Get the active node network
    let network = match self.node_type_registry.node_networks.get(network_name) {
      Some(network) => network,
      None => return false,
    };
    
    // Check if there's an active node ID
    let active_node_id = match network.active_node_id {
      Some(id) => id,
      None => return false,
    };
    
    // Check if the active node is displayed
    if !network.is_node_displayed(active_node_id) {
      return false;
    }
    
    // Get the active node's type name
    let node_type_name = match network.nodes.get(&active_node_id) {
      Some(node) => &node.node_type_name,
      None => return false,
    };
    
    // Return true only if the node's type name matches the needed node type name
    node_type_name == needed_node_type_name
  }

  /// Returns a clone of the pending changes to determine what needs to be refreshed
  pub fn get_pending_changes(&self) -> StructureDesignerChanges {
    self.pending_changes.clone()
  }

  /// Returns true if the pending refresh is lightweight (for Renderer)
  pub fn is_pending_refresh_lightweight(&self) -> bool {
    self.pending_changes.is_lightweight()
  }

  /// Marks a node's data as changed
  pub fn mark_node_data_changed(&mut self, node_id: u64) {
    self.pending_changes.mark_node_data_changed(node_id);
  }

  /// Marks that a full refresh is needed (for complex/unknown changes)
  pub fn mark_full_refresh(&mut self) {
    self.pending_changes.set_mode(RefreshMode::Full);
  }

  /// Marks that a lightweight refresh is needed (gadget tessellation only)
  pub fn mark_lightweight_refresh(&mut self) {
    self.pending_changes.set_mode(RefreshMode::Lightweight);
  }

  /// Marks that selection changed
  pub fn mark_selection_changed(&mut self, previous_selection: Option<u64>, current_selection: Option<u64>) {
    self.pending_changes.mark_selection_changed(previous_selection, current_selection);
  }

  // Generates the scene to be rendered according to the displayed nodes of the active node network
  pub fn refresh(&mut self, changes: &StructureDesignerChanges) {
    // Clear pending changes at the start of refresh
    self.pending_changes.clear();

    // Check if node_network_name exists and clone it to avoid borrow conflicts
    let node_network_name = match &self.active_node_network_name {
      Some(name) => name.clone(),
      None => return, // Return if node_network_name is None
    };

    match changes.mode {
      RefreshMode::Lightweight => {
        // Lightweight refresh - only update gadget tessellation without re-evaluation
        // The gadget is already active and should not be recreated
        self.refresh_scene_dependent_node_data();
        if let Some(gadget) = &self.gadget {
          self.last_generated_structure_designer_scene.tessellatable = Some(gadget.as_tessellatable());
        }
      }
      
      RefreshMode::Full => {
        // Full refresh - re-evaluate everything
        self.refresh_full(&node_network_name);
      }
      
      RefreshMode::Partial => {
        // Partial refresh - use tracked changes
        self.refresh_partial(&node_network_name, changes);
      }
    }
  }
  
  // Full refresh implementation - re-evaluates all displayed nodes
  fn refresh_full(&mut self, node_network_name: &str) {
    let network = match self.node_type_registry.node_networks.get(node_network_name) {
      Some(network) => network,
      None => return,
    };
    
    // Create new scene with empty node_data HashMap and invisibility cache.
    self.last_generated_structure_designer_scene = StructureDesignerScene::new();
    
    // Track selected node's unit cell
    let mut selected_node_unit_cell: Option<UnitCellStruct> = None;
    
    // Generate NodeSceneData for each displayed node and populate node_data HashMap
    for node_entry in &network.displayed_node_ids {
      let node_id = *node_entry.0;
      let display_type = *node_entry.1;
      
      // Generate NodeSceneData for this node
      let node_data = self.network_evaluator.generate_scene(
        node_network_name,
        node_id,
        display_type,
        &self.node_type_registry,
        &self.preferences.geometry_visualization_preferences,
        self.cli_top_level_parameters.clone(),
      );
      
      // Capture the active node's unit cell
      if Some(node_id) == network.active_node_id {
        selected_node_unit_cell = node_data.unit_cell.clone();
      }
      
      // Insert into final scene's node_data HashMap
      self.last_generated_structure_designer_scene.node_data.insert(node_id, node_data);
    }
    
    // Set the selected node's unit cell as global scene property
    // Note: eval_cache is now accessed directly from node_data via get_selected_node_eval_cache()
    self.last_generated_structure_designer_scene.unit_cell = selected_node_unit_cell;

    self.refresh_scene_dependent_node_data();
    
    // Recreate the gadget for the selected node
    if let Some(network) = self.node_type_registry.node_networks.get(node_network_name) {
      self.gadget = network.provide_gadget(self);
    }

    if let Some(gadget) = &self.gadget {
      self.last_generated_structure_designer_scene.tessellatable = Some(gadget.as_tessellatable());
    }
  }
  
  // Partial refresh implementation - only re-evaluates affected nodes
  // Uses invisible node caching for ultra-fast visibility changes
  fn refresh_partial(&mut self, node_network_name: &str, changes: &StructureDesignerChanges) {

    let network = match self.node_type_registry.node_networks.get(node_network_name) {
      Some(network) => network,
      None => return,
    };
    
    // Clone necessary data before mutable borrows to avoid borrow checker issues
    let active_node_id = network.active_node_id;
    
    // Step 1: Cache nodes that became invisible
    for &node_id in &changes.visibility_changed {
      if !network.displayed_node_ids.contains_key(&node_id) {
        // Node became invisible - move to cache for potential future restoration
        self.last_generated_structure_designer_scene.move_to_cache(node_id);
      }
    }
    
    // Step 2: Compute transitive dependencies of data changes and invalidate cache
    let affected_by_data_changes = if !changes.data_changed.is_empty() {
      let affected = compute_downstream_dependents(network, &changes.data_changed);
      self.last_generated_structure_designer_scene.invalidate_cached_nodes(&affected);
      affected
    } else {
      HashSet::new()
    };
    
    // Step 3: Restore nodes that became visible from cache (if possible)
    // (At this point we have data in the cache that actually can be restored.)
    let mut nodes_needing_evaluation = HashSet::new();
    
    for &node_id in &changes.visibility_changed {
      if network.displayed_node_ids.contains_key(&node_id) {
        // Node became visible - try to restore from cache (ultra-fast path)
        let restored = self.last_generated_structure_designer_scene.restore_from_cache(node_id);
  
        if !restored {
          // Not in cache (or was invalidated) - needs re-evaluation
          nodes_needing_evaluation.insert(node_id);
        }
        // Note: If restored successfully, eval_cache is preserved in node_data
        // and accessible via get_selected_node_eval_cache() for gadget creation
      }
    }
    
    // Step 4: Add visible nodes affected by data changes to evaluation set
    for &node_id in &affected_by_data_changes {
      if network.displayed_node_ids.contains_key(&node_id) {
        nodes_needing_evaluation.insert(node_id);
      }
    }
    
    // Step 4.5: Handle selection changes - re-evaluate affected nodes to update from_selected_node flag
    if changes.selection_changed {
      // Add previous selected node (needs from_selected_node set to false)
      if let Some(prev_node_id) = changes.previous_selection {
        if network.displayed_node_ids.contains_key(&prev_node_id) {
          nodes_needing_evaluation.insert(prev_node_id);
        }
      }
      // Add current selected node (needs from_selected_node set to true)
      if let Some(curr_node_id) = changes.current_selection {
        if network.displayed_node_ids.contains_key(&curr_node_id) {
          nodes_needing_evaluation.insert(curr_node_id);
        }
      }
    }
      
    // Track selected node's unit cell
    let mut selected_node_unit_cell: Option<UnitCellStruct> = None;
    
    // Step 5: Re-evaluate nodes that need it (skip if empty)
    if !nodes_needing_evaluation.is_empty() {
      for &node_id in &nodes_needing_evaluation {
        // Get the display type for this node (it must be displayed if it's in nodes_needing_evaluation)
        let display_type = {
          let network = match self.node_type_registry.node_networks.get(node_network_name) {
            Some(network) => network,
            None => continue,
          };
          match network.displayed_node_ids.get(&node_id) {
            Some(&display_type) => display_type,
            None => continue, // Skip if not displayed (shouldn't happen)
          }
        };
        
        // Generate NodeSceneData for this node
        let node_data = self.network_evaluator.generate_scene(
          node_network_name,
          node_id,
          display_type,
          &self.node_type_registry,
          &self.preferences.geometry_visualization_preferences,
          self.cli_top_level_parameters.clone(),
        );
        
        // Capture the active node's unit cell
        if Some(node_id) == active_node_id {
          selected_node_unit_cell = node_data.unit_cell.clone();
        }
        
        // Update or insert into scene's node_data HashMap
        self.last_generated_structure_designer_scene.node_data.insert(node_id, node_data);
      }
      
      // Update the selected node's unit cell if it was re-evaluated
      // Note: eval_cache is now accessed directly from node_data via get_selected_node_eval_cache()
      if selected_node_unit_cell.is_some() {
        self.last_generated_structure_designer_scene.unit_cell = selected_node_unit_cell;
      }
    }
    
    self.refresh_scene_dependent_node_data();
    
    // Always refresh the gadget (simplest approach - handles all cases)
    // This ensures gadget is updated when:
    // - Selected node was re-evaluated
    // - Selected node was restored from cache
    // - Selection changed
    // - Node with gadget becomes node without gadget (gadget disappears)
    if let Some(network) = self.node_type_registry.node_networks.get(node_network_name) {
      self.gadget = network.provide_gadget(self);
      if let Some(gadget) = &self.gadget {
        self.last_generated_structure_designer_scene.tessellatable = Some(gadget.as_tessellatable());
      } else {
        // No gadget for selected node - clear tessellatable
        self.last_generated_structure_designer_scene.tessellatable = None;
      }
    }
  }

  // node network methods

  pub fn add_new_node_network(&mut self) {
    // Generate a unique name for the new node network
    let mut name = "UNTITLED".to_string();
    let mut i = 1;
    while self.node_type_registry.node_networks.contains_key(&name) {
      name = format!("UNTITLED{}", i);
      i += 1;
    }
    self.add_node_network(&name);
    // Mark design as dirty since we added a new network
    self.set_dirty(true);
    // Adding a network is a structural change requiring full refresh
    self.mark_full_refresh();
  }
  
  pub fn add_node_network(&mut self, node_network_name: &str) {
    self.node_type_registry.add_node_network(NodeNetwork::new(
      NodeType {
        name: node_network_name.to_string(),
        description: "".to_string(),
        summary: None,
        category: crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory::Custom,
        parameters: Vec::new(),
        output_type: DataType::None,
        node_data_creator: || Box::new(CustomNodeData::default()),
        node_data_saver: generic_node_data_saver::<CustomNodeData>,
        node_data_loader: generic_node_data_loader::<CustomNodeData>,
        public: true,
      }
    ));
  }

  pub fn rename_node_network(&mut self, old_name: &str, new_name: &str) -> bool {
    // Check if the old network exists and the new name doesn't already exist
    if !self.node_type_registry.node_networks.contains_key(old_name) {
      return false; // Old network doesn't exist
    }
    if self.node_type_registry.node_networks.contains_key(new_name) {
      return false; // New name already exists
    }
    // Check if the new name conflicts with a built-in node type
    if self.node_type_registry.built_in_node_types.contains_key(new_name) {
      return false; // New name conflicts with built-in node type
    }

    // Take the network out of the registry
    let mut network = match self.node_type_registry.node_networks.remove(old_name) {
      Some(network) => network,
      None => return false, // Should never happen because we checked contains_key above
    };

    // Update the network's internal node type name
    network.node_type.name = new_name.to_string();

    // Add the network back with the new name
    self.node_type_registry.node_networks.insert(new_name.to_string(), network);

    // Update the active_node_network_name if it was the renamed network
    if let Some(active_name) = &self.active_node_network_name {
      if active_name == old_name {
        self.active_node_network_name = Some(new_name.to_string());
      }
    }

    // Update navigation history to reflect the rename
    self.navigation_history.rename_network(old_name, new_name);

    // Update all nodes in all node networks that reference the old node type name
    // This is necessary because node networks can be used as custom nodes in other networks
    for (_network_name, network) in self.node_type_registry.node_networks.iter_mut() {
      for (_node_id, node) in network.nodes.iter_mut() {
        if node.node_type_name == old_name {
          node.node_type_name = new_name.to_string();
        }
      }
    }

    // Update backtick references in comment nodes and node network metadata across all networks
    // This keeps documentation in sync when node networks are renamed
    let old_pattern = format!("`{}`", old_name);
    let new_pattern = format!("`{}`", new_name);
    for (_network_name, network) in self.node_type_registry.node_networks.iter_mut() {
      // Update summary and description fields of the node network
      if network.node_type.description.contains(&old_pattern) {
        network.node_type.description = network.node_type.description.replace(&old_pattern, &new_pattern);
      }
      if let Some(ref mut summary) = network.node_type.summary {
        if summary.contains(&old_pattern) {
          *summary = summary.replace(&old_pattern, &new_pattern);
        }
      }

      // Update comment nodes
      for (_node_id, node) in network.nodes.iter_mut() {
        if node.node_type_name == "Comment" {
          if let Some(comment_data) = node.data.as_any_mut().downcast_mut::<CommentData>() {
            if comment_data.label.contains(&old_pattern) {
              comment_data.label = comment_data.label.replace(&old_pattern, &new_pattern);
            }
            if comment_data.text.contains(&old_pattern) {
              comment_data.text = comment_data.text.replace(&old_pattern, &new_pattern);
            }
          }
        }
      }
    }

    // Mark design as dirty since we renamed a network
    self.set_dirty(true);
    // Renaming a network is a structural change requiring full refresh
    self.mark_full_refresh();
    true
  }

  pub fn delete_node_network(&mut self, network_name: &str) -> Result<(), String> {
    // Check if the network exists
    if !self.node_type_registry.node_networks.contains_key(network_name) {
      return Err(format!("Node network '{}' does not exist", network_name));
    }

    // Check if any nodes in any network reference this network
    // Collect the names of networks that contain nodes referencing this network
    let mut referencing_networks = Vec::new();
    for (current_network_name, network) in self.node_type_registry.node_networks.iter() {
      for (_node_id, node) in network.nodes.iter() {
        if node.node_type_name == network_name {
          referencing_networks.push(current_network_name.clone());
          break; // No need to check more nodes in this network
        }
      }
    }

    // If there are references, return an error with the referencing network names
    if !referencing_networks.is_empty() {
      return Err(format!(
        "Cannot delete node network '{}' because it is referenced by nodes in the following networks: {}",
        network_name,
        referencing_networks.join(", ")
      ));
    }

    // Remove the network from the registry
    self.node_type_registry.node_networks.remove(network_name);

    // Update the active_node_network_name if it was the deleted network
    if let Some(active_name) = &self.active_node_network_name {
      if active_name == network_name {
        self.active_node_network_name = None;
      }
    }

    // Remove the deleted network from navigation history
    self.navigation_history.remove_network(network_name);

    // Mark design as dirty since we deleted a network
    self.set_dirty(true);
    // Deleting a network requires full refresh (complex change)
    self.mark_full_refresh();
    Ok(())
  }

  pub fn add_node(&mut self, node_type_name: &str, position: DVec2) -> u64 {
    // Early return if active_node_network_name is None
    let node_network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return 0,
    };
    // First get the node type info
    let (num_parameters, mut node_data) = match self.node_type_registry.get_node_type(node_type_name) {
      Some(node_type) => {
        let data_creator = &node_type.node_data_creator;
        (node_type.parameters.len(), (data_creator)())
      },
      None => return 0,
    };

    // Special handling for parameter nodes
    if node_type_name == "parameter" {
      if let Some(node_network) = self.node_type_registry.node_networks.get_mut(node_network_name) {
        let current_param_count = node_network.node_type.parameters.len();

        // Assign a unique param_id from the network's counter
        let param_id = node_network.next_param_id;
        node_network.next_param_id += 1;

        // Downcast to ParameterData and set properties
        if let Some(param_data) = node_data.as_any_mut().downcast_mut::<crate::structure_designer::nodes::parameter::ParameterData>() {
          param_data.param_id = Some(param_id);  // Assign unique ID for wire preservation
          param_data.param_name = format!("param{}", current_param_count);
          param_data.sort_order = current_param_count as i32;
        }
      }
    }

    // Early return if the node network doesn't exist
    let node_id = self.node_type_registry.node_networks.get_mut(node_network_name)
      .map(|node_network| node_network.add_node(node_type_name, position, num_parameters, node_data))
      .unwrap_or(0);
    
    // If we successfully added a node, initialize custom node type if needed
    if node_id != 0 {
      // Split the borrow to avoid conflicts
      let (built_in_types, node_networks) = (&self.node_type_registry.built_in_node_types, &mut self.node_type_registry.node_networks);
      if let Some(network) = node_networks.get_mut(node_network_name) {
        if let Some(node) = network.nodes.get_mut(&node_id) {
          // Call the populate function with the split borrows
          NodeTypeRegistry::populate_custom_node_type_cache_with_types(built_in_types, node, true);
        }
      }
    }
    
    // If we successfully added a node, apply the display policy with this node as dirty
    if node_id != 0 {
      // Mark design as dirty since we added a node
      self.set_dirty(true);
      
      // Create a HashSet with just the new node ID
      let mut dirty_nodes = HashSet::new();
      dirty_nodes.insert(node_id);
      
      // Track visibility change for the new node (it was set to visible in add_node)
      // This is needed because the node was made visible directly on node_network,
      // bypassing StructureDesigner.set_node_display which normally tracks this
      self.pending_changes.visibility_changed.insert(node_id);
      
      // Apply display policy considering only this node as dirty
      self.apply_node_display_policy(Some(&dirty_nodes));
      
      // Check if we need to validate the network
      let should_validate = node_type_name == "parameter" || {
        // Check if this node references an invalid node network
        self.node_type_registry.node_networks.get(node_type_name)
          .map(|network| !network.valid)
          .unwrap_or(false)
      };
      
      if should_validate {
        self.validate_active_network();
      }
    }
    
    node_id
  }

  pub fn duplicate_node(&mut self, node_id: u64) -> u64 {
    // Early return if active_node_network_name is None
    let node_network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return 0,
    };

    // Early return if the node network doesn't exist
    let new_node_id = self.node_type_registry.node_networks.get_mut(node_network_name)
      .and_then(|node_network| node_network.duplicate_node(node_id))
      .unwrap_or(0);
    
    // If we successfully duplicated a node, apply the display policy with this node as dirty
    if new_node_id != 0 {
      // Mark design as dirty since we duplicated a node
      self.set_dirty(true);
      
      // Create a HashSet with just the new node ID
      let mut dirty_nodes = HashSet::new();
      dirty_nodes.insert(new_node_id);
      
      // Apply display policy considering only this node as dirty
      self.apply_node_display_policy(Some(&dirty_nodes));
    }

    self.mark_node_data_changed(node_id);
    
    new_node_id
  }


  pub fn move_node(&mut self, node_id: u64, position: DVec2) {
    // Early return if active_node_network_name is None
    let node_network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(node_network) = self.node_type_registry.node_networks.get_mut(node_network_name) {
      node_network.move_node(node_id, position);
      // Mark design as dirty since we moved a node
      self.set_dirty(true);
    }
  }

  pub fn can_connect_nodes(&self, source_node_id: u64, source_output_pin_index: i32, dest_node_id: u64, dest_param_index: usize) -> bool {
    // Early return if active_node_network_name is None
    let node_network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return false,
    };
    
    // Get the network
    let network = match self.node_type_registry.node_networks.get(node_network_name) {
      Some(network) => network,
      None => return false,
    };
    
    network.can_connect_nodes(source_node_id, source_output_pin_index, dest_node_id, dest_param_index, &self.node_type_registry)
  }

  pub fn connect_nodes(&mut self, source_node_id: u64, source_output_pin_index: i32, dest_node_id: u64, dest_param_index: usize) {
    // Early return if active_node_network_name is None
    let node_network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    // First validate the connection
    let dest_param_is_multi = {
      // Get the network
      let network = match self.node_type_registry.node_networks.get(node_network_name) {
        Some(network) => network,
        None => return,
      };

      // Get the destination node
      let dest_node = match network.nodes.get(&dest_node_id) {
        Some(node) => node,
        None => return,
      };

      // Get the node type and check parameter
      match self.node_type_registry.get_node_type_for_node(dest_node) {
        Some(node_type) => {
          if dest_param_index >= node_type.parameters.len() {
            return;
          }
          node_type.parameters[dest_param_index].data_type.is_array()
        }
        None => return,
      }
    };

    // Then make the connection
    if let Some(node_network) = self.node_type_registry.node_networks.get_mut(node_network_name) {
      node_network.connect_nodes(
        source_node_id,
        source_output_pin_index,
        dest_node_id,
        dest_param_index,
        dest_param_is_multi,
      );
      
      // Mark design as dirty since we connected nodes
      self.set_dirty(true);
      // Mark the destination node as having data changed (new input connection)
      self.mark_node_data_changed(dest_node_id);
      
      // Create a HashSet with the source and destination nodes marked as dirty
      let mut dirty_nodes = HashSet::new();
      dirty_nodes.insert(source_node_id);
      dirty_nodes.insert(dest_node_id);
      
      // Apply display policy considering only these nodes as dirty
      self.apply_node_display_policy(Some(&dirty_nodes));
    }
  }

  /// Auto-connects a source pin to the first compatible pin on a target node.
  /// 
  /// - When `source_is_output` is true: connects source output to target's first compatible input
  /// - When `source_is_output` is false: connects target's output to source input
  /// 
  /// Returns true if a connection was made, false otherwise.
  pub fn auto_connect_to_node(
    &mut self,
    source_node_id: u64,
    source_pin_index: i32,
    source_is_output: bool,
    target_node_id: u64,
  ) -> bool {
    // Early return if active_node_network_name is None
    let node_network_name = match &self.active_node_network_name {
      Some(name) => name.clone(),
      None => return false,
    };

    // Get source and target node types to find compatible pins
    let connection_info = {
      let network = match self.node_type_registry.node_networks.get(&node_network_name) {
        Some(network) => network,
        None => return false,
      };

      let source_node = match network.nodes.get(&source_node_id) {
        Some(node) => node,
        None => return false,
      };

      let target_node = match network.nodes.get(&target_node_id) {
        Some(node) => node,
        None => return false,
      };

      let source_node_type = match self.node_type_registry.get_node_type_for_node(source_node) {
        Some(nt) => nt,
        None => return false,
      };

      let target_node_type = match self.node_type_registry.get_node_type_for_node(target_node) {
        Some(nt) => nt,
        None => return false,
      };

      if source_is_output {
        // Source is output, find first compatible input on target
        let source_output_type = source_node_type.get_output_pin_type(source_pin_index);
        
        // Find first compatible input parameter on target node
        let mut compatible_param_index: Option<usize> = None;
        for (param_idx, param) in target_node_type.parameters.iter().enumerate() {
          if DataType::can_be_converted_to(&source_output_type, &param.data_type) {
            compatible_param_index = Some(param_idx);
            break;
          }
        }

        compatible_param_index.map(|param_idx| (source_node_id, source_pin_index, target_node_id, param_idx))
      } else {
        // Source is input, connect target's output to source's input pin
        let target_output_type = &target_node_type.output_type;
        let source_param_type = self.node_type_registry.get_node_param_data_type(source_node, source_pin_index as usize);
        
        if DataType::can_be_converted_to(target_output_type, &source_param_type) {
          // Connect target output (pin 0) to source input
          Some((target_node_id, 0, source_node_id, source_pin_index as usize))
        } else {
          None
        }
      }
    };

    // Make the connection if we found compatible pins
    if let Some((src_node, src_pin, dest_node, dest_param)) = connection_info {
      self.connect_nodes(src_node, src_pin, dest_node, dest_param);
      return true;
    }

    false
  }

  /// Returns all compatible pins on the target node for auto-connection.
  /// Each tuple contains (pin_index, pin_name, data_type_string).
  /// When source_is_output is true, returns compatible INPUT pins on target.
  /// When source_is_output is false, returns the OUTPUT pin if compatible.
  pub fn get_compatible_pins_for_auto_connect(
    &self,
    source_node_id: u64,
    source_pin_index: i32,
    source_is_output: bool,
    target_node_id: u64,
  ) -> Vec<(i32, String, String)> {
    let node_network_name = match &self.active_node_network_name {
      Some(name) => name.clone(),
      None => return Vec::new(),
    };

    let network = match self.node_type_registry.node_networks.get(&node_network_name) {
      Some(network) => network,
      None => return Vec::new(),
    };

    let source_node = match network.nodes.get(&source_node_id) {
      Some(node) => node,
      None => return Vec::new(),
    };

    let target_node = match network.nodes.get(&target_node_id) {
      Some(node) => node,
      None => return Vec::new(),
    };

    let source_node_type = match self.node_type_registry.get_node_type_for_node(source_node) {
      Some(nt) => nt,
      None => return Vec::new(),
    };

    let target_node_type = match self.node_type_registry.get_node_type_for_node(target_node) {
      Some(nt) => nt,
      None => return Vec::new(),
    };

    let mut compatible_pins = Vec::new();

    if source_is_output {
      // Source is output, find all compatible input parameters on target
      let source_output_type = source_node_type.get_output_pin_type(source_pin_index);
      
      for (param_idx, param) in target_node_type.parameters.iter().enumerate() {
        if DataType::can_be_converted_to(&source_output_type, &param.data_type) {
          compatible_pins.push((
            param_idx as i32,
            param.name.clone(),
            param.data_type.to_string(),
          ));
        }
      }
    } else {
      // Source is input, check if target's output is compatible
      let target_output_type = &target_node_type.output_type;
      let source_param_type = self.node_type_registry.get_node_param_data_type(source_node, source_pin_index as usize);
      
      if DataType::can_be_converted_to(target_output_type, &source_param_type) {
        // Output pin is always index 0 with name "output"
        compatible_pins.push((
          0,
          "output".to_string(),
          target_output_type.to_string(),
        ));
      }
    }

    compatible_pins
  }

  pub fn set_node_network_data(&mut self, node_id: u64, mut data: Box<dyn NodeData>) {
    // Early return if active_node_network_name is None, clone to avoid borrow conflicts
    let network_name = match &self.active_node_network_name {
      Some(name) => name.clone(),
      None => return,
    };
    
    // Check node type before modification
    let is_expr_node = if let Some(network) = self.node_type_registry.node_networks.get(&network_name) {
      if let Some(node) = network.nodes.get(&node_id) {
        node.node_type_name == "expr"
      } else {
        false
      }
    } else {
      false
    };
    
    // For expr nodes, validate the expression before setting the data
    let mut expr_validation_errors = Vec::new();
    if is_expr_node {
      if let Some(expr_data) = data.as_any_mut().downcast_mut::<crate::structure_designer::nodes::expr::ExprData>() {
        expr_validation_errors = expr_data.parse_and_validate(node_id);
      }
    }
    
    if let Some(network) = self.node_type_registry.node_networks.get_mut(&network_name) {
      network.set_node_network_data(node_id, data);
      // Mark design as dirty since we modified node data
      self.set_dirty(true);
      // Track that this node's data changed
      self.mark_node_data_changed(node_id);
    }
    
    // Cache custom NodeType if needed after data is set
    let (built_in_types, node_networks) = (&self.node_type_registry.built_in_node_types, &mut self.node_type_registry.node_networks);
    let custom_node_type_populated = if let Some(network) = node_networks.get_mut(&network_name) {
      if let Some(node) = network.nodes.get_mut(&node_id) {
        // Call the populate function with the split borrows
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(built_in_types, node, true)
      } else {
        false
      }
    } else {
      false
    };

    // Validate if this node has a custom node type
    if custom_node_type_populated {
      let initial_errors = if expr_validation_errors.is_empty() {
        None
      } else {
        Some(expr_validation_errors)
      };
      self.validate_active_network_with_initial_errors(initial_errors);
    }
  }

  // Refresh special gadgets that are dependent on the scene, not only on node data.
  fn refresh_scene_dependent_node_data(&mut self) {
    self.refresh_scene_dependent_edit_atom_data();
  }
  
  fn refresh_scene_dependent_edit_atom_data(&mut self) {
    // First calculate the selection transform
    let selection_transform = self.get_atomic_structure_from_selected_node()
      .and_then(calc_selection_transform);
    
    // Then update the edit atom data with the pre-calculated transform
    if let Some(edit_atom_data) = get_selected_edit_atom_data_mut(self) {
      edit_atom_data.selection_transform = selection_transform;
    }
  }

  pub fn get_node_network_data(&self, node_id: u64) -> Option<&dyn NodeData> {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return None,
    };
    self.node_type_registry
      .node_networks
      .get(network_name)
      .and_then(|network| network.get_node_network_data(node_id))
  }

  pub fn get_node_network_data_mut(&mut self, node_id: u64) -> Option<&mut dyn NodeData> {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return None,
    };
    self.pending_changes.mark_node_data_changed(node_id);
    self.node_type_registry
      .node_networks
      .get_mut(network_name)
      .and_then(|network| network.get_node_network_data_mut(node_id))
  }

  pub fn get_network_evaluator(&self) -> &NetworkEvaluator {
    &self.network_evaluator
  }

  /// Returns a reference to the active node network, if any
  pub fn get_active_node_network(&self) -> Option<&NodeNetwork> {
    let network_name = self.active_node_network_name.as_ref()?;
    self.node_type_registry.node_networks.get(network_name)
  }

  /// Returns a mutable reference to the active node network, if any
  pub fn get_active_node_network_mut(&mut self) -> Option<&mut NodeNetwork> {
    let network_name = self.active_node_network_name.as_ref()?;
    self.node_type_registry.node_networks.get_mut(network_name)
  }

  /// Gets the description of the active node network
  pub fn get_active_network_description(&self) -> Option<String> {
    let network = self.get_active_node_network()?;
    Some(network.node_type.description.clone())
  }

  /// Sets the description of the active node network
  pub fn set_active_network_description(&mut self, description: String) -> Result<(), String> {
    let network_name = self.active_node_network_name.as_ref()
      .ok_or("No active node network")?;

    let network = self.node_type_registry.node_networks.get_mut(network_name)
      .ok_or("Active network not found")?;

    network.node_type.description = description;
    self.set_dirty(true);
    Ok(())
  }

  /// Gets the summary of the active node network
  pub fn get_active_network_summary(&self) -> Option<String> {
    let network = self.get_active_node_network()?;
    network.node_type.summary.clone()
  }

  /// Sets the summary of the active node network
  /// Pass None or empty string to clear the summary
  pub fn set_active_network_summary(&mut self, summary: Option<String>) -> Result<(), String> {
    let network_name = self.active_node_network_name.as_ref()
      .ok_or("No active node network")?;

    let network = self.node_type_registry.node_networks.get_mut(network_name)
      .ok_or("Active network not found")?;

    // Convert empty string to None
    network.node_type.summary = summary.filter(|s| !s.is_empty());
    self.set_dirty(true);
    Ok(())
  }

  /// Gets the name and description of a specific node type (built-in or custom network)
  /// Returns (name, description) tuple
  pub fn get_network_description(&self, network_name: &str) -> Option<(String, String)> {
    // First check built-in node types
    if let Some(node_type) = self.node_type_registry.built_in_node_types.get(network_name) {
      return Some((node_type.name.clone(), node_type.description.clone()));
    }
    
    // Then check custom node networks
    if let Some(network) = self.node_type_registry.node_networks.get(network_name) {
      return Some((network.node_type.name.clone(), network.node_type.description.clone()));
    }
    
    None
  }

  /// Sets the active node network and returns the camera settings to apply (if any).
  /// The caller is responsible for applying the returned settings to the renderer.
  pub fn set_active_node_network_name(&mut self, node_network_name: Option<String>) -> Option<CameraSettings> {
    self.navigation_history.navigate_to(node_network_name.clone());
    self.active_node_network_name = node_network_name;
    // Switching networks requires full refresh (everything changes)
    self.mark_full_refresh();
    // Return camera settings from the newly active network
    self.get_active_node_network()
      .and_then(|n| n.camera_settings.clone())
  }

  /// Returns true if the design has been modified since the last save/load
  pub fn is_dirty(&self) -> bool {
    self.is_dirty
  }

  /// Sets the dirty flag to indicate the design has been modified
  pub fn set_dirty(&mut self, dirty: bool) {
    self.is_dirty = dirty;
  }

  /// Returns the file path where the design was last saved/loaded, or None if never saved/loaded
  pub fn get_file_path(&self) -> Option<&String> {
    self.file_path.as_ref()
  }

  /// Clears all networks and creates a fresh project with a single "Main" network.
  ///
  /// This resets the state to match a newly opened application:
  /// - Clears all networks
  /// - Creates a new empty "Main" network
  /// - Clears the file path (no file associated)
  /// - Clears the dirty flag
  /// - Clears navigation history
  /// - Clears evaluation cache
  pub fn new_project(&mut self) {
    // Clear all networks
    self.node_type_registry.node_networks.clear();

    // Create a fresh "Main" network and set it as active
    self.add_node_network("Main");
    self.active_node_network_name = Some("Main".to_string());

    // Clear file state
    self.file_path = None;
    self.is_dirty = false;

    // Clear navigation history
    self.navigation_history.clear();

    // Clear evaluation cache
    self.network_evaluator.clear_csg_cache();

    // Clear the last generated scene
    self.last_generated_structure_designer_scene = StructureDesignerScene::new();

    // Mark for full refresh
    self.mark_full_refresh();
  }

  /// Navigates back in network history
  /// Returns (success, camera_settings) where success indicates if navigation occurred
  /// and camera_settings contains the camera settings to apply (if any)
  pub fn navigate_back(&mut self) -> (bool, Option<CameraSettings>) {
    if let Some(network_name) = self.navigation_history.navigate_back() {
      self.active_node_network_name = network_name;
      self.mark_full_refresh();
      let camera_settings = self.get_active_node_network()
        .and_then(|n| n.camera_settings.clone());
      (true, camera_settings)
    } else {
      (false, None)
    }
  }

  /// Navigates forward in network history
  /// Returns (success, camera_settings) where success indicates if navigation occurred
  /// and camera_settings contains the camera settings to apply (if any)
  pub fn navigate_forward(&mut self) -> (bool, Option<CameraSettings>) {
    if let Some(network_name) = self.navigation_history.navigate_forward() {
      self.active_node_network_name = network_name;
      self.mark_full_refresh();
      let camera_settings = self.get_active_node_network()
        .and_then(|n| n.camera_settings.clone());
      (true, camera_settings)
    } else {
      (false, None)
    }
  }

  /// Checks if we can navigate backward in network history
  pub fn can_navigate_back(&self) -> bool {
    self.navigation_history.can_navigate_back()
  }

  /// Checks if we can navigate forward in network history
  pub fn can_navigate_forward(&self) -> bool {
    self.navigation_history.can_navigate_forward()
  }
}

impl StructureDesigner {
  pub fn set_node_display(&mut self, node_id: u64, is_displayed: bool) {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      network.set_node_display(node_id, is_displayed);
      // Track that this node's visibility changed
      self.pending_changes.visibility_changed.insert(node_id);
    }
  }

  pub fn sync_gadget_data(&mut self) -> bool {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return false,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      if let Some(node_id) = &network.active_node_id {
        let data = network.get_node_network_data_mut(*node_id);
        if let Some(node_data) = data {
          if let Some(g) = &self.gadget {
            g.sync_data(node_data);
            // Mark design as dirty since gadget data was synced back to node
            self.set_dirty(true);
          }
        }
      }
      true
    } else {
      false
    }
  }

  pub fn select_node(&mut self, node_id: u64) -> bool {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return false,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      // Get the previously active node ID before changing selection
      let previously_active_node_id = network.active_node_id;
      
      // Update the selection
      let ret = network.select_node(node_id);
      
      // If the selection was successful, update the display policy
      if ret {
        // Track selection change
        let current_selection = Some(node_id);
        self.mark_selection_changed(previously_active_node_id, current_selection);
        
        // Create a HashSet with the previous and newly selected node IDs
        let mut dirty_nodes = HashSet::new();
        dirty_nodes.insert(node_id); // New selection
        
        // Add previously active node to dirty nodes if it existed
        if let Some(prev_id) = previously_active_node_id {
          dirty_nodes.insert(prev_id);
        }
        
        // Apply display policy considering these nodes as dirty
        self.apply_node_display_policy(Some(&dirty_nodes));
      }
      
      ret
    } else {
      false
    }
  }

  pub fn select_wire(&mut self, source_node_id: u64, source_output_pin_index: i32, destination_node_id: u64, destination_argument_index: usize) -> bool {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return false,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      // Get the previously active node ID before changing selection
      let previously_active_node_id = network.active_node_id;
      
      // Update the selection
      let ret = network.select_wire(source_node_id, source_output_pin_index, destination_node_id, destination_argument_index);
      
      // If the selection was successful
      if ret {
        // Track selection change (wire selection clears node selection)
        self.mark_selection_changed(previously_active_node_id, None);
        
        // If there was a previously active node, update display policy
        if let Some(prev_id) = previously_active_node_id {
          // Create a HashSet with just the previously active node ID
          let mut dirty_nodes = HashSet::new();
          dirty_nodes.insert(prev_id);
          
          // Apply display policy considering only the previously active node as dirty
          self.apply_node_display_policy(Some(&dirty_nodes));
        }
      }
      
      ret
    } else {
      false
    }
  }
  
  pub fn clear_selection(&mut self) {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      // Get the previously active node ID before clearing selection
      let previously_active_node_id = network.active_node_id;
      
      // Clear the selection
      network.clear_selection();

      // Track selection change
      self.mark_selection_changed(previously_active_node_id, None);

      // If there was a previously active node
      if let Some(prev_id) = previously_active_node_id {
        // Create a HashSet with just the previously active node ID
        let mut dirty_nodes = HashSet::new();
        dirty_nodes.insert(prev_id);
        
        // Apply display policy considering only the previously active node as dirty
        self.apply_node_display_policy(Some(&dirty_nodes));
      }
    }
  }

  /// Toggle node in selection (for Ctrl+click)
  pub fn toggle_node_selection(&mut self, node_id: u64) -> bool {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return false,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let previously_active_node_id = network.active_node_id;
      let ret = network.toggle_node_selection(node_id);
      if ret {
        let current_selection = network.active_node_id;
        self.mark_selection_changed(previously_active_node_id, current_selection);
        let mut dirty_nodes = HashSet::new();
        dirty_nodes.insert(node_id);
        if let Some(prev_id) = previously_active_node_id {
          dirty_nodes.insert(prev_id);
        }
        self.apply_node_display_policy(Some(&dirty_nodes));
      }
      ret
    } else {
      false
    }
  }

  /// Add node to selection (for Shift+click)
  pub fn add_node_to_selection(&mut self, node_id: u64) -> bool {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return false,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let previously_active_node_id = network.active_node_id;
      let ret = network.add_node_to_selection(node_id);
      if ret {
        self.mark_selection_changed(previously_active_node_id, Some(node_id));
        let mut dirty_nodes = HashSet::new();
        dirty_nodes.insert(node_id);
        if let Some(prev_id) = previously_active_node_id {
          dirty_nodes.insert(prev_id);
        }
        self.apply_node_display_policy(Some(&dirty_nodes));
      }
      ret
    } else {
      false
    }
  }

  /// Select multiple nodes (for rectangle selection)
  pub fn select_nodes(&mut self, node_ids: Vec<u64>) -> bool {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return false,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let previously_active_node_id = network.active_node_id;
      let ret = network.select_nodes(node_ids.clone());
      if ret {
        let current_selection = network.active_node_id;
        self.mark_selection_changed(previously_active_node_id, current_selection);
        let mut dirty_nodes: HashSet<u64> = node_ids.into_iter().collect();
        if let Some(prev_id) = previously_active_node_id {
          dirty_nodes.insert(prev_id);
        }
        self.apply_node_display_policy(Some(&dirty_nodes));
      }
      ret
    } else {
      false
    }
  }

  /// Toggle multiple nodes in selection (for Ctrl+rectangle)
  pub fn toggle_nodes_selection(&mut self, node_ids: Vec<u64>) {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let previously_active_node_id = network.active_node_id;
      network.toggle_nodes_selection(node_ids.clone());
      let current_selection = network.active_node_id;
      self.mark_selection_changed(previously_active_node_id, current_selection);
      let mut dirty_nodes: HashSet<u64> = node_ids.into_iter().collect();
      if let Some(prev_id) = previously_active_node_id {
        dirty_nodes.insert(prev_id);
      }
      self.apply_node_display_policy(Some(&dirty_nodes));
    }
  }

  /// Add multiple nodes to selection (for Shift+rectangle)
  pub fn add_nodes_to_selection(&mut self, node_ids: Vec<u64>) {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let previously_active_node_id = network.active_node_id;
      network.add_nodes_to_selection(node_ids.clone());
      let current_selection = network.active_node_id;
      self.mark_selection_changed(previously_active_node_id, current_selection);
      let mut dirty_nodes: HashSet<u64> = node_ids.into_iter().collect();
      if let Some(prev_id) = previously_active_node_id {
        dirty_nodes.insert(prev_id);
      }
      self.apply_node_display_policy(Some(&dirty_nodes));
    }
  }

  /// Get all selected node IDs
  pub fn get_selected_node_ids(&self) -> Vec<u64> {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return Vec::new(),
    };
    if let Some(network) = self.node_type_registry.node_networks.get(network_name) {
      network.get_selected_node_ids().iter().copied().collect()
    } else {
      Vec::new()
    }
  }

  /// Move all selected nodes by delta
  pub fn move_selected_nodes(&mut self, delta: glam::f64::DVec2) {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      network.move_selected_nodes(delta);
    }
  }

  /// Toggle wire in selection (for Ctrl+click)
  pub fn toggle_wire_selection(&mut self, source_node_id: u64, source_output_pin_index: i32, destination_node_id: u64, destination_argument_index: usize) -> bool {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return false,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let previously_active_node_id = network.active_node_id;
      let ret = network.toggle_wire_selection(source_node_id, source_output_pin_index, destination_node_id, destination_argument_index);
      if ret {
        self.mark_selection_changed(previously_active_node_id, None);
        if let Some(prev_id) = previously_active_node_id {
          let mut dirty_nodes = HashSet::new();
          dirty_nodes.insert(prev_id);
          self.apply_node_display_policy(Some(&dirty_nodes));
        }
      }
      ret
    } else {
      false
    }
  }

  /// Add wire to selection (for Shift+click)
  pub fn add_wire_to_selection(&mut self, source_node_id: u64, source_output_pin_index: i32, destination_node_id: u64, destination_argument_index: usize) -> bool {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return false,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let previously_active_node_id = network.active_node_id;
      let ret = network.add_wire_to_selection(source_node_id, source_output_pin_index, destination_node_id, destination_argument_index);
      if ret {
        self.mark_selection_changed(previously_active_node_id, None);
        if let Some(prev_id) = previously_active_node_id {
          let mut dirty_nodes = HashSet::new();
          dirty_nodes.insert(prev_id);
          self.apply_node_display_policy(Some(&dirty_nodes));
        }
      }
      ret
    } else {
      false
    }
  }

  /// Get all selected wires
  pub fn get_selected_wires(&self) -> Vec<crate::structure_designer::node_network::Wire> {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return Vec::new(),
    };
    if let Some(network) = self.node_type_registry.node_networks.get(network_name) {
      network.get_selected_wires().clone()
    } else {
      Vec::new()
    }
  }

  /// Select multiple wires (replaces current selection)
  pub fn select_wires(&mut self, wires: Vec<crate::structure_designer::node_network::Wire>) {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let previously_active_node_id = network.active_node_id;
      network.select_wires(wires);
      self.mark_selection_changed(previously_active_node_id, None);
      if let Some(prev_id) = previously_active_node_id {
        let mut dirty_nodes = HashSet::new();
        dirty_nodes.insert(prev_id);
        self.apply_node_display_policy(Some(&dirty_nodes));
      }
    }
  }

  /// Add multiple wires to selection (for Shift+rectangle)
  pub fn add_wires_to_selection(&mut self, wires: Vec<crate::structure_designer::node_network::Wire>) {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let previously_active_node_id = network.active_node_id;
      network.add_wires_to_selection(wires);
      self.mark_selection_changed(previously_active_node_id, None);
      if let Some(prev_id) = previously_active_node_id {
        let mut dirty_nodes = HashSet::new();
        dirty_nodes.insert(prev_id);
        self.apply_node_display_policy(Some(&dirty_nodes));
      }
    }
  }

  /// Toggle multiple wires in selection (for Ctrl+rectangle)
  pub fn toggle_wires_selection(&mut self, wires: Vec<crate::structure_designer::node_network::Wire>) {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let previously_active_node_id = network.active_node_id;
      network.toggle_wires_selection(wires);
      self.mark_selection_changed(previously_active_node_id, None);
      if let Some(prev_id) = previously_active_node_id {
        let mut dirty_nodes = HashSet::new();
        dirty_nodes.insert(prev_id);
        self.apply_node_display_policy(Some(&dirty_nodes));
      }
    }
  }

  /// Select nodes and wires together (for rectangle selection)
  pub fn select_nodes_and_wires(&mut self, node_ids: Vec<u64>, wires: Vec<crate::structure_designer::node_network::Wire>) {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let previously_active_node_id = network.active_node_id;
      network.select_nodes_and_wires(node_ids.clone(), wires);
      let current_selection = network.active_node_id;
      self.mark_selection_changed(previously_active_node_id, current_selection);
      let mut dirty_nodes: HashSet<u64> = node_ids.into_iter().collect();
      if let Some(prev_id) = previously_active_node_id {
        dirty_nodes.insert(prev_id);
      }
      self.apply_node_display_policy(Some(&dirty_nodes));
    }
  }

  /// Add nodes and wires to existing selection (for Shift+rectangle)
  pub fn add_nodes_and_wires_to_selection(&mut self, node_ids: Vec<u64>, wires: Vec<crate::structure_designer::node_network::Wire>) {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let previously_active_node_id = network.active_node_id;
      network.add_nodes_and_wires_to_selection(node_ids.clone(), wires);
      let current_selection = network.active_node_id;
      self.mark_selection_changed(previously_active_node_id, current_selection);
      let mut dirty_nodes: HashSet<u64> = node_ids.into_iter().collect();
      if let Some(prev_id) = previously_active_node_id {
        dirty_nodes.insert(prev_id);
      }
      self.apply_node_display_policy(Some(&dirty_nodes));
    }
  }

  /// Toggle nodes and wires in selection (for Ctrl+rectangle)
  pub fn toggle_nodes_and_wires_selection(&mut self, node_ids: Vec<u64>, wires: Vec<crate::structure_designer::node_network::Wire>) {
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let previously_active_node_id = network.active_node_id;
      network.toggle_nodes_and_wires_selection(node_ids.clone(), wires);
      let current_selection = network.active_node_id;
      self.mark_selection_changed(previously_active_node_id, current_selection);
      let mut dirty_nodes: HashSet<u64> = node_ids.into_iter().collect();
      if let Some(prev_id) = previously_active_node_id {
        dirty_nodes.insert(prev_id);
      }
      self.apply_node_display_policy(Some(&dirty_nodes));
    }
  }

  pub fn delete_selected(&mut self) {
    // Early return if active_node_network_name is None
    let node_network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    
    // Collect nodes that will need to be marked as dirty after deletion
    let mut dirty_nodes = HashSet::new();
    let mut should_validate = false;
    
    if let Some(node_network) = self.node_type_registry.node_networks.get(node_network_name) {
      // If nodes are selected, all connected nodes will be dirty
      if !node_network.selected_node_ids.is_empty() {
        for &selected_node_id in &node_network.selected_node_ids {
          // Get all nodes connected to the selected node
          dirty_nodes.extend(node_network.get_connected_node_ids(selected_node_id));
          
          // Check if the selected node requires validation
          if let Some(node) = node_network.nodes.get(&selected_node_id) {
            if node.node_type_name == "parameter" || {
              // Check if this node references an invalid node network
              self.node_type_registry.node_networks.get(&node.node_type_name)
                .map(|network| !network.valid)
                .unwrap_or(false)
            } {
              should_validate = true;
            }
          }
        }
      } 
      // If wires are selected, both source and destination nodes will be dirty
      else if !node_network.selected_wires.is_empty() {
        for wire in &node_network.selected_wires {
          dirty_nodes.insert(wire.source_node_id);
          dirty_nodes.insert(wire.destination_node_id);
        }
      }
    }
    
    // Perform the deletion
    if let Some(node_network) = self.node_type_registry.node_networks.get_mut(node_network_name) {
      node_network.delete_selected();
      // Mark design as dirty since we deleted something
      self.set_dirty(true);
      // TODO: we do a full refresh for now,
      // but this can be a partial refresh with marking data changes
      // in all nodes wired to the output node of the deleted node.
      self.mark_full_refresh();
    }
    
    // Only apply display policy if there were dirty nodes
    if !dirty_nodes.is_empty() {
      self.apply_node_display_policy(Some(&dirty_nodes));
    }
    
    // Validate if we deleted a parameter node or invalid network node
    if should_validate {
      self.validate_active_network();
    }
  }

  // -------------------------------------------------------------------------------------------------------------------------
  // --- Raytracing methods                                                                                              ---
  // -------------------------------------------------------------------------------------------------------------------------
  
  /// Traces a ray into the current scene, checking both atomic structures and implicit geometry
  /// 
  /// # Arguments
  /// 
  /// * `ray_origin` - The origin point of the ray
  /// * `ray_direction` - The direction vector of the ray (does not need to be normalized)
  /// * `visualization` - The visualization mode to use for hit testing
  /// 
  /// # Returns
  /// 
  /// The distance to the closest intersection, or None if no intersection was found
  pub fn raytrace(&self, ray_origin: &DVec3, ray_direction: &DVec3, visualization: &AtomicStructureVisualization) -> Option<f64> {
    let mut min_distance: Option<f64> = None;
    let display_visualization = match visualization {
      AtomicStructureVisualization::BallAndStick => crate::display::preferences::AtomicStructureVisualization::BallAndStick,
      AtomicStructureVisualization::SpaceFilling => crate::display::preferences::AtomicStructureVisualization::SpaceFilling,
    };
    
    use crate::structure_designer::structure_designer_scene::NodeOutput;
    // First, check all atomic structures in the scene
    for node_data in self.last_generated_structure_designer_scene.node_data.values() {
      if let NodeOutput::Atomic(atomic_structure) = &node_data.output {
        match atomic_structure.hit_test(ray_origin, ray_direction, visualization, 
          |atom| get_displayed_atom_radius(atom, &display_visualization), BAS_STICK_RADIUS) {
          crate::crystolecule::atomic_structure::HitTestResult::Atom(_, distance) | 
          crate::crystolecule::atomic_structure::HitTestResult::Bond(_, distance) => {
          // Update minimum distance if this hit is closer
          min_distance = match min_distance {
            None => Some(distance),
            Some(current_min) if distance < current_min => Some(distance),
            _ => min_distance,
          };
        },
        crate::crystolecule::atomic_structure::HitTestResult::None => {}
        }
      }
    }
    
    // Collect all geo_trees from node_data
    let geometries: Vec<&dyn ImplicitGeometry3D> = self.last_generated_structure_designer_scene.node_data
      .values()
      .filter_map(|node_data| node_data.geo_tree.as_ref())
      .map(|geo_node| geo_node as &dyn ImplicitGeometry3D)
      .collect();
  
    // Raytrace the implicit geometries using the world scale
    if let Some(geo_distance) = raytrace_geometries(
      &geometries, 
      ray_origin, 
      ray_direction, 
      1.0
    ) {
      // Update minimum distance if this hit is closer
      min_distance = match min_distance {
        None => Some(geo_distance),
        Some(current_min) if geo_distance < current_min => Some(geo_distance),
        _ => min_distance,
      };
    }
  
    //println!("raytrace min_distance: {:?}", min_distance);

    min_distance
  }
  
  // -------------------------------------------------------------------------------------------------------------------------
  // --- Preferences management                                                                                         ---
  // -------------------------------------------------------------------------------------------------------------------------

  /// Applies the node display policy to the active node network
  /// 
  /// This will resolve the display policy using the current preferences and apply 
  /// the changes to the node network. If dirty_node_ids is None, all nodes will be considered dirty.
  /// 
  /// # Parameters
  /// * `dirty_node_ids` - The set of node IDs that are dirty, or None to consider all nodes dirty
  pub fn apply_node_display_policy(&mut self, dirty_node_ids: Option<&HashSet<u64>>) {
    // Only apply if there's an active node network
    if let Some(network_name) = &self.active_node_network_name {
      if let Some(node_network) = self.node_type_registry.node_networks.get_mut(network_name) {
        // Resolve the display policy with the provided dirty_node_ids
        let changes = self.node_display_policy_resolver.resolve(
          node_network,
          &self.preferences.node_display_preferences,
          dirty_node_ids
        );
        
        // Track visibility changes
        for node_id in changes.keys() {
          self.pending_changes.visibility_changed.insert(*node_id);
        }
        
        // Apply the changes to the node network
        for (node_id, display_type) in changes {
          node_network.set_node_display_type(node_id, display_type);
        }
      }
    }
  }

  /// Sets the preferences for the structure designer and applies necessary updates
  pub fn set_preferences(&mut self, preferences: StructureDesignerPreferences) {
    // Check if node display preferences have changed
    let node_display_prefs_changed = self.preferences.node_display_preferences != preferences.node_display_preferences;
    
    // Update the preferences
    self.preferences = preferences;
    
    // If node display preferences have changed, reapply the node display policy
    if node_display_prefs_changed {
      self.apply_node_display_policy(None);
      // Preference changes require full refresh
      self.mark_full_refresh();
    }
  }

  // -------------------------------------------------------------------------------------------------------------------------
  // --- Gadget delegation methods                                                                                        ---
  // -------------------------------------------------------------------------------------------------------------------------

  pub fn gadget_hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
    if let Some(gadget) = &self.gadget {
      return gadget.hit_test(ray_origin, ray_direction);
    }
    None
  }

  pub fn gadget_start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
    if let Some(gadget) = &mut self.gadget {
      gadget.start_drag(handle_index, ray_origin, ray_direction);
    }
    self.mark_lightweight_refresh();
  }

  pub fn gadget_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
    if let Some(gadget) = &mut self.gadget {
      gadget.drag(handle_index, ray_origin, ray_direction);
    }
    // Gadget dragging only needs lightweight refresh (tessellation update)
    self.mark_lightweight_refresh();
  }

  pub fn gadget_end_drag(&mut self) {
    if let Some(gadget) = &mut self.gadget {
      gadget.end_drag();
      self.sync_gadget_data();
      // Ending drag syncs data back to the node
      if let Some(network_name) = &self.active_node_network_name.clone() {
        if let Some(network) = self.node_type_registry.node_networks.get(network_name) {
          if let Some(node_id) = network.active_node_id {
            self.mark_node_data_changed(node_id);
          }
        }
      }
    }
  }

  /// Sets a node as the return node for the active network.
  /// 
  /// # Parameters
  /// * `node_id` - The ID of the node to set as the return node, or None to clear the return node
  /// 
  /// # Returns
  /// Returns true if the operation was successful, false otherwise.
  pub fn set_return_node_id(&mut self, node_id: Option<u64>) -> bool {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return false,
    };
    
    // If node_id is None, clear the return node
    if node_id.is_none() {
      if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
        network.return_node_id = None;
        // Mark design as dirty since we changed the return node
        self.set_dirty(true);
        self.validate_active_network();
        return true;
      }
      return false;
    }

    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let ret = network.set_return_node(node_id.unwrap());
      if ret {
        // Mark design as dirty since we set the return node
        self.set_dirty(true);
      }
      self.validate_active_network();
      ret
    } else {
      false
    }
  }

  // Saves node networks to a file (Save As functionality)
  pub fn save_node_networks_as(&mut self, file_path: &str) -> std::io::Result<()> {
    use std::path::Path;
    let result = node_networks_serialization::save_node_networks_to_file(&mut self.node_type_registry, Path::new(file_path));
    
    // Clear dirty flag and set file path if save was successful
    if result.is_ok() {
      self.is_dirty = false;
      self.file_path = Some(file_path.to_string());
    }
    
    result
  }

  // Saves node networks to the current file (Save functionality)
  pub fn save_node_networks(&mut self) -> Option<std::io::Result<()>> {
    match &self.file_path {
      Some(file_path) => {
        let file_path = file_path.clone(); // Clone to avoid borrow issues
        Some(self.save_node_networks_as(&file_path))
      }
      None => None, // No file path available
    }
  }

  /// Imports selected node networks from the loaded import library
  /// 
  /// This is a wrapper around the import manager that adds business logic
  /// such as marking the design as dirty and applying display policies.
  /// 
  /// # Arguments
  /// * `network_names` - List of network names to import
  /// * `name_prefix` - Optional prefix to prepend to imported network names
  /// 
  /// # Returns
  /// * `Ok(())` if all networks were imported successfully
  /// * `Err(String)` with error message if import failed
  pub fn import_networks(&mut self, network_names: &[String], name_prefix: Option<&str>) -> Result<(), String> {
    let result = self.import_manager.import_networks_and_clear(
      network_names,
      &mut self.node_type_registry,
      name_prefix
    );
    
    if result.is_ok() {
      // Mark as dirty since we modified the design
      self.is_dirty = true;
      
      // Apply display policy to newly imported networks
      self.apply_node_display_policy(None);
      
      // Importing networks is a structural change requiring full refresh
      self.mark_full_refresh();
    }
    
    result
  }

  /// Loads node networks from a file and returns the camera settings of the active network (if any).
  /// Sets the active_node_network_name to the first network if available, otherwise None.
  pub fn load_node_networks(&mut self, file_path: &str) -> std::io::Result<Option<CameraSettings>> {

    let first_network_name = node_networks_serialization::load_node_networks_from_file(
      &mut self.node_type_registry,
      file_path
    )?;

    // Validate all networks in dependency order (dependencies first)
    // This ensures call sites can be repaired before validating their parent networks
    let networks_in_order = self.node_type_registry.get_networks_in_dependency_order();
    for network_name in networks_in_order {
      // Split borrows: use raw pointer access to avoid double mutable borrow
      // This is safe because validate_network only mutates the current network and the registry,
      // and we're iterating one network at a time
      let registry_ptr = &mut self.node_type_registry as *mut NodeTypeRegistry;
      unsafe {
        if let Some(network) = (*registry_ptr).node_networks.get_mut(&network_name) {
          validate_network(network, &mut *registry_ptr, None);
        }
      }
    }

    // Clear navigation history since we're loading a new design file
    self.navigation_history.clear();

    // Set active node network to the first network if available, otherwise None
    // Capture camera settings from the newly active network
    let camera_settings = if first_network_name.is_empty() {
      self.set_active_node_network_name(None)
    } else {
      self.set_active_node_network_name(Some(first_network_name))
    };

    // Apply display policy to all nodes
    self.apply_node_display_policy(None);

    // Clear CSG conversion cache since we loaded a completely new file
    self.network_evaluator.clear_csg_cache();

    // Clear dirty flag since we just loaded a saved state
    self.is_dirty = false;

    // Set the file path since we just loaded from this file
    self.file_path = Some(file_path.to_string());

    // Loading networks is a structural change requiring full refresh
    self.mark_full_refresh();

    Ok(camera_settings)
  }

  /// Validates the active node network and propagates validation to dependent networks
  /// 
  /// This method implements dependency invalidation propagation:
  /// - When a network becomes valid, invalid parent networks need revalidation
  /// - When a network becomes invalid, valid parent networks need revalidation
  /// - Continues until no more networks need validation
  pub fn validate_active_network(&mut self) -> Option<NetworkValidationResult> {
    self.validate_active_network_with_initial_errors(None)
  }

  /// Validates the active network with optional initial validation errors (e.g., from expr nodes)
  fn validate_active_network_with_initial_errors(&mut self, initial_errors: Option<Vec<crate::structure_designer::node_network::ValidationError>>) -> Option<NetworkValidationResult> {
    // Get the active network name
    let network_name = match &self.active_node_network_name {
      Some(name) => name.clone(),
      None => return None,
    };
    
    // Initialize the set of networks to validate
    let mut to_validate = HashSet::new();
    to_validate.insert(network_name.clone());
    
    let mut final_result = None;
    
    // Process networks until the set is empty
    while let Some(current_network_name) = to_validate.iter().next().cloned() {
      to_validate.remove(&current_network_name);
      
      // Get the current validation state before validation
      let was_valid = self.node_type_registry.node_networks
        .get(&current_network_name)
        .map(|network| network.valid)
        .unwrap_or(false);
      
      // Validate the current network
      let validation_result = {
        // Check if network exists first
        if !self.node_type_registry.node_networks.contains_key(&current_network_name) {
          continue; // Skip if network doesn't exist
        }
        
        // Extract the network temporarily to avoid borrowing conflicts
        let mut network = self.node_type_registry.node_networks.remove(&current_network_name).unwrap();
        
        // Use initial errors only for the originally requested network
        let errors_to_use = if current_network_name == network_name {
          initial_errors.clone()
        } else {
          None
        };
        
        // Validate with the registry and initial errors
        let result = validate_network(&mut network, &mut self.node_type_registry, errors_to_use);
        
        // Put the network back
        self.node_type_registry.node_networks.insert(current_network_name.clone(), network);
        
        result
      };
      
      // Store the result if this is the originally requested network
      if current_network_name == network_name {
        final_result = Some(validation_result.clone());
      }
      
      // Check if validation state changed OR interface changed
      let is_now_valid = validation_result.valid;
      let interface_changed = validation_result.interface_changed;
      
      if was_valid != is_now_valid || interface_changed {
        // Find all parent networks that use this network as a node
        let parent_networks = self.node_type_registry.find_parent_networks(&current_network_name);
        
        for parent_name in parent_networks {
          if interface_changed {
            // If interface changed, validate ALL parent networks regardless of their current state
            to_validate.insert(parent_name);
          } else if let Some(parent_network) = self.node_type_registry.node_networks.get(&parent_name) {
            // If only validity changed, add parent networks based on validity logic:
            // - Parent is invalid and child became valid (parent might become valid)
            // - Parent is valid and child became invalid (parent might become invalid)
            if (!parent_network.valid && is_now_valid) || (parent_network.valid && !is_now_valid) {
              to_validate.insert(parent_name);
            }
          }
        }
      }
    }
    
    final_result
  }

  /// Evaluate a specific node and return its result for CLI inspection.
  ///
  /// This triggers evaluation of the node (if not already cached) and returns
  /// the NetworkResult converted to strings for display.
  ///
  /// # Arguments
  /// * `node_id` - The ID of the node to evaluate
  /// * `verbose` - If true, include detailed output for complex types
  ///
  /// # Returns
  /// * `Ok(APINodeEvaluationResult)` - The evaluation result
  /// * `Err(String)` - If node not found or network not active
  pub fn evaluate_node_for_cli(
    &mut self,
    node_id: u64,
    verbose: bool,
  ) -> Result<APINodeEvaluationResult, String> {
    // Check that an active network is set
    let network_name = self.active_node_network_name.as_ref()
      .ok_or_else(|| "No active node network".to_string())?
      .clone();

    // Get the network and verify the node exists
    let network = self.node_type_registry.node_networks.get(&network_name)
      .ok_or_else(|| format!("Network '{}' not found", network_name))?;

    // Check if the network is valid
    if !network.valid {
      return Err(format!("Network '{}' is invalid and cannot be evaluated", network_name));
    }

    // Look up the node
    let node = network.nodes.get(&node_id)
      .ok_or_else(|| format!("Node {} not found in network '{}'", node_id, network_name))?;

    // Get the node type name and custom name
    let node_type_name = node.node_type_name.clone();
    let custom_name = node.custom_name.clone();

    // Get the output type from the node type registry
    let output_type = self.node_type_registry.get_node_type_for_node(node)
      .map(|nt| nt.output_type.to_string())
      .unwrap_or_else(|| "Unknown".to_string());

    // Set up evaluation context
    let mut context = NetworkEvaluationContext::new();
    if let Some(params) = self.cli_top_level_parameters.clone() {
      context.top_level_parameters = params;
    }

    // Create the network stack
    let network = self.node_type_registry.node_networks.get(&network_name).unwrap();
    let network_stack = vec![NetworkStackElement { node_network: network, node_id: 0 }];

    // Evaluate the node (output pin 0 is the main output)
    let result = self.network_evaluator.evaluate(
      &network_stack,
      node_id,
      0,  // output pin index
      &self.node_type_registry,
      false,  // decorate - false since this is just for text output
      &mut context
    );

    // Build the response
    let display_string = result.to_display_string();
    let detailed_string = if verbose {
      Some(result.to_detailed_string())
    } else {
      None
    };

    // Check for errors
    let (success, error_message) = match &result {
      NetworkResult::Error(msg) => (false, Some(msg.clone())),
      _ => (true, None),
    };

    Ok(APINodeEvaluationResult {
      node_id,
      node_type_name,
      custom_name,
      output_type,
      display_string,
      detailed_string,
      success,
      error_message,
    })
  }

  /// Find a node ID by its display name in the active network.
  ///
  /// Since all nodes have persistent names assigned at creation,
  /// this is a simple search through the custom_name fields.
  ///
  /// # Arguments
  /// * `name` - The name to search for
  ///
  /// # Returns
  /// * `Some(node_id)` if a node with the given name exists
  /// * `None` if no node with the given name is found or no network is active
  pub fn find_node_id_by_name(&self, name: &str) -> Option<u64> {
    let network_name = self.active_node_network_name.as_ref()?;
    let network = self.node_type_registry.node_networks.get(network_name)?;

    for (node_id, node) in &network.nodes {
      if node.custom_name.as_deref() == Some(name) {
        return Some(*node_id);
      }
    }

    None
  }

  /// Exports all visible atomic structures as a single file (XYZ or MOL format)
  /// Merges all atomic structures from the last generated scene into one structure before saving
  /// File format is determined by the file extension (.xyz or .mol)
  pub fn export_visible_atomic_structures(&self, file_path: &str) -> Result<(), String> {
    use crate::structure_designer::structure_designer_scene::NodeOutput;
    
    // Create a new merged atomic structure
    let mut merged_structure = AtomicStructure::new();
    let mut has_structures = false;

    // Merge all atomic structures from node_data into one
    for node_data in self.last_generated_structure_designer_scene.node_data.values() {
      if let NodeOutput::Atomic(atomic_structure) = &node_data.output {
        merged_structure.add_atomic_structure(atomic_structure);
        has_structures = true;
      }
    }
    
    // Check if we have any atomic structures to export
    if !has_structures {
      return Err("No atomic structures available to export".to_string());
    }

    // Check if the merged structure has any atoms
    if merged_structure.get_num_of_atoms() == 0 {
      return Err("No atoms found in the atomic structures to export".to_string());
    }

    // Determine file format from extension and save accordingly
    let file_path_lower = file_path.to_lowercase();
    if file_path_lower.ends_with(".xyz") {
      match save_xyz(&merged_structure, file_path) {
        Ok(()) => Ok(()),
        Err(err) => Err(format!("Failed to save XYZ file '{}': {}", file_path, err)),
      }
    } else if file_path_lower.ends_with(".mol") {
      match save_mol_v3000(&merged_structure, file_path) {
        Ok(()) => Ok(()),
        Err(err) => Err(format!("Failed to save MOL file '{}': {}", file_path, err)),
      }
    } else {
      Err(format!("Unsupported file format. Please use .xyz or .mol extension. Got: {}", file_path))
    }
  }
  
}
