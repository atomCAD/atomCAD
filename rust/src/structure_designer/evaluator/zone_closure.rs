//! Zone closures: the detached, runnable bundle behind every HOF body.
//!
//! A [`ZoneClosure`] is exactly the four flattened fields a `Walker::MapZone`
//! used to carry loose — `{ body, captures, zone_output_wires, owner_node_id }`
//! — plus the type metadata (`param_types`, `return_type`) that lets the bundle
//! be treated as a typed function value. It is the "per-element computation,
//! ready to run". An inline HOF body, a `closure` node's body, and a body wired
//! into an HOF's `f` pin are three sources of the *same* bundle, all consumed by
//! the same substrate. See `doc/design_closures.md`.
//!
//! Phase 1 introduced the bundle and routed the four existing HOFs through it
//! with no user-visible change. As of Phase 2 the bundle is also the payload of
//! `NetworkResult::Function` — a first-class (but not yet user-constructible)
//! function value.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::structure_designer::data_type::{DataType, FunctionType};
use crate::structure_designer::evaluator::network_evaluator::{
    CaptureKey, NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_network::{Argument, IncomingWire, NodeNetwork, SourcePin};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;

/// A detached zone body bundled with everything needed to run it: the body
/// network, its pre-evaluated (frozen) captures, the wire(s) delivering the
/// body's result, the scope-stack key for iteration frames, and the
/// arity/type metadata.
///
/// All fields are `Arc`-backed or plain `Copy`/small, so `Clone` is cheap
/// (refcount bumps only) — this keeps `Walker`'s clone-independence invariant
/// (Invariant 2) holding for the variants that embed a `ZoneClosure`.
#[derive(Clone)]
pub struct ZoneClosure {
    /// The body network. CoW-shared, cheap to clone (Arc bump).
    pub body: Arc<NodeNetwork>,
    /// Captured environment, pre-evaluated and frozen at definition time.
    pub captures: Arc<HashMap<CaptureKey, NetworkResult>>,
    /// One wire per zone-output pin, delivering the body's result(s).
    pub zone_output_wires: Arc<Vec<IncomingWire>>,
    /// Scope-stack key for iteration frames: the id of the node that *owns*
    /// the body (the HOF node for an inline body). Determines which
    /// `current_zone_input_values` entry the consumer pushes frames onto. This
    /// key is **not unique** across networks; see `doc/design_closures.md`
    /// (§"`owner_node_id`: the model's one conceptual debt") for why that is
    /// nonetheless safe.
    pub owner_node_id: u64,
    /// Arity/types, mirrored from the owner's zone pins. Carried so a consumer
    /// can sanity-check shape and so the value's `DataType::Function` can be
    /// inferred. Unused in Phase 1 — populated for Phase 2+ where the bundle
    /// becomes a typed `NetworkResult::Function`.
    pub param_types: Vec<DataType>,
    pub return_type: DataType,
}

impl ZoneClosure {
    /// The function type this closure value carries, derived from its
    /// arity/return metadata. Used to infer the `DataType::Function` of a
    /// `NetworkResult::Function(ZoneClosure)` value and for its display string.
    pub fn function_type(&self) -> FunctionType {
        FunctionType::new(self.param_types.clone(), self.return_type.clone())
    }
}

/// Build a [`ZoneClosure`] from an HOF node's own inline zone body.
///
/// This is the inline-body logic factored out of the four HOFs (`map`,
/// `filter`, `fold`, `foreach`): grab `node.zone`, collect the
/// `zone_output_arguments` wire(s), pre-evaluate captures once via
/// [`build_captures`] against a body-pushed stack, and fill `owner_node_id` and
/// the type metadata. `label` is the HOF's name, used only to prefix eval-time
/// error strings so they read identically to the pre-refactor messages.
///
/// On a malformed body (missing zone, no zone-output wire, capture failure)
/// returns `Err(NetworkResult::Error(_))` so callers can `return
/// EvalOutput::single(e)` directly.
///
/// `NetworkResult` is a large enum, so the `Err` variant trips
/// `clippy::result_large_err`; we keep the un-boxed `NetworkResult` error
/// (matching the design's `obtain_closure` signature and letting callers
/// forward it without a deref) and silence the lint.
#[allow(clippy::result_large_err)]
pub fn build_inline_closure<'a>(
    evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    label: &str,
) -> Result<ZoneClosure, NetworkResult> {
    let node = NetworkStackElement::get_top_node(network_stack, node_id);

    // Grab the body. A populated HOF without a zone is ill-formed — populate
    // runs at add_node time.
    let body = match node.zone.as_ref() {
        Some(b) => Arc::clone(b),
        None => {
            return Err(NetworkResult::Error(format!("{label}: missing zone body")));
        }
    };

    // The zone-output pin must have at least one incoming wire — otherwise the
    // body cannot deliver a per-iteration result. Reports as an eval-time
    // error; Phase 5 will surface this at validation time as well.
    let zone_output_wires: Vec<IncomingWire> = match node.zone_output_arguments.first() {
        Some(arg) if !arg.incoming_wires.is_empty() => arg.incoming_wires.clone(),
        _ => {
            return Err(NetworkResult::Error(format!(
                "{label}: body has no incoming wire on zone-output pin"
            )));
        }
    };

    // Pre-evaluate captures. Push the body so `source_scope_depth` walks land
    // correctly, pre-evaluate against the caller's context, then drop the
    // pushed stack.
    let mut body_stack = network_stack.to_vec();
    body_stack.push(NetworkStackElement {
        node_network: body.as_ref(),
        node_id,
    });

    let captures = match build_captures(
        evaluator,
        &body_stack,
        registry,
        context,
        body.as_ref(),
        &zone_output_wires,
    ) {
        Ok(c) => c,
        Err(err) => return Err(NetworkResult::Error(format!("{label}: {err}"))),
    };

    // Arity/type metadata mirrored from the owner's resolved zone pins. Unused
    // in Phase 1; carried so a later `NetworkResult::Function` can infer its
    // `DataType::Function`.
    let (param_types, return_type) = match registry.get_node_type_for_node(node) {
        Some(nt) => {
            let param_types = nt
                .zone_input_pins
                .iter()
                .map(|opd| opd.fixed_type().cloned().unwrap_or(DataType::None))
                .collect();
            let return_type = nt
                .zone_output_pins
                .first()
                .map(|p| p.data_type.clone())
                .unwrap_or(DataType::None);
            (param_types, return_type)
        }
        None => (Vec::new(), DataType::None),
    };

    Ok(ZoneClosure {
        body,
        captures,
        zone_output_wires: Arc::new(zone_output_wires),
        owner_node_id: node_id,
        param_types,
        return_type,
    })
}

