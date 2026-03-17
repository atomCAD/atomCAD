use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::util::as_any::AsAny;
use std::fmt::Debug;

pub trait EditAtomCommand: AsAny + Debug {
    fn execute(&self, model: &mut AtomicStructure);
    fn clone_box(&self) -> Box<dyn EditAtomCommand>;
}
