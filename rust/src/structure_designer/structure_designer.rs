use super::camera_settings::CameraSettings;
use super::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement, PrintLogEntry,
};
use super::evaluator::network_result::NetworkResult;
use super::navigation_history::NavigationHistory;
use super::network_validator::{NetworkValidationResult, validate_network};
use super::node_display_policy_resolver::NodeDisplayPolicyResolver;
use super::node_network::{NodeNetwork, NodeRef};
use super::node_network_gadget::NodeNetworkGadget;
use super::node_networks_import_manager::NodeNetworksImportManager;
use super::node_type::{NodeType, OutputPinDefinition};
use super::node_type_registry::NodeTypeRegistry;
use super::preferences::load_preferences;
use super::structure_designer_changes::{RefreshMode, StructureDesignerChanges};
use super::undo::snapshot::PendingMove;
use super::undo::{UndoCommand, UndoContext, UndoRefreshMode, UndoStack};
use crate::api::structure_designer::structure_designer_api_types::APIExecuteResult;
use crate::api::structure_designer::structure_designer_api_types::APINodeEvaluationResult;
use crate::api::structure_designer::structure_designer_preferences::{
    AtomicStructureVisualization, StructureDesignerPreferences,
};
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_structure_utils::calc_selection_transform;
use crate::crystolecule::io::mol_exporter::save_mol_v3000;
use crate::crystolecule::io::xyz_saver::save_xyz;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::display::atomic_tessellator::{BAS_STICK_RADIUS, get_displayed_atom_radius};
use crate::geo_tree::implicit_geometry::ImplicitGeometry3D;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::implicit_eval::ray_tracing::{
    raytrace_geometries, raytrace_geometry,
};
use crate::structure_designer::node_data::CustomNodeData;
use crate::structure_designer::node_data::DragDirection;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_dependency_analysis::compute_downstream_dependents;
use crate::structure_designer::node_type::{generic_node_data_loader, generic_node_data_saver};
use crate::structure_designer::nodes::edit_atom::edit_atom::get_selected_edit_atom_data_mut;
use crate::structure_designer::serialization::node_networks_serialization;
use crate::structure_designer::structure_designer_scene::StructureDesignerScene;
use glam::f64::DVec2;
use glam::f64::DVec3;
use std::collections::{HashMap, HashSet};

/// A ray hit result associated with a specific node.
///
/// Returned by `StructureDesigner::raytrace_per_node()` to identify which
/// node's output was intersected and at what distance along the ray.
#[derive(Debug, Clone, PartialEq)]
pub struct PerNodeRayHit {
    pub node_id: u64,
    pub distance: f64,
}

/// Drag-source context propagated from the add-node popup to `add_node`.
/// Drives `NodeData::adapt_for_drag_source` so type-parameterized nodes are
/// instantiated with their type properties already set to make the
/// just-completed wire drag connect. See `doc/design_drag_aware_add_node.md`.
#[derive(Debug, Clone)]
pub struct DragSource {
    pub source_type: DataType,
    pub direction: DragDirection,
}

pub struct StructureDesigner {
    pub node_type_registry: NodeTypeRegistry,
    pub network_evaluator: NetworkEvaluator,
    pub gadget: Option<Box<dyn NodeNetworkGadget>>,
    pub active_node_network_name: Option<String>,
    pub last_generated_structure_designer_scene: StructureDesignerScene,
    pub preferences: StructureDesignerPreferences,
    pub node_display_policy_resolver: NodeDisplayPolicyResolver,
    pub import_manager: NodeNetworksImportManager,
    pub is_dirty: bool,
    pub file_path: Option<String>,
    // Tracks pending changes since last refresh to determine what needs to be refreshed
    pending_changes: StructureDesignerChanges,
    // Temporary storage for CLI parameters during evaluation (used in headless mode)
    pub cli_top_level_parameters: Option<HashMap<String, NetworkResult>>,
    // Navigation history for back/forward functionality
    navigation_history: NavigationHistory,
    // Clipboard for copy/paste operations (stores copied nodes as an isolated NodeNetwork)
    pub clipboard: Option<NodeNetwork>,
    // Undo/redo stack for all network mutations
    pub undo_stack: UndoStack,
    // Temporary state during a node drag operation (for move coalescing)
    pub pending_move: Option<PendingMove>,
    // Temporary state during an atom edit drag operation (for drag coalescing)
    pub pending_atom_edit_drag: Option<super::nodes::atom_edit::atom_edit::PendingAtomEditDrag>,
    // Temporary state during a gadget drag operation (for undo coalescing of non-atom_edit nodes)
    pub pending_gadget_drag: Option<super::undo::snapshot::PendingGadgetDrag>,
    // Temporary state during comment node editing (text typing or resize drag)
    pub pending_comment_edit: Option<super::undo::snapshot::PendingGadgetDrag>,
    // Direct editing mode: simplified UI focused on a single atom_edit node
    pub direct_editing_mode: bool,
    // CLI access rules: sparse map of namespace/network prefixes to allowed (true) / denied (false).
    // To determine access for a network, find the longest matching prefix in this map.
    // If no match, CLI write access is allowed by default.
    // Setting a rule prunes all descendant rules to keep the map minimal.
    pub cli_access_rules: HashMap<String, bool>,
    // Per-CAD-instance print log buffer. Accumulates entries pushed by the
    // `print` node across both display passes (when `execute_only == false`)
    // and explicit Execute passes. Drained by Flutter via `take_print_log` and
    // displayed in the Console panel. See `doc/design_node_execution.md`
    // (Phase 4 — Console panel).
    pub print_log: Vec<PrintLogEntry>,
}

impl Default for StructureDesigner {
    fn default() -> Self {
        Self::new()
    }
}

impl StructureDesigner {
    pub fn new() -> Self {
        let node_type_registry = NodeTypeRegistry::new();
        let network_evaluator = NetworkEvaluator::new();
        let node_display_policy_resolver = NodeDisplayPolicyResolver::new();
        // Load persisted preferences from config directory, or use defaults if not available
        let preferences = load_preferences();

        Self {
            node_type_registry,
            network_evaluator,
            gadget: None,
            active_node_network_name: None,
            last_generated_structure_designer_scene: StructureDesignerScene::new(),
            preferences,
            node_display_policy_resolver,
            import_manager: NodeNetworksImportManager::new(),
            is_dirty: false,
            file_path: None,
            pending_changes: StructureDesignerChanges::default(),
            cli_top_level_parameters: None,
            navigation_history: NavigationHistory::new(),
            clipboard: None,
            undo_stack: UndoStack::default(),
            pending_move: None,
            pending_atom_edit_drag: None,
            pending_gadget_drag: None,
            pending_comment_edit: None,
            direct_editing_mode: true,
            cli_access_rules: HashMap::new(),
            print_log: Vec::new(),
        }
    }
}

impl StructureDesigner {
    /// Run an evaluation with a fresh `NetworkEvaluationContext`, then drain
    /// any prints the pass produced into `self.print_log` (Phase 4 — for
    /// now the buffer is taken and dropped). The closure receives split
    /// borrows of the evaluator, the registry, the preferences, and the
    /// context, so eval-driving call sites do not need to construct a
    /// context inline.
    ///
    /// **This is the only legitimate construction site for a
    /// `NetworkEvaluationContext` inside `rust/src/structure_designer/`,
    /// alongside `FunctionEvaluator::evaluate`'s inner-body context (which
    /// drains its `print_buffer` back into its outer caller).** Reviewers
    /// grepping for `NetworkEvaluationContext::new(` outside those two
    /// sites — and outside test crates, which are exempt — have a one-shot
    /// audit. Centralising the construct + drain pair eliminates the
    /// foot-gun where a missed call site silently swallows prints. See
    /// `doc/design_node_execution.md` (Centralized drain — no per-call-site
    /// boilerplate).
    pub fn with_eval_context<R>(
        &mut self,
        execute: bool,
        f: impl FnOnce(
            &mut NetworkEvaluator,
            &NodeTypeRegistry,
            &StructureDesignerPreferences,
            &mut NetworkEvaluationContext,
        ) -> R,
    ) -> R {
        let mut context = NetworkEvaluationContext::new();
        context.execute = execute;
        context.use_vdw_cutoff = self.preferences.simulation_preferences.use_vdw_cutoff;
        if let Some(params) = self.cli_top_level_parameters.clone() {
            context.top_level_parameters = params;
        }
        let result = f(
            &mut self.network_evaluator,
            &self.node_type_registry,
            &self.preferences,
            &mut context,
        );
        // Drain regardless of how `f` returned — prints accumulated up to a
        // mid-pass error are still worth showing to the user.
        let entries = std::mem::take(&mut context.print_buffer);
        self.print_log.extend(entries);
        result
    }

    /// Drain and return the accumulated print log entries. Called from FFI
    /// (`take_print_log`) at a sensible cadence by the Flutter Console panel.
    /// Drain-on-read prevents the buffer from growing indefinitely so long as
    /// the panel is occasionally opened. See `doc/design_node_execution.md`
    /// (Phase 4 — FFI).
    pub fn take_print_log(&mut self) -> Vec<PrintLogEntry> {
        std::mem::take(&mut self.print_log)
    }

    /// Clear the accumulated print log without returning entries (Console
    /// panel "Clear" button).
    pub fn clear_print_log(&mut self) {
        self.print_log.clear();
    }

    /// Resolve `(active_node_network_name, scope_path)` to the targeted
    /// `&NodeNetwork`. Empty `scope_path` returns the active top-level network
    /// (today's behavior). A non-empty path walks `Node.zone` down the chain of
    /// HOF node IDs and returns the deepest body. Returns `None` if no network
    /// is active, the network is missing, any node in the chain doesn't exist,
    /// or any chain node is not an HOF (i.e. `node.zone == None`).
    ///
    /// Centralising the walk here is the gotcha noted in
    /// `doc/design_zones_ui.md` §"Phase U2 → Gotchas": every scoped mutation
    /// shares this helper so the body-resolution logic lives in exactly one
    /// place.
    pub fn get_scope_network(&self, scope_path: &[u64]) -> Option<&NodeNetwork> {
        let network_name = self.active_node_network_name.as_ref()?;
        let mut current = self.node_type_registry.node_networks.get(network_name)?;
        for hof_id in scope_path {
            let node = current.nodes.get(hof_id)?;
            let zone = node.zone.as_ref()?;
            current = zone;
        }
        Some(current)
    }

    /// Mutable counterpart of [`get_scope_network`]. Each step calls
    /// `Node::zone_mut` so the `Arc<NodeNetwork>` is uniquely owned (CoW)
    /// before the descent continues. Returns `None` under the same conditions
    /// as the immutable variant.
    pub fn get_scope_network_mut(&mut self, scope_path: &[u64]) -> Option<&mut NodeNetwork> {
        let network_name = self.active_node_network_name.as_ref()?.clone();
        let mut current = self
            .node_type_registry
            .node_networks
            .get_mut(&network_name)?;
        for hof_id in scope_path {
            let node = current.nodes.get_mut(hof_id)?;
            // `zone_mut` returns None for non-HOF nodes (zone == None) so this
            // naturally rejects malformed scope paths.
            current = node.zone_mut()?;
        }
        Some(current)
    }

    /// Returns the atomic structure from the interactive pin of the selected node, if any.
    /// The interactive pin is the lowest-indexed displayed output pin.
    pub fn get_atomic_structure_from_selected_node(&self) -> Option<&AtomicStructure> {
        use crate::structure_designer::structure_designer_scene::NodeOutput;
        for node_data in self
            .last_generated_structure_designer_scene
            .node_data
            .values()
        {
            if let Some(interactive_output) = node_data.interactive_output() {
                if let NodeOutput::Atomic(atomic_structure, _) = interactive_output {
                    if atomic_structure.decorator().from_selected_node {
                        return Some(atomic_structure);
                    }
                }
            }
        }
        None
    }

    /// Returns the interactive pin index for the selected node.
    /// The interactive pin is the lowest-indexed displayed output pin.
    /// For atom_edit: pin 0 = result view, pin 1 = diff view.
    pub fn get_selected_node_interactive_pin(&self) -> Option<i32> {
        let network_name = self.active_node_network_name.as_ref()?;
        let network = self.node_type_registry.node_networks.get(network_name)?;
        let active_node_id = network.active_node_id?;
        self.last_generated_structure_designer_scene
            .node_data
            .get(&active_node_id)
            .and_then(|data| data.interactive_pin_index())
    }

    /// Returns true if the selected atom_edit node's interactive pin is the diff pin (pin 1).
    /// This replaces the old `output_diff` flag for determining hit test ID space.
    pub fn is_selected_node_in_diff_view(&self) -> bool {
        self.get_selected_node_interactive_pin() == Some(1)
    }

    /// Gets the eval cache for the currently active node (used for gadget creation)
    /// Returns None if no node is active or the active node has no eval cache
    pub fn get_selected_node_eval_cache(&self) -> Option<&Box<dyn std::any::Any>> {
        let network_name = self.active_node_network_name.as_ref()?;
        let network = self.node_type_registry.node_networks.get(network_name)?;
        let active_node_id = network.active_node_id?;
        self.last_generated_structure_designer_scene
            .get_node_eval_cache(active_node_id)
    }

    /// Helper method to get the active node ID of a node of a specific type
    ///
    /// Returns None if:
    /// - There is no active node network
    /// - No node is active in the active network
    /// - The active node has a different type name than the needed node type name
    pub fn get_selected_node_id_with_type(&self, needed_node_type_name: &str) -> Option<u64> {
        // Get active node network name
        let network_name = self.active_node_network_name.as_ref()?;

        // Get the active node network
        let network = self.node_type_registry.node_networks.get(network_name)?;

        // Get the active node ID
        let active_node_id = network.active_node_id?;

        // Get the active node's type name
        let node_type_name = network.nodes.get(&active_node_id)?.node_type_name.as_str();

        // Check if the node is with the needed node type name
        if node_type_name != needed_node_type_name {
            return None;
        }

        Some(active_node_id)
    }

    // Returns true if the active node is displayed and has the needed node type name
    pub fn is_node_type_active(&self, needed_node_type_name: &str) -> bool {
        // Check if active_node_network_name exists
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return false,
        };

        // Get the active node network
        let network = match self.node_type_registry.node_networks.get(network_name) {
            Some(network) => network,
            None => return false,
        };

        // Check if there's an active node ID
        let active_node_id = match network.active_node_id {
            Some(id) => id,
            None => return false,
        };

        // Check if the active node is displayed
        if !network.is_node_displayed(active_node_id) {
            return false;
        }

        // Get the active node's type name
        let node_type_name = match network.nodes.get(&active_node_id) {
            Some(node) => &node.node_type_name,
            None => return false,
        };

