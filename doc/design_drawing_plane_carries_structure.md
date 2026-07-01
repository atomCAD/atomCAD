# Drawing plane carries the full Structure (extrude motif preservation + silicon reconstruction)

## Problem

Two independent bugs prevented surface reconstruction (and correct element
assignment) for any 2D→3D pipeline built on a non-default crystal structure —
e.g. mechadense's `SPM_tip` / `TESTs` / `disk(100)` / `circle_free`, all built on
a silicon `structure.14Si`.

1. **`extrude` discarded the motif.** `extrude` reconstituted the Blueprint's
   `Structure` via `Structure::from_lattice_vecs(drawing_plane.unit_cell)`, which
   pairs the drawing plane's *unit cell* with the hardcoded **default carbon**
   zincblende motif. Its own `structure` input pin was deprecated/ignored (see
   below). So a silicon design produced **carbon atoms on a 5.431 Å silicon
   lattice** — physically nonsensical, and rejected by the reconstruction gate.

   Root cause: the `DrawingPlane` type stored only `unit_cell`, never a motif.
   The motif was dropped the moment a 2D shape was placed (`drawing_plane` node:
   `let unit_cell = structure.lattice_vecs.clone();`).

2. **The reconstruction applicability gate rejected genuine silicon motifs.**
   `get_reconstruction_params` first required
   `motif.is_structurally_equal(&DEFAULT_ZINCBLENDE_MOTIF)`, and
   `is_structurally_equal` compares parameter *default atomic numbers*. A silicon
   motif (PARAM PRIMARY/SECONDARY Si) therefore failed the first gate, making the
   silicon reconstruction branch effectively dead code — reachable only via
   `materialize`'s `parameter_element_value_definition` element override on a
   carbon-default motif.

## Why extrude's `structure` pin was deprecated

Commit `0cc14560` (2025-12-09) deprecated the pin (then `unit_cell: LatticeVecs`)
because **the unit cell is knowable from the 2D shape's drawing plane** — a
separate pin was redundant. `fa51dd00` (lattice-space refactoring) renamed it
`unit_cell → structure` but kept it ignored and switched to
`Structure::from_lattice_vecs`. The deprecation rationale (single source of truth
= the drawing plane) is **sound**; the refactoring simply left the `DrawingPlane`
carrying only the cell, so the single source of truth became lossy (no motif).

## Fix

**Single source of truth restored.** The `DrawingPlane` now carries the full
crystal field; extrude reads it; the deprecated pin stays ignored (no override).

### Part A — `DrawingPlane` carries the structure
- `DrawingPlane` gains `motif: Motif` + `motif_offset: DVec3` fields, plus a
  `with_structure(motif, offset)` builder and a `structure()` accessor that
  assembles a `Structure { lattice_vecs: unit_cell, motif, motif_offset }`.
  `unit_cell` stays the stored 3D lattice (it drives `effective_unit_cell` and
  all in-plane geometry) — a full `Structure` field would duplicate it and force
  a wide, risky rename. `DrawingPlane` is **runtime-only** (derives Clone/Debug,
  not Serialize), so there is **no `.cnnd` migration**. Fields default to the
  zincblende carbon motif + zero offset, so every existing constructor caller is
  byte-identical to before.
- The `drawing_plane` node attaches the input structure's motif/offset via
  `.with_structure(...)` instead of discarding them.
- `extrude` builds the Blueprint's structure from `shape.drawing_plane.structure()`.

**Why not honor the `structure` pin as an override?** The 2D geometry's
coordinates are baked against the drawing plane's lattice; an override with a
different lattice would desync the atom lattice from the geometry — the exact
inconsistency the deprecation removed. Motif-only flexibility already belongs on
the drawing plane's `structure` input, or on `materialize`'s element override.

### Part B — reconstruction gate compares topology only
- `Motif::is_topologically_equal` mirrors `is_structurally_equal` but ignores
  parameter *default atomic numbers* (compares parameter names, sites, bonds).
- `get_reconstruction_params` uses it, then gates on the **effective** element
  values + cell size (unchanged). A genuine silicon motif now passes the topology
  gate and the Si + 5.431 Å branch fires. Diamond is unaffected.

## Result
Silicon (and any supported) designs reconstruct with **no node-graph change** and
diamond designs are byte-identical. Regression tests:
`tests/crystolecule/lattice_fill_test.rs` (silicon/diamond reconstruction change
atom counts) and `tests/structure_designer/extrude_structure_test.rs` (extrude
preserves the drawing plane's motif).
