use crate::structure_designer::nodes::edit_atom::edit_atom_command::EditAtomCommand;
use crate::common::atomic_structure::AtomicStructure;
use crate::common::atomic_structure_utils::calc_selection_transform;
use serde::{Serialize, Deserialize};

/*
 * Delete command: deletes the current selection
 */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteCommand {
}

impl DeleteCommand {
  pub fn new() -> Self {
      Self {
      }
  }
}

impl EditAtomCommand for DeleteCommand {
  fn execute(&self, model: &mut AtomicStructure) {
    // First, collect all selected bond IDs
    let selected_bond_ids: Vec<u64> = model.bonds
      .iter()
      .filter(|(_, bond)| bond.selected)
      .map(|(id, _)| *id)
      .collect();

    // Delete all selected bonds
    for bond_id in selected_bond_ids {
      model.delete_bond(bond_id);
    }

    // Now collect all selected atom IDs
    let selected_atom_ids: Vec<u64> = model.atoms
      .iter()
      .filter(|(_, atom)| atom.selected)
      .map(|(id, _)| *id)
      .collect();

    // Delete all selected atoms
    for atom_id in selected_atom_ids {
      model.delete_atom(atom_id);
    }

    model.selection_transform = calc_selection_transform(model);
  }

  fn clone_box(&self) -> Box<dyn EditAtomCommand> {
    Box::new(self.clone())
  }
}
