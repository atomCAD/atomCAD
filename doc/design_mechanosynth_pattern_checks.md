# Design: pattern checks in the mechanosynthesis engine — bonds, degree, anchors, steric clashes

Status: **drafted and reviewed 2026-09-16**. Phase 0 (§8) and Phase 1 (§12)
implemented 2026-09-16; Phases 2–4 and the out-of-repo generator work are not.

Two decisions Phase 1 had to make that this document did not anticipate, both
recorded here rather than in the sections they touch:

- **`deg` belongs on a `before` atom only**, and is a load error on an `after`
  one: it is a statement about the workpiece a step *matches*, and on the
  `after` side it would describe a structure that does not exist yet and that
  the rewrite has already decided.
- **The anchor frame-atom rule (§3.4) exempts an operation that changes
  nothing.** Every atom of such an operation is a frame atom by the letter of
  `is_frame_atom`, and "a click that places the reaction somewhere else" has no
  meaning where there is no reaction to place. Likewise an operation whose
  `before` is **empty** — a pure addition — has no atom to click, so the
  `1..=before.atoms.len()` range is skipped and a stated `anchors` is refused.

One ordering choice worth recording: at replay the **participant** checks
(`StepOnTool`, `StepAcrossParticipants`, `ToolSideOffTool`) run *before* the
pattern checks, so `match_before` is split into a positional pass and
`verify_pattern` and the scene replay interposes between them. A tool side that
matched a base atom necessarily fails the bond check too, and "the tool side of
tool 0 matched the C of base" says *why* where "the bond between atoms 1 and 2
is missing" only says *what*. `check_pattern` is still the one predicate.

