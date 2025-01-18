use super::model::Model;
use super::command::Command;
use glam::f32::Vec3;
use super::commands::add_atom_command::AddAtomCommand;
use super::commands::add_bond_command::AddBondCommand;
use super::commands::select_command::SelectCommand;
use std::ops::Deref;

pub struct Kernel {
  model: Model,
  history: Vec<Box<dyn Command>>,
  next_history_index: usize, // Next index (the one that was last executed plus one) in the history vector.
}

impl Kernel {

  pub fn new() -> Self {
    Self {
      model: Model::new(),
      history: Vec::new(),
      next_history_index: 0,
    }
  }

  pub fn get_model(&self) -> &Model {
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

  // -------------------------------------------------------------------------------------------------------------------------
  // --- Wrapper methods for issuing undoable commands. See the command documentations for the meaning of the parameters.  ---
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
}
