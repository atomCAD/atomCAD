use crate::common::atomic_structure::AtomicStructure;
use crate::common::atomic_structure_utils::calc_selection_transform;
use crate::util::transform::Transform;
use glam::f64::DVec3;
use glam::f64::DVec2;
use super::edit_atom_commands::transform_command::TransformCommand;
use super::node_type_registry::NodeTypeRegistry;
use super::node_network::NodeNetwork;
use super::node_type::DataType;
use super::node_type::NodeType;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_data::NoData;
use crate::structure_designer::nodes::edit_atom::EditAtomData;
use crate::structure_designer::edit_atom_commands::select_command::SelectCommand;
use crate::structure_designer::edit_atom_commands::delete_command::DeleteCommand;
use crate::structure_designer::edit_atom_commands::replace_command::ReplaceCommand;
use crate::structure_designer::edit_atom_commands::add_atom_command::AddAtomCommand;
use crate::structure_designer::edit_atom_commands::add_bond_command::AddBondCommand;
use super::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::structure_designer_scene::StructureDesignerScene;
use super::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::serialization::node_networks_serialization;
use crate::api::common_api_types::SelectModifier;
use crate::common::atomic_structure::BondReference;
use crate::common::atomic_structure::HitTestResult;
use crate::structure_designer::nodes::edit_atom::EditAtomTool;

pub struct StructureDesigner {
  pub node_type_registry: NodeTypeRegistry,
  pub network_evaluator: NetworkEvaluator,
  pub gadget: Option<Box<dyn NodeNetworkGadget>>,
  pub active_node_network_name: Option<String>,
  pub last_generated_structure_designer_scene: StructureDesignerScene,
}

impl StructureDesigner {

  pub fn new() -> Self {

    let node_type_registry = NodeTypeRegistry::new();
    let network_evaluator = NetworkEvaluator::new();

    Self {
      node_type_registry,
      network_evaluator,
      gadget: None,
      active_node_network_name: None,
      last_generated_structure_designer_scene: StructureDesignerScene::new(),
    }
  }
}

impl StructureDesigner {

  pub fn set_last_generated_structure_designer_scene(&mut self, scene: StructureDesignerScene) {
    self.last_generated_structure_designer_scene = scene;
    self.refresh_scene_dependent_node_data();
  }

  // Returns the first atomic structure generated from a selected node, if any
  pub fn get_atomic_structure_from_selected_node(&self) -> Option<&AtomicStructure> {
    // Find the first atomic structure with from_selected_node = true
    self.last_generated_structure_designer_scene.atomic_structures.iter()
      .find(|structure| structure.from_selected_node)
  }

  // Generates the scene to be rendered according to the displayed nodes of the active node network
  pub fn generate_scene(&mut self, lightweight: bool) -> StructureDesignerScene {

    let mut scene: StructureDesignerScene = StructureDesignerScene::new();

    if !lightweight {
      // Check if node_network_name exists
      let node_network_name = match &self.active_node_network_name {
        Some(name) => name,
        None => return scene, // Return empty scene if node_network_name is None
      };
      
      let network = match self.node_type_registry.node_networks.get(node_network_name) {
        Some(network) => network,
        None => return scene,
      };
      for node_id in &network.displayed_node_ids {
        scene.merge(self.network_evaluator.generate_scene(node_network_name, *node_id, &self.node_type_registry));
      }
    }

    if let Some(gadget) = &self.gadget {
      scene.tessellatable = Some(gadget.as_tessellatable());
    }

    return scene;
  }

  // Returns whether an atom or a bond was hit or not.
  pub fn select_atom_or_bond_by_ray(&mut self, ray_start: &DVec3, ray_dir: &DVec3, select_modifier: SelectModifier) -> bool {
    let atomic_structure = self.get_atomic_structure_from_selected_node();
    if atomic_structure.is_none() {
      return false;
    }
    let atomic_structure = atomic_structure.unwrap();
    
    // Use the unified hit_test function instead of separate atom and bond tests
    match atomic_structure.hit_test(ray_start, ray_dir) {
      HitTestResult::Atom(atom_id, _distance) => {
        self.select_atom_by_id(atom_id, select_modifier);
        true
      },
      HitTestResult::Bond(bond_id, _distance) => {
        // Get a proper bond reference from the bond ID
        if let Some(bond_reference) = atomic_structure.get_bond_reference_by_id(bond_id) {
          self.select_bond_by_reference(&bond_reference, select_modifier);
          true
        } else {
          // Bond ID was valid during hit test but no longer exists
          false
        }
      },
      HitTestResult::None => false
    }
  }

