//! Convert a custom-network instance ⇄ a `closure` node — the closure-aware
//! analogue of *Inline a Custom Node* (`node_inlining.rs`) and *Factor Selection
//! into Subnetwork* (`selection_factoring.rs`). See
//! `doc/design_closure_network_conversion.md`.
//!
//! **Phase 1: Network → Closure, top level.** Replacing a custom-network
//! instance node `I` (whose function pin is used, or which is unconsumed) with a
//! `closure` node `C` whose inline body is a copy of `I`'s network `N`. `I`'s
//! **wired** input pins become **captures** in the body; its **unwired** input
//! pins become the closure's **parameters** (zone-input pins).
//!
//! **Phase 2: Closure → Network, top level.** The inverse: lift a `closure`
//! node `C`'s body `B` into a fresh standalone network `N` (with parameter nodes
//! for both the closure's parameters and its captures) and replace `C` with an
//! instance `I` of `N`, wired so `I`'s `-1` (function) value reproduces `C`'s.
//! See [`extract_network_from_closure`].
//!
//! The semantic bridge is the function pin (`output_pin_index == -1`): a
//! custom-network instance `I` used through its `-1` pin and a `closure` node
//! `C` expose the *same* `Function` value, just on different output pins. The
//! conversion is a graph rewrite between the two representations; see the design
//! doc for the proof of equivalence.
//!
//! This module holds the pure, registry-light building blocks. The orchestrator
//! (`StructureDesigner::convert_instance_to_closure`) handles scope resolution,
//! consumer redirection, display-state cleanup, validation, and undo.

use glam::f64::DVec2;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::data_type::DataType;
use super::node_data::CustomNodeData;
use super::node_inlining::{content_bounding_box, copy_content_into};
use super::node_network::{
    Argument, CollapseMode, DEFAULT_BODY_HEIGHT, DEFAULT_BODY_WIDTH, IncomingWire, Node,
    NodeNetwork, SourcePin,
};
use super::node_type::{
    NodeType, OutputPinDefinition, Parameter, PinOutputType, generic_node_data_loader,
    generic_node_data_saver,
};
use super::node_type_registry::NodeTypeRegistry;
use super::nodes::closure::{ClosureData, ClosureKind};
use super::nodes::parameter::ParameterData;
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;

/// How a parameter node of `N` maps into the closure body after conversion.
enum ParamClass {
    /// The instance's pin was **unwired** → a closure parameter at this dense
    /// zone-input index (`cp`, ascending pin order).
    ClosureParam(usize),
    /// The instance's pin was **wired** → a capture. Carries the instance's
    /// incoming wire(s) on that pin (the capture source(s), as seen from the
    /// host scope `H`).
    Capture(Vec<IncomingWire>),
}

/// The **Network → Closure** wire splice (per wire, at nesting `k` in the body
/// `B`). Mirrors `node_inlining::DescentA`, but the boundary class (a reference
/// to a `parameter` node of `N`) routes to a closure `ZoneInput` pin or to the
/// instance's capture wire instead of to the instance's input wires.
///
/// All ids it indexes by are in `N`'s original id space — the copied content's
/// wires still carry `N`'s ids on entry (see `copy_content_into`).
struct SpliceNetworkToClosure<'a> {
    /// `N`'s `parameter` node id → its classification.
    param_class: &'a HashMap<u64, ParamClass>,
    /// `N`'s top-level non-`parameter` node id → its new (copied) id in `B`.
    id_mapping: &'a HashMap<u64, u64>,
    /// The closure node's id (`== I.id`); the owner of `B`, reached from nesting
    /// `k` at `source_scope_depth == k + 1`.
    closure_id: u64,
}

