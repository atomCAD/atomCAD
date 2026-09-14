//! The one undo command behind every `mechanosynth_edit` block edit.
//!
//! Insert, delete, move and metadata-edit all change the same two things — the
//! authored block and the cursor — and all of them restore by replacing that
//! pair wholesale. Four structs with identical fields and identical `undo`
//! bodies would say nothing the `description` does not already say, so there is
//! one command and four constructors on `StructureDesigner`
//! (`mechanosynth_edit_ops.rs`).
//!
//! **The cursor rides along with the block, but is never a command of its own.**
//! Deleting the cursor step has to move the cursor, and undoing that deletion
//! has to put it back; scrubbing has to leave the stack alone. Storing the pair
//! is what makes both true.
//!
//! `coalesce` is the key consecutive metadata edits to the same field of the
//! same step merge on, so typing into a chip leaves one undo entry rather than
//! one per keystroke. It is `None` for every structural edit — those never
//! merge.

use crate::nodes::mechanosynth_edit::{AuthoredStep, MechanosynthEditData};
use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Which field of which step a metadata edit touched — the identity two
/// consecutive edits must share to be one undo entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataEditKey {
    pub node_id: u64,
    pub scope_path: Vec<u64>,
    pub step_index: usize,
    pub field: String,
}

#[derive(Debug)]
pub struct MechanosynthEditBlockCommand {
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub description: String,
    pub before: (Vec<AuthoredStep>, i32),
    pub after: (Vec<AuthoredStep>, i32),
    /// `Some` for a metadata edit, and the key consecutive edits coalesce on.
    pub coalesce: Option<MetadataEditKey>,
}

impl MechanosynthEditBlockCommand {
    fn apply(&self, ctx: &mut UndoContext, state: &(Vec<AuthoredStep>, i32)) {
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
        data.authored = state.0.clone();
        data.cursor = state.1;
        // The tool's pending candidates were computed against a workpiece the
        // restored block no longer produces, so they are stale by construction.
        data.placement.clear_query();
    }
}

impl UndoCommand for MechanosynthEditBlockCommand {
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