  /// Helper method to get the selected node ID of an edit_atom node
  /// 
  /// Returns None if:
  /// - There is no active node network
  /// - No node is selected in the active network
  /// - The selected node is not an edit_atom node
  fn get_active_edit_atom_node_id(&self) -> Option<u64> {
    // Get active node network name
    let network_name = self.active_node_network_name.as_ref()?;
    
    // Get the active node network
    let network = self.node_type_registry.node_networks.get(network_name)?;
    
    // Get the selected node ID
    let selected_node_id = network.selected_node_id?;
    
    // Get the selected node's type name
    let node_type_name = network.nodes.get(&selected_node_id)?.node_type_name.as_str();
    
    // Check if the node is an edit_atom node
    if node_type_name != "edit_atom" {
      return None;
    }
    
    Some(selected_node_id)
  }

  /// Gets the EditAtomData for the currently active edit_atom node (immutable)
  /// 
  /// Returns None if:
  /// - There is no active node network
  /// - No node is selected in the active network
  /// - The selected node is not an edit_atom node
  /// - The EditAtomData cannot be retrieved or cast
  pub fn get_active_edit_atom_data(&self) -> Option<&EditAtomData> {
    let selected_node_id = self.get_active_edit_atom_node_id()?;
    
    // Get the node data and cast it to EditAtomData
    let node_data = self.get_node_network_data(selected_node_id)?;
    
    // Try to downcast to EditAtomData
    node_data.as_any_ref().downcast_ref::<EditAtomData>()
  }

  /// Gets the EditAtomData for the currently active edit_atom node (mutable)
  /// 
  /// Returns None if:
  /// - There is no active node network
  /// - No node is selected in the active network
  /// - The selected node is not an edit_atom node
  /// - The EditAtomData cannot be retrieved or cast
  pub fn get_active_edit_atom_data_mut(&mut self) -> Option<&mut EditAtomData> {
    let selected_node_id = self.get_active_edit_atom_node_id()?;
    
    // Get the node data and cast it to EditAtomData
    let node_data = self.get_node_network_data_mut(selected_node_id)?;
    
    // Try to downcast to EditAtomData
    node_data.as_any_mut().downcast_mut::<EditAtomData>()
  }

  // Selects an atom by its ID using the active edit_atom node
  pub fn select_atom_by_id(&mut self, atom_id: u64, select_modifier: SelectModifier) {
    // Get the EditAtomData from the active edit_atom node
    let edit_atom_data = match self.get_active_edit_atom_data_mut() {
      Some(data) => data,
      None => return,
    };
    
    // Create the SelectCommand with the selected atom ID
    let select_command = Box::new(SelectCommand::new(
      vec![atom_id],         // atom_ids
      vec![],                // bond_references (empty)
      select_modifier        // select_modifier
    ));
    
    // Add the command to the edit_atom_data
    edit_atom_data.add_command(select_command);
  }

  pub fn delete_selected_atoms_and_bonds(&mut self) {
    let edit_atom_data = match self.get_active_edit_atom_data_mut() {
      Some(data) => data,
      None => return,
    };
    
    let delete_command = Box::new(DeleteCommand::new());
    
    edit_atom_data.add_command(delete_command);
  }

