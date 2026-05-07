# Node Execution: Side Effects, Print, and an `execute` Flag

## Motivation

Three related needs:

1. **Side-effect nodes need an explicit trigger.** `export_xyz` writes a file as
   a side effect of being evaluated. Today the only way to trigger that
   evaluation is to make the node visible (or to display something downstream
   of it). That's a hack with two real bugs:
   - Editing any upstream parameter while the node is visible silently
     re-runs the export, overwriting files.
   - There is no way to express "export N variants" without either
     materializing all N atomic structures into an `Array` (memory cost) or
     hand-toggling visibility N times.

2. **Side-effect iteration must skip work entirely when not executing.** The
   batch-export use case wants a higher-order primitive that *literally does
   nothing* on a normal display pass — it should not pull a single element
   from its iterator, not build a `FunctionEvaluator` for its body, not touch
   any of its upstream computation. That requires either a node that knows it
   is a side-effect HOF (and short-circuits on `execute == false`) or
   user-led discipline to keep huge iterators behind a manually-toggled
   visibility — and the latter is exactly the hack we are removing.

3. **Debug visibility into the graph.** When an expression-heavy network goes
   wrong, there is no way to see the intermediate values flowing through a
   wire short of inserting a `display` node and visually inspecting whatever
   node display can render. There is no "print" mechanism.

All three close cleanly with one new evaluator-context flag, one new
side-effect HOF, and two new nodes (`print`, `foreach`) plus a new `Unit`
type.

The motivating end-to-end use case is **mass batch export**:

```
product[Variant]  →  foreach( variant →
                       export_xyz(  build_molecule(variant),
                                    `out/${variant.species}_${variant.size}.xyz` ) )
```

The user right-clicks the `foreach` node, picks **Execute**, and the
per-variant exports run lazily through the iterator without ever
materializing an `Array[Molecule]`. On a normal display pass the `foreach`
node returns immediately without touching its inputs, so even a million-element
iterator costs nothing.

## Design summary

- A single `bool execute` field is added to `NetworkEvaluationContext`.
- The flag is `false` for all normal display / scene-generation evaluations.
- The flag is `true` only for evaluations triggered via a new
  **Execute** action (right-click on a node → menu → Execute).
- A new **`Unit`** primitive type is added (`DataType::Unit`,
  `NetworkResult::Unit`). It is the type with exactly one value, used as
  the return type of effect nodes. A universal `T → Unit` widening
  ("discard") is added at field-level so any sub-network output can be
  consumed by an effect-typed pin.
- **Central skip rule**: in the evaluator's per-node entry point, before
  calling `NodeData::eval`, if `!context.execute` *and every resolved
  output pin of the node is `DataType::Unit`*, the call is skipped and
  the evaluator synthesises an `EvalOutput` of all `NetworkResult::Unit`
  values directly. This gates *every* Unit-returning node in one place —
  no per-node guards, no risk of forgetting one.
- `export_xyz` is changed to return `Unit` instead of passthrough
  Molecule. Its `eval` body just calls `save_xyz` unconditionally — the
  central rule guarantees `eval` is only invoked under
  `context.execute == true`. (No migration burden — the node was unused.)
- A new **`foreach`** node mirrors `map`'s shape (`xs: Iter[T]`,
  `f: Function([T] → Unit)`) and returns `Unit`. Its `eval` is a plain
  drain-the-walker loop; the perf-critical "skip the iterator and the
  body during display passes" property falls out of the central rule
  because `foreach` returns `Unit`.
- The execute flag propagates through `FunctionEvaluator` so that bodies
  of `foreach`, `map`, `filter`, `fold` all inherit it. Combined with the
  central rule, this is what lets effects nested inside a
  `map(...) → foreach` chain fire under Execute (the inner
  `export_xyz`'s `eval` is reached because the FE call inherits
  `execute == true`, and the central rule doesn't skip it).
- A new **`print`** node takes a `String` input, returns the same `String`
  (passthrough), and as a side effect appends an entry to a per-CAD-instance
  print-log buffer. A bool property `execute_only` (default `false`) gates
  whether the side effect fires only under the execute flag or on every
  evaluation.
- Flutter gains a toggleable bottom **Console** panel that displays the
  print-log buffer. The panel is hidden by default and surfaces via a
  toolbar button / keyboard shortcut.

`map` keeps its data semantics. The `map(... export_xyz ...)` pattern still
works under Execute (because the flag propagates through `FunctionEvaluator`),
but `foreach` is the recommended primitive for batch export because of the
display-pass short-circuit: `map`'s iterator output makes it eligible for the
display path's `ITER_DISPLAY_CAP = 256` auto-collect, which would silently
materialize 256 elements during normal editing — exactly what the use case
wants to avoid. `foreach` returns `Unit`, which is never displayed, so it is
inert during normal editing by construction.

## The `Unit` type

`Unit` is the type with exactly one value. It is the standard
functional-language equivalent of `void`, but unlike `void` it is a real
type that can sit in collections, be the return type of a function, and
flow through wires.

The only motivation for adding it now is *honest signatures for effect
nodes*: `export_xyz` and `foreach` produce no useful value, and pretending
they return Molecule (passthrough) or Iter[T] would mislead readers and
invite the visibility-toggle hack to come back.

### `DataType::Unit`

Add a new variant to `rust/src/structure_designer/data_type.rs`:

```rust
pub enum DataType {
    // ... existing primitives ...
    Unit,
    // ... existing compound types ...
}
```

Conversion rules in `DataType::can_be_converted_to`:

| From | To | Allowed? | Notes |
|------|------|---------|-------|
| `Unit` | `Unit` | yes | identity |
| `T` (any) | `Unit` | **yes** | universal "discard" widening |
| `Unit` | `T` (any non-Unit) | **no** | a Unit value carries no information; cannot materialize a `T` from it |
| `Unit` field of `Record(...)` | same | yes | structural; Unit is a normal field type |
| `Iter[Unit]` | `Iter[Unit]` | yes | identity passthrough only (matches existing iterator rule that `Iter[S] → Iter[T]` with `S ≠ T` is disallowed in v1) |
| `Function([A] → T)` | `Function([A] → Unit)` | **yes** | falls out of covariant output rule + universal `T → Unit` |

The universal `T → Unit` widening is what lets a user wire a sub-network
ending in `print` (output type `String`) into `foreach.f` (declared type
`Function([T] → Unit)`): the `String → Unit` conversion is taken on the
function's output position. The reverse `Unit → T` is forbidden so that
`Unit` can never be implicitly used in place of a real value.

### `NetworkResult::Unit`

Add a single-fielded variant to
`rust/src/structure_designer/evaluator/network_result.rs`:

```rust
pub enum NetworkResult {
    // ... existing variants ...
    Unit,
}
```

`infer_data_type` returns `DataType::Unit`. `to_display_string` returns
`"()"`. No `extract_unit` accessor — there is nothing to extract.

`convert_to(source, &DataType::Unit)` returns `NetworkResult::Unit` for
every non-error source variant (including `NetworkResult::Iterator(_)` —
the walker is dropped without being drained, which is the desired
"discard" semantic for an unused iterator). `Error` short-circuits and is
returned as-is.

### Display semantics

A `Unit` output pin is **not displayable**. Concretely:

- The eye icon next to a `Unit` output pin is hidden in
  `NodeView`'s pin-row rendering (treat as a special case in the
  Flutter pin-row widget).
