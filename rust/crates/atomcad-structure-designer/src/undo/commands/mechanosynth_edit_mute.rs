//! The undo command behind muting and unmuting a `mechanosynth_edit` node's
//! operations (`doc/design_mechanosynth_op_muting.md`).
//!
//! **Separate from [`MechanosynthEditBlockCommand`] on purpose.** That command
//! stores `(authored, cursor)` and is the single restore path for four editors
//! of that pair; muting touches neither. Folding a third field in would make
//! every block edit carry a copy of the mute set and every mute carry a copy of
//! the block — two independent pieces of state, so two commands.
//!
//! The whole set is stored on each side rather than the names that changed,
//! because a group chip mutes a dozen operations in one user action and one
//! `Ctrl+Z` has to put all twelve back.
//!
//! [`MechanosynthEditBlockCommand`]:
//!     crate::undo::commands::mechanosynth_edit_block::MechanosynthEditBlockCommand

use crate::nodes::mechanosynth_edit::MechanosynthEditData;
use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};
use std::collections::BTreeSet;

#[derive(Debug)]
pub struct MechanosynthEditMuteCommand {
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub description: String,
    pub before: BTreeSet<String>,
    pub after: BTreeSet<String>,
}

impl MechanosynthEditMuteCommand {
    fn apply(&self, ctx: &mut UndoContext, state: &BTreeSet<String>) {
        let Some(network) = ctx.network_in_scope_mut(&self.network_name, &self.scope_path) else {
            return;
        };
        let Some(node) = network.nodes.get_mut(&self.node_id) else {
            return;
        };
        let Some(data) = node
            .data
            .as_any_mut()
            .downcast_mut::<MechanosynthEditData>()
        else {
            return;
        };
        data.muted = state.clone();
        // **The placement state is deliberately not reset**, which is where
        // this differs from every other command touching this node. They reset
        // because they change the workpiece the open candidates were fitted
        // against; muting changes no fit, so the rows stay exactly as valid as
        // they were. Dropping them would close a popup the user is reading, for
        // a change that did not invalidate anything in it.
        //
        // This reaches into the node's data directly rather than through a
        // refresh, so it owes the input cache the invalidation the refresh
        // system would otherwise have done. **The cache is all it owes**: muting
        // changes no pin, so nothing downstream can have gone stale.
        data.invalidate_input_cache();
    }
}

impl UndoCommand for MechanosynthEditMuteCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, &self.before);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, &self.after);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::NodeDataChanged(vec![self.node_id])
    }
}
