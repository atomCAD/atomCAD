# Iterators in atomCAD

## Scope

`product → map → filter → fold` is the killer pipeline for combinatorial design: enumerate ~10⁵–10⁷ molecule variations, generate each, score it, keep statistics. The intermediate `Array[Molecule]` *must never materialize* — that is the entire point of this work.

This document designs:

1. A new pin/runtime type `Iter[T]` that represents a lazily-evaluated stream of `T` values.
2. The runtime representation as a unified **Walker tree** built during evaluation.
3. Conversions between `Array[T]` and `Iter[T]` at wire time.
4. Updates to four existing nodes — `map`, `filter`, `fold`, `product` — and one existing node — `range` — to operate on iterators.
5. One new node — `collect` — to materialize an iterator into an array.
6. A backward-compatibility migration for `.cnnd` files.

Out of scope:

- Iterator support inside the `expr` expression language. Expressions are eager and have no lambda support; nothing here changes that.
- New higher-order operators (`flat_map`, `take`, `skip`, `zip`). All are natural follow-ups but not in this drop.
- `iter_at` / `iter_len`. Both force consumption; users who want them can `collect` first and pay the cost explicitly.
- Caching/memoization of iterators. Each consumer drives its own walker; replay is "re-evaluate from the source", not "recall buffered values".

## Motivation

The four nodes today (`map`, `filter`, `fold`, `product`) all produce or consume `Array[T]` values represented as `NetworkResult::Array(Vec<NetworkResult>)`. For a 10⁶-element cartesian product, that vector — populated with full `Molecule` payloads after `map` — is gigabytes. Today's `map.rs` literally builds the result vector element-by-element (`map.rs:91-107`); the consumer downstream then walks the full vector again. There is no way for the producer and the consumer to fuse.

We want stream fusion: `range → map → filter → fold` should pull one element at a time from `range` through the chain, with the steady-state memory cost being one element plus the walker's small state machine.

## Design decisions

| Question | Decision |
|---|---|
| Type spelling | `Iter[T]` (`DataType::Iterator(Box<DataType>)`). Display string: `Iter[T]`. |
| Wire-time `[S] → Iter[T]` (S → T allowed) | Implicit. Eagerly converts each item, wraps as `WalkerKind::FromArray`. The array is materialized anyway, so conversion is a one-time O(N) cost — same as today's `[S] → [T]` rule. |
| Wire-time `Iter[T] → [T]` | **Disallowed.** Force users to write a `collect` node. The whole point is that iterator → array is the expensive operation. |
| Wire-time `S → Iter[T]` (S → T allowed) | Implicit (1-element broadcast, mirrors today's `T → [T]` rule). Eagerly converts the single value. |
| Wire-time `Iter[S] → Iter[T]` (S ≠ T) | **Disallowed in v1.** Lazy element conversion across iterator boundaries is deferred (see open questions). Users insert a `map` with the conversion, or `collect` + manual rebuild. The identity case (`Iter[T] → Iter[T]`) is of course allowed. |
| Recipe vs Walker | One unified `Walker` tree. No separate immutable recipe type. |
| Walker shape | `struct Walker { kind: WalkerKind }` with `Box<Walker>` for single children, `Vec<Walker>` for variadic — same template `geo_tree::GeoNode` uses. |
| Walker mutability | `next(&mut self, ...)` advances. `reset(&mut self)` rewinds. No public `init()` — `Product`'s priming is internal state. |
| Walker `Clone` | Derived. **Runtime requirement, not structural insurance** — `EvalOutput::get`, `evaluate_required`, and `parameter::eval` all clone the enclosing `NetworkResult` on every read. Each `Walker::clone()` must produce a walker whose `next()` advances independently. See §"Evaluation model and walker lifecycle". |
| `FunctionEvaluator` lifecycle | Constructed inside `map.eval()` / `filter.eval()` (once per call) and stored in the `WalkerKind` variant. Per-element work is `set_argument_value` + `evaluate`, never FE construction. Because the evaluator does not cache pin results, FE construction cost is **per-consumer-per-evaluation**, not amortized across consumers. |
| Per-consumer state isolation | Each consumer's `evaluate_arg` triggers a fresh upstream `eval()` → fresh walker. Verified at `network_evaluator.rs:1006-1120` and `:901-1003`. See §"Evaluation model and walker lifecycle" for the contract. |
| Iterator as top-level parameter | **Disallowed in v1.** The CLI/API binding rejects `Iter[T]` arguments (parallel to the closure-capture restriction). Semantics would be well-defined under Invariant 2, but keeping iterator lifetime entirely *inside* a single network evaluation simplifies the v1 surface. |
| Iterator displayed as a node output | Auto-collect with a fixed cap (256 elements). Subtitle hints "showing 256 of N". |
| Errors mid-stream | A walker yielding an `Error` propagates: `Some(NetworkResult::Error(...))` to the consumer, which aborts immediately. The outer `Walker`'s `fused` flag enforces "Error once, then `None`" uniformly across variants. Same semantics as today's eager nodes. |
| Iterator captures into closures | **Disallowed in v1.** A function pin's captured value-pin types must not contain `Iter[T]`. Captured walkers would be aliased across invocations and corrupt under repeated use; force users to `collect` upstream of the value-pin. |
| Backward compatibility | One-shot `.cnnd` migration: insert `collect` on every old `map`/`filter`/`product` output that connects to a non-iterator pin. |

## Evaluation model and walker lifecycle

The iterator design relies on three invariants of the existing evaluator. This section pins them down and lists the implications for `Walker`.

### Invariant 1: no result memoization

`NetworkEvaluator::evaluate` (`network_evaluator.rs:1006-1120`) calls `node.data.eval()` directly, with no result cache. Same for `evaluate_all_outputs` (`:901-1003`). `NetworkEvaluationContext` carries `node_errors`, `node_output_strings`, `selected_node_eval_cache`, and `top_level_parameters`, but none of these are pin-result caches:

- `node_errors` / `node_output_strings`: per-pin diagnostic strings, written as a side effect of evaluation.
- `selected_node_eval_cache`: a single-slot scratchpad for the *currently selected* node's gadget data — not keyed on pin index, not a result cache.
- `top_level_parameters`: input bindings supplied by the API/CLI caller; see Invariant 3.

**Consequence:** for an output pin with fan-out N, the producing node's `eval()` runs N times during one downstream evaluation pass — and each run constructs a fresh `Walker`. Two consumers of the same `Iter[T]` pin therefore drain *different* walker instances; one cannot starve the other.

### Invariant 2: `Walker::clone()` produces an independent walker

Several paths in the evaluator clone `NetworkResult` values:

| Site | Why |
|---|---|
| `EvalOutput::get` (`node_data.rs:50`, uses `.cloned()`) | Called from `generate_scene` at least twice per pin (`network_evaluator.rs:211, 220` for pin 0; once per non-zero pin in the multi-output loop). |
| `EvalOutput::get_display` (`node_data.rs:68`) | Display-override readout. |
| `evaluate_required` (`network_evaluator.rs:751`, `extractor(result.clone())`) | Pattern used by `fold` / `collect` to inspect a result before deciding whether to consume it. |
| `parameter::eval` (`parameter.rs:63`, `cli_value.clone()`) | Top-level parameter readout (Invariant 3). |

For an `Iter[T]`-typed value, every one of these clones the enclosed `Walker`. **`Walker::clone()` is therefore a hard correctness requirement, not "structural insurance":** each clone must produce a walker whose `next()` advances independently of the original.

The design's enum (everything owned, `Box<Walker>` for children, `Arc<Vec<NetworkResult>>` on `FromArray::items` only) satisfies this naturally. Per-variant cost:

| Variant | Clone cost |
|---|---|
| `FromArray` | O(1) — `Arc`-shared `items`, per-clone `idx` |
| `Range` | O(1) — five integers |
| `Map` / `Filter` | recursive clone of `source` + `FunctionEvaluator::clone` (clones the inner ad-hoc `NodeNetwork`) |
| `Product` | recursive clone of all axes + clone of `current: Vec<NetworkResult>` |

The `Map`/`Filter` clone cost is non-trivial (FE clones a small `NodeNetwork`), but happens at most O(consumer_count + display_clones) times per evaluation pass — never per element. The per-element hot path is unaffected.

### Invariant 3: top-level parameters

`context.top_level_parameters: HashMap<String, NetworkResult>` is populated by the caller (CLI runner or API layer). `parameter::eval` reads it via `cli_value.clone()` on each invocation of the parameter node. With multiple uses of the same parameter inside the network body, each use clones the value independently — which would, under Invariant 2, be safe for `Iter[T]`.

**For v1, however, iterator-typed top-level parameters are disallowed.** The CLI/API binding code rejects `Iter[T]` arguments with a clear error pointing the user at `collect` (and a chain to wrap an array literal). Rationale: the v1 surface for top-level parameters is small (mostly scalars and arrays), and forbidding iterators avoids the edge case where the same caller-supplied walker is cloned dozens of times across a fan-out body. The restriction can be relaxed later by removing the rejection — Invariant 2 already gives correct semantics.

This parallels the closure-capture restriction (§"Iterator values cannot be captured into closures"): both rules keep walker lifetime entirely *inside* a single network evaluation.

### Worked example: fan-out

Pipeline:

```
range(0, 1, 1_000_000) → map(double) ⟶ fold(sum)             (consumer A)
                                    ↘
                                      collect → array_len    (consumer B)
```

When both `fold` and `array_len` are evaluated:

1. **A's chain:** `fold.eval()` → `evaluate_arg(0)` → `evaluate(map)` → `map.eval()` → `evaluate_arg(0)` → `evaluate(range)` → `range.eval()` constructs **walker R₁**. `map.eval()` builds FE #1 and wraps R₁ as `Walker::map`. `fold` drains it.
2. **B's chain:** independently calls `evaluate(map)` → `map.eval()` → `range.eval()` constructs **walker R₂**. Fresh FE #2. `collect` drains R₂ into an `Array[T]` (this materializes — that's the `collect` contract).

