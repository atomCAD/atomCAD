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
   today's sphere of radius `r·|a|`. (In practice cubic cells never even
   reach the ellipsoid code path: the constructor's spherical-basis fast
   path — see Architecture — snaps them to the existing `Sphere`/`Circle`
   primitives, making back-compat *byte*-exact. The identity here is what
   makes that snap mathematically legitimate and keeps *near*-cubic bases
   just outside the snap tolerance continuous with it.)
2. **Lattice covariance.** Swap/shear the `structure` input and the shape
   transforms with it.
3. **Discrete content is lattice-invariant.** The set of lattice points
   inside is `{ u ∈ ℤ³ : |u − c₀| ≤ r }` — the *same* discrete ball for every
   lattice. For a node whose center and radius are deliberately integers,
   this is the defining property.
4. **Volume invariance in cell units.** `vol(E) = (4/3)πr³ · |det L|` —
   always exactly `(4/3)πr³` unit cells, so materialized atom counts are
   roughly independent of cell shape at fixed `r`.

The same statements hold in 2D for `circle` with the drawing plane's
2×2 effective cell (`L₂ = [a₂ b₂]`, ellipse
`|L₂⁻¹y − c₀| ≤ r`). Note 2D is where this bites *even for diamond*: the
effective cell of e.g. a (111) plane is non-square, so today's
`radius × |u|` is already arbitrary on the default lattice. The other 2D
primitives (`rect`, `reg_poly`, `polygon`) already map their **vertices**
through the lattice and are therefore covariant; `circle` is the odd one out.

### Rejected alternatives

- Keep a Euclidean sphere but pick a fairer radius unit (`∛det(L)`,
  `min(|a|,|b|,|c|)`, …) — fixes only the unit arbitrariness, keeps
  non-covariance, and satisfies none of properties 2–4. Rejected.
- Branch in the node evals on `is_approximately_cubic` (emit
  `GeoNode::sphere` directly for cubic structures) — achieves the same
  cubic back-compat, but duplicates the decision across two node files,
  doesn't cover future direct callers of `GeoNode::ellipsoid`, and misses
  rotated-orthonormal bases. Subsumed by the constructor's spherical-basis
  fast path (see Architecture), which keeps the nodes single-code-path.

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
  `free_circle`, and emitted by the new constructors' spherical-basis fast
  path — see below).

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
  `GeoNode::ellipse(center: DVec2, basis: DMat2)`. Two normalizations, in
  order:
  1. **Spherical-basis fast path (the cubic-regression guard).** If the
     basis columns are pairwise orthogonal with equal lengths — concretely:
     every column length > 0, all pairwise `|colᵢ·colⱼ| ≤ tol·|colᵢ|·|colⱼ|`,
     and all `||colᵢ| − |colⱼ|| ≤ tol·max_k|colₖ|`, with `tol ≈ 1e-9` — the
     shape is a true Euclidean sphere/circle of
     radius = the common column length — return a plain
     `GeoNodeKind::Sphere` / `Circle` instead of the new variant. On
     (approximately) cubic cells — the overwhelmingly common case in the
     wild — the nodes therefore produce the **identical GeoNode as today**:
     same SDF arm, same CSG tessellation path, same BLAKE3 hash, so
     materialize atom counts, viewport meshes, and CSG cache entries are
     unchanged bit-for-bit. Regression safety by construction, not by
     proof. This also catches rotated-orthonormal bases (equally Euclidean
     spheres). The threshold discontinuity is harmless: at the boundary the
     two representations agree to within the tolerance, dwarfed by the
     0.01 Å fill margin — and the σ_min `lipschitz_scale` (below) is what
     keeps the SDF *magnitude* continuous across the snap threshold.
  2. **Degenerate basis.** If `|basis.determinant()| < 1e-12`, store a
     degenerate marker (`lipschitz_scale = 0.0`) and have eval return
     `f64::MAX` (empty shape) — never panic; matches the "handle
     missing/bad input gracefully" convention.
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

with `lipschitz_scale = σ_min(basis)` (smallest singular value of `basis`;
for 2D likewise). Batch arm: same loop over `BATCH_SIZE`.

