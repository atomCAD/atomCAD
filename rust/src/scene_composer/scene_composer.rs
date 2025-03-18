use crate::common::atomic_structure::AtomicStructure;
use crate::common::scene::StructureDesignerScene;

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
  pub fn generate_scene(&mut self) -> StructureDesignerScene {
    let mut scene: StructureDesignerScene = StructureDesignerScene::new();  
    return scene;
  }
}