        // Return true only if the node's type name matches the needed node type name
        node_type_name == needed_node_type_name
    }

    /// Returns a clone of the pending changes to determine what needs to be refreshed
    pub fn get_pending_changes(&self) -> StructureDesignerChanges {
        self.pending_changes.clone()
    }

    /// Returns true if the pending refresh is lightweight (for Renderer)
    pub fn is_pending_refresh_lightweight(&self) -> bool {
        self.pending_changes.is_lightweight()
    }

    /// Marks a node's data as changed
    pub fn mark_node_data_changed(&mut self, node_id: u64) {
        self.pending_changes.mark_node_data_changed(node_id);
    }

    /// Marks that a full refresh is needed (for complex/unknown changes)
    pub fn mark_full_refresh(&mut self) {
        self.pending_changes.set_mode(RefreshMode::Full);
    }

    /// Marks that a lightweight refresh is needed (gadget tessellation only)
    pub fn mark_lightweight_refresh(&mut self) {
        self.pending_changes.set_mode(RefreshMode::Lightweight);
    }

    /// Marks that the next Partial refresh should skip downstream dependents.
    /// Used during interactive drag to avoid expensive downstream re-evaluation.
    pub fn mark_skip_downstream(&mut self) {
        self.pending_changes.skip_downstream = true;
    }

    /// Marks that selection changed
    pub fn mark_selection_changed(
        &mut self,
        previous_selection: Option<u64>,
        current_selection: Option<u64>,
    ) {
        self.pending_changes
            .mark_selection_changed(previous_selection, current_selection);
    }

    // --- Undo/Redo ---

    /// Undo the last command. Returns true if an undo was performed.
    pub fn undo(&mut self) -> bool {
        // Temporarily take the undo stack to avoid borrow conflict
        let mut stack = std::mem::take(&mut self.undo_stack);
        let result = stack.undo(&mut UndoContext {
            node_type_registry: &mut self.node_type_registry,
            active_network_name: &mut self.active_node_network_name,
        });
        self.undo_stack = stack;

        if let Some(refresh_mode) = result {
            self.apply_undo_refresh_mode(refresh_mode);
            true
        } else {
            false
        }
    }

    /// Redo the last undone command. Returns true if a redo was performed.
    pub fn redo(&mut self) -> bool {
        let mut stack = std::mem::take(&mut self.undo_stack);
        let result = stack.redo(&mut UndoContext {
            node_type_registry: &mut self.node_type_registry,
            active_network_name: &mut self.active_node_network_name,
        });
        self.undo_stack = stack;

        if let Some(refresh_mode) = result {
            self.apply_undo_refresh_mode(refresh_mode);
            true
        } else {
            false
        }
    }

    /// Push a new undo command onto the stack.
    pub fn push_command(&mut self, command: impl UndoCommand + 'static) {
        self.undo_stack.push(Box::new(command));
    }

    /// Apply the appropriate refresh after an undo/redo operation.
    fn apply_undo_refresh_mode(&mut self, mode: UndoRefreshMode) {
        match mode {
            UndoRefreshMode::Lightweight => {
                self.mark_lightweight_refresh();
            }
            UndoRefreshMode::NodeDataChanged(node_ids) => {
                for node_id in node_ids {
                    self.mark_node_data_changed(node_id);
                }
            }
            UndoRefreshMode::Full => {
                self.mark_full_refresh();
                // Reapply display policy so the display state matches what
                // the original mutation methods would have produced.
                self.apply_node_display_policy(None);
                // Re-validate network (updates derived state like output_type)
                self.validate_active_network();
            }
        }
        self.set_dirty(true);
    }

    /// Scope-aware variant of [`snapshot_node_data`]. Walks `scope_path` from
    /// `network_name` down through HOF `zone` networks to find the node, then
    /// serializes its data via the registered `node_data_saver`. An empty
    /// `scope_path` delegates to the existing top-level helper. Used by the
    /// scope-aware `set_node_network_data_scoped` to capture before/after JSON
    /// for the `SetNodeDataCommand` undo entry — see
    /// `doc/design_zones_ui.md` §"Mutation APIs grow a `scope_path` parameter".
    pub fn snapshot_node_data_scoped(
        &mut self,
        network_name: &str,
        scope_path: &[u64],
        node_id: u64,
    ) -> Option<serde_json::Value> {
        if scope_path.is_empty() {
            return self.snapshot_node_data(network_name, node_id);
        }

        // Look up the saver function via an immutable walk. `node_data_saver`
        // is a fn pointer (Copy), so we can extract it before the mutable
        // borrow below.
        let saver = {
            let mut current = self.node_type_registry.node_networks.get(network_name)?;
            for hof_id in scope_path {
                let node = current.nodes.get(hof_id)?;
                current = node.zone.as_ref()?;
            }
            let node = current.nodes.get(&node_id)?;
            let node_type_name = node.node_type_name.clone();

            if let Some(node_type) = self
                .node_type_registry
                .built_in_node_types
                .get(&node_type_name)
            {
                node_type.node_data_saver
            } else if let Some(other_network) =
                self.node_type_registry.node_networks.get(&node_type_name)
            {
                other_network.node_type.node_data_saver
            } else {
                return None;
            }
        };

        // Now mutable walk to call the saver on the body node's data.
        let network = self.get_scope_network_mut(scope_path)?;
        let node = network.nodes.get_mut(&node_id)?;
        saver(node.data.as_mut(), None).ok()
    }

    /// Serialize a node's data to JSON using the registered node_data_saver.
    /// Returns None if the node or network doesn't exist.
    pub fn snapshot_node_data(
        &mut self,
        network_name: &str,
        node_id: u64,
    ) -> Option<serde_json::Value> {
        // First, look up the saver function (needs immutable access).
        // node_data_saver is a fn pointer (Copy), so we can extract it before mutable borrow.
        let saver = {
            let node_type_name = self
                .node_type_registry
                .node_networks
                .get(network_name)?
                .nodes
                .get(&node_id)?
                .node_type_name
                .clone();

            if let Some(node_type) = self
                .node_type_registry
                .built_in_node_types
                .get(&node_type_name)
            {
                node_type.node_data_saver
            } else if let Some(other_network) =
                self.node_type_registry.node_networks.get(&node_type_name)
            {
                other_network.node_type.node_data_saver
            } else {
                return None;
            }
        };

        // Now get mutable access to the node's data to call the saver
        let network = self
            .node_type_registry
            .node_networks
            .get_mut(network_name)?;
        let node = network.nodes.get_mut(&node_id)?;
        saver(node.data.as_mut(), None).ok()
    }

    /// Snapshot a node's full state for undo purposes.
    pub fn snapshot_node(
        &mut self,
        network_name: &str,
        node_id: u64,
    ) -> Option<super::undo::snapshot::NodeSnapshot> {
        use super::undo::snapshot::{ArgumentSnapshot, NodeSnapshot};

        let data_json = self.snapshot_node_data(network_name, node_id)?;
        let network = self.node_type_registry.node_networks.get(network_name)?;
        let node = network.nodes.get(&node_id)?;

        Some(NodeSnapshot {
            node_id: node.id,
            node_type_name: node.node_type_name.clone(),
            position: node.position,
            custom_name: node.custom_name.clone(),
            node_data_json: data_json,
            arguments: node
                .arguments
                .iter()
                .map(|arg| ArgumentSnapshot {
                    incoming_wires: arg.incoming_wires.clone(),
                })
                .collect(),
        })
    }

    /// Snapshot an entire network to a serializable form for undo purposes.
    pub fn snapshot_network(
        &mut self,
        network_name: &str,
    ) -> Option<super::serialization::node_networks_serialization::SerializableNodeNetwork> {
        use super::serialization::node_networks_serialization::node_network_to_serializable;

        let (built_in_types, node_networks) = (
            &self.node_type_registry.built_in_node_types,
            &mut self.node_type_registry.node_networks,
        );

        let network = node_networks.get_mut(network_name)?;
        node_network_to_serializable(network, built_in_types, None).ok()
    }

    // Generates the scene to be rendered according to the displayed nodes of the active node network
    pub fn refresh(&mut self, changes: &StructureDesignerChanges) {
        // Clear pending changes at the start of refresh
        self.pending_changes.clear();

        // Check if node_network_name exists and clone it to avoid borrow conflicts
        let node_network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => {
                // No active network — clear the scene so the viewport doesn't
                // keep rendering stale output from a previously active network.
                self.last_generated_structure_designer_scene = StructureDesignerScene::new();
                return;
            }
        };

        match changes.mode {
            RefreshMode::Lightweight => {
                // Lightweight refresh - only update gadget tessellation without re-evaluation
                // The gadget is already active and should not be recreated
                self.refresh_scene_dependent_node_data();
                if let Some(gadget) = &self.gadget {
                    self.last_generated_structure_designer_scene.tessellatable =
                        Some(gadget.as_tessellatable());
                }
            }

            RefreshMode::Full => {
                // Full refresh - re-evaluate everything
                self.refresh_full(&node_network_name);
            }

            RefreshMode::Partial => {
                // Partial refresh - use tracked changes
                self.refresh_partial(&node_network_name, changes);
            }
        }
    }

    // Full refresh implementation - re-evaluates all displayed nodes
    fn refresh_full(&mut self, node_network_name: &str) {
        let (active_node_id, displayed_node_ids) = {
            let network = match self.node_type_registry.node_networks.get(node_network_name) {
                Some(network) => network,
                None => return,
            };

            // Create new scene with empty node_data HashMap and invisibility cache.
            self.last_generated_structure_designer_scene = StructureDesignerScene::new();
            self.last_generated_structure_designer_scene.active_node_id = network.active_node_id;

            // Clear input caches on all displayed nodes (full refresh: upstream may have changed)
            for node_entry in &network.displayed_nodes {
                if let Some(data) = network.get_node_network_data(*node_entry.0) {
                    data.clear_input_cache();
                }
            }

            // Snapshot the (id, display_type) pairs so the closure below can
            // iterate without re-borrowing `self` while `with_eval_context`
            // owns the mutable borrow on it.
            let displayed: Vec<(u64, super::node_network::NodeDisplayType)> = network
                .displayed_nodes
                .iter()
                .map(|(id, state)| (*id, state.display_type))
                .collect();
            (network.active_node_id, displayed)
        };

        // Track selected node's unit cell
        let mut selected_node_unit_cell: Option<UnitCellStruct> = None;
        let mut new_scenes: Vec<(
            u64,
            crate::structure_designer::structure_designer_scene::NodeSceneData,
        )> = Vec::with_capacity(displayed_node_ids.len());

        self.with_eval_context(false, |evaluator, registry, prefs, context| {
            for (node_id, display_type) in &displayed_node_ids {
                let node_data = evaluator.generate_scene(
                    node_network_name,
                    *node_id,
                    *display_type,
                    registry,
                    &prefs.geometry_visualization_preferences,
                    context,
                );

                if Some(*node_id) == active_node_id {
                    selected_node_unit_cell = node_data.unit_cell.clone();
                }
                new_scenes.push((*node_id, node_data));
            }
        });

        for (node_id, node_data) in new_scenes {
            self.last_generated_structure_designer_scene
                .node_data
                .insert(node_id, node_data);
        }

        // Set the selected node's unit cell as global scene property
        // Note: eval_cache is now accessed directly from node_data via get_selected_node_eval_cache()
        self.last_generated_structure_designer_scene.unit_cell = selected_node_unit_cell;

        self.refresh_scene_dependent_node_data();

        // Recreate the gadget for the selected node
        if let Some(network) = self.node_type_registry.node_networks.get(node_network_name) {
            self.gadget = network.provide_gadget(self);
        }

        if let Some(gadget) = &self.gadget {
            self.last_generated_structure_designer_scene.tessellatable =
                Some(gadget.as_tessellatable());
        }
    }

    // Partial refresh implementation - only re-evaluates affected nodes
    // Uses invisible node caching for ultra-fast visibility changes
    fn refresh_partial(&mut self, node_network_name: &str, changes: &StructureDesignerChanges) {
        let network = match self.node_type_registry.node_networks.get(node_network_name) {
            Some(network) => network,
            None => return,
        };

        // Clone necessary data before mutable borrows to avoid borrow checker issues
        let active_node_id = network.active_node_id;
        self.last_generated_structure_designer_scene.active_node_id = active_node_id;

        // Step 1: Cache nodes that became invisible
        for &node_id in &changes.visibility_changed {
            if !network.displayed_nodes.contains_key(&node_id) {
                // Node became invisible - move to cache for potential future restoration
                self.last_generated_structure_designer_scene
                    .move_to_cache(node_id);
            }
        }

        // Step 2: Compute transitive dependencies of data changes and invalidate cache.
        // `affected_by_data_changes` is scope-aware: it may contain body-internal
        // NodeRefs as well as top-level ones. Top-level ids drive the displayed-
        // node intersection in Step 4 below — the synthetic body-node → HOF edge
        // in `build_scope_reverse_dependency_map` guarantees that any body edit
        // lifts dirtiness up to a top-level HOF, so displayed nodes downstream
        // of an HOF are reached even when only its body changed.
        let affected_by_data_changes: HashSet<NodeRef> = if !changes.data_changed.is_empty() {
            if changes.skip_downstream {
                // During interactive drag: only re-evaluate the directly changed nodes,
                // skip computing downstream dependents for better performance.
                let directly_changed = changes.data_changed.clone();
                let top_level_ids: HashSet<u64> = directly_changed
                    .iter()
                    .filter(|nr| nr.is_top_level())
                    .map(|nr| nr.node_id)
                    .collect();
                self.last_generated_structure_designer_scene
                    .invalidate_cached_nodes(&top_level_ids);
                directly_changed
            } else {
                let affected = compute_downstream_dependents(network, &changes.data_changed);
                // Clear input caches on affected nodes (upstream may have changed).
                // Walk scope_path to land on the right body — body-internal node ids
                // can collide with top-level ids (per-body `next_node_id` counters).
                for node_ref in &affected {
                    if let Some(data) = find_node_data_at_scope(network, node_ref) {
                        data.clear_input_cache();
                    }
                }
                let top_level_ids: HashSet<u64> = affected
                    .iter()
                    .filter(|nr| nr.is_top_level())
                    .map(|nr| nr.node_id)
                    .collect();
                self.last_generated_structure_designer_scene
                    .invalidate_cached_nodes(&top_level_ids);
                affected
            }
        } else {
            HashSet::new()
        };

        // Step 3: Restore nodes that became visible from cache (if possible)
        // (At this point we have data in the cache that actually can be restored.)
        let mut nodes_needing_evaluation = HashSet::new();

        for &node_id in &changes.visibility_changed {
            if network.displayed_nodes.contains_key(&node_id) {
                // Node became visible - try to restore from cache (ultra-fast path)
                let restored = self
                    .last_generated_structure_designer_scene
                    .restore_from_cache(node_id);

                if !restored {
                    // Not in cache (or was invalidated) - needs re-evaluation
                    nodes_needing_evaluation.insert(node_id);
                }
                // Note: If restored successfully, eval_cache is preserved in node_data
                // and accessible via get_selected_node_eval_cache() for gadget creation
            }
        }

        // Step 4: Add visible nodes affected by data changes to evaluation set.
        // Only top-level NodeRefs participate — body-internal nodes aren't
        // separately displayed today, and body dirtiness has already been
        // lifted to its enclosing top-level HOF by the synthetic edge in
        // `build_scope_reverse_dependency_map`.
        for node_ref in &affected_by_data_changes {
            if node_ref.is_top_level()
                && network.displayed_nodes.contains_key(&node_ref.node_id)
            {
                nodes_needing_evaluation.insert(node_ref.node_id);
            }
        }

        // Step 4.5: Handle selection changes - re-evaluate affected nodes to update from_selected_node flag
        if changes.selection_changed {
            // Add previous selected node (needs from_selected_node set to false)
            if let Some(prev_node_id) = changes.previous_selection {
                if network.displayed_nodes.contains_key(&prev_node_id) {
                    nodes_needing_evaluation.insert(prev_node_id);
                }
            }
            // Add current selected node (needs from_selected_node set to true)
            if let Some(curr_node_id) = changes.current_selection {
                if network.displayed_nodes.contains_key(&curr_node_id) {
                    nodes_needing_evaluation.insert(curr_node_id);
                }
            }
        }

        // Track selected node's unit cell
        let mut selected_node_unit_cell: Option<UnitCellStruct> = None;

        // Step 5: Re-evaluate nodes that need it (skip if empty)
        if !nodes_needing_evaluation.is_empty() {
            // Snapshot (id, display_type) pairs upfront so the closure body
            // does not need to re-borrow `self.node_type_registry`.
            let to_evaluate: Vec<(u64, super::node_network::NodeDisplayType)> = {
                let network = match self.node_type_registry.node_networks.get(node_network_name) {
                    Some(network) => network,
                    None => return,
                };
                nodes_needing_evaluation
                    .iter()
                    .filter_map(|&node_id| {
                        network
                            .displayed_nodes
                            .get(&node_id)
                            .map(|state| (node_id, state.display_type))
                    })
                    .collect()
            };

            let mut new_scenes: Vec<(
                u64,
                crate::structure_designer::structure_designer_scene::NodeSceneData,
            )> = Vec::with_capacity(to_evaluate.len());

            self.with_eval_context(false, |evaluator, registry, prefs, context| {
                for (node_id, display_type) in &to_evaluate {
                    let node_data = evaluator.generate_scene(
                        node_network_name,
                        *node_id,
                        *display_type,
                        registry,
                        &prefs.geometry_visualization_preferences,
                        context,
                    );

                    if Some(*node_id) == active_node_id {
                        selected_node_unit_cell = node_data.unit_cell.clone();
                    }
                    new_scenes.push((*node_id, node_data));
                }
            });

            for (node_id, node_data) in new_scenes {
                self.last_generated_structure_designer_scene
                    .node_data
                    .insert(node_id, node_data);
            }

            // Update the selected node's unit cell if it was re-evaluated
            // Note: eval_cache is now accessed directly from node_data via get_selected_node_eval_cache()
            if selected_node_unit_cell.is_some() {
                self.last_generated_structure_designer_scene.unit_cell = selected_node_unit_cell;
            }
        }

        self.refresh_scene_dependent_node_data();

        // Always refresh the gadget (simplest approach - handles all cases)
        // This ensures gadget is updated when:
        // - Selected node was re-evaluated
        // - Selected node was restored from cache
        // - Selection changed
        // - Node with gadget becomes node without gadget (gadget disappears)
        if let Some(network) = self.node_type_registry.node_networks.get(node_network_name) {
            self.gadget = network.provide_gadget(self);
            if let Some(gadget) = &self.gadget {
                self.last_generated_structure_designer_scene.tessellatable =
                    Some(gadget.as_tessellatable());
            } else {
                // No gadget for selected node - clear tessellatable
                self.last_generated_structure_designer_scene.tessellatable = None;
            }
        }
    }

    // node network methods

    pub fn add_new_node_network(&mut self) {
        // Generate a unique name. Skip any name already taken anywhere in the
        // user-type namespace (networks, user record defs, built-in record
        // defs, built-in node types) so the auto-generated name is never a
        // collision.
        let mut name = "UNTITLED".to_string();
        let mut i = 1;
        while self.node_type_registry.name_is_taken(&name) {
            name = format!("UNTITLED{}", i);
            i += 1;
        }

        // Capture previous active network for undo
        let previous_active_network = self.active_node_network_name.clone();

        self.add_node_network(&name);
        // Mark design as dirty since we added a new network
        self.set_dirty(true);
        // Adding a network is a structural change requiring full refresh
        self.mark_full_refresh();

        // Push undo command
        self.push_command(super::undo::commands::add_network::AddNetworkCommand {
            network_name: name,
            previous_active_network,
        });
    }

    /// Add a named node network and push an undo command.
    /// Used by the API layer for user-initiated "add network with name" actions.
    pub fn add_node_network_with_undo(
        &mut self,
        node_network_name: &str,
    ) -> Result<(), super::identifier::InvalidNameReason> {
        super::identifier::is_valid_user_name(node_network_name)?;
        let previous_active_network = self.active_node_network_name.clone();
        self.add_node_network(node_network_name);
        self.set_dirty(true);
        self.mark_full_refresh();
        self.push_command(super::undo::commands::add_network::AddNetworkCommand {
            network_name: node_network_name.to_string(),
            previous_active_network,
        });
        Ok(())
    }

    pub fn add_node_network(&mut self, node_network_name: &str) {
        self.node_type_registry.add_node_network(NodeNetwork::new(
      NodeType {
        name: node_network_name.to_string(),
        description: "".to_string(),
        summary: None,
        category: crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory::Custom,
        parameters: Vec::new(),
        output_pins: OutputPinDefinition::single(DataType::None),
        node_data_creator: || Box::new(CustomNodeData::default()),
        node_data_saver: generic_node_data_saver::<CustomNodeData>,
        node_data_loader: generic_node_data_loader::<CustomNodeData>,
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
      }
    ));
    }

    pub fn rename_node_network(&mut self, old_name: &str, new_name: &str) -> bool {
        // Reject names that violate the user-name rules (empty, backtick,
        // control chars, edge whitespace).
        if super::identifier::is_valid_user_name(new_name).is_err() {
            return false;
        }
        // Check if the old network exists and the new name doesn't already exist
        if !self.node_type_registry.node_networks.contains_key(old_name) {
            return false;
        }
        // The new name must not collide across the whole user-type namespace
        // (networks, user record defs, built-in record defs, built-in node
        // types). See `doc/design_atom_replace_rules_input.md` Phase A.
        if self.node_type_registry.name_is_taken(new_name) {
            return false;
        }

        // Core rename (registry, active name, node type refs, backtick refs)
        super::undo::commands::rename_helpers::apply_rename_core(
            &mut self.node_type_registry,
            &mut self.active_node_network_name,
            old_name,
            new_name,
        );

        // Navigation history (not available in UndoContext)
        self.navigation_history.rename_network(old_name, new_name);

        // Clipboard node_type_names (not available in UndoContext)
        if let Some(ref mut clipboard) = self.clipboard {
            for node in clipboard.nodes.values_mut() {
                if node.node_type_name == old_name {
                    node.node_type_name = new_name.to_string();
                }
            }
        }

        self.set_dirty(true);
        self.mark_full_refresh();

        self.push_command(
            super::undo::commands::rename_network::RenameNetworkCommand {
                old_name: old_name.to_string(),
                new_name: new_name.to_string(),
            },
        );

        true
    }

    pub fn rename_namespace(&mut self, old_prefix: &str, new_prefix: &str) -> bool {
        // Collect affected networks: names starting with "old_prefix."
        let prefix_dot = format!("{}.", old_prefix);
        let affected: Vec<String> = self
            .node_type_registry
            .node_networks
            .keys()
            .filter(|name| name.starts_with(&prefix_dot))
            .cloned()
            .collect();

        if affected.is_empty() {
            return false;
        }

        // Compute rename pairs and validate
        let new_prefix_dot = format!("{}.", new_prefix);
        let mut rename_pairs: Vec<(String, String)> = Vec::new();
        for old_name in &affected {
            let suffix = &old_name[prefix_dot.len()..];
            let new_name = format!("{}{}", new_prefix_dot, suffix);

            // Check for collision with existing network or built-in type
            if self
                .node_type_registry
                .node_networks
                .contains_key(&new_name)
                && !affected.contains(&new_name)
            {
                return false;
            }
            if self
                .node_type_registry
                .built_in_node_types
                .contains_key(&new_name)
            {
                return false;
            }
            rename_pairs.push((old_name.clone(), new_name));
        }

        // Perform all renames
        for (old_name, new_name) in &rename_pairs {
            super::undo::commands::rename_helpers::apply_rename_core(
                &mut self.node_type_registry,
                &mut self.active_node_network_name,
                old_name,
                new_name,
            );
        }

        // Update navigation history for all renames
        for (old_name, new_name) in &rename_pairs {
            self.navigation_history.rename_network(old_name, new_name);
        }

        // Update clipboard for all renames
        if let Some(ref mut clipboard) = self.clipboard {
            for node in clipboard.nodes.values_mut() {
                for (old_name, new_name) in &rename_pairs {
                    if node.node_type_name == *old_name {
                        node.node_type_name = new_name.clone();
                        break;
                    }
                }
            }
        }

        self.set_dirty(true);
        self.mark_full_refresh();

        self.push_command(
            super::undo::commands::rename_namespace::RenameNamespaceCommand {
                renames: rename_pairs,
            },
        );

        true
    }

    /// Check if any network outside `targets` references any network in `targets`.
    /// Returns Ok(()) if safe to delete, or Err with details if blocked.
    ///
    /// Intra-set references (networks in `targets` referencing each other) are not blocking —
    /// they're all being deleted together.
    fn check_delete_references(
        &self,
        targets: &std::collections::HashSet<&str>,
    ) -> Result<(), String> {
        let mut referencing_networks = Vec::new();
        for (current_network_name, network) in self.node_type_registry.node_networks.iter() {
            // Skip networks that are themselves being deleted
            if targets.contains(current_network_name.as_str()) {
                continue;
            }
            // Walk recursively into HOF zone bodies — a body-internal
            // reference to a target still blocks the deletion.
            let mut found = false;
            crate::structure_designer::node_network::walk_all_nodes(network, &mut |node| {
                if !found && targets.contains(node.node_type_name.as_str()) {
                    found = true;
                }
            });
            if found {
                referencing_networks.push(current_network_name.clone());
            }
        }

        if referencing_networks.is_empty() {
            Ok(())
        } else {
            let target_names: Vec<&str> = targets.iter().copied().collect();
            Err(format!(
                "Cannot delete {} because referenced by nodes in: {}",
                if target_names.len() == 1 {
                    format!("network '{}'", target_names[0])
                } else {
                    format!("networks under prefix ({})", target_names.join(", "))
                },
                referencing_networks.join(", ")
            ))
        }
    }

    pub fn delete_node_network(&mut self, network_name: &str) -> Result<(), String> {
        if !self
            .node_type_registry
            .node_networks
            .contains_key(network_name)
        {
            return Err(format!("Node network '{}' does not exist", network_name));
        }

        // Check references using shared helper
        let targets: std::collections::HashSet<&str> =
            std::collections::HashSet::from([network_name]);
        self.check_delete_references(&targets)?;

        // Snapshot the network before deletion (for undo)
        let network_snapshot = self.snapshot_network(network_name);

        // Capture active network before deletion
        let active_network_before = self.active_node_network_name.clone();

        // Remove the network from the registry
        self.node_type_registry.node_networks.remove(network_name);

        // Update the active_node_network_name if it was the deleted network
        if let Some(active_name) = &self.active_node_network_name {
            if active_name == network_name {
                self.active_node_network_name = None;
            }
        }

        // Remove the deleted network from navigation history
        self.navigation_history.remove_network(network_name);

        // Clear clipboard if it references the deleted network type
        if let Some(ref clipboard) = self.clipboard {
            if clipboard
                .nodes
                .values()
                .any(|n| n.node_type_name == network_name)
            {
                self.clipboard = None;
            }
        }

        // Capture active network after deletion
        let active_network_after = self.active_node_network_name.clone();

        self.set_dirty(true);
        self.mark_full_refresh();

        // Push undo command if we successfully snapshotted
        if let Some(snapshot) = network_snapshot {
            self.push_command(
                super::undo::commands::delete_network::DeleteNetworkCommand {
                    network_name: network_name.to_string(),
                    network_snapshot: snapshot,
                    active_network_before,
                    active_network_after,
                },
            );
        }

        Ok(())
    }

    pub fn delete_namespace(&mut self, prefix: &str) -> Result<(), String> {
        // Collect affected networks: names starting with "prefix."
        let prefix_dot = format!("{}.", prefix);
        let affected: Vec<String> = self
            .node_type_registry
            .node_networks
            .keys()
            .filter(|name| name.starts_with(&prefix_dot))
            .cloned()
            .collect();

        if affected.is_empty() {
            return Err(format!("No networks found under namespace '{}'", prefix));
        }

        // Check references: only block on references from outside the set
        let targets: std::collections::HashSet<&str> =
            affected.iter().map(|s| s.as_str()).collect();
        self.check_delete_references(&targets)?;

        // Snapshot all affected networks
        let mut network_snapshots = Vec::new();
        for name in &affected {
            if let Some(snapshot) = self.snapshot_network(name) {
                network_snapshots.push((name.clone(), snapshot));
            }
        }

        let active_network_before = self.active_node_network_name.clone();

        // Remove all affected networks
        for name in &affected {
            self.node_type_registry.node_networks.remove(name);
        }

        // Update active network if it was under the prefix
        if let Some(active_name) = &self.active_node_network_name {
            if active_name.starts_with(&prefix_dot) {
                self.active_node_network_name = None;
            }
        }

        // Remove from navigation history
        for name in &affected {
            self.navigation_history.remove_network(name);
        }

        // Clear clipboard if it references any deleted network
        if let Some(ref clipboard) = self.clipboard {
            if clipboard
                .nodes
                .values()
                .any(|n| targets.contains(n.node_type_name.as_str()))
            {
                self.clipboard = None;
            }
        }

        let active_network_after = self.active_node_network_name.clone();

        self.set_dirty(true);
        self.mark_full_refresh();

        self.push_command(
            super::undo::commands::delete_namespace::DeleteNamespaceCommand {
                network_snapshots,
                active_network_before,
                active_network_after,
            },
        );

        Ok(())
    }

    // ---------------------------------------------------------------
    // Record type defs (see doc/design_record_types.md, Phase 2)
    // ---------------------------------------------------------------

    /// Add a new record type def. Validates name uniqueness, distinct field
    /// names, and acyclic references. Pushes an undo command on success.
    /// Marks the design dirty and triggers a full refresh so the active
    /// network's validation re-runs against the new registry contents.
    pub fn add_record_type_def(
        &mut self,
        def: super::node_type_registry::RecordTypeDef,
    ) -> Result<(), super::node_type_registry::RecordTypeDefError> {
        let def_clone = def.clone();
        self.node_type_registry.add_record_type_def(def)?;
        self.set_dirty(true);
        self.mark_full_refresh();
        self.push_command(
            super::undo::commands::add_record_type_def::AddRecordTypeDefCommand { def: def_clone },
        );
        Ok(())
    }

    /// Delete a record type def. Snapshots every network beforehand so undo
    /// can restore wires that get disconnected by the post-delete repair pass.
    /// Errors out if the def doesn't exist.
    pub fn delete_record_type_def(
        &mut self,
        name: &str,
    ) -> Result<(), super::node_type_registry::RecordTypeDefError> {
        if !self.node_type_registry.record_type_defs.contains_key(name) {
            return Err(super::node_type_registry::RecordTypeDefError::NotFound(
                name.to_string(),
            ));
        }

        // Snapshot every network before delete (the conservative choice — any
        // network may carry a `Record(Named(name))` reference at any depth).
        let snapshots =
            super::undo::commands::delete_record_type_def::snapshot_all_networks_for_record_def_change(
                &mut self.node_type_registry,
            );

        // Remove the def, then run repair on every network so wires whose
        // pin-types now resolve via a dangling reference are disconnected.
        let def = self
            .node_type_registry
            .delete_record_type_def(name)
            .expect("contains_key checked above");

        let names: Vec<String> = self
            .node_type_registry
            .node_networks
            .keys()
            .cloned()
            .collect();
        for n in names {
            if let Some(mut network) = self.node_type_registry.node_networks.remove(&n) {
                self.node_type_registry.repair_node_network(&mut network);
                self.node_type_registry.node_networks.insert(n, network);
            }
        }

        self.set_dirty(true);
        self.mark_full_refresh();
        self.push_command(
            super::undo::commands::delete_record_type_def::DeleteRecordTypeDefCommand {
                def,
                affected_network_snapshots: snapshots,
            },
        );

        Ok(())
    }

    /// Rename a record type def. The registry-level call validates uniqueness
    /// and walks every embedded `Named(old)` reference. No wires are
    /// disconnected by a rename — every reference resolves to the same
    /// schema, just under a new name — so no per-network snapshot is needed.
    /// We still run `repair_node_network` on every network so that record
    /// nodes whose `custom_node_type` was cleared by the rename walker get
    /// their pin layouts repopulated against the renamed def.
    pub fn rename_record_type_def(
        &mut self,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), super::node_type_registry::RecordTypeDefError> {
        self.node_type_registry
            .rename_record_type_def(old_name, new_name)?;

        let names: Vec<String> = self
            .node_type_registry
            .node_networks
            .keys()
            .cloned()
            .collect();
        for n in names {
            if let Some(mut network) = self.node_type_registry.node_networks.remove(&n) {
                self.node_type_registry.repair_node_network(&mut network);
                self.node_type_registry.node_networks.insert(n, network);
            }
        }

        self.set_dirty(true);
        self.mark_full_refresh();
        self.push_command(
            super::undo::commands::rename_record_type_def::RenameRecordTypeDefCommand {
                old_name: old_name.to_string(),
                new_name: new_name.to_string(),
            },
        );
        Ok(())
    }

    /// Replace the field list of an existing record type def. Snapshots every
    /// network beforehand and re-runs repair afterward, just like delete.
    pub fn update_record_type_def(
        &mut self,
        name: &str,
        new_fields: Vec<(String, DataType)>,
    ) -> Result<(), super::node_type_registry::RecordTypeDefError> {
        // Capture old fields for undo *before* the update overwrites them.
        let old_fields = match self.node_type_registry.record_type_defs.get(name) {
            Some(def) => def.fields.clone(),
            None => {
                return Err(super::node_type_registry::RecordTypeDefError::NotFound(
                    name.to_string(),
                ));
            }
        };

        // Snapshot every network before the update — wires whose source type
        // no longer satisfies a retyped field will be disconnected by the
        // repair pass below.
        let snapshots =
            super::undo::commands::delete_record_type_def::snapshot_all_networks_for_record_def_change(
                &mut self.node_type_registry,
            );

        let new_fields_clone = new_fields.clone();
        self.node_type_registry
            .update_record_type_def(name, new_fields)?;

        let names: Vec<String> = self
            .node_type_registry
            .node_networks
            .keys()
            .cloned()
            .collect();
        for n in names {
            if let Some(mut network) = self.node_type_registry.node_networks.remove(&n) {
                self.node_type_registry.repair_node_network(&mut network);
                self.node_type_registry.node_networks.insert(n, network);
            }
        }

        self.set_dirty(true);
        self.mark_full_refresh();
        self.push_command(
            super::undo::commands::update_record_type_def::UpdateRecordTypeDefCommand {
                name: name.to_string(),
                old_fields,
                new_fields: new_fields_clone,
                network_snapshots_before: snapshots,
            },
        );

        Ok(())
    }

    pub fn add_node(&mut self, node_type_name: &str, position: DVec2) -> u64 {
        self.add_node_with_drag_source(node_type_name, position, None)
    }

    /// Scope-aware variant of [`add_node`]. With an empty `scope_path` falls
    /// through to [`add_node_with_drag_source`] (existing top-level behavior).
    /// With a non-empty `scope_path` adds the node directly into the named
    /// body via the scope-network helper — no display-policy / undo /
    /// validation orchestration is run, since those are top-level concerns
    /// that U4 will redo on the body level. Phase U2 of
    /// `doc/design_zones_ui.md`.
    pub fn add_node_scoped(
        &mut self,
        scope_path: &[u64],
        node_type_name: &str,
        position: DVec2,
        drag_source: Option<DragSource>,
    ) -> u64 {
        if scope_path.is_empty() {
            return self.add_node_with_drag_source(node_type_name, position, drag_source);
        }
        // Body scope: drag-source adaptation and per-node-type bookkeeping
        // (parameter `param_id` / param_name) are top-level concerns and not
        // exercised inside an HOF body in U2. Add the node with default data
        // and rely on U4 to re-introduce the richer adapter pass under a
        // scope-aware add path.
        let (num_parameters, node_data) =
            match self.node_type_registry.get_node_type(node_type_name) {
                Some(node_type) => {
                    let data_creator = &node_type.node_data_creator;
                    (node_type.parameters.len(), (data_creator)())
                }
                None => return 0,
            };
        let node_id = match self.get_scope_network_mut(scope_path) {
            Some(network) => network.add_node(node_type_name, position, num_parameters, node_data),
            None => return 0,
        };
        if node_id != 0 {
            // Initialize custom-node-type cache (incl. `ensure_zone_init` for
            // nested HOFs). Mirrors the split-borrow pattern in
            // `add_node_with_drag_source`'s top-level path: the read-only
            // maps and the mutable `node_networks` are *sibling fields* of
            // `node_type_registry`, so disjoint access through field
            // destructuring is allowed by the borrow checker. We walk the
            // scope path manually here (rather than through
            // `get_scope_network_mut`, which borrows all of `self`) so the
            // splits compose.
            let active_name = match self.active_node_network_name.as_ref() {
                Some(name) => name.clone(),
                None => return node_id,
            };
            let (built_in_types, record_type_defs, built_in_record_type_defs, node_networks) = (
                &self.node_type_registry.built_in_node_types,
                &self.node_type_registry.record_type_defs,
                &self.node_type_registry.built_in_record_type_defs,
                &mut self.node_type_registry.node_networks,
            );
            if let Some(top) = node_networks.get_mut(&active_name) {
                let mut current: Option<&mut NodeNetwork> = Some(top);
                for hof_id in scope_path {
                    current = match current {
                        Some(net) => net.nodes.get_mut(hof_id).and_then(|n| n.zone_mut()),
                        None => None,
                    };
                }
                if let Some(network) = current {
                    if let Some(node) = network.nodes.get_mut(&node_id) {
                        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
                            built_in_types,
                            record_type_defs,
                            built_in_record_type_defs,
                            node,
                            true,
                        );
                    }
                }
            }
            self.set_dirty(true);
        }
        node_id
    }

    /// Variant of `add_node` that pre-configures the new node's type
    /// properties to match a dragged source pin.
    ///
    /// Behavior on `drag_source = None` is identical to `add_node`. With a
    /// drag source, the node's stored data is asked to adapt via
    /// `NodeData::adapt_for_drag_source`; the adapter's claim is verified by
    /// re-running the static-pin compatibility check against the resolved
    /// node type, so an over-promising adapter is silently dropped to
    /// default data rather than producing a mis-typed node. See
    /// `doc/design_drag_aware_add_node.md`.
    pub fn add_node_with_drag_source(
        &mut self,
        node_type_name: &str,
        position: DVec2,
        drag_source: Option<DragSource>,
    ) -> u64 {
        // Early return if active_node_network_name is None
        let node_network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return 0,
        };
        // First get the node type info
        let (num_parameters, mut node_data) =
            match self.node_type_registry.get_node_type(node_type_name) {
                Some(node_type) => {
                    let data_creator = &node_type.node_data_creator;
                    (node_type.parameters.len(), (data_creator)())
                }
                None => return 0,
            };

        // Drag-source adapter: only applied when the adapted node still
        // statically matches the drag under the *strict* predicate (mirrors
        // the popup filter's Stage-2 verification — rejects matches that
        // would only land via scalar→collection broadcast). Protects
        // callers that bypass the popup (CLI, direct API, stale popups
        // after concurrent network mutations) and keeps create-time
        // behavior in lockstep with the picker. See
        // `doc/design_drag_aware_add_node.md` §"Asymmetric verification".
        if let Some(drag) = drag_source.as_ref() {
            if let Some(node_type) = self.node_type_registry.get_node_type(node_type_name) {
                if let Some(adapted) = node_data.adapt_for_drag_source(
                    &drag.source_type,
                    drag.direction,
                    &self.node_type_registry,
                ) {
                    let resolved = adapted
                        .calculate_custom_node_type(node_type)
                        .unwrap_or_else(|| node_type.clone());
                    if crate::structure_designer::node_type_registry::static_match_strict(
                        &resolved,
                        &drag.source_type,
                        drag.direction,
                        &self.node_type_registry,
                    ) {
                        node_data = adapted;
                    }
                }
            }
        }

        // Capture counters before node creation (for undo)
        let (next_node_id_before, next_param_id_before) = self
            .node_type_registry
            .node_networks
            .get(&node_network_name)
            .map(|n| (n.next_node_id, n.next_param_id))
            .unwrap_or((0, 0));

        // Special handling for parameter nodes
        let mut assigned_param_id: Option<u64> = None;
        if node_type_name == "parameter" {
            if let Some(node_network) = self
                .node_type_registry
                .node_networks
                .get_mut(&node_network_name)
            {
                let current_param_count = node_network.node_type.parameters.len();

                // Assign a unique param_id from the network's counter
                let param_id = node_network.next_param_id;
                node_network.next_param_id += 1;
                assigned_param_id = Some(param_id);

                // Downcast to ParameterData and set properties
                if let Some(param_data) = node_data
                    .as_any_mut()
                    .downcast_mut::<crate::structure_designer::nodes::parameter::ParameterData>(
                ) {
                    param_data.param_id = Some(param_id); // Assign unique ID for wire preservation
                    param_data.param_name = format!("param{}", current_param_count);
                    param_data.sort_order = current_param_count as i32;
                }
            }
        }

        // Early return if the node network doesn't exist
        let node_id = self
            .node_type_registry
            .node_networks
            .get_mut(&node_network_name)
            .map(|node_network| {
                node_network.add_node(node_type_name, position, num_parameters, node_data)
            })
            .unwrap_or(0);

        // If we successfully added a node, initialize custom node type if needed
        if node_id != 0 {
            // Split the borrow to avoid conflicts
            let (built_in_types, record_type_defs, built_in_record_type_defs, node_networks) = (
                &self.node_type_registry.built_in_node_types,
                &self.node_type_registry.record_type_defs,
                &self.node_type_registry.built_in_record_type_defs,
                &mut self.node_type_registry.node_networks,
            );
            if let Some(network) = node_networks.get_mut(&node_network_name) {
                if let Some(node) = network.nodes.get_mut(&node_id) {
                    // Call the populate function with the split borrows
                    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
                        built_in_types,
                        record_type_defs,
                        built_in_record_type_defs,
                        node,
                        true,
                    );
                }
            }
        }

        // If we successfully added a node, apply the display policy with this node as dirty
        if node_id != 0 {
            // Mark design as dirty since we added a node
            self.set_dirty(true);

            // Create a HashSet with just the new node ID
            let mut dirty_nodes = HashSet::new();
            dirty_nodes.insert(node_id);

            // Track visibility change for the new node (it was set to visible in add_node)
            // This is needed because the node was made visible directly on node_network,
            // bypassing StructureDesigner.set_node_display which normally tracks this
            self.pending_changes.visibility_changed.insert(node_id);

            // Apply display policy considering only this node as dirty
            self.apply_node_display_policy(Some(&dirty_nodes));

            // Check if we need to validate the network
            let should_validate = node_type_name == "parameter" || {
                // Check if this node references an invalid node network
                self.node_type_registry
                    .node_networks
                    .get(node_type_name)
                    .map(|network| !network.valid)
                    .unwrap_or(false)
            };

            if should_validate {
                self.validate_active_network();
            }

            // Push undo command: snapshot the node after creation
            if let Some(node_data_json) = self.snapshot_node_data(&node_network_name, node_id) {
                let custom_name = self
                    .node_type_registry
                    .node_networks
                    .get(&node_network_name)
                    .and_then(|n| n.nodes.get(&node_id))
                    .and_then(|n| n.custom_name.clone());

                self.push_command(super::undo::commands::add_node::AddNodeCommand {
                    description: format!("Add {}", node_type_name),
                    network_name: node_network_name.clone(),
                    node_id,
                    node_type_name: node_type_name.to_string(),
                    position,
                    node_data_json,
                    custom_name,
                    num_parameters,
                    param_id: assigned_param_id,
                    next_param_id_before,
                    next_node_id_before,
                });
            }
        }

        node_id
    }

    pub fn duplicate_node(&mut self, node_id: u64) -> u64 {
        // Early return if active_node_network_name is None
        let node_network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return 0,
        };

        // Capture next_node_id before duplication (for undo)
        let next_node_id_before = self
            .node_type_registry
            .node_networks
            .get(&node_network_name)
            .map(|n| n.next_node_id)
            .unwrap_or(0);

        // Early return if the node network doesn't exist
        let new_node_id = self
            .node_type_registry
            .node_networks
            .get_mut(&node_network_name)
            .and_then(|node_network| node_network.duplicate_node(node_id))
            .unwrap_or(0);

        // If we successfully duplicated a node, apply the display policy with this node as dirty
        if new_node_id != 0 {
            // Mark design as dirty since we duplicated a node
            self.set_dirty(true);

            // Create a HashSet with just the new node ID
            let mut dirty_nodes = HashSet::new();
            dirty_nodes.insert(new_node_id);

            // Apply display policy considering only this node as dirty
            self.apply_node_display_policy(Some(&dirty_nodes));

            // Push undo command
            if let Some(node_snapshot) = self.snapshot_node(&node_network_name, new_node_id) {
                self.push_command(
                    super::undo::commands::duplicate_node::DuplicateNodeCommand {
                        description: format!("Duplicate {}", node_snapshot.node_type_name),
                        network_name: node_network_name.clone(),
                        new_node_id,
                        node_snapshot,
                        next_node_id_before,
                    },
                );
            }
        }

        self.mark_node_data_changed(node_id);

        new_node_id
    }

    /// Copies the currently selected nodes to the clipboard.
    /// Returns true if something was copied, false if selection was empty.
    pub fn copy_selection(&mut self) -> bool {
        let node_network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return false,
        };

        let active_network = match self
            .node_type_registry
            .node_networks
            .get(&node_network_name)
        {
            Some(network) => network,
            None => return false,
        };

        if active_network.selected_node_ids.is_empty() {
            return false;
        }

        // Compute centroid of selected nodes' positions
        let selected_ids = active_network.selected_node_ids.clone();
        let mut sum = DVec2::ZERO;
        let mut count = 0u64;
        for &id in &selected_ids {
            if let Some(node) = active_network.nodes.get(&id) {
                sum += node.position;
                count += 1;
            }
        }
        if count == 0 {
            return false;
        }
        let centroid = sum / count as f64;

        // Create clipboard and copy nodes centered at (0, 0)
        let mut clipboard = NodeNetwork::new_empty();
        clipboard.copy_nodes_from(active_network, &selected_ids, -centroid);
        self.clipboard = Some(clipboard);
        true
    }

    /// Pastes clipboard contents into the active network at the given position.
    /// Returns the list of newly created node IDs (empty if clipboard was empty).
    pub fn paste_at_position(&mut self, position: DVec2) -> Vec<u64> {
        let node_network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return vec![],
        };

        let clipboard = match &self.clipboard {
            Some(cb) => cb,
            None => return vec![],
        };

        let all_clipboard_ids: HashSet<u64> = clipboard.nodes.keys().copied().collect();
        if all_clipboard_ids.is_empty() {
            return vec![];
        }

        // Snapshot the clipboard since we need to borrow self mutably for the active network
        let mut clipboard_snapshot = NodeNetwork::new_empty();
        clipboard_snapshot.copy_nodes_from(clipboard, &all_clipboard_ids, DVec2::ZERO);
        let snapshot_ids: HashSet<u64> = clipboard_snapshot.nodes.keys().copied().collect();

        // Capture next_node_id before paste for undo
        let next_node_id_before = self
            .node_type_registry
            .node_networks
            .get(&node_network_name)
            .map(|n| n.next_node_id)
            .unwrap_or(0);

        let active_network = match self
            .node_type_registry
            .node_networks
            .get_mut(&node_network_name)
        {
            Some(network) => network,
            None => return vec![],
        };

        let new_ids = active_network.copy_nodes_from(&clipboard_snapshot, &snapshot_ids, position);

        // Select the pasted nodes
        active_network.select_nodes(new_ids.clone());

        // Mark design as dirty and trigger full refresh
        self.set_dirty(true);
        self.mark_full_refresh();

        // Push undo command: snapshot all pasted nodes and their internal wires
        if !new_ids.is_empty() {
            let new_id_set: HashSet<u64> = new_ids.iter().copied().collect();
            let mut pasted_nodes = Vec::new();
            let mut pasted_wires = Vec::new();
            let mut display_states = Vec::new();

            for &node_id in &new_ids {
                if let Some(snap) = self.snapshot_node(&node_network_name, node_id) {
                    pasted_nodes.push(snap);
                }
            }

            // Collect wires between pasted nodes and display states
            if let Some(network) = self
                .node_type_registry
                .node_networks
                .get(&node_network_name)
            {
                for &node_id in &new_ids {
                    if let Some(node) = network.nodes.get(&node_id) {
                        for (param_idx, arg) in node.arguments.iter().enumerate() {
                            for (src_id, src_pin) in arg.iter_source_pins() {
                                if new_id_set.contains(&src_id) {
                                    pasted_wires.push(super::undo::snapshot::WireSnapshot {
                                        source_node_id: src_id,
                                        source_output_pin_index: src_pin,
                                        dest_node_id: node_id,
                                        dest_param_index: param_idx,
                                    });
                                }
                            }
                        }
                    }

                    if let Some(state) = network.displayed_nodes.get(&node_id) {
                        display_states.push((node_id, state.display_type));
                    }
                }
            }

            self.push_command(super::undo::commands::paste_nodes::PasteNodesCommand {
                network_name: node_network_name.clone(),
                pasted_nodes,
                pasted_wires,
                display_states,
                next_node_id_before,
            });
        }

        new_ids
    }

    /// Cuts the currently selected nodes (copy + delete).
    /// Returns true if something was cut.
    pub fn cut_selection(&mut self) -> bool {
        if !self.copy_selection() {
            return false;
        }
        self.delete_selected();
        true
    }

    /// Returns true if the clipboard contains content.
    pub fn has_clipboard_content(&self) -> bool {
        self.clipboard.is_some()
    }

    pub fn move_node(&mut self, node_id: u64, position: DVec2) {
        self.move_node_scoped(&[], node_id, position);
    }

    /// Scope-aware variant of [`move_node`]. With an empty `scope_path` the
    /// targeted network is the active top-level network (existing behavior);
    /// with a non-empty path the move is applied inside the named HOF body.
    /// Phase U2 of `doc/design_zones_ui.md` — see §"Phase U2".
    pub fn move_node_scoped(&mut self, scope_path: &[u64], node_id: u64, position: DVec2) {
        if let Some(node_network) = self.get_scope_network_mut(scope_path) {
            node_network.move_node(node_id, position);
            // Mark design as dirty since we moved a node
            self.set_dirty(true);
        }
    }

    pub fn can_connect_nodes(
        &self,
        source_node_id: u64,
        source_output_pin_index: i32,
        dest_node_id: u64,
        dest_param_index: usize,
    ) -> bool {
        // Early return if active_node_network_name is None
        let node_network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return false,
        };

        // Get the network
        let network = match self.node_type_registry.node_networks.get(node_network_name) {
            Some(network) => network,
            None => return false,
        };

        network.can_connect_nodes(
            source_node_id,
            source_output_pin_index,
            dest_node_id,
            dest_param_index,
            &self.node_type_registry,
        )
    }

    pub fn connect_nodes(
        &mut self,
        source_node_id: u64,
        source_output_pin_index: i32,
        dest_node_id: u64,
        dest_param_index: usize,
    ) {
        // Early return if active_node_network_name is None
        let node_network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return,
        };
        // First validate the connection
        let dest_param_is_multi = {
            // Get the network
            let network = match self.node_type_registry.node_networks.get(node_network_name) {
                Some(network) => network,
                None => return,
            };

            // Get the destination node
            let dest_node = match network.nodes.get(&dest_node_id) {
                Some(node) => node,
                None => return,
            };

            // Get the node type and check parameter
            match self.node_type_registry.get_node_type_for_node(dest_node) {
                Some(node_type) => {
                    if dest_param_index >= node_type.parameters.len() {
                        return;
                    }
                    node_type.parameters[dest_param_index].data_type.is_array()
                }
                None => return,
            }
        };

        // Capture the existing wire on this pin before connecting (for undo)
        let replaced_wire = if !dest_param_is_multi {
            if let Some(network) = self.node_type_registry.node_networks.get(node_network_name) {
                if let Some(dest_node) = network.nodes.get(&dest_node_id) {
                    if let Some(arg) = dest_node.arguments.get(dest_param_index) {
                        if !arg.is_empty() {
                            // There's an existing wire that will be replaced
                            arg.iter_source_pins().next().map(|(src_id, src_pin)| {
                                super::undo::snapshot::WireSnapshot {
                                    source_node_id: src_id,
                                    source_output_pin_index: src_pin,
                                    dest_node_id,
                                    dest_param_index,
                                }
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let network_name_owned = node_network_name.clone();

        // Then make the connection
        if let Some(node_network) = self
            .node_type_registry
            .node_networks
            .get_mut(&network_name_owned)
        {
            node_network.connect_nodes(
                source_node_id,
                source_output_pin_index,
                dest_node_id,
                dest_param_index,
                dest_param_is_multi,
            );

            // Mark design as dirty since we connected nodes
            self.set_dirty(true);
            // Mark the destination node as having data changed (new input connection)
            self.mark_node_data_changed(dest_node_id);

            // Create a HashSet with the source and destination nodes marked as dirty
            let mut dirty_nodes = HashSet::new();
            dirty_nodes.insert(source_node_id);
            dirty_nodes.insert(dest_node_id);

            // Apply display policy considering only these nodes as dirty
            self.apply_node_display_policy(Some(&dirty_nodes));
        }

        // Push undo command
        self.push_command(super::undo::commands::connect_wire::ConnectWireCommand {
            network_name: network_name_owned,
            wire: super::undo::snapshot::WireSnapshot {
                source_node_id,
                source_output_pin_index,
                dest_node_id,
                dest_param_index,
            },
            replaced_wire,
        });
    }

    /// Scope-aware variant of [`can_connect_nodes`]. With an empty `scope_path`
    /// queries the active top-level network (today's behavior); a non-empty
    /// path queries the named HOF body. Phase U4 of `doc/design_zones_ui.md` —
    /// only intra-body wires are validated here (cross-scope wires land in
    /// U5 and have their own predicate).
    pub fn can_connect_nodes_scoped(
        &self,
        scope_path: &[u64],
        source_node_id: u64,
        source_output_pin_index: i32,
        dest_node_id: u64,
        dest_param_index: usize,
    ) -> bool {
        if scope_path.is_empty() {
            return self.can_connect_nodes(
                source_node_id,
                source_output_pin_index,
                dest_node_id,
                dest_param_index,
            );
        }
        let network = match self.get_scope_network(scope_path) {
            Some(n) => n,
            None => return false,
        };
        network.can_connect_nodes(
            source_node_id,
            source_output_pin_index,
            dest_node_id,
            dest_param_index,
            &self.node_type_registry,
        )
    }

    /// Scope-aware variant of [`connect_nodes`]. Phase U4 — intra-body wires
    /// only. With a non-empty `scope_path` the wire is added to the named HOF
    /// body without the top-level display-policy / undo orchestration; body-
    /// scope undo lands when body authoring is fully reachable.
    pub fn connect_nodes_scoped(
        &mut self,
        scope_path: &[u64],
        source_node_id: u64,
        source_output_pin_index: i32,
        dest_node_id: u64,
        dest_param_index: usize,
    ) {
        if scope_path.is_empty() {
            self.connect_nodes(
                source_node_id,
                source_output_pin_index,
                dest_node_id,
                dest_param_index,
            );
            return;
        }
        let dest_param_is_multi = {
            let network = match self.get_scope_network(scope_path) {
                Some(n) => n,
                None => return,
            };
            let dest_node = match network.nodes.get(&dest_node_id) {
                Some(n) => n,
                None => return,
            };
            match self.node_type_registry.get_node_type_for_node(dest_node) {
                Some(node_type) => {
                    if dest_param_index >= node_type.parameters.len() {
                        return;
                    }
                    node_type.parameters[dest_param_index].data_type.is_array()
                }
                None => return,
            }
        };
        if let Some(network) = self.get_scope_network_mut(scope_path) {
            network.connect_nodes(
                source_node_id,
                source_output_pin_index,
                dest_node_id,
                dest_param_index,
                dest_param_is_multi,
            );
        }
        self.set_dirty(true);
        // Mark the destination dirty at its actual scope so the partial-refresh
        // path picks it up (and lifts it to the enclosing HOF via the synthetic
        // body→HOF edge). Without this, an intra-body wire add would never
        // trigger re-evaluation of the enclosing HOF.
        self.pending_changes
            .mark_node_data_changed_scoped(scope_path, dest_node_id);
        // Re-validate so zone-rule errors raised by a previous pass clear once
        // the user adds the wire that satisfies them.
        self.validate_active_network();
    }

    /// Scope-aware variant of `connect_nodes` for cross-scope wires (captures
    /// and iteration-value references). Phase U5 of `doc/design_zones_ui.md`.
    ///
    /// `dest_scope_path` is the network where the wire is stored — the body
    /// containing the destination node's `arguments` (External argument kind).
    /// `source_scope_depth` measures how many ancestor frames up from
    /// `dest_scope_path` the source pin lives:
    ///   - `0` — same scope (regular wire)
    ///   - `≥ 1` — capture (`NodeOutput` source) or iteration-value reference
    ///     (`ZoneInput` source) from an ancestor scope
    ///
    /// Body-return wires (`destination_argument_kind = ZoneOutput`) go through
    /// the separate [`connect_zone_output_wire`] path — their storage scope
    /// differs from the destination's evaluation scope.
    pub fn connect_wire_scoped(
        &mut self,
        dest_scope_path: &[u64],
        source_node_id: u64,
        source_pin: crate::structure_designer::node_network::SourcePin,
        source_scope_depth: u8,
        dest_node_id: u64,
        dest_param_index: usize,
    ) {
        // Same-scope NodeOutput wires can route through the existing
        // `connect_nodes_scoped` for parity with U4-era callers (display-policy
        // / dirty-flag bookkeeping). Cross-scope / ZoneInput wires use the
        // generalized `connect_wire` on NodeNetwork.
        if source_scope_depth == 0 {
            if let crate::structure_designer::node_network::SourcePin::NodeOutput { pin_index } =
                source_pin
            {
                self.connect_nodes_scoped(
                    dest_scope_path,
                    source_node_id,
                    pin_index,
                    dest_node_id,
                    dest_param_index,
                );
                return;
            }
        }
        // Resolve dest pin's multi-ness against the dest node's type.
        let dest_param_is_multi = {
            let network = match self.get_scope_network(dest_scope_path) {
                Some(n) => n,
                None => return,
            };
            let dest_node = match network.nodes.get(&dest_node_id) {
                Some(n) => n,
                None => return,
            };
            match self.node_type_registry.get_node_type_for_node(dest_node) {
                Some(node_type) => {
                    if dest_param_index >= node_type.parameters.len() {
                        return;
                    }
                    node_type.parameters[dest_param_index].data_type.is_array()
                }
                None => return,
            }
        };
        if let Some(network) = self.get_scope_network_mut(dest_scope_path) {
            network.connect_wire(
                source_node_id,
                source_pin,
                source_scope_depth,
                dest_node_id,
                dest_param_index,
                dest_param_is_multi,
            );
        }
        self.set_dirty(true);
        // Mark the destination dirty so the partial-refresh path picks up the
        // new wire and re-evaluates the consuming body (and, via the synthetic
        // body→HOF edge, the enclosing HOF and its downstream).
        self.pending_changes
            .mark_node_data_changed_scoped(dest_scope_path, dest_node_id);
        // Re-validate so zone-rule errors raised by a previous pass clear once
        // the user adds the wire that satisfies them.
        self.validate_active_network();
    }

    /// Scope-aware predicate for cross-scope wires (Phase U5). Returns `true`
    /// if a wire with the given shape could legally be created (basic type
    /// compatibility + structural sanity). Pairs with [`connect_wire_scoped`].
    ///
    /// The rule from `doc/design_zones_ui.md` §"Computing source_scope_depth
    /// at wire creation": `source.scopeChain` must be a prefix of the
    /// destination's evaluation scope; the Flutter caller has already enforced
    /// this when computing `source_scope_depth`, so here we only walk the
    /// scope chain to find the source's network and validate the type.
    pub fn can_connect_wire_scoped(
        &self,
        dest_scope_path: &[u64],
        source_node_id: u64,
        source_pin: crate::structure_designer::node_network::SourcePin,
        source_scope_depth: u8,
        dest_node_id: u64,
        dest_param_index: usize,
    ) -> bool {
        // Source's scope is `dest_scope_path` with the last `source_scope_depth`
        // elements stripped. Reject if the requested depth exceeds the path.
        if (source_scope_depth as usize) > dest_scope_path.len() {
            return false;
        }
        let source_scope_path =
            &dest_scope_path[..dest_scope_path.len() - source_scope_depth as usize];

        let dest_network = match self.get_scope_network(dest_scope_path) {
            Some(n) => n,
            None => return false,
        };
        let dest_node = match dest_network.nodes.get(&dest_node_id) {
            Some(n) => n,
            None => return false,
        };
        if dest_param_index >= dest_node.arguments.len() {
            return false;
        }
        let dest_param_type = self
            .node_type_registry
            .get_node_param_data_type(dest_node, dest_param_index);

        // Resolve the source pin's data type in its containing network.
        let source_network = match self.get_scope_network(source_scope_path) {
            Some(n) => n,
            None => return false,
        };
        let source_node = match source_network.nodes.get(&source_node_id) {
            Some(n) => n,
            None => return false,
        };
        let source_type = match source_pin {
            crate::structure_designer::node_network::SourcePin::NodeOutput { pin_index } => {
                match self.node_type_registry.resolve_output_type(
                    source_node,
                    source_network,
                    pin_index,
                ) {
                    Some(t) => t,
                    None => return false,
                }
            }
            crate::structure_designer::node_network::SourcePin::ZoneInput { pin_index } => {
                // The source node must be a zone-owning (HOF) node, and the
                // pin_index must be a valid zone-input pin. Note: a ZoneInput
                // source can only be authored when the wire is being created
                // inside that HOF's body — the source's network is the HOF's
                // *containing* network, and the destination scope is that
                // HOF's body. The Flutter caller is responsible for ensuring
                // this; here we just look up the pin's declared type.
                let source_type = match self.node_type_registry.get_node_type_for_node(source_node)
                {
                    Some(t) => t,
                    None => return false,
                };
                if !source_type.has_zone() {
                    return false;
                }
                let pin = match source_type.zone_input_pins.get(pin_index) {
                    Some(p) => p,
                    None => return false,
                };
                // Zone-input pins reuse `OutputPinDefinition`; for `Fixed`
                // declarations we use the type directly. Polymorphic
                // (`SameAsInput`) zone-inputs aren't in use yet — fall back
                // to None and reject.
                match &pin.data_type {
                    crate::structure_designer::node_type::PinOutputType::Fixed(t) => t.clone(),
                    _ => return false,
                }
            }
        };

        crate::structure_designer::data_type::DataType::can_be_converted_to(
            &source_type,
            &dest_param_type,
            &self.node_type_registry,
        )
    }

    /// Connect a body-return wire: source is a body node, destination is an
    /// HOF's zone-output pin (stored in the HOF's `zone_output_arguments`).
    /// `body_scope_path` identifies the body the source lives in; the HOF
    /// owning the zone-output is at the *last* element of the path — the
    /// wire is added to that HOF's `zone_output_arguments[zone_output_index]`.
    /// Phase U4 — see `doc/design_zones_ui.md` §"Wire-creation API
    /// generalisation" (Body return row).
    pub fn connect_zone_output_wire(
        &mut self,
        body_scope_path: &[u64],
        source_node_id: u64,
        source_output_pin_index: i32,
        zone_output_index: usize,
    ) {
        if body_scope_path.is_empty() {
            // Body-return wires only exist inside an HOF body, never at the
            // top level.
            return;
        }
        let hof_id = *body_scope_path.last().unwrap();
        let parent_path = &body_scope_path[..body_scope_path.len() - 1];
        let dest_param_is_multi = {
            // The destination is a zone-output pin on the HOF, whose
            // declaration lives on the HOF's NodeType. The HOF lives in
            // `parent_path`'s network.
            let parent_network = match self.get_scope_network(parent_path) {
                Some(n) => n,
                None => return,
            };
            let hof_node = match parent_network.nodes.get(&hof_id) {
                Some(n) => n,
                None => return,
            };
            let hof_type = match self.node_type_registry.get_node_type_for_node(hof_node) {
                Some(t) => t,
                None => return,
            };
            if zone_output_index >= hof_type.zone_output_pins.len() {
                return;
            }
            hof_type.zone_output_pins[zone_output_index]
                .data_type
                .is_array()
        };
        // Now mutate: walk to parent network and reach the HOF's
        // `zone_output_arguments`.
        let parent_network = match self.get_scope_network_mut(parent_path) {
            Some(n) => n,
            None => return,
        };
        let hof_node = match parent_network.nodes.get_mut(&hof_id) {
            Some(n) => n,
            None => return,
        };
        // Ensure the `zone_output_arguments` vec has a slot for this index.
        while hof_node.zone_output_arguments.len() <= zone_output_index {
            hof_node
                .zone_output_arguments
                .push(crate::structure_designer::node_network::Argument::new());
        }
        let argument = &mut hof_node.zone_output_arguments[zone_output_index];
        if !dest_param_is_multi && !argument.is_empty() {
            argument.clear();
        }
        argument.set_source(source_node_id, source_output_pin_index);
        self.set_dirty(true);
        // Mark the HOF dirty (in its parent scope): a body-return wire change
        // alters what the HOF emits per iteration, so every downstream consumer
        // of the HOF needs to re-evaluate. The HOF lives in `parent_path`.
        self.pending_changes
            .mark_node_data_changed_scoped(parent_path, hof_id);
        // Re-validate: this is the wire that satisfies zone validation rule 1
        // ("every zone-output pin has at least one incoming wire"). Without
        // re-running validation here the rule-1 error raised by a previous
        // pass would persist on `validation_errors` even though the wire that
        // satisfies it now exists.
        self.validate_active_network();
    }

    /// Scope-aware variant of [`duplicate_node`]. Phase U4 — body-scope dup
    /// runs without top-level undo / display-policy orchestration.
    pub fn duplicate_node_scoped(&mut self, scope_path: &[u64], node_id: u64) -> u64 {
        if scope_path.is_empty() {
            return self.duplicate_node(node_id);
        }
        let new_id = match self.get_scope_network_mut(scope_path) {
            Some(network) => network.duplicate_node(node_id).unwrap_or(0),
            None => return 0,
        };
        if new_id != 0 {
            self.set_dirty(true);
        }
        new_id
    }

    /// Scope-aware variant of [`toggle_node_selection`]. Phase U4.
    pub fn toggle_node_selection_scoped(&mut self, scope_path: &[u64], node_id: u64) -> bool {
        if scope_path.is_empty() {
            return self.toggle_node_selection(node_id);
        }
        match self.get_scope_network_mut(scope_path) {
            Some(network) => network.toggle_node_selection(node_id),
            None => false,
        }
    }

    /// Scope-aware variant of [`add_node_to_selection`]. Phase U4.
    pub fn add_node_to_selection_scoped(&mut self, scope_path: &[u64], node_id: u64) -> bool {
        if scope_path.is_empty() {
            return self.add_node_to_selection(node_id);
        }
        match self.get_scope_network_mut(scope_path) {
            Some(network) => network.add_node_to_selection(node_id),
            None => false,
        }
    }

    /// Scope-aware variant of [`select_nodes`]. Phase U4.
    pub fn select_nodes_scoped(&mut self, scope_path: &[u64], node_ids: Vec<u64>) -> bool {
        if scope_path.is_empty() {
            return self.select_nodes(node_ids);
        }
        match self.get_scope_network_mut(scope_path) {
            Some(network) => network.select_nodes(node_ids),
            None => false,
        }
    }

    /// Auto-connects a source pin to the first compatible pin on a target node.
    ///
    /// - When `source_is_output` is true: connects source output to target's first compatible input
    /// - When `source_is_output` is false: connects target's output to source input
    ///
    /// Returns true if a connection was made, false otherwise.
    pub fn auto_connect_to_node(
        &mut self,
        source_node_id: u64,
        source_pin_index: i32,
        source_is_output: bool,
        target_node_id: u64,
    ) -> bool {
        // Early return if active_node_network_name is None
        let node_network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return false,
        };

        // Get source and target node types to find compatible pins
        let connection_info = {
            let network = match self
                .node_type_registry
                .node_networks
                .get(&node_network_name)
            {
                Some(network) => network,
                None => return false,
            };

            let source_node = match network.nodes.get(&source_node_id) {
                Some(node) => node,
                None => return false,
            };

            let target_node = match network.nodes.get(&target_node_id) {
                Some(node) => node,
                None => return false,
            };

            let source_node_type = match self.node_type_registry.get_node_type_for_node(source_node)
            {
                Some(nt) => nt,
                None => return false,
            };

            let target_node_type = match self.node_type_registry.get_node_type_for_node(target_node)
            {
                Some(nt) => nt,
                None => return false,
            };

            let _ = source_node_type;

            if source_is_output {
                // Source is output, find first compatible input on target.
                // Resolve the source pin's concrete type against the current
                // network state; unresolved polymorphic pins cannot connect.
                let source_output_type = match self.node_type_registry.resolve_output_type(
                    source_node,
                    network,
                    source_pin_index,
                ) {
                    Some(t) => t,
                    None => return false,
                };

                // Find first compatible input parameter on target node
                let mut compatible_param_index: Option<usize> = None;
                for (param_idx, param) in target_node_type.parameters.iter().enumerate() {
                    if DataType::can_be_converted_to(
                        &source_output_type,
                        &param.data_type,
                        &self.node_type_registry,
                    ) {
                        compatible_param_index = Some(param_idx);
                        break;
                    }
                }

                compatible_param_index
                    .map(|param_idx| (source_node_id, source_pin_index, target_node_id, param_idx))
            } else {
                let _ = target_node_type;
                // Source is input, connect target's output to source's input pin
                let target_output_type =
                    match self
                        .node_type_registry
                        .resolve_output_type(target_node, network, 0)
                    {
                        Some(t) => t,
                        None => return false,
                    };
                let source_param_type = self
                    .node_type_registry
                    .get_node_param_data_type(source_node, source_pin_index as usize);

                if DataType::can_be_converted_to(
                    &target_output_type,
                    &source_param_type,
                    &self.node_type_registry,
                ) {
                    // Connect target output (pin 0) to source input
                    Some((target_node_id, 0, source_node_id, source_pin_index as usize))
                } else {
                    None
                }
            }
        };

        // Make the connection if we found compatible pins
        if let Some((src_node, src_pin, dest_node, dest_param)) = connection_info {
            self.connect_nodes(src_node, src_pin, dest_node, dest_param);
            return true;
        }

        false
    }

    /// Returns all compatible pins on the target node for auto-connection.
    /// Each tuple contains (pin_index, pin_name, data_type_string).
    /// When source_is_output is true, returns compatible INPUT pins on target.
    /// When source_is_output is false, returns the OUTPUT pin if compatible.
    pub fn get_compatible_pins_for_auto_connect(
        &self,
        source_node_id: u64,
        source_pin_index: i32,
        source_is_output: bool,
        target_node_id: u64,
    ) -> Vec<(i32, String, String)> {
        let node_network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return Vec::new(),
        };

        let network = match self
            .node_type_registry
            .node_networks
            .get(&node_network_name)
        {
            Some(network) => network,
            None => return Vec::new(),
        };

        let source_node = match network.nodes.get(&source_node_id) {
            Some(node) => node,
            None => return Vec::new(),
        };

        let target_node = match network.nodes.get(&target_node_id) {
            Some(node) => node,
            None => return Vec::new(),
        };

        let source_node_type = match self.node_type_registry.get_node_type_for_node(source_node) {
            Some(nt) => nt,
            None => return Vec::new(),
        };

        let target_node_type = match self.node_type_registry.get_node_type_for_node(target_node) {
            Some(nt) => nt,
            None => return Vec::new(),
        };

        let mut compatible_pins = Vec::new();

        let _ = source_node_type;

        if source_is_output {
            // Source is output, find all compatible input parameters on target.
            let source_output_type = match self.node_type_registry.resolve_output_type(
                source_node,
                network,
                source_pin_index,
            ) {
                Some(t) => t,
                None => return Vec::new(),
            };

            for (param_idx, param) in target_node_type.parameters.iter().enumerate() {
                if DataType::can_be_converted_to(
                    &source_output_type,
                    &param.data_type,
                    &self.node_type_registry,
                ) {
                    compatible_pins.push((
                        param_idx as i32,
                        param.name.clone(),
                        param.data_type.to_string(),
                    ));
                }
            }
        } else {
            let _ = target_node_type;
            // Source is input, check if target's output is compatible
            let target_output_type =
                match self
                    .node_type_registry
                    .resolve_output_type(target_node, network, 0)
                {
                    Some(t) => t,
                    None => return Vec::new(),
                };
            let source_param_type = self
                .node_type_registry
                .get_node_param_data_type(source_node, source_pin_index as usize);

            if DataType::can_be_converted_to(
                &target_output_type,
                &source_param_type,
                &self.node_type_registry,
            ) {
                // Output pin is always index 0 with name "output"
                compatible_pins.push((0, "output".to_string(), target_output_type.to_string()));
            }
        }

        compatible_pins
    }

    /// Top-level convenience wrapper. Equivalent to
    /// [`set_node_network_data_scoped`] with an empty `scope_path`.
    pub fn set_node_network_data(&mut self, node_id: u64, data: Box<dyn NodeData>) {
        self.set_node_network_data_scoped(&[], node_id, data);
    }

    /// Set the per-node `NodeData` for a node identified by `(scope_path,
    /// node_id)`. An empty `scope_path` targets the active top-level network
    /// (today's behavior); a non-empty path walks the chain of HOF body
    /// `zone`s down to the target body — see `doc/design_zones_ui.md`
    /// §"Mutation APIs grow a `scope_path` parameter".
    ///
    /// Handles all the orchestration around a property edit in one place so
    /// every `set_*_data` API can just call this and inherit:
    /// * before/after JSON snapshots routed to the right body for undo,
    /// * `expr` parse-and-validate on the new data,
    /// * dirty-flag and per-node "data changed" pending-change tracking,
    /// * one coalesced [`SetNodeDataCommand`] with the scope_path baked in,
    /// * custom-node-type cache repopulation against the body's node,
    /// * cascading network validation when the node owns a custom type.
    ///
    /// Validation runs through `validate_active_network_with_initial_errors`,
    /// which validates the active top-level network *and recursively
    /// validates every zone body it contains* via `validate_zones_recursive`
    /// — so a body-node edit that breaks zone rules surfaces errors without
    /// needing a body-scoped validator entry point.
    pub fn set_node_network_data_scoped(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        mut data: Box<dyn NodeData>,
    ) {
        // Early return if active_node_network_name is None, clone to avoid borrow conflicts
        let network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return,
        };

        // Check node type before modification. `get_scope_network` handles
        // both empty (top-level) and non-empty (body) scope paths uniformly.
        let (is_expr_node, node_type_name) = match self
            .get_scope_network(scope_path)
            .and_then(|net| net.nodes.get(&node_id))
        {
            Some(node) => (node.node_type_name == "expr", node.node_type_name.clone()),
            None => return,
        };

        // Capture before-state for undo (skip for deprecated edit_atom and atom_edit nodes;
        // atom_edit has its own incremental undo commands)
        let old_data_json = if node_type_name != "edit_atom"
            && !crate::structure_designer::nodes::atom_edit::atom_edit::is_atom_edit_family(
                &node_type_name,
            ) {
            self.snapshot_node_data_scoped(&network_name, scope_path, node_id)
        } else {
            None
        };

        // For expr nodes, validate the expression before setting the data
        let mut expr_validation_errors = Vec::new();
        if is_expr_node {
            if let Some(expr_data) =
                data.as_any_mut()
                    .downcast_mut::<crate::structure_designer::nodes::expr::ExprData>()
            {
                expr_validation_errors = expr_data.parse_and_validate(node_id);
            }
        }

        // Apply mutation in the resolved scope. The block scopes the
        // `&mut self` borrow held by `get_scope_network_mut` so we can call
        // `self.set_dirty` / `self.mark_node_data_changed` immediately after.
        let mutated = {
            if let Some(network) = self.get_scope_network_mut(scope_path) {
                network.set_node_network_data(node_id, data);
                true
            } else {
                false
            }
        };
        if mutated {
            self.set_dirty(true);
            // Mark the edited node dirty at its actual scope. For top-level
            // edits this is equivalent to the old top-level `mark_node_data_changed`;
            // for body edits the scope is what lets `compute_downstream_dependents`
            // lift the dirtiness out of the body to the enclosing HOF.
            self.pending_changes
                .mark_node_data_changed_scoped(scope_path, node_id);
        }

        // Capture after-state and push undo command
        if let Some(old_json) = old_data_json {
            if let Some(new_json) =
                self.snapshot_node_data_scoped(&network_name, scope_path, node_id)
            {
                if old_json != new_json {
                    self.push_command(super::undo::commands::set_node_data::SetNodeDataCommand {
                        description: format!("Edit {}", node_type_name),
                        network_name: network_name.clone(),
                        scope_path: scope_path.to_vec(),
                        node_id,
                        node_type_name,
                        old_data_json: old_json,
                        new_data_json: new_json,
                    });
                }
            }
        }

        // Cache custom NodeType if needed after data is set. Mirrors the
        // split-borrow pattern used by `add_node_scoped` so the read-only
        // type maps and the mutable `node_networks` are accessed as sibling
        // fields of `node_type_registry`. Walks down `scope_path` manually
        // because the helper `get_scope_network_mut` would borrow all of
        // `self`, which would conflict with the read-only type maps.
        let (built_in_types, record_type_defs, built_in_record_type_defs, node_networks) = (
            &self.node_type_registry.built_in_node_types,
            &self.node_type_registry.record_type_defs,
            &self.node_type_registry.built_in_record_type_defs,
            &mut self.node_type_registry.node_networks,
        );
        let custom_node_type_populated = {
            let mut current: Option<&mut NodeNetwork> = node_networks.get_mut(&network_name);
            for hof_id in scope_path {
                current = match current {
                    Some(net) => net.nodes.get_mut(hof_id).and_then(|n| n.zone_mut()),
                    None => None,
                };
            }
            match current.and_then(|net| net.nodes.get_mut(&node_id)) {
                Some(node) => NodeTypeRegistry::populate_custom_node_type_cache_with_types(
                    built_in_types,
                    record_type_defs,
                    built_in_record_type_defs,
                    node,
                    true,
                ),
                None => false,
            }
        };

        // Validate if this node has a custom node type. `validate_active_network`
        // recursively walks zones via `validate_zones_recursive`, so body-node
        // edits get the same validation cascade as top-level edits.
        if custom_node_type_populated {
            let initial_errors = if expr_validation_errors.is_empty() {
                None
            } else {
                Some(expr_validation_errors)
            };
            self.validate_active_network_with_initial_errors(initial_errors);
        }
    }

    // Refresh special gadgets that are dependent on the scene, not only on node data.
    fn refresh_scene_dependent_node_data(&mut self) {
        self.refresh_scene_dependent_edit_atom_data();
    }

    fn refresh_scene_dependent_edit_atom_data(&mut self) {
        // First calculate the selection transform
        let selection_transform = self
            .get_atomic_structure_from_selected_node()
            .and_then(calc_selection_transform);

        // Then update the edit atom data with the pre-calculated transform
        if let Some(edit_atom_data) = get_selected_edit_atom_data_mut(self) {
            edit_atom_data.selection_transform = selection_transform;
        }
    }

    pub fn get_node_network_data(&self, node_id: u64) -> Option<&dyn NodeData> {
        // Search the whole scope tree under the active top-level network so
        // body-scope per-node-type data getters (called by the property panel
        // when a body node is selected) find their target. The lookup is
        // ambiguous in principle — top-level and body networks each have
        // their own id counter — but it always finds *some* node with the
        // matching id, and the property panel's active-id flow takes care
        // not to ask about colliding ids. Phase U4 of `doc/design_zones_ui.md`.
        let network_name = self.active_node_network_name.as_ref()?;
        let network = self.node_type_registry.node_networks.get(network_name)?;
        find_node_data_recursive(network, node_id)
    }

    pub fn get_node_network_data_mut(&mut self, node_id: u64) -> Option<&mut dyn NodeData> {
        let network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return None,
        };
        self.pending_changes.mark_node_data_changed(node_id);
        let network = self
            .node_type_registry
            .node_networks
            .get_mut(&network_name)?;
        find_node_data_mut_recursive(network, node_id)
    }

    pub fn get_network_evaluator(&self) -> &NetworkEvaluator {
        &self.network_evaluator
    }

    /// Returns a reference to the active node network, if any
    pub fn get_active_node_network(&self) -> Option<&NodeNetwork> {
        let network_name = self.active_node_network_name.as_ref()?;
        self.node_type_registry.node_networks.get(network_name)
    }

    /// Returns a mutable reference to the active node network, if any
    pub fn get_active_node_network_mut(&mut self) -> Option<&mut NodeNetwork> {
        let network_name = self.active_node_network_name.as_ref()?;
        self.node_type_registry.node_networks.get_mut(network_name)
    }

    /// Gets the description of the active node network
    pub fn get_active_network_description(&self) -> Option<String> {
        let network = self.get_active_node_network()?;
        Some(network.node_type.description.clone())
    }

    /// Sets the description of the active node network
    pub fn set_active_network_description(&mut self, description: String) -> Result<(), String> {
        let network_name = self
            .active_node_network_name
            .as_ref()
            .ok_or("No active node network")?;

        let network = self
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .ok_or("Active network not found")?;

        network.node_type.description = description;
        self.set_dirty(true);
        Ok(())
    }

    /// Gets the summary of the active node network
    pub fn get_active_network_summary(&self) -> Option<String> {
        let network = self.get_active_node_network()?;
        network.node_type.summary.clone()
    }

    /// Sets the summary of the active node network
    /// Pass None or empty string to clear the summary
    pub fn set_active_network_summary(&mut self, summary: Option<String>) -> Result<(), String> {
        let network_name = self
            .active_node_network_name
            .as_ref()
            .ok_or("No active node network")?;

        let network = self
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .ok_or("Active network not found")?;

        // Convert empty string to None
        network.node_type.summary = summary.filter(|s| !s.is_empty());
        self.set_dirty(true);
        Ok(())
    }

    /// Gets the name and description of a specific node type (built-in or custom network)
    /// Returns (name, description) tuple
    pub fn get_network_description(&self, network_name: &str) -> Option<(String, String)> {
        // First check built-in node types
        if let Some(node_type) = self
            .node_type_registry
            .built_in_node_types
            .get(network_name)
        {
            return Some((node_type.name.clone(), node_type.description.clone()));
        }

        // Then check custom node networks
        if let Some(network) = self.node_type_registry.node_networks.get(network_name) {
            return Some((
                network.node_type.name.clone(),
                network.node_type.description.clone(),
            ));
        }

        None
    }

    /// Sets the active node network and returns the camera settings to apply (if any).
    /// The caller is responsible for applying the returned settings to the renderer.
    pub fn set_active_node_network_name(
        &mut self,
        node_network_name: Option<String>,
    ) -> Option<CameraSettings> {
        self.navigation_history
            .navigate_to(node_network_name.clone());
        self.active_node_network_name = node_network_name;
        // Switching networks requires full refresh (everything changes)
        self.mark_full_refresh();
        // Return camera settings from the newly active network
        self.get_active_node_network()
            .and_then(|n| n.camera_settings.clone())
    }

    /// Returns true if the design has been modified since the last save/load
    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    /// Sets the dirty flag to indicate the design has been modified
    pub fn set_dirty(&mut self, dirty: bool) {
        self.is_dirty = dirty;
    }

    /// Returns the file path where the design was last saved/loaded, or None if never saved/loaded
    pub fn get_file_path(&self) -> Option<&String> {
        self.file_path.as_ref()
    }

    /// Clears all networks and creates a fresh project with a single "Main" network.
    ///
    /// This resets the state to match a newly opened application:
    /// - Clears all networks
    /// - Creates a new empty "Main" network
    /// - Clears the file path (no file associated)
    /// - Clears the dirty flag
    /// - Clears navigation history
    /// - Clears evaluation cache
    pub fn new_project(&mut self) {
        // Clear all networks
        self.node_type_registry.node_networks.clear();

        // Create a fresh "Main" network and set it as active
        self.add_node_network("Main");
        self.active_node_network_name = Some("Main".to_string());

        // Clear file state
        self.file_path = None;
        self.is_dirty = false;
        self.direct_editing_mode = false;

        // Clear navigation history
        self.navigation_history.clear();

        // Clear undo stack — new project has no history
        self.undo_stack.clear();
        self.pending_move = None;
        self.pending_gadget_drag = None;
        self.pending_comment_edit = None;

        // Clear evaluation cache
        self.network_evaluator.clear_csg_cache();

        // Clear the last generated scene
        self.last_generated_structure_designer_scene = StructureDesignerScene::new();

        // Mark for full refresh
        self.mark_full_refresh();
    }

    /// Creates a new project in direct editing mode.
    /// Sets up a single "Main" network with one atom_edit node that is displayed,
    /// selected, and set as the return node.
    pub fn new_project_direct_editing(&mut self) {
        // Start with a clean project
        self.new_project();
        self.direct_editing_mode = true;

        // Add an atom_edit node at origin
        let node_id = self.add_node("atom_edit", glam::DVec2::ZERO);
        if node_id == 0 {
            return;
        }

        // Set as return node (without undo tracking — fresh project)
        if let Some(network) = self.node_type_registry.node_networks.get_mut("Main") {
            network.return_node_id = Some(node_id);
        }

        // Select the node
        self.select_node(node_id);

        // Set active tool to Add Atom so the user can immediately start placing atoms
        if let Some(data) = self.get_node_network_data_mut(node_id) {
            if let Some(atom_edit_data) = data
                .as_any_mut()
                .downcast_mut::<crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData>()
            {
                atom_edit_data.active_tool =
                    crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditTool::AddAtom(
                        crate::structure_designer::nodes::atom_edit::atom_edit::AddAtomToolState::Idle,
                    );
            }
        }

        // Clear undo stack — new project should have no history
        self.undo_stack.clear();

        // Clear dirty flag — this is a fresh project
        self.is_dirty = false;
    }

    /// Checks whether the current state allows switching to direct editing mode.
    /// Criteria: the active network must have exactly one displayed atom_edit node,
    /// and that node must be the currently selected node.
    pub fn can_switch_to_direct_editing_mode(&self) -> bool {
        let network = match self
            .active_node_network_name
            .as_ref()
            .and_then(|name| self.node_type_registry.node_networks.get(name))
        {
            Some(n) => n,
            None => return false,
        };

        // Find displayed atom_edit nodes
        let displayed_atom_edit_ids: Vec<u64> = network
            .displayed_nodes
            .keys()
            .filter(|&&id| {
                network.nodes.get(&id).is_some_and(|node| {
                    crate::structure_designer::nodes::atom_edit::atom_edit::is_atom_edit_family(
                        &node.node_type_name,
                    )
                })
            })
            .copied()
            .collect();

        // Exactly one displayed atom_edit node
        if displayed_atom_edit_ids.len() != 1 {
            return false;
        }

        let atom_edit_id = displayed_atom_edit_ids[0];

        // That atom_edit node must be the currently selected node
        network.active_node_id == Some(atom_edit_id)
    }

    /// Sets direct editing mode and marks the design as dirty.
    /// When entering direct editing mode, sets the active tool to Add Atom.
    pub fn set_direct_editing_mode(&mut self, mode: bool) {
        if self.direct_editing_mode != mode {
            self.direct_editing_mode = mode;
            self.is_dirty = true;

            // When switching to direct editing mode, set the active tool to Add Atom
            // so the user is ready to start placing atoms immediately.
            if mode {
                let node_id = self
                    .get_active_node_network()
                    .and_then(|n| n.active_node_id);
                if let Some(node_id) = node_id {
                    if let Some(data) = self.get_node_network_data_mut(node_id) {
                        if let Some(atom_edit_data) = data
                            .as_any_mut()
                            .downcast_mut::<crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData>()
                        {
                            atom_edit_data.active_tool =
                                crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditTool::AddAtom(
                                    crate::structure_designer::nodes::atom_edit::atom_edit::AddAtomToolState::Idle,
                                );
                        }
                    }
                }
            }
        }
    }

    /// Imports an XYZ file into the active atom_edit node's diff layer.
    /// Atoms and bonds are merged directly as pure additions (no node wiring).
    /// This is used by direct editing mode for incremental imports.
    /// Must be called inside `with_atom_edit_undo` for undo support.
    pub fn import_xyz_into_atom_edit(&mut self, file_path: &str) -> Result<(), String> {
        use crate::crystolecule::io::xyz_loader::load_xyz;
        use crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;

        let atomic_structure =
            load_xyz(file_path, true).map_err(|e| format!("Failed to load XYZ file: {}", e))?;

        // Get the selected atom_edit/motif_edit node and merge the structure into its diff
        let selected_node_id =
            crate::structure_designer::nodes::atom_edit::atom_edit::get_selected_atom_edit_family_node_id(self)
                .ok_or("No atom_edit/motif_edit node selected")?;
        self.mark_node_data_changed(selected_node_id);

        let node_data = self
            .get_node_network_data_mut(selected_node_id)
            .ok_or("Failed to get node data")?;
        let atom_edit_data = node_data
            .as_any_mut()
            .downcast_mut::<AtomEditData>()
            .ok_or("Selected node is not an atom_edit node")?;

        atom_edit_data.merge_atomic_structure(&atomic_structure);

        self.mark_full_refresh();
        Ok(())
    }

    /// Navigates back in network history
    /// Returns (success, camera_settings) where success indicates if navigation occurred
    /// and camera_settings contains the camera settings to apply (if any)
    pub fn navigate_back(&mut self) -> (bool, Option<CameraSettings>) {
        if let Some(network_name) = self.navigation_history.navigate_back() {
            self.active_node_network_name = network_name;
            self.mark_full_refresh();
            let camera_settings = self
                .get_active_node_network()
                .and_then(|n| n.camera_settings.clone());
            (true, camera_settings)
        } else {
            (false, None)
        }
    }

    /// Navigates forward in network history
    /// Returns (success, camera_settings) where success indicates if navigation occurred
    /// and camera_settings contains the camera settings to apply (if any)
    pub fn navigate_forward(&mut self) -> (bool, Option<CameraSettings>) {
        if let Some(network_name) = self.navigation_history.navigate_forward() {
            self.active_node_network_name = network_name;
            self.mark_full_refresh();
            let camera_settings = self
                .get_active_node_network()
                .and_then(|n| n.camera_settings.clone());
            (true, camera_settings)
        } else {
            (false, None)
        }
    }

    /// Checks if we can navigate backward in network history
    pub fn can_navigate_back(&self) -> bool {
        self.navigation_history.can_navigate_back()
    }

    /// Checks if we can navigate forward in network history
    pub fn can_navigate_forward(&self) -> bool {
        self.navigation_history.can_navigate_forward()
    }
}

impl StructureDesigner {
    pub fn set_node_display(&mut self, node_id: u64, is_displayed: bool) {
        self.set_node_display_scoped(&[], node_id, is_displayed);
    }

    /// Scope-aware variant of [`set_node_display`]. Empty path: existing
    /// top-level behavior (undo command + visibility tracking). Non-empty
    /// path: flip the body node's display flag directly via the scope-network
    /// helper. Body-internal display undo is deferred to U4 when body
    /// authoring lands (`doc/design_zones_ui.md` §"Phase U4").
    pub fn set_node_display_scoped(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        is_displayed: bool,
    ) {
        if !scope_path.is_empty() {
            if let Some(network) = self.get_scope_network_mut(scope_path) {
                network.set_node_display(node_id, is_displayed);
            }
            return;
        }

        // Early return if active_node_network_name is None
        let network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return,
        };

        // Capture old display state before mutation
        let old_display_type = self
            .node_type_registry
            .node_networks
            .get(&network_name)
            .and_then(|net| net.get_node_display_type(node_id));

        if let Some(network) = self.node_type_registry.node_networks.get_mut(&network_name) {
            network.set_node_display(node_id, is_displayed);
            // Track that this node's visibility changed
            self.pending_changes.visibility_changed.insert(node_id);
        }

        // Capture new display state after mutation
        let new_display_type = self
            .node_type_registry
            .node_networks
            .get(&network_name)
            .and_then(|net| net.get_node_display_type(node_id));

        // Only push command if display state actually changed
        if old_display_type != new_display_type {
            let node_type_name = self
                .node_type_registry
                .node_networks
                .get(&network_name)
                .and_then(|net| net.nodes.get(&node_id))
                .map(|n| n.node_type_name.as_str())
                .unwrap_or("node");
            let description = format!("Toggle {} display", node_type_name);
            self.push_command(
                super::undo::commands::set_node_display::SetNodeDisplayCommand {
                    network_name,
                    node_id,
                    old_display_type,
                    new_display_type,
                    description,
                },
            );
        }
    }

    /// Toggle the display state of a specific output pin on a node.
    /// The node must already be displayed (in `displayed_nodes`) for this to take effect.
    pub fn toggle_output_pin_display(&mut self, node_id: u64, pin_index: i32) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return,
        };

        // Capture old display state before mutation
        let old_display_state = self
            .node_type_registry
            .node_networks
            .get(&network_name)
            .and_then(|net| net.displayed_nodes.get(&node_id))
            .cloned();

        if let Some(network) = self.node_type_registry.node_networks.get_mut(&network_name) {
            network.set_pin_displayed(
                node_id,
                pin_index,
                !network
                    .get_displayed_pins(node_id)
                    .is_some_and(|pins| pins.contains(&pin_index)),
            );
            self.pending_changes.visibility_changed.insert(node_id);

            // Update displayed_pins on cached NodeSceneData so it renders the
            // correct pins when restored from cache without re-evaluation.
            let new_pins = network
                .get_displayed_pins(node_id)
                .cloned()
                .unwrap_or_default();
            // Update in live node_data
            if let Some(scene_data) = self
                .last_generated_structure_designer_scene
                .node_data
                .get_mut(&node_id)
            {
                scene_data.displayed_pins = new_pins.clone();
            }
            // Update in invisible cache
            self.last_generated_structure_designer_scene
                .update_cached_displayed_pins(node_id, new_pins);
        }

        // Capture new display state after mutation
        let new_display_state = self
            .node_type_registry
            .node_networks
            .get(&network_name)
            .and_then(|net| net.displayed_nodes.get(&node_id))
            .cloned();

        // Only push command if display state actually changed
        if old_display_state != new_display_state {
            let node_type_name = self
                .node_type_registry
                .node_networks
                .get(&network_name)
                .and_then(|net| net.nodes.get(&node_id))
                .map(|n| n.node_type_name.as_str())
                .unwrap_or("node");
            let description = format!("Toggle {} pin {} display", node_type_name, pin_index);
            self.push_command(
                super::undo::commands::set_output_pin_display::SetOutputPinDisplayCommand {
                    network_name,
                    node_id,
                    old_display_state,
                    new_display_state,
                    description,
                },
            );
        }
    }

    pub fn sync_gadget_data(&mut self) -> bool {
        // Early return if active_node_network_name is None
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return false,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            if let Some(node_id) = &network.active_node_id {
                let data = network.get_node_network_data_mut(*node_id);
                if let Some(node_data) = data {
                    if let Some(g) = &self.gadget {
                        g.sync_data(node_data);
                        // Mark design as dirty since gadget data was synced back to node
                        self.set_dirty(true);
                    }
                }
            }
            true
        } else {
            false
        }
    }

    pub fn select_node(&mut self, node_id: u64) -> bool {
        self.select_node_scoped(&[], node_id)
    }

    /// Scope-aware variant of [`select_node`]. See `doc/design_zones_ui.md`
    /// §"Phase U2". The display-policy / dirty-node bookkeeping below is a
    /// top-level-only concern (per-body display policy is a U4-onwards
    /// problem); when called with a non-empty `scope_path` it sets the
    /// body's selection but skips the global policy refresh.
    pub fn select_node_scoped(&mut self, scope_path: &[u64], node_id: u64) -> bool {
        if scope_path.is_empty() {
            // Top-level: keep the existing behavior verbatim (display policy
            // + selection-change tracking apply to the top-level network).
            let network_name = match &self.active_node_network_name {
                Some(name) => name,
                None => return false,
            };
            if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
                let previously_active_node_id = network.active_node_id;
                let ret = network.select_node(node_id);
                if ret {
                    let current_selection = Some(node_id);
                    self.mark_selection_changed(previously_active_node_id, current_selection);
                    let mut dirty_nodes = HashSet::new();
                    dirty_nodes.insert(node_id);
                    if let Some(prev_id) = previously_active_node_id {
                        dirty_nodes.insert(prev_id);
                    }
                    self.apply_node_display_policy(Some(&dirty_nodes));
                }
                ret
            } else {
                false
            }
        } else {
            // Body scope: route to the named body via the scope helper.
            if let Some(network) = self.get_scope_network_mut(scope_path) {
                network.select_node(node_id)
            } else {
                false
            }
        }
    }

    pub fn select_wire(
        &mut self,
        source_node_id: u64,
        source_output_pin_index: i32,
        destination_node_id: u64,
        destination_argument_index: usize,
    ) -> bool {
        // Early return if active_node_network_name is None
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return false,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            // Get the previously active node ID before changing selection
            let previously_active_node_id = network.active_node_id;

            // Update the selection
            let ret = network.select_wire(
                source_node_id,
                source_output_pin_index,
                destination_node_id,
                destination_argument_index,
            );

            // If the selection was successful
            if ret {
                // Track selection change (wire selection clears node selection)
                self.mark_selection_changed(previously_active_node_id, None);

                // If there was a previously active node, update display policy
                if let Some(prev_id) = previously_active_node_id {
                    // Create a HashSet with just the previously active node ID
                    let mut dirty_nodes = HashSet::new();
                    dirty_nodes.insert(prev_id);

                    // Apply display policy considering only the previously active node as dirty
                    self.apply_node_display_policy(Some(&dirty_nodes));
                }
            }

            ret
        } else {
            false
        }
    }

    pub fn clear_selection(&mut self) {
        self.clear_selection_scoped(&[]);
    }

    /// Clear selection (and active_node_id) at every scope reachable from the
    /// active top-level network. Used when the user clicks on empty top-level
    /// space — the design's per-body selection means an active body node
    /// keeps its `.active` flag even after the user deselects at the top
    /// level, which surfaces as a stale "this node is active" highlight in
    /// the property panel. This helper resets it everywhere.
    pub fn clear_selection_all_scopes(&mut self) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return,
        };
        let previously_active_node_id = self
            .node_type_registry
            .node_networks
            .get(&network_name)
            .and_then(|n| n.active_node_id);
        if let Some(network) = self.node_type_registry.node_networks.get_mut(&network_name) {
            clear_selection_recursive(network);
        }
        self.mark_selection_changed(previously_active_node_id, None);
        if let Some(prev_id) = previously_active_node_id {
            let mut dirty_nodes = HashSet::new();
            dirty_nodes.insert(prev_id);
            self.apply_node_display_policy(Some(&dirty_nodes));
        }
    }

    /// Scope-aware variant of [`clear_selection`]. With an empty `scope_path`
    /// behavior is identical to today's `clear_selection`; with a non-empty
    /// path the body's `selected_node_ids` (and any wire/active state) is
    /// cleared without touching the top-level display policy.
    pub fn clear_selection_scoped(&mut self, scope_path: &[u64]) {
        if scope_path.is_empty() {
            let network_name = match &self.active_node_network_name {
                Some(name) => name,
                None => return,
            };
            if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
                let previously_active_node_id = network.active_node_id;
                network.clear_selection();
                self.mark_selection_changed(previously_active_node_id, None);
                if let Some(prev_id) = previously_active_node_id {
                    let mut dirty_nodes = HashSet::new();
                    dirty_nodes.insert(prev_id);
                    self.apply_node_display_policy(Some(&dirty_nodes));
                }
            }
        } else if let Some(network) = self.get_scope_network_mut(scope_path) {
            network.clear_selection();
        }
    }

    /// Toggle node in selection (for Ctrl+click)
    pub fn toggle_node_selection(&mut self, node_id: u64) -> bool {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return false,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            let previously_active_node_id = network.active_node_id;
            let ret = network.toggle_node_selection(node_id);
            if ret {
                let current_selection = network.active_node_id;
                self.mark_selection_changed(previously_active_node_id, current_selection);
                let mut dirty_nodes = HashSet::new();
                dirty_nodes.insert(node_id);
                if let Some(prev_id) = previously_active_node_id {
                    dirty_nodes.insert(prev_id);
                }
                self.apply_node_display_policy(Some(&dirty_nodes));
            }
            ret
        } else {
            false
        }
    }

    /// Add node to selection (for Shift+click)
    pub fn add_node_to_selection(&mut self, node_id: u64) -> bool {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return false,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            let previously_active_node_id = network.active_node_id;
            let ret = network.add_node_to_selection(node_id);
            if ret {
                self.mark_selection_changed(previously_active_node_id, Some(node_id));
                let mut dirty_nodes = HashSet::new();
                dirty_nodes.insert(node_id);
                if let Some(prev_id) = previously_active_node_id {
                    dirty_nodes.insert(prev_id);
                }
                self.apply_node_display_policy(Some(&dirty_nodes));
            }
            ret
        } else {
            false
        }
    }

    /// Select multiple nodes (for rectangle selection)
    pub fn select_nodes(&mut self, node_ids: Vec<u64>) -> bool {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return false,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            let previously_active_node_id = network.active_node_id;
            let ret = network.select_nodes(node_ids.clone());
            if ret {
                let current_selection = network.active_node_id;
                self.mark_selection_changed(previously_active_node_id, current_selection);
                let mut dirty_nodes: HashSet<u64> = node_ids.into_iter().collect();
                if let Some(prev_id) = previously_active_node_id {
                    dirty_nodes.insert(prev_id);
                }
                self.apply_node_display_policy(Some(&dirty_nodes));
            }
            ret
        } else {
            false
        }
    }

    /// Toggle multiple nodes in selection (for Ctrl+rectangle)
    pub fn toggle_nodes_selection(&mut self, node_ids: Vec<u64>) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            let previously_active_node_id = network.active_node_id;
            network.toggle_nodes_selection(node_ids.clone());
            let current_selection = network.active_node_id;
            self.mark_selection_changed(previously_active_node_id, current_selection);
            let mut dirty_nodes: HashSet<u64> = node_ids.into_iter().collect();
            if let Some(prev_id) = previously_active_node_id {
                dirty_nodes.insert(prev_id);
            }
            self.apply_node_display_policy(Some(&dirty_nodes));
        }
    }

    /// Add multiple nodes to selection (for Shift+rectangle)
    pub fn add_nodes_to_selection(&mut self, node_ids: Vec<u64>) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            let previously_active_node_id = network.active_node_id;
            network.add_nodes_to_selection(node_ids.clone());
            let current_selection = network.active_node_id;
            self.mark_selection_changed(previously_active_node_id, current_selection);
            let mut dirty_nodes: HashSet<u64> = node_ids.into_iter().collect();
            if let Some(prev_id) = previously_active_node_id {
                dirty_nodes.insert(prev_id);
            }
            self.apply_node_display_policy(Some(&dirty_nodes));
        }
    }

    /// Get all selected node IDs
    pub fn get_selected_node_ids(&self) -> Vec<u64> {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return Vec::new(),
        };
        if let Some(network) = self.node_type_registry.node_networks.get(network_name) {
            network.get_selected_node_ids().iter().copied().collect()
        } else {
            Vec::new()
        }
    }

    /// Move all selected nodes by delta
    pub fn move_selected_nodes(&mut self, delta: glam::f64::DVec2) {
        self.move_selected_nodes_scoped(&[], delta);
    }

    /// Scope-aware variant of [`move_selected_nodes`]. Empty path: existing
    /// top-level behavior. Non-empty path: moves the body's selected nodes
    /// inside the named body. Phase U2 plumbing — see
    /// `doc/design_zones_ui.md`.
    pub fn move_selected_nodes_scoped(&mut self, scope_path: &[u64], delta: glam::f64::DVec2) {
        if let Some(network) = self.get_scope_network_mut(scope_path) {
            network.move_selected_nodes(delta);
        }
    }

    /// Called when a node drag begins. Captures start positions for undo coalescing.
    pub fn begin_move_nodes(&mut self) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return,
        };
        if let Some(network) = self.node_type_registry.node_networks.get(network_name) {
            let start_positions: Vec<(u64, glam::f64::DVec2)> = network
                .get_selected_node_ids()
                .iter()
                .filter_map(|&node_id| {
                    network
                        .nodes
                        .get(&node_id)
                        .map(|node| (node_id, node.position))
                })
                .collect();
            self.pending_move = Some(super::undo::snapshot::PendingMove { start_positions });
        }
    }

    /// Called when a node drag ends. Creates a single MoveNodesCommand from start to final positions.
    pub fn end_move_nodes(&mut self) {
        let pending = match self.pending_move.take() {
            Some(p) => p,
            None => return,
        };

        let network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return,
        };

        let network = match self.node_type_registry.node_networks.get(&network_name) {
            Some(n) => n,
            None => return,
        };

        // Build moves: (node_id, old_pos, new_pos), filtering out nodes that didn't move
        let moves: Vec<(u64, glam::f64::DVec2, glam::f64::DVec2)> = pending
            .start_positions
            .iter()
            .filter_map(|&(node_id, old_pos)| {
                network.nodes.get(&node_id).and_then(|node| {
                    if node.position != old_pos {
                        Some((node_id, old_pos, node.position))
                    } else {
                        None
                    }
                })
            })
            .collect();

        if !moves.is_empty() {
            let description = if moves.len() == 1 {
                let node_type_name = network
                    .nodes
                    .get(&moves[0].0)
                    .map(|n| n.node_type_name.as_str())
                    .unwrap_or("node");
                format!("Move {}", node_type_name)
            } else {
                format!("Move {} nodes", moves.len())
            };
            self.push_command(super::undo::commands::move_nodes::MoveNodesCommand {
                network_name,
                moves,
                description,
            });
        }
    }

    /// Toggle wire in selection (for Ctrl+click)
    pub fn toggle_wire_selection(
        &mut self,
        source_node_id: u64,
        source_output_pin_index: i32,
        destination_node_id: u64,
        destination_argument_index: usize,
    ) -> bool {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return false,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            let previously_active_node_id = network.active_node_id;
            let ret = network.toggle_wire_selection(
                source_node_id,
                source_output_pin_index,
                destination_node_id,
                destination_argument_index,
            );
            if ret {
                self.mark_selection_changed(previously_active_node_id, None);
                if let Some(prev_id) = previously_active_node_id {
                    let mut dirty_nodes = HashSet::new();
                    dirty_nodes.insert(prev_id);
                    self.apply_node_display_policy(Some(&dirty_nodes));
                }
            }
            ret
        } else {
            false
        }
    }

    /// Add wire to selection (for Shift+click)
    pub fn add_wire_to_selection(
        &mut self,
        source_node_id: u64,
        source_output_pin_index: i32,
        destination_node_id: u64,
        destination_argument_index: usize,
    ) -> bool {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return false,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            let previously_active_node_id = network.active_node_id;
            let ret = network.add_wire_to_selection(
                source_node_id,
                source_output_pin_index,
                destination_node_id,
                destination_argument_index,
            );
            if ret {
                self.mark_selection_changed(previously_active_node_id, None);
                if let Some(prev_id) = previously_active_node_id {
                    let mut dirty_nodes = HashSet::new();
                    dirty_nodes.insert(prev_id);
                    self.apply_node_display_policy(Some(&dirty_nodes));
                }
            }
            ret
        } else {
            false
        }
    }

    /// Get all selected wires
    pub fn get_selected_wires(&self) -> Vec<crate::structure_designer::node_network::Wire> {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return Vec::new(),
        };
        if let Some(network) = self.node_type_registry.node_networks.get(network_name) {
            network.get_selected_wires().clone()
        } else {
            Vec::new()
        }
    }

    /// Select multiple wires (replaces current selection)
    pub fn select_wires(&mut self, wires: Vec<crate::structure_designer::node_network::Wire>) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            let previously_active_node_id = network.active_node_id;
            network.select_wires(wires);
            self.mark_selection_changed(previously_active_node_id, None);
            if let Some(prev_id) = previously_active_node_id {
                let mut dirty_nodes = HashSet::new();
                dirty_nodes.insert(prev_id);
                self.apply_node_display_policy(Some(&dirty_nodes));
            }
        }
    }

    /// Add multiple wires to selection (for Shift+rectangle)
    pub fn add_wires_to_selection(
        &mut self,
        wires: Vec<crate::structure_designer::node_network::Wire>,
    ) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            let previously_active_node_id = network.active_node_id;
            network.add_wires_to_selection(wires);
            self.mark_selection_changed(previously_active_node_id, None);
            if let Some(prev_id) = previously_active_node_id {
                let mut dirty_nodes = HashSet::new();
                dirty_nodes.insert(prev_id);
                self.apply_node_display_policy(Some(&dirty_nodes));
            }
        }
    }

    /// Toggle multiple wires in selection (for Ctrl+rectangle)
    pub fn toggle_wires_selection(
        &mut self,
        wires: Vec<crate::structure_designer::node_network::Wire>,
    ) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            let previously_active_node_id = network.active_node_id;
            network.toggle_wires_selection(wires);
            self.mark_selection_changed(previously_active_node_id, None);
            if let Some(prev_id) = previously_active_node_id {
                let mut dirty_nodes = HashSet::new();
                dirty_nodes.insert(prev_id);
                self.apply_node_display_policy(Some(&dirty_nodes));
            }
        }
    }

    /// Select nodes and wires together (for rectangle selection)
    pub fn select_nodes_and_wires(
        &mut self,
        node_ids: Vec<u64>,
        wires: Vec<crate::structure_designer::node_network::Wire>,
    ) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            let previously_active_node_id = network.active_node_id;
            network.select_nodes_and_wires(node_ids.clone(), wires);
            let current_selection = network.active_node_id;
            self.mark_selection_changed(previously_active_node_id, current_selection);
            let mut dirty_nodes: HashSet<u64> = node_ids.into_iter().collect();
            if let Some(prev_id) = previously_active_node_id {
                dirty_nodes.insert(prev_id);
            }
            self.apply_node_display_policy(Some(&dirty_nodes));
        }
    }

    /// Add nodes and wires to existing selection (for Shift+rectangle)
    pub fn add_nodes_and_wires_to_selection(
        &mut self,
        node_ids: Vec<u64>,
        wires: Vec<crate::structure_designer::node_network::Wire>,
    ) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            let previously_active_node_id = network.active_node_id;
            network.add_nodes_and_wires_to_selection(node_ids.clone(), wires);
            let current_selection = network.active_node_id;
            self.mark_selection_changed(previously_active_node_id, current_selection);
            let mut dirty_nodes: HashSet<u64> = node_ids.into_iter().collect();
            if let Some(prev_id) = previously_active_node_id {
                dirty_nodes.insert(prev_id);
            }
            self.apply_node_display_policy(Some(&dirty_nodes));
        }
    }

    /// Toggle nodes and wires in selection (for Ctrl+rectangle)
    pub fn toggle_nodes_and_wires_selection(
        &mut self,
        node_ids: Vec<u64>,
        wires: Vec<crate::structure_designer::node_network::Wire>,
    ) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => return,
        };
        if let Some(network) = self.node_type_registry.node_networks.get_mut(network_name) {
            let previously_active_node_id = network.active_node_id;
            network.toggle_nodes_and_wires_selection(node_ids.clone(), wires);
            let current_selection = network.active_node_id;
            self.mark_selection_changed(previously_active_node_id, current_selection);
            let mut dirty_nodes: HashSet<u64> = node_ids.into_iter().collect();
            if let Some(prev_id) = previously_active_node_id {
                dirty_nodes.insert(prev_id);
            }
            self.apply_node_display_policy(Some(&dirty_nodes));
        }
    }

    pub fn delete_selected(&mut self) {
        self.delete_selected_scoped(&[]);
    }

    /// Scope-aware variant of [`delete_selected`]. With a non-empty `scope_path`
    /// the body's `delete_selected` runs without the top-level display-policy /
    /// undo machinery — body-scope undo lands in U4 when body authoring is
    /// reachable (`doc/design_zones_ui.md` §"Phase U4 → Gotchas").
    pub fn delete_selected_scoped(&mut self, scope_path: &[u64]) {
        if !scope_path.is_empty() {
            if let Some(network) = self.get_scope_network_mut(scope_path) {
                network.delete_selected();
            }
            return;
        }

        // Early return if active_node_network_name is None
        let node_network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return,
        };

        // Collect nodes that will need to be marked as dirty after deletion
        let mut dirty_nodes = HashSet::new();
        let mut should_validate = false;

        if let Some(node_network) = self
            .node_type_registry
            .node_networks
            .get(&node_network_name)
        {
            // If nodes are selected, all connected nodes will be dirty
            if !node_network.selected_node_ids.is_empty() {
                for &selected_node_id in &node_network.selected_node_ids {
                    // Get all nodes connected to the selected node
                    dirty_nodes.extend(node_network.get_connected_node_ids(selected_node_id));

                    // Check if the selected node requires validation
                    if let Some(node) = node_network.nodes.get(&selected_node_id) {
                        if node.node_type_name == "parameter" || {
                            // Check if this node references an invalid node network
                            self.node_type_registry
                                .node_networks
                                .get(&node.node_type_name)
                                .map(|network| !network.valid)
                                .unwrap_or(false)
                        } {
                            should_validate = true;
                        }
                    }
                }
            }
            // If wires are selected, both source and destination nodes will be dirty
            else if !node_network.selected_wires.is_empty() {
                for wire in &node_network.selected_wires {
                    dirty_nodes.insert(wire.source_node_id);
                    dirty_nodes.insert(wire.destination_node_id);
                }
            }
        }

        // Capture deletion info and node snapshots before deletion (for undo)
        let deletion_info = self
            .node_type_registry
            .node_networks
            .get(&node_network_name)
            .map(|n| n.collect_deletion_info());

        // Snapshot deleted nodes before deletion
        let mut deleted_node_snapshots = Vec::new();
        if let Some(ref info) = deletion_info {
            if info.is_node_deletion {
                for &node_id in &info.deleted_node_ids {
                    if let Some(snap) = self.snapshot_node(&node_network_name, node_id) {
                        deleted_node_snapshots.push(snap);
                    }
                }
            }
        }

        // Perform the deletion
        if let Some(node_network) = self
            .node_type_registry
            .node_networks
            .get_mut(&node_network_name)
        {
            node_network.delete_selected();
            // Mark design as dirty since we deleted something
            self.set_dirty(true);
            // TODO: we do a full refresh for now,
            // but this can be a partial refresh with marking data changes
            // in all nodes wired to the output node of the deleted node.
            self.mark_full_refresh();
        }

        // Check if we're deleting the return node (needed for validation below)
        let deleted_return_node = deletion_info
            .as_ref()
            .is_some_and(|info| info.was_return_node.is_some());

        // Push undo command
        if let Some(info) = deletion_info {
            use super::undo::snapshot::WireSnapshot;

            if info.is_node_deletion && !deleted_node_snapshots.is_empty() {
                let deleted_wires: Vec<WireSnapshot> = info
                    .deleted_wires
                    .iter()
                    .map(|w| WireSnapshot {
                        source_node_id: w.source_node_id,
                        source_output_pin_index: w.expect_node_output_pin(),
                        dest_node_id: w.destination_node_id,
                        dest_param_index: w.destination_argument_index,
                    })
                    .collect();

                let description = if deleted_node_snapshots.len() == 1 {
                    format!("Delete {}", deleted_node_snapshots[0].node_type_name)
                } else {
                    format!("Delete {} nodes", deleted_node_snapshots.len())
                };
                self.push_command(super::undo::commands::delete_nodes::DeleteNodesCommand {
                    network_name: node_network_name.clone(),
                    deleted_nodes: deleted_node_snapshots,
                    deleted_wires,
                    was_return_node: info.was_return_node,
                    display_states: info.display_states,
                    description,
                });
            } else if !info.is_node_deletion && !info.selected_wires.is_empty() {
                let deleted_wires: Vec<WireSnapshot> = info
                    .selected_wires
                    .iter()
                    .map(|w| WireSnapshot {
                        source_node_id: w.source_node_id,
                        source_output_pin_index: w.expect_node_output_pin(),
                        dest_node_id: w.destination_node_id,
                        dest_param_index: w.destination_argument_index,
                    })
                    .collect();

                self.push_command(super::undo::commands::delete_wires::DeleteWiresCommand {
                    network_name: node_network_name.clone(),
                    deleted_wires,
                });
            }
        }

        // Only apply display policy if there were dirty nodes
        if !dirty_nodes.is_empty() {
            self.apply_node_display_policy(Some(&dirty_nodes));
        }

        // Validate if we deleted a parameter node, invalid network node, or the return node
        if should_validate || deleted_return_node {
            self.validate_active_network();
        }
    }

    // -------------------------------------------------------------------------------------------------------------------------
    // --- Raytracing methods                                                                                              ---
    // -------------------------------------------------------------------------------------------------------------------------

    /// Traces a ray into the current scene, checking both atomic structures and implicit geometry
    ///
    /// # Arguments
    ///
    /// * `ray_origin` - The origin point of the ray
    /// * `ray_direction` - The direction vector of the ray (does not need to be normalized)
    /// * `visualization` - The visualization mode to use for hit testing
    ///
    /// # Returns
    ///
    /// The distance to the closest intersection, or None if no intersection was found
    pub fn raytrace(
        &self,
        ray_origin: &DVec3,
        ray_direction: &DVec3,
        visualization: &AtomicStructureVisualization,
    ) -> Option<f64> {
        let mut min_distance: Option<f64> = None;
        let display_visualization = match visualization {
            AtomicStructureVisualization::BallAndStick => {
                crate::display::preferences::AtomicStructureVisualization::BallAndStick
            }
            AtomicStructureVisualization::SpaceFilling => {
                crate::display::preferences::AtomicStructureVisualization::SpaceFilling
            }
        };

        use crate::structure_designer::structure_designer_scene::NodeOutput;
        // First, check all atomic structures in the scene
        for node_data in self
            .last_generated_structure_designer_scene
            .node_data
            .values()
        {
            for (_pin_index, pin_output, _pin_geo_tree) in node_data.displayed_outputs() {
                if let NodeOutput::Atomic(atomic_structure, _) = pin_output {
                    match atomic_structure.hit_test(
                        ray_origin,
                        ray_direction,
                        visualization,
                        |atom| get_displayed_atom_radius(atom, &display_visualization),
                        BAS_STICK_RADIUS,
                    ) {
                        crate::crystolecule::atomic_structure::HitTestResult::Atom(_, distance)
                        | crate::crystolecule::atomic_structure::HitTestResult::Bond(_, distance) =>
                        {
                            // Update minimum distance if this hit is closer
                            min_distance = match min_distance {
                                None => Some(distance),
                                Some(current_min) if distance < current_min => Some(distance),
                                _ => min_distance,
                            };
                        }
                        crate::crystolecule::atomic_structure::HitTestResult::None => {}
                    }
                }
            }
        }

        // Collect all geo_trees from displayed outputs
        let geometries: Vec<&dyn ImplicitGeometry3D> = self
            .last_generated_structure_designer_scene
            .node_data
            .values()
            .flat_map(|node_data| node_data.displayed_outputs())
            .filter_map(|(_pin_index, _output, geo_tree)| geo_tree)
            .map(|geo_node| geo_node as &dyn ImplicitGeometry3D)
            .collect();

        // Raytrace the implicit geometries using the world scale
        if let Some(geo_distance) = raytrace_geometries(&geometries, ray_origin, ray_direction, 1.0)
        {
            // Update minimum distance if this hit is closer
            min_distance = match min_distance {
                None => Some(geo_distance),
                Some(current_min) if geo_distance < current_min => Some(geo_distance),
                _ => min_distance,
            };
        }

        //println!("raytrace min_distance: {:?}", min_distance);

        min_distance
    }

    /// Hit-test across ALL visible AtomicStructures in the scene.
    /// Returns (atom_id, &AtomicStructure) for the closest atom hit,
    /// using the user's current visualization preference for radius calculation.
    pub fn hit_test_all_atomic_structures(
        &self,
        ray_origin: &DVec3,
        ray_direction: &DVec3,
    ) -> Option<(u32, &AtomicStructure)> {
        use crate::crystolecule::atomic_structure::HitTestResult;
        use crate::display::preferences as display_prefs;
        use crate::structure_designer::structure_designer_scene::NodeOutput;

        let visualization = &self
            .preferences
            .atomic_structure_visualization_preferences
            .visualization;
        let display_visualization = match visualization {
            AtomicStructureVisualization::BallAndStick => {
                display_prefs::AtomicStructureVisualization::BallAndStick
            }
            AtomicStructureVisualization::SpaceFilling => {
                display_prefs::AtomicStructureVisualization::SpaceFilling
            }
        };

        let mut closest: Option<(u32, &AtomicStructure, f64)> = None;

        for node_data in self
            .last_generated_structure_designer_scene
            .node_data
            .values()
        {
            for (_pin_index, pin_output, _pin_geo_tree) in node_data.displayed_outputs() {
                if let NodeOutput::Atomic(atomic_structure, _) = pin_output {
                    if let HitTestResult::Atom(atom_id, distance) = atomic_structure.hit_test(
                        ray_origin,
                        ray_direction,
                        visualization,
                        |atom| get_displayed_atom_radius(atom, &display_visualization),
                        BAS_STICK_RADIUS,
                    ) {
                        if closest.as_ref().is_none_or(|c| distance < c.2) {
                            closest = Some((atom_id, atomic_structure, distance));
                        }
                    }
                }
            }
        }

        closest.map(|(id, structure, _)| (id, structure))
    }

    /// Like `hit_test_all_atomic_structures`, but also returns the node ID
    /// and distance of the closest hit. Used by hover tooltip to show which
    /// node produced the hovered atom.
    pub fn hit_test_all_atomic_structures_with_node_id(
        &self,
        ray_origin: &DVec3,
        ray_direction: &DVec3,
    ) -> Option<(u32, &AtomicStructure, u64, f64)> {
        use crate::crystolecule::atomic_structure::HitTestResult;
        use crate::display::preferences as display_prefs;
        use crate::structure_designer::structure_designer_scene::NodeOutput;

        let visualization = &self
            .preferences
            .atomic_structure_visualization_preferences
            .visualization;
        let display_visualization = match visualization {
            AtomicStructureVisualization::BallAndStick => {
                display_prefs::AtomicStructureVisualization::BallAndStick
            }
            AtomicStructureVisualization::SpaceFilling => {
                display_prefs::AtomicStructureVisualization::SpaceFilling
            }
        };

        let mut closest: Option<(u32, &AtomicStructure, u64, f64)> = None;

        for (&node_id, node_data) in self
            .last_generated_structure_designer_scene
            .node_data
            .iter()
        {
            for (_pin_index, pin_output, _pin_geo_tree) in node_data.displayed_outputs() {
                if let NodeOutput::Atomic(atomic_structure, _) = pin_output {
                    if let HitTestResult::Atom(atom_id, distance) = atomic_structure.hit_test(
                        ray_origin,
                        ray_direction,
                        visualization,
                        |atom| get_displayed_atom_radius(atom, &display_visualization),
                        BAS_STICK_RADIUS,
                    ) {
                        if closest.as_ref().is_none_or(|c| distance < c.3) {
                            closest = Some((atom_id, atomic_structure, node_id, distance));
                        }
                    }
                }
            }
        }

        closest
    }

    /// Per-node raycast: returns a list of ray hits with associated node IDs.
    ///
    /// Unlike `raytrace()` which returns only the closest overall distance,
    /// this method preserves which node each hit belongs to. It iterates
    /// `node_data.iter()` (not `.values()`) to capture node ID keys.
    ///
    /// Used by click-to-activate: when the user clicks in the viewport,
    /// this determines which node(s) the click intersects so the correct
    /// node can be activated.
    pub fn raytrace_per_node(
        &self,
        ray_origin: &DVec3,
        ray_direction: &DVec3,
        visualization: &AtomicStructureVisualization,
    ) -> Vec<PerNodeRayHit> {
        let display_visualization = match visualization {
            AtomicStructureVisualization::BallAndStick => {
                crate::display::preferences::AtomicStructureVisualization::BallAndStick
            }
            AtomicStructureVisualization::SpaceFilling => {
                crate::display::preferences::AtomicStructureVisualization::SpaceFilling
            }
        };

        use crate::structure_designer::structure_designer_scene::NodeOutput;

        let mut hits = Vec::new();

        for (&node_id, node_data) in self
            .last_generated_structure_designer_scene
            .node_data
            .iter()
        {
            let mut min_distance: Option<f64> = None;

            // Hit-test all displayed outputs for this node
            for (_pin_index, pin_output, pin_geo_tree) in node_data.displayed_outputs() {
                if let NodeOutput::Atomic(atomic_structure, _) = pin_output {
                    match atomic_structure.hit_test(
                        ray_origin,
                        ray_direction,
                        visualization,
                        |atom| get_displayed_atom_radius(atom, &display_visualization),
                        BAS_STICK_RADIUS,
                    ) {
                        crate::crystolecule::atomic_structure::HitTestResult::Atom(_, distance)
                        | crate::crystolecule::atomic_structure::HitTestResult::Bond(_, distance) =>
                        {
                            min_distance = match min_distance {
                                None => Some(distance),
                                Some(current_min) if distance < current_min => Some(distance),
                                _ => min_distance,
                            };
                        }
                        crate::crystolecule::atomic_structure::HitTestResult::None => {}
                    }
                }

                // Hit-test geometry (SDF)
                if let Some(geo_tree) = pin_geo_tree {
                    if let Some(geo_distance) =
                        raytrace_geometry(geo_tree, ray_origin, ray_direction, 1.0)
                    {
                        min_distance = match min_distance {
                            None => Some(geo_distance),
                            Some(current_min) if geo_distance < current_min => Some(geo_distance),
                            _ => min_distance,
                        };
                    }
                }
            }

            if let Some(distance) = min_distance {
                hits.push(PerNodeRayHit { node_id, distance });
            }
        }

        // Sort by distance (closest first)
        hits.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());

        hits
    }

    /// Returns a human-readable display name for a node.
    ///
    /// Uses the node's `custom_name` if set, otherwise falls back to
    /// `"{node_type_name} #{node_id}"`.
    pub fn get_node_display_name(&self, node_id: u64) -> String {
        let network = self
            .active_node_network_name
            .as_ref()
            .and_then(|name| self.node_type_registry.node_networks.get(name));

        if let Some(network) = network {
            if let Some(node) = network.nodes.get(&node_id) {
                if let Some(ref custom_name) = node.custom_name {
                    return custom_name.clone();
                }
                return format!("{} #{}", node.node_type_name, node_id);
            }
        }

        format!("node #{}", node_id)
    }

    // -------------------------------------------------------------------------------------------------------------------------
    // --- Preferences management                                                                                         ---
    // -------------------------------------------------------------------------------------------------------------------------

    /// Applies the node display policy to the active node network
    ///
    /// This will resolve the display policy using the current preferences and apply
    /// the changes to the node network. If dirty_node_ids is None, all nodes will be considered dirty.
    ///
    /// # Parameters
    /// * `dirty_node_ids` - The set of node IDs that are dirty, or None to consider all nodes dirty
    pub fn apply_node_display_policy(&mut self, dirty_node_ids: Option<&HashSet<u64>>) {
        // Only apply if there's an active node network
        if let Some(network_name) = &self.active_node_network_name {
            if let Some(node_network) = self.node_type_registry.node_networks.get_mut(network_name)
            {
                // Resolve the display policy with the provided dirty_node_ids
                let changes = self.node_display_policy_resolver.resolve(
                    node_network,
                    &self.preferences.node_display_preferences,
                    dirty_node_ids,
                );

                // Track visibility changes
                for node_id in changes.keys() {
                    self.pending_changes.visibility_changed.insert(*node_id);
                }

                // Apply the changes to the node network
                for (node_id, display_type) in changes {
                    node_network.set_node_display_type(node_id, display_type);
                }
            }
        }
    }

    /// Sets the preferences for the structure designer and applies necessary updates
    pub fn set_preferences(&mut self, preferences: StructureDesignerPreferences) {
        // Check if node display preferences have changed
        let node_display_prefs_changed =
            self.preferences.node_display_preferences != preferences.node_display_preferences;

        // Check if geometry visualization preferences have changed (e.g. the
        // shell-display flag or mesh smoothing). These affect cached evaluator
        // output for every displayed node, so we must re-evaluate.
        let geometry_vis_prefs_changed = self.preferences.geometry_visualization_preferences
            != preferences.geometry_visualization_preferences;

        // Update the preferences
        self.preferences = preferences;

        // If node display preferences have changed, reapply the node display policy
        if node_display_prefs_changed {
            self.apply_node_display_policy(None);
            self.mark_full_refresh();
        } else if geometry_vis_prefs_changed {
            self.mark_full_refresh();
        }
    }

    // -------------------------------------------------------------------------------------------------------------------------
    // --- Gadget delegation methods                                                                                        ---
    // -------------------------------------------------------------------------------------------------------------------------

    pub fn gadget_hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        if let Some(gadget) = &self.gadget {
            return gadget.hit_test(ray_origin, ray_direction);
        }
        None
    }

    pub fn gadget_start_drag(
        &mut self,
        handle_index: i32,
        ray_origin: DVec3,
        ray_direction: DVec3,
    ) {
        // Begin atom_edit drag recording before the gadget starts dragging
        super::nodes::atom_edit::atom_edit::begin_atom_edit_drag(self);

        // For non-atom_edit nodes, snapshot the node data before the drag starts
        // so we can push a SetNodeDataCommand when the drag ends.
        self.begin_gadget_drag_snapshot();

        if let Some(gadget) = &mut self.gadget {
            gadget.start_drag(handle_index, ray_origin, ray_direction);
        }
        self.mark_lightweight_refresh();
    }

    pub fn gadget_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        if let Some(gadget) = &mut self.gadget {
            gadget.drag(handle_index, ray_origin, ray_direction);
        }

        // Continuous minimization: sync atom positions to the diff each frame
        // so the minimizer sees current geometry, then relax neighbors.
        if super::nodes::atom_edit::atom_edit::get_active_atom_edit_data(self)
            .is_some_and(|d| d.continuous_minimization)
        {
            // Apply current gadget delta to diff atom positions
            self.sync_gadget_data();

            // Take promoted_base_atoms out of pending drag to avoid borrow conflict
            let mut promoted = self
                .pending_atom_edit_drag
                .as_mut()
                .map(|p| std::mem::take(&mut p.promoted_base_atoms))
                .unwrap_or_default();
            let _ = super::nodes::atom_edit::atom_edit::continuous_minimize_during_drag(
                self,
                &mut promoted,
            );
            // Put it back
            if let Some(pending) = &mut self.pending_atom_edit_drag {
                pending.promoted_base_atoms = promoted;
            }
        }

        // Gadget dragging only needs lightweight refresh (tessellation update)
        self.mark_lightweight_refresh();
    }

    pub fn gadget_end_drag(&mut self) {
        if let Some(gadget) = &mut self.gadget {
            gadget.end_drag();
            self.sync_gadget_data();

            // Settle burst: run additional steepest descent steps before finalizing
            if super::nodes::atom_edit::atom_edit::get_active_atom_edit_data(self)
                .is_some_and(|d| d.continuous_minimization)
            {
                let mut promoted = self
                    .pending_atom_edit_drag
                    .as_mut()
                    .map(|p| std::mem::take(&mut p.promoted_base_atoms))
                    .unwrap_or_default();
                let _ = super::nodes::atom_edit::atom_edit::continuous_minimize_settle(
                    self,
                    &mut promoted,
                );
                if let Some(pending) = &mut self.pending_atom_edit_drag {
                    pending.promoted_base_atoms = promoted;
                }
            }

            // End atom_edit drag recording and push the coalesced undo command
            super::nodes::atom_edit::atom_edit::end_atom_edit_drag(self);
            // For non-atom_edit nodes, snapshot after and push undo command
            self.end_gadget_drag_snapshot();
            // Ending drag syncs data back to the node
            if let Some(network_name) = &self.active_node_network_name.clone() {
                if let Some(network) = self.node_type_registry.node_networks.get(network_name) {
                    if let Some(node_id) = network.active_node_id {
                        self.mark_node_data_changed(node_id);
                    }
                }
            }
        }
    }

    /// Snapshot the active node's data before a gadget drag starts.
    /// Skips atom_edit nodes (they have their own incremental undo mechanism).
    pub fn begin_gadget_drag_snapshot(&mut self) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return,
        };
        let (node_id, node_type_name) =
            match self.node_type_registry.node_networks.get(&network_name) {
                Some(network) => match network.active_node_id {
                    Some(id) => match network.nodes.get(&id) {
                        Some(node) => (id, node.node_type_name.clone()),
                        None => return,
                    },
                    None => return,
                },
                None => return,
            };

        // atom_edit/motif_edit have their own undo mechanism via begin/end_atom_edit_drag
        if crate::structure_designer::nodes::atom_edit::atom_edit::is_atom_edit_family(
            &node_type_name,
        ) {
            return;
        }

        if let Some(old_data_json) = self.snapshot_node_data(&network_name, node_id) {
            self.pending_gadget_drag = Some(super::undo::snapshot::PendingGadgetDrag {
                network_name,
                node_id,
                node_type_name,
                old_data_json,
            });
        }
    }

    /// After a gadget drag ends and sync_gadget_data() has written back the new values,
    /// compare the before/after snapshots and push a SetNodeDataCommand if they differ.
    pub fn end_gadget_drag_snapshot(&mut self) {
        let pending = match self.pending_gadget_drag.take() {
            Some(p) => p,
            None => return,
        };

        if let Some(new_data_json) = self.snapshot_node_data(&pending.network_name, pending.node_id)
        {
            if pending.old_data_json != new_data_json {
                self.push_command(super::undo::commands::set_node_data::SetNodeDataCommand {
                    description: format!("Edit {}", pending.node_type_name),
                    network_name: pending.network_name,
                    scope_path: Vec::new(),
                    node_id: pending.node_id,
                    node_type_name: pending.node_type_name,
                    old_data_json: pending.old_data_json,
                    new_data_json: new_data_json,
                });
            }
        }
    }

    /// Called when a comment node text field gains focus or resize drag begins.
    /// Captures a snapshot of the comment data before editing starts.
    pub fn begin_comment_edit(&mut self, node_id: u64) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return,
        };
        let node_type_name = match self.node_type_registry.node_networks.get(&network_name) {
            Some(network) => match network.nodes.get(&node_id) {
                Some(node) => node.node_type_name.clone(),
                None => return,
            },
            None => return,
        };

        if let Some(old_data_json) = self.snapshot_node_data(&network_name, node_id) {
            self.pending_comment_edit = Some(super::undo::snapshot::PendingGadgetDrag {
                network_name,
                node_id,
                node_type_name,
                old_data_json,
            });
        }
    }

    /// Called when a comment node text field loses focus or resize drag ends.
    /// Compares the before/after snapshots and pushes a SetNodeDataCommand if they differ.
    pub fn end_comment_edit(&mut self) {
        let pending = match self.pending_comment_edit.take() {
            Some(p) => p,
            None => return,
        };

        if let Some(new_data_json) = self.snapshot_node_data(&pending.network_name, pending.node_id)
        {
            if pending.old_data_json != new_data_json {
                self.push_command(super::undo::commands::set_node_data::SetNodeDataCommand {
                    description: format!("Edit {}", pending.node_type_name),
                    network_name: pending.network_name,
                    scope_path: Vec::new(),
                    node_id: pending.node_id,
                    node_type_name: pending.node_type_name,
                    old_data_json: pending.old_data_json,
                    new_data_json,
                });
            }
        }
    }

    /// Sets a node as the return node for the active network.
    ///
    /// # Parameters
    /// * `node_id` - The ID of the node to set as the return node, or None to clear the return node
    ///
    /// # Returns
    /// Returns true if the operation was successful, false otherwise.
    pub fn set_return_node_id(&mut self, node_id: Option<u64>) -> bool {
        // Early return if active_node_network_name is None
        let network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return false,
        };

        // Capture old return node before mutation
        let old_return_node_id = self
            .node_type_registry
            .node_networks
            .get(&network_name)
            .and_then(|net| net.return_node_id);

        // If node_id is None, clear the return node
        if node_id.is_none() {
            // Look up old return node type name before mutation
            let old_type_name = old_return_node_id.and_then(|id| {
                self.node_type_registry
                    .node_networks
                    .get(&network_name)
                    .and_then(|net| net.nodes.get(&id))
                    .map(|n| n.node_type_name.clone())
            });
            if let Some(network) = self.node_type_registry.node_networks.get_mut(&network_name) {
                network.return_node_id = None;
                // Mark design as dirty since we changed the return node
                self.set_dirty(true);
                self.validate_active_network();

                // Push undo command if return node actually changed
                if old_return_node_id.is_some() {
                    let description = match &old_type_name {
                        Some(name) => format!("Clear {} return node", name),
                        None => "Clear return node".to_string(),
                    };
                    self.push_command(
                        super::undo::commands::set_return_node::SetReturnNodeCommand {
                            network_name,
                            old_return_node_id,
                            new_return_node_id: None,
                            description,
                        },
                    );
                }
                return true;
            }
            return false;
        }

        // Look up new node type name before mutation
        let new_type_name = node_id.and_then(|id| {
            self.node_type_registry
                .node_networks
                .get(&network_name)
                .and_then(|net| net.nodes.get(&id))
                .map(|n| n.node_type_name.clone())
        });
        if let Some(network) = self.node_type_registry.node_networks.get_mut(&network_name) {
            let ret = network.set_return_node(node_id.unwrap());
            if ret {
                // Mark design as dirty since we set the return node
                self.set_dirty(true);

                // Push undo command
                let description = match &new_type_name {
                    Some(name) => format!("Set {} as return node", name),
                    None => "Set return node".to_string(),
                };
                self.push_command(
                    super::undo::commands::set_return_node::SetReturnNodeCommand {
                        network_name,
                        old_return_node_id,
                        new_return_node_id: node_id,
                        description,
                    },
                );
            }
            self.validate_active_network();
            ret
        } else {
            false
        }
    }

    // Saves node networks to a file (Save As functionality)
    pub fn save_node_networks_as(&mut self, file_path: &str) -> std::io::Result<()> {
        use std::path::Path;
        let result = node_networks_serialization::save_node_networks_to_file(
            &mut self.node_type_registry,
            Path::new(file_path),
            self.direct_editing_mode,
            &self.cli_access_rules,
        );

        // Clear dirty flag and set file path if save was successful
        if result.is_ok() {
            self.is_dirty = false;
            self.file_path = Some(file_path.to_string());
        }

        result
    }

    // Saves node networks to the current file (Save functionality)
    pub fn save_node_networks(&mut self) -> Option<std::io::Result<()>> {
        match &self.file_path {
            Some(file_path) => {
                let file_path = file_path.clone(); // Clone to avoid borrow issues
                Some(self.save_node_networks_as(&file_path))
            }
            None => None, // No file path available
        }
    }

    /// Imports selected node networks from the loaded import library
    ///
    /// This is a wrapper around the import manager that adds business logic
    /// such as marking the design as dirty and applying display policies.
    ///
    /// # Arguments
    /// * `network_names` - List of network names to import
    /// * `name_prefix` - Optional prefix to prepend to imported network names
    ///
    /// # Returns
    /// * `Ok(())` if all networks were imported successfully
    /// * `Err(String)` with error message if import failed
    pub fn import_networks(
        &mut self,
        network_names: &[String],
        name_prefix: Option<&str>,
    ) -> Result<(), String> {
        let result = self.import_manager.import_networks_and_clear(
            network_names,
            &mut self.node_type_registry,
            name_prefix,
        );

        if result.is_ok() {
            // Mark as dirty since we modified the design
            self.is_dirty = true;

            // Apply display policy to newly imported networks
            self.apply_node_display_policy(None);

            // Importing networks is a structural change requiring full refresh
            self.mark_full_refresh();
        }

        result
    }

    /// Loads node networks from a file and returns the camera settings of the active network (if any).
    /// Sets the active_node_network_name to the first network if available, otherwise None.
    pub fn load_node_networks(
        &mut self,
        file_path: &str,
    ) -> std::io::Result<Option<CameraSettings>> {
        let load_result = node_networks_serialization::load_node_networks_from_file(
            &mut self.node_type_registry,
            file_path,
        )?;
        let first_network_name = load_result.first_network_name;

        // Validate all networks in dependency order (dependencies first)
        // This ensures call sites can be repaired before validating their parent networks
        let networks_in_order = self.node_type_registry.get_networks_in_dependency_order();
        for network_name in networks_in_order {
            // Split borrows: use raw pointer access to avoid double mutable borrow
            // This is safe because validate_network only mutates the current network and the registry,
            // and we're iterating one network at a time
            let registry_ptr = &mut self.node_type_registry as *mut NodeTypeRegistry;
            unsafe {
                if let Some(network) = (*registry_ptr).node_networks.get_mut(&network_name) {
                    validate_network(network, &mut *registry_ptr, None);
                }
            }
        }

        // Clear navigation history since we're loading a new design file
        self.navigation_history.clear();

        // Clear undo stack — loaded file starts with fresh history
        self.undo_stack.clear();
        self.pending_move = None;
        self.pending_gadget_drag = None;

        // Set active node network to the first network if available, otherwise None
        // Capture camera settings from the newly active network
        let camera_settings = if first_network_name.is_empty() {
            self.set_active_node_network_name(None)
        } else {
            self.set_active_node_network_name(Some(first_network_name))
        };

        // Apply display policy to all nodes
        self.apply_node_display_policy(None);

        // Clear CSG conversion cache since we loaded a completely new file
        self.network_evaluator.clear_csg_cache();

        // Restore CLI access rules from the loaded file
        self.cli_access_rules = load_result.cli_access_rules;

        // Clear dirty flag since we just loaded a saved state
        self.is_dirty = false;

        // Set the file path since we just loaded from this file
        self.file_path = Some(file_path.to_string());

        // Restore direct editing mode from file, with validation
        if load_result.direct_editing_mode {
            if self.can_switch_to_direct_editing_mode() {
                self.direct_editing_mode = true;
            } else {
                // Criteria not met — fall back to node network mode
                self.direct_editing_mode = false;
                println!(
                    "Warning: Could not enter Direct Editing Mode — opening in Node Network Mode."
                );
            }
        } else {
            self.direct_editing_mode = false;
        }

        // Loading networks is a structural change requiring full refresh
        self.mark_full_refresh();

        Ok(camera_settings)
    }

    /// Validates the active node network and propagates validation to dependent networks
    ///
    /// This method implements dependency invalidation propagation:
    /// - When a network becomes valid, invalid parent networks need revalidation
    /// - When a network becomes invalid, valid parent networks need revalidation
    /// - Continues until no more networks need validation
    pub fn validate_active_network(&mut self) -> Option<NetworkValidationResult> {
        self.validate_active_network_with_initial_errors(None)
    }

    /// Validates the active network with optional initial validation errors (e.g., from expr nodes)
    fn validate_active_network_with_initial_errors(
        &mut self,
        initial_errors: Option<Vec<crate::structure_designer::node_network::ValidationError>>,
    ) -> Option<NetworkValidationResult> {
        // Get the active network name
        let network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return None,
        };

        // Initialize the set of networks to validate
        let mut to_validate = HashSet::new();
        to_validate.insert(network_name.clone());

        let mut final_result = None;

        // Process networks until the set is empty
        while let Some(current_network_name) = to_validate.iter().next().cloned() {
            to_validate.remove(&current_network_name);

            // Get the current validation state before validation
            let was_valid = self
                .node_type_registry
                .node_networks
                .get(&current_network_name)
                .map(|network| network.valid)
                .unwrap_or(false);

            // Validate the current network
            let validation_result = {
                // Check if network exists first
                if !self
                    .node_type_registry
                    .node_networks
                    .contains_key(&current_network_name)
                {
                    continue; // Skip if network doesn't exist
                }

                // Extract the network temporarily to avoid borrowing conflicts
                let mut network = self
                    .node_type_registry
                    .node_networks
                    .remove(&current_network_name)
                    .unwrap();

                // Use initial errors only for the originally requested network
                let errors_to_use = if current_network_name == network_name {
                    initial_errors.clone()
                } else {
                    None
                };

                // Validate with the registry and initial errors
                let result =
                    validate_network(&mut network, &mut self.node_type_registry, errors_to_use);

                // Put the network back
                self.node_type_registry
                    .node_networks
                    .insert(current_network_name.clone(), network);

                result
            };

            // Store the result if this is the originally requested network
            if current_network_name == network_name {
                final_result = Some(validation_result.clone());
            }

            // Check if validation state changed OR interface changed
            let is_now_valid = validation_result.valid;
            let interface_changed = validation_result.interface_changed;

            if was_valid != is_now_valid || interface_changed {
                // Find all parent networks that use this network as a node
                let parent_networks = self
                    .node_type_registry
                    .find_parent_networks(&current_network_name);

                for parent_name in parent_networks {
                    if interface_changed {
                        // If interface changed, validate ALL parent networks regardless of their current state
                        to_validate.insert(parent_name);
                    } else if let Some(parent_network) =
                        self.node_type_registry.node_networks.get(&parent_name)
                    {
                        // If only validity changed, add parent networks based on validity logic:
                        // - Parent is invalid and child became valid (parent might become valid)
                        // - Parent is valid and child became invalid (parent might become invalid)
                        if (!parent_network.valid && is_now_valid)
                            || (parent_network.valid && !is_now_valid)
                        {
                            to_validate.insert(parent_name);
                        }
                    }
                }

                // Clear clipboard if it contains nodes of the changed type
                if interface_changed {
                    if let Some(ref clipboard) = self.clipboard {
                        if clipboard
                            .nodes
                            .values()
                            .any(|n| n.node_type_name == current_network_name)
                        {
                            self.clipboard = None;
                        }
                    }
                }
            }
        }

        final_result
    }

    /// Evaluate a specific node and return its result for CLI inspection.
    ///
    /// This triggers evaluation of the node (if not already cached) and returns
    /// the NetworkResult converted to strings for display.
    ///
    /// # Arguments
    /// * `node_id` - The ID of the node to evaluate
    /// * `verbose` - If true, include detailed output for complex types
    ///
    /// # Returns
    /// * `Ok(APINodeEvaluationResult)` - The evaluation result
    /// * `Err(String)` - If node not found or network not active
    pub fn evaluate_node_for_cli(
        &mut self,
        node_id: u64,
        verbose: bool,
    ) -> Result<APINodeEvaluationResult, String> {
        // Check that an active network is set
        let network_name = self
            .active_node_network_name
            .as_ref()
            .ok_or_else(|| "No active node network".to_string())?
            .clone();

        // Get the network and verify the node exists
        let network = self
            .node_type_registry
            .node_networks
            .get(&network_name)
            .ok_or_else(|| format!("Network '{}' not found", network_name))?;

        // Check if the network is valid
        if !network.valid {
            return Err(format!(
                "Network '{}' is invalid and cannot be evaluated",
                network_name
            ));
        }

        // Look up the node
        let node = network
            .nodes
            .get(&node_id)
            .ok_or_else(|| format!("Node {} not found in network '{}'", node_id, network_name))?;

        // Get the node type name and custom name
        let node_type_name = node.node_type_name.clone();
        let custom_name = node.custom_name.clone();

        // Get the output type from the node type registry
        let output_type = self
            .node_type_registry
            .get_node_type_for_node(node)
            .map(|nt| nt.output_type().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        // Evaluate the node (output pin 0 is the main output) through the
        // central context helper so prints / future per-pass state drain
        // consistently. CLI evaluations are normal (non-Execute) display
        // passes — Phase 3's `execute_node` orchestrator is what passes
        // `execute = true`.
        let result = self.with_eval_context(false, |evaluator, registry, _prefs, context| {
            let network = registry.node_networks.get(&network_name).unwrap();
            let network_stack = vec![NetworkStackElement {
                node_network: network,
                node_id: 0,
            }];
            evaluator.evaluate(
                &network_stack,
                node_id,
                0, // output pin index
                registry,
                false, // decorate - false since this is just for text output
                context,
            )
        });

        // Build the response
        let display_string = result.to_display_string();
        let detailed_string = if verbose {
            Some(result.to_detailed_string())
        } else {
            None
        };

        // Check for errors
        let (success, error_message) = match &result {
            NetworkResult::Error(msg) => (false, Some(msg.clone())),
            _ => (true, None),
        };

        Ok(APINodeEvaluationResult {
            node_id,
            node_type_name,
            custom_name,
            output_type,
            display_string,
            detailed_string,
            success,
            error_message,
        })
    }

    /// Run an explicit **Execute** pass on a single node.
    ///
    /// Triggered from the right-click context menu in the node-graph UI. Sets
    /// `context.execute = true` for one evaluation pass through
    /// `with_eval_context`, which is what gates side-effect nodes
    /// (`export_xyz`, `foreach`, `print` with `execute_only`, …) to actually
    /// fire. Independent of display state: whether the node is visible or
    /// not, the targeted node and its transitive inputs are evaluated fresh.
    /// One-shot: no subscription, no recurring trigger — the user must invoke
    /// it again to re-fire. See `doc/design_node_execution.md` (Phase 3).
    ///
    /// Errors during the pass surface as `APIExecuteResult::error`; structural
    /// problems (missing network, missing node) surface as `Err(String)`.
    pub fn execute_node(
        &mut self,
        network_name: &str,
        node_id: u64,
    ) -> Result<APIExecuteResult, String> {
        // Verify the network and node exist before launching the pass — keeps
        // the orchestrator honest about what an empty `Ok(_)` means.
        let network = self
            .node_type_registry
            .node_networks
            .get(network_name)
            .ok_or_else(|| format!("Network '{}' not found", network_name))?;
        if !network.valid {
            return Err(format!(
                "Network '{}' is invalid and cannot be executed",
                network_name
            ));
        }
        if !network.nodes.contains_key(&node_id) {
            return Err(format!(
                "Node {} not found in network '{}'",
                node_id, network_name
            ));
        }

        let network_name_owned = network_name.to_string();
        // Slice off entries appended by *this* pass only — any earlier
        // display-pass prints already in `print_log` are left alone for the
        // Console panel's normal `take_print_log` polling cadence to pick up.
        // Without this slicing the panel would re-receive prior entries via
        // `APIExecuteResult.logs` and double-display them. See
        // `doc/design_node_execution.md` (Centralized drain).
        let pass_start = self.print_log.len();
        // Run the pass with `execute = true`. The central skip rule in the
        // evaluator only invokes `eval` on a Unit-returning node when this
        // flag is set; that is what lets `export_xyz` / `foreach` /
        // `print(execute_only)` fire here while staying inert during display.
        let result = self.with_eval_context(true, |evaluator, registry, _prefs, context| {
            let network = registry.node_networks.get(&network_name_owned).unwrap();
            let network_stack = vec![NetworkStackElement {
                node_network: network,
                node_id: 0,
            }];
            evaluator.evaluate(
                &network_stack,
                node_id,
                0, // Execute always targets pin 0 — the right-click menu fires on the node, not a pin.
                registry,
                false, // decorate
                context,
            )
        });

        let (ok, error) = match result {
            NetworkResult::Error(msg) => (false, Some(msg)),
            _ => (true, None),
        };

        let logs: Vec<
            crate::api::structure_designer::structure_designer_api_types::APIPrintLogEntry,
        > = self.print_log[pass_start..]
            .iter()
            .map(Into::into)
            .collect();

        Ok(APIExecuteResult { ok, error, logs })
    }

    /// Best-effort: evaluate the `default` input pin of the parameter node
    /// named `param_name` inside `subnetwork_name`, in isolation.
    ///
    /// Evaluating with a single-element network stack ("in isolation") makes
    /// `parameter.rs::eval` take its `eval_default` path — the value of the
    /// parameter node's `default` input pin (argument 0). An unconnected
    /// default pin or any evaluation error surfaces as a `NetworkResult` the
    /// caller can reject (`NetworkResult::None` / `NetworkResult::Error`).
    ///
    /// Returns `None` only when the subnetwork or the named parameter node
    /// cannot be found.
    ///
    /// Takes `&mut self`: evaluation goes through `with_eval_context`, which
    /// mutably borrows the evaluator and drains the print buffer. This is
    /// logically a read, but not side-effect-free.
    pub fn resolve_parameter_default(
        &mut self,
        subnetwork_name: &str,
        param_name: &str,
    ) -> Option<NetworkResult> {
        use crate::structure_designer::nodes::parameter::ParameterData;

        // Find the parameter node id by name. The registry borrow ends here,
        // before the `&mut self` `with_eval_context` call below.
        let param_node_id = {
            let subnetwork = self.node_type_registry.node_networks.get(subnetwork_name)?;
            subnetwork.nodes.iter().find_map(|(id, node)| {
                if node.node_type_name != "parameter" {
                    return None;
                }
                let param_data = node.data.as_any_ref().downcast_ref::<ParameterData>()?;
                (param_data.param_name == param_name).then_some(*id)
            })?
        };

        let subnetwork_name = subnetwork_name.to_string();
        let result = self.with_eval_context(false, |evaluator, registry, _prefs, context| {
            let subnetwork = registry.node_networks.get(&subnetwork_name).unwrap();
            let network_stack = vec![NetworkStackElement {
                node_network: subnetwork,
                node_id: 0,
            }];
            evaluator.evaluate(&network_stack, param_node_id, 0, registry, false, context)
        });
        Some(result)
    }

    /// Find a node ID by its display name in the active network.
    ///
    /// Since all nodes have persistent names assigned at creation,
    /// this is a simple search through the custom_name fields.
    ///
    /// # Arguments
    /// * `name` - The name to search for
    ///
    /// # Returns
    /// * `Some(node_id)` if a node with the given name exists
    /// * `None` if no node with the given name is found or no network is active
    pub fn find_node_id_by_name(&self, name: &str) -> Option<u64> {
        let network_name = self.active_node_network_name.as_ref()?;
        let network = self.node_type_registry.node_networks.get(network_name)?;

        for (node_id, node) in &network.nodes {
            if node.custom_name.as_deref() == Some(name) {
                return Some(*node_id);
            }
        }

        None
    }

    /// Exports all visible atomic structures as a single file (XYZ or MOL format)
    /// Merges all atomic structures from the last generated scene into one structure before saving
    /// File format is determined by the file extension (.xyz or .mol)
    pub fn export_visible_atomic_structures(&self, file_path: &str) -> Result<(), String> {
        use crate::structure_designer::structure_designer_scene::NodeOutput;

        // Create a new merged atomic structure
        let mut merged_structure = AtomicStructure::new();
        let mut has_structures = false;

        // Merge all displayed atomic structures from node_data into one
        for node_data in self
            .last_generated_structure_designer_scene
            .node_data
            .values()
        {
            for (_pin_index, pin_output, _pin_geo_tree) in node_data.displayed_outputs() {
                if let NodeOutput::Atomic(atomic_structure, _) = pin_output {
                    merged_structure.add_atomic_structure(atomic_structure);
                    has_structures = true;
                }
            }
        }

        // Check if we have any atomic structures to export
        if !has_structures {
            return Err("No atomic structures available to export".to_string());
        }

        // Check if the merged structure has any atoms
        if merged_structure.get_num_of_atoms() == 0 {
            return Err("No atoms found in the atomic structures to export".to_string());
        }

        // Determine file format from extension and save accordingly
        let file_path_lower = file_path.to_lowercase();
        if file_path_lower.ends_with(".xyz") {
            match save_xyz(&merged_structure, file_path) {
                Ok(()) => Ok(()),
                Err(err) => Err(format!("Failed to save XYZ file '{}': {}", file_path, err)),
            }
        } else if file_path_lower.ends_with(".mol") {
            match save_mol_v3000(&merged_structure, file_path) {
                Ok(()) => Ok(()),
                Err(err) => Err(format!("Failed to save MOL file '{}': {}", file_path, err)),
            }
        } else {
            Err(format!(
                "Unsupported file format. Please use .xyz or .mol extension. Got: {}",
                file_path
            ))
        }
    }

    /// Factors the current selection in the active network into a new subnetwork.
    ///
    /// This is a thin wrapper that coordinates the functions from the selection_factoring module.
    ///
    /// # Arguments
    /// * `subnetwork_name` - The name for the new subnetwork (must not already exist)
    /// * `param_names` - Names for the parameters (must match the number of external inputs)
    ///
    /// # Returns
    /// * `Ok(new_node_id)` - The ID of the new custom node that replaced the selection
    /// * `Err(String)` - If validation fails or any error occurs
    pub fn factor_selection_into_subnetwork(
        &mut self,
        subnetwork_name: &str,
        param_names: Vec<String>,
    ) -> Result<u64, String> {
        use super::selection_factoring;

        // 1. Validate the name itself (relaxed user-name rules) and that it
        // does not already exist.
        if let Err(reason) = super::identifier::is_valid_user_name(subnetwork_name) {
            return Err(format!("Invalid subnetwork name: {}", reason));
        }
        if self
            .node_type_registry
            .get_node_type(subnetwork_name)
            .is_some()
        {
            return Err(format!("Node type '{}' already exists", subnetwork_name));
        }

        // 2. Get active network
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or("No active network")?;

        // 3. Analyze selection (using module function)
        let network = self
            .node_type_registry
            .node_networks
            .get(&network_name)
            .ok_or("Network not found")?;
        let analysis =
            selection_factoring::analyze_selection_for_factoring(network, &self.node_type_registry);

        if !analysis.is_valid {
            return Err(analysis
                .invalid_reason
                .unwrap_or("Invalid selection".to_string()));
        }

        // 4. Validate param names count matches
        if param_names.len() != analysis.external_inputs.len() {
            return Err(format!(
                "Parameter count mismatch: expected {}, got {}",
                analysis.external_inputs.len(),
                param_names.len()
            ));
        }

        // 5. Snapshot source network BEFORE factoring (for undo)
        let source_network_before = self.snapshot_network(&network_name);

        // 6. Create subnetwork (using module function)
        let source_network = self
            .node_type_registry
            .node_networks
            .get(&network_name)
            .unwrap();
        let new_network = selection_factoring::create_subnetwork_from_selection(
            source_network,
            &analysis,
            subnetwork_name,
            &param_names,
            &self.node_type_registry,
        );

        // 7. Register subnetwork
        let num_params = new_network.node_type.parameters.len();
        self.node_type_registry.add_node_network(new_network);

        // 8. Replace selection with custom node (using module function)
        let network = self
            .node_type_registry
            .node_networks
            .get_mut(&network_name)
            .unwrap();
        let new_node_id = selection_factoring::replace_selection_with_custom_node(
            network,
            &analysis,
            subnetwork_name,
            num_params,
        );

        // 9. Validate networks
        self.validate_active_network();

        // 10. Push undo command
        if let Some(source_before) = source_network_before {
            if let (Some(source_after), Some(subnetwork_snap)) = (
                self.snapshot_network(&network_name),
                self.snapshot_network(subnetwork_name),
            ) {
                use super::undo::commands::factor_selection::FactorSelectionCommand;
                self.push_command(FactorSelectionCommand {
                    source_network_name: network_name.clone(),
                    subnetwork_name: subnetwork_name.to_string(),
                    source_network_before: source_before,
                    source_network_after: source_after,
                    subnetwork_snapshot: subnetwork_snap,
                });
            }
        }

        // 11. Mark dirty and schedule refresh
        self.is_dirty = true;
        self.mark_full_refresh();

        Ok(new_node_id)
    }

    /// Promotes a node to a parameter.
    ///
    /// Creates a new `parameter` node typed after the given node's output
    /// pin 0 (resolved), wires that pin into the parameter's default input,
    /// and rewires every downstream consumer of pin 0 — including a return
    /// node reference — to read from the parameter instead. The source node
    /// becomes the parameter's default value provider.
    ///
    /// # Returns
    /// * `Ok(parameter_node_id)` — id of the newly created parameter node.
    /// * `Err(String)` — if the node doesn't exist, is already a parameter,
    ///   or its pin 0 type is not eligible (abstract, `Function`, `Unit`,
    ///   `Iter[T]`, unresolved).
    pub fn promote_node_to_parameter(&mut self, node_id: u64) -> Result<u64, String> {
        use super::promote_to_parameter;

        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or("No active network")?;

        let network_before = self
            .snapshot_network(&network_name)
            .ok_or_else(|| "Failed to snapshot network".to_string())?;

        // Temporarily remove the network from the registry so we can pass
        // both `&mut NodeNetwork` and `&NodeTypeRegistry` to the core function
        // without a borrow conflict.
        let mut network = self
            .node_type_registry
            .node_networks
            .remove(&network_name)
            .ok_or("Network not found")?;

        let promote_result = promote_to_parameter::promote_node_to_parameter(
            &mut network,
            node_id,
            &self.node_type_registry,
        );

        self.node_type_registry
            .node_networks
            .insert(network_name.clone(), network);

        let new_id = promote_result?;

        // Refresh custom node type cache on the new parameter node so its
        // pin signature is populated.
        let (built_in_types, record_type_defs, built_in_record_type_defs, node_networks) = (
            &self.node_type_registry.built_in_node_types,
            &self.node_type_registry.record_type_defs,
            &self.node_type_registry.built_in_record_type_defs,
            &mut self.node_type_registry.node_networks,
        );
        if let Some(network) = node_networks.get_mut(&network_name) {
            if let Some(node) = network.nodes.get_mut(&new_id) {
                NodeTypeRegistry::populate_custom_node_type_cache_with_types(
                    built_in_types,
                    record_type_defs,
                    built_in_record_type_defs,
                    node,
                    true,
                );
            }
        }

        // Mark the new parameter as displayed so the user can see it.
        if let Some(network) = self.node_type_registry.node_networks.get_mut(&network_name) {
            network.set_node_display(new_id, true);
            self.pending_changes.visibility_changed.insert(new_id);
        }

        self.validate_active_network();

        if let Some(network_after) = self.snapshot_network(&network_name) {
            use super::undo::commands::promote_to_parameter::PromoteToParameterCommand;
            self.push_command(PromoteToParameterCommand {
                network_name: network_name.clone(),
                network_before,
                network_after,
            });
        }

        self.is_dirty = true;
        self.mark_full_refresh();

        Ok(new_id)
    }

    /// Gets information about whether/how the current selection can be factored.
    ///
    /// This analyzes the selection without making any changes, returning information
    /// that can be used to populate the factoring dialog.
    ///
    /// # Returns
    /// Information about the selection's eligibility for factoring and suggested names.
    pub fn get_factor_selection_info(&self) -> FactorSelectionInfo {
        use super::selection_factoring;

        // Check for active network
        let network_name = match &self.active_node_network_name {
            Some(name) => name,
            None => {
                return FactorSelectionInfo {
                    can_factor: false,
                    invalid_reason: Some("No active network".to_string()),
                    suggested_name: String::new(),
                    suggested_param_names: Vec::new(),
                };
            }
        };

        // Get the network
        let network = match self.node_type_registry.node_networks.get(network_name) {
            Some(n) => n,
            None => {
                return FactorSelectionInfo {
                    can_factor: false,
                    invalid_reason: Some("Network not found".to_string()),
                    suggested_name: String::new(),
                    suggested_param_names: Vec::new(),
                };
            }
        };

        // Analyze selection
        let analysis =
            selection_factoring::analyze_selection_for_factoring(network, &self.node_type_registry);

        if !analysis.is_valid {
            return FactorSelectionInfo {
                can_factor: false,
                invalid_reason: analysis.invalid_reason,
                suggested_name: String::new(),
                suggested_param_names: Vec::new(),
            };
        }

        // Generate suggested name
        let suggested_name = self.generate_unique_subnetwork_name();

        // Collect suggested parameter names
        let suggested_param_names: Vec<String> = analysis
            .external_inputs
            .iter()
            .map(|input| input.suggested_name.clone())
            .collect();

        FactorSelectionInfo {
            can_factor: true,
            invalid_reason: None,
            suggested_name,
            suggested_param_names,
        }
    }

    /// Generates a unique subnetwork name like "subnetwork1", "subnetwork2", etc.
    fn generate_unique_subnetwork_name(&self) -> String {
        let base = "subnetwork";
        let mut counter = 1;
        loop {
            let name = format!("{}{}", base, counter);
            if self.node_type_registry.get_node_type(&name).is_none() {
                return name;
            }
            counter += 1;
        }
    }

    // =========================================================================
    // CLI Access Rules
    // =========================================================================

    /// Check whether CLI write access is locked for a given network name.
    ///
    /// Finds the longest matching prefix in `cli_access_rules` and returns its value.
    /// If no rule matches, CLI write access is allowed (default: unlocked).
    pub fn is_cli_write_locked(&self, network_name: &str) -> bool {
        let mut best_prefix_len = 0usize;
        let mut locked = false; // default: unlocked

        for (prefix, &allowed) in &self.cli_access_rules {
            // The prefix must match exactly or be a proper namespace prefix (followed by '.')
            let matches =
                network_name == prefix || network_name.starts_with(&format!("{}.", prefix));
            if matches && prefix.len() > best_prefix_len {
                best_prefix_len = prefix.len();
                locked = !allowed;
            }
        }

        locked
    }

    /// Set CLI access for a namespace or network name, pruning all descendant rules.
    ///
    /// `allowed = true` means CLI can write, `allowed = false` means CLI is locked out.
    /// After setting, all entries whose prefix is a descendant of `name` are removed,
    /// keeping the map minimal and easy to reason about.
    pub fn set_cli_access(&mut self, name: &str, allowed: bool) {
        // Remove all descendant rules
        let child_prefix = format!("{}.", name);
        self.cli_access_rules
            .retain(|k, _| k != name && !k.starts_with(&child_prefix));

        // Insert the new rule
        self.cli_access_rules.insert(name.to_string(), allowed);

        self.is_dirty = true;
    }

    /// Remove the CLI access rule for a specific prefix (revert to inherited behavior).
    pub fn clear_cli_access(&mut self, name: &str) {
        if self.cli_access_rules.remove(name).is_some() {
            self.is_dirty = true;
        }
    }

    /// Get all CLI access rules (for serialization and UI display).
    pub fn get_cli_access_rules(&self) -> &HashMap<String, bool> {
        &self.cli_access_rules
    }
}

