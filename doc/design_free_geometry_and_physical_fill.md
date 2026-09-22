# Free Geometry and the Physical Fill

Draft, 2026-09-22.

## 1. The problem

Creating geometry in atomCAD is hard, for two reasons that compound.

The first is the modelling paradigm. A shape is a Boolean combination of
implicit surfaces: half spaces, spheres, extruded polygons, intersected and
subtracted through a node network. Implicit modelling is a legitimate way to
describe a solid, but it is not a convenient way to *author* one. There is no
direct manipulation for most primitives, no way to grab a face and pull it,
and every change to a shape is a change to a formula. Users who know Blender
or a mechanical CAD tool have to leave what they know at the door.

The second is the lattice. Almost every primitive takes integer inputs:
`half_space`, `cuboid`, `facet_shell`, `drawing_plane`, `polygon` and
`extrude` all want Miller indices for their normals and whole lattice cells
for their positions, and `sphere` and `circle` are lattice-covariant
ellipsoids rather than round balls. In practice only small Miller indices are
usable, so an oblique face has to be approximated by a staircase of low-index
planes, and a position between two lattice cells needs a separate
`subdivision` factor. Only `free_sphere` and `free_circle` escape this. The
lattice constraint was adopted because it is the shape of an annealed crystal
(section 3.3), but a mechanosynthesised part never anneals, so the constraint
buys very little physics for what it costs in usability.

The fill that turns geometry into atoms has its own limits. It keeps every
lattice site that lies inside the drawn volume, a step this document calls
the **geometric cut** (in the code it is a sign test on the signed-distance
function, SDF, that represents the volume), and then runs a chain of repairs:
remove unbonded atoms, remove single-bonded atoms, dimerise {100} faces,
passivate, and fix clashes in concave corners. The surface reconstruction
knows only the six {100} normals, only cubic diamond in carbon or silicon,
and only one global dimer phase. Edges, corners and every other face get
generic passivation. On diamond an unpaired (100) carbon then carries a
dihydride that cannot exist. The concave repair exists because the
reconstruction has no notion of an unpaired atom.

## 2. The goals

> Let the user model the shape in whatever tool suits them, and let atomCAD
> turn that shape into a crystal that can be built and that stays built.

Concretely: given any closed volume, whether a triangle mesh imported as STL
or OBJ from Blender or a mechanical CAD tool, or later a shape composed from
free-form implicit surfaces, fill it with silicon (and later diamond) so that
the result is plausibly mechanosynthesisable and stable at room temperature
once built. For some parts a third goal applies: control over the
crystallography of chosen faces.

### 2.1 Mechanosynthesisability

The part must be buildable site by site with positional tools at cryogenic
temperature in ultra-high vacuum.

We do not know of a publicly available, concrete mechanosynthesis operation
library that has been shown to work in practice. This document therefore
assumes that a working library will be similar to the minimal toolset for
diamond mechanosynthesis (Freitas and Merkle, 2008): tools that abstract a
hydrogen, tools that donate one, and tools that place a single atom or a
dimer on a prepared site. Any library of this kind has three properties that
the fill has to respect:

- It acts on a **finite catalogue of surface sites**. Each operation has an
  applicability pattern, a local arrangement of atoms and terminators it can
  act on. A surface site that matches no pattern cannot be built.
- Every site must be **reachable by a tool tip**. A one-atom pit at the
  bottom of a narrow trench may be chemically fine and still impossible to
  build.
- **Every terminator costs operations.** A finished part is fully hydrogen
  terminated, and each of those hydrogens was placed, and the atom under it
  was placed before that. Fewer surface sites and fewer dangling bonds mean a
  shorter build.

Most of what the fill does beyond the geometric cut serves this goal: fewer
site types, fewer operations, every site reachable, and a model close enough
to its relaxed geometry that a simulation checks the design rather than
rebuilds it.

### 2.2 Room-temperature stability

Once built, the part must not rearrange, desorb or react at room temperature.
This is the easier goal, and section 3.1 explains why: complete passivation
and a rule against two terminators sitting too close are sufficient on both
materials. On silicon the cut surface almost satisfies them already; on
diamond satisfying them takes a reconstruction. The design is for vacuum;
section 3.2 says what changes in air.

### 2.3 Controlled surface crystallography

A mechanical design often has one or two faces whose crystallography matters:
a pair of sliding surfaces that should be superlubric, a pair of mating
surfaces that should be commensurate and stick, or simply a face that the
designer wants on a particular lattice plane at a particular orientation.
The free fill does not provide this on its own, because a free shape has no
notion of which of its faces is special. Section 7 describes the feature that
adds it, aligning a chosen face of the imported shape to a chosen Miller
index. It is deferred only in schedule, not in motivation.

## 3. What the physics constrains

### 3.1 Hydrogen-terminated surfaces do not move

A hydrogen-terminated silicon or diamond surface is kinetically stable far
above room temperature. Silicon, the weaker of the two, as the worked
example:

| Process on H-terminated Si | Onset |
|---|---|
| H₂ desorption from the (100) monohydride | ~700 K |
| (100) dihydride → monohydride conversion | ~650 K |
| Si–H bond dissociation | ~3.2 eV, never thermal at 300 K |