impl SpliceNetworkToClosure<'_> {
    /// Rebuild every argument list's wires at nesting `k`, classifying each wire
    /// whose `source_scope_depth == k`. Wires at other depths point into
    /// intermediate verbatim-cloned sub-bodies (preserved ids) and are kept
    /// verbatim. `N` is self-contained, so no `source_scope_depth > k` arises.
    fn reclassify(&self, args: &mut [Argument], k: u8) {
        for arg in args.iter_mut() {
            let mut new_wires: Vec<IncomingWire> = Vec::with_capacity(arg.incoming_wires.len());
            for wire in &arg.incoming_wires {
                if wire.source_scope_depth != k {
                    new_wires.push(wire.clone());
                    continue;
                }
                if let Some(&new_id) = self.id_mapping.get(&wire.source_node_id) {
                    // Reference to a co-copied node: follow the id remap, pin and
                    // depth unchanged.
                    new_wires.push(IncomingWire {
                        source_node_id: new_id,
                        source_pin: wire.source_pin,
                        source_scope_depth: wire.source_scope_depth,
                    });
                } else if let Some(class) = self.param_class.get(&wire.source_node_id) {
                    match class {
                        ParamClass::ClosureParam(cp) => {
                            // From nesting `k`, the closure (the body's owner) is
                            // `k + 1` frames up; read its zone-input pin `cp`.
                            new_wires.push(IncomingWire {
                                source_node_id: self.closure_id,
                                source_pin: SourcePin::ZoneInput { pin_index: *cp },
                                source_scope_depth: k + 1,
                            });
                        }
                        ParamClass::Capture(iws) => {
                            // Replace with the instance's wire(s), each rebased to
                            // reach the same physical source from inside the body:
                            // `B` adds one extra frame below `H`, hence `+1`
                            // beyond the inline `k + iw.depth` formula. An empty
                            // instance pin drops the wire.
                            for iw in iws {
                                new_wires.push(IncomingWire {
                                    source_node_id: iw.source_node_id,
                                    source_pin: iw.source_pin,
                                    source_scope_depth: (k + 1) + iw.source_scope_depth,
                                });
                            }
                        }
                    }
                }
                // otherwise drop — cannot happen for a valid self-contained N.
            }
            arg.incoming_wires = new_wires;
        }
    }

    /// Recurse into a copied body, processing body-node `arguments` +
    /// `zone_output_arguments` at the body's nesting.
    fn descend_body(&self, body: &mut NodeNetwork, nesting: u8) {
        for node in body.nodes.values_mut() {
            self.reclassify(&mut node.arguments, nesting);
            self.reclassify(&mut node.zone_output_arguments, nesting);
            if let Some(nested) = node.zone_mut() {
                self.descend_body(nested, nesting + 1);
            }
        }
    }
}

