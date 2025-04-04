use super::super::command::Command;
use crate::common::atomic_structure::AtomicStructure;

/*
 * Command to add a bond between the given atoms with the given multiplicity (1-3).
 */
pub struct AddBondCommand {
  pub atom_id1: u64,
  pub atom_id2: u64,
  pub multiplicity: i32,

  // undo information
  pub bond_id: u64,
}

impl AddBondCommand {
  pub fn new(atom_id1: u64, atom_id2: u64, multiplicity: i32) -> Self {
      Self { atom_id1, atom_id2, multiplicity, bond_id: 0 }
  }
}

impl Command for AddBondCommand {
  fn execute(&mut self, model: &mut AtomicStructure, is_redo: bool) {
    if !is_redo {
      self.bond_id = model.obtain_next_bond_id();
    }
    model.add_bond_with_id(self.bond_id, self.atom_id1, self.atom_id2, self.multiplicity);
  }

  fn undo(&mut self, model: &mut AtomicStructure) {
    model.delete_bond(self.bond_id);
  }
}