- `NodeSceneData::interactive_pin_index` skips Unit pins when picking
  the interactive pin — a Unit pin can never be the lowest displayed
  pin because it can never be displayed at all.
- The text-format serializer treats `Unit` exactly like any other type
  in its grammar (a single token name); no special formatting.

### Out-of-scope follow-ups

- A user-facing `Unit` literal in `expr` (currently no use case — `Unit`
  values originate only from effect nodes).
- A `discard` node (`T → Unit`) for explicit discard at the node-graph
  level. The universal field-level coercion already covers every case
  the design needs; an explicit node would mostly duplicate it.

## The execute flag

### Wiring into `NetworkEvaluationContext`

`rust/src/structure_designer/evaluator/network_evaluator.rs` already defines
the per-pass evaluation context:

```rust
pub struct NetworkEvaluationContext {
    pub node_errors: HashMap<u64, String>,
    pub node_output_strings: HashMap<u64, Vec<String>>,
    pub selected_node_eval_cache: Option<Box<dyn Any>>,
    pub top_level_parameters: HashMap<String, NetworkResult>,
    pub use_vdw_cutoff: bool,
    /// NEW: when true, side-effect nodes (export_xyz, print w/ execute_only)
    /// actually perform their effect during this evaluation pass.
    /// Set to `true` only when the user triggers an explicit Execute action.
    pub execute: bool,
}
```

`Default` / `new()` initialise it to `false`. Every existing call site that
constructs a context for a normal display pass continues to get `execute: false`
and is unaffected.

### Propagation through `FunctionEvaluator`

`rust/src/structure_designer/evaluator/function_evaluator.rs` currently does:

```rust
// TODO: think about whether the context is ok this way?
evaluator.evaluate(
    &network_stack,
    self.main_node_id,
    0,
    registry,
    false,
    &mut NetworkEvaluationContext::new(),   // <-- fresh context, drops execute
)
```

That `NetworkEvaluationContext::new()` is the propagation hole: when
`map`/`fold`/`filter` evaluate their per-element body via the
`FunctionEvaluator`, they construct a fresh context, so any `execute` flag set
by the caller would be silently dropped. The body's `export_xyz` would never
fire.

`FunctionEvaluator::evaluate` must accept the **outer** `NetworkEvaluationContext`
by `&mut` reference, inherit the relevant fields into a fresh inner context for
the body evaluation, and drain the inner `print_buffer` back into the outer at
end so prints from inside `map` / `filter` / `fold` / `foreach` bodies aren't
silently dropped:

```rust
pub fn evaluate(
    &mut self,
    evaluator: &NetworkEvaluator,
    registry: &NodeTypeRegistry,
    outer_context: &mut NetworkEvaluationContext,   // NEW parameter; &mut so we can drain prints back
) -> NetworkResult {
    let mut inner = NetworkEvaluationContext::new();
    inner.execute = outer_context.execute;
    inner.use_vdw_cutoff = outer_context.use_vdw_cutoff;
    // top_level_parameters / node_errors / node_output_strings /
    // selected_node_eval_cache are intentionally NOT inherited — they are
    // per-pass scratch state scoped to the outer network.
    // print_buffer is NOT inherited (each FE call starts with an empty buffer)
    // and is instead drained back into outer_context.print_buffer below, so
    // prints from inner-body nodes aggregate into the single per-pass log.
    let result = evaluator.evaluate(
        &network_stack,
        self.main_node_id,
        0,
        registry,
        false,
        &mut inner,
    );
    outer_context.print_buffer.append(&mut inner.print_buffer);
    result
}
```

The `append` call is O(k) in the number of prints produced by the body and is
free when the body contains none. The order of entries in
`outer_context.print_buffer` after the call is: outer prints recorded *before*
this FE call, then the body's prints in their evaluation order. That matches
the chronological story the Console panel wants to show.

All callers of `FunctionEvaluator::evaluate` (today: `map.eval`, `filter.eval`,
`fold.eval`, `Walker::Map::next`, `Walker::Filter::next`) already have access
to the outer `NetworkEvaluationContext` (it's the `context` parameter on every
`NodeData::eval` and on `Walker::next`). They forward it as `&mut`.

This is the **only** propagation site. Plain (non-HOF) downstream nodes
already share the caller's context via the recursive `evaluate()` traversal —
nothing else changes.

### Central skip rule for Unit-returning nodes

The `execute` flag is consulted in **exactly one** place in the evaluator
(plus the `print` node's per-node `execute_only` check, which is its own
private opt-in to a different mechanism). That place is the central
per-node evaluation entry point in
`rust/src/structure_designer/evaluator/network_evaluator.rs` —
`evaluate_all_outputs` (which then dispatches to `NodeData::eval`).

The rule, conceptually:

> When evaluating a node, if `context.execute` is `false` **and** every
> resolved output pin of that node has `DataType::Unit`, skip
> `NodeData::eval` entirely and return an `EvalOutput` whose pins are
> all `NetworkResult::Unit`.

In code (sketch — exact integration depends on the existing structure of
`evaluate_all_outputs`):

```rust
fn evaluate_all_outputs(...) -> EvalOutput {
    // ... existing setup: resolve node, look up node_type, etc. ...

    if !context.execute && all_resolved_output_pins_are_unit(node, node_type, registry) {
        let n = node_type.output_pins.len();
        return EvalOutput::multi(vec![NetworkResult::Unit; n]);
    }

    // ... existing path: call node_data.eval(...) ...
}
```

The check uses the **resolved** output type (via the existing
`NodeTypeRegistry::resolve_output_type` machinery), not the declared
`OutputPinDefinition` — so a hypothetical future `SameAsInput` pin that
resolves to `Unit` is also covered. This is the same resolution path
that wire validation uses, so there's no new "what does this pin really
produce?" question to answer.

The rule applies only when **all** output pins are Unit. A multi-output
node with a mix (say, `Float` plus `Unit`) is evaluated normally — its
non-Unit outputs may be needed downstream, and we cannot synthesise a
Float without running `eval`. No such mixed node exists today; the
all-Unit form covers `export_xyz`, `foreach`, and any future pure-effect
nodes. If a mixed-output effect node ever surfaces, its author must
gate the effect by hand, and the design-doc principle is that they
shouldn't — they should split it into a data node + a Unit sink.

#### What this buys

- **No per-node guards.** `export_xyz.eval` and `foreach.eval` (and any
  future effect node) call their effect logic unconditionally; the
  evaluator wrapper guarantees they only run when execute is true.
- **`foreach`'s perf property is automatic.** The "skip the iterator,
  skip building the FE, skip everything" behaviour is not bespoke to
  `foreach` — it's the central rule, observed by every Unit node.
  A user can wire a million-element `product` upstream of a `foreach`
  and pay zero cost during normal editing.
- **One audit point for the effect/non-effect split.** Reviewers grepping
  for the execute flag find one site in the evaluator and one site in
  `print.rs`. There is no fourth, fifth, or twentieth effect node
  silently growing its own per-node check.

#### Tradeoff: lost runtime input feedback on Unit nodes

Per-node `eval` arms today often perform light runtime input validation
(e.g. `export_xyz` returns `"Missing export XYZ file name"` when the
stored `file_name` is empty and no `file_name` input is wired). Under
the central rule, that arm never runs during display passes — the
error only surfaces on Execute.

