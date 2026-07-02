# Design: `free_sphere` and `free_circle` nodes (issue #381)

**Issue:** https://github.com/atomCAD/atomCAD/issues/381 — "circle and sphere
nodes should have non lattice aligned analogs too"

## Motivation

`sphere` and `circle` take integer lattice coordinates (`center: IVec3` /
`IVec2`, `radius: Int`), so cutters can only be placed and sized in whole
lattice steps. Users who want a sphere "between" lattice points today must
compose `sphere` + `free_move` (Blueprint only — `free_move` does not accept
`Geometry2D`, so circles have **no** workaround at all), and there is no way to
get a non-whole-cell radius in either dimension.

We add two new primitive nodes authored directly in **real-space (Å) float
coordinates**:

| Node | Analog of | Input pins | Output |
|---|---|---|---|
| `free_sphere` | `sphere` | `center: Vec3`, `radius: Float`, `structure: Structure` (opt, default diamond) | `Blueprint` |
| `free_circle` | `circle` | `center: Vec2`, `radius: Float`, `d_plane: DrawingPlane` (opt, default XY) | `Geometry2D` |

## Why this shape (alternatives considered)

- **Optional `free_radius` override pin on the existing nodes** — rejected.
  Mixed unit systems on one node (lattice center + real radius) make the
  node's meaning depend on which pins are wired; the center would still be
  lattice-quantized, requiring `free_move` anyway; and circles would still
  need a brand-new `free_move_2d`, so it doesn't even save a node type.
