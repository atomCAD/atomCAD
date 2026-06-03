use crate::structure_designer::node_network::{Argument, IncomingWire};
use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use crate::structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};
use std::sync::Arc;

/// Snapshot of an HOF body for undo. Captures both the body `NodeNetwork`
/// (where add/move/delete/duplicate and intra-body / capture / zone-input wires
/// live) and the HOF's `zone_output_arguments` (where body-return wires live).
/// Together these cover every body-scoped structural edit uniformly — see
/// `EditZoneBodyCommand`.
pub struct ZoneBodySnapshot {
    /// The HOF's owned body network, serialized.
    pub body: SerializableNodeNetwork,
    /// The HOF's `zone_output_arguments`, one entry (the wires) per zone-output
    /// pin. Stored as wires (not `Argument`) so the snapshot is `PartialEq`-able
    /// for the no-op change check.
    pub zone_output_wires: Vec<Vec<IncomingWire>>,
}

/// Returns true if the two body snapshots differ.
///
/// Wire lists compare structurally. The body compares via its JSON value: when
/// only wires changed (no node added/removed) the body's `nodes`/`displayed_*`
/// HashMaps are unmodified, so two serializations of the *same* map instance
/// iterate in the same order and compare equal — a reliable no-op signal. When
/// a node was added/removed the map contents differ, so the JSON differs.
fn zone_body_changed(before: &ZoneBodySnapshot, after: &ZoneBodySnapshot) -> bool {
    if before.zone_output_wires != after.zone_output_wires {
        return true;
    }
    serde_json::to_value(&before.body).ok() != serde_json::to_value(&after.body).ok()
}

/// Undo/redo for a structural edit inside an HOF body: adding, moving, deleting,
/// or duplicating a body node, or creating an intra-body / capture / zone-input
/// / body-return wire. Rather than a surgical per-operation command, this stores
/// a full before/after snapshot of the affected body and restores it wholesale —
/// the body networks are small and this handles every wire shape (and nested
/// bodies) uniformly. `scope_path` is the body's chain `[parent.., hof_id]`.
///
/// See `doc/design_zones_ui.md` §"Undo/redo".
pub struct EditZoneBodyCommand {
    pub network_name: String,
    pub scope_path: Vec<u64>,
    pub before: ZoneBodySnapshot,
    pub after: ZoneBodySnapshot,
    pub description: String,
}

// Manual Debug: SerializableNodeNetwork doesn't derive Debug (mirrors
// FactorSelectionCommand).
impl std::fmt::Debug for EditZoneBodyCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditZoneBodyCommand")
            .field("network_name", &self.network_name)
            .field("scope_path", &self.scope_path)
            .field("description", &self.description)
            .finish()
    }
}

impl EditZoneBodyCommand {
    /// True iff the snapshot actually represents a change worth recording.
    pub fn is_meaningful(before: &ZoneBodySnapshot, after: &ZoneBodySnapshot) -> bool {
        zone_body_changed(before, after)
    }

    /// Restore the HOF's body (and zone-output wires) from `snap`. The HOF is at
    /// the last element of `scope_path`, living in the network reached by the
    /// preceding path.
    #[allow(clippy::arc_with_non_send_sync)]
    fn restore(&self, ctx: &mut UndoContext, snap: &ZoneBodySnapshot) {
        let Some((hof_id, parent_path)) = self.scope_path.split_last() else {
            return; // empty scope path is not a body edit
        };

        // Deserialize the body and re-populate its custom-node-type caches while
        // we hold an immutable registry borrow. This must precede the mutable
        // `network_in_scope_mut` walk below.
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
        // The re-init above reset any `apply` / `map` body node to its bare
        // default layout (erasing post-pass-derived arg-pin names). Re-derive
        // those layouts preserving the arguments vector positionally, so the
        // body's arg wires survive (the post-Full-refresh validate pass would
        // otherwise drop them on its by-name rebuild). No-op for bodies without
        // a wired-`f` apply/map.
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

        // Install into the HOF (mutable network borrow).
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

impl UndoCommand for EditZoneBodyCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.restore(ctx, &self.before);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.restore(ctx, &self.after);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
