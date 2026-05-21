# Function Pins: Whole-Node Function Values as a Direct Shortcut

## Scope

This document designs the revival of the **function pin** — the inside-of-the
title-bar output pin (`pin_index == -1`) that every non-HOF node still *renders*
but which has been **dead** since closures Phase 2 deleted the
`FunctionEvaluator` / `output_pin_index == -1` machinery (`doc/design_closures.md`
§"Relationship to the zones dead-weight-cleanup plan").

The feature re-wires that pin to the existing **closure substrate**
(`evaluator/zone_closure.rs`) so it produces a real `NetworkResult::Function`
value, under a deliberately **minimal** rule:

> A node's function pin is typed `(all input pins) -> output pin 0` — exactly
> today's `NodeType::get_function_type()` (`node_type.rs:217`). It may be wired
> into an HOF's `f` pin (or `apply.f`) **only when that type matches the pin**.
> Every input is a parameter, in declared order; a node whose function pin is
> wired must therefore leave **all input pins disconnected** (a wired input would
> be dead — see §"Function mode"). Captures are **not** expressed through the
> function pin — when the shape doesn't fit, the user reaches for the explicit
> `closure` node (which already exists and works).

This is the strict, no-capture, no-ambiguity reading of `design_closures.md`
**Open Question 1 ("Implicit closures / auto-wrap")**. It deliberately does *not*
implement partial application / auto-capture: the "which input is the argument"
problem is dissolved by fiat (all inputs are parameters), and the explicit
`closure` node is the full-power escape hatch for everything else.

In scope:
- Conceptual model: the function pin as the whole node, applied per element.
- Evaluator change: one new synthesizer + re-adding the `-1` branch in `evaluate`.
- Connection / validation gating for `pin_index == -1` sources, including the
  **function-mode rule** (a node's function pin and its input pins are mutually
  exclusive).
- The (small) Flutter editor work to make the pin wireable again.
- Function-mode display suppression: a node whose function pin is consumed
  produces no scene output and its pin-0 eye is disabled (tooltip → `apply`). To
  preview a function on a chosen argument, wire it into `apply` and display that.

Out of scope:
- Partial application / capture via the function pin (use the `closure` node).
- Any change to the `closure` / `apply` nodes or the four HOFs' eval — they are
  reused verbatim.
- `.cnnd` migration (no user-facing fixtures wire the function pin today, because
  the UI never allowed it; nothing to migrate).
- Text-format syntax for function-pin wires (deferred; the wire is an ordinary
  `NodeOutput { pin_index: -1 }` incoming wire and serializes like any other).

### Relationship to the prior cleanup plan

`doc/design_zones_ui.md` §U7 scheduled the **removal** of function-pin rendering
(`NODE_VERT_WIRE_OFFSET_FUNCTION_PIN`, the title-bar `PinWidget`, the
`PinKind.functionPin` arm). **This design supersedes that removal** — instead of
deleting the function pin, we keep it and give it a working meaning. The
transitional `PinKind.functionPin` (`scope_resolver.dart`, `node_widget.dart`,
`node_network_painter.dart`) becomes permanent, repurposed from "legacy `-1`
convention" to "whole-node function value".

## Scope of this branch — build/test contract

| Must pass | When |
|---|---|
| `rust/` workspace (`cargo build`, `cargo clippy`, `cargo test`) | Phases 1–2 |
| `flutter_rust_bridge_codegen generate` succeeds | Phase 3 |
| Rust integration/roundtrip tests under `rust/tests/` | Phases 1–2 |
| `flutter run` launches; function-pin wiring works end-to-end | Phase 3 |
| Existing editing (zones, closures, non-zone) still works | every phase (regression) |

The runtime change (Phase 1) is gated independently of the UI: Phase 1 tests
construct the function-pin wire programmatically, so the feature is fully
exercised in Rust before any Flutter work.

## Concept

### A function pin is the whole node, applied per element

