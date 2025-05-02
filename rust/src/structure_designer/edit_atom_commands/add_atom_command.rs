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
}

impl AddAtomCommand {
  pub fn new(atomic_number: i32, position: DVec3) -> Self {
      Self { atomic_number, position }
  }
}

impl EditAtomCommand for AddAtomCommand {
  fn execute(&self, model: &mut AtomicStructure) {
    let atom_id = model.obtain_next_atom_id();
    model.add_atom_with_id(atom_id, self.atomic_number, self.position, 1);
  }
}