Extends `doc/design_mechanosynth_editor.md` (the placement engine and the offer
popup), `doc/design_mechanosynth_tools.md` (the `/2` formats, tool sides) and
`doc/design_mechanosynth_op_muting.md` (the offer sweep's admit seam). It
changes the operation-library format to **`atomcad-msops/3`**, adds three
checks to the engine that run at **placement and at replay alike**, adds one
placement rule that says which atoms of a pattern may be clicked, and adds one
sanity check on every structure the engine is handed. Everything it does not
mention is unchanged.

Requested by mechadense after first use of the editor:

> Checking for steric clashes that would occur when the pattern is actually
> added. Removal or orange listing at the end. — I spotted it suggesting odd
> diagonal applications of the precursor patterns some of which pointing down
> into the bulk.

and by the maintainer's follow-up, which sets the principle this document
implements: **bonds are first-class citizens of the engine, checked everywhere
it is possible to check them, replay included.** atomCAD's purpose is atomically
precise manufacturing; a workpiece whose bond model is wrong is not a model of
anything, and the engine should say so rather than work around it.

---

## 1. Motivation: what the editor offers today, measured

The claim that the offers are too broad was measured rather than taken on
trust. A scratch program built the v3 chlorinated Si(100) workpiece with the
same `materialize` settings the generator uses, loaded the v3 library, and
called the engine's own `applicable_ops` on three atoms. For every candidate it
then measured how close the atoms the step *adds or moves* land to atoms they
are *not bonded to* (§5 has the exact rule, and §5.3 says how the measurement
differed from it).

| clicked atom | ops offered | candidates | land on an existing atom (≤ 0.38 Å) | no overlap, but chemically impossible |
|---|---|---|---|---|
| Cl-terminated surface Si | 7 | 13 | 7 | `bridge` ×2 (the pair is already bonded), `si_pickup` (lifts a chlorinated atom), `precursor_chemisorb` ×2 (placed upside down, into the bulk) |
| subsurface Si, four Si neighbours | 6 | 53 | 41 | `dimerize` ×8 (a fifth bond on a bulk atom), `bridge` ×4 (already bonded) |
| bare dimer Si | 6 | 12 | 1 | `bridge` ×2 (already bonded), `precursor_chemisorb` ×2 (upside down) |

Four findings decide the design.

**Every donation variant fits a bulk atom exactly.** Three of a bulk atom's
four neighbours are a perfect tetrahedral frame, so `si_donate_site` fits with
residual 5e-7 Å and puts the new silicon on the fourth neighbour. The precursor
chemisorption alone has 32 candidates on a subsurface atom, every one with a
chlorine 0.38 Å from a silicon. This is mechadense's "pointing down into the
bulk".

**The bad offers that overlap nothing are the ones geometry cannot see.** A
`bridge` between two atoms that are already bonded, a `dimerize` of two bulk
atoms, a `si_pickup` from a chlorinated surface atom: the fit is exact and no
placed atom lands on anything. What is wrong is the bond count of the host, or
a bond the pattern assumes absent that is present.

**A planar `before` pattern is placed upside down.** `precursor_chemisorb`
names four coplanar surface silicons. A rectangle has an in-plane two-fold axis,
so a *proper* rotation exists that maps the four onto themselves with the
molecule below the surface. Its worst contact is a chlorine 1.84 Å from a bulk
silicon: 0.86 of the covalent-radius sum, which is *not* an overlap. Degree does
not catch it (the four hosts really are three-coordinate) and bonds do not
catch it (the two dimer bonds really exist). Only a steric threshold above 0.86
or a library rule against planar frames does.

**A click on the wrong kind of atom is placed anyway, in whatever role admits
it.** Clicking a terminator chlorine offers `bridge_c` as an exact fit, with the
chlorine playing the pattern's *second* atom — the `"*"` at 2.01 Å that stands
for the T-centre carbon — and the step would bond the chlorine to the silicon it
is already bonded to. The three near misses on the same click (`bridge`,
`dimerize`, `si_pickup`) all have the chlorine playing a `"*"` frame role of an
operation that has nothing to do with chlorine. This is the role rule's
smallest-eligible-id fallback, and §4.3 replaces it.

Root causes, all deliberate design of the current engine rather than bugs:
matching is nearest-atom-within-tolerance on **position and element**; bonds in
a pattern are read only to compute the rewrite and are never compared against
the workpiece; the host of every donation is `"*"`; a pattern cannot state how
many neighbours an atom has; and nothing looks at where a placed atom lands.

One side finding, fixed in Phase 0: `si_donate_dimer` shows **two identical
candidates** on a bare dimer atom. The proper and the mirrored fit produce the
same after state, but the collapse quantises positions at 1e-6 Å and the fit
residual of 3.6e-7 Å straddles a bucket.

---

## 2. The principle, and what "checked everywhere" means

The engine has two halves, and the same predicate must hold in both:

- **placement** (`place`, `applicable_ops`): given a click, which steps are
  offerable here;
- **replay** (`apply_step`, `apply_step_in_scene`, tool sides): given a step,
  apply it or fail naming the step.

Today the two disagree on almost nothing because both check almost nothing. This
design adds three predicates — **bonds**, **degree**, **clash** — and states as
an invariant that **a candidate is offerable if and only if the step it produces
replays without error on the same workpiece.** The test suite already pins that
invariant for the geometric match (`every_candidate_the_engine_offers_replays`);
each new predicate extends the same test.

"Everywhere it is possible" is, concretely:

| where | bonds | degree | clash |
|---|---|---|---|
| library load (`parse`) | endpoints, duplicates, self-bonds, unbonded close pair, valence plausibility | `deg` range, `deg` against listed bonds | planar-frame warning |
| placement, target side | prune in the assignment search, role rule | prune, role rule | per candidate |
| placement, tool side (`tool_readiness`) | same (its own match loop, §4.2) | same | same |
| replay, target side | error | error | error |
| replay, tool side | error | error | error |
| scene construction (base, feedstocks, tools) | bond-model sanity of every participant | — | pair check of every participant |

A fourth rule, **anchors** (§3.4, §4.3), is a placement-only rule: it says
which atoms of a pattern a *click* may play. Replay has no click, so it has
nothing to check there.

What this does **not** do: it does not teach the engine any reaction. The
three predicates are the generic chemistry the maintainer accepted — atoms do
not overlap, bond counts are what the library says, bonds the library names are
there — and nothing else. A library still says everything about *what* happens.

---

## 3. The `/3` library format

`LIBRARY_FORMAT` becomes `atomcad-msops/3`. A `/2` file is refused with the
usual "regenerate" message, for the usual reason: every library is
machine-written, and the bond semantics below change the meaning of a pattern
that lists no bonds. The build-script format is unchanged (`atomcad-msbuild/2`).

### 3.1 Bonds in a pattern are a complete statement

**Within a pattern, the bond list is closed-world.** For every pair of atoms
the `before` pattern names:

- a listed bond `[a, b, order]` means *the workpiece has a bond between the
  matched atoms, of that order*;
- an unlisted pair means *the workpiece has no bond between the matched atoms*.

Either way a mismatch is a failure, at placement (the candidate is not
produced) and at replay (`MechanosynthError::BondMismatch`, naming the step, the
operation, both pattern ids, the expected order and the found one).

Bonds to atoms **outside** the pattern are not constrained by the bond list.
That is what `deg` is for (§3.2).

This has three consequences worth stating because they are the point:

- **The rewrite is unchanged.** Comparing `before` to `after` by pattern id
  still decides what a step adds, deletes and re-orders. A bond only in `after`
  is added, and the closed-world rule guarantees it was absent; a bond only in
  `before` is deleted, and the rule guarantees it existed. The silent "delete
  an absent bond" no-op of the current engine cannot occur.
- **`bridge` on an already bonded pair is refused for free.** Its `before`
  lists no bond between ids 1 and 2, so a bonded pair fails the closed-world
  check. No separate "the step would change nothing" rule is needed; every
  such case is a bond mismatch.
- **Frame atoms keep working.** The frame-atom rule (`is_frame_atom`) looks
  for a bond *change* between `before` and `after`, not for the absence of
  bonds. A generator that lists the host-to-frame bonds in both halves — which
  it now must — leaves every frame atom a frame atom.

The generator must therefore write every bond that exists among the atoms it
names, in `before` and in `after`. For the v3 library that is the host's bonds
to its frame atoms in every donation, the dimer bonds in `precursor_chemisorb`
and `si_pickup`, and the tool apex's bonds to its legs in every tool side. The
loader helps: a pattern, `before` or `after`, with two atoms closer than
**1.1 × the sum of their covalent radii** and no bond between them is a
**load-time warning** naming the pair — either the bond is missing from the
file, or the library really means "these two are not bonded", in which case
the workpiece had better agree (and, in `after`, the pair had better clear the
`clash` factor of §5, or the operation is blocked on every host).

### 3.2 `deg`: the one SMARTS primitive worth adopting

The atom masks of force-field files are SMARTS atom primitives. OpenFF's
SMIRNOFF force fields assign every parameter by such a pattern with atom-map
indices, for example `[#6X4:1]-[#1:2]`; the vocabulary is degree (`D`), total
connections (`X`), hydrogen count (`H`), valence (`v`), ring membership (`R`,
`r`), aromaticity, charge, plus logical operators and recursive sub-patterns.
MDL query atoms carry the same idea as a "substitution count", including a value
that literally means *as drawn*. Reaction SMARTS in RDKit is the graph-based
twin of the msops before/after pair. Lattice kinetic Monte Carlo models get the
same effect by listing *vacant* sites in a pattern.

Only degree transfers. The libraries are positional: frame atoms already carry
the environment's geometry, and ring, aromaticity, charge and recursive matching
would need a subgraph matcher, which is a second engine. So a `before` atom may
carry one optional integer:

```json
{ "id": 1, "el": "*", "pos": [0, 0, 0], "deg": 3 }
```

*The matched workpiece atom has exactly `deg` bonds*, of any order, counting
each bond once. Absent means "don't care", so a library that does not trust its
workpiece's bonds — an xyz import without bond perception — can leave it out
and lose only this check. The generator writes it *as drawn*: the number of
bonds the atom had in the workpiece the pattern was derived from. For the v3
library that is 3 on every donation host, 1 on the abstracted chlorine, 2 on
the edge host, 1 on the ad-atom of `bridge`, and 3 on each of the precursor's
four silicons.

Checked at placement as a prune in the assignment search, and on the clicked
atom itself in the role rule — where a wrong degree does not silently drop the
operation the way a wrong element does, but keeps it as a dimmed row with the
reason when the geometry would otherwise have fit (§4.2, §7); at replay as
`MechanosynthError::DegreeMismatch` naming the step, the pattern id, the
expected and the found count. Load-time validation: `deg` must be ≥ the number
of bonds the pattern lists at that atom, because a pattern cannot list more
bonds than the atom has.

**Why an integer and not an "as drawn" flag.** An op-level flag would have to
hold for every atom, and it cannot: a frame atom below the host has four bonds
of which the pattern lists one. A per-atom boolean "all my bonds are listed"
would work for hosts and fail for the same frame atoms, and it says less than
the integer for the same price. The integer is also what every atom-typing
system settled on.

**Why `deg` still matters once bonds are closed-world.** The closed-world rule
speaks only about pairs *inside* the pattern. A bulk atom with three listed
neighbours and one unlisted one passes every bond check; `deg: 3` is what
rejects it. Conversely `deg` alone cannot say a listed bond has the right order
or that a specific pair is unbonded. The two are complementary and both cheap.

### 3.3 `clash`: a library may state its own steric factor

A library may carry an optional top-level `"clash": 0.9`, the blocking factor
of §5, in the same spirit as its `tolerance`: one value per library, stated by
whoever computed the patterns, with the engine's constant as the default. A
library that legitimately places atoms closer than the default allows says so
here, and the guide says when that is honest (§5.4).

### 3.4 `anchors`: which atoms a click may play

An operation may carry an optional integer:

```json
{ "name": "…", "anchors": 2, "method": "tip", ... }
```

*The `before` atoms with ids `1..=anchors` are the operation's **anchors**: the
atoms the reaction primarily acts on, and the only atoms a user may click to
place it.* Default 1, which is the origin convention (`ORIGIN_PATTERN_ATOM_ID`)
made strict: id 1 is the atom at the origin, the atom to click, and now the
only such atom unless the library says otherwise.

What the field does and does not do, stated precisely because the obvious
reading is wrong. The role rule (§4.3) gives a click the anchor at the origin
whenever that anchor's element admits it, and id 1 is the atom at the origin.
So for a **homonuclear** symmetric operation — `dimerize`, two silicons —
`anchors: 2` changes nothing: a click on either silicon already plays id 1 and
the search assigns the other to id 2, today and under this design alike. The
field takes effect only for a **heteronuclear** pair of primary atoms — an
insertion that acts on a silicon *and* a carbon, say — where a click on the
carbon is rejected by id 1 and admitted by id 2. Then the click plays id 2,
which is off the origin, and the fit's `t` lands on the silicon rather than on
the clicked atom. That is acceptable precisely because the library *declared*
it: the operation says both atoms are things a user may click, and the engine
places the reaction where the pattern puts it. The default of 1 is what turns
the origin convention from a preference into the rule, which is the whole
point of §4.3; `anchors` is the escape hatch a heteronuclear library may
never need, and it costs one optional integer.

The count form rather than a per-atom flag, for one reason: the library already
has a numbering convention that puts the primary atom first, and a count keeps
*one* convention where a flag would add a second that has to stay aligned with
it. Renumbering is what a generator does anyway when it decides which atom is
primary — mechadense's note (B), read in §4.3, is exactly such a renumbering.

Validation: `anchors` is in `1..=before.atoms.len()`; every anchor is a
**reacting** atom, not a frame atom (`is_frame_atom` false), because an
anchor that the operation does not touch is a click that places the reaction
somewhere else. The second is an error, not a warning: it is the case §4.3
exists to remove.

### 3.5 Load-time validation, extended

Existing: bond endpoints name pattern atoms, no self-bonds, order in 1..=7.
Added:

- **duplicate bond** (`[1,2]` and `[2,1]`, or twice `[1,2]`): error;
- **`deg` below the listed bond count** at that atom: error;
- **`deg` outside 0..=8**: error (nothing this engine models has more);
- **planar frame**: a `before` pattern of two or more atoms whose atoms span
  at most a plane (rank ≤ 2, by the same `rank_of` the fit uses) while
  `after` places an atom off that plane is a **warning**: "the fit cannot
  tell this pattern's up from its down; name a frame atom off the plane". A
  one-atom `before` is exempt: its orientation comes from the bond-derived
  fallback (`needs_derived_orientation`), which is a design, not an
  oversight. This is the precursor case of
  §1, and it is the root fix for it — a fifth `before` atom under the dimer
  makes the upside-down fit fail on geometry, whatever the steric threshold.
  On the v3 library it fires twice: `precursor_chemisorb` (four coplanar
  silicons) and `si_donate_edge` (a host and its two neighbours), and the
  latter's flipped fit is the "diagonal" candidate of §1;
- **valence plausibility**: for a `before` atom with a concrete element and a
  `deg`, `deg` plus the bonds `after` adds at that id minus the bonds it
  removes must not exceed the element's maximum covalent valence (H 1, C 4,
  N 4, O 2, F/Cl/Br/I 1, Si/Ge 4, and no limit for an element the table does
  not list). A warning, not an error, because the table is the one
  element-specific thing in this design and a library author may know better;