Today the four HOFs and `apply` obtain a `ZoneClosure`
(`evaluator/zone_closure.rs:36` — `{ body, captures, zone_output_wires,
owner_node_id, param_types, return_type }`) and run it once per element via
`run_closure_once`. A `closure` node produces one by wrapping its inline body;
an HOF builds one from its own inline body. **This feature adds a third
producer:** "node N viewed as a function of all its inputs", synthesized on
demand when N's `-1` output pin is evaluated.

The synthesized closure's body is a one-node synthetic network containing a clone
of N, with **every** input pin fed from a zone-input parameter and **no
captures**:

```
function pin of N : (in_0, in_1, …, in_{M-1}) -> out_0

  synthetic body:
    ┌───────────────────────────────────┐
    param_0 ●─┐                          │
    param_1 ●─┼─→ [ clone of N ] ─→ ● result   (= N.output[0])
       …      │                          │
    param_M ●─┘                          │
    └───────────────────────────────────┘
       (captures: empty)
```

Per element, the consumer (HOF or `apply`) pushes the iteration values as the
parameter frame and resolves the result wire — exactly the existing
`run_closure_once` step. Because there are no captures, this is the simplest
possible closure.

### Type match, all inputs are parameters

- **No argument designation.** Every input pin of N is a parameter, in declared
  order. There is no "free vs. captured" inference, no UI gesture, no
  defaults-muddiness — the function type is wiring-independent.
- **Connect only on match.** `(Int) -> Int` connects to `map.f`; `(Int, Int) ->
  Int` does not (arity differs) — it connects to `fold.f`. The match check is
  the existing `DataType::can_be_converted_to` `Function` arm: **structural
  convertibility** — same arity, with parameters and return pairwise convertible
  (so `(Int) -> Int` also fits a `(Float) -> Float` pin). No new type code.
- **Captures live in the `closure` node.** To map a body that references an
  outside value, the user builds an `(Int) -> Int` `closure`, drops the bigger
  node inside it, wires one zone-input as the parameter, and captures the rest —
  the existing, working flow. The function pin is a shortcut for the clean
  matching case only.

### Function mode: inputs and the function pin are mutually exclusive

Because every input is a parameter, a connected input on a function-pinned node
is a **dead wire** — the synthesizer never reads it. Rather than silently
discard such a wire, the design **forbids the combination**: a node's function
pin (`-1`) may be wired **only when all of the node's input pins are
disconnected**, and conversely an input pin may be wired only when the function
pin is not. Enforced both at connection time (drag gating, both directions) and
as a validation rule for the paths that bypass the gate (`.cnnd` loads,
text-format edits) — see §"Validation & connection gating".

This is not strictness bolted on; it makes the design self-consistent. The
function type is wiring-independent *by construction* (all inputs are
parameters), so a wired input cannot affect it — forbidding the dead wire is the
honest expression of that invariant. The escape hatch for "fix some inputs, vary
others" remains the `closure` node, where a captured wire *is* a real,
meaningful input.

### Display in function mode

A node whose function pin is **consumed** (wired into an HOF `f` or `apply.f`) is
a function definition, not a value source — so it produces no displayable scene
and its pin-0 eye is **disabled**, rather than showing a stand-in value. This is
the output-side dual of the existing HOF affordance: when an HOF's `f` *input* is
wired its inline body is hidden behind the "driven by `f`" placeholder; when a
node's `f` *output* is consumed its own value/eye is hidden.

"Function mode" is the derived predicate "this node's `-1` output pin has a
consumer" — the same reverse lookup the destination-side connection gate uses
(§"Validation & connection gating"). In that state:

- **No scene output.** The scene builder skips the node: it is not evaluated as a
  top-level displayed node and emits nothing to the viewport. (It is still
  evaluated *indirectly* and cheaply when the consuming HOF resolves its `f`
  arg — that only builds the closure bundle; the body runs per element.) The skip
  must override the **display policy**, not just the manual eye, so a
  function-mode node selected under the Selected/Frontier policy still renders
  nothing.
- **Eye disabled.** The pin-0 eye becomes non-interactive, with the tooltip
  "Used as a function — wire into `apply` to preview it." The tooltip is the
  redirect: it turns "why won't this display?" into a signpost to the node that
  previews a function correctly.

