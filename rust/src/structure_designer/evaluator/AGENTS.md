# Evaluator - Agent Instructions

Network evaluation engine. Processes the node DAG to produce displayable output.

## Files

| File | Purpose |
|------|---------|
| `network_evaluator.rs` | Main evaluator: traverses DAG, evaluates nodes, builds scene |
| `network_result.rs` | `NetworkResult` enum: all possible node output values |
| `function_evaluator.rs` | Evaluates closures by constructing temporary networks |
| `iterator_walker.rs` | `Walker` tree: lazy stream runtime for `Iter[T]` (carried by `NetworkResult::Iterator`) |

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

Handles both built-in nodes (call `NodeData::eval()`) and custom node types (recursive network evaluation via `FunctionEvaluator`).

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
Iterator(Walker), Function(Closure), Error(String)
```

- **Three-phase payload structs** (lattice-space refactoring):
  - `BlueprintData { structure, geo_tree_root }` — geometry + structure, no atoms.
  - `CrystalData { structure, atoms, geo_tree_root: Option<_> }` — materialized atoms still bound to a structure.
  - `MoleculeData { atoms, geo_tree_root: Option<_> }` — atoms with no structure.
- **No abstract variants at runtime**: every `NetworkResult` carries a concrete phase (Blueprint/Crystal/Molecule). Abstract `DataType`s (HasAtoms/HasStructure/HasFreeLinOps) are pin-level only. `infer_data_type` debug-asserts this.
- **No `frame_transform`** on `BlueprintData` or `AtomicStructure`. Movement nodes (`structure_move`, `free_move`, etc.) bake transforms into atom positions and wrap `geo_tree_root` in `GeoNode::transform`. `GeometrySummary2D` still carries `frame_transform` (2D-only, unrelated to the refactoring).
- `Structure` bundles lattice_vecs + motif + motif_offset; emitted by the `structure` node and flowed into primitives.
- `Record` carries fields only — **no type name** at runtime. Field list is stored in **canonical (sorted-by-name) order** — `NetworkResult::record(...)` sorts on construction, so derived `PartialEq` is structural and `extract_record_field()` does binary search by name. Pass-through coercion: a record value flowing into a pin declared with a smaller schema is **not** projected — the runtime value carries any extra fields through unchanged. See `doc/design_record_types.md`.
- `Closure` captures a function node's network for deferred evaluation.
- Type conversion via `convert_to(source_type, target_type)` follows `DataType` rules.
- Accessor methods: `extract_float()`, `extract_crystal()`, `extract_molecule()`, `extract_atomic()` (accepts both Crystal and Molecule and returns their `AtomicStructure`), `extract_structure()`, etc. `get_unit_cell()` extracts the `UnitCellStruct` from LatticeVecs/DrawingPlane/Geometry2D/Blueprint/Crystal/Structure results.

## Walker (iterator runtime)

`Walker` is the runtime representation of `Iter[T]`. One unified tree (no separate immutable recipe), carried by `NetworkResult::Iterator(Walker)`.

- **Variants** (all in `iterator_walker.rs`): `FromArray { items: Arc<Vec<NetworkResult>>, idx }`, `Range { start, step, count, emitted }`, `Map { source, fe }`, `Filter { source, fe }`, `Product { axes, field_names, current, primed, done }`.
- **API**: `next(&mut self, evaluator, registry) -> Option<NetworkResult>` advances; `reset(&mut self)` rewinds. `None` = stream end; `Some(NetworkResult::Error(_))` = error mid-stream.
- **Outer fuse**: `Walker { kind, fused }` — variants yield `Some(Error(_))` once and the outer wrapper flips `fused` so subsequent calls return `None`. Individual variants do **not** track their own error fuse.
- **`Product`** primes by pulling one element from every axis on first `next`; subsequent `next` advances rightmost-first with mixed-radix carry. Empty axis → empty product. The `done` flag tracks natural odometer exhaustion, **not** error state.
- **Construction-time errors** (closure FE can't be built — source network missing, source node missing) must be detected by `map.eval()` / `filter.eval()` and returned as `EvalOutput::single(NetworkResult::Error(_))` — do **not** construct a degenerate walker, or errors multiply per element.

### Invariant 2: clone independence (load-bearing)

`NetworkResult` is cloned on multiple hot paths — `EvalOutput::get` (`node_data.rs:50`, `.cloned()`), `EvalOutput::get_display`, `evaluate_required` (`network_evaluator.rs:751`), `parameter::eval` (`parameter.rs:63`). Every one of these clones any enclosed `Walker`. **`Walker::clone()` must therefore produce a walker whose `next()` advances independently of the original** — anyone adding a new walker variant must preserve this. The current design satisfies it naturally: per-walker `idx`/`emitted`/`current` state is owned, `FromArray::items` is `Arc`-shared so cloning is O(1) regardless of array length, and `FunctionEvaluator` derives `Clone` with no shared mutable state.

The evaluator does **not** memoize pin results, so for an `Iter[T]` pin with fan-out N the producing node's `eval()` runs N times in one pass — each call constructs a fresh walker. Two consumers of the same iterator pin therefore drain *different* walkers; one cannot starve the other. The display path is one such consumer (it auto-collects up to `ITER_DISPLAY_CAP = 256` elements over a clone of the displayed pin's walker).

## FunctionEvaluator

Evaluates `Closure` values (used by `map`/`filter` per-element function inputs and `fold`'s combining function):
- Builds a temporary `NodeNetwork` from the closure's captured network
- Creates `Value` nodes as placeholders for function arguments
- `set_argument_value()` allows reuse with different inputs
- **Derives `Clone`** — required because `WalkerKind::Map` / `Filter` carry an FE and walker clones propagate. Each clone is an independent FE (no `Rc`/`Arc` interior); `set_argument_value` on a clone does not disturb the original. Construction (`FunctionEvaluator::new`) is "somewhat expensive" — `map`/`filter` pay this once per `eval()` call and store the FE in the walker so the per-element hot path is just `set_argument_value` + `evaluate`.

## Scene Output Types

`NodeOutput` variants (in `structure_designer_scene.rs`):
- `Atomic(AtomicStructure)` - Atom/bond data
- `SurfacePointCloud` / `SurfacePointCloud2D` - SDF surface samples
- `PolyMesh` - Explicit polygon mesh
- `DrawingPlane` - 2D construction plane
- `None` - No visual output
