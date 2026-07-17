use crate::structure_designer::node_network::FunctionPinRole;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Undo/redo for overriding one input pin's role in a node's `-1` function-pin
/// view. `scope_path` identifies the body the node lives in (empty = top-level
/// `network_name`), resolved via `ctx.network_in_scope_mut` like
/// `SetCollapseModeCommand`.
///
/// `None` means "no entry" — the map's canonical representation of
/// `FunctionPinRole::Auto` (it never stores an explicit `Auto`), so the
/// `Option`s here mirror entry presence exactly. See
/// `doc/design_function_pin_roles.md` §"Undo".
#[derive(Debug)]
pub struct SetFunctionPinRoleCommand {
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub pin_index: usize,
    pub old_role: Option<FunctionPinRole>,
    pub new_role: Option<FunctionPinRole>,
    pub description: String,
}

impl SetFunctionPinRoleCommand {
    fn apply(&self, ctx: &mut UndoContext, role: Option<FunctionPinRole>) {
        if let Some(network) = ctx.network_in_scope_mut(&self.network_name, &self.scope_path)
            && let Some(node) = network.nodes.get_mut(&self.node_id)
        {
            match role {
                Some(r) => {
                    node.function_pin_roles.insert(self.pin_index, r);
                }
                None => {
                    node.function_pin_roles.remove(&self.pin_index);
                }
            }
        }
    }
}

impl UndoCommand for SetFunctionPinRoleCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, self.old_role);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, self.new_role);
    }

    /// The refresh mode depends on the scope, because **both legs of the
    /// `NodeDataChanged` path are top-level-only**: `mark_node_data_changed`
    /// marks `NodeRef::top(node_id)`, and the arm's conditional revalidation
    /// checks `function_pin_consumed` against the *active top-level* network.
    ///
    /// - Top level → `NodeDataChanged`. The arm re-validates when the node's
    ///   function pin is consumed, which — since the `Supplied`-required warning
    ///   is itself gated on consumption — covers every validation-visible effect
    ///   of the toggle.
    /// - Inside a body → `Full`. For a body node the `NodeDataChanged` mode
    ///   would dirty the *wrong* node on an id collision (per-body
    ///   `next_node_id` counters make collisions routine) and would skip the
    ///   revalidation a body-internal `-1` consumer needs. `Full` re-validates
    ///   recursively (`validate_zones_recursive`) and reapplies display policy,
    ///   matching what the forward setter's `validate_active_network()`
    ///   produced. Body role edits are rare enough that the blunter refresh is
    ///   fine.
    fn refresh_mode(&self) -> UndoRefreshMode {
        if self.scope_path.is_empty() {
            UndoRefreshMode::NodeDataChanged(vec![self.node_id])
        } else {
            UndoRefreshMode::Full
        }
    }
}
