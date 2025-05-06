use crate::structure_designer::node_data::node_data::NodeData;
use crate::structure_designer::gadgets::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::edit_atom_command::EditAtomCommand;
use crate::common::atomic_structure::AtomicStructure;
use crate::api::structure_designer_api_types::APIEditAtomTool;

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
}

impl EditAtomData {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            next_history_index: 0,
            active_tool: EditAtomTool::Default(DefaultToolState {
                replacement_atomic_number: 6, // Default to carbon
            }),
        }
    }

    pub fn get_history_size(&self) -> usize {
      self.history.len()
    }
  
    pub fn eval(&self, atomic_structure: &mut AtomicStructure) {
      for i in 0..self.next_history_index {
        self.history[i].execute(atomic_structure);
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
}



impl NodeData for EditAtomData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}