- **Free variants with `IVec3` center + `free_move` for positioning** —
  rejected. Only useful when composed with `free_move` (which `Geometry2D`
  can't be), and the primitive's `center` parameter becomes a lie once a
  `free_move` sits downstream.
- **Float *lattice* coordinates (fractional cells)** — rejected. On a
  non-cubic cell "radius 1.5 cells" is ambiguous (see Background below), and
  the issue explicitly asks for non-lattice-aligned placement. Users who want
  lattice-relative-but-fractional placement can compute real coordinates from
  `lattice_vecs_unpack` outputs in an `expr` and wire the resulting `Vec3`.

The chosen design is nearly free to implement because of a key codebase fact:
**the geo_tree is already real-space**. `sphere.rs` converts lattice→real *at
eval time* (`ivec3_lattice_to_real(&center)`, `int_lattice_to_real(radius)`)
and then builds `GeoNode::sphere(real_center, real_radius)`. The free variants
simply skip the conversion; all downstream machinery (CSG, `materialize`,
`fill_lattice`, extrude) consumes real-space SDFs unchanged.

## Background: how the lattice nodes behave on non-cubic cells

(Answers the side question in the issue.) Today's `sphere` is a **true round
sphere in real space** even on non-cubic lattices: the center uses the full
basis (`x·a + y·b + z·c` via `ivec3_lattice_to_real`), but the radius is
scaled by `|a|` only (`int_lattice_to_real` = `value * a.length()`). It is
never an ellipsoid; rather, "radius 2" spans different cell counts along
`b`/`c` than along `a` on anisotropic cells. The free variants sidestep this
entirely: radius is given in Å.

## Naming

`free_sphere` / `free_circle` — matches the established `free_move` /
`free_rot` convention: **free = real-space float coordinates**. The names sort
adjacent to `free_move`/`free_rot` in the alphabetical add-node palette.
Categories are `Geometry3D` / `Geometry2D` respectively (same as their lattice
analogs — these are geometry primitives, not movement nodes).

## Alignment decision: `Aligned`

`free_sphere` emits `BlueprintData { alignment: Alignment::Aligned,
alignment_reason: None }`, **not** `LatticeUnaligned`.

Rationale is `doc/design_blueprint_alignment.md` §3.5: geometry primitives
whose *cutting geometry* sits at fractional lattice positions (e.g.
`half_space` with fractional d-spacing via `subdivision`) do **not** affect
alignment — atoms are always placed on motif sites during materialization;
the cutter merely decides which atoms survive. A free-positioned sphere is
exactly such a fractional cut. Marking it unaligned would put a scary badge on
the feature's headline use case for no downstream risk.

Note the deliberate asymmetry with `free_move` (which unconditionally promotes
a Blueprint to `LatticeUnaligned` per §3.3): `free_move` acts on arbitrary
already-built Blueprints and taints conservatively; `free_sphere` *is* the
primitive and knows its geometry is just a cutter. If review prefers strict
consistency with `free_move`, flipping this is a two-line change in `eval` —
but §3.5 is the controlling precedent.

`free_circle` needs no decision: `GeometrySummary2D` carries no alignment
field, and `extrude` emits `Aligned` — consistent with the above.

## Implementation shape

Both nodes mirror their lattice analog's file almost line-for-line, minus the
lattice→real conversion, plus float storage. No eval cache, **no gadget**
(`provide_gadget` returns `None` — same as `sphere`/`circle`/`cuboid` today;
see Deferred work).

### `free_sphere` (`nodes/free_sphere.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeSphereData {
    #[serde(with = "dvec3_serializer")]   // exists in util/serialization_utils.rs
    pub center: DVec3,
    pub radius: f64,
}
```

- `eval`: `evaluate_or_default` pin 0 with `NetworkResult::extract_vec3`,
  pin 1 with `extract_float`, pin 2 with `extract_structure` (default
  `Structure::diamond()` — the Blueprint still needs a lattice for downstream
  `materialize`), then:

  ```rust
  EvalOutput::single(NetworkResult::Blueprint(BlueprintData {
      structure,
      geo_tree_root: GeoNode::sphere(center, radius),  // no conversion
      alignment: Alignment::Aligned,
      alignment_reason: None,
  }))
  ```

- Defaults: `center = DVec3::ZERO`, `radius = 5.0` (Å; arbitrary visible
  size — the lattice `sphere`'s default of 1 cell is a structure-dependent
  length (3.567 Å on diamond), so there is no single canonical float
  default).
- `get_subtitle`: same show-unless-wired logic as `sphere`, float-formatted
  like `free_move` (`{:.2}`): `c: (1.50, 2.25, -0.75) r: 4.20`.
- `get_text_properties` / `set_text_properties`: `TextValue::Vec3` /
  `TextValue::Float`; read back via `as_vec3()` / `as_float()`. These
  accessors accept the integer variants too (`as_vec3` converts an `IVec3`),
  which matters because the text-format parser produces `IVec3` for
  whole-number component lists like `(1, 2, 3)` — using `as_vec3`/`as_vec2`
  (as `free_move` does) makes `center: (1, 2, 3)` just work.
- `get_parameter_metadata`: `structure` optional with `"diamond"` default
  label (copy `sphere`).
- Radius validation: none in `eval`, matching `sphere` (whose editor-side
  `minimumValue: 1` is the only guard and wired pins bypass it anyway). A
  non-positive radius yields an empty/degenerate SDF, which is harmless.

### `free_circle` (`nodes/free_circle.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeCircleData {
    #[serde(with = "dvec2_serializer")]   // exists
    pub center: DVec2,
    pub radius: f64,
}
```

- `eval`: pin 0 `extract_vec2`, pin 1 `extract_float`, pin 2
  `extract_drawing_plane` (default `DrawingPlane::default()`), then — exactly
  `circle.rs` minus the `effective_unit_cell` conversions:

  ```rust
  EvalOutput::single(NetworkResult::Geometry2D(GeometrySummary2D {
      drawing_plane,
      frame_transform: Transform2D::new(center, 0.0),
      geo_tree_root: GeoNode::circle(center, radius),
  }))
  ```

  `center` is in real-space Å **within the drawing-plane frame** — the plane
  itself remains lattice-derived; only the position/size within it is free.
- Defaults: `center = DVec2::ZERO`, `radius = 5.0`.
- Subtitle / text properties / parameter metadata (`d_plane` → `"XY plane"`):
  mirror `circle` with the float formatting above.

### Registration

- `nodes/mod.rs`: `pub mod free_circle;` / `pub mod free_sphere;`
  (alphabetical).
- `node_type_registry.rs` `create_built_in_node_types()`: register
  `free_circle` next to `circle` (Geometry2D block) and `free_sphere` next to
  `sphere` (Geometry3D block).
- `public: true`, `zone_input_pins`/`zone_output_pins` empty,
  `calculate_custom_node_type` → `None`, `generic_node_data_saver`/`loader`.
- Node `description` strings should state the units explicitly, e.g.
  "Outputs a sphere with real-space (Å) center coordinates and radius —
  the non-lattice-aligned analog of `sphere`."

### Type-system freebies (no code)

`Int → Float` and `IVec3 → Vec3` / `IVec2 → Vec2` are existing implicit wire
conversions, so an `int` node into `radius` or an `ivec3` into `center` works
out of the box. Pins are static (not property-driven), so the drag-aware
add-node popup surfaces both nodes with **no `adapt_for_drag_source`**.

### Validation / repair

Nothing new. Fixed pins, no zones, no parameter-interface impact; standard
wire type-checking covers mis-typed inputs.

### Text format

Stored properties serialize through the existing `TextValue::Vec3` / `Vec2` /
`Float` arms in `text_format/serializer.rs` and parse through the component-
count dispatch in `parser.rs` — no parser/serializer change:

```
fs = free_sphere { center: (1.5, 2.25, -0.75), radius: 4.2 }
fc = free_circle { center: (0.5, 1.25), radius: 3.0 }
e  = extrude { shape: fc, height: 2 }
```

### API + Flutter (property panel)

Mirror the `sphere`/`circle` plumbing exactly:

- **Rust API** (`api/structure_designer/structure_designer_api.rs` + types):
  `APIFreeSphereData { center: APIVec3, radius: f64 }` and
  `APIFreeCircleData { center: APIVec2, radius: f64 }`;
  `get_free_sphere_data` / `set_free_sphere_data` (and circle equivalents),
  `#[frb(sync)]`, **taking `scope_path: Vec<u64>`** like every sibling
  property API (rust/AGENTS.md "Addressing Nodes Across Scopes" — a bare
  `node_id` getter is exactly the mistake to avoid). Setters follow the same
  undo/refresh pattern as `set_sphere_data` (snapshot node data, mutate,
  refresh).
