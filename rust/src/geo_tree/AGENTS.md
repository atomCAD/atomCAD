# geo_tree Module - Agent Instructions

The geo_tree module is a high-performance 3D geometry library. It represents geometry as an immutable expression tree (`GeoNode`) that supports two evaluation modes: **CSG** (polygon mesh conversion via `csgrs`) and **SDF** (signed distance field / implicit evaluation). Used by the node network for geometry representation and by `crystolecule/lattice_fill` for determining which lattice points lie inside a shape.

## Module Structure

```
geo_tree/
├── mod.rs                          # GeoNode struct, GeoNodeKind enum, constructors, hashing
├── csg_types.rs                    # Type aliases: CSGMesh, CSGSketch (wraps csgrs)
├── csg_utils.rs                    # Coordinate scaling, glam↔nalgebra conversions
├── csg_conversion.rs               # GeoNode → CSGMesh/CSGSketch polygon conversion
├── implicit_eval.rs                # ImplicitGeometry2D/3D trait implementations on GeoNode
├── implicit_geometry.rs            # Trait definitions: ImplicitGeometry3D, ImplicitGeometry2D
├── csg_cache.rs                    # Memory-bounded LRU cache for CSG results + statistics
└── batched_implicit_evaluator.rs   # Batched & multi-threaded SDF evaluation engine
```

## Key Types

| Type | Location | Purpose |
|------|----------|---------|
| `GeoNode` | `mod.rs` | Core type: immutable tree of geometric operations with pre-computed BLAKE3 hash |
| `GeoNodeKind` | `mod.rs` | Enum: primitives (HalfSpace, Sphere, Circle, HalfPlane, Polygon) + operations (Union, Intersection, Difference, Transform, Extrude) |
| `ImplicitGeometry3D` | `implicit_geometry.rs` | Trait: `implicit_eval_3d`, `implicit_eval_3d_batch`, `get_gradient`, `is3d` |
| `ImplicitGeometry2D` | `implicit_geometry.rs` | Trait: `implicit_eval_2d`, `implicit_eval_2d_batch`, `get_gradient_2d`, `is2d` |
| `BatchedImplicitEvaluator` | `batched_implicit_evaluator.rs` | Accumulates points, evaluates in 1024-point batches, optional rayon parallelism |
| `CsgConversionCache` | `csg_cache.rs` | Memory-bounded LRU cache keyed by BLAKE3 hash (default: 200 MB meshes, 56 MB sketches) |
| `CacheStats` | `csg_cache.rs` | Hit/miss counts and memory usage for cache diagnostics |
| `CSGMesh` / `CSGSketch` | `csg_types.rs` | Type aliases for `csgrs::mesh::Mesh<()>` / `csgrs::sketch::Sketch<()>` |

## Core Concepts

**GeoNode Hashing**: Every `GeoNode` has a pre-computed BLAKE3 hash (computed at construction time). Each variant uses a unique tag byte (0x01-0x0D) plus parameter bytes. Composite nodes hash their children's hashes. This enables O(1) cache lookups, change detection, and subtree deduplication.

**SDF Evaluation**: Returns signed distance from a point to the geometry surface. Negative = inside, zero = on surface, positive = outside. Operations compose as: Union = `min(a, b)`, Intersection = `max(a, b)`, Difference = `max(base, -sub)`. Transform applies inverse transform to the sample point.

**Batch Evaluation**: `BATCH_SIZE = 1024`. Points are processed in fixed-size arrays for better cache locality and branch prediction. `BatchedImplicitEvaluator` pads to BATCH_SIZE multiples and truncates results. Multi-threading threshold: 2048+ points, max 7 threads (rayon work-stealing).

**CSG Conversion**: Recursively converts GeoNode trees to polygon meshes via `csgrs`. Circle = 36-segment polygon, Sphere = 24x12 mesh. Results optionally cached by hash in `CsgConversionCache` with LRU eviction.

## Dependencies

```
geo_tree depends on:
├── glam          (DVec2, DVec3, DMat4 - math)
├── blake3        (content hashing)
├── csgrs         (polygon CSG operations)
├── rayon         (parallel batch evaluation)
├── nalgebra      (Point3/Vector3 for csgrs interop)
├── geo           (2D geometry types from csgrs)
└── util          (Transform, MemorySizeEstimator, MemoryBoundedLruCache)
```

**Architectural constraint:** This module is independent of `renderer`, `display`, `crystolecule`, and `structure_designer`. The `display` module converts geo_tree output into renderable meshes. Never add upstream dependencies here.

## Testing

Tests live in `rust/tests/geo_tree/` (never inline `#[cfg(test)]`). Test modules are registered in `rust/tests/geo_tree.rs`.

```
tests/geo_tree/
├── implicit_eval_test.rs                   # SDF evaluation correctness
├── csg_cache_test.rs                       # Cache hit/miss, eviction, statistics
├── batched_implicit_evaluator_test.rs      # Batch evaluation, padding, result ordering
└── multi_threaded_batch_evaluator_test.rs  # Parallel evaluation correctness
```

**Running:** `cd rust && cargo test geo_tree`

## Modifying This Module

**Adding a new primitive**: Add variant to `GeoNodeKind` in `mod.rs` with a unique tag byte (next: 0x0E). Add constructor, Display match arm, MemorySizeEstimator match arm. Implement SDF evaluation in `implicit_eval.rs` (both 2D/3D trait as appropriate). Implement CSG conversion in `csg_conversion.rs`. Add tests.

**Adding a new CSG operation**: Same as primitive but the operation must compose child results (e.g., min/max for union/intersection).

**Changing batch size**: Update `BATCH_SIZE` in `implicit_geometry.rs`. This affects all batch evaluation signatures and `BatchedImplicitEvaluator` padding logic.

**Tuning cache defaults**: Edit `CsgConversionCache::with_defaults()` in `csg_cache.rs`. Current: 200 MB meshes, 56 MB sketches.
