use crate::data_type::DataType;
use crate::declared_inputs::DeclaredInputs;
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::NetworkResult;
use crate::node_jobs::{JobInputs, JobWork};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeType;
use crate::node_type_registry::{NodeTypeRegistry, RecordDefs};
use crate::overlay::Overlay;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use crate::trace::{TraceContext, TraceProbe, TraceStep};
use atomcad_crystolecule::unit_cell_struct::UnitCellStruct;
use atomcad_util::as_any::AsAny;
use atomcad_util::memory_size_estimator::MemorySizeEstimator;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::{HashMap, HashSet};

/// What [`NodeData::build_record_node_type`] does when a def the node needs is
/// not in the [`RecordDefs`] it is given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingDefs {
    /// A live node: a missing def gives an empty (or dangling-row) layout. A
    /// def under an unloaded library mount never gets here: the node is frozen
    /// instead.
    Dangle,
    /// A frozen node being rebuilt from a recorded interface: return `None`.
    Refuse,
}

/// Direction the user dragged a wire from when opening the add-node popup.
/// Drives `NodeData::adapt_for_drag_source`: an adapter must arrange for an
/// input pin (`FromOutput`) or the output pin (`FromInput`) of the new node
/// to be compatible with the source pin type. See
/// `doc/design_drag_aware_add_node.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragDirection {
    /// User dragged from an *output* pin of `source_type`.
    /// Adapter must arrange for at least one of this node's *input* pins to accept it.
    FromOutput,
    /// User dragged from an *input* pin of `source_type`.
    /// Adapter must arrange for this node's *output* (pin 0) to satisfy it.
    FromInput,
}

/// Return type for `NodeData::eval()`. Wraps one or more results, one per output pin.
#[derive(Clone)]
pub struct EvalOutput {
    pub results: Vec<NetworkResult>,
    /// Optional per-pin display overrides. When present for a pin index,
    /// the scene generator uses this value for viewport rendering instead
    /// of results[index]. Downstream wire evaluation always uses results[index].
    pub display_results: HashMap<usize, NetworkResult>,
    /// Explicit unit cell override. When set, the scene generator uses this
    /// instead of extracting from the primary result. Used by motif_edit where
    /// the wire value is Motif (which doesn't carry a UnitCellStruct).
    pub unit_cell_override: Option<UnitCellStruct>,
    /// Optional per-pin subtitle overrides for the node-graph UI. When
    /// present for a pin index, the evaluator stamps this string into
    /// `context.node_output_strings` instead of the default
    /// `result.to_display_string()`. Used by `collect` to surface a count
    /// like "(5 elements)" / "(stopped at limit 100)" in place of the raw
    /// array dump.
    pub pin_subtitles: HashMap<usize, String>,
    /// Line work this node wants drawn **about** its output — a tool's
    /// collision envelope, and whatever the next [`OverlayKind`] is. The scene
    /// generator copies it onto `NodeSceneData` and the tessellator draws each
    /// kind iff that kind's preference is on, so a node emits its segments
    /// unconditionally and no preference ever reaches an evaluation
    /// (`crate::overlay`).
    ///
    /// [`OverlayKind`]: crate::overlay::OverlayKind
    pub overlays: Vec<Overlay>,
}

impl EvalOutput {
    /// Convenience for single-output nodes.
    pub fn single(result: NetworkResult) -> Self {
        EvalOutput {
            results: vec![result],
            display_results: HashMap::new(),
            unit_cell_override: None,
            pin_subtitles: HashMap::new(),
            overlays: Vec::new(),
        }
    }

    /// Multi-output constructor.
    pub fn multi(results: Vec<NetworkResult>) -> Self {
        EvalOutput {
            results,
            display_results: HashMap::new(),
            unit_cell_override: None,
            pin_subtitles: HashMap::new(),
            overlays: Vec::new(),
        }
    }

    /// Get result for a given output pin index (wire value for downstream nodes).
    pub fn get(&self, output_pin_index: i32) -> NetworkResult {
        self.results
            .get(output_pin_index as usize)
            .cloned()
            .unwrap_or(NetworkResult::None)
    }

