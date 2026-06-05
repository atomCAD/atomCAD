# Nullary Function Coercion: `() -> T` → `T`

## Context

Resolves issue #327 ("make functions with no arguments and values behave
exactly equivalently everywhere"). A zero-parameter `closure` node — or any
node viewed as a function whose every input is captured — produces a
`NetworkResult::Function` of type `() -> T`. Today that value can only be
consumed by a function-shaped pin (`apply.f`, an HOF `f` pin); to feed it into
a plain `T`-typed input the user must insert an `apply` node with no arguments.
The ceremony is the complaint.

This doc adds a one-directional coercion that lets a `() -> T` source flow
directly into a `T`-typed input pin, applying the closure at the consuming pin.

Builds on: `doc/design_closures.md` (the `ZoneClosure` value + `run_closure_once`),
`doc/design_currying.md` (canonical-flat `FunctionType`),
`doc/design_function_pin_unification.md` (`AnyFunction`, function-vs-value pins).

## Philosophy (why a coercion, not a type identity)

In a pure, total setting `() -> T` and `T` are **canonically isomorphic** —
maps from the terminal object are exactly the points of `T` (`Hom(1, T) ≅ T`,
i.e. the exponential `T^1 ≅ T`). Mathematics says "collapse them."

But isomorphic is not equal, and identifying two canonically-iso types is a
*design choice*, not a forced truth. In any language that is **not** pure-and-
total — and this graph has effect nodes (`export_xyz`, `print`, `foreach`),
potential non-termination, and a notion of evaluation cost/sharing — a
suspended computation is genuinely different from a value: *when* it runs, *how
many times*, and *with what effects* are all observable. (This is the
value-vs-computation stratification that Call-By-Push-Value makes foundational.)

The function type therefore strictly dominates `T`: it carries everything `T`
does plus delay/re-runnability. So we **keep the types distinct** and expose
the isomorphism's *forcing* direction (`f ↦ f()`) as a one-way coercion. We do
**not** declare type equality, and we do **not** add the reverse
auto-suspension (`t ↦ λ_. t`) — that direction throws away the information the
distinction exists to preserve.

## Concept

A new directed widening in `DataType::can_be_converted_to`:

> `Function([], T)` is convertible to `U` whenever `T` is convertible to `U`,
> for any **non-function** destination `U`.

Plus the runtime twin: when a `() -> T` value crosses such a wire, the closure
is run once with an empty argument frame and the produced `T` is then converted
to `U` as usual.

`() -> T → () -> T` stays plain identity (handled by the existing
function-vs-function arm), so `apply.f` / `map.f` / HOF `f` pins still receive
an actual function value. The coercion only fires when the destination is *not*
function-shaped.

## Decisions

### D1 — Top-level only (no recursion through *source* collections), for v1

Nullary forcing fires only when the **source** value at the top-level pin is a
`() -> T`. It does **not** fire at nested element / parameter / record-field
positions: an *array of* nullary functions `[() -> T]` does **not** coerce to
`[T]`, and `{f: () -> T}` does **not** coerce to `{f: T}`.

Rationale: the runtime application lives at the input-pin chokepoint
(`evaluate_arg`), which forces only the top-level (or one-Array-level)
value, not arbitrarily-nested thunks. If the static rule recursed into source
collections but the runtime hook did not, the validator would accept wires the
evaluator can't honor — a type-lie. Recursive coercion (running every nested
thunk during element/field conversion) is a deliberate non-goal here.

**Implementation.** The rule is enforced with a `top_level: bool` flag threaded
through `can_be_converted_to`: the public entry calls
`can_be_converted_to_impl(.., true)`, and **every** internal recursive call
passes `false`. The nullary arm is gated on `top_level`, so it can never be
reached through the `Array → Array` element-wise arm, the function-param arms,
etc. Every wire gate (`can_connect_nodes`, the validator, repair) routes through
the public entry, so they all get the rule consistently — there is no risk of a
repair site silently dropping a wire the connect gate accepted.

**Composition with scalar broadcast is allowed (and consistent).** A *scalar*
`() -> T` source flowing into a collection pin (`[T]`, `Iter[T]`) **is** accepted:
the top-level nullary arm forces `() -> T → T`, then the ordinary
single-element broadcast wraps it (`T → [T]`). This is still "top-level" — the
source is a top-level function, not a nested one — and the runtime honors it by
peeking **one** `Array` level in `force_nullary_arg` (so it forces each `() -> T`
wire of an `[T]` pin before the merge, while leaving the functions untouched for
an `[() -> T]` pin).

### D2 — Arity-0 functions always have a non-function output (invariant, not luck)

`FunctionType::new` absorbs nested `Function` returns into the parameter list,
so `() -> (A -> B)` canonicalizes to `(A) -> B`. Consequently a value of type
`Function([], T)` **always** has a non-function `T`: an arity-0 function whose
result is itself a function cannot exist in canonical form. This makes the
coercion arm total and unambiguous (no "which arrow do I peel" question) and is
the reason D1's "`U` non-function" guard fully characterizes the source.

Do not "fix" the absorbing behavior of `FunctionType::new` without revisiting
this arm.

### D3 — Re-run / effect semantics: apply at the consuming pin, per consumer

The closure runs at the pin that consumes it, via
`run_closure_once(evaluator, network_stack, registry, context, &closure, vec![])`.
Implications, all intended for v1:

- **Fan-out runs it N times.** The evaluator does not memoize pin results, so a
  `() -> T` wired to N value inputs runs N times in a pass. This matches the
  philosophy (a thunk is a computation, not a shared value) and matches how
  every other fanned-out pin already behaves. Memoization is a separate concern
  and explicitly **not** delivered here (issue #327's use-case 4 is decoupled).
- **Display passes force the thunk.** Forcing happens in `evaluate_arg`
  regardless of `context.execute`. If the thunk body contains effect nodes
  (Unit-returning), those are skipped by the existing central skip rule under a
  display pass, so no side effects fire outside Execute. A thunk producing an
  ordinary value evaluates normally during display, which is the desired
  preview behavior.
- **Captures are frozen at closure build time** (standard `ZoneClosure`
  semantics), so forcing at the consumer is well-defined and independent of the
  consumer's scope.

### D4 — Interaction with `apply` and the `-1` function pin is intended, not incidental

A node with every input captured synthesizes a `() -> T` via
`build_node_function_closure` (the title-bar `-1` pin). Wiring such a node
directly into a value pin now auto-collapses to `T`. This is intended: it is
the same isomorphism, applied to the function-pin source rather than a `closure`
node. `apply` with zero arguments remains available and unchanged (its
identity-partial guard is a separate path); the coercion simply removes the
*need* for it in the common "thunk → value" wiring.

## Implementation (as built)

1. **`data_type.rs`** — `can_be_converted_to` becomes a thin wrapper over a
   private `can_be_converted_to_impl(.., top_level: bool)` (see D1). Early in
   the impl (right after the `T → Unit` discard, before the record/iterator/
   array arms) sits the gated nullary arm:

   ```rust
   if top_level {
       if let DataType::Function(src_ft) = source_type {
           if src_ft.parameter_types.is_empty() && !dest_type.is_function_shape() {
               return Self::can_be_converted_to_impl(
                   &src_ft.output_type, dest_type, registry, false,
               );
           }
       }
   }
   ```

   Placed *early* so it applies uniformly to scalar, array, and iterator
   destinations (the later iterator/array arms would otherwise `return` first).
   **Every** internal recursive call in the impl passes `top_level = false`, so
   the arm never fires at a nested element/param/field position (closes the
   `[() -> T] → [T]` hole). The `!is_function_shape` guard lets `() -> T → () -> T`
   and `→ AnyFunction` fall through to the function arms as ordinary function
   values.

2. **`data_type.rs::can_be_converted_to_strict_no_broadcast`** — deliberately
   **not** given the nullary arm. Its only caller is the drag-aware add-node
   popup, which stays conservative (D1 / Non-goals). A short comment marks the
   intentional omission.

3. **`network_evaluator.rs::evaluate_arg`** — a helper `force_nullary_arg`
   forces an arity-0 function before `convert_to`, called in **both** branches
   (single-arg and array-merge) after the `Error` check:

   ```rust
   fn force_nullary_arg(&self, network_stack, registry, context,
                        result, source_type, expected_type)
       -> (NetworkResult, DataType) {
       let NetworkResult::Function(closure) = &result else { return (result, source_type); };
       if !closure.param_types.is_empty() { return (result, source_type); }
       // Peek ONE Array level: `[T]` forces each wire, `[() -> T]` keeps them.
       let slot = match expected_type { DataType::Array(e) => e.as_ref(), other => other };
       if slot.is_function_shape() { return (result, source_type); }
       let declared_return = *closure.function_type().output_type; // infer fallback
       let forced = run_closure_once(self, network_stack, registry, context, closure, Vec::new());
       let forced_type = forced.infer_data_type().unwrap_or(declared_return);
       (forced, forced_type)
   }
   ```

   `param_types` is the closure's *body frame size* (see `design_currying.md`) —
   the correct "consumes zero caller args" test, also true for a partially-
   applied closure that has become nullary. `infer_data_type` returns
   `Option`; the declared return type is the fallback. An `Error` from forcing
   flows into the caller's existing post-force `Error` check and surfaces as an
   input error.

## Testing (as built)

- **Conversion unit** (`tests/structure_designer/data_type_test.rs`,
  `nullary_*` + `higher_arity_functions_are_never_forced`): `() -> Int → Int`,
  `() -> Crystal → HasAtoms`, `() -> Int → Float` (true); `() -> Int → Bool`
  (false); `Int → () -> Int` (false, one-directional); `() -> Int → () -> Float`
  and `→ Function*` (true, stays a function); `() -> Int → [Int]` / `→ Iter[Int]`
  (true, case A composes with broadcast); `[() -> Int] → [Int]` (false, D1
  case B); strict variant `() -> Int → Int` (false); `(Int) -> Int → Int`
  (false, only arity-0 forces).
- **Evaluator E2E** (`tests/structure_designer/closures_test.rs`,
  `nullary_closure_*`): a nullary `closure` returning `42` wired straight into
  an `Int` `expr` pin (no `apply`) computes `84`; the connection gate accepts
  the wire; the same closure forced by two consumers yields each consumer's
  value (reuse). The N-times fan-out behavior follows from the documented
  no-memoization invariant rather than a test that asserts it.
- **Function-pin source (D4)**: a node with all inputs captured (`-1` pin)
  wired into a value pin collapses to `T`.
- **`.cnnd` roundtrip**: a graph using the coercion loads, validates, and
  evaluates identically after save/reload (no new serialized state is
  introduced — the coercion is purely a conversion rule, so this mostly guards
  against the validator rejecting a previously-accepted wire).

## Non-goals

- Recursive coercion through collections / function parameters (D1).
- Memoization / sharing of forced thunks (issue #327 use-case 4) — orthogonal.
- The reverse `T → () -> T` auto-suspension.
- Any change to `FunctionType::new`'s canonical form (D2 depends on it).
