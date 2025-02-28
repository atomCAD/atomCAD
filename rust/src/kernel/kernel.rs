use super::atomic_structure::AtomicStructure;
use super::command::Command;
use glam::f32::Vec3;
use glam::f32::Vec2;
use super::commands::add_atom_command::AddAtomCommand;
use super::commands::add_bond_command::AddBondCommand;
use super::commands::select_command::SelectCommand;
use super::node_type_registry::NodeTypeRegistry;
use super::node_network::NodeNetwork;
use super::node_type::DataType;
use super::node_type::NodeType;
use super::node_type::NodeData;
use super::implicit_network_evaluator::ImplicitNetworkEvaluator;
use super::surface_point_cloud::SurfacePointCloud;
use super::scene::Scene;
use super::gadgets::gadget::Gadget;
use std::ops::Deref;

pub struct Kernel {
  pub model: AtomicStructure,
  pub history: Vec<Box<dyn Command>>,
  pub next_history_index: usize, // Next index (the one that was last executed plus one) in the history vector.
  pub node_type_registry: NodeTypeRegistry,
  pub network_evaluator: ImplicitNetworkEvaluator,
  pub gadget: Option<Box<dyn Gadget>>,
}

impl Kernel {

  pub fn new() -> Self {

    let node_type_registry = NodeTypeRegistry::new();
    let network_evaluator = ImplicitNetworkEvaluator::new();

    Self {
      model: AtomicStructure::new(),
      history: Vec::new(),
      next_history_index: 0,
      node_type_registry,
      network_evaluator,
      gadget: None,
    }
  }

  pub fn get_atomic_structure(&self) -> &AtomicStructure {
    &self.model
  }

  pub fn get_history_size(&self) -> usize {
    self.history.len()
  }

  pub fn execute_command(&mut self, mut command: Box<dyn Command>) -> & Box<dyn Command> {
    if self.history.len() > self.next_history_index {
      self.history.drain(self.next_history_index..);
    }
    command.execute(&mut self.model, false);
    self.history.push(command);
    self.next_history_index = self.history.len();

    & self.history[self.history.len() - 1]
  }

  pub fn undo(&mut self) -> bool {
    if self.next_history_index == 0 {
      return false;
    }
    self.next_history_index -= 1;
    self.history[self.next_history_index].undo(&mut self.model);
    return true;
  }

  pub fn redo(&mut self) -> bool {
    if self.next_history_index >= self.history.len() {
      return false;
    }
    self.history[self.next_history_index].execute(&mut self.model, true);
    return true;
  }

  // Generates the scene to be rendered according to the displayed nodes of the given node network
  pub fn generate_scene(&mut self, node_network_name: &str) -> Scene {
    let network = match self.node_type_registry.node_networks.get(node_network_name) {
      Some(network) => network,
      None => return Scene::new(),
    };
    let mut scene: Scene = Scene::new();
    for node_id in &network.displayed_node_ids {
      scene.merge(self.network_evaluator.generate_scene(node_network_name, *node_id, &self.node_type_registry));
    }

    if let Some(gadget) = &self.gadget {
      scene.gadget = Some(gadget.clone_box());
    }

    return scene;
  }

  // -------------------------------------------------------------------------------------------------------------------------
  // -------------------------------------------------------------------------------------------------------------------------

  // Issue an AddAtomCommand
  pub fn add_atom(&mut self, atomic_number: i32, position: Vec3) -> u64 {
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

  // node network methods

  pub fn add_node_network(&mut self, node_network_name: &str) {
    self.node_type_registry.add_node_network(NodeNetwork::new(
      NodeType {
        name: node_network_name.to_string(),
        parameters: Vec::new(),
        output_type: DataType::Geometry // TODO: change this
      }
    ));
  }

  pub fn add_node(&mut self, node_network_name: &str, node_type_name: &str, position: Vec2) -> u64 {
    // First get the node type info
    let num_parameters = match self.node_type_registry.get_node_type(node_type_name) {
      Some(node_type) => node_type.parameters.len(),
      None => return 0,
    };

    // Then modify the network
    if let Some(node_network) = self.node_type_registry.node_networks.get_mut(node_network_name) {
      node_network.add_node(node_type_name, position, num_parameters)
    } else {
      0
    }
  }

  pub fn move_node(&mut self, node_network_name: &str, node_id: u64, position: Vec2) {
    if let Some(node_network) = self.node_type_registry.node_networks.get_mut(node_network_name) {
      node_network.move_node(node_id, position);
    }
  }

  pub fn connect_nodes(&mut self, node_network_name: &str, source_node_id: u64, dest_node_id: u64, dest_param_index: usize) {
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

  pub fn set_node_network_data(&mut self, network_name: &str, node_id: u64, data: Box<dyn NodeData>) {
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      network.set_node_network_data(node_id, data);
    }
  }

  pub fn get_node_network_data(&self, network_name: &str, node_id: u64) -> Option<&dyn NodeData> {
    self.node_type_registry
      .node_networks
      .get(network_name)
      .and_then(|network| network.get_node_network_data(node_id))
  }

  pub fn get_network_evaluator(&self) -> &ImplicitNetworkEvaluator {
    &self.network_evaluator
  }

  pub fn set_node_display(&mut self, network_name: &str, node_id: u64, is_displayed: bool) {
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      network.set_node_display(node_id, is_displayed);
    }
  }

  pub fn select_node(&mut self, network_name: &str, node_id: u64) -> bool {
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let ret = network.select_node(node_id);
      self.gadget = network.provide_gadget();
      ret
    } else {
      false
    }
  }

  pub fn select_wire(&mut self, network_name: &str, source_node_id: u64, destination_node_id: u64, destination_argument_index: usize) -> bool {
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      let ret = network.select_wire(source_node_id, destination_node_id, destination_argument_index);
      self.gadget = network.provide_gadget();
      ret
    } else {
      false
    }
  }

  pub fn clear_selection(&mut self, network_name: &str) {
    if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
      network.clear_selection();
      self.gadget = network.provide_gadget();
    }
  }

  pub fn delete_selected(&mut self, node_network_name: &str) {
    if let Some(node_network) = self.node_type_registry.node_networks.get_mut(node_network_name) {
      node_network.delete_selected();
    }
  }
}