/// Build the `closure` node `C` (id = instance id) to drop in place of the
/// custom-network instance `I`.
///
/// Reads the cloned definition `N` (`source`) while leaving it untouched.
/// Returns the new `Node`; the caller replaces `I` with it (reusing the id),
/// redirects consumers of `I`'s `-1` pin to `C`'s pin `0`, and clears any stale
/// display state. Errors only on a genuinely lossy/ill-defined input (no return
/// node, unresolved output type).
///
/// The gate that `I` is a custom-network instance used as a function (no
/// normal-output consumers) lives in the orchestrator, which has the host scope
/// `H` to walk; this builder assumes that has passed.
pub fn build_closure_from_instance(
    instance: &Node,
    source: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> Result<Node, String> {
    // A closure must deliver a result.
    let return_id = source
        .return_node_id
        .ok_or_else(|| "The custom network has no return node".to_string())?;

    // N's parameter nodes, collected as (id, param_index, name, type) and sorted
    // by param_index so closure-param indices (`cp`) are assigned in ascending
    // pin order — matching `resolve_output_type(I, -1)` for the round-trip.
    let mut params: Vec<(u64, usize, String, DataType)> = Vec::new();
    for node in source.nodes.values() {
        if node.node_type_name == "parameter" {
            // `as_ref()` first so the downcast resolves on `dyn NodeData`, not on
            // the `Box` itself.
            if let Some(pd) = node
                .data
                .as_ref()
                .as_any_ref()
                .downcast_ref::<ParameterData>()
            {
                params.push((
                    node.id,
                    pd.param_index,
                    pd.param_name.clone(),
                    pd.data_type.clone(),
                ));
            }
        }
    }
    params.sort_by_key(|(_, idx, _, _)| *idx);

    // Classify each parameter: unwired pin → closure parameter (dense `cp`),
    // wired pin → capture (the instance's wire(s) on that pin).
    let mut param_class: HashMap<u64, ParamClass> = HashMap::new();
    let mut closure_param_types: Vec<DataType> = Vec::new();
    let mut closure_param_names: Vec<String> = Vec::new();
    for (pid, pidx, pname, ptype) in &params {
        let wired = instance
            .arguments
            .get(*pidx)
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        if wired {
            let iws = instance.arguments[*pidx].incoming_wires.clone();
            param_class.insert(*pid, ParamClass::Capture(iws));
        } else {
            let cp = closure_param_types.len();
            param_class.insert(*pid, ParamClass::ClosureParam(cp));
            closure_param_types.push(ptype.clone());
            closure_param_names.push(pname.clone());
        }
    }

    // The closure's return type = N's pin-0 output type. Use the network's
    // declared output type (the same source `resolve_output_type(I, -1)` reads),
    // so the function types match exactly across the round-trip.
    let ret = source.node_type.output_type().clone();
    if ret == DataType::None {
        return Err(
            "The custom network's output type is unresolved (polymorphic); it cannot be converted \
             to a closure"
                .to_string(),
        );
    }

    // Build the ClosureData directly as a `Custom` closure so N's parameter
    // names are preserved (`closure_data_for_signature` carries types only).
    let mut type_args = closure_param_types.clone();
    type_args.push(ret.clone());
    // Carry `N`'s identity onto the closure as its display label so the user can
    // still see which network it came from (and so the inverse *Closure →
    // Network* conversion can re-suggest the name). The network name is
    // qualified (`Foo.Bar.Baz`); the label uses the **non-qualified** simple
    // name (`Baz`) since the title bar has no room for a full namespace path.
    let simple_name = source
        .node_type
        .name
        .rsplit('.')
        .next()
        .unwrap_or(&source.node_type.name);
    let custom_label = if simple_name.is_empty() {
        None
    } else {
        Some(simple_name.to_string())
    };
    let closure_data = ClosureData {
        kind: ClosureKind::Custom,
        type_args,
        param_names: closure_param_names,
        custom_label,
    };

    // Body B: copy N's non-`parameter` content (fresh ids for B-top nodes,
    // nested body Arcs verbatim).
    let (content_min, _content_size) = content_bounding_box(source, registry);
    let mut body = NodeNetwork::new_empty();
    let id_mapping = copy_content_into(&mut body, source, DVec2::ZERO, content_min);

    // Splice B's wires: parameter refs → closure ZoneInput / capture wire;
    // copied-node refs → remapped.
    let splice = SpliceNetworkToClosure {
        param_class: &param_class,
        id_mapping: &id_mapping,
        closure_id: instance.id,
    };
    let copied_ids: Vec<u64> = id_mapping.values().copied().collect();
    for &new_id in &copied_ids {
        if let Some(node) = body.nodes.get_mut(&new_id) {
            splice.reclassify(&mut node.arguments, 0);
            if let Some(nested) = node.zone_mut() {
                splice.descend_body(nested, 1);
            }
        }
    }

    // Result wire: the closure's `zone_output_arguments[0]` reads R's output
    // pin 0. If R is a `parameter` node (the network forwards an argument),
    // route it the same way the splice routes a reference to that parameter at
    // `k = 0` — to the closure's ZoneInput pin (closure param) or to the
    // instance's capture wire(s).
    let result_wires: Vec<IncomingWire> = if let Some(&new_id) = id_mapping.get(&return_id) {
        vec![IncomingWire {
            source_node_id: new_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
        }]
    } else if let Some(class) = param_class.get(&return_id) {
        match class {
            ParamClass::ClosureParam(cp) => vec![IncomingWire {
                source_node_id: instance.id,
                source_pin: SourcePin::ZoneInput { pin_index: *cp },
                source_scope_depth: 1,
            }],
            ParamClass::Capture(iws) => iws
                .iter()
                .map(|iw| IncomingWire {
                    source_node_id: iw.source_node_id,
                    source_pin: iw.source_pin,
                    source_scope_depth: 1 + iw.source_scope_depth,
                })
                .collect(),
        }
    } else {
        return Err("The custom network's return node is invalid".to_string());
    };

    // Build C: reuse I's id and position. Pins / zone state are filled in by
    // `populate_custom_node_type_cache_with_types` below.
    #[allow(clippy::arc_with_non_send_sync)]
    let mut closure_node = Node {
        id: instance.id,
        node_type_name: "closure".to_string(),
        custom_name: instance.custom_name.clone(),
        position: instance.position,
        arguments: vec![],
        data: Box::new(closure_data),
        custom_node_type: None,
        zone: Some(Arc::new(body)),
        zone_output_arguments: vec![Argument {
            incoming_wires: result_wires,
        }],
        body_width: DEFAULT_BODY_WIDTH,
        body_height: DEFAULT_BODY_HEIGHT,
        collapse_mode: CollapseMode::default(),
    };

    // Derive C's `custom_node_type` (named zone-input pins + the single
    // `Function`-valued output) from the base `closure` type. `ensure_zone_init`
    // keeps the body and the (size-1) `zone_output_arguments` we just set.
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        &mut closure_node,
        true,
    );

    Ok(closure_node)
}

/// True iff some wire anywhere in `network` (or its sub-bodies) consumes a
/// **normal** output pin (index `>= 0`) of `node_id` — i.e. the node is used as
/// a *value*, not purely as a function through its `-1` pin.
///
/// Depth-gated against id collisions across bodies: `node_id` lives at the top
/// of `network` (nesting `k = 0`), so a consuming wire references it at
/// `source_scope_depth == k` for its own nesting `k`.
pub fn node_consumed_as_value(network: &NodeNetwork, node_id: u64) -> bool {
    consumed_as_value_rec(network, node_id, 0)
}

fn consumed_as_value_rec(network: &NodeNetwork, node_id: u64, k: u8) -> bool {
    for node in network.nodes.values() {
        for arg in node
            .arguments
            .iter()
            .chain(node.zone_output_arguments.iter())
        {
            for w in &arg.incoming_wires {
                if w.source_scope_depth == k
                    && w.source_node_id == node_id
                    && let SourcePin::NodeOutput { pin_index } = w.source_pin
                    && pin_index >= 0
                {
                    return true;
                }
            }
        }
        if let Some(body) = &node.zone
            && consumed_as_value_rec(body, node_id, k + 1)
        {
            return true;
        }
    }
    false
}