On diamond every corresponding number is higher. The (100) 1×1 dihydride is
a real room-temperature phase on silicon. A one-atom pit terminated with
three hydrogens is a hydrogenated vacancy, a well-known stable defect. A
one-coordinated atom with three hydrogens is a silyl or methyl group,
chemically ordinary. A random-shaped hydrogen-terminated nanocrystal of
either element sits in vacuum indefinitely because nothing on its surface
can move.

**The stability floor** is therefore the same two conditions for both
materials: no dangling bond is left unterminated, and no two terminators are
forced below their steric limit. Nothing else in this design is needed
for stability.

A consequence worth stating once: **the built part is whatever the model
says**. On a fully terminated surface every alternative arrangement, a
different dimer phase, an unpaired dihydride, a step that is or is not
rebonded, is separated from its neighbour by breaking an X–H or X–X bond at
2 to 4 eV. Nature does not correct the model; it freezes it. The
reconstruction pass of section 5.5 is therefore not predicting what the
surface will do. It is choosing what we build, under the single constraint
that every local configuration it chooses exists as a stable minimum. The
exceptions are the barrierless cases: a dihydride cants, two terminators
inside their steric limit react, and a bare dangling bond reacts with whatever
it meets.

**Where the naive fill violates the floor.** A geometric cut followed by
complete passivation satisfies the first condition by construction. Whether
it satisfies the second depends on the material, and this is the only place
silicon and diamond differ.

On silicon the geometric cut produces clashes only in special places: two
terminators pointing at each other across a concave corner, and neighbouring
(100) dihydrides whose hydrogens sit close but can be relieved by canting.
The repair is local and keeps the bond graph. A plain geometric cut, complete
passivation and a clash rule already produce a room-temperature-stable
silicon object.

On diamond the clash sits on the most common site. The (100) site spacing
shrinks from 3.84 Å on silicon to 2.52 Å on diamond, while the X–H bond only
shrinks from 1.48 Å to 1.09 Å, so the terminators are relatively much larger.
An unreconstructed C(100) dihydride field is a clash at every site, and
canting cannot open it. The only repair is to change the bond graph by
dimerising. So on diamond the reconstruction pass decides whether the surface
exists at all, and it is part of the fill rather than a cleanup after it; on
silicon the same pass is an optimisation of build cost. The rule is the same
in both cases. What differs is how much of the cut surface the rule rejects.

### 3.2 Vacuum and air

The stability argument above is for vacuum, and vacuum is where the part is
built. It is worth separating the two environments explicitly.

In vacuum a fully terminated silicon or diamond part is stable indefinitely
at room temperature. A single unterminated dangling bond is also harmless in
vacuum; it just stays a radical.

In air the picture differs by material. A hydrogen-terminated diamond surface
is stable in air. A hydrogen-terminated silicon surface oxidises slowly, over
hours to days, as freshly HF-etched wafers do; an unterminated dangling bond
reacts immediately. So a silicon part that must live in air ends up with an
oxide shell, or with a different terminator, regardless of which lattice
sites were filled.

This document designs for vacuum. Air stability of silicon is a question of
surface chemistry after the build, not of the fill, and is left open.

### 3.3 Low-index facets are convenient, not required

The half-space-with-Miller-index model comes from equilibrium thermodynamics.
A crystal that minimises its surface free energy at fixed volume takes the
Wulff shape: an intersection of half spaces, one per orientation, each at a
distance proportional to the surface energy of that orientation. That is the
exact shape of an annealed crystal, and it is the justification for building
the whole of a part out of Miller-indexed planes. The other reason to want a
lattice plane, a face that must slide or mate against another part, applies
to particular faces rather than to the whole shape, and section 7 provides
for it.

A mechanosynthesised part is built site by site and never anneals. Facet
selection by surface energy never happens to it. What remains is a constraint
per site: every surface atom must be locally stable, passivatable without a
steric clash, and reachable by a tip. Adamantane is the existence proof: ten
carbons cut from the diamond lattice with no facet at all, perfectly stable
because every carbon is CH or CH₂. Any shape whose surface sites all belong to
a known, stable catalogue is a valid design.

Low-index facets still matter, for a different reason. For a covalent crystal
the first estimate of surface energy is the density of dangling bonds, and
for diamond cubic:

| Face | Dangling bonds per a² |
|---|---|
| (111) | 2.31 |
| (110) | 2.83 |
| (100) unreconstructed | 4.00 |
| (100) 2×1 dimerised | 2.00 |

Dangling bonds are terminators, and terminators are operations. The faces
with the fewest dangling bonds are also the faces with the fewest site types,
which are exactly the sites an operation library covers. So {111} and {100}
are cheap to build and easy to cover, and a fill that minimises dangling
bonds will drift toward them on its own. Every other orientation is a
staircase of these terraces with steps between, and what such a surface needs
is correct chemistry at the steps, not a prohibition on the plane.

This reframes the fill. The question is no longer "which planes may the user
draw" but "given any target volume, which subset of lattice sites near it has
only acceptable surface sites". That is a discrete optimisation on the bond
graph, and it is what section 5 computes.

### 3.4 The correct position quantum is the layer spacing

Independently of the above, the lattice-constrained nodes quantise the
position of a cutting plane wrongly. A plane is a classifier of which atoms
are in. The meaningful quantum for its position is the spacing between
successive atomic layers along its normal, and the cut should sit between
layers, never on one. Today the `shift` is a multiple of the
conventional-cell d-spacing divided by an optional `subdivision`. For
diamond cubic that is wrong in two ways:

| Normal | d-spacing used today | Actual layer spacings | Consequence |
|---|---|---|---|
| (100) | a | a/4 | one termination in four is reachable |
| (110) | a/√2 | a√2/4 | one in two |
| (111) | a/√3 | 0.144a and 0.433a alternating | shift lands on a layer, not in the wide gap |

The (111) case matters most. Diamond (111) stacks in bilayers: two layers
0.144a apart, then a gap of 0.433a. A cut in the wide gap leaves one dangling
bond per surface atom, the monohydride (111) surface. A cut in the narrow gap
leaves three per atom, a surface nothing forms. An integer `shift` on (111)
today lands exactly on a layer, so one face of a slab comes out as methyls
and the opposite face as monohydride. The rule follows from the motif, not
from the cell: the shift quantum along (hkl) is the set of gaps between the
projected motif layers, and the UI should snap to the midpoints of those
gaps.

### 3.5 What is generic and what is specific to diamond cubic

Much of the geometry layer was built to work for any lattice, while the
target for the foreseeable future is cubic silicon, with diamond second. This
design separates what depends only on the motif and the bond graph from what
is honestly specific to diamond cubic.

Generic to any covalent lattice:

- the coordination of each placed atom against its bulk coordination z,
- the direction of each missing bond,
- steric clash between terminators placed on those directions,
- the peel-and-fill healing of section 5.3, whose thresholds are expressed
  in z.

Specific to diamond cubic silicon and carbon, and the scope of the
reconstruction pass:

- the 2×1 dimer on {100}, with its measured geometry (Si dimer 2.44 Å, C
  dimer 1.63 Å, vertical drop derived so the back-bonds stay bulk length, one
  geometry per lattice regardless of passivation),
- which hydride phases exist: on Si(100) the 2×1 monohydride, the mixed 3×1
  and the canted 1×1 dihydride are all real, while on C(100) only the 2×1
  monohydride is sterically possible,
- (111) and (110) hydrogen-terminated surfaces are 1×1 for both elements;
  the clean-surface reconstructions (Si 7×7, Pandey chains) do not survive
  termination and never need generating,
- the step structures on (100): single S_A and S_B steps, the rebonded S_B,
  and the double D_B step,
- the edge and corner structures of hydrogen-terminated nanocrystals.

## 4. Where the geometry comes from

The fill consumes a Blueprint: a Structure plus a signed-distance geometry
tree. It does not care where the tree came from. Three sources, in delivery
order.

### 4.1 Mesh import, first

Users model in Blender or a mechanical CAD tool and export a triangle mesh.
atomCAD imports it as geometry. This comes first because it gives designers a
complete modelling tool on day one, and because the fill needs nothing from
the geometry but a sign and a distance.

**Node.** `import_mesh` in the Geometry3D category, with pins `file: Text`,
`scale: Float` (file units to ångströms, default 1.0) and an optional
`transform`. It outputs a Blueprint flagged as lattice-unaligned, paired with
a `structure` pin the same way `free_sphere` is. Formats: binary and ASCII
STL, and OBJ with triangles and quads (quads split). The mesh is stored as a
path relative to the design file and re-parsed on load, as `import_xyz` and
`import_cube` do; it is never embedded in the `.cnnd`.

**Primitive.** A new `Mesh` kind in the geometry tree crate (hash tag 0x10,
the next free one) holding the triangle list and a bounding-volume hierarchy
built once at construction.

- Sign by generalised winding number (Jacobson et al. 2013). It is robust to
  small holes and to inconsistent triangle orientation, which real exports
  have, and it degrades gracefully instead of flipping the inside of the
  whole part on one bad triangle. The exact sum costs one term per triangle
  per query; use the BVH-based approximation from the same paper, with the
  exact sum only near the surface.
- Magnitude as the distance to the closest triangle. It is 1-Lipschitz, so
  the existing box-culling test in the fill stays valid with no scale factor.
- Batch evaluation through the existing 1024-point interface; the BVH makes
  each query logarithmic in triangle count.
- CSG conversion is trivial: the triangles are already the mesh.

**Diagnostics.** The node reports the triangle count, whether the mesh is
closed, and the bounding box in ångströms, and warns when the box is smaller
than one unit cell or larger than the fill volume.

Editing meshes inside atomCAD is out of scope. Aligning one face of a mesh to
a lattice plane is the deferred node of section 7.

### 4.2 STEP import, later, by tessellation

A STEP file is an exact boundary representation: trimmed surface patches
joined by a topology of edges and loops. Point-in-solid on that
representation is a CAD-kernel problem, and we will not write a kernel. The
route is to tessellate at import and hand the triangles to the mesh
primitive. The intended implementation is the pure-Rust `truck` crates:
`truck-stepio` reads the file into a B-rep and `truck-meshalgo` triangulates
it to a chord tolerance, which keeps the build free of a C++ kernel on every
Flutter platform. When a file uses an entity the crate does not handle, the
node reports it and suggests STL export. Open CASCADE bindings were
considered and rejected for the build cost. Until this lands, exporting STL
from the authoring tool is the interim.

### 4.3 The free node family

The existing nodes remain as the lattice-constrained library. They are still
the right tool when the user wants an exact (111) face or a part that must
tile with another on integer lattice vectors. Alongside them grows a free
library: the same primitives in real-space ångströms with no snapping,
following `free_sphere` and `free_circle`. Each lowers to an existing
geometry-tree kind:

| Free node | Lowers to | Inputs |
|---|---|---|
| `free_half_space` | half space | point (Vec3 Å), normal (Vec3) |
| `free_box` | intersection of 6 half spaces | centre, half extents, orientation |
| `free_cylinder` | extrude of circle | base point, axis, radius, height |
| `free_extrude` | extrude | 2D shape, plane origin, plane normal, height |
| `free_polygon` | polygon | vertices (Vec2 Å) on a free plane |
| `free_plane` | drawing-plane analogue | origin, normal, in-plane u |
| `free_transform` | transform | translation, rotation, uniform scale |

This is the long-term modelling surface for shapes authored inside atomCAD.
It is still implicit geometry, and nobody has built an editing experience
for implicit geometry that matches a boundary-representation modeller, so
the free library complements mesh import rather than replacing it.

### 4.4 Two fixes to the lattice-constrained nodes

The shift quantum of `half_space`, `drawing_plane`, `extrude` and
`facet_shell` becomes the motif layer spacing along the normal (section
3.4), snapping to gap midpoints, with a float `offset` pin as the override.
`subdivision` becomes unnecessary for these nodes and is kept only for
loading old files. `facet_shell` currently hard-codes a subdivision of 1 and
gets the same treatment.

## 5. The physical fill

### 5.1 Pipeline

The new `materialize` pipeline, with the pass it replaces on the right:

| Step | Pass | Generic? | Replaces |
|---|---|---|---|
| 1 | Sample motif sites, keep the signed distance per atom | yes | unchanged |
| 2 | Create motif bonds | yes | unchanged |
| 3 | **Heal**: peel and fill on the bond graph | yes | `rm_unbonded`, `rm_single` |
| 4 | **Classify** surface sites by their dangling bonds | yes | the axis-threshold orientation test |
| 5 | **Reconstruct**: dimer matching, rebond, unmatched-site rule | diamond cubic | the {100} phase tables, concave rebonding |
| 6 | **Passivate** from the classification, then check clashes | yes | passivation (placement unchanged, derivation changes) |
| 7 | **Access check** against the tool envelopes, optional | yes | new |
| 8 | **Report** and tag | yes | new |

Steps 3 to 7 operate only on atoms within a shallow band of the surface,
found through the signed distance the fill already stores on each atom, so
the whole chain is linear in the number of surface atoms.

One principle governs the switches. We do not yet have measurements of how
much each pass matters on realistic shapes, and the right way to get them is
to run the same shape with a pass on and off and compare. So **every pass is
individually switchable**, the defaults are the recommended configuration,
and the baseline configuration (section 5.9) reproduces today's geometric cut
plus passivation exactly, atom for atom. Nothing is silently mandatory except
passivation itself, because an unterminated surface is the one thing that is
not stable. The switches are region-overridable through the existing
`MaterializeRegion` mechanism, so one part can carry a healed body and an
unhealed face for comparison.

### 5.2 Sampling and the candidate band

Step 1 is unchanged: every motif site with signed distance at or below the
inclusion threshold is placed, and the distance is kept on the atom. One
addition: sites in the outer band `0 < sdf ≤ δ_fill` are enumerated too, not
placed, but recorded in the placed-atom tracker as *candidates* with their
lattice address. The fill move below can only ever add a candidate, so the
realised structure never strays further than `δ_fill` outside the target
volume. The default is one bond length, and it is a pin.

Placed atoms and candidates share the tracker's addressing, so neighbour
lookups are constant time for both.

### 5.3 Healing: peel and fill

This pass serves buildability, build cost and tip access. It is not needed
for stability on silicon.

Let z be the bulk coordination of a motif site (4 for every diamond-cubic
site). Two moves, applied to convergence:

- **Peel**: remove any placed atom whose coordination is at most 1. A
  one-bonded atom is a methyl or silyl group hanging off the surface: it has
  to be built as a single-bonded atom and then given three hydrogens. This is
  what `rm_single` and `rm_unbonded` do today.
- **Fill**: add any candidate whose present neighbours number at least
  z − 1. Such a site is a one-atom pit: three terminators crowding one hole,
  and the single worst tip-access site on the surface. Filling it removes
  z − 1 dangling bonds and creates one.

**Convergence and order.** A peel only lowers the coordination of its
neighbours, so it can trigger further peels (a whisker unzips) but never
creates a fill candidate. A fill only raises the coordination of its
neighbours, so it can trigger further fills but never a peel. One pass of
peel to fixpoint followed by one pass of fill to fixpoint is therefore a
fixpoint of both, and the result is order-independent within each pass.

**Why the moves produce facets.** Both moves are the greedy descent of a
lattice-gas model whose energy is the number of dangling bonds, restricted to
a band around the target volume. The healed surface of a sphere or a Blender
blob comes out composed of {111} and {100} micro-facets on its own, because
those are the low-dangling-bond local arrangements, and those are the site
types the operation library knows. The user did not have to draw them.

**What it costs.** The healed surface may lie up to `δ_fill` outside the
drawn volume and one atomic layer inside it, and where it lies depends on the
local geometry. Two faces drawn to be complementary are not complementary
after healing. A face that must match something, or a shape that is itself
the object of study, should be filled with `heal: none` or a region override.

