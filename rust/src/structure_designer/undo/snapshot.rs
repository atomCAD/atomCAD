use glam::f64::DVec2;
use serde_json::Value;
use std::collections::HashMap;

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
    /// Maps source_node_id → output_pin_index
    pub argument_output_pins: HashMap<u64, i32>,
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
    /// (node_id, position_at_drag_start)
    pub start_positions: Vec<(u64, DVec2)>,
}
