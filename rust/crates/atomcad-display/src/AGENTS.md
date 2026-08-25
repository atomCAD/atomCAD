# atomcad-display - Agent Instructions

The **domain → renderer adapter**: it turns `atomcad-crystolecule` structures,
`atomcad-geo-tree` geometry and display preferences into the meshes
`atomcad-renderer` draws. It is the only crate that depends on both the domain
and the renderer, which is what makes it the adapter — and it must never depend
on `atomcad-structure-designer` or on the root crate. See `rust/AGENTS.md` for
the workspace DAG and for why `scene_tessellator` lives one layer up.

## Module map

| Module | What it adapts |
|---|---|
| `atomic_tessellator` | atoms and bonds → triangle meshes or impostors |
| `csg_to_poly_mesh` | `GeoNode` → `PolyMesh` (csgrs, earcut for sketches) |
| `poly_mesh` / `poly_mesh_tessellator` | face-centric mesh + its wireframe/solid tessellation |
| `surface_point_cloud` / `surface_point_tessellator` | SDF surface splatting |
| `isosurface/` | `IsosurfaceData` → `SurfaceMesh` (marching cubes) |
| `coordinate_system_tessellator`, `unit_cell_wireframe_tessellator` | overlays |
| `gadget`, `half_space_utils`, `xyz_gadget_utils` | shared gadget geometry, half hit-test |
| `preferences` | `DisplayPreferences` and friends |

## Invariants

- **Ångström in, Ångström out.** Every coordinate crossing this crate's
  boundary is real-space Ångström, matching `AtomicStructure` and the
  `ScalarField` contract. Loaders convert units once, at load time; nothing here
  converts.
- **Triangles are counter-clockwise seen from outside.** Back-face culling and,
  for translucent surfaces, the two-pass `cull_mode: Front` then `Back` draw
  both depend on it. An inverted mesh does not look obviously broken — it looks
  slightly wrong — so winding is asserted, not eyeballed.
- **Quality settings come from preferences, never from the value.** Resolution,
  smoothing angle and sample density are parameters of *this* layer;
  a node's value stays resolution-free. That is why every adapter here takes a
  preferences argument rather than reading node data.

## Surface extraction (`isosurface/`)

Design doc: `doc/design_isosurface_node.md`.

The marching-cubes case table is **generated from a face-locality rule**, not
transcribed from the classic 15-case table, because the classic one leaves holes
on ambiguous faces. If you touch `case_table.rs`, the property to preserve is:

> the segments a cell's patch leaves on a face are a function of that face's
> four corner signs alone.

Closure across the whole grid follows from it by induction, for any field.
`isosurface_case_table_test.rs` checks it directly over all 256 masks; a
per-example "the sphere came out closed" count does **not** imply it.

Two pitfalls that cost real time:

- **The level set is clipped where it leaves the sampled box.** The extractor
  marches exactly the lattice `Lattice` describes and adds no capping layer, so
  a field whose value exceeds the level on a boundary plane yields a surface
  that is *open* there. Everything downstream assumes closed surfaces. Test
  fixtures must therefore pick a level above the fixture's boundary maximum, and
  the tests in `tests/display/isosurface_*.rs` say so where they do.
- **A tie is not exact.** `SampledField::sample` round-trips a point through
  `inv_basis`, so a stored value that equals the level comes back a few ULPs
  off. The coincident-vertex merge is therefore quantized with a 27-cell probe,
  not keyed on exact bits — see `merge_coincident`.

## Testing

Tests live in `crates/atomcad-display/tests/display/` and **must be registered
in `tests/display.rs`** with a `#[path]` module. An unregistered file silently
never compiles and never runs — the same failure mode the workspace
`default-members` comment warns about.

The two tests that drive `tessellate_scene_content` are *not* here: that
function is a `structure_designer` module, and a test's home is decided by what
it imports.
