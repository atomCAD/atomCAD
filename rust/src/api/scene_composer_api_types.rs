use crate::api::common_api_types::APIVec3;

#[derive(Clone)]
pub enum SelectModifier {
  Replace,
  Toggle,
  Expand
}

pub struct ClusterView {
    pub id: u64,
    pub name: String,
    pub selected: bool,
}

#[derive(Clone)]
pub enum APISceneComposerTool {
  Default,
  Align,
  AtomInfo,
  Distance,
}

pub struct SceneComposerView {
  pub clusters: Vec<ClusterView>,
  pub active_tool: APISceneComposerTool,
  pub available_tools: Vec<APISceneComposerTool>,
  pub is_undo_available: bool,
  pub is_redo_available: bool,
}

pub struct AtomView {
  pub id: u64,
  pub atomic_number: i32,
  pub symbol: String,
  pub cluster_id: u64,
  pub cluster_name: String,
  pub position: APIVec3,
  pub element_name: String,
  pub covalent_radius: f64,
}
