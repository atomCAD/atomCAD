use crate::common::atomic_structure::AtomicStructure;
use crate::util::as_any::AsAny;

pub trait EditAtomCommand : AsAny { 
  fn execute(&self, model: &mut AtomicStructure);
}
