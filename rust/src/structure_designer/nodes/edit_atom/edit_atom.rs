use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::nodes::edit_atom::edit_atom_command::EditAtomCommand;
use crate::common::atomic_structure::AtomicStructure;
use crate::api::structure_designer_api_types::APIEditAtomTool;
use crate::util::transform::Transform;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;

pub struct DefaultToolState {
  pub replacement_atomic_number: i32,
}

pub struct AddAtomToolState {
  pub atomic_number: i32,
}

pub struct AddBondToolState {
  pub last_atom_id: Option<u64>,
}

pub enum EditAtomTool {
  Default(DefaultToolState),
  AddAtom(AddAtomToolState),
  AddBond(AddBondToolState),
}

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
  
    pub fn eval(&self, atomic_structure: &mut AtomicStructure) {
      for i in 0..self.next_history_index {
        self.history[i].execute(atomic_structure);
      }
      
      // Clear any previously marked atoms first
      atomic_structure.clear_marked_atoms();
      
      // If the active tool is AddBond and there's a last_atom_id, mark that atom
      if let EditAtomTool::AddBond(state) = &self.active_tool {
        if let Some(atom_id) = state.last_atom_id {
          if let Some(atom) = atomic_structure.atoms.get_mut(&atom_id) {
            atom.marked = true;
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
    
    pub fn set_default_tool_atomic_number(&mut self, replacement_atomic_number: i32) -> bool {
        match &mut self.active_tool {
            EditAtomTool::Default(state) => {
                state.replacement_atomic_number = replacement_atomic_number;
                true
            },
            _ => false,
        }
    }
    
    pub fn set_add_atom_tool_atomic_number(&mut self, atomic_number: i32) -> bool {
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
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

pub fn eval_edit_atom<'a>(network_evaluator: &NetworkEvaluator, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);

  let input_val = if node.arguments[0].argument_node_ids.is_empty() {
    return NetworkResult::Atomic(AtomicStructure::new());
  } else {
    let input_node_id = node.arguments[0].get_node_id().unwrap();
    network_evaluator.evaluate(network_stack, input_node_id, registry)[0].clone()
  };

  if let NetworkResult::Atomic(mut atomic_structure) = input_val {
    let edit_atom_data = &node.data.as_any_ref().downcast_ref::<EditAtomData>().unwrap();

    edit_atom_data.eval(&mut atomic_structure);
    return NetworkResult::Atomic(atomic_structure);
  }
  return NetworkResult::Atomic(AtomicStructure::new());
}