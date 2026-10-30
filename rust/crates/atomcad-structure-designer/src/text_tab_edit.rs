//! The in-app *Text* tab's apply: replace the active network with the text,
//! keeping positions and the active node by name, as one undo step.
//!
//! Like an AI edit ([`crate::ai_text_edit`]) it is all or nothing: the editor
//! carries on past a failed statement, and in replace mode it has cleared the
//! network before the first one runs, so an error puts the untouched network
//! back rather than leaving a half-rebuilt one with no undo step.

use std::collections::HashMap;

use glam::DVec2;

use crate::ai_text_edit::rolled_back_errors;
use crate::network_validator::{capture_call_sites, validate_network};
use crate::node_type_registry::NodeTypeRegistry;
use crate::serialization::node_networks_serialization::node_network_to_serializable;
use crate::structure_designer::StructureDesigner;
use crate::text_format::{EditResult, ParseError, Parser, edit_network};
use crate::undo::commands::text_edit_network::TextEditNetworkCommand;

/// The undo label of a Text-tab apply.
pub const TEXT_TAB_EDIT_COMMAND_DESCRIPTION: &str = "Text edit network";

/// What [`StructureDesigner::apply_text_to_active_network`] did.
pub enum TextTabEditOutcome {
    /// Nothing was attempted: no active network, a read-only one, or one that
    /// is missing.
    Rejected(String),
    /// The text does not parse; the network is untouched.
    ParseError(ParseError),
    /// The editor ran. On an error (`result.success == false`) the network
    /// was put back as it was.
    Edited(EditResult),
}

impl StructureDesigner {
    /// Apply the Text tab's `code` to the active network in replace mode.
    ///
    /// Nodes that keep their name keep their position, and the active node is
    /// re-selected by name. A successful apply that changed something is one
    /// undo step and dirties the project; the caller owes a refresh.
    pub fn apply_text_to_active_network(&mut self, code: &str) -> TextTabEditOutcome {
        let Some(network_name) = self.active_node_network_name.clone() else {
            return TextTabEditOutcome::Rejected("No active node network".to_string());
        };
        // Linked content is read-only (`doc/design_library_linking.md` §6).
        if let Err(e) = self.ensure_editable(&network_name) {
            return TextTabEditOutcome::Rejected(e);
        }
        if let Err(e) = Parser::parse(code) {
            return TextTabEditOutcome::ParseError(e);
        }

        // Out of the registry for the edit: the editor needs `&mut` the
        // network and `&` the registry it lives in.
        let Some(mut network) = self.node_type_registry.node_networks.remove(&network_name) else {
            return TextTabEditOutcome::Rejected(format!("Network '{}' not found", network_name));
        };

        // Positions by name, to put back on the nodes the replace re-creates.
        let saved_positions: HashMap<String, DVec2> = network
            .nodes
            .values()
            .filter_map(|node| {
                node.custom_name
                    .as_ref()
                    .map(|name| (name.clone(), node.position))
            })
            .collect();
        let active_custom_name: Option<String> = network
            .active_node_id
            .and_then(|id| network.nodes.get(&id))
            .and_then(|node| node.custom_name.clone());

        // The undo step's "before". A text edit can delete or reorder
        // `parameter`s; the undo step restores the callers' wires into them
        // from `call_sites`.
        let before_snapshot = node_network_to_serializable(
            &mut network,
            &self.node_type_registry.built_in_node_types,
            None,
            false,
        )
        .ok();
        let call_sites = capture_call_sites(&self.node_type_registry, &network_name);

        let original = network.clone();
        let mut result = edit_network(&mut network, &self.node_type_registry, code, true);
        if !result.success {
            self.node_type_registry
                .node_networks
                .insert(network_name, original);
            result.errors = rolled_back_errors(std::mem::take(&mut result.errors));
            result.nodes_created.clear();
            result.nodes_updated.clear();
            result.nodes_deleted.clear();
            result.connections_made.clear();
            result.description_set = None;
            result.summary_set = None;
            result.output_set = None;
            return TextTabEditOutcome::Edited(result);
        }

        for node in network.nodes.values_mut() {
            if let Some(saved_pos) = node
                .custom_name
                .as_ref()
                .and_then(|name| saved_positions.get(name))
            {
                node.position = *saved_pos;
            }
        }
        if let Some(active_name) = &active_custom_name {
            let node_id = network
                .nodes
                .values()
                .find(|n| n.custom_name.as_deref() == Some(active_name.as_str()))
                .map(|n| n.id);
            if let Some(id) = node_id {
                network.select_node(id);
            }
        }

        let after_snapshot = node_network_to_serializable(
            &mut network,
            &self.node_type_registry.built_in_node_types,
            None,
            false,
        )
        .ok();

        self.node_type_registry
            .node_networks
            .insert(network_name.clone(), network);
        {
            let registry_ptr = &mut self.node_type_registry as *mut NodeTypeRegistry;
            // SAFETY: the same split borrow as `ai_text_edit`: the network is
            // validated against the registry that holds it.
            unsafe {
                if let Some(network) = (*registry_ptr).node_networks.get_mut(&network_name) {
                    validate_network(network, &mut *registry_ptr, None);
                }
            }
        }

        let made_changes = !result.nodes_created.is_empty()
            || !result.nodes_updated.is_empty()
            || !result.nodes_deleted.is_empty()
            || !result.connections_made.is_empty()
            || result.description_set.is_some()
            || result.summary_set.is_some()
            || result.output_set.is_some();
        if made_changes && let (Some(before), Some(after)) = (before_snapshot, after_snapshot) {
            self.push_command(TextEditNetworkCommand {
                network_name,
                before_snapshot: before,
                after_snapshot: after,
                description: TEXT_TAB_EDIT_COMMAND_DESCRIPTION,
                call_sites,
            });
        }

        self.mark_full_refresh();
        if made_changes {
            self.set_dirty(true);
        }
        TextTabEditOutcome::Edited(result)
    }
}