    /// Get the primary (pin 0) result.
    pub fn primary(&self) -> &NetworkResult {
        &self.results[0]
    }

    /// Set a display override for a specific output pin.
    pub fn set_display_override(&mut self, pin_index: usize, result: NetworkResult) {
        self.display_results.insert(pin_index, result);
    }

    /// Get the display result for a pin, falling back to the wire result.
    pub fn get_display(&self, pin_index: usize) -> NetworkResult {
        self.display_results
            .get(&pin_index)
            .cloned()
            .unwrap_or_else(|| self.get(pin_index as i32))
    }

    /// Does any result carried here contain a lazy `Iter[T]` walker?
    ///
    /// The per-pass evaluation memo (`doc/design_eval_memoization.md` D4 / D6
    /// R4) stores a whole `EvalOutput` under one key, so the exclusion has to
    /// be asked of the whole output, not of one pin. `display_results` counts:
    /// it is a second full payload, not a view of `results`.
    pub fn contains_iterator(&self) -> bool {
        self.results.iter().any(NetworkResult::contains_iterator)
            || self
                .display_results
                .values()
                .any(NetworkResult::contains_iterator)
    }
}

/// Sizes the value type the evaluation memo stores (D2: the *whole*
/// `EvalOutput`, not one pin's `NetworkResult`).
///
/// `display_results` is counted **deeply**: a decorated structure is a second
/// full payload sitting beside the wire value, not a view of it, and on an
/// `atom_edit` root it is the larger of the two.
impl MemorySizeEstimator for EvalOutput {
    fn estimate_memory_bytes(&self) -> usize {
        let results_size = self.results.capacity() * std::mem::size_of::<NetworkResult>()
            + self
                .results
                .iter()
                .map(NetworkResult::heap_bytes)
                .sum::<usize>();

        // `HashMap` has no public capacity-in-buckets figure; `len` times the
        // entry size is the honest approximation, and undercounting the table's
        // slack is the direction D6 asks for.
        let display_results_size = self
            .display_results
            .values()
            .map(|value| {
                std::mem::size_of::<usize>()
                    + std::mem::size_of::<NetworkResult>()
                    + value.heap_bytes()
            })
            .sum::<usize>();

        let pin_subtitles_size = self
            .pin_subtitles
            .values()
            .map(|s| std::mem::size_of::<usize>() + std::mem::size_of::<String>() + s.capacity())
            .sum::<usize>();

        let overlays_size = self.overlays.capacity() * std::mem::size_of::<Overlay>()
            + self.overlays.iter().map(Overlay::heap_bytes).sum::<usize>();

        // `unit_cell_override` is an inline `Option<UnitCellStruct>` — plain
        // data, already covered by `size_of::<EvalOutput>()`.
        std::mem::size_of::<EvalOutput>()
            + results_size
            + display_results_size
            + pin_subtitles_size
            + overlays_size
    }
}

/// Forward `Clone for Box<dyn NodeData>` to `clone_box`. This makes `Node` and
/// `NodeNetwork` derivable as `Clone`, which the iterator runtime relies on: a
/// `ZoneClosure` (carried by `Walker::MapZone`/`FilterZone` and by
/// `NetworkResult::Function`) holds an `Arc<NodeNetwork>` body, and the CoW
/// `Node::zone_mut` path clones a `NodeNetwork` on first mutation. Cloning a
/// `NetworkResult::Iterator(_)` must remain cheap and independent (Invariant 2
/// in `doc/design_iterators.md`); the `ZoneClosure` is entirely `Arc`-backed so
/// that holds. The wrapped `dyn NodeData` is local to this crate, so this
/// `impl` does not run afoul of orphan rules.
impl Clone for Box<dyn NodeData> {
    fn clone(&self) -> Box<dyn NodeData> {
        self.clone_box()
    }
}

