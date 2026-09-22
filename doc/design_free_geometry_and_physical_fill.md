# Free Geometry and the Physical Fill

## Status: Draft for refinement (2026-09-22)

Two companion efforts feed this design and are tracked separately:

- a **site catalogue** of hydrogen-terminated silicon and diamond surface
  sites: for each local environment the fill can produce, whether it exists as
  a stable minimum, its geometry, and the steric limit between its
  terminators. Sources are the literature and UMA relaxations through
  Elementa, with DFT on a subset. §0.5 says why the catalogue is scoped this
  way and not as a surface-energy survey.
- **measurement programs** on the `crystolecule` crate that answer questions
  about what the fill produces on realistic shapes.

Anything marked `TODO(research)` is a question for the site catalogue,
`TODO(measure)` is a question a small program built on the `crystolecule`
crate can answer, and `TODO(compute)` is a question for the simulation queue.

## Depends on / relates to

- `doc/design_lattice_space_refactoring.md` — Blueprint / Crystal / Molecule
  phases. This design keeps that split: the fill still turns a Blueprint into
  a Crystal. It changes what a Blueprint's geometry may be and what the fill
  does at the surface.
- `doc/design_blueprint_alignment.md` — the `Alignment` flag. Mesh and free
  geometry are `lattice_unaligned` by construction; the fill must accept that.
- `doc/design_free_sphere_circle.md` — the first free-geometry nodes and the
  decision that free coordinates are real-space ångströms, not fractional
  cells. This design generalises that decision to a whole node family.
- `doc/surface_reconstructions.md` — the current (100) 2×1 algorithm. §4.5
  replaces its phase tables with a matching; its geometry constants and its
  `PlacedAtomTracker` addressing are kept.
- `doc/design_concave_rebonding.md` — the post-passivation clash repair. §4.5
  absorbs it as an ordinary move of the reconstruction pass, which removes the
  ordering constraint that forced it to run after passivation.
- `doc/design_hydrogen_passivation.md`, `doc/design_halogen_passivation.md` —
  terminator placement and bond lengths, reused unchanged.
- `doc/design_blueprint_region_atom_edits.md` — `MaterializeRegion` overrides.
  Every option in §4.9 is region-overridable through the same resolver.
- `doc/design_surface_patches.md` — the hand-authored alternative. It stays
  the escape hatch for reconstructions this design does not generate.

## 0. Why: what the fill is for, and what it is not

The one-line version: **the design volume is free, and the lattice and the
chemistry enter only in `materialize`** (decision D1). The fill is a printer,
not a machine tool. It turns any volume the user draws or imports into a
stable, passivated, buildable crystal. It does not make bearing faces, it does
not choose build order, and it does not predict which shape a crystal would
anneal to, because a mechanosynthesised part never anneals.

### 0.1 Two facts about passivated silicon that shape everything

**Stability is the floor, not the goal.** A hydrogen-terminated silicon
surface is kinetically stable far above room temperature:

| Process on H-terminated Si | Onset |
|---|---|
| H₂ desorption from the (100) monohydride | ~700 K |
| (100) dihydride → monohydride conversion | ~650 K |
| Si–H bond dissociation | ~3.2 eV, never thermal at 300 K |

The (100) 1×1 dihydride is a real room-temperature phase on silicon. A
one-atom pit terminated with three hydrogens is a hydrogenated vacancy, a
well-known stable defect. A one-coordinated atom is a silyl group, chemically
fine. A random-shaped hydrogen-terminated silicon nanocrystal sits in vacuum
indefinitely because nothing on its surface can move. So on silicon a plain
sign test, complete passivation, and a rule for two terminators that clash
already produce a room-temperature-stable object. Only two things in this
design are load-bearing *for stability*: no dangling bond is left
unterminated, and no two terminators are forced below their steric limit.

**Nature follows the model; it does not correct it.** On a fully terminated
surface every rearrangement, a different dimer phase, an unpaired dihydride,
a step that is or is not rebonded, is separated from the alternative by
breaking an Si–H or Si–Si bond at 2 to 3 eV. Nothing moves at room
temperature. The built part is whatever the model said, provided each site is
a local minimum. The reconstruction pass of §4.5 is therefore not predicting
what the surface will do; it is **choosing what we build**, under the single
constraint that every local configuration exists. The exceptions are
barrierless: a dihydride cants, two terminators inside their steric limit
react, and a bare dangling bond reacts with whatever it meets, which is fine
in vacuum and not in air.

Diamond is the exception to the first fact only. On C(100) the 1×1 dihydride
is sterically impossible, so there the reconstruction pass is the difference
between a molecule and nonsense, not a refinement.

### 0.2 What the passes buy

Everything beyond the floor is justified by one of the following goals, in
this order of importance. Each pass in §4 names the goals it serves.

- **G1 Buildability: fewer site types.** The silicon operation library covers
  a finite catalogue of local environments. Minimising dangling bonds drives
  every surface toward {111} and {100} micro-facets, which are exactly the
  environments the library knows. A raw sign-test surface has a long tail of
  odd sites, each of which needs its own applicability pattern or ends up
  blocked. The healer (§4.3) is a buildability pass.
- **G2 Build cost: fewer operations.** Every terminator on the finished part
  is a deposition, and every surface site is several operations. Dimerising
  (100) halves the dangling bonds on that face; filling a pit removes three
  terminators and adds one; peeling a whisker removes a three-hydrogen group
  that had to be built as a single-bonded atom. On tens of thousands of
  surface atoms this is a large fraction of the total step count.
- **G3 Tip access.** §2.2 lists "reachable by a tool tip" as a per-site
  constraint. Stable and reachable are different properties. The healer helps
  by accident, because one-atom pits and one-atom whiskers are the worst
  access cases, but narrow terraces and inner step corners survive it. §4.10
  adds an explicit, reported check. This is the constraint that actually
  limits builds of free shapes.
- **G4 Simulation readiness.** A model with unreconstructed dihydrides or
  clashing terminators is far from its relaxed geometry. Relaxation then moves
  atoms a lot, converges slowly (the dihydride (100) fixture already needs
  several hundred iterations), and the structure that gets simulated is no
  longer the one that was drawn. Placing dimers and canting at fill time keeps
  the model near a minimum, so a relax is a check rather than a rebuild.
