# Evaluator - Agent Instructions

Network evaluation engine. Processes the node DAG to produce displayable output.

## Files

| File | Purpose |
|------|---------|
| `network_evaluator.rs` | Main evaluator: traverses DAG, evaluates nodes, builds scene |
| `network_result.rs` | `NetworkResult` enum: all possible node output values |
| `function_evaluator.rs` | Evaluates closures by constructing temporary networks |

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
Array(Vec<NetworkResult>), Function(Closure), Error(String)
```

- **Three-phase payload structs** (lattice-space refactoring):
  - `BlueprintData { structure, geo_tree_root }` — geometry + structure, no atoms.
  - `CrystalData { structure, atoms, geo_tree_root: Option<_> }` — materialized atoms still bound to a structure.
  - `MoleculeData { atoms, geo_tree_root: Option<_> }` — atoms with no structure.
- **No abstract variants at runtime**: every `NetworkResult` carries a concrete phase (Blueprint/Crystal/Molecule). Abstract `DataType`s (Atomic/StructureBound/Unanchored) are pin-level only. `infer_data_type` debug-asserts this.
- **No `frame_transform`** on `BlueprintData` or `AtomicStructure`. Movement nodes (`structure_move`, `free_move`, etc.) bake transforms into atom positions and wrap `geo_tree_root` in `GeoNode::transform`. `GeometrySummary2D` still carries `frame_transform` (2D-only, unrelated to the refactoring).
- `Structure` bundles lattice_vecs + motif + motif_offset; emitted by the `structure` node and flowed into primitives.
- `Closure` captures a function node's network for deferred evaluation.
- Type conversion via `convert_to(source_type, target_type)` follows `DataType` rules.
- Accessor methods: `extract_float()`, `extract_crystal()`, `extract_molecule()`, `extract_atomic()` (accepts both Crystal and Molecule and returns their `AtomicStructure`), `extract_structure()`, etc. `get_unit_cell()` extracts the `UnitCellStruct` from LatticeVecs/DrawingPlane/Geometry2D/Blueprint/Crystal/Structure results.

## FunctionEvaluator

Evaluates `Closure` values (from Map node's function input):
- Builds a temporary `NodeNetwork` from the closure's captured network
- Creates `Value` nodes as placeholders for function arguments
- `set_argument_value()` allows reuse with different inputs

## Scene Output Types

`NodeOutput` variants (in `structure_designer_scene.rs`):
- `Atomic(AtomicStructure)` - Atom/bond data
- `SurfacePointCloud` / `SurfacePointCloud2D` - SDF surface samples
- `PolyMesh` - Explicit polygon mesh
- `DrawingPlane` - 2D construction plane
- `None` - No visual output
