use glam::f64::DVec3;
use crate::structure_designer::nodes::edit_atom::edit_atom_command::EditAtomCommand;
use crate::crystolecule::atomic_structure::AtomicStructure;
use serde::{Serialize, Deserialize};
use crate::util::serialization_utils::dvec3_serializer;

/*
 * Command to add an atom with the given atomic number and position.
 */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddAtomCommand {
  pub atomic_number: i16,
  #[serde(with = "dvec3_serializer")]
  pub position: DVec3,
}

impl AddAtomCommand {
  pub fn new(atomic_number: i16, position: DVec3) -> Self {
      Self { atomic_number, position }
  }
}

impl EditAtomCommand for AddAtomCommand {
  fn execute(&self, model: &mut AtomicStructure) {
    model.add_atom(self.atomic_number, self.position);
  }

  fn clone_box(&self) -> Box<dyn EditAtomCommand> {
    Box::new(self.clone())
  }
}