/// Redirect every consumer of `node_id`'s function pin (`NodeOutput { -1 }`) to
/// its primary output pin (`NodeOutput { 0 }`), across `network` and all its
/// sub-bodies. After *Network → Closure* the function value moved from `I`'s
/// `-1` pin to `C`'s pin `0` (same node id), so the only externally-visible
/// change is this `-1 → 0` pin flip on consuming wires.
///
/// Depth-gated identically to [`node_consumed_as_value`].
pub fn redirect_function_consumers(network: &mut NodeNetwork, node_id: u64) {
    redirect_function_consumers_rec(network, node_id, 0);
}

fn redirect_function_consumers_rec(network: &mut NodeNetwork, node_id: u64, k: u8) {
    for node in network.nodes.values_mut() {
        for arg in node
            .arguments
            .iter_mut()
            .chain(node.zone_output_arguments.iter_mut())
        {
            for w in &mut arg.incoming_wires {
                if w.source_scope_depth == k
                    && w.source_node_id == node_id
                    && matches!(w.source_pin, SourcePin::NodeOutput { pin_index: -1 })
                {
                    w.source_pin = SourcePin::NodeOutput { pin_index: 0 };
                }
            }
        }
        if let Some(body) = node.zone_mut() {
            redirect_function_consumers_rec(body, node_id, k + 1);
        }
    }
}

/// Redirect every consumer of `node_id`'s primary output pin
/// (`NodeOutput { 0 }`) to its function pin (`NodeOutput { -1 }`), across
/// `network` and all its sub-bodies. This is the *Closure → Network* mirror of
/// [`redirect_function_consumers`]: after the conversion the function value
/// moved from `C`'s pin `0` to `I`'s function pin `-1` (same node id), so the
/// only externally-visible change is this `0 → -1` pin flip on consuming wires.
///
/// Depth-gated identically to [`node_consumed_as_value`].
pub fn redirect_value_consumers(network: &mut NodeNetwork, node_id: u64) {
    redirect_value_consumers_rec(network, node_id, 0);
}

fn redirect_value_consumers_rec(network: &mut NodeNetwork, node_id: u64, k: u8) {
    for node in network.nodes.values_mut() {
        for arg in node
            .arguments
            .iter_mut()
            .chain(node.zone_output_arguments.iter_mut())
        {
            for w in &mut arg.incoming_wires {
                if w.source_scope_depth == k
                    && w.source_node_id == node_id
                    && matches!(w.source_pin, SourcePin::NodeOutput { pin_index: 0 })
                {
                    w.source_pin = SourcePin::NodeOutput { pin_index: -1 };
                }
            }
        }
        if let Some(body) = node.zone_mut() {
            redirect_value_consumers_rec(body, node_id, k + 1);
        }
    }
}

// ===========================================================================
// Direction B — Closure → Network (closure ⇒ custom instance)
// ===========================================================================

/// Absolute identity of a capture: `(external_level, source_node_id,
/// source_pin)`. The **external level** `e` is how many frames *above the host
/// scope `H`* the capture source lives (`e == 0` → source in `H`; `e >= 1` → `e`
/// frames above `H`). At this absolute level the referenced ancestor scope is
/// fixed, so `(source_node_id, source_pin)` is unambiguous — two body wires at
/// different nestings denote the *same* capture iff their `CaptureId` matches.
type CaptureId = (u8, u64, SourcePin);

/// The result of lifting a `closure` node's body into a standalone network.
pub struct ExtractionPlan {
    /// The new network `N`: parameter nodes (closure params then captures) plus
    /// the copied body, with its return node set. Its interior custom-node-type
    /// caches are populated here at build time (mirrors
    /// `create_subnetwork_from_selection`), since `N` is registered as a
    /// standalone network and a later host walk never reaches its interior.
    pub network: NodeNetwork,
    /// One wire per capture pin, in pin order, as seen from the host scope `H`.
    /// The orchestrator wires `I`'s capture pin `closure_param_count + i` to
    /// `capture_wires[i]`. For an `e == 0` capture this is a normal same-scope
    /// wire; for `e >= 1` it is a capture wire on `I` at depth `e`.
    pub capture_wires: Vec<IncomingWire>,
    /// `m` — the number of leading closure-parameter pins on `I` (left unwired,
    /// they become the `-1` value's parameters). Capture pins follow at indices
    /// `m..m + capture_wires.len()`.
    pub closure_param_count: usize,
}

