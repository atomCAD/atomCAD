use crate::structure_designer::node_network::NodeRef;
use std::collections::HashSet;

/// Refresh mode for structure designer operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RefreshMode {
    /// Lightweight refresh - only update gadget tessellation
    Lightweight,
    /// Partial refresh - use tracked changes (visibility, data, selection)
    #[default]
    Partial,
    /// Full refresh - re-evaluate everything (fallback for complex/unknown changes)
    Full,
}

/// Tracks changes to the structure designer to determine what needs to be refreshed
/// This is the single source of truth for refresh operations, replacing the old
/// StructureDesignerRefreshMode enum which couldn't represent multiple simultaneous changes
#[derive(Default, Clone)]
pub struct StructureDesignerChanges {
    /// The refresh mode - defaults to Partial
    pub mode: RefreshMode,
    /// Nodes whose visibility changed. Carries scope for the same reason
    /// [`Self::data_changed`] does — per-body `next_node_id` counters let a
    /// body-internal id collide with a top-level one. Production code only
    /// marks top-level refs today; scoped refs arrive with Phase 2 of
    /// `doc/design_zero_ary_closure_body_display.md`.
    pub visibility_changed: HashSet<NodeRef>,
    /// Nodes whose data changed. Carries scope so a body-internal edit doesn't
    /// collide with a top-level id that happens to match numerically (each
    /// body has its own `next_node_id` counter — see `node_network.rs`).
    pub data_changed: HashSet<NodeRef>,
    /// Selection change tracking
    pub previous_selection: Option<u64>,
    pub current_selection: Option<u64>,
    pub selection_changed: bool,
    /// When true, Partial refresh re-evaluates only the directly changed nodes
    /// without propagating to downstream dependents. Used during interactive drag
    /// to avoid expensive downstream re-evaluation on every frame.
    pub skip_downstream: bool,
}

impl StructureDesignerChanges {
    /// Creates a new empty changes tracker with Partial mode
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears all tracked changes and resets to Partial mode
    pub fn clear(&mut self) {
        self.mode = RefreshMode::Partial;
        self.visibility_changed.clear();
        self.data_changed.clear();
        self.previous_selection = None;
        self.current_selection = None;
        self.selection_changed = false;
        self.skip_downstream = false;
    }

    /// Sets the refresh mode
    pub fn set_mode(&mut self, mode: RefreshMode) {
        self.mode = mode;
    }

    /// Returns true if this is a lightweight refresh
    pub fn is_lightweight(&self) -> bool {
        self.mode == RefreshMode::Lightweight
    }

    /// Returns true if this is a full refresh
    pub fn is_full(&self) -> bool {
        self.mode == RefreshMode::Full
    }

    /// Returns true if this is a partial refresh
    pub fn is_partial(&self) -> bool {
        self.mode == RefreshMode::Partial
    }

    /// Mark a top-level node's data as changed. Convenience for the common
    /// case; body-scope mutations use [`mark_node_data_changed_scoped`].
    pub fn mark_node_data_changed(&mut self, node_id: u64) {
        self.data_changed.insert(NodeRef::top(node_id));
    }

    /// Mark a node's data as changed at a specific scope. `scope_path` is the
    /// chain of HOF ids identifying the body the node lives in; empty means
    /// the top-level network.
    pub fn mark_node_data_changed_scoped(&mut self, scope_path: &[u64], node_id: u64) {
        self.data_changed
            .insert(NodeRef::scoped(scope_path, node_id));
    }

    /// Marks a top-level node's visibility as changed. Convenience for the
    /// common case; body-scope toggles use
    /// [`Self::mark_node_visibility_changed_scoped`].
    pub fn mark_node_visibility_changed(&mut self, node_id: u64) {
        self.visibility_changed.insert(NodeRef::top(node_id));
    }

    /// Marks a node's visibility as changed at a specific scope. `scope_path`
    /// is the chain of HOF/closure ids identifying the body the node lives in;
    /// empty means the top-level network.
    pub fn mark_node_visibility_changed_scoped(&mut self, scope_path: &[u64], node_id: u64) {
        self.visibility_changed
            .insert(NodeRef::scoped(scope_path, node_id));
    }

    /// Marks that selection changed
    pub fn mark_selection_changed(
        &mut self,
        previous_selection: Option<u64>,
        current_selection: Option<u64>,
    ) {
        self.previous_selection = previous_selection;
        self.current_selection = current_selection;
        self.selection_changed = true;
    }

    /// Creates a lightweight refresh (gadget tessellation only)
    pub fn lightweight() -> Self {
        Self {
            mode: RefreshMode::Lightweight,
            ..Default::default()
        }
    }

    /// Creates a full refresh (re-evaluate everything)
    pub fn full() -> Self {
        Self {
            mode: RefreshMode::Full,
            ..Default::default()
        }
    }

    /// Creates a partial refresh with a top-level node's data marked changed.
    pub fn node_data_changed(node_id: u64) -> Self {
        let mut changes = Self::new();
        changes.mark_node_data_changed(node_id);
        changes
    }

    /// Creates a partial refresh with specific node visibility changes
    pub fn visibility_changed(node_id: u64) -> Self {
        let mut changes = Self::new();
        changes.mark_node_visibility_changed(node_id);
        changes
    }

    /// Creates a partial refresh with selection change
    pub fn selection_changed(
        previous_selection: Option<u64>,
        current_selection: Option<u64>,
    ) -> Self {
        let mut changes = Self::new();
        changes.mark_selection_changed(previous_selection, current_selection);
        changes
    }
}