**Contract.** The value has the *exact* sign and zero set of the ellipsoid,
and its magnitude is a *guaranteed underestimate* of true Euclidean distance:
the inner function `q ↦ |q| − 1` composed with `inv_basis` has Lipschitz
constant `‖inv_basis‖₂ = σ_max(inv_basis) = 1/σ_min(basis)`, so scaling by
`σ_min(basis)` makes the result exactly 1-Lipschitz; a 1-Lipschitz function
vanishing on the boundary satisfies `|f(x)| ≤ dist(x, ∂E)`. Computing
`σ_min` needs no SVD library (glam has none): `σ_min = √λ_min(basisᵀ·basis)`,
and the eigenvalues of a symmetric 3×3 have the standard closed-form
trigonometric solution (2×2: quadratic formula) — computed once in the
constructor. This is the tightest single-scalar rescaling: for
`basis = rs·I` it gives `σ_min = rs`, so the eval reduces *exactly* to
today's sphere `|p − x₀| − rs`. Exactly-cubic bases are snapped to
`GeoNodeKind::Sphere` by the constructor and never reach this arm, but the
tight constant still matters at the snap boundary: a *near*-cubic basis just
outside the tolerance evaluates continuously with the snapped sphere,
whereas a looser bound (e.g. the Frobenius norm, off by √3 even for cubic
bases) would make the snap threshold a visible √3 jump in SDF magnitude.
Worst-case underestimation elsewhere is the cell's anisotropy ratio
`σ_max/σ_min`.

**Why conservative is sufficient.** geo_tree's composite SDFs are *already*
only conservative bounds (union/intersection/difference via `min`/`max` are
inexact near feature interactions), so every consumer already lives with the
"exact sign, conservative magnitude" contract:

- `fill_lattice` subdivision culling: an underestimated distance can only
  cull *less*, never incorrectly — correctness unaffected, cost bounded by
  the cell's anisotropy (σ_max/σ_min at worst, negligible recursion
  overhead for real cells).