- **G5 Diamond.** The same pipeline must serve the second material, where
  §4.5 is mandatory. It is cheaper to design the pass once.
- **G6 Determinism and visibility.** A matching with a seeded tie-break gives
  the same surface on every run, where a global phase table could not handle
  steps at all. The report pin (§4.7) tells the user what the fill changed
  instead of changing it silently.

### 0.3 Non-goals

- **Energy prediction.** Which of two stable surfaces has the lower energy
  does not decide whether a part exists, and it is not what the fill
  optimises. Site energies enter only through G4 (§5).
- **Functional faces.** A sliding or meshing face needs to be atomically flat,
  low-index, and lattice-mismatched against its partner. Two parts modelled in
  place and materialised in one scene share one lattice orientation, so any
  two flat terraces that touch are in registry and lock. The fill does not
  address this; the deferred `align_face` node of §6 does, by rotating the
  imported mesh so that one picked face becomes a chosen lattice plane.
- **Build order.** The fill produces the final structure; the sequencing is
  the mechanosynthesis editor's job.

### 0.4 Experimentation over argument

Most of the `TODO(measure)` items below are questions of the form "does pass X
matter on realistic shapes". We do not have those measurements yet, and the
right way to get them is to run the same shape with the pass on and off and
compare the report. Therefore **every pass is individually switchable**, the
defaults are the recommended configuration, and the baseline configuration of
§4.9 reproduces today's sign test plus passivation exactly, atom for atom.
Nothing in the pipeline is silently mandatory except passivation itself. The
switches are region-overridable through `MaterializeRegion`, so one part can
carry a healed body and an unhealed face for comparison.

### 0.5 What we need to know, and what we do not

Because nature follows the model (§0.1), the research behind this design is
not "what does a silicon surface do" but "which terminated sites exist". Three
kinds of question, of very different value:

| Question | Needed? |
|---|---|
| Does this terminated site type exist as a stable minimum, and what is its geometry: dihydride canting, S_A / S_B / rebonded step geometries, the 3×1 phase? | Yes. This is the site catalogue. It is what the op library builds against and what G4 needs. |
| Below what separation do two terminators make a site chemically impossible? | Yes. The one stability-critical number; on diamond it decides existence. |
| Which phase is the ground state, surface energies γ(hkl), equilibrium shapes, annealing behaviour? | No. That is the physics of annealed surfaces, and a mechanosynthesised part never anneals. Background only. |

UMA through Elementa answers the first two questions directly for any
candidate site: build the cluster, relax it, and see whether it stays and
where it ends up. The literature validates UMA on the known cases and
supplies the geometries that have been measured. Every `TODO(research)` in
this document is one of the first two kinds.

## 1. The problem with the current system

atomCAD's geometry library is built on one idea: a shape is a Boolean
combination of half spaces whose normals are Miller indices and whose
positions are quantised to the lattice. `half_space`, `cuboid`, `facet_shell`,
`drawing_plane`, `polygon` and `extrude` all take integer inputs, and the
`sphere` and `circle` nodes are lattice-covariant ellipsoids rather than
Euclidean balls. Only `free_sphere` and `free_circle` escape this.

Three things are wrong with it.

**It is brutally hard to use.** 3D modelling is hard in any tool. Doing it
through integer Miller indices, integer cell offsets and integer shifts with a
separate subdivision factor, with no direct manipulation for most primitives,
is harder still. This is the main reason the geometry side of atomCAD is not
pleasant, and a real barrier to adoption.

**Its rules do not follow from the physics of the parts we build.** "Low
Miller indices" is a fact about annealed crystals, not about kinetically
frozen ones (§2.1, §2.2). The position quantum is the conventional-cell
d-spacing, which is the wrong quantum for the atomic layers of diamond cubic
(§2.3). The subdivision pins that were added on request are a symptom of that
mismatch, not a feature.

**The fill is a sign test followed by ad-hoc repairs.** `fill_lattice` places
every motif site whose signed distance is non-positive, then runs a chain of
fixes: remove unbonded, remove single-bonded, dimerise {100}, passivate, repair
concave clashes. The reconstruction handles only the six {100} normals, only
cubic diamond in carbon or silicon, and only one global dimer phase. Edges,
corners and every other face classify as unknown and get whatever generic
passivation produces. On diamond, an unpaired (100) carbon then carries a
dihydride that is sterically impossible (§2.4). The concave repair exists
because the reconstruction has no concept of an unpaired atom.

A fourth cost is indirect: the generality tax. Much of the geometry layer was
built to work for any lattice, but the company's target for the foreseeable
future is cubic silicon, with diamond as the second material. This design
separates what is truly lattice-generic (the bond-graph passes, §4.3–4.4) from
what is honestly diamond-cubic-specific (the reconstruction rules, §4.5) and
does not apologise for the second.

## 2. Physics background

This section states what the physics constrains, so that the fill is designed
from it instead of from habits.

### 2.1 Where the Miller-index idea comes from

A crystal at thermodynamic equilibrium minimises its total surface free energy
at fixed volume. The solution, the Wulff construction, is an intersection of
half spaces, one per orientation, each at a distance from the centre
proportional to the surface energy γ of that orientation. So "geometry is a
Boolean of Miller-indexed half spaces" is the exact shape of an annealed
equilibrium crystal. That is the true form of the idea atomCAD adopted, and
its only justification.

For a covalent crystal the first-order estimate of γ is the density of broken
bonds per area. For diamond cubic:

| Face | Dangling bonds per a² | Comment |
|---|---|---|
| (111) | 2.31 | one per atom, cut between bilayers |
| (110) | 2.83 | one per atom |
| (100) unreconstructed | 4.00 | two per atom |
| (100) 2×1 dimerised | 2.00 | the dimer bond recovers half |

This table matters to the design for a reason unrelated to equilibrium:
dangling bonds are terminators, terminators are operations, and the healer of
§4.3 minimises exactly this count (G2). It is also why {111} and {100} are
the faces with few site types (G1).