/// Information about whether/how a selection can be factored into a subnetwork
pub struct FactorSelectionInfo {
    /// Whether the selection can be factored
    pub can_factor: bool,
    /// If not valid, the reason why
    pub invalid_reason: Option<String>,
    /// Suggested name for the new subnetwork
    pub suggested_name: String,
    /// Suggested names for the parameters (one per external input)
    pub suggested_param_names: Vec<String>,
}

/// Clear selection and active state in [network] and every HOF body reachable
/// from it. Used by `clear_selection_all_scopes` so an empty-space click at
/// the top level deselects body nodes too.
fn clear_selection_recursive(network: &mut NodeNetwork) {
    network.clear_selection();
    for node in network.nodes.values_mut() {
        if let Some(zone) = node.zone_mut() {
            clear_selection_recursive(zone);
        }
    }
}

/// Walk `network` along `node_ref.scope_path` and return the data for the
/// node at the precise scoped address. Returns `None` if any HOF on the path
/// is missing a zone or doesn't exist. Used by the partial refresh path to
/// clear input caches without relying on the ambiguous "first id match"
/// fallback in [`find_node_data_recursive`].
fn find_node_data_at_scope<'a>(
    network: &'a NodeNetwork,
    node_ref: &NodeRef,
) -> Option<&'a dyn NodeData> {
    let mut current: &NodeNetwork = network;
    for hof_id in &node_ref.scope_path {
        let hof = current.nodes.get(hof_id)?;
        current = hof.zone.as_deref()?;
    }
    current.get_node_network_data(node_ref.node_id)
}