- **unbonded close pair** in `before` or `after` (§3.1): warning.

Warnings go where library warnings go today: `OpLibrary::warnings`, shown by
the `ops_library` node.

---

## 4. The bond and degree checks in the engine

### 4.1 Replay

`match_before` grows a second pass after every `before` atom has found its
workpiece atom. For each pattern atom with a `deg`, compare
`workpiece.get_atom(id).bonds.len()`. For each pair of pattern atoms, look up
`bond_order_between` and compare with the listed order or with "none". The
first failure is the error; both variants carry the step number, the operation
name and the translation, like `NoMatch` does, and are boxed into the same
`NoMatch`-sized payload discipline (`clippy::result_large_err`).

This runs for the target side and for the tool side, because at replay both
go through `match_before`. A tool side whose apex has lost a leg, or whose
cargo bond is missing, fails with the tool named, which is what the tools
design wanted the symbolic state check to approximate.

The second pass is **one function**, taking the pattern, the match (pattern
id → workpiece atom id) and the workpiece, returning the first bond or degree
mismatch. `match_before` calls it; so do the two placement paths of §4.2.
Three call sites, one predicate, which is what makes the invariant of §2 a
fact rather than a discipline.

The current header comment in `apply.rs` — "checking bonds would only add a way
for a correct script to fail" — is replaced. A script whose bonds do not hold
was generated against a different workpiece, and that is exactly what a match
failure is for.

### 4.2 Placement

The assignment search (`Search::descend`) already prunes on pairwise distance
as each `before` atom is assigned. The same loop gains two prunes at the same
point: the candidate workpiece atom's degree against `deg`, and its bond to
every already-assigned atom — the role atom included — against the pattern's
bond list. Both are O(1) per assignment and they prune *earlier* than the
distance test on a crowded neighbourhood, so the pruning-regression test's
assignment count goes down, not up. The role atom itself is **not** pruned on
degree: its mismatch is the refusal described next, not a dead branch.