This is acceptable because:

- **Network validation** (in `network_validator.rs`) already catches
  structural problems — missing required input connections, type
  mismatches, broken wires — and lights them up red in the graph
  during editing. Most user-visible misconfigurations remain visible
  immediately.
- **Runtime-only checks** (an empty stored string, a divide-by-zero in
  a parameter expression) are the residual class. For a Unit-returning
  node these now defer to Execute. The user sees them in the Console
  panel + status bar at Execute time. Annoying but not destructive: no
  files are written, nothing is overwritten.

The cleaner generic rule is worth the slimmer feedback loop. If a
specific Unit node really needs eager input feedback, it can implement
`get_subtitle` to surface the issue at editing time without going
through `eval` (e.g. `export_xyz`'s subtitle could read "(no file
name)" when its stored `file_name` is empty and the input pin is
unconnected).

### Triggering execute mode from the UI

A new top-level orchestration entry point on `StructureDesigner`. Note that
context construction + print-buffer drain are *not* spelled out inline here
— they are owned by the `with_eval_context` helper (see "Console panel →
Centralized drain — no per-call-site boilerplate") that every eval-driving
code path in `StructureDesigner` shares. The exact body is given there; the
shape from the caller's perspective is:

```rust
pub fn execute_node(
    &mut self,
    network_name: &str,
    node_id: u64,
) -> ExecuteResult {
    let pass_start = self.print_log.len();
    let result = self.with_eval_context(/*execute=*/true, |evaluator, context| {
        // ... build network_stack from network_name ...
        evaluator.evaluate(
            &network_stack,
            node_id,
            0,                                  // pin 0 — execute always targets pin 0
            &self.node_type_registry,
            false,
            context,
        )
    });
    // Logs from THIS pass only (see drain helper section for why we slice
    // rather than calling `take_print_log()`).
    let logs = self.print_log[pass_start..].iter().map(Into::into).collect();
    ExecuteResult { ok: !matches!(result, NetworkResult::Error(_)),
                    error: match result { NetworkResult::Error(s) => Some(s), _ => None },
                    logs }
}
```

Three properties of an execute pass:

1. **One-shot.** No subscription, no recurring trigger. The user must invoke
   it again to re-fire.
2. **Independent of display state.** Whether the node is visible or not,
   whether anything downstream is displayed, the execute pass evaluates the
   targeted node (and its transitive inputs) fresh.
3. **No effect on display caches.** Per `evaluator/AGENTS.md`, the evaluator
   does not memoize `NetworkResult` across calls, so an execute pass and a
   subsequent display pass do not interfere with each other. The `CSG cache`
   on `NetworkEvaluator` is benign (geometry conversion only). Therefore
   nothing extra needs to be invalidated, flushed, or keyed on the flag.

### FFI

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn execute_node(network_name: String, node_id: u64) -> APIExecuteResult { ... }
```

`APIExecuteResult { ok: bool, error: Option<String>, logs: Vec<APIPrintLogEntry> }`
mirrors the Rust struct. The Flutter side surfaces `error` as a snackbar /
status message and appends `logs` to the Console panel state.

### Flutter UI: right-click → Execute

In `lib/structure_designer/node_network/`, the existing right-click context
menu on a node gains one new entry: **Execute** (icon: ▶). Always enabled —
non-side-effect nodes simply produce a value and discard it; the action is
harmless. The menu item calls the model's `executeNode(node_id)` which
forwards to the FFI and updates the Console panel + status bar from the
returned `APIExecuteResult`.

A toolbar shortcut (e.g. **Ctrl+E** when a node is selected) is a follow-up
nicety, not part of phase 1.

### UX during execution: modal placard, sync FFI, yielded frame

An execute pass on a `foreach` over a large iterator can take seconds to
minutes. Without UI feedback the user gets a frozen window and assumes the
app crashed. We need a modal "Executing…" dialog that paints *before* the
FFI call begins.

The recipe:

```dart
Future<void> executeNode(int nodeId) async {
  // 1. Schedule the modal placard.
  showDialog(
    context: context,
    barrierDismissible: false,
    builder: (_) => DraggableDialog(
      width: 320,
      dismissible: false,
      child: const Padding(
        padding: EdgeInsets.all(24),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          Icon(Icons.hourglass_empty),
          SizedBox(width: 16),
          Text('Executing…'),
        ]),
      ),
    ),
  );

  // 2. Yield until the dialog frame has actually painted.
  //    showDialog only schedules the dialog; without this yield the
  //    sync FFI below would block the UI thread before the dialog
  //    ever reaches the screen.
  await SchedulerBinding.instance.endOfFrame;

  // 3. Run the synchronous FFI call inside try/finally so the placard
  //    is always dismissed — including on a thrown FFI error or a
  //    Rust panic surfaced through FRB. Without the finally, the
  //    dialog (barrierDismissible: false, dismissible: false) would
  //    be stuck on screen with no way for the user to close it.
  APIExecuteResult? result;
  Object? thrown;
  try {
    result = sd_api.executeNode(networkName: networkName, nodeId: nodeId);
  } catch (e) {
    thrown = e;
  } finally {
    if (mounted) Navigator.of(context).pop();
  }

  // 4. Surface the outcome after the dialog is gone.
  if (thrown != null) {
    showSnackbar('Execute failed: $thrown');
    return;
  }
  if (result!.error != null) {
    showSnackbar(result.error!);
  }
  // Phase 4: console panel pulls result.logs.
}
```

Note the lack of a `CircularProgressIndicator`. The UI thread is blocked
during step 3, so any animated widget would freeze mid-frame and look
broken. A static icon + text is honest and adequate — the contract with
the user is "this dialog means execution is in progress; it goes away when
done."

#### Why not async (worker thread) FFI

The natural alternative is to drop `frb(sync)` on `execute_node`, let FRB
run it on a worker thread, and `await` it normally — the UI thread stays
live, the dialog can have a real spinner, etc. We rejected this for two
reasons grounded in the current codebase:

1. **`CAD_INSTANCE` has no synchronization.** `rust/src/api/api_common.rs`
   declares `pub static mut CAD_INSTANCE: Option<CADInstance> = None;`
   and the `with_mut_cad_instance` / `with_cad_instance` helpers use raw
   `addr_of_mut!` / `addr_of!` access. Their doc comments explicitly
   state *"The caller must ensure proper synchronization"* — the whole
   codebase assumes single-threaded access from the Dart UI thread.
   Running `execute_node` on a worker thread would create a data race
   on a raw `static mut` (undefined behaviour in Rust).
2. **Per-frame `provide_texture` would race even if the user can't
   click.** `lib/common/cad_viewport.dart` registers a persistent frame
   callback that calls `provide_texture` (`rust/src/api/common_api.rs`)
   every frame — also via `with_mut_cad_instance`. Persistent frame
   callbacks fire regardless of modal-dialog state (they're tied to the
   render pipeline, not input). So even with a barrier-blocking dialog,
   an async `execute_node` on a worker thread would race against the UI
   thread's per-frame renderer call on the same raw `static mut`.

Making async safe would require wrapping `CAD_INSTANCE` in a `Mutex`
(project-wide refactor of every `with_*` helper) AND teaching
`provide_texture` to `try_lock` and skip the texture update on
contention (so the renderer doesn't itself block waiting for execute).
That's a much larger design surface than this feature can justify on
its own, and it would still leave the same UX (renderer skips frames
while execute holds the lock, so the dialog can't usefully animate
either). Out of scope for this design; revisit if a future feature
genuinely needs concurrent UI + Rust.

The sync + yielded-frame approach is safe by construction: everything
serializes on the UI thread, the dialog paints first, the persistent
frame callback can't fire during the FFI call (the event loop isn't
running), so no concurrent access is possible.

## Changes to `export_xyz`

`rust/src/structure_designer/nodes/export_xyz.rs` — `eval()` body becomes:

```rust
// ... existing input extraction (atomic_structure, file_name, resolved_path) ...

