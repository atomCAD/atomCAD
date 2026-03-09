use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing network rename.
#[derive(Debug)]
pub struct RenameNetworkCommand {
    pub old_name: String,
    pub new_name: String,
}

impl RenameNetworkCommand {
    /// Perform a rename from `from` to `to` within the UndoContext.
    /// This replicates the core rename logic from StructureDesigner::rename_node_network
    /// but operates on the UndoContext (no navigation history or clipboard access).
    fn do_rename(from: &str, to: &str, ctx: &mut UndoContext) {
        // Take the network out and re-insert with new name
        let mut network = match ctx.node_type_registry.node_networks.remove(from) {
            Some(n) => n,
            None => return,
        };
        network.node_type.name = to.to_string();
        ctx.node_type_registry
            .node_networks
            .insert(to.to_string(), network);

        // Update active network name if it was the renamed network
        if ctx.active_network_name.as_deref() == Some(from) {
            *ctx.active_network_name = Some(to.to_string());
        }

        // Update all nodes in all networks that reference the old type name
        for network in ctx.node_type_registry.node_networks.values_mut() {
            for node in network.nodes.values_mut() {
                if node.node_type_name == from {
                    node.node_type_name = to.to_string();
                }
            }
        }

        // Update backtick references in comment nodes and network metadata
        let old_pattern = format!("`{}`", from);
        let new_pattern = format!("`{}`", to);
        for network in ctx.node_type_registry.node_networks.values_mut() {
            if network.node_type.description.contains(&old_pattern) {
                network.node_type.description = network
                    .node_type
                    .description
                    .replace(&old_pattern, &new_pattern);
            }
            if let Some(ref mut summary) = network.node_type.summary {
                if summary.contains(&old_pattern) {
                    *summary = summary.replace(&old_pattern, &new_pattern);
                }
            }

            for node in network.nodes.values_mut() {
                if node.node_type_name == "Comment" {
                    if let Some(comment_data) = node
                        .data
                        .as_any_mut()
                        .downcast_mut::<crate::structure_designer::nodes::comment::CommentData>()
                    {
                        if comment_data.label.contains(&old_pattern) {
                            comment_data.label =
                                comment_data.label.replace(&old_pattern, &new_pattern);
                        }
                        if comment_data.text.contains(&old_pattern) {
                            comment_data.text =
                                comment_data.text.replace(&old_pattern, &new_pattern);
                        }
                    }
                }
            }
        }
    }
}

impl UndoCommand for RenameNetworkCommand {
    fn description(&self) -> &str {
        "Rename network"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        Self::do_rename(&self.new_name, &self.old_name, ctx);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        Self::do_rename(&self.old_name, &self.new_name, ctx);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
