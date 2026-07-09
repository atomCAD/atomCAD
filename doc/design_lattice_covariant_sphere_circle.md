# Design: lattice-covariant `sphere` and `circle` (ellipsoid semantics on non-cubic cells)

## Motivation

`sphere` and `circle` are the *lattice-discrete* geometry primitives: integer
center (`IVec3` / `IVec2`) and integer radius, in lattice units. On a cubic
cell they behave perfectly. On any other unit cell the current behavior is
wrong in two independent ways (`nodes/sphere.rs`, `nodes/circle.rs`):

1. **The radius unit is arbitrary.** The center converts through the full
   basis (`ivec3_lattice_to_real` = `x·a + y·b + z·c`), but the radius is
   scaled by `|a|` alone (`int_lattice_to_real` =
   `value * a.length()`, `unit_cell_struct.rs`). The lengths of `b` and `c`
   and all three cell angles are ignored — "radius 2" spans a different
   number of cells along `b`/`c` than along `a`.
2. **The shape is not lattice-covariant.** A Euclidean sphere does not
   transform with the lattice: change the `structure` input from cubic to
   sheared and the set of lattice cells inside the shape changes arbitrarily.

Per project decision, the non-cubic behavior is treated as a **bug**: we owe
backward compatibility only for (approximately) cubic cells. Users who want a
*physically round* shape independent of the lattice already have
`free_sphere` / `free_circle` (real-space Å floats,
`doc/design_free_sphere_circle.md`) — those nodes are **not** touched by this
design and stay Euclidean.

## The new semantics

**Define the shape as a ball in fractional (lattice) coordinates and map it
to real space through the lattice matrix.**

Let `L = [a b c]` (columns = basis vectors), `c₀ ∈ ℤ³` the stored center,
`r ∈ ℤ` the stored radius. The shape is

```
E = L(B)  where  B = { u ∈ ℝ³ : |u − c₀| ≤ r }
```

equivalently, in real space,

```
x ∈ E  ⇔  |L⁻¹x − c₀| ≤ r  ⇔  (x − x₀)ᵀ (L Lᵀ)⁻¹ (x − x₀) ≤ r²,   x₀ = L·c₀
```

### Theorem: this is always an ellipsoid

`x ∈ E ⇔ |L⁻¹(x − x₀)| ≤ r ⇔ (x−x₀)ᵀ L⁻ᵀL⁻¹ (x−x₀) ≤ r²`, and
`L⁻ᵀL⁻¹ = (LLᵀ)⁻¹`. `LLᵀ` is symmetric positive definite
(`vᵀLLᵀv = |Lᵀv|² > 0` for `v ≠ 0`, `L` invertible since the cell has nonzero
volume), so by the spectral theorem the set is an ellipsoid. ∎

SVD picture: `L = UΣVᵀ`; `Vᵀ` maps the ball to itself, `Σ` stretches it into
an axis-aligned ellipsoid with semi-axes `rσ₁, rσ₂, rσ₃`, `U` rotates it into
place. The principal axes are the **left singular vectors of `L`** — in
general *not* the lattice vectors; they coincide with `a, b, c` only for
orthogonal cells. This is the metric of the crystallographic Gram matrix
`G = LᵀL` (distance `√(uᵀGu)` in fractional coordinates) — the standard way
crystallography measures real distance from fractional coordinates.

### Why this is the correct choice

1. **Cubic back-compat is exact.** For `L = s·I`:
   `(|(p−x₀)|/(rs) − 1)·rs = |p − x₀| − rs` — mathematically identical to
   today's sphere of radius `r·|a|`.
2. **Lattice covariance.** Swap/shear the `structure` input and the shape
   transforms with it.
3. **Discrete content is lattice-invariant.** The set of lattice points
   inside is `{ u ∈ ℤ³ : |u − c₀| ≤ r }` — the *same* discrete ball for every
   lattice. For a node whose center and radius are deliberately integers,
   this is the defining property.
4. **Volume invariance in cell units.** `vol(E) = (4/3)πr³ · det(L)` —
   always exactly `(4/3)πr³` unit cells, so materialized atom counts are
   roughly independent of cell shape at fixed `r`.

