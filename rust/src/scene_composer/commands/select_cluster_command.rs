
use crate::api::common_api_types::SelectModifier;
use std::collections::HashSet;
use crate::scene_composer::scene_composer_model::SceneComposerModel;
use crate::scene_composer::commands::scene_composer_command::SceneComposerCommand;

pub struct SelectClusterCommand {
  pub cluster_id: u64,
  pub select_modifier: SelectModifier,
  
  // undo information
  // The clusters that were selected/deselected by this command
  pub inverted_cluster_selections: HashSet<u64>,
}

impl SelectClusterCommand {
  pub fn new(cluster_id: u64, select_modifier: SelectModifier) -> Self {
    Self { cluster_id, select_modifier, inverted_cluster_selections: HashSet::new() }
  }
}

impl SceneComposerCommand for SelectClusterCommand {
  fn execute(&mut self, model: &mut SceneComposerModel, is_redo: bool) {
    let inverted_cluster_ids = model.select_cluster_by_id(self.cluster_id, self.select_modifier.clone());
    if !is_redo {
      self.inverted_cluster_selections = inverted_cluster_ids;
    }
  }

  fn undo(&mut self, model: &mut SceneComposerModel) {
    model.invert_cluster_selections(&self.inverted_cluster_selections);
  }
}
