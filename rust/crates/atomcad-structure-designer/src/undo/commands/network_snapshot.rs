use crate::network_validator::CallSite;
use crate::serialization::node_networks_serialization::SerializableNodeNetwork;
use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Generic whole-network snapshot command: one undo step for any edit of a
/// network whose fallout reaches beyond one node's data blob — a variadic-pin
/// list change that drops or remaps external/body wires, a declared input
/// appended together with its wire, a retype that revalidation re-derives.
///
/// The generic node-data snapshot (`SetNodeDataCommand`) captures only the
/// `node_data_saver` blob — no `arguments`, no zone body — so it cannot restore
/// that wire fallout. Like `TextEditNetworkCommand`, this instead stores
/// before/after snapshots of the owning **top-level** network; bodies travel
/// inside their HOF nodes, so body-internal edits are covered, and redo never
/// depends on re-deriving a pin before re-adding its wire.
///
/// Shared by `zip_with` lane edits (`doc/design_zip_with.md` Phase 3),
/// `switch` case edits (`doc/design_switch_node.md` Phase 2) and the declared
/// input operations (`declared_input_ops.rs`, "Add input from wire" / "Take
/// type from wire"); `description` distinguishes them in the undo history.
pub struct NetworkSnapshotCommand {
    pub network_name: String,
    /// Human-readable label shown in the undo history (e.g. "Edit switch
    /// cases", "Edit zip_with lanes").
    pub description: String,
    /// Serialized network state before the edit.
    pub before_snapshot: SerializableNodeNetwork,
    /// Serialized network state after the edit.
    pub after_snapshot: SerializableNodeNetwork,
    /// The wiring of this network's instances before the edit
    /// (`network_validator::capture_call_sites`), for an edit that can remove
    /// a `parameter`: the call-site repair drops the instances' wires into
    /// it, outside this network. Empty when the edit cannot.
    pub call_sites: Vec<CallSite>,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug.
impl std::fmt::Debug for NetworkSnapshotCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkSnapshotCommand")
            .field("network_name", &self.network_name)
            .field("description", &self.description)
            .finish()
    }
}

impl UndoCommand for NetworkSnapshotCommand {
    fn description(&self) -> &str {
        &self.description
    }

    /// The network is restored keeping the live interface
    /// ([`UndoContext::restore_network_keeping_interface`]), and the callers'
    /// wiring is handed back for after the refresh's validation.
    fn undo(&self, ctx: &mut UndoContext) {
        ctx.restore_network_keeping_interface(&self.network_name, &self.before_snapshot);
        ctx.restore_call_sites(&self.network_name, &self.call_sites);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        ctx.restore_network_keeping_interface(&self.network_name, &self.after_snapshot);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
