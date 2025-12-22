use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::nodes::edit_atom::edit_atom_command::EditAtomCommand;
use crate::crystolecule::atomic_structure::{AtomDisplayState, AtomicStructure};
use crate::util::transform::Transform;
use crate::structure_designer::evaluator::network_result::{NetworkResult, input_missing_error, error_in_input};
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::crystolecule::atomic_structure::HitTestResult;
use crate::api::common_api_types::SelectModifier;
use crate::display::atomic_tessellator::{get_displayed_atom_radius, BAS_STICK_RADIUS};
use crate::display::preferences as display_prefs;
use glam::f64::DVec3;
use crate::structure_designer::nodes::edit_atom::commands::select_command::SelectCommand;
use crate::structure_designer::nodes::edit_atom::commands::delete_command::DeleteCommand;
use crate::structure_designer::nodes::edit_atom::commands::replace_command::ReplaceCommand;
use crate::structure_designer::nodes::edit_atom::commands::add_atom_command::AddAtomCommand;
use crate::structure_designer::nodes::edit_atom::commands::add_bond_command::AddBondCommand;
use crate::structure_designer::nodes::edit_atom::commands::transform_command::TransformCommand;
use crate::crystolecule::atomic_structure::BondReference;
use crate::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization;
use crate::api::structure_designer::structure_designer_api_types::APIEditAtomTool;
use crate::structure_designer::node_type::NodeType;

#[derive(Debug)]
pub struct DefaultToolState {
  pub replacement_atomic_number: i16,
}

#[derive(Debug)]
pub struct AddAtomToolState {
  pub atomic_number: i16,
}

#[derive(Debug)]
pub struct AddBondToolState {
  pub last_atom_id: Option<u32>,
}

#[derive(Debug)]
pub enum EditAtomTool {
  Default(DefaultToolState),
  AddAtom(AddAtomToolState),
  AddBond(AddBondToolState),
}

#[flutter_rust_bridge::frb(ignore)]
#[derive(Debug)]
pub struct EditAtomData {
    pub history: Vec<Box<dyn EditAtomCommand>>,
    pub next_history_index: usize, // Next index (the one that was last executed plus one) in the history vector.
    pub active_tool: EditAtomTool,
    pub selection_transform: Option<Transform>,
}

