# SDF Evaluation Optimization for Crystal Lattice Filling

## Overview

This document describes the algorithm for efficiently evaluating Signed Distance Fields (SDFs) represented as CSG trees (called "geo trees") to fill crystal lattices with atoms in atomCAD. The challenge is evaluating potentially complex geometric constraints across millions of lattice points.

**Key optimizations:**
1. **Adaptive spatial subdivision** - exploiting the Lipschitz property to skip large empty/filled regions
2. **Batched evaluation** - processing 1024 points at once for better cache and branch prediction
3. **Multi-threading** - parallel batch processing using work-stealing
4. **BVH acceleration** (future) - bounding volumes to skip expensive subtree evaluations

---

## Geo Tree Structure

A geo tree is composed of geo nodes representing primitives and CSG operations:

```
enum GeoNodeKind {
  HalfSpace {
    normal: DVec3,
    center: DVec3,
  },
  HalfPlane {
    // inside is to the left of the line defined by point1 -> point2
    point1: DVec2,
    point2: DVec2,
  },
  Circle {
    center: DVec2,
    radius: f64,  
  },
  Sphere {
    center: DVec3,
    radius: f64,
  },
  Polygon {
    vertices: Vec<DVec2>,
  },
  Extrude {
    height: f64,
    direction: DVec3,
    shape: Box<GeoNode>,
  },
  Transform {
    transform: Transform,
    shape: Box<GeoNode>,
  },
  Union2D {
    shapes: Vec<GeoNode>,
  },
  Union3D {
    shapes: Vec<GeoNode>,
  },
  Intersection2D {
    shapes: Vec<GeoNode>,
  },
  Intersection3D {
    shapes: Vec<GeoNode>,
  },
  Difference2D {
    base: Box<GeoNode>,
    sub: Box<GeoNode>
  },
  Difference3D {
    base: Box<GeoNode>,
    sub: Box<GeoNode>
  },
}
```

---

## Current Algorithm

### 1. Adaptive Spatial Subdivision

**Naive approach:** Evaluate SDF at every potential atom position in the crystal lattice.
**Problem:** Millions of evaluations, most in empty space or deep inside filled regions.

**Solution:** Exploit the **1-Lipschitz property** of SDFs:

```
‖∇f‖ ≤ 1  ⟹  distance can change by at most 1 per unit distance traveled
```

**Visual explanation:**
```
     SDF=+5 at box center          SDF=-5 at box center         SDF=+1 at box center
     ┌─────────────┐                ┌─────────────┐              ┌─────────────┐
     │             │                │█████████████│              │      ?      │
     │   EMPTY     │                │█████████████│              │    ? ? ?    │
     │             │   surface      │█████FILLED██│              │   ?????     │
     │             │   must be      │█████████████│              │  ??? ? ?    │
     │             │   >5 away      │█████████████│              │    ?????    │
     └─────────────┘                └─────────────┘              └─────────────┘
     half_diag=3 → SKIP             half_diag=3 → FILL           Need subdivision
```

**Algorithm:**
1. Evaluate SDF at box center → value `v`
2. Calculate box half-diagonal `d`
3. Decide:
   - `v > d`: Entire box is empty → **skip all atoms**
   - `v < -d`: Entire box is filled → **evaluate all atoms** (for depth values)
   - Otherwise: **subdivide** into 2, 4, or 8 children and recurse

When a box is small or filled, evaluate each atom position:
- `SDF < 0`: Add atom with depth value
- `SDF ≥ 0`: Discard atom

### 2. Batched Evaluation

**Problem:** Evaluating SDFs one-by-one causes:
- Branch mispredictions (unpredictable CSG tree paths)
- Function call overhead
- Poor cache utilization

**Solution:** Collect 1024 sample points and evaluate them together.

```
Single evaluation:              Batched evaluation (1024 points):
                                
point₁ → eval → result₁         [point₁, point₂, ..., point₁₀₂₄]
point₂ → eval → result₂              ↓
point₃ → eval → result₃         eval_batch (single tree traversal)
  ...                                ↓
                                [result₁, result₂, ..., result₁₀₂₄]

❌ Poor branch prediction        ✅ Better branch prediction
❌ Scattered memory access       ✅ Better cache locality
❌ Per-call overhead            ✅ Amortized overhead
```

