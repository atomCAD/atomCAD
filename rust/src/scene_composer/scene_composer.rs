use crate::common::atomic_structure::AtomicStructure;

pub struct SceneComposer {
    pub model: AtomicStructure,
}

impl SceneComposer {
  pub fn new() -> Self {
    Self {
      model: AtomicStructure::new(),
    }
  }
}