/// Walks a `closure` body collecting the distinct captures in a deterministic
/// order (sorted node ids, descending into bodies; the closure result wire
/// last). The walk frame model matches [`SpliceClosureToNetwork`] exactly so
/// every boundary wire the splice encounters has a collected entry.
struct CaptureCollector {
    /// The closure node's id `C.id` (a `ZoneInput` to it is a closure parameter,
    /// not a capture).
    closure_id: u64,
    order: Vec<CaptureId>,
    seen: HashSet<CaptureId>,
}

impl CaptureCollector {
    /// Classify a single wire living at frame `k` (B-top = 0). Records a capture
    /// when the wire reaches at or above `H`.
    fn classify(&mut self, wire: &IncomingWire, k: u8) {
        let s = wire.source_scope_depth;
        // Intra-body: `NodeOutput`/`ZoneInput` resolving within `B` (`s <= k`
        // reaches `N`-top or a deeper sub-body — both stay internal after
        // lifting). Not a capture.
        if s <= k {
            return;
        }
        // Closure parameter: a reference to `C`'s own zone-input pin
        // (necessarily `s == k + 1`). Not a capture.
        if matches!(wire.source_pin, SourcePin::ZoneInput { .. })
            && wire.source_node_id == self.closure_id
        {
            return;
        }
        // Capture: external level `e = s - (k + 1)`.
        let e = s - k - 1;
        let key = (e, wire.source_node_id, wire.source_pin);
        if self.seen.insert(key) {
            self.order.push(key);
        }
    }

    fn collect_args(&mut self, args: &[Argument], k: u8) {
        for arg in args {
            for w in &arg.incoming_wires {
                self.classify(w, k);
            }
        }
    }

    /// Recurse through `body` at frame `frame`. A node's `arguments` resolve at
    /// `frame`; a zone-owning node's `zone_output_arguments` resolve against its
    /// *own* body (`frame + 1`), where its body interior also lives.
    fn collect_body(&mut self, body: &NodeNetwork, frame: u8) {
        let mut ids: Vec<u64> = body.nodes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let node = &body.nodes[&id];
            self.collect_args(&node.arguments, frame);
            if node.zone.is_some() {
                self.collect_args(&node.zone_output_arguments, frame + 1);
            }
            if let Some(nested) = &node.zone {
                self.collect_body(nested, frame + 1);
            }
        }
    }
}

/// The **Closure → Network** wire splice. Rewrites every wire in the lifted body
/// (now living in `N`) per its frame `k`: `s == k` reaches `N`-top (id remap),
/// `s < k` is a deeper verbatim sub-body (unchanged), `s >= k + 1` is a boundary
/// — a closure parameter or a capture — rewired to read the matching parameter
/// node at `N`-top (`NodeOutput { 0 }`, depth `k`).
struct SpliceClosureToNetwork<'a> {
    closure_id: u64,
    /// `B`'s top-level node id → its new (copied) id in `N`.
    id_mapping: &'a HashMap<u64, u64>,
    /// closure-param zone-input `pin_index` → its parameter node id in `N`.
    closure_param_node: &'a HashMap<usize, u64>,
    /// absolute capture identity → its parameter node id in `N`.
    capture_node: &'a HashMap<CaptureId, u64>,
}

impl SpliceClosureToNetwork<'_> {
    fn remap_wire(&self, wire: &IncomingWire, k: u8) -> IncomingWire {
        let s = wire.source_scope_depth;
        if s < k {
            // Intermediate verbatim-cloned sub-body (preserved ids): leave as-is.
            return wire.clone();
        }
        if s == k {
            // Reaches `N`-top: follow the id remap, keep pin and depth.
            let new_id = self
                .id_mapping
                .get(&wire.source_node_id)
                .copied()
                .unwrap_or(wire.source_node_id);
            return IncomingWire {
                source_node_id: new_id,
                source_pin: wire.source_pin,
                source_scope_depth: s,
            };
        }
        // s >= k + 1 — boundary. Both classes rewire to a parameter node living
        // at `N`-top, reached from frame `k` at depth `k` on `NodeOutput` pin 0.
        if let SourcePin::ZoneInput { pin_index } = wire.source_pin
            && wire.source_node_id == self.closure_id
        {
            let pnode = self
                .closure_param_node
                .get(&pin_index)
                .copied()
                .expect("closure-parameter pin has no parameter node");
            return IncomingWire {
                source_node_id: pnode,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: k,
            };
        }
        let e = s - k - 1;
        let key = (e, wire.source_node_id, wire.source_pin);
        let pnode = self
            .capture_node
            .get(&key)
            .copied()
            .expect("capture has no parameter node");
        IncomingWire {
            source_node_id: pnode,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: k,
        }
    }

    fn process_args(&self, args: &mut [Argument], k: u8) {
        for arg in args.iter_mut() {
            for wire in arg.incoming_wires.iter_mut() {
                *wire = self.remap_wire(wire, k);
            }
        }
    }

    /// Mirror of [`CaptureCollector::collect_body`] but mutating: same frame
    /// model so the classification lines up wire-for-wire.
    fn process_body(&self, body: &mut NodeNetwork, frame: u8) {
        for node in body.nodes.values_mut() {
            self.process_args(&mut node.arguments, frame);
            if node.zone.is_some() {
                self.process_args(&mut node.zone_output_arguments, frame + 1);
            }
            if let Some(nested) = node.zone_mut() {
                self.process_body(nested, frame + 1);
            }
        }
    }
}