if let Err(err) = save_xyz(&atomic_structure, &resolved_path) {
    return EvalOutput::single(NetworkResult::Error(format!(
        "Failed to save XYZ file '{}': {}", file_name, err
    )));
}

EvalOutput::single(NetworkResult::Unit)
```

No `if context.execute { … }` guard — the central skip rule (see "The
execute flag → Central skip rule for Unit-returning nodes") guarantees
this `eval` is only invoked when `context.execute == true`.

The `output_pins` field of the `NodeType` changes from
`OutputPinDefinition::single_fixed(DataType::HasAtoms)` to
`OutputPinDefinition::single_fixed(DataType::Unit)`. The `molecule` input
parameter type stays `HasAtoms` — what the node *consumes* is unchanged;
only what it *produces* is honest now. The Unit output is what makes
the central skip rule apply.

Two notes:

1. The output is no longer a passthrough Molecule — a user wanting both
   the export side effect *and* the molecule downstream must wire the
   molecule directly downstream and the `export_xyz` node as a sibling
   sink. This is a more honest shape and matches the semantics the
   execute flag introduces.
2. The node was unused before this change (per project memory); no
   migration path needed.

For eager UX feedback on the empty-`file_name` case (which used to
surface during display passes via `eval`), see the `get_subtitle`
suggestion in the central-skip-rule tradeoff discussion.

## The `print` node

A debug node — no production use case. The point is to let users insert a
node in the middle of an expression chain and see what is flowing through
without breaking the wire.

### Signature

| Field | Value |
|-------|-------|
| `name` | `print` |
| `category` | `MathProgramming` |
| `parameters` | `text: String` |
| `output_pins` | `OutputPinDefinition::single_fixed(DataType::String)` |
| `description` | "Passes its `text` input through unchanged. As a side effect, appends the text to the Console panel." |

Stored data (`PrintData` in `rust/src/structure_designer/nodes/print.rs`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintData {
    /// If true, the print fires only when evaluated under the execute flag.
    /// If false (default), the print fires on every evaluation, including
    /// display passes — useful for "what is flowing through this wire".
    pub execute_only: bool,
}
```

`get_text_properties` / `set_text_properties` expose `execute_only` as a
`TextValue::Bool`. A `bool` checkbox in the node-data widget edits it.

### Eval

```rust
fn eval(&self, evaluator, network_stack, node_id, registry, _decorate, context) -> EvalOutput {
    let text = match evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 0,
        String::new(), NetworkResult::extract_string,
    ) {
        Ok(v) => v,
        Err(err) => return EvalOutput::single(err),
    };

    let should_print = if self.execute_only { context.execute } else { true };
    if should_print {
        // Push to per-CAD-instance log buffer (see "Console panel" below).
        // The node label / network name come from the network_stack head;
        // node_id is already known.
        push_print_log(network_stack, node_id, &text, context.execute);
    }

    EvalOutput::single(NetworkResult::String(text))
}
```

The push routine takes a reference to the registry's print-log buffer and
appends an entry with the source location and the text. See next section.

### Frequency / dedupe

A `print` inside a `map` body fires once per element (lazy iteration —
correct, matches user intent). A `print` upstream of two displayed sinks
fires once per consuming evaluation, which can be twice per refresh —
acceptable for a debug node. No dedupe is performed; the Console panel
shows everything in arrival order with timestamps so the user can tell
duplicates apart.

## The `foreach` node

The side-effect counterpart of `map`. Same shape, different semantics on
both axes:

| Axis | `map` | `foreach` |
|------|-------|----------|
| Output | `Iter[U]` (lazy) | `Unit` |
| Display-pass cost | Constructs walker; auto-collects up to 256 elements when displayed | **Zero** — central skip rule applies because output is Unit; `eval` is not even called |
| Execute-pass cost | Same as display (no special exec semantics) | Drains every element, runs body once per element, discards results |

### Signature