  pub fn add_atom_by_ray(&mut self, atomic_number: i32, plane_normal: &DVec3, ray_start: &DVec3, ray_dir: &DVec3) {
    // Get the atomic structure from the selected node
    let atomic_structure = match self.get_atomic_structure_from_selected_node() {
      Some(structure) => structure,
      None => return,
    };
    
    // Find the closest atom to the ray
    let closest_atom_position = atomic_structure.find_closest_atom_to_ray(ray_start, ray_dir);
    
    // Calculate the plane distance and intersection point
    let default_distance = 5.0; // Default distance to use if no atom was hit
    let plane_distance = match closest_atom_position {
      Some(atom_pos) => plane_normal.dot(atom_pos), // Plane passes through closest atom
      None => plane_normal.dot(*ray_start) + default_distance, // Plane at default distance
    };
    
    // Calculate the intersection of the ray with the plane
    // For a plane equation: plane_normal·point = plane_distance
    // And a ray equation: point = ray_start + t*ray_dir
    // Solving for t: plane_normal·(ray_start + t*ray_dir) = plane_distance
    // t = (plane_distance - plane_normal·ray_start) / (plane_normal·ray_dir)
    let denominator = plane_normal.dot(*ray_dir);
    
    // Check if ray is parallel to the plane (or nearly so)
    if denominator.abs() < 1e-6 {
      return; // Ray is parallel to the plane, no intersection
    }
    
    let t = (plane_distance - plane_normal.dot(*ray_start)) / denominator;
    
    // Check if intersection is behind the ray origin
    if t < 0.0 {
      return; // Intersection is behind the ray origin
    }
    
    // Calculate the intersection point
    let intersection_point = *ray_start + *ray_dir * t;
    
    // Add the atom at the calculated position
    self.add_atom(atomic_number, intersection_point);
  }

  pub fn draw_bond_by_ray(&mut self, ray_start: &DVec3, ray_dir: &DVec3) {
    let atomic_structure = match self.get_atomic_structure_from_selected_node() {
      Some(structure) => structure,
      None => return,
    };

    // Find the atom along the ray, ignoring bond hits
    let atom_id = match atomic_structure.hit_test(ray_start, ray_dir) {
      HitTestResult::Atom(id, _) => id,
      _ => return,
    };

    let edit_atom_data = match self.get_active_edit_atom_data_mut() {
      Some(data) => data,
      None => return,
    };

    // Check if we have a last atom ID stored in the tool state
    if let Some(bond_tool_state) = edit_atom_data.get_add_bond_tool_state() {
      match bond_tool_state.last_atom_id {
        Some(last_id) => {
          // If we're clicking on the same atom again, cancel the bond and reset
          if last_id == atom_id {
            // Reset the last atom ID to None
            if let EditAtomTool::AddBond(state) = &mut edit_atom_data.active_tool {
              state.last_atom_id = None;
            }
          } else {
            // Create a bond between the last atom and the current atom
            let add_bond_command = Box::new(AddBondCommand::new(last_id, atom_id, 1));
            edit_atom_data.add_command(add_bond_command);
            
            // Update the last_atom_id to the current atom for continuous bonding
            if let EditAtomTool::AddBond(state) = &mut edit_atom_data.active_tool {
              state.last_atom_id = Some(atom_id);
            }
          }
        },
        None => {
          // No previous atom selected, store this one
          if let EditAtomTool::AddBond(state) = &mut edit_atom_data.active_tool {
            state.last_atom_id = Some(atom_id);
          }
        }
      }
    }
  }  

  pub fn add_atom(&mut self, atomic_number: i32, position: DVec3) {
    let edit_atom_data = match self.get_active_edit_atom_data_mut() {
      Some(data) => data,
      None => return,
    };
    
    let add_atom_command = Box::new(AddAtomCommand::new(atomic_number, position));
    
    edit_atom_data.add_command(add_atom_command);
  }

  // Replaces all selected atoms with the specified atomic number
  pub fn replace_selected_atoms(&mut self, atomic_number: i32) {
    let edit_atom_data = match self.get_active_edit_atom_data_mut() {
      Some(data) => data,
      None => return,
    };
    
    let replace_command = Box::new(ReplaceCommand::new(atomic_number));
    
    edit_atom_data.add_command(replace_command);
  }

