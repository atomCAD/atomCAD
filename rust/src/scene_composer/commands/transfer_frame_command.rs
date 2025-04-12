
use crate::scene_composer::scene_composer_model::SceneComposerModel;
use crate::scene_composer::commands::scene_composer_command::SceneComposerCommand;
use crate::util::transform::Transform;

pub struct TransferFrameCommand {
  pub transform: Transform,

  // undo information
  pub previous_transform: Transform,
}

impl TransferFrameCommand {
  pub fn new(transform: Transform) -> Self {
    Self { 
        transform,
        previous_transform: Transform::default()
    }
  }
}

impl SceneComposerCommand for TransferFrameCommand {
  fn execute(&mut self, model: &mut SceneComposerModel, is_redo: bool) {
    if !is_redo {
      self.previous_transform = model.get_selected_frame_transform().unwrap();
    }
    model.set_selected_frame_transform(self.transform.clone());
  }

  fn undo(&mut self, model: &mut SceneComposerModel) {
    model.set_selected_frame_transform(self.previous_transform.clone());
  }
}
