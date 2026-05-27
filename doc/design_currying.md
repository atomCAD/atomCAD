# Currying and Partial Application: Function Types Up To Currying

## Scope

This document designs **currying** — treating function types up to the
currying equivalence

```
(A, B, C) → D  ≡  (A) → ((B, C) → D)  ≡  (A, B) → ((C) → D)  ≡  (A) → (B) → (C) → D
```

— and adding **partial application** as the runtime operation that makes
this equivalence constructive. The motivating problem is that today's
closure substrate (`doc/design_closures.md`) treats `Function {
parameter_types: Vec<DataType>, output_type: DataType }` as a flat
*tuple-then-result* with a hard barrier between "where the argument ends
and where the result begins". As a consequence:

- `(Float, Float) → Int` cannot flow into a pin expecting `(Float) →
  ((Float) → Int)` even though the two are isomorphic.
- The `apply` node requires *every* argument pin to be wired; there is no way
  to call a 3-arg function on one or two args and get the remaining function
  back.
- The only authoring path for partial application is **manually nesting
  closure nodes** — for a 3-arg body, three frames deep, with capture wires
  threaded between each layer (see the screenshots in the originating
  discussion: a 3-arg `expr` body nested as `Float → (Float → (Float →
  Float))` is six nodes for what an uncurried `expr` writes in one).

Concretely, this branch adds:

1. **Currying-equivalent function-type conversion.** `can_be_converted_to`
   for `DataType::Function` flattens both sides before the structural
   same-arity check. `Function((A,), Function((B,), C))` and `Function((A,
   B), C)` become interchangeable wherever a function-typed pin is declared.
2. **`ZoneClosure.pre_supplied_args`.** A small additional `Arc<Vec<…>>`
   field on the runtime closure value, holding arguments already bound. The
   shared `run_closure_once` prepends them to the caller-supplied frame; the
   body's `ZoneInput { pin_index }` resolution is unchanged because positions
   line up.
3. **Partial application on `apply`.** The `apply` node derives its arg pins
   from the connected `f` source's flattened arity. The user wires a
   *contiguous prefix* of arg pins (`arg0…arg_{k-1}`) and leaves the rest
   unwired; the output pin's type follows: full arity (`k == n`) ⇒ `R`,
   partial (`k < n`) ⇒ `Function((remaining_params), R)`.
4. **Editor support for partial `apply`.** When `f` is wired, the `apply`
   shape panel becomes a read-only *derived display* of the wired source's
   flat function type, and the existing `ClosureShapeEditor` (kind picker)
   is hidden. When `f` is unwired, the kind picker returns as the
   intended-shape affordance. The `closure` node's kind picker is
   unchanged — it still authors a body shape.

In scope:
- Conceptual model and worked examples.
- `DataType::Function` conversion rule changes.
- `ZoneClosure` field addition and `run_closure_once` prepend.
- `apply` node rewrite (`calculate_custom_node_type`, `eval`, validation).
- Editor changes for the `apply` shape editor and pin-derivation flow.
- Implementation phases, each ending in `cargo test` green plus (Phase 4)
  `flutter run` working.

Out of scope:
- **HOF auto-partialization.** Letting `(T, X…) → Y` flow into `map.f: (T) →
  U` directly (with `map.output_type` inferred to `Function((X…,), Y)`)
  needs `map.output_type` to be derived from `f` rather than user-configured.
  That's a separate, additive change; for v1 users insert an `apply` to
  pre-curry before `map`. See Open Question 1.
- **Smarter shape inference when `f` is disconnected.** With nothing wired,
  `apply` still needs *some* default shape to render arg pins against. v1
  keeps the existing `ClosureKind` picker for this case (demoted to "shape
  hint when `f` is absent"); anything cleverer — auto-inferring from
  downstream consumers, or hiding the pins entirely until `f` is wired —
  is deferred. See Open Question 2.
- **0-arity functions / thunks.** The user mentioned "functions without
  arguments still being functions that need evaluation". Adding a 0-arity
  closure kind (or relaxing the "at least one arrow" assumption on closure
  authoring) is a small follow-up that does not need any of the substrate
  changes here. See Open Question 3.
- **Composition / `flip` combinators.** Deferred per `design_closures.md`
  open questions; this doc unblocks them by making partial application
  cheap, but does not deliver them.
- **`.cnnd` migration.** No version bump is required: `pre_supplied_args`
  is a runtime-only field (not serialized — see Phase 2 Gotchas), and the
  on-disk shape of `ApplyData` is unchanged. Existing closures load with
  empty `pre_supplied_args` and existing `apply` nodes load with `kind` /
  `type_args` exactly as today — both via `serde` defaults / verbatim
  load. The follow-up file-format work (deprecating `ApplyData.kind` once
  the derived shape lands fully) is filed as a polish task in §"Out of
  phase plan".

## Scope of this branch — build/test contract

The verification gate is `cd rust && cargo test` green and `cargo clippy`
clean for every Rust phase (1–3), plus `flutter run` launching a working
editor at the end of Phase 4. Existing closure behavior is preserved
byte-identically by every phase (empty `pre_supplied_args`, today's
same-arity conversion is a *special case* of the new rule after flattening).

| Must pass | When |
|---|---|
| `rust/` workspace (`cargo build`, `cargo clippy`, `cargo test`) | every phase |
| `flutter_rust_bridge_codegen generate` succeeds | every phase |
| Existing closure-/HOF-using tests, **byte-identical results** | Phase 1, 2, 3 |
| `flutter run` launches; partial `apply` authoring works end-to-end | Phase 4 |
| Existing non-closure editing still works | every phase (regression) |

The branch starts after the zones / closures / function-pins / migration
docs have all landed. Baseline: the full Rust suite (3400+ tests) green,
the four HOFs and `closure`/`apply` evaluating via the substrate in
`evaluator/zone_closure.rs`, `DataType::Function` carrying a flat
`parameter_types` + `output_type`.

## Motivation

The user's complaint, condensed: *"Float → Float → Float → Float, (Float,
Float) → (Float → Float), (Float, Float, Float) → Float — these are all the
same thing. Where the argument ends and where the result begins depends on
how many slots one fills when partially evaluating. With curried format being
the go-to default there is no way to do partial evaluations. And setting up
currying is bad UX ATM."*