  /// Transform selected atoms using an absolute transform
  /// 
  /// Takes an absolute transform and converts it to a relative transform
  /// by comparing with the current selection transform. Then creates and
  /// executes a TransformCommand with that relative transform.
  /// 
  /// # Arguments
  /// * `abs_transform` - The absolute transform to apply
  pub fn transform_selected(&mut self, abs_transform: &Transform) {
    // First get the current transform to avoid borrowing issues
    let current_transform_opt = {
      // Get the current atomic structure
      if let Some(structure) = self.get_atomic_structure_from_selected_node() {
        // Clone the transform if it exists
        structure.selection_transform.clone()
      } else {
        return; // No atomic structure, exit early
      }
    };
    
    // If we don't have a current transform, we can't proceed
    let current_transform = match current_transform_opt {
      Some(transform) => transform,
      None => return,
    };
    
    // Now get the edit atom data (after we're done with the atomic structure)
    let edit_atom_data = match self.get_active_edit_atom_data_mut() {
      Some(data) => data,
      None => return,
    };
    
    // Calculate the relative transform (delta) needed to go from current to desired absolute transform
    let relative_transform = abs_transform.delta_from(&current_transform);
    
    // Create a transform command with the relative transform
    let transform_command = Box::new(TransformCommand::new(relative_transform));
    
    // Add the command to the edit atom data
    edit_atom_data.add_command(transform_command);
  }

  pub fn edit_atom_undo(&mut self) {
    let edit_atom_data = match self.get_active_edit_atom_data_mut() {
      Some(data) => data,
      None => return,
    };
    edit_atom_data.undo();
    self.edit_atom_tool_refresh();
  }

  pub fn edit_atom_redo(&mut self) {
    let edit_atom_data = match self.get_active_edit_atom_data_mut() {
      Some(data) => data,
      None => return,
    };
    edit_atom_data.redo();
    self.edit_atom_tool_refresh();
  }    

  fn edit_atom_tool_refresh(&mut self) {
    // First, get information without mutable borrow
    let last_atom_id_opt = {
      // Check if we're in add bond mode and have a last_atom_id
      if let Some(edit_atom_data) = self.get_active_edit_atom_data() {
        if let Some(bond_tool_state) = edit_atom_data.get_add_bond_tool_state() {
          bond_tool_state.last_atom_id
        } else {
          return; // Not in add bond mode
        }
      } else {
        return; // No edit atom data
      }
    };
    
    // If there's no last atom ID, nothing to validate
    if last_atom_id_opt.is_none() {
      return;
    }
    
    let last_atom_id = last_atom_id_opt.unwrap();
    
    // Check if the atom still exists
    let atom_exists = {
      if let Some(atomic_structure) = self.get_atomic_structure_from_selected_node() {
        atomic_structure.get_atom(last_atom_id).is_some()
      } else {
        false
      }
    };
    
    // If the atom doesn't exist, reset the last_atom_id
    if !atom_exists {
      if let Some(edit_atom_data) = self.get_active_edit_atom_data_mut() {
        if let EditAtomTool::AddBond(state) = &mut edit_atom_data.active_tool {
          state.last_atom_id = None;
        }
      }
    }
  }

  // Selects a bond by its ID using the active edit_atom node
  pub fn select_bond_by_reference(&mut self, bond_reference: &BondReference, select_modifier: SelectModifier) {
    let edit_atom_data = match self.get_active_edit_atom_data_mut() {
      Some(data) => data,
      None => return,
    };

    let select_command = Box::new(SelectCommand::new(
      vec![],
      vec![bond_reference.clone()],
      select_modifier
    ));
    
    edit_atom_data.add_command(select_command);
  }

  // Returns true if the selected node is displayed and has an 'edit_atom' node type name
  pub fn is_edit_atom_active(&self) -> bool {
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
    if !network.displayed_node_ids.contains(&selected_node_id) {
      return false;
    }
    
    // Get the selected node's type name
    let node_type_name = match network.nodes.get(&selected_node_id) {
      Some(node) => &node.node_type_name,
      None => return false,
    };
    
    // Return true only if the node's type name is 'edit_atom'
    node_type_name == "edit_atom"
  }

  // -------------------------------------------------------------------------------------------------------------------------
  // -------------------------------------------------------------------------------------------------------------------------

