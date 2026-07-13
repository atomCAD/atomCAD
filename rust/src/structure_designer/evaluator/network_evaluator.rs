use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

use crate::api::structure_designer::structure_designer_preferences::GeometryVisualization;
use crate::api::structure_designer::structure_designer_preferences::GeometryVisualizationPreferences;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::display::csg_to_poly_mesh::convert_csg_mesh_to_poly_mesh;
use crate::display::csg_to_poly_mesh::convert_csg_sketch_to_poly_mesh;
use crate::geo_tree::GeoNode;
use crate::geo_tree::csg_cache::CsgConversionCache;
use crate::structure_designer::common_constants::ARRAY_DISPLAY_CAP;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::structure_designer::evaluator::zone_closure::{
    build_node_function_closure, run_closure_once,
};
use crate::structure_designer::implicit_eval::surface_splatting_2d::generate_2d_point_cloud;
use crate::structure_designer::implicit_eval::surface_splatting_3d::generate_point_cloud;
use crate::structure_designer::node_data::EvalOutput;
use crate::structure_designer::node_network::{
    IncomingWire, Node, NodeDisplayType, NodeNetwork, NodeRef, SourcePin,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::nodes::facet_shell::FacetShellData;
use crate::structure_designer::structure_designer_scene::{
    DisplayedPinOutput, NodeOutput, NodeSceneData,
};

use super::network_result::input_missing_error;
use super::network_result::{
    Alignment, BlueprintData, GeometrySummary2D, propagate_alignment_with_reason,
};
use crate::crystolecule::structure::Structure;
use crate::util::transform::Transform2D;
use glam::f64::DVec2;

#[derive(Clone)]
pub struct NetworkStackElement<'a> {
    pub node_network: &'a NodeNetwork,
    pub node_id: u64,
}

