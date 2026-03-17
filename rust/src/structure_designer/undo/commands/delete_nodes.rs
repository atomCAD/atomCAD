use crate::structure_designer::node_network::NodeDisplayType;
use crate::structure_designer::undo::snapshot::{NodeSnapshot, WireSnapshot};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing deletion of nodes.
#[derive(Debug)]
pub struct DeleteNodesCommand {
    pub network_name: String,
    /// Full snapshot of each deleted node
    pub deleted_nodes: Vec<NodeSnapshot>,
    /// All wires that were removed (both wires between deleted nodes and
    /// wires connecting deleted nodes to surviving nodes)
    pub deleted_wires: Vec<WireSnapshot>,
    /// Was the return node among the deleted?
    pub was_return_node: Option<u64>,
    /// Display state of deleted nodes
    pub display_states: Vec<(u64, NodeDisplayType)>,
    pub description: String,
}

impl DeleteNodesCommand {
    fn restore_nodes(&self, ctx: &mut UndoContext) {
        // Load all node data first (needs registry access)
        let node_data_vec: Vec<Option<Box<dyn crate::structure_designer::node_data::NodeData>>> =
            self.deleted_nodes
                .iter()
                .map(|snap| {
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
                        return None;
                    };
                    loader(&snap.node_data_json, None).ok()
                })
                .collect();

        if let Some(network) = ctx.network_mut(&self.network_name) {
            // Re-add all deleted nodes
            for (snap, data_opt) in self.deleted_nodes.iter().zip(node_data_vec) {
                let data = match data_opt {
                    Some(d) => d,
                    None => continue,
                };

                network.add_node_with_id(
                    snap.node_id,
                    &snap.node_type_name,
                    snap.position,
                    snap.arguments.len(),
                    data,
                );

                // Restore custom name and arguments
                if let Some(node) = network.nodes.get_mut(&snap.node_id) {
                    node.custom_name = snap.custom_name.clone();
                    // Restore argument connections
                    for (i, arg_snap) in snap.arguments.iter().enumerate() {
                        if let Some(arg) = node.arguments.get_mut(i) {
                            arg.argument_output_pins = arg_snap.argument_output_pins.clone();
                        }
                    }
                }
            }

            // Re-establish all wires (for connections to surviving nodes)
            for wire in &self.deleted_wires {
                if let Some(dest_node) = network.nodes.get_mut(&wire.dest_node_id) {
                    if let Some(arg) = dest_node.arguments.get_mut(wire.dest_param_index) {
                        arg.argument_output_pins
                            .insert(wire.source_node_id, wire.source_output_pin_index);
                    }
                }
            }

            // Restore return node
            if let Some(return_id) = self.was_return_node {
                network.return_node_id = Some(return_id);
            }

            // Restore display states
            for &(node_id, display_type) in &self.display_states {
                network.displayed_node_ids.insert(node_id, display_type);
            }
        }
    }

    fn delete_nodes(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            let node_ids: Vec<u64> = self.deleted_nodes.iter().map(|s| s.node_id).collect();

            for &node_id in &node_ids {
                // Remove references to this node from all other nodes' arguments
                let all_node_ids: Vec<u64> = network.nodes.keys().copied().collect();
                for other_id in all_node_ids {
                    if let Some(node) = network.nodes.get_mut(&other_id) {
                        for arg in node.arguments.iter_mut() {
                            arg.argument_output_pins.remove(&node_id);
                        }
                    }
                }

                // Clear return node if needed
                if network.return_node_id == Some(node_id) {
                    network.return_node_id = None;
                }

                // Remove from displayed nodes
                network.displayed_node_ids.remove(&node_id);

                // Remove the node
                network.nodes.remove(&node_id);
            }

            // Clean up selection/active state
            network.cleanup_selection_for_removed_nodes(&node_ids);
        }
    }
}

impl UndoCommand for DeleteNodesCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.restore_nodes(ctx);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.delete_nodes(ctx);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
