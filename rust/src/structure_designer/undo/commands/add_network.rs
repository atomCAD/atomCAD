use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing network creation.
#[derive(Debug)]
pub struct AddNetworkCommand {
    pub network_name: String,
    /// The active network before this one was added (restored on undo)
    pub previous_active_network: Option<String>,
    /// Ancestor empty-folder markers absorbed (pruned) when this network was
    /// created; restored on undo so the empty folder it filled reappears.
    /// See `doc/design_empty_folders.md`.
    pub pruned_folders: Vec<String>,
}

impl UndoCommand for AddNetworkCommand {
    fn description(&self) -> &str {
        "Add network"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Remove the network
        ctx.node_type_registry
            .node_networks
            .remove(&self.network_name);

        // Restore any empty-folder markers this network's creation absorbed.
        for f in &self.pruned_folders {
            ctx.node_type_registry.folders.insert(f.clone());
        }

        // Restore previous active network
        *ctx.active_network_name = self.previous_active_network.clone();
    }

    fn redo(&self, ctx: &mut UndoContext) {
        use crate::structure_designer::node_data::CustomNodeData;
        use crate::structure_designer::node_network::NodeNetwork;
        use crate::structure_designer::node_type::{
            NodeType, generic_node_data_loader, generic_node_data_saver,
        };

        // Re-add empty network with same name
        let network = NodeNetwork::new(NodeType {
            name: self.network_name.clone(),
            description: "".to_string(),
            summary: None,
            category:
                crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory::Custom,
            parameters: Vec::new(),
            output_pins: crate::structure_designer::node_type::OutputPinDefinition::single(crate::structure_designer::data_type::DataType::None),
            node_data_creator: || Box::new(CustomNodeData::default()),
            node_data_saver: generic_node_data_saver::<CustomNodeData>,
            node_data_loader: generic_node_data_loader::<CustomNodeData>,
            zone_input_pins: vec![],
            zone_output_pins: vec![],
            public: true,
        });
        ctx.node_type_registry.add_node_network(network);

        // Switch active to the new network
        *ctx.active_network_name = Some(self.network_name.clone());
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
