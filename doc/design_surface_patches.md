# Surface Reconstruction Patches (`patch_build`, `patch_latticefill`)

## Status: Reviewed; ready for implementation (open questions in §8 to settle during build)

## Depends on / relates to

- **`weld_coincident_atoms`** — one small new primitive in `crystolecule` (§3); the only new core machinery this feature needs.
- Built-in record infrastructure (`ElementMapping`) — the representation precedent (§2). (`MaterializeRegion` is a proposed, not-yet-implemented built-in record — see `doc/design_materialize_regions.md`.)
- `materialize` / `fill_lattice` — its hydrogen passivation is reused for residual edge danglers (§5); and the feature is conceptually "fill a region with a patch" the way `materialize` fills a region with a crystal.
- `DrawingPlane` (`rust/src/crystolecule/drawing_plane.rs`) — supplies the in-plane lattice vectors `u_axis/v_axis` for the common 2D-surface case.
- `plane_tiling_vectors` node (`rust/src/structure_designer/nodes/plane_tiling_vectors.rs`) + the `IMat2` type — the implemented, ergonomic way to produce `tiling_vectors` from a `DrawingPlane` + a 2×2 integer superlattice (see §4).

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

The user classifies nothing by hand: the build step flags only the outward atoms as **ghosts** (the *patch-ghost* flag of §4 — and "ghost" means patch-ghost throughout this doc, distinct from the unrelated display-only ghost flag); the rest are real, and everything finer — which real atoms are shared boundaries, which ghosts are neighbour-tile copies vs. bulk collar — is settled by coincidence at weld time.

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
| Precedent | Matches `ElementMapping` (and the proposed `MaterializeRegion`) | None |
| Behaviour on the type | None needed — the patch is pure data; all behaviour lives in `patch_latticefill` | Would carry behaviour, but there is none to carry |
| Dedicated preview / compatibility viz | Separate widget keyed on the record def | Natural home on the type |

**Recommendation: built-in record `Patch`.** Because no atom classification or periodic-bond metadata lives in the patch, it is *pure data of existing types* — exactly the record sweet spot, and a native type would buy nothing but a preview hook (deferrable; a record can be promoted later if the compatibility visualization justifies it).

### Schema

```text
Patch = {
    tile:           Molecule,             // atoms + intra-tile bonds, is_diff = false; includes shared-boundary,
                                          //   ghost, and collar atoms; kept in authored absolute
                                          //   coordinates (§4)
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
/// assert equal bond order), and unions flags. The result keeps the patch-ghost
/// flag (bit 6) only if every fused atom was a patch-ghost — any real atom in the
/// cluster makes the survivor real (the flag is cleared).
/// Used at apply time to realize both periodic (tile↔tile) and bulk (tile↔collar)
/// bonds; a bulk atom's existing bonds are inherited because they are part of the union.
fn weld_coincident_atoms(structure: &mut AtomicStructure, tolerance: f64) { … }
```

Element/flag conflicts: the welded atoms are equal by construction (a shared/ghost atom and its twin, or a collar atom continuing the bulk element); a mismatch is a `warn` (or error under a strict flag), not silent. `tolerance` must be below the smallest interatomic spacing so distinct sites never over-merge (0.1 Å is safely below bond lengths). This is the only new core machinery; the diff system (`apply_diff`, `compose_diffs`) is *not* used — the tile is a real structure, not a delta.

## 4. Node: `patch_build`

The authoring model is **draw, don't assemble.** The user builds an ordinary big slab of the reconstructed surface sitting on its bulk (a `Crystal` or `Molecule`), then draws **one tile's volume** as a normal `Blueprint` (half-spaces, a box on the drawing plane, CSG — the same geometry nodes used everywhere). Both go into `patch_build`, which extracts the tile automatically. The user never marks individual atoms as interior / collar / ghost.

