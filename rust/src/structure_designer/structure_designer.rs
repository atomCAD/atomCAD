use crate::common::atomic_structure::AtomicStructure;
use crate::common::atomic_structure_utils::calc_selection_transform;
use glam::f64::DVec3;
use glam::f64::DVec2;
use super::node_type_registry::NodeTypeRegistry;
use super::node_network::NodeNetwork;
use crate::api::structure_designer::structure_designer_api_types::APIDataType;
use super::node_type::NodeType;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_data::NoData;
use super::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::structure_designer_scene::StructureDesignerScene;
use super::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::serialization::node_networks_serialization;
use crate::structure_designer::nodes::edit_atom::edit_atom::get_selected_edit_atom_data_mut;
use crate::api::structure_designer::structure_designer_preferences::StructureDesignerPreferences;
use super::node_display_policy_resolver::NodeDisplayPolicyResolver;
use super::network_validator::{validate_network, NetworkValidationResult};
use std::collections::HashSet;
use crate::structure_designer::implicit_eval::ray_tracing::raytrace_geometries;
use crate::structure_designer::implicit_eval::implicit_geometry::ImplicitGeometry3D;
use crate::structure_designer::common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;

pub struct StructureDesigner {
  pub node_type_registry: NodeTypeRegistry,
  pub network_evaluator: NetworkEvaluator,
  pub gadget: Option<Box<dyn NodeNetworkGadget>>,
  pub active_node_network_name: Option<String>,
  pub last_generated_structure_designer_scene: StructureDesignerScene,
  pub preferences: StructureDesignerPreferences,
  pub node_display_policy_resolver: NodeDisplayPolicyResolver,
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
    }
  }
}

impl StructureDesigner {

  // Returns the first atomic structure generated from a selected node, if any
  pub fn get_atomic_structure_from_selected_node(&self) -> Option<&AtomicStructure> {
    // Find the first atomic structure with from_selected_node = true
    self.last_generated_structure_designer_scene.atomic_structures.iter()
      .find(|structure| structure.from_selected_node)
  }

  /// Helper method to get the selected node ID of a node of a specific type
  /// 
  /// Returns None if:
  /// - There is no active node network
  /// - No node is selected in the active network
  /// - The selected node has a different type name than the needed node type name
  pub fn get_selected_node_id_with_type(&self,  needed_node_type_name: &str) -> Option<u64> {
    // Get active node network name
    let network_name = self.active_node_network_name.as_ref()?;
    
    // Get the active node network
    let network = self.node_type_registry.node_networks.get(network_name)?;
    
    // Get the selected node ID
    let selected_node_id = network.selected_node_id?;
    
    // Get the selected node's type name
    let node_type_name = network.nodes.get(&selected_node_id)?.node_type_name.as_str();
    
    // Check if the node is with the needed node type name
    if node_type_name != needed_node_type_name {
      return None;
    }

    Some(selected_node_id)
  }

  // Returns true if the selected node is displayed and has the needed node type name
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
    
    // Check if there's a selected node ID
    let selected_node_id = match network.selected_node_id {
      Some(id) => id,
      None => return false,
    };
    
    // Check if the selected node is displayed
    if !network.is_node_displayed(selected_node_id) {
      return false;
    }
    
    // Get the selected node's type name
    let node_type_name = match network.nodes.get(&selected_node_id) {
      Some(node) => &node.node_type_name,
      None => return false,
    };
    