/// Build a [`ZoneClosure`] from a non-HOF node viewed as a function of *all*
/// its inputs — the "function pin" producer (`output_pin_index == -1`).
///
/// The synthesized closure's body is a one-node synthetic network holding a
/// clone of `N`, with **every** input pin fed from a zone-input parameter and
/// **no captures**. Per element the consumer pushes the iteration values as the
/// parameter frame and resolves the result wire — exactly the existing
/// [`run_closure_once`] step, so a function-pin wire drops into an HOF's `f`
/// pin / `apply` identically to a `closure` node's output. See
/// `doc/design_function_pins.md`.
///
/// The synthesized function type is `(all input pins) -> output pin 0` —
/// exactly `NodeType::get_function_type()`. We read its parts directly so the
/// body wiring and the carried `param_types` / `return_type` stay in lock-step.
///
/// Rejects (`Err(NetworkResult::Error(_))`):
/// - a node with **zero inputs** — a `() -> T` function matches no HOF; and
/// - a node with an **unresolved / polymorphic** pin-0 output type
///   (`SameAsInput` / `SameAsArrayElements` read as `DataType::None` here): its
///   function-type return is unknowable, since the function-mode rule keeps
///   every input disconnected. See design Open Question 2.
///
/// `_evaluator` is unused (a no-capture closure needs no pre-evaluation) but
/// kept for signature symmetry with [`build_inline_closure`] and the design's
/// `evaluate` call site.
///
/// `NetworkResult` is a large enum, so the `Err` variant trips
/// `clippy::result_large_err`; we keep the un-boxed error so the `evaluate`
/// `-1` branch can forward it directly.
#[allow(clippy::result_large_err)]
#[allow(clippy::arc_with_non_send_sync)]
pub fn build_node_function_closure<'a>(
    _evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,
    registry: &NodeTypeRegistry,
) -> Result<ZoneClosure, NetworkResult> {
    let node = NetworkStackElement::get_top_node(network_stack, node_id);

    let node_type = match registry.get_node_type_for_node(node) {
        Some(nt) => nt,
        None => {
            return Err(NetworkResult::Error(format!(
                "function pin: unknown node type '{}'",
                node.node_type_name
            )));
        }
    };

    let param_types: Vec<DataType> = node_type
        .parameters
        .iter()
        .map(|p| p.data_type.clone())
        .collect();
    let return_type = node_type.output_type().clone();

    if param_types.is_empty() {
        return Err(NetworkResult::Error(format!(
            "function pin: '{}' has no inputs, so its function type () -> _ matches no consumer",
            node.node_type_name
        )));
    }
    if return_type == DataType::None {
        return Err(NetworkResult::Error(format!(
            "function pin: '{}' has an unresolved (polymorphic) output type",
            node.node_type_name
        )));
    }

    // The body node: a clone of N with a fresh body-local id, every input pin
    // fed from a zone-input parameter (depth 1, keyed by N's id). N's inputs
    // are guaranteed empty by the function-mode rule, so these fill empty slots
    // — no live wire is discarded. N is non-HOF, so its `zone` /
    // `zone_output_arguments` are already empty and come along inert.
    const BODY_NODE_ID: u64 = 1;
    let owner_key = node_id; // scope-frame key; distinct in role from BODY_NODE_ID

    let mut body_node = node.clone();
    body_node.id = BODY_NODE_ID;
    body_node.arguments = (0..param_types.len())
        .map(|i| {
            let mut arg = Argument::new();
            arg.set_source_full(owner_key, SourcePin::ZoneInput { pin_index: i }, 1);
            arg
        })
        .collect();

    // The synthetic one-node body network (mirrors `NodeNetwork::new_empty()`
    // shape, `next_node_id` ahead of the body node id).
    let mut body_network = NodeNetwork::new_empty();
    body_network.nodes.insert(BODY_NODE_ID, body_node);
    body_network.next_node_id = BODY_NODE_ID + 1;

    Ok(ZoneClosure {
        body: Arc::new(body_network),
        captures: Arc::new(HashMap::new()),
        zone_output_wires: Arc::new(vec![IncomingWire {
            source_node_id: BODY_NODE_ID,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
        }]),
        owner_node_id: owner_key,
        param_types,
        return_type,
    })
}

