use super::atomic_structure::AtomicStructure;
use super::as_any::AsAny;

pub trait Command : AsAny { 
  fn execute(&mut self, model: &mut AtomicStructure, is_redo: bool);
  fn undo(&mut self, model: &mut AtomicStructure);
}
