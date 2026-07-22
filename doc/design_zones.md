# Zones: Inline Higher-Order Function Bodies

## Scope

This document designs **zones** — inline, bounded regions inside higher-order function (HOF) nodes that contain the body of the function-being-applied. Zones replace the current function-pin + `Closure` machinery for the user-visible programming model, while keeping the runtime evaluator close to what we already have.

Concretely, this branch replaces the way users write the body of a `map`, `filter`, `fold`, or `foreach` node. Instead of constructing a separate named subnetwork (or wiring an `expr` node into a function pin), the body lives **inside** the HOF node as a visually inline node-network region. Iteration-fed values come from inside-facing pins on the HOF's left edge; the body's return value flows into inside-facing pins on the HOF's right edge; values from the surrounding scope reach the body as ordinary wires crossing the zone boundary.

In scope:
- Conceptual model and UX of zones
- Data-model changes (`Node`, `Wire`, `NodeNetwork`)
- Evaluator changes — what reuses today's machinery and what's new
- The four HOF node types (`map`, `filter`, `fold`, `foreach`) rewritten on top of zones
- Old machinery that becomes dead weight (kept for the duration of the branch, removed when migration lands)
- Implementation phases for the Rust side, each ending in `cargo test` green

Out of scope:
- Details of the Flutter painter, layout, hit testing, and pan/zoom (deferred to a separate UI design)
- `.cnnd` migration from the function-pin world to zones (deferred — branch breaks compatibility freely)
- Text-format syntax for zones (deferred — covered separately once data model stabilizes)
- Named-subnetwork custom node types and their `parameter` / `return_node_id` mechanism — these remain unchanged. Zones are an addition for HOF bodies, not a replacement for user-defined named functions.
- `product` — it has no function-bearing pin and is not a HOF in the zone sense. It stays as a variadic iterator producer.

## Scope of this branch — build/test contract

All work in this document is **Rust-side only**. The verification gate is `cd rust && cargo test` passing — this is the contract every phase must satisfy at completion. The Flutter consumer code in `lib/` (outside auto-regenerated `lib/src/rust/`) is expected to be broken throughout this branch and is left broken; it will be reworked under a separate UI design after the Rust side stabilizes.

Where the "must compile" boundary sits:

| Must compile and tests must pass | Don't care (branch sandbox) |
|---|---|
| `rust/` workspace (`cargo build`, `cargo clippy`, `cargo test`) | `flutter analyze` |
| `rust/src/api/` — the FFI surface (it's Rust code) | `flutter build` |
| `flutter_rust_bridge_codegen generate` succeeds | `lib/` everything outside `lib/src/rust/` |
| Rust integration/roundtrip tests under `rust/tests/` | Flutter integration tests (`integration_test/`) |

The **Rust API layer** (`rust/src/api/structure_designer/`) is part of the Rust workspace and must stay compilable. When a phase's data-model change affects an API function signature (e.g. wire construction helpers), update the API function to the new shape — that's Rust-side plumbing work, part of the phase. Do **not** touch Flutter code that calls these APIs; FRB regenerates the bindings, Flutter consumers go red, we ignore them.

Insta snapshot tests will fail when serialization shapes change (Phase 1 and Phase 2). Use `cargo insta review` to accept the new snapshots as part of the phase.

## Motivation

Today, to map a `range` over a body that builds a `Blueprint` (anything `expr` can't express), the user must:

1. Create a top-level named subnetwork in the user-types panel.
2. Add a `parameter` node inside it for the iterated value.
3. Optionally add more `parameter` nodes for each captured value.
4. Wire the body. Set the return node.
5. Return to the parent network and place an instance of the custom subnetwork.
6. Wire it into `map.f`. The first parameter is silently treated as the hot slot; trailing parameters are captured at wire time.

This is friction-heavy enough that users either avoid HOFs or end up with a user-types panel polluted with one-off helper networks. It also produces a UX gap on the call-site: the hot vs. captured distinction on the instance's input pins is invisible (see `reports/programmability_improvements_2026-04-29.md` §C1/§D).

Zones collapse all of this into "draw the body inside the HOF, drag wires across the boundary for captures." The function-value abstraction (`DataType::Function`, `Closure`, partial application, the F→G conversion rule) is no longer user-visible — the body simply *is* a node-network region tied to its single call site.

## Concept

A **zone** is a region inside an HOF node containing a node-network body. The HOF node has four pin sets:

| Set | Position | Role | Wire direction |
|---|---|---|---|
| External inputs | Outer-left edge | HOF data inputs (`xs`, `init`, …) | Wires arrive from outside |
| External outputs | Outer-right edge | HOF data results (`Iter[U]`, accumulator, …) | Wires leave to outside |
| **Zone inputs** | Inner-left edge | Iteration values fed to the body each step | Wires *leave* the pin into the body |
| **Zone outputs** | Inner-right edge | Values the body must produce each step | Wires *arrive* at the pin from the body |

Zone-input pins are *sources* from the body's perspective — body-internal wires read from them. Zone-output pins are *destinations* from the body's perspective — body-internal wires terminate at them. The four pin sets together form the HOF's complete interface: external pins face the surrounding network, inner pins face the body.

The body region between left and right is a complete, evaluable `NodeNetwork`. Inside it:

- Nodes are placed and wired as in any other network.
- Wires consume iteration values by sourcing from one of the HOF's **zone-input pins**.
- Wires terminate at the HOF's **zone-output pins** to deliver the body's per-iteration return value(s).
- **Captures**: any wire whose source is a node *outside* the body (anywhere up the ancestor chain) is a capture. Captures are evaluated once at zone entry and pinned for the duration of all iterations of that zone-evaluation. Visually, captures are wires that cross the zone boundary; nothing special is required of the user.

The HOF owns its body — when the HOF is created, the body is created empty; when the HOF is deleted, the body and everything in it goes with it. The body is per-instance, not registry-keyed.

Each HOF declares its zone interface as part of its node type:

| HOF | Zone-input pins (inside-left) | Zone-output pins (inside-right) |
|---|---|---|
| `map` | `element: T` | `result: U` |
| `filter` | `element: T` | `keep: Bool` |
| `fold` | `acc: A`, `element: T` | `new_acc: A` |
| `foreach` | `element: T` | `out: Unit` |

For `foreach`, the body terminates at `out: Unit`. Because the universal `T → Unit` discard widening already exists, the body can wire any value-producing node into `out` and the value is discarded — typical use is an `export_xyz` chain or a `print`.

## UX

A new `map` placed in a network looks like a node with a translucent body region inside it. The body's left edge shows the `element` pin facing inward; the body's right edge shows the `result` pin facing inward. The body's interior is empty. The user adds nodes inside, wires them, and connects the body's terminal node into `result`. There is no separate "set return" gesture and no special-status nodes inside the body — every node inside the body is a normal node.

To capture a value from the surrounding scope (e.g. a `gap` parameter to drive the body's geometry), the user drags a wire from the outside-the-zone source pin to the desired inside-the-zone destination pin. The wire is drawn crossing the zone boundary. From the user's perspective there's nothing special about this wire — it just works. Internally, the wire is marked as a capture; the evaluator handles it.

Zones can nest: a `fold` whose body contains a `map` simply nests one zone region inside another. Capture wires can cross multiple zone boundaries (an inner `map` body referencing a value from the outer parent network is allowed). Each crossing is independent — there is no name shadowing because zones are not name-scoped.

A node inside a zone can be displayed (eye icon). The displayed snapshot is from the most recent iteration that produced a value (placeholder behavior: display the value from the *last* element drained from the stream). Per-iteration scrubbing is interesting future work but is not v1.

Promote-to-named (extract a zone body into a top-level named subnetwork for reuse) and the inverse demote-to-inline operation are valuable but deferred.

## Data model

### `Node` gains owned body and inside-facing destination wires

```rust
pub struct Node {
    pub id: u64,
    pub node_type_name: String,
    pub custom_name: Option<String>,
    pub position: DVec2,
    pub arguments: Vec<Argument>,                // EXISTING — external input wires
    pub data: Box<dyn NodeData>,
    pub custom_node_type: Option<NodeType>,
    pub zone: Option<Arc<NodeNetwork>>,          // NEW — the body, CoW-shared via Arc
    pub zone_output_arguments: Vec<Argument>,    // NEW — wires from the body to zone-output pins
}
```

Only HOF nodes populate `zone` and `zone_output_arguments`. For every other node both are `None` / empty.

The body is stored behind `Arc` rather than `Box` so that walkers can grab a reference for free at HOF eval time (`Arc::clone(node.zone.as_ref().unwrap())` is a refcount bump), and so that `Node::clone` — used by copy/paste and by every undo snapshot — stops being a deep walk through the body. All body mutation goes through a single accessor that does CoW:

```rust
impl Node {
    pub fn zone_mut(&mut self) -> Option<&mut NodeNetwork> {
        self.zone.as_mut().map(|arc| Arc::make_mut(arc))
    }
}
```

In synchronous evaluation the body's `strong_count` is 1 at edit time (walkers are constructed and consumed within one `eval` call), so `Arc::make_mut` returns the existing allocation without copying. The clone-on-write only triggers when an HOF is mutated while a long-lived reference is still outstanding — exactly the case where independence matters.

`zone_output_arguments` mirrors `arguments`: one `Argument` per zone-output pin, holding the wire(s) the body uses to deliver that pin's value. The source of each such wire is a node **inside this node's body** (one level deeper than the HOF itself).

Rationale for per-instance owned body:
- Lifecycle is automatic: delete the HOF → body deleted with it.
- No name collisions: zone bodies have no name; they are addressed by `(parent_network, hof_node_id)`.
- No pollution of the user-types panel.
- Copy/paste and undo of an HOF carry the body along naturally — `Node::clone` is an `Arc::clone` of the body (one refcount bump, no deep walk); the deep copy is paid lazily by `Arc::make_mut` the first time either copy is actually edited.

Cost: a few existing utility functions that walk all networks (validation, snapshot diffing, save/load) need to also walk into nodes' zones. Bounded and easily greppable.

### `NodeType` declares zone interface pins

```rust
pub struct NodeType {
    pub name: String,
    pub description: String,
    pub summary: Option<String>,
    pub category: NodeTypeCategory,
    pub parameters: Vec<Parameter>,              // EXISTING — external inputs
    pub output_pins: Vec<OutputPinDefinition>,   // EXISTING — external outputs
    pub zone_input_pins: Vec<OutputPinDefinition>,    // NEW — inside-facing left
    pub zone_output_pins: Vec<Parameter>,             // NEW — inside-facing right
    // ... unchanged fields ...
}
```

`zone_input_pins` reuses `OutputPinDefinition` because zone-input pins are sources (they produce values). `zone_output_pins` reuses `Parameter` because zone-output pins are destinations (they consume values). For non-HOF node types both vectors are empty.

A `NodeType` with non-empty zone pins is, by definition, a zone-bearing (HOF) type. There is no separate "HOF" flag — presence of zone pins *is* the marker.

The `calculate_custom_node_type` path (used by `map`'s `MapData::calculate_custom_node_type` etc. to specialize pin types from stored properties) extends in the obvious way: each HOF's `NodeData` writes its zone-input and zone-output pin types from its `input_type` / `output_type` properties at the same time it writes its external pins.

### `Argument` becomes the source-of-truth carrier of wire metadata

Today, the source of truth for pin connections is `Node.arguments[i].argument_output_pins: HashMap<u64, i32>` (`node_network.rs:61-72`): keyed by source `node_id`, value is the source pin index (`-1` for the legacy function pin, `0` for the primary output, ...). The `Wire` struct (`node_network.rs:107-113`) is a *derived view* used for selection state, deletion info, painter input, etc.

Two things make this map insufficient under zones:

1. We need a new dimension on each wire: source-pin kind (regular output vs. zone-input) and source scope depth (which ancestor scope the source lives in).
2. The map keys by `node_id` alone, but `node_id` is only unique within one `NodeNetwork`. With cross-scope wires (captures, zone-input references), a body destination could pull from two different sources that happen to share a numeric `node_id` across scopes — the map would silently collide.

Both push toward replacing the HashMap with a `Vec` of richer per-wire records:

```rust
pub struct Argument {
    pub incoming_wires: Vec<IncomingWire>,
}

pub struct IncomingWire {
    pub source_node_id: u64,
    pub source_pin: SourcePin,
    pub source_scope_depth: u8,            // 0 = local to this argument's scope, ≥ 1 = walk ancestors
}

pub enum SourcePin {
    NodeOutput { pin_index: i32 },         // pre-existing semantics (incl. legacy -1 dead-weight)
    ZoneInput { pin_index: usize },        // inside-facing source pin on a zone-owning node
}
```

`IncomingWire` is the storage shape; one entry per inbound wire on this argument pin. For wires that exist today (external-to-external, single-network), each `IncomingWire` has `source_pin = NodeOutput { pin_index }` and `source_scope_depth = 0` — no semantic change.

#### Two argument arrays per node

To accommodate zone-output destinations, `Node` carries two parallel argument lists (already named in the `Node` definition above):

| List on `Node` | Holds wires terminating at | The argument's scope (where its `source_scope_depth = 0` resolves) |
|---|---|---|
| `arguments` | External input pins (today's pins) | The destination node's containing network |
| `zone_output_arguments` | Inside-facing zone-output pins (HOF only) | The destination node's owned `zone` body |

The choice of list implicitly determines the wire's *evaluating scope*. A wire in `arguments` is evaluated against the network the destination node lives in (today's semantics, generalized by `source_scope_depth` to allow ancestor sources for captures). A wire in `zone_output_arguments` is evaluated against the HOF's owned body — `source_scope_depth = 0` for such wires means "the source is a body-internal node," which is the only legal case (body-return wires never reach further down).

#### `Wire` (the view) updated in parallel

The `Wire` struct stays as a cross-cutting view used by selection state, deletion info, painter, undo records, etc. Its fields grow to mirror `IncomingWire` and to carry which argument array it came from:

```rust
pub struct Wire {
    // Source side (mirrors IncomingWire)
    pub source_node_id: u64,
    pub source_pin: SourcePin,
    pub source_scope_depth: u8,
    // Destination side
    pub destination_node_id: u64,
    pub destination_argument_index: usize,
    pub destination_argument_kind: ArgumentKind,
}

pub enum ArgumentKind {
    External,        // sourced from destination's `arguments`
    ZoneOutput,      // sourced from destination's `zone_output_arguments`
}
```

`Wire` is constructed from an `IncomingWire` plus its containing argument index and kind when callers (selection, deletion, painter) need a self-contained record. The storage path always goes through `Argument.incoming_wires` — `Wire` is never stored on `Node`.

#### How wires represent each role under zones

| Wire role | Stored in | Source pin | Scope depth |
|---|---|---|---|
| Today's wires (in-network) | destination's `arguments` | `NodeOutput { pin_index }` | `0` |
| Capture (outside → inside body) | body-internal destination's `arguments` | `NodeOutput { pin_index }` | `≥ 1` |
| Iteration value (HOF zone-input → body node) | body-internal destination's `arguments` | `ZoneInput { pin_index }` | `≥ 1` (usually `1`; deeper for nested-zone references to an ancestor HOF — see capture rules) |
| Body return (body node → HOF zone-output) | HOF's `zone_output_arguments` | `NodeOutput { pin_index }` | `0` (relative to body scope) |

All four cases are uniformly expressed by the same `IncomingWire` shape — the difference is which argument list the wire lives in and which combination of `source_pin` / `source_scope_depth` it carries.

### `NodeNetwork` (body) requires no structural change

The body is a regular `NodeNetwork`. Its `nodes`, `displayed_nodes`, `selected_node_ids`, `valid`, `validation_errors`, `camera_settings` all behave as in any other network.

Two fields are **unused** in zone bodies:
- `return_node_id` — replaced by the HOF's `zone_output_arguments`. Always `None` for a zone body.
- `node_type` — the body has no callable type; nothing outside references it as a node type. Set to a synthetic placeholder.

These are wasted bytes on every zone body but the alternative — a parallel "body network" type — duplicates `NodeNetwork`'s machinery for no real gain. Accept the dead fields.

## Evaluator changes

### Reuse from today's machinery

| Existing piece | Reused for zones? |
|---|---|
| `NetworkEvaluator::evaluate` and `evaluate_all_outputs` | Yes, unchanged for body evaluation |
| `NetworkStackElement` and the `network_stack: &[NetworkStackElement]` pattern | Yes — the HOF pushes a stack element when entering its body, same as today's custom-subnetwork dispatch (`network_evaluator.rs:1020-1067`) |
| `NetworkEvaluationContext` | Yes, unchanged except for two new scratch slots (see below). `execute` and `use_vdw_cutoff` already propagate into inner-body evaluations; the same pattern carries over |
| `Walker` and the iterator runtime | Yes, unchanged at the shell. `Walker::Map` / `Walker::Filter` continue to drive per-element evaluation, but they now drive a *zone body* rather than a closure |
| The central skip rule for `Unit`-returning nodes | Yes, unchanged — applies to a `foreach` zone exactly as it does today |
| `evaluate_arg_required` / `evaluate_arg` | Yes, but extended to honor `WireSource::ZoneInput` and `source_scope_depth` |
| Per-node `eval` implementations (sphere, cuboid, expr, …) | Yes — fully unchanged. They don't know or care whether they're inside a zone |

### What's new

1. **HOF eval bodies are rewritten.** `MapData::eval`, `FilterData::eval`, `FoldData::eval`, `ForeachData::eval` no longer extract a `Closure` from a function pin and build a `FunctionEvaluator`. The order of operations is load-bearing:

   a. **Resolve the HOF's external inputs first** (`xs`, `init`, …) — runs in the HOF's containing network scope, *before* pushing the body. (`xs`-feeding nodes may reference enclosing-HOF zone-input pins; those must resolve against the stack as-is, not against a stack that has this body pushed.)
   b. Look up the HOF's owned body (`node.zone.as_ref()`) and push `NetworkStackElement { node_network: body, node_id: <hof_id> }` onto the stack.
   c. **Pre-evaluate captures** of this body (with the body now on the stack, so `source_scope_depth` walks land correctly). Build an `Arc<HashMap<CaptureKey, NetworkResult>>`. Captures evaluated here **once** for this HOF invocation.
   d. For `map` and `filter`: construct `Walker::Map { source, body: Arc<NodeNetwork>, captures: Arc<HashMap<…>>, hof_node_id }` / `Walker::Filter { … }`. Pop the body off the stack. Return the walker. The body and captures travel via the walker; subsequent iterations re-push the body in `next()` and use the cached captures.
   e. For `fold` and `foreach`: leave the body on the stack, iterate eagerly. **Push** a fresh frame onto the scope-stack at `current_zone_input_values[hof_id]` (see point 3); per element, write the iteration's values into that top frame and evaluate `zone_output_arguments[i]`. After the stream is drained, **pop** the frame and pop the body.

   Inside a `Walker::Map`/`Walker::Filter` per-element step (`next()`), the work is: push body, **push a fresh frame** onto `current_zone_input_values[hof_id]` carrying the element, evaluate `zone_output_arguments[i]` against the body, **pop the frame**, pop body. The push/pop straddles each `next()` call: when the walker is drained inside an outer iterating HOF whose `hof_id` happens to collide numerically (a routine occurrence — see point 3), the outer's frame sits underneath the walker's frame for the duration of the step and is exposed at the top again the moment the step returns. Captures sit in the walker's `Arc` and are consulted via the context (see point 4).

2. **`evaluate_arg` learns two new arms.** It now iterates `argument.incoming_wires` and resolves each `IncomingWire`:

   ```rust
   for incoming in &argument.incoming_wires {
       // First: if this wire is a capture (computed at body entry, frozen for
       // the duration of this body's iteration), serve it from the cache.
       if let Some(value) = context.captured_source_values.get(&CaptureKey::from(incoming)) {
           // ... use cached value ...
           continue;
       }
       match incoming.source_pin {
           SourcePin::NodeOutput { pin_index } => {
               // Local source (or an unguarded ancestor source). Walk
               // `source_scope_depth` levels up the stack and evaluate.
               let depth_idx = network_stack.len() - 1 - (incoming.source_scope_depth as usize);
               evaluator.evaluate(&network_stack[..=depth_idx], incoming.source_node_id, pin_index, ...)
           }
           SourcePin::ZoneInput { pin_index } => {
               // Iteration value of THIS body's enclosing HOF — read from
               // the top of the HOF id's scope-stack in
               // `current_zone_input_values` (see point 3 — the value is a
               // `Vec<Vec<NetworkResult>>`, a stack of per-iteration frames,
               // not a single frame). References to a *non-immediately-
               // enclosing* HOF are captures (served above).
               context.current_zone_input_values[&incoming.source_node_id]
                   .last()
                   .expect("zone-input read with no active iteration frame")
                   [pin_index]
                   .clone()
           }
       }
   }
   ```

   `evaluate_arg` is the single dispatch point — every node's `eval` calls it to resolve inputs, so every node naturally picks up the new behavior with no per-node changes.

3. **Iteration-value scope-stacks on `NetworkEvaluationContext`, keyed by HOF.**

   ```rust
   pub struct NetworkEvaluationContext {
       // ... existing fields ...
       pub current_zone_input_values: HashMap<u64, Vec<Vec<NetworkResult>>>,
       //                              hof_node_id ↦ stack of iteration frames
       //                              (each frame: values per zone-input pin)
   }
   ```

   The value is a **stack** of per-iteration frames (each frame is one `Vec<NetworkResult>` indexed by zone-input pin), not a single frame. Reads always consult `last()` — the top of the stack is the innermost iterating HOF with that id. The stack shape is load-bearing because **`hof_node_id` is not globally unique**: `next_node_id` is per-network and starts at 1 (`node_network.rs:329`), so an outer HOF in one network and an inner HOF in another network commonly share a numeric id. This is the same uniqueness gap that drove `Argument` to grow `source_scope_depth` (§"`Argument` becomes the source-of-truth carrier of wire metadata"); the symmetric fix here is to scope-stack the live-lookup map. Without it, a lazy walker for an inner HOF whose id collides with an outer iterating HOF's id would silently overwrite the outer's iteration value when `next()` runs and corrupt the outer iteration after the step returns.

   Provide thin helpers on the context so call sites never reach into the inner Vec directly:

   ```rust
   impl NetworkEvaluationContext {
       fn push_zone_input_frame(&mut self, hof_id: u64, frame: Vec<NetworkResult>);
       fn pop_zone_input_frame(&mut self, hof_id: u64);
       fn current_zone_input(&self, hof_id: u64, pin_index: usize) -> &NetworkResult;
       // ^ panics in debug if the stack is empty; in release returns `Error`
       //   via the validation invariant in §"Validation".
   }
   ```

   When an HOF begins iterating it `push_zone_input_frame`s a fresh frame onto its id's stack and **mutates that top frame** per element (a small helper `write_zone_input_pin(hof_id, pin_index, value)` is convenient for `fold`'s acc-then-element update); when iteration completes it `pop_zone_input_frame`s. For lazy walkers (`Walker::Map` / `Walker::Filter`) the push/pop straddles each `next()` call (one frame per element), so an interleaved drain inside an outer body's iteration leaves the outer's frame untouched on the stack and re-exposed at the top after the step returns. Reads from any depth land on the most-recently-pushed frame for that id, which is exactly the immediately-enclosing HOF's iteration values (deeper-than-immediate references go through the capture cache and never live-lookup — see point 4).

   This is the direct analog of today's `FunctionEvaluator` machinery — each FE owns a separate `_tmp_` network so nested FEs have independent state by construction (`function_evaluator.rs:72-141`). Scope-stacking gives the same isolation in a single shared context, paying one stack push + stack pop per body iteration instead of an FE construction.

4. **Capture pre-evaluation.** At body entry, the HOF walks all wires inside the body that are *captures* — defined as: wire's source is anywhere outside this body. Equivalently: `source_scope_depth > 0`, **except** for `WireSource::ZoneInput` references to this body's own enclosing HOF (those are iteration values, not captures — they vary per iteration of this body).

   Each capture is evaluated *once*, cached, and used unchanged for every iteration of this body. The cache key identifies the **source side** (so multiple body wires consuming the same upstream pin share one entry):

   ```rust
   pub captured_source_values: Arc<HashMap<CaptureKey, NetworkResult>>,

   pub struct CaptureKey {
       pub source_node_id: u64,
       pub source_scope_depth: u8,
       pub source_pin: SourcePin,    // `NodeOutput { pin_index }` or `ZoneInput { pin_index }`
   }
   ```

   The map is stored behind `Arc` so the lazy-walker per-`next()` swap (see §"Sub-context pattern for body evaluation") is three pointer-sized ops instead of a HashMap clone. Pre-evaluation builds a plain mutable `HashMap`, then seals it: `let captures = Arc::new(captures);` after the last insert. All read paths consult `context.captured_source_values.get(...)` — `Arc<HashMap>` derefs to `&HashMap` transparently, so the dispatch in §"What's new" point 2 is unchanged.

   Non-zone evaluation contexts share one empty allocation:

   ```rust
   static EMPTY_CAPTURES: LazyLock<Arc<HashMap<CaptureKey, NetworkResult>>> =
       LazyLock::new(|| Arc::new(HashMap::new()));
   ```

   `CaptureKey::from(incoming)` is the natural projection from an `IncomingWire`. When body-internal `evaluate_arg` is asked to resolve an incoming wire and the wire's source identifies a capture (per the rule above), it returns the cached value rather than re-evaluating the upstream source. Captures of `ZoneInput` sources are legal — an inner body referencing an outer ancestor HOF's iteration values is a perfectly normal capture, pre-evaluated against the outer's current iteration values at inner-body-entry. Inner captures are recomputed at each entry of the inner body (which itself happens once per outer-body iteration).

   Semantics match today's `Closure::captured_argument_values`: once-per-call, snapshot at the moment the body is entered.

5. **Zone-output evaluation.** After setting the current iteration's zone-input values, the HOF reads each zone-output pin's value via a small helper that runs `evaluate_arg` against the HOF's `zone_output_arguments[i]` in the body's stack scope. Each such wire's source is a body-internal node, so resolution is the normal local-source path; nothing exotic happens here.

   ```rust
   fn evaluate_zone_output(
       &self,
       network_stack: &[NetworkStackElement],   // top = body
       hof_node_id: u64,
       zone_output_index: usize,
       registry: &NodeTypeRegistry,
       context: &mut NetworkEvaluationContext,
   ) -> NetworkResult
   ```

   The helper exists for readability — the dispatch is just "look up the HOF in `network_stack[len-2]`, read its `zone_output_arguments[zone_output_index]`, run `evaluate_arg` against the body."

### Sub-context pattern for body evaluation

**Eager HOFs** (`fold`, `foreach`) create an inner `NetworkEvaluationContext` for the body's iterations — mirroring the FE pattern today (`function_evaluator.rs:179-194`). The inner context **inherits**:
- `execute`, `use_vdw_cutoff` (already inherited by FE today)
- `current_zone_input_values` (cloned from outer — ancestor HOFs' scope-stacks come along intact; this body pushes its own frame onto the `hof_id` stack at iteration start, mutates the top frame per element, and pops at iteration end)

And gets **fresh**:
- `captured_source_values` (this body's pre-evaluated captures only)
- `node_errors`, `node_output_strings`, `selected_node_eval_cache` (per-pass scratch, scoped to this body)

`print_buffer` is drained back into the outer context at end-of-call, same as today.

**Lazy walkers** (`Walker::Map` / `Walker::Filter`) cannot afford a fresh inner context per `next()` — that's one HashMap allocation per element across a potentially long stream. They run each `next()` directly against the **caller's** context, relying on two strict save-and-restore disciplines to keep the caller's state intact across the step:

1. **`current_zone_input_values[hof_id]`**: push a fresh frame at the start of `next()`, pop at the end. The scope-stack shape in §"What's new" point 3 is what makes this safe under the routine case of `hof_id` colliding with an outer iterating HOF's id.
2. **`captured_source_values`**: swap the caller's `Arc<HashMap<…>>` field with the walker's `captures: Arc<HashMap<…>>` for the duration of the step, restore at end. Both sides share the same `Arc<HashMap<…>>` type (§"Capture pre-evaluation"), so the swap is three pointer-sized ops: `std::mem::replace` saves the caller's `Arc`, `Arc::clone(&walker.captures)` (one refcount bump) goes in, the saved `Arc` is restored at end. No HashMap clone, no allocation, regardless of capture-set size.

   RAII'd via a `CapturesGuard` so early returns can't leak:

   ```rust
   struct CapturesGuard<'a> {
       ctx: &'a mut NetworkEvaluationContext,
       saved: Arc<HashMap<CaptureKey, NetworkResult>>,
   }
   impl<'a> CapturesGuard<'a> {
       fn swap_in(ctx: &'a mut NetworkEvaluationContext,
                  new: Arc<HashMap<CaptureKey, NetworkResult>>) -> Self {
           let saved = std::mem::replace(&mut ctx.captured_source_values, new);
           Self { ctx, saved }
       }
   }
   impl Drop for CapturesGuard<'_> {
       fn drop(&mut self) {
           self.ctx.captured_source_values =
               std::mem::replace(&mut self.saved, EMPTY_CAPTURES.clone());
       }
   }
   ```

`execute`, `use_vdw_cutoff`, and `print_buffer` are *not* swapped: they're inherited from the caller's context as-is. This matches today's FE behavior — `execute` propagates so a `print` or `export_xyz` nested inside the body still fires under Execute, and `print_buffer` is the caller's per-pass buffer so prints from inside the step land in the right log.

The scope-stack discipline makes save/restore of `current_zone_input_values` recursion-safe at the eager call site too. When an inner HOF nested inside an eager body runs, *its* inner context starts fresh (its `captured_source_values` is empty, its `node_errors` is empty) and doesn't disturb this body's captures; the cloned `current_zone_input_values` already carries the outer's frames at the bottom of each stack, and the inner body pushes its own frame on top.

### Iterator integration

`Walker::Map` and `Walker::Filter` today carry a `FunctionEvaluator`. They will be restructured to carry `body: Arc<NodeNetwork>` plus `captures: Arc<HashMap<CaptureKey, NetworkResult>>` and `hof_node_id: u64`.

Per-element hot path: construct a `CapturesGuard::swap_in(ctx, Arc::clone(&self.captures))` (one refcount bump — see §"Sub-context pattern for body evaluation"), push body, **push a fresh frame** onto `current_zone_input_values[hof_id]` carrying the element, evaluate `zone_output_arguments[0]`, **pop the frame**, pop body. The guard restores the caller's captures on drop, so early-return on error is safe. Captures are not recomputed per element; they sit in the walker as a sealed `Arc<HashMap<…>>` and are made visible to the context for the duration of the step via one Arc swap. The push/pop on `current_zone_input_values[hof_id]` is what keeps a numerically-colliding outer iterating HOF's frame from being clobbered when this walker is drained inside it — see §"What's new" point 3.

Cloning is **strictly cheaper than today**. Per `evaluator/AGENTS.md` "Invariant 2", today's `Walker::Map`/`Filter` clone cost is "recursive source clone + `FunctionEvaluator::clone` (clones the inner ad-hoc NodeNetwork)" — that FE clone is the expensive part. Under zones it becomes "recursive source clone + two Arc bumps." All zone walker clones share the same body and captures.

Construction-time errors (today's `FunctionEvaluator::try_build` returning an `Err` on missing source network/node) translate to: body-validation errors that surface at the HOF's `eval` entry, returning `EvalOutput::single(NetworkResult::Error(_))` exactly as `try_build` does today. Examples: a zone-output pin has no incoming wire; a capture wire's parent-scope source has been deleted.

The `Clone` independence invariant for `Walker` (`evaluator/AGENTS.md`, "Invariant 2") carries over: body and captures are `Arc`-shared (cheap immutable share, correct under clone), per-walker iteration state is owned.

### Worked example — when do captures fire?

Two reference points worth pinning down explicitly:

**Chained pipeline** `range → filter → map → collect`. `collect` is consumed once → `map.eval` runs once → resolves `xs` by running `filter.eval` once → resolves `xs` by running `range.eval` once. Each HOF's captures are pre-evaluated exactly once during its own `eval`. `collect` then drains the `Walker::Map`, which drains the underlying `Walker::Filter`. Per-element work uses the cached captures of each layer.

**Nested HOF** — outer `fold` whose body contains inner `fold` whose body captures a parent-network value `K` (depth = 2 from the inner body): the inner `fold.eval` runs **once per outer iteration**, because it lives in the outer fold's body. Each invocation pre-evaluates `K` once for the inner body's captures. So `K` is evaluated N times for an outer fold over N elements — once per outer iteration. This is the correct semantics ("captures snapshot at the moment the body is entered") and matches today's FE behavior exactly: if `K` doesn't depend on the outer iteration value, the work is technically redundant but cheap; if it did depend on the outer iteration value, each outer iteration would correctly see fresh captures.

The take-away: capture pre-evaluation happens at the granularity of *HOF eval invocation*, not per-element of the HOF itself. An HOF inside another iterating HOF's body is invoked per outer iteration, and its captures are pre-evaluated per outer iteration.

### Validation

Three new validation checks for zones:

1. **Every zone-output pin has at least one incoming wire** in the HOF's `zone_output_arguments[i].incoming_wires`. A zone body that doesn't deliver every promised output is invalid — the HOF cannot produce a return value otherwise. Reports as a validation error on the HOF node.

2. **Every capture wire's source resolves.** For every `IncomingWire` inside any body with `source_scope_depth > 0` and `source_pin = NodeOutput`, walking `depth` levels up the stack must land in a network that contains `source_node_id` with a compatible output pin.

3. **Every zone-input reference is valid.** `IncomingWire`s with `source_pin = ZoneInput { pin_index }` must have a `source_node_id` and `source_scope_depth` that together resolve to an HOF on the stack, and `pin_index < num_zone_input_pins` of that HOF. Depth ≥ 1 (you cannot reference a sibling node's zone-input pin from outside that HOF's body). Depth > 1 is legal and corresponds to "this inner body wants the outer HOF's iteration value" — those wires are captures, pre-evaluated at inner-body-entry against the outer's then-current iteration value.

4. **No `parameter` node inside a body** (added later, issue #417). A `parameter` node declares an input pin of the *enclosing network*; a body has no interface, so the node is meaningless there — body inputs are zone-input pins and captures. Reports as a **non-blocking** error on the body node (the eval guard in `ParameterData::eval` localizes the failure, so a legacy file keeps rendering the rest of its network). Every authoring path refuses the state up front, so this check only ever fires on hand-authored or pre-#417 `.cnnd`. The rule has one definition, `node_type_registry::allowed_in_zone_body`.

Existing checks (acyclic-DAG within a network, type compatibility, etc.) apply unchanged to the body. Type compatibility extends naturally: the source type for `WireSource::ZoneInput { i }` is the `i`-th zone-input pin's declared type; the destination type for `ArgumentKind::ZoneOutput` is the corresponding `zone_output_pin`'s declared type. The `can_be_converted_to` rules don't change.

### Display

A displayed node inside a body should produce viewport output. For `fold`/`foreach`, the body is evaluated eagerly so the displayed pin shows the *last* iteration's value. For `map`/`filter`, the displayed pin produces output only when the stream is being drained — typically because a downstream `collect` is forcing it; otherwise the display pass short-circuits.

The auto-collect-with-cap mechanism for `Iter[T]` display (256-element cap with subtitle hint, `doc/design_iter_display_via_collect.md`) applies unchanged.

## Reuse map (summary)

**Reused unchanged:**
- `NodeNetwork`, including `displayed_nodes`, validation state (note: `return_node_id` exists but is unused by zone bodies)
- `NetworkEvaluator`, `evaluate`, `evaluate_all_outputs`
- `NetworkStackElement` and the `network_stack: &[…]` pattern
- `Walker`, the iterator runtime, the `Iter[T]` data type
- The central skip rule for `Unit`-returning nodes
- Per-node `eval` implementations across all non-HOF nodes
- `NodeType`, `OutputPinDefinition`, `Parameter`, abstract types, polymorphic resolution, the rest of the type system
- `parameter` node, used for *named* subnetworks (zones do not use it)

**Reused with small extensions:**
- `NodeType`: new `zone_input_pins` and `zone_output_pins` fields, empty for non-HOF nodes
- `Node`: new `zone: Option<Arc<NodeNetwork>>` (CoW-shared via `Arc::make_mut`; cheap `Node::clone`) and `zone_output_arguments: Vec<Argument>` fields, empty for non-HOF nodes
- `Argument`: replace `argument_output_pins: HashMap<u64, i32>` with `incoming_wires: Vec<IncomingWire>` (new per-wire storage type carrying `source_node_id`, `source_pin`, `source_scope_depth`). Existing wires up-convert with no semantic change
- `Wire` (the view): grows `source_pin`, `source_scope_depth`, `destination_argument_kind` fields in parallel
- `NetworkEvaluationContext`: new `current_zone_input_values` (HOF id ↦ **scope-stack** of per-iteration frames; push/pop discipline straddles each body iteration) and `captured_source_values: Arc<HashMap<CaptureKey, NetworkResult>>` (Arc-shared so the lazy-walker swap is a refcount bump) fields
- `evaluate_arg` / `evaluate_arg_required`: two new arms handling `SourcePin::ZoneInput` and non-zero `source_scope_depth`; iterates `incoming_wires` instead of the old map
- Validation pass: three new checks (see "Validation")
- Serialization, snapshot/undo, copy/paste: descend into `Node.zone` and `Node.zone_output_arguments`; the storage shift of `Argument` is a format change that the branch absorbs freely

**New from scratch:**
- Rewritten `eval` for `map`, `filter`, `fold`, `foreach`
- Zone-aware Walker variants replacing the FE-carrying ones
- Capture pre-evaluation logic at body entry
- `evaluate_zone_output` helper
- Zone-body validation checks

**Dead weight kept for the branch, removed at migration time.** None of this is reused by zone evaluation — the zone path goes straight through `NetworkEvaluator` against a real `NodeNetwork` (`Node.zone`), with no synthetic temp network and no `FunctionEvaluator` wrapping.

- `FunctionEvaluator` (`function_evaluator.rs`) — entirely deleted. Its file-level comment flags it as a hack ("builds a little node network so that nodes can be evaluated in a node network context"); zones make that hack unnecessary because the body *is* a real network. The per-call `set_argument_value` mutation of `ValueData` is replaced by writing to `context.current_zone_input_values[hof_id]`. The inner-context construction pattern (selective field inheritance, `print_buffer` drain-back) is the same pattern as before but is now built directly inside HOF eval rather than wrapped in FE.
- `Closure` struct (`network_result.rs`) and the `NetworkResult::Function(Closure)` variant — closures were purely the in-flight currency between "request output pin −1 of a node" (closure construction site) and "FE consumes a closure" (FE construction). Both endpoints disappear.
- `DataType::Function(FunctionType)` and `FunctionType` — no function-typed pins exist in the user-visible type system any more, and nothing internal uses them.
- The F→G conversion rule in `data_type.rs:378-414` (trailing-extras partial application — source's first `K` params match the destination's, with the trailing `N-K` source params filled by captures) — partial application is now expressed as explicit boundary-crossing wires; the rule has no consumer.
- The `output_pin_index == -1` branch of `evaluate()` (`network_evaluator.rs:1142-1162`) — the only construction site for `Closure` values.
- The whole "output pin index −1 is special" convention. Pin index space returns to "non-negative for outputs."
- Function-pin rendering in Flutter (`node_widget.dart:799-804`, `node_network_painter.dart:149-152`) and the `NODE_VERT_WIRE_OFFSET_FUNCTION_PIN` constant.

**Survives the cleanup**, despite being adjacent: the `value` node and `ValueData` (`nodes/value.rs`). FE used `value` nodes to inject captured args into its temp network, but `value` is also a user-facing math/programming node for inline literals. It stays for its standalone authoring use.

These deletions don't actively harm anything during the branch as long as no node type produces or consumes a `Closure`. Under zones, no HOF does. Cleanup is a follow-up after migration lands.

## Open questions

1. **Promote-to-named / demote-to-inline.** UX gesture to extract a zone body into a top-level named subnetwork (and vice versa) is high-value but not blocking. Defer.

2. **Per-iteration display of inner-body nodes.** Showing only the last iteration's value is sufficient for v1. A richer "scrub through iterations" experience is interesting future work.

3. **Nested zones and capture depth.** Three levels deep is the realistic upper bound (`fold` containing `map` containing `filter`). The data model permits arbitrary depth; the editor may want to cap rendering complexity but the evaluator does not need a limit.

4. **Text-format syntax.** Deferred. The current `f: @body` syntax for function-pin references stops being relevant. Some way to express a zone body inline in the text format is needed but doesn't block runtime work.

5. **`expr` and the function pin.** An `expr` node with a free parameter is currently a function value (via the top-right function pin). In the zones world, function pins are dead weight on the user-visible side. Users who want a one-line body inside a zone simply place an `expr` node inside the zone. Conceptual simplification; nothing else changes for `expr`.

## Implementation phases

Each phase below is self-contained: an AI agent (or human) can pick one up with no prior context beyond this document and the prior phase's state. Every phase ends with `cd rust && cargo test` passing and `cargo clippy` clean. Phases are strictly sequential — later phases assume earlier ones have landed.

The branch starts on `zones` from `main`. Baseline: ~2005 Rust tests passing, function-pin/closure machinery present and used by `map`/`filter`/`fold`/`foreach`.

### Phase 1: Argument storage refactor — semantic-preserving

**Goal.** Replace `Argument.argument_output_pins: HashMap<u64, i32>` with `Argument.incoming_wires: Vec<IncomingWire>`. Extend the `Wire` view struct with `source_pin: SourcePin` and `source_scope_depth: u8`. No semantic change — every existing wire upconverts to `IncomingWire { source_node_id, source_pin: NodeOutput { pin_index }, source_scope_depth: 0 }`.

**Scope.**
- `rust/src/structure_designer/node_network.rs` — define new types (`IncomingWire`, `SourcePin`); change `Argument.argument_output_pins` to `Argument.incoming_wires: Vec<IncomingWire>`; update `Wire` struct (add `source_pin`, `source_scope_depth`; leave `destination_argument_kind` for Phase 2).
- Every caller of `argument_output_pins` (grep the whole `rust/` tree). Notable callsites: `node_network.rs:271-323` (dependency walks), `network_validator.rs`, `network_evaluator.rs::evaluate_arg`, undo snapshot code, serialization, the `selection_factoring.rs` extractor.
- Every callsite that constructs a `Wire` literal. Add a `From<&IncomingWire>` impl or similar helper that constructs a `Wire` given the IncomingWire plus its destination context.
- `rust/src/api/structure_designer/` — wire-construction API functions (`connect_nodes`, etc.) update to the new shape.
- Serialization — the `.cnnd` JSON shape changes here. This branch breaks compatibility freely; just regenerate fixtures.
- Snapshot tests under `rust/tests/structure_designer/node_snapshot_test.rs` — re-accept via `cargo insta review`.

**Tests.** No new tests. The existing ~2005-test suite is the regression check. CNND roundtrip fixtures need their JSON regenerated to match new shape — easiest path is `cargo test cnnd_roundtrip` to see what fails, regenerate the failing fixtures by serializing fresh, hand-verify a couple, commit.

**Verification.** `cd rust && cargo test` green; `cargo clippy` clean; `cargo insta accept` after review.

**Gotchas.**
- The old HashMap allowed O(1) "is this node a source for this argument?" via `contains_key`. The new Vec does linear scan. Add a helper method `Argument::has_source(node_id) -> bool` for the few hot spots, but don't pre-optimize — typical N is 1.
- Multiple `IncomingWire` entries with the same `source_node_id` were impossible under the HashMap; under Vec they're representable. For Phase 1, **the migration must produce uniquely-keyed wires** (preserve semantics). Add a debug-assert helper and an invariant comment on `Argument`.
- HashMap iteration order was non-deterministic; existing tests already use `normalize_json` to sort. Vec iteration is deterministic — some test comparisons may become more strict and reveal previously-hidden ordering bugs in wire construction. Fix the construction order, not the test.

### Phase 2: Zone data model — fields only, no behavior

**Goal.** Add the zone fields to `Node` and `NodeType`, plus `ArgumentKind` on `Wire`. All zone fields are empty/`None` for existing nodes; no node populates them yet. No semantic change.

**Scope.**
- `rust/src/structure_designer/node_type.rs` — add `zone_input_pins: Vec<OutputPinDefinition>` and `zone_output_pins: Vec<Parameter>` to `NodeType`. Default to empty `Vec` for all existing node types.
- `rust/src/structure_designer/node_network.rs` — add `zone: Option<Arc<NodeNetwork>>` and `zone_output_arguments: Vec<Argument>` to `Node`. Default `None` / empty. Add `ArgumentKind` enum and the `destination_argument_kind: ArgumentKind` field on the `Wire` view. Add `Node::zone_mut(&mut self) -> Option<&mut NodeNetwork>` (single point through which body edits go, wrapping `Arc::make_mut`) so callers don't reach into the `Arc` directly.
- `Node::clone`, equality, hashing (where applicable): propagate the new fields.
- Serialization: add `#[serde(default)]` for backward-compatibility *within this branch* (so fixtures regenerated in Phase 1 don't all need updating again). The `.cnnd` migration from main is out of scope.
- Snapshot/undo code that captures node state: include `zone` and `zone_output_arguments` in snapshots.
- `rust/src/api/structure_designer/` — no API changes required yet; nothing exposes zones outward yet.
- Copy/paste: a copied HOF carries its body along via `Arc::clone` (one refcount bump); subsequent edits to either copy CoW-clone the body lazily through `Arc::make_mut`. Existing copy/paste of non-HOF nodes is unaffected.

**Tests.** No new tests. Existing tests pass. Snapshot tests may regenerate to reflect the new (empty) fields in serialized output.

**Verification.** `cd rust && cargo test` green; `cargo insta accept`.

**Gotchas.**
- `Node` was previously cheap to clone (no nested networks). With `Option<Arc<NodeNetwork>>`, `Node::clone` stays cheap regardless of body size — it's a refcount bump rather than a deep walk. The deep copy is paid lazily by `Arc::make_mut` when an HOF Node is mutated while another reference is live. In synchronous evaluation the refcount at mutation time is normally 1, so `make_mut` returns the existing allocation without copying. No issue for Phase 2 since no zone is ever populated.
- Add a debug-assert: a `Node` whose `node_type_name` corresponds to a non-zone-bearing `NodeType` must have `zone == None` and `zone_output_arguments == empty`. Enforce in `Node`-construction helpers.

### Phase 3: Evaluator scaffolding for zones — unreached code paths

**Goal.** Add the zone-aware fields on `NetworkEvaluationContext` and the two new arms in `evaluate_arg`. Since no node populates zone data yet, the new code paths never fire. All existing tests still pass.

**Scope.**
- `rust/src/structure_designer/evaluator/network_evaluator.rs` — add to `NetworkEvaluationContext`:
  - `current_zone_input_values: HashMap<u64, Vec<Vec<NetworkResult>>>` (HOF id ↦ scope-stack of per-iteration frames; see §"What's new" point 3 for why the stack shape is load-bearing)
  - `captured_source_values: Arc<HashMap<CaptureKey, NetworkResult>>` (with `CaptureKey` defined as in §"Capture pre-evaluation"). Initialize new contexts to `EMPTY_CAPTURES.clone()` — a shared `static LazyLock<Arc<HashMap<…>>>` initialized to the empty map, so non-zone evaluation pays zero allocation for this field.
  - Helper methods on `NetworkEvaluationContext`: `push_zone_input_frame`, `pop_zone_input_frame`, `current_zone_input(hof_id, pin_index) -> &NetworkResult`, and `write_zone_input_pin(hof_id, pin_index, value)` (mutates the top frame; convenient for `fold`'s acc-then-element update). Direct indexing into the inner Vec is reserved for the helpers — call sites use the API so the push/pop discipline can't be circumvented.
  - `CapturesGuard` RAII helper (see §"Sub-context pattern for body evaluation" for the impl) — Phase 4 walkers use it to swap captures in/out cheaply.
- Extend `evaluate_arg` / `evaluate_arg_required` to iterate `incoming_wires` and dispatch on `source_pin` and `source_scope_depth`:
  - `SourcePin::NodeOutput`, `source_scope_depth == 0`: today's path.
  - `SourcePin::NodeOutput`, `source_scope_depth > 0`: walk `network_stack[len-1-depth..]` and evaluate. Check capture cache first.
  - `SourcePin::ZoneInput`: call `context.current_zone_input(source_node_id, pin_index)` (after capture-cache check) — reads the top frame of the HOF id's scope-stack.
- Add the `evaluate_zone_output` helper (signature in §"Zone-output evaluation").
- Inner-context construction helper for eager HOFs (mirror `FunctionEvaluator::evaluate` pattern at `function_evaluator.rs:167-194`). Inherits `execute`, `use_vdw_cutoff`, `current_zone_input_values` (cloned — the `HashMap<u64, Vec<Vec<NetworkResult>>>` deep-clones the stacks, but the depth is small so the cost is negligible); fresh `captured_source_values` (this body's captures are built into a mutable `HashMap` during pre-evaluation, then sealed with `Arc::new(captures)` before being installed on the inner context — see §"Capture pre-evaluation"); fresh scratch fields; drains `print_buffer` back. Pull this out as a method on the context or a free function — it'll be called from each eager HOF's eval in Phase 5. Lazy walkers (Phase 4) skip this helper and use the caller's context directly under the push/pop discipline plus the `CapturesGuard` swap (see §"Sub-context pattern for body evaluation").

**Tests.** Optional: a single unit test constructing a `NetworkEvaluationContext` with the new fields and confirming default-empty behavior. Not strictly required.

**Verification.** `cd rust && cargo test` green; `cargo clippy` clean.

**Gotchas.**
- The capture-cache check in `evaluate_arg` must come *before* the per-`SourcePin` dispatch, otherwise captures of `ZoneInput` sources won't hit the cache (they'd fall into the live-lookup path and read from `current_zone_input_values`, which is wrong for captures — see the nested-HOF worked example).
- `network_stack.len() - 1 - (depth as usize)` underflows if `depth > stack_len - 1`. Validation should catch this earlier (Phase 6), but in Phase 3 it's worth panicking with a clear message in debug builds rather than silently returning garbage.
- The push/pop discipline on `current_zone_input_values` is what keeps the scope-stack semantics intact. Phase 3 lands the field and the helpers but no HOF uses them yet; in Phase 4 and 5, every `push_zone_input_frame` MUST be balanced by a `pop_zone_input_frame` along every exit path (including early-return on error). Add a debug invariant in the helper that records the stack depth on push and asserts it on pop, so a missing pop is caught at first occurrence rather than as silent corruption a few iterations later. Reach for a scope-guard helper if it makes the call sites cleaner.

### Phase 4: `map` rewrite to zones — first behavioral change

**Goal.** Convert `map` to declare zone pins and use the zone-body evaluation path. This is the first phase that breaks existing HOF tests; they get rewritten to use zones.

**Scope.**
- `rust/src/structure_designer/nodes/map.rs`:
  - `MapData::calculate_custom_node_type`: write `zone_input_pins = [("element", input_type)]` and `zone_output_pins = [("result", output_type)]`. Remove the `f` external parameter — `map` only has `xs` externally now.
  - `MapData::eval` rewritten end-to-end. Resolve `xs` first (no body on stack). Push body. Pre-evaluate captures (walk body for capture wires, build a mutable `HashMap` then seal with `Arc::new(captures)`). Pop body. Grab the body as `let body = Arc::clone(node.zone.as_ref().expect("HOF without zone"));` (refcount bump — see §"Data model"). Construct `Walker::Map { source, body, captures, hof_node_id }`. Return wrapped in `NetworkResult::Iterator`.
- `rust/src/structure_designer/evaluator/iterator_walker.rs`:
  - Replace the `Map { source, fe }` variant with `Map { source, body: Arc<NodeNetwork>, captures: Arc<HashMap<CaptureKey, NetworkResult>>, hof_node_id: u64 }`.
  - `Walker::Map::next`: construct `let _g = CapturesGuard::swap_in(ctx, Arc::clone(&self.captures));` (one refcount bump, restores on drop — see §"Sub-context pattern for body evaluation"), push body, `push_zone_input_frame(hof_id, vec![element])`, evaluate the HOF's `zone_output_arguments[0]` via `evaluate_zone_output`, `pop_zone_input_frame(hof_id)`, pop body, return result. The walker runs against the caller's context — not the eager inner-context helper — so the guard + push/pop is what keeps the caller's state intact. Wrap the body push and the zone-input frame push in their own RAII guards too if the call site has multiple early-return paths, so a `?` or panic can't leak a frame.
- `Node.zone` for a fresh `map` node: pre-populate with an empty body containing nothing — the user wires within it. For tests that construct map nodes directly, build the body explicitly.
- `rust/src/structure_designer/node_type_registry.rs` — `repair_node_network` or its equivalent must handle the new pin layout when `map`'s `input_type`/`output_type` change.
- `rust/src/api/structure_designer/`: add API functions for accessing/mutating a node's body (`get_node_zone`, helpers to add nodes inside a zone, etc.) **only as needed for tests**. The full UI-facing API can wait — but tests in Phase 4 need to construct zone bodies somehow.

**Tests.**
- **Delete or rewrite** the existing `map`-specific tests that wire a function pin. Look under `rust/tests/structure_designer/` — likely in `text_format_test.rs`, possibly elsewhere. Don't preserve the old "function pin → map" syntax; it no longer exists.
- **Add** new tests in `rust/tests/structure_designer/zones_test.rs` (new file):
  - Trivial: `range(3) → map(zone: element + 1) → collect` yields `[1, 2, 3]`.
  - Capture: parent network has `k = int(5)`; `range(3) → map(zone: element + k) → collect` yields `[5, 6, 7]`. Verifies capture pre-evaluation.
  - Walker cloning: two `collect` consumers of the same map output produce independent walkers.
  - Empty body error: a `map` whose body's `zone_output` pin has no incoming wire returns an `Error` at eval time.

**Verification.** `cd rust && cargo test` green. Specifically watch `cargo test --test structure_designer zones` for the new tests and `cargo test --test structure_designer` for the broader suite. Old `map`-using tests in `text_format_test.rs` etc. should be rewritten or removed in this phase, not left broken.

**Gotchas.**
- `MapData::adapt_for_drag_source` exists for the drag-aware add-node feature. Its logic remains correct (peel `Iter[T]`/`Array[T]`) but it should no longer touch any function-pin assumptions.
- The `text_format` path (`get_text_properties`/`set_text_properties`) for `map` still serializes `input_type` and `output_type` properties — those stay. The body itself is serialized via the new `Node.zone` field, not via text properties.
- `FunctionEvaluator`, `Closure`, the `output_pin_index == -1` branch — leave them in place as dead weight. Don't delete in Phase 4. They're unused by `map` after this phase, still used by `filter`/`fold`/`foreach` until Phase 5.

### Phase 5: `filter`, `fold`, `foreach` to zones

**Goal.** Replicate Phase 4's pattern for the remaining three HOFs.

**Scope.**
- `rust/src/structure_designer/nodes/filter.rs`, `fold.rs`, `foreach.rs`: same shape as map's rewrite. Each declares its zone-input and zone-output pins per the table in §"Concept". `eval` resolves external inputs, pushes body, pre-evaluates captures, then either constructs a walker variant (filter — lazy) or iterates eagerly (fold, foreach).
- `iterator_walker.rs`: replace `Walker::Filter { source, fe }` with the zones-shaped variant. `fold` and `foreach` don't need walker variants — they drain in `eval`.
- `fold`'s zone has two zone-input pins (`acc`, `element`); push one frame onto `current_zone_input_values[fold_id]` at the start of iteration, then on each step **mutate the top frame** to `vec![current_acc, current_element]` (do not push a new frame per element — `fold` is one iteration, the frame is per call, not per step). Pop the frame after the stream is drained.
- `foreach` is `Unit`-returning; the central skip rule (`network_evaluator.rs:999-1010`) already handles the display-pass short-circuit and applies unchanged. Verify by trace.

**Tests.**
- Delete/rewrite existing function-pin-based tests for `filter`, `fold`, `foreach` (`rust/tests/structure_designer/fold_test.rs` and similar).
- Extend `zones_test.rs` with:
  - `filter`: `range(10) → filter(zone: element % 2 == 0) → collect` yields `[0, 2, 4, 6, 8]`.
  - `fold`: sum-fold over a range, with and without a captured initial offset.
  - `foreach`: build a body containing `export_xyz` or similar Unit-returning effect; verify the Execute-flag gating still works.
  - **Nested**: outer fold over `[1,2,3]` whose body contains an inner fold capturing a parent-scope constant. Match the worked example in §"Worked example — when do captures fire?".
  - **Chained**: `range → filter → map → collect`, verify captures of each HOF pre-evaluate exactly once.
  - **Scope-stack regression (id collision)**: outer fold whose body contains an inner `map → collect_inner` where the inner `map` is intentionally given the same numeric `node_id` as the outer `fold` (force the id via the test helpers that bypass `next_node_id`). The body computes `acc + collect_inner.length()`, and the inner map's body reads outer-fold's `acc` via a depth-2 capture. Asserts that each outer iteration sees the correct `acc` even though the inner walker's `next()` repeatedly pushes/pops a frame on the same `hof_id` key. Without scope-stack semantics this test would silently produce wrong totals; with them, push/pop restores outer's frame to the top of stack between inner `next()` calls. This is the explicit regression for the issue called out in §"What's new" point 3.

**Verification.** `cd rust && cargo test` green. The entire test suite is now using zone-based HOFs.

**Gotchas.**
- The `Walker::Filter::next` loop skips non-passing elements internally before yielding. Capture and iteration-value lifetimes are the same as `Walker::Map`'s (snapshot at body entry, set per iteration), just inside a loop that may iterate the source multiple times per yielded element.
- For `fold`, captures must be pre-evaluated BEFORE the iteration loop starts (once per fold call), not per iteration. Easy to get this wrong by accident — write the `fold.eval` such that the pre-evaluation is unambiguously outside the loop.
- After Phase 5, `FunctionEvaluator` is unreachable from any HOF node's `eval`. Confirm with `cargo build` (warnings will surface). It's still alive (called from the `output_pin_index == -1` branch of `evaluate`, which can be reached by anyone wiring something into a `DataType::Function` input pin — but no node has such pins any more). Leave it as dead weight per the deferred-cleanup plan.

### Phase 6: Zone validation rules

**Goal.** Add the three new validation checks from §"Validation" to `network_validator.rs`. Surface zone-specific errors at validation time rather than as runtime evaluation errors.

**Scope.**
- `rust/src/structure_designer/network_validator.rs` — three new checks:
  1. Every zone-output pin has at least one `IncomingWire` in the HOF's `zone_output_arguments[i]`. If not, report a validation error on the HOF node.
  2. Every `IncomingWire` with `source_scope_depth > 0` and `source_pin = NodeOutput` resolves: walking `depth` levels up the network stack lands in a network containing `source_node_id` with a compatible output pin.
  3. Every `IncomingWire` with `source_pin = ZoneInput { pin_index }` resolves: walking `depth` levels up lands on an HOF node whose `zone_input_pins` has `pin_index < length`.
- `repair_node_network` (`node_type_registry.rs`) — when a zone-bearing node's input/output type changes and zone-input or zone-output pin types shift, repair the wires inside the body that reference those pins. Pattern mirrors existing record-type-def repair.
- Validation walks must descend into `Node.zone` recursively for zone-bearing nodes.

**Tests.** Extend `zones_test.rs`:
- Each of the three new validation rules: construct an invalid network, assert the expected `ValidationError`.
- Repair: change a `map`'s `input_type`; verify body wires referencing the (now-typed-differently) zone-input pin get disconnected if incompatible.

**Verification.** `cd rust && cargo test` green; `cargo clippy` clean.

**Gotchas.**
- A capture wire's "source resolves" check needs to be done at validation time, not just runtime — runtime resolution under a deleted source node panics or produces an `Error`, but the user should see this in the validation pane earlier.
- For nested-zone networks, the validator's recursive walk must keep an accurate `network_stack` so that `source_scope_depth` checks resolve against the right ancestor at each level. Mirror the evaluator's stack discipline.

---

### Out of phase plan (deferred)

- **Editor (Flutter)**: separate design doc. Picks up after Phase 6 lands.
- **Migration**: `.cnnd` migrator from function-pin closures to zone bodies. Picks up after the editor exists.
- **Dead-weight cleanup**: delete `FunctionEvaluator`, `Closure`, `DataType::Function`, the `output_pin_index == -1` branch, function-pin rendering. Cleanest done after migration so the test fixtures don't have stale references.

The first six phases are the critical path for "Rust side of zones complete." After Phase 6, `cargo test` is green with the zone path exercising every HOF use case, and the Flutter UI consumers are appropriately broken — ready for the UI design phase to start.
