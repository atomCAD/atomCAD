use super::super::edit_atom_command::EditAtomCommand;
use crate::common::atomic_structure::AtomicStructure;
use crate::common::atomic_structure::BondReference;
use serde::{Serialize, Deserialize};
use crate::api::common_api_types::SelectModifier;

/*
 * A selection command.
 */
#[derive(Debug, Serialize, Deserialize)]
pub struct SelectCommand {
  pub atom_ids: Vec<u64>,
  pub bond_references: Vec<BondReference>,
  pub select_modifier: SelectModifier,
}

impl SelectCommand {
  pub fn new(atom_ids: Vec<u64>, bond_references: Vec<BondReference>, select_modifier: SelectModifier) -> Self {
      Self {
        atom_ids,
        bond_references,
        select_modifier,
      }
  }
}

impl EditAtomCommand for SelectCommand {
  fn execute(&self, model: &mut AtomicStructure) {
    model.select(&self.atom_ids, &self.bond_references, self.select_modifier.clone());
  }
}
