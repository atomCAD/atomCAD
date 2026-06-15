# Surface Reconstruction Patches (`patch_build`, `patch_latticefill`)

## Status: Draft (for review)

## Depends on / relates to

- **`weld_coincident_atoms`** — one small new primitive in `crystolecule` (§3); the only new core machinery this feature needs.
- Built-in record infrastructure (`ElementMapping`, `MaterializeRegion`) — the representation precedent (§2).
- `materialize` / `fill_lattice` — its hydrogen passivation is reused for residual edge danglers (§5); and the feature is conceptually "fill a region with a patch" the way `materialize` fills a region with a crystal.
- `DrawingPlane` (`rust/src/crystolecule/drawing_plane.rs`) — the ergonomic way to obtain `tiling_vectors` for the common 2D-surface case (Miller index + shift → in-plane primitive vectors).

GitHub discussion: [atomCAD/atomCAD#347](https://github.com/atomCAD/atomCAD/discussions/347). This is a concrete, reduced-scope realization of mechadense's `patch_build` / `patch_latticefill` proposal; deviations are listed in §7.

## Motivation

A surface reconstruction is periodic: a small per-cell rearrangement (form a dimer, add an adatom, depassivate/repassivate, remove or replace surface atoms) repeats across a crystal face. We want to author that unit **once** and tile it, with mechadense's two requirements: tiles must be **lattice-commensurate** with the substrate (no gaps, no metric drift — his "boundary conditions"), and the result must be a **covalently connected** structure.

The key simplification: **periodic bonds need not be represented at all — they emerge from coincidence.** If the tile includes the atoms it shares with its neighbours, then after laying tiles out on the tiling lattice, each shared atom from one tile lands on the identical position as the corresponding atom of the next tile. Fusing coincident atoms (a "weld") turns every boundary-crossing bond into an ordinary intra-structure bond. The *same* weld fuses the tile to the surrounding bulk. So the patch's atomic content is a plain `AtomicStructure` — no motif, no fractional coordinates, no diff — and the whole feature rests on one primitive plus a volume cut.

## 1. Model

A **patch** = a **tile** (a `Molecule` — an ordinary atomic structure) + a small set of **integer tiling vectors** (each a lattice translation of the substrate) + an optional **cut volume**.

The tile is authored in real space and deliberately **includes the atoms it shares with adjacent tiles and with the bulk**:

- a bond crossing a tile boundary onto a shared atom → that atom is in the tile; the neighbour's copy welds to it;
- a bond crossing a boundary with *no* atom on it (e.g. a dimer bond spanning the cell edge) → the tile also includes a **ghost** copy of the neighbour's bonding atom at its real position; the neighbour's real atom welds onto it;
- a bond down into the bulk → the tile includes the bulk **collar** atom it attaches to; on application that collar welds onto the surviving substrate atom and inherits its bulk bonds.

No per-atom flags distinguish these — interior / shared-boundary / ghost / collar are *all* resolved by coincidence at weld time.

**Apply** (`patch_latticefill`), over a fill region:

```
remove substrate atoms inside the (tiled) cut_volume       // delete the old surface the reconstruction displaces
for each cell c in region:  place a copy of tile + (c expressed in real space)
weld all coincident atoms                                   // fuses tile↔tile (periodic) and tile↔bulk (collar) at once
passivate residual danglers                                 // optional: true edges
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
    tile:           Molecule,             // atoms + intra-tile bonds, Cartesian, is_diff = false;
                                          //   includes shared-boundary, ghost, and collar atoms
    tiling_vectors: Array[IVec3],         // 1–3 periodic directions, each an integer combination of the
                                          //   substrate lattice; count = periodic dimensionality
    cut_volume:     Blueprint,            // geometry of one tile: defines the interior at build time
                                          //   and removes the displaced substrate atoms at apply time
}
```

Why each field:

- **`tile: Molecule`** — exactly atoms + bonds (`AtomicStructure`), nothing more. The *stored field* is a `Molecule` because the tile needs no substrate lattice, but `patch_build` *accepts* `HasAtoms` (a `Crystal` is fine — its atoms are taken and the structure dropped). *Not* `Motif`/`Structure`, which would force fractional coordinates tied to a cell, element-substitution `parameters`, the `bonds_by_site` caches, and a hard 3D-periodicity assumption (`SiteSpecifier.relative_cell` is always `IVec3`) — all made unnecessary by the weld.
- **`tiling_vectors: Array[IVec3]`** — defines where copies go and, by being integer combinations of the substrate lattice, *guarantees* welds coincide (commensurability — mechadense's pre-verification). Integer, not real-space, so it is verifiable and substrate-relative. The count replaces mechadense's Bool-triple: 1 = chain/edge, 2 = surface, 3 = bulk twin.

  **Only periodic directions are stored — there is no entry for non-periodic axes, and no separate periodicity flag.** This is complete *because the tile is Cartesian*. In a `Structure`/motif model the non-periodic cell vectors are load-bearing (motif sites are fractional w.r.t. all three, so you need all three to know where the atoms are, plus a flag for which to repeat). Here the atoms are in absolute real space, so the finite extent in any non-periodic direction is already implicit in the atom positions; placement steps only the periodic vectors; the normal/depth comes from `patch_latticefill`'s `origin` plus the tile's own coordinates; and the removal extent comes from `cut_volume`. A non-periodic axis therefore has nothing left to specify, and storing one would be a redundant second source of truth that could drift from the atom positions. (Any orientation a UI wants — e.g. the surface normal — is derived on demand from the spanned vectors, not stored.)
- **`cut_volume: Blueprint`** — does double duty. At **build** time it separates the slab into interior (kept as real tile atoms) and ghosts (§4); at **apply** time it removes the displaced old surface atoms, which welding cannot do (they sit at *different* positions than any tile atom). This is mechadense's "volume of the patch as filter," and because it is intrinsic to extraction it is required (a purely additive patch simply has a volume whose interior contains no removable substrate).

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
| `tile` | `Crystal` \| `Molecule` (`HasAtoms`) | Yes | The whole authored slab (reconstruction **on its bulk**). Only its atoms are read. (Named `tile` for the role it plays; it is the *source* the tile is extracted from.) |
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

### Behaviour & caveats

- Validate `1 ≤ len(tiling_vectors) ≤ 3` and linear independence. The substrate the patch is finally tiled onto is `patch_latticefill`'s region, which must share `lattice`.
- The `cut_volume`'s translates under `tiling_vectors` should **tile the reconstructed strip without gaps** (else old surface atoms survive between tiles); `patch_build` can warn if they don't.
- *Ergonomic vectors:* instead of hand-entering `tiling_vectors`, feed a `DrawingPlane` (Miller + shift) and a superlattice `(n,m)`; `DrawingPlane` yields the in-plane primitive lattice vectors and the patch vectors are `n·u, m·v`.

## 5. Node: `patch_latticefill`

Tiles a patch across a region and welds it in. (Pin shape kept close to mechadense's proposal.)

| Pin | Type | Req | Role |
|---|---|---|---|
| `target` | `Crystal` \| `Molecule` (`HasAtoms`) | Yes | The structure being reconstructed. |
| `region` | `Crystal` \| `Blueprint` (`HasStructure`) | No | Where to tile; supplies the substrate `lattice_vecs` and the fill extent. Default: `target`'s extent (then `target` must be a `Crystal`). |
| `patch` | `Patch` (record) | Yes | From `patch_build`. |
| `origin` | `IVec3` | No | Tiling origin in `region` lattice coordinates. Default: region centre. |
| `tolerance` | `Float` | No | Weld tolerance (Å). Default 0.1. |
| → (out) | `Crystal` | — | The reconstructed crystal. |

Algorithm:

1. **Verify commensurability** — each `patch.tiling_vectors[i]` is an integer combination of `region.lattice_vecs` (true by construction when `patch_build`'s `lattice` matched `region`); else error.
2. **Cut** — if `patch.cut_volume` is set, remove `target` atoms inside it at every tiled cell (dropping their bonds).
3. **Place** — for each cell `c = origin + Σ kᵢ·vᵢ` whose stamp falls in `region`, add a copy of `patch.tile` translated by `c` in real space.
4. **Weld** — `weld_coincident_atoms(result, tolerance)` over the placed copies *and* the surviving substrate: this fuses tile↔tile (periodic bonds) and tile↔bulk (collar, inheriting bulk bonds) in one pass. A weld that includes any non-ghost atom yields a real atom; a cluster of only ghosts stays a ghost.
5. **Drop unwelded ghosts** — any atom still flagged ghost found no real twin (it points at a neighbour tile that was never placed — a true reconstruction edge); remove it, leaving a dangling bond on the boundary interior atom.
6. **Passivate** residual danglers (optional, reusing `materialize`'s hydrogenation), then wrap as `Crystal`.

(A and B are kept as separate pins because, as mechadense notes, in 3D the fill volume need not match the workpiece's volume.)

## 6. Bonding & boundary conditions

mechadense's "boundary conditions" are the **geometric lattice-matching** of §1/§5.1 — the precondition that makes welding well-defined, not the bonding step itself. Both interfaces are the same weld:

- **Tile ↔ tile (periodic).** Shared boundary atoms (and ghosts of cross-edge bond partners) coincide between adjacent tiles and weld; the boundary-crossing bond becomes an ordinary bond. Dimer bonds spanning a cell edge work because the ghost atom welds onto the neighbour's real atom.
- **Tile ↔ bulk (aperiodic / APB).** The tile's **collar** atoms overlap surviving substrate atoms and weld onto them; the merged atom inherits the substrate atom's outward bulk bonds (they are part of the union) while keeping the tile's inward bonds — continuous `bulk —(inherited)— collar —(tile)— interior`. Anchor the collar to the **less-relaxed sub-surface layer** (real reconstructions displace the top layer; mechadense's Si(111) note: "minimal subsurface shifts").
- **True edges** (a collar atom with no substrate partner, at the crystal boundary) leave a dangler that passivation saturates — the edges/corners case mechadense defers. Consistent.

A **compatibility check** falls out for free: count collar atoms that weld vs. those left orphaned, and check post-weld coordination — this catches mechadense's stated failure modes (a patch "applied too high" → floating, un-welded collars; "applied too low / subsurface" → over-coordinated welds). It is also the natural data source for his compatibility visualization / identicons idea.

## 7. Deviations from mechadense's proposal (for review)

| mechadense's proposal | This design | Why |
|---|---|---|
| Patch is **a `Structure` with metadata** (motif → periodic bonds) | Patch is a **record around a plain `Molecule` tile**; periodic bonds emerge from welding shared atoms | Drops motif/fractional/3D-periodicity baggage; one mechanism (weld) covers both boundaries |
| `patch_build` input **X = `Crystal` example** ("big proxy with a single unit") | **`tile: HasAtoms` slab + `cut_volume`**, from which the node *extracts* interior + ghosts (§4) | Matches his big-proxy intent directly; extraction is automatic, no `Structure` needed |
| **Bool-triple** for which vectors are periodic | **`Array[IVec3]`**: periodic iff a vector is given | Fewer inputs, no vector-but-aperiodic ambiguity |
| Removal via **"volume of the patch as filter"** | **Kept** (`cut_volume`) | Agreement |
| Output **"new type? or record?"** | **Built-in record** | §2 — pure data of existing types |
| `patch_latticefill` A/B/patch/origin | **Kept**, + optional `tolerance` | Agreement; tolerance surfaces the weld knob |
| Bonding **unspecified** | **§6**: weld (periodic + collar) + passivation (edges) | Fills the gap with one primitive |
| `motif_edit → structure_edit`, periodic minimization | Not adopted | A plain-atomic tile is not a `Structure`; periodic minimization could later run on the tile-plus-ghosts if wanted (noted, not blocking) |

## 8. Scope / non-goals (v1)

- **One face at a time.** Multi-face stitching and **edges/corners** are out — deferred to passivation/manual, as mechadense agrees.
- **Commensurate substrates only.** Patch and region must share a lattice (integer tiling vectors); genuinely incommensurate interfaces are out.
- **Nearest-neighbour-range boundary bonds.** Bonds reaching more than one cell need ghost atoms that far out; supported, but the common case is one cell.

## 9. Open questions

1. **Build threshold `ε`.** The interior/ghost split uses `cut_volume` membership SDF ≤ `ε` (§4). It must be large enough to catch atoms authored right on the cut surface but smaller than the nearest interplanar spacing so it never grabs the layer below — same trade-off as `materialize`'s region margin. Confirm a default.
2. **Where the cut happens.** `cut_volume` is per-tile and tiled with the patch; alternatively a single region-wide cut. Per-tile is more local and composes with the patch; proposed default.
3. **Strictness of welding.** On element/flag mismatch or an un-welded collar (floating patch), warn vs. error. Proposed: surface as a non-blocking compatibility badge (§6) by default, with an `error_on_incompatible` flag.
4. **Compatibility visualization.** Where to show weld/coordination stats (node subtitle badge vs. panel vs. mechadense's identicons). Out of scope to build in v1; the data is produced by step 4.
5. **Deriving a default `cut_volume`.** It is required (it defines the interior), but a sensible default — the in-plane tiling cell extruded to a chosen depth — could be offered so the user only draws one when they want a non-prismatic cut. Nice-to-have, not v1.