Two independent `range → map` chains run, two independent FE constructions, no shared mutable walker state. Per-element memory footprint of A's chain is O(1) regardless of B's existence.

## Type system: `Iter[T]`

### Adding the variant

```rust
// data_type.rs
pub enum DataType {
    // ... existing variants ...
    Iterator(Box<DataType>),
}

impl fmt::Display for DataType {
    // ...
    DataType::Iterator(elem) => write!(f, "Iter[{}]", elem),
    // ...
}
```

`from_string` parses `Iter[T]` analogously to the existing `[T]` array syntax. The bare identifier `Iter` is reserved as a type-name keyword (only legal in type-expression positions) so the existing record-name lookup rule isn't disturbed.

### Conversions in `can_be_converted_to`

Add three rules to `DataType::can_be_converted_to`, in this order:

1. **`[S] → Iter[T]`** (wrap with eager element conversion): if dst is `Iterator(d)` and src is `Array(s)`, recurse on `s → d`. Conversion runs eagerly at wrap time over the materialized array — equivalent cost to today's `[S] → [T]` rule.
2. **`S → Iter[T]`** (single-element broadcast, mirrors the existing `T → [T]`): if dst is `Iterator(d)`, recurse on `src → d`. The single value is converted eagerly.
3. **`Iter[T] → Iter[T]`** (identity only): if both sides are `Iterator(_)`, the inner types must be **equal**. Lazy element conversion across iterator boundaries (`Iter[S] → Iter[T]` with `S ≠ T`) is deferred to a follow-up; for v1, users insert a `map` with the conversion or `collect` + manual rebuild.

There is **no** `Iter[T] → [T]` rule. There is **no** `Iter[T] → T` rule. There is no record/array auto-promotion through iterator boundaries beyond what already exists.

The eager-wrap rules (1, 2) are sufficient for the common cases: array literals into `fold`/`map`/`filter`, `[Int] → fold(Float, …)` numeric widening, and similar — all keep working without an explicit conversion node, just as they do today. The restriction only bites when an already-lazy `Iter[S]` chain needs to feed a consumer expecting a different element type.

### `is_tag_only_widening` / `can_be_structurally_converted_to`

`Iter[T]` is not allowed as a record field type or in any other tag-only-widening context. Records exist to bundle small heterogeneous values; an iterator inside a record would create extremely confusing semantics ("when does the iterator advance? when the record is destructured?"). If we ever need this, it's a separate design.

### Iterator values cannot be captured into closures

A function pin captures upstream value pins into the closure that flows downstream. If a `Walker` were captured this way and the closure were invoked more than once (e.g. as the `f` of a `map` over a 10⁶-element source), every invocation would share — and advance — the *same* underlying walker. The first invocation would drain it; subsequent invocations would see an exhausted iterator. This is silently wrong.

The v1 rule: **a function pin's captured value-pin types must not contain `Iter[T]` (anywhere — neither as the captured type itself nor inside a record/array)**. The closure-construction code in the validator rejects wires whose captured argument contains an iterator. The error message names the offending value-pin and points the user at `collect`.

If a user genuinely needs the elements of an iterator inside a closure body, they wire `collect` upstream of the value-pin and capture the resulting array. This matches the doc's overall philosophy: iterator → array is an explicit, expensive step.

(This restriction can be relaxed later — e.g. by deep-cloning the captured walker on each FE invocation, or by introducing `Iter` semantics in the closure body — but neither is needed for v1.)

### Subtitle / hover

Iterator-typed pins render in the same neutral color as today's array pins (consistent: both are "many of T"). Hover tooltip shows `Iter[T]` with the resolved element type.

## Runtime: the unified Walker tree

### Definition

```rust
// new module: rust/src/structure_designer/evaluator/iterator_walker.rs

use std::sync::Arc;
use crate::structure_designer::evaluator::function_evaluator::FunctionEvaluator;
use crate::structure_designer::evaluator::network_evaluator::{NetworkEvaluationContext, NetworkEvaluator};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;

#[derive(Clone)]
pub struct Walker {
    kind: WalkerKind,
    fused: bool,
}

#[derive(Clone)]
enum WalkerKind {
    FromArray {
        // Shared so `Walker::clone` is O(1). The display path,
        // `evaluate_required`, and `parameter::eval` all clone the walker
        // on every read; without sharing, a 10⁶-element source array would
        // copy its full payload per clone — defeating the fusion goal for
        // the very common `[T] → Iter[T]` wrap. See §"Evaluation model".
        // (`Arc` over `Rc` for forward-compat with multi-threaded eval;
        // swap to `Rc` if `Walker` ends up not needing `Send`.)
        items: Arc<Vec<NetworkResult>>,
        idx: usize,
    },
    Range {
        start: i32,
        step: i32,
        count: i32,
        emitted: i32,
    },
    Map {
        source: Box<Walker>,
        fe: FunctionEvaluator,
    },
    Filter {
        source: Box<Walker>,
        fe: FunctionEvaluator,
    },
    Product {
        axes: Vec<Walker>,
        field_names: Vec<String>,
        current: Vec<NetworkResult>,
        primed: bool,
        done: bool,
    },
}
```

`Walker` lives next to `network_result.rs`, in `evaluator/iterator_walker.rs`. The `NetworkResult::Iterator(Walker)` variant carries a walker as its payload.

### Constructors (geo_tree style)

```rust
impl Walker {
    pub fn from_array(items: Vec<NetworkResult>) -> Self { ... }
    pub fn range(start: i32, step: i32, count: i32) -> Self { ... }
    pub fn map(source: Walker, fe: FunctionEvaluator) -> Self { ... }
    pub fn filter(source: Walker, fe: FunctionEvaluator) -> Self { ... }
    pub fn product(axes: Vec<Walker>, field_names: Vec<String>) -> Self { ... }
}
```

All constructors initialize `fused: false`.

No hash. We do not need content-addressed caching the way `geo_tree` does; walkers are consumed once and dropped.

### `next` and `reset`

```rust
impl Walker {
    pub fn next(
        &mut self,
        evaluator: &NetworkEvaluator,
        registry: &NodeTypeRegistry,
    ) -> Option<NetworkResult>;

    pub fn reset(&mut self);
}
```

`next` returns:
- `None` — stream end.
- `Some(NetworkResult::Error(_))` — error mid-stream; consumer aborts.
- `Some(other)` — the next element.

The `fused` flag on the outer `Walker` enforces the "Error once, then `None`" contract uniformly:

```rust
pub fn next(&mut self, evaluator, registry) -> Option<NetworkResult> {
    if self.fused { return None; }
    let result = self.kind.next_inner(evaluator, registry);
    if let Some(NetworkResult::Error(_)) = &result {
        self.fused = true;
    }
    result
}
```

