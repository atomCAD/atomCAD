//! Lazy iterator runtime: the `Walker` tree.
//!
//! `Walker` is the runtime payload of `NetworkResult::Iterator(_)`. It is a
//! unified tree (no separate immutable recipe type) whose `next()` advances
//! by one element at a time, fusing source → map → filter → fold etc. without
//! materializing intermediate `Vec<NetworkResult>`s. Design doc:
//! `doc/design_iterators.md`.
//!
//! Cloning produces an independent walker (Invariant 2 in the design doc):
//! every read site in the evaluator (`EvalOutput::get`, `evaluate_required`,
//! `parameter::eval`, …) clones the enclosing `NetworkResult`, so two clones
//! must advance independently of each other. The enum encodes that with owned
//! state on every variant; only `FromArray::items` is `Arc`-shared so cloning
//! a 10⁶-element wrapped array is O(1).

use std::collections::HashMap;
use std::sync::Arc;

use crate::structure_designer::evaluator::function_evaluator::FunctionEvaluator;
use crate::structure_designer::evaluator::network_evaluator::{
    CaptureKey, NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_network::{IncomingWire, NodeNetwork, SourcePin};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;

/// Lazy stream of `NetworkResult` values. The outer `fused` flag uniformly
/// enforces the "Error once, then `None`" contract across all variants — see
/// `Walker::next` below.
#[derive(Clone)]
pub struct Walker {
    kind: WalkerKind,
    fused: bool,
}

#[derive(Clone)]
enum WalkerKind {
    /// Wrapper around an already-materialized array. `items` is `Arc`-shared
    /// across walker clones so cloning is O(1) regardless of the array's
    /// length; `idx` is per-walker, so independent advancement of clones is
    /// preserved (Invariant 2).
    FromArray {
        items: Arc<Vec<NetworkResult>>,
        idx: usize,
    },
    /// Emits `start, start+step, …, start+(count-1)*step`.
    Range {
        start: i32,
        step: i32,
        count: i32,
        emitted: i32,
    },
    /// `map` driven by an inline zone body. The body and captures are shared
    /// via `Arc` so walker clones (Invariant 2) pay only refcount bumps. Per
    /// `next()` the walker stands up a synthetic network stack of just this
    /// body, pushes a fresh frame onto
    /// `current_zone_input_values[hof_node_id]`, swaps captures into the
    /// caller's context, evaluates the wire delivering the body's `result`
    /// zone-output pin (resolved against the synthetic body-only stack),
    /// then pops the frame and restores the caller's captures.
    ///
    /// We carry `zone_output_wires` rather than reach back through the outer
    /// network stack (which `Walker::next` doesn't have access to) — the
    /// design relies on captures being pre-evaluated at body entry so the
    /// body-only synthetic stack is sufficient at runtime. See
    /// `doc/design_zones.md` (Phase 4, §"Sub-context pattern").
    MapZone {
        source: Box<Walker>,
        body: Arc<NodeNetwork>,
        captures: Arc<HashMap<CaptureKey, NetworkResult>>,
        zone_output_wires: Arc<Vec<IncomingWire>>,
        hof_node_id: u64,
    },
    /// Legacy FE-driven `map`. No production node constructs this variant
    /// after Phase 4 — `map` flips to `MapZone` — but the path is kept alive
    /// as dead weight so unit tests targeting the FE walker plumbing keep
    /// working until Phase 5 retires `FunctionEvaluator` and `Closure`
    /// outright. See `doc/design_zones.md` (Phase 4 "Gotchas" — FE remains).
    Map {
        source: Box<Walker>,
        fe: FunctionEvaluator,
    },
    /// Legacy FE-driven `filter`. No production node constructs this variant
    /// after Phase 5 — `filter` flips to `FilterZone` — but the path is kept
    /// alive as dead weight so unit tests targeting the FE walker plumbing
    /// keep working until `FunctionEvaluator` and `Closure` are retired
    /// outright (deferred). See `doc/design_zones.md` (Phase 5 "Gotchas" —
    /// FE remains).
    Filter {
        source: Box<Walker>,
        fe: FunctionEvaluator,
    },
    /// `filter` driven by an inline zone body. Per `next()` the walker stands
    /// up a synthetic body-only network stack, pushes a fresh frame onto
    /// `current_zone_input_values[hof_node_id]`, swaps captures into the
    /// caller's context, evaluates the wire delivering the body's `keep`
    /// zone-output pin (resolved against the synthetic body-only stack), and
    /// pops everything. If the body yields `Bool(true)` the element is
    /// emitted; `Bool(false)` skips and continues the loop; non-Bool /
    /// Error halts the walker with an Error.
    ///
    /// Mirrors `MapZone`'s shape — see `WalkerKind::MapZone` for the
    /// design-doc deviation that motivates carrying `zone_output_wires`
    /// rather than reaching back through the outer network stack.
    FilterZone {
        source: Box<Walker>,
        body: Arc<NodeNetwork>,
        captures: Arc<HashMap<CaptureKey, NetworkResult>>,
        zone_output_wires: Arc<Vec<IncomingWire>>,
        hof_node_id: u64,
    },
    /// Cartesian product over `axes` (rightmost varies fastest). Each emitted
    /// element is a record whose field order is given by `field_names` and
    /// whose values come from the most recent pull on each axis.
    Product {
        axes: Vec<Walker>,
        field_names: Vec<String>,
        current: Vec<NetworkResult>,
        primed: bool,
        done: bool,
    },
}

impl Walker {
    /// Wrap a materialized array. Backing storage is `Arc`-shared so clones are O(1).
    ///
    /// The `Arc` is currently held over `Vec<NetworkResult>`, which is not
    /// `Send`/`Sync` (closures contain `Box<dyn NodeData>`). Clippy flags
    /// this; we keep `Arc` (over `Rc`) deliberately for forward-compat with
    /// multi-threaded evaluation, per `doc/design_iterators.md`.
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn from_array(items: Vec<NetworkResult>) -> Self {
        Self {
            kind: WalkerKind::FromArray {
                items: Arc::new(items),
                idx: 0,
            },
            fused: false,
        }
    }

    /// Numeric range walker. Emits exactly `count` values; if `count <= 0` the
    /// walker is immediately exhausted.
    pub fn range(start: i32, step: i32, count: i32) -> Self {
        Self {
            kind: WalkerKind::Range {
                start,
                step,
                count,
                emitted: 0,
            },
            fused: false,
        }
    }

    /// Construct a `map` walker driven by an inline zone body. See
    /// `WalkerKind::MapZone` for the per-`next()` discipline.
    pub fn map_zone(
        source: Walker,
        body: Arc<NodeNetwork>,
        captures: Arc<HashMap<CaptureKey, NetworkResult>>,
        zone_output_wires: Arc<Vec<IncomingWire>>,
        hof_node_id: u64,
    ) -> Self {
        Self {
            kind: WalkerKind::MapZone {
                source: Box::new(source),
                body,
                captures,
                zone_output_wires,
                hof_node_id,
            },
            fused: false,
        }
    }

    /// Legacy FE-driven `map` constructor. No production code builds this
    /// after Phase 4; kept alive so unit tests targeting the FE walker
    /// plumbing continue to compile until Phase 5 retires FE entirely.
    pub fn map(source: Walker, fe: FunctionEvaluator) -> Self {
        Self {
            kind: WalkerKind::Map {
                source: Box::new(source),
                fe,
            },
            fused: false,
        }
    }

    pub fn filter(source: Walker, fe: FunctionEvaluator) -> Self {
        Self {
            kind: WalkerKind::Filter {
                source: Box::new(source),
                fe,
            },
            fused: false,
        }
    }

    /// Construct a `filter` walker driven by an inline zone body. See
    /// `WalkerKind::FilterZone` for the per-`next()` discipline.
    pub fn filter_zone(
        source: Walker,
        body: Arc<NodeNetwork>,
        captures: Arc<HashMap<CaptureKey, NetworkResult>>,
        zone_output_wires: Arc<Vec<IncomingWire>>,
        hof_node_id: u64,
    ) -> Self {
        Self {
            kind: WalkerKind::FilterZone {
                source: Box::new(source),
                body,
                captures,
                zone_output_wires,
                hof_node_id,
            },
            fused: false,
        }
    }

    pub fn product(axes: Vec<Walker>, field_names: Vec<String>) -> Self {
        debug_assert_eq!(
            axes.len(),
            field_names.len(),
            "product: axes and field_names must align"
        );
        Self {
            kind: WalkerKind::Product {
                axes,
                field_names,
                current: Vec::new(),
                primed: false,
                done: false,
            },
            fused: false,
        }
    }

    /// Returns the next element, `None` at end-of-stream.
    ///
    /// Contract:
    /// - `None` — stream end (sticky).
    /// - `Some(NetworkResult::Error(_))` — error mid-stream (fires at most
    ///   once for the lifetime of this walker; subsequent calls return `None`).
    /// - `Some(other)` — next element.
    ///
    /// `context` is the outer pass's evaluation context. Walker variants that
    /// embed a `FunctionEvaluator` (`Filter`) forward the same `&mut`
    /// context to `FunctionEvaluator::evaluate`, which is what propagates
    /// `context.execute` into per-element body evaluations and drains the
    /// inner body's `print_buffer` back into the outer context. `MapZone` runs
    /// directly against the caller's context under a `CapturesGuard` swap
    /// plus push/pop discipline on `current_zone_input_values[hof_node_id]` —
    /// see `doc/design_zones.md` (§"Sub-context pattern for body evaluation").
    pub fn next(
        &mut self,
        evaluator: &NetworkEvaluator,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
    ) -> Option<NetworkResult> {
        if self.fused {
            return None;
        }
        let result = self.kind.next_inner(evaluator, registry, context);
        if let Some(NetworkResult::Error(_)) = &result {
            self.fused = true;
        }
        result
    }

    /// Rewind the walker to its initial state and clear the error fuse.
    /// Cascades through child walkers.
    pub fn reset(&mut self) {
        self.fused = false;
        self.kind.reset();
    }

    /// Returns true when the outer error-fuse has tripped (a previous `next`
    /// yielded `Some(Error(_))`). Test-only convenience.
    pub fn is_fused(&self) -> bool {
        self.fused
    }

    /// For `FromArray` walkers, returns the `Arc::strong_count` of the
    /// shared `items` storage. Returns `None` for any other variant. Used by
    /// tests to assert that cloning a `FromArray`-rooted walker is O(1) (the
    /// `Arc` is shared, not deep-copied).
    pub fn from_array_items_strong_count(&self) -> Option<usize> {
        if let WalkerKind::FromArray { items, .. } = &self.kind {
            Some(Arc::strong_count(items))
        } else {
            None
        }
    }
}

impl WalkerKind {
    fn next_inner(
        &mut self,
        evaluator: &NetworkEvaluator,
        registry: &NodeTypeRegistry,
        context: &mut NetworkEvaluationContext,
    ) -> Option<NetworkResult> {
        match self {
            WalkerKind::FromArray { items, idx } => {
                if *idx >= items.len() {
                    return None;
                }
                let v = items[*idx].clone();
                *idx += 1;
                Some(v)
            }
            WalkerKind::Range {
                start,
                step,
                count,
                emitted,
            } => {
                if *emitted >= *count {
                    return None;
                }
                // Use i64 for the multiplication to avoid overflow on large
                // counts/steps; the design constrains values to `i32` so the
                // narrowing back is safe for in-range emissions.
                let value = (*start as i64) + (*step as i64) * (*emitted as i64);
                *emitted += 1;
                Some(NetworkResult::Int(value as i32))
            }
            WalkerKind::MapZone {
                source,
                body,
                captures,
                zone_output_wires,
                hof_node_id,
            } => match source.next(evaluator, registry, context) {
                None => None,
                Some(NetworkResult::Error(e)) => Some(NetworkResult::Error(e)),
                Some(elem) => {
                    // Per-element step under push/pop + captures swap.
                    let hof_id = *hof_node_id;
                    let body_arc = Arc::clone(body);
                    let wires = Arc::clone(zone_output_wires);

                    // Swap captures in for the duration of the step.
                    let saved_captures =
                        std::mem::replace(&mut context.captured_source_values, Arc::clone(captures));

                    context.push_zone_input_frame(hof_id, vec![elem]);

                    let result = eval_step(evaluator, registry, context, &body_arc, &wires, hof_id);

                    context.pop_zone_input_frame(hof_id);
                    context.captured_source_values = saved_captures;

                    Some(result)
                }
            },
            WalkerKind::Map { source, fe } => match source.next(evaluator, registry, context) {
                None => None,
                Some(NetworkResult::Error(e)) => Some(NetworkResult::Error(e)),
                Some(elem) => {
                    fe.set_argument_value(0, elem);
                    Some(fe.evaluate(evaluator, registry, context))
                }
            },
            WalkerKind::Filter { source, fe } => loop {
                match source.next(evaluator, registry, context) {
                    None => return None,
                    Some(NetworkResult::Error(e)) => return Some(NetworkResult::Error(e)),
                    Some(elem) => {
                        fe.set_argument_value(0, elem.clone());
                        let predicate = fe.evaluate(evaluator, registry, context);
                        match predicate {
                            NetworkResult::Bool(true) => return Some(elem),
                            NetworkResult::Bool(false) => continue,
                            NetworkResult::Error(e) => return Some(NetworkResult::Error(e)),
                            _ => {
                                return Some(NetworkResult::Error(
                                    "filter: f returned non-Bool".to_string(),
                                ));
                            }
                        }
                    }
                }
            },
            WalkerKind::FilterZone {
                source,
                body,
                captures,
                zone_output_wires,
                hof_node_id,
            } => loop {
                match source.next(evaluator, registry, context) {
                    None => return None,
                    Some(NetworkResult::Error(e)) => return Some(NetworkResult::Error(e)),
                    Some(elem) => {
                        // Per-element step under push/pop + captures swap.
                        // Mirrors `MapZone`'s discipline.
                        let hof_id = *hof_node_id;
                        let body_arc = Arc::clone(body);
                        let wires = Arc::clone(zone_output_wires);

                        let saved_captures = std::mem::replace(
                            &mut context.captured_source_values,
                            Arc::clone(captures),
                        );
                        context.push_zone_input_frame(hof_id, vec![elem.clone()]);

                        let predicate =
                            eval_step(evaluator, registry, context, &body_arc, &wires, hof_id);

                        context.pop_zone_input_frame(hof_id);
                        context.captured_source_values = saved_captures;

                        match predicate {
                            NetworkResult::Bool(true) => return Some(elem),
                            NetworkResult::Bool(false) => continue,
                            NetworkResult::Error(e) => return Some(NetworkResult::Error(e)),
                            other => {
                                return Some(NetworkResult::Error(format!(
                                    "filter: body returned non-Bool (got {})",
                                    other.to_display_string()
                                )));
                            }
                        }
                    }
                }
            },
            WalkerKind::Product {
                axes,
                field_names,
                current,
                primed,
                done,
            } => {
                if *done {
                    return None;
                }
                if !*primed {
                    current.clear();
                    for axis in axes.iter_mut() {
                        match axis.next(evaluator, registry, context) {
                            None => {
                                *done = true;
                                return None;
                            }
                            Some(NetworkResult::Error(e)) => {
                                *done = true;
                                return Some(NetworkResult::Error(e));
                            }
                            Some(v) => current.push(v),
                        }
                    }
                    *primed = true;
                    return Some(build_product_record(field_names, current));
                }

                // Mixed-radix advance: bump the rightmost axis first (it
                // varies fastest, matching today's `product.rs`). When an
                // axis exhausts, reset it and carry into the next-leftward
                // axis. Done when the leftmost carry overflows.
                let n = axes.len();
                let mut i = n;
                loop {
                    if i == 0 {
                        *done = true;
                        return None;
                    }
                    i -= 1;
                    match axes[i].next(evaluator, registry, context) {
                        Some(NetworkResult::Error(e)) => {
                            *done = true;
                            return Some(NetworkResult::Error(e));
                        }
                        Some(v) => {
                            current[i] = v;
                            return Some(build_product_record(field_names, current));
                        }
                        None => {
                            axes[i].reset();
                            match axes[i].next(evaluator, registry, context) {
                                None => {
                                    // An axis that produced at least one value
                                    // on first prime cannot now be empty after
                                    // reset unless its underlying source is
                                    // non-deterministic (which we don't
                                    // support). Treat as exhaustion to be
                                    // safe.
                                    *done = true;
                                    return None;
                                }
                                Some(NetworkResult::Error(e)) => {
                                    *done = true;
                                    return Some(NetworkResult::Error(e));
                                }
                                Some(v) => {
                                    current[i] = v;
                                    // Carry: continue the loop to advance the
                                    // next-leftward axis.
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn reset(&mut self) {
        match self {
            WalkerKind::FromArray { idx, .. } => *idx = 0,
            WalkerKind::Range { emitted, .. } => *emitted = 0,
            WalkerKind::MapZone { source, .. } => source.reset(),
            WalkerKind::Map { source, .. } => source.reset(),
            WalkerKind::Filter { source, .. } => source.reset(),
            WalkerKind::FilterZone { source, .. } => source.reset(),
            WalkerKind::Product {
                axes,
                current,
                primed,
                done,
                ..
            } => {
                for axis in axes.iter_mut() {
                    axis.reset();
                }
                current.clear();
                *primed = false;
                *done = false;
            }
        }
    }
}

/// One zone-body step: stand up a body-only network stack, then resolve the
/// first zone-output wire against it.
///
/// The walker doesn't carry the outer network stack — only the body. That's
/// sufficient because:
/// * Body-local wires (`source_scope_depth == 0`) resolve inside this body.
/// * Outer-scope wires (`source_scope_depth > 0`) were pre-evaluated at body
///   entry by `MapData::eval` and live in `context.captured_source_values`;
///   the `resolve_incoming_wire` capture-cache check fires before any stack
///   walk, so they never reach the outer frames the walker doesn't have.
/// * `ZoneInput` wires referencing the immediately enclosing HOF read from
///   `current_zone_input_values` via the live-lookup path.
///
/// We can't call `evaluator.evaluate_zone_output` directly because it
/// expects the HOF node's containing network at `stack[len-2]` (so it can
/// look up the HOF's `zone_output_arguments`); the walker stores those
/// wires itself instead, and we resolve them here via the public `evaluate`
/// path.
fn eval_step(
    evaluator: &NetworkEvaluator,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    body: &Arc<NodeNetwork>,
    zone_output_wires: &[IncomingWire],
    hof_node_id: u64,
) -> NetworkResult {
    let incoming = match zone_output_wires.first() {
        Some(w) => w,
        None => {
            return NetworkResult::Error(
                "HOF body has no incoming wire on zone-output pin".to_string(),
            );
        }
    };

    let body_stack = vec![NetworkStackElement {
        node_network: body.as_ref(),
        node_id: hof_node_id,
    }];

    // Capture-cache short-circuit: zone-output wires are always body-local
    // (`source_scope_depth == 0`), so this should never fire for them, but
    // we mirror `resolve_incoming_wire`'s order-of-checks for safety.
    let key = CaptureKey::from_incoming(incoming);
    if let Some(cached) = context.captured_source_values.get(&key) {
        return cached.clone();
    }

    match incoming.source_pin {
        SourcePin::NodeOutput { pin_index } => evaluator.evaluate(
            &body_stack,
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

fn build_product_record(field_names: &[String], current: &[NetworkResult]) -> NetworkResult {
    let fields: Vec<(String, NetworkResult)> = field_names
        .iter()
        .zip(current.iter())
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    NetworkResult::record(fields)
}
