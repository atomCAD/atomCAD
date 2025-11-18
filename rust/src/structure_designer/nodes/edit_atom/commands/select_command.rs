use crate::structure_designer::nodes::edit_atom::edit_atom_command::EditAtomCommand;
use crate::common::atomic_structure::AtomicStructure;
use crate::common::atomic_structure::BondReference;
use serde::{Serialize, Deserialize};
use crate::api::common_api_types::SelectModifier;
use crate::common::atomic_structure_utils::calc_selection_transform;

/*
 * A selection command.
 */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectCommand {
  pub atom_ids: Vec<u32>,
  pub bond_references: Vec<BondReference>,
  pub select_modifier: SelectModifier,
}

impl SelectCommand {
  pub fn new(atom_ids: Vec<u32>, bond_references: Vec<BondReference>, select_modifier: SelectModifier) -> Self {
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
    model.selection_transform = calc_selection_transform(model);
  }

  fn clone_box(&self) -> Box<dyn EditAtomCommand> {
    Box::new(self.clone())
  }
}