impl<'a> NetworkStackElement<'a> {
    pub fn get_top_node(network_stack: &[NetworkStackElement<'a>], node_id: u64) -> &'a Node {
        network_stack
            .last()
            .unwrap()
            .node_network
            .nodes
            .get(&node_id)
            .unwrap()
    }

    pub fn is_node_selected_in_root_network(
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
    ) -> bool {
        network_stack
            .first()
            .unwrap()
            .node_network
            .is_node_selected(node_id)
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct NodeInvocationId {
    root_network_name: String,
    node_id_stack: Vec<u64>,
}

/// Identifies the **source side** of a pre-evaluated capture wire. Wires whose
/// source is anywhere outside the destination body are captures — evaluated
/// once at body entry, cached, and reused unchanged for every iteration. Multiple
/// body-internal wires consuming the same upstream pin share one cache entry,
/// so the key projects only the source-side fields of `IncomingWire`.
///
/// Phase 3 lands the type but no HOF populates the cache yet, so the lookup
/// path is exercised only by tests.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CaptureKey {
    pub source_node_id: u64,
    pub source_scope_depth: u8,
    pub source_pin: SourcePin,
}

impl CaptureKey {
    /// Project an `IncomingWire` onto its source-side identity. Used both when
    /// pre-evaluating captures (insert) and from `evaluate_arg`'s capture-cache
    /// short-circuit (lookup).
    pub fn from_incoming(incoming: &IncomingWire) -> Self {
        Self {
            source_node_id: incoming.source_node_id,
            source_scope_depth: incoming.source_scope_depth,
            source_pin: incoming.source_pin,
        }
    }
}

/// Build a fresh empty captures map, wrapped in `Arc`. Non-zone evaluation
/// contexts use this for the `captured_source_values` field; it's one tiny
/// allocation per context construction.
///
/// (`NetworkResult` is not `Sync` — it contains `Box<dyn NodeData>` through
/// `ZoneClosure` and `Walker` — so a `static LazyLock<Arc<…>>` shared sentinel
/// can't be expressed. The per-context allocation is cheap enough that the
/// "share one empty allocation" optimization from the design doc isn't worth
/// chasing.)
///
/// `Arc` (rather than `Rc`) matches the design doc and the surrounding
/// convention in the evaluator (`Walker::FromArray` already carries
/// `Arc<Vec<NetworkResult>>`). The evaluator is single-threaded in practice
/// — `NetworkResult` cannot cross threads — so the `arc_with_non_send_sync`
/// clippy lint is suppressed here.
#[allow(clippy::arc_with_non_send_sync)]
fn empty_captures() -> Arc<HashMap<CaptureKey, NetworkResult>> {
    Arc::new(HashMap::new())
}

/// One entry in the per-pass print log. Produced by the `print` node (Phase 4)
/// and by any future node that wants to surface text to the in-app Console
/// panel. The field shape lands now (Phase 2) so the eval layer can carry the
/// buffer through `Walker` / zone-closure propagation without later
/// re-touching every signature. See `doc/design_node_execution.md` (Console
/// panel section).
#[derive(Debug, Clone)]
pub struct PrintLogEntry {
    pub timestamp: SystemTime,
    pub network_name: String,
    pub node_id: u64,
    pub node_label: String,
    pub text: String,
    /// True when the entry was produced under `context.execute == true`
    /// (an explicit Execute pass), false for normal display passes.
    pub from_execute: bool,
}

pub struct NetworkEvaluationContext {
    /// Per-node evaluation errors, keyed by the node's **scope-aware** address
    /// ([`NodeRef`]). Body-internal node ids share the top-level id space
    /// (per-body `next_node_id` counters), so a bare-`u64` key would let a body
    /// node and a top-level node with the same id clobber each other within one
    /// pass. The scope path is maintained on `eval_scope_path` (pushed on HOF
    /// body / custom-network entry).
    pub node_errors: HashMap<NodeRef, String>,
    /// Per-node, per-pin output display strings (the "hover values" shown on
    /// output pins), keyed by scope-aware [`NodeRef`] for the same reason as
    /// `node_errors`.
    pub node_output_strings: HashMap<NodeRef, Vec<String>>,
    /// The scope path of the node currently being evaluated — the chain of HOF
    /// body-owner ids (and custom-network instance ids) from the displayed
    /// top-level node down to the current frame. Maintained by
    /// [`Self::push_eval_scope`] / [`Self::pop_eval_scope`] at every body /
    /// custom-network boundary, and combined with the node id to form the
    /// `NodeRef` keys above. Empty = top-level.
    pub eval_scope_path: Vec<u64>,
    pub selected_node_eval_cache: Option<Box<dyn Any>>,
    pub top_level_parameters: HashMap<String, NetworkResult>,
    /// Whether to use spatial grid cutoff for vdW interactions during minimization.
    pub use_vdw_cutoff: bool,
    /// When `true`, side-effect nodes (`export_atoms`, `print` with
    /// `execute_only`, future effect nodes) actually perform their effect
    /// during this evaluation pass. Set to `true` only when the user
    /// triggers an explicit Execute action; `false` for all normal display
    /// / scene-generation evaluations. The flag is consulted in exactly one
    /// place in the evaluator — the central skip rule in
    /// `evaluate_all_outputs` — plus the `print` node's per-node opt-in.
    /// Flows into inner-body evaluations: the lazy zone walkers
    /// (`MapZone`/`FilterZone`) run the body against this same context, and the
    /// eager HOFs (`fold`/`foreach`) copy it into a `fresh_inner_for_eager_body`
    /// context — so effects nested inside `map`/`filter`/`fold`/`foreach`
    /// chains fire correctly under Execute. See
    /// `doc/design_node_execution.md` (Phase 2).
    pub execute: bool,
    /// Per-pass print buffer. Each `print` node `eval` (Phase 4) appends to
    /// this; the orchestrator drains it into `StructureDesigner.print_log`
    /// at end-of-pass via the `with_eval_context` helper. The lazy zone walkers
    /// run bodies against this same context directly; the eager HOFs use a
    /// `fresh_inner_for_eager_body` context and `drain_inner_context` its
    /// buffer back here at end-of-call, so prints from inner bodies aggregate
    /// into the single per-pass log either way.
    pub print_buffer: Vec<PrintLogEntry>,
    /// HOF id ↦ stack of per-iteration frames (each frame is one
    /// `Vec<NetworkResult>` indexed by zone-input pin). Reads always consult
    /// the top frame (`last()`) — the innermost iterating HOF with that id.
    /// The stack shape is load-bearing because `next_node_id` is per-network,
    /// so an outer HOF in one network and an inner HOF in another network can
    /// share a numeric id; without scope-stacking, a lazy walker for an inner
    /// HOF would silently overwrite an outer's iteration value when its
    /// `next()` runs. All push/pop/read access goes through the helper
    /// methods on `NetworkEvaluationContext` so the discipline can't be
    /// circumvented. See `doc/design_zones.md` (§"What's new" point 3).
    ///
    /// Phase 3 lands the field; no node populates it yet, so it stays empty
    /// in every existing code path.
    pub current_zone_input_values: HashMap<u64, Vec<Vec<NetworkResult>>>,
    /// Pre-evaluated captures for the currently-active zone body. Populated
    /// by the HOF's `eval` at body entry (once per HOF invocation) and
    /// consulted from `evaluate_arg` ahead of the per-`SourcePin` dispatch so
    /// captured upstream values are read from cache rather than re-evaluated
    /// per iteration. `Arc`-shared so the lazy-walker per-`next()` swap is
    /// three pointer-sized ops (`std::mem::replace` + `Arc::clone`) instead
    /// of a HashMap clone. Non-zone evaluation contexts share one empty
    /// allocation via `EMPTY_CAPTURES`. See `doc/design_zones.md`
    /// (§"Capture pre-evaluation", §"Sub-context pattern for body
    /// evaluation").
    ///
    /// Phase 3 lands the field; no HOF builds a captures map yet, so the
    /// lookup path always misses in existing code.
    pub captured_source_values: Arc<HashMap<CaptureKey, NetworkResult>>,
}

impl Default for NetworkEvaluationContext {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkEvaluationContext {
    /// Construct a fresh evaluation context. `execute` defaults to `false`
    /// (normal display passes); set to `true` only for explicit Execute
    /// actions.
    ///
    /// In production code paths inside `rust/src/structure_designer/`, the
    /// only legitimate `::new()` caller is `StructureDesigner::with_eval_context`.
    /// The eager HOFs (`fold`/`foreach`) build their per-iteration body context
    /// via `fresh_inner_for_eager_body` (a struct literal, outside this `::new()`
    /// audit) and drain it back with `drain_inner_context`; the old
    /// `FunctionEvaluator::evaluate` inner-body caller was removed in closures
    /// Phase 2. Tests are exempt.
    pub fn new() -> Self {
        Self {
            node_errors: HashMap::new(),
            node_output_strings: HashMap::new(),
            eval_scope_path: Vec::new(),
            selected_node_eval_cache: None,
            top_level_parameters: HashMap::new(),
            use_vdw_cutoff: false,
            execute: false,
            print_buffer: Vec::new(),
            current_zone_input_values: HashMap::new(),
            captured_source_values: empty_captures(),
        }
    }

    /// Push a fresh iteration frame onto the HOF's zone-input scope-stack.
    /// Called by an HOF's `eval` (eager) or its `Walker::next` (lazy) at the
    /// start of each iteration; **must** be balanced by `pop_zone_input_frame`
    /// along every exit path including early-return on error. The debug
    /// invariant records the new depth so a missing pop is caught at first
    /// occurrence rather than as silent corruption a few iterations later.
    ///
    /// Phase 3 lands the helper; no HOF calls it yet.
    pub fn push_zone_input_frame(&mut self, hof_id: u64, frame: Vec<NetworkResult>) {
        self.current_zone_input_values
            .entry(hof_id)
            .or_default()
            .push(frame);
    }

    /// Pop the top iteration frame from the HOF's zone-input scope-stack.
    /// Debug-panics if the stack is empty (would indicate a missing push or
    /// a double-pop).
    pub fn pop_zone_input_frame(&mut self, hof_id: u64) {
        match self.current_zone_input_values.get_mut(&hof_id) {
            Some(stack) => {
                let popped = stack.pop();
                debug_assert!(
                    popped.is_some(),
                    "pop_zone_input_frame: stack for HOF id {} is empty",
                    hof_id,
                );
                if stack.is_empty() {
                    // Keep the map tight so the common case (no active HOF)
                    // never leaves a stale empty Vec around.
                    self.current_zone_input_values.remove(&hof_id);
                }
            }
            None => {
                debug_assert!(
                    false,
                    "pop_zone_input_frame: no entry for HOF id {}",
                    hof_id,
                );
            }
        }
    }

    /// Read the `pin_index`-th value of the top iteration frame for `hof_id`.
    /// Debug-panics if no frame is active for this HOF — `evaluate_arg`
    /// reaches this path only from a body-internal wire whose source is the
    /// enclosing HOF's zone-input pin, which by construction means a frame
    /// has been pushed.
    pub fn current_zone_input(&self, hof_id: u64, pin_index: usize) -> &NetworkResult {
        let stack = self
            .current_zone_input_values
            .get(&hof_id)
            .unwrap_or_else(|| {
                panic!(
                    "current_zone_input: no scope-stack entry for HOF id {}",
                    hof_id
                )
            });
        let frame = stack.last().unwrap_or_else(|| {
            panic!(
                "current_zone_input: scope-stack for HOF id {} is empty",
                hof_id
            )
        });
        frame.get(pin_index).unwrap_or_else(|| {
            panic!(
                "current_zone_input: pin_index {} out of range for HOF id {} frame of len {}",
                pin_index,
                hof_id,
                frame.len()
            )
        })
    }

    /// Build an inner context for an eager HOF body's iterations
    /// (`fold`, `foreach`). Inherit-vs-fresh policy:
    ///
    /// **Inherited from the caller:**
    /// - `execute`, `use_vdw_cutoff` — effects nested inside the body must
    ///   see the same flags as the outer pass.
    /// - `current_zone_input_values` — ancestor HOFs' scope-stacks come along
    ///   intact; the inner body will push its own frame on top at iteration
    ///   start and pop at iteration end.
    ///
    /// **Fresh:**
    /// - `captured_source_values` — the inner body's captures are
    ///   pre-evaluated into a separate map by its `eval` and sealed onto the
    ///   inner context afterward; until that point the inner context shares
    ///   the empty sentinel.
    /// - `node_errors`, `node_output_strings`, `selected_node_eval_cache`,
    ///   `top_level_parameters` — per-pass scratch state, scoped to the body.
    /// - `print_buffer` — drained back into the outer context at end of call
    ///   via [`drain_inner_context`] so prints emitted from inside the body
    ///   aggregate into the single per-pass log.
    ///
    /// Phase 3 lands the helper; eager HOFs (`fold`, `foreach`) call it in
    /// Phase 5. See `doc/design_zones.md` (§"Sub-context pattern for body
    /// evaluation").
    pub fn fresh_inner_for_eager_body(&self) -> Self {
        Self {
            node_errors: HashMap::new(),
            node_output_strings: HashMap::new(),
            // The body's nodes are scoped under the same path as the eager HOF
            // that is iterating them — carry it through so any strings/errors
            // they record are keyed consistently (the eager body's own
            // `run_closure_once` push appends the HOF id on top of this).
            eval_scope_path: self.eval_scope_path.clone(),
            selected_node_eval_cache: None,
            top_level_parameters: HashMap::new(),
            use_vdw_cutoff: self.use_vdw_cutoff,
            execute: self.execute,
            print_buffer: Vec::new(),
            current_zone_input_values: self.current_zone_input_values.clone(),
            captured_source_values: empty_captures(),
        }
    }

    /// Drain an eager-body inner context back into this (the outer) context.
    /// Matches the policy of [`fresh_inner_for_eager_body`]: prints are
    /// aggregated; per-pass scratch state and the inner context's
    /// `current_zone_input_values` are dropped (the outer context's
    /// scope-stacks are unaffected by the inner body's push/pop cycle, which
    /// happens entirely on `inner.current_zone_input_values`).
    pub fn drain_inner_context(&mut self, mut inner: NetworkEvaluationContext) {
        self.print_buffer.append(&mut inner.print_buffer);
    }

    /// Mutate the `pin_index`-th value of the top iteration frame for
    /// `hof_id`. Convenient for `fold`'s acc-then-element per-step update
    /// (the frame is per-call, not per-step, so the top frame's slots are
    /// rewritten rather than a new frame pushed each iteration). Debug-panics
    /// if no frame is active or `pin_index` is out of range.
    pub fn write_zone_input_pin(&mut self, hof_id: u64, pin_index: usize, value: NetworkResult) {
        let stack = self
            .current_zone_input_values
            .get_mut(&hof_id)
            .unwrap_or_else(|| {
                panic!(
                    "write_zone_input_pin: no scope-stack entry for HOF id {}",
                    hof_id
                )
            });
        let frame = stack.last_mut().unwrap_or_else(|| {
            panic!(
                "write_zone_input_pin: scope-stack for HOF id {} is empty",
                hof_id
            )
        });
        let frame_len = frame.len();
        let slot = frame.get_mut(pin_index).unwrap_or_else(|| {
            panic!(
                "write_zone_input_pin: pin_index {} out of range for HOF id {} frame of len {}",
                pin_index, hof_id, frame_len,
            )
        });
        *slot = value;
    }

    /// Enter a nested scope (an HOF/closure body or a custom-network instance)
    /// owned by `owner_node_id`. Subsequent `node_errors` / `node_output_strings`
    /// inserts are keyed under the extended path until the matching
    /// [`Self::pop_eval_scope`]. Always pair push/pop (run_closure_once and the
    /// custom-network branches bracket their recursion with them).
    pub fn push_eval_scope(&mut self, owner_node_id: u64) {
        self.eval_scope_path.push(owner_node_id);
    }

    /// Leave the most recently entered scope. See [`Self::push_eval_scope`].
    pub fn pop_eval_scope(&mut self) {
        self.eval_scope_path.pop();
    }

    /// The scope-aware address of `node_id` at the current evaluation scope.
    pub fn node_ref(&self, node_id: u64) -> NodeRef {
        NodeRef::scoped(&self.eval_scope_path, node_id)
    }
}

/// RAII guard that swaps a fresh `Arc<HashMap<CaptureKey, NetworkResult>>`
/// into a context's `captured_source_values` for the duration of a body step,
/// restoring the caller's previous value on drop. Used by lazy walkers
/// (`Walker::Map`/`Walker::Filter`) so each `next()` runs against the
/// caller's context with the walker's captures visible — without paying a
/// HashMap clone per element. Both sides share the same `Arc<HashMap<…>>`
/// type, so swap is three pointer-sized ops:
/// 1. `std::mem::replace` saves the caller's Arc and installs the walker's
///    Arc.
/// 2. On Drop, `std::mem::replace` puts the caller's Arc back and replaces
///    the saved slot with the shared empty sentinel (so the guard's storage
///    doesn't keep the caller's Arc alive past the restore).
///
/// Phase 3 lands the type; lazy walkers in later phases use it.
pub struct CapturesGuard<'a> {
    ctx: &'a mut NetworkEvaluationContext,
    saved: Arc<HashMap<CaptureKey, NetworkResult>>,
}

impl<'a> CapturesGuard<'a> {
    /// Install `new` into `ctx.captured_source_values` and return a guard
    /// that will restore the previous value on drop. The caller passes an
    /// already-`Arc<…>`-wrapped captures map (typically `Arc::clone(...)` of
    /// the walker's stored `captures`) so the swap is a refcount bump.
    pub fn swap_in(
        ctx: &'a mut NetworkEvaluationContext,
        new: Arc<HashMap<CaptureKey, NetworkResult>>,
    ) -> Self {
        let saved = std::mem::replace(&mut ctx.captured_source_values, new);
        Self { ctx, saved }
    }
}

impl Drop for CapturesGuard<'_> {
    fn drop(&mut self) {
        // Swap the saved Arc back into the context. `self.saved` then holds
        // the installed (walker's) Arc, which drops naturally when the guard
        // itself is dropped a moment later.
        std::mem::swap(&mut self.ctx.captured_source_values, &mut self.saved);
    }
}

pub struct NetworkEvaluator {
    csg_conversion_cache: CsgConversionCache,
}

/*
 * Node network evaluator.
 * The node network evaluator is able to generate displayable representation for a node in a node network.
 * It delegates node related evaluation to functions in node specific modules.
 */
impl Default for NetworkEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkEvaluator {
    pub fn new() -> Self {
        Self {
            csg_conversion_cache: CsgConversionCache::with_defaults(),
        }
    }

    /// Clear the CSG conversion cache
    pub fn clear_csg_cache(&mut self) {
        self.csg_conversion_cache.clear();
    }

    /// Get cache statistics
    pub fn get_csg_cache_stats(&self) -> crate::geo_tree::csg_cache::CacheStats {
        self.csg_conversion_cache.stats()
    }

    // Creates the Scene that will be displayed for the given node by the Renderer, and is retained
    // for interaction purposes.
    //
    // The caller passes in a `NetworkEvaluationContext` so that per-pass state
    // (`execute`, `print_buffer`, etc.) is set up at a higher level and drained
    // consistently. In production code, `StructureDesigner::with_eval_context`
    // is the only legitimate caller-side construction site — see
    // `doc/design_node_execution.md` (Centralized drain).
    #[allow(clippy::too_many_arguments)]
    pub fn generate_scene(
        &mut self,
        network_name: &str,
        node_id: u64,
        _display_type: NodeDisplayType, //TODO: use display_type
        registry: &NodeTypeRegistry,
        geometry_visualization_preferences: &GeometryVisualizationPreferences,
        context: &mut NetworkEvaluationContext,
    ) -> NodeSceneData {
        //let _timer = Timer::new("generate_scene");

        let network = match registry.node_networks.get(network_name) {
            Some(network) => network,
            None => return NodeSceneData::new(NodeOutput::None),
        };

        // Do not evaluate invalid networks
        if !network.valid {
            return NodeSceneData::new(NodeOutput::None);
        }

        // Reset per-call scratch fields — `generate_scene` is invoked once per
        // displayed node within a single shared `context`, but each
        // `NodeSceneData` should reflect only its own evaluation's errors,
        // output strings, and selected-node eval cache. Per-pass state that
        // *should* aggregate across calls (`print_buffer`, `execute`,
        // `use_vdw_cutoff`, `top_level_parameters`) is intentionally not
        // touched.
        context.node_errors.clear();
        context.node_output_strings.clear();
        // `generate_scene` always starts a displayed node's evaluation at the
        // top level; the scope path is rebuilt by the push/pop bracketing as
        // the pass descends into bodies / custom networks. Clear defensively in
        // case a prior pass left it unbalanced.
        context.eval_scope_path.clear();
        context.selected_node_eval_cache = None;

        // We assign the root node network zero node id. It is not used in the evaluation.
        let network_stack = vec![NetworkStackElement {
            node_network: network,
            node_id: 0,
        }];

        let node = match network.nodes.get(&node_id) {
            Some(node) => node,
            None => return NodeSceneData::new(NodeOutput::None),
        };

        // Function mode (doc/design_function_pins.md §"Display in function
        // mode"): a node whose function pin is consumed (wired into an HOF `f`
        // or `apply.f`) is a function definition, not a value source — it emits
        // no viewport output. This overrides the display policy, so a
        // function-mode node selected under the Selected/Frontier policy (or
        // with its pin-0 eye on) still renders nothing. Derived from the wire
        // graph — no stored mutation, so disconnecting `f` restores its prior
        // display for free.
        if network.function_pin_consumed(node_id) {
            return NodeSceneData::new(NodeOutput::None);
        }

        let from_selected_node = network_stack
            .last()
            .unwrap()
            .node_network
            .is_node_selected(node_id);

        // Evaluate all outputs once (avoids redundant evaluation for multi-output nodes)
        let eval_output = {
            //let _timer = Timer::new("evaluate inside generate_scene");
            self.evaluate_all_outputs(
                &network_stack,
                node_id,
                registry,
                from_selected_node,
                context,
            )
        };

        // A node whose displayed pin output is `Iter[T]` produces no
        // viewport output — materialization is the consumer's job. To
        // preview elements of a stream, wire it into a `collect` node and
        // display that. See `doc/design_iter_display_via_collect.md`.

        // Get the unit cell: prefer explicit override (e.g. motif_edit), else extract from primary
        let unit_cell = eval_output
            .unit_cell_override
            .clone()
            .or_else(|| eval_output.primary().get_unit_cell());

        // Convert primary result (pin 0) to NodeOutput for backward compat.
        // Use display override if present (e.g., motif_edit shows Atomic in viewport
        // while the wire carries Motif).
        let node_type = registry.get_node_type_for_node(node).unwrap();
        let (display_result_0, display_type_0) =
            if let Some(dr) = eval_output.display_results.get(&0) {
                let dt = dr
                    .infer_data_type()
                    .unwrap_or_else(|| node_type.output_type().clone());
                (dr.clone(), dt)
            } else {
                // Infer from the concrete result first so that pins declared
                // as `SameAsInput(..)` (for which `output_type()` falls
                // through to `DataType::None`) still map to the right
                // NodeOutput variant. Fall back to the declared type when the
                // result is None/Error/etc.
                let result = eval_output.get(0);
                let dt = result
                    .infer_data_type()
                    .unwrap_or_else(|| node_type.output_type().clone());
                (result, dt)
            };
        // Capture alignment from the wire-level result for pin 0 (not the
        // display override, which may be an unrelated phase — e.g. motif_edit
        // shows Atomic in the viewport while the wire carries Motif).
        let pin_0_alignment = eval_output.get(0).get_alignment();
        let pin_0_alignment_reason = eval_output
            .get(0)
            .get_alignment_reason()
            .map(|s| s.to_string());
        let (output, geo_tree) = self.convert_result_to_node_output(
            display_result_0,
            &display_type_0,
            from_selected_node,
            &network_stack,
            node_id,
            registry,
            context,
            geometry_visualization_preferences,
        );

        // Build pin_outputs for ALL output pins (not just displayed ones).
        // This makes NodeSceneData cache-safe: pin display can be toggled
        // without re-evaluation. displayed_outputs() filters at render time.
        let pin_count = node_type.output_pin_count();
        let mut pin_outputs = Vec::with_capacity(pin_count);
        for pin_index_usize in 0..pin_count {
            let pin_index = pin_index_usize as i32;
            if pin_index == 0 {
                // Pin 0's actual data lives in NodeSceneData.output / .geo_tree.
                // displayed_outputs() resolves pin 0 from those fields. Alignment
                // is still tracked here so the API layer can surface it per pin.
                pin_outputs.push(DisplayedPinOutput {
                    pin_index: 0,
                    output: NodeOutput::None,
                    geo_tree: None,
                    alignment: pin_0_alignment,
                    alignment_reason: pin_0_alignment_reason.clone(),
                });
                continue;
            }
            let (pin_result, pin_data_type) =
                if let Some(dr) = eval_output.display_results.get(&pin_index_usize) {
                    let dt = dr
                        .infer_data_type()
                        .unwrap_or_else(|| node_type.get_output_pin_type(pin_index));
                    (dr.clone(), dt)
                } else {
                    let result = eval_output.get(pin_index);
                    let dt = result
                        .infer_data_type()
                        .unwrap_or_else(|| node_type.get_output_pin_type(pin_index));
                    (result, dt)
                };
            // Wire-level alignment (same rationale as pin 0 above).
            let pin_alignment = eval_output.get(pin_index).get_alignment();
            let pin_alignment_reason = eval_output
                .get(pin_index)
                .get_alignment_reason()
                .map(|s| s.to_string());
            let (pin_output, pin_geo_tree) = self.convert_result_to_node_output(
                pin_result,
                &pin_data_type,
                from_selected_node,
                &network_stack,
                node_id,
                registry,
                context,
                geometry_visualization_preferences,
            );
            pin_outputs.push(DisplayedPinOutput {
                pin_index,
                output: pin_output,
                geo_tree: pin_geo_tree,
                alignment: pin_alignment,
                alignment_reason: pin_alignment_reason,
            });
        }

        // Get current displayed pins from the network
        let displayed_pins = network
            .get_displayed_pins(node_id)
            .cloned()
            .unwrap_or_else(|| HashSet::from([0]));

        // Capture the construction plane of the interactive (lowest-displayed)
        // pin from the wire-level result, so the view-up "from displayed plane"
        // action (issue #349) can read it even though the display-level
        // NodeOutput drops it. Uses the wire-level result (not display override)
        // for the same reason the alignment capture above does.
        let construction_plane = displayed_pins
            .iter()
            .copied()
            .min()
            .and_then(|pin| eval_output.get(pin).construction_plane());

        // Show unit cell wireframe when eval explicitly provided a unit cell
        // (motif_edit sets unit_cell_override; other nodes don't)
        let show_unit_cell_wireframe = eval_output.unit_cell_override.is_some();

        // Build NodeSceneData. We `.take()` the eval cache so the next
        // `generate_scene` call sharing this context does not inherit it.
        NodeSceneData {
            output,
            geo_tree,
            pin_outputs,
            displayed_pins,
            node_errors: context.node_errors.clone(),
            node_output_strings: context.node_output_strings.clone(),
            unit_cell,
            construction_plane,
            show_unit_cell_wireframe,
            selected_node_eval_cache: context.selected_node_eval_cache.take(),
        }
    }

    /// Converts a geometry shell (`GeoNode`) retained on a Crystal/Molecule
    /// into a renderable `NodeOutput` using the user's current visualization
    /// method and sharpness settings. Returns `None` if conversion yields no
    /// mesh (e.g. empty CSG result).
    fn build_atomic_shell_output(
        &mut self,
        geo_tree: &GeoNode,
        context: &mut NetworkEvaluationContext,
        geometry_visualization_preferences: &GeometryVisualizationPreferences,
    ) -> Option<NodeOutput> {
        match geometry_visualization_preferences.geometry_visualization {
            GeometryVisualization::SurfaceSplatting => {
                let point_cloud =
                    generate_point_cloud(geo_tree, context, geometry_visualization_preferences);
                Some(NodeOutput::SurfacePointCloud(point_cloud))
            }
            GeometryVisualization::ExplicitMesh => {
                let csg_mesh = geo_tree.to_csg_mesh_cached(Some(&mut self.csg_conversion_cache))?;
                let mut poly_mesh = convert_csg_mesh_to_poly_mesh(&csg_mesh, false, false);
                poly_mesh.detect_sharp_edges(
                    geometry_visualization_preferences.sharpness_angle_threshold_degree,
                    true,
                );
                Some(NodeOutput::PolyMesh(poly_mesh))
            }
        }
    }

    fn generate_explicit_mesh_output<'a>(
        &mut self,
        result: NetworkResult,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        _registry: &NodeTypeRegistry,
        _context: &mut NetworkEvaluationContext,
        geometry_visualization_preferences: &GeometryVisualizationPreferences,
    ) -> (NodeOutput, Option<GeoNode>) {
        //let _timer = Timer::new("generate_explicit_mesh_output");
        let from_selected_node = network_stack
            .last()
            .unwrap()
            .node_network
            .is_node_selected(node_id);

        let poly_mesh = match &result {
            NetworkResult::Blueprint(geometry_summary) => {
                if let Some(csg_mesh) = geometry_summary
                    .geo_tree_root
                    .to_csg_mesh_cached(Some(&mut self.csg_conversion_cache))
                {
                    let node = network_stack
                        .last()
                        .unwrap()
                        .node_network
                        .nodes
                        .get(&node_id)
                        .unwrap();
                    let is_half_space = node.node_type_name == "half_space";
                    let mut poly_mesh =
                        convert_csg_mesh_to_poly_mesh(&csg_mesh, is_half_space, is_half_space);
                    poly_mesh.detect_sharp_edges(
                        geometry_visualization_preferences.sharpness_angle_threshold_degree,
                        true,
                    );
                    // Highlight faces if the last node is facet_shell and it's selected
                    if node.node_type_name == "facet_shell" && from_selected_node {
                        // Downcast the node data to FacetShellData
                        if let Some(facet_shell_data) =
                            node.data.as_any_ref().downcast_ref::<FacetShellData>()
                        {
                            // Call the highlight method
                            facet_shell_data.highlight_selected_facets(&mut poly_mesh);
                        }
                    }
                    Some(poly_mesh)
                } else {
                    None
                }
            }
            NetworkResult::Geometry2D(geometry_summary_2d) => {
                if let Some(csg_sketch) = geometry_summary_2d
                    .geo_tree_root
                    .to_csg_sketch_cached(Some(&mut self.csg_conversion_cache))
                {
                    let mut poly_mesh = convert_csg_sketch_to_poly_mesh(
                        csg_sketch,
                        !geometry_visualization_preferences.wireframe_geometry,
                        &geometry_summary_2d.drawing_plane,
                    );
                    poly_mesh.detect_sharp_edges(
                        geometry_visualization_preferences.sharpness_angle_threshold_degree,
                        true,
                    );
                    Some(poly_mesh)
                } else {
                    None
                }
            }
            _ => None,
        };

        // Extract geo_tree_root from the result based on its type
        let geo_tree = match result {
            NetworkResult::Blueprint(geometry_summary) => Some(geometry_summary.geo_tree_root),
            NetworkResult::Geometry2D(geometry_summary_2d) => {
                Some(geometry_summary_2d.geo_tree_root)
            }
            _ => None,
        };

        // Return output and geo_tree
        let output = if let Some(mesh) = poly_mesh {
            NodeOutput::PolyMesh(mesh)
        } else {
            NodeOutput::None
        };

        (output, geo_tree)
    }

    /// Converts a NetworkResult to a NodeOutput based on the data type and visualization preferences.
    #[allow(clippy::too_many_arguments)]
    fn convert_result_to_node_output<'a>(
        &mut self,
        result: NetworkResult,
        data_type: &DataType,
        from_selected_node: bool,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        geometry_visualization_preferences: &GeometryVisualizationPreferences,
    ) -> (NodeOutput, Option<GeoNode>) {
        if *data_type == DataType::DrawingPlane {
            if let NetworkResult::DrawingPlane(drawing_plane) = result {
                (NodeOutput::DrawingPlane(drawing_plane), None)
            } else {
                (NodeOutput::None, None)
            }
        } else if *data_type == DataType::Geometry2D {
            if geometry_visualization_preferences.geometry_visualization
                == GeometryVisualization::SurfaceSplatting
            {
                if let NetworkResult::Geometry2D(geometry_summary_2d) = result {
                    let point_cloud = generate_2d_point_cloud(
                        &geometry_summary_2d.geo_tree_root,
                        context,
                        geometry_visualization_preferences,
                    );
                    (
                        NodeOutput::SurfacePointCloud2D(point_cloud),
                        Some(geometry_summary_2d.geo_tree_root),
                    )
                } else {
                    (NodeOutput::None, None)
                }
            } else if geometry_visualization_preferences.geometry_visualization
                == GeometryVisualization::ExplicitMesh
            {
                self.generate_explicit_mesh_output(
                    result,
                    network_stack,
                    node_id,
                    registry,
                    context,
                    geometry_visualization_preferences,
                )
            } else {
                (NodeOutput::None, None)
            }
        } else if *data_type == DataType::Blueprint {
            if geometry_visualization_preferences.geometry_visualization
                == GeometryVisualization::SurfaceSplatting
            {
                if let NetworkResult::Blueprint(geometry_summary) = result {
                    let point_cloud = generate_point_cloud(
                        &geometry_summary.geo_tree_root,
                        context,
                        geometry_visualization_preferences,
                    );
                    (
                        NodeOutput::SurfacePointCloud(point_cloud),
                        Some(geometry_summary.geo_tree_root),
                    )
                } else {
                    (NodeOutput::None, None)
                }
            } else if geometry_visualization_preferences.geometry_visualization
                == GeometryVisualization::ExplicitMesh
            {
                self.generate_explicit_mesh_output(
                    result,
                    network_stack,
                    node_id,
                    registry,
                    context,
                    geometry_visualization_preferences,
                )
            } else {
                (NodeOutput::None, None)
            }
        } else if matches!(
            data_type,
            DataType::HasAtoms | DataType::Crystal | DataType::Molecule
        ) {
            // Accept both the abstract `Atomic` (still declared by not-yet-migrated
            // nodes as Fixed(Atomic)) and the concrete `Crystal`/`Molecule` pin
            // types. In all three cases the NetworkResult carries a
            // Crystal(..) or Molecule(..) variant from which we extract the
            // inner AtomicStructure for display.
            let (atomic_structure, shell_geo_tree) = match result {
                NetworkResult::Crystal(c) => (Some(c.atoms), c.geo_tree_root),
                NetworkResult::Molecule(m) => (Some(m.atoms), m.geo_tree_root),
                _ => (None, None),
            };
            if let Some(mut atomic_structure) = atomic_structure {
                atomic_structure.decorator_mut().from_selected_node = from_selected_node;
                let shell_output =
                    if geometry_visualization_preferences.show_geometry_shell_for_atomic {
                        shell_geo_tree.and_then(|geo_tree| {
                            self.build_atomic_shell_output(
                                &geo_tree,
                                context,
                                geometry_visualization_preferences,
                            )
                        })
                    } else {
                        None
                    };
                (
                    NodeOutput::Atomic(atomic_structure, shell_output.map(Box::new)),
                    None,
                )
            } else {
                (NodeOutput::None, None)
            }
        } else if let DataType::Array(inner_type) = data_type {
            if let NetworkResult::Array(elements) = result {
                self.convert_array_to_node_output(
                    elements,
                    inner_type,
                    from_selected_node,
                    network_stack,
                    node_id,
                    registry,
                    context,
                    geometry_visualization_preferences,
                )
            } else {
                (NodeOutput::None, None)
            }
        } else {
            // `Iter[T]` pins render no viewport output — materialization is
            // the consumer's job (`collect`). See
            // `doc/design_iter_display_via_collect.md`.
            (NodeOutput::None, None)
        }
    }

