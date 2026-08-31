use crate::nodes::comment::{CommentAnchor, CommentData};
use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Undo/redo for one comment node's anchor list
/// (`doc/design_wire_annotations.md` §"Undo").
///
/// `scope_path` identifies the body the comment lives in (empty = top-level
/// `network_name`), resolved via `ctx.network_in_scope_mut` like
/// `SetFunctionPinRoleCommand`.
///
/// The refresh mode is `Lightweight` in **every** scope, unlike the display and
/// function-pin commands: anchors are inert with respect to evaluation, type
/// resolution, validation, dirty propagation and memoization (D7), so there is
/// nothing to re-evaluate — only a leader line to repaint.
///
/// Two situations produce one of these:
///
/// 1. The user sets or clears a comment's anchors directly.
/// 2. A **deletion elsewhere** orphans them. The comments whose anchors a
///    deletion clears are, by definition, the ones *not* in the delete set, so
///    `DeleteNodesCommand` / `DeleteWiresCommand` never snapshot them — without
///    this command bundled alongside, undo would restore the wire but not the
///    association. See `StructureDesigner::delete_selected_scoped`.
#[derive(Debug)]
pub struct SetCommentAnchorsCommand {
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub old_anchors: Vec<CommentAnchor>,
    pub new_anchors: Vec<CommentAnchor>,
    pub description: String,
}

impl SetCommentAnchorsCommand {
    fn apply(&self, ctx: &mut UndoContext, anchors: &[CommentAnchor]) {
        if let Some(network) = ctx.network_in_scope_mut(&self.network_name, &self.scope_path)
            && let Some(node) = network.nodes.get_mut(&self.node_id)
            && let Some(comment) = node.data.as_any_mut().downcast_mut::<CommentData>()
        {
            comment.anchors = anchors.to_vec();
        }
    }
}

impl UndoCommand for SetCommentAnchorsCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, &self.old_anchors);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, &self.new_anchors);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Lightweight
    }
}
