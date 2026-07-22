use crate::structure_designer::node_network::{
    Argument, CollapseMode, FunctionPinRole, IncomingWire, Node, NodeNetwork,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, serializable_to_node_network,
};
use glam::f64::DVec2;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Full snapshot of a node, used by undo commands to restore deleted/pasted nodes.
#[derive(Debug, Clone)]
pub struct NodeSnapshot {
    pub node_id: u64,
    pub node_type_name: String,
    pub position: DVec2,
    pub custom_name: Option<String>,
    pub node_data_json: Value,
    /// All input arguments (connections into this node)
    pub arguments: Vec<ArgumentSnapshot>,
    /// The owned zone body of a zone-owning node (HOF / `closure`), serialized
    /// as a `SerializableNodeNetwork` JSON value. `None` for zone-less nodes.
    /// Without this, restoring a deleted HOF brings it back with an empty body
    /// (issue #415).
    pub zone_json: Option<Value>,
    /// The HOF's `zone_output_arguments` (body-return wires), one wire list per
    /// zone-output pin. Empty for zone-less nodes.
    pub zone_output_wires: Vec<Vec<IncomingWire>>,
    /// Stored body dimensions (meaningful only when `zone_json.is_some()`).
    pub body_width: f64,
    pub body_height: f64,
    pub collapse_mode: CollapseMode,
    pub function_pin_roles: BTreeMap<usize, FunctionPinRole>,
}

impl NodeSnapshot {
    /// Rebuild the zone body captured in `zone_json`, re-initializing the
    /// derived per-node state a deserialized body needs (custom-node-type
    /// caches, then the `apply`/`map`/`zip_with` arg-preserving layout
    /// post-passes — same recipe as `EditZoneBodyCommand::restore`). Returns
    /// `None` for zone-less snapshots. Must run while the registry is
    /// borrowable, i.e. before taking the mutable network borrow.
    pub fn load_zone_body(&self, registry: &mut NodeTypeRegistry) -> Option<NodeNetwork> {
        let zone_json = self.zone_json.as_ref()?;
        let serializable: SerializableNodeNetwork =
            serde_json::from_value(zone_json.clone()).ok()?;
        let mut body =
            serializable_to_node_network(&serializable, &registry.built_in_node_types, None)
                .ok()?;
        registry.initialize_custom_node_types_for_network(&mut body);
        registry.update_apply_pin_layouts_for_network_preserving_args(&mut body);
        registry.update_map_pin_layouts_for_network_preserving_args(&mut body);
        registry.update_zip_with_pin_layouts_for_network_preserving_args(&mut body);
        Some(body)
    }

    /// Apply the zone-related snapshot fields onto a freshly re-created node
    /// (`add_node_with_id` resets them all to zone-less defaults). `body` is
    /// the result of [`load_zone_body`]. A no-op-equivalent for zone-less
    /// snapshots, so callers can apply unconditionally.
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn apply_zone_state(&self, node: &mut Node, body: Option<NodeNetwork>) {
        node.zone = body.map(Arc::new);
        node.zone_output_arguments = self
            .zone_output_wires
            .iter()
            .map(|wires| Argument {
                incoming_wires: wires.clone(),
            })
            .collect();
        node.body_width = self.body_width;
        node.body_height = self.body_height;
        node.collapse_mode = self.collapse_mode;
        node.function_pin_roles = self.function_pin_roles.clone();
    }
}

/// Snapshot of a node's argument (one input pin).
#[derive(Debug, Clone)]
pub struct ArgumentSnapshot {
    /// Inbound wires on this argument pin. Mirrors `Argument.incoming_wires`.
    pub incoming_wires: Vec<IncomingWire>,
}

/// Snapshot of a wire connection.
#[derive(Debug, Clone)]
pub struct WireSnapshot {
    pub source_node_id: u64,
    pub source_output_pin_index: i32,
    pub dest_node_id: u64,
    pub dest_param_index: usize,
}

/// Temporary state held during a drag operation.
#[derive(Debug, Clone)]
pub struct PendingMove {
    /// Scope of the body whose nodes are being dragged. Empty = top-level
    /// network. The drag start/end both resolve through this path so body-scope
    /// drags coalesce into a single scope-aware `MoveNodesCommand`.
    pub scope_path: Vec<u64>,
    /// (node_id, position_at_drag_start)
    pub start_positions: Vec<(u64, DVec2)>,
}

/// Temporary state held during an HOF body resize drag (for undo coalescing).
/// Captures the body dimensions before the drag so a single `SetZoneSizeCommand`
/// can be pushed when the drag ends. Mirrors `PendingMove` / the comment-node
/// resize coalescing pattern. See `doc/design_zones_ui.md` §"Resize handles".
#[derive(Debug, Clone)]
pub struct PendingZoneResize {
    pub network_name: String,
    /// Scope of the body the HOF lives in (empty = top-level).
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub old_width: f64,
    pub old_height: f64,
}

/// Temporary state held during a gadget drag operation (for undo coalescing).
/// Captures the node data snapshot before the drag so a single `SetNodeDataCommand`
/// can be pushed when the drag ends.
#[derive(Debug, Clone)]
pub struct PendingGadgetDrag {
    pub network_name: String,
    /// Scope of the edited node (empty = top-level). Comment nodes can live
    /// inside HOF zone bodies; the gadget-drag user is always top-level.
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub node_type_name: String,
    pub old_data_json: Value,
}