impl EditAtomData {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            next_history_index: 0,
            active_tool: EditAtomTool::Default(DefaultToolState {
                replacement_atomic_number: 6, // Default to carbon
            }),
            selection_transform: None,
        }
    }

    pub fn get_history_size(&self) -> usize {
      self.history.len()
    }
  
    pub fn eval(&self, atomic_structure: &mut AtomicStructure, decorate: bool) {
      for i in 0..self.next_history_index {
        self.history[i].execute(atomic_structure);
      }

      // If the active tool is AddBond and there's a last_atom_id, mark that atom
      if decorate {
        if let EditAtomTool::AddBond(state) = &self.active_tool {
          if let Some(atom_id) = state.last_atom_id {
            atomic_structure.decorator_mut().set_atom_display_state(atom_id, AtomDisplayState::Marked);
          }
        }
      }
    }

    pub fn add_command(&mut self, command: Box<dyn EditAtomCommand>) -> & Box<dyn EditAtomCommand> {
      if self.history.len() > self.next_history_index {
        self.history.drain(self.next_history_index..);
      }
      self.history.push(command);
      self.next_history_index = self.history.len();
  
      & self.history[self.history.len() - 1]
    }
  
    pub fn undo(&mut self) -> bool {
      if self.next_history_index == 0 {
        return false;
      }
      self.next_history_index -= 1;
      return true;
    }
  
    pub fn redo(&mut self) -> bool {
      if self.next_history_index >= self.history.len() {
        return false;
      }
      self.next_history_index += 1;
      return true;
    }
    
    pub fn can_undo(&self) -> bool {
        self.next_history_index > 0
    }
    
    pub fn can_redo(&self) -> bool {
        self.next_history_index < self.history.len()
    }
    
    pub fn set_active_tool(&mut self, api_tool: APIEditAtomTool) {
        self.active_tool = match api_tool {
            APIEditAtomTool::Default => {
                EditAtomTool::Default(DefaultToolState {
                    replacement_atomic_number: 6, // Default to carbon
                })
            },
            APIEditAtomTool::AddAtom => {
                EditAtomTool::AddAtom(AddAtomToolState {
                    atomic_number: 6, // Default to carbon
                })
            },
            APIEditAtomTool::AddBond => {
                EditAtomTool::AddBond(AddBondToolState {
                    last_atom_id: None,
                })
            },
        }
    }

    pub fn get_active_tool(&self) -> APIEditAtomTool {
        match &self.active_tool {
            EditAtomTool::Default(_) => APIEditAtomTool::Default,
            EditAtomTool::AddAtom(_) => APIEditAtomTool::AddAtom,
            EditAtomTool::AddBond(_) => APIEditAtomTool::AddBond,
        }
    }
    
    pub fn set_default_tool_atomic_number(&mut self, replacement_atomic_number: i16) -> bool {
        match &mut self.active_tool {
            EditAtomTool::Default(state) => {
                state.replacement_atomic_number = replacement_atomic_number;
                true
            },
            _ => false,
        }
    }
    
    pub fn set_add_atom_tool_atomic_number(&mut self, atomic_number: i16) -> bool {
        match &mut self.active_tool {
            EditAtomTool::AddAtom(state) => {
                state.atomic_number = atomic_number;
                true
            },
            _ => false,
        }
    }
    
    pub fn get_add_bond_tool_state(&self) -> Option<&AddBondToolState> {
        match &self.active_tool {
            EditAtomTool::AddBond(state) => Some(state),
            _ => None,
        }
    }
}

impl NodeData for EditAtomData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
      None
    }

    fn eval<'a>(
      &self,
      network_evaluator: &NetworkEvaluator,
      network_stack: &Vec<NetworkStackElement<'a>>,
      node_id: u64,
      registry: &NodeTypeRegistry,
      decorate: bool,
      context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext
    ) -> NetworkResult {
      let input_val = network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
    
      if let NetworkResult::Error(_) = input_val {
        return input_val;
      }
    
      if let NetworkResult::Atomic(mut atomic_structure) = input_val {
        self.eval(&mut atomic_structure, decorate);
        return NetworkResult::Atomic(atomic_structure);
      }
      return NetworkResult::Atomic(AtomicStructure::new());
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        // Clone the history by using the clone_box method on each command
        let cloned_history: Vec<Box<dyn EditAtomCommand>> = self.history
            .iter()
            .map(|command| command.clone_box())
            .collect();

        Box::new(EditAtomData {
            history: cloned_history,
            next_history_index: self.next_history_index,
            active_tool: match &self.active_tool {
                EditAtomTool::Default(state) => EditAtomTool::Default(DefaultToolState {
                    replacement_atomic_number: state.replacement_atomic_number,
                }),
                EditAtomTool::AddAtom(state) => EditAtomTool::AddAtom(AddAtomToolState {
                    atomic_number: state.atomic_number,
                }),
                EditAtomTool::AddBond(state) => EditAtomTool::AddBond(AddBondToolState {
                    last_atom_id: state.last_atom_id,
                }),
            },
            selection_transform: self.selection_transform.clone(),
        })
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        Some(format!("ops: {}", self.history.len()))
    }
}