**Key insight:** Box centers need immediate results (single evaluation), but atom evaluations can be batched.

### 3. Multi-Threading

Batches are distributed across threads using Rayon's work-stealing scheduler:

```
Thread 1: [batch₁] [batch₄] [batch₇] ...
Thread 2: [batch₂] [batch₅] [batch₈] ...
Thread 3: [batch₃] [batch₆] [batch₉] ...
```

**Spatial locality:** Batches are ordered by **Morton codes** (Z-order curve) to keep spatially nearby points together, improving cache coherence.


---

## Future Optimizations

### 1. Tree Rewriting

Algebraic simplifications that reduce tree depth without changing geometry:

#### Flatten Nested Operations

```
Before:                        After:
   Union                        Union
   /  \                        / | | \
Union Union         ⟹         A  B C  D
 / \   / \
A   B C   D

4 nodes, depth 3              5 nodes, depth 2
```

**Rules:**
- `Intersection(Intersection(A,B), Intersection(C,D))` → `Intersection(A,B,C,D)`
- `Union(Union(A,B), Union(C,D))` → `Union(A,B,C,D)`

#### Transform Elimination

Push transforms to leaves where they can be baked into primitive parameters:

```
Before:                        After:
  Transform                     Union
     |                          /   \
   Union                   Sphere₁' Sphere₂'
   /   \            ⟹    (transformed) (transformed)
Sphere₁ Sphere₂

Runtime transform overhead     Pre-transformed, faster evaluation
```

**Strategy:**
1. `Transform(Union(A,B))` → `Union(Transform(A), Transform(B))`
2. `Transform₁(Transform₂(A))` → `Transform_combined(A)`
3. At leaves: `Transform(Sphere(c,r))` → `Sphere(transform(c), r)`

### 2. Bounding Volume Hierarchies (BVH)

**Challenge:** Unlike ray tracing (which only needs hit/miss), in lattice filling we often need exact SDF values, but not always:

- Box center evaluations: Only need to know if `|SDF| > half_diagonal`
- Atom evaluations: Need exact value only when `SDF < 0` (for depth); positive values don't matter

**Key insight:** For points outside a bounding volume, the BV's SDF provides a conservative **lower bound**:

```
Point outside BV:              Point inside BV:
     p•                             ┌─────┐
      \                             │  p• │
       \  actual geometry           │ ▲▲▲ │
        \    ▲▲▲                    │▲▲▲▲▲│
         \  ▲▲▲▲▲                   └─────┘
    ┌─────▼─────┐                   
    │    BV     │                   No guarantee
    └───────────┘
    
sdf_BV(p) ≤ sdf_actual(p)          (BV might be closer than actual geometry)
```

**BVH Proxy Creation:** For expensive subtrees, create a bounding volume proxy. Only worthwhile when subtree evaluation cost >>  BV proxy evaluation cost.
How to do it:
Option 1: create an Axis-Aligned Bounding Box (AABB) from the mesh representation. Drawback: the mesh itself can be expensive to create. (Although might be in our geo mesh cache)
Option 2: Use the 'sphere carving' method described in this paper: https://dl.acm.org/doi/10.1145/3730845

#### BVH-Adjusted Union

**Standard union:** `min(child₁, child₂, ..., childₙ)` - must evaluate all children.

**Optimization:** Use BV proxies as lower bounds to skip expensive full evaluations.

**Core principle:** 
```
sdf_proxy(p) ≤ sdf_real(p)    (proxy is a lower bound)

Therefore: If proxy_j > current_best_real, then real_j ≥ proxy_j > current_best_real
          → Child j cannot be the minimum → Skip it!
```

**Complete Algorithm:**
```
1. Evaluate all proxy SDFs (cheap): proxies = [p₁, p₂, ..., pₙ]
2. Initialize: best_real = +∞, unevaluated = {1, 2, ..., n}

3. While unevaluated is not empty:
   a. Find i = argmin(proxies[j] for j in unevaluated)
   
   b. Termination check: if proxies[i] ≥ best_real
      → All remaining children have proxies ≥ best_real
      → They cannot be the minimum
      → STOP
   
   c. Fully evaluate child i (expensive): real_i = SDF(child_i, point)
   
   d. Update: best_real = min(best_real, real_i)
   
   e. Remove i from unevaluated

4. Return best_real
```