/// A stored-data problem reported by [`NodeData::get_data_error`], which the
/// validator turns into a `ValidationError` attributed to the node.
///
/// `blocking` follows the same litmus as every other validation rule (see
/// `structure_designer/AGENTS.md`, "Validation errors: blocking vs
/// non-blocking vs interface"): *is the node's output still useful?* A `motif`
/// whose definition failed to parse has no motif to emit, so it blocks (and is
/// cone-poisoned); `materialize` / `motif_sub` merely lose their per-element
/// overrides and keep producing atoms, so they warn.
pub struct NodeDataError {
    pub message: String,
    pub blocking: bool,
}

impl NodeDataError {
    /// The node's output is unavailable — cone-poisons the node (D3).
    pub fn blocking(message: String) -> Self {
        Self {
            message,
            blocking: true,
        }
    }

    /// Advisory — the node keeps evaluating and producing a useful value.
    pub fn warning(message: String) -> Self {
        Self {
            message,
            blocking: false,
        }
    }
}

pub trait NodeData: Any + AsAny {
    fn provide_gadget(
        &self,
        structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>>;

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType>;

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput;

    // Method to clone the trait object
    fn clone_box(&self) -> Box<dyn NodeData>;

    // Method to provide an optional subtitle for the node
    // connected_input_pins contains the names of input pins that are connected
    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String>;

    /// Returns the properties to serialize for text format output.
    ///
    /// Keys are property names as they appear in text format.
    /// These should match parameter names where applicable.
    /// Only returns properties that have stored values (not input-only params).
    ///
    /// Default implementation returns an empty list.
    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![]
    }

    /// Updates node data from parsed text properties.
    ///
    /// Only properties present in the map are updated.
    /// Returns error if a property value has wrong type or is invalid.
    ///
    /// Default implementation does nothing and returns Ok.
    fn set_text_properties(&mut self, _props: &HashMap<String, TextValue>) -> Result<(), String> {
        Ok(())
    }

    /// [`get_text_properties`](Self::get_text_properties) for a node whose
    /// text form depends on the record defs — `record_get` prints its path
    /// rows by their **current** field names, which only the defs know. The
    /// text format (serializer and editor) calls this one. Default: the
    /// defs-free version.
    fn get_text_properties_with_defs(&self, _defs: RecordDefs<'_>) -> Vec<(String, TextValue)> {
        self.get_text_properties()
    }

    /// [`set_text_properties`](Self::set_text_properties) for a node that has
    /// to resolve what it is given against the record defs (`record_get`'s
    /// path rows arrive as names). The text-format editor calls this one.
    /// Default: the defs-free version.
    fn set_text_properties_with_defs(
        &mut self,
        props: &HashMap<String, TextValue>,
        _defs: RecordDefs<'_>,
    ) -> Result<(), String> {
        self.set_text_properties(props)
    }

    /// Pin names the text format accepts a stored literal under although
    /// [`get_text_properties_with_defs`](Self::get_text_properties_with_defs)
    /// lists no value for them yet — a `record_set` row with no literal. The
    /// editor treats these like listed properties: known keys, and a literal
    /// on such a pin replaces its wire. Default: none.
    fn text_literal_keys_with_defs(&self, _defs: RecordDefs<'_>) -> Vec<String> {
        Vec::new()
    }

    /// Clears any cached input data used for interactive editing performance.
    /// Called by the refresh system before evaluation when upstream may have changed.
    /// Default implementation does nothing.
    fn clear_input_cache(&self) {}

    /// The data files this node reads **when it is loaded** and caches — its
    /// stored path, as authored (a relative one resolves against the folder
    /// of the file that owns the node). Library linking watches them
    /// (`doc/design_library_linking.md` D7) and, when one changes on disk,
    /// refreshes the node by running its data loader again, which re-reads
    /// the file and keeps every user-set field. A new node that caches file
    /// content at load time overrides this one method. Default: none.
    fn file_paths(&self) -> Vec<String> {
        Vec::new()
    }

    /// Rewrites every file path this node **stores** (the ones it reads —
    /// [`file_paths`](Self::file_paths) — and the ones it writes) through
    /// `rebase`, which returns the new spelling or `None` to keep a path.
    /// *Make local copy* uses it when a library's networks become the
    /// design's own: a relative path authored against the library's folder
    /// must keep naming the same file from the design's folder
    /// (`doc/design_library_linking.md` §10). The cached content read from the
    /// file stays valid — it is the same file. Default: nothing stored.
    fn rebase_file_paths(&mut self, _rebase: &dyn Fn(&str) -> Option<String>) {}

