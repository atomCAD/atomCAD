use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Undo/redo for resizing an HOF node's body (its stored `body_width` /
/// `body_height`). One command per resize drag, coalesced via
/// `StructureDesigner::begin_zone_resize` / `end_zone_resize`. `scope_path`
/// identifies the (possibly nested) body the HOF lives in (empty = top-level),
/// resolved via `ctx.network_in_scope_mut` like `SetCollapseModeCommand`. See
/// `doc/design_zones_ui.md` §"Resize handles" / §"Undo/redo".
#[derive(Debug)]
pub struct SetZoneSizeCommand {
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub old_width: f64,
    pub old_height: f64,
    pub new_width: f64,
    pub new_height: f64,
    pub description: String,
}

impl SetZoneSizeCommand {
    fn apply(&self, ctx: &mut UndoContext, width: f64, height: f64) {
        if let Some(network) = ctx.network_in_scope_mut(&self.network_name, &self.scope_path)
            && let Some(node) = network.nodes.get_mut(&self.node_id) {
                node.body_width = width;
                node.body_height = height;
            }
    }
}

impl UndoCommand for SetZoneSizeCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, self.old_width, self.old_height);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, self.new_width, self.new_height);
    }

    /// Body size is presentational (the renderer uses `max(stored, content)`),
    /// so no re-evaluation is needed — only a fresh view, same as node-move.
    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Lightweight
    }
}
