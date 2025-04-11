use crate::util::as_any::AsAny;
use crate::scene_composer::scene_composer::SceneComposer;

pub trait SceneComposerCommand : AsAny { 
  fn execute(&mut self, scene_composer: &mut SceneComposer, is_redo: bool);
  fn undo(&mut self, scene_composer: &mut SceneComposer);
}