/// Yield the [`ZoneClosure`] an HOF should run for this evaluation.
///
/// If the node's `f` pin (at `f_param_index`) is wired, evaluate it and take
/// the carried function value. Otherwise — `f` disconnected — fall back to the
/// node's own inline zone via [`build_inline_closure`]. This is what lets the
/// four HOFs accept *either* a wired-in function value or their own inline
/// body, with one branch. An `apply` node, which has no inline body, never
/// reaches the fallback (its `f` is required) and so does not use this helper.
///
/// `label` prefixes eval-time error strings, matching [`build_inline_closure`].
///
/// `NetworkResult` is a large enum, so the `Err` variant trips
/// `clippy::result_large_err`; we keep the un-boxed error so callers can
/// `return EvalOutput::single(e)` directly.
#[allow(clippy::result_large_err)]
pub fn obtain_closure<'a>(
    evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    f_param_index: usize,
    label: &str,
) -> Result<ZoneClosure, NetworkResult> {
    match evaluator.evaluate_arg(network_stack, node_id, registry, context, f_param_index) {
        // `f` is wired and carries a function value — run that.
        NetworkResult::Function(zc) => Ok(zc),
        // An error resolving `f` propagates as this node's error.
        e @ NetworkResult::Error(_) => Err(e),
        // `f` not connected — fall back to this node's own inline zone body.
        NetworkResult::None => {
            build_inline_closure(evaluator, network_stack, node_id, registry, context, label)
        }
        other => Err(NetworkResult::Error(format!(
            "{label}: f is not a function (got {})",
            other.to_display_string()
        ))),
    }
}

