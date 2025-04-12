use crate::util::as_any::AsAny;
use crate::scene_composer::scene_composer_model::SceneComposerModel;

pub trait SceneComposerCommand : AsAny { 
  fn execute(&mut self, model: &mut SceneComposerModel, is_redo: bool);
  fn undo(&mut self, model: &mut SceneComposerModel);
}
