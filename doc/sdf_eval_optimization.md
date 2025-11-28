# Efficient geo_tree evaluation in atomCAD for lattice fill.

## What we do today

A geo tree is composed of geo nodes. Here are the geo node kinds:

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

We need to evaluate the SDF of a geo tree to do an atomic fill of the crystal. The naive algorithm would be to evaluate the SDF at each point where there supposed to be an atom in the infinite crystal lattice. If the value is negative the atom is included, it is positive there is no atom there.
In practice we do this in a finite box, the evaluation box to make the algorithm finite, but it would be still too slow this way.
So what we do is we take advantage of the 1-Lipschitz property of the SDF:

‖∇f‖ ≤ 1

which means that if the value is positive 'v' at a point then there is no surface in the 'v' radius sphere from that point.
Because of this we treat the evaluation space using adaptive spatial subdivision. We evaluate the SDF at the center of a box and:
- if the value is bigger than the half diagonal then we treat the whole box as empty.
- If the value is smaller than minus half diagonal then we treat the whole box as filled.
- otherwise we subdivide the box (into 2, 4, or 8 children depending on which dimensions need subdivision) and do the above recursively.

If the box is small enough or the box is filled we evaluate each atom position according to the motif inside the box.

- If the value is positive we discard the atom
- If the value is negative we add the atom to the atomic structure with the appropriate depth value.

(The reason that we do an evaluation even if the cube is filled is that we want to calculate the depth value for each atom.)

This is much faster, but still too slow.

The next optimization we do is batched evaluation. We collect 1024 evaluation tasks and evaluate the geo tree for 1024 sample points at once.
This is a big win because the processor's branch prediction gets much less misses and there is less function call overhead per sample task.

Then another optimization that we do is that we assign batches to threads and we evaluate on multiple threads.

Please note that we only do the batched evaluation for the evaluations at atoms. In case we need the evaluation value immediately (evaluations at box centers) we do
a single non-batched evaluation. The number of these evaluations is much lower than the atom evaluations but these are much slower, so they still contribute to the overall runtime
significantly.


## Optimization possibilities in the future

### Tree rewriting

It is possible to do transformations on the geo-tree which do not alter the overall geometry but decreases the evaluation complexity of the tree.
This needs to be reaearched with example geo trees, but here are some of the trivial transformations that make the evaluation time smaller:

#### Interesction of intersactions is an intersection

This is true for Intersection2D and Intersection3D too.

Example:

```
Intersection3D(Intersection3D(A, B), Intersection3D(C, D)) => Intersection3D(A, B, C, D)
```

#### Union of unions is an union

This is true for Union2D and Union3D too.

```
Union3D(Union3D(A, B), Union3D(C, D)) => Union3D(A, B, C, D)
```

#### A Transform can be eliminated by applying it

Example: intersection of halfspaces transformed => transform the half spaces and make the intersection.

### Bounding Volume Hierarchies BVHs.

Using BVHs for generic SDF evaluation is not as trivial as using it for ray tracing. The reason is that we are usually not just interested in a Boolean outcome (hit or not hit),
but usually we are interested in the exact value of the SDF.
In atomCAD lattice fill, most of the time we are interested in the in the actual SDF value and not just its sign, but not completely:

- when doing the cube center evaluation we need to know whether the value is bigger than half diagonal or smaller than minus half diagonal
- when doing evaluation for an atom we need the exact value only if the value is negative: we store depth information for atoms. The exact positive value is not interesting for us: it is just out of the shape.

Optimizations can be made when the exact value is not needed, but now we will concentrate on algorithms where we calculate the exact SDF value.

For these algorithms we create BVH proxies for certain nodes in the tree. Creating a BVH proxy efficiently is also not trivial. For now let's assume we create the csgrs mesh for each node and we calculate the AABB (Axis-Aligned Bounding Box) of the csgrs mesh as a BVH proxy. We calculate a BVH proxy only for nodes which has much more evaluation cost than evaluating the BVH.

#### BVH-adjusted union

The union node in SDF is a `min` function.

For any point in space outside the Bounding Volume (BV) proxy of a node, the SDF value of the proxy provides a conservative lower bound: `sdf_proxy(p) ≤ sdf_actual(p)`. The actual geometry is always at least as far away as the bounding volume suggests. Inside the BV, no such guarantee exists.

Here is the optimized evaluation of the union node:

1. Evaluate the proxy SDF for all child nodes at the sample point → get proxy values [p₁, p₂, ..., pₙ]
2. Find the child i with the minimal proxy value: i = argmin(pⱼ)
3. Evaluate child i fully (not just its proxy) → get real value rᵢ
4. Note that rᵢ ≥ pᵢ (the real value is always greater than or equal to the proxy value)
5. Check if any other proxy values are smaller than rᵢ. If pⱼ < rᵢ for some j ≠ i, those children might actually have smaller real values.
6. Fully evaluate all children j where pⱼ < rᵢ
7. Take the minimum of all fully evaluated real values

In the best case (when rᵢ < pⱼ for all j ≠ i), we only evaluate one child fully. In the worst case, we need to fully evaluate all children.

#### BVH-adjusted union on batched evaluations

For batched evaluation, we can achieve significant speedup while maintaining correctness:

1. Evaluate all proxy SDFs for all children for all sample points in the batch
2. For each sample point, determine which child has the minimal proxy SDF value
3. Group sample points by their minimal child (each group needs that child evaluated)
4. Fully evaluate only the children that are minimal for at least one sample point in the batch
5. For each sample point, take the minimum across all fully-evaluated children for that point

This approach ensures correctness: each point gets the true minimum SDF from the children that could potentially be minimal for that point. The batching benefit comes from evaluating each child once for multiple points, rather than separately.

**Important considerations for efficiency:**
- Batch size should not be too large, as larger batches are less spatially coherent (more children will need evaluation)
- Batches should be as spatially localized as possible for better BVH effectiveness
- Iteration over **Morton codes** (Z-order curve encoding that interleaves x, y, z coordinate bits) helps maintain spatial locality by mapping 3D positions to 1D values that preserve spatial coherence