Symptoms in the editor today:

- **The non-nested representation is unreachable from existing entry
  points.** Authoring an N-arg body as a single `expr` inside one `closure`
  produces a `Function((P0, …, P_{N-1}), R)` value. There is no way to
  partially apply it: `apply` requires all N pins wired, and no HOF accepts
  a multi-arg function on its `(T) → U` / `(T) → Bool` / `(A, T) → A` / `(T)
  → Unit` `f` pin.
- **The nested representation is brutal.** To express the same 3-arg body
  with partial-application potential, the user has to draw three closure
  frames deep, each capturing one arg of the next. Visual clutter scales
  linearly with arity; the user reports it as "no user would do that".
- **The asymmetry between authoring and consumption is forced by the type
  system.** Internally the substrate is already arity-agnostic — frames are
  `Vec<NetworkResult>`, `run_closure_once` pushes whatever the caller passes,
  `ZoneInput { pin_index }` resolves positionally. The barrier between
  parameters and result lives in `DataType::Function`'s structural
  same-arity rule (`data_type.rs:391-414`) and in `apply`'s requirement that
  every declared arg pin be wired (`apply.rs:131-143`).

The fix: remove the barrier in the type system, add a tiny runtime field to
carry pre-bound args, and let `apply` partially apply on the wired prefix.
The rest of the substrate stays unchanged.

## Concept

### Currying equivalence

Two function types are **currying-equivalent** iff their *flattened*
canonical forms are pairwise convertible. Flattening absorbs `Function`
return types into the parameter list:

```
flatten(Function((P0, …, P_{n-1}), Function((Q0, …, Q_{m-1}), R)))
    = flatten(Function((P0, …, P_{n-1}, Q0, …, Q_{m-1}), R))

flatten(Function((P0, …, P_{n-1}), R))   where R is not a Function
    = Function((P0, …, P_{n-1}), R)
```

After flattening, the return type is guaranteed non-`Function`. Comparison
is then today's "same arity, pairwise convertible parameters + return"
rule. So:

- `(A, B, C) → D` ≡ `(A) → ((B, C) → D)` ≡ `(A, B) → ((C) → D)` ≡ `(A) →
  (B) → (C) → D` — all flatten to `(A, B, C) → D`.
- `(A, B) → Int` and `(A) → ((B) → Int)` — both flatten to `(A, B) → Int`,
  so they unify.
- `(A, B) → Int` vs. `(A) → Int` — flatten to different arities, still
  rejected (no information loss).

This is a pure relaxation: today's same-arity-after-no-flattening is the
special case where neither side's return is a `Function`, which is the
common case and stays identical.

### `pre_supplied_args` and partial application

A `ZoneClosure` gains one field:

```rust
pub struct ZoneClosure {
    // ... existing fields ...
    pub pre_supplied_args: Arc<Vec<NetworkResult>>,  // default: Arc::new(Vec::new())
}
```

The semantics are: *when a consumer pushes its iteration frame, the actual
frame becomes `[pre_supplied_args… ++ caller_args…]`.* The body's
`ZoneInput { pin_index }` resolution is unchanged — pins are positional, the
frame is just longer than the caller's `args` vector. `param_types`
continues to be "the types of the slots a caller must fill" — i.e. the
*remaining*, unbound positions. A freshly-built closure (from
`build_inline_closure` or `build_node_function_closure`) has
`pre_supplied_args = empty`, `param_types = full` — identical to today.

**Partial application** is then the operation that, given a closure `C`
with `param_types = [P_0, …, P_{n-1}]` and `k` arg values `a_0, …, a_{k-1}`,
produces a new closure `C'`:

```
C'.pre_supplied_args = Arc::new(C.pre_supplied_args ++ [a_0, …, a_{k-1}])
C'.param_types       = C.param_types[k..].to_vec()
C'.body, captures, zone_output_wires, owner_node_id, return_type — unchanged
```

(`++` is concatenation, expressed in Rust as `let mut v =
(*C.pre_supplied_args).clone(); v.extend(new_args);`.) `C'` is exactly the
function `(P_k, …, P_{n-1}) → R` that `C` *evaluated on its first k
arguments* would have left behind. No body is rewritten; no captures are
recomputed; no new node is materialized. The cost is one `Vec` clone +
extend + one Arc allocation for the extended args vector — independent
of body size.

When `k == n` (the consumer supplied every remaining arg), there is no
"new closure" — we instead push the full frame and resolve the body, exactly
as today's `run_closure_once`. This is the path every existing HOF and
existing fully-wired `apply` already takes; the only difference is the
prepended `pre_supplied_args` (empty in the existing cases).

### `apply` semantics

`apply` is the operator that *chooses k* — how many of the function's
arguments are supplied at this call site. Today it implicitly fixes
`k = n` (every arg pin required). After this design:

- `apply` derives its argument pin shape from the wired `f`'s flattened
  function type. Number of arg pins = `f.flat.param_types.len()`.
- The user wires the **contiguous prefix** `arg0 … arg_{k-1}` and leaves
  `arg_k … arg_{n-1}` unwired. `k = number of wired arg pins from arg0`.
