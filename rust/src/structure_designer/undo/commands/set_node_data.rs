use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};
use serde_json::Value;

/// Command for undoing/redoing node data changes.
///
/// Covers all `set_*_data` API calls. Uses JSON snapshots via the
/// registered `node_data_saver`/`node_data_loader` for each node type.
#[derive(Debug)]
pub struct SetNodeDataCommand {
    pub network_name: String,
    pub node_id: u64,
    pub node_type_name: String,
    pub old_data_json: Value,
    pub new_data_json: Value,
    pub description: String,
}

impl SetNodeDataCommand {
    /// Look up the node_data_loader for this node's type from the registry,
    /// deserialize the given JSON, and set it on the node.
    fn apply_data(&self, ctx: &mut UndoContext, data_json: &Value) {
        // Look up the loader function (fn pointer is Copy)
        let loader = if let Some(node_type) = ctx
            .node_type_registry
            .built_in_node_types
            .get(&self.node_type_name)
        {
            node_type.node_data_loader
        } else if let Some(network) = ctx
            .node_type_registry
            .node_networks
            .get(&self.node_type_name)
        {
            network.node_type.node_data_loader
        } else {
            return;
        };

        // Deserialize the data
        let data = match loader(data_json, None) {
            Ok(d) => d,
            Err(_) => return,
        };

        // Set on the node
        if let Some(network) = ctx.network_mut(&self.network_name) {
            network.set_node_network_data(self.node_id, data);
        }
    }
}

impl UndoCommand for SetNodeDataCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.apply_data(ctx, &self.old_data_json);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.apply_data(ctx, &self.new_data_json);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::NodeDataChanged(vec![self.node_id])
    }
}
