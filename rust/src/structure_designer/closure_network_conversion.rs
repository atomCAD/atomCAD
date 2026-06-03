//! Convert a custom-network instance ⇄ a `closure` node — the closure-aware
//! analogue of *Inline a Custom Node* (`node_inlining.rs`) and *Factor Selection
//! into Subnetwork* (`selection_factoring.rs`). See
//! `doc/design_closure_network_conversion.md`.
//!
//! **Phase 1 (this file): Network → Closure, top level.** Replacing a
//! custom-network instance node `I` (whose function pin is used, or which is
//! unconsumed) with a `closure` node `C` whose inline body is a copy of `I`'s
//! network `N`. `I`'s **wired** input pins become **captures** in the body; its
//! **unwired** input pins become the closure's **parameters** (zone-input pins).
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
use std::collections::HashMap;
use std::sync::Arc;

use super::data_type::DataType;
use super::node_inlining::{content_bounding_box, copy_content_into};
use super::node_network::{
    Argument, CollapseMode, DEFAULT_BODY_HEIGHT, DEFAULT_BODY_WIDTH, IncomingWire, Node,
    NodeNetwork, SourcePin,
};
use super::node_type_registry::NodeTypeRegistry;
use super::nodes::closure::{ClosureData, ClosureKind};
use super::nodes::parameter::ParameterData;

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
    let closure_data = ClosureData {
        kind: ClosureKind::Custom,
        type_args,
        param_names: closure_param_names,
        custom_label: None,
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
