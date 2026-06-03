use crate::structure_designer::node_network::Argument;
use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::commands::edit_zone_body::ZoneBodySnapshot;
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};
use std::sync::Arc;

/// Command for undoing/redoing a body-scoped **Closure → Network** extraction
/// (*Direction B* inside an HOF body).
///
/// Like [`super::factor_selection::FactorSelectionCommand`], the extraction both
/// **creates** a new network `N` (registry change) **and** mutates the host that
/// held the closure. The difference is the host: here it is an HOF body, so it is
/// restored from a [`ZoneBodySnapshot`] (the body `NodeNetwork` + the owner HOF's
/// `zone_output_arguments`) rather than a named-network snapshot. See
/// `doc/design_closure_network_conversion.md` §"Undo (Closure → Network)".
pub struct ExtractClosureBodyCommand {
    /// The active (top-level) network whose body tree contains the host scope.
    pub network_name: String,
    /// The newly created network `N`.
    pub subnetwork_name: String,
    pub subnetwork_snapshot: SerializableNodeNetwork,
    /// Chain `[parent.., hof_id]` down to the host body that held the closure.
    pub scope_path: Vec<u64>,
    /// Host body before the extraction (the closure `C` in place).
    pub body_before: ZoneBodySnapshot,
    /// Host body after the extraction (the instance `I` in place).
    pub body_after: ZoneBodySnapshot,
}

// Manual Debug: SerializableNodeNetwork doesn't derive Debug (mirrors
// FactorSelectionCommand / EditZoneBodyCommand).
impl std::fmt::Debug for ExtractClosureBodyCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtractClosureBodyCommand")
            .field("network_name", &self.network_name)
            .field("subnetwork_name", &self.subnetwork_name)
            .field("scope_path", &self.scope_path)
            .finish()
    }
}

impl ExtractClosureBodyCommand {
    /// Replace a network in the registry with one deserialized from a snapshot,
    /// re-populating its (incl. body-node) custom-node-type caches. Mirrors
    /// `FactorSelectionCommand::restore_network`.
    fn restore_subnetwork(ctx: &mut UndoContext, name: &str, snapshot: &SerializableNodeNetwork) {
        if let Ok(mut network) = serializable_to_node_network(
            snapshot,
            &ctx.node_type_registry.built_in_node_types,
            None,
        ) {
            ctx.node_type_registry
                .initialize_custom_node_types_for_network(&mut network);
            ctx.node_type_registry
                .node_networks
                .insert(name.to_string(), network);
        }
    }

    /// Restore the host HOF's body (and zone-output wires) from `snap`. Mirrors
    /// `EditZoneBodyCommand::restore`.
    #[allow(clippy::arc_with_non_send_sync)]
    fn restore_body(&self, ctx: &mut UndoContext, snap: &ZoneBodySnapshot) {
        let Some((hof_id, parent_path)) = self.scope_path.split_last() else {
            return; // empty scope path is not a body edit
        };

        // Deserialize + re-populate caches under an immutable registry borrow,
        // before the mutable `network_in_scope_mut` walk below.
        let mut body = match serializable_to_node_network(
            &snap.body,
            &ctx.node_type_registry.built_in_node_types,
            None,
        ) {
            Ok(network) => network,
            Err(_) => return,
        };
        ctx.node_type_registry
            .initialize_custom_node_types_for_network(&mut body);
        // Re-derive `apply` / `map` layouts preserving the arguments vector so
        // body arg wires survive the re-init's reset to the bare default layout
        // (see `EditZoneBodyCommand::restore`).
        ctx.node_type_registry
            .update_apply_pin_layouts_for_network_preserving_args(&mut body);
        ctx.node_type_registry
            .update_map_pin_layouts_for_network_preserving_args(&mut body);

        let zone_output_arguments: Vec<Argument> = snap
            .zone_output_wires
            .iter()
            .map(|wires| Argument {
                incoming_wires: wires.clone(),
            })
            .collect();

        let Some(parent) = ctx.network_in_scope_mut(&self.network_name, parent_path) else {
            return;
        };
        let Some(hof) = parent.nodes.get_mut(hof_id) else {
            return;
        };
        hof.zone = Some(Arc::new(body));
        hof.zone_output_arguments = zone_output_arguments;
    }
}

impl UndoCommand for ExtractClosureBodyCommand {
    fn description(&self) -> &str {
        "Extract closure to network"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Restore the body to its pre-extraction state, then remove `N`.
        self.restore_body(ctx, &self.body_before);
        ctx.node_type_registry
            .node_networks
            .remove(&self.subnetwork_name);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        // Re-add `N`, then restore the body to its post-extraction state.
        Self::restore_subnetwork(ctx, &self.subnetwork_name, &self.subnetwork_snapshot);
        self.restore_body(ctx, &self.body_after);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
