use crate::structure_designer::node_data::node_data::NodeData;
use crate::structure_designer::gadgets::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::edit_atom_command::EditAtomCommand;
use crate::common::atomic_structure::AtomicStructure;

pub struct EditAtomData {
    pub history: Vec<Box<dyn EditAtomCommand>>,
    pub next_history_index: usize, // Next index (the one that was last executed plus one) in the history vector.
}

impl EditAtomData {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            next_history_index: 0,
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
}

impl NodeData for EditAtomData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}