Every other orientation is a staircase. In the terrace–ledge–kink picture, a
(5 1 0) surface is (100) terraces separated by a regular array of steps. The
lattice fill already produces this staircase when a (5 1 0) plane cuts the
crystal. What it lacks is correct chemistry at the step edges, not a
prohibition on the plane.

### 2.2 Why equilibrium facets do not constrain APM structures

All of §2.1 is the thermodynamics of grown, annealed crystals. A
mechanosynthesised silicon or diamond part is built site by site and is
kinetically frozen: nothing reshapes at room temperature (§0.1). Facet
selection by surface energy never happens to it.

The constraint that remains is **per site**. Every surface atom must sit in a
configuration that is

1. locally stable (does not spontaneously rearrange or desorb),
2. passivatable without steric clash between terminators, and
3. reachable by a tool tip during the build.

Diamondoids are the existence proof. Adamantane is a ten-carbon fragment of
the diamond lattice with no facet at all, and it is perfectly stable because
every carbon is CH or CH₂. Low-index facets are *convenient*: they contain few
site types and therefore need few operations in the build library. They are
not *required*. Any shape whose surface sites all belong to a known, stable
catalogue is a valid design.

This reframes the fill. The question is no longer "which planes may the user
draw" but "given any target volume, which subset of lattice sites near it has
only acceptable surface sites". That is a discrete optimisation on the bond
graph, and it is what §4 computes.

### 2.3 The correct position quantum is the layer spacing

A cutting plane is a classifier of which atoms are in. The only physically
meaningful quantum for its position is the spacing between successive atomic
layers along its normal, and the cut should sit *between* layers, never on
one. The current `shift` is a multiple of the conventional-cell d-spacing from
the reciprocal lattice, divided by an optional `subdivision`. For diamond
cubic that is wrong in two ways:

| Normal | d-spacing used today | Actual layer spacings | Consequence |
|---|---|---|---|
| (100) | a | a/4 | one termination in four is reachable |
| (110) | a/√2 | a√2/4 | one in two |
| (111) | a/√3 | 0.144a and 0.433a alternating | shift lands on a layer, not in the wide gap |

The (111) case matters most. The diamond (111) stacking is a bilayer: two
layers 0.144a apart, then a gap of 0.433a. A cut in the wide gap leaves one
dangling bond per surface atom, the well-known monohydride (111) surface. A
cut in the narrow gap leaves three dangling bonds per atom, a surface nothing
forms. The rule therefore derives from the motif, not from the cell: the shift
quantum along (hkl) is the set of gaps between the projected motif layers, and
the UI should snap to the midpoints of those gaps.

This is not a theoretical worry. mechadense's demolib hit it: an integer
`shift` on (111) lands exactly on a layer, so one face of a slab comes out as
methyls while the opposite face comes out monohydride. His
`atom_fill_halfshift` wrapper moved the motif by (½,½,½), which is 1.5·d₁₁₁
and flips both faces into the wide gap. It is a global toggle rather than a
per-face rule and does nothing for (100) or (110), but it is the right fix in
miniature.

`TODO(measure)`: write a small `crystolecule` program that, for a Structure
and an (hkl), projects all motif sites onto the normal modulo the conventional
d-spacing and prints the sorted distinct layer offsets and gaps. Its output is
the snapping table for §3.1 and a regression test for the (111) bilayer.

### 2.4 What is general and what is diamond-cubic-specific

**General to any covalent lattice** (depends only on the motif and the bond
graph):

- coordination number of each placed atom versus its bulk coordination z,
- the direction of each missing bond (the motif bond that has no partner),
- steric clash between terminators placed on those directions,
- the peel-and-fill healing of §4.3, whose thresholds are expressed in z.

**Specific to diamond cubic silicon and carbon** (and the honest scope of the
reconstruction pass):

- the 2×1 dimer reconstruction on {100}, with its measured geometry
  (Si dimer 2.44 Å, C dimer 1.63 Å, vertical drop derived so the back-bonds
  stay bulk length, one geometry per lattice regardless of passivation),
- which hydride phases exist: on Si(100) monohydride 2×1, mixed 3×1 and
  canted dihydride 1×1 are all real, while on C(100) only the 2×1 monohydride
  is sterically possible and the 1×1 dihydride places hydrogens far inside
  their van der Waals contact,
- (111) and (110) hydrogen-terminated surfaces are 1×1 for both elements; the
  clean-surface reconstructions (Si 7×7, Pandey chains) do not survive
  termination and never need generating,
- the catalogue of step structures on (100): single S_A and S_B steps, the
  rebonded S_B, and the double D_B step,
- the edge and corner structures of hydrogen-terminated nanocrystals.

`TODO(research)`: for each item in the second list, record whether the
terminated site exists, its geometry, and the source. Energies are not
needed (§0.5).

## 3. Geometry sources

The fill of §4 consumes a Blueprint: a Structure plus a signed-distance
geometry tree. It does not care where the tree came from. This section lists
where it can come from, in the order we intend to deliver.

### 3.1 Two geometry libraries: lattice-constrained and free

The existing nodes remain the **lattice-constrained library**. They are still
the right tool when the user wants an exact (111) face or a part that must
tile with another on integer lattice vectors. Two fixes apply to them:

- The shift quantum of `half_space`, `drawing_plane`, `extrude` and
  `facet_shell` becomes the motif layer spacing along the normal (§2.3),
  snapping to gap midpoints, with a float `offset` pin as the override.
  `subdivision` becomes unnecessary for these nodes and is kept only for
  loading old files.
- `facet_shell` gets the same treatment; it currently hard-codes
  `subdivision = 1`.

Alongside them grows the **free library**: the same primitives in real-space
ångströms with no lattice snapping, following `free_sphere` and `free_circle`.
The set to add, each a sibling of `free_sphere` that lowers to the existing
`GeoNodeKind` variants:

| Free node | Lowers to | Inputs |
|---|---|---|
| `free_half_space` | `HalfSpace` | point (Vec3 Å), normal (Vec3) |
| `free_box` | intersection of 6 `HalfSpace` | centre, half extents, orientation |
| `free_cylinder` | `Extrude` of `Circle` | base point, axis, radius, height |
| `free_extrude` | `Extrude` | 2D shape, plane origin, plane normal, height |
| `free_polygon` | `Polygon` | vertices (Vec2 Å) on a free plane |
| `free_plane` | drawing-plane analogue | origin, normal, in-plane u |
| `free_transform` | `Transform` | translation, rotation, uniform scale |

The free library is the long-term modelling surface for atomCAD. It is still
an implicit representation, and nobody has built an editing experience for
implicit geometry that matches a boundary-representation modeller. So the free
library is complemented, not replaced, by §3.2.

### 3.2 Mesh import: the short-term focus

Users model in Blender or a mechanical CAD tool and export a triangle mesh.
atomCAD imports it as geometry. This is the first thing to deliver for the new
fill, because it gives designers a full modelling tool on day one and because
the fill of §4 needs nothing but a sign and a distance.

**Node.** `import_mesh` (category Geometry3D) with pins `file: Text`,
`scale: Float` (file units to ångströms; default 1.0), and an optional
`transform`. Output `Blueprint` with `Alignment::lattice_unaligned`, paired
with the `structure` pin the same way `free_sphere` is. Formats: binary and
ASCII STL, and OBJ (triangles and quads, quads split). The payload is stored
as a path relative to the design file and re-parsed on load, exactly as
`import_xyz` and `import_cube` do; the mesh is never embedded in the `.cnnd`.

**Geometry primitive.** A new `GeoNodeKind::Mesh` in `atomcad-geo-tree`
(hash tag 0x10, the next free tag) holding the triangle list and a
bounding-volume hierarchy built once at construction:

- **Sign** by generalised winding number (Jacobson et al. 2013). It is robust
  to small holes and to inconsistent triangle orientation, which real
  exports have, and it degrades gracefully instead of flipping the inside of
  the whole part on one bad triangle.
- **Magnitude** as the distance to the closest triangle. That distance is
  1-Lipschitz, so the box-culling test in `fill_algorithm.rs` stays valid with
  no scale factor.
- **Batch evaluation** through the existing 1024-point interface; the BVH
  makes each query logarithmic in triangle count.
- **CSG conversion** is trivial: the triangles are already the mesh.

The winding number's cost is proportional to the number of triangles per
query unless it is accelerated; use the BVH-based fast approximation from the
same paper, with the exact sum only for points near the surface.

**Diagnostics.** The node reports triangle count, whether the mesh is closed,
the bounding box in ångströms, and warns when the box is smaller than one unit
cell or larger than the implicit fill volume.

Mesh *editing* inside atomCAD is a long-term possibility and is out of scope
here. Aligning a mesh face to a lattice plane is the deferred `align_face`
node of §6.

### 3.3 STEP import: later, by tessellation

A STEP file is an exact boundary representation: trimmed NURBS and analytic
surface patches joined by a topology of edges and loops. Point-in-solid
directly on that representation is a CAD-kernel problem and we will not write
one. The route is to tessellate at import and hand the triangles to the
`Mesh` primitive of §3.2.

We will **not** ask users to tessellate outside atomCAD as the long-term
answer; STL export is the interim. The intended implementation is the pure
Rust `truck` crates: `truck-stepio` reads ISO 10303-21 into a B-rep and
`truck-meshalgo` triangulates it to a chord tolerance. This keeps the build
free of a C++ kernel on every Flutter platform. When a file uses an entity the
crate does not handle, the node reports it and suggests STL export; we do not
extend the kernel. Open CASCADE bindings were considered and rejected for the
build cost.

`TODO(measure)`: verify current `truck-stepio` entity coverage against STEP
files exported from Blender (via add-on), FreeCAD and Fusion 360.

## 4. The physical fill

### 4.1 Pipeline

The new `materialize` pipeline, with the pass it replaces on the right:

| Step | Pass | Generic? | Replaces |
|---|---|---|---|
| 1 | Sample motif sites, keep signed distance per atom | yes | unchanged |
| 2 | Create motif bonds | yes | unchanged |
| 3 | **Heal**: peel and fill on the bond graph (§4.3) | yes | `rm_unbonded`, `rm_single` |
| 4 | **Classify** surface sites by dangling bonds (§4.4) | yes | `classify_atom_surface_orientation` |
| 5 | **Reconstruct**: dimer matching, rebond, unmatched-site rule, iterate (§4.5) | diamond-cubic | `reconstruct_surface`, `concave_rebond` |
| 6 | **Passivate** from the classification's dangling-bond list, then the clash check (§4.6) | yes | `hydrogen_passivate` (derivation changes, placement does not) |
| 7 | **Access check** against the tool envelopes (§4.10), optional | yes | new |
| 8 | **Report** and tag (§4.7) | yes | new |

Steps 3, 5 and 7 are switchable (§0.4, §4.9); with all of them off the
pipeline is today's sign test plus passivation.

Steps 3 to 7 operate only on atoms within a shallow band of the surface,
found through the signed distance already stored as `in_crystal_depth`, so
the whole chain is linear in the number of surface atoms.

### 4.2 Sampling and the candidate band

Step 1 is unchanged: every motif site with signed distance at or below the
inclusion threshold is placed, and the distance is kept on the atom. One
addition: sites in the **outer band** `0 < sdf ≤ δ_fill` are enumerated too,
not placed, but recorded in the tracker as *candidates* with their address.
The fill move of §4.3 can only ever add a candidate, so the realised structure
never strays further than `δ_fill` outside the target volume. Default
`δ_fill` is one bond length; it is a pin.

The same tracker addressing (`motif_space_pos`, `site_index`) is used for
placed atoms and candidates, so neighbour lookups are O(1) for both.

### 4.3 Healing: peel and fill

Serves G1, G2 and G3. Not needed for stability on silicon (§0.1).

Let z be the bulk coordination of a motif site (4 for every diamond-cubic
site). Two moves, applied to convergence:

- **Peel**: remove any placed atom whose coordination is ≤ 1. A one-bonded
  atom is a methyl or silyl group hanging off the surface: it has to be built
  as a single-bonded atom and then terminated with three hydrogens. The
  current `rm_single` already removes it; zero-bonded atoms are the
  `rm_unbonded` case.