- **FRB codegen** after the API edits:
  `flutter_rust_bridge_codegen generate`.
- **Flutter**: `free_sphere_editor.dart` / `free_circle_editor.dart` in
  `lib/structure_designer/node_data/`, cloned from `sphere_editor.dart` /
  `circle_editor.dart` with `Vec3Input`/`Vec2Input` + `FloatInput` (all exist
  in `lib/inputs/`). Register both cases in `node_data_widget.dart`, passing
  `scopePath: model.propertyEditorScopePath` on the getter like siblings.
  Model methods `setFreeSphereData` / `setFreeCircleData` in
  `structure_designer_model.dart` forward `propertyEditorScopeChain` and call
  `refreshFromKernel()`.

## Phased plan

Each phase compiles and tests green on its own. Tests go in `rust/tests/`
(never inline), in a new `rust/tests/structure_designer/free_geometry_nodes_test.rs`
registered in `rust/tests/structure_designer.rs`. Build/test commands per
project convention (`cargo test -j 4` on Windows; never two cargo commands
concurrently).

### Phase 1 — `free_sphere` (Rust core)

- `nodes/free_sphere.rs` per the shape above; register in `nodes/mod.rs` +
  `node_type_registry.rs`.
- **Tests** (`free_geometry_nodes_test.rs`, new, registered):
  - *SDF placement:* stored `center: (1.5, 2.25, -0.75)`, `radius: 4.2` →
    output is `Blueprint`; `geo_tree_root` implicit eval ≈ `-4.2` at the
    center, ≈ `0` at `center + (4.2, 0, 0)`, `> 0` well outside. (This is the
    load-bearing "no lattice quantization" assertion — the center is not
    representable in whole cells.)
  - *Roundness on a non-cubic lattice:* wire a non-cubic `structure` (e.g.
    distinct a/b/c lengths); SDF is ≈ 0 at `center + r·d` for several
    non-axis unit directions `d` — the sphere stays round in real space
    regardless of the structure input.
  - *Alignment:* output `alignment == Aligned`, `alignment_reason == None`.
  - *Wired pins override stored:* `vec3` → `center`, `float` → `radius`;
    plus one implicit-conversion case (`int` node → `radius`).
  - *Materialize integration:* `free_sphere → materialize` yields a Crystal
    with `> 0` atoms; shifting `center` by half a lattice vector changes the
    resulting atom set (demonstrates sub-cell sensitivity — the point of the
    feature).
  - *Text properties:* `get_text_properties`/`set_text_properties` roundtrip,
    including setting `center` from a `TextValue::IVec3` (whole-number parse
    path).