The placement-side tool check, `tool_readiness` in `place.rs`, does **not** go
through `match_before`; it has its own nearest-atom loop over the tool side's
`before` at the bound pose. It gains the same second pass (the shared
function of §4.1) after that loop, reporting a mismatch as `ready: false`
with the reason, so a tool whose cargo bond is missing is dimmed at placement
exactly as it fails at replay.

The role rule (`role_atom`) checks the clicked atom's degree too, but does
not treat a failure the way it treats a wrong element. A wrong element means
the operation has nothing to say about this atom, so the sweep drops it. A
wrong degree on an atom the operation *would* act on is a coverage report —
"`si_donate_site` needs a host with 3 bonds; the clicked Si has 4" — so
`place` runs the search anyway and, if a fit exists, returns its candidates
with `Refusal::Degree` set (§7). Because the clicked atom is the same in every
candidate of one call, this refusal is on every candidate of the row, and the
row as a whole is dimmed with that reason (§5.2 has the general rule); a
single `place` call with no fit at all is the usual `NoPlacement`.

Because the same predicates run at both ends, the one-click commit path and
the sweep cannot disagree, and `every_candidate_the_engine_offers_replays`
keeps its meaning.

### 4.3 Anchors: which atom a click may play

The role rule today is: among the `before` atoms whose element admits the
clicked atom, the one at the origin, else the one with the smallest id. The
second clause is what §1's chlorine finding exercises. It exists for libraries
that do not follow the origin convention, and it means that *every* atom of
every pattern is clickable, frame atoms included, whenever the origin atom's
element happens not to admit the click. `bridge_c` has a silicon at the origin
and a `"*"` at 2.01 Å, so a chlorine click cannot be the silicon and is
therefore made the `"*"`.

The rule becomes: **among the anchors (§3.4) whose element admits the clicked
atom, the one at the origin, else the smallest anchor id; if there is none, the
operation does not act on this atom.** The `NoRole` error and the sweep's
silent drop apply as they do for an inadmissible element today, and the
message names the anchors: "`bridge_c` acts on Si (its anchor); the clicked
atom is Cl". Frame atoms are never clickable, whatever their element, because
a frame atom is by definition one the operation does not touch, and a click is
a statement about where the reaction should happen.

Everything else about the role rule stays: the role is decided once, before the
search, and never revisited; a fit that fails in the assigned role is an error
naming it. What changes is only which roles a click can be given.

**Measured effect on the three probe atoms.** The chlorine click drops from two
offers to one, `cl_abstract`, which is the only operation of the library that
acts on a chlorine. Note that a `"*"` anchor admits every element, so the
donation hosts and `bridge`'s ad-atom still *admit* the chlorine as id 1; what
removes them is the search (no silicon frame at silicon distances around a
chlorine) or the degree refusal of §4.2, and an operation whose geometry
lands within the near-miss range may remain as a near miss, which is the
coverage report working as designed. The two silicon clicks lose nothing on
their own — every operation offered there has its host at id 1 — which is why
anchors are a complement to the three checks and not a substitute for them:
they decide *which atom you may click*, the checks decide *whether the
pattern fits around it*.

**mechadense's note, read against this.** His (B) says the anchor of one
operation should be the shell-one silicon that receives the inserted bond
rather than the shell-two silicon it is on now. Under this design that is a
library decision and a renumbering: the generator makes that atom id 1 at the
origin. Nothing in the engine has an opinion about it, and nothing should. His
(B1) asks for a symmetric insertion primitive with two equivalent first-shell
silicons and "no unique preferable anchor" — is already what the origin rule
gives a homonuclear pair: a click on either silicon plays id 1 and the search
assigns the other to id 2 (§3.4 says why `anchors: 2` adds nothing there, and
why a generator may still write it as a statement of intent). The other
assignment is the mirror image, and the after-state collapse (§8) makes the
two one candidate when the reaction is symmetric and two when it is not,
which is what `dimerize` does today. His (B2), multiselection, is **not** what
the editor has now: the editor has one click and one role. What he describes
is selecting a second atom to demand that it take part too, which would filter
the offers further. Neither he nor the maintainer wants that complication in
the placement tool, and this design does not add it. It records, in §14, the
cheap form it would take if ever wanted: not a second role in the engine but a
filter on `Candidate::roles` — keep only the candidates in which the second
selected atom is matched, in any role. No search change, no format change, one
predicate over a list the engine already returns.

---

## 5. The steric check

### 5.1 Definitions

For a candidate (or a step at replay) with rigid transform `(r, t)`:

- **placed atoms**: the `after` atoms the step **adds** (ids only in `after`)
  or **moves** (ids in both with a different position), at `r · p + t`;
- **the after state**: the structure the step is applied to — the whole
  scene, feedstocks and tools included, since a placed atom that lands inside
  a parked tool is a collision whoever it belongs to — as the step leaves it:
  deleted atoms gone, placed atoms at their placed positions, bonds as the
  rewrite makes them (an added atom has the bonds `after` gives it; a moved
  atom keeps the bonds it has, minus the ones `before` alone lists, plus the
  ones `after` alone lists).

**One rule: two atoms that are not bonded to each other must not be closer
than `clash` of their covalent-radius sum. Bonded atoms are never checked.**
A bond is the library's statement that the two atoms belong at bond distance,
whatever that distance is, and the engine has no better opinion. Everything
else is a non-bonded contact and one threshold applies to all of them.

Every pair that does not involve a placed atom is unchanged by the step, and
§6 has already checked it on the inputs, so the check runs over the placed
atoms only: for each placed atom, every atom of the after state within a fixed
search radius (4 Å covers every pair of elements the engine models) that it is
not bonded to, and

```
ratio = d / (r_cov(placed) + r_cov(other))
```

with the covalent radii the application already carries (`ATOM_INFO`). The
candidate's **contact** is the pair with the smallest ratio. Two placed atoms
that are unbonded and too close are a pair like any other — that is a library
whose `after` geometry is wrong, blocked on every host, and the load-time
close-pair warning of §3.1, extended to `after`, tells the author first.

**Nothing is mutated to run the check, at either end.** `apply_matched` is
infallible by design and there is no rollback of a half-applied step, so the
check runs *before* it, from the match and the pattern alone, at replay and
at placement alike. Concretely, one function takes the workpiece, the
operation, the match and the step and returns the contact:

- a placed atom's position is `step.place(after.pos)`;
- an added atom's bonds are the `after` bonds at its id, mapped through the
  match (or to another placed atom); a moved atom's are its matched workpiece
  atom's current bonds, minus the `before`-only bonds at its id, plus the
  `after`-only ones;