- **Fill**: add any candidate whose present neighbours number ≥ z − 1. Such a
  site is a one-atom pit: three terminators crowding one hole, and the single
  worst tip-access site on the surface. Filling it removes z − 1 dangling
  bonds and creates one.

**Convergence and order.** A peel only lowers the coordination of its
neighbours, so it can trigger further peels (a whisker unzips) but never
creates a fill candidate. A fill only raises the coordination of its
neighbours, so it can trigger further fills but never a peel. Therefore one
pass of peel-to-fixpoint followed by one pass of fill-to-fixpoint is a
fixpoint of both, and the result is order-independent within each pass. The
argument holds for each move on its own.

**Why the moves produce facets.** Both moves are the greedy descent of a
lattice-gas model whose energy is the number of dangling bonds, restricted to
a band around the target volume. The healed surface of a sphere or a Blender
blob therefore comes out composed of {111} and {100} micro-facets on its own,
because those are the low-dangling-bond local arrangements, and those are the
site types the operation library knows (G1). The user did not have to draw
them.

**What it costs: surface fidelity.** The healed surface may lie up to
`δ_fill` outside the drawn volume and one atomic layer inside it, and where
it lies depends on the local geometry. Two faces drawn to be complementary
are not complementary after healing. A face that must match something, or a
shape that is itself the object of study, should be filled with `heal: none`
or a region override.

**Options.** `heal: none | peel | fill | peel_fill` (default `peel_fill`).
The two moves are separately switchable because they answer different
questions: `peel` alone reproduces today's `rm_single` plus `rm_unbonded`;
`fill` alone shows how much volume the pit-filling adds without the whisker
removal confounding it. `none` reproduces today's sign test exactly, is the
baseline for every comparison, and stays the default for ionic lattices, where
`rm_unbonded: false` is used today. The thresholds are derived from z, so the
pass runs unchanged on any motif. The report (§4.7) counts what each move
changed, so a with-and-without run on the same shape is a one-number
comparison.

`TODO(measure)`: run the healer on a set of random free shapes (spheres,
cylinders, a torus, an imported blob) at several sizes and histogram the
surface site types of §4.4 before and after. This tells us which site types
the reconstruction pass must handle first, and how much volume the healer
moves.

### 4.4 Surface site classification

For each placed atom within the band, compute from the motif and the tracker:

- `coord`: number of present lattice neighbours,
- `n_db = z − coord`: number of dangling bonds,
- `db_dirs`: unit vectors of the missing bonds in real space (the same
  computation `hydrogen_passivate_dangling_bond` performs today),
- `n_hat`: the normalised sum of `db_dirs`, the local outward normal
  (undefined when `n_db = 0`).

Atoms with `n_db = 0` are bulk. This classification is the *only* input to
both reconstruction and passivation. It replaces the axis-threshold test of
`classify_atom_surface_orientation`, which recognised {100} atoms by their two
bonds lying along one Cartesian axis; the new test recognises every site type
by the count and geometry of its dangling bonds and is independent of the
cell's orientation in world space.

For diamond cubic the types are:

| `n_db` | Meaning | Example |
|---|---|---|
| 1 | monohydride site | (111) terrace, (110) row, S_A step edge |
| 2 | dimer candidate, else dihydride | (100) terrace, (100)/(111) corner |
| 3 | coordination 1 | removed by peel, never reaches here |

### 4.5 Reconstruction on diamond cubic

On diamond this pass serves existence (G5). On silicon it serves G2 (a dimer
halves the terminators on a (100) site), G4 (the 2×1 monohydride is a
minimum the model can start from; the 1×1 dihydride relaxes slowly and far),
and G1 (dimer rows and S_A/S_B steps are catalogued sites; an arbitrary
dihydride field is not). It is not needed for stability on silicon. Because
nature follows the model (§0.1), what this pass decides is what gets built,
so its rules are choices constrained by the site catalogue, not predictions.

It is explicitly scoped to the cubic diamond motif in carbon and silicon; the
gate in `get_reconstruction_params` stays, and a structure that fails it
skips the pass with a warning rather than silently. Everything in it is
expressed in terms of the §4.4 classification plus one lattice fact: on
diamond cubic, the two atoms of a 2×1 dimer are second neighbours at distance
a/√2 whose dangling bonds are parallel.

The pass is switchable as a whole (`surf_recon`) and in its two sub-moves,
`rebond` and the unmatched-site rule `dihydride`, so the effect of each can
be measured separately. `surf_recon: false` leaves every (100) atom a
dihydride and is the comparison baseline.

**Dimer candidates.** Build a graph whose vertices are the `n_db = 2` atoms and
whose edges join pairs (i, j) with

- |r_ij| within tolerance of a/√2,
- n̂_i · n̂_j above a cosine threshold (parallel outward normals, so both atoms
  are on the same facet), and
- r̂_ij perpendicular to n̂_i (the pair lies in the surface).

On a perfect (100) terrace this graph is a square grid and has exactly two
perfect matchings, the phases the current tables call A and B. At steps,
corners and domain boundaries the grid is irregular and the phase question
answers itself.

**Matching.** Take a maximum-cardinality matching of the candidate graph. The
graph is bipartite on a single terrace but not in general, so use a general
matching algorithm or, in the first implementation, greedy augmentation from
a seed per connected component with the checkerboard propagated along the
component. The seed choice is the only global freedom and the existing
`invert_phase` pin becomes its tie-break. Rows are seeded to run along the
component's longer extent; both orientations are stable, and the choice is
open question 6.

**Applying a dimer.** Unchanged from today: both atoms slide symmetrically to
the dimer bond length and drop along the outward normal by the amount that
keeps the two back-bonds bulk length. One geometry per lattice regardless of
passivation. The known limitation stays: the second layer is not relaxed
upward, so the first layer ends slightly low. `TODO(compute)`: relax a
dimerised, terminated slab with UMA and record the layer-2 displacement, to
decide whether to add it as a second derived constant (G4).