**This is derived, not a stored mutation.** Connecting the function pin does not
remove the node from `displayed_nodes`; the scene builder and the eye widget both
consult the predicate at render time. So there is no persisted change and no undo
command to add (cf. `feedback_persisted_mutations_must_be_undoable`), and
disconnecting `f` restores the node's prior eye/display state for free — exactly
as `resolve_body_collapsed` is a derived decision rather than stored state.

For a *sampled* preview on a chosen argument, the principled path is the existing
`apply` node: wire the function pin into `apply(<arg>)` and display that.
`f(defaults)` is deliberately not offered — it is an error for an `expr` free
variable and trivial otherwise.

## Data model

**No data-model changes.** A function-pin wire is an ordinary `IncomingWire`
(`node_network.rs:116`):

```rust
IncomingWire {
    source_node_id: N,
    source_pin: SourcePin::NodeOutput { pin_index: -1 },  // already representable
    source_scope_depth: 0,
}
```

stored on the consumer's `f` argument (an ordinary `External` argument). It
serializes, undoes, copies, and pastes exactly like any wire — no new cases. The
synthesized `ZoneClosure` is purely runtime and is never stored.

## Evaluator changes

### New: the synthesizer

A free function alongside `build_inline_closure` in
`evaluator/zone_closure.rs`:

```rust
#[allow(clippy::result_large_err)]
pub fn build_node_function_closure<'a>(
    evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,                 // N, in the top frame of network_stack
    registry: &NodeTypeRegistry,
) -> Result<ZoneClosure, NetworkResult>
```

Steps:

1. Resolve N's node type via `registry.get_node_type_for_node(node)`; read its
   `parameters` (→ `param_types`, in order) and `output_type()` (→
   `return_type`). Reject (`Err(Error)`) if N has zero inputs (a `() -> T`
   function matches no HOF) or a polymorphic/`None` output type (its function
   type's return is unresolved — see Open Questions).
2. Clone N into `body_node`; give it a fresh body-local id (e.g. `1`). Set
   `body_node.arguments`: for each input pin `i`, install a single wire
   `IncomingWire { source_node_id: <owner_key>, source_pin: ZoneInput {
   pin_index: i }, source_scope_depth: 1 }` (via `Argument::set_source_full`).
   N's input pins are guaranteed empty by the function-mode rule (§"Function
   mode"), so this fills empty slots — there are no live wires to discard. (N is
   non-HOF, so its `zone` / `zone_output_*` are already empty.)
3. Build the synthetic body `NodeNetwork` (mirror `NodeNetwork::new_empty()`, as
   `ensure_zone_init` does at `node_network.rs:619`): `nodes = { 1: body_node }`,
   `next_node_id = 2`.
4. Return:
   ```rust
   ZoneClosure {
       body: Arc::new(synthetic_network),
       captures: EMPTY_CAPTURES.clone(),               // no captures, ever
       zone_output_wires: Arc::new(vec![IncomingWire {
           source_node_id: 1,                          // the body node
           source_pin: SourcePin::NodeOutput { pin_index: 0 },
           source_scope_depth: 0,
       }]),
       owner_node_id: <owner_key>,                     // scope-stack frame key
       param_types, return_type,
   }
   ```

`owner_key` is the scope-stack key the consumer pushes the parameter frame onto;
it must equal the `source_node_id` used by the body node's `ZoneInput` wires in
step 2. Use N's original `node_id` — distinct from the body node's id (`1`), so
there is no body-id/owner-id coincidence. The `owner_node_id`-non-uniqueness
safety argument in `doc/design_closures.md` §"`owner_node_id`: the model's one
conceptual debt" holds trivially here because the closure has **no captures** and
each `run_closure_once` step is bracketed by its own push/pop.

### Re-add the `-1` branch in `evaluate`

Insert near the top of `NetworkEvaluator::evaluate` (`network_evaluator.rs`),
right after the missing-node guard (~line 1538) and **before** the central skip
rule and the built-in/custom dispatch — the function pin runs no `eval` and is
never `Unit`:

```rust
if output_pin_index == -1 {
    return build_node_function_closure(self, network_stack, node_id, registry)
        .map(NetworkResult::Function)
        .unwrap_or_else(|e| e);   // e is already NetworkResult::Error(_)
}
```

