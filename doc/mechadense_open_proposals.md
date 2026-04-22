# mechadense's Open Proposals from Discussion #294

This document distills the feature requests mechadense made in the
[Lattice Space Refactoring discussion](https://github.com/atomCAD/atomCAD/discussions/294)
(and in the attached `AtomCAD_ArchitectureTypes_2026-04-10_15-20-CET.drawio` file,
Panes A–G) that are **not yet implemented** in the refactoring we just finished,
or that we implemented differently from what he proposed.

Scope: only the proposals I consider most important. For each one I state:

- **What he wants** — my best reconstruction.
- **Understanding** — whether I believe I understood it fully.
- **My take** — whether I agree, disagree, or am on the fence, and why.

The ranking is rough: higher-numbered items are still worth discussing but less
central.

---

## 1. Three levels of "dissonance" for Blueprint geometry

**What he wants.** Currently a Blueprint's geometry is either "aligned with the
structure" or not — a single binary bit (and today we don't even track that).
mechadense wants **three** levels, visualized as warning colors on the geometry
or on pins/wires:

| Level | Meaning | Proposed color |
|---|---|---|
| Black | Geometry fully aligned with the structure's space group (motif-preserving) | — |
| Brown | Geometry matches the lattice but breaks the space group (a.k.a. "point-group-breaking" rotations/subdivisions) | Brown |
| Orange | Geometry is misaligned with the lattice itself | Orange |

The motivating use-case: when you `set_structure` or do a boolean op that
silently introduces a space-group mismatch, the user should see *that something
changed* even though geometry looks the same visually. Pane A explicitly marks
this as "three stages of dissonance for blueprint geometry".

The design doc lists a related open question ("Mismatch levels within
structure space — three dissonance levels exist; how should they be visually
communicated?") but no implementation was attempted.

**Understanding.** I think I understood this fully, modulo one fuzzy bit:
detecting "structure matches lattice but not space group" requires us to
compute the symmetry group induced by the current motif, not just the lattice
vectors. We don't have that machinery yet — currently `Structure` carries
`lattice_vecs + motif + motif_offset` without any explicit space group.
Classifying a motion into black/brown/orange needs both a lattice test and a
space-group test, and the latter is non-trivial for custom motifs.

**My take.** **I agree this is valuable**, but it's a substantial feature, not
a cleanup. Orange (off-lattice) is cheap; brown (off-space-group) is expensive
and needs new code in `crystolecule`. Doing orange first and deferring brown
would capture most of the day-to-day value. We should *not* block the rest of
the refactoring on this; it belongs as its own design doc.

### Discussion results

**Scope: orange only.** We agreed to implement the orange / not-orange (lattice
mismatch) distinction and drop brown from v1.

**How orange is computed (numerical accumulated-transform test).**
Every geometry-carrying output carries a `LatticeFrame = (Structure,
offset: Vec3, rotation: Mat3)` alongside its geometry. Primitives start at
`offset = 0`, `rotation = I`. Each movement node composes its parameters into
the frame. Each boolean / combining op takes the frame of its inputs if they
match, else demotes to "unknown". Alignment is then a numerical test on the
accumulated frame:

- **Translation aligned** iff `M⁻¹·offset` has all-integer components (within
  `ε ≈ 1e-6·‖M‖`), where `M` is the structure's lattice matrix.
- **Rotation aligned** iff `M⁻¹·rotation·M` has integer entries within the
  same tolerance.

If both pass → aligned (black). Else → orange. This handles `free_move` that
happens to land on a lattice vector correctly (it is recognized as aligned),
which the pure static-flag approach would over-report as misaligned.

**Why brown was dropped even hypothetically.** Even if we had the motif's
point group `P` available, brown detection on Blueprint outputs is
ill-defined. The problem is that most in-lattice primitives are already
intrinsically brown: a generic cuboid has only D₂h symmetry, a half-space
with normal [hkl] has only C∞ᵥ along that axis, etc. Being "inside the
space group" requires the geometry to be a fixed point of `P`, which only
spheres, appropriately-sized cubes, and carefully composed symmetric
arrangements satisfy. So the brown indicator would be "on" essentially all
the time, carrying almost no signal. The originally-hoped-for use case
(notice when a `set_structure` silently broke the space group) doesn't
fire, because you typically go from brown to brown, not black to brown.

**Where alignment applies.** Only to phases that carry geometry relative to
a lattice:

- **Blueprint** — always.
- **Crystal** — only when the optional geometry shell is present.
- **Molecule** — never (no structure, nothing to be aligned to).
- **Structure, LatticeVecs, primitives (Float/Vec3/...)** — never.

**When alignment is known.** At **evaluation time only**, not validation
time. A pin's declared type (Blueprint, Crystal, ...) is a compile-time
property of the node registration and can color the pin before anything
runs. Alignment depends on *parameter values*: `free_move((1.0,0,0))` and
`free_move((1.5,0,0))` have identical pin types but opposite alignments. So
alignment lives as a new field on `BlueprintData` / `CrystalData`, populated
by the evaluator, stale when upstream parameters change, absent when a node
errors.

**Where to display it.** Pin color and wire color are already consumed by
the data-type visual channel, so alignment needs a separate channel.
Preferred combination:

1. **Node badge next to the output pin** — a small orange dot when
   misaligned, nothing when aligned. Scannable at graph-level zoom, doesn't
   touch pin rendering.
2. **3D viewport tint on displayed geometry** — misaligned Blueprints /
   Crystals render with an orange outline or wash in the 3D preview. Most
   semantically honest (alignment is a property of the *shape*, not of the
   pin) and directly visible where the user is looking when it matters. Only
   fires for displayed outputs, which is fine.

A third `Unknown` state (e.g. boolean op on two Blueprints with inconsistent
frames) renders as a neutral grey cue, so users notice when their graph is
producing ill-defined alignment rather than silently collapsing to orange.

---

## 2. Visual language for phases: port shapes + line styles

**What he wants.** Extend the dissonance-color idea into a richer visual
language on the node graph:

- Distinguish the three phases by **port shape**:
  Blueprint → circle, Crystal → square, Molecule → triangle
  (his comment 12 writes "Atomic: circle; StructureBound: square; Untethered:
  triangle" — I read that as him mixing up the abstract/concrete mapping;
  Panes A/F use the shapes for the *concrete* phases).
- Distinguish them by **line style**: solid / dashed / dotted for the
  Blueprint/Crystal/Molecule wires.
- Stack this with dissonance colors (item 1) so a wire carries two signals at
  once: what phase the value is in, and how aligned its geometry is.

**Understanding.** Fully understood. This is a pure-UI proposal.

**My take.** **Mostly agree, with a caveat.** Distinct shapes on output pins
would genuinely help users disambiguate phase at a glance — right now we only
have the node color, which is easy to miss on long wires. But we should be
careful about combining shapes **and** line styles **and** dissonance colors
on the same wire: three overlapping visual dimensions get noisy fast. I'd
start with **one** distinguishing feature (port shape is my preference because
it's attached to the node, not the wire) and add more only if users get
confused.

### Discussion:

Until now pin and wire color represented data type. It is probably not a good idea to
mix this up in special cases.

---

## 3. `structure_reduce` — automatic common-sublattice finding

**What he wants.** A node that takes two `Structure` inputs and returns the
**largest structure whose lattice is a common sublattice of both**. Rather
than the user awkwardly fighting `set_structure` to reconcile two different
source structures, `structure_reduce` would derive a reconciled "AB structure"
whose lattice-vectors work for both inputs.

Pane B frames it as *the* right way to combine blueprints built in different
structures (diamond+lonsdaleite, diamond+silicon, etc.). He notes the
reduction is not unique in general, so tooling to help pick would be
useful (Pane B: *"lattice might not be unique though => eventual aiding
tools"*).

In our design doc this is delegated to the user: if two blueprints have
incompatible lattices, the user runs `set_structure` on one to force them to
match. mechadense thinks that's the wrong answer.

**Understanding.** I understand the intent but I **do not fully understand the
math**. Finding a common sublattice is a lattice-theory problem (Hermite / HNF
on the union of generators) and the "not unique" part is exactly the problem
that multiple orientations give different common sublattices — picking one
needs either a heuristic or user guidance. I also don't know how mechadense
wants to handle the motif: two different motifs in a shared sublattice
typically cannot both be preserved; one motif or both motifs might need to be
"expanded" into the larger unit cell, or the motif becomes the union of both.

**My take.** **On the fence, leaning against for v1.** The concept is right —
users really will want to combine diamond and silicon, and `set_structure`
isn't a satisfying answer. But `structure_reduce` is a *research feature*, not
an afternoon's work. Before committing to it we should (a) see how painful
manual lattice reconciliation actually is in practice, (b) collect real user
examples, and (c) decide whether the answer is this node or a handful of
convenience presets like `diamond_lonsdaleite_commensurate`. I'd file it as a
future design task.

### Discussion

How to derive the motif of the output structure of structure_reduce?

---

## 4. Boolean ops belong in the Crystal phase, not the Blueprint phase

**What he wants.** Our design puts `union`/`intersect`/`diff` on `Blueprint`.
mechadense disagrees: Pane B calls Blueprint-phase boolean ops *"generally
problematic"* (in red) and says *"Better: matching/combining lattices in
Crystal phase"* (in green). His reasoning:

- A Blueprint has no atoms to look at, so the user cannot visually verify
  that boundary conditions are met at the seam of a boolean cut.
- When combining two structures (per item 3), a Crystal carries an
  already-materialized motif so there's something concrete to merge; a
  Blueprint is just two overlapping cookie-cutters.

Practically this means operations like `atom_union` / `atom_cut` (working on
atoms, i.e. in Crystal phase) should remain first-class rather than being
replaced by Blueprint booleans followed by `materialize`.

**Understanding.** Understood. This is an architectural direction disagreement,
not a misreading.

**My take.** **Partially disagree.** I think Blueprint boolean ops are still
valuable and that's what we should keep as the default path — they're
parametric (you can tweak the primitives and everything rebuilds), they
compose cleanly with `set_structure` / `materialize`, and the "can't preview
atoms" concern is answered by just materializing to see. But mechadense's
deeper point — that **`atom_union` on Crystals + a free-form `atom_cut` for
when structures genuinely differ** is still needed — I think is right. Our
design already keeps `atom_union` polymorphic over `Atomic`; the gap is a
corresponding `atom_diff` / free-form crystal cutter, which we don't have.
Worth discussing separately (see item 8).

### Discussion

Boolean operations on Blueprints are obviously needed to create any nontrivial geoemtry.

---

## 5. Fractional-coordinate alternative for `set_structure`

**What he wants.** Two modes for `set_structure`:

- **Breaking mode (our current behavior):** geometry stays in real-space
  coordinates; the structure swaps underneath. Visual result: the shape
  doesn't move but is now filled with a different material. Pane B marks
  this red and calls it *"in a different lattice then this seems like a BAD
  HACK"*.
- **Fractional mode (what he wants as default):** geometry is interpreted in
  *fractional* (lattice) coordinates, so swapping the structure rescales
  geometry to match. Pane B marks this green: *"fractional coordinate space
  geometry then this seems good / natural / ok"*.

The use case: if I design a cuboid in diamond and swap to silicon, fractional
mode gives me a naturally-sized silicon cuboid with the same number of unit
cells; our current (real-space) mode gives me a silicon cuboid of exactly the
same physical dimensions but containing a different atom count. Both are
useful; he thinks fractional is the **natural default** and real-space is the
exception.

**Understanding.** Fully understood. Straightforward.

**My take.** **I think the concept is good, but I'm unsure about the
default.** Real-space preservation is what every CAD user expects from
"change the material" and matches how `set_structure` reads aloud — *"keep the
shape, change the structure"*. Fractional-space feels more like *"keep the
pattern, rescale the shape"* and deserves its own node name
(`set_structure_fractional`, or a boolean flag on `set_structure`). Very worth
adding; I'd push back on making it the default.

### Discussion

The second type of set_structure can simply be achieved by rewiring the input structure
into the blueprint. I cannot see a reason why wire it in after the geoemtry is built.
The first (HACK) part can only be done after the fact. If we need this node we need it for the first use case:
transfering a geometry from one structure to the other. For the other use case only build a node if it is demonstrated
that it is needed as it complicates the code base singnificantly. 

---

## 6. Richer symmetry operations beyond translate/rotate

**What he wants.** Pane F calls out that our movement set is missing:
**scale, reflect, rotoreflect, invert, glide, screw**. Some of these are
space-group-preserving on structured objects (glide, screw, reflect for
certain axes); others are useful on `HasFreeLinOps` objects (free scale, free
reflect).

He suggests they be added polymorphically over `HasStructure` / `HasFreeLinOps`
in the same shape as `structure_move` / `free_move`.

**Understanding.** Understood.

**My take.** **Agree in principle, prioritize by demand.** These are useful,
but most users won't reach for "glide" before they've exhausted translate /
rotate. I'd add them piecemeal as people ask, with the caveat that reflect
and invert have to distinguish "space-group-compatible reflect" (e.g. along
a mirror plane of the lattice) from "arbitrary reflect that breaks the
structure" — which loops back into item 1's dissonance machinery. Scale is
only well-defined in fractional mode (item 5) or as a free-space op.

---

## 7. Crystal-as-motif repackaging (instead of / alongside `enter_structure`)

**What he wants.** Our design has `enter_structure :: (Molecule, Structure) →
Crystal` for repackaging. In his Pane D, he argues this is unnatural: a
Molecule has no lattice information, so `enter_structure` has to guess how the
molecule tiles. His alternative: take a **Crystal** (which already has a
lattice + atoms) and use it as the **motif input** to a generalized
`structure` constructor, which then tiles the crystal as a repeating motif.
The crystal's atoms become the motif; its lattice vectors may or may not be
the new structure's lattice vectors.

He also draws a Pane-D middle row for "commensurate superstructure
repackaging" — i.e. repackaging where the new lattice is a supercell of the
old lattice and the motif is the old crystal copied into it.

**Understanding.** Partially understood. I see the shape but not the exact
input/output contract: does the Crystal-as-motif node produce a Structure
(lattice+motif) or directly a Blueprint/Crystal? And how does it handle the
Crystal's own geometry (drop it, preserve it as a clipping region, ...)? The
drawio is hand-drawn enough that I could misread.

**My take.** **On the fence.** This is a genuinely useful workflow —
defining a "unit" crystal once and tiling it into a larger pattern is a thing
users want. But I'm not convinced `enter_structure` is the wrong primitive;
`enter_structure` is about *asserting* a structure onto a free object, while
Crystal-as-motif is about *constructing* a new structure from a unit. They
solve different problems and I'd keep both. Worth a separate design doc if
we commit to adding this.

### Discussion

Creating a motif is not trivial. We have to define the extent of the supercell and the bonds between neighbouring supercells.
Why not simply create supercells in a node which has a structure as an input and diffs as defects to stamps
into it?

---

## 8. Free-form cutter nodes for Molecules

**What he wants.** Pane E notes (in red) that *"freefrom cutting molecules
may well still be useful; Blueprint cutter needing a Lattice/Structure is
pointless here"*. In our design, all boolean ops require a Blueprint, which
requires a structure. Cutting a free molecule with a sphere-shaped region
currently has no natural path.

He wants primitives that operate on Molecules directly: a sphere cutter, a
half-space cutter, etc., that produce a region and apply `diff` / `intersect`
to a Molecule's atoms in real-space (no structure needed).

**Understanding.** Fully understood.

**My take.** **Agree, modest-sized feature.** A "geometric region" that is
structure-free would cleanly answer both this and a related issue for
free-space design. Could be done by introducing a new `Region` type
(geometry-without-structure), with Blueprint = Region + Structure. That's a
type-system change though, so it's not trivial. Alternatively, cheaper: add
`atom_cut_region(Molecule, Region) → Molecule` as a one-off. I'd lean toward
the one-off unless we see a family of region-based operations coming.

---

## 9. Pane G: edge / corner / guide extraction

**What he wants.** Pane G is left as a placeholder — *"nodes still to come
particularly for extracting edges and corners that can be used as guiding
constraints"*. He imagines nodes that take a Blueprint (or Crystal) and
extract geometric features — facet edges, vertex points, crystallographic
directions — that can then be plugged into other nodes as alignment
constraints (e.g. align two blueprints along a shared edge).

**Understanding.** Understood as a direction, not as a concrete feature.

**My take.** **Agree this is a real need, but deferred.** Alignment
constraints are a large separate feature and would need its own design doc;
the refactoring we just finished doesn't block it and isn't blocked by it.

---

## Smaller items worth noting

These came up in the discussion but I don't think warrant full sections:

- **"Phase is a bad pick" (his comment 10).** He doesn't like the word
  "phase" because of thermodynamic-phase overloading in crystallography. He
  didn't suggest a replacement. I'm content to keep "phase"; the context
  disambiguates it. Open to bikeshed if he has a proposal.
- **Preset structures (`diamond`, `lonsdaleite`, `silicon`, ...).** The main
  design doc defers these; they're genuinely trivial and we should just add
  them when the refactoring lands.
- **Amber warning on off-structure Blueprint moves.** Also deferred in the
  main design doc. Related to item 1 (dissonance).
- **"Recalculate symops" nodes (Panes C, D).** He wants a node that
  recomputes a `Structure`'s derived symmetry after motif edits. We don't
  store derived symmetries at all today, so this depends on a lot of other
  groundwork.
- **Crystal phase "extensively used" (comment 14).** Pane D emphasizes
  Crystal as *the* working phase rather than just an intermediate —
  consistent with our design, but he wants a richer set of Crystal-specific
  operations (see item 4).

---

## Recommended next steps

Not a commitment — just a proposed order if we decide to act:

1. **Cheap wins first:** preset structures (`diamond`, `lonsdaleite`, ...),
   orange-only dissonance (lattice mismatch) + amber warning on Blueprint
   free-moves. Both are listed as deferred in the current design doc and
   each is a few hours of work.
2. **Medium:** fractional-coordinate variant of `set_structure` (item 5),
   a free-form cutter for Molecules (item 8).
3. **Design-required:** three-level dissonance with space-group detection
   (item 1), additional symmetry ops (item 6), Crystal-as-motif
   repackaging (item 7).
4. **Research / long-term:** `structure_reduce` (item 3), edge/corner
   extraction (item 9), Blueprint-vs-Crystal phase for boolean ops (item 4,
   mostly a direction we re-evaluate over time).