**Rebond.** Two unmatched dangling bonds that point at each other, with their
hosts closer than a threshold, become a host–host bond. This is the concave
corner case of `design_concave_rebonding.md` and also the rebonded S_B step;
the criteria (fraction of the van der Waals sum, host separation, facing
cosine) are kept. Because this now runs *before* passivation and the
classification is recomputed after every move, the ordering constraint that
forced the old pass to run after passivation disappears.

**Unmatched `n_db = 2` atoms.** What remains after matching and rebonding is
governed by the `dihydride` option, whose default depends on the element:

- `keep` (default on **silicon**): a dihydride is physical (the canted 1×1
  phase). Keep it, provided the terminator clash check of §4.6 passes against
  its neighbours.
- `remove` (default on **diamond**): a dihydride next to another dihydride or
  a dimer is sterically forbidden. Remove the atom (a peel) and recompute; the
  removal may free a partner for its neighbour. Iterate to a fixpoint; the
  loop is bounded because every iteration removes at least one atom.

Either value may be forced on either element. `remove` on silicon is the
experiment "how much volume does a dihydride-free (100) surface cost", and
`keep` on diamond shows, tagged and counted, exactly which sites the steric
rule would have removed.

`TODO(research)`: the exact steric rule on C(100). Is an isolated dihydride
between two monohydride dimer rows a stable site, or must every C(100)
surface carbon be dimerised? The 3×1 phase on Si and its absence on C is the
starting point.

**Steps and edges.** The matching produces S_A steps (rows parallel to the
edge) and S_B steps (rows perpendicular) from geometry alone. The rebonded
S_B configuration, where the step-edge dimer bonds down to the lower terrace,
is a rebond move and should emerge from the rule above when the geometry is
right. `TODO(measure)`: build the four canonical Si(100) step fixtures (S_A,
S_B non-rebonded, S_B rebonded, D_B) as cut geometries, run the pass, and
compare to Chadi's structures atom by atom.

**Out of scope for this pass**, and handled by authored patches if needed:
any reconstruction that changes the number of atoms in a surface unit cell
(adatom or dimer-adatom-stacking-fault structures on clean (111)), and any
non-diamond-cubic motif.

### 4.6 Passivation

Placement of terminators, bond lengths, halogen handling and the 24° dimer
tilt are reused as they are. What changes is the *input*: passivation places
one terminator per entry in the §4.4 `db_dirs` list as it stands after
reconstruction, instead of re-deriving dangling-ness from the motif. A host
that gained a dimer or rebond bond therefore has that direction removed from
its list and never receives an extra terminator, which is the invariant the
old code enforced through pass ordering and a per-atom flag.

**Clash check.** After placement, any two terminators closer than the clash
fraction of their van der Waals sum are found. What happens next is the
`clash` option: `report` only tags and counts them; `cant` (default on
silicon) resolves a clash between two dihydrides by canting both,
`TODO(research)`: the canting geometry of the Si(100) 1×1 dihydride phase;
`remove` peels one host and recomputes. On diamond a clash that survives §4.5
is an error in the reconstruction pass and is reported as such regardless of
the option. Together with complete passivation, this check is the one part of
the pipeline that is load-bearing for stability (§0.1), which is why `report`
never suppresses the tag, only the repair.

### 4.7 Report and tags

Serves G6, and is the instrument for §0.4.

The fill tags atoms so the user can see what happened: `healed:filled`,
`dimer`, `dihydride`, `rebond`, `step`, `clash`, `unreachable`. Tags flow
through the existing atom tag system and are visible in the viewport and the
text format.

`materialize` gains a second output pin, a `FillReport` record: counts of
peeled and filled atoms, dimers, rebonds, dihydrides, forbidden-site removals,
clashes found and repaired, and unreachable sites (§4.10); the number of
terminators, which is the G2 proxy; a histogram of §4.4 site types, which is
the G1 proxy; the maximum outward displacement of the realised surface from
the target volume, which is the fidelity cost of §4.3; and the bounding boxes
of any region where the fill could not reach a clash-free state. A node
network can assert on it, and two runs of the same shape with a pass toggled
differ in it by exactly what that pass did.

### 4.8 Performance and the old fast path

Every pass touches only atoms in the surface band, found through the stored
depth, and every neighbour lookup goes through the tracker. The target is that
a one-million-atom part fills in the same order of time as today. The current
table-driven {100} algorithm is kept behind the existing `surf_recon` path
until the new pass reproduces its results on the regression fixtures
(the 2391-atom cuboid, the Si(100) step-edge fixture, the region tests); then
it is removed. The mesh primitive's winding-number evaluation is the one new
cost that scales with input complexity, and it is bounded by the BVH.

`TODO(measure)`: time the healer and the matching on the T-centre nanobeam
(1.07 M atoms) and on a 100 k-atom imported mesh.

### 4.9 Node interface

`materialize` keeps its name and its Blueprint input; a mesh Blueprint is just
a Blueprint whose alignment is `lattice_unaligned`. New and changed pins:

| Pin | Type | Default | Serves | Notes |
|---|---|---|---|---|
| `heal` | Int (enum) | `peel_fill` | G1 G2 G3 | `none`, `peel`, `fill`, `peel_fill` (§4.3) |
| `fill_band` | Float | one bond length | — | `δ_fill` of §4.2; bounds the fidelity cost |
| `surf_recon` | Bool | true | G5 G2 G4 G1 | the matching pass of §4.5 |
| `rebond` | Bool | true | G4 | existing pin; the rebond move of §4.5 |
| `dihydride` | Int (enum) | by element | G4 G5 | `keep`, `remove` (§4.5); Si → `keep`, C → `remove` |
| `invert_phase` | Bool | false | G6 | seed tie-break for the matching |
| `clash` | Int (enum) | by element | stability | `report`, `cant`, `remove` (§4.6); Si → `cant`, C → `report` |
| `access_check` | Bool | false | G3 | the tip-access check of §4.10; off until Phase 2b lands |
| `rm_unbonded`, `rm_single` | Bool | — | — | kept for old files; `heal` supersedes them |
| `report` (output) | Record | — | G6 | §4.7 |

Passivation itself (`passivate`, `passiv_elem`) keeps its current pins and
is the one step with no experimental "off" beyond what exists today, because
an unterminated surface is the one thing that is not stable (§0.1).