- **Deliverable:** real-space spheres cut crystals at arbitrary positions.

### Phase 2 — `free_circle` (Rust core)

- `nodes/free_circle.rs`; register in both files.
- **Tests** (same module):
  - *SDF placement:* stored fractional center/radius → `Geometry2D` whose
    `geo_tree_root` 2D implicit eval matches (inside/boundary/outside), and
    `frame_transform` translation equals the stored center.
  - *Default plane:* no `d_plane` wired → default XY drawing plane (mirror
    whatever `circle`'s existing default-plane behavior/tests assert).
  - *Extrude chain:* `free_circle → extrude → materialize` yields atoms;
    shifting the circle center by a sub-cell amount changes the atom set.
  - *Wired pins override stored* + *text properties roundtrip* (including the
    `IVec2`-from-whole-numbers path via `as_vec2`).
- **Deliverable:** the 2D sketch workaround in the issue screenshot is
  obsolete.

### Phase 3 — Text format + serialization roundtrips

- **Text format tests** (`text_format_test.rs`): the examples above parse →
  serialize → reparse identically; explicitly cover whole-number component
  lists (`center: (1, 2, 3)` on `free_sphere` — parser yields `IVec3`,
  `as_vec3` accepts) and float formatting stability of `format_float`.
- **`.cnnd` roundtrip** (`serialization_test.rs` / the `cnnd_roundtrip`
  suite): a network using both nodes (with wires into `center`/`radius`)
  survives save/load with data and wires intact.
- **Snapshots:** the node snapshot suite loads fixture `.cnnd` files rather
  than enumerating registered types, so registration alone produces no
  pending snapshots (verified for the unpack nodes). Optionally add a small
  fixture using both nodes and `cargo insta review`; skip if Phases 1–2
  already cover eval output (they do).
- **Registry assertions:** re-grep before committing in case exact-count or
  name-list assertions have landed:
  `rg -n "add_node_type|built_in_node_types\.len" rust/tests`.
- **Deliverable:** both nodes are first-class citizens of the AI text format
  and project files.

### Phase 4 — API + Flutter property editors

- Rust API types/getters/setters (with `scope_path`) + FRB codegen + the two
  Dart editors + `node_data_widget.dart` registration + model methods, per
  the API + Flutter section above.
- **Automated tests:** none new on the Rust side (thin API wrappers — project
  convention is to test the underlying core, done in Phases 1–2). Run the
  full gate: `cargo fmt && cargo clippy && cargo test`, `dart format`,
  `flutter analyze`, and the Flutter smoke test
  (`flutter test integration_test/`).
- **Manual walkthrough:** add each node from the popup (and via drag-from-pin
  for a `Vec3`/`Float` source), edit center/radius in the panel with live
  viewport update, wire/unwire pins and confirm the editor stays in sync with
  the subtitle, materialize a free_sphere and extrude a free_circle, Ctrl+Z
  through property edits.
- **Deliverable:** feature complete for issue #381 (panel-driven editing).

## Deferred work (explicitly out of scope)

- **Viewport gadgets.** `sphere`, `circle`, and `cuboid` have no gadgets
  today (`provide_gadget` → `None`); the free variants launch at parity,
  panel-edited. A later `FreeSphereGadget` can reuse `xyz_gadget_utils` (as
  `FreeMoveGadget` does) with no snapping logic, plus a radius handle;
  `free_circle` would get an in-plane gadget. Nothing in this design blocks
  that — but no gadget code, eval caches, or "free move tool" work now.
- **`free_cuboid` / `free_rect` / other free primitives.** Same pattern would
  apply; add on demand.
- **Ellipsoid / per-axis radii.** Orthogonal feature; not requested.
- **Expr-language construction of Blueprints.** Nodes cover the need.
