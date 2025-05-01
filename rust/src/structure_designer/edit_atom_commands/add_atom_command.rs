use glam::f64::DVec3;
use super::super::edit_atom_command::EditAtomCommand;
use crate::common::atomic_structure::AtomicStructure;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::dvec3_serializer;

/*
 * Command to add an atom with the given atomic number and position.
 */
#[derive(Debug, Serialize, Deserialize)]
pub struct AddAtomCommand {
  pub atomic_number: i32,
  #[serde(with = "dvec3_serializer")]
  pub position: DVec3,

  // undo information
  pub atom_id: u64,
}

impl AddAtomCommand {
  pub fn new(atomic_number: i32, position: DVec3) -> Self {
      Self { atomic_number, position, atom_id: 0 }
  }
}

impl EditAtomCommand for AddAtomCommand {
  fn execute(&mut self, model: &mut AtomicStructure, is_redo: bool) {
    if !is_redo {
      self.atom_id = model.obtain_next_atom_id();
    }
    model.add_atom_with_id(self.atom_id, self.atomic_number, self.position, 1);
  }

  fn undo(&mut self, model: &mut AtomicStructure) {
    model.delete_atom(self.atom_id);
  }
}