**Options.** `heal: none | peel | fill | peel_fill`, default `peel_fill`.
The two moves are separately switchable because they answer different
questions: `peel` alone reproduces today's behaviour, and `fill` alone shows
how much volume pit-filling adds without whisker removal confounding it.
`none` reproduces today's geometric cut exactly, is the baseline for every
comparison, and stays the default for ionic lattices, where `rm_unbonded:
false` is used today. The thresholds are derived from z, so the pass runs
unchanged on any motif.

### 5.4 Surface site classification

For each placed atom within the band, compute from the motif and the tracker:

- `coord`: the number of present lattice neighbours,
- `n_db = z − coord`: the number of dangling bonds,
- `db_dirs`: unit vectors of the missing bonds in real space (the same
  computation passivation performs today),
- `n_hat`: the normalised sum of `db_dirs`, the local outward normal.

Atoms with `n_db = 0` are bulk. This classification is the only input to both
reconstruction and passivation. It replaces the current test, which
recognises {100} atoms by their two bonds lying along one Cartesian axis; the
new test recognises every site type by the count and geometry of its dangling
bonds and is independent of the cell's orientation in world space.

For diamond cubic the types are:

| `n_db` | Meaning | Example |
|---|---|---|
| 1 | monohydride site | (111) terrace, (110) row, S_A step edge |
| 2 | dimer candidate, else dihydride | (100) terrace, (100)/(111) corner |
| 3 | coordination 1 | removed by peel, never reaches here |

### 5.5 Reconstruction on diamond cubic

On diamond this pass decides existence. On silicon it halves the terminators
on a (100) site, turns an arbitrary dihydride field into catalogued sites
(dimer rows, S_A and S_B steps), and gives a simulation a starting point that
is already near a minimum, where the 1×1 dihydride relaxes slowly and far. It
is not needed for stability on silicon. Because the built part is whatever
the model says (section 3.1), what this pass decides is what gets built, so
its rules are choices constrained by the site catalogue, not predictions.

It is scoped to the cubic diamond motif in carbon and silicon. The existing
gate stays, and a structure that fails it skips the pass with a warning
rather than silently. Everything in the pass is expressed in terms of the
classification above plus one lattice fact: on diamond cubic, the two atoms
of a 2×1 dimer are second neighbours at distance a/√2 whose dangling bonds
are parallel.

The pass is switchable as a whole (`surf_recon`) and in its two sub-moves,
`rebond` and the unmatched-site rule `dihydride`. With `surf_recon: false`
every (100) atom stays a dihydride, which is the comparison baseline.

**Dimer candidates.** Build a graph whose vertices are the `n_db = 2` atoms
and whose edges join pairs (i, j) with |r_ij| within tolerance of a/√2,
outward normals parallel above a cosine threshold (both atoms on the same
facet), and r̂_ij perpendicular to the normal (the pair lies in the surface).
On a perfect (100) terrace this graph is a square grid with exactly two
perfect matchings, the phases the current tables call A and B. At steps,
corners and domain boundaries the grid is irregular and the phase question
answers itself.

**Matching.** Take a maximum-cardinality matching of the candidate graph. The
graph is bipartite on a single terrace but not in general, so use a general
matching algorithm or, in the first implementation, greedy augmentation from
a seed per connected component with the checkerboard propagated along the
component. The seed is the only global freedom, and the existing
`invert_phase` pin becomes its tie-break. Rows are seeded to run along the
component's longer extent; both orientations are stable, and the choice is
open question 5.

**Applying a dimer.** Unchanged from today: both atoms slide symmetrically to
the dimer bond length and drop along the outward normal by the amount that
keeps the two back-bonds at bulk length. One geometry per lattice regardless
of passivation. The known limitation stays: the second layer is not relaxed
upward, so the first layer ends slightly low.

**Rebond.** Two unmatched dangling bonds that point at each other, with their
hosts closer than a threshold, become a host-to-host bond. This is the
concave corner case the current post-passivation repair handles, and also the
rebonded S_B step; the existing criteria (fraction of the van der Waals sum,
host separation, facing cosine) are kept. Because this now runs before
passivation and the classification is recomputed after every move, the
ordering constraint that forced the old repair to run after passivation
disappears.

**Unmatched `n_db = 2` atoms.** What remains after matching and rebonding is
governed by the `dihydride` option, whose default depends on the element:

- `keep`, the default on silicon: a dihydride is physical (the canted 1×1
  phase). Keep it, provided the clash check of section 5.6 passes against its
  neighbours.
- `remove`, the default on diamond: a dihydride next to another dihydride or
  a dimer is sterically forbidden. Remove the atom and recompute; the removal
  may free a partner for its neighbour. Iterate to a fixpoint; the loop is
  bounded because every iteration removes at least one atom.

Either value may be forced on either element. `remove` on silicon answers
"how much volume does a dihydride-free (100) surface cost", and `keep` on
diamond shows, tagged and counted, which sites the steric rule would have
removed.

**Steps and edges.** The matching produces S_A steps (rows parallel to the
edge) and S_B steps (rows perpendicular) from geometry alone. The rebonded
S_B configuration, where the step-edge dimer bonds down to the lower terrace,
is a rebond move and should emerge from the rule above when the geometry is
right.