This means individual variants do **not** need to track an error-fuse flag of their own. They yield `Some(Error(...))` once when something goes wrong; the outer `Walker` flips `fused` and returns `None` on every subsequent call. (Product's internal `done` flag is unrelated — it tracks natural exhaustion of the cartesian counter, not error state.)

`reset` rewinds the walker to its initial state and clears `fused`. Cascading is mechanical; there are no public `init()` / `start()` methods on the consumer side.

### Per-variant semantics

**`FromArray`** — `idx` points to the next item to yield. `next` returns `items[idx].clone()` and increments; returns `None` when `idx == items.len()`. `reset` sets `idx = 0`. `items` is `Arc`-shared across walker clones so cloning is O(1) regardless of array length; `idx` is per-walker, so independent advancement of clones is preserved (Invariant 2). The constructor `Walker::from_array(items: Vec<NetworkResult>)` wraps in `Arc::new` internally — callers don't see the `Arc`.

**`Range`** — emits `start, start+step, ..., start+(count-1)*step`. `emitted` counts how many it has produced. `reset` sets `emitted = 0`.

**`Map`** — `next` calls `source.next()`, propagates `None` and `Error`, otherwise calls `fe.set_argument_value(0, elem); fe.evaluate(...)` and returns the result (which may itself be `NetworkResult::Error(...)` — the outer `Walker`'s fuse handles stickiness). `reset` calls `source.reset()`. The FE has no per-element state to reset (each `next` overwrites argument 0 in full).

**`Filter`** — `next` loops: pull from `source`, if `Bool(true)` yield the element, if `Bool(false)` continue, if `Error` propagate, if anything else return `Some(Error("filter: f returned non-Bool"))`. `reset` calls `source.reset()`. As with `Map`, no internal fuse state — the outer `Walker` handles it.

**`Product`** — the only variant with non-trivial state. Pseudocode:

```
fn next(&mut self, evaluator, registry) -> Option<NetworkResult> {
    if self.done { return None; }

    if !self.primed {
        for axis in &mut self.axes {
            match axis.next(evaluator, registry) {
                None              => { self.done = true; return None; }
                Some(Error(e))    => { self.done = true; return Some(Error(e)); }
                Some(v)           => self.current.push(v),
            }
        }
        self.primed = true;
        return Some(self.build_record());
    }

    let n = self.axes.len();
    for i in (0..n).rev() {
        match self.axes[i].next(evaluator, registry) {
            Some(Error(e))    => { self.done = true; return Some(Error(e)); }
            Some(v)           => {
                self.current[i] = v;
                return Some(self.build_record());
            }
            None              => {
                self.axes[i].reset();
                match self.axes[i].next(evaluator, registry) {
                    None             => { self.done = true; return None; }
                    Some(Error(e))   => { self.done = true; return Some(Error(e)); }
                    Some(v)          => self.current[i] = v,
                }
                // fall through to advance the next-leftward axis
            }
        }
    }

    self.done = true;
    None
}

fn build_record(&self) -> NetworkResult {
    let fields: Vec<(String, NetworkResult)> = self.field_names
        .iter()
        .zip(self.current.iter())
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    NetworkResult::record(fields)
}
```

`reset` calls `axis.reset()` for every axis, clears `current`, sets `primed = false` and `done = false`.

Output ordering matches today's `product.rs`: rightmost field varies fastest. Output cardinality = ∏|axes|. Empty axis ⇒ empty product.

### `FunctionEvaluator` lifecycle

`FunctionEvaluator::new` (function_evaluator.rs:40-108) is "somewhat expensive" by its own docstring: it allocates a one-node-plus-N-value-nodes ad-hoc `NodeNetwork`, clones source data, populates the dynamic-type-cache, wires value nodes into parameters. We pay this cost **once per `map.eval()` / `filter.eval()` invocation** and store the FE inside `WalkerKind::Map` / `WalkerKind::Filter`.

Per-element cost is then:

```
fe.set_argument_value(0, elem);   // single ValueData replacement
fe.evaluate(evaluator, registry); // runs the standard evaluator on the FE's tiny network
```

`fe.evaluate` allocates a fresh `NetworkEvaluationContext` per call — same as today's `map.rs:91-107`. If profiling shows context allocation as a hot spot we can revisit, but it's not in scope here.

`FunctionEvaluator` must derive `Clone`. This requires `NodeNetwork: Clone` (already true; networks are cloned in serialization, snapshots, etc.). The clone **is** exercised at runtime — `EvalOutput::get`, `evaluate_required`, etc. all clone the enclosing `NetworkResult` and therefore the enclosed FE — but at most O(consumer_count + display_clones) times per evaluation pass, never per element. Each clone produces an independent FE (no shared mutable state via `Rc`/`Arc` inside FE), so `set_argument_value` on a clone does not disturb the original.

Because the evaluator does not cache pin results (Invariant 1), `map.eval()` runs once per consumer per evaluation pass — so FE construction cost is **per-consumer-per-evaluation**, not amortized across consumers. For pipelines with high fan-out, each consumer pays one FE construction. This is the per-call cost; the per-element cost (`set_argument_value` + `fe.evaluate`) is unaffected.

### Construction-time error handling

If a `Closure` cannot be turned into a working FE — source network missing from the registry, source node missing from the network — `map.eval()` / `filter.eval()` should detect this and emit `EvalOutput::single(NetworkResult::Error(...))` directly, **without** producing a walker. Today's `FunctionEvaluator::new` silently produces a degenerate FE in this case; with the iterator design, a degenerate FE would multiply confusing errors across the stream. Adding an early-return at recipe-build time fixes both old and new use cases. (`FunctionEvaluator::new` itself does not change; the check lives in the calling node's `eval`.)

## Per-node changes

### `map`

**Today.** Stored properties: `input_type`, `output_type`. Pins: `xs: Array[input_type]`, `f: input_type -> output_type`, output: `Array[output_type]`. Eager: builds the full result vector.

**Proposed.** Stored properties unchanged. Pins: `xs: Iter[input_type]` (accepts `[input_type]` via the implicit wire conversion), `f: input_type -> output_type`, output: `Iter[output_type]`.

Eval skeleton:

```rust
fn eval(&self, evaluator, stack, node_id, registry, _decorate, context) -> EvalOutput {
    let xs = evaluate_arg_required(..., 0);
    let xs_walker = match xs {
        NetworkResult::Iterator(w) => w,
        NetworkResult::Array(items) => Walker::from_array(items),  // belt-and-braces; the wire conversion already does this
        NetworkResult::Error(_)    => return EvalOutput::single(xs),
        _ => return EvalOutput::single(NetworkResult::Error("map: xs is not an iterator".into())),
    };

    let f = evaluate_arg_required(..., 1);
    let closure = match f {
        NetworkResult::Function(c) => c,
        NetworkResult::Error(_)    => return EvalOutput::single(f),
        _ => return EvalOutput::single(NetworkResult::Error("map: f is not a function".into())),
    };

    let fe = match try_build_fe(closure, registry) {
        Ok(fe)   => fe,
        Err(msg) => return EvalOutput::single(NetworkResult::Error(format!("map: {}", msg))),
    };

    EvalOutput::single(NetworkResult::Iterator(Walker::map(xs_walker, fe)))
}
```

`try_build_fe` is a small helper that calls `FunctionEvaluator::new` and verifies the constructed FE is well-formed (the source network and node both resolved). If `FunctionEvaluator::new` itself becomes fallible (returning `Result<Self, String>`), the helper is just a thin wrapper.

`calculate_custom_node_type` updates the parameter and output pin types to use `Iter[T]`:

```rust
custom_node_type.parameters[0].data_type = DataType::Iterator(Box::new(self.input_type.clone()));
custom_node_type.parameters[1].data_type = DataType::Function(FunctionType {
    parameter_types: vec![self.input_type.clone()],
    output_type: Box::new(self.output_type.clone()),
});
custom_node_type.output_pins =
    OutputPinDefinition::single(DataType::Iterator(Box::new(self.output_type.clone())));
```

### `filter`

Same shape as `map`. Pins: `xs: Iter[T]`, `f: T -> Bool`, output: `Iter[T]`. Returns `NetworkResult::Iterator(Walker::filter(xs_walker, fe))`. The non-`Bool` runtime check moves into `Walker::filter`'s `next()`.

### `fold` (terminal consumer)

**Today.** Pins: `xs: Array[T]`, `init: Acc`, `f: (Acc, T) -> Acc`, output: `Acc`. Walks the full vector eagerly.

**Proposed.** Pins: `xs: Iter[T]` (accepts `[T]` via wire conversion), `init: Acc`, `f: (Acc, T) -> Acc`, output: `Acc` (unchanged).

Eval skeleton:

```rust
fn eval(...) -> EvalOutput {
    let xs = evaluate_arg_required(..., 0);
    let mut walker = match xs {
        NetworkResult::Iterator(w) => w,
        NetworkResult::Array(items) => Walker::from_array(items),
        NetworkResult::Error(_)    => return EvalOutput::single(xs),
        _ => return EvalOutput::single(NetworkResult::Error("fold: xs is not an iterator".into())),
    };

    let init = evaluate_arg_required(..., 1);
    if let NetworkResult::Error(_) = init { return EvalOutput::single(init); }

    let f = evaluate_arg_required(..., 2);
    let closure = match f { /* same as map */ };
    let mut fe = try_build_fe(closure, registry)?;

    let mut acc = init;
    loop {
        match walker.next(evaluator, registry) {
            None => break,
            Some(NetworkResult::Error(e)) => return EvalOutput::single(NetworkResult::Error(e)),
            Some(elem) => {
                fe.set_argument_value(0, acc);
                fe.set_argument_value(1, elem);
                acc = fe.evaluate(evaluator, registry);
                if let NetworkResult::Error(_) = &acc {
                    return EvalOutput::single(acc);
                }
            }
        }
    }

    EvalOutput::single(acc)
}
```

`fold` is the only one of the original four nodes that does not produce an iterator — it consumes one. The other terminal consumer is the new `collect` node.

### `product`

**Today.** Pins: `Array[T_i]` per field of the target record schema, output: `Array[Record(target)]`. Eager: builds the full vector with mixed-radix counter.

**Proposed.** Pins: `Iter[T_i]` per field (accepts `[T_i]` via wire conversion), output: `Iter[Record(target)]`. The same odometer logic moves into `Walker::product`'s `next()`/`reset()`. Pins keep their authored field order; emitted record values keep their canonical (sorted-by-name) field order — same convention as today.

Eval skeleton:

```rust
fn eval(...) -> EvalOutput {
    let Some(def) = registry.lookup_record_type_def(&self.target) else {
        return EvalOutput::single(NetworkResult::None);
    };

    if def.fields.is_empty() {
        // empty target: cartesian product of zero axes is one record value with no fields
        return EvalOutput::single(NetworkResult::Iterator(
            Walker::from_array(vec![NetworkResult::record(vec![])])
        ));
    }

    let mut axes = Vec::with_capacity(def.fields.len());
    let mut field_names = Vec::with_capacity(def.fields.len());
    for (i, (field_name, _)) in def.fields.iter().enumerate() {
        let v = evaluate_arg(..., i);
        let walker = match v {
            NetworkResult::Iterator(w) => w,
            NetworkResult::Array(items) => Walker::from_array(items),
            NetworkResult::None        => return EvalOutput::single(NetworkResult::None),
            NetworkResult::Error(_)    => return EvalOutput::single(v),
            _ => return EvalOutput::single(NetworkResult::Error(format!(
                "product: input '{}' did not resolve to an iterator", field_name))),
        };
        axes.push(walker);
        field_names.push(field_name.clone());
    }

    EvalOutput::single(NetworkResult::Iterator(Walker::product(axes, field_names)))
}
```

The output element type for the iterator is `Record(Named(target))`, so the wire-time `Iter[S] → Iter[T]` rule still gates whether downstream consumers accept it.

### `range`

**Today.** Pins: `start: Int`, `step: Int`, `count: Int`, output: `Array[Int]`. Stored properties for default scalar values.

**Proposed.** Same pins and properties. Output type changes to `Iter[Int]`. Eval becomes:

```rust
EvalOutput::single(NetworkResult::Iterator(Walker::range(start, step, count)))
```

### `collect` (new node)

**Purpose.** Converts `Iter[T]` to `[T]` by exhausting the iterator. The escape hatch when a downstream array-typed node really does want the whole vector.

**Stored property.** `element_type: DataType` — the element type T.

**Pins.** `iter: Iter[ElementType]` (input), single output `Array[ElementType]`.

**Eval.**

```rust
fn eval(...) -> EvalOutput {
    let v = evaluate_arg_required(..., 0);
    let mut walker = match v {
        NetworkResult::Iterator(w) => w,
        NetworkResult::Array(items) => return EvalOutput::single(NetworkResult::Array(items)), // no-op pass-through
        NetworkResult::Error(_)    => return EvalOutput::single(v),
        _ => return EvalOutput::single(NetworkResult::Error("collect: input is not an iterator".into())),
    };

    let mut out = Vec::new();
    loop {
        match walker.next(evaluator, registry) {
            None => break,
            Some(NetworkResult::Error(e)) => return EvalOutput::single(NetworkResult::Error(e)),
            Some(v) => out.push(v),
        }
    }
    EvalOutput::single(NetworkResult::Array(out))
}
```

No length cap. If the user wires a 10⁹-element iterator into `collect` they get OOM — that's the contract. (We considered an optional `max` property; punted as premature.)

`get_text_properties` / `set_text_properties`: `element_type` round-trips as a `TextValue::DataType`. Same pattern as `array_at` / `array_concat` / `array_append`.

### Nodes that do NOT change

`array_at`, `array_len`, `array_concat`, `array_append`, `sequence` keep their `Array[T]` typing. They are random-access primitives; making them iterator-aware would either force consumption (defeats the purpose) or add cap-related properties (premature).

`expr` array literals, `expr` array indexing (`arr[i]`), `expr` `len(arr)` / `concat` / `append` are unchanged. Expressions are eager.

Function partial application is unchanged. Closure capture of upstream pin values is unchanged for non-iterator types — but **`Iter[T]` values are not capturable into closures** (see §"Iterator values cannot be captured into closures" below). The only thing that changes on the closure side is what some nodes (`map`, `filter`) *do* with the captured closure — they store the FE in a walker rather than running it eagerly inside `eval()`.

## Wire-time conversions

All conversions live in `DataType::can_be_converted_to` and the matching arms in `NetworkResult::convert_to`:

| Source | Destination | Runtime effect |
|---|---|---|
| `Array(items)` (elem type S) | `Iter[T]` (where `S → T`) | eagerly convert each item to T, then wrap as `Walker::from_array(converted_items)` |
| `S` (scalar, where `S → T`) | `Iter[T]` | eagerly convert the value to T, wrap as `Walker::from_array(vec![converted_value])` |
| `Iter[T]` | `Iter[T]` | identity — pass the walker through unchanged |
| `Iter[S]` | `Iter[T]` (`S ≠ T`) | **disallowed at validation time** (deferred to a follow-up) |
| `Iter[T]` | `[T]` | **disallowed at validation time** |
| `Iter[T]` | `T` | **disallowed at validation time** |

Because every wire that produces an iterator value either yields elements of the *exact* declared element type (the wrap rules convert eagerly; the identity rule preserves it) or fails validation, every walker in the runtime tree yields elements of its declared element type. Consumers (`Walker::Map`, `Walker::Filter`, `fold`, `collect`, product axes) feed pulled elements straight to their `FunctionEvaluator` / accumulator / output without any per-element `convert_to` call.

This is the v1 simplification that buys correctness for free: no converting-walker variant, no per-consumer per-element conversion code, no surprises about *when* conversion runs in a lazy chain. Lazy `Iter[S] → Iter[T]` conversion is a clean future addition (one new walker variant) when there's demand.

## Display

A node whose displayed pin output is `Iter[T]` cannot show its full payload — it could be unbounded. Display behavior:

1. Auto-collect with a fixed cap of **256 elements**.
2. Show subtitle hint: `Iter[T] (showing first 256)` if the walker yielded ≥256 elements; otherwise `Iter[T] (N elements)` if the walker exhausted before the cap.
3. Errors mid-stream surface as today (the displayed result is `NetworkResult::Error(...)` with the captured message).

The 256 figure is tunable; ship it as a constant in `common_constants.rs` (`ITER_DISPLAY_CAP`). If users want more they can wire `collect` and display the array.

### Display path interaction with consumers

The display path is a separate evaluation from any downstream consumer:

1. `generate_scene(displayed_node)` calls `evaluate_all_outputs(displayed_node)`, which calls `displayed_node.data.eval()` once. For an `Iter[T]` output, this constructs **walker D**, owned by the resulting `EvalOutput`.
2. The pin-value loop in `generate_scene` calls `eval_output.get(pin_index)` (`network_evaluator.rs:211, 257-268`), which `.cloned()`s the result. The auto-collect drain consumes the *clone*; the original D in `EvalOutput` is dropped after the multi-output loop finishes.
3. Any downstream consumer of `displayed_node` runs its own `evaluate_arg` → `evaluate(displayed_node)` → fresh `eval()` → fresh **walker D'**, independent of D.

So display drains one walker, each consumer drains its own. They do not share, and the display drain (capped at 256) does not partially consume any consumer's view of the stream. Per Invariant 2, the clone in step 2 is O(1) for `FromArray` (shared `Arc<Vec>`) and `Range`; for `Map` / `Filter` / `Product` it pays a recursive walker clone plus an FE clone, but never per-element work.

## Errors

Errors are **values that flow through the stream**, not panics. A walker yields `Some(NetworkResult::Error(_))` exactly once and then `None` on subsequent calls — enforced uniformly by the outer `Walker`'s `fused` flag (see §"Per-variant semantics"), so individual variants don't need their own error-fuse bookkeeping. Consumers (`fold`, `collect`, the display path) check for the `Error` variant on every `next` and abort.

This matches today's eager nodes: `map.rs:101-103` early-returns on the first error; `fold` does likewise. The only new wrinkle is that for lazy pipelines, errors can surface at element K rather than during the producer's `eval()`.

## Backward compatibility

The breaking change is: in v4, the output pin type of `map` / `filter` / `product` / `range` switches from `[T]` to `Iter[T]`. A v3 `.cnnd` file may have wires from those outputs into destination pins typed `[T]`; under v4 typing rules those wires are invalid because `Iter[T] → [T]` is not an implicit conversion. To keep v3 files loadable, a JSON pre-pass inserts a `collect` node on each affected wire.

### Strategy summary

Mirrors the existing v2→v3 migration (`doc/design_cnnd_migration_v2_to_v3.md`):

- **In-process**, at file load time. No separate offline converter.
- **Bump `SERIALIZATION_VERSION` from 3 to 4.**
- **JSON pre-pass** on `serde_json::Value` before strict deserialization. Lets us synthesize new nodes (`collect`s) and rewire arguments — serde-level field-default compat cannot express node synthesis, the same reason v2→v3 needed a pre-pass for the `atom_fill` split.
- **Version-4 files never go back through the pre-pass.** The pre-pass is a one-way historical up-converter; version dispatch skips it for current files.

### Version dispatch

Extend the existing single-step dispatch in `load_node_networks_from_file` (`node_networks_serialization.rs:632-664`) to a chained dispatch:

```rust
if version < 3 {
    super::migrate_v2_to_v3::migrate_v2_to_v3(&mut root_value)?;
}
if version < 4 {
    super::migrate_v3_to_v4::migrate_v3_to_v4(&mut root_value)?;
}
// Bump in-memory `version` to SERIALIZATION_VERSION (= 4) after all passes.
```

A v2 file chains through both passes; a v3 file runs only v3→v4; a v4 file runs neither. The `MigrationError` type is reused (re-export from `migrate_v2_to_v3` or hoist into `serialization/mod.rs`); the load path wraps `Err` into `io::Error` prefixed `"v3→v4 migration failed: "`, parallel to the v2→v3 wrapper at `node_networks_serialization.rs:651-656`.

### Migration module layout

New module: `rust/src/structure_designer/serialization/migrate_v3_to_v4.rs`. Top-level entry:

```rust
pub fn migrate_v3_to_v4(root: &mut serde_json::Value) -> Result<(), MigrationError>;
```

Internal helpers, one per logical pass:

- `compute_iterator_producer_set(root)` — fixed-point over all networks: which custom networks now produce `Iter[T]` because their return node is one of the four built-in producers (transitively through nested custom networks).
- `insert_collect_for_iter_to_array_wires(network_json, iter_producers, &mut next_collect_id)` — per network, for each wire matching predicates (A) and (B) below, allocate a new id, synthesize a `collect`, rewire the destination.

Each helper is independent and unit-tested. Top-level entry orchestrates: compute the set once, then iterate networks.

### Detection: which wires need a `collect`

Two predicates govern the rewrite. Insert a `collect` iff both hold for a given wire `(src_node_id, src_pin_index) → (dst_node_id, dst_param_index)`:

**(A) Source produces `Iter[T]` in v4.** Source is either:
- A built-in node of type `range` / `map` / `filter` / `product`, or
- An instance of a custom network in the iterator-producer set computed transitively (see below).

**(B) Destination's expected pin type is *not* `Iter[T]`.** Determined by:
- For a built-in destination: a hardcoded **v4 iterator-pin table** baked into the migration module — the small, stable set of built-in pins that natively accept `Iter[T]`:

  ```rust
  // (node_type_name, parameter_index): pins that accept Iter[T] natively in v4.
  // Frozen at the v4 release; do not refresh against the live registry.
  const ITERATOR_PINS_V4: &[(&str, usize)] = &[
      ("map", 0),     // xs
      ("filter", 0),  // xs
      ("fold", 0),    // xs
      ("collect", 0), // iter
      // product: every parameter pin (variable count, all axes accept Iter)
  ];
  // product is special-cased: for any parameter index, return true.
  ```

  Hardcoded — not read from the live `NodeTypeRegistry` — for the same reason `migrate_v2_to_v3` hardcodes `PRIMITIVE_LATTICE_PIN`: migration logic is **frozen at the v4 release**. Future registry changes (renames, new iterator pins) must not retroactively alter how a v3 file gets up-converted.

- For a custom-network destination: read from the file's `node_type.parameters[dst_param_index].data_type` string. If it parses as `Iter[..]`, predicate (B) fails (no insertion); otherwise it holds. Custom networks store their parameter pin types as the user-authored DataType in the JSON — those strings are unchanged by v4, so reading them is correct.

Wires whose destination expects something other than `[T]` *or* `Iter[T]` (e.g. `Int`, a record, etc.) were already invalid in v3 and remain invalid in v4 — predicate (B) still holds, so a `collect` is inserted, but the resulting `Array[T] → Int` (or whatever) wire is then dropped by the validator on the destination side. That's fine: the migration's job is only to remove the *iterator-specific* incompatibility introduced by v4; pre-existing invalidities pass through to the validator unchanged.

### Computing the iterator-producer set transitively

A custom network's v4 output type is determined by its return node's output type:

```text
fn produces_iter(network_name, visited) -> Option<DataType>:
    if network_name in visited: return None  // cycle: defensive, custom-network cycles are
                                             // rejected at network creation but possible in
                                             // hand-edited files.
    visited.insert(network_name)
    let net = networks[network_name]
    let ret_id = net.return_node_id?  // None if return node missing → no iter output
    let ret_node = net.nodes[ret_id]?  // None if dangling → no iter output
    match ret_node.node_type_name:
        "range"   => Some(Iter[Int])
        "map"     => Some(Iter[<MapData.output_type from JSON>])
        "filter"  => Some(Iter[<FilterData.element_type from JSON>])
        "product" => Some(Iter[Record(Named(<ProductData.target from JSON>))])
        custom_name if custom_name in networks =>
            produces_iter(custom_name, visited)
        _ => None
```

Run as a **fixed-point pass** over all networks (custom A's iterator-ness depends on B's; B's may depend on a deeper custom). The function above is naturally recursive; memoize per network. The result is a `HashMap<String, DataType>` mapping custom-network name → its v4 `Iter[T]` output type. Networks not in the map produce non-iterator output and are not iterator sources for predicate (A).

Note the field names: `MapData.output_type` already exists and is touched by v2→v3 (`migrate_v2_to_v3.rs:182-189`); `FilterData.element_type` and `ProductData.target` exist in v3 nodes already. The migration reads them as JSON values without going through strict deserialization.

### Per-wire transformation

For each wire matching predicates (A) and (B), in deterministic order (sort by `(network_name, dst_node_id, dst_param_index, src_node_id, src_pin_index)` in the read-only pre-pass):

1. **Allocate** a new node id from the network's `next_node_id`. Bump `next_node_id` after all allocations for that network.
2. **Compute element type T** of the source's `Iter[T]`:
   - `range` → `DataType::Int`.
   - `map` → `MapData.output_type` from the source's `data` JSON.
   - `filter` → `FilterData.element_type` from the source's `data` JSON.
   - `product` → `Record(Named(ProductData.target))`.
   - Custom-network source → strip the outer `Iter[..]` from the value computed by `produces_iter`.
3. **Synthesize** a `collect` node and append it to the network's `nodes` array:

   ```rust
   serde_json::json!({
       "id": new_id,
       "node_type_name": "collect",
       "position": [src_position[0] + 130.0, src_position[1]],  // snapped to integers
       "arguments": [
           { "argument_output_pins": { src_node_id.to_string(): src_pin_index } }
       ],
       "data_type": "collect",
       "data": { "element_type": <T encoded as a DataType JSON value> }
   })
   ```

   The `element_type` JSON encoding mirrors how `array_at` / `array_concat` already serialize their stored DataType — primitive variants are plain strings (`"Int"`), `Array(inner)` is `{"Array": <inner>}`, `Iterator(inner)` is `{"Iterator": <inner>}`, `Record(Named(n))` is `{"Record": {"Named": n}}`. The migration helper uses the same encoder.

4. **Rewire** the destination's argument: in `dst_node.arguments[dst_param_index].argument_output_pins`, remove the entry `{src_node_id: src_pin_index}` and insert `{new_id: 0}`. If the destination's argument also held wires from *other* sources (a fan-in array pin), those are left in place — only the iterator-source entry is rewritten.

### ID, position, and ordering rules

- **ID allocation order is deterministic.** A read-only pre-pass collects all triples to rewrite (sorted as above); the mutation pass allocates ids in that order. This makes the produced JSON byte-identical across runs and idempotent under double-migration in tests (same as v2→v3's `splits` and `adaptations` patterns).
- **Position:** snap to integers. The default placement is 130 units to the right of the source node, on the same y. Snapping avoids the f64 round-trip ULP drift that breaks `cnnd_roundtrip_test` (documented in `migrate_v2_to_v3.rs:631-633` and `:983-987`).
- **Fan-out from one source pin to multiple non-iterator destinations** synthesizes one `collect` per destination, not one shared `collect`. Matches v2→v3's policy of synthesizing per-consumer adapters; keeps the migration purely local and avoids the "where do I place a shared adapter" question.

### Idempotence

Production never re-runs the v3→v4 pass on a v4 file (version dispatch skips). The internal logic is also naturally idempotent: a v4 file run through the migration a second time finds zero wires matching predicate (A) → predicate (B) (every iterator-producer source already feeds either an iterator-accepting pin or an inserted `collect`, never a raw array consumer). Tests that exercise double-migration assert byte-identity on the second pass, mirroring `migrate_v2_to_v3`'s test harness.

### Display state cleanup

The migration does **not** touch `displayed_node_ids` or `displayed_output_pins`. Existing entries reference source/destination nodes (which still exist post-migration); the newly inserted `collect`s default to hidden, matching how a freshly-added node would look. No filtering needed (unlike v2→v3's deleted-node drop).

### Error policy

Drop-with-dangling-wires, consistent with v2→v3 (`migrate_v2_to_v3.rs:16-37`):

- Malformed `arguments` array (wrong shape, non-integer keys) on a node: skipped silently with `let Some(..) else { continue }`. The validator surfaces it post-load.
- `MapData` / `FilterData` / `ProductData` whose stored type field can't be parsed as a DataType: synthesize the `collect` with `element_type = DataType::None` as a defensive default. The validator will then drop the now-mistyped `collect.iter` wire on the destination side; the user sees a broken wire rather than a failed load.
- Custom-network source whose iterator-ness depends on a missing return node or a return node referring to a missing custom network: predicate (A) returns `None`, no insertion. Same partial-breakage outcome.

The `MigrationError::MalformedStructure` variant is reserved for hard-fail conditions; v3→v4 does not need to construct it for v1 of the migration.

### Bumping `next_node_id` per network

Each network's `next_node_id` is bumped by the count of `collect` nodes inserted into that network. The pre-pass computes the count; the mutation pass writes the new value once. Pattern matches `synthesise_structure_for_atom_fill` (`migrate_v2_to_v3.rs:686-688`).

### In-process compatibility (no migration needed)

The implicit wire conversions `[T] → Iter[T]` and `T → Iter[T]` (see §"Type system: `Iter[T]`") cover every wire whose **source is unchanged but whose destination is now an iterator pin**. So `[1,2,3] → map.xs` keeps working with no migration intervention — the v3 source is still a `NetworkResult::Array`, and the wire-time conversion eagerly wraps it as `Walker::from_array`. The migration is needed only when an old `<iterator producer> → <array consumer>` wire would lose its source's array-ness in v4.

### Tests

Following the existing pattern in `rust/tests/fixtures/`:

| Fixture | Pre-iterator (v3) shape | Post-migration (v4) shape |
|---|---|---|
| `iterator_migration/old_map_to_array_at.cnnd` | `map → array_at` | `map → collect → array_at` |
| `iterator_migration/old_range_to_array_len.cnnd` | `range → array_len` | `range → collect → array_len` |
| `iterator_migration/old_filter_to_array_concat.cnnd` | `filter → array_concat` | `filter → collect → array_concat` |
| `iterator_migration/old_product_to_array_at.cnnd` | `product → array_at` | `product → collect → array_at` |
| `iterator_migration/old_map_to_map.cnnd` | `map → map` | unchanged (new `map.xs` accepts `Iter[T]`) |
| `iterator_migration/old_map_to_fold.cnnd` | `map → fold` | unchanged (new `fold.xs` accepts `Iter[T]`) |
| `iterator_migration/old_range_to_custom_array_pin.cnnd` | `range → custom_net.xs` where `custom_net` declares `xs: [Int]` parameter | `range → collect → custom_net.xs` |
| `iterator_migration/old_custom_iter_producer_to_array_at.cnnd` | `[custom whose return node is `range`] → array_at` | `[custom instance] → collect → array_at` (verifies transitive predicate A) |
| `iterator_migration/old_double_fanout.cnnd` | one `map` source pin feeding both `array_at` and `array_len` | two `collect`s inserted, one per consumer site |
| `iterator_migration/old_v2_with_iterator_chain.cnnd` | a v2 file containing a `map → array_at` chain | chains through `migrate_v2_to_v3` then `migrate_v3_to_v4`; final shape: post-rename + `collect` inserted |
| `iterator_migration/idempotence_v4_double_run.cnnd` | already-v4 file containing previously inserted `collect`s | re-running `migrate_v3_to_v4` is a no-op; output is byte-identical to input (asserts §"Idempotence") |
| `iterator_migration/old_malformed_map_output_type.cnnd` | `MapData.output_type` field unparseable as a DataType | migration synthesises a `collect` with `element_type = DataType::None`; validator drops the now-mistyped wire (asserts §"Error policy") |
| `iterator_migration/old_malformed_arguments.cnnd` | a node with malformed `arguments` shape on a wire that would otherwise need rewriting | the affected wire is silently skipped during migration; surfaces post-load via the validator (asserts §"Error policy") |

The `cnnd_roundtrip_test.rs` harness loads the pre-iterator fixture, runs the load path (which executes the migration), and asserts:

- The loaded network passes `network_validator::validate_network` cleanly.
- Evaluated output matches a snapshotted post-iterator value.
- Re-saving and reloading produces byte-identical JSON the second time around (idempotence on v4).

Each touched node type (`map`, `filter`, `product`, `range`) has at least one fixture; the transitive and double-migration cases each have one fixture.

## Implementation phases

Each phase is independently testable and shippable.

### Phase 1: Type system + Walker scaffolding

1. Add `DataType::Iterator(Box<DataType>)` + `Iter[T]` text format + `from_string` parsing.
2. Add `can_be_converted_to` rules: `[T] → Iter[T]`, `T → Iter[T]`, `Iter[S] → Iter[T]`. **Not** `Iter[T] → [T]`.
3. Add `NetworkResult::Iterator(Walker)` variant.
4. Implement `Walker` and all `WalkerKind` variants in `evaluator/iterator_walker.rs`.
5. Implement `next` and `reset` for every variant.
6. Make `FunctionEvaluator` derive `Clone` (verify `NodeNetwork: Clone` first).
7. **Test scope for this phase**: walker unit tests in `tests/structure_designer/iterator_walker_test.rs` (every row of §"Tests / Walker unit tests" — construction, `next`, `reset`, error propagation, outer-fuse stickiness, clone independence, `Arc` sharing, partial-drain reset, construction-time error). Wire/validator tests in `tests/structure_designer/iter_type_test.rs` (every row of §"Tests / Type system / wire validation tests" — conversion rules, closure-capture rejection, top-level-parameter rejection at the CLI/API binding).
8. **Manual smoke**: nothing user-visible — pure scaffolding. The `Iter[T]` type appears in tooltips only after a node is flipped to produce it (Phase 3 onwards).

### Phase 2: `collect` node

1. Implement `nodes/collect.rs` (mirrors `array_concat` / `array_append` shape).
2. Register in `nodes/mod.rs` and `node_type_registry.rs`.
3. Snapshot test, text-format roundtrip, `.cnnd` roundtrip fixture.
4. Reference-guide entry.
5. **Test scope for this phase**: `collect` snapshot, text-format roundtrip (`get_text_properties`/`set_text_properties` for `element_type`), `.cnnd` roundtrip on a `collect` instance.
6. **Manual smoke**: `collect` appears in the node palette; the node-properties UI exposes the `element_type` property and round-trips through save/load. End-to-end manual testing of `collect` waits on Phase 3 (a node that produces `Iter[T]`).

### Phase 3: Flip `range` (and stand up the migration module)

1. Change `range`'s `output_pins` to `OutputPinDefinition::single(DataType::Iterator(Box::new(DataType::Int)))`.
2. Change `range.rs::eval` to produce `NetworkResult::Iterator(Walker::range(...))`.
3. **Bump `SERIALIZATION_VERSION` from 3 to 4.**
4. **Create `serialization/migrate_v3_to_v4.rs`** with the full structure laid out in §"Backward compatibility": entry point, `compute_iterator_producer_set`, `insert_collect_for_iter_to_array_wires`, `ITERATOR_PINS_V4` table, helper builders. Predicate (A) handles `range` as source in this phase; the `match` arms for `map` / `filter` / `product` are stubbed to return `None` and filled in by phases 4 and 6 below.
5. Hook the new pass into `load_node_networks_from_file` after `migrate_v2_to_v3` (chained dispatch as in §"Version dispatch").
6. Fixture: `iterator_migration/old_range_to_array_len.cnnd`.
7. **Test scope for this phase**: existing `range` tests pass via implicit conversions; new fixture asserting `range → fold` works without an explicit `collect`; v3→v4 migration smoke for the `range` arm of predicate (A); chained-dispatch v2→v3→v4 verified end-to-end on a v2 fixture containing `range`.
8. **Manual smoke**: open a v3 project containing `range`; verify it loads, evaluates, and the tooltip on `range`'s output reads `Iter[Int]`. Open a v3 file with `range → array_at`: post-load a synthesised `collect` should sit between them. Wire `range → fold` in a fresh project: should work without a manual `collect`.

### Phase 4: Flip `map` and `filter`

1. Update `map.rs::eval` / `filter.rs::eval` to build walkers (`Walker::map(..)` / `Walker::filter(..)`).
2. Update `calculate_custom_node_type` for both to declare `Iter[T]` pins.
3. Extend `migrate_v3_to_v4` predicate (A): fill in the `map` and `filter` arms of `produces_iter` (use `MapData.output_type` / `FilterData.element_type` for the element type of the synthesized `collect`).
4. Add fixtures: `iterator_migration/old_map_to_array_at.cnnd`, `old_filter_to_array_concat.cnnd`, `old_double_fanout.cnnd`.
5. **Test scope for this phase**: existing `map` / `filter` correctness tests pass via auto-conversion at the consumer end; the three new migration fixtures from step 4; the `range → map → fold` fusion test (asserts O(1) live elements via a counter inside the test FE); a `map → map` chain test (no intermediate materialisation).
6. **Manual smoke**: build `range → map → array_at` in a fresh project — auto-conversion handles the `Iter[T] → ...` boundary at the array consumer end. Open `old_double_fanout.cnnd`; verify two separate `collect` nodes are inserted, one per consumer (not a shared one).

### Phase 5: Flip `fold` to consume iterators

1. Update `fold.rs::eval` to walker-based consumption.
2. Update `calculate_custom_node_type` to declare `Iter[T]` for `xs`.
3. Add `("fold", 0)` to `ITERATOR_PINS_V4` (predicate (B) recognises `fold.xs` as Iter-accepting; existing `[T] → fold` wires keep working via implicit `[T] → Iter[T]` and need no migration).
4. **Test scope for this phase**: existing `fold` tests pass via implicit `[T] → Iter[T]`; new fusion correctness test (`range(0,1,N) → map(double) → fold(sum)` produces the expected sum); `[1,2,3] → fold(sum)` legacy-syntax test.
5. **Manual smoke**: build `[1,2,3] → fold(sum)` in the UI — the old-style array literal still evaluates correctly; build `range(0,1,1000) → map(double) → fold(sum)` and verify the numeric result.

### Phase 6: Flip `product` to lazy

1. Update `product.rs::eval` to build a walker.
2. Update the registry-aware cache populator (`build_node_type_for_target_with_defs`) to declare `Iter[T_i]` per axis pin.
3. Extend `migrate_v3_to_v4`: fill in the `product` arm of `produces_iter` (`Iter[Record(Named(ProductData.target))]`); special-case `product` in predicate (B) to treat *every* parameter index as iterator-accepting.
4. Add fixture: `iterator_migration/old_product_to_array_at.cnnd`.
5. **Test scope for this phase**: existing `product` tests pass via auto-conversion; the new `old_product_to_array_at.cnnd` fixture; fusion test (`product → map → fold` over a 10⁶-element product, asserts O(1) live elements); 3-axis `product` cardinality and ordering correctness; partial-drain-then-reset behaviour for `Product` mid-odometer.
6. **Manual smoke**: build a 2- or 3-axis `product` over small ranges in a fresh project; wire to an array consumer — auto-conversion synthesises `collect`. Open `old_product_to_array_at.cnnd`; verify a `collect` is inserted post-load.

### Phase 7: Transitive custom-network migration + display polish

1. Implement `compute_iterator_producer_set` as the fixed-point pass over all networks (§"Computing the iterator-producer set transitively"). Until this phase, the migration's predicate (A) only recognised the four built-in producers; with this phase a custom network whose return node transitively reaches one of them is also detected as an iterator source.
2. Add fixture: `iterator_migration/old_custom_iter_producer_to_array_at.cnnd` and `old_v2_with_iterator_chain.cnnd` (cross-tests v2→v3 + v3→v4 chaining).
3. Wire up auto-collect-with-cap on the display path (256 elements; constant `ITER_DISPLAY_CAP` in `common_constants.rs`).
4. Subtitle hint: `Iter[T] (showing first N)` / `Iter[T] (N elements)`.
5. Final migration sweep: collect any other pre-iterator fixtures across `tests/fixtures/` and snapshot the migrated forms with `cargo insta review`.
6. **Test scope for this phase**: transitive-detection fixtures (`old_custom_iter_producer_to_array_at.cnnd`, `old_v2_with_iterator_chain.cnnd`); migration idempotence fixture (re-run is byte-identical); defensive-parsing fixtures (malformed `MapData.output_type`, malformed `arguments`); display-cap boundary tests at 255 / 256 / 257 elements (subtitle wording per §"Display").
7. **Manual smoke**: open a v3 file whose custom network's return node is `range`; verify the synthesised `collect` is at the *outer* boundary, not inside the custom network. Display an `Iter[T]` output of a 1000-element walker — subtitle reads `(showing first 256)`. Display a 50-element walker — subtitle reads `(50 elements)`.

### Phase 8: Reference-guide rewrite

Rewrite the four affected sections in `doc/reference_guide/nodes/math_programming.md`:

- `map`: pins are `Iter[T]`, output is `Iter[U]`.
- `filter`: pins are `Iter[T]`, output is `Iter[T]`.
- `fold`: `xs` is `Iter[T]`; output and other pins unchanged.
- `product`: input pins are `Iter[T_i]`, output is `Iter[Record(target)]`.
- `range`: output is `Iter[Int]`.

Add a new section `### collect` under `## Math and programming nodes`. Add a short opening note explaining the iterator type, the implicit `[T] → Iter[T]` rule, and the explicit-`collect` requirement.

**Manual smoke**: read-through of the rewritten reference-guide section; cross-check terminology with the implementation; verify the doc covers the implicit `[T] → Iter[T]` / `T → Iter[T]` rules, the disallowed `Iter[T] → [T]`, and the explicit `collect` requirement.

## Tests

In addition to the per-phase unit tests:

### Walker unit tests (`tests/structure_designer/iterator_walker_test.rs`)

| Scenario | Walker | Expected |
|---|---|---|
| Construct empty `FromArray`, `next` | FromArray | `None` |
| Construct 3-element `FromArray`, drain | FromArray | yields all 3 in order, then `None` |
| Drain twice without reset | FromArray | second drain yields `None` immediately |
| Drain, `reset`, drain | FromArray | second drain yields all 3 again |
| `Range(0, 1, 5)` drain | Range | `0, 1, 2, 3, 4`, then `None` |
| `Range(10, -2, 3)` drain | Range | `10, 8, 6`, then `None` |
| `Range` drain, reset, drain | Range | second drain matches first |
| `Map { Range(0,1,5), f = x*2 }` drain | Map | `0, 2, 4, 6, 8`, then `None` |
| `Map` drain, reset, drain | Map | second drain matches first |
| `Filter { Range(0,1,10), f = x % 2 == 0 }` drain | Filter | `0, 2, 4, 6, 8`, then `None` |
| `Filter` with all-false predicate | Filter | drains to `None` immediately |
| `Filter` with non-Bool predicate | Filter | yields `Some(Error("filter: f returned non-Bool"))`, then `None` |
| `Product` of `[a, b]` × `[c, d]` | Product | `(a,c), (a,d), (b,c), (b,d)`, then `None`; rightmost varies fastest |
| `Product` with empty axis | Product | `None` immediately |
| `Product` of 1-axis | Product | yields each value |
| `Product` drain, reset, drain | Product | second drain matches first |
| Nested `Map { Filter { Range, even? }, x*x }` | composite | yields `0, 4, 16, 36, 64` for `Range(0,1,10)` |
| Error propagation: `Map` whose `f` errors at element 3 | Map | yields elements 0,1,2 then `Error`, then `None` |
| Outer-fuse stickiness: `Map` after first error | Map | next call after `Some(Error(_))` returns `None`, not another `Error` |
| Outer-fuse stickiness: `Filter` after non-Bool predicate | Filter | next call after `Some(Error("filter: f returned non-Bool"))` returns `None` |
| Clone independence (Invariant 2): `Map` mid-drain | Map | clone, advance the clone by 2, original `next()` still yields the next-in-sequence element relative to its own position |
| Clone independence (Invariant 2): `Product` mid-odometer | Product | clone, advance the clone, original yields the record it would have yielded without the clone |
| `FunctionEvaluator` clone independence | (FE) | clone, `set_argument_value(0, X)` on clone, original's stored argument 0 unchanged |
| `FromArray::items` Arc sharing | FromArray | clone walker; assert `Arc::strong_count == 2` and items are not deep-copied |
| Partial-drain reset: `FromArray` | FromArray | drain 2 of 5, reset, full drain yields all 5 in order |
| Partial-drain reset: `Product` | Product | drain 3 records mid-odometer, reset, full drain matches an initial drain |
| `Product` with 3 axes | Product | mixed-radix carry verified across all 3 axes (rightmost varies fastest) |
| Construction-time error: `map.eval()` with closure whose source network is missing | (eval-level) | `EvalOutput::single(NetworkResult::Error(_))`; no walker is constructed |

### Type system / wire validation tests (`tests/structure_designer/iter_type_test.rs`)

| Scenario | Expected |
|---|---|
| `[Int] → Iter[Int]` wire | Allowed; runtime wraps as `Walker::from_array` |
| `[Int] → Iter[Float]` wire (uses `Int → Float`) | Allowed; eager element conversion at wrap time |
| `Int → Iter[Int]` wire (single-element broadcast) | Allowed; runtime wraps single value as a 1-element `FromArray` |
| `Iter[Int] → Iter[Int]` wire | Allowed; identity passthrough — walker handed through, not re-wrapped |
| `Iter[Int] → Iter[Float]` wire (S ≠ T) | Rejected at validation; error names the deferred lazy-conversion case |
| `Iter[Int] → [Int]` wire | Rejected at validation; error points the user at `collect` |
| `Iter[Int] → Int` wire | Rejected at validation |
| Function pin captures an `Iter[T]` value-pin | Rejected at validation with documented error pointing at `collect` |
| Function pin captures `[Iter[T]]` (iterator nested in array) | Rejected at validation |
| Function pin captures `Record { field: Iter[T] }` | Rejected at validation |
| Top-level parameter declared `Iter[T]` (CLI/API binding) | Rejected at binding layer with documented error pointing at `collect` |

### Integration / fusion tests

| Pipeline | Memory profile assertion |
|---|---|
| `range(0,1,1_000_000) → map(double) → fold(sum)` | only one element alive at any time inside the test instrumentation |
| `range(0,1,1000) → product(target=Foo) → map(generate) → filter(passes_check) → fold(count)` | same |
| `[1,2,3] → fold(sum)` (legacy syntax via implicit conversion) | yields 6 without a wire migration |
| `range(0,1,N) → map(double)` with two consumers (`fold(sum)` and `collect → array_len`) | both consumers see the full stream — verifies Invariant 2 (per-consumer fresh walkers, no starvation under fan-out) |
| `range(0,1,N) → map(big)` displayed *and* consumed by `fold` | display drains its own clone (capped at 256), `fold` drains a separate walker over all N — verifies §"Display path interaction with consumers" |
| `[N elements] → map(double) → fold(sum)` with N=10⁶, fanned out to 3 consumers | total memory traffic stays O(N), not O(3·N) — verifies `Arc`-shared `FromArray::items` |
| Display cap boundary: walker yields 255 / 256 / 257 elements when its node is displayed | Subtitle reads `Iter[T] (255 elements)` / `Iter[T] (showing first 256)` / `Iter[T] (showing first 256)` respectively |

### `.cnnd` migration tests

See §"Backward compatibility / Tests" for the canonical fixture list (covers every touched node type, transitive custom-network detection, v2→v3→v4 chaining, fan-out, idempotence, and defensive parsing). The `cnnd_roundtrip_test.rs` harness asserts validator-clean load, snapshotted post-iterator value, and byte-identical re-save on the second pass.

## Open questions / left for follow-up

1. **`collect` cap?** Should `collect` have an optional `max: Int` property that errors out if exceeded? Useful guardrail; left out of this drop. If we add it later it's a property addition with the existing default-zero semantic, no breaking change.
2. **Lazy `Iter[S] → Iter[T]` element conversion.** Disallowed in v1. The clean follow-up is a new walker variant (e.g. `WalkerKind::Convert { source: Box<Walker>, source_elem_type, target_elem_type }`) that the wire layer wraps when `S → T` is allowed and `S ≠ T`. Pure additive change — no breaking impact on v1 wires or files. Add when users hit the restriction in practice.
3. **`flat_map`?** Easy to bolt on (`WalkerKind::FlatMap { source, fe, current_inner: Option<Walker> }` — pull from `source`, run `f` on each element to get an inner walker, drain that, then advance). Deferred; not blocking.
4. **`take` / `skip`?** Same shape as `flat_map`, trivial. Deferred.
5. **`zip`?** Needs a tuple type or `Record(target)` — cleanest path is an iterator analog of `product` that advances all axes in lockstep. Deferred.
6. **Iterators inside record fields?** Currently disallowed (see Type system / `is_tag_only_widening` section). If we ever want `{ items: Iter[Int] }` we need to design when the iterator advances relative to record destructuring. Not urgent.
7. **Display cap value (256).** Tunable. If users hit it routinely we make it configurable.
8. **Iterator captures in closures: relaxation path.** Disallowed in v1 (see §"Iterator values cannot be captured into closures"). Future relaxations, in increasing order of complexity: (a) deep-clone the captured walker before each FE invocation — easy but loses fusion across invocations; (b) introduce explicit `Iter`-aware semantics in the closure body, with documented advance-once-per-invocation rules. Not urgent; the v1 rule is a clean error message and `collect` is the workaround.
9. **Iterators as top-level parameters: relaxation path.** Disallowed in v1 (see §"Evaluation model and walker lifecycle", Invariant 3). Under Invariant 2 the semantics are already correct (each `parameter::eval` invocation clones the walker independently), so relaxation is just removing the rejection at the API/CLI binding layer. Defer until a concrete use case shows up — the v1 workaround is to pass an `Array[T]` and `collect`-skip via wiring.
10. **`Rc` vs `Arc` for `FromArray::items`.** Spec'd as `Arc` for forward-compatibility with multi-threaded evaluation. If `Walker` ends up not needing `Send`/`Sync`, swap to `Rc` — pure perf win, no API change.
