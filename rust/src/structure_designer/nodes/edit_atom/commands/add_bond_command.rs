use crate::structure_designer::nodes::edit_atom::edit_atom_command::EditAtomCommand;
use crate::common::atomic_structure::AtomicStructure;
use serde::{Serialize, Deserialize};

/*
 * Command to add a bond between the given atoms with the given multiplicity (1-3).
 */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddBondCommand {
  pub atom_id1: u32,
  pub atom_id2: u32,
  pub multiplicity: i32,
}

impl AddBondCommand {
  pub fn new(atom_id1: u32, atom_id2: u32, multiplicity: i32) -> Self {
      Self { atom_id1, atom_id2, multiplicity }
  }
}

impl EditAtomCommand for AddBondCommand {
  fn execute(&self, model: &mut AtomicStructure) {
    let bond_id = model.obtain_next_bond_id();
    model.add_bond_with_id(bond_id, self.atom_id1, self.atom_id2, self.multiplicity);
  }

  fn clone_box(&self) -> Box<dyn EditAtomCommand> {
    Box::new(self.clone())
  }
}