**Out of scope for this pass**, and handled by hand-authored surface patches
if needed: any reconstruction that changes the number of atoms in a surface
unit cell (adatom or dimer-adatom-stacking-fault structures on clean (111)),
and any non-diamond-cubic motif.

### 5.6 Passivation and the clash check

Placement of terminators, bond lengths, halogen handling and the 24° dimer
tilt are reused as they are. What changes is the input: passivation places
one terminator per entry in the `db_dirs` list as it stands after
reconstruction, instead of re-deriving dangling-ness from the motif. A host
that gained a dimer or rebond bond has that direction removed from its list
and never receives an extra terminator. This is the invariant the old code
enforced through pass ordering and a per-atom flag.

**Clash check.** After placement, any two terminators closer than the clash
fraction of their van der Waals sum are found. What happens next is the
`clash` option: `report` only tags and counts them; `cant`, the default on
silicon, resolves a clash between two dihydrides by canting both; `remove`
peels one host and recomputes. On diamond a clash that survives the
reconstruction pass is an error in that pass and is reported as such
regardless of the option. Together with complete passivation, this check is
the one part of the pipeline that is load-bearing for stability, which is
why `report` never suppresses the tag, only the repair.

### 5.7 Tip-access check

For every surface site in the classification, test whether some tool in the
active operation library can reach it: take each tool's approach envelope
(the solid the library already stores for the trajectory sweep), place it on
the site's outward normal, and test for overlap with the placed atoms. A site
no tool can reach is tagged `unreachable` and counted in the report, with the
nearest obstructing atoms recorded so the user can see which step or terrace
is in the way.

This is a check, not a move: it changes nothing and reports. The healer
already removes the worst access cases, one-atom pits and one-atom whiskers,
by accident, but narrow terraces and inner step corners survive it. Whether
to turn the check into a move (peel the obstruction, or widen the terrace) is
decided once the check has run on realistic shapes and we know how often it
fires. It is off by default until it lands, because it needs an operation
library on the node and costs one envelope test per surface site per tool.
Of all the constraints in this document, tip access is the one that actually
limits builds of free shapes on silicon.

### 5.8 Report and tags

The fill tags atoms so the user can see what happened: `healed:filled`,
`dimer`, `dihydride`, `rebond`, `step`, `clash`, `unreachable`. Tags flow
through the existing atom tag system and are visible in the viewport and the
text format.

`materialize` gains a second output pin, a `FillReport` record: counts of
peeled and filled atoms, dimers, rebonds, dihydrides, forbidden-site
removals, clashes found and repaired, and unreachable sites; the number of
terminators, which is the build-cost proxy; a histogram of site types, which
is the buildability proxy; the maximum outward displacement of the realised
surface from the target volume, which is the fidelity cost of healing; and
the bounding boxes of any region where the fill could not reach a clash-free
state. A node network can assert on it, and two runs of the same shape with
a pass toggled differ in it by exactly what that pass did.

### 5.9 Node interface

`materialize` keeps its name and its Blueprint input; a mesh Blueprint is
just a Blueprint whose alignment is lattice-unaligned. New and changed pins:

| Pin | Type | Default | Notes |
|---|---|---|---|
| `heal` | Int (enum) | `peel_fill` | `none`, `peel`, `fill`, `peel_fill` |
| `fill_band` | Float | one bond length | `δ_fill`; bounds the fidelity cost |
| `surf_recon` | Bool | true | the matching pass |
| `rebond` | Bool | true | existing pin; the rebond move |
| `dihydride` | Int (enum) | by element | `keep`, `remove`; Si → `keep`, C → `remove` |
| `invert_phase` | Bool | false | seed tie-break for the matching |
| `clash` | Int (enum) | by element | `report`, `cant`, `remove`; Si → `cant`, C → `report` |
| `access_check` | Bool | false | the tip-access check |
| `rm_unbonded`, `rm_single` | Bool | — | kept for old files; `heal` supersedes them |
| `report` (output) | Record | — | the fill report |

Passivation itself (`passivate`, `passiv_elem`) keeps its current pins. All
of the above are region-overridable, so a single part can carry different
settings on different faces. Text-format names follow the pin names. Old
`.cnnd` files load with `heal` derived from `rm_unbonded` and `rm_single`,
and with every new option at its default.

**The baseline configuration** for any comparison is `heal: none`,
`surf_recon: false`, `clash: report`, `access_check: false`. It is today's
geometric cut plus passivation, and every other configuration is measured
against it through the report.

### 5.10 Performance and the old path

Every pass touches only atoms in the surface band, and every neighbour lookup
goes through the tracker. The target is that a one-million-atom part fills
in the same order of time as today. The current table-driven {100} algorithm
is kept behind the existing `surf_recon` path until the new pass reproduces
its results on the regression fixtures (the 2391-atom cuboid, the Si(100)
step-edge fixture, the region tests); then it is removed. The mesh
primitive's winding-number evaluation is the one new cost that scales with
input complexity, and it is bounded by the BVH.

## 6. The site catalogue

Because the built part is whatever the model says, the research behind this
design is not "what does a silicon surface do" but "which terminated sites
exist". Three kinds of question, of very different value:

| Question | Needed? |
|---|---|
| Does this terminated site exist as a stable minimum, and what is its geometry: dihydride canting, S_A / S_B / rebonded step geometries, the 3×1 phase? | Yes. This is what the operation library builds against and what a simulation starts from. |
| Below what separation do two terminators make a site chemically impossible? | Yes. The one stability-critical number; on diamond it decides existence. |
| Which phase is the ground state, surface energies, equilibrium shapes, annealing behaviour? | No. That is the physics of annealed surfaces. Background only. |