/// Walk [network] and every HOF body reachable from it, returning the first
/// node with id == [node_id]. Used by `get_node_network_data` so per-node-type
/// data getters work for body nodes in U4. The walk is depth-first; the
/// first match wins.
fn find_node_data_recursive(network: &NodeNetwork, node_id: u64) -> Option<&dyn NodeData> {
    if let Some(data) = network.get_node_network_data(node_id) {
        return Some(data);
    }
    for node in network.nodes.values() {
        if let Some(zone) = node.zone.as_ref() {
            if let Some(data) = find_node_data_recursive(zone, node_id) {
                return Some(data);
            }
        }
    }
    None
}

/// Mutable variant of [`find_node_data_recursive`]. Each step calls
/// `Node::zone_mut` so the `Arc<NodeNetwork>` is uniquely owned (CoW) before
/// recursion continues.
fn find_node_data_mut_recursive(
    network: &mut NodeNetwork,
    node_id: u64,
) -> Option<&mut dyn NodeData> {
    if network.nodes.contains_key(&node_id) {
        return network.get_node_network_data_mut(node_id);
    }
    for node in network.nodes.values_mut() {
        if let Some(zone) = node.zone_mut() {
            if let Some(data) = find_node_data_mut_recursive(zone, node_id) {
                return Some(data);
            }
        }
    }
    None
}