/// Resolve a capture's parameter type and a base name from the ancestor scope it
/// lives in. `host_ancestors[0]` is `H` (external level 0), `host_ancestors[e]`
/// the scope `e` frames above. Phase 2 only ever sees `e == 0`; Phase 3 (body
/// scope) reaches `e >= 1` and `ZoneInput` (iteration-value) captures.
fn resolve_capture_type_and_name(
    host_ancestors: &[&NodeNetwork],
    registry: &NodeTypeRegistry,
    key: &CaptureId,
) -> Result<(DataType, String), String> {
    let (e, src_id, src_pin) = *key;
    let net = host_ancestors.get(e as usize).ok_or_else(|| {
        "Capture reaches above the available scope chain (the closure is malformed)".to_string()
    })?;
    let node = net
        .nodes
        .get(&src_id)
        .ok_or_else(|| "Capture source node not found in its scope".to_string())?;
    match src_pin {
        SourcePin::NodeOutput { pin_index } => {
            let dt = registry
                .resolve_output_type(node, net, pin_index)
                .unwrap_or(DataType::None);
            let base = node
                .custom_name
                .clone()
                .unwrap_or_else(|| node.node_type_name.clone());
            Ok((dt, format!("{base}_cap")))
        }
        // Capturing an enclosing HOF's iteration value (`element` / `acc`). The
        // source node is that HOF (living `e` frames above `H`); the captured
        // value's type is the HOF's declared zone-input pin `pin_index`. This
        // only arises when the closure is nested inside another HOF body — i.e.
        // a body-scope conversion (`e >= 1`).
        SourcePin::ZoneInput { pin_index } => {
            let hof_type = registry
                .get_node_type_for_node(node)
                .ok_or_else(|| "Capture source HOF has no node type".to_string())?;
            let pin = hof_type
                .zone_input_pins
                .get(pin_index)
                .ok_or_else(|| "Capture references an invalid zone-input pin".to_string())?;
            // For the common `Fixed(concrete)` case this is the concrete
            // iteration-value type; a polymorphic declaration dead-ends at
            // `None` (concrete-only, like every other unresolved pin here).
            let dt = match &pin.data_type {
                PinOutputType::Fixed(dt) => dt.clone(),
                _ => DataType::None,
            };
            let base = node
                .custom_name
                .clone()
                .unwrap_or_else(|| node.node_type_name.clone());
            Ok((dt, format!("{base}_{}_cap", pin.name)))
        }
    }
}

