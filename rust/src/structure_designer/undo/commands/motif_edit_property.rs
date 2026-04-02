use crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

// =============================================================================
// MotifEditSetParameterElementsCommand
// =============================================================================

/// Command for undoing/redoing changes to motif_edit parameter_elements.
/// Stores the full vec before and after the change.
pub struct MotifEditSetParameterElementsCommand {
    pub description: String,
    pub network_name: String,
    pub node_id: u64,
    pub old_value: Vec<(String, i16)>,
    pub new_value: Vec<(String, i16)>,
}

impl std::fmt::Debug for MotifEditSetParameterElementsCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MotifEditSetParameterElementsCommand")
            .field("description", &self.description)
            .field("network_name", &self.network_name)
            .field("node_id", &self.node_id)
            .finish()
    }
}

impl UndoCommand for MotifEditSetParameterElementsCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        set_parameter_elements(ctx, &self.network_name, self.node_id, &self.old_value);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        set_parameter_elements(ctx, &self.network_name, self.node_id, &self.new_value);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::NodeDataChanged(vec![self.node_id])
    }
}

fn set_parameter_elements(
    ctx: &mut UndoContext,
    network_name: &str,
    node_id: u64,
    value: &[(String, i16)],
) {
    if let Some(data) = get_atom_edit_data_mut(ctx, network_name, node_id) {
        data.parameter_elements = value.to_vec();
    }
}

// =============================================================================
// MotifEditSetNeighborDepthCommand
// =============================================================================

/// Command for undoing/redoing changes to motif_edit neighbor_depth.
#[derive(Debug)]
pub struct MotifEditSetNeighborDepthCommand {
    pub network_name: String,
    pub node_id: u64,
    pub old_value: f64,
    pub new_value: f64,
}

impl UndoCommand for MotifEditSetNeighborDepthCommand {
    fn description(&self) -> &str {
        "Set neighbor depth"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        set_neighbor_depth(ctx, &self.network_name, self.node_id, self.old_value);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        set_neighbor_depth(ctx, &self.network_name, self.node_id, self.new_value);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::NodeDataChanged(vec![self.node_id])
    }
}

fn set_neighbor_depth(ctx: &mut UndoContext, network_name: &str, node_id: u64, value: f64) {
    if let Some(data) = get_atom_edit_data_mut(ctx, network_name, node_id) {
        data.neighbor_depth = value;
    }
}

// =============================================================================
// Shared helper
// =============================================================================

fn get_atom_edit_data_mut<'a>(
    ctx: &'a mut UndoContext,
    network_name: &str,
    node_id: u64,
) -> Option<&'a mut AtomEditData> {
    let network = ctx.network_mut(network_name)?;
    let node = network.nodes.get_mut(&node_id)?;
    node.data.as_mut().as_any_mut().downcast_mut()
}
