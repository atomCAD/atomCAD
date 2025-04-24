use crate::scene_composer::scene_composer_model::SceneComposerModel;
use crate::scene_composer::commands::scene_composer_command::SceneComposerCommand;

pub struct SetFrameLockedToAtomsCommand {
  pub locked: bool,
  
  // undo information
  pub previous_locked: bool,
}

impl SetFrameLockedToAtomsCommand {
  pub fn new(locked: bool) -> Self {
    Self { locked, previous_locked: true }
  }
}

impl SceneComposerCommand for SetFrameLockedToAtomsCommand {
  fn execute(&mut self, model: &mut SceneComposerModel, is_redo: bool) {
    if !is_redo {
        self.previous_locked = model.is_frame_locked_to_atoms();
    }
    model.set_frame_locked_to_atoms(self.locked); 
  }

  fn undo(&mut self, model: &mut SceneComposerModel) {
    model.set_frame_locked_to_atoms(self.previous_locked);
  }
}
