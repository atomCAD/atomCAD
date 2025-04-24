use crate::scene_composer::scene_composer_model::SceneComposerModel;
use crate::scene_composer::commands::scene_composer_command::SceneComposerCommand;
use crate::api::scene_composer_api_types::APISceneComposerTool;

pub struct SetActiveToolCommand {
  pub tool: APISceneComposerTool,
  
  // undo information
  pub previous_tool: APISceneComposerTool,
}

impl SetActiveToolCommand {
  pub fn new(tool: APISceneComposerTool) -> Self {
    Self { tool, previous_tool: APISceneComposerTool::Default }
  }
}

impl SceneComposerCommand for SetActiveToolCommand {
  fn execute(&mut self, model: &mut SceneComposerModel, is_redo: bool) {
    if !is_redo {
        self.previous_tool = model.get_active_tool();
    }
    model.set_active_tool(self.tool.clone()); 
  }

  fn undo(&mut self, model: &mut SceneComposerModel) {
    model.set_active_tool(self.previous_tool.clone());
  }
}
