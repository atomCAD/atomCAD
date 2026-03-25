use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};
use glam::f64::DVec2;
use serde_json::Value;

/// Command for undoing/redoing node addition.
#[derive(Debug)]
pub struct AddNodeCommand {
    pub description: String,
    pub network_name: String,
    pub node_id: u64,
    pub node_type_name: String,
    pub position: DVec2,
    pub node_data_json: Value,
    pub custom_name: Option<String>,
    pub num_parameters: usize,
    /// For parameter nodes: the assigned param_id
    pub param_id: Option<u64>,
    /// To restore network.next_param_id on undo
    pub next_param_id_before: u64,
    /// To restore network.next_node_id on undo
    pub next_node_id_before: u64,
}

impl AddNodeCommand {
    /// Look up the node_data_loader for this node type, deserialize the JSON data.
    fn load_node_data(
        &self,
        ctx: &mut UndoContext,
    ) -> Option<Box<dyn crate::structure_designer::node_data::NodeData>> {
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
            return None;
        };

        loader(&self.node_data_json, None).ok()
    }
}

impl UndoCommand for AddNodeCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        if let Some(network) = ctx.network_mut(&self.network_name) {
            // Remove references to this node from all other nodes' arguments
            let node_ids: Vec<u64> = network.nodes.keys().copied().collect();
            for other_id in node_ids {
                if let Some(node) = network.nodes.get_mut(&other_id) {
                    for arg in node.arguments.iter_mut() {
                        arg.argument_output_pins.remove(&self.node_id);
                    }
                }
            }

            // Remove from displayed nodes
            network.displayed_nodes.remove(&self.node_id);

            // Remove the node
            network.nodes.remove(&self.node_id);

            // Clean up selection/active state
            network.cleanup_selection_for_removed_nodes(&[self.node_id]);

            // Restore next_node_id
            network.next_node_id = self.next_node_id_before;

            // Restore next_param_id for parameter nodes
            if self.param_id.is_some() {
                network.next_param_id = self.next_param_id_before;
            }
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        let node_data = match self.load_node_data(ctx) {
            Some(mut data) => {
                // For parameter nodes, restore the param_id
                if let Some(param_id) = self.param_id {
                    if let Some(param_data) = data
                        .as_any_mut()
                        .downcast_mut::<crate::structure_designer::nodes::parameter::ParameterData>()
                    {
                        param_data.param_id = Some(param_id);
                    }
                }
                data
            }
            None => return,
        };

        if let Some(network) = ctx.network_mut(&self.network_name) {
            network.add_node_with_id(
                self.node_id,
                &self.node_type_name,
                self.position,
                self.num_parameters,
                node_data,
            );

            // Restore custom name
            if let Some(node) = network.nodes.get_mut(&self.node_id) {
                node.custom_name = self.custom_name.clone();
            }

            // Update next_param_id for parameter nodes
            if let Some(param_id) = self.param_id {
                if network.next_param_id <= param_id {
                    network.next_param_id = param_id + 1;
                }
            }
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
