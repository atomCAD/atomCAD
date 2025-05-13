use crate::structure_designer::nodes::edit_atom::edit_atom_command::EditAtomCommand;
use crate::common::atomic_structure::AtomicStructure;
use serde::{Serialize, Deserialize};
use crate::util::transform::Transform;

/*
 * A selection command.
 */
#[derive(Debug, Serialize, Deserialize)]
pub struct TransformCommand {
  pub relative_transform: Transform,
}

impl TransformCommand {
  pub fn new(relative_transform: Transform) -> Self {
      Self {
        relative_transform,
      }
  }
}

impl EditAtomCommand for TransformCommand {
  fn execute(&self, model: &mut AtomicStructure) {
    // Get all selected atom IDs
    let selected_atoms: Vec<u64> = model.atoms
      .iter()
      .filter(|(_, atom)| atom.selected)
      .map(|(id, _)| *id)
      .collect();
    
    // Apply the transform to each selected atom
    for atom_id in selected_atoms {
      model.transform_atom(atom_id, &self.relative_transform.rotation, &self.relative_transform.translation);
    }
    
    // Update selection transform
    model.selection_transform = model.selection_transform.as_ref().map(|t| t.apply_to_new(&self.relative_transform));
  }
}
