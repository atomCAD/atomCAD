# `patch_latticefill` cell-selection rewrite + debug views

## Status: agreed, implementing

Refines "which cells get a tile" (`doc/design_surface_patches.md` §5) and adds two debug visualizations. Supersedes the original rhombus/anchor approach.

## Problem — three faults in the old `select_patch_cells`

The old test was: build the parallelogram spanned by the tiling vectors, anchored at the tile's min-corner (`tile_reference_anchor`), slide its 4 corners along the surface normal to the region-AABB centre, and require all 4 inside the region. Three things were wrong:

1. **Wrong shape.** The tiling-vector parallelogram is not the actual cut/tile cross-section (e.g. a hexagon), so a tile could pass the rhombus test while its real content pokes past the region (issue raised by mechadense, "patch extends beyond the surface").
2. **Corner-anchor bias.** Hanging the footprint at the *min-corner* and growing it one-directionally shifts the passing set off-centre, so a symmetric target gives an asymmetric selection ("only on the −,− side").
3. **Global-AABB height.** The re-lift height was `region_bounds.center()`, an **axis-aligned** box centre. For a (111) slab tilted w.r.t. XYZ, that centre can sit in the empty wedge *outside* the slab → every projected point tests "outside" → zero cells. It only worked by luck of symmetry.

## Fix — test the interior atoms, projected to a normal-frame centre depth

For a candidate cell offset `o`:
- place each **interior** (non-ghost) tile atom at its absolute position `p = a.position + lattice·o`;
- project it onto the **test plane**: keep its in-plane coordinates, overwrite each non-periodic component with `center_depth[d]`;
- the cell is selected **iff every interior atom lands inside the region** (`region_volume(s) ≤ 0`, or the bounds when there is no volume).

`center_depth` per free (non-periodic) direction `d` has **two selectable sources** (boolean `test_height_at_origin`, default **`false`** = target-derived):

- **Target-derived (default).** `center_depth` = midpoint of the **target atoms'** min/max of `position·d`, measured *before* cutting. This is the one axis of an oriented bounding box that matters; measured along the real normal it always lies between the slab's bottom and top layers, so it is inside a prismatic region regardless of tilt **or offset from the origin** (e.g. a thin slab parked at a non-zero height). Robust but requires the target-atom scan. **Default**, because real surfaces are authored at the height where they sit, not at the lattice origin — the origin version silently selects nothing for an off-origin target.
- **Origin (opt-in).** `center_depth = 0` along every free direction — project onto the periodic subspace through the **lattice origin**. Simplest and most predictable, and the lattice origin has height 0 in both the source and target frames (shared lattice) so the `origin` pin offset never moves it; but it selects **nothing** unless the target straddles the origin along the normal. The "nothing placed" outcome is surfaced by `CompatibilityReport.placed_cells == 0` (the badge flags it rather than showing a misleading green).

Either way, using the atoms themselves as the sample set (each is, by construction, inside the cut) fixes faults 1 and 2: real shape, true position, no synthetic anchor. The height source only affects fault 3 — and only when the target does not straddle the origin.

**The `origin` pin offset is accounted for.** The test projects each interior atom at its **target-mapped** position `p + lattice·(origin + Σ kᵢ·vᵢ)`, not its source position. The offset's in-plane part shifts the tested footprint (the phase slide); its normal part shifts every atom's height uniformly and is then freed by the projection — correct, since selection is deliberately normal-free and a floating offset is caught by the weld, not by cell selection.

Ghost atoms are excluded from the test (they are meant to extend into the neighbour cell / bulk and weld-or-drop). For a 1-D edge patch there are two free directions; compute a `center_depth` for each and project in both. The fully general (non-prismatic region) answer is a ray-march of the region SDF along the normal — out of scope; the midpoint is exact for flat-face slabs.

**Region geometry is optional.** `region` is `HasStructure` = {`Blueprint`, `Crystal`}, and only `Blueprint` always carries geometry — `CrystalData.geo_tree_root` is `Option`. So `region_volume` may be `None` (a geometry-less Crystal). The lateral inclusion test then falls back to the target atoms' AABB, which is loose for a tilted slab (it over-approximates the footprint). The **height** fix is unaffected either way, because `center_depth` is derived from the target *atoms*, never from the region geometry. A future improvement could derive a lateral footprint from the region's atoms when it has no volume; not needed for the in-scope cases (and not a regression — the AABB was always the lateral fallback).

## Debug view A — project placed atoms to the test plane

Boolean `debug_project_to_test_plane` (default false). When on, the node outputs the patch atoms written to their **projected** positions (in-plane kept, normal = `center_depth`), with **no weld/cut/drop/passivate** — flattening them onto the exact plane the inclusion test runs on, so the user can see each atom's test position against the region footprint and read off why a tile passed or failed. Target atoms are left unprojected (the region geometry is the reference). Non-physical; debug only.

## Debug view B — show frontier tiles

Boolean `debug_show_frontier_tiles` (default false). When on, take the selected cells' integer index range in each periodic direction, widen it to `[min−1, max+1]`, and also place the full **Cartesian product** of those widened ranges (so the selection is "boxed", concavities filled). Atoms from cells that were **not** normally selected are flagged **frozen** (reusing the existing per-atom frozen flag — same bit that renders them distinctly; a viz overload, harmless here since this path never minimizes). Lets the user see the just-excluded neighbours beside the included ones. Frontier tiles are placed raw (not cut, not welded) as an overlay on the real welded result; when A is also on, everything is projected.

**Empty selection.** When *nothing* is selected the `[min−1, max+1]` range is undefined, and an empty frontier would be useless exactly when the user most needs it (to see *why* nothing tiled). So with an empty selection the frontier falls back to the **`[−1, +1]` block around the origin** — a `3ⁿ` ring of frozen tiles showing where the rejected tiles would have gone.

## Invariants

- The **compatibility report is always computed from the normal selection's real weld**, before any debug expansion, so the badge stays truthful in debug modes.
- Both booleans default false → the production path is byte-identical to the non-debug pipeline.
- Empty normal selection → B produces no frontier.

## Implementation

`rust/src/structure_designer/nodes/patch_latticefill.rs`:
- `PatchLatticeFillData` gains `debug_project_to_test_plane`/`debug_show_frontier_tiles` (`#[serde(default)]`), exposed via `get/set_text_properties`; **no new input pins** (debug checkboxes are properties only).
- New helpers `region_center_depths`, `project_to_test_plane`, `point_in_region`; `select_patch_cells` rewritten (atom-based; returns each cell's `k` indices for the frontier box); `compute_frontier`. Drop `tile_reference_anchor`, `footprint_corners`, `corner_in_region_shadow`; keep `free_directions`.
- `apply_patch` gains the two flags and the debug branching; always runs the real weld on the selected cells for the report, then builds the output (real, or projected/frontier overlay).

## Tests (`rust/tests/structure_designer/patch_latticefill_test.rs`)
- **tilted region height:** a region whose XYZ-AABB centre is *outside* the slab selects zero cells under the old rule but the right cells under the normal-depth midpoint.
- **symmetry:** symmetric region + symmetric tiling → symmetric selection (no corner bias).
- **shape poke-out:** an interior atom that would land outside the region rejects its cell.
- **debug A:** placed atoms land at `center_depth`, no hydrogens/weld.
- **debug B:** frontier cells appear, flagged frozen; report unchanged vs. non-debug.

## Flutter (follow-up)
`APIPatchLatticeFillData` gains the two booleans; `patch_latticefill_editor.dart` gains two checkboxes under a "Debug" group.