/// Run a closure once on a single argument frame — the per-element step shared
/// by the lazy walkers (`MapZone`, `FilterZone`) and the eager HOFs (`fold`,
/// `foreach`); later phases (`apply`, the `f`-driven HOFs) reuse it verbatim.
///
/// Swaps the closure's frozen captures into `context`, pushes `args` as the
/// iteration frame keyed by `owner_node_id`, resolves the zone-output wire
/// ([`eval_step`]) against `network_stack` with the closure's body pushed on
/// top, then pops the frame and restores the caller's captures. The whole step
/// is bracketed by its own push/pop, so it is safe to nest under a colliding
/// `owner_node_id` (see `doc/design_closures.md`, §"`owner_node_id`: the
/// model's one conceptual debt").
///
/// `network_stack` is the base stack the body is pushed onto. The eager HOFs
/// pass their real containing-network stack, so a *nested* HOF inside the body
/// can still resolve captures that reach past the immediate body (e.g. a
/// grandparent constant at `source_scope_depth == 2`). The lazy walkers don't
/// hold the outer stack and pass `&[]` (body-only) — their bodies' own deep
/// captures are pre-frozen at the producing HOF's `eval`, so the body-only
/// stack is sufficient for them. See `doc/design_zones.md` (§"Sub-context
/// pattern").
pub fn run_closure_once<'a>(
    evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    closure: &ZoneClosure,
    args: Vec<NetworkResult>,
) -> NetworkResult {
    // Swap captures in for the duration of the step.
    let saved_captures = std::mem::replace(
        &mut context.captured_source_values,
        Arc::clone(&closure.captures),
    );

    context.push_zone_input_frame(closure.owner_node_id, args);

    // Push the closure's body onto the base stack. For the lazy walkers
    // (`network_stack == &[]`) this is a body-only stack; for the eager HOFs
    // it is the full containing-network stack + body.
    let mut body_stack = network_stack.to_vec();
    body_stack.push(NetworkStackElement {
        node_network: closure.body.as_ref(),
        node_id: closure.owner_node_id,
    });

    let result = eval_step(
        evaluator,
        &body_stack,
        registry,
        context,
        &closure.zone_output_wires,
    );

    context.pop_zone_input_frame(closure.owner_node_id);
    context.captured_source_values = saved_captures;

    result
}

/// Walk the body for capture wires and pre-evaluate them once at body entry.
///
/// A wire is a *capture* iff its source is outside this body — see
/// [`is_capture`]. Each unique source-side identity (`CaptureKey`) is
/// evaluated once and stored in the cache.
///
/// We do **not** clear the caller's existing `captured_source_values` while
/// building — for a nested HOF, the outer body's captures are already
/// installed there (via the lazy walker's captures swap) and would be needed
/// when resolving an inner capture whose source itself reads through the outer
/// body's captures.
#[allow(clippy::arc_with_non_send_sync)]
fn build_captures<'a>(
    evaluator: &NetworkEvaluator,
    body_stack: &[NetworkStackElement<'a>],
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    body: &NodeNetwork,
    zone_output_wires: &[IncomingWire],
) -> Result<Arc<HashMap<CaptureKey, NetworkResult>>, String> {
    let mut seen: HashSet<CaptureKey> = HashSet::new();
    let mut captures: HashMap<CaptureKey, NetworkResult> = HashMap::new();

    // Collect every wire on every body-internal node, plus the wires
    // terminating at the HOF's zone-output arguments. The iteration order is
    // deterministic so capture insertion order is stable across runs.
    let mut body_node_ids: Vec<u64> = body.nodes.keys().copied().collect();
    body_node_ids.sort();

    let mut wires_to_check: Vec<IncomingWire> = Vec::new();
    for nid in body_node_ids {
        if let Some(node) = body.nodes.get(&nid) {
            for arg in &node.arguments {
                for w in &arg.incoming_wires {
                    wires_to_check.push(w.clone());
                }
            }
        }
    }
    for w in zone_output_wires {
        wires_to_check.push(w.clone());
    }

    for incoming in &wires_to_check {
        if !is_capture(incoming) {
            continue;
        }
        let key = CaptureKey::from_incoming(incoming);
        if !seen.insert(key.clone()) {
            continue;
        }
        let value = resolve_capture_source(evaluator, body_stack, registry, context, incoming);

        if let NetworkResult::Error(e) = &value {
            return Err(format!("failed to pre-evaluate capture: {}", e));
        }
        captures.insert(key, value);
    }

    Ok(Arc::new(captures))
}