**Catalogue.** Enumerate the local environments the fill produces on
realistic shapes, and for each record whether it exists as a stable minimum,
its relaxed geometry, and the steric limit between its terminators. The
catalogue lives in a data file with provenance, like the mechanosynthesis
operation libraries, not in code. The fill's geometry constants (dimer
lengths, dihydride canting, second-layer displacement) are read from it, so
that improving a number does not mean editing a pass.

**How the entries are made.** For each environment build a small terminated
cluster or slab, relax it with a machine-learned potential such as UMA, and
confirm a subset with DFT. An environment that rearranges during relaxation
does not exist and is removed from what the fill may produce; one that stays
gives its geometry. The literature validates the potential on the known cases
and supplies the geometries that have been measured. The initial set is the
(100) 2×1 monohydride, the (100) dihydride on silicon, the (111) and (110)
monohydrides, the four (100) step types, the (100)/(111) concave corner with
and without rebond, and the same for diamond.

**Verification relax.** A final relaxation of the surface shell of a filled
part with a reactive model is a check on the fill's output, not a
replacement for it: the unreconstructed 1×1 surface is a saddle point that
gradient descent does not leave on its own, so the dimers must be placed
before relaxation. The check is that the relaxed shell stays close to the
model; a large move is a site the catalogue got wrong.

**Not planned: energy-driven healing.** Replacing the dangling-bond count
with summed site energies and the greedy descent with annealing was
considered. It would rank stable alternatives by energy, which is not a goal;
the ranking this design cares about is operation count and site-type
coverage, and the dangling-bond count is a direct proxy for both. If a
ranking by energy is ever wanted, it is a change of scorer with the same
moves.

## 7. Deferred: aligning one face of a mesh to the lattice

This is the third goal, controlled surface crystallography (section 2.3). It
is not part of the phasing below, and is recorded here so that the fill
design does not have to be reopened when it is picked up.

A part for a mechanical design usually has one or two faces that matter: the
face that slides, meshes or mates. Those faces need to be exact lattice
planes so that the fill produces a flat terrace rather than a staircase, and
the relative orientation of two such faces on two parts decides whether the
contact is commensurate and locks, or is incommensurate and slides. Two parts
modelled in place and materialised in one scene share one lattice
orientation, so any two flat terraces that touch are in registry. The fill
does not address this. An `align_face` node does, in the simplest form: the
lattice stays fixed and the mesh is rigidly rotated so that one picked face
becomes a chosen lattice plane, optionally translated so that the face sits
in a chosen layer gap.

Matching the face normal to (hkl) fixes two rotational degrees of freedom.
The spin about the normal defaults to the minimal rotation and can be
overridden with one field. Position along the normal defaults to unchanged
and can be snapped to a layer gap of section 3.4.

| Pin | Type | Default | Meaning |
|---|---|---|---|
| `shape` | Blueprint | required | the imported mesh, or any free geometry |
| `point` | Vec3 | picked | a point on the chosen face, Å |
| `normal` | Vec3 | picked | the face's outward normal |
| `miller` | IVec3 | (1,0,0) | the lattice plane the face becomes |
| `spin` | Float | 0.0 | extra rotation about the normal, degrees |
| `snap` | Bool | false | translate along the normal so the face sits in a layer gap |
| `gap` | Int | 0 | which gap of the layer table; 0 = the monohydride gap nearest the face |

The face is identified by a point and a normal, not by a triangle index, so
re-exporting the mesh from Blender keeps the constraint. In the text format:

```
part = align_face { shape: mesh, point: (12.0, 3.5, 40.2), normal: (0.31, 0.95, 0.0), miller: (1, 1, 1), snap: true }
```

The UI needs a pick-face mode in the viewport (hovering highlights the planar
region under the cursor, found by grouping connected triangles whose normals
agree; clicking writes `point` and `normal`), a hint next to the Miller index
field listing the low-index planes nearest the face's current orientation
with their angles, and a translucent overlay of the target plane so the
effect of `spin` and `snap` is visible immediately. A second face is not a
separate constraint: the user reads its angle off the hint list after
aligning the first and adjusts `spin` until that angle is small. That is
enough for a bearing. Two-face consistency checking, least-squares fitting
of several faces, and a commensurability indicator for a pair of faces all
build on this node without changing it.

## 8. Phasing

| Phase | Content | Size | Depends on |
|---|---|---|---|
| 0 | Layer-spacing shift quantum and float offset on the lattice-constrained nodes; a program that prints the layer table for a structure and an (hkl) | small | — |
| 1 | `Mesh` primitive and `import_mesh` node | medium | — |
| 2 | Healer with the `heal` switch, candidate band, report pin, tags | medium | — |
| 2b | Tip-access check | medium | 2, an operation library on the node |
| 3 | Classification, matching reconstruction with `dihydride`, rebond, passivation from the classification with `clash` | large | 2, catalogue entries for the sites it places |
| 4 | Regression parity with the old path, removal of the old path, performance, reference-guide update | medium | 3 |
| 5 | Free node family | medium, incremental | — |
| 6 | Site catalogue as a data file, fill constants read from it, verification relax | ongoing | 3 |