    /// The input pins through which a **wired** file path can reach this
    /// node. Such a path is only known at evaluation time, so it is not a
    /// dependency the Save As copy can see (`file_dependencies`, D8); the
    /// dialog says so when one of these pins is wired. Default: none.
    fn file_path_pins(&self) -> &'static [&'static str] {
        &[]
    }

    /// Called when this data is about to **replace** `previous` on the same
    /// node (`NodeNetwork::set_node_network_data`: a panel setter, and the
    /// undo/redo of one, which rebuilds the data from its JSON snapshot).
    /// Lets a node keep runtime-only, `#[serde(skip)]` state that a JSON
    /// round trip would otherwise drop.
    ///
    /// Only for state that is **safe to keep whatever the new settings are** —
    /// `chemisorb`'s stored search, which is keyed by an input fingerprint and
    /// so can never be output for inputs it was not computed from. Default:
    /// nothing is inherited.
    fn inherit_runtime_state(&mut self, _previous: &dyn NodeData) {}

    /// A run-on-demand node's explicit action (`doc/design_background_node_jobs.md`):
    /// evaluate what it needs through `inputs` and return the work to do off
    /// the UI thread, owning everything it reads. `None` = this node has no
    /// job; `Some(Err)` = it has one but cannot run now (broken inputs or
    /// settings), with a message for the user.
    ///
    /// The result the work produces must be stored keyed by a fingerprint of
    /// every input and setting (D3) — that is what makes installing it safe
    /// whatever was edited while it ran. Default: no job.
    fn prepare_job(&self, _inputs: &mut JobInputs) -> Option<Result<Box<dyn JobWork>, String>> {
        None
    }

    /// Returns metadata for input parameters that cannot be derived from get_text_properties().
    ///
    /// This is used by the `describe` command to provide accurate information about inputs.
    /// Maps parameter name -> (is_required, default_description).
    ///
    /// - If a parameter has a matching property in get_text_properties(), no entry is needed here
    /// - If a parameter is required (must be wired), return (true, None)
    /// - If a parameter has a hardcoded default constant, return (false, Some("description"))
    ///
    /// Default implementation returns empty HashMap (assumes all non-property parameters are required).
    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        HashMap::new()
    }

    /// A problem in this node's **stored data** — typically a parse error in a
    /// definition string the user typed — that the validator should surface as
    /// a `ValidationError` on every validate pass.
    ///
    /// Nodes like `motif` / `motif_sub` / `materialize` parse a stored text
    /// definition when their data is set (`parse_and_validate`) and keep the
    /// failure in a `#[serde(skip)]` `error` field. Before this hook that
    /// failure reached the user only through the node's own eval/badge — it was
    /// a third, ad-hoc error channel invisible to the panel's error list and to
    /// the F8 cycle. Reporting it here folds it into the one unified list
    /// (`doc/design_error_management.md` D9), and because the validator asks on
    /// every pass there is no transient `initial_errors` plumbing to keep in
    /// sync.
    ///
    /// `connected_input_pins` carries the *names* of this node's wired input
    /// parameters, exactly as [`NodeData::get_subtitle`] receives them, so a
    /// rule may depend on whether a value is wired: `isosurface` warns that its
    /// `level` pin is ignored in auto mode. It is the only wiring context
    /// available here — the node's own data and its incoming wires, nothing
    /// upstream and no evaluated value.
    ///
    /// Default: `None` — a node with no stored-data failure mode.
    fn get_data_error(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<NodeDataError> {
        None
    }

    /// Adapt this node's stored data so its pins line up with a dragged source pin.
    ///
    /// Called by the drag-aware add-node popup when this node's static-pin
    /// signature did not already match. The implementation typically clones
    /// `self`, overwrites any type properties (e.g. `MapData::input_type`),
    /// and returns the new boxed `NodeData`. Returning `None` means the node
    /// cannot be adapted to accept this drag — the candidate is silently
    /// skipped.
    ///
    /// Default: returns `None`. Drag filtering and create-time plumbing fall
    /// back to a static-pin check for nodes that don't override this — the
    /// same path as before this hook existed.
    ///
    /// See `doc/design_drag_aware_add_node.md`.
    fn adapt_for_drag_source(
        &self,
        _source_type: &DataType,
        _direction: DragDirection,
        _registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        None
    }

    /// The node's declared inputs, when its input list is its own data. `None`
    /// (the default) means the node's inputs are fixed by its type or by a def.
    /// May depend on the data. `Some` is also what gives the node the open
    /// port (the `+` tab shown during a wire drag) and *Take type from wire*;
    /// see `crate::declared_inputs` and `doc/design_wire_declared_inputs.md`.
    fn declared_inputs(&self) -> Option<&dyn DeclaredInputs> {
        None
    }

    /// Mutable counterpart of [`Self::declared_inputs`]; must answer `Some`
    /// exactly when it does.
    fn declared_inputs_mut(&mut self) -> Option<&mut dyn DeclaredInputs> {
        None
    }

    /// A concrete type hint for dragging a wire *off* the given input pin,
    /// overriding the pin's declared type for drag-aware add-node inference.
    ///
    /// Needed when a pin's declared type is deliberately lossy: `map.f` is
    /// `AnyFunction { leading_params: [element] }`, which carries the parameter
    /// prefix but not the return type. `MapData` overrides this to return the
    /// natural concrete signature `Function([input_type], output_type)` so a
    /// `closure` dropped from `map.f` reflects the map's output type exactly,
    /// rather than defaulting the return to the element type.
    ///
    /// Default: `None` — the pin's declared type drives drag inference, which
    /// is correct for every pin whose declared type is already concrete (e.g.
    /// `filter.f` / `fold.f` / `foreach.f`). See
    /// `doc/design_drag_aware_add_node.md`.
    fn drag_hint_for_input_pin(&self, _pin_index: usize) -> Option<DataType> {
        None
    }

    // --- Record nodes (`doc/design_record_get_set.md`, "Registry and repair
    // integration"). A record node is one whose pins come from a record def
    // rather than from its own data alone. Every site that needs to know about
    // record nodes goes through these hooks; no site lists them by name.

    /// The record def the node's pins are built on (`schema` / `target`),
    /// possibly the empty string when none is chosen yet. `None` (the
    /// default) means "not a record node".
    fn record_schema_ref(&self) -> Option<&str> {
        None
    }

    /// Rewrites the schema name (record-def rename, namespace move,
    /// cross-document paste). Called only when `record_schema_ref` is `Some`.
    fn set_record_schema_ref(&mut self, _name: String) {}

    /// Builds the node's pins from `defs`. `None` for a non-record node, and
    /// for a record node only under [`MissingDefs::Refuse`] when a def it needs
    /// is missing; the caller then keeps whatever layout the node has.
    fn build_record_node_type(
        &self,
        _base: &NodeType,
        _defs: RecordDefs<'_>,
        _missing: MissingDefs,
    ) -> Option<NodeType> {
        None
    }

    /// The record fields the node declares in its own data — an anonymous
    /// `record_construct`'s — with the allocator floor of their ids. `None`
    /// (the default) for every other node. A named-mode `record_construct`
    /// answers `Some` with no fields, so the invariant checker sees a node
    /// that has both. The record-def reference walkers
    /// (`collect_record_refs_in_node`, `rewrite_record_names_in_node`,
    /// `canonicalize`) read the field types through this hook, never by
    /// downcast.
    fn local_record_fields(
        &self,
    ) -> Option<(&[crate::nodes::record_construct::LocalRecordField], u64)> {
        None
    }

    /// Mutable counterpart of [`Self::local_record_fields`], for rewriting and
    /// canonicalizing the field types (which never allocates an id).
    fn local_record_fields_mut(
        &mut self,
    ) -> Option<&mut [crate::nodes::record_construct::LocalRecordField]> {
        None
    }

    /// Per-node warnings that need the defs to compute (one per dangling
    /// path row, or per local field type naming a missing def).
    fn record_node_problems(&self, _defs: RecordDefs<'_>) -> Vec<String> {
        Vec::new()
    }

    /// The names of the defs, other than the schema, that the node's rows walk
    /// into — including a missing one, where a row's walk stops. Read by
    /// `library_links::unresolved_mount_ref` so that a nested def under a
    /// missing mount freezes the node. Only path-based record nodes return any.
    fn record_path_defs(&self, _defs: RecordDefs<'_>) -> Vec<String> {
        Vec::new()
    }

    /// Whether a freshly-added instance of this node should display *all* of its
    /// output pins by default (eye on), instead of just pin 0 (the global
    /// default in `NodeDisplayState`).
    ///
    /// Multi-output nodes normally show only pin 0 so that e.g. `atom_edit`
    /// doesn't draw both `result` and `diff` at once. But for the stateless
    /// destructure nodes (`structure_unpack`, `lattice_vecs_unpack`,
    /// `lattice_vecs_params`) every output is a scalar/structure value that
    /// draws *no* viewport geometry (it only feeds the per-pin hover readout),
    /// so showing them all by default lets users inspect every unpacked value
    /// immediately with no clutter. See
    /// `doc/design_structure_lattice_unpack_nodes.md`.
    ///
    /// Default: `false` (pin 0 only).
    fn default_display_all_output_pins(&self) -> bool {
        false
    }

    /// Which output pins a freshly-placed instance of this node should display,
    /// when the global default (pin 0) is the wrong one.
    ///
    /// [`NodeData::default_display_all_output_pins`] is the *all pins* answer,
    /// meant for the destructure nodes whose outputs draw no geometry. This is
    /// the general one: an explicit set. Both `mechanosynth` nodes answer
    /// `{2}` — `result` and the base part of `scene` are the same atoms, so
    /// showing both draws the workpiece twice, and the scene is what a user
    /// scrubbing a build with tools wants to look at (and what the editor has
    /// to be looking at to author a recharge). See
    /// `doc/design_mechanosynth_tools.md`.
    ///
    /// Two things follow it besides `add_node`: the serializer omits a pin set
    /// that equals *this* default rather than a hard-coded `{0}`, so a saved
    /// default round-trips and a file that chose `{0}` explicitly keeps it.
    ///
    /// Default: `None` — pin 0, the global default.
    fn default_displayed_output_pins(&self) -> Option<HashSet<i32>> {
        None
    }

    /// Output-driven editing (`doc/design_output_driven_editing.md`): `probe` is
    /// something in this node's output pin `pin` (in element `element` of it,
    /// when the pin's value is an array). Say what this node did to it and where
    /// in the inputs it came from. Read inputs and outputs through `ctx`.
    ///
    /// `None` = "I cannot answer this": the node does not trace at all, or it
    /// does not handle this probe kind on this pin. Either way the trace engine
    /// falls back to "possibly contributed" and continues through every
    /// phase-typed input.
    ///
    /// May read the node's **persisted** data, never runtime caches: subnetwork
    /// node data is shared by every call site.
    fn trace(
        &self,
        _pin: usize,
        _element: Option<usize>,
        _probe: &TraceProbe,
        _ctx: &TraceContext,
    ) -> Option<TraceStep> {
        None
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NoData {}

impl NodeData for NoData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        _network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        EvalOutput::single(NetworkResult::Error(format!(
            "eval not implemented for node {}",
            node.node_type_name
        )))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(NoData {})
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        None
    }
}

/// Node data for custom nodes (user-defined node networks).
///
/// Custom nodes can store literal values for their parameters, which are
/// evaluated when no wire is connected to the corresponding input.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CustomNodeData {
    /// Literal values for parameters, keyed by parameter name.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub literal_values: HashMap<String, TextValue>,
}

impl NodeData for CustomNodeData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        _network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        EvalOutput::single(NetworkResult::Error(format!(
            "eval not implemented for node {}",
            node.node_type_name
        )))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        None
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        // Return stored literal values for serialization
        self.literal_values
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        for (k, v) in props {
            self.literal_values.insert(k.clone(), v.clone());
        }
        Ok(())
    }
}