The same statements hold in 2D for `circle` with the drawing plane's
2×2 effective cell (`L₂ = [a₂ b₂]`, ellipse
`|L₂⁻¹y − c₀| ≤ r`). Note 2D is where this bites *even for diamond*: the
effective cell of e.g. a (111) plane is non-square, so today's
`radius × |u|` is already arbitrary on the default lattice. The other 2D
primitives (`rect`, `reg_poly`, `polygon`) already map their **vertices**
through the lattice and are therefore covariant; `circle` is the odd one out.

### Rejected alternative

Keep a Euclidean sphere but pick a fairer radius unit (`∛det(L)`,
`min(|a|,|b|,|c|)`, …) — fixes only the unit arbitrariness, keeps
non-covariance, and satisfies none of properties 2–4. Rejected.

## Architecture

The change is **eval-only** at the node level plus **two new geo_tree
primitives**. Everything else in the stack is untouched.

### What does NOT change

- Node names, pins, categories, stored `NodeData` (`center`, `radius`
  integers), defaults, subtitles, `get_text_properties` /
  `set_text_properties`, `get_parameter_metadata`.
- Text format, `.cnnd` serialization — **no migration** (node data is
  unchanged; only evaluation output differs, and only on non-cubic cells).
- API layer, FRB bindings, Flutter editors, gadgets (none exist).
- `Alignment::Aligned` on the sphere's Blueprint output — the ellipsoid is
  lattice-aligned *by construction*.
- `free_sphere` / `free_circle` — stay Euclidean (that is their point).
- `GeoNodeKind::Sphere` / `Circle` remain (used by `free_sphere` /
  `free_circle`).

### New geo_tree primitives

`GeoNodeKind::Transform` is rigid-only (translation + quaternion,
`util/transform.rs`), so a transformed-sphere encoding is not available.
Add two first-class primitives instead (keeps the no-closed-form-SDF
question contained to one variant each):

```rust
Ellipsoid {
    center: DVec3,        // real-space center = L·c₀
    basis: DMat3,         // columns = r·a, r·b, r·c  (maps unit ball → E)
    // derived, precomputed by the constructor, EXCLUDED from hashing:
    inv_basis: DMat3,     // basis.inverse()
    lipschitz_scale: f64, // conservative distance scale, see SDF section
},
Ellipse {
    center: DVec2,
    basis: DMat2,         // columns = r·a₂, r·b₂
    inv_basis: DMat2,
    lipschitz_scale: f64,
},
```

- **Hash tag bytes:** `Ellipsoid = 0x0E`, `Ellipse = 0x0F` (next free after
  0x0D; update `geo_tree/AGENTS.md` to "next: 0x10"). Hash **only**
  `center` + `basis` bytes — derived fields must not affect identity.
- **Constructors:** `GeoNode::ellipsoid(center: DVec3, basis: DMat3)`,
  `GeoNode::ellipse(center: DVec2, basis: DMat2)`. If
  `|basis.determinant()| < 1e-12`, store a degenerate marker
  (`lipschitz_scale = 0.0`) and have eval return `f64::MAX` (empty shape) —
  never panic; matches the "handle missing/bad input gracefully" convention.
- **Display arm** (`display_with_indent`), **MemorySizeEstimator arm**
  (`size_of::<DVec3>() + 3·size_of::<DMat3>()`-ish; exactness irrelevant),
  `is3d()` / `is2d()` membership.

### SDF evaluation (`implicit_eval.rs`)

```rust
fn ellipsoid_implicit_eval(center, inv_basis, lipschitz_scale, p: &DVec3) -> f64 {
    if lipschitz_scale == 0.0 { return f64::MAX; }        // degenerate cell
    let q = inv_basis * (p - center);                     // unit-ball space
    (q.length() - 1.0) * lipschitz_scale
}
```

with `lipschitz_scale = 1.0 / frobenius_norm(inv_basis)` (√(sum of squared
elements); for 2D likewise). Batch arm: same loop over `BATCH_SIZE`.