- The `≤ 0.01 Å` inside-with-margin tests (atom placement, region
  membership): underestimation slightly *widens* the effective margin (to at
  most `0.01·σ_max/σ_min` Å; exactly `0.01` on cubic cells, so the
  materialize regression anchor in Phase 3 is safe) — still far below
  interatomic scales, and the sign itself is exact.
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
    // unit sphere (radius 1.0, NOT scale_to_csg(1.0)), then one affine map;
    // same tessellation density as Sphere
    CSGMesh::sphere(1.0, 24, 12, None)
        .transform(&affine)   // affine = [ scale_to_csg(basis) | scale_to_csg(center) ]
}
```

`Ellipse` likewise: `CSGSketch::circle(1.0, 36, None)
.transform(&affine₂)` (Sketch implements the same trait). A linear map sends
sphere-inscribed vertices to ellipsoid-inscribed vertices one-to-one, so mesh
quality relative to the true surface is unchanged. Degenerate basis → return
`CSGMesh::new()` / `CSGSketch::new()` (empty), matching the SDF arm.

Build the `Matrix4` via the existing glam↔nalgebra helpers in
`csg_utils.rs` (add a `dmat3_to_csg_matrix4`-style helper there if none
fits). Mind the `scale_to_csg` unit scaling: it must be applied **exactly
once**, in the affine map — to the *basis columns* and the *translation*
(both carry length units) — with the source sphere built at plain radius
`1.0`. Scaling both the unit-sphere radius and the basis would square the
factor (`k²·basis·u`); invisible today because `CSG_SCALING = 1.0`
(`csg_utils.rs`), silently wrong the moment that constant changes. The
matrix layout is column-major in both glam and nalgebra.

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

**Non-positive radius guard (both nodes).** `radius` is a user-editable
integer and can be 0 or negative. Before building the basis, keep today's
emission verbatim for `radius <= 0`:
`GeoNode::sphere(real_center, radius as f64 * l.a.length())` (circle:
`GeoNode::circle(real_center, radius as f64 * uc.a.length())` — inline the
`|a|` multiplication; do not keep `int_lattice_to_real` alive for this, see
Cleanup). This is not just back-compat, it is also the correct new
semantics: the fractional ball `|u − c₀| ≤ r` is a single point for `r = 0`
(→ the radius-0 sphere) and **empty** for `r < 0` (→ the negative-radius
sphere, whose SDF is positive everywhere). Without the guard, a negative
radius would silently flip from "empty" to a full-size shape: `basis =
−|r|·L` is still invertible, and neither the snap nor the ellipsoid
membership test can see the sign (`|inv_basis·(p−c)| ≤ 1` describes the same
set for `±basis`).

No cubic-detection branch in the nodes: for `radius ≥ 1` they
unconditionally call `GeoNode::ellipsoid` / `GeoNode::ellipse`, and the
constructor's spherical-basis fast path snaps cubic cells to the existing
`Sphere` / `Circle` primitives. The cubic case is therefore
**byte-identical to today** — same geo hash (no CSG cache miss), same SDF
values, identical materialize counts — while the ellipsoid arms only ever
handle genuinely non-spherical shapes.

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
- *Conservativeness:* for random sample points `x`, `|f(x)| ≤ min_j |x − y_j|
  + ε` over a dense sampling `y_j` of the surface (the sampled min
  overestimates true distance, so this is a valid one-sided check; the
  absolute value matters — a signed bound is trivially true for interior
  points); and `f` has the same sign as the exact membership test
  `|inv_basis·(x−c)| − 1`.
- *Spherical-basis snap (the cubic-regression guard):*
  `GeoNode::ellipsoid(c, s·I)` returns `GeoNodeKind::Sphere` **hash-equal**
  to `GeoNode::sphere(c, s)` — hash equality pins the SDF arm, the CSG arm,
  and cache identity in a single assertion. Repeat with a
  rotated-orthonormal basis `s·R` (still radius `s`, center unchanged).
- *Near-threshold continuity:* a basis perturbed just *outside* the snap
  tolerance stays an `Ellipsoid`, and its eval agrees with the snapped
  sphere's to ~the perturbation size (exercises the σ_min math right where
  it matters — no magnitude jump across the snap threshold).
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
conservativeness, square-basis-snap-vs-`Circle` (incl. hash equality and
near-threshold continuity), degenerate-basis, sketch-vertices-on-ellipse,
batch-consistency. Plus:
- *Extrude integration at the geo level:* `Extrude { shape: Ellipse, .. }`
  3D eval has correct sign inside/outside the extruded elliptic cylinder.

### Phase 3 — `sphere` node emits the ellipsoid

Change `sphere.rs::eval` per above; update its `description`.

**Tests** (`lattice_covariant_primitives_test.rs`):
- *Cubic back-compat (the promise):* sphere node on default diamond
  structure emits a `geo_tree_root` **hash-equal** to
  `GeoNode::sphere(L·c₀, r·|a|)` — the constructor snap makes this exact,
  not approximate; and `sphere → materialize` atom counts on diamond are
  **identical** to the values before this change for **several radii, at
  least r = 1 and r = 4** (hardcode the current counts as regression
  anchors *before* touching the node). Multiple radii matter because
  integer-radius spheres always have lattice sites at distance exactly
  `r·|a|` — boundary sites are the sensitive ones, and a single radius is
  too thin an anchor.
- *`free_sphere` untouched guard:* `free_sphere` still emits
  `GeoNodeKind::Sphere` (one-liner; protects the "stays Euclidean"
  invariant through the constructor work).
- *Non-positive radius:* `r = 0` emits the legacy point sphere
  (`GeoNode::sphere(x₀, 0)`, hash-equal to today); `r = −2` emits an
  everywhere-positive SDF (empty) — byte-identical to today's behavior for
  both, on cubic *and* on a triclinic structure.
- *Discrete covariance (the point of the feature):* for a triclinic
  `structure` (distinct lengths, non-right angles) and `c₀ = 0, r = 3`:
  scanning integer lattice points `u` over a bounded box (e.g.
  `u ∈ [−6, 6]³`), every `u` with `|u| ≤ 3` maps to real `L·u` with
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
- *Square back-compat:* on the default XY plane of a cubic lattice, the
  emitted `geo_tree_root` is **hash-equal** to
  `GeoNode::circle(center_real, r·|a|)` (constructor snap);
  `frame_transform` unchanged.
- *Cubic extrude-chain anchors:* `circle → extrude → materialize` on the
  default XY plane of a cubic lattice — atom counts **identical** to the
  values before this change for at least two radii (hardcode the current
  counts *before* touching the node; extruded circles on the default plane
  are among the most common patterns in the wild).
- *`free_circle` untouched guard:* `free_circle` still emits
  `GeoNodeKind::Circle`.
- *Non-positive radius:* `r = 0` and `r = −2` behave byte-identically to
  today (point circle / empty), mirroring the sphere test.
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
