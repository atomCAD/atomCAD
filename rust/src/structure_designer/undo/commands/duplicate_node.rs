use crate::structure_designer::undo::snapshot::NodeSnapshot;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing node duplication.
#[derive(Debug)]
pub struct DuplicateNodeCommand {
    pub network_name: String,
    /// ID of the new duplicate node
    pub new_node_id: u64,
    /// Full snapshot of the duplicated node (for redo re-creation)
    pub node_snapshot: NodeSnapshot,
    /// To restore network.next_node_id on undo
    pub next_node_id_before: u64,
    pub description: String,
}

impl UndoCommand for DuplicateNodeCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Remove the duplicated node
        if let Some(network) = ctx.network_mut(&self.network_name) {
            // Remove references to this node from all other nodes' arguments
            let node_ids: Vec<u64> = network.nodes.keys().copied().collect();
            for other_id in node_ids {
                if let Some(node) = network.nodes.get_mut(&other_id) {
                    for arg in node.arguments.iter_mut() {
                        arg.argument_output_pins.remove(&self.new_node_id);
                    }
                }
            }

            // Remove from displayed nodes
            network.displayed_node_ids.remove(&self.new_node_id);

            // Remove the node
            network.nodes.remove(&self.new_node_id);

            // Clean up selection/active state
            network.cleanup_selection_for_removed_nodes(&[self.new_node_id]);

            // Restore next_node_id
            network.next_node_id = self.next_node_id_before;
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        let snap = &self.node_snapshot;

        // Load node data
        let loader = if let Some(node_type) = ctx
            .node_type_registry
            .built_in_node_types
            .get(&snap.node_type_name)
        {
            node_type.node_data_loader
        } else if let Some(network) = ctx
            .node_type_registry
            .node_networks
            .get(&snap.node_type_name)
        {
            network.node_type.node_data_loader
        } else {
            return;
        };

        let data = match loader(&snap.node_data_json, None) {
            Ok(d) => d,
            Err(_) => return,
        };

        if let Some(network) = ctx.network_mut(&self.network_name) {
            network.add_node_with_id(
                snap.node_id,
                &snap.node_type_name,
                snap.position,
                snap.arguments.len(),
                data,
            );

            // add_node_with_id always displays the node, but duplicate_node
            // does not set display state (it's handled by the display policy).
            // Remove from displayed to match the original behavior.
            network.displayed_node_ids.remove(&snap.node_id);

            // Restore custom name and arguments
            if let Some(node) = network.nodes.get_mut(&snap.node_id) {
                node.custom_name = snap.custom_name.clone();
                for (i, arg_snap) in snap.arguments.iter().enumerate() {
                    if let Some(arg) = node.arguments.get_mut(i) {
                        arg.argument_output_pins = arg_snap.argument_output_pins.clone();
                    }
                }
            }
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