**Contract.** The value has the *exact* sign and zero set of the ellipsoid,
and its magnitude is a *guaranteed underestimate* of true Euclidean distance:
the inner function `q ↦ |q| − 1` composed with `inv_basis` has Lipschitz
constant `‖inv_basis‖₂ = σ_max(inv_basis)`, and
`σ_max ≤ ‖·‖_F`, so scaling by `1/‖inv_basis‖_F` makes the result
≤ 1-Lipschitz; a ≤ 1-Lipschitz function vanishing on the boundary satisfies
`|f(x)| ≤ dist(x, ∂E)`. Using the Frobenius norm avoids needing an SVD/eigen
solve (glam has none); it costs at most an extra √3 (√2 in 2D) of
underestimation versus true `σ_min`.

**Why conservative is sufficient.** geo_tree's composite SDFs are *already*
only conservative bounds (union/intersection/difference via `min`/`max` are
inexact near feature interactions), so every consumer already lives with the
"exact sign, conservative magnitude" contract:

- `fill_lattice` subdivision culling: an underestimated distance can only
  cull *less*, never incorrectly — correctness unaffected, cost bounded by
  the cell's anisotropy (√3 × σ_max/σ_min at worst, negligible recursion
  overhead for real cells).
- The `≤ 0.01 Å` inside-with-margin tests (atom placement, region
  membership): underestimation slightly *widens* the effective margin (to at
  most `0.01·√3·σ_max/σ_min` Å) — still far below interatomic scales, and
  the sign itself is exact.
- `get_gradient` (finite differences over `implicit_eval_3d`): works
  unchanged; the function is smooth except at the center point.

**Deferred upgrade path** (documented, not implemented): Quilez's ellipsoid
approximation (first-order exact at the surface, needs the principal frame =
one SVD at construction) or exact distance via Eberly root-finding. Both are
internal to the same match arm; nothing in the API constrains this.

### CSG conversion (`csg_conversion.rs`)

csgrs (vendored, `flutter_cad/csgrs/`) exposes arbitrary-matrix
`CSGOps::transform(&Matrix4<Real>)` with correct inverse-transpose normal
handling (`csgrs/src/mesh/mod.rs::transform`). So:

```rust
fn ellipsoid_to_csg(center: DVec3, basis: DMat3) -> CSGMesh {
    // unit sphere, then one affine map; same tessellation density as Sphere
    CSGMesh::sphere(scale_to_csg(1.0), 24, 12, None)
        .transform(&affine)   // affine = [ scale_to_csg(basis) | scale_to_csg(center) ]
}
```

`Ellipse` likewise: `CSGSketch::circle(scale_to_csg(1.0), 36, None)
.transform(&affine₂)` (Sketch implements the same trait). A linear map sends
sphere-inscribed vertices to ellipsoid-inscribed vertices one-to-one, so mesh
quality relative to the true surface is unchanged. Degenerate basis → return
`CSGMesh::new()` / `CSGSketch::new()` (empty), matching the SDF arm.

Build the `Matrix4` via the existing glam↔nalgebra helpers in
`csg_utils.rs` (add a `dmat3_to_csg_matrix4`-style helper there if none
fits). Mind the `scale_to_csg` unit scaling: it applies to the *translation*
and to the *basis columns* (they carry length units); the matrix layout is
column-major in both glam and nalgebra.

### Node changes

`nodes/sphere.rs::eval` — replace the two conversion lines + `GeoNode` call:

```rust
let l = &structure.lattice_vecs;
let real_center = l.ivec3_lattice_to_real(&center);
let basis = DMat3::from_cols(l.a, l.b, l.c) * (radius as f64);
// geo_tree_root:
GeoNode::ellipsoid(real_center, basis)
```

`nodes/circle.rs::eval` — same with the drawing plane's effective cell:

```rust
let uc = &drawing_plane.effective_unit_cell;
let real_center = uc.ivec2_lattice_to_real(&center);
let a2 = DVec2::new(uc.a.x, uc.a.y);      // same 2D embedding as
let b2 = DVec2::new(uc.b.x, uc.b.y);      // dvec2_lattice_to_real
let basis = DMat2::from_cols(a2, b2) * (radius as f64);
// geo_tree_root:
GeoNode::ellipse(real_center, basis)
// frame_transform unchanged: Transform2D::new(real_center, 0.0)
```

