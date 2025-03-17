use crate::common::atomic_structure::AtomicStructure;
use crate::util::as_any::AsAny;

pub trait Command : AsAny { 
  fn execute(&mut self, model: &mut AtomicStructure, is_redo: bool);
  fn undo(&mut self, model: &mut AtomicStructure);
}
