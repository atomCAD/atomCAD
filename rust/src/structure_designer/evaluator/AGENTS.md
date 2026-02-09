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
- `evaluate()` - Recursive node evaluation with caching
- `evaluate_arg()` / `evaluate_arg_required()` - Extract input pin values with type conversion
- `generate_scene()` - Top-level entry point producing the full scene

Handles both built-in nodes (call `NodeData::eval()`) and custom node types (recursive network evaluation via `FunctionEvaluator`).

## NetworkResult

All possible node output values:

```
Bool, Int, Float, Vec2, Vec3, IVec2, IVec3, String,
Geometry(GeometrySummary), Atomic(AtomicStructure),
Array(Vec<NetworkResult>), Function(Closure), Error(String)
```

- `GeometrySummary` wraps both 2D and 3D geometry with optional unit cell/drawing plane
- `Closure` captures a function node's network for deferred evaluation
- Type conversion via `convert_to(target_type)` follows `DataType` rules
- Accessor methods: `extract_float()`, `extract_geometry()`, `extract_atomic()`, etc.

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
