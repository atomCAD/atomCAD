use super::super::edit_atom_command::EditAtomCommand;
use crate::common::atomic_structure::AtomicStructure;
use crate::common::atomic_structure::BondReference;
use serde::{Serialize, Deserialize};

/*
 * A selection command. If unselect == true it unselects otherwise it selects
 * the given atoms and bonds.
 */
#[derive(Debug, Serialize, Deserialize)]
pub struct SelectCommand {
  pub atom_ids: Vec<u64>,
  pub bond_references: Vec<BondReference>,
  pub unselect: bool, // whether this is an unselect or select command
}

impl SelectCommand {
  pub fn new(atom_ids: Vec<u64>, bond_references: Vec<BondReference>, unselect: bool) -> Self {
      Self {
        atom_ids,
        bond_references,
        unselect
      }
  }
}

impl EditAtomCommand for SelectCommand {
  fn execute(&self, model: &mut AtomicStructure) {
    model.select(&self.atom_ids, &self.bond_references, self.unselect);
  }
}