All of these are region-overridable through `MaterializeRegion`, so a single
part can carry different settings on different faces. Text-format names follow
the pin names. Old `.cnnd` files load with `heal` derived from `rm_unbonded`
and `rm_single`, and with every new option at its default.

**The baseline configuration** for any comparison is `heal: none`,
`surf_recon: false`, `clash: report`, `access_check: false`. It is today's
sign test plus passivation, and every other configuration is measured against
it through the report.

### 4.10 Tip-access check

Serves G3, the constraint §2.2 states and the passes above do not enforce.

For every surface site in the §4.4 classification, test whether some tool in
the active operation library can reach it: take each tool's approach envelope
(the solid the library already stores for the trajectory sweep), place it on
the site's outward normal, and test for overlap with the placed atoms. A site
that no tool can reach is tagged `unreachable` and counted in the report,
with the nearest obstructing atoms recorded so the user can see which step or
terrace is in the way.

In this design it is a **check, not a move**: it changes nothing and reports.
Turning it into a move (peel the obstruction, or widen the terrace) is a
question for a later revision once the census below shows how often it fires
on realistic shapes. It is off by default until Phase 2b lands, because it
needs an operation library on the node and costs one envelope test per
surface site per tool.

`TODO(measure)`: run the check on the random-shape set of §4.3 and on the
four Si(100) step fixtures of §4.5, and report the fraction of unreachable
sites by site type. This is the number that decides how much of G3 the
healer already delivers by accident.

## 5. The site catalogue and the verification relax

Serves G4, and supplies the `TODO(research)` answers the passes depend on.

**Catalogue.** Enumerate the local environments that §4 produces on realistic
shapes (the census of §4.3), and for each one record whether it exists as a
stable minimum, its relaxed geometry, and the steric limit between its
terminators (§0.5). The catalogue lives in a data file with provenance, like
the mechanosynthesis operation libraries, not in code. The fill's geometry
constants (dimer lengths, dihydride canting, layer-2 displacement) are read
from it, so that improving a number does not mean editing a pass.

**How the entries are made.** For each environment build a small terminated
cluster or slab, relax it with UMA through Elementa, and later confirm a
subset with DFT. An environment that rearranges during relaxation does not
exist and is removed from what the fill may produce; one that stays gives its
geometry. `TODO(compute)`: the initial set is the (100) 2×1 monohydride, the
(100) dihydride on Si, the (111) and (110) monohydrides, the four (100) step
types, the (100)/(111) concave corner with and without rebond, and the same
for diamond.

**Verification.** A final relaxation of the surface shell of a filled part
with a reactive model (UMA, or a Tersoff-type potential if one is added to
`simulation`) is a check on the fill's output, not a replacement for it: the
unreconstructed 1×1 surface is a saddle point that gradient descent does not
leave on its own, so the dimers must be placed before relaxation. The check
is that the relaxed shell stays close to the model; a large move is a site
the catalogue got wrong.

**Not planned: energy-driven healing.** Replacing the dangling-bond count
with summed site energies and the greedy descent with annealing was
considered. It would rank stable alternatives by energy, which is not a goal
(§0.3); the ranking the design cares about is operation count and site-type
coverage, and the dangling-bond count is already a direct proxy for both. If
a ranking by energy is ever wanted, it is a change of scorer with the same
moves, and can be added then.

## 6. Deferred: features for mechanical design

Not part of the phasing below. Recorded here so that the fill design does not
have to be reopened when they are picked up.

### 6.1 Aligning an imported mesh by one face: `align_face`

**Why.** A part for a mechanical design usually has one or two faces that
matter: the face that slides, meshes or mates. Those faces need to be exact
lattice planes so that the fill produces a flat terrace rather than a
staircase, and the relative orientation of two such faces on two parts is
what decides whether the contact is commensurate or superlubric (§0.3). The
rest of the part can be whatever Blender produced. `align_face` gives the
user that control in the simplest form: **the lattice stays fixed, the mesh
is rigidly rotated** so that one picked face becomes a chosen lattice plane,
and optionally translated so that face sits in a chosen layer gap.

**What one face fixes.** Matching the face normal to (hkl) fixes two
rotational degrees of freedom. The remaining one, the spin about the normal,
defaults to the minimal rotation that takes the mesh normal onto the lattice
normal, and can be overridden with one field. Position along the normal
defaults to unchanged and can be snapped to a layer gap of §2.3. In-plane
position stays where the mesh was.

**Node.** `align_face` in Geometry3D:

| Pin | Type | Default | Meaning |
|---|---|---|---|
| `shape` | Blueprint | required | the imported mesh, or any free geometry |
| `point` | Vec3 | picked | a point on the chosen face, Å |
| `normal` | Vec3 | picked | the face's outward normal |
| `miller` | IVec3 | (1,0,0) | the lattice plane the face becomes |
| `spin` | Float | 0.0 | extra rotation about the normal, degrees, applied after the alignment |
| `snap` | Bool | false | translate along the normal so the face sits in a layer gap |
| `gap` | Int | 0 | which gap of the §2.3 layer table; 0 = the monohydride gap nearest to the face's current position |

Output: the same Blueprint, rigidly transformed. The face is identified by a
point and a normal, not by a triangle index, so re-exporting the mesh from
Blender keeps the constraint. In the text format:

```
part = align_face { shape: mesh, point: (12.0, 3.5, 40.2), normal: (0.31, 0.95, 0.0), miller: (1, 1, 1), snap: true }
```

**Fill interaction.** None required. The aligned face is an exact lattice
plane, so the ordinary fill produces a flat terrace on it and staircases
elsewhere. If the healer's fidelity cost (§4.3) on that face is unwanted, a
`MaterializeRegion` with `heal: none` over it is the existing mechanism.

**UI.** Three things.

- **Pick face.** A button on the node's property panel enters a viewport pick
  mode. Hovering highlights the planar region under the cursor, found by
  grouping connected triangles whose normals agree within a small tolerance.
  Clicking writes `point` and `normal`.
