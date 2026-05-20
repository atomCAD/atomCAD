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

use std::sync::Arc;

use crate::structure_designer::evaluator::function_evaluator::FunctionEvaluator;
use crate::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::zone_closure::{ZoneClosure, run_closure_once};
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
    /// `map` driven by a zone closure. The closure (body + frozen captures +
    /// zone-output wires + owner id) is `Arc`-backed throughout so walker
    /// clones (Invariant 2) pay only refcount bumps. Per `next()` the walker
    /// runs the closure once on the pulled element via [`run_closure_once`],
    /// which stands up a body-only stack, pushes a fresh frame onto
    /// `current_zone_input_values[owner_node_id]`, swaps the frozen captures
    /// into the caller's context, evaluates the body's `result` zone-output
    /// wire, then pops the frame and restores the caller's captures.
    ///
    /// The closure carries `zone_output_wires` rather than reaching back
    /// through the outer network stack (which `Walker::next` doesn't have
    /// access to) — the design relies on captures being pre-evaluated at body
    /// entry so the body-only stack is sufficient at runtime. See
    /// `doc/design_zones.md` (Phase 4, §"Sub-context pattern") and
    /// `doc/design_closures.md`.
    MapZone {
        source: Box<Walker>,
        closure: ZoneClosure,
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
    /// `filter` driven by a zone closure. Per `next()` the walker runs the
    /// closure once on the pulled element via [`run_closure_once`] (same
    /// discipline as `MapZone`). If the body yields `Bool(true)` the element is
    /// emitted; `Bool(false)` skips and continues the loop; non-Bool / Error
    /// halts the walker with an Error.
    ///
    /// Mirrors `MapZone`'s shape — see `WalkerKind::MapZone` for the closure
    /// bundle and the body-only-stack rationale.
    FilterZone {
        source: Box<Walker>,
        closure: ZoneClosure,
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

    /// Construct a `map` walker driven by a zone closure. See
    /// `WalkerKind::MapZone` for the per-`next()` discipline.
    pub fn map_zone(source: Walker, closure: ZoneClosure) -> Self {
        Self {
            kind: WalkerKind::MapZone {
                source: Box::new(source),
                closure,
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

    /// Construct a `filter` walker driven by a zone closure. See
    /// `WalkerKind::FilterZone` for the per-`next()` discipline.
    pub fn filter_zone(source: Walker, closure: ZoneClosure) -> Self {
        Self {
            kind: WalkerKind::FilterZone {
                source: Box::new(source),
                closure,
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
            WalkerKind::MapZone { source, closure } => {
                match source.next(evaluator, registry, context) {
                    None => None,
                    Some(NetworkResult::Error(e)) => Some(NetworkResult::Error(e)),
                    Some(elem) => Some(run_closure_once(
                        evaluator,
                        &[],
                        registry,
                        context,
                        closure,
                        vec![elem],
                    )),
                }
            }
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
            WalkerKind::FilterZone { source, closure } => loop {
                match source.next(evaluator, registry, context) {
                    None => return None,
                    Some(NetworkResult::Error(e)) => return Some(NetworkResult::Error(e)),
                    Some(elem) => {
                        // Per-element step. Mirrors `MapZone`'s discipline.
                        let predicate = run_closure_once(
                            evaluator,
                            &[],
                            registry,
                            context,
                            closure,
                            vec![elem.clone()],
                        );

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

fn build_product_record(field_names: &[String], current: &[NetworkResult]) -> NetworkResult {
    let fields: Vec<(String, NetworkResult)> = field_names
        .iter()
        .zip(current.iter())
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    NetworkResult::record(fields)
}