No branch on `is_approximately_cubic`: the single code path degenerates to
the exact sphere/circle math on cubic cells (see theorem section); the only
cubic-case differences are float-rounding-level (different op order) and a
changed geo hash (one-time CSG cache miss).

Update both nodes' `description` strings to state the semantics, e.g.
"Outputs the lattice image of a sphere: integer center and radius in lattice
cells; an ellipsoid on non-cubic cells." (This churns node-type snapshots —
see Phase 5.)

### Downstream consumers — no changes

- `materialize` / `fill_lattice`: consume `ImplicitGeometry3D` generically.
- `extrude`: consumes the 2D SDF via `extrude_implicit_eval` and the sketch
  via `to_csg_sketch_cached` — the new arms slot in.
- `display/csg_to_poly_mesh`, implicit visualization
  (`structure_designer/implicit_eval/`): generic over the trait / mesh.
- Region pins (`map_atomic_in_region`), `half_space_utils`, `facet_shell`:
  don't build spheres from lattice radii.

### Cleanup

After both nodes switch, `UnitCellStruct::int_lattice_to_real` has **no
callers** — delete it. `float_lattice_to_real` keeps one caller
(`half_plane.rs:352`, grid sizing) — keep it, but its doc comment should
state it scales by `|a|` and is not a general length conversion.

## Phased implementation plan

Each phase compiles and runs green standalone (`cargo test -j 4` per Windows
convention; never two cargo commands concurrently). geo_tree tests go in
`rust/tests/geo_tree/ellipsoid_test.rs` (new, registered in
`rust/tests/geo_tree.rs`); node-level tests in
`rust/tests/structure_designer/lattice_covariant_primitives_test.rs` (new,
registered in `rust/tests/structure_designer.rs`).

### Phase 1 — `GeoNodeKind::Ellipsoid` (geo_tree, 3D)

Variant + constructor + hash tag `0x0E` + Display + MemorySizeEstimator +
`is3d` + SDF single/batch arms + CSG arm, per the specs above.

**Tests** (`ellipsoid_test.rs`):
- *Sign & zero set:* for a skewed basis (e.g. columns `(4,0,0)`, `(1,3,0)`,
  `(0.5,0.5,5)`), eval at center < 0; at `center + basis·u` for several unit
  vectors `u` ≈ 0 (tolerance 1e-9); at `center + basis·(2u)` > 0.
- *Conservativeness:* for random sample points `x`, `f(x) ≤ min_j |x − y_j|
  + ε` over a dense sampling `y_j` of the surface (the sampled min
  overestimates true distance, so this is a valid one-sided check); and
  `f` has the same sign as the exact membership test `|inv_basis·(x−c)| − 1`.
- *Cubic degeneracy:* with `basis = s·I`, eval equals
  `GeoNode::sphere(center, s)`'s eval at the same points to 1e-12.
- *Hash:* two ellipsoids with equal center/basis hash equal; differing basis
  → differing hash; hash unaffected by derived-field values (construct via
  the public constructor only — this is implicitly covered, state it).
- *Degenerate basis:* zero-determinant basis → `f64::MAX` everywhere, empty
  CSG mesh, no panic.
- *CSG mesh:* every vertex `v` of `to_csg` output satisfies
  `| inv_basis·(v − center) | ≈ 1` (all vertices lie on the ellipsoid);
  vertex count matches the sphere tessellation (24×12).
- *Batch = scalar:* `implicit_eval_3d_batch` matches per-point
  `implicit_eval_3d` (mirrors existing batch tests).
- *Composition smoke:* `Difference3D { cuboid-ish halfspace, ellipsoid }`
  evaluates with correct signs at hand-picked points.

### Phase 2 — `GeoNodeKind::Ellipse` (geo_tree, 2D)

Same shape of work, tag `0x0F`, `is2d`, `implicit_eval_2d` single/batch,
`to_csg_sketch` arm.

**Tests** (same file): 2D mirrors of Phase 1's sign/zero-set,
conservativeness, cubic(square)-degeneracy-vs-`Circle`, degenerate-basis,
sketch-vertices-on-ellipse, batch-consistency. Plus:
- *Extrude integration at the geo level:* `Extrude { shape: Ellipse, .. }`
  3D eval has correct sign inside/outside the extruded elliptic cylinder.