This is automatically reached by `obtain_closure` → `evaluate_arg` →
`resolve_incoming_wire`: a `NodeOutput { pin_index: -1 }` wire routes through
`self.evaluate(source_slice, N, -1, …)` (`network_evaluator.rs:1286`). The
returned `Function(zc)` then flows into the HOF/`apply` exactly like a `closure`
node's output — **no consumer change**.

Today `evaluate` is never called with `output_pin_index == -1` (the only producer
was the deleted `-1` branch; no UI wires the pin), so the new branch fires only
for genuine function-pin wires. Early-returning skips the bottom-of-`evaluate`
display-string recording, which is correct — the function pin has no display
value, and a node in function mode emits no scene output at all (§"Display in
function mode").

### Source-type resolution for `-1`

`resolve_incoming_wire` computes the source type for `convert_to` via
`registry.resolve_output_type(source_node, source_network, -1)` with a fallback
to `get_output_pin_type(-1)` (`network_evaluator.rs:1274-1281`).
`resolve_output_type` already short-circuits `-1` to `get_function_type()`
directly (`node_type_registry.rs:682-687`), and `get_output_pin_type(-1)`
(`node_type.rs:229`) returns the same as the fallback — so the source type is
already correct with **no patch needed**.

## Type system

No new types. `DataType::Function` and its `can_be_converted_to` arm (structural,
same-arity, pairwise-convertible — closures Phase 2) already cover the match. The
function pin's type is `get_function_type()` = `Function((param types) ->
output[0])`, unchanged.

## Validation & connection gating

**One new validation rule (function mode) plus connect-time gating.** The
function pin and the input pins of a node are mutually exclusive (§"Function
mode"), and the function-pin wire must type-match the `f` pin. Both are enforced:

- **`can_connect_nodes`** (API, the authority), in *both* directions:
  - Drag source is `pin_index == -1`: **reject if the source node has any wired
    input pin**; otherwise the candidate source type is `get_function_type()`,
    accepted iff it `can_be_converted_to` the destination `f` pin's declared
    `Function` type (this enforces the type match at authoring time).
  - Drag destination is an input pin: **reject if that node's function pin is
    already wired out**.
- **`network_validator`** — a new rule flags any node that has **both** a wired
  function pin and one or more wired input pins (attributed to the node), catching
  the paths that bypass the drag gate (`.cnnd` loads, text-format edits). The same
  pass resolves a `-1` source's type via `get_function_type()` for the existing
  wire type-compatibility check, so an arity/type mismatch on a stored
  function-pin wire still surfaces.

The HOF rules already in place need **no change**: `function_input_pin_connected`
(`node_network.rs:524`) detects a wired `f` pin regardless of the source pin kind,
so the "zone-output pin must have a wire" rule is already suspended when an HOF's
`f` is driven by a function pin; `apply`'s required-`f` rule already applies.

## Editor (Flutter) changes

The function pin is **already rendered** on every non-HOF node
(`node_widget.dart:967-977`, typed from `node.functionType`) and already maps
`sourceOutputPinIndex == -1 → PinKind.functionPin` on read
(`node_network.dart:953`, `node_network_painter.dart:94`). The only gaps are
authoring:

1. **Allow the drag.** `canConnectPins` must accept a `PinKind.functionPin`
   source dropped on an `externalInput` of `Function` type. `connectPins` must
   package the source as `APISourcePin::NodeOutput { pin_index: -1 }`,
   `source_scope_depth: 0`, `destination_argument_kind: External`, and call the
   existing `connect_nodes`. (The Rust `can_connect_nodes` is the authority on
   both type fit *and* the function-mode mutual-exclusion rule — the Dart check
   can be permissive and let Rust reject; mirroring the rule in Dart only buys
   earlier drag feedback.)
2. **Drag feedback.** `_getPinPositionAndDataType` for a `functionPin` source
   already resolves via `pinScreenPosition` (`scope_resolver.dart` has the
   `functionPin` arm). Confirm the drag overlay and `onWillAccept` use the
   source's `node.functionType` as the dragged data type.
3. **Function-mode display suppression.** When the node's function pin is
   consumed, disable its pin-0 eye (tooltip: "Used as a function — wire into
   `apply` to preview it"). This reads the same derived "function-pin-consumed"
   predicate the scene-builder skip uses (§"Display in function mode"); neither
   persists state, so disconnecting `f` restores the prior display.

Optional polish (Open Questions): suppress the function pin on nodes where it
can't plausibly match (e.g. arity > 2 or polymorphic output) to reduce
"why won't it connect?" confusion.

## Reuse map (summary)

**Reused unchanged:**
- `ZoneClosure`, `run_closure_once`, `obtain_closure`, the four HOFs' eval, the
  `apply` node, `Walker::MapZone`/`FilterZone`.
- `DataType::Function` + `can_be_converted_to`.
- `function_input_pin_connected` and the closures validation rules.
- Wire storage / serialization / undo / copy-paste (`IncomingWire` already
  represents `NodeOutput { -1 }`).
- The Flutter function-pin rendering + `PinKind.functionPin` (now permanent).

**New from scratch:**
- `build_node_function_closure` (`evaluator/zone_closure.rs`).
- The `-1` branch in `NetworkEvaluator::evaluate`.
- The function-mode rule: mutual-exclusion gating in `can_connect_nodes` (both
  directions) + a `network_validator` rule.
- Function-mode display suppression: the scene builder skips a node whose
  function pin is consumed, and the Flutter pin-0 eye is disabled (tooltip →
  `apply`). Both reuse the "function-pin-consumed" predicate; derived, not stored.

**Reused with small patches:**
- `can_connect_nodes` / validator: add the type-match check for `-1` sources.
  (`resolve_output_type` already resolves `-1` to `get_function_type()` —
  `node_type_registry.rs:682` — so no patch there.)
- Flutter `canConnectPins` / `connectPins`: allow + package the `functionPin →
  Function` wire (optionally mirroring the function-mode rule for early feedback).

## Implementation phases

Each phase ends with `cd rust && cargo test` green and `cargo clippy` clean; the
final phase additionally ends with `flutter run` launching a working editor.
Phases are sequential.

### Phase 1: Rust runtime — synthesizer + `-1` eval branch

**Goal.** A function-pin wire produces a working `NetworkResult::Function`
consumed by the HOFs and `apply`.

**Scope.**
- Add `build_node_function_closure` to `evaluator/zone_closure.rs` (§"Evaluator
  changes").
- Add the `output_pin_index == -1` branch to `NetworkEvaluator::evaluate`.
- Patch `resolve_output_type` to return `get_function_type()` for `-1` if it
  doesn't already (so `resolve_incoming_wire`'s source typing is correct).

**Tests** (new `rust/tests/structure_designer/function_pin_test.rs`; register in
`structure_designer.rs`). Construct wires programmatically (bypass the UI):
- `range(3) → map(f: <expr "x+1">.fn) → collect` yields `[1, 2, 3]` (one-input
  `expr`'s function pin into `map.f`).
- A single-input built-in: `map(f: sphere.fn)` over a range of radii yields
  Blueprints (smoke; assert count / no error).
- `apply(f: <expr "x*2">.fn, 10)` yields `20` (single-shot, no iterator).
- `fold(f: <expr "a+b">.fn, init)` over a range sums correctly (arity-2 match;
  param order = input 0 → acc, input 1 → element).
- Custom subnetwork: a one-parameter subnetwork's function pin into `map.f`
  evaluates via the recursive custom-node path.
- Independence: two `collect`s of the same `map(f: …)` drain independent walkers
  (clone-independence of the embedded `ZoneClosure`).
- Error: a zero-input node's function pin returns `Error` at eval.

**Gotchas.** `owner_node_id` must differ from the body node's id; param order is
N's input-pin order (document it in the test for `fold`); the synthetic body must
be a well-formed `NodeNetwork` (`new_empty()` shape, `next_node_id` ahead of the
body node id).

### Phase 2: Rust connection gating — type match + function mode

**Goal.** Authoring a function-pin wire is allowed iff (a) the source node has no
wired input pins and (b) the function type matches the `f` pin; and a stored
function-pin wire validates on both counts. A node whose function pin is consumed
also stops emitting scene output.

**Scope.**
- `can_connect_nodes` (API), both directions:
  - source pin `-1`: reject if the source node has any wired input pin; else
    resolve the source type to `get_function_type()` and accept iff
    `can_be_converted_to` the `f` pin's `Function` type.
  - destination is an input pin: reject if that node's function pin is wired out.
- `network_validator`: (a) a new rule flagging a node with **both** a wired
  function pin and wired input pin(s); (b) the wire type-compatibility check
  resolves a `-1` source's type via `get_function_type()` so an arity/type
  mismatch surfaces.
- Scene builder: skip a node whose function pin is consumed (the same
  "function-pin-wired-out" predicate used by the destination-side gate above) so
  it emits no viewport output regardless of display policy. Derived — no change
  to `displayed_nodes`.

**Tests** (extend `function_pin_test.rs`):
- `can_connect` accepts `(Int)->Int` function pin → `map.f`; rejects
  `(Int,Int)->Int` → `map.f`; accepts `(Int,Int)->Int` → `fold.f`.
- `can_connect` rejects a function-pin drag from a node that has a wired input,
  and rejects wiring an input pin on a node whose function pin is already wired.
- A node with both a wired function pin and a wired input produces the
  function-mode `ValidationError`; disconnecting either side clears it.
- A wired-but-type-mismatched function pin produces the expected
  `ValidationError`; a matched one validates clean.
- An HOF driven by a function pin with an empty inline body validates (rule-1
  suspension via `function_input_pin_connected`); `apply` with a disconnected
  `f` still errors.
- A node whose function pin is consumed produces no scene output even with its
  pin-0 display on (scene-skip overrides display policy); removing the wire
  restores its output.

### Phase 3: Editor (Flutter) — make the pin wireable

**Goal.** Drag a node's function pin into an HOF `f` / `apply.f` pin in the
editor and have it evaluate; the source node's pin-0 eye is disabled while it
acts as a function.

**Scope.** §"Editor (Flutter) changes" items 1–3 (allow + package the
`functionPin → Function` wire; disable the pin-0 eye on a function-mode node)
plus FRB regen.

**Tests.** Manual walkthrough (thin UI; evaluation already covered by Phases 1–2,
per `feedback_manual_test_for_editor_ui`): place an `expr "x+1"` (one free var
`x: Int`), drag its function pin into a `map`'s `f` pin, feed `range(3)`, display
a downstream `collect`, confirm `[1, 2, 3]`. Then confirm: a `(Int,Int)->Int`
node's function pin **won't** connect to `map.f` (it does to `fold.f`); a node
with a wired input won't let you drag its function pin (and an input pin won't
accept a wire once the function pin is wired); the source node's pin-0 eye is
disabled (its tooltip points at `apply`) and it draws nothing in the viewport,
and disconnecting `f` re-enables the eye; existing non-function-pin editing is
unchanged.

**Verification.** `flutter run` works; `cd rust && cargo test` still green.

## Open questions

1. **Should the pin be shown on every node?** It renders on all non-HOF nodes but
   realistically matches only purpose-shaped nodes (`expr` with the right free
   vars; single-/matching-arity custom subnetworks; the odd single-input
   built-in). On multi-input built-ins it's mostly inert. Option: suppress it
   where arity > the largest HOF arity (2) or where the output is polymorphic, to
   cut "why won't it connect?" confusion. Deferred polish.

2. **Polymorphic-output nodes.** A node whose pin-0 type is `SameAsInput` /
   `SameAsArrayElements` has an unresolved (`None`) function-type return, so its
   function pin can't match anything concrete. Phase 1 rejects these at synthesis;
   revisit if a real use case appears.

3. **Optional inputs become mandatory parameters.** Treating *all* inputs as
   parameters means a node's defaults vanish in its function view (e.g. a node
   with two optional inputs presents as `(A, B) -> out`, not `(A) -> out`). This
   is intentional (wiring-independent typing) but narrows applicability; the
   `closure` node remains the answer when you want to fix some inputs.
