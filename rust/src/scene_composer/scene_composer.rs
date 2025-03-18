use crate::common::atomic_structure::AtomicStructure;
use crate::common::scene::Scene;

pub struct SceneComposer {
    pub model: AtomicStructure,
}

impl SceneComposer {
  pub fn new() -> Self {
    Self {
      model: AtomicStructure::new(),
    }
  }

  // Generates the scene to be rendered
  pub fn generate_scene(&mut self) -> Scene {
    let mut scene: Scene = Scene::new();  
    return scene;
  }
}