- **Miller index field with a hint.** Next to the index field, the angle the
  mesh will be rotated by, and a short list of the low-index planes nearest
  to the face's current orientation with their angles, so a user who
  modelled the part roughly aligned in Blender sees "(111) at 1.3°" and picks
  it.
- **Spin and snap.** A degrees field and a checkbox with a gap number. The
  viewport shows the lattice axes gizmo at the part and the target plane as a
  translucent overlay, so the effect of each field is visible immediately.

**Second face.** Not a separate constraint. A user who wants a second face on
a lattice plane reads its angle off the hint list after aligning the first
and adjusts `spin` until that angle is small. This is enough for a bearing:
align the sliding face of each part with its own `align_face`, and use `spin`
on one of them to leave or enter registry.

**Deferred beyond this.** Two-face consistency checking with suggested
indices, least-squares fitting of several faces, an in-plane phase for
commensurate mating, a commensurability indicator for a pair of faces, and
face tags that a later mechanical simulator can use to find contact pairs.
All of them build on `align_face` without changing it.

## 7. Phasing

| Phase | Content | Serves | Size | Depends on |
|---|---|---|---|---|
| 0 | Layer-spacing shift quantum and float offset on the lattice-constrained nodes (§3.1); the layer-census program | correctness of the existing nodes | small | — |
| 1 | `Mesh` primitive and `import_mesh` node (§3.2) | the whole premise (D1) | medium | — |
| 2 | Healer with the `heal` switch, candidate band, report pin, tags (§4.2, 4.3, 4.7) | G1 G2 G3 G6 | medium | — |
| 2b | Tip-access check (§4.10) | G3 | medium | 2, op library on the node |
| 3 | Classification, matching reconstruction with `dihydride`, rebond, passivation from classification with `clash` (§4.4–4.6) | G5, then G2 G4 G1 | large | 2, catalogue entries for the sites it places |
| 4 | Regression parity with the old path, removal of the old path, performance, reference-guide update | — | medium | 3 |
| 5 | Free node family (§3.1 table) | usability | medium, incremental | — |
| 6 | Site catalogue as a data file, fill constants read from it, verification relax (§5) | G4, research | ongoing | 3 |

Phases 0, 1, 2 and 5 are independent and can proceed in any order. Phase 2 is
the first one that produces measurements: once the report exists, every
`TODO(measure)` in §4 is a with-and-without run. Phase 2b is placed before 3
because G3 is the constraint that limits free-shape builds on silicon, and
because its census tells Phase 3 which step types matter. The catalogue
starts before Phase 3 and never finishes; Phase 3 needs entries only for the
sites it places. The user-facing reference guide pages for `materialize`, the
geometry nodes and the new import node are updated in the phase that changes
them.

## 8. Open questions

1. Should the free nodes and the lattice-constrained nodes share one node type
   with a mode, or stay separate types as `free_sphere` does today? Separate
   types are the current precedent and keep the text format unambiguous.
2. Does the healer's fill move need to respect the motif's *sublattice* on
   zincblende-type motifs with two elements, so that it never creates a
   like–like bond? On homonuclear diamond it cannot matter; on a compound it
   is automatic because the candidate carries its site index.
3. Is `δ_fill` better expressed in bond lengths or in layer spacings along the
   local normal?
4. How should the report expose "could not reach a clash-free state": as a
   warning on the node, as tags, or both?
5. Whether the (111) monohydride surface needs any geometric relaxation in the
   fill (the H-terminated (111) is essentially bulk-terminated; the current
   code applies none and that is probably right).
6. The dimer-row tie-break at steps (§4.5). Rows along the longer extent
   give S_A steps on one edge and S_B on the other; both exist. The choice
   should be the one whose step sites the operation library covers, which
   the §4.10 census will show.
7. Whether the tip-access check (§4.10) should become a move, and if so which
   one: peel the obstruction, or leave a terrace one atom wider. Decide from
   the §4.10 census, not in advance.
8. Whether a face that must mate with another part needs a first-class
   "fidelity" marker rather than a region override with `heal: none`. This
   belongs with `align_face` (§6) and is only noted here.

## 9. Decisions so far

- **D1.** The design volume is continuous and unconstrained; the lattice and
  the chemistry enter only in `materialize`. Lattice-constrained nodes remain
  as a convenience, not as the model.
- **D2.** Mesh import comes before the free node family. Both feed the same
  fill.
- **D3.** STEP is handled by tessellation to the mesh primitive, in pure Rust,
  later. No CAD kernel dependency.
- **D4.** Healing and classification are lattice-generic and expressed in the
  motif's coordination. Reconstruction is diamond-cubic C and Si only, gated
  with a visible warning.
- **D5.** The dimer phase is a matching tie-break, not a global table.
- **D6.** Rebonding is a move of the reconstruction pass and runs before
  passivation; passivation reads the post-reconstruction dangling-bond list.
- **D7.** One dimer geometry per lattice regardless of passivation, as already
  decided for the current code.
- **D8.** Stability is the floor, not the goal. On silicon only complete
  passivation and the terminator clash check are load-bearing for stability;
  every other pass is justified by G1–G6 (§0.2) and says so where it is
  described.
- **D9.** Nature follows the model. The reconstruction pass chooses what is
  built, constrained by the site catalogue; it does not predict. Research is
  therefore scoped to existence, geometry and steric limits of terminated
  sites (§0.5); surface energies and equilibrium shapes are background.
- **D10.** Every pass is individually switchable, with the baseline of §4.9
  as the exact reproduction of today's sign test plus passivation. Peel and
  fill are separate values of `heal` because they answer different
  questions. Defaults are the recommended configuration; the switches exist
  so that the `TODO(measure)` questions are settled by the report, not by
  argument.
- **D11.** Tip access is an explicit, reported check (§4.10, Phase 2b), not a
  move, until its census shows what a move should do.
- **D12.** Energy-driven healing is not planned (§5). The site catalogue is a
  data file that the fill reads its constants from; the moves stay as
  designed.
- **D13.** Functional faces (bearing faces, lattice mismatch between parts)
  are not produced by the fill. They are made by aligning the imported mesh
  one face at a time (`align_face`, §6, deferred); the lattice stays fixed
  and the mesh rotates.
