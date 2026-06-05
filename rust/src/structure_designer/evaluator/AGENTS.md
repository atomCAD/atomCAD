# Evaluator - Agent Instructions

Network evaluation engine. Processes the node DAG to produce displayable output.

## Files

| File | Purpose |
|------|---------|
| `network_evaluator.rs` | Main evaluator: traverses DAG, evaluates nodes, builds scene |
| `network_result.rs` | `NetworkResult` enum: all possible node output values |
| `iterator_walker.rs` | `Walker` tree: lazy stream runtime for `Iter[T]` (carried by `NetworkResult::Iterator`) |
| `zone_closure.rs` | `ZoneClosure` bundle (incl. `pre_supplied_args` for partial application — see `doc/design_currying.md`) + the shared per-element `run_closure_once` / `build_inline_closure` / `build_node_function_closure` (powers the HOF zone bodies, the `closure` node, the function pin, the `apply` node's recursive consumption loop, and the `NetworkResult::Function` value) |

## NetworkEvaluator

Core evaluation logic in `generate_scene()`:
1. Determines which nodes are visible (display policy)
2. Evaluates each visible node via recursive `evaluate()`
3. For each node: evaluate input arguments → call `NodeData::eval()` → cache result
4. Converts results to `NodeOutput` (Atomic, SurfacePointCloud, PolyMesh, etc.)
5. Builds `StructureDesignerScene` with visible + cached invisible node data

Key methods:
- `evaluate(network_stack, node_id, output_pin_index, ...)` - Recursive node evaluation; returns `NetworkResult` for one pin
- `evaluate_all_outputs(network_stack, node_id, ...)` - Returns full `EvalOutput` (all pins) from a single `eval()` call
- `evaluate_arg()` / `evaluate_arg_required()` - Extract input pin values with type conversion
- `evaluate_or_default(...)` - Optional input with fallback literal
- `generate_scene()` - Top-level entry point producing the full scene

Handles both built-in nodes (call `NodeData::eval()`) and custom node types (recursive network evaluation: `evaluate`/`evaluate_all_outputs` push the sub-network onto the stack and recurse into its return node).

## NetworkEvaluationContext

Per-pass scratch state threaded through every `evaluate*` call. Notable fields:
- `node_errors: HashMap<u64, String>`, `node_output_strings: HashMap<u64, Vec<String>>` — written during the pass and read by `generate_scene` to populate `NodeSceneData`.
- `selected_node_eval_cache: Option<Box<dyn Any>>` — the active node's eval-cache slot (used by gadgets).
- `top_level_parameters: HashMap<String, NetworkResult>` — CLI/headless parameter injection.
- `use_vdw_cutoff: bool` — minimization preference.
- **`execute: bool`** — `true` only for explicit Execute passes (right-click → Execute on a node). Drives the central skip rule below and flows into inner-body evaluations: the lazy zone walkers (`MapZone`/`FilterZone`) run the body against the *same* context, while the eager HOFs (`fold`/`foreach`) copy it into a `fresh_inner_for_eager_body` context.
- **`print_buffer: Vec<PrintLogEntry>`** — appended to by the `print` node's `eval`; drained by the orchestrator (`StructureDesigner::with_eval_context`) into `StructureDesigner.print_log` at end-of-pass.

In production code paths inside `rust/src/structure_designer/`, the only `NetworkEvaluationContext::new()` caller is `StructureDesigner::with_eval_context` (the per-pass orchestrator). The eager HOFs (`fold`/`foreach`) build their per-iteration body context via `fresh_inner_for_eager_body` (a struct literal, outside the `::new()` audit) and `drain_inner_context` it back; the lazy zone walkers reuse the caller's context. The old `FunctionEvaluator::evaluate` inner-body `::new()` site was removed in closures Phase 2. Tests are exempt; reviewers grepping for `NetworkEvaluationContext::new(` outside `with_eval_context` have a one-shot audit.

## Central skip rule (Unit-returning nodes)

In `evaluate_all_outputs`, before dispatching to `NodeData::eval`:

> If `!context.execute` **and** every resolved output pin of the node has `DataType::Unit`, skip the call and synthesise an `EvalOutput` of all `NetworkResult::Unit` directly.

This gates *every* effect node (`export_xyz`, `foreach`, future Unit-returning nodes) in one place — no per-node `if context.execute` guards, no risk of forgetting one. The check uses the **resolved** output type (via the existing `NodeTypeRegistry::resolve_output_type` machinery), not the declared `OutputPinDefinition` — so a hypothetical future `SameAsInput` pin that resolves to `Unit` is also covered. The rule applies only when **all** output pins are Unit; a hypothetical mixed-output node (some Float, some Unit) is evaluated normally because the non-Unit outputs may be needed downstream and we cannot synthesise a Float without running `eval`. Design doc: `doc/design_node_execution.md` ("Central skip rule for Unit-returning nodes").

## Multi-Output Evaluation

- `NodeData::eval()` returns `EvalOutput` (wraps `Vec<NetworkResult>`). Single-output nodes use `EvalOutput::single()`.
- `evaluate()` calls `evaluate_all_outputs()` internally and extracts the requested pin.
- `generate_scene()` uses `evaluate_all_outputs()` once per displayed node, avoiding redundant evaluation when multiple pins are displayed.
- **Custom network nodes** pass through all outputs from the return node: `evaluate_all_outputs()` calls itself recursively on the return node, and `evaluate()` forwards the `output_pin_index` to the return node.

## NetworkResult

All possible node output values:

```
Bool, Int, Float, Vec2, Vec3, IVec2, IVec3, String,
LatticeVecs(UnitCellStruct), DrawingPlane(DrawingPlane),
Geometry2D(GeometrySummary2D), Blueprint(BlueprintData),
Crystal(CrystalData), Molecule(MoleculeData),
Motif(Motif), Structure(Structure),
Array(Vec<NetworkResult>), Record(Vec<(String, NetworkResult)>),
Iterator(Walker), Function(ZoneClosure), Unit, Error(String)
```

`Unit` is the empty-payload variant used as the runtime value of effect nodes (`export_xyz`, `foreach`). `infer_data_type` returns `DataType::Unit`, `to_display_string` returns `"()"`, and `convert_to(any, &DataType::Unit)` collapses every non-Error source to `NetworkResult::Unit` (an iterator on the source side is dropped without being drained — the desired "discard" semantic). The reverse `Unit → T` is rejected. See `doc/design_node_execution.md` ("The Unit type").

- **Three-phase payload structs** (lattice-space refactoring):
  - `BlueprintData { structure, geo_tree_root }` — geometry + structure, no atoms.
  - `CrystalData { structure, atoms, geo_tree_root: Option<_> }` — materialized atoms still bound to a structure.
  - `MoleculeData { atoms, geo_tree_root: Option<_> }` — atoms with no structure.
- **No abstract variants at runtime**: every `NetworkResult` carries a concrete phase (Blueprint/Crystal/Molecule). Abstract `DataType`s (HasAtoms/HasStructure/HasFreeLinOps) are pin-level only. `infer_data_type` debug-asserts this.
- **No `frame_transform`** on `BlueprintData` or `AtomicStructure`. Movement nodes (`structure_move`, `free_move`, etc.) bake transforms into atom positions and wrap `geo_tree_root` in `GeoNode::transform`. `GeometrySummary2D` still carries `frame_transform` (2D-only, unrelated to the refactoring).
- `Structure` bundles lattice_vecs + motif + motif_offset; emitted by the `structure` node and flowed into primitives.
- `Record` carries fields only — **no type name** at runtime. Field list is stored in **canonical (sorted-by-name) order** — `NetworkResult::record(...)` sorts on construction, so derived `PartialEq` is structural and `extract_record_field()` does binary search by name. Pass-through coercion: a record value flowing into a pin declared with a smaller schema is **not** projected — the runtime value carries any extra fields through unchanged. See `doc/design_record_types.md`.
- `Function(ZoneClosure)` is a first-class function value — the same detached zone-body bundle (`evaluator/zone_closure.rs`: `{ body, captures, zone_output_wires, owner_node_id, param_types, return_type, pre_supplied_args }`) that an inline HOF body carries, handed around as a value. `infer_data_type` derives its `DataType::Function` from `ZoneClosure::function_type` (which canonicalizes via `FunctionType::new`). `pre_supplied_args: Arc<Vec<NetworkResult>>` carries partial-application bound args; `run_closure_once` prepends them to the caller-supplied frame, default empty. `param_types` is the **body's actual frame size** (how many caller args one `run_closure_once` consumes), which can be *narrower* than the closure's declared canonical-flat function type when the body returns a `Function` value — `apply.eval`'s recursive consumption loop reconciles the two. Produced by the `closure` node (`nodes/closure.rs`, via `build_inline_closure`) and consumed by an HOF's optional `f` pin (via `obtain_closure`) and by the `apply` node (one-shot or partial call via `run_closure_once` in a loop). See `doc/design_closures.md`, `doc/design_currying.md`, `doc/design_function_pin_unification.md`.
- Type conversion via `convert_to(source_type, target_type)` follows `DataType` rules. The `Function → Function` arm is an identity passthrough (function values need no runtime conversion).
- Accessor methods: `extract_float()`, `extract_crystal()`, `extract_molecule()`, `extract_atomic()` (accepts both Crystal and Molecule and returns their `AtomicStructure`), `extract_structure()`, etc. `get_unit_cell()` extracts the `UnitCellStruct` from LatticeVecs/DrawingPlane/Geometry2D/Blueprint/Crystal/Structure results.

## Walker (iterator runtime)

`Walker` is the runtime representation of `Iter[T]`. One unified tree (no separate immutable recipe), carried by `NetworkResult::Iterator(Walker)`.

- **Variants** (all in `iterator_walker.rs`): `FromArray { items: Arc<Vec<NetworkResult>>, idx }`, `Range { start, step, count, emitted }`, `Product { axes, field_names, current, primed, done }`, `MapZone { source, closure }`, `FilterZone { source, closure }`, `Convert { source, source_elem_type, target_elem_type }`. The `Zone` variants are the forms of `map` / `filter`: each carries a `ZoneClosure` (`evaluator/zone_closure.rs`) — the bundle `{ body, captures, zone_output_wires, owner_node_id, param_types, return_type }` — and on each `next` runs it once on the pulled element via `zone_closure::run_closure_once` (push a scope frame, swap the frozen captures in, resolve the body-return wire, pop). The lazy walkers pass `&[]` as the base network stack (body-only), since `next` doesn't hold the outer stack. The `Convert` variant (issue #330) implements lazy `Iter[S] → Iter[T]` element conversion: the wire layer (`NetworkResult::convert_to`) wraps an iterator source whose element type differs from the destination's, and each `next` pulls one element and runs `convert_to(source_elem_type → target_elem_type)`. The legacy FE-driven `Map`/`Filter` variants were removed in closures Phase 2. See "Zones" in `../AGENTS.md` and `doc/design_closures.md` for the design.
- **API**: `next(&mut self, evaluator, registry, context: &mut NetworkEvaluationContext) -> Option<NetworkResult>` advances; `reset(&mut self)` rewinds. `None` = stream end; `Some(NetworkResult::Error(_))` = error mid-stream. The `context` parameter is the outer pass's evaluation context — the `Zone` walkers run their closure body against it (via `run_closure_once`) so bodies inherit `context.execute` and so prints from inner-body nodes drain back into the outer context. Without `&mut` here, prints emitted from inside a zone body would have nowhere to drain to and would be silently lost on every walker step. See `doc/design_node_execution.md` (Phase 2 — Walker propagation).
- **Outer fuse**: `Walker { kind, fused }` — variants yield `Some(Error(_))` once and the outer wrapper flips `fused` so subsequent calls return `None`. Individual variants do **not** track their own error fuse.
- **`Product`** primes by pulling one element from every axis on first `next`; subsequent `next` advances rightmost-first with mixed-radix carry. Empty axis → empty product. The `done` flag tracks natural odometer exhaustion, **not** error state.
- **Construction-time errors**: when the body itself is malformed (no zone-output wire, missing inner source node, …) `map.eval()` / `filter.eval()` must detect it via `build_inline_closure` and return `EvalOutput::single(NetworkResult::Error(_))` — do **not** construct a degenerate walker, or errors multiply per element.

### `zone_closure::run_closure_once` (the shared per-step body)

`zone_closure::run_closure_once(evaluator, network_stack, registry, context, closure, args)` runs a `ZoneClosure` once on one argument frame and is the single per-element resolver shared by **all four HOFs** and the `apply` node: the lazy walkers (`MapZone`/`FilterZone::next`), the eager drain loops (`fold`/`foreach::eval`), and `apply.eval`'s recursive consumption loop. The actual frame is `[pre_supplied_args… ++ args…]` — `run_closure_once` prepends the closure's partial-application bound args (default empty) to the caller-supplied args before pushing the iteration frame keyed by `owner_node_id`. It swaps the closure's frozen captures into `context`, resolves the body-return wire against `network_stack` + the body pushed on top (`eval_step`), then pops the frame and restores the captures.

The `network_stack` parameter is load-bearing: the **eager** HOFs pass their real containing-network stack, so a *nested* HOF inside the body can resolve captures reaching past the immediate body (e.g. a grandparent constant at `source_scope_depth == 2`). The **lazy** walkers pass `&[]` (body-only) because `next` doesn't hold the outer stack — their bodies' deep captures are pre-frozen at the producing HOF's `eval`, so body-only is sufficient. The `apply` node is also an eager consumer and passes its real stack.

**`apply.eval`'s recursive consumption loop** (`nodes/apply.rs`) reconciles the closure's body arity (`param_types.len()`) with the caller's wired-arg count `k`: each iteration consumes `n_body` args from a `VecDeque` and runs `run_closure_once`. If fewer args remain than `n_body`, the remaining args are drained into a `pre_supplied_args`-extended clone of the closure and returned as a `NetworkResult::Function` (partial application). If `k == 0` on a non-thunk, an identity-partial guard short-circuits and returns the original `f` unchanged. If the body returned another `Function` and there are still args to consume, `f_current` advances and the loop continues — this is how a closure whose body returns a `Function` (declared canonical-flat arity wider than `n_body`) is driven to full evaluation by a chain of `apply` arguments. Design doc: `doc/design_currying.md`.

Three more helpers live alongside `run_closure_once`:
- `zone_closure::build_inline_closure` — builds a `ZoneClosure` from a node's *own* inline zone body (grab `node.zone`, freeze captures via `build_captures`, collect the `zone_output_arguments` wires, fill type metadata). Used by the four HOFs (inline-body path) and by the `closure` node's `eval` (which wraps the result as `NetworkResult::Function`).
- `zone_closure::build_node_function_closure` — the **function-pin** synthesizer (`doc/design_function_pins.md`): builds a *capture-free* `ZoneClosure` from "the whole node viewed as a function of all its inputs" — clones the node into a one-node synthetic body, feeds each input pin from a `ZoneInput` parameter, and returns output pin 0. Reached from the revived `output_pin_index == -1` branch in `NetworkEvaluator::evaluate`, so the title-bar `-1` pin produces a `NetworkResult::Function` consumed by the HOF `f` pins / `apply` like any other closure. Rejects zero-input and polymorphic-output (`DataType::None`) nodes.
- `zone_closure::obtain_closure` — the HOF dispatcher: if the node's `f` (Function) pin is wired, evaluate it and take the carried `ZoneClosure`; otherwise fall back to `build_inline_closure`. This is the single branch that lets `map`/`filter`/`fold`/`foreach` accept *either* a wired-in function value or their own inline body. The `apply` node does **not** use it — its `f` pin is required and read directly.

The legacy `network_evaluator::evaluate_zone_output` was deleted in closures Phase 2 — its only callers (`fold`/`foreach`) had already moved to `run_closure_once`.

### Invariant 2: clone independence (load-bearing)

`NetworkResult` is cloned on multiple hot paths — `EvalOutput::get` (`node_data.rs:50`, `.cloned()`), `EvalOutput::get_display`, `evaluate_required` (`network_evaluator.rs:751`), `parameter::eval` (`parameter.rs:63`). Every one of these clones any enclosed `Walker`. **`Walker::clone()` must therefore produce a walker whose `next()` advances independently of the original** — anyone adding a new walker variant must preserve this. The current design satisfies it naturally: per-walker `idx`/`emitted`/`current` state is owned, `FromArray::items` is `Arc`-shared so cloning is O(1) regardless of array length, and the `Zone` variants' embedded `ZoneClosure` is entirely `Arc`-backed (body, captures, zone-output wires) so cloning it is refcount bumps with no shared *mutable* state.

The evaluator does **not** memoize pin results, so for an `Iter[T]` pin with fan-out N the producing node's `eval()` runs N times in one pass — each call constructs a fresh walker. Two consumers of the same iterator pin therefore drain *different* walkers; one cannot starve the other. (A node whose displayed pin output is `Iter[T]` produces no viewport output — materialization is the consumer's job. To preview a stream, wire it into `collect` and display that. See `doc/design_iter_display_via_collect.md`.)

## Scene Output Types

`NodeOutput` variants (in `structure_designer_scene.rs`):
- `Atomic(AtomicStructure)` - Atom/bond data
- `SurfacePointCloud` / `SurfacePointCloud2D` - SDF surface samples
- `PolyMesh` - Explicit polygon mesh
- `DrawingPlane` - 2D construction plane
- `None` - No visual output
