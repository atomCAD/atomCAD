use crate::common::atomic_structure::AtomicStructure;
use crate::util::as_any::AsAny;

pub trait EditAtomCommand : AsAny { 
  fn execute(&mut self, model: &mut AtomicStructure, is_redo: bool);
  fn undo(&mut self, model: &mut AtomicStructure);
}
