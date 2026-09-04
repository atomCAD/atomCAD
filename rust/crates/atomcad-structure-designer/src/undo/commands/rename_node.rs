use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Undo/redo for a GUI rename of a node's `custom_name`
/// (`doc/design_node_names_in_ui.md` D3).
///
/// Id-keyed, like every other node command: a name is a label, never an
/// identity. `scope_path` resolves the (possibly nested) body the node lives
/// in, resolved through `ctx.network_in_scope_mut` exactly as
/// `SetCollapseModeCommand` does.
///
/// `old_name` is an `Option` because `Node::custom_name` is one; in practice
/// every node has a name (`add_node` mints one, the loader backfills), but the
/// command restores whatever was actually there.
#[derive(Debug)]
pub struct RenameNodeCommand {
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub old_name: Option<String>,
    pub new_name: String,
}

impl RenameNodeCommand {
    fn apply(&self, ctx: &mut UndoContext, name: Option<String>) {
        if let Some(network) = ctx.network_in_scope_mut(&self.network_name, &self.scope_path)
            && let Some(node) = network.nodes.get_mut(&self.node_id)
        {
            node.custom_name = name;
        }
    }
}

impl UndoCommand for RenameNodeCommand {
    fn description(&self) -> &str {
        "Rename node"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, self.old_name.clone());
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, Some(self.new_name.clone()));
    }

    /// A name is a label: nothing evaluated depends on it, so the undo needs a
    /// fresh view and no re-evaluation (same reasoning as the collapse mode).
    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Lightweight
    }
}