`rust/src/structure_designer/nodes/foreach.rs`. `ForeachData` mirrors
`MapData` but has only an `input_type` (no `output_type` — the body's
return is discarded into `Unit`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeachData {
    pub input_type: DataType,   // T — the element type of `xs`
}
```

`calculate_custom_node_type` rebuilds the pin signature exactly like
`MapData` does, with `output_type` hard-coded to `Unit`:

```rust
fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
    let mut t = base_node_type.clone();
    t.parameters[0].data_type = DataType::Iterator(Box::new(self.input_type.clone()));
    t.parameters[1].data_type = DataType::Function(FunctionType {
        parameter_types: vec![self.input_type.clone()],
        output_type: Box::new(DataType::Unit),
    });
    t.output_pins = OutputPinDefinition::single_fixed(DataType::Unit);
    Some(t)
}
```

The body's declared return type is `Unit`. Because of the universal
`T → Unit` widening, a sub-network ending in *any* node — `export_xyz`
(natural fit, returns `Unit`), `print` (returns `String`, widened),
even a pure data computation (whose value is discarded) — type-checks as
a valid body. This keeps composition easy without weakening the signature
that documents intent.

`get_node_type` registers the default `input_type: Float` shape, parallel
to `map`'s default. `adapt_for_drag_source` peels `Iter[T]` / `Array[T]`
/ `T` from the dragged source, parallel to `map.adapt_for_drag_source`.

### Eval

```rust
fn eval<'a>(
    &self,
    network_evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,
    registry: &NodeTypeRegistry,
    _decorate: bool,
    context: &mut NetworkEvaluationContext,
) -> EvalOutput {
    // No `if !context.execute { return Unit; }` guard — `foreach`'s
    // output is `Unit`, so the central skip rule in the evaluator
    // already short-circuits this node entirely on display passes
    // (see "The execute flag → Central skip rule for Unit-returning
    // nodes"). When this `eval` runs, `context.execute` is true.

    let xs_walker = match network_evaluator.evaluate_arg_required(
        network_stack, node_id, registry, context, 0,
    ) {
        NetworkResult::Iterator(w) => w,
        NetworkResult::Array(items) => Walker::from_array(items),
        err @ NetworkResult::Error(_) => return EvalOutput::single(err),
        other => return EvalOutput::single(NetworkResult::Error(format!(
            "foreach: xs is not an iterator (got {})", other.to_display_string()
        ))),
    };

    let closure = match network_evaluator.evaluate_arg_required(
        network_stack, node_id, registry, context, 1,
    ) {
        NetworkResult::Function(c) => c,
        err @ NetworkResult::Error(_) => return EvalOutput::single(err),
        _ => return EvalOutput::single(NetworkResult::Error(
            "foreach: f is not a function".to_string()
        )),
    };

    let mut fe = match FunctionEvaluator::try_build(closure, registry) {
        Ok(fe) => fe,
        Err(msg) => return EvalOutput::single(NetworkResult::Error(format!("foreach: {}", msg))),
    };

    let mut walker = xs_walker;
    while let Some(elem) = walker.next(network_evaluator, registry, context) {
        if let NetworkResult::Error(_) = elem {
            return EvalOutput::single(elem);  // surface mid-stream errors
        }
        fe.set_argument_value(0, elem);
        let result = fe.evaluate(network_evaluator, registry, context);
        if let NetworkResult::Error(_) = result {
            return EvalOutput::single(result);  // first body error halts the loop
        }
        // Successful results are dropped — body return type is discarded into Unit.
    }

    EvalOutput::single(NetworkResult::Unit)
}
```

The display-pass perf property (zero work for a million-element
iterator upstream) **is not implemented in this `eval`** — it falls out
of the central skip rule applied to every Unit-returning node. The
`foreach.eval` arm above is only ever reached under `execute == true`.

Two semantic decisions worth calling out:

- **Fail-fast on first body error.** If the body returns
  `NetworkResult::Error` for one element, the loop halts and the error
  becomes `foreach`'s output. This matches how `fold` and `collect`
  behave with mid-stream errors today. Rationale: continuing past errors
  during a batch export silently produces a partial result set with no
  visible signal, which is the worst of all worlds. The user sees the
  first error in the Console / status bar, fixes it, re-executes.
- **No partial-progress reporting in phase 1.** A long-running `foreach`
  blocks the call. A future enhancement could stream per-element progress
  through the same Console panel mechanism, but that's out of scope here.

### Walker propagation: the `next()` signature change

`Walker::next` currently has signature
`(&mut self, evaluator, registry) -> Option<NetworkResult>` (per
`evaluator/AGENTS.md`). The internal `Walker::Map::next` and
`Walker::Filter::next` variants call `FunctionEvaluator::evaluate`. For
the execute flag to propagate into walker bodies (so an
`Iter[T]` produced by an upstream `map` whose body contains `export_xyz`
sees `execute == true` when foreach drains it), `Walker::next` must
also take the outer `NetworkEvaluationContext`:

```rust
pub fn next(
    &mut self,
    evaluator: &NetworkEvaluator,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,   // NEW
) -> Option<NetworkResult>;
```

All call sites of `Walker::next` (today: `fold.eval`, `collect.eval`, the
display-path auto-collect in `network_evaluator.rs`, and any future
`foreach.eval`) already have access to the outer context — they
forward it as `&mut`. The implementation of `Walker::Map::next` /
`Walker::Filter::next` then forwards the same `&mut` context to its enclosed
`FunctionEvaluator::evaluate(..., outer_context)` call, which is what enables
the print-buffer drain-back described under "The execute flag → Propagation
through `FunctionEvaluator`". Without `&mut` here, prints emitted from inside
a `Walker::Map` body would have nowhere to drain to and would be silently lost
on every walker step.

This is a mechanical change and is the same wiring fix as the
`FunctionEvaluator::evaluate` signature change described under "The
execute flag → Propagation through `FunctionEvaluator`" above. It is
listed once more in the implementation checklist for the walker layer
because the file is different.

### Why this is not an `activate: bool` in disguise

The earlier design considered an `activate: bool` input on each effect
node (rejected). The central skip rule looks superficially similar
("only run when a flag is true"). It is different in three ways:

1. The flag is not a *value* the user wires — it is a property of the
   evaluation pass set by an explicit user action. There is no
   "armed" state, no false→true→false toggling, no risk of leaving
   a node in the wrong mode.
2. The flag is set per call, not per node. A user can have a hundred
   `foreach` nodes in the graph; only the one they right-click +
   Execute on (and the transitive sub-evaluation that runs through it)
   sees `execute == true`.
3. The semantic is "I want this side-effect chain to happen once", not
   "I want this node to start producing values continuously". The
   one-shot framing is what makes Execute safe to invoke even on
   in-progress edits.

## Console panel (where prints land)

### Why a docked bottom panel

Three options were considered:

| Option | Pro | Con |
|---|---|---|
| Bottom-docked toggleable panel | Familiar (browser dev tools, IDE consoles). Doesn't compete with the node graph. Easy to ignore. | Steals vertical space when open. |
| Floating window | Always available; can be moved off-screen. | Window-management chrome on a debug feature is overkill; usually ends up covering the graph. |
| Per-node inline display | Visually colocated with the source. | Doesn't compose with iteration (N elements → N inline labels), can't show ordering across nodes. |

The bottom-docked panel wins on two counts: it composes with iteration (a
single chronological stream is the natural representation of N print events)
and it's the convention every developer already recognises.

### Panel UI

- **Toggle:** toolbar button labelled **Console** (with a small dot when new
  entries arrived since last open). Default keyboard shortcut: `Ctrl+`` `
  (backtick — same as VSCode/Chrome).
- **Layout:** docked to the bottom of the structure-designer view, ~25% of
  the viewport height by default, drag-resizable, collapse-to-zero when
  toggled off.
- **Contents:** a vertically-scrolling list of entries, newest at the bottom.
  Each row shows: `[HH:MM:SS]`  `network_name / node_label_or_id`  `text`.
  Entries from execute passes get a subtle ▶ marker so the user can tell
  them apart from display-pass prints.
- **Controls:** **Clear** button; an autoscroll toggle (default on).
- **Persistence:** none. The panel state (open/closed, log contents) is
  in-memory only. Closing the application clears the log.

### Where the log lives

Per-`StructureDesigner` instance, *not* per-evaluation-context:

```rust
// In StructureDesigner:
pub struct PrintLogEntry {
    pub timestamp: SystemTime,
    pub network_name: String,
    pub node_id: u64,
    pub node_label: String,    // from node.label or node_type.name
    pub text: String,
    pub from_execute: bool,
}