/// A wire is a capture iff its source is outside the body that contains its
/// destination.
fn is_capture(w: &IncomingWire) -> bool {
    match w.source_pin {
        // Local body-internal wire: not a capture.
        SourcePin::NodeOutput { .. } => w.source_scope_depth > 0,
        // `ZoneInput { depth = 1 }` references the immediately-enclosing
        // HOF's iteration values — per-iteration, not a capture. Deeper
        // references go through the cache.
        SourcePin::ZoneInput { .. } => w.source_scope_depth > 1,
    }
}

/// Resolve a capture wire's source value during pre-evaluation.
///
/// Walk `source_scope_depth` levels up the stack and evaluate via the normal
/// path. `ZoneInput` captures (depth > 1) read from the live
/// `current_zone_input_values` of the enclosing HOF, which is correct because
/// at capture-build time that frame is the outer iteration's current frame.
fn resolve_capture_source<'a>(
    evaluator: &NetworkEvaluator,
    body_stack: &[NetworkStackElement<'a>],
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    incoming: &IncomingWire,
) -> NetworkResult {
    let depth = incoming.source_scope_depth as usize;
    let stack_len = body_stack.len();
    match incoming.source_pin {
        SourcePin::NodeOutput { pin_index } => {
            // depth >= 1 (caller guaranteed via `is_capture`).
            let source_frame_idx = stack_len.saturating_sub(1 + depth);
            let source_slice = &body_stack[..=source_frame_idx];
            evaluator.evaluate(
                source_slice,
                incoming.source_node_id,
                pin_index,
                registry,
                false,
                context,
            )
        }
        SourcePin::ZoneInput { pin_index } => {
            // depth > 1 (caller guaranteed). Read the live iteration value of
            // the referenced outer HOF.
            context
                .current_zone_input(incoming.source_node_id, pin_index)
                .clone()
        }
    }
}

/// Resolve the first zone-output wire against `body_stack` (whose top frame is
/// the closure's body). Caller has already pushed the iteration frame and
/// swapped the frozen captures in.
///
/// Resolution mirrors `resolve_incoming_wire`'s order of checks:
/// * Body-local wires (`source_scope_depth == 0`) resolve against the body
///   frame at the top of `body_stack`.
/// * Outer-scope wires (`source_scope_depth > 0`) hit the capture cache
///   (checked first, before any stack walk).
/// * `ZoneInput` wires referencing the immediately enclosing HOF read from
///   `current_zone_input_values` via the live-lookup path.
fn eval_step<'a>(
    evaluator: &NetworkEvaluator,
    body_stack: &[NetworkStackElement<'a>],
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    zone_output_wires: &[IncomingWire],
) -> NetworkResult {
    let incoming = match zone_output_wires.first() {
        Some(w) => w,
        None => {
            return NetworkResult::Error(
                "HOF body has no incoming wire on zone-output pin".to_string(),
            );
        }
    };

    // Capture-cache short-circuit: zone-output wires are always body-local
    // (`source_scope_depth == 0`), so this should never fire for them, but we
    // mirror `resolve_incoming_wire`'s order-of-checks for safety.
    let key = CaptureKey::from_incoming(incoming);
    if let Some(cached) = context.captured_source_values.get(&key) {
        return cached.clone();
    }

    match incoming.source_pin {
        SourcePin::NodeOutput { pin_index } => evaluator.evaluate(
            body_stack,
            incoming.source_node_id,
            pin_index,
            registry,
            false,
            context,
        ),
        SourcePin::ZoneInput { pin_index } => {
            // A body-return wire that sources from the HOF's own zone-input
            // pin: legal (passes through the iteration value unchanged). Read
            // from the live frame.
            context
                .current_zone_input(incoming.source_node_id, pin_index)
                .clone()
        }
    }
}
