use crate::structure_designer::node_network::IncomingWire;
use glam::f64::DVec2;
use serde_json::Value;

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
    pub node_id: u64,
    pub node_type_name: String,
    pub old_data_json: Value,
}