- Output pin type is **dynamic in k**:
  - `k == n` ⇒ `R` (full eval; equivalent to today's behavior).
  - `k < n` ⇒ `Function(params[k..], R)` (partial; emits a new
    `NetworkResult::Function`).
  - `k > n` is impossible by construction (we declared exactly n arg pins).
- Wiring `arg_j` while `arg_i` (i < j) is unwired is a **validation error**
  attributed to `apply`. Prefix-only is the rule. (Allowing arbitrary holes
  — "skip arg1, supply arg0 and arg2" — would require permutation
  bookkeeping and effectively undo currying's positional clarity; see Open
  Question 4.)

### Worked examples

**1. The user's screenshot, fixed.** A `closure` `g` of kind `Custom`
with `param_names = ["x", "y"]`, `type_args = [Float, Float, Int]`, body
`expr` evaluating `x * y` (the body's return type is `Int`, matching
`type_args[2]`). Its output type is `Function((Float, Float), Int)`. The
user wants to turn this into `Iter[Function((Float,), Int)]` by mapping
over an `Iter[Float]`. With the new substrate:

```
range(3)          --xs-->  map  --result-->  collect
                            ^
                         f = closure of shape (Float) → Function((Float,), Int)
                             body: apply(g, element)
                             (g captured from outer scope; element is the map's zone-input)
```

The `map`'s body holds an `apply` node with `f = g` (captured) and
`arg0 = element` (the map's zone-input). `apply` sees `g`'s flattened
arity = 2, `k = 1` wired → output type `Function((Float,), Int)`. The
walker yields one partially-applied function per `xs` element. No
nesting required.

Note `map.output_type` must still be **set by the user** to
`Function((Float,), Int)` (or an equivalent curried form) for the
top-level `map`'s output pin to type correctly — HOF auto-partialization
is out of scope (Open Question 1). The body wiring above produces a
correctly-typed `NetworkResult::Function` per element either way; this
is purely about the `map`'s declared output type.

**2. Direct partial chain.** Top-level: `g: (Float, Float, Float) →
Float` with body `expr` evaluating `x + y + z`. Wire `apply(g,
float(2.0))` — `n=3`, `k=1`, output `Function((Float, Float), Float)`.
Wire *that* into a second `apply` with `arg0 = float(3.0)` — output
`Function((Float,), Float)`. Wire *that* into a third `apply` with
`arg0 = float(4.0)` — output `Float`, evaluating to `9.0`.

The user's "right side" reference shape (the chain of three `apply`
nodes in the second screenshot) becomes the *flat* equivalent of their
nested left-side ladder, and is the substrate's *primary* way to
express partial application going forward.

**3. Existing fully-wired apply.** Unchanged. `n = k`,
`pre_supplied_args` is empty before and after — we go straight to the
full-frame `run_closure_once` and emit `R`.

## Data model

### `DataType::Function` — type is unchanged, conversion is curry-aware

The `FunctionType` struct stays as it is (`data_type.rs:8-11`):

```rust
pub struct FunctionType {
    pub parameter_types: Vec<DataType>,
    pub output_type: Box<DataType>,
}
```

A `FunctionType` can be stored either *flattened* (return ≠ `Function`) or
*nested* (return = `Function`). Both forms are valid storage; equality and
conversion are decided on the flattened forms. We deliberately do **not**
canonicalize on construction — storing a user's authored
`Function((A,), Function((B,), C))` verbatim preserves the visual shape they
chose to write. The cheap flattening happens lazily at comparison sites.

### `ZoneClosure` — one new field

```rust
pub struct ZoneClosure {
    pub body: Arc<NodeNetwork>,
    pub captures: Arc<HashMap<CaptureKey, NetworkResult>>,
    pub zone_output_wires: Arc<Vec<IncomingWire>>,
    pub owner_node_id: u64,
    pub param_types: Vec<DataType>,
    pub return_type: DataType,
    /// Args already bound by partial application. Prepended to the
    /// caller-supplied frame inside `run_closure_once`. Default empty.
    /// `Arc`-shared so cloning a partially-applied closure (Walker
    /// Invariant 2) stays a refcount bump.
    pub pre_supplied_args: Arc<Vec<NetworkResult>>,
}
```

`ZoneClosure::function_type()` continues to report `FunctionType {
parameter_types: self.param_types.clone(), output_type: …
self.return_type.clone() }` — i.e. the type of the *remaining* function.
That is the type the consumer wires against, which is what the type system
needs to see.

Cloning cost: an empty `Arc<Vec<_>>` shares one allocation across all
contexts (or use `Arc::new(Vec::new())` per construction site — both are
fine for v1; the EMPTY_ARGS `LazyLock` micro-optimization is a follow-up
only if profiling shows it). Partial-application clones bump the refcount
of the extended args vec; no deep copy.

### `ApplyData` — stored shape unchanged; `kind` becomes a disconnected-`f` fallback

```rust
pub struct ApplyData {
    pub kind: ClosureKind,
    pub type_args: Vec<DataType>,
    #[serde(default)]
    pub param_names: Vec<String>,
}
```

Unchanged on the data model side. The semantic role shifts:

- **`f` connected** (the new and increasingly common case): the wired
  source's flattened function type drives `calculate_custom_node_type`'s
  arg-pin enumeration and the output-pin type. `ApplyData.kind` /
  `type_args` are ignored at evaluation, and the editor de-emphasizes the
  kind picker.
- **`f` disconnected**: the kind picker still drives the default arg
  layout, exactly as today — `f` must be connected before evaluation can
  produce anything, so this is a "what shape does the user *intend*"
  affordance, not an evaluation input.

The stored data is therefore forward-compatible: every existing `apply`
node loads as today, and any change is purely additive (the new "derived
shape from connected `f`" view kicks in once a wire lands).

### Editor view (`APIApplyView`, surfaced via `NodeView`)

The Flutter side already receives `APIApplyData` (the `kind`/`type_args`/
`param_names` stored shape). Phase 4 adds an `APIApplyView` field on the
node view that carries the **resolved** shape — what the editor should
actually render:

```rust
pub struct APIApplyView {
    /// Arity of the resolved function (n in §"apply semantics").
    pub arity: usize,
    /// Per-arg display name (from the closure source's pin names when
    /// available, falling back to "arg0", "arg1", …).
    pub arg_names: Vec<String>,
    pub param_types: Vec<DataType>,
    pub return_type: DataType,
    /// True if the resolved shape came from a connected `f` source; false
    /// if we fell back to the kind picker.
    pub from_wired_f: bool,
}
```

The "what shape am I actually showing" is computed Rust-side, both because
the wire-source's function type needs `NodeTypeRegistry::resolve_output_type`
and because the same logic drives `calculate_custom_node_type` (the source
of truth for pin layout). The Flutter side reads this view and renders.

## Evaluator changes

### Reuse from today's substrate (unchanged)

| Existing piece | Reused? |
|---|---|
| `build_inline_closure`, `build_node_function_closure` | Yes — emit closures with empty `pre_supplied_args` |
| `obtain_closure` | Yes — the choice of inline-body vs. wired-`f` is orthogonal to currying |
| `run_closure_once` | Yes, with one tiny prepend (next section) |
| `Walker::MapZone`/`FilterZone::next` | Yes — they call `run_closure_once`; the prepend is inside that helper |
| Eager `fold`/`foreach` drain loops | Yes — same path |
| `evaluate_arg` / `evaluate_arg_required` | Yes |
| `CapturesGuard` and the `current_zone_input_values` scope-stack | Yes |
| Per-node `eval` implementations | Yes |

### `run_closure_once` — prepend `pre_supplied_args`

The only substrate change is one line in `zone_closure::run_closure_once`:

```rust
// Today:
//   context.push_zone_input_frame(closure.owner_node_id, args);
//
// After:
let mut frame = Vec::with_capacity(closure.pre_supplied_args.len() + args.len());
frame.extend(closure.pre_supplied_args.iter().cloned());
frame.extend(args);
context.push_zone_input_frame(closure.owner_node_id, frame);
```

For an empty `pre_supplied_args` (every callsite that exists today), this
is one extra `Vec::with_capacity(args.len())` and two zero-length `extend`s
folded into the existing single `extend(args)` — measurable cost: nil. The
clone-per-element of `pre_supplied_args` is `NetworkResult::clone` per
pre-bound argument; for the realistic case (1–3 partially-applied args,
each a primitive or an `Arc`-backed structure value) this is negligible.

If profiling later shows the per-iteration clone is a real cost for
large-payload pre-bound args, a `Cow`-style "share if read-only" path is
straightforward but unnecessary for v1.

### `apply.eval` — the partial branch

```rust
fn eval(...) -> EvalOutput {
    // 1. Resolve f. (Errors and "not a function" cases match today's apply.)
    let f_closure: ZoneClosure = match evaluator.evaluate_arg(.., f_idx) {
        NetworkResult::Function(zc) => zc,
        NetworkResult::None => return EvalOutput::single(
            NetworkResult::Error("apply: f not connected".into())),
        e @ NetworkResult::Error(_) => return EvalOutput::single(e),
        other => return EvalOutput::single(
            NetworkResult::Error(format!("apply: f is not a function (got {})",
                                          other.to_display_string()))),
    };

    // 2. n = remaining-arity of the (already-curry-aware) closure value.
    //    `param_types` is the unbound tail — pre-bound args are already
    //    accounted for by the producing apply, so no further flattening
    //    is required here.
    let n = f_closure.param_types.len();
    let k = count_wired_arg_prefix(self, node);  // contiguous prefix; arg pins are 1..1+n

    // 3. Resolve the k wired arg pins, short-circuiting on the first error.
    let mut new_args = Vec::with_capacity(k);
    for i in 0..k {
        match evaluator.evaluate_arg_required(.., 1 + i) {
            v @ NetworkResult::Error(_) => return EvalOutput::single(v),
            v => new_args.push(v),
        }
    }

    // 4. Dispatch on (k vs n).
    if k == n {
        // Full eval — identical to today's path (with empty
        // pre_supplied_args this collapses to the existing code).
        let mut inner_ctx = context.fresh_inner_for_eager_body();
        let r = run_closure_once(.., &mut inner_ctx, &f_closure, new_args);
        context.drain_inner_context(inner_ctx);
        EvalOutput::single(r)
    } else {
        // Partial — build a new closure with extended pre_supplied_args
        // and sliced param_types. body / captures / zone_output_wires /
        // owner_node_id / return_type are inherited unchanged via Arc share.
        let mut extended = (*f_closure.pre_supplied_args).clone();
        extended.extend(new_args);
        let partial = ZoneClosure {
            body: Arc::clone(&f_closure.body),
            captures: Arc::clone(&f_closure.captures),
            zone_output_wires: Arc::clone(&f_closure.zone_output_wires),
            owner_node_id: f_closure.owner_node_id,
            param_types: f_closure.param_types[k..].to_vec(),
            return_type: f_closure.return_type.clone(),
            pre_supplied_args: Arc::new(extended),
        };
        EvalOutput::single(NetworkResult::Function(partial))
    }
}
```

A subtle point worth pinning down: step 2 reads `n` from
`f_closure.param_types.len()` *without* re-flattening the function type.
This is correct because `param_types` is already the "unbound tail" — if
`f` came from another partial application, `param_types` has already
been sliced once at that site. Flattening would only matter if the
*upstream wire's type system* needed to accept a nested-return source
for an arg pin declared with a flattened-out target, and that's handled
by `can_be_converted_to`'s Function arm (§"Type system"), not here.

Note that the body's `ZoneInput { pin_index }` references **continue to use
the original pin indices** even after a partial. The frame `[pre_supplied…
++ new_args…]` has the same total length and the same positional meaning as
a single full-arity call would have; the body simply doesn't know whether
its caller supplied arg0 "long ago" via a chain of partials or "just now".
This is the same invariant the existing substrate maintains: positional
parameter passing through a global scope-stack keyed by `owner_node_id`.

### Edge cases

- **Nested partials.** `apply(apply(g, x), y)` where `g: (A, B, C) → R`.
  The inner `apply` sees `n=3, k=1`, emits a `NetworkResult::Function(C')`
  with `C'.pre_supplied_args = [x]` and `C'.param_types = [B, C]`. The
  outer `apply` sees `C'`'s remaining arity = 2, resolves `k=1`, builds
  `C''` with `pre_supplied_args = [x, y]` and `param_types = [C]`. Each
  level extends the args vector by one entry; no body duplication, no
  capture re-evaluation.
- **Partials of a 1-arg function.** `g: (A) → R`, `apply(g, a)`.
  `n=1`, `k=1`, full-eval branch. The partial branch is never taken —
  same code path as today.
- **`k = 0` (the identity case).** An `apply` with `f` wired but no arg
  pins wired hits `n > 0, k = 0`. The partial branch builds a `C'`
  whose `pre_supplied_args` is unchanged (no new args) and whose
  `param_types` is unchanged (no slicing) — `C'` is semantically equal
  to `f_closure`. Allowed by the rule (it's a no-op pass-through of a
  function value), but not useful in practice. An eager short-circuit
  to "return `f_closure` unchanged" is possible but not worth the
  branch; the redundant `Arc::new(Vec::new())` allocation costs
  nanoseconds.
- **`fold`-shaped function (`(A, T) → A`) wired into `map.f: (T) → U`.**
  Rejected directly: both sides are already flat, arity 2 vs. 1 —
  same-arity check fails after flattening. The user inserts one `apply`
  inside `map`'s body to supply the `A` arg, getting `(T) → A`, then
  feeds that result into the body's downstream computation. This is the
  v1 ergonomic floor; lifting it (so a 2-arg function wires straight
  into `map.f` and `map.output_type` is auto-inferred) needs HOF
  auto-partialization (Open Question 1).

## Type system

The single change is in `data_type.rs::can_be_converted_to`'s Function arm
(currently `data_type.rs:391-414`):

```rust
if let (DataType::Function(src), DataType::Function(dst)) = (source_type, dest_type) {
    let src_flat = flatten_function_type(src);
    let dst_flat = flatten_function_type(dst);
    if src_flat.parameter_types.len() != dst_flat.parameter_types.len() {
        return false;
    }
    if !DataType::can_be_converted_to(&src_flat.output_type, &dst_flat.output_type, registry) {
        return false;
    }
    for (s, d) in src_flat.parameter_types.iter().zip(dst_flat.parameter_types.iter()) {
        if !DataType::can_be_converted_to(s, d, registry) {
            return false;
        }
    }
    return true;
}
```

`flatten_function_type` is a small helper, side-effect-free, returning a
fresh `FunctionType` whose `output_type` is guaranteed not to be a
`Function`. Its recursion depth is the depth to which `Function` returns
are nested inside the input type — in realistic networks, 1–3 levels
(the user's screenshot shows two). Unrelated to network nesting depth;
flattening a single `FunctionType` value never crosses a node boundary.

The **strict-no-broadcast** variant (`can_be_converted_to_strict_no_broadcast`
in `data_type.rs`, used by the drag-aware add-node popup) gets the same
flattening treatment in its Function arm. No other type-system code
needs to change.

### Effect on existing wires

- Every wire that exists today has source and destination types whose
  flattened forms are *identical to themselves* (no nested `Function`
  returns appear in any built-in pin signature). So flattening is the
  identity for them, and the same-arity rule fires identically. **No
  existing wires change validity.** Phase 1 should add a regression test
  that this holds across the existing fixture set.
- New wires that *do* exercise the relaxation (a `closure` of kind
  `Custom` producing nested returns, or a partial `apply` emitting one)
  start working from Phase 1 onward.

## Validation

Three small additions to `network_validator.rs`:

1. **Prefix-only wiring on `apply`.** If `apply.arguments[1+j]` is wired
   while `apply.arguments[1+i]` for some `i < j` is unwired, attach an
   error to the `apply` node ("argument pins must be wired as a contiguous
   prefix").
2. **`apply`-arity-vs-`f` consistency.** When `f` is wired and resolved,
   `apply`'s declared arg-pin count equals the flattened arity of `f`'s
   function type. The drift case is repaired by `repair_node_network`
   following the existing "input type changes ⇒ disconnect now-incompatible
   wires" pattern (`doc/design_zones.md` §"Repair") — when `f`'s source's
   function type changes, the validator runs, `apply`'s
   `calculate_custom_node_type` recomputes the arg-pin layout, and stale
   wires beyond the new arity are dropped.
3. **`apply` with `f` disconnected.** Today this is an immediate
   validation error ("apply: f not connected"). After this design, it
   stays an error — `apply` always requires `f`. Partial application is
   not "make `f` optional"; it's "make some of the arg pins optional".

The existing "every required input pin must be wired" rule is **relaxed
selectively for `apply`'s arg pins**: only pin 0 (`f`) remains required.
The arg pins are marked optional in `get_parameter_metadata`
(`apply.rs:175-184`), and a count of wired arg pins drives the output
type and partial/full dispatch. This is the only departure from the
"required = required" convention; it is localized to `apply`.

## Editor (Flutter) changes

The Flutter changes are small because the body-rendering, wire-drawing,
and pin-position machinery is already generic. The new work is concentrated
in the `apply` node's property panel and the dynamic typing of its output
pin:

1. **`apply` shape panel re-skin.** When `f` is connected, hide the kind
   dropdown and show a read-only summary instead: *"f: (A, B, C) → R —
   derived from the wired source."* The summary shows per-arg pin labels
   (sourced from the wired `closure`'s authored `param_names` when
   available, else `arg0 … arg_{n-1}`). When `f` is disconnected, the
   existing `ClosureShapeEditor` (shared with the `closure` node) returns
   as the affordance for declaring the intended shape.
2. **`APIApplyView`** plumbed through `NodeView`. The Flutter widget reads
   `arity`, `arg_names`, `param_types`, `return_type`, `from_wired_f` and
   renders accordingly.
3. **Output pin typing.** The output pin's data-type indicator (color, hover
   label) reflects either `R` (full) or `Function((remaining,), R)`
   (partial). Today the output type is a `Fixed(ret)` value computed in
   `apply.rs`'s `calculate_custom_node_type`; after this change it
   becomes the resolved current-state type, recomputed when wiring
   changes — same lifecycle as `record_destructure`'s schema-driven pin
   types.
4. **Per-arg-pin "unconnected = partial" affordance.** The arg pins
   render with a different default-value placeholder than today (no
   "default literal" hint, since there isn't one — an unwired arg
   *defers* the application rather than supplying a default). A small
   tooltip on each arg pin can read *"unwired ⇒ part of the resulting
   function's parameter list"*.
5. **No changes to wire creation or drag.** A `Function` value is still a
   normal typed wire. The currying-equivalent conversion rule kicks in
   inside `can_connect_nodes`'s existing pin-compat check; no Flutter-side
   code change needed.
6. **No changes to `closure`.** The `closure` node's kind picker keeps its
   role as *body shape author*. Closures of kind `Custom` already exist
   (see `nodes/closure.rs`), so an N-arg closure with `param_names = ["x",
   "y", "z"]` and a single `expr` body is already authorable.

## Reuse map (summary)

**Reused unchanged:**
- `Walker::MapZone` / `FilterZone` — the walker carries a `ZoneClosure`,
  the `pre_supplied_args` field travels with it via Arc-share.
- `build_inline_closure`, `build_node_function_closure`, `obtain_closure`.
- Every per-node `eval` implementation except `apply`.
- `CapturesGuard`, `current_zone_input_values` scope-stack, `eval_step`.
- The `closure` node and its `ClosureKind` (including `Custom`).
- The four HOFs' `f` pin dispatch (`obtain_closure`).
- `repair_node_network` machinery.

**Reused with small extensions:**
- `ZoneClosure` — one new field, `pre_supplied_args: Arc<Vec<NetworkResult>>`.
- `run_closure_once` — prepends `pre_supplied_args` to the caller's args.
- `DataType::can_be_converted_to` Function arm — flatten both sides, then
  same-arity-pairwise as today.
- `can_be_converted_to_strict_no_broadcast` Function arm — same flattening
  treatment, kept symmetric.
- `apply.eval` — the partial/full branch.
- `apply.calculate_custom_node_type` — derive arg-pin layout from the wired
  `f`'s flattened type when present, fall back to the kind picker.
- `apply.get_parameter_metadata` — arg pins become optional (only `f`
  required).
- Validator — three new checks (prefix-only, arity consistency, `apply`
  `f` required).

**New from scratch:**
- `flatten_function_type(&FunctionType) -> FunctionType` helper.
- `APIApplyView` API type + Flutter widget glue.
- One new property-panel skin for `apply` when `f` is connected.

**Deleted / removed:**
- Nothing. `ApplyData.kind` is kept as a fallback for the disconnected `f`
  case. Its eventual removal (once the editor exclusively renders the
  derived shape) is a polish follow-up and is **not** part of this
  branch.

## Implementation phases

Each phase ends with `cd rust && cargo test` green plus `cargo clippy`
clean; Phase 4 additionally ends with `flutter run` launching a working
editor. Phases are strictly sequential.

### Phase 1: Curry-equivalent function-type conversion

**Goal.** Flatten function types before the structural-equality check.
Every existing wire continues to validate; new wires using
currying-equivalent shapes start working. No `apply` or `ZoneClosure`
change yet.

**Scope.**
- `data_type.rs` — add `flatten_function_type` helper. Update the
  Function arm of `can_be_converted_to` (and
  `can_be_converted_to_strict_no_broadcast`) to flatten both sides first,
  then run today's same-arity-pairwise check.
- No node changes. No editor changes.

**Tests.** New unit tests in `rust/tests/structure_designer/`:
- `flatten_function_type` golden cases (nested `Function` in return →
  flat; already-flat → identity; mixed-depth nested).
- `can_be_converted_to` curry-equivalence: `(A) → ((B,) → C)` ↔ `(A, B)
  → C`, both directions; with leaf conversions on params (`Int → Float`)
  and return; rejection on arity mismatch after flattening.
- A regression test that every existing fixture continues to validate
  (the full fixture set under `rust/tests/fixtures/`).

**Gotchas.**
- `Box<DataType>` recursion. `flatten` builds a fresh `FunctionType` —
  cloning leaf types only; param_types Vecs are extended once at the
  outermost call. No reference cycles to worry about.
- The strict-no-broadcast variant must get the same flattening
  treatment, or the drag-aware add-node popup will silently mis-filter
  currying-equivalent candidates.

### Phase 2: `ZoneClosure.pre_supplied_args` — substrate-only

**Goal.** Add the field, plumb it through `run_closure_once`, ensure
every closure constructor emits an empty value. No node yet *produces* a
non-empty `pre_supplied_args`, so all existing tests pass byte-identically.

**Scope.**
- `evaluator/zone_closure.rs` — add the field to `ZoneClosure`. Update
  `build_inline_closure` and `build_node_function_closure` to set
  `pre_supplied_args: Arc::new(Vec::new())`. Update `run_closure_once`'s
  push to prepend.
- Update any `ZoneClosure { … }` struct literals in tests.

**Tests.** No new tests; the existing closure / HOF suite is the
regression check. Add one unit test that hand-constructs a `ZoneClosure`
with non-empty `pre_supplied_args` and verifies `run_closure_once`
prepends correctly (against a synthetic two-param body).

**Gotchas.**
- The Walker `Clone` independence invariant (Invariant 2 in
  `evaluator/AGENTS.md`): `pre_supplied_args` is `Arc<Vec<…>>`, so cloning
  a walker that embeds a partially-applied closure stays a refcount bump
  — the invariant continues to hold.
- Serialization: `pre_supplied_args` is a runtime-only field
  (`NetworkResult` is not `Serialize`); `ZoneClosure` is also not
  serialized today (the body is — through `Node.zone` — and captures are
  re-evaluated on load). Confirm by grep that no `Serialize` impl exists
  for `ZoneClosure`; if it does, add `#[serde(skip)]`.

### Phase 3: `apply` becomes partial-application capable

**Goal.** The minimum viable partial-`apply`: derived arg-pin shape from
connected `f`, dynamic output pin type, the partial/full branch in `eval`.

**Scope.**
- `nodes/apply.rs`:
  - `calculate_custom_node_type` — when the `f` source's function type
    is resolvable, derive `params`/`ret` from the flattened wired source
    instead of from `ApplyData.kind`. Fall back to the kind picker when
    `f` is disconnected.
  - `eval` — implement the partial/full dispatch in the prose above.
  - `get_parameter_metadata` — mark arg pins optional.
- `network_validator.rs` — three checks per §"Validation".
- `node_type_registry.rs::repair_node_network` — extend the wire-retention
  filter to handle "`f` source's function type changed ⇒ refresh
  `apply`'s arg pin layout, disconnect stale wires past the new arity".

**Tests.** New file `rust/tests/structure_designer/currying_test.rs`:
- **Identity partial:** `apply(g, …)` on a 1-arg `g` with `k=0` wired
  yields a `Function` semantically equal to `g`. Round-trip through
  another `apply` with `k=1` ⇒ full result.
- **One-arg partial:** 3-arg `g`, partial with `k=1` yields a 2-arg
  function. Wire into a second `apply` with `k=2` ⇒ full result. Compare
  to a fully-wired single `apply`.
- **Currying-equivalent acceptance:** an `apply` declaring `(A,B,C) → D`
  accepts a source whose type is `(A) → ((B,C) → D)`, and vice versa.
  Evaluation matches.
- **The user's screenshot scenario:** a `Custom` closure producing
  `(Float, Float) → Int` flows into a `map`'s body via an `apply(g,
  element)` arrangement (per §"Worked examples" example 1). The test
  fixture explicitly sets `map.output_type =
  Function((Float,), Int)` — HOF auto-partialization is out of scope, so
  the user (or the test author) must declare it. `collect` yields the
  expected list of partially-applied functions; the test asserts
  correctness by then applying each to a fixed second arg in a second
  `map` pass and comparing to a hand-computed reference.
- **Validation:** prefix-only rule rejects "arg0 unwired, arg1 wired";
  `f` disconnected still errors; arity-drift after a kind change is
  repaired by `repair_node_network`.
- **Walker clone independence:** a partially-applied closure flowing
  through a `map → collect`-and-`map → collect` fanout produces
  independent walkers.

**Gotchas.**
- **Resolving `f`'s function type at `calculate_custom_node_type` time.**
  `calculate_custom_node_type` runs during pin-layout updates and does not
  have access to the full evaluator. It can, however, read the wired
  source's declared output type via the registry — the same path
  `record_destructure` uses to derive its per-field output pins from the
  connected schema. Mirror that pattern.
- **`ApplyData.kind` becomes vestigial when `f` is connected.** Until the
  follow-up that removes it, the editor must take care not to *show* the
  kind picker when `f` is wired (it would suggest the kind is editable,
  which it isn't — the wired source dictates the shape). Phase 4
  Flutter work handles the visual side.
- **Empty `pre_supplied_args` after `k = 0` partial.** This is the
  identity case. The eval path still allocates a fresh
  `Arc::new(Vec::new())` for the "extended" args; that's two allocations
  per identity partial. If it ever shows up in profiling, share a single
  `EMPTY_ARGS` static. Almost certainly not worth it for v1.

### Phase 4: Editor (Flutter) — surface partial `apply` in the UI

**Goal.** Author and use partial `apply` end-to-end in the editor. The
shape panel reflects connected `f`; the output pin shows the dynamic
type; arg pins indicate that "unwired ⇒ part of the result".

**Scope.**
- `APIApplyView` — new API type carrying the resolved arity, arg names,
  param types, return type, and a `from_wired_f` flag. Populated by
  `build_node_view` for `apply` nodes; FRB-regenerated for Flutter.
- Flutter `apply` widget — read `APIApplyView`, render the read-only
  derived summary when `from_wired_f`, render the existing
  ClosureShapeEditor when not.
- Pin colors / tooltips — output pin shows dynamic type; arg pins gain a
  short "unwired ⇒ deferred" tooltip.
- `flutter analyze` clean.

**Tests.** Manual walkthrough:
1. Place a `closure` of kind `Custom`, `param_names = ["x","y"]`,
   `type_args = [Float, Float, Float]`, body `expr: x+y`.
2. Place an `apply` and wire the closure into `f`. Observe the arg pins
   become two `Float`s, output pin is `Float`.
3. Wire only `arg0` to a `float` literal. Output pin re-types to
   `Function((Float,), Float)`. Wire that output into another `apply`,
   confirm shape derives correctly, wire `arg0` on the second apply,
   confirm result is the expected sum.
4. Disconnect `f` on the original apply — kind picker returns,
   declaring the intended shape works as before.
5. Regression: existing closure / HOF networks load and evaluate
   unchanged.

**Verification.** `cd rust && cargo test` green; `flutter run` launches;
manual walkthrough passes.

**Gotchas.**
- **Showing the wired closure's pin names.** The closure source (often a
  `closure` node) has authored `param_names`. The `apply` view should
  propagate them so the user sees `arg0 = x`, `arg1 = y` rather than
  generic `arg0`, `arg1`. Where the source is a function pin
  (`output_pin_index == -1`) or a subnetwork's `Function` output, fall
  back to the node-type's parameter names.
- **Re-render trigger on wiring change.** `calculate_custom_node_type`
  re-runs when input wires change; the existing change-propagation
  pipeline already covers this, but verify the `apply` widget's view
  updates without an explicit user click.

### Out of phase plan (deferred)

- **HOF auto-partialization.** Let `(T, X…) → Y` flow into `map.f: (T)
  → U` by inferring `U = Function((X…,), Y)` from the wired `f`. Needs
  `map`'s `output_type` to be derived from `f` rather than user-set.
  Additive; substrate already supports it via Phase 2. See Open
  Question 1.
- **`ApplyData.kind` deprecation.** Once the editor exclusively renders
  the derived shape and the kind picker becomes a niche
  default-shape-hint, retire the field entirely. Needs a one-version
  serialization migration.
- **0-arity closures.** A `() → R` kind, or a `Custom` with `param_names
  = []`. Today the substrate already supports it (`run_closure_once`
  with `args = vec![]`); the editor and `ClosureShapeEditor` need a
  tiny relaxation. See Open Question 3.
- **`compose` / `flip` and other combinators.** Per
  `design_closures.md`'s deferred list; partial application makes these
  much easier to express but doesn't deliver them.

## Open questions

1. **HOF auto-partialization.** Should `map.f: (T) → U` accept any `(T,
   X…) → Y` source, with `map.output_type` inferred to `Function((X…,),
   Y)`? Mechanically the substrate supports it as of Phase 2; the type
   system already accepts the conversion as of Phase 1. The hold-up is
   `map`/`filter`/`fold`/`foreach`'s `output_type` (and equivalents)
   being user-configured properties today, not derived from `f`. Lifting
   this is additive: change each HOF's `calculate_custom_node_type` to
   derive `output_type` from `f` when present, fall back to the stored
   property when `f` is disconnected. Defer until users hit it; the
   `apply`-in-body workaround is one node, not a tower.
2. **Disconnected-`f` shape on `apply`.** The kind picker is the v1
   fallback, which is workable but slightly awkward — the user sees a
   picker that "doesn't really matter once you wire `f`". Alternative:
   show no pins at all until `f` is wired (apply is "empty" without
   `f`). Decide during Phase 4 UX work.
3. **0-arity closures.** The user's offhand remark: *"functions without
   arguments still being functions that need evaluation. Does the custom
   type really need to enforce at least one arrow?"* No — the substrate
   doesn't need it. The closure-shape editor's `Custom` kind currently
   accepts arbitrary arity, but verify it accepts arity 0 too; if not,
   relax it. Mostly a UX consistency item, not a substrate change.
4. **Non-prefix wiring on `apply`.** Allow wiring `arg0` and `arg2`
   while `arg1` is empty? The result would be `Function((P_1,), R)` with
   `P_0 = a_0`, `P_2 = a_2`, `P_1` left to the caller. This requires
   tracking a "supplied mask" rather than just `k`, and re-indexing
   `ZoneInput` references — neither expensive but a real conceptual
   addition. v1 stays with prefix-only; revisit if users actually want
   the freedom.
5. **`pre_supplied_args` deep-clone cost.** For large payloads (a
   `Crystal` or `Molecule` value pre-bound into a closure), the
   per-iteration `NetworkResult::clone` could matter. The payloads are
   already `Arc`-backed (`CrystalData`'s `atoms`, `geo_tree_root`), so
   clones are refcount bumps in practice — but a `Cow`-style "share
   read-only" path inside `run_closure_once` would be a clean
   optimization if profiling ever shows it.
6. **Cycle detection.** `Function`-typed values can already flow into
   captures, into `pre_supplied_args`, and through `ZoneInput` reads to
   downstream `apply` consumers — and today's evaluator does no cycle
   detection across that flow. This design adds one new value path
   (`pre_supplied_args`) but does not introduce a fundamentally new
   cycle vector: a `NetworkResult::Function` carried as a pre-bound arg
   is structurally identical to one carried as a capture. If a future
   recursion / fixed-point story for closures lands, cycle detection
   should be designed there, not here. Defer.

## Phasing summary

| Phase | Outcome |
|---|---|
| 1 | Curry-equivalent `Function` conversion (no node behavior change) |
| 2 | `ZoneClosure.pre_supplied_args` substrate (no node yet produces a non-empty value) |
| 3 | `apply` partial-application: derived pins + dynamic output type + partial/full eval branch |
| 4 | Editor surface: `APIApplyView`, derived-shape rendering, arg-pin "deferred" affordance |

Each phase's exit gate is the same as elsewhere in the project: `cd rust
&& cargo test` green plus (Phase 4) `flutter run` launching a working
editor. The user-visible payoff lands fully at Phase 3; Phase 4 is the
ergonomic polish that makes it the obvious tool.
