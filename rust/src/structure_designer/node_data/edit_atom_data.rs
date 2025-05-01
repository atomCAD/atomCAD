use crate::structure_designer::node_data::node_data::NodeData;
use crate::structure_designer::gadgets::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::edit_atom_command::EditAtomCommand;
use crate::common::atomic_structure::AtomicStructure;

pub struct EditAtomData {
    pub history: Vec<Box<dyn EditAtomCommand>>,
    pub next_history_index: usize, // Next index (the one that was last executed plus one) in the history vector.
    pub model: AtomicStructure,
}

impl EditAtomData {
    pub fn new(model: AtomicStructure) -> Self {
        Self {
            history: Vec::new(),
            next_history_index: 0,
            model
        }
    }

    pub fn get_history_size(&self) -> usize {
      self.history.len()
    }
  
    pub fn execute_command(&mut self, mut command: Box<dyn EditAtomCommand>) -> & Box<dyn EditAtomCommand> {
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
}

impl NodeData for EditAtomData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}
