# DNA Origami Support — Discussion Draft

> **Status:** early concept, for team discussion. Not an implementation spec — it
> deliberately omits struct/serialization/test detail. The goal is to agree on the
> *model* and the *node pipeline* before we go deeper.

## Why

atomCAD today targets covalent, crystal-lattice nanostructures. A growing branch of
nanotechnology instead builds shapes from **DNA origami**: a single long "scaffold"
strand folded into a target shape by a few hundred short "staple" strands.

DNA origami is an unusually good fit for atomCAD because, unlike protein design, it
needs **no folding prediction and no machine learning** to produce a complete,
manufacturable result:

- **Geometry is canonical.** A DNA double helix is the same regular shape regardless
  of sequence (~0.34 nm rise per base; ~10.5 bp/turn). Once we know which helix a base
  sits on and its index along that helix, its 3D position is pure arithmetic. (The exact
  twist density and crossover spacing are lattice-dependent constants — honeycomb uses
  10.5 bp/turn with crossovers every 7 bases, square uses 10.67 bp/turn every 8 bases;
  the latter is why square-lattice bundles carry a known residual global twist.)
- **Sequence is computed, not designed.** The scaffold sequence is fixed (a known
  virus genome). Each staple sequence is simply the Watson–Crick complement of the
  scaffold bases it covers.
- **It reuses our lattice machinery.** The standard design style packs parallel
  helices onto a 2D lattice (square or honeycomb) — directly analogous to our crystal
  lattice / tiling-vector code.

So the user authors a **shape**; the system derives the strands. The deliverable is a
list of staple sequences a lab can order and mix, plus a 3D model for visualization.

## Scope (first version)

We target the mainstream **lattice-based** style only:

- All helices are **parallel**, aligned to one axis (z).
- Helix cross-sections sit on a fixed **2D lattice** (square or honeycomb).
- Each helix is a straight helical extrusion along z.

Explicitly **out of scope for now**: wireframe/mesh origami (helices along polyhedron
edges, not parallel) and intentionally curved/twisted bundles. Both can be added later
without disturbing this model.

## The model: two data types + a solver

### 1. `DnaLatticeShape` — the editable design

The shape the user authors. Conceptually:

- a **grid type** (Square or Honeycomb), and
- for each lattice cell, either **off** (no helix) or an **integer** interval
  **`(z_start, z_end)`** giving how far that helix extends along the axis.

**Everything here is discrete.** The cell coordinates are lattice integers `(x, y)`,
and `z_start`/`z_end` are **integer base indices** along the helix axis — not
continuous lengths. This is the defining property of the whole system: a DNA origami
design is a *combinatorial* object (which integer cells are on, over which integer
ranges), with no continuous parameters anywhere. That discreteness is exactly what
makes the solver deterministic and the geometry exact arithmetic.

This is the minimal, intuitive representation — a 2D footprint where each "on" cell
carries a z-extent (like a height-map, but with both ends free). It is produced by
nodes and edited in a dedicated **editor node** (in the spirit of `atom_edit`): a
2D grid panel for the footprint plus per-cell z-extents.

Two validity rules the editor should surface (not hard constraints on the type, but
feedback):

- **Neighbor overlap** — two adjacent "on" cells must overlap in z to host a crossover.
  Note this is necessary but not sufficient: crossovers can only land at the discrete,
  lattice-phase-aligned z positions where the two backbones come into contact (the
  7/8-base spacing above), so the overlap must actually *contain* such a site.
- **Length budget** — the total length of all helices can't exceed the scaffold
  length (~7,249 bases for M13). The editor should track "bases used / budget."

*v1 simplification:* one interval per cell. (Real caDNAno allows multiple disjoint
segments per helix; defer.)

### 2. `dna_solve` — the synthesizer node

Turns a shape into a fully solved, manufacturable design.

- **Inputs:** a `DnaLatticeShape`, plus a **scaffold sequence** (default M13).
- **Output:** a `DnaOrigamiSolved` (below), plus a **report** indicating success or
  why it failed.

What it does:

1. **Route the scaffold** — find a single path that threads through every "on" cell,
   crossing over only between *adjacent* cells. This is a covering walk on the
   lattice-adjacency graph (trivial boustrophedon for solid rectangles; may need care,
   or be impossible, for shapes with holes — hence the report).
2. **Design the staples** — break the complementary strand into ~32-base staples with
   crossovers between adjacent helices.
3. **Assign sequences** — scaffold from the input; each staple = complement of the
   scaffold bases it covers.

### 3. `DnaOrigamiSolved` — the solved design

Carries everything downstream needs:

- the original `DnaLatticeShape`,
- the **routing**: scaffold and staple paths as segments, each tagged with its helix
  `(x, y)` and position range along z,