    // Return true only if the node's type name matches the needed node type name
    node_type_name == needed_node_type_name
  }

  // Generates the scene to be rendered according to the displayed nodes of the active node network
  pub fn refresh(&mut self, lightweight: bool) {

    // Check if node_network_name exists and clone it to avoid borrow conflicts
    let node_network_name = match &self.active_node_network_name {
      Some(name) => name.clone(),
      None => return, // Return if node_network_name is None
    };

    if !lightweight {
      let network = match self.node_type_registry.node_networks.get(&node_network_name) {
        Some(network) => network,
        None => return,
      };
      self.last_generated_structure_designer_scene = StructureDesignerScene::new();
      for node_entry in &network.displayed_node_ids {
        self.last_generated_structure_designer_scene.merge(self.network_evaluator.generate_scene(
          &node_network_name,
          *node_entry.0,
          *node_entry.1,
          &self.node_type_registry,
          &self.preferences.geometry_visualization_preferences,
        ));
      }
    }

    self.refresh_scene_dependent_node_data();
    // Recreates the gadget if this in not a lightweight refresh
    // in case a lightweight refresh the gasget is in action and should not be recreated.
    if !lightweight {
      // Use immutable access to avoid borrow conflicts with provide_gadget
      if let Some(network) = self.node_type_registry.node_networks.get(&node_network_name) {
        self.gadget = network.provide_gadget(&self);
      }
    }

    if let Some(gadget) = &self.gadget {
      self.last_generated_structure_designer_scene.tessellatable = Some(gadget.as_tessellatable());
    }
  }    

  // node network methods

  pub fn add_new_node_network(&mut self) {
    // Generate a unique name for the new node network
    let mut name = "untitled".to_string();
    let mut i = 1;
    while self.node_type_registry.node_networks.contains_key(&name) {
      name = format!("untitled{}", i);
      i += 1;
    }
    self.add_node_network(&name);
  }
  
  pub fn add_node_network(&mut self, node_network_name: &str) {
    self.node_type_registry.add_node_network(NodeNetwork::new(
      NodeType {
        name: node_network_name.to_string(),
        parameters: Vec::new(),
        output_type: APIDataType::None,
        node_data_creator: || Box::new(NoData {}),
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

    true
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
      if let Some(node_network) = self.node_type_registry.node_networks.get(node_network_name) {
        let current_param_count = node_network.node_type.parameters.len();
        
        // Downcast to ParameterData and set properties
        if let Some(param_data) = node_data.as_any_mut().downcast_mut::<crate::structure_designer::nodes::parameter::ParameterData>() {
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
    if node_id != 0 && (node_type_name == "parameter" || node_type_name == "expr") {
      // Split the borrow to avoid conflicts
      let (built_in_types, node_networks) = (&self.node_type_registry.built_in_node_types, &mut self.node_type_registry.node_networks);
      if let Some(network) = node_networks.get_mut(node_network_name) {
        if let Some(node) = network.nodes.get_mut(&node_id) {
          // Call the populate function with the split borrows
          crate::structure_designer::node_type_registry::NodeTypeRegistry::populate_custom_node_type_cache_with_types(built_in_types, node);
        }
      }
    }
    
    // If we successfully added a node, apply the display policy with this node as dirty
    if node_id != 0 {
      
      // Create a HashSet with just the new node ID
      let mut dirty_nodes = HashSet::new();
      dirty_nodes.insert(node_id);
      
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


  pub fn move_node(&mut self, node_id: u64, position: DVec2) {
    // Early return if active_node_network_name is None
    let node_network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(node_network) = self.node_type_registry.node_networks.get_mut(node_network_name) {
      node_network.move_node(node_id, position);
    }
  }

  pub fn can_connect_nodes(&self, source_node_id: u64, dest_node_id: u64, dest_param_index: usize) -> bool {
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
    
    network.can_connect_nodes(source_node_id, dest_node_id, dest_param_index, &self.node_type_registry)
  }

  pub fn connect_nodes(&mut self, source_node_id: u64, dest_node_id: u64, dest_param_index: usize) {
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
      match self.node_type_registry.get_node_type_for_node(&dest_node) {
        Some(node_type) => {
          if dest_param_index >= node_type.parameters.len() {
            return;
          }
          node_type.parameters[dest_param_index].multi
        }
        None => return,
      }
    };

    // Then make the connection
    if let Some(node_network) = self.node_type_registry.node_networks.get_mut(node_network_name) {
      node_network.connect_nodes(
        source_node_id,
        dest_node_id,
        dest_param_index,
        dest_param_is_multi,
      );
      
      // Create a HashSet with the source and destination nodes marked as dirty
      let mut dirty_nodes = HashSet::new();
      dirty_nodes.insert(source_node_id);
      dirty_nodes.insert(dest_node_id);
      
      // Apply display policy considering only these nodes as dirty
      self.apply_node_display_policy(Some(&dirty_nodes));
    }
  }

  pub fn set_node_network_data(&mut self, node_id: u64, mut data: Box<dyn NodeData>) {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    
    // Check node type before modification
    let (is_parameter_node, is_expr_node) = if let Some(network) = self.node_type_registry.node_networks.get(network_name) {
      if let Some(node) = network.nodes.get(&node_id) {
        (node.node_type_name == "parameter", node.node_type_name == "expr")
      } else {
        (false, false)
      }
    } else {
      (false, false)
    };
    
    // For expr nodes, validate the expression before setting the data
    let mut expr_validation_errors = Vec::new();
    if is_expr_node {
      if let Some(expr_data) = data.as_any_mut().downcast_mut::<crate::structure_designer::nodes::expr::ExprData>() {
        expr_validation_errors = expr_data.parse_and_validate(node_id);
      }
    }
    
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      network.set_node_network_data(node_id, data);
    }
    
    // Cache custom NodeType for parameter and expr nodes after data is set
    if is_parameter_node || is_expr_node {
      // Split the borrow to avoid conflicts
      let (built_in_types, node_networks) = (&self.node_type_registry.built_in_node_types, &mut self.node_type_registry.node_networks);
      if let Some(network) = node_networks.get_mut(network_name) {
        if let Some(node) = network.nodes.get_mut(&node_id) {
          // Call the populate function with the split borrows
          crate::structure_designer::node_type_registry::NodeTypeRegistry::populate_custom_node_type_cache_with_types(built_in_types, node);
        }
      }
    }
    
    // Validate if this was a parameter or expr node modification
    if is_parameter_node || is_expr_node {
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
      .and_then(|atomic_structure| calc_selection_transform(atomic_structure));
    
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
    self.node_type_registry
      .node_networks
      .get_mut(network_name)
      .and_then(|network| network.get_node_network_data_mut(node_id))
  }

  pub fn get_network_evaluator(&self) -> &NetworkEvaluator {
    &self.network_evaluator
  }

  // Sets the active node network name
  pub fn set_active_node_network_name(&mut self, node_network_name: Option<String>) {
    self.active_node_network_name = node_network_name;
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
    }
  }

  pub fn sync_gadget_data(&mut self) -> bool {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return false,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      if let Some(node_id) = &network.selected_node_id {
        let data = network.get_node_network_data_mut(*node_id);
        if let Some(node_data) = data {
          if let Some(g) = &self.gadget {
            g.sync_data(node_data);
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
      // Get the previously selected node ID before changing selection
      let previously_selected_node_id = network.selected_node_id;
      
      // Update the selection
      let ret = network.select_node(node_id);
      
      // If the selection was successful, update the display policy
      if ret {
        // Create a HashSet with the previous and newly selected node IDs
        let mut dirty_nodes = HashSet::new();
        dirty_nodes.insert(node_id); // New selection
        
        // Add previously selected node to dirty nodes if it existed
        if let Some(prev_id) = previously_selected_node_id {
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

  pub fn select_wire(&mut self, source_node_id: u64, destination_node_id: u64, destination_argument_index: usize) -> bool {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return false,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      // Get the previously selected node ID before changing selection
      let previously_selected_node_id = network.selected_node_id;
      
      // Update the selection
      let ret = network.select_wire(source_node_id, destination_node_id, destination_argument_index);
      
      // If the selection was successful and there was a previously selected node
      if ret && previously_selected_node_id.is_some() {
        // Create a HashSet with just the previously selected node ID
        let mut dirty_nodes = HashSet::new();
        dirty_nodes.insert(previously_selected_node_id.unwrap());
        
        // Apply display policy considering only the previously selected node as dirty
        self.apply_node_display_policy(Some(&dirty_nodes));
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
      // Get the previously selected node ID before clearing selection
      let previously_selected_node_id = network.selected_node_id;
      
      // Clear the selection
      network.clear_selection();

      // If there was a previously selected node
      if let Some(prev_id) = previously_selected_node_id {
        // Create a HashSet with just the previously selected node ID
        let mut dirty_nodes = HashSet::new();
        dirty_nodes.insert(prev_id);
        
        // Apply display policy considering only the previously selected node as dirty
        self.apply_node_display_policy(Some(&dirty_nodes));
      }
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
      // If a node is selected, all connected nodes will be dirty
      if let Some(selected_node_id) = node_network.selected_node_id {
        // Get all nodes connected to the selected node
        dirty_nodes = node_network.get_connected_node_ids(selected_node_id);
        
        // Check if the selected node requires validation
        if let Some(node) = node_network.nodes.get(&selected_node_id) {
          should_validate = node.node_type_name == "parameter" || {
            // Check if this node references an invalid node network
            self.node_type_registry.node_networks.get(&node.node_type_name)
              .map(|network| !network.valid)
              .unwrap_or(false)
          };
        }
      } 
      // If a wire is selected, both source and destination nodes will be dirty
      else if let Some(ref wire) = node_network.selected_wire {
        dirty_nodes.insert(wire.source_node_id);
        dirty_nodes.insert(wire.destination_node_id);
      }
    }
    
    // Perform the deletion
    if let Some(node_network) = self.node_type_registry.node_networks.get_mut(node_network_name) {
      node_network.delete_selected();
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
  /// 
  /// # Returns
  /// 
  /// The distance to the closest intersection, or None if no intersection was found
  pub fn raytrace(&self, ray_origin: &DVec3, ray_direction: &DVec3) -> Option<f64> {
    let mut min_distance: Option<f64> = None;
    
    // First, check all atomic structures in the scene
    for atomic_structure in &self.last_generated_structure_designer_scene.atomic_structures {
      match atomic_structure.hit_test(ray_origin, ray_direction) {
        crate::common::atomic_structure::HitTestResult::Atom(_, distance) | 
        crate::common::atomic_structure::HitTestResult::Bond(_, distance) => {
          // Update minimum distance if this hit is closer
          min_distance = match min_distance {
            None => Some(distance),
            Some(current_min) if distance < current_min => Some(distance),
            _ => min_distance,
          };
        },
        crate::common::atomic_structure::HitTestResult::None => {}
      }
    }
    
    let geo_trees = &self.last_generated_structure_designer_scene.geo_trees;
  
    // Convert GeoNodes to ImplicitGeometry3D trait references
    let geometries: Vec<&dyn ImplicitGeometry3D> = 
      geo_trees.iter().map(|geo_node| geo_node as &dyn ImplicitGeometry3D).collect();
  
    // Raytrace the implicit geometries using the world scale
    if let Some(geo_distance) = raytrace_geometries(
      &geometries, 
      ray_origin, 
      ray_direction, 
      DIAMOND_UNIT_CELL_SIZE_ANGSTROM
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
  }

  pub fn gadget_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
    if let Some(gadget) = &mut self.gadget {
      gadget.drag(handle_index, ray_origin, ray_direction);
    }
  }

  pub fn gadget_end_drag(&mut self) {
    if let Some(gadget) = &mut self.gadget {
      gadget.end_drag();
      self.sync_gadget_data();
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
        self.validate_active_network();
        return true;
      }
      return false;
    }

    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let ret = network.set_return_node(node_id.unwrap());
      self.validate_active_network();
      ret
    } else {
      false
    }
  }

  // Saves node networks to a file
  pub fn save_node_networks(&mut self, file_path: &str) -> std::io::Result<()> {
    node_networks_serialization::save_node_networks_to_file(&mut self.node_type_registry, file_path)
  }

  // Loads node networks from a file
  // Resets the active_node_network_name to None
  pub fn load_node_networks(&mut self, file_path: &str) -> std::io::Result<()> {

    let result = node_networks_serialization::load_node_networks_from_file(
      &mut self.node_type_registry, 
      file_path
    );

    // Reset active node network to None
    self.set_active_node_network_name(None);
    
    // Apply display policy to all nodes
    self.apply_node_display_policy(None);

    result
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
        let result = validate_network(&mut network, &self.node_type_registry, errors_to_use);
        
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
  
}