// Returns whether an atom or a bond was hit or not.
pub fn select_atom_or_bond_by_ray(structure_designer: &mut StructureDesigner, ray_start: &DVec3, ray_dir: &DVec3, select_modifier: SelectModifier) -> bool {
  let atomic_structure = structure_designer.get_atomic_structure_from_selected_node();
  if atomic_structure.is_none() {
    return false;
  }
  let atomic_structure = atomic_structure.unwrap();
    
  // Use the unified hit_test function instead of separate atom and bond tests
  let visualization = &structure_designer.preferences.atomic_structure_visualization_preferences.visualization;
  let display_visualization = match visualization {
    AtomicStructureVisualization::BallAndStick => display_prefs::AtomicStructureVisualization::BallAndStick,
    AtomicStructureVisualization::SpaceFilling => display_prefs::AtomicStructureVisualization::SpaceFilling,
  };
  match atomic_structure.hit_test(ray_start, ray_dir, visualization, 
    |atom| get_displayed_atom_radius(atom, &display_visualization), BAS_STICK_RADIUS) {
    HitTestResult::Atom(atom_id, _distance) => {
      select_atom_by_id(structure_designer, atom_id, select_modifier);
      true
    },
    HitTestResult::Bond(bond_reference, _distance) => {
      select_bond_by_reference(structure_designer, &bond_reference, select_modifier);
      true
    },
    HitTestResult::None => false
  }
}

pub fn delete_selected_atoms_and_bonds(structure_designer: &mut StructureDesigner) {
  let edit_atom_data = match get_selected_edit_atom_data_mut(structure_designer) {
    Some(data) => data,
    None => return,
  };
    
  let delete_command = Box::new(DeleteCommand::new());
    
  edit_atom_data.add_command(delete_command);
}