- the **raw sequences**: scaffold + the list of staple strands.

Neither this type nor `DnaLatticeShape` stores an atomic structure. Atoms are
produced only on demand by a separate `dna_materialize` node (below), keeping both
types lean and abstract.

### Display: abstract, not atomic

Like `Blueprint` — which atomCAD renders differently from a `Crystal` — the DNA types
have their **own abstract visual representations**, not balls-and-sticks:

- **`DnaLatticeShape`** displays as its 2D footprint extruded along z — a block/
  height-field-like view of which cells are on and how far each extends. This is the
  cheap, always-on preview while authoring.
- **`DnaOrigamiSolved`** displays as **helices/rods and strand paths** (cylinders with
  the scaffold/staple routing drawn on them), not atoms. This is the natural "wiring
  diagram" of the folded object.

Both go through the existing `display/` domain→renderer adapter; only `dna_materialize`
produces an atom-level model.

### 4. `dna_materialize` — convert to atoms

The bridge from the abstract DNA representation to an atomic structure, parallel to the
existing `materialize` node for geometry/crystals.

- **Input:** a `DnaOrigamiSolved`.
- **Output:** an atomic structure — either a **Crystal-like mixed representation**
  (lattice-aware, lighter weight) or a straight **`Molecule`** (explicit atoms +
  bonds), selectable on the node.

Placement is pure canonical arithmetic (lattice cell → `(x, y)`, base index → z and
twist angle), using motif templates for each nucleotide. This is the only node that
materializes atoms; everything upstream stays abstract.

## Node pipeline

```
[shape sources] ──► DnaLatticeShape ──► dna_solve ──► DnaOrigamiSolved ──┬─► dna_materialize ─► Crystal | Molecule
   • dna_lattice_edit (editor)              ▲                            ├─► (staple list export)
   • dna_lattice_fill (from Geometry)       │                            └─► (report / validity badge)
   • dna_rect, … (future)            scaffold sequence (default M13)

   (DnaLatticeShape and DnaOrigamiSolved each render abstractly — footprint and
    helix/strand views — without going through dna_materialize.)
```

- **Shape sources** produce a `DnaLatticeShape` — the hand editor first; procedural
  generators (fill a `Geometry` footprint, etc.) later.
- **`dna_solve`** is the heart; its report drives a compatibility/validity badge in the
  UI (cf. the surface-patch report pattern).
- **Abstract display** is always available on both DNA types directly (footprint and
  helix/strand views) — no materialization needed to see the design.
- **`dna_materialize`** is the only path to atoms (Crystal-like or `Molecule`), for
  when the user wants an atom-level model, simulation, or atomic export.
- **Other downstream** of a solved design: export staple sequences (and optionally
  caDNAno/oxDNA formats), surface the report.

## Why this is a good base

- It mirrors the proven caDNAno decomposition (author a shape, solve a routing).
- The work is **discrete and combinatorial — no physics solver, no ML.** Two distinct
  parts: geometry and sequence are *exact arithmetic* (the real win over protein
  folding), while scaffold routing is a *graph search* — trivial for solid rectangles
  (boustrophedon), but a genuine combinatorial problem that can need search or be
  impossible for shapes with holes (hence the `dna_solve` report). Both are
  deterministic and testable; only the second is algorithmically hard.
- It reuses existing atomCAD infrastructure: lattice/tiling for cross-sections,
  motif templates for base→atoms, the renderer for display, the `atom_edit`-style
  node-with-editor pattern, and the report-badge pattern.
- The shared polymer abstraction it forces us to build (an ordered chain of monomers
  placed by a canonical per-step transform, plus a sequence plane) is the same
  foundation any future protein support would reuse.

## Open questions for discussion

1. **New first-class data types vs. records?** `DnaLatticeShape` / `DnaOrigamiSolved`
   as new domain `DataType`s (like `Atomic`), or structured `Record`s? The editor node
   and custom rendering lean toward first-class types.
2. **One solver node or several?** Keep route + staple + sequence as one `dna_solve`,
   or split (e.g., expose scaffold routing as its own node for manual control)?
3. **How much routing control does the user get?** Fully automatic first; do we need
   manual scaffold-path / crossover editing later (a la caDNAno's hand-routing)?
4. **Square vs honeycomb first?** Support one lattice initially, or both from the start?
5. **Export targets.** Staple CSV is the must-have. Do we also want caDNAno JSON and/or
   oxDNA (for an optional external mechanical-relaxation pass)?
6. **Mechanical relaxation.** The idealized placement assumes straight rods. Real
   bundles bend/twist under strain. Out of scope for v1, but do we want a hook to an
   external solver (CanDo/oxDNA) on the roadmap?