  /*
  // Issue an AddAtomCommand
  pub fn add_atom(&mut self, atomic_number: i32, position: DVec3) -> u64 {
    let executed_command = self.execute_command(Box::new(AddAtomCommand::new(atomic_number, position)));
    let c: &AddAtomCommand = executed_command.deref().as_any_ref().downcast_ref().unwrap();
    c.atom_id
  }

  // Issue an AddBondCommand
  pub fn add_bond(&mut self, atom_id1: u64, atom_id2: u64, multiplicity: i32) -> u64 {
    let executed_command = self.execute_command(Box::new(AddBondCommand::new(atom_id1, atom_id2, multiplicity)));
    let c: &AddBondCommand = executed_command.deref().as_any_ref().downcast_ref().unwrap();
    c.bond_id
  }

  // Issue a SelectCommand
  pub fn select(&mut self, atom_ids: Vec<u64>, bond_ids: Vec<u64>, unselect: bool) {
    self.execute_command(Box::new(SelectCommand::new(atom_ids, bond_ids, unselect)));
  }
  */

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
        output_type: DataType::Geometry, // TODO: change this
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
    let (num_parameters, node_data) = match self.node_type_registry.get_node_type(node_type_name) {
      Some(node_type) => {
        let data_creator = &node_type.node_data_creator;
        (node_type.parameters.len(), (data_creator)())
      },
      None => return 0,
    };

    // Then modify the network
    if let Some(node_network) = self.node_type_registry.node_networks.get_mut(node_network_name) {
      node_network.add_node(node_type_name, position, num_parameters, node_data)
    } else {
      0
    }
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
      match self.node_type_registry.get_node_type(&dest_node.node_type_name) {
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
    }
  }

  pub fn set_node_network_data(&mut self, node_id: u64, data: Box<dyn NodeData>) {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      network.set_node_network_data(node_id, data);
      self.gadget = network.provide_gadget();
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
    if let Some(edit_atom_data) = self.get_active_edit_atom_data_mut() {
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
      let ret = network.select_node(node_id);
      self.gadget = network.provide_gadget();
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
      let ret = network.select_wire(source_node_id, destination_node_id, destination_argument_index);
      self.gadget = network.provide_gadget();
      ret
    } else {
      false
    }
  }

  fn refresh_gadget(&mut self) {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      self.gadget = network.provide_gadget();
    }
  }

  pub fn clear_selection(&mut self) {
    // Early return if active_node_network_name is None
    let network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      network.clear_selection();
      self.gadget = network.provide_gadget();
    }
  }

  pub fn delete_selected(&mut self) {
    // Early return if active_node_network_name is None
    let node_network_name = match &self.active_node_network_name {
      Some(name) => name,
      None => return,
    };
    if let Some(node_network) = self.node_type_registry.node_networks.get_mut(node_network_name) {
      node_network.delete_selected();
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
    }
  }

  /// Sets a node as the return node for the active network.
  /// Determines the output type using the NodeTypeRegistry and updates the network's output_type.
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
        return true;
      }
      return false;
    }
    
    // Unwrap the node_id as we know it's Some
    let node_id_unwrapped = node_id.unwrap();
    
    // Get the node from the network to determine its type
    let node_type_name = {
      let network = match self.node_type_registry.node_networks.get(network_name) {
        Some(network) => network,
        None => return false,
      };
      
      match network.nodes.get(&node_id_unwrapped) {
        Some(node) => node.node_type_name.clone(),
        None => return false,
      }
    };
    
    // Get the output type from the node type registry
    let output_type = match self.node_type_registry.get_node_type(&node_type_name) {
      Some(node_type) => node_type.output_type,
      None => return false,
    };
    
    // Set the return node with the determined output type
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      network.set_return_node(node_id_unwrapped, output_type)
    } else {
      false
    }
  }

  // Saves node networks to a file
  pub fn save_node_networks(&self, file_path: &str) -> std::io::Result<()> {
    node_networks_serialization::save_node_networks_to_file(&self.node_type_registry, file_path)
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
    
    result
  }
}
