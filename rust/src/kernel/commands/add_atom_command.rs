use glam::f32::Vec3;
use super::super::command::Command;
use super::super::atomic_structure::AtomicStructure;

/*
 * Command to add an atom with the given atomic number and position.
 */
pub struct AddAtomCommand {
  pub atomic_number: i32,
  pub position: Vec3,

  // undo information
  pub atom_id: u64,
}

impl AddAtomCommand {
  pub fn new(atomic_number: i32, position: Vec3) -> Self {
      Self { atomic_number, position, atom_id: 0 }
  }
}

impl Command for AddAtomCommand {
  fn execute(&mut self, model: &mut AtomicStructure, is_redo: bool) {
    if !is_redo {
      self.atom_id = model.obtain_next_id();
    }
    model.add_atom(self.atom_id, self.atomic_number, self.position);
  }

  fn undo(&mut self, model: &mut AtomicStructure) {
    model.delete_atom(self.atom_id);
  }
}
