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

use crate::structure_designer::data_type::DataType;
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
    /// `zip_with` driven by a zone closure: N source streams combined
    /// element-wise. Per `next()` one element is pulled from every source **in
    /// lane order**; if any source is exhausted the zip ends (elements already
    /// pulled this step from earlier lanes are discarded — the shortest input
    /// terminates the stream, Haskell `zipWith` convention). The pulled frame
    /// then runs through the closure exactly like `MapZone`'s single element,
    /// including the currying auto-partialization — see
    /// [`run_closure_on_frame`], the helper shared with `MapZone` so the two
    /// cannot drift. Clone independence (Invariant 2) holds: per-source state
    /// is owned via the `Vec<Walker>` and `ZoneClosure` is fully `Arc`-backed.
    ZipZone {
        sources: Vec<Walker>,
        closure: ZoneClosure,
    },
    /// Lazy `Iter[S] → Iter[T]` element conversion (`S → T` allowed, `S ≠ T`).
    /// Per `next()` the walker pulls one element from `source` and runs
    /// [`NetworkResult::convert_to`] on it from `source_elem_type` to
    /// `target_elem_type`. The wire layer wraps an iterator source in this
    /// variant whenever its declared element type differs from the
    /// destination's (see `network_result::convert_to` and
    /// `doc/design_iterators.md`, open question #2). No closure or frozen
    /// captures are involved — element conversion is a pure, stateless
    /// transform — so clone independence (Invariant 2) holds with just the
    /// owned `source` walker.
    Convert {
        source: Box<Walker>,
        source_elem_type: DataType,
        target_elem_type: DataType,
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

    /// Construct a `zip_with` walker driven by a zone closure over N source
    /// streams. See `WalkerKind::ZipZone` for the per-`next()` discipline.
    pub fn zip_zone(sources: Vec<Walker>, closure: ZoneClosure) -> Self {
        Self {
            kind: WalkerKind::ZipZone { sources, closure },
            fused: false,
        }
    }

    /// Construct a lazy element-converting walker. See `WalkerKind::Convert`.
    /// Used by `network_result::convert_to` to wrap an iterator source whose
    /// element type differs from the destination's (`Iter[S] → Iter[T]`).
    pub fn convert(source: Walker, source_elem_type: DataType, target_elem_type: DataType) -> Self {
        Self {
            kind: WalkerKind::Convert {
                source: Box::new(source),
                source_elem_type,
                target_elem_type,
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
    /// `context` is the outer pass's evaluation context. The zone-closure
    /// variants (`MapZone` / `FilterZone`) run their closure against the
    /// caller's context via [`run_closure_once`], which swaps the frozen
    /// captures in and brackets a push/pop on
    /// `current_zone_input_values[owner_node_id]` — this is what propagates
    /// `context.execute` into per-element body evaluations and drains the inner
    /// body's `print_buffer` back into the outer context. See
    /// `doc/design_zones.md` (§"Sub-context pattern for body evaluation") and
    /// `doc/design_closures.md`.
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
                    // The frame step (run vs. auto-partialization) is shared
                    // with `ZipZone` — see `run_closure_on_frame`. (For map,
                    // body arity 0 is a thunk; the element is silently
                    // discarded — pathological but well-defined.)
                    Some(elem) => Some(run_closure_on_frame(
                        evaluator,
                        registry,
                        context,
                        closure,
                        vec![elem],
                    )),
                }
            }
            WalkerKind::ZipZone { sources, closure } => {
                // Pull one element from each source in lane order. Any
                // exhausted source ends the zip — elements already pulled this
                // step from earlier lanes are discarded (documented; sources
                // are always pulled in lane order, so `print`-node side
                // effects inside upstream walkers fire deterministically). A
                // mid-stream source error is yielded; the outer fuse then
                // terminates the stream.
                let mut frame = Vec::with_capacity(sources.len());
                for source in sources.iter_mut() {
                    match source.next(evaluator, registry, context) {
                        None => return None,
                        Some(NetworkResult::Error(e)) => return Some(NetworkResult::Error(e)),
                        Some(elem) => frame.push(elem),
                    }
                }
                // A closure with fewer parameters than lanes is unreachable
                // through type-checking (`AnyFunction { leading_params }`
                // requires arity ≥ N and inline bodies have exactly N params)
                // but reachable via a hand-authored file — yield an error
                // rather than panicking or silently truncating the frame.
                if closure.param_types.len() < frame.len() {
                    return Some(NetworkResult::Error(format!(
                        "zip_with: function takes {} parameter(s) but {} lanes are zipped",
                        closure.param_types.len(),
                        frame.len()
                    )));
                }
                Some(run_closure_on_frame(
                    evaluator, registry, context, closure, frame,
                ))
            }
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
            WalkerKind::Convert {
                source,
                source_elem_type,
                target_elem_type,
            } => match source.next(evaluator, registry, context) {
                None => None,
                Some(NetworkResult::Error(e)) => Some(NetworkResult::Error(e)),
                Some(elem) => Some(elem.convert_to(source_elem_type, target_elem_type, registry)),
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
            WalkerKind::ZipZone { sources, .. } => {
                for source in sources.iter_mut() {
                    source.reset();
                }
            }
            WalkerKind::FilterZone { source, .. } => source.reset(),
            WalkerKind::Convert { source, .. } => source.reset(),
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

/// Run `closure` on one pulled argument frame — the per-element step shared by
/// the zone-driven mapping walkers (`MapZone`, `ZipZone`) so the two cannot
/// drift.
///
/// Implements the currying auto-partialization branch
/// (`doc/design_currying.md`, §"HOF auto-partialization (`map`)"): when the
/// closure's remaining arity exceeds the frame size, the frame fills the
/// leading slots — bound into `pre_supplied_args` — and the result is a
/// partially-applied `Function` value; downstream consumers (`apply`, another
/// `map`, …) absorb the rest. Otherwise the body runs once via
/// [`run_closure_once`] with a body-only stack (deep captures were pre-frozen
/// at the producing HOF's `eval`).
///
/// A closure whose arity is *smaller* than the frame is the callers' concern:
/// `MapZone` deliberately lets a 0-arity thunk discard its element, while
/// `ZipZone` rejects the undersized closure before calling here.
fn run_closure_on_frame(
    evaluator: &NetworkEvaluator,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    closure: &ZoneClosure,
    frame: Vec<NetworkResult>,
) -> NetworkResult {
    if closure.param_types.len() > frame.len() {
        let consumed = frame.len();
        #[allow(clippy::arc_with_non_send_sync)]
        let extended = {
            let mut v = (*closure.pre_supplied_args).clone();
            v.extend(frame);
            Arc::new(v)
        };
        NetworkResult::Function(ZoneClosure {
            body: Arc::clone(&closure.body),
            captures: Arc::clone(&closure.captures),
            zone_output_wires: Arc::clone(&closure.zone_output_wires),
            owner_node_id: closure.owner_node_id,
            param_types: closure.param_types[consumed..].to_vec(),
            return_type: closure.return_type.clone(),
            pre_supplied_args: extended,
        })
    } else {
        run_closure_once(evaluator, &[], registry, context, closure, frame)
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