    /// Merges an array of results into a single displayable output.
    ///
    /// For `Array<Blueprint>`: creates a CSG union of all shapes (like the `union` node).
    /// For `Array<Geometry2D>`: creates a 2D CSG union (like the `union_2d` node).
    /// For `Array<Atomic>`: merges all atomic structures (like the `atom_union` node).
    /// Other element types or empty arrays return `NodeOutput::None`.
    #[allow(clippy::too_many_arguments)]
    fn convert_array_to_node_output<'a>(
        &mut self,
        elements: Vec<NetworkResult>,
        inner_type: &DataType,
        from_selected_node: bool,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        geometry_visualization_preferences: &GeometryVisualizationPreferences,
    ) -> (NodeOutput, Option<GeoNode>) {
        if elements.is_empty() {
            return (NodeOutput::None, None);
        }

        match inner_type {
            DataType::Blueprint => {
                let mut shapes: Vec<GeoNode> = Vec::new();
                let mut first_lattice_vecs = None;
                let mut alignment = Alignment::Aligned;
                let mut alignment_reason: Option<String> = None;
                for element in elements {
                    if let NetworkResult::Blueprint(geo) = element {
                        if first_lattice_vecs.is_none() {
                            first_lattice_vecs = Some(geo.structure.lattice_vecs.clone());
                        }
                        propagate_alignment_with_reason(
                            &mut alignment,
                            &mut alignment_reason,
                            geo.alignment,
                            &geo.alignment_reason,
                        );
                        shapes.push(geo.geo_tree_root);
                    }
                }
                if shapes.is_empty() {
                    return (NodeOutput::None, None);
                }
                let merged = NetworkResult::Blueprint(BlueprintData {
                    structure: Structure::from_lattice_vecs(first_lattice_vecs.unwrap()),
                    geo_tree_root: GeoNode::union_3d(shapes),
                    alignment,
                    alignment_reason,
                });
                self.convert_result_to_node_output(
                    merged,
                    &DataType::Blueprint,
                    from_selected_node,
                    network_stack,
                    node_id,
                    registry,
                    context,
                    geometry_visualization_preferences,
                )
            }
            DataType::Geometry2D => {
                let mut shapes: Vec<GeoNode> = Vec::new();
                let mut frame_translation = DVec2::ZERO;
                let mut first_drawing_plane = None;
                for element in elements {
                    if let NetworkResult::Geometry2D(geo) = element {
                        if first_drawing_plane.is_none() {
                            first_drawing_plane = Some(geo.drawing_plane.clone());
                        }
                        frame_translation += geo.frame_transform.translation;
                        shapes.push(geo.geo_tree_root);
                    }
                }
                if shapes.is_empty() {
                    return (NodeOutput::None, None);
                }
                let count = shapes.len() as f64;
                frame_translation /= count;
                let merged = NetworkResult::Geometry2D(GeometrySummary2D {
                    drawing_plane: first_drawing_plane.unwrap(),
                    frame_transform: Transform2D::new(frame_translation, 0.0),
                    geo_tree_root: GeoNode::union_2d(shapes),
                });
                self.convert_result_to_node_output(
                    merged,
                    &DataType::Geometry2D,
                    from_selected_node,
                    network_stack,
                    node_id,
                    registry,
                    context,
                    geometry_visualization_preferences,
                )
            }
            DataType::HasAtoms | DataType::Crystal | DataType::Molecule => {
                // Same handling for abstract `Atomic` arrays (not-yet-migrated
                // nodes) and concrete `Crystal`/`Molecule` arrays — extract the
                // inner AtomicStructure from each Crystal(..)/Molecule(..)
                // variant and union them for display.
                let mut structures: Vec<AtomicStructure> = Vec::new();
                for element in elements {
                    if let Some(structure) = element.extract_atomic() {
                        structures.push(structure);
                    }
                }
                if structures.is_empty() {
                    return (NodeOutput::None, None);
                }
                let mut result = structures.remove(0);
                for other in &structures {
                    result.add_atomic_structure(other);
                }
                result.decorator_mut().from_selected_node = from_selected_node;
                (NodeOutput::Atomic(result, None), None)
            }
            _ => (NodeOutput::None, None),
        }
    }

    /// Helper method for the common pattern: get value from node data, or override with input pin
    /// Returns the input pin value if connected, otherwise returns the default value
    /// If the input pin evaluation results in an error, returns that error
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::result_large_err)]
    pub fn evaluate_or_default<'a, T>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        parameter_index: usize,
        default_value: T,
        extractor: impl FnOnce(NetworkResult) -> Option<T>,
    ) -> Result<T, NetworkResult> {
        let result = self.evaluate_arg(network_stack, node_id, registry, context, parameter_index);

        if let NetworkResult::None = result {
            return Ok(default_value);
        }

        // Check for error first
        if result.is_error() {
            return Err(result);
        }

        // Try to extract the value
        if let Some(value) = extractor(result) {
            Ok(value)
        } else {
            Ok(default_value)
        }
    }

    /// Helper method for the common pattern: get value from required input pin
    /// Returns the input pin value if connected, otherwise returns the missing input error
    /// If the input pin evaluation results in an error, returns that error
    #[allow(clippy::result_large_err)]
    pub fn evaluate_required<'a, T>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        parameter_index: usize,
        extractor: impl FnOnce(NetworkResult) -> Option<T>,
    ) -> Result<T, NetworkResult> {
        let result =
            self.evaluate_arg_required(network_stack, node_id, registry, context, parameter_index);

        // Check for error first
        if result.is_error() {
            return Err(result);
        }

        // Try to extract the value
        if let Some(value) = extractor(result.clone()) {
            Ok(value)
        } else {
            Err(result)
        }
    }

    // Evaluates an argument of a node.
    // Can return an Error NetworkResult, or a valid NetworkResult.
    // If the atgument is not connected that is an error.
    // If the return value is not an Error, it is guaranteed to be converted to the
    // type of the parameter.
    pub fn evaluate_arg_required<'a>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        parameter_index: usize,
    ) -> NetworkResult {
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        let input_name = registry.get_parameter_name(node, parameter_index);
        let result = self.evaluate_arg(network_stack, node_id, registry, context, parameter_index);
        if let NetworkResult::None = result {
            input_missing_error(&input_name)
        } else {
            result
        }
    }

    // Evaluates an argument of a node.
    // Can return a NetworkResult::None, NetworkResult::Error, or a valid NetworkResult.
    // Returns NetworkResult::None if the input was not connected.
    // If the return value is not an Error or None, it is guaranteed to be converted to the
    // type of the parameter.
    pub fn evaluate_arg<'a>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        parameter_index: usize,
    ) -> NetworkResult {
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        let input_name = registry.get_parameter_name(node, parameter_index);

        // Get the expected input type for this parameter
        let expected_type = registry.get_node_param_data_type(node, parameter_index);

        // Defensive guard against an `arguments` / parameter-count desync.
        // `node.arguments` is supposed to carry one slot per declared parameter,
        // but it can transiently lag the node's parameter list: the slots are
        // grown to match by `repair_network_arguments` (top-level *and* body
        // nodes) which only runs inside `validate_network`, while the load-time
        // `repair_node_network` grows only top-level `network.nodes`. Switching
        // the active network (`set_active_node_network_name`) marks a full
        // refresh but does **not** validate, so `generate_scene` can evaluate a
        // freshly-activated, not-yet-validated network whose node has
        // `parameters.len() > arguments.len()` — most easily an `expr` node,
        // whose `eval` iterates its own `self.parameters` and calls us with a
        // `parameter_index` past the end of `arguments`. Treat a missing slot as
        // "nothing connected" (the meaning of an empty slot) rather than
        // panicking with an out-of-bounds index. The next validate pass repairs
        // the desync permanently.
        if parameter_index >= node.arguments.len() {
            return NetworkResult::None;
        }

        // Clone the wires list so we can iterate while passing
        // `context: &mut` and `network_stack: &` into evaluate calls. The
        // wires list is generally small (often 1) and `IncomingWire` is plain
        // POD, so the clone is cheap.
        let incoming_wires: Vec<IncomingWire> =
            node.arguments[parameter_index].incoming_wires.clone();

        if expected_type.is_array() {
            if incoming_wires.is_empty() {
                return NetworkResult::None; // Nothing is connected
            }

            let mut merged_items = Vec::new();

            // Sort by source node id for deterministic merge order. Pre-zones
            // the wires came from a HashMap with non-deterministic iteration
            // order; the Vec is deterministic but we keep the sort so merge
            // order doesn't depend on construction order.
            let mut indices: Vec<usize> = (0..incoming_wires.len()).collect();
            indices.sort_by_key(|&i| incoming_wires[i].source_node_id);

            for idx in indices {
                let incoming = &incoming_wires[idx];
                let (result, source_type) =
                    self.resolve_incoming_wire(network_stack, registry, context, incoming);

                if let NetworkResult::Error(_) = result {
                    return error_in_input(&input_name);
                }

                // Force a `() -> T` source to `T` before the merge when the
                // array's element type wants the value (see
                // `doc/design_nullary_function_coercion.md`).
                let (result, source_type) = self.force_nullary_arg(
                    network_stack,
                    registry,
                    context,
                    result,
                    source_type,
                    &expected_type,
                );
                if let NetworkResult::Error(_) = result {
                    return error_in_input(&input_name);
                }

                // convert_to handles conversion to array types, so we can convert directly.
                // The result is guaranteed to be an array, containing one or more elements.
                let converted_result = result.convert_to(&source_type, &expected_type, registry);

                if let NetworkResult::Array(array_data) = converted_result {
                    merged_items.extend(array_data);
                } else {
                    // This should not happen based on the logic of convert_to, but we handle it just in case.
                    return error_in_input(&input_name);
                }
            }

            NetworkResult::Array(merged_items)
        } else {
            // single argument evaluation
            if let Some(incoming) = incoming_wires.first() {
                let (result, source_type) =
                    self.resolve_incoming_wire(network_stack, registry, context, incoming);
                if let NetworkResult::Error(_) = result {
                    return error_in_input(&input_name);
                }
                // Force a `() -> T` source to `T` when this pin wants the value
                // (see `doc/design_nullary_function_coercion.md`).
                let (result, source_type) = self.force_nullary_arg(
                    network_stack,
                    registry,
                    context,
                    result,
                    source_type,
                    &expected_type,
                );
                if let NetworkResult::Error(_) = result {
                    return error_in_input(&input_name);
                }
                result.convert_to(&source_type, &expected_type, registry)
            } else {
                NetworkResult::None // Nothing is connected
            }
        }
    }

    /// Nullary function forcing: if `result` is an arity-0 function value
    /// (`() -> T`) flowing into a slot that wants the value `T`, run the
    /// closure once with an empty frame and return the produced value plus its
    /// inferred type; otherwise pass the value through untouched.
    ///
    /// The "wants the value" test peeks **one** `Array` level — so `[T]` forces
    /// each `() -> T` wire to `T` before the merge, while `[() -> T]` keeps the
    /// functions — but does not recurse deeper. This is the runtime twin of the
    /// top-level static arm in `DataType::can_be_converted_to`; the two reach
    /// exactly the same set of (source, dest) pairs. Higher-arity or
    /// non-function sources are returned unchanged. An `Error` produced while
    /// forcing propagates as the new result for the caller's error check.
    ///
    /// `param_types` (the closure's body frame size) is the correct
    /// "consumes zero caller args" test: a partially-applied closure that has
    /// become nullary has empty `param_types`, and `run_closure_once` prepends
    /// its `pre_supplied_args` to the (empty) caller frame. See
    /// `doc/design_nullary_function_coercion.md`.
    fn force_nullary_arg<'a>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        result: NetworkResult,
        source_type: DataType,
        expected_type: &DataType,
    ) -> (NetworkResult, DataType) {
        let NetworkResult::Function(closure) = &result else {
            return (result, source_type);
        };
        if !closure.param_types.is_empty() {
            return (result, source_type);
        }
        // The slot that will receive the value: an array pin merges/broadcasts
        // at its element type, every other pin at its own type.
        let slot = match expected_type {
            DataType::Array(elem) => elem.as_ref(),
            other => other,
        };
        if slot.is_function_shape() {
            return (result, source_type);
        }
        // Fall back to the declared return type if the forced value can't be
        // inferred (e.g. an array whose elements don't infer); `Error`/`None`
        // forced values are handled by the caller's error check / `convert_to`.
        let declared_return = *closure.function_type().output_type;
        let forced = run_closure_once(self, network_stack, registry, context, closure, Vec::new());
        let forced_type = forced.infer_data_type().unwrap_or(declared_return);
        (forced, forced_type)
    }

    /// Resolve one `IncomingWire` to its `(NetworkResult, source_data_type)`
    /// pair. Single dispatch point for all four wire shapes (today's local
    /// regular-output wire, capture wire crossing a zone boundary, iteration
    /// value from an enclosing HOF's zone-input pin, deeper-than-immediate
    /// captured zone-input). Order of checks:
    ///
    /// 1. **Capture cache** — if this wire was pre-evaluated at body entry,
    ///    serve the cached value. This must come before the per-`SourcePin`
    ///    dispatch so that captures of `ZoneInput` sources hit the cache
    ///    rather than falling into the live-lookup path (which would read
    ///    the wrong frame; nested-HOF captures see outer-iteration values
    ///    snapshot at inner-body entry, not the current outer iteration).
    /// 2. **`NodeOutput` source** — walk `source_scope_depth` levels up the
    ///    network stack and evaluate via the normal `evaluate` path against
    ///    the sliced stack. Depth `0` is today's path.
    /// 3. **`ZoneInput` source** — read the top frame of the HOF's
    ///    scope-stack in `current_zone_input_values`. The HOF lives at depth
    ///    `source_scope_depth` (≥ 1) above the destination's body; its id is
    ///    `incoming.source_node_id`.
    ///
    /// Phase 3 lands the helper. The two new arms (`NodeOutput` with
    /// `source_scope_depth > 0`, and `ZoneInput`) are unreached in existing
    /// code because no node populates zone data yet — every wire today has
    /// `source_scope_depth = 0` and `source_pin = NodeOutput { .. }`. See
    /// `doc/design_zones.md` (§"What's new" point 2).
    fn resolve_incoming_wire<'a>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
        incoming: &IncomingWire,
    ) -> (NetworkResult, DataType) {
        // 1. Capture cache. The cached value carries its own concrete type, so
        // inferring it from the value is sufficient for downstream
        // `convert_to`.
        let key = CaptureKey::from_incoming(incoming);
        if let Some(cached) = context.captured_source_values.get(&key) {
            let value = cached.clone();
            let dt = value.infer_data_type().unwrap_or(DataType::None);
            return (value, dt);
        }

        match incoming.source_pin {
            SourcePin::NodeOutput { pin_index } => {
                let depth = incoming.source_scope_depth as usize;
                let stack_len = network_stack.len();

                // Walk `depth` frames up the stack. Depth `0` resolves
                // against the destination's containing network (today's
                // behavior). Phase 6 validation catches a depth that
                // overflows the stack; this debug-asserts in case a wire
                // sneaks through with a bad depth.
                let source_frame_idx = stack_len.checked_sub(1 + depth).unwrap_or_else(|| {
                    debug_assert!(
                        false,
                        "NodeOutput wire source_scope_depth {} exceeds stack length {}",
                        depth, stack_len,
                    );
                    0
                });
                let source_slice = &network_stack[..=source_frame_idx];
                let source_network = source_slice.last().unwrap().node_network;

                let source_type =
                    if let Some(source_node) = source_network.nodes.get(&incoming.source_node_id) {
                        // Resolve the concrete output type. For polymorphic pins
                        // (`SameAsInput` / `SameAsArrayElements`) the declared
                        // type is `DataType::None`, which would defeat
                        // `convert_to`'s single→array auto-wrap.
                        registry
                            .resolve_output_type(source_node, source_network, pin_index)
                            .unwrap_or_else(|| {
                                registry
                                    .get_node_type_for_node(source_node)
                                    .map(|nt| nt.get_output_pin_type(pin_index))
                                    .unwrap_or(DataType::None)
                            })
                    } else {
                        DataType::None
                    };

                let result = self.evaluate(
                    source_slice,
                    incoming.source_node_id,
                    pin_index,
                    registry,
                    false,
                    context,
                );

                (result, source_type)
            }
            SourcePin::ZoneInput { pin_index } => {
                // Live lookup against the HOF's scope-stack. Reading at any
                // depth lands on the most-recently-pushed frame for this
                // HOF id, which is exactly the immediately-enclosing HOF's
                // iteration values. Deeper-than-immediate references go
                // through the capture cache and never reach this branch
                // (handled at step 1 above) — see worked example in
                // `doc/design_zones.md`.
                let value = context
                    .current_zone_input(incoming.source_node_id, pin_index)
                    .clone();

                // Source type is the declared type of the HOF's
                // `pin_index`-th zone-input pin. The HOF's body frame sits
                // at `stack_len - depth`; the HOF node itself lives in
                // the network at `stack_len - depth - 1` with id matching
                // the wire's `source_node_id`.
                //
                // The lazy `Walker::MapZone` per-element step stands up a
                // body-only synthetic stack (`stack_len = 1`), so the HOF
                // ancestor isn't actually present in the slice we received.
                // In that case we fall back to inferring the source type
                // from the live iteration value — its concrete type is what
                // `convert_to` cares about for the downstream conversion.
                let depth = incoming.source_scope_depth as usize;
                let stack_len = network_stack.len();
                let source_type = if depth == 0 {
                    debug_assert!(
                        false,
                        "ZoneInput wire requires source_scope_depth >= 1 (got 0)"
                    );
                    DataType::None
                } else if let Some(body_frame_idx) = stack_len.checked_sub(depth) {
                    if body_frame_idx == 0 {
                        // Body-only synthetic stack (lazy walker): infer from
                        // the live iteration value.
                        value.infer_data_type().unwrap_or(DataType::None)
                    } else {
                        let hof_network = network_stack[body_frame_idx - 1].node_network;
                        let hof_id = network_stack[body_frame_idx].node_id;
                        hof_network
                            .nodes
                            .get(&hof_id)
                            .and_then(|hof_node| registry.get_node_type_for_node(hof_node))
                            .and_then(|nt| {
                                nt.zone_input_pins
                                    .get(pin_index)
                                    .and_then(|opd| opd.fixed_type().cloned())
                            })
                            .unwrap_or_else(|| value.infer_data_type().unwrap_or(DataType::None))
                    }
                } else {
                    debug_assert!(
                        false,
                        "ZoneInput wire source_scope_depth {} exceeds stack length {}",
                        depth, stack_len,
                    );
                    DataType::None
                };

                (value, source_type)
            }
        }
    }

    /// Evaluate a node and return all output pin results.
    /// Used by generate_scene() to avoid redundant evaluation when
    /// displaying multiple output pins of the same node.
    pub fn evaluate_all_outputs<'a>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let node = NetworkStackElement::get_top_node(network_stack, node_id);

        // Central skip rule for `Unit`-returning nodes: when the pass is *not*
        // an explicit Execute and every resolved output pin of this node is
        // `DataType::Unit`, skip `NodeData::eval` entirely and synthesise an
        // `EvalOutput` of all `NetworkResult::Unit` values directly. This is
        // what gates side-effect nodes (`export_atoms`, `foreach`, future
        // effect nodes) on display passes — `eval` only runs when the user
        // actually invokes Execute. The check uses **resolved** output types
        // (via `resolve_output_type`) so polymorphic pins resolving to Unit
        // are also covered. See `doc/design_node_execution.md` (Phase 2 —
        // Central skip rule for Unit-returning nodes).
        if !context.execute
            && let Some(node_type) = registry.get_node_type_for_node(node)
        {
            let pin_count = node_type.output_pin_count();
            if pin_count > 0 {
                let current_network = network_stack.last().unwrap().node_network;
                let all_unit = (0..pin_count).all(|pin_idx| {
                    registry
                        .resolve_output_type(node, current_network, pin_idx as i32)
                        .map(|t| t == DataType::Unit)
                        .unwrap_or(false)
                });
                if all_unit {
                    let results = vec![NetworkResult::Unit; pin_count];
                    // Record per-pin display strings so the UI renders the
                    // node consistently with non-skipped passes.
                    let pin_strings: Vec<String> =
                        results.iter().map(|r| r.to_display_string()).collect();
                    let key = context.node_ref(node_id);
                    context.node_output_strings.insert(key, pin_strings);
                    return EvalOutput::multi(results);
                }
            }
        }

        let eval_output = if registry
            .built_in_node_types
            .contains_key(&node.node_type_name)
        {
            node.data
                .eval(self, network_stack, node_id, registry, decorate, context)
        } else if let Some(child_network) = registry.node_networks.get(&node.node_type_name) {
            // custom node — evaluate return node, pass through all outputs
            if !child_network.valid {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "{} is invalid",
                    node.node_type_name
                )));
            }
            let mut child_network_stack = network_stack.to_vec();
            child_network_stack.push(NetworkStackElement {
                node_network: child_network,
                node_id,
            });
            if child_network.return_node_id.is_none() {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "{} has no return node",
                    node.node_type_name
                )));
            }
            // Entering a custom-network instance: its internal node ids share
            // the top-level id space, so scope their errors/strings under this
            // instance id (keeps them from clobbering same-id nodes in the
            // active network — those internals are never read back in the view).
            context.push_eval_scope(node_id);
            let eval_output = self.evaluate_all_outputs(
                &child_network_stack,
                child_network.return_node_id.unwrap(),
                registry,
                false,
                context,
            );
            context.pop_eval_scope();
            // Wrap errors with the custom network name for better diagnostics
            let child_display_results = eval_output.display_results;
            let results: Vec<NetworkResult> = eval_output
                .results
                .into_iter()
                .map(|r| {
                    if let NetworkResult::Error(_) = &r {
                        NetworkResult::Error(format!("Error in {}", node.node_type_name))
                    } else {
                        r
                    }
                })
                .collect();
            let mut output = EvalOutput::multi(results);
            output.display_results = child_display_results;
            output
        } else {
            EvalOutput::single(NetworkResult::Error(format!(
                "Unknown node type: {}",
                node.node_type_name
            )))
        };

        // Runtime guard: if a node produced a value whose inferred data type
        // is abstract, that is a bug in a polymorphic node's `eval` (it failed
        // to re-wrap its result in a concrete variant). Replace such values
        // with a NetworkResult::Error so downstream state is not corrupted.
        // In debug builds this also asserts — should be unreachable in a
        // valid, well-implemented graph.
        let mut eval_output = eval_output;
        for (pin_idx, result) in eval_output.results.iter_mut().enumerate() {
            if let Some(t) = result.infer_data_type()
                && t.is_abstract()
            {
                debug_assert!(
                    false,
                    "node {} pin {} produced value with abstract type {:?}",
                    node_id, pin_idx, t
                );
                *result = NetworkResult::Error(format!(
                    "node produced value with abstract type {:?} on pin {}",
                    t, pin_idx
                ));
            }
        }

        // Record error from primary (pin 0) result
        let primary = eval_output.primary();
        if let NetworkResult::Error(error_message) = primary {
            let key = context.node_ref(node_id);
            context.node_errors.insert(key, error_message.clone());
        }
        // Record per-pin display strings. A node may publish a custom
        // subtitle via `EvalOutput::pin_subtitles` (e.g. `collect` reports
        // "(stopped at limit N)" when the cap was hit); falls back to the
        // result's display string (truncated to `ARRAY_DISPLAY_CAP` array
        // elements) when no override is set.
        let pin_strings: Vec<String> = eval_output
            .results
            .iter()
            .enumerate()
            .map(|(idx, r)| {
                eval_output
                    .pin_subtitles
                    .get(&idx)
                    .cloned()
                    .unwrap_or_else(|| r.to_display_string_capped(ARRAY_DISPLAY_CAP))
            })
            .collect();
        let key = context.node_ref(node_id);
        context.node_output_strings.insert(key, pin_strings);

        eval_output
    }

    // Evaluates the specified node (calculates the NetworkResult on its output pin).
    pub fn evaluate<'a>(
        &self,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        output_pin_index: i32,
        registry: &NodeTypeRegistry,
        decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> NetworkResult {
        // Defensive: if the node has gone missing from the top of the network
        // stack (a stale wire source_node_id, a displayed_node id that survived
        // a delete, or any other invariant violation) we return an Error
        // rather than panic. The panic this guards against was reproducible by
        // selecting a freshly-added expr node — see network_evaluator.rs:1591
        // in the bug report. Even if a follow-up fix removes the underlying
        // cause, this guard keeps the evaluator robust to similar issues.
        if !network_stack
            .last()
            .is_some_and(|frame| frame.node_network.nodes.contains_key(&node_id))
        {
            let msg = format!("evaluate: node {} not found in active frame", node_id);
            let key = context.node_ref(node_id);
            context.node_errors.insert(key, msg.clone());
            return NetworkResult::Error(msg);
        }

        // Function pin (`output_pin_index == -1`): synthesize a `Function`
        // value from this node viewed as a function of its *unconnected* inputs,
        // with its connected inputs frozen as captures. It runs no `eval`, is
        // never `Unit`, and has no display string — so it short-circuits ahead
        // of the central skip rule and the built-in/custom dispatch below. The
        // returned `Function(zc)` flows into an HOF `f` pin / `apply` exactly
        // like a `closure` node's output. See
        // `doc/design_node_function_pin_captures.md`.
        if output_pin_index == -1 {
            return build_node_function_closure(self, network_stack, node_id, registry, context)
                .map(NetworkResult::Function)
                .unwrap_or_else(|e| e); // e is already NetworkResult::Error(_)
        }

        // Subtitle override published by `NodeData::eval` via
        // `EvalOutput::pin_subtitles` for the requested pin. The outer
        // single-pin clobber at the end of this method honors it instead
        // of the result's `to_display_string()`.
        let mut pin_subtitle_override: Option<String> = None;

        let result = {
            let node = NetworkStackElement::get_top_node(network_stack, node_id);

            // Central skip rule for `Unit`-returning nodes (mirrors
            // `evaluate_all_outputs`). When the pass is not an Execute and
            // every resolved output pin of this node is `DataType::Unit`, we
            // synthesise `NetworkResult::Unit` directly instead of running
            // the node's `eval`. This is what makes side-effect nodes
            // (`export_atoms`, `foreach`, …) cost-free on display passes
            // regardless of whether they are reached via
            // `evaluate_all_outputs` (top-level displayed node) or via
            // `evaluate` (consumed as another node's input). See
            // `doc/design_node_execution.md` (Phase 2 — Central skip rule).
            if !context.execute
                && let Some(node_type) = registry.get_node_type_for_node(node)
            {
                let pin_count = node_type.output_pin_count();
                if pin_count > 0 {
                    let current_network = network_stack.last().unwrap().node_network;
                    let all_unit = (0..pin_count).all(|pin_idx| {
                        registry
                            .resolve_output_type(node, current_network, pin_idx as i32)
                            .map(|t| t == DataType::Unit)
                            .unwrap_or(false)
                    });
                    if all_unit {
                        let pin_strings: Vec<String> = (0..pin_count)
                            .map(|_| NetworkResult::Unit.to_display_string())
                            .collect();
                        let key = context.node_ref(node_id);
                        context.node_output_strings.insert(key, pin_strings);
                        return NetworkResult::Unit;
                    }
                }
            }

            if registry
                .built_in_node_types
                .contains_key(&node.node_type_name)
            {
                let eval_output =
                    node.data
                        .eval(self, network_stack, node_id, registry, decorate, context);
                // Record all pin strings now, since eval() already computed them all.
                // This prevents partial overwrites when get_all_node_output_strings()
                // aggregates across multiple generate_scene() contexts. Pins may
                // publish a custom subtitle via `EvalOutput::pin_subtitles`.
                let pin_strings: Vec<String> = eval_output
                    .results
                    .iter()
                    .enumerate()
                    .map(|(idx, r)| {
                        eval_output
                            .pin_subtitles
                            .get(&idx)
                            .cloned()
                            .unwrap_or_else(|| r.to_display_string())
                    })
                    .collect();
                let key = context.node_ref(node_id);
                context.node_output_strings.insert(key, pin_strings);
                // Capture subtitle for the requested pin so the outer clobber
                // below preserves it (otherwise it would be replaced by the
                // result's raw display string — e.g. an array dump).
                let requested_pin_idx = if output_pin_index < 0 {
                    0
                } else {
                    output_pin_index as usize
                };
                pin_subtitle_override = eval_output.pin_subtitles.get(&requested_pin_idx).cloned();
                eval_output.get(output_pin_index)
            } else if let Some(child_network) = registry.node_networks.get(&node.node_type_name) {
                // custom node — pass through the requested output pin index to the return node
                if !child_network.valid {
                    return NetworkResult::Error(format!("{} is invalid", node.node_type_name));
                }
                let mut child_network_stack = network_stack.to_vec();
                child_network_stack.push(NetworkStackElement {
                    node_network: child_network,
                    node_id,
                });
                if child_network.return_node_id.is_none() {
                    return NetworkResult::Error(format!(
                        "{} has no return node",
                        node.node_type_name
                    ));
                }
                // Scope the custom-network internals under this instance id
                // (see the matching branch in `evaluate_all_outputs`).
                context.push_eval_scope(node_id);
                let result = self.evaluate(
                    &child_network_stack,
                    child_network.return_node_id.unwrap(),
                    output_pin_index,
                    registry,
                    false,
                    context,
                );
                context.pop_eval_scope();
                if let NetworkResult::Error(_error) = &result {
                    NetworkResult::Error(format!("Error in {}", node.node_type_name))
                } else {
                    result
                }
            } else {
                NetworkResult::Error(format!("Unknown node type: {}", node.node_type_name))
            }
        };

        // Check for error and store it in the context
        if let NetworkResult::Error(error_message) = &result {
            let key = context.node_ref(node_id);
            context.node_errors.insert(key, error_message.clone());
        }

        // Record per-pin display string (single-pin evaluation overwrites).
        // A subtitle override published via `EvalOutput::pin_subtitles` (e.g.
        // `collect`'s "(stopped at limit N)") wins over the raw result
        // display (truncated to `ARRAY_DISPLAY_CAP` array elements).
        let display_string = pin_subtitle_override
            .unwrap_or_else(|| result.to_display_string_capped(ARRAY_DISPLAY_CAP));
        let pin_index = if output_pin_index < 0 {
            0
        } else {
            output_pin_index as usize
        };
        let key = context.node_ref(node_id);
        let entry = context.node_output_strings.entry(key).or_default();
        // Grow the vec if needed
        if entry.len() <= pin_index {
            entry.resize(pin_index + 1, String::new());
        }
        entry[pin_index] = display_string;

        result
    }
}