| Pin | Type | Req | Role |
|---|---|---|---|
| `source` | `Crystal` \| `Molecule` (`HasAtoms`) | Yes | The whole authored slab (reconstruction **on its bulk**); only its atoms are read. This is the *source* the tile is extracted from — the stored `patch.tile` is **computed** from it (§"Extraction"), not equal to it. (Pin is deliberately *not* called `tile`, to avoid confusion with the output record's `tile` field.) |
| `lattice` | `Crystal` \| `Blueprint` (`HasStructure`) | Yes | Provides `lattice_vecs` to interpret and verify the integer tiling vectors. |
| `tiling_vectors` | `Array[IVec3]` | Yes | 1–3 periodic directions in `lattice`'s coordinates. |
| `cut_volume` | `Blueprint` | Yes | Geometry of one tile. Defines the interior at build time **and** is stored in the patch to drive removal at apply time — one volume, two uses. |
| → (out) | `Patch` (record) | — | The tileable patch. |

### Extraction (the stored tile is *not* the input)

`patch_build` computes the patch's `tile` from the slab and the cut volume:

1. **Interior** `I` = slab atoms inside `cut_volume` (membership SDF ≤ build threshold `ε`).
2. **Ghosts** `G` = slab atoms *outside* `cut_volume` that are bonded to some atom in `I`; copy them and set the **patch-ghost flag** (`Atom` flags **bit 6**, a freshly allocated bit — see `atom.rs`). This is deliberately **not** the existing display ghost flag (bit 5, `is_ghost`/`set_ghost`), whose semantics are "display-only neighbouring-cell copy in motif_edit mode": that bit is transient render state, whereas the patch-ghost flag is durable structural state that must survive serialization and drive weld survivorship (§3) and the drop step (§5, step 6) without being clobbered by — or clobbering — motif_edit rendering.
3. **Bonds** = every slab bond with **at least one endpoint in `I`** (interior–interior and interior–ghost). Ghost–ghost bonds are dropped.

The result `{ I ∪ G, those bonds }` is stored (normalized to a `Molecule`) as `patch.tile`.

### Why this is exactly the right set

The outside-the-cut atoms bonded to the interior are, by construction, of exactly **two kinds, both needed**:

- **neighbour-tile atoms** — across a tile boundary, in the adjacent reconstruction cell. The ghost welds onto the neighbour tile's real interior atom at apply → realizes the **periodic** bond.
- **bulk collar atoms** — one step into the substrate. The ghost welds onto the surviving substrate atom → realizes the **tile↔bulk** bond and inherits the bulk bonds.

The slab already contains both the neighbouring cells and the bulk, so the single rule captures both. **Distance-1 (direct bonds) suffices**: every boundary-crossing bond has an interior endpoint, so its outer endpoint is captured; atoms further out carry no bond into the interior, and their own bonds arrive with the real twin at weld time (which is why ghost–ghost bonds are dropped).

**Coordination is preserved automatically** because apply does *cut-then-weld* (§5): the cut deletes the old surface atoms inside the volume and, with them, the collar substrate atom's bond *to that old surface*, leaving a dangling bond that the collar ghost's inward bond exactly replaces. If the reconstruction the user drew is coordination-correct, so is the result; the post-weld coordination check (§6) flags it otherwise.

Two conveniences fall out: atoms on the shared cut boundary land inside the cut for *both* adjacent tiles (within `ε`) and simply weld — the "atoms at both ends" closure with no special case; and the same `cut_volume` serves extraction and removal, so there is no second region to keep consistent.

### Coordinate frame: the tile keeps its authored coordinates

`patch_build` keeps the extracted atoms **and the `cut_volume` geometry** in the **absolute coordinates they were drawn in**. No re-expression, no hidden reference cell, no extra stored field. This is correct *because the atoms came straight off an authored crystal*, so they are already lattice-registered in absolute space — every atom sits on a real lattice point plus its motif offset, exactly as the target's atoms do.

`patch_latticefill` then only ever translates the tile by **whole lattice vectors** — the tiling steps `Σ kᵢ·vᵢ` plus the optional integer `origin` offset. A whole-lattice-vector translation maps motif sites onto motif sites, so every weld hits its target; and at the default offset `(0,0,0)` with no tiling step, nothing is translated at all, so the patch reappears **exactly where it was authored**. This is the property that makes the node predictable: *build → apply to the same crystal with the default `origin` is the identity* (modulo the reconstruction itself).

Why this beats a re-expressed local frame: re-anchoring the tile to some internal reference cell `R` (e.g. the cut's min corner) and placing `R` at an absolute `origin` makes the landing position depend on a value the user cannot see — to reproduce the drawing they would have to reverse-engineer `R`. Keeping absolute coordinates removes `R` entirely: the authored position *is* the answer, and `origin` is a transparent whole-cell nudge on top of it (default `0` = as drawn). The reconstruction's **phase** w.r.t. the lattice (which sites pair into dimers, etc.) is preserved automatically because the atom positions are never touched; `origin` shifts that phase by whole cells when the tiling supercell leaves room for it.

This still places two distinct requirements, on two different `patch_latticefill` pins:

- **`target`** (the atom source the collar welds onto) must share the build `lattice`'s **full lattice — `UnitCellStruct` *and* motif registration**, not merely be tiling-commensurate: a collar patch-ghost coincides with a surviving substrate atom only if the tile's motif sites land exactly on the target's. The atoms are what weld, so this constraint is on the structure carrying the atoms — `target` — *not* on `region` (which may be a `Blueprint` carrying a lattice but no bulk atoms at all). Because `target` and `region` are separate pins (§5), this is a real, separately-checkable precondition, not automatic.
- **`region`** (the extent + `lattice_vecs` source) need only supply lattice vectors **commensurate** with `patch.tiling_vectors`, so tiling steps land on lattice points.

Both are naturally satisfied in the common case where the slab is authored from the same crystal that is later passed as both `target` and `region` — which, with the authored-coordinate frame above, is also exactly when the default `origin` lands the reconstruction back on its own surface.

### Behaviour & caveats

- Validate `1 ≤ len(tiling_vectors) ≤ 3` and linear independence. At apply time `patch_latticefill`'s `region` must be tiling-commensurate with build `lattice`, and its `target` must share build `lattice`'s full lattice + motif registration (see above).
- The `cut_volume`'s translates under `tiling_vectors` should **tile the reconstructed strip without gaps** (else old surface atoms survive between tiles); `patch_build` can warn if they don't.
- *Ergonomic vectors (optional):* the canonical input is `tiling_vectors: Array[IVec3]`, but the user need not hand-solve the in-plane crystallography — the **`plane_tiling_vectors`** node (`rust/src/structure_designer/nodes/plane_tiling_vectors.rs`) produces it. Pins: `plane: DrawingPlane` (supplies the in-plane vectors `u_axis/v_axis`) and an optional `superlattice: IMat2` override (else its stored, UI-editable 2×2); output `Array[IVec3]`. Each superlattice **row** is one tiling vector in the `(u_axis, v_axis)` basis — `(1×1)` = identity, diagonal `n×m` = rows `(n,0),(0,m)`, √3×√3 R30° = `(2,1),(-1,1)`, c(2×2) = `(1,1),(1,-1)`. 2D-surface case only; 1D/3D patches enter `tiling_vectors` directly.

## 5. Node: `patch_latticefill`

Tiles a patch across a region and welds it in.

| Pin | Type | Req | Role |
|---|---|---|---|
| `target` | `Crystal` \| `Molecule` (`HasAtoms`) | Yes | The structure being reconstructed. |
| `region` | `Crystal` \| `Blueprint` (`HasStructure`) | No | Where to tile; supplies the substrate `lattice_vecs` and the fill extent. Default: `target`'s extent (then `target` must be a `Crystal`). |
| `patch` | `Patch` (record) | Yes | From `patch_build`. |
| `origin` | `IVec3` | No | Whole-cell **offset** applied to the entire reconstruction. Default `(0,0,0)` = where it was authored (§4); tiling fills the region regardless, with `origin` only shifting the common phase (which sites pair into dimers). A shift by a full tiling vector is a no-op. |
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

1. **Verify commensurability** — each `patch.tiling_vectors[i]` is an integer combination of `region.lattice_vecs` (true by construction when `patch_build`'s `lattice` matched `region`); else error. (This checks `region` only. The separate requirement that `target` share build `lattice`'s lattice + motif registration (§4) is not verified upfront — a mismatch shows up as collar patch-ghosts that fail to weld, caught by the post-weld compatibility check in §6.)
2. **Select cells** `P` — the cells `c = origin + Σ kᵢ·vᵢ` satisfying the containment rule above.
3. **Cut** — for every cell in `P`, remove `target` atoms inside the translated `cut_volume` (dropping their bonds).
4. **Place** — for every cell in `P`, add a copy of `patch.tile` translated by `c` in real space.
5. **Weld** — `weld_coincident_atoms(result, tolerance)` over the placed copies *and* the surviving substrate: fuses tile↔tile (periodic bonds) and tile↔bulk (collar, inheriting bulk bonds) in one pass. A weld including any non-patch-ghost atom yields a real atom; a cluster of only patch-ghosts stays a patch-ghost.
6. **Drop unwelded patch-ghosts** — any atom still flagged patch-ghost found no real twin (it points at a neighbour cell outside `P` — a true reconstruction edge, or a collar with no substrate partner at a true crystal edge); remove it, leaving a dangling bond on the boundary interior atom.
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
4. **Compatibility visualization.** Where to show weld/coordination stats (node subtitle badge vs. panel vs. identicons). Out of scope to build in v1; the data is produced by the apply algorithm (the weld, §5 step 5, surfaced via `apply_patch`'s `CompatibilityReport` — §9 Phase 3).
5. **Deriving a default `cut_volume`.** It is required (it defines the interior), but a sensible default — the in-plane tiling cell extruded to a chosen depth — could be offered so the user only draws one when they want a non-prismatic cut. Nice-to-have, not v1.
6. **Projected containment test (§5).** How exactly to test "cut_volume's periodic footprint ⊆ region's shadow" for arbitrary SDF geometry. Corner-sampling the cell parallelogram is exact for convex region footprints; non-convex regions need denser sampling or a real projection. Settle the sampling density / method; consider whether the region footprint can be precomputed once rather than per cell.
7. **`rm_single` companion to `passivate`.** `materialize` pairs passivation with a "remove ≤1-bond atoms first" toggle; dropping unwelded ghosts can leave a reconstruction atom with a single bond, which passivation would otherwise cap into something spurious. Decide whether to add a matching `rm_single` pin or fold the cleanup into the drop-ghosts step unconditionally.

## 9. Implementation plan

Four bottom-up phases: each lands a self-contained, independently-testable layer before the one above depends on it. **Per `rust/AGENTS.md`, all Rust tests go in `rust/tests/` (never inline `#[cfg(test)]`), mirror the source hierarchy, and are registered in the parent test-crate file** (e.g. add `#[path = "crystolecule/weld_coincident_atoms_test.rs"] mod weld_coincident_atoms_test;` to `rust/tests/crystolecule.rs`). Run with `cd rust && cargo test`. Each phase is "done" only when its listed tests are green and `cargo fmt && cargo clippy` are clean.

All atom-level correctness lives in Phases 1–3 so the welding model is proven on plain `AtomicStructure`s before any UI work — the hard logic is testable without the node-network machinery.

### Phase 1 — Foundations: `weld_coincident_atoms` + patch-ghost flag + `Patch` record

The one new piece of core machinery (§3), the bit-6 accessors reserved in `atom.rs`, and the built-in record that carries a patch (§2). Bundled because each is small, independently testable, and everything above needs all three.

- **Source:**
  - `rust/src/crystolecule/atomic_structure/atom.rs` — add `is_patch_ghost()` / `set_patch_ghost()` for `ATOM_FLAG_PATCH_GHOST = 1 << 6` (bit already reserved).
  - new `rust/src/crystolecule/weld.rs` (declared `pub` in `crystolecule/mod.rs`) — `weld_coincident_atoms(structure: &mut AtomicStructure, tolerance: f64)`: spatial-grid bucket by position (reuse the existing `4.0 Å` grid), cluster atoms within `tolerance`, fuse each cluster into one survivor — union bond lists (dedup by partner, assert equal order), union flags, survivor is patch-ghost iff every member was (else real, flag cleared), rewrite all bond endpoints to the survivor id.
  - `rust/src/structure_designer/node_type_registry.rs` — `built_in_record_type_defs.insert("Patch", …)` next to `ElementMapping`, fields `tile: Molecule`, `tiling_vectors: Array[IVec3]`, `cut_volume: Blueprint` (per §"Schema").
- **Rust tests:**
  - `rust/tests/crystolecule/weld_coincident_atoms_test.rs`:
    1. two coincident atoms fuse into one; bond lists union; partner ids rewritten.
    2. atoms farther apart than `tolerance` do **not** merge (no over-merge below smallest bond length).
    3. real+patch-ghost → survivor is **real** (flag cleared); patch-ghost+patch-ghost → survivor stays patch-ghost.
    4. duplicate bond to same partner dedups; **conflicting bond order panics/`warn`s** per the strictness flag.
    5. bulk-bond inheritance: a collar-like atom welds onto a bulk atom and the survivor carries the bulk atom's outward bonds + the collar's inward bond.
    6. three-way coincident cluster collapses to one survivor.
    7. flag accessor round-trip: `set_patch_ghost(true/false)` toggles bit 6 only, leaving bits 0–5 untouched.
  - `rust/tests/structure_designer/patch_record_test.rs`:
    8. `lookup_record_type_def("Patch")` resolves with the three expected fields and types.
    9. a network with `record_construct`/`record_destructure` on `Patch` validates and round-trips a value.
    10. dangling-ref check: a user record referencing `Patch` is **not** flagged dangling (built-in resolves).

### Phase 2 — `patch_build` extraction

The "draw, don't assemble" authoring step (§4): extract the tile from a slab + cut volume, keeping it in its authored coordinates.

- **Source:** `rust/src/structure_designer/nodes/patch_build.rs` (+ register in `nodes/mod.rs`, `node_type_registry.rs`). Extraction helpers (interior/ghost split, bond selection) factored into a plain function so they test without the node wrapper.
- **Rust tests** — `rust/tests/structure_designer/patch_build_test.rs`:
  1. **interior split:** atoms with SDF ≤ `ε` are interior (real); atoms outside are not interior.
  2. **ghost capture:** an outside atom bonded to an interior atom becomes a patch-ghost; an outside atom with no bond into the interior is excluded; **distance-1 only**.
  3. **bond selection:** interior–interior and interior–ghost bonds kept; ghost–ghost bonds dropped.
  4. **shared-boundary closure:** an atom on the shared cut face lands inside the cut for both adjacent tiles (within `ε`) and is real in both (not a ghost).
  5. **coordinate frame:** extracted atoms keep their authored absolute coordinates (no re-expression) — this is what makes `patch_latticefill`'s default `origin` reproduce the reconstruction in place.
  6. **`HasAtoms` input:** a `Crystal` source and a `Molecule` source yield the same tile (only atoms read).
  7. validation: `1 ≤ len(tiling_vectors) ≤ 3` and linear independence enforced; degenerate vectors error.

### Phase 3 — `patch_latticefill` apply + compatibility stats + round-trip

The core algorithm (§5) plus the two things that fall directly out of it: the compatibility stats (§6) and serialization. This is where the model proves out end to end; keep it node-free-testable via a core `apply_patch(...)` function that also returns the weld/coordination report.

- **Source:** `rust/src/structure_designer/nodes/patch_latticefill.rs` (+ registration). Core `apply_patch(target, region, patch, origin, passivate, tolerance) -> (AtomicStructure, CompatibilityReport)` plus the projected-containment cell-selection helper. The report carries welded-vs-orphaned collar counts and post-weld coordination. `Patch` is a record and both nodes are ordinary node types, so serialization needs **no new plumbing** — the round-trip tests just lock that in.
- **Rust tests:**
  - `rust/tests/structure_designer/patch_latticefill_test.rs`:
    1. **periodic weld (tile↔tile):** two adjacent placed tiles whose shared/ghost atoms coincide weld into a continuous structure; the boundary-crossing (e.g. dimer) bond becomes an ordinary bond; no duplicate atoms remain.
    2. **bulk weld (tile↔collar):** collar patch-ghosts weld onto surviving substrate atoms and inherit bulk bonds; coordination preserved (`bulk —(inherited)— collar —(tile)— interior`).
    3. **cut-then-weld coordination:** the cut removes the displaced surface and the collar's bond to it; the collar's inward bond replaces it — no net dangler at a welded collar.
    4. **drop unwelded patch-ghosts:** a tile at a true edge (no neighbour in `P`) leaves a dangling bond on the boundary interior atom after its ghost is dropped.
    5. **containment rule:** periodic directions require whole-cell containment (no partial lateral tiles); the non-periodic/normal direction is free (cut_volume may stick out). 1D / 2D / 3D periodicity each exercised.
    6. **cut == place cell set:** substrate is never removed in a cell that is not also reconstructed.
    7. **passivate on/off:** `passivate=true` saturates residual danglers; `passivate=false` leaves them exposed.
    8. **tolerance:** distinct-but-close lattice sites do not over-merge at the default `0.1 Å`.
    9. **golden end-to-end:** a small hand-built slab + 2×1-style reconstruction tile over a 2-cell region produces the expected atom/bond count and connectivity (snapshot via `insta` if convenient).
    10. **compatibility stats:** applied too high → orphaned collars > 0; correct depth → zero orphaned, coordination clean; too low → over-coordinated weld flagged.
  - `rust/tests/integration/patch_roundtrip_test.rs`:
    11. a network containing `patch_build` + `patch_latticefill` serializes to `.cnnd` and reloads with identical structure (`normalize_json` for HashMap ordering).
    12. text-format serialize → `edit_network` round-trip is stable.
    13. a tile `Molecule` with patch-ghost-flagged atoms round-trips with bit 6 intact.

### Phase 4 — Flutter UI ✅ DONE (2026-06-16)

Node editors for `patch_build` / `patch_latticefill` (registry-driven add-node is free), the `cut_volume` / `tiling_vectors` wiring (reuse `plane_tiling_vectors`), and the compatibility badge from Phase 3. Covered by `flutter analyze` + the integration smoke test (`integration_test/`); no new Rust tests. Manual `flutter run` walkthrough left to the user.

**Implementation notes:**
- **API layer** (`api/structure_designer/structure_designer_api{,_types}.rs`): `APIPatchBuildData { epsilon }`, `APIPatchLatticeFillData { passivate, tolerance, report: Option<APICompatibilityReport> }`, `APICompatibilityReport { welded_ghosts, orphaned_ghosts, overcoordinated_atoms }` (`usize` → Dart `BigInt`). Scope-aware getters/setters `get_/set_patch_build_data`, `get_/set_patch_latticefill_data`, mirroring `get_/set_collect_data`.
- **Compatibility badge plumbing**: `PatchLatticeFillData` gained `#[serde(skip)] last_report: RefCell<Option<CompatibilityReport>>` (interior mutability — `eval` takes `&self`, same pattern as `MaterializeData::available_parameters`). `eval` clears it at the top and stores the report after a successful `apply_patch` (previously discarded as `_report`); the setter rebuilds with `..Default::default()` since the report is transient.
- **Flutter**: `node_data/patch_build_editor.dart` (ε `FloatInput` + wiring hint), `node_data/patch_latticefill_editor.dart` (passivate checkbox + tolerance `FloatInput` + `_CompatibilityBadge`: green "Compatible" when no orphaned collars and no over-coordination, amber "Check fit" with too-high / too-low hints otherwise, "not yet evaluated" when the node hasn't been displayed). Routed in `node_data_widget.dart`; model setters `setPatchBuildData` / `setPatchLatticefillData`.

**Deferred (post-v1, per §7 / open questions):** multi-face stitching, edges/corners, the compatibility visualization beyond a badge, default-`cut_volume` derivation, and the `rm_single` toggle.