pub fn add_atom_by_ray(structure_designer: &mut StructureDesigner, atomic_number: i16, plane_normal: &DVec3, ray_start: &DVec3, ray_dir: &DVec3) {
  // Get the atomic structure from the selected node
  let atomic_structure = match structure_designer.get_atomic_structure_from_selected_node() {
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
  add_atom(structure_designer, atomic_number, intersection_point);
}

pub fn draw_bond_by_ray(structure_designer: &mut StructureDesigner, ray_start: &DVec3, ray_dir: &DVec3) {
  let atomic_structure = match structure_designer.get_atomic_structure_from_selected_node() {
    Some(structure) => structure,
    None => return,
  };

  // Find the atom along the ray, ignoring bond hits
  let visualization = &structure_designer.preferences.atomic_structure_visualization_preferences.visualization;
  let display_visualization = match visualization {
    AtomicStructureVisualization::BallAndStick => display_prefs::AtomicStructureVisualization::BallAndStick,
    AtomicStructureVisualization::SpaceFilling => display_prefs::AtomicStructureVisualization::SpaceFilling,
  };
  let atom_id = match atomic_structure.hit_test(ray_start, ray_dir, visualization, 
    |atom| get_displayed_atom_radius(atom, &display_visualization), BAS_STICK_RADIUS) {
    HitTestResult::Atom(id, _) => id,
    _ => return,
  };

  let edit_atom_data = match get_selected_edit_atom_data_mut(structure_designer) {
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

// Replaces all selected atoms with the specified atomic number
pub fn replace_selected_atoms(structure_designer: &mut StructureDesigner, atomic_number: i16) {
  let edit_atom_data = match get_selected_edit_atom_data_mut(structure_designer) {
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
pub fn transform_selected(structure_designer: &mut StructureDesigner, abs_transform: &Transform) {
  // First get the current transform to avoid borrowing issues
  let current_transform_opt = {
    // Get the current atomic structure
    if let Some(structure) = structure_designer.get_atomic_structure_from_selected_node() {
      // Clone the transform if it exists
      structure.decorator().selection_transform.clone()
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
  let edit_atom_data = match get_selected_edit_atom_data_mut(structure_designer) {
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

pub fn edit_atom_undo(structure_designer: &mut StructureDesigner) {
  let edit_atom_data = match get_selected_edit_atom_data_mut(structure_designer) {
    Some(data) => data,
    None => return,
  };
  edit_atom_data.undo();
  edit_atom_tool_refresh(structure_designer);
}

pub fn edit_atom_redo(structure_designer: &mut StructureDesigner) {
  let edit_atom_data = match get_selected_edit_atom_data_mut(structure_designer) {
    Some(data) => data,
    None => return,
  };
  edit_atom_data.redo();
  edit_atom_tool_refresh(structure_designer);
}

/// Gets the EditAtomData for the currently active edit_atom node (immutable)
/// 
/// Returns None if:
/// - There is no active node network
/// - No node is selected in the active network
/// - The selected node is not an edit_atom node
/// - The EditAtomData cannot be retrieved or cast
pub fn get_active_edit_atom_data(structure_designer: &StructureDesigner) -> Option<&EditAtomData> {
  let selected_node_id = structure_designer.get_selected_node_id_with_type("edit_atom")?;
    
  // Get the node data and cast it to EditAtomData
  let node_data = structure_designer.get_node_network_data(selected_node_id)?;
    
  // Try to downcast to EditAtomData
  node_data.as_any_ref().downcast_ref::<EditAtomData>()
}

/// Gets the EditAtomData for the currently selected edit_atom node (mutable)
/// 
/// This method automatically marks the node data as changed since it's only called
/// when the caller intends to modify the EditAtomData.
/// 
/// Returns None if:
/// - There is no active node network
/// - No node is selected in the active network
/// - The selected node is not an edit_atom node
/// - The EditAtomData cannot be retrieved or cast
pub fn get_selected_edit_atom_data_mut(structure_designer: &mut StructureDesigner) -> Option<&mut EditAtomData> {
  let selected_node_id = structure_designer.get_selected_node_id_with_type("edit_atom")?;

  // Mark that this node's data will be changed (since this method is only called for mutations)
  structure_designer.mark_node_data_changed(selected_node_id);

  // Get the node data and cast it to EditAtomData
  let node_data = structure_designer.get_node_network_data_mut(selected_node_id)?;
    
  // Try to downcast to EditAtomData
  node_data.as_any_mut().downcast_mut::<EditAtomData>()
}

fn add_atom(structure_designer: &mut StructureDesigner, atomic_number: i16, position: DVec3) {
  let edit_atom_data = match get_selected_edit_atom_data_mut(structure_designer) {
    Some(data) => data,
    None => return,
  };
    
  let add_atom_command = Box::new(AddAtomCommand::new(atomic_number, position));
    
  edit_atom_data.add_command(add_atom_command);
}

// Selects a bond by its ID using the active edit_atom node
fn select_bond_by_reference(structure_designer: &mut StructureDesigner, bond_reference: &BondReference, select_modifier: SelectModifier) {
  let edit_atom_data = match get_selected_edit_atom_data_mut(structure_designer) {
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

// Selects an atom by its ID using the active edit_atom node
fn select_atom_by_id(structure_designer: &mut StructureDesigner, atom_id: u32, select_modifier: SelectModifier) {
  // Get the EditAtomData from the active edit_atom node
  let edit_atom_data = match get_selected_edit_atom_data_mut(structure_designer) {
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

fn edit_atom_tool_refresh(structure_designer: &mut StructureDesigner) {
  // First, get information without mutable borrow
  let last_atom_id_opt = {
    // Check if we're in add bond mode and have a last_atom_id
    if let Some(edit_atom_data) = get_active_edit_atom_data(structure_designer) {
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
    if let Some(atomic_structure) = structure_designer.get_atomic_structure_from_selected_node() {
      atomic_structure.get_atom(last_atom_id).is_some()
    } else {
      false
    }
  };
    
  // If the atom doesn't exist, reset the last_atom_id
  if !atom_exists {
    if let Some(edit_atom_data) = get_selected_edit_atom_data_mut(structure_designer) {
      if let EditAtomTool::AddBond(state) = &mut edit_atom_data.active_tool {
        state.last_atom_id = None;
      }
    }
  }
}
