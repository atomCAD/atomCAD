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
//! Phase 1 introduces the bundle and routes the four existing HOFs through it
//! with no user-visible change. The struct is not yet a `NetworkResult` value;
//! Phase 2 makes `NetworkResult::Function` carry it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::{
    CaptureKey, NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_network::{IncomingWire, NodeNetwork, SourcePin};
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
