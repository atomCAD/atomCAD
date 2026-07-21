use crate::structure_designer::node_network::{NodeDisplayState, NodeDisplayType};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing toggling a node's display state.
///
/// `scope_path` addresses the network the node lives in: empty = the named
/// top-level network, non-empty = the chain of zone-owning node ids down to a
/// body (display toggles inside a 0-ary closure body — see
/// `doc/design_zero_ary_closure_body_display.md` §5).
#[derive(Debug)]
pub struct SetNodeDisplayCommand {
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub old_display_type: Option<NodeDisplayType>,
    pub new_display_type: Option<NodeDisplayType>,
    pub description: String,
}

impl SetNodeDisplayCommand {
    fn apply(&self, ctx: &mut UndoContext, display_type: Option<NodeDisplayType>) {
        if let Some(network) = ctx.network_in_scope_mut(&self.network_name, &self.scope_path) {
            match display_type {
                Some(dt) => {
                    network
                        .displayed_nodes
                        .entry(self.node_id)
                        .and_modify(|s| s.display_type = dt)
                        .or_insert_with(|| NodeDisplayState::with_type(dt));
                }
                None => {
                    network.displayed_nodes.remove(&self.node_id);
                }
            }
        }
    }
}

impl UndoCommand for SetNodeDisplayCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, self.old_display_type);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, self.new_display_type);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        if self.scope_path.is_empty() {
            // Top-level: unchanged behavior.
            UndoRefreshMode::Lightweight
        } else {
            // A body toggle adds or removes a *scoped* scene entry, and neither
            // `Lightweight` (no re-evaluation at all) nor `NodeDataChanged`
            // (bare `u64` ids, top-level only) can express that. `Full`
            // re-derives the displayed set from scratch, which is exactly the
            // eligibility-gated collection this feature keys off.
            UndoRefreshMode::Full
        }
    }
}
