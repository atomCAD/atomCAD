use super::camera_settings::CameraSettings;
use super::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement, PrintLogEntry,
};
use super::evaluator::network_result::NetworkResult;
use super::navigation_history::NavigationHistory;
use super::network_validator::{NetworkValidationResult, validate_network};
use super::node_display_policy_resolver::NodeDisplayPolicyResolver;
use super::node_network::{
    CollapseMode, NodeNetwork, NodeRef, Wire, collapsable_type_name, resolve_body_collapsed,
};
use super::node_network_gadget::NodeNetworkGadget;
use super::node_networks_import_manager::NodeNetworksImportManager;
use super::node_type::{NodeType, OutputPinDefinition};
use super::node_type_registry::{NodeTypeRegistry, UserTypeKind};
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

/// One network affected by a namespace rename/move: its current name and the
/// name it would take. `conflict` is `true` when `new_name` collides with an
/// existing user type (network, record def, or built-in) that is *not* itself
/// part of this rename — i.e. applying the rename as-is would be rejected.
#[derive(Debug, Clone)]
pub struct NamespaceRenameItem {
    pub old_name: String,
    pub new_name: String,
    pub conflict: bool,
    /// Whether this leaf is a custom network or a user record def. Drives the
    /// per-kind dispatch in `rename_namespace` and the kind-tagged undo command.
    pub kind: UserTypeKind,
}

/// The full plan for shifting every network under one namespace prefix to a
/// new prefix. Drives both the live preview (`preview_namespace_rename`) and
/// the actual mutation (`rename_namespace`). An empty target prefix promotes
/// the contents to the top level (root). Items are sorted by `old_name` for a
/// deterministic preview.
#[derive(Debug, Clone)]
pub struct NamespaceRenamePlan {
    pub items: Vec<NamespaceRenameItem>,
    /// `false` if any resulting name (entity or folder) fails the user-name
    /// rules (empty, backtick, control chars, edge whitespace).
    pub valid_names: bool,
    /// Empty-folder markers remapped by this move: `(old_marker, new_marker)`
    /// where `new_marker` is `None` when the folder vanishes (an empty folder
    /// promoted to root). Lets the move/rename of an *empty* folder be
    /// applicable even though no entities are affected. See
    /// `doc/design_empty_folders.md`.
    pub folder_changes: Vec<(String, Option<String>)>,
    /// `true` if any remapped folder target collides with an existing,
    /// non-affected user type or folder.
    pub folder_conflict: bool,
}

impl NamespaceRenamePlan {
    /// Nothing matches the source prefix — no entities and no empty folders to
    /// rename.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.folder_changes.is_empty()
    }

    /// Any resulting name collides with an existing, non-affected user type.
    pub fn has_conflicts(&self) -> bool {
        self.folder_conflict || self.items.iter().any(|item| item.conflict)
    }

    /// The plan can be applied: it affects at least one entity or empty folder,
    /// every resulting name is valid, and nothing collides.
    pub fn is_applicable(&self) -> bool {
        !self.is_empty() && self.valid_names && !self.has_conflicts()
    }
}

pub struct StructureDesigner {
    pub node_type_registry: NodeTypeRegistry,
    pub network_evaluator: NetworkEvaluator,
    pub gadget: Option<Box<dyn NodeNetworkGadget>>,
    pub active_node_network_name: Option<String>,
    /// The user record type def currently open in the schema editor, if any.
    /// Backend-owned (mirrors `active_node_network_name`) so it survives
    /// undo/redo of a rename/move/delete — the undo commands remap or clear it
    /// through `UndoContext`. Flutter mirrors this value in `refreshFromKernel`
    /// (Phase 2). See `doc/design_hierarchical_records.md` §8.
    pub active_record_def_name: Option<String>,
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
    // Temporary state during an HOF body resize drag (for undo coalescing)
    pub pending_zone_resize: Option<super::undo::snapshot::PendingZoneResize>,
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

    // Per-load report of parameter-id de-duplication repairs (F6 of
    // `doc/design_parameter_wire_stability.md`). Populated by
    // `load_node_networks` when a loaded project contained duplicate `param_id`s
    // left by the `next_param_id` bug; drained by `take_load_param_id_repairs`
    // so the UI can show a one-time "auto-repaired" modal. Empty when the loaded
    // project needed no repair.
    pub pending_load_param_id_repairs: Vec<String>,
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
            active_record_def_name: None,
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
            pending_zone_resize: None,
            direct_editing_mode: true,
            cli_access_rules: HashMap::new(),
            print_log: Vec::new(),
            pending_load_param_id_repairs: Vec::new(),
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
    /// **This is the only `NetworkEvaluationContext::new()` caller inside
    /// `rust/src/structure_designer/`.** The eager HOFs (`fold`/`foreach`)
    /// build their per-iteration body context via `fresh_inner_for_eager_body`
    /// (a struct literal, outside the `::new()` audit) and drain it back; the
    /// old `FunctionEvaluator::evaluate` inner-body context was removed in
    /// closures Phase 2. Reviewers grepping for `NetworkEvaluationContext::new(`
    /// outside this site — and outside test crates, which are exempt — have a
    /// one-shot audit. Centralising the construct + drain pair eliminates the
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

    /// Drains the parameter-id repair messages produced by the most recent
    /// `load_node_networks` (F6 of `doc/design_parameter_wire_stability.md`).
    /// Returns an empty vector when the loaded project needed no repair. The UI
    /// reads this once after a load to decide whether to show the "auto-repaired"
    /// modal.
    pub fn take_load_param_id_repairs(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_load_param_id_repairs)
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

