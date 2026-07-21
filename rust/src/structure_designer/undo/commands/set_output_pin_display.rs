use crate::structure_designer::node_network::NodeDisplayState;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing toggling an output pin's display state.
///
/// Stores the full `NodeDisplayState` before and after the change, so that
/// undo/redo restores the exact pin set and display type atomically.
///
/// `scope_path` addresses the network the node lives in: empty = the named
/// top-level network, non-empty = the chain of zone-owning node ids down to a
/// body (per-pin toggles inside a 0-ary closure body — see
/// `doc/design_zero_ary_closure_body_display.md` §5).
#[derive(Debug)]
pub struct SetOutputPinDisplayCommand {
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub old_display_state: Option<NodeDisplayState>,
    pub new_display_state: Option<NodeDisplayState>,
    pub description: String,
}

impl SetOutputPinDisplayCommand {
    fn apply(&self, ctx: &mut UndoContext, state: &Option<NodeDisplayState>) {
        if let Some(network) = ctx.network_in_scope_mut(&self.network_name, &self.scope_path) {
            match state {
                Some(state) => {
                    network.displayed_nodes.insert(self.node_id, state.clone());
                }
                None => {
                    network.displayed_nodes.remove(&self.node_id);
                }
            }
        }
    }
}

impl UndoCommand for SetOutputPinDisplayCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, &self.old_display_state);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, &self.new_display_state);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        if self.scope_path.is_empty() {
            // Top-level: unchanged behavior.
            UndoRefreshMode::Lightweight
        } else {
            // A body toggle adds, removes or re-pins a *scoped* scene entry,
            // and neither `Lightweight` (no re-evaluation at all) nor
            // `NodeDataChanged` (bare `u64` ids, top-level only) can express
            // that. `Full` re-derives the displayed set from scratch — the
            // eligibility-gated collection this feature keys off. Same
            // reasoning as `SetNodeDisplayCommand`.
            UndoRefreshMode::Full
        }
    }
}