### Phase 3 — `sphere` node emits the ellipsoid

Change `sphere.rs::eval` per above; update its `description`.

**Tests** (`lattice_covariant_primitives_test.rs`):
- *Cubic back-compat (the promise):* sphere node on default diamond
  structure; eval output's `geo_tree_root` SDF agrees with
  `GeoNode::sphere(L·c₀, r·|a|)` to 1e-9 at a grid of sample points; and a
  `sphere → materialize` atom count on diamond is **identical** to the value
  before this change (hardcode the current count as the regression anchor).
- *Discrete covariance (the point of the feature):* for a triclinic
  `structure` (distinct lengths, non-right angles) and `c₀ = 0, r = 3`:
  every integer lattice point `u` with `|u| ≤ 3` maps to real `L·u` with
  SDF ≤ 0 (+margin), and every `u` with `|u| ≥ 4` maps to SDF > 0. The
  contained lattice-point set equals the cubic case's set.
- *Volume invariance:* `sphere(r=4) → materialize` atom counts on cubic vs.
  a sheared cell of equal cell volume and same motif agree within a few
  percent (loose tolerance; it's a sanity check, not an exact invariant).
- *Wired pins still override stored* (center/radius/structure) — guard
  against regressions while touching eval.
- *Alignment:* still `Aligned`, reason `None`.

### Phase 4 — `circle` node emits the ellipse

Change `circle.rs::eval` per above; update its `description`.

**Tests** (same file):
- *Square back-compat:* on the default XY plane of a cubic lattice, SDF
  agrees with `GeoNode::circle(center_real, r·|a|)` at sample points;
  `frame_transform` unchanged.
- *Non-square effective cell:* build a drawing plane whose effective cell is
  non-square (a (111)-type plane on diamond, or any plane on a non-cubic
  lattice); assert 2D discrete covariance: integer in-plane points with
  `|u| ≤ r` are inside, `|u| ≥ r+1` outside.
- *Extrude → materialize chain:* `circle → extrude → materialize` on the
  non-square plane produces atoms, and the placed atoms all satisfy the
  fractional membership test within margin.
- *Text-format smoke:* `c = circle { center: (1,2), radius: 3 }` still
  parses/serializes identically (node data untouched — this is a
  no-regression check, not new behavior).

### Phase 5 — cleanup, docs, snapshots

- Delete `UnitCellStruct::int_lattice_to_real`; adjust
  `float_lattice_to_real` doc comment. (Compile check is the test.)
- `cargo insta review` for node-snapshot churn from the two `description`
  strings.
- Update `geo_tree/AGENTS.md` (primitives list, "next tag: 0x10", CSG
  conversion notes) and `nodes/AGENTS.md` (Geometry 2D/3D bullets: state the
  ellipsoid semantics and the sphere/circle ↔ free_sphere/free_circle
  Euclidean split).
- Full gate: `cargo fmt && cargo clippy && cargo test -j 4`,
  `flutter analyze` (should be untouched — no Dart changes).
- **Manual walkthrough:** diamond sphere unchanged in viewport; load/build a
  non-cubic structure, confirm the sphere renders as a tilted ellipsoid and
  `materialize` fills it; circle on a (111) plane renders as an ellipse in
  the 2D view and extrudes correctly; hover values and subtitles unchanged.

## Deferred work (explicitly out of scope)

- **Tighter/exact ellipsoid SDF** (Quilez approximation or Eberly exact) —
  drop-in inside the same match arms if a future feature (offsetting,
  shelling) needs surface-accurate magnitudes.
- **A general linear-transform GeoNode** (`LinearTransform { matrix, shape }`)
  — more powerful, but drags the approximate-SDF question into every
  primitive at once; the dedicated variants keep the blast radius small.
  Revisit if a third transformed primitive shows up.
- **`free_ellipsoid` / per-axis-radii free primitives** — orthogonal
  feature; add on demand.
- **Adaptive mesh density for very anisotropic cells** — the 24×12 / 36-seg
  tessellation is applied in unit-sphere space; extreme anisotropy stretches
  triangles. Cosmetic only; revisit if it ever looks bad in practice.