    /// Returns the enclosing-zone ancestor chain for `scope_path`, in the
    /// indexing convention used by the validator and
    /// [`NodeTypeRegistry::resolve_output_type_scoped`]: `ancestors[i]` is the
    /// network at depth `i` from the active root, and `ancestor_hof_ids[i]` is
    /// the HOF id in `ancestors[i]` whose zone body is `ancestors[i + 1]` — the
    /// deepest entry's body being the network `scope_path` itself resolves to
    /// (i.e. [`get_scope_network`]'s return value). Both vectors have length
    /// `scope_path.len()`; an empty `scope_path` yields two empty vectors
    /// (top-level network, no enclosing zones). The resolved body network is
    /// **not** included — it is what the caller passes as the resolver's
    /// `network` argument.
    pub fn get_scope_ancestors(&self, scope_path: &[u64]) -> Option<(Vec<&NodeNetwork>, Vec<u64>)> {
        let network_name = self.active_node_network_name.as_ref()?;
        let mut current = self.node_type_registry.node_networks.get(network_name)?;
        let mut ancestors: Vec<&NodeNetwork> = Vec::with_capacity(scope_path.len());
        let mut hof_ids: Vec<u64> = Vec::with_capacity(scope_path.len());
        for hof_id in scope_path {
            ancestors.push(current);
            hof_ids.push(*hof_id);
            let node = current.nodes.get(hof_id)?;
            let zone = node.zone.as_ref()?;
            current = zone;
        }
        Some((ancestors, hof_ids))
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

    /// If the just-added node opts into displaying all of its output pins by
    /// default (see [`NodeData::default_display_all_output_pins`]) and it is
    /// currently displayed, mark its remaining output pins (`1..N`) displayed —
    /// pin 0 is already on from `NodeNetwork::add_node`. Both display setters
    /// preserve an existing `displayed_pins` set, so calling this *after* the
    /// display-policy pass is safe (the policy only flips the node-level
    /// display type, never the pin set). We deliberately do not force-show a
    /// node the policy chose to hide. Used by the stateless unpack/destructure
    /// nodes so every unpacked value is hover-inspectable the moment the node
    /// is dropped (their outputs draw no viewport geometry). See
    /// `doc/design_structure_lattice_unpack_nodes.md`.
    fn apply_default_all_pin_display(
        &mut self,
        scope_path: &[u64],
        node_type_name: &str,
        node_id: u64,
    ) {
        let pin_count = match self.node_type_registry.get_node_type(node_type_name) {
            Some(node_type) => node_type.output_pins.len(),
            None => return,
        };
        if pin_count <= 1 {
            return;
        }
        if let Some(network) = self.get_scope_network_mut(scope_path) {
            let opt_in = network
                .nodes
                .get(&node_id)
                .map(|n| n.data.default_display_all_output_pins())
                .unwrap_or(false);
            if !opt_in || !network.is_node_displayed(node_id) {
                return;
            }
            for pin_index in 1..pin_count {
                network.set_pin_displayed(node_id, pin_index as i32, true);
            }
        }
    }

    /// Returns the scope path of the network that currently holds the
    /// selection, or `None` if nothing is selected anywhere. An empty `Vec`
    /// means the top-level active network. The single-scope selection
    /// invariant guarantees at most one network has a non-empty selection, so
    /// the first match found by the depth-first walk is unambiguous. Used by
    /// copy/cut so they operate on the selection wherever it lives, including
    /// inside a zone body.
    pub fn find_selection_scope(&self) -> Option<Vec<u64>> {
        let network_name = self.active_node_network_name.as_ref()?;
        let network = self.node_type_registry.node_networks.get(network_name)?;
        let mut prefix = Vec::new();
        find_selection_scope_recursive(network, &mut prefix)
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
            if let Some(interactive_output) = node_data.interactive_output()
                && let NodeOutput::Atomic(atomic_structure, _) = interactive_output
                && atomic_structure.decorator().from_selected_node
            {
                return Some(atomic_structure);
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
        // Leaving an atom_edit node drops its transient Guideline tool state (#368).
        if previous_selection != current_selection
            && let Some(prev_id) = previous_selection
        {
            crate::structure_designer::nodes::atom_edit::atom_edit::clear_guideline_tool_on_node_deselect(
                self, prev_id,
            );
        }
    }

    // --- Undo/Redo ---

    /// Undo the last command. Returns true if an undo was performed.
    pub fn undo(&mut self) -> bool {
        // Temporarily take the undo stack to avoid borrow conflict
        let mut stack = std::mem::take(&mut self.undo_stack);
        let result = stack.undo(&mut UndoContext {
            node_type_registry: &mut self.node_type_registry,
            active_network_name: &mut self.active_node_network_name,
            active_record_def_name: &mut self.active_record_def_name,
        });
        self.undo_stack = stack;

        if let Some(refresh_mode) = result {
            self.apply_undo_refresh_mode(refresh_mode);
            // The picked atom of the Guideline tool may have moved/vanished;
            // auto-unpick to avoid a stale constrained-drag state (#368).
            crate::structure_designer::nodes::atom_edit::atom_edit::auto_unpick_active_atom_edit_guideline(self);
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
            active_record_def_name: &mut self.active_record_def_name,
        });
        self.undo_stack = stack;

        if let Some(refresh_mode) = result {
            self.apply_undo_refresh_mode(refresh_mode);
            // The picked atom of the Guideline tool may have moved/vanished;
            // auto-unpick to avoid a stale constrained-drag state (#368).
            crate::structure_designer::nodes::atom_edit::atom_edit::auto_unpick_active_atom_edit_guideline(self);
            true
        } else {
            false
        }
    }

    /// Push a new undo command onto the stack.
    pub fn push_command(&mut self, command: impl UndoCommand + 'static) {
        self.undo_stack.push(Box::new(command));
    }

    /// Set an HOF node's collapse mode, capturing the before-state and pushing
    /// an undoable command. Guarded so only collapsable HOFs honor the mode;
    /// a no-op change pushes nothing. `scope_path` resolves the (possibly
    /// nested) body the HOF lives in. See `doc/design_hof_node_collapse.md`.
    pub fn set_collapse_mode(&mut self, scope_path: &[u64], hof_node_id: u64, mode: CollapseMode) {
        let network_name = match &self.active_node_network_name {
            Some(n) => n.clone(),
            None => return,
        };
        // Resolve the (possibly nested) body and read the old value, guarding so
        // only collapsable HOFs honor the mode. Read-only here: the pre-edit
        // footprints must be captured before any mutation (see below).
        let old = {
            let Some(network) = self.get_scope_network(scope_path) else {
                return;
            };
            let Some(node) = network.nodes.get(&hof_node_id) else {
                return;
            };
            if !collapsable_type_name(&node.node_type_name) {
                return;
            }
            node.collapse_mode
        };
        if old == mode {
            return; // no-op; don't push an empty command
        }

        // Capture the pre-edit footprint chain *before* flipping the mode: an
        // Expand grows the HOF's rendered footprint, and if the HOF is itself
        // nested in a body that growth cascades up. Reflow re-estimates the
        // *after* sizes, so the *before* sizes must be snapshotted now.
        let old_sizes = self.capture_footprint_chain(scope_path, hof_node_id);

        // Apply the mode change.
        {
            let Some(network) = self.get_scope_network_mut(scope_path) else {
                return;
            };
            let Some(node) = network.nodes.get_mut(&hof_node_id) else {
                return;
            };
            node.collapse_mode = mode;
        }

        // Push neighbours out of the way of the grown (possibly cascading)
        // footprint. A Collapse/shrink produces no moves (reflow sees delta 0).
        let scoped_moves = self.reflow_for_footprint_change(scope_path, hof_node_id, &old_sizes);

        let collapse_cmd = super::undo::commands::set_collapse_mode::SetCollapseModeCommand {
            network_name: network_name.clone(),
            scope_path: scope_path.to_vec(),
            node_id: hof_node_id,
            old_mode: old,
            new_mode: mode,
            description: "Set HOF collapse mode".to_string(),
        };

        if scoped_moves.is_empty() {
            // No neighbour moved — push the bare command (never a 1-child
            // composite, per the reflow design).
            self.push_command(collapse_cmd);
        } else {
            // Bundle the collapse with one MoveNodesCommand per reflowed scope so
            // the mode flip and the neighbour shifts undo/redo as a single step.
            let mut commands: Vec<Box<dyn UndoCommand>> =
                Vec::with_capacity(1 + scoped_moves.len());
            commands.push(Box::new(collapse_cmd));
            for sm in scoped_moves {
                commands.push(Box::new(
                    super::undo::commands::move_nodes::MoveNodesCommand {
                        network_name: network_name.clone(),
                        scope_path: sm.scope_path,
                        moves: sm.moves,
                        description: "Reflow neighbours".to_string(),
                    },
                ));
            }
            self.push_command(super::undo::commands::composite::CompositeCommand {
                commands,
                description: "Set HOF collapse mode".to_string(),
            });
        }
    }

    /// Apply the appropriate refresh after an undo/redo operation.
    fn apply_undo_refresh_mode(&mut self, mode: UndoRefreshMode) {
        match mode {
            UndoRefreshMode::Lightweight => {
                self.mark_lightweight_refresh();
            }
            UndoRefreshMode::NodeDataChanged(node_ids) => {
                for &node_id in &node_ids {
                    self.mark_node_data_changed(node_id);
                }
                // If any affected node's `-1` pin is consumed as a function
                // value, the undone/redone wire edit changed its exposed arity
                // (a capture added or removed), which must re-derive the
                // consumer's type. The forward connect/delete paths validate on
                // exactly this condition; mirror it here so undo/redo don't
                // reintroduce the staleness those triggers fixed
                // (`doc/design_node_function_pin_captures.md` §"Revalidation
                // triggers"). `NodeDataChanged` otherwise skips validation, so
                // without this an undone capture-wire edit would leave the
                // consumer's derived type stale.
                let needs_validate = self
                    .get_active_node_network()
                    .map(|net| node_ids.iter().any(|&id| net.function_pin_consumed(id)))
                    .unwrap_or(false);
                if needs_validate {
                    self.validate_active_network();
                }
            }
            UndoRefreshMode::Full => {
                self.mark_full_refresh();
                // Rebuild the per-node `custom_node_type` cache for the active
                // network. It is `#[serde(skip)]`, so a snapshot-restoring
                // command (`PromoteToParameterCommand`, …) loses it, and the
                // in-place re-add commands (`AddNodeCommand::redo`,
                // `DeleteNodesCommand::undo`) re-create nodes via
                // `add_node_with_id` without it — unlike the live `add_node`
                // path. Left unpopulated, a derived-layout node (parameter /
                // expr / HOF / closure / …) is observed in the stale `None`
                // cache state (B): the next `refresh_args = true` repair pass
                // mis-types it and drops its wires (the same class as the
                // rename wire-loss bug). Repopulating here — alongside the
                // existing display-policy / output_type derived-state rebuild —
                // keeps the invariant. Uses `refresh_args = false` (preserves
                // wires positionally) and recurses into HOF bodies. See
                // `doc/design_custom_node_type_cache_invariant.md`.
                self.repopulate_active_network_custom_node_types();
                // Reapply display policy so the display state matches what
                // the original mutation methods would have produced.
                self.apply_node_display_policy(None);
                // Re-validate network (updates derived state like output_type)
                self.validate_active_network();
            }
        }
        self.set_dirty(true);
    }

    /// Repopulate every node's `custom_node_type` cache in the active network
    /// (recursing into HOF bodies) from the current per-node data + registry.
    /// Used by the undo/redo `Full` refresh path: restored / re-added nodes
    /// arrive with the `#[serde(skip)]` cache cleared, and that derived state
    /// must be rebuilt before validation/evaluation observes it. `refresh_args
    /// = false` (via `initialize_custom_node_types_for_network`) preserves the
    /// positional `arguments` so no wire is lost. No-op when there is no active
    /// network. See `doc/design_custom_node_type_cache_invariant.md`.
    fn repopulate_active_network_custom_node_types(&mut self) {
        let Some(name) = self.active_node_network_name.clone() else {
            return;
        };
        if let Some(mut network) = self.node_type_registry.node_networks.remove(&name) {
            self.node_type_registry
                .initialize_custom_node_types_for_network(&mut network);
            self.node_type_registry.node_networks.insert(name, network);
        }
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

    /// Snapshot an HOF body for body-scoped undo: the body network at
    /// `scope_path` plus the owning HOF's `zone_output_arguments`. `scope_path`
    /// must be non-empty (`[parent.., hof_id]`); returns `None` for the empty
    /// path or any broken step of the walk. Pairs with
    /// [`push_zone_body_command`]. See `doc/design_zones_ui.md` §"Undo/redo".
    pub fn snapshot_zone_body(
        &mut self,
        scope_path: &[u64],
    ) -> Option<super::undo::commands::edit_zone_body::ZoneBodySnapshot> {
        use super::serialization::node_networks_serialization::node_network_to_serializable;
        use super::undo::commands::edit_zone_body::ZoneBodySnapshot;

        let (hof_id, parent_path) = scope_path.split_last()?;
        let network_name = self.active_node_network_name.as_ref()?.clone();

        let (built_in_types, node_networks) = (
            &self.node_type_registry.built_in_node_types,
            &mut self.node_type_registry.node_networks,
        );

        // Walk down to the HOF node that owns the body.
        let mut current = node_networks.get_mut(&network_name)?;
        for id in parent_path {
            current = current.nodes.get_mut(id)?.zone_mut()?;
        }
        let hof = current.nodes.get_mut(hof_id)?;

        // Body-return wires live on the HOF's zone_output_arguments.
        let zone_output_wires = hof
            .zone_output_arguments
            .iter()
            .map(|arg| arg.incoming_wires.clone())
            .collect();

        // Body-internal state lives in the HOF's owned zone network.
        let body = hof.zone_mut()?;
        let serialized = node_network_to_serializable(body, built_in_types, None).ok()?;

        Some(ZoneBodySnapshot {
            body: serialized,
            zone_output_wires,
        })
    }

    /// Finalize a body-scoped structural edit by snapshotting the body's
    /// after-state and pushing an [`EditZoneBodyCommand`] iff it differs from
    /// `before`. `before` is the snapshot captured before the mutation (a `None`
    /// before, e.g. a broken scope path, suppresses the command).
    pub fn push_zone_body_command(
        &mut self,
        scope_path: &[u64],
        description: String,
        before: Option<super::undo::commands::edit_zone_body::ZoneBodySnapshot>,
    ) {
        if let Some(command) = self.build_zone_body_command(scope_path, description, before) {
            self.push_command(command);
        }
    }

    /// Build (but do not push) the [`EditZoneBodyCommand`] for a body-scoped
    /// structural edit: snapshots the body's after-state and returns the command
    /// iff it differs from `before`. Returns `None` when `before` is `None`
    /// (e.g. a broken scope path), the edit was a no-op, or there is no active
    /// network. Most callers want [`push_zone_body_command`]; callers that must
    /// **bundle** the body edit with sibling commands (the Case A / C reflow
    /// ancestor `MoveNodesCommand`s in a `CompositeCommand`) use this directly so
    /// the whole thing undoes/redoes as one step. See
    /// `doc/design_reflow_on_footprint_change.md`.
    pub fn build_zone_body_command(
        &mut self,
        scope_path: &[u64],
        description: String,
        before: Option<super::undo::commands::edit_zone_body::ZoneBodySnapshot>,
    ) -> Option<super::undo::commands::edit_zone_body::EditZoneBodyCommand> {
        use super::undo::commands::edit_zone_body::EditZoneBodyCommand;

        let before = before?;
        let after = self.snapshot_zone_body(scope_path)?;
        if !EditZoneBodyCommand::is_meaningful(&before, &after) {
            return None;
        }
        let network_name = self.active_node_network_name.clone()?;
        Some(EditZoneBodyCommand {
            network_name,
            scope_path: scope_path.to_vec(),
            before,
            after,
            description,
        })
    }

    /// Capture the pre-edit footprint chain of the HOF that *owns* the body at
    /// `scope_path` (Case C of `doc/design_reflow_on_footprint_change.md`). The
    /// owning HOF is `scope_path.last()`, viewed from its parent network
    /// `scope_path[..len-1]`; the returned chain is exactly the `old_sizes`
    /// slice [`reflow_for_footprint_change`] expects when started at the parent
    /// scope for that HOF (index 0 = the owning HOF's footprint in its parent,
    /// index `k` ≥ 1 = the `k`-th further ancestor HOF's footprint).
    ///
    /// MUST be called **before** the body edit — once the edit has grown the
    /// body the owning HOF's *before* footprint can no longer be re-derived.
    /// Returns empty for an empty `scope_path` (top-level edits never cascade).
    /// Pairs with [`push_zone_body_command_with_ancestor_reflow`].
    pub fn capture_body_owner_footprint_chain(&self, scope_path: &[u64]) -> Vec<DVec2> {
        match scope_path.split_last() {
            Some((hof_id, parent)) => self.capture_footprint_chain(parent, *hof_id),
            None => Vec::new(),
        }
    }

    /// Finalize a body-scoped structural edit that may have grown the body's
    /// footprint, cascading reflow into **ancestor** scopes — Case C of
    /// `doc/design_reflow_on_footprint_change.md`. Adding / pasting /
    /// duplicating a node (or wiring one) inside a body does not grow an
    /// existing in-body node *in place*, so reflow produces **no moves inside
    /// the edited body itself**. What grows is the body's owning HOF, whose
    /// rendered footprint expands in the **parent** network — and that growth
    /// can cascade up several scope levels. Every reflowed move therefore lands
    /// in a scope the `EditZoneBodyCommand`'s body snapshot does **not** cover,
    /// so each is bundled into the same undo step.
    ///
    /// `before` is the pre-edit body snapshot (a `None` before suppresses the
    /// command, as in [`push_zone_body_command`]); `old_ancestor_sizes` is the
    /// chain captured by [`capture_body_owner_footprint_chain`] **before** the
    /// edit. Pushes either the bare `EditZoneBodyCommand` (no ancestor grew —
    /// `delta == 0`) or a [`super::undo::commands::composite::CompositeCommand`]
    /// bundling it with one `MoveNodesCommand` per reflowed ancestor scope.
    pub fn push_zone_body_command_with_ancestor_reflow(
        &mut self,
        scope_path: &[u64],
        description: String,
        before: Option<super::undo::commands::edit_zone_body::ZoneBodySnapshot>,
        old_ancestor_sizes: &[DVec2],
    ) {
        // The owning HOF lives one scope up; an empty path cannot cascade, so
        // fall back to the plain push (no ancestor to reflow around).
        let Some((hof_id, parent)) = scope_path.split_last() else {
            self.push_zone_body_command(scope_path, description, before);
            return;
        };
        let parent = parent.to_vec();

        // Reflow the ancestor scopes only. The moves inside the edited body
        // itself (if any) ride the fresh after-snapshot `build_zone_body_command`
        // takes at push time, so reflow deliberately starts one scope up — every
        // returned `ScopedMoves` lands in `parent` or higher, never `scope_path`.
        let scoped_moves = self.reflow_for_footprint_change(&parent, *hof_id, old_ancestor_sizes);

        let Some(edit_cmd) = self.build_zone_body_command(scope_path, description, before) else {
            return;
        };

        if scoped_moves.is_empty() {
            // Body absorbed the growth (or nothing grew) — push the bare body
            // command (never a 1-child composite, per the reflow design).
            self.push_command(edit_cmd);
            return;
        }

        let Some(network_name) = self.active_node_network_name.clone() else {
            self.push_command(edit_cmd);
            return;
        };

        let composite_description = edit_cmd.description().to_string();
        let mut commands: Vec<Box<dyn UndoCommand>> = vec![Box::new(edit_cmd)];
        for sm in scoped_moves {
            commands.push(Box::new(
                super::undo::commands::move_nodes::MoveNodesCommand {
                    network_name: network_name.clone(),
                    scope_path: sm.scope_path,
                    moves: sm.moves,
                    description: "Reflow neighbours".to_string(),
                },
            ));
        }
        self.push_command(super::undo::commands::composite::CompositeCommand {
            commands,
            description: composite_description,
        });
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
            if node_ref.is_top_level() && network.displayed_nodes.contains_key(&node_ref.node_id) {
                nodes_needing_evaluation.insert(node_ref.node_id);
            }
        }

        // Step 4.5: Handle selection changes - re-evaluate affected nodes to update from_selected_node flag
        if changes.selection_changed {
            // Add previous selected node (needs from_selected_node set to false)
            if let Some(prev_node_id) = changes.previous_selection
                && network.displayed_nodes.contains_key(&prev_node_id)
            {
                nodes_needing_evaluation.insert(prev_node_id);
            }
            // Add current selected node (needs from_selected_node set to true)
            if let Some(curr_node_id) = changes.current_selection
                && network.displayed_nodes.contains_key(&curr_node_id)
            {
                nodes_needing_evaluation.insert(curr_node_id);
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

    /// Add a node network with an auto-generated unique name.
    /// Returns the generated name so the caller can activate the new network
    /// (the registry is a HashMap, so callers cannot reliably recover the new
    /// name by inspecting list order — see issue #315).
    pub fn add_new_node_network(&mut self) -> String {
        self.add_new_node_network_in_namespace("")
    }

    /// Like [`add_new_node_network`] but places the new network under the given
    /// `namespace` (a dot-delimited prefix, e.g. `"Physics.Mechanics"`). An
    /// empty namespace creates the network at the root. The simple name is
    /// auto-generated to be unique across the whole user-type namespace.
    pub fn add_new_node_network_in_namespace(&mut self, namespace: &str) -> String {
        // Generate a unique name. Skip any name already taken anywhere in the
        // user-type namespace (networks, user record defs, built-in record
        // defs, built-in node types) so the auto-generated name is never a
        // collision.
        let qualify = |simple: &str| -> String {
            if namespace.is_empty() {
                simple.to_string()
            } else {
                format!("{}.{}", namespace, simple)
            }
        };
        let mut name = qualify("UNTITLED");
        let mut i = 1;
        while self.node_type_registry.name_is_taken(&name) {
            name = qualify(&format!("UNTITLED{}", i));
            i += 1;
        }

        // Capture previous active network for undo
        let previous_active_network = self.active_node_network_name.clone();

        // Capture ancestor empty-folder markers this network will absorb (so
        // undo restores them). See `doc/design_empty_folders.md`.
        let pruned_folders = self.node_type_registry.ancestor_folders_present(&name);

        self.add_node_network(&name);
        // Mark design as dirty since we added a new network
        self.set_dirty(true);
        // Adding a network is a structural change requiring full refresh
        self.mark_full_refresh();

        // Push undo command
        self.push_command(super::undo::commands::add_network::AddNetworkCommand {
            network_name: name.clone(),
            previous_active_network,
            pruned_folders,
        });

        name
    }

    /// Add a named node network and push an undo command.
    /// Used by the API layer for user-initiated "add network with name" actions.
    pub fn add_node_network_with_undo(
        &mut self,
        node_network_name: &str,
    ) -> Result<(), super::identifier::InvalidNameReason> {
        super::identifier::is_valid_user_name(node_network_name)?;
        let previous_active_network = self.active_node_network_name.clone();
        let pruned_folders = self
            .node_type_registry
            .ancestor_folders_present(node_network_name);
        self.add_node_network(node_network_name);
        self.set_dirty(true);
        self.mark_full_refresh();
        self.push_command(super::undo::commands::add_network::AddNetworkCommand {
            network_name: node_network_name.to_string(),
            previous_active_network,
            pruned_folders,
        });
        Ok(())
    }

    /// Create an empty folder marker at `path` (dot-delimited). Validates the
    /// name and rejects collisions with any existing user type or folder.
    /// Pushes an undo command on success. See `doc/design_empty_folders.md`.
    pub fn add_folder(&mut self, path: &str) -> Result<(), String> {
        super::identifier::is_valid_user_name(path)
            .map_err(|e| format!("Invalid folder name: {}", e))?;
        // Capture absorbed ancestor markers for undo before the add prunes them.
        let pruned_ancestors = self.node_type_registry.ancestor_folders_present(path);
        self.node_type_registry.add_folder(path)?;
        self.set_dirty(true);
        self.mark_full_refresh();
        self.push_command(super::undo::commands::add_folder::AddFolderCommand {
            path: path.to_string(),
            pruned_ancestors,
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

        // Clipboard node_type_names (not available in UndoContext). Walk into
        // HOF/closure zone bodies too — a copied body's instance of the renamed
        // network must be updated or it dangles on paste (mirrors
        // `apply_rename_core`, which recurses for the same reason).
        if let Some(ref mut clipboard) = self.clipboard {
            crate::structure_designer::node_network::walk_all_nodes_mut(clipboard, &mut |node| {
                if node.node_type_name == old_name {
                    node.node_type_name = new_name.to_string();
                }
            });
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

    /// Compute the plan for shifting every network under `old_prefix` to
    /// `new_prefix`, without mutating anything. An empty `new_prefix` promotes
    /// the contents to the top level (root). This is the single source of
    /// truth for both the live preview and the actual rename, so the dialog's
    /// conflict/validity feedback matches exactly what `rename_namespace` will
    /// accept.
    pub fn compute_namespace_rename(
        &self,
        old_prefix: &str,
        new_prefix: &str,
    ) -> NamespaceRenamePlan {
        // Affected user types: names strictly under "old_prefix." (a leaf
        // named exactly `old_prefix` is not part of the namespace). Sweeps
        // BOTH networks and user record defs — records are first-class members
        // of the same hierarchy (doc/design_hierarchical_records.md). Built-in
        // record defs live in a separate map and are never affected.
        let prefix_dot = format!("{}.", old_prefix);
        let mut affected: Vec<String> = self
            .node_type_registry
            .node_networks
            .keys()
            .chain(self.node_type_registry.record_type_defs.keys())
            .filter(|name| name.starts_with(&prefix_dot))
            .cloned()
            .collect();
        affected.sort();

        // Names vacated by this rename don't count as collisions — they're
        // moving out of the way as part of the same atomic operation.
        let affected_set: HashSet<&str> = affected.iter().map(|s| s.as_str()).collect();

        let mut items = Vec::with_capacity(affected.len());
        let mut valid_names = true;
        for old_name in &affected {
            let suffix = &old_name[prefix_dot.len()..];
            // Empty target prefix => promote to root (no leading dot).
            let new_name = if new_prefix.is_empty() {
                suffix.to_string()
            } else {
                format!("{}.{}", new_prefix, suffix)
            };

            if super::identifier::is_valid_user_name(&new_name).is_err() {
                valid_names = false;
            }

            // A collision is any existing user type (network, user/built-in
            // record def, built-in node type) sharing the target name, except
            // a network that is itself being renamed away.
            let conflict = !affected_set.contains(new_name.as_str())
                && self.node_type_registry.name_is_taken(&new_name);

            // Every affected name resolves to a user network or user record def
            // (it came from one of the two swept maps); default to Network if a
            // concurrent edit somehow removed it.
            let kind = self
                .node_type_registry
                .user_type_kind(old_name)
                .unwrap_or(UserTypeKind::Network);

            items.push(NamespaceRenameItem {
                old_name: old_name.clone(),
                new_name,
                conflict,
                kind,
            });
        }

        // Empty-folder markers affected by this move: the folder itself (marker
        // named exactly `old_prefix`) plus any empty subfolders under it. These
        // are NOT caught by the `prefix_dot` entity sweep above (a marker named
        // exactly `old_prefix` has no trailing dot). See
        // `doc/design_empty_folders.md`.
        let mut affected_markers: Vec<String> = self
            .node_type_registry
            .folders
            .iter()
            .filter(|m| m.as_str() == old_prefix || m.starts_with(&prefix_dot))
            .cloned()
            .collect();
        affected_markers.sort();

        // Names vacated by this move (entity old-names + marker old-names) don't
        // count as collisions for a folder target.
        let mut vacating: HashSet<&str> = affected_set.clone();
        for m in &affected_markers {
            vacating.insert(m.as_str());
        }

        let mut folder_changes = Vec::with_capacity(affected_markers.len());
        let mut folder_conflict = false;
        for marker in &affected_markers {
            let new_marker: Option<String> = if marker.as_str() == old_prefix {
                // The folder itself: promote-to-root (empty target) makes an
                // empty folder vanish — there is no empty-named root folder.
                if new_prefix.is_empty() {
                    None
                } else {
                    Some(new_prefix.to_string())
                }
            } else {
                let suffix = &marker[prefix_dot.len()..];
                Some(if new_prefix.is_empty() {
                    suffix.to_string()
                } else {
                    format!("{}.{}", new_prefix, suffix)
                })
            };

            if let Some(nm) = &new_marker {
                if super::identifier::is_valid_user_name(nm).is_err() {
                    valid_names = false;
                }
                if !vacating.contains(nm.as_str()) && self.node_type_registry.name_is_taken(nm) {
                    folder_conflict = true;
                }
            }
            folder_changes.push((marker.clone(), new_marker));
        }

        NamespaceRenamePlan {
            items,
            valid_names,
            folder_changes,
            folder_conflict,
        }
    }

    /// Compute the plan for moving/renaming a single *leaf* `old_name` (a
    /// custom network OR a user record def) to the fully-qualified `new_name`,
    /// without mutating anything. Reuses the same `NamespaceRenamePlan` shape
    /// as `compute_namespace_rename` so the move dialog renders both with one
    /// code path (a leaf plan always has exactly one item). The kind is
    /// detected via `user_type_kind`; an unknown name or a built-in produces an
    /// empty (non-applicable) plan. A no-op (`new_name == old_name`) is reported
    /// as applicable-with-no-conflict; the dialog disables Apply for it.
    pub fn compute_leaf_rename(&self, old_name: &str, new_name: &str) -> NamespaceRenamePlan {
        // Unknown source / built-in => nothing to rename (empty plan).
        let Some(kind) = self.node_type_registry.user_type_kind(old_name) else {
            return NamespaceRenamePlan {
                items: Vec::new(),
                valid_names: true,
                folder_changes: Vec::new(),
                folder_conflict: false,
            };
        };

        let valid_names = super::identifier::is_valid_user_name(new_name).is_ok();
        // The source name itself doesn't count as a collision (prefilled value).
        let conflict = new_name != old_name && self.node_type_registry.name_is_taken(new_name);

        NamespaceRenamePlan {
            items: vec![NamespaceRenameItem {
                old_name: old_name.to_string(),
                new_name: new_name.to_string(),
                conflict,
                kind,
            }],
            valid_names,
            folder_changes: Vec::new(),
            folder_conflict: false,
        }
    }

    /// Backward-compatible network-leaf preview. Delegates to the kind-aware
    /// [`compute_leaf_rename`]; for a network name it behaves exactly as before.
    pub fn compute_network_rename(&self, old_name: &str, new_name: &str) -> NamespaceRenamePlan {
        self.compute_leaf_rename(old_name, new_name)
    }

    pub fn rename_namespace(&mut self, old_prefix: &str, new_prefix: &str) -> bool {
        let plan = self.compute_namespace_rename(old_prefix, new_prefix);
        if !plan.is_applicable() {
            return false;
        }

        use super::undo::commands::rename_namespace::NamespaceRename;
        // Empty-folder markers moved by this rename (applied below alongside the
        // entity renames). See `doc/design_empty_folders.md`.
        let folder_changes = plan.folder_changes.clone();
        let renames: Vec<NamespaceRename> = plan
            .items
            .into_iter()
            .map(|item| NamespaceRename {
                old_name: item.old_name,
                new_name: item.new_name,
                kind: item.kind,
            })
            .collect();

        // Remap empty-folder markers (old → new, or remove when promoted to root).
        for (old, new) in &folder_changes {
            self.node_type_registry.folders.remove(old);
            if let Some(n) = new {
                self.node_type_registry.folders.insert(n.clone());
            }
        }

        // Perform all renames, dispatching per kind. Networks go through
        // `apply_rename_core` (registry move + node_type_name + active-network
        // remap); records through the infallible `rename_record_type_def_unchecked`
        // (Helper 1 — registry move + `Named` rewrite). The plan's conflict
        // check already gated the whole batch as applicable.
        let mut touched_record = false;
        for r in &renames {
            match r.kind {
                UserTypeKind::Network => {
                    super::undo::commands::rename_helpers::apply_rename_core(
                        &mut self.node_type_registry,
                        &mut self.active_node_network_name,
                        &r.old_name,
                        &r.new_name,
                    );
                }
                UserTypeKind::Record => {
                    self.node_type_registry
                        .rename_record_type_def_unchecked(&r.old_name, &r.new_name);
                    // Backend-owned active record def follows the move.
                    if self.active_record_def_name.as_deref() == Some(r.old_name.as_str()) {
                        self.active_record_def_name = Some(r.new_name.clone());
                    }
                    touched_record = true;
                }
            }
        }

        // Update navigation history for network renames (records are not navigated).
        for r in &renames {
            if r.kind == UserTypeKind::Network {
                self.navigation_history
                    .rename_network(&r.old_name, &r.new_name);
            }
        }

        // Update clipboard node_type_name refs for network renames. Walk into
        // HOF/closure zone bodies too so a copied body's instance of a renamed
        // network is updated and doesn't dangle on paste (same body-skip class
        // as the single rename). Clipboard record refs are out of scope (matches
        // standalone `rename_record_type_def`).
        if let Some(ref mut clipboard) = self.clipboard {
            crate::structure_designer::node_network::walk_all_nodes_mut(clipboard, &mut |node| {
                for r in &renames {
                    if r.kind == UserTypeKind::Network && node.node_type_name == r.old_name {
                        node.node_type_name = r.new_name.clone();
                        break;
                    }
                }
            });
        }

        // Helper 2: refresh record-node pin layouts if any record moved (the
        // `Named` rewrite cleared their `custom_node_type`).
        if touched_record {
            self.node_type_registry.repair_all_networks();
        }

        self.set_dirty(true);
        self.mark_full_refresh();

        self.push_command(
            super::undo::commands::rename_namespace::RenameNamespaceCommand {
                renames,
                folder_changes,
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
        if let Some(active_name) = &self.active_node_network_name
            && active_name == network_name
        {
            self.active_node_network_name = None;
        }

        // Remove the deleted network from navigation history
        self.navigation_history.remove_network(network_name);

        // Clear clipboard if it references the deleted network type
        if let Some(ref clipboard) = self.clipboard
            && clipboard
                .nodes
                .values()
                .any(|n| n.node_type_name == network_name)
        {
            self.clipboard = None;
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

    /// Duplicate a named node network under a fresh unique name and return that
    /// name. The new network becomes the active network. Pushes an undo command.
    ///
    /// This is a **shallow** duplicate: the network's own content is copied,
    /// including every inline HOF / closure zone body (recursively — bodies are
    /// owned `Node.zone` networks that travel with the snapshot). References to
    /// *other* named custom networks are kept as references (by `node_type_name`)
    /// and are **not** themselves duplicated. Both behaviors fall out of the
    /// serialize → deserialize round-trip with no special handling.
    pub fn duplicate_node_network(&mut self, source_name: &str) -> Result<String, String> {
        use super::serialization::node_networks_serialization::serializable_to_node_network;

        if !self
            .node_type_registry
            .node_networks
            .contains_key(source_name)
        {
            return Err(format!("Node network '{}' does not exist", source_name));
        }

        // Snapshot the source network.
        let snapshot = self
            .snapshot_network(source_name)
            .ok_or_else(|| format!("Failed to snapshot network '{}'", source_name))?;

        // Pick a unique name for the copy (kept in the source's namespace).
        let new_name = self.generate_unique_copy_name(source_name);

        // Deserialize a fresh copy and give it the new name (the registry keys
        // on `node_type.name`, so the internal name must match the key).
        let mut network = serializable_to_node_network(
            &snapshot,
            &self.node_type_registry.built_in_node_types,
            None,
        )
        .map_err(|e| format!("Failed to duplicate network: {}", e))?;
        network.node_type.name = new_name.clone();

        // Repopulate per-node custom-type caches (incl. nodes inside zone
        // bodies), mirroring `FactorSelectionCommand::restore_network`.
        self.node_type_registry
            .initialize_custom_node_types_for_network(&mut network);

        let previous_active_network = self.active_node_network_name.clone();
        let pruned_folders = self.node_type_registry.ancestor_folders_present(&new_name);

        self.node_type_registry.add_node_network(network);

        // Snapshot the freshly added (renamed) network for the undo command, so
        // redo restores it under the correct name.
        let copy_snapshot = self
            .snapshot_network(&new_name)
            .ok_or_else(|| format!("Failed to snapshot duplicated network '{}'", new_name))?;

        // The API layer activates the new copy (so it can apply camera
        // settings); the undo command captures the active switch on redo/undo.

        self.set_dirty(true);
        self.mark_full_refresh();

        self.push_command(
            super::undo::commands::duplicate_network::DuplicateNetworkCommand {
                network_name: new_name.clone(),
                network_snapshot: copy_snapshot,
                previous_active_network,
                pruned_folders,
            },
        );

        Ok(new_name)
    }

    /// Generate a unique name for a duplicate of `source_name`: `<source>_copy`,
    /// then `<source>_copy_2`, `<source>_copy_3`, … Collisions are checked
    /// against the whole user-type namespace via `name_is_taken`. Because the
    /// suffix is appended to the full dotted name, the copy stays in the source's
    /// folder/namespace.
    fn generate_unique_copy_name(&self, source_name: &str) -> String {
        let first = format!("{}_copy", source_name);
        if !self.node_type_registry.name_is_taken(&first) {
            return first;
        }
        let mut counter = 2;
        loop {
            let name = format!("{}_copy_{}", source_name, counter);
            if !self.node_type_registry.name_is_taken(&name) {
                return name;
            }
            counter += 1;
        }
    }

    /// Read-only check that no entity *outside* the deleted set references any
    /// record in `target_records` via `RecordType::Named`. A surviving entity is
    /// a network whose name is not in `deleted_networks`, or a user record def
    /// whose name is not in `target_records`. Walks each surviving network's
    /// signature + all nodes (incl. zone bodies) and each surviving record
    /// def's field types. Returns the (sorted) list of external references that
    /// would be left dangling — empty means the delete is safe. Callers format
    /// their own message (single-def vs namespace). Shared by `delete_namespace`
    /// (batch) and `delete_record_type_def` (single).
    /// See `doc/design_hierarchical_records.md`.
    fn record_delete_blockers(
        &self,
        target_records: &std::collections::HashSet<&str>,
        deleted_networks: &std::collections::HashSet<&str>,
    ) -> Vec<String> {
        let mut blockers: Vec<String> = Vec::new();

        // Surviving networks referencing a deleted record.
        for (name, network) in self.node_type_registry.node_networks.iter() {
            if deleted_networks.contains(name.as_str()) {
                continue;
            }
            let mut refs: HashSet<String> = HashSet::new();
            super::node_type_registry::collect_record_refs_in_network(network, &mut refs);
            for r in &refs {
                if target_records.contains(r.as_str()) {
                    blockers.push(format!("network '{}' references record '{}'", name, r));
                }
            }
        }

        // Surviving user record defs whose fields reference a deleted record.
        for (name, def) in self.node_type_registry.record_type_defs.iter() {
            if target_records.contains(name.as_str()) {
                continue;
            }
            let mut refs: HashSet<String> = HashSet::new();
            for field in &def.fields {
                super::node_type_registry::collect_record_refs_in_type(&field.data_type, &mut refs);
            }
            for r in &refs {
                if target_records.contains(r.as_str()) {
                    blockers.push(format!("record '{}' references record '{}'", name, r));
                }
            }
        }

        blockers.sort();
        blockers
    }

    pub fn delete_namespace(&mut self, prefix: &str) -> Result<(), String> {
        // Collect affected networks AND user record defs: names under "prefix."
        let prefix_dot = format!("{}.", prefix);
        let affected_networks: Vec<String> = self
            .node_type_registry
            .node_networks
            .keys()
            .filter(|name| name.starts_with(&prefix_dot))
            .cloned()
            .collect();
        let affected_records: Vec<String> = self
            .node_type_registry
            .record_type_defs
            .keys()
            .filter(|name| name.starts_with(&prefix_dot))
            .cloned()
            .collect();
        // Empty-folder markers under the prefix: the folder itself (named
        // exactly `prefix`) plus any empty subfolders. See
        // `doc/design_empty_folders.md`.
        let affected_folders: Vec<String> = self
            .node_type_registry
            .folders
            .iter()
            .filter(|m| m.as_str() == prefix || m.starts_with(&prefix_dot))
            .cloned()
            .collect();

        if affected_networks.is_empty()
            && affected_records.is_empty()
            && affected_folders.is_empty()
        {
            return Err(format!("No items found under namespace '{}'", prefix));
        }

        // Reference checks: block on references from outside the deleted set,
        // for both kinds (chosen policy — a batch delete never silently dangles).
        let network_targets: std::collections::HashSet<&str> =
            affected_networks.iter().map(|s| s.as_str()).collect();
        self.check_delete_references(&network_targets)?;

        let record_targets: std::collections::HashSet<&str> =
            affected_records.iter().map(|s| s.as_str()).collect();
        let record_blockers = self.record_delete_blockers(&record_targets, &network_targets);
        if !record_blockers.is_empty() {
            return Err(format!(
                "Cannot delete namespace because referenced from outside: {}",
                record_blockers.join(", ")
            ));
        }

        // Snapshot all affected networks (for undo).
        let mut network_snapshots = Vec::new();
        for name in &affected_networks {
            if let Some(snapshot) = self.snapshot_network(name) {
                network_snapshots.push((name.clone(), snapshot));
            }
        }

        // Snapshot all affected record defs (RecordTypeDef is Clone).
        let mut record_snapshots = Vec::new();
        for name in &affected_records {
            if let Some(def) = self.node_type_registry.record_type_defs.get(name) {
                record_snapshots.push((name.clone(), def.clone()));
            }
        }

        let active_network_before = self.active_node_network_name.clone();
        let active_record_def_before = self.active_record_def_name.clone();

        // Remove all affected networks and record defs.
        for name in &affected_networks {
            self.node_type_registry.node_networks.remove(name);
        }
        for name in &affected_records {
            self.node_type_registry.record_type_defs.remove(name);
        }
        // Remove affected empty-folder markers.
        for marker in &affected_folders {
            self.node_type_registry.folders.remove(marker);
        }

        // If any record was removed, repair every network so wires that now
        // resolve through a dangling `Named` ref are disconnected and
        // record-node pin layouts refresh (Helper 2 — matches forward delete).
        if !affected_records.is_empty() {
            self.node_type_registry.repair_all_networks();
        }

        // Update active network if it was under the prefix.
        if let Some(active_name) = &self.active_node_network_name
            && active_name.starts_with(&prefix_dot)
        {
            self.active_node_network_name = None;
        }
        // Update active record def if it was under the prefix.
        if let Some(active_rec) = &self.active_record_def_name
            && active_rec.starts_with(&prefix_dot)
        {
            self.active_record_def_name = None;
        }

        // Remove networks from navigation history.
        for name in &affected_networks {
            self.navigation_history.remove_network(name);
        }

        // Clear clipboard if it references any deleted network.
        if let Some(ref clipboard) = self.clipboard
            && clipboard
                .nodes
                .values()
                .any(|n| network_targets.contains(n.node_type_name.as_str()))
        {
            self.clipboard = None;
        }

        let active_network_after = self.active_node_network_name.clone();
        let active_record_def_after = self.active_record_def_name.clone();

        self.set_dirty(true);
        self.mark_full_refresh();

        self.push_command(
            super::undo::commands::delete_namespace::DeleteNamespaceCommand {
                network_snapshots,
                record_snapshots,
                active_network_before,
                active_network_after,
                active_record_def_before,
                active_record_def_after,
                folder_markers: affected_folders,
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
        // Capture ancestor empty-folder markers this def will absorb (undo
        // restores them). See `doc/design_empty_folders.md`.
        let pruned_folders = self.node_type_registry.ancestor_folders_present(&def.name);
        self.node_type_registry.add_record_type_def(def)?;
        self.set_dirty(true);
        self.mark_full_refresh();
        self.push_command(
            super::undo::commands::add_record_type_def::AddRecordTypeDefCommand {
                def: def_clone,
                pruned_folders,
            },
        );
        Ok(())
    }

    /// Adds a new empty record type def under the given `namespace` (a
    /// dot-delimited prefix, e.g. `"Physics"`). An empty namespace creates the
    /// def at the root. The simple name is auto-generated to be unique across
    /// the whole user-type namespace. Returns the generated qualified name.
    pub fn add_new_record_type_def_in_namespace(
        &mut self,
        namespace: &str,
    ) -> Result<String, super::node_type_registry::RecordTypeDefError> {
        let qualify = |simple: &str| -> String {
            if namespace.is_empty() {
                simple.to_string()
            } else {
                format!("{}.{}", namespace, simple)
            }
        };
        let mut name = qualify("UNTITLED");
        let mut i = 1;
        while self.node_type_registry.name_is_taken(&name) {
            name = qualify(&format!("UNTITLED{}", i));
            i += 1;
        }

        let def = super::node_type_registry::RecordTypeDef::new(name.clone());
        self.add_record_type_def(def)?;
        Ok(name)
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

        // Block deletion while any surviving network or record def still
        // references this record via `RecordType::Named` — consistent with
        // network deletion and `delete_namespace`. This is the single-def
        // counterpart of the batch check; without it, deleting a referenced
        // def would leave a dangling `Record(Named(name))` behind (the old
        // repair pass only disconnected wires, never fixed other record defs).
        let targets: std::collections::HashSet<&str> = std::iter::once(name).collect();
        let no_deleted_networks: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let blockers = self.record_delete_blockers(&targets, &no_deleted_networks);
        if !blockers.is_empty() {
            return Err(super::node_type_registry::RecordTypeDefError::Referenced(
                name.to_string(),
                blockers.join(", "),
            ));
        }

        // No surviving reference remains, so no wire can dangle and no other
        // def needs repair — remove the def outright (no delete-and-repair).
        let def = self
            .node_type_registry
            .delete_record_type_def(name)
            .expect("contains_key checked above");

        // Clear the backend-owned active record def if it was the deleted one
        // (capture first so undo can restore the schema-editor selection).
        let was_active = self.active_record_def_name.as_deref() == Some(name);
        if was_active {
            self.active_record_def_name = None;
        }

        self.set_dirty(true);
        self.mark_full_refresh();
        self.push_command(
            super::undo::commands::delete_record_type_def::DeleteRecordTypeDefCommand {
                def,
                was_active,
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

        self.node_type_registry.repair_all_networks();

        // Backend-owned active record def follows the rename.
        if self.active_record_def_name.as_deref() == Some(old_name) {
            self.active_record_def_name = Some(new_name.to_string());
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

    /// Replace the field list of an existing record type def from authored
    /// `(name, type)` pairs, with **no per-row identity information**. Field
    /// identity is inferred by name-match against the current def — a name that
    /// already exists keeps its `FieldId`, a new name becomes a new field — so a
    /// rename reads as delete+add and drops the field's wire (historical
    /// behaviour; see `record_types_phase3_test::field_rename_disconnects_old_pin_wires`).
    ///
    /// The identity-preserving entry point is
    /// [`update_record_type_def_with_ids`](Self::update_record_type_def_with_ids),
    /// used by the schema editor, which closes #377.
    pub fn update_record_type_def(
        &mut self,
        name: &str,
        new_fields: Vec<(String, DataType)>,
    ) -> Result<(), super::node_type_registry::RecordTypeDefError> {
        // Resolve per-row ids by name against the current def, then delegate to
        // the identity-aware core. A name present before keeps its id; a new name
        // sends `None`.
        let edits = match self.node_type_registry.record_type_defs.get(name) {
            Some(def) => {
                let ids: HashMap<&str, super::node_type_registry::FieldId> =
                    def.fields.iter().map(|f| (f.name.as_str(), f.id)).collect();
                new_fields
                    .into_iter()
                    .map(
                        |(field_name, data_type)| super::node_type_registry::RecordFieldEdit {
                            id: ids.get(field_name.as_str()).copied(),
                            name: field_name,
                            data_type,
                        },
                    )
                    .collect()
            }
            None => {
                return Err(super::node_type_registry::RecordTypeDefError::NotFound(
                    name.to_string(),
                ));
            }
        };
        self.update_record_type_def_with_ids(name, edits)
    }

    /// Replace the field list of an existing record type def from an
    /// identity-aware [`RecordFieldEdit`](super::node_type_registry::RecordFieldEdit)
    /// list (each row carries the editing identity of an existing field, or
    /// `None` for a new field). Surviving fields keep their `FieldId` across
    /// rename / reorder / retype, so input-pin wires on `record_construct` /
    /// `product` are preserved by id — at top level **and** inside every HOF
    /// body (`repair_node_network` recurses). Re-keys `record_construct` literal
    /// defaults for renamed fields, snapshots every network for undo, and
    /// re-runs repair afterward (just like delete). Closes #377
    /// (`doc/design_record_field_identity.md` R2).
    pub fn update_record_type_def_with_ids(
        &mut self,
        name: &str,
        edits: Vec<super::node_type_registry::RecordFieldEdit>,
    ) -> Result<(), super::node_type_registry::RecordTypeDefError> {
        // Capture the exact pre-update field list + allocator floor for a
        // faithful undo restore (ids round-trip verbatim — R2/R4).
        let (old_fields, old_next_field_id) =
            match self.node_type_registry.record_type_defs.get(name) {
                Some(def) => (def.fields.clone(), def.next_field_id),
                None => {
                    return Err(super::node_type_registry::RecordTypeDefError::NotFound(
                        name.to_string(),
                    ));
                }
            };

        // Snapshot every network before the update — wires whose source type
        // no longer satisfies a retyped field will be disconnected by the
        // repair pass below, and pre-rename literal keys must be restorable.
        let snapshots =
            super::undo::commands::delete_record_type_def::snapshot_all_networks_for_record_def_change(
                &mut self.node_type_registry,
            );

        let edits_clone = edits.clone();
        let renames = self
            .node_type_registry
            .update_record_type_def_with_edits(name, edits)?;

        // Re-key `record_construct` literal defaults for any surviving renamed
        // field (top-level AND inside HOF bodies).
        self.node_type_registry
            .rekey_record_construct_literals(name, &renames);

        self.node_type_registry.repair_all_networks();

        self.set_dirty(true);
        self.mark_full_refresh();
        self.push_command(
            super::undo::commands::update_record_type_def::UpdateRecordTypeDefCommand {
                name: name.to_string(),
                old_fields,
                old_next_field_id,
                new_edits: edits_clone,
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
        // Capture the body's before-state for undo (whole-body snapshot).
        let before = self.snapshot_zone_body(scope_path);
        // Case C reflow (doc/design_reflow_on_footprint_change.md): adding a node
        // grows the body, which grows the owning HOF in the parent network and
        // may cascade up. Capture the owning HOF's pre-edit footprint chain now,
        // before the add — once the body has grown it can't be re-derived.
        let old_ancestor_sizes = self.capture_body_owner_footprint_chain(scope_path);
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
                if let Some(network) = current
                    && let Some(node) = network.nodes.get_mut(&node_id)
                {
                    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
                        built_in_types,
                        record_type_defs,
                        built_in_record_type_defs,
                        node,
                        true,
                    );
                }
            }
            self.set_dirty(true);
            // A zone-bearing node added inside a body starts with an empty body
            // (its zone-output pin has no incoming wire), so re-validate to
            // surface the zone-rule error consistently — same reasoning as the
            // top-level `add_node_with_drag_source` path.
            if self
                .node_type_registry
                .get_node_type(node_type_name)
                .map(|nt| nt.has_zone())
                .unwrap_or(false)
            {
                self.validate_active_network();
            }
            // Opt-in nodes (the stateless unpack/destructure nodes) show all
            // their output pins on creation, not just pin 0. Applied before the
            // body-snapshot command push so undo/redo restore it faithfully.
            self.apply_default_all_pin_display(scope_path, node_type_name, node_id);
            self.push_zone_body_command_with_ancestor_reflow(
                scope_path,
                format!("Add {} node", node_type_name),
                before,
                &old_ancestor_sizes,
            );
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
        if let Some(drag) = drag_source.as_ref()
            && let Some(node_type) = self.node_type_registry.get_node_type(node_type_name)
            && let Some(adapted) = node_data.adapt_for_drag_source(
                &drag.source_type,
                drag.direction,
                &self.node_type_registry,
            )
        {
            let resolved = self
                .node_type_registry
                .resolve_drag_candidate_type(node_type, adapted.as_ref());
            if crate::structure_designer::node_type_registry::static_match_strict(
                &resolved,
                &drag.source_type,
                drag.direction,
                &self.node_type_registry,
            ) {
                node_data = adapted;
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
        if node_type_name == "parameter"
            && let Some(node_network) = self
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
            if let Some(network) = node_networks.get_mut(&node_network_name)
                && let Some(node) = network.nodes.get_mut(&node_id)
            {
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

            // Opt-in nodes (the stateless unpack/destructure nodes) show all
            // their output pins on creation, not just pin 0.
            self.apply_default_all_pin_display(&[], node_type_name, node_id);

            // Check if we need to validate the network
            let should_validate = node_type_name == "parameter"
                // A freshly added zone-bearing node (`closure`, `map`, `filter`,
                // `fold`, `foreach`) has an empty body, so its zone-output pin
                // has no incoming wire — an immediate zone-rule violation. Without
                // validating here the network stays `valid` and the only feedback
                // is the eval-time "body has no incoming wire on zone-output pin"
                // hover message; validating surfaces the canonical validation
                // error (and blanks the viewport) consistently with every other
                // path that reaches the same invalid state.
                || self
                    .node_type_registry
                    .get_node_type(node_type_name)
                    .map(|nt| nt.has_zone())
                    .unwrap_or(false)
                || {
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
    ///
    /// The selection lives in exactly one scope at a time (the single-scope
    /// selection invariant), so copy locates that scope itself via
    /// [`find_selection_scope`] rather than trusting a caller-supplied scope
    /// path. This makes Ctrl+C "just work" on a zone-body selection even when
    /// the active scope chain points elsewhere (e.g. the user clicked an empty
    /// body interior, which moves the active scope without moving the
    /// selection). The clipboard is a flat, scope-agnostic `NodeNetwork` that
    /// can subsequently be pasted into any scope.
    pub fn copy_selection(&mut self) -> bool {
        let scope = match self.find_selection_scope() {
            Some(scope) => scope,
            None => return false,
        };

        let source = match self.get_scope_network(&scope) {
            Some(network) => network,
            None => return false,
        };

        if source.selected_node_ids.is_empty() {
            return false;
        }

        // Compute centroid of selected nodes' positions
        let selected_ids = source.selected_node_ids.clone();
        let mut sum = DVec2::ZERO;
        let mut count = 0u64;
        for &id in &selected_ids {
            if let Some(node) = source.nodes.get(&id) {
                sum += node.position;
                count += 1;
            }
        }
        if count == 0 {
            return false;
        }
        let centroid = sum / count as f64;

        // Create clipboard and copy nodes centered at (0, 0). `copy_nodes_from`
        // only retains wires whose source is among the copied set, so any
        // cross-scope wire (an ancestor capture or an enclosing HOF's
        // iteration-value reference) is dropped — the clipboard holds a
        // self-contained fragment that pastes cleanly into any scope.
        let mut clipboard = NodeNetwork::new_empty();
        clipboard.copy_nodes_from(source, &selected_ids, -centroid);
        // Copying *out of a zone body* (non-empty scope): body nodes carry dead
        // `displayed=true` state (there is no eye UI inside a body), so dropping
        // it here prevents a copy-from-body → paste-to-top-level from re-opening
        // every eye (issue #340, copy-out-of-body path). Top-level copies keep
        // their real eye state so a top→top paste reproduces it.
        if !scope.is_empty() {
            clipboard.displayed_nodes.clear();
        }
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

        // Re-derive validation-dependent state for the pasted nodes. The
        // refresh path alone does not validate (see
        // `project_refresh_does_not_validate`), so without this an `apply`
        // node's arg-pin layout (installed by
        // `update_apply_pin_layouts_for_network`, which only runs inside
        // validation) stays collapsed to the bare `f` pin until the node is
        // next touched, dropping its wired arg connections from view. Runs
        // before the undo snapshot below so the snapshot captures the
        // settled, post-validation node state. See issue #326.
        self.validate_active_network();

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

    /// Scope-aware variant of [`paste_at_position`]. With an empty `scope_path`
    /// it delegates to the top-level `paste_at_position` (which keeps its own
    /// `PasteNodesCommand` undo path). With a non-empty path it pastes the
    /// clipboard into the addressed zone body and records the edit as a
    /// whole-body `EditZoneBodyCommand`, mirroring `duplicate_node_scoped` /
    /// `delete_selected_scoped`.
    pub fn paste_at_position_scoped(&mut self, scope_path: &[u64], position: DVec2) -> Vec<u64> {
        if scope_path.is_empty() {
            return self.paste_at_position(position);
        }

        // Snapshot the clipboard into a fresh network first — `copy_nodes_from`
        // needs an immutable borrow of the clipboard while we later borrow the
        // body mutably, so decouple them up front.
        let clipboard = match &self.clipboard {
            Some(cb) => cb,
            None => return vec![],
        };
        let all_clipboard_ids: HashSet<u64> = clipboard.nodes.keys().copied().collect();
        if all_clipboard_ids.is_empty() {
            return vec![];
        }
        let mut clipboard_snapshot = NodeNetwork::new_empty();
        clipboard_snapshot.copy_nodes_from(clipboard, &all_clipboard_ids, DVec2::ZERO);
        let snapshot_ids: HashSet<u64> = clipboard_snapshot.nodes.keys().copied().collect();

        // Whole-body before-state for undo (captured before mutation).
        let before = self.snapshot_zone_body(scope_path);
        // Case C reflow (doc/design_reflow_on_footprint_change.md): pasting nodes
        // grows the body, which grows the owning HOF in the parent and may
        // cascade up. Capture the owning HOF's pre-edit footprint chain now.
        let old_ancestor_sizes = self.capture_body_owner_footprint_chain(scope_path);

        let new_ids = match self.get_scope_network_mut(scope_path) {
            Some(body) => {
                let ids = body.copy_nodes_from(&clipboard_snapshot, &snapshot_ids, position);
                // Nodes inside a zone body have no eye UI, so display state is
                // dead/meaningless here. `copy_nodes_from` now preserves source
                // visibility (issue #340); strip it for body targets so the
                // source's eye state is ignored AND so that later copying these
                // nodes back out to a rendering scope does not re-open all eyes.
                for &id in &ids {
                    body.set_node_display(id, false);
                }
                // Keep all body content inside the body rect. The body grows
                // right/down to fit content but never up/left, so a paste near
                // the body's top-left corner can place nodes at negative
                // body-local coords that render outside (clipped) the rect.
                // Shift the whole body's content right/down so the top-left-most
                // node sits at the body inset; the layout pass then grows the
                // body on the right/bottom to fit. The inset matches the
                // Flutter drag-clamp floor (`_ZONE_BODY_DRAG_INSET`).
                shift_body_content_inside(body, ZONE_BODY_CONTENT_INSET);
                // Select the pasted nodes inside the body scope.
                body.select_nodes(ids.clone());
                ids
            }
            None => return vec![],
        };

        if new_ids.is_empty() {
            return vec![];
        }

        // Single-scope selection invariant: the pasted nodes are now the
        // selection (in the body scope), so clear any selection elsewhere.
        self.clear_selection_in_other_scopes(scope_path);

        // A body edit changes what the enclosing HOF emits, so re-validate and
        // re-evaluate from the top (matches `delete_selected_scoped`). The
        // refresh path alone does not validate (see
        // `project_refresh_does_not_validate`), so derived state like a pasted
        // `apply` node's arg-pin layout would otherwise stay stale (issue #326).
        self.set_dirty(true);
        self.mark_full_refresh();
        self.validate_active_network();
        self.push_zone_body_command_with_ancestor_reflow(
            scope_path,
            "Paste".to_string(),
            before,
            &old_ancestor_sizes,
        );

        new_ids
    }

    /// Cuts the currently selected nodes (copy + delete).
    /// Returns true if something was cut.
    ///
    /// Copy locates the selection's scope; the matching delete must run in that
    /// same scope so a zone-body selection is removed from its body (not the
    /// top-level network).
    pub fn cut_selection(&mut self) -> bool {
        if !self.copy_selection() {
            return false;
        }
        // `copy_selection` does not clear the selection, so it is still in the
        // same scope `copy_selection` located.
        let scope = self.find_selection_scope().unwrap_or_default();
        self.delete_selected_scoped(&scope);
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

        // Top-level active network: no enclosing zones.
        network.can_connect_nodes(
            source_node_id,
            source_output_pin_index,
            dest_node_id,
            dest_param_index,
            &self.node_type_registry,
            &[],
            &[],
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
        let (dest_param_is_multi, dest_is_function_pin, dest_is_apply, dest_function_pin_consumed) = {
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

            // Wiring an *ordinary input pin* on a node whose `-1` pin is
            // consumed changes the exposed function arity (the new wire freezes
            // a parameter into a capture), so the consumer's derived type must
            // re-derive. `function_pin_consumed` is the source-side trigger,
            // the analog of `dest_is_apply` on the consumer side
            // (`doc/design_node_function_pin_captures.md` §"Revalidation
            // triggers").
            let dest_function_pin_consumed = network.function_pin_consumed(dest_node_id);

            // Get the node type and check parameter
            match self.node_type_registry.get_node_type_for_node(dest_node) {
                Some(node_type) => {
                    if dest_param_index >= node_type.parameters.len() {
                        return;
                    }
                    let dt = &node_type.parameters[dest_param_index].data_type;
                    // `dest_is_function_pin` covers both `Function(_)` and
                    // `AnyFunction { .. }` (the destination-only constraint
                    // added in Function-pin Unification Phase A). After Phases
                    // B/C, `apply.f` / `map.f` are declared as `AnyFunction`,
                    // so a wire into them must still trigger revalidation.
                    (
                        dt.is_array(),
                        dt.is_function_shape(),
                        dest_node.node_type_name == "apply",
                        dest_function_pin_consumed,
                    )
                }
                None => return,
            }
        };

        // A wire that carries a *function value* toggles structural rules that
        // the connect-time type gate (`can_connect_nodes`) does not evaluate:
        // an HOF's `f` pin suspends the "zone-output pin needs a wire" rule, the
        // `apply` node requires its `f`, and a consumed function pin (`-1`)
        // forces the source into function-mode. The partial refresh that follows
        // re-evaluates but does not validate, so without an explicit re-validate
        // those errors would go stale (e.g. the zone-output error lingering on a
        // `map` whose `f` was just wired).
        //
        // Currying Phase 3 (`doc/design_currying.md`): wiring an arg pin on an
        // `apply` node also requires revalidation because the output pin type
        // depends on `k` (the count of wired arg pins). Without this, a wire
        // into `apply.arg0` would leave the output type stale at its previous
        // partial/full shape and downstream consumers would type-check against
        // the wrong type. Apply destinations therefore always revalidate.
        let revalidate = dest_is_function_pin
            || source_output_pin_index < 0
            || dest_is_apply
            || dest_function_pin_consumed;

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

        // Re-validate when a function wire was added (see `revalidate` above) so
        // a stale structural error from a previous pass clears now that the wire
        // satisfies the rule.
        if revalidate {
            self.validate_active_network();
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
        let (ancestors, ancestor_hof_ids) = match self.get_scope_ancestors(scope_path) {
            Some(t) => t,
            None => return false,
        };
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
            &ancestors,
            &ancestor_hof_ids,
        )
    }

    /// Scope-aware variant of [`connect_nodes`] — intra-body wires. With a
    /// non-empty `scope_path` the wire is added to the named HOF body and the
    /// edit is recorded via a whole-body `EditZoneBodyCommand` (no top-level
    /// display-policy orchestration; body display is per-body).
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
        let before = self.snapshot_zone_body(scope_path);
        // Case C reflow (doc/design_reflow_on_footprint_change.md): a wire can
        // grow an in-body node's footprint (e.g. an `apply`/`map` gaining arg
        // pins from the post-pass), growing the owning HOF in the parent and
        // possibly cascading up. Capture the owning HOF's pre-edit footprint
        // chain now; reflow sees `delta == 0` and moves nothing if it doesn't.
        let old_ancestor_sizes = self.capture_body_owner_footprint_chain(scope_path);
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
        self.push_zone_body_command_with_ancestor_reflow(
            scope_path,
            "Connect wire".to_string(),
            before,
            &old_ancestor_sizes,
        );
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
        if source_scope_depth == 0
            && let crate::structure_designer::node_network::SourcePin::NodeOutput { pin_index } =
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
        // Cross-scope / ZoneInput wire — capture the destination body's
        // before-state for undo (the wire is stored on a body-internal node).
        let before = self.snapshot_zone_body(dest_scope_path);
        // Case C reflow (doc/design_reflow_on_footprint_change.md): the wire can
        // grow the destination node (e.g. an `apply`/`map` gaining arg pins),
        // growing the owning HOF in the parent and possibly cascading up.
        // Capture the owning HOF's pre-edit footprint chain now (delta == 0 ⇒
        // no moves if the wire doesn't grow anything).
        let old_ancestor_sizes = self.capture_body_owner_footprint_chain(dest_scope_path);
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
        self.push_zone_body_command_with_ancestor_reflow(
            dest_scope_path,
            "Connect wire".to_string(),
            before,
            &old_ancestor_sizes,
        );
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
                // Resolve with the source's enclosing-zone chain so a
                // polymorphic (`SameAsInput`) source pin inside an HOF body —
                // e.g. `free_rot` fed by the body's delayed-argument `element`
                // pin — refines to the concrete element type instead of
                // dead-ending. See `get_scope_ancestors`.
                let (ancestors, ancestor_hof_ids) =
                    match self.get_scope_ancestors(source_scope_path) {
                        Some(t) => t,
                        None => return false,
                    };
                match self.node_type_registry.resolve_output_type_scoped(
                    source_node,
                    source_network,
                    pin_index,
                    &ancestors,
                    &ancestor_hof_ids,
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
        // The body-return wire lives on the HOF's `zone_output_arguments`, which
        // the body snapshot (keyed by the body scope path) captures.
        let before = self.snapshot_zone_body(body_scope_path);
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
        self.push_zone_body_command(
            body_scope_path,
            "Connect body return wire".to_string(),
            before,
        );
    }

    /// Scope-aware variant of [`duplicate_node`]. Body-scope duplication is
    /// recorded via a whole-body `EditZoneBodyCommand` (no top-level
    /// display-policy orchestration).
    pub fn duplicate_node_scoped(&mut self, scope_path: &[u64], node_id: u64) -> u64 {
        if scope_path.is_empty() {
            return self.duplicate_node(node_id);
        }
        let before = self.snapshot_zone_body(scope_path);
        // Case C reflow (doc/design_reflow_on_footprint_change.md): the duplicate
        // grows the body, which grows the owning HOF in the parent and may
        // cascade up. Capture the owning HOF's pre-edit footprint chain now.
        let old_ancestor_sizes = self.capture_body_owner_footprint_chain(scope_path);
        let new_id = match self.get_scope_network_mut(scope_path) {
            Some(network) => network.duplicate_node(node_id).unwrap_or(0),
            None => return 0,
        };
        if new_id != 0 {
            self.set_dirty(true);
            self.push_zone_body_command_with_ancestor_reflow(
                scope_path,
                "Duplicate node".to_string(),
                before,
                &old_ancestor_sizes,
            );
        }
        new_id
    }

    /// Scope-aware variant of [`toggle_node_selection`]. Phase U4.
    pub fn toggle_node_selection_scoped(&mut self, scope_path: &[u64], node_id: u64) -> bool {
        self.clear_selection_in_other_scopes(scope_path);
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
        self.clear_selection_in_other_scopes(scope_path);
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
        self.clear_selection_in_other_scopes(scope_path);
        if scope_path.is_empty() {
            return self.select_nodes(node_ids);
        }
        match self.get_scope_network_mut(scope_path) {
            Some(network) => network.select_nodes(node_ids),
            None => false,
        }
    }

    /// Scope-aware variant of [`toggle_nodes_selection`] (Ctrl+rectangle).
    /// Applies the single-scope invariant, then toggles within the target
    /// scope. Replaces the inline scope-dispatch the API used to do.
    pub fn toggle_nodes_selection_scoped(&mut self, scope_path: &[u64], node_ids: Vec<u64>) {
        self.clear_selection_in_other_scopes(scope_path);
        if scope_path.is_empty() {
            self.toggle_nodes_selection(node_ids);
        } else if let Some(network) = self.get_scope_network_mut(scope_path) {
            network.toggle_nodes_selection(node_ids);
        }
    }

    /// Scope-aware variant of [`add_nodes_to_selection`] (Shift+rectangle).
    /// Applies the single-scope invariant, then adds within the target scope.
    pub fn add_nodes_to_selection_scoped(&mut self, scope_path: &[u64], node_ids: Vec<u64>) {
        self.clear_selection_in_other_scopes(scope_path);
        if scope_path.is_empty() {
            self.add_nodes_to_selection(node_ids);
        } else if let Some(network) = self.get_scope_network_mut(scope_path) {
            network.add_nodes_to_selection(node_ids);
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
        if is_expr_node
            && let Some(expr_data) =
                data.as_any_mut()
                    .downcast_mut::<crate::structure_designer::nodes::expr::ExprData>()
        {
            expr_validation_errors = expr_data.parse_and_validate(node_id);
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
        if let Some(old_json) = old_data_json
            && let Some(new_json) =
                self.snapshot_node_data_scoped(&network_name, scope_path, node_id)
            && old_json != new_json
        {
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

    /// Whole-list lane edit on a `zip_with` node — the positional id merge
    /// (`doc/design_zip_with.md` Phase 3): the lane at position `i` keeps the
    /// old position-`i` id (retype preserves identity), growth mints fresh ids
    /// from `next_lane_id`, shrink drops the tail **and disconnects body wires
    /// referencing the dropped tail indices** (recursively, including nested
    /// HOF bodies — validation rule 3 only flags those red, it never cleans
    /// them). An empty lane list is rejected. Undo: whole-top-level-network
    /// before/after snapshots via [`NodeStructureEditCommand`] — a node-data
    /// snapshot cannot capture the wire fallout.
    ///
    /// [`NodeStructureEditCommand`]: super::undo::commands::node_structure_edit::NodeStructureEditCommand
    pub fn set_zip_with_lanes(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        lane_types: Vec<DataType>,
    ) -> Result<(), String> {
        use crate::structure_designer::nodes::zip_with::ZipWithData;

        if lane_types.is_empty() {
            return Err("zip_with requires at least one lane".to_string());
        }
        // Lane-only edit: preserve the node's current stored output type and
        // delegate to the combined data setter (single undo command).
        let output_type = {
            let node = self
                .get_scope_network(scope_path)
                .and_then(|net| net.nodes.get(&node_id))
                .ok_or_else(|| "Node not found".to_string())?;
            node.data
                .as_any_ref()
                .downcast_ref::<ZipWithData>()
                .ok_or_else(|| "Node is not a zip_with".to_string())?
                .output_type
                .clone()
        };
        self.set_zip_with_data(scope_path, node_id, lane_types, output_type)
    }

    /// Combined whole-list lane + output-type edit on a `zip_with` node — the
    /// path the API setter drives (`doc/design_zip_with.md` Phase 5). Lanes
    /// merge positionally (id-preserving retype, tail-drop shrink with body-wire
    /// cleanup, `next_lane_id`-minted growth); the stored `output_type` is set
    /// alongside so a retype of the `result` zone-output pin flows through the
    /// same repair + undo path (`repair_zone_body` drops an incompatible result
    /// wire). Empty lane list is rejected. Undo: whole-top-level-network
    /// before/after snapshots via [`NodeStructureEditCommand`], shared with
    /// [`Self::remove_zip_with_lane`] — the API layer adds no undo logic.
    ///
    /// [`NodeStructureEditCommand`]: super::undo::commands::node_structure_edit::NodeStructureEditCommand
    pub fn set_zip_with_data(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        lane_types: Vec<DataType>,
        output_type: DataType,
    ) -> Result<(), String> {
        use crate::structure_designer::nodes::zip_with::{
            ZipWithData, disconnect_zip_body_wires_to_dropped_lanes,
        };

        if lane_types.is_empty() {
            return Err("zip_with requires at least one lane".to_string());
        }
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or_else(|| "No active network".to_string())?;

        // Read-only pre-check: resolve the node and reject before any
        // mutation or snapshot so an error leaves the designer untouched.
        let (old_types, old_output) = {
            let node = self
                .get_scope_network(scope_path)
                .and_then(|net| net.nodes.get(&node_id))
                .ok_or_else(|| "Node not found".to_string())?;
            let data = node
                .data
                .as_any_ref()
                .downcast_ref::<ZipWithData>()
                .ok_or_else(|| "Node is not a zip_with".to_string())?;
            (data.lane_types(), data.output_type.clone())
        };
        if old_types == lane_types && old_output == output_type {
            return Ok(()); // no-op; don't push an empty command
        }

        let before = self.snapshot_network(&network_name);

        {
            let node = self
                .get_scope_network_mut(scope_path)
                .and_then(|net| net.nodes.get_mut(&node_id))
                .ok_or_else(|| "Node not found".to_string())?;
            let data = node
                .data
                .as_any_mut()
                .downcast_mut::<ZipWithData>()
                .ok_or_else(|| "Node is not a zip_with".to_string())?;
            data.merge_lane_types(lane_types.clone())?;
            data.output_type = output_type;
            if lane_types.len() < old_types.len() {
                disconnect_zip_body_wires_to_dropped_lanes(node, lane_types.len());
            }
        }

        self.finish_node_structure_edit(
            scope_path,
            node_id,
            &network_name,
            before,
            "Edit zip_with lanes",
        );
        Ok(())
    }

    /// Id-accurate removal of one specific `zip_with` lane
    /// (`doc/design_zip_with.md` Phase 3). Surviving lanes keep their hidden
    /// stable ids, so external wires follow them while the `xs{i}` labels
    /// renumber; the removed lane's external wire is dropped by the by-id
    /// argument rebuild. Body wires referencing the removed
    /// `ZoneInput { pin_index }` are disconnected and wires to later indices
    /// decremented **here, at mutation time** — a repair pass has no removal
    /// diff, and a shifted-but-in-range wire between same-typed lanes is
    /// silently wrong rather than invalid. The remap recurses into nested HOF
    /// bodies with exact depth + id matching (node ids collide across scopes).
    /// Removing the last remaining lane is rejected.
    pub fn remove_zip_with_lane(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        lane_index: usize,
    ) -> Result<(), String> {
        use crate::structure_designer::nodes::zip_with::{
            ZipWithData, remap_zip_body_wires_for_lane_removal,
        };

        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or_else(|| "No active network".to_string())?;

        // Read-only pre-check (see `set_zip_with_lanes`).
        {
            let node = self
                .get_scope_network(scope_path)
                .and_then(|net| net.nodes.get(&node_id))
                .ok_or_else(|| "Node not found".to_string())?;
            let data = node
                .data
                .as_any_ref()
                .downcast_ref::<ZipWithData>()
                .ok_or_else(|| "Node is not a zip_with".to_string())?;
            if lane_index >= data.lanes.len() {
                return Err(format!(
                    "lane index {} out of range ({} lanes)",
                    lane_index,
                    data.lanes.len()
                ));
            }
            if data.lanes.len() == 1 {
                return Err("zip_with requires at least one lane".to_string());
            }
        }

        let before = self.snapshot_network(&network_name);

        {
            let node = self
                .get_scope_network_mut(scope_path)
                .and_then(|net| net.nodes.get_mut(&node_id))
                .ok_or_else(|| "Node not found".to_string())?;
            node.data
                .as_any_mut()
                .downcast_mut::<ZipWithData>()
                .ok_or_else(|| "Node is not a zip_with".to_string())?
                .remove_lane(lane_index)?;
            remap_zip_body_wires_for_lane_removal(node, lane_index);
        }

        self.finish_node_structure_edit(
            scope_path,
            node_id,
            &network_name,
            before,
            "Edit zip_with lanes",
        );
        Ok(())
    }

    /// Whole-data edit on a `switch` node (`doc/design_switch_node.md` Phase 2):
    /// set the `selector_type`, `value_type`, and case list in one step, with
    /// the value-keyed id merge preserving wires. `case_values` are already in
    /// the **target** selector domain (the API layer parses the editor's text
    /// fields per `selector_type`).
    ///
    /// Order is load-bearing: `convert_selector_type` flips the stored cases
    /// into the new domain (ids untouched) **before** `merge_cases`, so a
    /// same-type value match can still follow a wire across a selector flip
    /// (String `"1"` ≠ Int `1` otherwise). The edit is validated on a **clone**
    /// first, so any rejection (bad selector type, unparseable / colliding
    /// String→Int flip, duplicate / empty case list, domain mismatch) leaves the
    /// designer completely untouched — no snapshot, no command. Undo: whole-
    /// top-level-network before/after snapshots via
    /// [`NodeStructureEditCommand`] (a node-data snapshot cannot capture the
    /// dropped-case / retype wire fallout), shared with the `zip_with` ops.
    ///
    /// [`NodeStructureEditCommand`]: super::undo::commands::node_structure_edit::NodeStructureEditCommand
    pub fn set_switch_data(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        selector_type: DataType,
        value_type: DataType,
        case_values: Vec<crate::structure_designer::nodes::switch::SwitchCaseValue>,
    ) -> Result<(), String> {
        use crate::structure_designer::nodes::switch::{SwitchCaseValue, SwitchData};

        if !matches!(selector_type, DataType::Int | DataType::String) {
            return Err("selector_type must be Int or String".to_string());
        }
        // Every supplied value must already match the target selector domain.
        for v in &case_values {
            let matches_domain = matches!(
                (&selector_type, v),
                (DataType::Int, SwitchCaseValue::Int(_))
                    | (DataType::String, SwitchCaseValue::String(_))
            );
            if !matches_domain {
                return Err(format!(
                    "case value {} does not match selector type {}",
                    v.to_display_string(),
                    selector_type
                ));
            }
        }

        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or_else(|| "No active network".to_string())?;

        // Read-only pre-check: resolve the node, then run the full edit on a
        // clone so a convert/merge rejection leaves the designer untouched.
        let (old_selector, old_value_type, old_values, new_data) = {
            let node = self
                .get_scope_network(scope_path)
                .and_then(|net| net.nodes.get(&node_id))
                .ok_or_else(|| "Node not found".to_string())?;
            let data = node
                .data
                .as_any_ref()
                .downcast_ref::<SwitchData>()
                .ok_or_else(|| "Node is not a switch".to_string())?;
            let old_selector = data.selector_type.clone();
            let old_value_type = data.value_type.clone();
            let old_values: Vec<SwitchCaseValue> =
                data.cases.iter().map(|c| c.value.clone()).collect();
            let mut trial = data.clone();
            trial.convert_selector_type(&selector_type)?;
            trial.value_type = value_type.clone();
            trial.merge_cases(case_values.clone())?;
            (old_selector, old_value_type, old_values, trial)
        };

        // No-op check: identical selector / value type / case values means the
        // ids and wires are unchanged too — don't push an empty command (which
        // would truncate the redo tail).
        if old_selector == selector_type
            && old_value_type == value_type
            && old_values == case_values
        {
            return Ok(());
        }

        let before = self.snapshot_network(&network_name);

        {
            let node = self
                .get_scope_network_mut(scope_path)
                .and_then(|net| net.nodes.get_mut(&node_id))
                .ok_or_else(|| "Node not found".to_string())?;
            let data = node
                .data
                .as_any_mut()
                .downcast_mut::<SwitchData>()
                .ok_or_else(|| "Node is not a switch".to_string())?;
            *data = new_data;
        }

        self.finish_node_structure_edit(
            scope_path,
            node_id,
            &network_name,
            before,
            "Edit switch cases",
        );
        Ok(())
    }

    /// Shared tail of the variadic-pin structural edit ops (`zip_with` lane
    /// edits, `switch` case edits): repair (the node's external arguments
    /// rebuild by hidden stable id in `repair_node_network`'s populate pass,
    /// and — for zone-bearing nodes — `repair_zone_body` drops retype-
    /// incompatible / out-of-range depth-1 body wires), re-validate, mark
    /// refresh state, and push the whole-network undo command with `description`
    /// as its history label. The caller has already established that the edit
    /// actually changed the network, so the command is always meaningful.
    fn finish_node_structure_edit(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        network_name: &str,
        before: Option<super::serialization::node_networks_serialization::SerializableNodeNetwork>,
        description: &str,
    ) {
        // Split-borrow pattern: take the network out so `repair_node_network`
        // can consult the registry it lives in.
        if let Some(mut network) = self.node_type_registry.node_networks.remove(network_name) {
            self.node_type_registry.repair_node_network(&mut network);
            self.node_type_registry
                .node_networks
                .insert(network_name.to_string(), network);
        }
        self.validate_active_network();

        self.set_dirty(true);
        self.pending_changes
            .mark_node_data_changed_scoped(scope_path, node_id);
        // Wire fallout can reach nodes other than the edited one (dropped
        // external wire, body remap), so a partial refresh keyed on the edited
        // node alone would leave stale output.
        self.mark_full_refresh();

        if let (Some(before_snapshot), Some(after_snapshot)) =
            (before, self.snapshot_network(network_name))
        {
            self.push_command(
                super::undo::commands::node_structure_edit::NodeStructureEditCommand {
                    network_name: network_name.to_string(),
                    description: description.to_string(),
                    before_snapshot,
                    after_snapshot,
                },
            );
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

    /// Resolve `node_id` against the **top-level active network only** (no
    /// recursion into HOF zone bodies). Body-scope nodes have their own
    /// per-body id counters, so a bare id is only unambiguous at the top level.
    /// This is the right lookup for interactive subsystems that act on the
    /// top-level *active* node (atom_edit / edit_atom / facet_shell / import).
    /// The property panel, which can target a node in any scope, must use
    /// [`get_node_network_data_scoped`] instead.
    pub fn get_node_network_data(&self, node_id: u64) -> Option<&dyn NodeData> {
        self.get_active_node_network()?
            .get_node_network_data(node_id)
    }

    /// Scope-aware read: resolve `node_id` against the network identified by
    /// `scope_path` (empty = top-level active network). Used by the per-node
    /// property getters so a body node whose id collides with a top-level id is
    /// addressed unambiguously.
    pub fn get_node_network_data_scoped(
        &self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Option<&dyn NodeData> {
        self.get_scope_network(scope_path)?
            .get_node_network_data(node_id)
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
        network.get_node_network_data_mut(node_id)
    }

    /// Scope-aware in-place mutable accessor (the `_mut` counterpart of
    /// [`get_node_network_data_scoped`]). Marks the node dirty at its actual
    /// scope and returns a mutable handle to its `NodeData`. Use for nodes that
    /// mutate their data in place (e.g. `facet_shell`, or `import_xyz`/
    /// `import_cif` loading a file) rather than replacing it wholesale via
    /// [`set_node_network_data_scoped`].
    pub fn get_node_network_data_mut_scoped(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Option<&mut dyn NodeData> {
        self.pending_changes
            .mark_node_data_changed_scoped(scope_path, node_id);
        self.get_scope_network_mut(scope_path)?
            .get_node_network_data_mut(node_id)
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
        // Activating a network leaves the schema editor: the active record def
        // is backend-owned (see `doc/design_hierarchical_records.md` §8), so we
        // clear it here rather than relying on the Flutter side to do it.
        self.active_record_def_name = None;
        // Switching networks requires full refresh (everything changes)
        self.mark_full_refresh();
        // Validate (and thereby repair) the newly active network *before* the
        // caller's refresh evaluates it. Refresh paths never validate on their
        // own (see `doc/design_..` / the refresh contract), and a freshly
        // *loaded* network has only been through the load-time
        // `repair_node_network`, which grows `arguments` to match parameter
        // counts for top-level `network.nodes` only — body (HOF/zone) nodes are
        // grown solely by `repair_network_arguments`, which runs only inside
        // `validate_network`. Without this call, switching to such a network
        // would `generate_scene` over an unrepaired graph: a node with
        // `parameters.len() > arguments.len()` (most easily an `expr`) read a
        // missing argument slot, which used to panic and now silently evaluates
        // as "unconnected" — either way the displayed output was wrong until the
        // user poked the canvas and something *else* triggered a validate. This
        // makes the activated network correct on the first frame.
        self.validate_active_network();
        // Return camera settings from the newly active network
        self.get_active_node_network()
            .and_then(|n| n.camera_settings.clone())
    }

    /// The user record type def currently open in the schema editor. Backend-
    /// owned source of truth (see `doc/design_hierarchical_records.md` §8);
    /// Flutter mirrors it in `refreshFromKernel` (Phase 2).
    pub fn get_active_record_def_name(&self) -> Option<String> {
        self.active_record_def_name.clone()
    }

    /// Set the active record def. `None` clears the schema-editor selection.
    pub fn set_active_record_def_name(&mut self, name: Option<String>) {
        self.active_record_def_name = name;
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

        // Clear user-declared record type definitions (they belong to the old
        // document). Built-in record defs (e.g. ElementMapping) are
        // application-supplied and intentionally preserved.
        self.node_type_registry.record_type_defs.clear();

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
        self.pending_zone_resize = None;

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
        if let Some(data) = self.get_node_network_data_mut(node_id)
            && let Some(atom_edit_data) = data
                .as_any_mut()
                .downcast_mut::<crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData>()
            {
                atom_edit_data.active_tool =
                    crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditTool::AddAtom(
                        crate::structure_designer::nodes::atom_edit::atom_edit::AddAtomToolState::Idle,
                    );
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
                if let Some(node_id) = node_id
                    && let Some(data) = self.get_node_network_data_mut(node_id)
                        && let Some(atom_edit_data) = data
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
                if let Some(node_data) = data
                    && let Some(g) = &self.gadget
                {
                    g.sync_data(node_data);
                    // Mark design as dirty since gadget data was synced back to node
                    self.set_dirty(true);
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
        self.clear_selection_in_other_scopes(scope_path);
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

    /// Enforce the **single-scope selection invariant**: clear the selection
    /// (selected nodes, selected wires, and active node) in every scope
    /// reachable from the active top-level network *except* the scope addressed
    /// by `keep_scope_path`.
    ///
    /// Every scoped selection mutator calls this first, so a selection action
    /// in one scope wipes any selection lingering in another. It is
    /// **modifier-agnostic**: a Shift/Ctrl click that crosses a scope boundary
    /// collapses to a fresh single-scope selection, because the prior scope is
    /// cleared here and the additive modifier then applies against the
    /// now-empty target scope. Selection therefore lives in exactly one scope
    /// at a time — the editor can never show two highlighted scopes whose
    /// keyboard-delete target silently disagrees with what's highlighted.
    ///
    /// When the kept scope is a body (`keep_scope_path` non-empty) the
    /// top-level network is among those cleared, so this runs the same
    /// selection-change / display-policy bookkeeping that
    /// [`clear_selection_all_scopes`] does (so a "Selected"/"Frontier" display
    /// policy reacts to the top-level node losing its selection).
    pub fn clear_selection_in_other_scopes(&mut self, keep_scope_path: &[u64]) {
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
            clear_selection_except_recursive(network, keep_scope_path);
        }
        if !keep_scope_path.is_empty() {
            self.mark_selection_changed(previously_active_node_id, None);
            if let Some(prev_id) = previously_active_node_id {
                let mut dirty_nodes = HashSet::new();
                dirty_nodes.insert(prev_id);
                self.apply_node_display_policy(Some(&dirty_nodes));
            }
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

    /// Called when a top-level node drag begins. Captures start positions for
    /// undo coalescing.
    pub fn begin_move_nodes(&mut self) {
        self.begin_move_nodes_scoped(&[]);
    }

    /// Scope-aware variant of [`begin_move_nodes`]. Captures the start positions
    /// of the selected nodes in the body identified by `scope_path` (empty =
    /// top-level) so the matching [`end_move_nodes`] coalesces the drag into a
    /// single scope-aware `MoveNodesCommand`.
    pub fn begin_move_nodes_scoped(&mut self, scope_path: &[u64]) {
        if let Some(network) = self.get_scope_network(scope_path) {
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
            self.pending_move = Some(super::undo::snapshot::PendingMove {
                scope_path: scope_path.to_vec(),
                start_positions,
            });
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

        let network = match self.get_scope_network(&pending.scope_path) {
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
                scope_path: pending.scope_path,
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

    /// Scope-aware variant of [`select_wire`] (plain click). Selects a single
    /// wire in the network identified by `scope_path` (empty = top-level),
    /// after applying the single-scope invariant. Only same-scope regular
    /// wires are addressable this way (the only shape `NodeNetwork::select_wire`
    /// stores); captures, zone-input refs and zone-output wires are not
    /// selectable.
    pub fn select_wire_scoped(
        &mut self,
        scope_path: &[u64],
        source_node_id: u64,
        source_output_pin_index: i32,
        destination_node_id: u64,
        destination_argument_index: usize,
    ) -> bool {
        self.clear_selection_in_other_scopes(scope_path);
        if scope_path.is_empty() {
            return self.select_wire(
                source_node_id,
                source_output_pin_index,
                destination_node_id,
                destination_argument_index,
            );
        }
        match self.get_scope_network_mut(scope_path) {
            Some(network) => network.select_wire(
                source_node_id,
                source_output_pin_index,
                destination_node_id,
                destination_argument_index,
            ),
            None => false,
        }
    }

    /// Scope-aware variant of [`toggle_wire_selection`] (Ctrl+click).
    pub fn toggle_wire_selection_scoped(
        &mut self,
        scope_path: &[u64],
        source_node_id: u64,
        source_output_pin_index: i32,
        destination_node_id: u64,
        destination_argument_index: usize,
    ) -> bool {
        self.clear_selection_in_other_scopes(scope_path);
        if scope_path.is_empty() {
            return self.toggle_wire_selection(
                source_node_id,
                source_output_pin_index,
                destination_node_id,
                destination_argument_index,
            );
        }
        match self.get_scope_network_mut(scope_path) {
            Some(network) => network.toggle_wire_selection(
                source_node_id,
                source_output_pin_index,
                destination_node_id,
                destination_argument_index,
            ),
            None => false,
        }
    }

    /// Scope-aware variant of [`add_wire_to_selection`] (Shift+click).
    pub fn add_wire_to_selection_scoped(
        &mut self,
        scope_path: &[u64],
        source_node_id: u64,
        source_output_pin_index: i32,
        destination_node_id: u64,
        destination_argument_index: usize,
    ) -> bool {
        self.clear_selection_in_other_scopes(scope_path);
        if scope_path.is_empty() {
            return self.add_wire_to_selection(
                source_node_id,
                source_output_pin_index,
                destination_node_id,
                destination_argument_index,
            );
        }
        match self.get_scope_network_mut(scope_path) {
            Some(network) => network.add_wire_to_selection(
                source_node_id,
                source_output_pin_index,
                destination_node_id,
                destination_argument_index,
            ),
            None => false,
        }
    }

    /// Scope-aware variant of [`select_nodes_and_wires`] (rectangle select).
    pub fn select_nodes_and_wires_scoped(
        &mut self,
        scope_path: &[u64],
        node_ids: Vec<u64>,
        wires: Vec<crate::structure_designer::node_network::Wire>,
    ) {
        self.clear_selection_in_other_scopes(scope_path);
        if scope_path.is_empty() {
            self.select_nodes_and_wires(node_ids, wires);
        } else if let Some(network) = self.get_scope_network_mut(scope_path) {
            network.select_nodes_and_wires(node_ids, wires);
        }
    }

    /// Scope-aware variant of [`add_nodes_and_wires_to_selection`].
    pub fn add_nodes_and_wires_to_selection_scoped(
        &mut self,
        scope_path: &[u64],
        node_ids: Vec<u64>,
        wires: Vec<crate::structure_designer::node_network::Wire>,
    ) {
        self.clear_selection_in_other_scopes(scope_path);
        if scope_path.is_empty() {
            self.add_nodes_and_wires_to_selection(node_ids, wires);
        } else if let Some(network) = self.get_scope_network_mut(scope_path) {
            network.add_nodes_and_wires_to_selection(node_ids, wires);
        }
    }

    /// Scope-aware variant of [`toggle_nodes_and_wires_selection`].
    pub fn toggle_nodes_and_wires_selection_scoped(
        &mut self,
        scope_path: &[u64],
        node_ids: Vec<u64>,
        wires: Vec<crate::structure_designer::node_network::Wire>,
    ) {
        self.clear_selection_in_other_scopes(scope_path);
        if scope_path.is_empty() {
            self.toggle_nodes_and_wires_selection(node_ids, wires);
        } else if let Some(network) = self.get_scope_network_mut(scope_path) {
            network.toggle_nodes_and_wires_selection(node_ids, wires);
        }
    }

    pub fn delete_selected(&mut self) {
        self.delete_selected_scoped(&[]);
    }

    /// Scope-aware variant of [`delete_selected`]. With a non-empty `scope_path`
    /// the body's `delete_selected` runs (without the top-level display-policy
    /// machinery) and the edit is recorded via a whole-body
    /// `EditZoneBodyCommand`.
    pub fn delete_selected_scoped(&mut self, scope_path: &[u64]) {
        if !scope_path.is_empty() {
            // Whole-body snapshot for undo. If nothing was selected the body is
            // unchanged and `push_zone_body_command`'s diff check drops the
            // no-op command.
            let before = self.snapshot_zone_body(scope_path);

            // Case A reflow (doc/design_reflow_on_footprint_change.md): predict
            // HOFs in this body that will expand when their `f` wire is removed,
            // and snapshot their compact footprint chains — both must be read
            // before the deletion applies.
            let mut reflow_targets: Vec<(u64, Vec<DVec2>)> = Vec::new();
            if let Some(network) = self.get_scope_network(scope_path) {
                let info = network.collect_deletion_info();
                let removed_wires = if info.is_node_deletion {
                    &info.deleted_wires
                } else {
                    &info.selected_wires
                };
                let expanding = self.predict_f_disconnect_expansions(
                    network,
                    removed_wires,
                    &info.deleted_node_ids,
                );
                for hof_id in expanding {
                    reflow_targets.push((hof_id, self.capture_footprint_chain(scope_path, hof_id)));
                }
            }

            if let Some(network) = self.get_scope_network_mut(scope_path) {
                network.delete_selected();
            }
            self.set_dirty(true);
            // A body-internal delete changes what the enclosing HOF emits, so
            // re-validate and re-evaluate from the top so downstream consumers
            // refresh (the undo command itself uses a Full refresh).
            self.mark_full_refresh();
            self.validate_active_network();

            // Reflow each expanded HOF starting INSIDE the body. The moves that
            // land in the body scope itself ride the fresh after-snapshot taken
            // by `build_zone_body_command`, so only the ANCESTOR cascade (if the
            // body grew past its stored size) needs explicit bundling.
            let mut ancestor_moves: Vec<ScopedMoves> = Vec::new();
            for (hof_id, old_sizes) in &reflow_targets {
                for sm in self.reflow_for_footprint_change(scope_path, *hof_id, old_sizes) {
                    if sm.scope_path.as_slice() != scope_path {
                        ancestor_moves.push(sm);
                    }
                }
            }

            let Some(edit_cmd) =
                self.build_zone_body_command(scope_path, "Delete".to_string(), before)
            else {
                return;
            };
            if ancestor_moves.is_empty() {
                self.push_command(edit_cmd);
            } else if let Some(network_name) = self.active_node_network_name.clone() {
                let description = edit_cmd.description().to_string();
                let mut commands: Vec<Box<dyn UndoCommand>> = vec![Box::new(edit_cmd)];
                for sm in ancestor_moves {
                    commands.push(Box::new(
                        super::undo::commands::move_nodes::MoveNodesCommand {
                            network_name: network_name.clone(),
                            scope_path: sm.scope_path,
                            moves: sm.moves,
                            description: "Reflow neighbours".to_string(),
                        },
                    ));
                }
                self.undo_stack.push(Box::new(
                    super::undo::commands::composite::CompositeCommand {
                        commands,
                        description,
                    },
                ));
            } else {
                self.push_command(edit_cmd);
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
        // If the network already carries validation errors, the deletion may
        // have removed the node or wire that caused one (e.g. a `closure`/HOF
        // whose zone body had no zone-output wire). The targeted
        // `should_validate` heuristics below only catch parameter /
        // invalid-network-reference / function-pin cases, so without this a
        // stale error could survive the deletion of its offending node. We key
        // off `validation_errors` (not just `!valid`) so this also clears
        // *non-blocking* errors (`ValidationError::warning`) — those keep
        // `network.valid == true`, so a `!valid` check would miss them and the
        // stale badge/entry would linger until the next unrelated edit.
        let mut should_validate = self
            .node_type_registry
            .node_networks
            .get(&node_network_name)
            .map(|network| !network.validation_errors.is_empty())
            .unwrap_or(false);

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
                    if let Some(node) = node_network.nodes.get(&selected_node_id)
                        && (node.node_type_name == "parameter" || {
                            // Check if this node references an invalid node network
                            self.node_type_registry
                                .node_networks
                                .get(&node.node_type_name)
                                .map(|network| !network.valid)
                                .unwrap_or(false)
                        })
                    {
                        should_validate = true;
                    }
                }

                // Deleting a node that feeds a *capture* input of a
                // function-consumed node retypes that node's `-1` pin (the
                // frozen capture becomes an unconnected parameter again),
                // changing the exposed arity. Any connected node that is
                // itself function-consumed therefore needs revalidation
                // (`doc/design_node_function_pin_captures.md` §"Revalidation
                // triggers"). `dirty_nodes` already enumerates the nodes
                // connected to the deletion, so it's the natural place to test.
                if !should_validate
                    && dirty_nodes
                        .iter()
                        .any(|&id| node_network.function_pin_consumed(id))
                {
                    should_validate = true;
                }
            }
            // If wires are selected, both source and destination nodes will be dirty
            else if !node_network.selected_wires.is_empty() {
                for wire in &node_network.selected_wires {
                    dirty_nodes.insert(wire.source_node_id);
                    dirty_nodes.insert(wire.destination_node_id);

                    // Removing a *function* wire un-suspends the structural rule
                    // it satisfied — an HOF's `f` pin re-enables the "zone-output
                    // pin needs a wire" rule, `apply` needs its `f`, and a freed
                    // function pin (`-1`) leaves function-mode. The connect-time
                    // gate doesn't evaluate those rules and the full refresh
                    // below doesn't validate, so request an explicit re-validate
                    // (the mirror of the function-wire case in `connect_nodes`).
                    let source_is_function_pin = wire.source_pin_index().is_some_and(|p| p < 0);
                    // Function-shape covers both `Function(_)` and
                    // `AnyFunction { .. }` (see `DataType::is_function_shape`).
                    // Function-pin Unification Phases B/C make `apply.f` /
                    // `map.f` declared as `AnyFunction`.
                    let dest_is_function_pin = node_network
                        .nodes
                        .get(&wire.destination_node_id)
                        .and_then(|n| self.node_type_registry.get_node_type_for_node(n))
                        .and_then(|nt| nt.parameters.get(wire.destination_argument_index))
                        .is_some_and(|p| p.data_type.is_function_shape());
                    // Currying Phase 3 / Function-pin Unification Phase D: an
                    // `apply` node's output type depends on `k` (the count of
                    // wired arg pins). Deleting *any* wire whose destination is
                    // an apply changes `k`, so the post-pass that rewrites
                    // apply's `custom_node_type` must re-run. Mirrors the
                    // `dest_is_apply` arm in `connect_nodes`; without it the
                    // declared output stays stale at the previous k's value
                    // while runtime returns the partial closure, and any
                    // downstream wire type-checks against a stale type.
                    let dest_is_apply = node_network
                        .nodes
                        .get(&wire.destination_node_id)
                        .is_some_and(|n| n.node_type_name == "apply");
                    // Deleting an *ordinary input wire* on a node whose `-1` pin
                    // is consumed restores a parameter (the frozen capture
                    // becomes an unconnected pin again), changing the exposed
                    // arity — so the consumer's derived type must re-derive
                    // (`doc/design_node_function_pin_captures.md` §"Revalidation
                    // triggers"). Source-side analog of `dest_is_apply`.
                    let dest_function_pin_consumed =
                        node_network.function_pin_consumed(wire.destination_node_id);
                    if source_is_function_pin
                        || dest_is_function_pin
                        || dest_is_apply
                        || dest_function_pin_consumed
                    {
                        should_validate = true;
                    }
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
        if let Some(ref info) = deletion_info
            && info.is_node_deletion
        {
            for &node_id in &info.deleted_node_ids {
                if let Some(snap) = self.snapshot_node(&node_network_name, node_id) {
                    deleted_node_snapshots.push(snap);
                }
            }
        }

        // Case A reflow (doc/design_reflow_on_footprint_change.md): predict which
        // collapsable HOFs will expand once this deletion removes their `f` wire,
        // and snapshot their *compact* footprints — both must be read before the
        // deletion applies. The moves themselves are computed after deletion (the
        // flip is a post-deletion fact) and bundled with the delete command.
        let mut reflow_targets: Vec<(u64, Vec<DVec2>)> = Vec::new();
        if let (Some(info), Some(network)) = (
            deletion_info.as_ref(),
            self.node_type_registry
                .node_networks
                .get(&node_network_name),
        ) {
            let removed_wires = if info.is_node_deletion {
                &info.deleted_wires
            } else {
                &info.selected_wires
            };
            let expanding = self.predict_f_disconnect_expansions(
                network,
                removed_wires,
                &info.deleted_node_ids,
            );
            for hof_id in expanding {
                reflow_targets.push((hof_id, self.capture_footprint_chain(&[], hof_id)));
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

        // Build the delete command (don't push yet — Case A reflow may bundle it
        // with the neighbour moves into one undo step).
        let mut delete_command: Option<Box<dyn UndoCommand>> = None;
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
                delete_command = Some(Box::new(
                    super::undo::commands::delete_nodes::DeleteNodesCommand {
                        network_name: node_network_name.clone(),
                        deleted_nodes: deleted_node_snapshots,
                        deleted_wires,
                        was_return_node: info.was_return_node,
                        display_states: info.display_states,
                        description,
                    },
                ));
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

                delete_command = Some(Box::new(
                    super::undo::commands::delete_wires::DeleteWiresCommand {
                        network_name: node_network_name.clone(),
                        deleted_wires,
                    },
                ));
            }
        }

        // Now that the deletion has applied, run reflow for each HOF that lost
        // its `f` wire and flipped to expanded, and bundle the resulting
        // neighbour moves with the delete command so they undo/redo as one step.
        if let Some(delete_command) = delete_command {
            let mut scoped_moves: Vec<ScopedMoves> = Vec::new();
            for (hof_id, old_sizes) in &reflow_targets {
                scoped_moves.extend(self.reflow_for_footprint_change(&[], *hof_id, old_sizes));
            }
            if scoped_moves.is_empty() {
                self.undo_stack.push(delete_command);
            } else {
                let description = delete_command.description().to_string();
                let mut commands: Vec<Box<dyn UndoCommand>> = vec![delete_command];
                for sm in scoped_moves {
                    commands.push(Box::new(
                        super::undo::commands::move_nodes::MoveNodesCommand {
                            network_name: node_network_name.clone(),
                            scope_path: sm.scope_path,
                            moves: sm.moves,
                            description: "Reflow neighbours".to_string(),
                        },
                    ));
                }
                self.undo_stack.push(Box::new(
                    super::undo::commands::composite::CompositeCommand {
                        commands,
                        description,
                    },
                ));
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
                if let NodeOutput::Atomic(atomic_structure, _) = pin_output
                    && let HitTestResult::Atom(atom_id, distance) = atomic_structure.hit_test(
                        ray_origin,
                        ray_direction,
                        visualization,
                        |atom| get_displayed_atom_radius(atom, &display_visualization),
                        BAS_STICK_RADIUS,
                    )
                    && closest.as_ref().is_none_or(|c| distance < c.2)
                {
                    closest = Some((atom_id, atomic_structure, distance));
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
                if let NodeOutput::Atomic(atomic_structure, _) = pin_output
                    && let HitTestResult::Atom(atom_id, distance) = atomic_structure.hit_test(
                        ray_origin,
                        ray_direction,
                        visualization,
                        |atom| get_displayed_atom_radius(atom, &display_visualization),
                        BAS_STICK_RADIUS,
                    )
                    && closest.as_ref().is_none_or(|c| distance < c.3)
                {
                    closest = Some((atom_id, atomic_structure, node_id, distance));
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
                if let Some(geo_tree) = pin_geo_tree
                    && let Some(geo_distance) =
                        raytrace_geometry(geo_tree, ray_origin, ray_direction, 1.0)
                {
                    min_distance = match min_distance {
                        None => Some(geo_distance),
                        Some(current_min) if geo_distance < current_min => Some(geo_distance),
                        _ => min_distance,
                    };
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

        if let Some(network) = network
            && let Some(node) = network.nodes.get(&node_id)
        {
            if let Some(ref custom_name) = node.custom_name {
                return custom_name.clone();
            }
            return format!("{} #{}", node.node_type_name, node_id);
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
        if let Some(network_name) = &self.active_node_network_name
            && let Some(node_network) = self.node_type_registry.node_networks.get_mut(network_name)
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
            if let Some(network_name) = &self.active_node_network_name.clone()
                && let Some(network) = self.node_type_registry.node_networks.get(network_name)
                && let Some(node_id) = network.active_node_id
            {
                self.mark_node_data_changed(node_id);
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
                scope_path: Vec::new(),
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
            && pending.old_data_json != new_data_json
        {
            self.push_command(super::undo::commands::set_node_data::SetNodeDataCommand {
                description: format!("Edit {}", pending.node_type_name),
                network_name: pending.network_name,
                scope_path: pending.scope_path,
                node_id: pending.node_id,
                node_type_name: pending.node_type_name,
                old_data_json: pending.old_data_json,
                new_data_json,
            });
        }
    }

    /// Called when a comment node text field gains focus or resize drag begins.
    /// Captures a snapshot of the comment data before editing starts.
    pub fn begin_comment_edit(&mut self, scope_path: Vec<u64>, node_id: u64) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return,
        };
        let node_type_name = match self.get_scope_network(&scope_path) {
            Some(network) => match network.nodes.get(&node_id) {
                Some(node) => node.node_type_name.clone(),
                None => return,
            },
            None => return,
        };

        if let Some(old_data_json) =
            self.snapshot_node_data_scoped(&network_name, &scope_path, node_id)
        {
            self.pending_comment_edit = Some(super::undo::snapshot::PendingGadgetDrag {
                network_name,
                scope_path,
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

        if let Some(new_data_json) = self.snapshot_node_data_scoped(
            &pending.network_name,
            &pending.scope_path,
            pending.node_id,
        ) && pending.old_data_json != new_data_json
        {
            self.push_command(super::undo::commands::set_node_data::SetNodeDataCommand {
                description: format!("Edit {}", pending.node_type_name),
                network_name: pending.network_name,
                scope_path: pending.scope_path,
                node_id: pending.node_id,
                node_type_name: pending.node_type_name,
                old_data_json: pending.old_data_json,
                new_data_json,
            });
        }
    }

    /// Set the stored body size of the HOF identified by (`scope_path`,
    /// `node_id`). Direct mutation (no undo command) — wrap a resize drag in
    /// [`begin_zone_resize`] / [`end_zone_resize`] to record a single coalesced
    /// `SetZoneSizeCommand`. Clamps to the renderer minimum. No-op for non-HOF
    /// nodes. See `doc/design_zones_ui.md` §"Resize handles".
    pub fn set_zone_size(&mut self, scope_path: &[u64], node_id: u64, width: f64, height: f64) {
        let width = width.max(100.0);
        let height = height.max(60.0);
        if let Some(network) = self.get_scope_network_mut(scope_path)
            && let Some(node) = network.nodes.get_mut(&node_id)
            && node.zone.is_some()
        {
            node.body_width = width;
            node.body_height = height;
        }
    }

    /// Called when an HOF body resize drag begins. Captures the body's current
    /// dimensions so [`end_zone_resize`] can push one coalesced command.
    pub fn begin_zone_resize(&mut self, scope_path: &[u64], node_id: u64) {
        let network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => return,
        };
        let dims = self
            .get_scope_network(scope_path)
            .and_then(|network| network.nodes.get(&node_id))
            .filter(|node| node.zone.is_some())
            .map(|node| (node.body_width, node.body_height));
        if let Some((old_width, old_height)) = dims {
            self.pending_zone_resize = Some(super::undo::snapshot::PendingZoneResize {
                network_name,
                scope_path: scope_path.to_vec(),
                node_id,
                old_width,
                old_height,
            });
        }
    }

    /// Called when an HOF body resize drag ends. Pushes a single
    /// `SetZoneSizeCommand` if the body actually changed size.
    pub fn end_zone_resize(&mut self) {
        let pending = match self.pending_zone_resize.take() {
            Some(p) => p,
            None => return,
        };
        let new_dims = self
            .get_scope_network(&pending.scope_path)
            .and_then(|network| network.nodes.get(&pending.node_id))
            .map(|node| (node.body_width, node.body_height));
        let (new_width, new_height) = match new_dims {
            Some(d) => d,
            None => return,
        };
        if new_width == pending.old_width && new_height == pending.old_height {
            return; // no-op drag; don't pollute the undo stack
        }
        self.push_command(super::undo::commands::set_zone_size::SetZoneSizeCommand {
            network_name: pending.network_name,
            scope_path: pending.scope_path,
            node_id: pending.node_id,
            old_width: pending.old_width,
            old_height: pending.old_height,
            new_width,
            new_height,
            description: "Resize HOF body".to_string(),
        });
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

        // F6 (`doc/design_parameter_wire_stability.md`): heal "Damage A" left in
        // projects saved by the `next_param_id` bug — parameter nodes that share a
        // `param_id`. Runs BEFORE the validate loop so the repair logic below sees
        // unique ids. Wires are stored positionally, so this never moves a
        // connection; it only restores parameter identity for future edits. The
        // collected messages are drained by the UI (`take_load_param_id_repairs`)
        // for a one-time "auto-repaired" modal, and echoed to the console.
        self.pending_load_param_id_repairs.clear();
        let param_id_fixes: Vec<crate::structure_designer::network_validator::ParamIdReassignment> =
            self.node_type_registry
                .node_networks
                .values_mut()
                .flat_map(|network| {
                    crate::structure_designer::network_validator::dedupe_param_ids_in_network(
                        network,
                    )
                })
                .collect();
        if !param_id_fixes.is_empty() {
            let mut repaired_networks: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            for fix in &param_id_fixes {
                let msg = format!(
                    "Network '{}': parameter '{}' (node {}) had duplicate id {} → reassigned {}",
                    fix.network_name,
                    fix.param_name,
                    fix.param_node_id,
                    fix.old_param_id,
                    fix.new_param_id
                );
                println!("[load repair] {}", msg);
                repaired_networks.insert(fix.network_name.clone());
                self.pending_load_param_id_repairs.push(msg);
            }
            println!(
                "[load repair] Auto-repaired {} duplicate parameter id(s) across {} network(s): {}. \
                 Some connections in these networks may have been mis-wired by the earlier bug and \
                 may need manual review.",
                param_id_fixes.len(),
                repaired_networks.len(),
                repaired_networks.into_iter().collect::<Vec<_>>().join(", ")
            );
        }

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
        self.pending_comment_edit = None;
        self.pending_zone_resize = None;

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
                // A network flipping valid⇄invalid (or changing its interface)
                // changes what *every* displayed node renders: `generate_scene`
                // short-circuits to `NodeOutput::None` for an invalid network
                // (and for nodes that depend on an invalid child network), so a
                // partial refresh keyed only on the edited node would leave
                // unrelated displayed nodes (e.g. a cuboid in the viewport)
                // showing stale output. Refresh paths never validate on their
                // own, so the validity change is only known here — force a Full
                // refresh so the whole active network re-evaluates.
                self.mark_full_refresh();

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
                if interface_changed
                    && let Some(ref clipboard) = self.clipboard
                    && clipboard
                        .nodes
                        .values()
                        .any(|n| n.node_type_name == current_network_name)
                {
                    self.clipboard = None;
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
        scope_path: &[u64],
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
        // Resolve the target within its scope so a body node's id (which can
        // collide with a top-level id under per-body id counters) is never
        // matched against the wrong network.
        let node_exists = self
            .get_scope_network(scope_path)
            .is_some_and(|n| n.nodes.contains_key(&node_id));
        if !node_exists {
            return Err(format!(
                "Node {} not found in network '{}'",
                node_id, network_name
            ));
        }
        if !scope_path.is_empty() {
            // A node inside an HOF zone body depends on per-iteration zone
            // inputs (element / acc) that don't exist outside an iteration, so
            // executing it in isolation is not well defined. Effect nodes
            // nested in a body still fire via the enclosing node's Execute pass.
            return Err(
                "Cannot execute a node inside a higher-order-function body directly; \
                 execute the enclosing node instead."
                    .to_string(),
            );
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
        if let Some(source_before) = source_network_before
            && let (Some(source_after), Some(subnetwork_snap)) = (
                self.snapshot_network(&network_name),
                self.snapshot_network(subnetwork_name),
            )
        {
            use super::undo::commands::factor_selection::FactorSelectionCommand;
            self.push_command(FactorSelectionCommand {
                source_network_name: network_name.clone(),
                subnetwork_name: subnetwork_name.to_string(),
                source_network_before: source_before,
                source_network_after: source_after,
                subnetwork_snapshot: subnetwork_snap,
            });
        }

        // 11. Mark dirty and schedule refresh
        self.is_dirty = true;
        self.mark_full_refresh();

        Ok(new_node_id)
    }

    /// Capture the pre-edit rendered footprints along a reflow cascade chain,
    /// starting at `node_id` in `network(scope_path)` and climbing one scope per
    /// step toward the top-level network. The returned vector is exactly the
    /// `old_sizes` slice [`reflow_for_footprint_change`] expects: index 0 is
    /// `node_id`'s own footprint, index `k` (k ≥ 1) is the footprint of the
    /// ancestor HOF `scope_path[len-k]` in its parent network.
    ///
    /// MUST be called **before** the edit that grows the node — once the edit
    /// has been applied the bodies have already grown and the *before* sizes can
    /// no longer be re-derived (the same contract reflow documents).
    pub fn capture_footprint_chain(&self, scope_path: &[u64], node_id: u64) -> Vec<DVec2> {
        use super::node_inlining::instance_size;

        let mut sizes: Vec<DVec2> = Vec::new();
        let mut path: Vec<u64> = scope_path.to_vec();
        let mut nid = node_id;
        loop {
            let Some(net) = self.get_scope_network(&path) else {
                break;
            };
            let Some(node) = net.nodes.get(&nid) else {
                break;
            };
            sizes.push(instance_size(node, &self.node_type_registry));
            if path.is_empty() {
                break;
            }
            let len = path.len();
            nid = path[len - 1];
            path.truncate(len - 1);
        }
        sizes
    }

    /// Predict which collapsable HOF nodes in `network` will flip from compact
    /// to expanded once `removed_wires` are gone — Case A of
    /// `doc/design_reflow_on_footprint_change.md`. An HOF expands when it is in
    /// [`CollapseMode::Auto`], is currently collapsed (its `f` function pin is
    /// wired, so [`resolve_body_collapsed`] is true), and the wire feeding that
    /// `f` pin is among `removed_wires` — whether the wire itself was selected
    /// for deletion, or its source node is being deleted (in which case the
    /// incident `f` wire lands in `DeletionInfo::deleted_wires`). HOFs that are
    /// themselves being deleted (`deleted_node_ids`) are excluded — there is
    /// nothing left to reflow around. Returns deduplicated HOF node ids.
    ///
    /// MUST be called on the **pre-deletion** network so the `f` wire and the
    /// currently-collapsed state are still observable.
    fn predict_f_disconnect_expansions(
        &self,
        network: &NodeNetwork,
        removed_wires: &[Wire],
        deleted_node_ids: &[u64],
    ) -> Vec<u64> {
        let mut expanding: Vec<u64> = Vec::new();
        for wire in removed_wires {
            let hof_id = wire.destination_node_id;
            if deleted_node_ids.contains(&hof_id) || expanding.contains(&hof_id) {
                continue;
            }
            let Some(node) = network.nodes.get(&hof_id) else {
                continue;
            };
            if !collapsable_type_name(&node.node_type_name)
                || node.collapse_mode != CollapseMode::Auto
            {
                continue;
            }
            let Some(node_type) = self.node_type_registry.get_node_type_for_node(node) else {
                continue;
            };
            // The removed wire must terminate at this HOF's `f` (function) pin.
            let Some(f_index) = node_type
                .parameters
                .iter()
                .position(|p| p.name == "f" && p.data_type.is_function_shape())
            else {
                continue;
            };
            if wire.destination_argument_index != f_index {
                continue;
            }
            // Auto + f currently wired ⇒ currently collapsed; removing the f wire
            // flips it to expanded. Guard on the current state for safety.
            if resolve_body_collapsed(node, node_type) {
                expanding.push(hof_id);
            }
        }
        expanding
    }

    /// One reflow step at `scope_path` for `node_id`, which has just grown in
    /// place from `old_sizes[0]`. Re-estimates the node's new rendered size; if
    /// it grew, pushes the surrounding nodes in its own network out of the way
    /// (via [`node_inlining::make_space_for_inline`]) and records the moves. If
    /// that network is itself a zone body whose own footprint grew past its
    /// stored size, the cascade recurses one scope up with the enclosing HOF as
    /// the node. Returns one [`ScopedMoves`] per scope that actually moved nodes
    /// (empty if nothing grew).
    ///
    /// CONTRACT: `node_id` MUST be a member of `network(scope_path)`. For in-body
    /// growth that has no in-place growth at `scope_path` itself (Case C in the
    /// design doc), the caller starts one scope up — passing the parent scope
    /// and the body-owning HOF as `node_id`.
    ///
    /// Every `old_sizes[k]` must be the footprint **captured before the edit**:
    /// by the time reflow runs the bodies have already grown, so the *before*
    /// sizes cannot be re-derived. `old_sizes[0]` is `node_id`'s pre-edit size;
    /// `old_sizes[k]` (k ≥ 1) is the pre-edit size of the ancestor HOF reached
    /// after the k-th cascade step. The slice need only be as long as the cascade
    /// can actually climb; a too-short slice simply stops the cascade early.
    ///
    /// This is the spatial primitive of `doc/design_reflow_on_footprint_change.md`
    /// — it only moves nodes; the caller bundles the returned moves into
    /// `MoveNodesCommand`s in the same undo step as the triggering edit.
    pub fn reflow_for_footprint_change(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        old_sizes: &[DVec2],
    ) -> Vec<ScopedMoves> {
        use super::node_inlining::{instance_size, make_space_for_inline};

        let mut out: Vec<ScopedMoves> = Vec::new();
        let mut path: Vec<u64> = scope_path.to_vec();
        let mut nid = node_id;
        let mut step = 0usize;

        loop {
            // The caller-supplied pre-edit footprint for this scope level. A
            // too-short slice stops the cascade gracefully.
            let Some(old) = old_sizes.get(step).copied() else {
                break;
            };

            // Immutable phase: estimate the grown node's new size and, only if it
            // actually grew, capture the sibling positions to diff against after
            // make_space. The registry and the resolved network are both
            // borrowed immutably here; the mutable borrow is taken below.
            let prep = {
                let Some(net) = self.get_scope_network(&path) else {
                    break;
                };
                let Some(node) = net.nodes.get(&nid) else {
                    break;
                };
                let anchor = node.position;
                let new = instance_size(node, &self.node_type_registry);
                let delta = (new - old).max(DVec2::ZERO);
                if delta.x == 0.0 && delta.y == 0.0 {
                    // Growth fully absorbed by this scope's existing slack — the
                    // cascade can climb no further.
                    None
                } else {
                    let before: Vec<(u64, DVec2)> = net
                        .nodes
                        .iter()
                        .filter(|&(&id, _)| id != nid)
                        .map(|(&id, n)| (id, n.position))
                        .collect();
                    Some((anchor, new, before))
                }
            };

            let Some((anchor, new, before)) = prep else {
                break;
            };

            // Mutable phase: make space, then diff the captured before-positions
            // against the post-move positions to build (id, old_pos, new_pos).
            let moves = {
                let Some(net) = self.get_scope_network_mut(&path) else {
                    break;
                };
                make_space_for_inline(net, nid, anchor, old, new);
                before
                    .into_iter()
                    .filter_map(|(id, old_pos)| {
                        let new_pos = net.nodes.get(&id)?.position;
                        (new_pos != old_pos).then_some((id, old_pos, new_pos))
                    })
                    .collect::<Vec<(u64, DVec2, DVec2)>>()
            };

            if !moves.is_empty() {
                out.push(ScopedMoves {
                    scope_path: path.clone(),
                    moves,
                });
            }

            if path.is_empty() {
                // Reached the top-level network — nothing further up.
                break;
            }

            // Cascade one scope up: the body `net` (owned by HOF `path.last()`)
            // grew, so that HOF grows in its parent network.
            let len = path.len();
            nid = path[len - 1];
            path.truncate(len - 1);
            step += 1;
        }

        out
    }

    /// Inlines a custom-network instance: replaces the single node `node_id`
    /// (whose `node_type_name` resolves to a user network `N`) with a copy of
    /// `N`'s contents, spliced into the parent network in place. The named
    /// definition in the registry is left untouched.
    ///
    /// Scope-aware: with an empty `scope_path` the target is the top-level
    /// active network; with a non-empty `scope_path` it is the HOF body resolved
    /// by that chain of node ids. The splice algorithm is scope-relative and
    /// identical in both cases; only target resolution and the undo command
    /// differ (a whole-network `InlineNodeCommand` at top level, an
    /// `EditZoneBodyCommand` inside a body). See `doc/design_inline_custom_node.md`.
    pub fn inline_custom_node(&mut self, scope_path: Vec<u64>, node_id: u64) -> Result<(), String> {
        use super::node_inlining;
        use super::node_type_registry::NodeTypeRegistry;

        // 1. Resolve the instance node and read what we need before mutating.
        let (type_name, anchor) = {
            let target = self
                .get_scope_network(&scope_path)
                .ok_or("Scope not found")?;
            let instance = target
                .nodes
                .get(&node_id)
                .ok_or("Node to inline not found")?;
            (instance.node_type_name.clone(), instance.position)
        };

        // 2. Gate: only custom-network instances can be inlined. Built-ins,
        //    HOFs, `apply`, and `closure` are not custom types, so this single
        //    check rejects them all.
        if !self.node_type_registry.is_custom_node_type(&type_name) {
            return Err("Only custom network nodes can be inlined".to_string());
        }

        // 3. Clone the definition N (read it while mutating the target).
        let source = self
            .node_type_registry
            .node_networks
            .get(&type_name)
            .ok_or("Custom network definition not found")?
            .clone();

        // 4. Placement geometry from N's non-parameter content.
        let (content_min, content_size) =
            node_inlining::content_bounding_box(&source, &self.node_type_registry);
        let original_size = {
            let target = self.get_scope_network(&scope_path).unwrap();
            let instance = target.nodes.get(&node_id).unwrap();
            node_inlining::instance_size(instance, &self.node_type_registry)
        };

        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or("No active network")?;

        // 5. Snapshot BEFORE the inline (for undo). Top level snapshots the whole
        //    active network; a body snapshots just the zone body (+ the owning
        //    HOF's zone_output_arguments).
        let (top_before, body_before) = if scope_path.is_empty() {
            (self.snapshot_network(&network_name), None)
        } else {
            (None, self.snapshot_zone_body(&scope_path))
        };

        // 6. Run the three helpers on the resolved target network (top-level
        //    active network or a nested body). The helpers touch no registry
        //    state, so a plain `&mut NodeNetwork` borrow suffices.
        {
            let target = self
                .get_scope_network_mut(&scope_path)
                .ok_or("Scope not found")?;
            node_inlining::make_space_for_inline(
                target,
                node_id,
                anchor,
                original_size,
                content_size,
            );
            let id_mapping = node_inlining::copy_content_into(target, &source, anchor, content_min);
            node_inlining::splice_inline_boundary(target, node_id, &source, &id_mapping);
        }

        // 7. Repopulate per-node custom-type caches for the copied content
        //    (descends into bodies, as `create_subnetwork_from_selection` does).
        //    Split-borrow walk of the scope path so it works whether the target
        //    is the top-level network or a body still living in `node_networks`.
        {
            let (built_in_types, record_type_defs, built_in_record_type_defs, node_networks) = (
                &self.node_type_registry.built_in_node_types,
                &self.node_type_registry.record_type_defs,
                &self.node_type_registry.built_in_record_type_defs,
                &mut self.node_type_registry.node_networks,
            );
            if let Some(top) = node_networks.get_mut(&network_name) {
                let mut current: Option<&mut NodeNetwork> = Some(top);
                for hof_id in &scope_path {
                    current = match current {
                        Some(net) => net.nodes.get_mut(hof_id).and_then(|n| n.zone_mut()),
                        None => None,
                    };
                }
                if let Some(target) = current {
                    NodeTypeRegistry::initialize_custom_node_types_for_network_with_types(
                        built_in_types,
                        record_type_defs,
                        built_in_record_type_defs,
                        target,
                    );
                }
            }
        }

        // 8. Validate — refresh paths do not validate. `validate_active_network`
        //    recurses into zone bodies, so body-scoped inlines are covered too.
        self.validate_active_network();

        // 9. Push undo command (scope-dependent).
        if scope_path.is_empty() {
            if let (Some(before), Some(after)) = (top_before, self.snapshot_network(&network_name))
            {
                use super::undo::commands::inline_node::InlineNodeCommand;
                self.push_command(InlineNodeCommand {
                    network_name: network_name.clone(),
                    before_snapshot: before,
                    after_snapshot: after,
                });
            }
        } else {
            self.push_zone_body_command(&scope_path, "Inline custom node".to_string(), body_before);
        }

        // 10. Mark dirty and schedule refresh.
        self.is_dirty = true;
        self.mark_full_refresh();

        Ok(())
    }

    /// Whether the node at `(scope_path, node_id)` can be converted to a closure
    /// (*Network → Closure*): it must be a custom-network instance, used as a
    /// function (no wire consumes a normal output pin) or unconsumed, whose
    /// definition has a return node. Cheap, side-effect-free — gates the
    /// context-menu item. See `doc/design_closure_network_conversion.md`.
    pub fn can_convert_instance_to_closure(&self, scope_path: &[u64], node_id: u64) -> bool {
        use super::closure_network_conversion as conv;

        let Some(target) = self.get_scope_network(scope_path) else {
            return false;
        };
        let Some(instance) = target.nodes.get(&node_id) else {
            return false;
        };
        // Gate 1: only custom-network instances (rejects built-ins, HOFs,
        // `apply`, `closure`).
        if !self
            .node_type_registry
            .is_custom_node_type(&instance.node_type_name)
        {
            return false;
        }
        // Gate 2: used as a function, not a value.
        if conv::node_consumed_as_value(target, node_id) {
            return false;
        }
        // Gate 3: the definition must have a return node.
        match self
            .node_type_registry
            .node_networks
            .get(&instance.node_type_name)
        {
            Some(source) => source.return_node_id.is_some(),
            None => false,
        }
    }

    /// Whether the node at `(scope_path, node_id)` can be extracted to a network
    /// (*Closure → Network*): it must be a `closure` node with a result wire.
    /// Cheap, side-effect-free — gates the context-menu item. See
    /// `doc/design_closure_network_conversion.md` (Direction B). (The
    /// secondary-output-pin rejection is checked at extraction time and surfaced
    /// as an error message rather than hidden from the menu.)
    pub fn can_extract_closure_to_network(&self, scope_path: &[u64], node_id: u64) -> bool {
        let Some(target) = self.get_scope_network(scope_path) else {
            return false;
        };
        let Some(c) = target.nodes.get(&node_id) else {
            return false;
        };
        if c.node_type_name != "closure" {
            return false;
        }
        // The closure must deliver a result.
        c.zone_output_arguments
            .first()
            .is_some_and(|arg| !arg.incoming_wires.is_empty())
    }

    /// Converts a custom-network instance node into a `closure` node
    /// (*Network → Closure*): replaces the instance `I` (whose function pin is
    /// used, or which is unconsumed) with a `closure` node `C` whose inline body
    /// is a copy of `I`'s network `N`. `I`'s **wired** input pins become
    /// **captures** in the body; its **unwired** input pins become the closure's
    /// **parameters**. `I` and `C` expose the same `Function` value — `I` on its
    /// function pin (`-1`), `C` on its primary output pin (`0`) — so the only
    /// externally-visible change is flipping consuming wires `-1 → 0`. The named
    /// definition `N` is left untouched in the registry.
    ///
    /// Works both at the top level (`scope_path` empty) and inside a zone body
    /// (`scope_path` = `[parent.., hof_id]` down to the body holding `I`). See
    /// `doc/design_closure_network_conversion.md` (Direction A).
    pub fn convert_instance_to_closure(
        &mut self,
        scope_path: Vec<u64>,
        node_id: u64,
    ) -> Result<(), String> {
        use super::closure_network_conversion as conv;
        use super::node_inlining;

        let scoped = !scope_path.is_empty();

        // 1. Resolve the instance node; read its type name.
        let type_name = {
            let target = self
                .get_scope_network(&scope_path)
                .ok_or("Scope not found")?;
            let instance = target
                .nodes
                .get(&node_id)
                .ok_or("Node to convert not found")?;
            instance.node_type_name.clone()
        };

        // 2. Gate: only custom-network instances can be converted. Built-ins,
        //    HOFs, `apply`, and `closure` are not custom types.
        if !self.node_type_registry.is_custom_node_type(&type_name) {
            return Err("Only custom node instances can be converted to a closure".to_string());
        }

        // 3. Gate: `I` must be used as a function, not a value — no wire anywhere
        //    consumes its normal output pins (index >= 0). Consumers of its `-1`
        //    pin (or no consumers at all) are fine.
        {
            let target = self.get_scope_network(&scope_path).unwrap();
            if conv::node_consumed_as_value(target, node_id) {
                return Err(
                    "This node is used as a value, not a function; only a node consumed \
                    through its function pin can be converted to a closure"
                        .to_string(),
                );
            }
        }

        // 4. Clone the definition N (read it while mutating the host).
        let source = self
            .node_type_registry
            .node_networks
            .get(&type_name)
            .ok_or("Custom network definition not found")?
            .clone();

        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or("No active network")?;

        // 5. Snapshot BEFORE the conversion (for undo): the whole host network at
        //    top level, or the host body (and its owner HOF's zone-output wires)
        //    when inside a zone body.
        let before_top = if scoped {
            None
        } else {
            self.snapshot_network(&network_name)
        };
        let before_body = if scoped {
            self.snapshot_zone_body(&scope_path)
        } else {
            None
        };

        // Case C residual cascade (doc/design_reflow_on_footprint_change.md):
        // inside a body, the closure `C` renders far larger than the instance it
        // replaces. The make-space in step 7 reflows `C`'s neighbours within the
        // body scope, but the body itself can grow past its stored size — growing
        // the owning HOF in the parent network and cascading up. Capture the
        // owning HOF's pre-edit footprint chain now (before step 7's growth);
        // the ancestor reflow runs at push time below. (Top-level conversion has
        // no body to grow, so no ancestor cascade.)
        let old_ancestor_sizes = if scoped {
            self.capture_body_owner_footprint_chain(&scope_path)
        } else {
            Vec::new()
        };

        // 6. Build the closure node `C` (reads N, registry).
        let closure_node = {
            let target = self.get_scope_network(&scope_path).unwrap();
            let instance = target.nodes.get(&node_id).unwrap();
            conv::build_closure_from_instance(instance, &source, &self.node_type_registry)?
        };

        // 6b. Placement geometry for make-space (immutable registry borrow, taken
        //     before the mutable target borrow below). `C` renders far larger
        //     than the instance it replaces — its body shows the inlined network,
        //     including nested zone nodes — so the lower-right region must be
        //     pushed out or `C` overlaps its neighbours (e.g. a downstream
        //     `collect`). The closure's size is measured from its actual body
        //     content (`instance_size` → `rendered_body_size`), not its flat
        //     `DEFAULT_BODY_*` placeholder.
        let closure_size = node_inlining::instance_size(&closure_node, &self.node_type_registry);
        let (anchor, original_size) = {
            let target = self.get_scope_network(&scope_path).unwrap();
            let instance = target.nodes.get(&node_id).unwrap();
            (
                instance.position,
                node_inlining::instance_size(instance, &self.node_type_registry),
            )
        };

        // 7. Replace `I` with `C` (same id), make room for the larger closure,
        //    redirect `-1` consumers to pin `0`, and drop any stale display state
        //    (C's pin 0 is a Function — no viewport output).
        {
            let target = self
                .get_scope_network_mut(&scope_path)
                .ok_or("Scope not found")?;
            target.nodes.insert(node_id, closure_node);
            target.displayed_nodes.remove(&node_id);
            node_inlining::make_space_for_inline(
                target,
                node_id,
                anchor,
                original_size,
                closure_size,
            );
            conv::redirect_function_consumers(target, node_id);
        }

        // 8. Repopulate per-node custom-type caches: the host's top-level network
        //    covers both `C` and `B`'s interior (`B` lives inside `C`). Use the
        //    split-borrow static variant (consults only the read-only type maps)
        //    to avoid a registry borrow conflict, as `inline_custom_node` does.
        //    The re-init resets every `apply` / `map` consumer to its bare
        //    `calculate_custom_node_type` default, erasing the post-pass-derived
        //    arg-pin names; the *preserving-args* post-passes below re-derive
        //    those layouts without rebuilding the arguments vector, so the
        //    `validate_active_network` post-pass that follows is a no-op and the
        //    arg wires survive (otherwise the by-name rebuild drops them).
        {
            let (built_in_types, record_type_defs, built_in_record_type_defs, node_networks) = (
                &self.node_type_registry.built_in_node_types,
                &self.node_type_registry.record_type_defs,
                &self.node_type_registry.built_in_record_type_defs,
                &mut self.node_type_registry.node_networks,
            );
            if let Some(top) = node_networks.get_mut(&network_name) {
                NodeTypeRegistry::initialize_custom_node_types_for_network_with_types(
                    built_in_types,
                    record_type_defs,
                    built_in_record_type_defs,
                    top,
                );
            }
        }
        if let Some(top) = self.node_type_registry.node_networks.remove(&network_name) {
            let mut top = top;
            self.node_type_registry
                .update_apply_pin_layouts_for_network_preserving_args(&mut top);
            self.node_type_registry
                .update_map_pin_layouts_for_network_preserving_args(&mut top);
            self.node_type_registry
                .update_zip_with_pin_layouts_for_network_preserving_args(&mut top);
            self.node_type_registry
                .node_networks
                .insert(network_name.clone(), top);
        }

        // 9. Validate — refresh paths do not validate. Validating the active
        //    network walks its whole body tree, so a body-scoped `C`/`B` is
        //    covered too.
        self.validate_active_network();

        // 10. Push undo command: a whole-network before/after snapshot at top
        //     level, or an `EditZoneBodyCommand` (whole-body snapshot) when the
        //     host scope is a zone body.
        if scoped {
            self.push_zone_body_command_with_ancestor_reflow(
                &scope_path,
                "Convert to closure".to_string(),
                before_body,
                &old_ancestor_sizes,
            );
        } else if let (Some(before), Some(after)) =
            (before_top, self.snapshot_network(&network_name))
        {
            use super::undo::commands::convert_to_closure::ConvertToClosureCommand;
            self.push_command(ConvertToClosureCommand {
                network_name: network_name.clone(),
                before_snapshot: before,
                after_snapshot: after,
            });
        }

        // 11. Mark dirty and schedule refresh.
        self.is_dirty = true;
        self.mark_full_refresh();

        Ok(())
    }

    /// Extracts a `closure` node into a new named custom network
    /// (*Closure → Network*): lifts the closure `C`'s inline body `B` into a
    /// fresh standalone network `N` — with `parameter` nodes for both the
    /// closure's parameters and its captures — and replaces `C` with an instance
    /// `I` of `N`, wired so `I`'s function value (`-1` pin) reproduces `C`'s. The
    /// closure's parameters become `I`'s **unwired** pins (the `-1` value's
    /// parameters); its captures become `I`'s **wired** pins (capture sources).
    /// Consumers of `C`'s pin `0` are flipped to `I`'s pin `-1` (same node id).
    ///
    /// Works both at the top level (`scope_path` empty) and inside a zone body
    /// (`scope_path` = `[parent.., hof_id]` down to the body holding `C`); a
    /// body-scope extraction collects captures across the full ancestor chain so
    /// captures reaching above the host scope (`e >= 1`) resolve. See
    /// `doc/design_closure_network_conversion.md` (Direction B). Returns the
    /// instance node id (equal to `node_id`).
    pub fn extract_closure_to_network(
        &mut self,
        scope_path: Vec<u64>,
        node_id: u64,
        network_name: &str,
    ) -> Result<u64, String> {
        use super::closure_network_conversion as conv;
        use super::node_network::{Argument, Node};

        let scoped = !scope_path.is_empty();

        // 1. Validate the network name (relaxed user-name rules) and uniqueness.
        if let Err(reason) = super::identifier::is_valid_user_name(network_name) {
            return Err(format!("Invalid network name: {}", reason));
        }
        if self.node_type_registry.name_is_taken(network_name) {
            return Err(format!("Node type '{}' already exists", network_name));
        }

        // 2. Resolve `C`; gate: only `closure` nodes can be extracted.
        {
            let target = self
                .get_scope_network(&scope_path)
                .ok_or("Scope not found")?;
            let c = target
                .nodes
                .get(&node_id)
                .ok_or("Node to extract not found")?;
            if c.node_type_name != "closure" {
                return Err("Only closure nodes can be extracted to a network".to_string());
            }
        }

        let host_name = self
            .active_node_network_name
            .clone()
            .ok_or("No active network")?;

        // 3. Snapshot BEFORE the extraction (for undo): the whole host network at
        //    top level, or the host body when inside a zone body.
        let before_top = if scoped {
            None
        } else {
            self.snapshot_network(&host_name)
        };
        let before_body = if scoped {
            self.snapshot_zone_body(&scope_path)
        } else {
            None
        };

        // 4. Build the extraction plan (reads `C` + the host ancestor chain,
        //    builds `N` with its interior caches populated). `host_ancestors` is
        //    `[H, parent, …, top]`: `H` first (external level 0), each enclosing
        //    scope above it — so captures reaching above `H` (`e >= 1`) resolve.
        //    At top level this is just `[H]`.
        let plan = {
            let (ancestors, _hof_ids) = self
                .get_scope_ancestors(&scope_path)
                .ok_or("Scope not found")?;
            let target = self.get_scope_network(&scope_path).unwrap();
            let mut host_ancestors: Vec<&NodeNetwork> = Vec::with_capacity(ancestors.len() + 1);
            host_ancestors.push(target);
            host_ancestors.extend(ancestors.iter().rev().copied());
            let c = target.nodes.get(&node_id).unwrap();
            conv::extract_network_from_closure(
                c,
                network_name,
                &host_ancestors,
                &self.node_type_registry,
            )?
        };
        let conv::ExtractionPlan {
            network: new_network,
            capture_wires,
            closure_param_count,
        } = plan;

        // Read `C`'s geometry/name and its declared node type before mutating.
        let (custom_name, position, body_width, body_height, collapse_mode) = {
            let target = self.get_scope_network(&scope_path).unwrap();
            let c = target.nodes.get(&node_id).unwrap();
            (
                c.custom_name.clone(),
                c.position,
                c.body_width,
                c.body_height,
                c.collapse_mode,
            )
        };
        let i_node_type = new_network.node_type.clone();
        let param_count = closure_param_count + capture_wires.len();

        // 5. Register `N` (its content caches are already populated).
        self.node_type_registry.add_node_network(new_network);

        // 6. Replace `C` with the instance `I` (same id + position): closure-param
        //    pins (0..m) stay unwired; capture pins (m..) carry the capture wires.
        //    Then flip consumers of `C`'s pin `0` to `I`'s function pin `-1`, and
        //    drop any stale display state.
        {
            let arguments: Vec<Argument> = (0..param_count)
                .map(|i| {
                    let mut arg = Argument::new();
                    if i >= closure_param_count {
                        arg.incoming_wires = vec![capture_wires[i - closure_param_count].clone()];
                    }
                    arg
                })
                .collect();

            let instance = Node {
                id: node_id,
                node_type_name: network_name.to_string(),
                custom_name,
                position,
                arguments,
                data: Box::new(CustomNodeData::default()),
                custom_node_type: Some(i_node_type),
                zone: None,
                zone_output_arguments: Vec::new(),
                body_width,
                body_height,
                collapse_mode,
            };

            let target = self
                .get_scope_network_mut(&scope_path)
                .ok_or("Scope not found")?;
            target.nodes.insert(node_id, instance);
            target.displayed_nodes.remove(&node_id);
            conv::redirect_value_consumers(target, node_id);
        }

        // 7. Validate `H` (revalidates `I` against the now-registered `N`).
        //    Refresh paths do not validate. Validating the active network walks
        //    its whole body tree, so a body-scoped `I` is covered too.
        self.validate_active_network();

        // 8. Push undo command. At top level reuse `FactorSelectionCommand`
        //    (adds/removes the subnetwork, restores the source by name). Inside a
        //    zone body use `ExtractClosureBodyCommand` — the same add/remove of
        //    `N`, but the host is restored from a `ZoneBodySnapshot`.
        if scoped {
            if let (Some(body_before), Some(body_after), Some(subnetwork_snap)) = (
                before_body,
                self.snapshot_zone_body(&scope_path),
                self.snapshot_network(network_name),
            ) {
                use super::undo::commands::extract_closure_body::ExtractClosureBodyCommand;
                self.push_command(ExtractClosureBodyCommand {
                    network_name: host_name.clone(),
                    subnetwork_name: network_name.to_string(),
                    subnetwork_snapshot: subnetwork_snap,
                    scope_path: scope_path.clone(),
                    body_before,
                    body_after,
                });
            }
        } else if let (Some(source_before), Some(source_after), Some(subnetwork_snap)) = (
            before_top,
            self.snapshot_network(&host_name),
            self.snapshot_network(network_name),
        ) {
            use super::undo::commands::factor_selection::FactorSelectionCommand;
            self.push_command(FactorSelectionCommand {
                source_network_name: host_name.clone(),
                subnetwork_name: network_name.to_string(),
                source_network_before: source_before,
                source_network_after: source_after,
                subnetwork_snapshot: subnetwork_snap,
            });
        }

        // 9. Mark dirty and schedule refresh.
        self.is_dirty = true;
        self.mark_full_refresh();

        Ok(node_id)
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
        if let Some(network) = node_networks.get_mut(&network_name)
            && let Some(node) = network.nodes.get_mut(&new_id)
        {
            NodeTypeRegistry::populate_custom_node_type_cache_with_types(
                built_in_types,
                record_type_defs,
                built_in_record_type_defs,
                node,
                true,
            );
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
    ///
    /// The name is prefixed with the namespace (folder path) of the active
    /// network, so a subnetwork factored out of `Foo.Bar.Baz` is suggested as
    /// `Foo.Bar.subnetwork1`. This keeps the new subnetwork in the same folder
    /// as the network it was extracted from.
    fn generate_unique_subnetwork_name(&self) -> String {
        // Derive the folder path (everything up to and including the last '.')
        // of the active network, if any.
        let prefix = self
            .active_node_network_name
            .as_deref()
            .and_then(|name| name.rsplit_once('.'))
            .map(|(namespace, _)| format!("{}.", namespace))
            .unwrap_or_default();

        let base = "subnetwork";
        let mut counter = 1;
        loop {
            let name = format!("{}{}{}", prefix, base, counter);
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

/// Recursively clear selection on every network in the tree rooted at
/// `network` **except** the one addressed by `keep_scope_path` (relative to
/// `network`). The kept network keeps its own selection; every other network —
/// the ancestors along the path and all bodies off the path — is cleared.
/// Backs [`StructureDesigner::clear_selection_in_other_scopes`].
/// Logical-pixel floor for a body node's top-left inside a zone body. Mirrors
/// the Flutter drag-clamp floor `_ZONE_BODY_DRAG_INSET` in
/// `structure_designer_model.dart`: a body's interior origin is `(0, 0)` and it
/// grows right/down but not up/left, so body content must stay at or beyond
/// this inset to remain inside the visible rect. Keep the two values in sync.
const ZONE_BODY_CONTENT_INSET: f64 = 8.0;

/// Shift every node in `network` right/down by the smallest non-negative delta
/// that brings the top-left-most node to `inset` on each axis. A no-op when the
/// content already clears the inset (it never moves content left/up). Used by
/// scoped paste to keep freshly-pasted body content inside the body rect (the
/// body has no leftward/upward growth). The whole body moves rigidly so
/// relative node layout — and wires — are preserved.
fn shift_body_content_inside(network: &mut NodeNetwork, inset: f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    for node in network.nodes.values() {
        if node.position.x < min_x {
            min_x = node.position.x;
        }
        if node.position.y < min_y {
            min_y = node.position.y;
        }
    }
    if !min_x.is_finite() || !min_y.is_finite() {
        return; // empty body — nothing to shift
    }
    let delta = DVec2::new((inset - min_x).max(0.0), (inset - min_y).max(0.0));
    if delta == DVec2::ZERO {
        return;
    }
    for node in network.nodes.values_mut() {
        node.position += delta;
    }
}

/// Depth-first search for the first network (including any zone body, at any
/// depth) whose `selected_node_ids` is non-empty. `prefix` accumulates the
/// chain of HOF node ids walked into; on a hit it holds the scope path of the
/// selected network. Returns `None` if no network has a selection. See
/// [`StructureDesigner::find_selection_scope`].
fn find_selection_scope_recursive(
    network: &NodeNetwork,
    prefix: &mut Vec<u64>,
) -> Option<Vec<u64>> {
    if !network.selected_node_ids.is_empty() {
        return Some(prefix.clone());
    }
    for (id, node) in network.nodes.iter() {
        if let Some(zone) = node.zone.as_ref() {
            prefix.push(*id);
            if let Some(found) = find_selection_scope_recursive(zone, prefix) {
                return Some(found);
            }
            prefix.pop();
        }
    }
    None
}

fn clear_selection_except_recursive(network: &mut NodeNetwork, keep_scope_path: &[u64]) {
    match keep_scope_path.split_first() {
        None => {
            // `network` is the kept scope: preserve its own selection, but
            // clear any selection in its descendant bodies (defensive — the
            // invariant means at most one scope is ever populated).
            for node in network.nodes.values_mut() {
                if let Some(zone) = node.zone_mut() {
                    clear_selection_recursive(zone);
                }
            }
        }
        Some((head, tail)) => {
            // `network` is an ancestor of the kept scope: clear its own
            // selection, descend into the named child, clear every other body.
            network.clear_selection();
            for (id, node) in network.nodes.iter_mut() {
                if let Some(zone) = node.zone_mut() {
                    if *id == *head {
                        clear_selection_except_recursive(zone, tail);
                    } else {
                        clear_selection_recursive(zone);
                    }
                }
            }
        }
    }
}

/// The neighbour moves [`StructureDesigner::reflow_for_footprint_change`]
/// applied in one network. Each entry maps directly onto a `MoveNodesCommand`:
/// `scope_path` is the network the moves apply to (resolved on undo/redo via
/// `UndoContext::network_in_scope_mut`), and `moves` is `(id, old_pos, new_pos)`
/// for every neighbour that actually shifted (the grown node itself is never
/// listed). See `doc/design_reflow_on_footprint_change.md`.
#[derive(Debug, Clone)]
pub struct ScopedMoves {
    pub scope_path: Vec<u64>,
    pub moves: Vec<(u64, DVec2, DVec2)>,
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
