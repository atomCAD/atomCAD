# Surface Reconstruction Patches (`patch_build`, `patch_latticefill`)

## Status: Draft (for review)

## Depends on / relates to

- **`weld_coincident_atoms`** — one small new primitive in `crystolecule` (§3); the only new core machinery this feature needs.
- Built-in record infrastructure (`ElementMapping`, `MaterializeRegion`) — the representation precedent (§2).
- `materialize` / `fill_lattice` — its hydrogen passivation is reused for residual edge danglers (§5); and the feature is conceptually "fill a region with a patch" the way `materialize` fills a region with a crystal.
- `DrawingPlane` (`rust/src/crystolecule/drawing_plane.rs`) — supplies the in-plane lattice vectors `u_axis/v_axis` for the common 2D-surface case.
- `doc/design_imat2_and_plane_tiling.md` — the `IMat2` type and the `plane_tiling_vectors` helper that ergonomically produces `tiling_vectors` from a plane + superlattice (see §4). Separate workstream; not required to ship patches.

Background: [atomCAD/atomCAD#347](https://github.com/atomCAD/atomCAD/discussions/347).

## Motivation

A surface reconstruction is periodic: a small per-cell rearrangement (form a dimer, add an adatom, depassivate/repassivate, remove or replace surface atoms) repeats across a crystal face. We want to author that unit **once** and tile it, under two requirements: tiles must be **lattice-commensurate** with the substrate (no gaps, no metric drift — the "boundary conditions"), and the result must be a **covalently connected** structure.

The key simplification: **periodic bonds need not be represented at all — they emerge from coincidence.** If the tile includes the atoms it shares with its neighbours, then after laying tiles out on the tiling lattice, each shared atom from one tile lands on the identical position as the corresponding atom of the next tile. Fusing coincident atoms (a "weld") turns every boundary-crossing bond into an ordinary intra-structure bond. The *same* weld fuses the tile to the surrounding bulk. So the patch's atomic content is a plain `AtomicStructure` — no motif, no fractional coordinates, no diff — and the whole feature rests on one primitive plus a volume cut.

## 1. Model

A **patch** = a **tile** (a `Molecule` — an ordinary atomic structure) + a small set of **integer tiling vectors** (each a lattice translation of the substrate) + a **cut volume**.

The tile is authored in real space and deliberately **includes the atoms it shares with adjacent tiles and with the bulk**:

- a bond crossing a tile boundary onto a shared atom → that atom is in the tile; the neighbour's copy welds to it;
- a bond crossing a boundary with *no* atom on it (e.g. a dimer bond spanning the cell edge) → the tile also includes a **ghost** copy of the neighbour's bonding atom at its real position; the neighbour's real atom welds onto it;
- a bond down into the bulk → the tile includes the bulk **collar** atom it attaches to; on application that collar welds onto the surviving substrate atom and inherits its bulk bonds.

The user classifies nothing by hand: the build step flags only the outward atoms as **ghosts** (§4); the rest are real, and everything finer — which real atoms are shared boundaries, which ghosts are neighbour-tile copies vs. bulk collar — is settled by coincidence at weld time.

**Apply** (`patch_latticefill`), over a fill region:

```
for each tiled cell c (those whose cut_volume fits in the region — §5):
    remove substrate atoms inside the cut_volume at c      // delete the old surface the reconstruction displaces
    place a copy of the tile translated to c
weld all coincident atoms                                  // fuses tile↔tile (periodic) and tile↔bulk (collar) at once
drop unwelded ghosts, then passivate residual danglers     // clean true edges
```

Because each cell `c` is an integer combination of the tiling vectors, and those are integer combinations of the substrate lattice, every placed copy lands at an **exact lattice translation** — which is precisely what makes the welds line up. This is "lattice-matching boundary conditions" turned into atom/bond placement.

## 2. Decision: built-in record vs. new native data type

With the weld model every field of a patch is an existing first-class network type, so a **built-in record** carries it with no baggage:

| | **Built-in record** (recommended) | **New native `DataType::Patch`** |
|---|---|---|
| Plumbing | **None** — serialization, FFI, validation, text format, `record_construct`/`record_destructure`, type dropdowns all exist | **Large** — new `DataType` + `NetworkResult` variant, serde, FFI, conversions, validator, text format, editor |
| Composability | High — assemble/inspect with record nodes; swap the tile or vectors with ordinary nodes | Low — opaque |
| Precedent | Matches `ElementMapping`, `MaterializeRegion` | None |
| Behaviour on the type | None needed — the patch is pure data; all behaviour lives in `patch_latticefill` | Would carry behaviour, but there is none to carry |
| Dedicated preview / compatibility viz | Separate widget keyed on the record def | Natural home on the type |

**Recommendation: built-in record `Patch`.** Because no atom classification or periodic-bond metadata lives in the patch, it is *pure data of existing types* — exactly the record sweet spot, and a native type would buy nothing but a preview hook (deferrable; a record can be promoted later if the compatibility visualization justifies it).

### Schema

```text
Patch = {
    tile:           Molecule,             // atoms + intra-tile bonds, is_diff = false; includes shared-boundary,
                                          //   ghost, and collar atoms; coordinates relative to the patch's
                                          //   lattice-point local origin (§4)
    tiling_vectors: Array[IVec3],         // 1–3 periodic directions, each an integer combination of the
                                          //   substrate lattice; count = periodic dimensionality
    cut_volume:     Blueprint,            // geometry of one tile: defines the interior at build time
                                          //   and removes the displaced substrate atoms at apply time
}
```

Why each field:

- **`tile: Molecule`** — exactly atoms + bonds (`AtomicStructure`), nothing more. The *stored field* is a `Molecule` because the tile needs no substrate lattice, but `patch_build` *accepts* `HasAtoms` (a `Crystal` is fine — its atoms are taken and the structure dropped). *Not* `Motif`/`Structure`, which would force fractional coordinates tied to a cell, element-substitution `parameters`, the `bonds_by_site` caches, and a hard 3D-periodicity assumption (`SiteSpecifier.relative_cell` is always `IVec3`) — all made unnecessary by the weld.
- **`tiling_vectors: Array[IVec3]`** — defines where copies go and, by being integer combinations of the substrate lattice, *guarantees* welds coincide (commensurability is verifiable by construction). Integer, not real-space, so it is verifiable and substrate-relative. The count encodes the periodic dimensionality: 1 = chain/edge, 2 = surface, 3 = bulk twin.

  **Only periodic directions are stored — there is no entry for non-periodic axes, and no separate periodicity flag.** This is complete *because the tile is Cartesian*. In a `Structure`/motif model the non-periodic cell vectors are load-bearing (motif sites are fractional w.r.t. all three, so you need all three to know where the atoms are, plus a flag for which to repeat). Here the atoms are in absolute real space, so the finite extent in any non-periodic direction is already implicit in the atom positions; placement steps only the periodic vectors; the normal/depth comes from `patch_latticefill`'s `origin` plus the tile's own coordinates; and the removal extent comes from `cut_volume`. A non-periodic axis therefore has nothing left to specify, and storing one would be a redundant second source of truth that could drift from the atom positions. (Any orientation a UI wants — e.g. the surface normal — is derived on demand from the spanned vectors, not stored.)
- **`cut_volume: Blueprint`** — does double duty. At **build** time it separates the slab into interior (kept as real tile atoms) and ghosts (§4); at **apply** time it removes the displaced old surface atoms, which welding cannot do (they sit at *different* positions than any tile atom). Because it is intrinsic to extraction it is required (a purely additive patch simply has a volume whose interior contains no removable substrate).

## 3. The `weld_coincident_atoms` primitive

```rust
// crystolecule
/// Fuse atoms that occupy the same position (within `tolerance`) into one.
/// The surviving atom unions both bond lists (dedup by partner; on a duplicate,
/// assert equal bond order), and unions flags. The result is a ghost only if every
/// fused atom was a ghost — any real atom in the cluster makes the survivor real.
/// Used at apply time to realize both periodic (tile↔tile) and bulk (tile↔collar)
/// bonds; a bulk atom's existing bonds are inherited because they are part of the union.
fn weld_coincident_atoms(structure: &mut AtomicStructure, tolerance: f64) { … }
```

Element/flag conflicts: the welded atoms are equal by construction (a shared/ghost atom and its twin, or a collar atom continuing the bulk element); a mismatch is a `warn` (or error under a strict flag), not silent. `tolerance` must be below the smallest interatomic spacing so distinct sites never over-merge (0.1 Å is safely below bond lengths). This is the only new core machinery; the diff system (`apply_diff`, `compose_diffs`) is *not* used — the tile is a real structure, not a delta.

## 4. Node: `patch_build`

The authoring model is **draw, don't assemble.** The user builds an ordinary big slab of the reconstructed surface sitting on its bulk (a `Crystal` or `Molecule`), then draws **one tile's volume** as a normal `Blueprint` (half-spaces, a box on the drawing plane, CSG — the same geometry nodes used everywhere). Both go into `patch_build`, which extracts the tile automatically. The user never marks individual atoms as interior / collar / ghost.

| Pin | Type | Req | Role |
|---|---|---|---|
| `tile` | `Crystal` \| `Molecule` (`HasAtoms`) | Yes | The whole authored slab (reconstruction **on its bulk**); only its atoms are read. Despite the pin name, this is the *source* the tile is extracted from — not the tile itself. |
| `lattice` | `Crystal` \| `Blueprint` (`HasStructure`) | Yes | Provides `lattice_vecs` to interpret and verify the integer tiling vectors. |
| `tiling_vectors` | `Array[IVec3]` | Yes | 1–3 periodic directions in `lattice`'s coordinates. |
| `cut_volume` | `Blueprint` | Yes | Geometry of one tile. Defines the interior at build time **and** is stored in the patch to drive removal at apply time — one volume, two uses. |
| → (out) | `Patch` (record) | — | The tileable patch. |

### Extraction (the stored tile is *not* the input)

`patch_build` computes the patch's `tile` from the slab and the cut volume:

1. **Interior** `I` = slab atoms inside `cut_volume` (membership SDF ≤ build threshold `ε`).
2. **Ghosts** `G` = slab atoms *outside* `cut_volume` that are bonded to some atom in `I`; copy them and set the **ghost flag** (`Atom` flags bit 5, the existing "neighbouring-cell copy" bit).
3. **Bonds** = every slab bond with **at least one endpoint in `I`** (interior–interior and interior–ghost). Ghost–ghost bonds are dropped.

The result `{ I ∪ G, those bonds }` is stored (normalized to a `Molecule`) as `patch.tile`.

### Why this is exactly the right set

The outside-the-cut atoms bonded to the interior are, by construction, of exactly **two kinds, both needed**:

- **neighbour-tile atoms** — across a tile boundary, in the adjacent reconstruction cell. The ghost welds onto the neighbour tile's real interior atom at apply → realizes the **periodic** bond.
- **bulk collar atoms** — one step into the substrate. The ghost welds onto the surviving substrate atom → realizes the **tile↔bulk** bond and inherits the bulk bonds.

The slab already contains both the neighbouring cells and the bulk, so the single rule captures both. **Distance-1 (direct bonds) suffices**: every boundary-crossing bond has an interior endpoint, so its outer endpoint is captured; atoms further out carry no bond into the interior, and their own bonds arrive with the real twin at weld time (which is why ghost–ghost bonds are dropped).

**Coordination is preserved automatically** because apply does *cut-then-weld* (§5): the cut deletes the old surface atoms inside the volume and, with them, the collar substrate atom's bond *to that old surface*, leaving a dangling bond that the collar ghost's inward bond exactly replaces. If the reconstruction the user drew is coordination-correct, so is the result; the post-weld coordination check (§6) flags it otherwise.

Two conveniences fall out: atoms on the shared cut boundary land inside the cut for *both* adjacent tiles (within `ε`) and simply weld — the "atoms at both ends" closure with no special case; and the same `cut_volume` serves extraction and removal, so there is no second region to keep consistent.

### Coordinate frame: the tile's origin

`patch_build` does **not** keep the slab's absolute coordinates — they would leave the tile floating wherever it was authored, making `patch_latticefill`'s `origin` meaningless. It re-expresses the extracted atoms **and the `cut_volume` geometry** relative to a reference **lattice point** `R`, so the patch's local origin `(0,0,0)` is that lattice point; `patch_latticefill` places `R` at the target lattice point named by `origin` and tiles from there.

The reference **must be a lattice point, not one of the tile's atoms.** Placement has to be a pure lattice translation (that is what maps the tile's atoms onto the target's motif sites so the welds land), and `origin` is itself a lattice point. A lattice-point `R` makes `origin − R` a lattice translation → motif sites map to motif sites → every weld hits its target. An *atom* sits at a lattice point **plus a fractional motif offset**; anchoring there would bake that fraction into the local origin, so placing it at a lattice point would shift the whole tile *off* the lattice and every weld would miss. Storing relative to a lattice point also preserves the reconstruction's **phase** w.r.t. the lattice (which sites pair into dimers, etc.), since atom positions relative to `R` are untouched.

`R` is derived deterministically from `cut_volume` — the lattice cell at its reference (min) corner — so `origin` is predictable ("it places the tile's corner cell here"). Any lattice point is correct; the choice only fixes a phase the user then shifts with `origin`. It bakes into the normalized coordinates, so there is no extra stored field and no extra pin.

This requires build `lattice` and apply `region` to be the **same lattice** (same `UnitCellStruct` + motif registration), not merely tiling-commensurate: collar welds coincide with substrate atoms only if the tile's motif sites match the target's. Naturally satisfied when the slab is authored from the crystal it will be applied to.

### Behaviour & caveats

- Validate `1 ≤ len(tiling_vectors) ≤ 3` and linear independence. The substrate the patch is finally tiled onto is `patch_latticefill`'s region, which must share `lattice` (see above).
- The `cut_volume`'s translates under `tiling_vectors` should **tile the reconstructed strip without gaps** (else old surface atoms survive between tiles); `patch_build` can warn if they don't.
- *Ergonomic vectors (optional):* the canonical input is `tiling_vectors: Array[IVec3]`, but the user need not hand-solve the in-plane crystallography. The **`plane_tiling_vectors`** helper node turns a Miller-indexed `DrawingPlane` (which already supplies the in-plane lattice vectors `u_axis/v_axis`) plus a 2×2 integer superlattice into the `Array[IVec3]` — covering `(1×1)`, diagonal `n×m`, and non-diagonal cells (√3×√3 R30°, c(2×2)). It is specified, together with the `IMat2` type it uses, in **`doc/design_imat2_and_plane_tiling.md`**. 2D-surface case only; 1D/3D patches enter `tiling_vectors` directly.

## 5. Node: `patch_latticefill`

Tiles a patch across a region and welds it in.

| Pin | Type | Req | Role |
|---|---|---|---|
| `target` | `Crystal` \| `Molecule` (`HasAtoms`) | Yes | The structure being reconstructed. |
| `region` | `Crystal` \| `Blueprint` (`HasStructure`) | No | Where to tile; supplies the substrate `lattice_vecs` and the fill extent. Default: `target`'s extent (then `target` must be a `Crystal`). |
| `patch` | `Patch` (record) | Yes | From `patch_build`. |
| `origin` | `IVec3` | No | Target lattice point at which the patch's local origin (its `cut_volume` corner cell, §4) is placed; tiling fills from there. Default: region centre. |
| `passivate` | `Bool` | No | Hydrogen-passivate the danglers left after welding (the dropped-ghost reconstruction edges, and any under-coordinated atoms). Default `true`. Set `false` to keep those danglers exposed — e.g. when a later `patch_latticefill` on an adjacent face is meant to bond to them — and passivate once at the end. (Matches `materialize`'s `passivate`.) |
| `tolerance` | `Float` | No | Weld tolerance (Å). Default 0.1. |
| → (out) | `Crystal` | — | The reconstructed crystal. |

### Which cells get a tile (how `region` is used)

Tiling must stop at the region boundary. The rule is asymmetric between the periodic and non-periodic directions:

> **Place a tile at cell `c` iff its `cut_volume`, translated to `c` and *projected onto the subspace spanned by `tiling_vectors`*, lies fully inside `region` — with no containment requirement along the non-periodic direction(s).**

- **Periodic directions — full containment.** A reconstruction cell is placed only where a *whole* cell fits within the region's footprint, so no partial tiles appear at the lateral edges (no "edge/corner territory"). Cells whose footprint pokes past the region are skipped.
- **Non-periodic direction(s) — free.** Along the surface normal the `cut_volume` legitimately sticks out of the region: it must reach the surface atoms it replaces *and* things just outside the nominal crystal volume — e.g. the passivation hydrogens above the top face. Requiring containment there would reject every cell, so the test ignores the non-periodic coordinate (it compares against the region's *shadow* along that axis).

For a 2D surface patch the non-periodic axis is the normal `v1 × v2`, and the test is whether the cut_volume's `v1`–`v2` footprint at `c` is inside the region's footprint in that plane, free along the normal. For a 1D edge patch the periodic subspace is the line along `v1` and both transverse directions are free. With 3 periodic vectors there is no free direction and the rule degenerates to ordinary full-3D containment.

*Implementation:* sample the cut_volume's periodic footprint (the cell parallelogram corners suffice for a convex region; a denser grid otherwise) and test each sample against the region's shadow via a ray/SDF test along the non-periodic axis. Exact projection of arbitrary geometry is an implementation detail (Open Question 6).

### Algorithm

1. **Verify commensurability** — each `patch.tiling_vectors[i]` is an integer combination of `region.lattice_vecs` (true by construction when `patch_build`'s `lattice` matched `region`); else error.
2. **Select cells** `P` — the cells `c = origin + Σ kᵢ·vᵢ` satisfying the containment rule above.
3. **Cut** — for every cell in `P`, remove `target` atoms inside the translated `cut_volume` (dropping their bonds).
4. **Place** — for every cell in `P`, add a copy of `patch.tile` translated by `c` in real space.
5. **Weld** — `weld_coincident_atoms(result, tolerance)` over the placed copies *and* the surviving substrate: fuses tile↔tile (periodic bonds) and tile↔bulk (collar, inheriting bulk bonds) in one pass. A weld including any non-ghost atom yields a real atom; a cluster of only ghosts stays a ghost.
6. **Drop unwelded ghosts** — any atom still flagged ghost found no real twin (it points at a neighbour cell outside `P` — a true reconstruction edge); remove it, leaving a dangling bond on the boundary interior atom.
7. **Passivate** — if `passivate` (default true), hydrogen-passivate the residual danglers (reusing `materialize`'s hydrogenation). Wrap as `Crystal`.

Cut and place share the same cell set `P`, so the cut never removes substrate it does not then reconstruct. Cells outside `P` keep their original (un-reconstructed) surface; the boundary between them and the reconstructed area is passivated by steps 6–7.

(`target` and `region` are separate pins because in 3D the fill volume need not match the workpiece's volume.)

## 6. Bonding & boundary conditions

The "boundary conditions" are the **geometric lattice-matching** of §1/§5 — the precondition that makes welding well-defined, not the bonding step itself. Both interfaces are the same weld:

- **Tile ↔ tile (periodic).** Shared boundary atoms (and ghosts of cross-edge bond partners) coincide between adjacent tiles and weld; the boundary-crossing bond becomes an ordinary bond. Dimer bonds spanning a cell edge work because the ghost atom welds onto the neighbour's real atom.
- **Tile ↔ bulk (aperiodic boundary).** The tile's **collar** atoms overlap surviving substrate atoms and weld onto them; the merged atom inherits the substrate atom's outward bulk bonds (they are part of the union) while keeping the tile's inward bonds — continuous `bulk —(inherited)— collar —(tile)— interior`. Anchor the collar to the **less-relaxed sub-surface layer** (real reconstructions displace the top layer, while sub-surface shifts are minimal — e.g. Si(111)).
- **True edges** (a collar atom with no substrate partner, at the crystal boundary) leave a dangler that passivation saturates — the deferred edges/corners case (§7).

A **compatibility check** falls out for free: count collar atoms that weld vs. those left orphaned, and check post-weld coordination — this catches the natural failure modes (a patch applied too high → floating, un-welded collars; applied too low / sub-surface → over-coordinated welds). It is also the natural data source for a compatibility visualization (e.g. identicons).

## 7. Scope / non-goals (v1)

- **One face at a time.** Multi-face stitching and **edges/corners** are out — deferred to passivation/manual.
- **Commensurate substrates only.** Patch and region must share a lattice (integer tiling vectors); genuinely incommensurate interfaces are out.
- **Nearest-neighbour-range boundary bonds.** Bonds reaching more than one cell need ghost atoms that far out; supported, but the common case is one cell.

## 8. Open questions

1. **Build threshold `ε`.** The interior/ghost split uses `cut_volume` membership SDF ≤ `ε` (§4). It must be large enough to catch atoms authored right on the cut surface but smaller than the nearest interplanar spacing so it never grabs the layer below — same trade-off as `materialize`'s region margin. Confirm a default.
2. **Where the cut happens.** `cut_volume` is per-tile and tiled with the patch; alternatively a single region-wide cut. Per-tile is more local and composes with the patch; proposed default.
3. **Strictness of welding.** On element/flag mismatch or an un-welded collar (floating patch), warn vs. error. Proposed: surface as a non-blocking compatibility badge (§6) by default, with an `error_on_incompatible` flag.
4. **Compatibility visualization.** Where to show weld/coordination stats (node subtitle badge vs. panel vs. identicons). Out of scope to build in v1; the data is produced by step 4.
5. **Deriving a default `cut_volume`.** It is required (it defines the interior), but a sensible default — the in-plane tiling cell extruded to a chosen depth — could be offered so the user only draws one when they want a non-prismatic cut. Nice-to-have, not v1.
6. **Projected containment test (§5).** How exactly to test "cut_volume's periodic footprint ⊆ region's shadow" for arbitrary SDF geometry. Corner-sampling the cell parallelogram is exact for convex region footprints; non-convex regions need denser sampling or a real projection. Settle the sampling density / method; consider whether the region footprint can be precomputed once rather than per cell.
7. **`rm_single` companion to `passivate`.** `materialize` pairs passivation with a "remove ≤1-bond atoms first" toggle; dropping unwelded ghosts can leave a reconstruction atom with a single bond, which passivation would otherwise cap into something spurious. Decide whether to add a matching `rm_single` pin or fold the cleanup into the drop-ghosts step unconditionally.
