use crate::network_validator::CallSite;
use crate::serialization::node_networks_serialization::SerializableNodeNetwork;
use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};

/// Command for undoing/redoing text-based network edits.
///
/// Text edits can make arbitrary changes to a network. Rather than decomposing
/// into fine-grained commands, we store before/after snapshots of the entire
/// network — which is also what makes this command cover **zone bodies** for
/// free, since `SerializableNode::zone` carries the whole body.
///
/// Two callers push it, and the label is how the undo tooltip tells them apart:
/// the in-app *Text* tab (`apply_text_to_active_network`) and the AI/CLI edit
/// surface (`ai_edit_network`, `doc/design_hof_body_text_format.md` Phase 5).
/// The distinction matters to the user: "what the AI just did to my network" is
/// exactly the step they reach for Ctrl+Z over.
pub struct TextEditNetworkCommand {
    pub network_name: String,
    /// Serialized network state before the text edit
    pub before_snapshot: SerializableNodeNetwork,
    /// Serialized network state after the text edit
    pub after_snapshot: SerializableNodeNetwork,
    /// Undo-tooltip label naming the edit's origin.
    pub description: &'static str,
    /// The wiring of this network's instances before the edit
    /// (`network_validator::capture_call_sites`): a text edit can delete or
    /// reorder `parameter`s, and the call-site repair then drops the
    /// instances' wires, outside this network.
    pub call_sites: Vec<CallSite>,
}

// Manual Debug impl because SerializableNodeNetwork doesn't derive Debug
impl std::fmt::Debug for TextEditNetworkCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextEditNetworkCommand")
            .field("network_name", &self.network_name)
            .field("description", &self.description)
            .finish()
    }
}

impl UndoCommand for TextEditNetworkCommand {
    fn description(&self) -> &str {
        self.description
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