**Visual Setup:**
```
Union of 3 complex shapes, evaluating at single point p:

    ┌───────┐              ┌───────┐              ┌───────┐
    │  BV₁  │              │  BV₂  │              │  BV₃  │
    │ ▲▲▲▲  │              │ ▲▲▲▲  │              │ ▲▲▲▲  │
    │▲▲▲▲▲▲ │              │▲▲▲▲▲▲ │              │▲▲▲▲▲▲ │
    └───────┘              └───────┘              └───────┘
       ↑                      ↑                      ↑
       └──────────────────────┼──────────────────────┘
                              │
                             p•
                             
    Distance to BV₁: 2.0     Distance to BV₂: 5.3     Distance to BV₃: 8.1
```

**Example - Best Case:**
```
Union of 3 shapes:  Proxy SDFs: [2.0, 5.3, 8.1]

Iteration 1: Evaluate child 1 (proxy=2.0) → real=3.5
             best_real = 3.5
             Remaining proxies: [5.3, 8.1] all > 3.5
             ✅ DONE! Evaluated only 1 child
```

**Example - Worst Case:**
```
Union of 3 shapes:  Proxy SDFs: [2.0, 2.1, 2.2]

Iteration 1: Evaluate child 1 (proxy=2.0) → real=8.0
             best_real = 8.0
             Remaining proxies: [2.1, 2.2] both < 8.0 → Must evaluate!

Iteration 2: Evaluate child 2 (proxy=2.1) → real=7.5
             best_real = 7.5
             Remaining proxies: [2.2] < 7.5 → Must evaluate!

Iteration 3: Evaluate child 3 (proxy=2.2) → real=3.0
             best_real = 3.0
             ✅ DONE! Evaluated all 3 children
```

**Performance:**
- **Best case:** 1 full evaluation (large proxy differences)
- **Typical case:** 1-2 full evaluations (some spatial coherence)
- **Worst case:** n full evaluations (tight proxy clustering)

#### BVH-Adjusted Union (Batched)

**Challenge:** With 1024 points, we can't process each point independently.

**Solution:** Amortize work across the batch.

**Algorithm:**
1. Evaluate all BV proxies for all children for all 1024 points
2. For each point, identify which child has the minimal proxy value
3. Group points by their minimal child:
   ```
   Child 1: [point₃, point₇, point₁₅, ...]  (412 points)
   Child 2: [point₁, point₉, point₂₃, ...]  (501 points)
   Child 3: [point₅, point₁₁, ...]          (111 points)
   ```
4. Fully evaluate only children that are minimal for at least one point
5. For each point, compute `min(fully_evaluated_children)`

**Example:**
```
                    Single-point           Batched (1024 points)
                    
Children to eval:   3 children            2 children (child 3 never minimal)
Evaluations:        3 × 1 = 3            2 × 1024 = 2048
vs naive:           3                    3 × 1024 = 3072

Speedup: ~1.5× for this batch
```

**Efficiency tips:**
- **Small batches** → Better spatial coherence → Fewer children needed per batch
- **Morton order** → Nearby points in batch → Similar minimal children
- **Sweet spot:** 1024 points balances amortization vs. coherence

---

### Alternative: Lipsitz pruning of the tree based on regions

This is an alternative algorithm compared to our BVH approach. It is probably faster. While the tree preparations make it slightly more complicated than our approach, at least after the tree reduction is done, the actual evaluation do not need to be modified.

https://onlinelibrary.wiley.com/doi/10.1111/cgf.70057 

## Summary

The SDF evaluation algorithm combines multiple optimizations working in concert:

| Optimization | Speedup Mechanism | Status |
|-------------|-------------------|---------|
| **Lipschitz subdivision** | Skip millions of empty/filled regions | ✅ Implemented |
| **Batched evaluation** | Amortize overhead, improve branch prediction | ✅ Implemented |
| **Multi-threading** | Parallel processing with work-stealing | ✅ Implemented |
| **Morton ordering** | Spatial locality for cache coherence | ✅ Implemented |
| **Tree rewriting** | Reduce tree depth, eliminate transforms | 🔜 Future |
| **BVH acceleration** | Skip expensive subtree evaluations | 🔜 Future |

**Current performance:** Enables semi real-time crystal lattice filling for complex CSG geometries.

**Future improvements:** BVH integration could provide 2-10× additional speedup for unions of complex shapes.
