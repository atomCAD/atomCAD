use crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Which boolean flag on AtomEditData is being toggled.
#[derive(Debug, Clone, Copy)]
pub enum AtomEditFlag {
    OutputDiff,
    ShowAnchorArrows,
    IncludeBaseBondsInDiff,
    ErrorOnStaleEntries,
    ContinuousMinimization,
}

/// Command for undoing/redoing atom_edit boolean flag toggles.
#[derive(Debug)]
pub struct AtomEditToggleFlagCommand {
    pub description: String,
    pub network_name: String,
    pub node_id: u64,
    pub flag: AtomEditFlag,
    pub old_value: bool,
    pub new_value: bool,
}

impl UndoCommand for AtomEditToggleFlagCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        apply_flag(ctx, &self.network_name, self.node_id, self.flag, self.old_value);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        apply_flag(ctx, &self.network_name, self.node_id, self.flag, self.new_value);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::NodeDataChanged(vec![self.node_id])
    }
}

fn get_atom_edit_data_mut<'a>(
    ctx: &'a mut UndoContext,
    network_name: &str,
    node_id: u64,
) -> Option<&'a mut AtomEditData> {
    let network = ctx.network_mut(network_name)?;
    let node = network.nodes.get_mut(&node_id)?;
    node.data.as_mut().as_any_mut().downcast_mut()
}

fn apply_flag(
    ctx: &mut UndoContext,
    network_name: &str,
    node_id: u64,
    flag: AtomEditFlag,
    value: bool,
) {
    match flag {
        AtomEditFlag::OutputDiff => {
            // OutputDiff now controls which pin is displayed rather than a data flag.
            // value=true means diff view (pin 1), value=false means result view (pin 0).
            if let Some(network) = ctx.network_mut(network_name) {
                if value {
                    // Add pin 1 before removing pin 0 to avoid emptying displayed_pins
                    network.set_pin_displayed(node_id, 1, true);
                    network.set_pin_displayed(node_id, 0, false);
                } else {
                    // Add pin 0 before removing pin 1 to avoid emptying displayed_pins
                    network.set_pin_displayed(node_id, 0, true);
                    network.set_pin_displayed(node_id, 1, false);
                }
            }
        }
        _ => {
            if let Some(data) = get_atom_edit_data_mut(ctx, network_name, node_id) {
                match flag {
                    AtomEditFlag::ShowAnchorArrows => data.show_anchor_arrows = value,
                    AtomEditFlag::IncludeBaseBondsInDiff => {
                        data.include_base_bonds_in_diff = value
                    }
                    AtomEditFlag::ErrorOnStaleEntries => data.error_on_stale_entries = value,
                    AtomEditFlag::ContinuousMinimization => {
                        data.continuous_minimization = value
                    }
                    AtomEditFlag::OutputDiff => unreachable!(),
                }
            }
        }
    }
}