- its neighbours are the workpiece atoms within the search radius, **skipping
  the atoms the step deletes** and the matched atoms of moved ids at their
  *old* positions, plus the other placed atoms of the same step at their
  placed positions;
- every neighbour it is not bonded to contributes a ratio; the smallest is
  the contact.

This is the same information the preview already derives (`preview_atoms`,
`preview_bonds`). At replay the function runs after `match_before` and before
`apply_matched`, for the target side and for the tool side; at placement it
runs per candidate after the fit.

This is the §6 pair check restricted to the atoms a step touches, which is
the point: one predicate — *unbonded and closer than `clash`* — on the inputs
and on every step, stated once.

### 5.2 One rule

A candidate whose contact has `ratio < CLASH_BLOCK` (library `clash`, default
0.9) is **blocked**: at placement it cannot be chosen, and carries the reason
where its residual would be ("would put Si 0.33 Å from Cl"); at replay it is
`MechanosynthError::Clash`, naming the step, both atoms, the distance and the
threshold. Every other ratio is silent.

**Blocking is a property of a candidate, and a row is only as blocked as all
of its candidates.** This matters because the rows that motivate the check are
mixed: `si_donate_edge` on a real edge host has one good fit and one flipped,
clash-blocked fit (§1), and the good one must stay offerable. So:

- a blocked candidate **stays in the row's candidate list**, at its index,
  after every unblocked candidate in the ranking, marked with its refusal.
  It stays because it must be previewable — the reason text is about the
  ghost the user is looking at — and `preview_at` and `CandidateRow.index`
  already index that list; a separate list would be a second `near_miss`
  with its own preview path for no gain;
- a row is **offerable** when it fits, its tool is ready, and **at least one**
  candidate is unblocked: `Applicability::offerable()` becomes
  `fits && tool ready && candidates.any(unblocked)`. A row whose every
  candidate is blocked goes below the rule with the near misses, dimmed, with
  the best candidate's reason where the residual would be. A mixed row stays
  above the rule and shows its blocked candidates dimmed inline, each with
  its reason;
- `choose` refuses **by candidate**: choosing a blocked candidate of an
  otherwise offerable row fails with that candidate's reason, so a blocked
  candidate cannot be committed by any path. The row-level refusals `choose`
  already has (near miss, tool not ready) stay as they are, and a row whose
  every candidate is blocked is refused at the row with its reason before the
  index is looked at.

The one refusal that is row-level by construction is the clicked atom's own
degree (§4.2): the clicked atom is the same in every candidate of a call, so
`Refusal::Degree` is on all of them and the row is dimmed whole. The
after-state collapse (§8) never has to arbitrate: two candidates with the same
after state place the same atoms and have the same contact.

The existing type-level fact — a row holds *either* candidates *or* a near
miss, never both — is untouched. A row with candidates and none offerable is
already a shape the editor has (a fitting row whose tool is not ready), and a
fully blocked row is that shape with a different reason.

There is deliberately no second, advisory tier. One was considered — an amber
chip for contacts between the block factor and about 1.1, which would have
flagged the donate-then-bridge intermediates and the 2.13 Å Cl–Cl geometry of
§5.3 — and dropped until someone asks for it: it is not clear it is needed, and
the data it would report is in this document. If it is ever added it is a
second constant and a chip, nothing structural.

The blocking rule reuses the mechanism a near miss and an unready tool already
use — the same `offerable()` question, the same below-the-rule presentation,
the same refusal in `choose` — with the one refinement above: the question is
asked of each candidate, and the row answers for all of them.

### 5.3 The constants, with the data that chose them

The maintainer asked for this to be argued rather than picked, and specifically
that "half the sum of the radii" and even 0.8 are too conservative, meaning
they block too little. The measurements below are from the scratch program of
§1, run on both existing libraries: every candidate the editor offers on the
three probe atoms, and every one of the 140 workpiece steps of the v3 build and
the 152 steps of the diamond build, replayed with the measurement described
after the first table.

**What must be blocked**, by ratio:

| case | ratio | caught by anything else? |
|---|---|---|
| placed atom on an existing atom (all donations on a bulk atom or a chlorinated host; the edge variant's diagonal candidate) | 0.00 – 0.31 | degree, for the bulk and chlorinated hosts |
| upside-down planar precursor: a Cl 1.84 Å from a bulk Si | **0.86** | only the planar-frame warning, once the generator acts on it |

(The bulk `dimerize` and `dimer_open` candidates compress moved atoms against
neighbours they are *bonded* to, at 0.82–0.89. Under §5.1 those pairs are not
checked; degree is what rejects those candidates, and it does.)

The measurements were taken with an earlier definition of the check that also
excluded every matched pattern atom from the comparison. Frame atoms sit at
second-neighbour distance or further from anything a step places, and the
host is bonded to what it receives, so the numbers are not expected to move;
the replay-floor fixture of §11 is what confirms it under §5.1 as written.

**What must pass**, the smallest ratios found in the two legitimate builds:

| case | ratio |
|---|---|
| diamond `gm_methylate`: the new carbon at 1.54 Å from a neighbouring carbon it does not yet bond to (the next step, `bridge`, bonds it) | **1.02** |
| silicon `cl_donate_site*`: a terrace chlorine 2.13 Å from the neighbouring chlorine | 1.04 |
| silicon `si_donate_*`: the new silicon at 2.35 Å from a neighbouring silicon it does not yet bond to | 1.06 |
| silicon `si_donate_edge`: the new silicon 2.01 Å from the T-centre carbon | 1.08 |
| every other contact in 292 steps | ≥ 1.2 |

Two things follow.

*The legitimate floor is structurally at 1.0, not at some larger number.* Both
libraries use a **donate-then-bridge** idiom: an atom is placed at exactly the
bond length from a neighbour, and a spontaneous `bridge` step one or two steps
later makes the bond. The intermediate state is real (it is what the tool
leaves behind) and it is a bond-length non-bonded contact by construction. A
rule that blocks anything under 1.0 therefore blocks legitimate builds. The
2.13 Å Cl–Cl contact is the one genuinely questionable geometry in the silicon
library; at 1.04 it passes, and it is recorded here so nobody rediscovers it.

*The block factor lives between 0.86 and 1.02, and the two sides are not
symmetric.* A false block at replay is an **error on a legitimate build**; a
false pass is the status quo, a bogus offer. So the margin on the legitimate
side matters more, which argues for the lower end. Against that, the one bogus
case at 0.86 has no other catcher until the generator adds a frame atom.

**Recommendation: `CLASH_BLOCK = 0.9`**, an engine constant, overridable per
library by `clash`. At 0.9 the margin to the legitimate floor is 0.12 (0.18 Å
for a C–C contact, 0.27 Å for Si–Si) and the upside-down precursor is blocked
by 0.04. Any value from 0.87 to 0.99 gives
the same answer on every case above; 0.95 would widen the bogus-side margin at
the cost of the legitimate one, and the choice between them changes nothing on
either library. Below 0.86 the precursor phantom returns; at or above 1.02 the
diamond build fails to replay.

**Why covalent radii and not van der Waals.** The legitimate floor by vdW-sum
ratio is 0.44 (that same diamond carbon) and the upside-down precursor sits at
0.46. The two are not separable on a vdW scale, because the legitimate idiom
places atoms at *bond* distance; only a bond-length scale separates "about to
bond" from "inside another atom".

**Why a ratio and not a fixed distance.** The generator's own collision constant
is a fixed 0.8 Å. It catches overlaps and nothing else; on an H–H contact 0.8 Å
is 1.3 bond lengths, on Si–Si it is a third of one. The ratio makes one constant
mean the same thing for every pair.

### 5.4 What a library author does about the factor

The donate-then-bridge idiom passes at 0.9 and the guide (§9) says so, with
its floor. A library whose chemistry genuinely places atoms closer than 0.9 of
a bond length to an atom they do not bond — none known — states `clash` and
says why in its `note`.

---

## 6. Structure sanity: the same rule, applied to the inputs

"Models not containing bonds correctly will be flagged as errors" is a
statement about the base, the feedstocks and the tools as much as about the
steps. `build_scene` (and the `mechanosynth` / `mechanosynth_edit` nodes'
validation of a bare base, when no scene is built) runs, per participant:

- **no bond model**: a participant with two or more atoms and zero bonds is an
  error: "`feedstock 1` carries no bonds; import with bonds or wire a `rebond`
  node". This is the xyz-import case, and it is the one that would otherwise
  make every degree check pass vacuously on a structure that has no degrees;
- **pair check**: every pair of atoms with `ratio < CLASH_BLOCK` and no bond
  between them is an error naming both atoms and their participant. The
  spatial grid makes this one radius query per atom.

Both run once per evaluation of the node, before the first step, and their
errors are node errors in the existing channel. Measured on the inputs of the
two existing demos with the same scratch program:

| structure | closest unbonded pair | ratio | at 0.9 |
|---|---|---|---|
| v3 Cl-passivated Si base | Cl–Cl 2.13 Å | 1.04 | passes |
| v3 bare Si reservoir | Si–Si 3.40 Å | 1.53 | passes |
| diamond H-passivated base | C–C 2.24 Å | 1.47 | passes |

The silicon base has thirteen of those 2.13 Å Cl–Cl pairs, the same geometry
the passivate phase later produces (§5.3): `materialize` puts two terminators
that close on the ideal-site atoms at the slab's row ends. They pass the check
by 0.14; they are a known defect of the base, recorded here, not something the
check reports.

---

## 7. What the editor and the panel show

`Candidate` gains

```rust
pub contact: Option<Contact>,        // the closest placed-vs-unbonded pair
pub refusal: Option<Refusal>,        // why the candidate is not offerable
```

with `Contact { distance, ratio, placed: (pattern id, element), other: (atom id, element) }`
and `Refusal::{Degree { expected, found }, Clash(Contact)}`. There is no
`Bond` refusal: a bond mismatch is pairwise, so the search prunes it and no
candidate is ever produced. A degree mismatch on a non-clicked atom is pruned
the same way; only the clicked atom's own degree reaches a candidate, because
"this host has 4 bonds and the operation needs 3" is the coverage report the
editor exists to give (§4.2).

`Applicability` gains nothing but the new `offerable()` rule of §5.2; blocked
candidates live in `candidates`, ranked after the unblocked ones. `rank`
orders a row's candidates blocked-last before its existing residual order,
so index 0 of an offerable row is always a placeable candidate and the row's
default preview is never a blocked one.

`CandidateRow` (and its API twin) gains `blocked: Option<String>`, the
refusal in the words the popup shows: the candidate is dimmed inline with
that text where its residual would be, and clicking it previews it (a blocked
ghost is the most useful thing to look at) but the place button is disabled.
`OfferRow` (and its API twin) gains `blocked: Option<String>` too, set only
when **every** candidate of the row is blocked, holding the best candidate's
reason; the popup then shows the row below the rule the way it shows a near
miss, with that text where the residual would be. A mixed row has `blocked:
None`, sits above the rule, and dims only the candidates that are. Nothing
else in the popup changes; the mute set is untouched. At replay a clash is an
error like any other match failure, so the `step` record does not change.

---

## 8. Phase 0: the duplicate-candidate fix

`after_state_key` keys a **kept** atom by its placed position, quantised at
1e-6 Å. A kept atom does not move, so its key should be the workpiece atom id
it matched; only added and moved atoms are keyed by position. With that, the
proper and mirrored fits of a symmetric frame collapse to one candidate
regardless of residual, and the quantisation constant stops being a hazard.
Independent of everything else in this document; one function.

**Keying a kept atom by identity forces the key to carry bonds too**, which the
first draft of this section missed. The key is an unordered set, so once the
kept atoms of two candidates are named by workpiece id rather than by position,
two assignments of a symmetric frame keep the *same* atoms and differ only in
which of them the step bonds — and the key could not see the difference. A
`bridge` over wildcard frame atoms is exactly that shape: three interchangeable
neighbours, three genuinely different reactions, one candidate offered.
Positional keying hid the hole rather than closing it, since it distinguished
those assignments only when the pattern happened to be asymmetric. So the key
also carries every bond the step adds, deletes or re-orders, its endpoints
named the same way — by the workpiece atom where the step has one, by where it
lands where it does not — enumerated the way `preview_bonds` enumerates them.

---

## 9. Where the library-authoring guidelines live

The maintainer asked where to write down how to author an operation library.
Today the format is documented inside the `mechanosynth` node's section of
`doc/reference_guide/nodes/atomic.md` ("The two files"), which is the right
audience — a library author is a user of the `ops_library` node — but the
wrong shape: a node page cannot carry a format reference plus a set of
authoring rules without swamping the node.

**Recommendation: a new reference-guide page, `doc/reference_guide/op_libraries.md`**,
"Operation libraries: the file format and how to author one", linked from the
hub `doc/atomCAD_reference_guide.md` beside the node pages, and from the
`ops_library`, `mechanosynth` and `mechanosynth_edit` sections. The format
reference moves there from `atomic.md` (which keeps a summary and a link), and
the guidelines follow it on the same page, because the rules only make sense
next to the fields they constrain. Its authoring half should state, each with
its reason:

1. **The origin convention, and anchors**: the atom the operation primarily
   acts on is id 1 at the origin, and it is the atom the user clicks. A
   reaction whose primary atoms are of *different* elements numbers them
   first and says `anchors: n`, so a click on any of them is admitted; a
   homonuclear pair needs no `anchors`, because a click on either already
   plays id 1. Frame atoms are never anchors.
2. **Frame atoms**: name the host's bonded neighbours as `"*"` atoms present
   unchanged in both halves — none for an abstraction that places nothing,
   and enough to span three dimensions whenever `after` places anything off
   the host (the planar-frame warning), so the fit is exact and cannot be
   flipped.
3. **Bonds are complete**: list every bond among the atoms you name, in both
   halves. An unlisted pair asserts there is no bond.
4. **`deg` on every atom whose environment you mean**: the host always; a frame
   atom when the operation depends on it being, say, three-coordinate.
5. **Environment variants, not tolerance**: one operation per computed
   environment, named `<operation>_<environment>` with a shared environment
   vocabulary; the 0.05 Å default gate is what makes the variants resolvable.
6. **`chiral`** when the reaction has a handedness.
7. **Donate-then-bridge**: allowed; it sits at 1.02–1.08 of a bond length,
   above the 0.9 block factor, and that margin is the reason not to write a
   library that goes lower.
8. **`clash`**: only with a stated reason.
9. **Tool frames**: four non-coplanar handle atoms, apex at the origin, never
   the cargo.
10. **Congruence**: a generated library's patterns agree with the workpiece to
    1e-4 Å; the generator asserts it.

`rust/crates/atomcad-crystolecule/src/AGENTS.md`'s mechanosynth entries point
at that page as the normative format description. The design docs keep the
rationale; the guide page keeps the rules.

---

## 10. Reference guide changes

- `nodes/atomic.md`, `mechanosynth` → "The two files": `/3`, the bond
  semantics, `deg`, `clash`, the load-time warnings; then the move to the new
  page (§9).
- `nodes/atomic.md`, `mechanosynth` → "How a step is applied": the bond,
  degree and clash checks and their errors; the structure sanity errors.
- `nodes/atomic.md`, `mechanosynth_edit` → "The offer popup": blocked
  candidates and blocked rows, their reasons, that a row is below the rule
  only when every way of placing it is blocked, and that a blocked candidate
  previews like a near miss.
- `nodes/atomic.md`, `mechanosynth_edit` → "Placing a step": "click the atom
  the operation acts on" becomes literally true — an operation is offered only
  on its anchors, and a click on any other atom of its pattern does not list
  it.
- the new `op_libraries.md`.

---

## 11. Tests

In `rust/crates/atomcad-crystolecule/tests/crystolecule/`:

- `mechanosynth_test.rs` (parser/replay): `/2` refused; duplicate bond, `deg`
  below listed count, `deg` out of range, `anchors` out of range or naming a
  frame atom are errors; planar frame, unbonded
  close pair, valence overflow are warnings naming the operation; replay fails
  with `BondMismatch` on a missing bond, a wrong order and an unlisted bond
  that exists; `DegreeMismatch` names expected and found; `Clash` names both
  atoms; tool-side bond and degree failures name the tool.
- `mechanosynth_place_test.rs`: a bonded pair is not offered `bridge`; a
  four-coordinate host is not offered a `deg: 3` donation and the sweep row
  says why; a click on a frame atom is `NoRole` naming the anchors, and the
  sweep drops the operation (the chlorine fixture: one offer, `cl_abstract`);
  either atom of a homonuclear symmetric operation places it with the clicked
  atom as id 1, with and without `anchors: 2`; on a heteronuclear fixture with
  `anchors: 2` a click on the id-2 element plays id 2 and the fit's `t` is
  the id-1 atom's position, and without `anchors` the same click is `NoRole`;
  the search prunes on bonds and degree (assignment count non-increasing on
  the pruning-regression fixture); a candidate landing on an atom is produced
  with `Refusal::Clash`, ranked after the row's unblocked candidates; a row
  with one blocked and one clean candidate is offerable and a row whose every
  candidate is blocked is not; a donate-then-bridge candidate is offerable;
  the proper and
  mirrored fits of a symmetric frame are one candidate at any residual below
  the gate; **every candidate the engine offers replays**, extended to the
  three predicates.
- `mechanosynth_tools_test.rs`: scene sanity errors (no bonds; close unbonded
  pair) name the participant; a placed atom inside a parked tool is blocked.
- a **replay-floor fixture**: the two real builds replay with every step's
  worst contact ratio ≥ 1.0 and no error at `CLASH_BLOCK = 0.9`. This pins the
  constant against the data that chose it; the fixture is the library and
  build files already under `tests/`, or a reduced pair if they are too large.

In `rust/crates/atomcad-structure-designer/tests/structure_designer/`:
`mechanosynth_edit_placement_test.rs` — `choose` refuses a blocked candidate
of an offerable row with that candidate's reason and commits its clean
sibling; `choose` refuses a fully blocked row at the row; the API candidate
row and offer row carry `blocked`, the latter only when every candidate is.

The Flutter smoke test is not run by agents; the popup's blocked candidates and fully blocked rows are a
manual walkthrough item.

---

## 12. Phases

| phase | what | touches |
|---|---|---|
| **0** | duplicate-candidate fix (§8) — **done** | `place.rs`, two tests |
| **1** | `/3`: closed-world bonds, `deg`, `anchors`, `clash` in the schema and parser; the anchor role rule; load-time validation of §3.5; bond and degree checks in `match_before` and in the placement search and role rule; new error variants — **done** | `schema.rs`, `parse.rs`, `apply.rs`, `place.rs`, `scene.rs`, tests, guide "The two files" / "How a step is applied" |
| **2** | steric check: `Contact`, `Refusal`, `offerable`, replay error; structure sanity in `build_scene` and the nodes | `place.rs`, `apply.rs`, `scene.rs`, `mechanosynth.rs`, `mechanosynth_edit.rs`, tests |
| **3** | editor surfacing: `CandidateRow.blocked`, `OfferRow.blocked`, `choose` by candidate, API, popup (blocked candidates inline, fully blocked rows below the rule); guide "The offer popup" | `mechanosynth_edit_ops.rs`, `mechanosynth_edit_api.rs`, FRB codegen, `mechanosynth_offer_popup.dart` |
| **4** | the guide page `op_libraries.md` and the move out of `atomic.md`; AGENTS pointers | docs |
| **ext** | outside the repo, before Phase 1 lands: the generator writes `/3` — bonds among named atoms in both halves, `deg` as drawn, a frame atom off the plane for the precursor and the edge host, the format string; `anchors` only if an operation has primary atoms of different elements, which none in v3 has — and both libraries are regenerated and replayed | `mechanosynth/gen` |

**Phase 1 brought two pieces of §5.2/§7 forward, because without them it would
have shipped a broken invariant.** The clicked atom's degree refusal has to
exist as soon as the degree check does, or `place` offers a candidate that
`apply_step` refuses. So `Candidate::refusal` and `place::Refusal` exist now
(with a `Degree` variant only — Phase 2 adds `Clash(Contact)`),
`Applicability::offerable` already asks whether any candidate is unrefused, and
`Applicability::blocked()` already returns the reason when none is. What Phase 2
adds to them is the steric half: `Contact`, `Candidate::contact`, the
`Refusal::Clash` variant and the blocked-last ranking of §7.

Phase 1 breaks every existing library on purpose; the generator change is the
pre-condition, and the diamond and silicon runs are the regression on it. The
replay of the regenerated v3 build must be step-identical to today's except for
the bonds the new patterns carry, which are the bonds the workpiece already
has.

**The in-repo fixtures are not generated and are migrated by hand in Phase
1.** The `/2` libraries under `rust/tests/fixtures/mechanosynth/`
(`place_ops.json`, `tool_ops.json`, `methylate_ops.json` and the rest) and the
inline libraries in `mechanosynth_test.rs` and `mechanosynth_tools_test.rs`
are hand-written. Under closed-world bonds a fixture pattern that names a
host and three unbonded frame atoms now asserts "unbonded", so the existing
placement and replay tests fail on semantics, not just on the format string.
For each fixture: bump the format, list the bonds its atoms actually have in
the fixture workpiece it is matched against, and add `deg` only where a test
is about it. `bad_ops.json` keeps its deliberate faults. A fixture whose
purpose is a `/2`-only behaviour (none is expected) is deleted with its test.

---

## 13. Considered and rejected

- **Bond checking as a warning at replay, error only at placement.** The
  argument for it is the xyz-import workpiece whose bonds were perceived
  differently. Rejected: that workpiece is what §6 refuses up front, and a
  replay that tolerates a wrong bond model produces a wrong model silently,
  which is the failure mode this whole document exists to remove.
- **An op-level "as drawn" flag instead of `deg`** (§3.2): frame atoms have
  unlisted neighbours.
- **Explicit "absent" atoms** — a pattern atom that must find *no* atom at
  its position. The steric check subsumes the case that matters (a placed
  atom's position is occupied), and the tool's bare apex is already the
  symbolic state. Cheap to add later if a library needs "this site is empty
  though I place nothing there".
- **Verifying `before` bonds without the closed-world rule** (listed bonds
  must exist, unlisted pairs unconstrained). It would need a second rule for
  `bridge`, and it would leave `dimerize` free to bond an already bonded pair.
  Closed-world within the pattern is the smaller rule.
- **A vdW-based clash scale** (§5.3): does not separate the legitimate
  donate-then-bridge contact from the upside-down precursor.
- **A fixed clash distance** (§5.3): means different things for different
  pairs.
- **A per-step valence guard at replay** (the "wanted" item in the node
  design). Superseded: with `deg` on the host and closed-world bonds, the
  host's bond count after the step is known from the pattern alone, so the
  guard is a load-time check on the library (§3.5) rather than a per-step
  check on the workpiece. The metals in a tool (a bcc tungsten apex has eight
  neighbours) are why it is a warning with an open table rather than an error.
- **Cross-participant exclusion in the steric check**, as the generator does.
  The generator excluded it to keep a tool's cargo from clashing with its own
  apex; under §5.1 the cargo is bonded to the apex and so is never checked
  against it, and a target-side atom landing inside a tool is a real
  collision.
- **Excluding every matched pattern atom from the steric check**, the
  definition an earlier draft used. It was a second rule beside the bond
  rule, it needed a special case for moved atoms, and it hid a placed atom
  landing on a frame atom. *Unbonded and too close* is the whole rule, and it
  is the same rule §6 applies to the inputs.
- **A per-atom `anchor` flag instead of a count** (§3.4): a second numbering
  convention beside the origin one, for no more expressiveness.
- **Blocked candidates in a separate list, like `near_miss`** (§5.2). A
  blocked candidate must be previewable, and it belongs to a row that may
  also hold placeable ones; keeping it in `candidates` at a stable index
  reuses `preview_at` and `CandidateRow.index` unchanged, where a second list
  would need a second preview path and a second index space.
- **Blocking the whole row on one blocked candidate.** The motivating rows
  are mixed (§5.2); a row-level block would dim `si_donate_edge` on every
  legitimate edge host because its flipped fit clashes.
- **Inferring anchors as "every reacting atom".** `c_insert` moves five atoms,
  including a hydrogen; a click on that hydrogen would place the insertion,
  which is not what anyone means by it. Which atoms are primary is knowledge
  the library has and the geometry does not.
- **Multiselection as a second role in the engine** (mechadense's B2): the
  role rule's whole value is that one click is one role decided by rule. A
  second selected atom, if ever wanted, is a filter over candidates (§4.3,
  §14), not a second search.
- **A checkbox to disable the checks.** No. A library that needs slack has
  `tolerance` and `clash`; a workpiece without bonds has `rebond`.

---

## 14. Open questions

- Whether the sweep should show *all* degree-rejected operations as dimmed
  rows, or only those whose geometry would otherwise have fit. This document
  says the latter (§4.2); the former is more complete and longer.
- The valence table of §3.5: which elements, and whether `N 4` (ammonium-like
  nitrogen) belongs in a covalent-only table.
- **A second selected atom as a candidate filter.** After the anchor rule
  and the three checks, the probe lists are short enough that this is not
  needed. If a library with many symmetric operations ever makes it needed,
  it is one predicate over `Candidate::roles` in the node, not in the engine.
- Whether `deg` should be **required** on the origin atom of every operation,
  so a library cannot forget the one place it matters most. Left optional;
  the guide says to write it, and a later `/4` can require it once every
  generator does.
