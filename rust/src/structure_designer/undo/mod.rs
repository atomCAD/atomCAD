pub mod commands;
pub mod snapshot;

use super::node_network::NodeNetwork;
use super::node_type_registry::NodeTypeRegistry;
use std::fmt::Debug;

/// What kind of refresh is needed after undo/redo.
#[derive(Debug, Clone)]
pub enum UndoRefreshMode {
    /// Only UI needs updating (e.g., node moved)
    Lightweight,
    /// Re-evaluate specific nodes (e.g., node data changed)
    NodeDataChanged(Vec<u64>),
    /// Re-evaluate entire network (e.g., structural change)
    Full,
}

/// Fold a sequence of child refresh modes into the single mode a
/// [`CompositeCommand`](commands::composite::CompositeCommand) should report:
/// the strongest child wins. Any `Full` ⇒ `Full`; otherwise the union of all
/// `NodeDataChanged` id-lists (a `Lightweight` contributes nothing); otherwise
/// `Lightweight`. This guarantees a child's heavier refresh is never downgraded
/// to a sibling's lighter one. See `doc/design_reflow_on_footprint_change.md`.
pub fn combine_refresh_modes(modes: impl IntoIterator<Item = UndoRefreshMode>) -> UndoRefreshMode {
    let mut node_ids: Vec<u64> = Vec::new();
    let mut saw_node_data = false;
    for mode in modes {
        match mode {
            UndoRefreshMode::Full => return UndoRefreshMode::Full,
            UndoRefreshMode::NodeDataChanged(ids) => {
                saw_node_data = true;
                node_ids.extend(ids);
            }
            UndoRefreshMode::Lightweight => {}
        }
    }
    if saw_node_data {
        UndoRefreshMode::NodeDataChanged(node_ids)
    } else {
        UndoRefreshMode::Lightweight
    }
}

/// Focused context passed to undo/redo methods.
///
/// Avoids passing all of StructureDesigner (which owns the UndoStack,
/// creating borrow conflicts).
pub struct UndoContext<'a> {
    pub node_type_registry: &'a mut NodeTypeRegistry,
    /// Mutable so commands like AddNetwork/DeleteNetwork can switch the active network.
    pub active_network_name: &'a mut Option<String>,
    /// Backend-owned active record def (mirrors `active_network_name`). Record
    /// rename/move/delete commands remap or clear it so the schema-editor
    /// selection survives undo/redo. See `doc/design_hierarchical_records.md` §8.
    pub active_record_def_name: &'a mut Option<String>,
}

impl<'a> UndoContext<'a> {
    /// Get mutable reference to a network by name.
    /// Commands use this with their stored network_name — NOT the active network,
    /// since undo/redo may fire while a different network is active.
    pub fn network_mut(&mut self, name: &str) -> Option<&mut NodeNetwork> {
        self.node_type_registry.node_networks.get_mut(name)
    }

    /// Get mutable reference to a body network nested inside the named
    /// top-level network. `scope_path` is the chain of HOF node IDs from the
    /// top-level down to the target body. Empty `scope_path` returns the
    /// top-level network itself (same as `network_mut`).
    ///
    /// Walks via `Node::zone_mut`, which forces `Arc::make_mut` on each
    /// `Arc<NodeNetwork>` along the way, so the returned reference is safe to
    /// mutate without aliasing other clones. Returns `None` if any step of
    /// the walk fails (network missing, HOF id missing, node has no zone).
    pub fn network_in_scope_mut(
        &mut self,
        name: &str,
        scope_path: &[u64],
    ) -> Option<&mut NodeNetwork> {
        let mut current = self.node_type_registry.node_networks.get_mut(name)?;
        for hof_id in scope_path {
            let node = current.nodes.get_mut(hof_id)?;
            current = node.zone_mut()?;
        }
        Some(current)
    }
}

/// Trait for undoable commands.
pub trait UndoCommand: Debug + Send + Sync {
    /// Human-readable description for UI display (e.g., "Add cuboid node")
    fn description(&self) -> &str;

    /// Reverse the command's effect
    fn undo(&self, ctx: &mut UndoContext);

    /// Re-apply the command's effect
    fn redo(&self, ctx: &mut UndoContext);

    /// What kind of refresh is needed after undo/redo
    fn refresh_mode(&self) -> UndoRefreshMode;
}

/// The undo/redo stack. Lives inside StructureDesigner.
pub struct UndoStack {
    /// Command history. Index 0 is the oldest command.
    history: Vec<Box<dyn UndoCommand>>,
    /// Points to the next available slot. Commands at indices [0..cursor) have been executed.
    /// Undo decrements cursor, redo increments it.
    cursor: usize,
    /// Maximum number of commands to retain (oldest are dropped when exceeded).
    pub max_history: usize,
    /// When true, `push()` calls are silently ignored.
    recording_suppressed: bool,
}

impl Default for UndoStack {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            cursor: 0,
            max_history: 100,
            recording_suppressed: false,
        }
    }
}

impl UndoStack {
    pub fn push(&mut self, command: Box<dyn UndoCommand>) {
        if self.recording_suppressed {
            return;
        }

        // Truncate redo tail if we're not at the end
        if self.cursor < self.history.len() {
            self.history.truncate(self.cursor);
        }

        // Append the new command
        self.history.push(command);
        self.cursor += 1;

        // Evict oldest if over max_history
        if self.history.len() > self.max_history {
            let excess = self.history.len() - self.max_history;
            self.history.drain(0..excess);
            self.cursor -= excess;
        }
    }

    pub fn undo(&mut self, ctx: &mut UndoContext) -> Option<UndoRefreshMode> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        let command = &self.history[self.cursor];
        command.undo(ctx);
        Some(command.refresh_mode())
    }

    pub fn redo(&mut self, ctx: &mut UndoContext) -> Option<UndoRefreshMode> {
        if self.cursor >= self.history.len() {
            return None;
        }
        let command = &self.history[self.cursor];
        command.redo(ctx);
        self.cursor += 1;
        Some(command.refresh_mode())
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor < self.history.len()
    }

    pub fn clear(&mut self) {
        self.history.clear();
        self.cursor = 0;
    }

    pub fn suppress_recording(&mut self) {
        self.recording_suppressed = true;
    }

    pub fn resume_recording(&mut self) {
        self.recording_suppressed = false;
    }

    /// Returns the description of the command that would be undone, if any.
    pub fn undo_description(&self) -> Option<&str> {
        if self.cursor > 0 {
            Some(self.history[self.cursor - 1].description())
        } else {
            None
        }
    }

    /// Returns the description of the command that would be redone, if any.
    pub fn redo_description(&self) -> Option<&str> {
        if self.cursor < self.history.len() {
            Some(self.history[self.cursor].description())
        } else {
            None
        }
    }
}
