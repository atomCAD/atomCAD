use std::collections::HashSet;

/// Specifies what kind of refresh operation should be performed on the structure designer
#[derive(Debug, Clone)]
pub enum StructureDesignerRefreshMode {
    /// Full refresh - re-evaluate all displayed nodes from scratch
    /// Used when structural changes occur (connections, node addition/deletion, network switch)
    Full,
    
    /// Lightweight refresh - only update gadget tessellation without re-evaluation
    /// Used during interactive gadget manipulation (dragging)
    /// The gadget is already active and should not be recreated
    Lightweight,
    
    /// Selection changed - update selection-dependent state and recreate gadget
    /// No node re-evaluation needed, but gadget must be recreated for new selection
    /// 
    /// Parameters:
    /// - previous_selection: The node ID that was previously selected (if any)
    /// - current_selection: The node ID that is now selected (if any)
    /// 
    /// Note: Current selection can also be read from state, but previous selection
    /// must be passed in since it's no longer in the state when refresh is called
    SelectionChanged {
        previous_selection: Option<u64>,
        current_selection: Option<u64>,
    },
    
    /// Visibility changed - nodes were shown or hidden
    /// Re-evaluate only newly displayed nodes, remove hidden ones, reuse cache for unchanged
    /// 
    /// Parameters:
    /// - changed_node_ids: Set of node IDs whose visibility changed
    /// 
    /// The current visibility state can be read from network.displayed_node_ids
    /// This set tells us which nodes to check (were they added or removed from display?)
    VisibilityChanged {
        changed_node_ids: HashSet<u64>,
    },
    
    /// Node data changed - specific nodes had their parameters/data modified
    /// Re-evaluate the displayed nodes that are downstream of the changed nodes
    /// 
    /// Parameters:
    /// - changed_node_ids: Set of node IDs whose data was modified
    /// 
    /// The refresh will compute downstream dependencies automatically
    NodeDataChanged {
        changed_node_ids: HashSet<u64>,
    },
}
