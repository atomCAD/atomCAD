use crate::structure_designer::node_network::CollapseMode;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Undo/redo for changing an HOF node's collapse mode. `scope_path` identifies
/// the body the HOF lives in (empty = top-level `network_name`), resolved via
/// `ctx.network_in_scope_mut` like `SetNodeDataCommand`. See
/// `doc/design_hof_node_collapse.md` §"Undo".
#[derive(Debug)]
pub struct SetCollapseModeCommand {
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub old_mode: CollapseMode,
    pub new_mode: CollapseMode,
    pub description: String,
}

impl SetCollapseModeCommand {
    fn apply(&self, ctx: &mut UndoContext, mode: CollapseMode) {
        if let Some(network) = ctx.network_in_scope_mut(&self.network_name, &self.scope_path)
            && let Some(node) = network.nodes.get_mut(&self.node_id)
        {
            node.collapse_mode = mode;
        }
    }
}

impl UndoCommand for SetCollapseModeCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, self.old_mode);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, self.new_mode);
    }

    /// Collapse is presentational — no re-evaluation is needed, only a fresh
    /// view. `Lightweight` (same as node-move) updates the view without
    /// re-running the network.
    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Lightweight
    }
}