// New field:
pub print_log: Vec<PrintLogEntry>,
```

The log accumulates across evaluation passes — both execute passes and
normal display passes (when `execute_only == false`) push into it. A bounded
ring buffer is **not** introduced in phase 1; users hit Clear when the log
gets long. If we later see real OOM risk from runaway prints, capping at
~10000 entries is a one-line follow-up.

### How prints get there

The `print` node's `eval` cannot reach `StructureDesigner` directly — it
only has the evaluation context. Two simple mechanisms:

**Option A (chosen):** extend `NetworkEvaluationContext` with a
`print_buffer: Vec<PrintLogEntry>` field. Each eval pass starts with an
empty buffer; the orchestrator (the call site in `StructureDesigner` that
runs `generate_scene` / `execute_node`) drains it into the persistent
`print_log` after the pass completes. This keeps the eval layer free of
direct `StructureDesigner` references and matches the pattern already used
for `node_errors` and `node_output_strings`.

**Option B (rejected):** thread a `&mut StructureDesigner` into the
evaluator. Breaks the existing layering and would force every `eval` to
take it.

Adding `print_buffer` to the context costs nothing for evaluations that don't
contain `print` nodes — the `Vec` stays empty and is dropped at end-of-pass.

### Centralized drain — no per-call-site boilerplate

A naive read of "the orchestrator drains the buffer" is that every call site
that constructs a `NetworkEvaluationContext`, runs an eval, and then drops the
context must remember to drain `context.print_buffer` into
`StructureDesigner.print_log` first. There are many such sites today
(`generate_scene`, validation passes, per-node scene refresh, snapshot
machinery, the Phase-3 `execute_node`, future ones), and "remember to drain"
is a foot-gun: every missed site silently swallows prints, with no compile
error and no obvious user-visible failure mode.

To avoid this, the **construct + drain pair is owned by a single helper** on
`StructureDesigner`, and direct construction of `NetworkEvaluationContext`
inside `StructureDesigner` is replaced with calls to it:

```rust
impl StructureDesigner {
    /// Run an evaluation with a fresh context, then drain any prints the
    /// pass produced into `self.print_log`. Every code path in
    /// StructureDesigner that needs to evaluate goes through this helper —
    /// there is no direct `NetworkEvaluationContext::new()` call in this
    /// crate outside of it (and the FunctionEvaluator inner-context site,
    /// which drains back into its outer caller — see the FE section).
    fn with_eval_context<R>(
        &mut self,
        execute: bool,
        f: impl FnOnce(&mut NetworkEvaluator, &mut NetworkEvaluationContext) -> R,
    ) -> R {
        let mut context = NetworkEvaluationContext::new();
        context.execute = execute;
        context.use_vdw_cutoff = self.use_vdw_cutoff;   // or whatever the existing pattern is
        let result = f(&mut self.network_evaluator, &mut context);
        // Drain regardless of how f returned — prints accumulated up to a
        // mid-pass error are still worth showing to the user.
        let entries = std::mem::take(&mut context.print_buffer);
        self.print_log.extend(entries);
        result
    }
}
```

`generate_scene`, `execute_node`, validation, and any future eval-driving code
in `StructureDesigner` calls `self.with_eval_context(execute, |eval, ctx| { … })`
instead of constructing a context inline. The drain happens in exactly one
place; new call sites get it for free.

Two enforcement notes:

- **Grep discipline.** The Phase 2 PR adds a comment on
  `NetworkEvaluationContext::new()` documenting that the only legitimate
  callers are `with_eval_context` and `FunctionEvaluator::evaluate`.
  Reviewers grepping for `NetworkEvaluationContext::new(` in
  `rust/src/structure_designer/` outside those two sites have a one-shot
  audit.
- **Test sites are exempt.** Test crates routinely build a context directly
  to inspect intermediate state; the rule is about production
  `StructureDesigner` code paths only.

The `execute_node` orchestrator from "The execute flag → Triggering execute
mode from the UI" reuses this helper:

```rust
pub fn execute_node(&mut self, network_name: &str, node_id: u64) -> ExecuteResult {
    let pass_start = self.print_log.len();
    let result = self.with_eval_context(true, |evaluator, context| {
        // ... build network_stack, run evaluator.evaluate(...) ...
    });
    // Logs from THIS pass only — slice off everything appended by with_eval_context.
    let logs: Vec<APIPrintLogEntry> = self.print_log[pass_start..]
        .iter().map(Into::into).collect();
    ExecuteResult { ok: !matches!(result, NetworkResult::Error(_)),
                    error: match result { NetworkResult::Error(s) => Some(s), _ => None },
                    logs }
}
```

Note the `pass_start` slice rather than `take_print_log()`: `APIExecuteResult.logs`
returns only the prints produced by *this* execute pass, leaving any earlier
display-pass prints intact in `print_log` for the Console panel's normal
`take_print_log` polling cadence to pick up. (Without this slicing, the panel
would re-receive prior display-pass entries via `APIExecuteResult.logs` and
double-display them.)

### FFI

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn take_print_log() -> Vec<APIPrintLogEntry>;

#[flutter_rust_bridge::frb(sync)]
pub fn clear_print_log();
```

`take_print_log` drains and returns; Flutter calls it at a sensible cadence
(after each evaluation triggered through the model layer, plus on Console-
panel open). Drain-on-read prevents the buffer from growing indefinitely
when the panel is closed for long stretches *if* the user occasionally
opens it; if the panel stays closed forever, the user-visible behaviour is
identical to having no `print` nodes (the buffer just sits in memory).

`APIPrintLogEntry` mirrors `PrintLogEntry`, with `timestamp` flattened to
an `i64` epoch-millis (FFI-friendly).

## Out of scope

- **A user-facing `Unit` literal in `expr`.** Currently no use case —
  `Unit` values originate only from effect nodes.
- **A `discard` (T → Unit) node.** The universal field-level coercion
  covers every case the design needs; an explicit node would mostly
  duplicate it. Trivially additive if a real use case appears.
- **Per-element progress reporting from a long-running `foreach`.** Phase
  1 blocks the call; if needed later, stream per-element status into the
  Console panel using the same buffer mechanism.
- **Continue-on-error semantics for `foreach`.** Phase 1 fails fast on
  the first body error. A future `foreach_keep_going` variant or a
  per-node `on_error: Skip | Halt` property is a clean follow-up.
- **Re-fire on input change while execute mode is "armed".** No "armed"
  state exists — execute is one-shot. If the user wants to re-export they
  invoke Execute again. Worth being explicit because the rejected
  `activate: bool` design *did* re-fire.
- **Persistence of the Console log across sessions.** No use case for it as
  a debug feature.
- **Filtering / search in the Console panel.** Out of scope phase 1; users
  hit Clear and re-execute.
- **A "Run all" / batch trigger across the whole graph.** No user request
  for it. Right-click Execute on a single node covers the iterator case
  already.
- **Print-to-stdout option on the `print` node.** `println!` from inside
  Rust already shows in the dev console (per `rust/AGENTS.md` debugging
  notes); the new `print` node's whole point is the in-app Console panel.

## Implementation

The work is broken into four phases, each landing as an independently
mergeable, independently testable PR. The phases follow the project's
existing "Phase N complete" pattern (record types, multi-output pins,
atom_edit undo, iterators).

**Dependency graph:**

```
Phase 1 (Unit type)
    │
    ▼
Phase 2 (execute flag + propagation + central skip rule)
    │
    ├──────────────┬──────────────┐
    ▼              ▼              ▼
Phase 3 (export_xyz Unit-ification + foreach + Execute trigger)
    │
    ▼
Phase 4 (print node + Console panel)
```

Phase 4 strictly depends only on Phase 2; it can land before or after
Phase 3. The "print inside a `foreach` body" integration test is the
only Phase 4 surface that requires Phase 3 — if Phase 4 lands first, add
that test when Phase 3 lands.

---

### Phase 1 — Unit type

**Goal.** Add the `Unit` primitive end-to-end: type system, runtime
value, conversions, text format, FFI. No semantics yet — nothing
observable for users. Lays the foundation for the central skip rule.

**Depends on.** Nothing.

**Steps:**

1. [ ] Add `DataType::Unit` to
       `rust/src/structure_designer/data_type.rs`. Implement the
       conversion table from "The Unit type → DataType::Unit" in
       `can_be_converted_to` (universal `T → Unit`; reject `Unit → T`
       for `T ≠ Unit`; `Iter[Unit]` identity passthrough only).
       Update `is_abstract` (returns `false` — Unit is concrete).
       Update any exhaustive match on `DataType` (compiler-driven).
2. [ ] Add `NetworkResult::Unit` to
       `rust/src/structure_designer/evaluator/network_result.rs`.
       Wire `infer_data_type → DataType::Unit`,
       `to_display_string → "()"`. Extend `convert_to(_, &DataType::Unit)`
       to return `NetworkResult::Unit` for every non-Error source.
3. [ ] Update text-format lexer/parser/serializer to recognise the new
       type name. Update snapshot tests covering type printing.
4. [ ] FFI: add `Unit` variant to whichever API enum mirrors `DataType`
       (`APIDataTypeBase` or equivalent). Run
       `flutter_rust_bridge_codegen generate`. Update Flutter
       `DataType` rendering (label, wire color — pick something
       distinguishable from the existing types, e.g. dim grey).

**Tests** (`rust/tests/structure_designer/unit_type_test.rs`):

- `Float → Unit` conversion produces `NetworkResult::Unit`.
- `Iter[Float] → Unit` produces `Unit` (walker discarded without being
  drained).
- `Unit → Float` rejected at validation.
- `Function([Int] → String)` converts to `Function([Int] → Unit)`
  (covariant output rule + universal `T → Unit`).
- Round-trip: a network containing a `Unit`-typed pin saves and reloads
  through the text format intact (covered by snapshot test).

**Verify:**

- `cd rust && cargo fmt && cargo clippy && cargo test`.
- `dart format lib/` and `flutter analyze`.

---

### Phase 2 — execute flag + propagation + central skip rule

**Goal.** Land the entire eval-time mechanism: the `execute` field, the
`print_buffer` field, FunctionEvaluator and Walker context propagation,
and the central skip rule. After this phase, *no built-in node yet
returns Unit*, so the rule is dormant for users — but it is fully
testable via a synthetic Unit-returning test node.

**Depends on.** Phase 1.

**Steps:**

1. [ ] Add `pub execute: bool` to `NetworkEvaluationContext`
       (`rust/src/structure_designer/evaluator/network_evaluator.rs`),
       defaulted to `false` in `new()`.
2. [ ] Add `pub print_buffer: Vec<PrintLogEntry>` to
       `NetworkEvaluationContext`, defaulted to `Vec::new()` in `new()`.
       Define `PrintLogEntry` in the same file (or a new
       `print_log.rs` sibling module). The print node does not exist yet,
       but landing the field now keeps Phase 4 from re-touching the
       evaluator core.
3. [ ] Change `FunctionEvaluator::evaluate` to take `outer_context:
       &mut NetworkEvaluationContext` (note `&mut`), inherit `execute`
       and `use_vdw_cutoff` into the inner context, and at the end of
       the call drain `inner.print_buffer` into
       `outer_context.print_buffer` via `Vec::append` so prints from
       inner-body nodes aggregate into the per-pass log instead of
       being silently dropped. Remove the existing `// TODO: think
       about whether the context is ok this way?` comment.
4. [ ] Change `Walker::next` signature in
       `rust/src/structure_designer/evaluator/iterator_walker.rs` to
       take `context: &mut NetworkEvaluationContext`. Forward into
       `Walker::Map::next` and `Walker::Filter::next` so they can pass
       the same `&mut` reference to `FunctionEvaluator::evaluate` (this
       is what enables the print-buffer drain-back at every walker
       step). Update all call sites (`fold.eval`, `collect.eval`, the
       display-path auto-collect in `network_evaluator.rs`).
5. [ ] Update the Walker section in
       `rust/src/structure_designer/evaluator/AGENTS.md` to reflect the
       new `Walker::next` signature.
6. [ ] Implement the **central skip rule** in `evaluate_all_outputs`
       (the per-node entry point in
       `rust/src/structure_designer/evaluator/network_evaluator.rs`):
       before dispatching to `NodeData::eval`, if `!context.execute`
       and `node_type_registry.resolve_output_type(...)` returns
       `DataType::Unit` for **every** output pin of the node, return
       `EvalOutput::multi(vec![NetworkResult::Unit; n])` directly
       without calling `eval`. Add a small inline comment pointing
       readers to the design-doc rule.
7. [ ] Add a `with_eval_context` helper on `StructureDesigner`
       (signature in "Console panel → Centralized drain — no per-call-
       site boilerplate"). Migrate every existing
       `NetworkEvaluationContext::new()` call site inside
       `rust/src/structure_designer/` (outside the FunctionEvaluator
       inner-context construction) to go through it. Add a doc-comment
       on `NetworkEvaluationContext::new` listing the only two
       legitimate callers (`with_eval_context` and `FE::evaluate`) so
       grepping is a one-shot audit. The helper is dormant until Phase
       4 introduces `print_log` on `StructureDesigner`; for now it just
       drops the (always-empty) buffer at end-of-pass, but landing the
       wiring in Phase 2 means Phase 4 doesn't have to revisit every
       call site.

**Tests** (`rust/tests/structure_designer/execute_flag_test.rs`):

Add a small synthetic test fixture in the test crate: a
`CounterUnitNode` whose `eval` increments an `Arc<AtomicUsize>` counter
and returns `NetworkResult::Unit`, plus a `MixedOutputTestNode`
returning `(Float, Unit)`. Both register only in the test
`NodeTypeRegistry` (not the production registry).

- **Central skip rule, all-Unit:** display-pass evaluation of a
  `CounterUnitNode` leaves the counter at 0; an execute-pass
  evaluation increments it once.
- **Central skip rule, mixed-output guard:** display-pass evaluation
  of a `MixedOutputTestNode` *does* call `eval` (counter increments)
  and the Float pin produces the expected value downstream — the
  all-pins-Unit precondition correctly excludes mixed-output nodes
  from the skip.
- **FunctionEvaluator propagation:** `collect(map(range(0..N),
  body))` where `body` is a sub-network containing
  `CounterUnitNode`. Right-click Execute on `collect`. Counter is
  incremented N times, proving the FE call inside the map body
  inherits `execute=true` from the outer context.
- **Walker propagation, identity case:** `fold` over a `range(0..N)`
  with a body that wraps the iterator value through a `map(...)` whose
  body contains `CounterUnitNode`. Counter increments N times under
  Execute. Verifies that `Walker::Map::next` forwards the context all
  the way down.
- **Negative case:** the same `collect(map(range(0..N), body))` under
  a *display* pass leaves the counter at 0 (the central rule skips
  the inner `CounterUnitNode` because the FE inherits `execute=false`).

**Verify:**

- `cd rust && cargo fmt && cargo clippy && cargo test`.
- `dart format lib/` and `flutter analyze` (no Flutter changes
  expected — sanity check only).

---

### Phase 3 — `export_xyz` Unit-ification, `foreach` node, Execute trigger

**Goal.** Land the user-visible payoff. `export_xyz` returns Unit;
`foreach` exists; the user can right-click → Execute on a node from the
UI. After this phase, the motivating `product → foreach → export_xyz`
batch-export pipeline works end-to-end.

**Depends on.** Phase 2.

**Steps:**

1. [ ] Modify `nodes/export_xyz.rs::eval` to call `save_xyz`
       unconditionally (no `if context.execute` guard — the central
       rule guarantees `eval` is only invoked under execute). Change
       `output_pins` to
       `OutputPinDefinition::single_fixed(DataType::Unit)`. Return
       `EvalOutput::single(NetworkResult::Unit)` on success.
       Optionally extend `get_subtitle` to surface `(no file name)`
       when the stored `file_name` is empty and the input pin is
       unconnected, to recover the eager UX feedback that previously
       came from the runtime check inside `eval`.
2. [ ] Create `nodes/foreach.rs` (`ForeachData { input_type }` +
       `NodeData` impl + `calculate_custom_node_type` mirroring `map`'s
       but with output fixed to `Unit` + `adapt_for_drag_source`
       mirroring `map`'s + standard saver/loader). The `eval` body has
       no execute-flag short-circuit (central rule covers it); it
       drains the walker and runs the body per element with fail-fast
       error handling. Register in `nodes/mod.rs` and
       `node_type_registry.rs::create_built_in_node_types`.
3. [ ] Add `StructureDesigner::execute_node(network_name, node_id) ->
       ExecuteResult` (orchestrator: runs an evaluation pass through
       `with_eval_context(true, …)` from Phase 2 step 7 on the targeted
       node, returns the result + any errors). Use the `pass_start`
       slice pattern for `APIExecuteResult.logs` so the field returns
       only this pass's prints, not pre-existing ones from prior
       display passes (see "Console panel → Centralized drain").
4. [ ] FFI: `execute_node` in
       `rust/src/api/structure_designer/structure_designer_api.rs`.
       Define `APIExecuteResult` (no `logs` field yet — print log lands
       in Phase 4) in `structure_designer_api_types.rs`. Run
       `flutter_rust_bridge_codegen generate`.
5. [ ] Flutter: in the pin-row widget, hide the eye icon for `Unit`
       output pins (display-disabled).
6. [ ] Flutter: add **Execute** to the node right-click context menu in
       `lib/structure_designer/node_network/`. Wire to a new
       `executeNode` model method that follows the modal-placard +
       `endOfFrame` yield + sync FFI recipe documented under "The
       execute flag → UX during execution". Use `DraggableDialog` per
       project convention (`lib/AGENTS.md`) with `dismissible: false`
       and `barrierDismissible: false`. The FFI call must run inside
       `try { … } finally { Navigator.of(context).pop(); }` so the
       placard always dismisses — including on a thrown FFI error or a
       Rust panic surfaced through FRB. Without the finally, those
       failure paths would leave the user staring at an undismissable
       dialog. Surface errors via the existing snackbar / status-
       message mechanism *after* dismissal.

**Tests** (`rust/tests/structure_designer/execute_node_test.rs` for
`export_xyz` / `foreach`; reuse the `CounterUnitNode` fixture from
Phase 2):

- `export_xyz` does not write a file during a display refresh (temp
  dir, assert absence — the central rule prevents `eval` from
  running).
- `export_xyz` writes the file under Execute.
- `foreach` with `execute=false` does not evaluate either input pin
  (wire the `xs` input from `CounterUnitNode` upstream — assert the
  counter stays at 0 across multiple display refreshes). This is the
  headline perf test.
- `foreach` with `execute=true` drains all N elements and runs the
  body N times.
- `foreach` body containing `export_xyz` writes N files when
  executed. (Phase 2 already covered FE propagation in isolation;
  this is the integration with the real effect node.)
- `foreach` over `map(...)` whose body contains `export_xyz` writes N
  files. (Phase 2 covered Walker::Map propagation in isolation; this
  is the integration.)
- `foreach` body returning a non-Error value has its result discarded
  (output is Unit, no error visible at output).
- `foreach` body returning Error halts the loop and surfaces the
  error as `foreach`'s output.
- Snapshot test (insta) for the `foreach` node-type registration.

**Verify:**

- `cd rust && cargo fmt && cargo clippy && cargo test`.
- `dart format lib/` and `flutter analyze`.
- **Manual smoke:** build a `product` → `foreach` pipeline where the
  `foreach` body wires the variant record into a sub-network ending in
  `export_xyz` with a template-literal file path. First confirm that a
  normal display refresh costs nothing observable (no files written,
  no errors). Then right-click `foreach` → Execute and confirm files
  land in the expected distinct paths.

---

### Phase 4 — `print` node + Console panel

**Goal.** Add the debug observability layer: a `print` node that emits
to a per-CAD-instance log buffer, and a Flutter Console panel that
displays it. Strictly orthogonal to Phase 3 — no code dependency, only
shared FFI plumbing.

**Depends on.** Phase 2 (needs `context.print_buffer`). Optionally Phase
3 (only required for the "print inside a `foreach` body" integration
test below).

**Steps:**

1. [ ] Create `nodes/print.rs` (`PrintData { execute_only }` +
       `NodeData` impl + standard saver/loader using `serde_json`).
       Register in `nodes/mod.rs` and
       `node_type_registry.rs::create_built_in_node_types`. The `eval`
       body pushes to `context.print_buffer` when `!self.execute_only
       || context.execute`.
2. [ ] Add `print_log: Vec<PrintLogEntry>` to `StructureDesigner`. The
       drain itself is already centralized — Phase 2 step 7 wired every
       eval-driving site through `with_eval_context`, which calls
       `mem::take(&mut context.print_buffer)` and extends `print_log`
       at end-of-pass. Phase 4 only needs to flip that extend from a
       no-op (the field didn't exist) to the real append. **No
       per-call-site drain code added in this phase** — that would
       reintroduce the foot-gun the helper exists to eliminate.
3. [ ] Add `take_print_log` / `clear_print_log` methods on
       `StructureDesigner`. Extend the Phase-3 `APIExecuteResult` with a
       `logs: Vec<APIPrintLogEntry>` field populated from the
       `pass_start..` slice of `print_log` (so it returns only this
       execute pass's prints, not pre-existing display-pass entries).
4. [ ] FFI: `take_print_log`, `clear_print_log`, and `APIPrintLogEntry`
       in `rust/src/api/structure_designer/structure_designer_api.rs` /
       `structure_designer_api_types.rs`. Run
       `flutter_rust_bridge_codegen generate`.
5. [ ] Flutter: add a `ConsolePanel` widget docked at the bottom of the
       structure designer view. Toolbar toggle button (with new-entries
       dot) and `Ctrl+`` ` shortcut. Holds a list of `APIPrintLogEntry`
       in a `ChangeNotifier`-backed model; pulls via `take_print_log`
       after every evaluation.

**Tests** (`rust/tests/structure_designer/print_node_test.rs`):

- print with `execute_only=false` appends during display eval and
  during execute eval. (`print` returns `String`, so the central skip
  rule does not apply — this also serves as a regression guard against
  accidentally over-applying the rule to non-Unit nodes.)
- print with `execute_only=true` appends only during execute eval
  (display passes still call `eval`, but the per-node check inside
  `eval` gates the buffer push).
- print passthrough output equals the input string in both gating
  modes and across display vs. execute passes.
- print inside a `foreach` body appends N times when the foreach is
  executed. *(Requires Phase 3. Skip / mark `#[ignore]` if Phase 3 has
  not landed yet, and add when it does.)*
- Snapshot test (insta) for the `print` node-type registration.

**Verify:**

- `cd rust && cargo fmt && cargo clippy && cargo test`.
- `dart format lib/` and `flutter analyze`.
- **Manual smoke:** open the Console panel; insert a `print` node mid-
  expression in a network that gets evaluated on display; confirm the
  `print`'s text appears in the Console as the upstream value
  changes. Toggle `execute_only=true` on the same node; confirm the
  Console no longer updates on edits but does update when an Execute
  pass is triggered through that subgraph.