/// Return a name not already in `taken`, appending `_2`, `_3`, … on collision,
/// and record the chosen name in `taken`.
fn make_unique_name(desired: &str, taken: &mut HashSet<String>) -> String {
    if taken.insert(desired.to_string()) {
        return desired.to_string();
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{desired}_{suffix}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

/// Build the standalone network `N` from a `closure` node `C`'s body — the
/// **Closure → Network** direction.
///
/// `host_ancestors[0]` is the host scope `H` (the network directly containing
/// `C`), `host_ancestors[e]` the scope `e` frames above. Top-level extraction
/// passes `&[H]` (every capture at external level 0); a body-scope extraction
/// passes the full chain `[H, parent, …, top]` so `e >= 1` captures resolve.
///
/// Returns the network to register plus the capture wires `I` must carry and the
/// closure-parameter count. The orchestrator registers `N`, builds `I` (reusing
/// `C`'s id), wires its capture pins to `capture_wires`, and flips consumers of
/// `C`'s pin `0` to `I`'s pin `-1`.
///
/// Errors on a malformed / lossy input: no body, no result wire, a result drawn
/// from a secondary output pin, or a capture that needs a deeper scope phase.
pub fn extract_network_from_closure(
    closure: &Node,
    network_name: &str,
    host_ancestors: &[&NodeNetwork],
    registry: &NodeTypeRegistry,
) -> Result<ExtractionPlan, String> {
    // 1. The closure body `B`.
    let body = closure
        .zone
        .as_ref()
        .ok_or_else(|| "The closure has no body".to_string())?;

    // 2. The result wire (`C.zone_output_arguments[0]`'s first wire).
    let result_wire = closure
        .zone_output_arguments
        .first()
        .and_then(|arg| arg.incoming_wires.first())
        .cloned()
        .ok_or_else(|| "The closure has no result".to_string())?;
    if let SourcePin::NodeOutput { pin_index } = result_wire.source_pin
        && pin_index > 0
    {
        return Err("The closure result comes from a secondary output pin".to_string());
    }

    // 3. Closure parameters, read straight off the `ClosureData` shape (types in
    //    `type_args`, names from the kind's labels). `m` of them.
    let closure_data = closure
        .data
        .as_ref()
        .as_any_ref()
        .downcast_ref::<ClosureData>()
        .ok_or_else(|| "Node is not a closure".to_string())?;
    let cp_types = closure_data
        .kind
        .param_types(&closure_data.type_args, &closure_data.param_names);
    let cp_names = closure_data.kind.param_names(&closure_data.param_names);
    let m = cp_types.len();

    // 4. Collect distinct captures (body interior + the result wire).
    let mut collector = CaptureCollector {
        closure_id: closure.id,
        order: Vec::new(),
        seen: HashSet::new(),
    };
    collector.collect_body(body, 0);
    collector.classify(&result_wire, 0);
    let captures = collector.order;

    // 5. Resolve each capture's type + base name from the ancestor scopes.
    let mut capture_types: Vec<DataType> = Vec::with_capacity(captures.len());
    let mut capture_base_names: Vec<String> = Vec::with_capacity(captures.len());
    for key in &captures {
        let (dt, name) = resolve_capture_type_and_name(host_ancestors, registry, key)?;
        capture_types.push(dt);
        capture_base_names.push(name);
    }

    // 6. Build `N`. Parameter nodes come first (ids 1..=m+c), then the body is
    //    copied (fresh ids after that). Names are de-duplicated across the whole
    //    parameter set: closure params keep their authored names, captures append
    //    `_cap` (then `_cap_2`, …) on collision.
    let mut network = NodeNetwork::new_empty();
    let mut taken_names: HashSet<String> = HashSet::new();

    // Closure-parameter node bookkeeping for the splice (zone-input pin → node).
    let mut closure_param_node: HashMap<usize, u64> = HashMap::new();
    let mut capture_node: HashMap<CaptureId, u64> = HashMap::new();
    let mut type_params: Vec<Parameter> = Vec::with_capacity(m + captures.len());

    // Closure parameters: param_index 0..m.
    for (cp, ptype) in cp_types.iter().enumerate() {
        let raw = cp_names
            .get(cp)
            .cloned()
            .unwrap_or_else(|| format!("p{cp}"));
        let name = make_unique_name(&raw, &mut taken_names);
        let pid = add_parameter_node(&mut network, cp, &name, ptype.clone());
        closure_param_node.insert(cp, pid);
        type_params.push(Parameter {
            id: Some(cp as u64 + 1),
            name,
            data_type: ptype.clone(),
        });
    }

    // Captures: param_index m..m+c.
    for (i, key) in captures.iter().enumerate() {
        let param_index = m + i;
        let name = make_unique_name(&capture_base_names[i], &mut taken_names);
        let pid = add_parameter_node(&mut network, param_index, &name, capture_types[i].clone());
        capture_node.insert(*key, pid);
        type_params.push(Parameter {
            id: Some(param_index as u64 + 1),
            name,
            data_type: capture_types[i].clone(),
        });
    }

    // Copy `B`'s top-level nodes into `N` (bodies verbatim, fresh B-top ids).
    let (content_min, _content_size) = content_bounding_box(body, registry);
    let id_mapping = copy_content_into(&mut network, body, DVec2::ZERO, content_min);

    // Splice every wire in the copied content (incl. nested bodies' arguments and
    // zone-output wires) from the closure's frame model into `N`'s.
    let splice = SpliceClosureToNetwork {
        closure_id: closure.id,
        id_mapping: &id_mapping,
        closure_param_node: &closure_param_node,
        capture_node: &capture_node,
    };
    let copied_ids: Vec<u64> = id_mapping.values().copied().collect();
    for &new_id in &copied_ids {
        if let Some(node) = network.nodes.get_mut(&new_id) {
            splice.process_args(&mut node.arguments, 0);
            if node.zone.is_some() {
                splice.process_args(&mut node.zone_output_arguments, 1);
            }
            if let Some(nested) = node.zone_mut() {
                splice.process_body(nested, 1);
            }
        }
    }

    // 7. Set `N`'s return node from the (single) result wire, classified at
    //    frame 0 the same way the splice classifies wires.
    let return_node_id = resolve_return_node(
        &result_wire,
        closure.id,
        &id_mapping,
        &closure_param_node,
        &capture_node,
    )?;
    network.return_node_id = Some(return_node_id);

    // 8. Finish `N`'s node type: parameters (built above) + output pins
    //    (multi-output passthrough from the return node). Populate the interior
    //    caches first so polymorphic return pins resolve.
    network.node_type = NodeType {
        name: network_name.to_string(),
        description: "Custom node extracted from a closure".to_string(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: type_params,
        output_pins: OutputPinDefinition::single(DataType::None), // replaced below
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(CustomNodeData::default()),
        node_data_saver: generic_node_data_saver::<CustomNodeData>,
        node_data_loader: generic_node_data_loader::<CustomNodeData>,
    };
    registry.initialize_custom_node_types_for_network(&mut network);
    network.node_type.output_pins = resolved_return_output_pins(&network, return_node_id, registry);

    // 9. The capture wires `I` must carry, in pin order, as seen from `H`.
    let capture_wires: Vec<IncomingWire> = captures
        .iter()
        .map(|(e, src_id, src_pin)| IncomingWire {
            source_node_id: *src_id,
            source_pin: *src_pin,
            source_scope_depth: *e,
        })
        .collect();

    Ok(ExtractionPlan {
        network,
        capture_wires,
        closure_param_count: m,
    })
}

/// Create a `parameter` node in `network` at `param_index` with the given name
/// and type, mirroring `create_subnetwork_from_selection`'s parameter-node
/// construction. Returns the new node's id.
fn add_parameter_node(
    network: &mut NodeNetwork,
    param_index: usize,
    name: &str,
    data_type: DataType,
) -> u64 {
    let param_id = network.next_node_id;
    network.next_node_id += 1;

    let position = DVec2::new(-300.0, param_index as f64 * 80.0);
    let param_data = ParameterData {
        param_id: Some(network.next_param_id),
        param_index,
        param_name: name.to_string(),
        data_type,
        sort_order: param_index as i32,
        data_type_str: None,
        error: None,
    };
    network.next_param_id += 1;

    let node = Node {
        id: param_id,
        node_type_name: "parameter".to_string(),
        custom_name: Some(name.to_string()),
        position,
        arguments: vec![Argument::new()],
        data: Box::new(param_data),
        custom_node_type: None,
        zone: None,
        zone_output_arguments: Vec::new(),
        body_width: DEFAULT_BODY_WIDTH,
        body_height: DEFAULT_BODY_HEIGHT,
        collapse_mode: CollapseMode::Auto,
    };
    network.nodes.insert(param_id, node);
    param_id
}

/// Classify the closure's result wire (at frame 0) to find `N`'s return node:
/// a copied body node (id-remapped), or a parameter node (closure-param /
/// capture) when the closure forwards an argument or captured value directly.
fn resolve_return_node(
    result_wire: &IncomingWire,
    closure_id: u64,
    id_mapping: &HashMap<u64, u64>,
    closure_param_node: &HashMap<usize, u64>,
    capture_node: &HashMap<CaptureId, u64>,
) -> Result<u64, String> {
    let s = result_wire.source_scope_depth;
    if s == 0 {
        // Reads a copied body node (the common case). `NodeOutput { 0 }` is
        // guaranteed by the secondary-pin gate above.
        return id_mapping
            .get(&result_wire.source_node_id)
            .copied()
            .ok_or_else(|| "The closure result wire has no valid source".to_string());
    }
    // Passthrough: the result is a closure parameter or a capture.
    if let SourcePin::ZoneInput { pin_index } = result_wire.source_pin
        && result_wire.source_node_id == closure_id
    {
        return closure_param_node
            .get(&pin_index)
            .copied()
            .ok_or_else(|| "The closure result references an unknown parameter".to_string());
    }
    let e = s - 1;
    let key = (e, result_wire.source_node_id, result_wire.source_pin);
    capture_node
        .get(&key)
        .copied()
        .ok_or_else(|| "The closure result references an unknown capture".to_string())
}

/// `N`'s output pins from its return node — multi-output passthrough, with
/// polymorphic pins substituted by their resolved concrete type (mirrors
/// `network_validator::update_network_output_type`).
fn resolved_return_output_pins(
    network: &NodeNetwork,
    return_node_id: u64,
    registry: &NodeTypeRegistry,
) -> Vec<OutputPinDefinition> {
    let Some(return_node) = network.nodes.get(&return_node_id) else {
        return OutputPinDefinition::single(DataType::None);
    };
    let Some(return_node_type) = registry.get_node_type_for_node(return_node) else {
        return OutputPinDefinition::single(DataType::None);
    };
    return_node_type
        .output_pins
        .iter()
        .enumerate()
        .map(|(i, pin)| {
            let data_type = match &pin.data_type {
                PinOutputType::Fixed(_) => pin.data_type.clone(),
                _ => PinOutputType::Fixed(
                    registry
                        .resolve_output_type(return_node, network, i as i32)
                        .unwrap_or(DataType::None),
                ),
            };
            OutputPinDefinition {
                name: pin.name.clone(),
                data_type,
                id: pin.id,
            }
        })
        .collect()
}