Phases 0, 1, 2 and 5 are independent and can proceed in any order. Phase 2 is
the first one that produces measurements: once the report exists, every
question of the form "does this pass matter" is a with-and-without run.
Phase 2b is placed before 3 because tip access is the constraint that limits
free-shape builds on silicon, and because its census tells Phase 3 which step
types matter. The catalogue starts before Phase 3 and never finishes; Phase 3
needs entries only for the sites it places. The reference guide pages for
`materialize`, the geometry nodes and the import node are updated in the
phase that changes them.

## 9. Measurements and research still to do

Measurements, each a small program on the `crystolecule` crate or a run of
the fill with the report pin:

- The layer table: for a Structure and an (hkl), project all motif sites onto
  the normal modulo the conventional d-spacing and print the distinct layer
  offsets and gaps. Its output is the snapping table for Phase 0 and a
  regression test for the (111) bilayer.
- Run the healer on random free shapes (spheres, cylinders, a torus, an
  imported blob) at several sizes and histogram the surface site types before
  and after. This says which site types the reconstruction pass must handle
  first, and how much volume the healer moves.
- Build the four canonical Si(100) step fixtures (S_A, S_B non-rebonded, S_B
  rebonded, D_B) as cut geometries, run the reconstruction pass, and compare
  to Chadi's structures atom by atom.
- Run the tip-access check on the same shapes and fixtures and report the
  fraction of unreachable sites by site type. This is the number that decides
  how much of tip access the healer already delivers by accident.
- Time the healer and the matching on the T-centre nanobeam (1.07 M atoms)
  and on a 100 k-atom imported mesh.
- Verify `truck-stepio` entity coverage against STEP files exported from
  Blender, FreeCAD and Fusion 360.

Research, for the site catalogue:

- For each diamond-cubic-specific item in section 3.5, whether the
  terminated site exists, its geometry, and the source.
- The exact steric rule on C(100): is an isolated dihydride between two
  monohydride dimer rows a stable site, or must every C(100) surface carbon
  be dimerised? The 3×1 phase on silicon and its absence on carbon is the
  starting point.
- The canting geometry of the Si(100) 1×1 dihydride phase, which the `cant`
  clash repair needs.
- Relax a dimerised, terminated slab and record the second-layer
  displacement, to decide whether to add it as a derived constant.

## 10. Open questions

1. Should the free nodes and the lattice-constrained nodes share one node
   type with a mode, or stay separate types as `free_sphere` does today?
   Separate types are the current precedent and keep the text format
   unambiguous.
2. Does the healer's fill move need to respect the sublattice on
   zincblende-type motifs with two elements, so that it never creates a
   like-to-like bond? On homonuclear diamond it cannot matter; on a compound
   it should be automatic because the candidate carries its site index.
3. Is `δ_fill` better expressed in bond lengths or in layer spacings along
   the local normal?
4. How should the report expose "could not reach a clash-free state": as a
   warning on the node, as tags, or both?
5. The dimer-row tie-break at steps. Rows along the longer extent give S_A
   steps on one edge and S_B on the other; both exist. The choice should be
   the one whose step sites the operation library covers, which the
   tip-access census will show.
6. Whether the tip-access check should become a move, and if so which one:
   peel the obstruction, or leave a terrace one atom wider.
7. Whether a face that must mate with another part needs a first-class
   fidelity marker rather than a region override with `heal: none`. This
   belongs with `align_face`.
8. Whether the (111) monohydride surface needs any geometric relaxation in
   the fill. The H-terminated (111) is essentially bulk-terminated; the
   current code applies none and that is probably right.

## 11. Decisions

- The design volume is continuous and unconstrained; the lattice and the
  chemistry enter only in `materialize`. Lattice-constrained nodes remain as
  a convenience, not as the model.
- Mesh import comes before the free node family. Both feed the same fill.
  STEP is handled by tessellation to the mesh primitive, in pure Rust, later.
- Healing and classification are lattice-generic and expressed in the
  motif's coordination. Reconstruction is diamond-cubic carbon and silicon
  only, gated with a visible warning.
- The dimer phase is a matching tie-break, not a global table. Rebonding is a
  move of the reconstruction pass and runs before passivation; passivation
  reads the post-reconstruction dangling-bond list. One dimer geometry per
  lattice regardless of passivation.
- Stability is the floor, not the goal. On both materials only complete
  passivation and the terminator clash check are load-bearing for stability;
  on diamond the clash check forces the reconstruction, on silicon it does
  not. Every other pass serves buildability, build cost, tip access or
  simulation readiness, and says so where it is described.
- The built part is whatever the model says. The reconstruction pass chooses
  what is built, constrained by the site catalogue; it does not predict.
  Research is scoped to the existence, geometry and steric limits of
  terminated sites; surface energies and equilibrium shapes are background.
- Every pass is individually switchable, with a baseline that reproduces
  today's geometric cut plus passivation exactly. Questions about whether a
  pass matters are settled by the report, not by argument. - Tip access is a
  reported check, not a move, until its census shows what a move should do. -
  Energy-driven healing is not planned. The site catalogue is a data file the
  fill reads its constants from. - Functional faces (bearing faces, registry
  between parts) are not produced by the fill. They are made by aligning the
  imported mesh one face at a time; the lattice stays fixed and the mesh
  rotates.