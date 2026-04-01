use crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing atom_edit tolerance changes.
#[derive(Debug)]
pub struct AtomEditSetToleranceCommand {
    pub network_name: String,
    pub node_id: u64,
    pub old_value: f64,
    pub new_value: f64,
}

impl UndoCommand for AtomEditSetToleranceCommand {
    fn description(&self) -> &str {
        "Set tolerance"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        set_tolerance(ctx, &self.network_name, self.node_id, self.old_value);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        set_tolerance(ctx, &self.network_name, self.node_id, self.new_value);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::NodeDataChanged(vec![self.node_id])
    }
}

fn set_tolerance(ctx: &mut UndoContext, network_name: &str, node_id: u64, value: f64) {
    if let Some(network) = ctx.network_mut(network_name) {
        if let Some(node) = network.nodes.get_mut(&node_id) {
            if let Some(data) = node
                .data
                .as_mut()
                .as_any_mut()
                .downcast_mut::<AtomEditData>()
            {
                data.tolerance = value;
            }
        }
    }
}
