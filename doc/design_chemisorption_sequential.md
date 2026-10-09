# Design: sequential chemisorption search with orientation coverage

Status: **Phases 0–4 done** (Phase 4, the debug view, on 2026-10-09, §17;
the debug view revised after a first look the same day, §18); next: the
tolerance calibration on a real adsorbate and the Phase 5 manual
walkthrough. Outcome of a design discussion and review between
the maintainer and Claude on 2026-10-08.

This document says what changed in the `chemisorb` node and the crystolecule
`chemisorption` engine. It is the **repository copy** of a document kept in
the external mechanosynth working folder, which also carries proprietary
context (the real tools the stand-in tripod and hexapod imitate). This copy
leaves that context out; the stand-ins themselves are synthetic fixtures in
`rust/crates/atomcad-crystolecule/tests/crystolecule/`. "Old §N" cites the
original all-at-once design, `design_chemisorption_search.md`, which stayed in
that folder and describes an engine that no longer exists (it was removed in
Phase 3). Section 4.5 is corrected here for the Phase 1 findings (§14.2 items
1, 2, 3 and 8); the external copy keeps the wording as reviewed.

---

## 1. Why

The current search takes **one pose** (old R1) and relaxes every combination
of sites within `reach` of the posed feet. A pose covers only the orientations
that `reach` allows around it, so the exhaustiveness claim holds for that pose
only. Old §9 deferred orientation sampling for this reason.

Brute force — the current search over many sampled orientations — is
redundant and expensive. Most orientations produce the same bond sets, each
one costs a full search, and results from different poses do not compare,
because each has its own reference state.

The insight that replaces it: **what distinguishes outcomes is which feet bond
to which sites, and the site pattern is constrained by the molecule's own
geometry.** Once one foot is bonded, the second foot can only reach sites on a
sphere around it. Once two are bonded, the third can only reach sites on a
torus. So the orientations do not need to be sampled. They follow from the
site choices, and each site choice gives its orientation directly.

This is the docking literature's *anchor-and-grow* (incremental
construction, as in DOCK and FlexX), adapted to covalent binding. It is also
closer to kinetic control than all-at-once placement: legs bind one after
another.

---

## 2. Decisions made in the discussion

| # | Decision |
|---|---|
| D1 | One node does the whole search. The search is **not** split into one chained `chemisorb` node per leg (§9 says why). |
| D2 | Leg 1 is searched within a small `anchor_reach` of the posed foot. Translations are mostly redundant on a periodic surface, and the variety of orientations comes from leg 2 onwards. |
| D3 | Leg 2 is searched on a **sphere** around leg 1. The full sphere is swept: no angle limit and no swing parameter. Users will say if they need one. |
| D4 | Leg 3 is searched on a **torus** around the leg 1–2 axis: the overlap of two exact shells, one around each bonded site (§4.4). |
| D5 | The adsorbate is placed by a **Kabsch fit** of its bonded feet onto their sites (lifted by a bond length, §4.5), not by relaxation. Nothing is relaxed as a step on the way to leg 3: a two-leg hypothesis is relaxed only when it is itself a candidate (§4.8, "What is relaxed"). |
| D6 | **Legs 4 and later** are searched within a local `reach` of the relaxed geometry (§4.6). The geometric alternative was rejected after the spike: hexapod feet move up to 3.26 Å on relaxation, past `reach` (§10 Q1, §13.7). |
| D7 | Three user parameters: `anchor_reach`, `tolerance` and `reach`. The radii of the sphere and the torus are computed from the molecule, never set by the user; one `tolerance` serves both (decided after the Phase 0 spike, §13.9). |
| D8 | **One-leg bindings are dropped**, except for an adsorbate with a single foot (§4.1). |
| D9 | Monovalent transfers use one **deterministic rule** (§5), not enumeration and not Monte Carlo. To be improved later. |
| D10 | The sphere and torus are **site-centred**: they assume no absolute direction for a new bond (§4.3). A cap on how far the bonds of one binding diverge was considered and deferred (§8). |
| D11 | The `best` output pin is removed (§6.2). |
| D12 | A **debug view**: the search tree in the properties panel, and the selected row drawn on two debug output pins (§6.5). This is what mechadense gets instead of chained nodes. |
| D13 | Clash detection after seating is always reported; pruning on it is `clash_filter`, **on by default** (§4.5). It was off by default until the spike showed no clashing seating relaxes into the window (§13.4). |
| D14 | Mirror-image three-leg assignments are **pruned by default** by an exact handedness check that abstains when undecided (§4.5, "Mirror check"). Unlike the clash filter it involves no threshold on distances. |

---

## 3. Terms

Old §3 still applies, except for the pair tolerance and `reach` as defined
there. New terms:

- **Foot** — an adsorbate reactive atom that can form a bond with a site:
  either a free valence, or an H it can donate under the transfer rule (§5).
  A *leg* is one foot–site bond, together with the H transfer it brings
  when the foot donates (§4, §5).
- **Bond length** `bᵢ` — the rest length of the bond foot `i` would form with
  its site, for that element pair (the UFF rest length).
- **Foot geometry** `F₁ … F_k` — the positions of the feet in the posed
  adsorbate. Only their relative positions matter: the search moves the
  molecule rigidly.
- **Site-centred** — every search shape (sphere, torus) is centred on the
  **sites**, which are fixed atoms of the substrate. Nothing assumes an
  absolute direction for a new bond (no normal, no dangling-bond direction);
  a foot may sit anywhere at its bond length from its site. The search uses
  only atoms and distances, as old §3 intends ("no plane, normal, facet").
- **Tolerance** — how much farther than the bond lengths allow a site may be
  from its exact shape (sphere or torus) and still be tried: the room the
  molecule has to flex. Bond directions are already covered by the bond
  lengths in each test.
- **Local up** `û` — for seating and the mirror check only (§4.5), never
  for the search's distance tests: the unit
  vector from the centroid of the substrate atoms within 5 Å of the bonded
  sites to the centroid of those sites. An estimate of the surface normal at
  the binding, from atoms only.
- **Seating** — the rigid transform, from the Kabsch fit, that places a
  hypothesis's start geometry.

---

## 4. The algorithm

A hypothesis is built leg by leg. A leg is one (foot, site) bond plus, for a
foot that donates its H, the transfer the rule fixes when that leg is added
(§5). The hypothesis's **change set** is its bonds and its transfers. Its
start geometry depends only on the change set, so deduplicating by change set
is exact (§4.7).

### 4.1 Feet and sites

Unchanged from today (old R2, R6), spelled out here. Two independent
conditions select the candidates on each side: the tag, and the valence.

**Sites** (substrate):

- **Tag:** if `substrate_tag` is set, only atoms carrying it are candidate
  sites; if it is empty, every substrate atom is.
- **Valence:** independently of the tag, a site needs a **free valence**: its
  standard valence minus its existing bonds, counting bond orders
  (`free_valence` in `enumerate.rs`). A fully bonded atom is never a site,
  even when tagged. An atom with two free valences can take two bonds.
- **Hydrogen blocks absolutely.** An H-capped surface atom has no free valence,
  so it is never a site. (Today it could become one through H abstraction,
  `to_adsorbate`; that is out of scope, §5.) Placing H by hand is still how to
  block spots.
- **Frozen** atoms can be sites. The only frozen rule is that no bond forms
  between two frozen atoms.

**Feet** (adsorbate), symmetrically:

- **Tag:** `adsorbate_tag` selects the candidate feet; empty = every adsorbate
  atom.
- **Valence:** a foot needs a free valence, or, when a `to_substrate`
  transfer record for that element is wired, a monovalent atom it can donate
  under the rule of §5 (an OH leg's H).

Validity is checked on the whole hypothesis, as today: every atom ends within
its valence, counting formed bonds and the transferred H together. A
transferred H can go only to a site with valence left at the moment its leg
is added (§5).

Further:

- Each foot forms **at most one** bond, in every phase, as today (old R6). A
  site with two free valences can still take two feet.
- **One foot** (water's O, a single-ended precursor): the search is leg 1
  only, and one-leg bindings are the results. This is the only case where they
  are listed (D8).

**Foot order** (added 2026-10-09, asked for by users who want to choose the
order the legs bind in). With `adsorbate_tag` = `foot`, atoms tagged `foot1`,
`foot2`, … (the tag followed by decimal digits only) are the feet, and the
number fixes the order: leg *k* is only ever the *k*-th foot by number, in
every phase (anchor, sphere, ring, local). Gaps are allowed (`foot1`, `foot3`,
`foot7`). A numbered foot that finds no site ends its branch; a later foot is
never bonded in its place, so the results are the order's prefixes. Plain
`foot` keeps the exhaustive search with dedupe. Refused (`FootOrder`): the
plain and the numbered form on one adsorbate, two atoms with one number, one
atom with two numbers, and a numbered atom that cannot be a foot (dropping it
silently would shift every later number). The fingerprint hashes the numbered
tags.

### 4.2 Leg 1: anchor

For each foot `f₁`, every site `s₁` within `anchor_reach` of the posed `F₁`
(foot to site, as the current `reach`). Nothing is relaxed or seated yet.

### 4.3 Leg 2: sphere

Foot 2 is always `d₁₂ = |F₂ − F₁|` from foot 1, and each foot is at its
bond length from its site, so `s₂ − s₁ = (F₂ − F₁) − (b₂d₂ − b₁d₁)`, and
the bond term is at most `b₁ + b₂` long. So, by the triangle inequality,
site 2 lies in a spherical shell around **site 1**. For each other foot `f₂`,
accept site `s₂` if

```
d₁₂ − b₁ − b₂ − tolerance  ≤  |s₂ − s₁|  ≤  d₁₂ + b₁ + b₂ + tolerance
```

Exact at tolerance 0 for every placement, whatever the bond directions.

The shell is thick: about ±3.3 Å plus the tolerance for two Si–O bonds, and
±3.8 Å for two C–Si bonds. For a compact adsorbate (`d₁₂` below that) the
inner bound is negative and the shell is a solid ball of radius
`d₁₂ + b₁ + b₂ + tol`; the leg-3 ring degenerates the same way. That is
accepted for now: it assumes nothing about bond directions, and what it
admits beyond physical bindings relaxes into high-strain candidates that
rank low. The spike measured the cost (§13.6): the default `clash_filter`
and a `substrate_tag` on the facet are the levers. A bond-divergence cap is
not one: the best tripod binding diverges by 100° (§13.5).

Every (f₁, s₁, f₂, s₂) that passes is a **two-leg hypothesis**. Bonds go to
different sites unless a site's valence allows two (old R6).

### 4.4 Leg 3: torus (the two-shell ring)

Two bonded feet leave one degree of freedom, a rotation about the axis
through them, so a third foot `f₃` moves on a circle around that axis. The
sites it can bond to form a ring around the `s₁s₂` axis: a solid torus. It is
found by the same triangle-inequality argument as the sphere (§4.3), applied
to foot 3 against **both** bonded sites. Foot 3 is always `d₁₃ = |F₃ − F₁|`
from foot 1 and `d₂₃ = |F₃ − F₂|` from foot 2, and every foot is at its bond
length from its site, so accept `s₃` if

```
d₁₃ − b₁ − b₃ − tolerance  ≤  |s₃ − s₁|  ≤  d₁₃ + b₁ + b₃ + tolerance
d₂₃ − b₂ − b₃ − tolerance  ≤  |s₃ − s₂|  ≤  d₂₃ + b₂ + b₃ + tolerance
```

The overlap of the two shells, one around `s₁` and one around `s₂`, is a
thick ring around the `s₁s₂` axis (decision 2026-10-08). Like the sphere, it is
**exact at tolerance 0**: every placement of the rigid molecule with each
foot at its bond length from its site passes, whatever the bond directions.
So the tolerance means the same thing in both tests, only the molecule's
flex, and one `tolerance` serves both (§10 Q2, decided).

**Do not replace it by a distance from a circle** drawn around the `s₁s₂`
axis (the foot circle moved onto the sites). An earlier draft did; moving
the circle off the foot axis shifts and tilts it by up to about a bond
length when the bonds tilt differently, so that test is not exact at
tolerance 0 and the tolerance silently absorbs the error. The ring's price
is thickness: its tube is about `b₁ + b₃` and `b₂ + b₃` wide, so more
hypotheses pass. The mirror check (§4.5) removes about half of them, and
the spike reports the count (§10 Q2).

Each accepted (f₃, s₃) extends the two-leg hypothesis to a **three-leg
hypothesis**. Every remaining foot has its own ring (its own `d₁₃`, `d₂₃`),
so a hexapod tries every remaining foot as `f₃`.

### 4.5 Seating

The search never needed bond directions, but a start geometry needs the feet
a bond length off their sites, on the body's side. Seating gets that from the
local up `û` (§3), estimated from atoms. Each bonded site gives a seating
point `pᵢ = sᵢ + bᵢ·û`.

**Three or more bonded feet.** A Kabsch fit of the feet `Fᵢ` onto the seating
points `pᵢ` gives the rotation and translation. With three non-collinear
points it is unique and never a reflection. **Collinear seating points** (sites
along one dimer row) leave the turn about their line undetermined, and an
eigen-solver returns an arbitrary one; `seat` then turns the body about that
line by the two-leg θ rule below (§14.2 item 2). Any mismatch is spread over all
the feet, which is "close enough" for relaxation (§10 Q3 checks this); the
relaxation finds the actual bond directions.

**Mirror check** (decision 2026-10-08). The torus is symmetric about the
`s₁s₂` axis, so it accepts a third site on either side of it. For a roughly
planar foot triangle, half of the foot-to-site assignments are then the
mirror image of the feet: no proper rotation puts the feet on their seating
points with the body on the open side, and Kabsch, which never reflects,
flips the body through the substrate instead. These are recognised by
handedness, before seating, from the three geometric-phase legs:

```
N_F = (F₂ − F₁) × (F₃ − F₁)       σ_F = sign(N_F · (B − F̄))
N_s = (p₂ − p₁) × (p₃ − p₁)       σ_s = sign(N_s · û)
```

`B` is the centroid of the adsorbate's heavy atoms other than the feet, `F̄`
the centroid of the three feet. `σ_F` says which side of its feet the body is
on (a property of the molecule); `σ_s` says which side of the seating points
is open. The assignment is **proper** when `σ_F = σ_s` and **mirrored** when
they differ.

- **Exact where it decides.** It reads only the seating points and the sign
  of `û` against their plane, so `û` needs to be within 90° of the true
  normal, a far weaker demand than seating itself makes. A body tilted
  sideways (a step face) still passes; only the side counts, not the angle.
  No clash threshold or neighbourhood radius takes part.
- **Abstains where it cannot.** When either sign is near zero,
  `|N·v| / (|N|·|v|)` below an internal margin (about sin 15°): a body nearly
  in its foot plane (a flat molecule), a site triangle standing steep against
  `û` (sites on different terraces). Nearly collinear **feet** abstain at the
  same margin on an order-free triangle quality, `2√3·|N| / Σ|edge|²`; nearly
  collinear **seating points** abstain only when degenerate to rounding
  (quality below 1e-6), since collinear sites are common and their normal is
  rounding noise that depended on the order of the legs (§14.2 item 1). An
  undecided hypothesis is **kept** and relaxed, as if there were no check.
- **Prunes by default**, unlike the clash filter, because it is exact
  geometry, not a threshold. Pruned hypotheses are counted as
  `pruned_mirror`, undecided ones as `mirror_undecided`, and both are marked
  in the debug tree (§6.5), so nothing disappears unseen. The spike confirms
  first that no pruned hypothesis relaxes into the energy window (§10 Q7);
  if one does, the check becomes report-only like `clash_filter` until the
  cause is understood.
- Local-phase hypotheses (§4.6) descend from a three-leg parent that already
  passed, so the check runs once, at leg 3. Two-leg seating is unaffected:
  its θ rule (below) already picks the body-up side.

**Exactly two bonded feet.** The rotation `θ` about the `p₁p₂` axis is not
fixed by the fit. A two-leg binding is a result in its own right ("stuck on
two legs"), but there is no third site to aim for, so a fixed rule is enough.
`θ = 0` is the turn that puts the body straight up, computed from the
molecule and the sites only (a shortest-arc start would carry the pose into
the seating, §14.2 item 3). The angles are tried 0, ±10°, ±20°, … and the
first clash-free one wins (clash as defined below): the body's height falls
with `|θ|`, so that is the clash-free angle with the body furthest along `û`.
If every angle clashes, the angle with the fewest clashing pairs. This chooses a
seating, it filters nothing, so it applies whether or not the clash filter is
on. The step size is an internal constant.

**Clash detection.** A seating *clashes* if two atoms pass through each
other, which relaxation cannot repair (the tangles of old §10 Q5), as opposed
to an ordinary close contact, which UFF clears easily and which approximate
seating produces routinely. Concretely: a heavy-atom pair, one adsorbate and
one substrate atom, closer than an internal fraction (about 0.6) of the sum of
their **covalent** radii, about 1.1 Å for Si–C. Excluded from the check:

- the bonded feet and their sites;
- the atoms within two bonds of each bonded foot (its own neighbours and
  their hydrogens, which sit at the surface by construction);
- transferred H atoms;
- all hydrogens, at first: an H overlap is cheap for UFF to repair.

An upside-down seating (the body inside the substrate) clashes only a
handful of times at this threshold (three pairs in the test case, §14.2 item
8), which is still enough to catch it. The mirror-image assignments that cause
most of them are already pruned by the mirror check above; the clash check
catches the rest, among them the undecided ones.

**Clash filter: on by default** (`clash_filter`, §6.1). It was planned off
by default, to avoid surprises from bad filtering while the algorithm was
new; the Phase 0 spike met §10 Q7's criterion (no clashing seating relaxed
into the window, the nearest 277 kcal/mol above the best, §13.4), so it is
on (decided 2026-10-08 after the Phase 0 spike, §13). It can still be turned off. Clashes are always **detected and reported**: the
hypothesis carries the flag, the candidate record has a `seating_clash` field
(§6.2), the statistics count `seating_clashes`, and the debug view marks the
clashing atoms (§6.5). Only with `clash_filter` on are clashing hypotheses
**pruned** before relaxation and counted as `pruned_clash`. So a user, or the
maintainer, can turn it off, look at what the filter removes, and check that
none of it relaxes into the energy window.

Kabsch is the crate-level `rigid_fit.rs` (promoted from
`mechanosynth/fit.rs` in Phase 1), shared with mechanosynth.

### 4.6 Legs 4 and later: local

This applies only to adsorbates with four or more feet; a tripod never gets
here.

Relax each three-leg hypothesis that survived the mirror check (and the
clash filter, when on), as a parent even when it is not a candidate (§4.8).
Then, for each unbonded foot, every site within `reach` of the foot's
**relaxed** position gives a four-leg hypothesis. Its start geometry is the
parent's relaxed geometry plus the new bond (and the new leg's H, §5); relax
from there (the stretch is at most `reach`, which UFF handles), and repeat
leg by leg.

- **Breadth-first**, one level per leg. Within a level, deduplicate by change
  set and keep the lowest strain (two orders of binding can give one change
  set). A new leg adds its own bond and transfer to the relaxed parent; the
  parent's transfers stay where they are (§5).
- **Frontier memory.** Between levels, a state stores only the positions of
  the atoms that move: the adsorbate plus unfrozen substrate atoms. That is
  24 B per atom, about 7 MB per 1,000 states of 300 movable atoms, and it is
  bounded by the budget. This is not the full-structure storage that old
  §11.4 removed. It is **working memory only**: at most two levels are held
  at a time (the one being grown from and the one being built), and all of it
  is freed when Run finishes. Nothing after Run reads it; the debug view
  re-relaxes instead (§6.5).
- **Canonical parent.** When dedup collapses two binding orders into one
  change set, the kept state records which parent it was grown from (the parent's
  row id, a few bytes). The debug view's replay follows these links (§6.5).
- A relaxed state is a candidate only if the filters admit it (§4.8); the
  others were relaxed as parents.

The geometric alternative (place the remaining feet by the three-foot fit
and relax once) was rejected after the spike (§10 Q1, §13.7).

### 4.7 Deduplication and determinism

- A hypothesis's seating is a function of its bond set: the same feet and
  sites give the same seating points and the same fit. Its transferred H's
  are placed from the seating and their acceptors. So the start geometry is a
  function of the change set, and deduplicating by the normalized change set
  (bonds plus transfers, old R8's change key) is exact in the geometric
  phase. Ordered pairs (f₁, f₂) and (f₂, f₁) collapse when they fix the same
  transfers.
- **Binding order can change the transfers.** An acceptor is fixed when its
  leg is added (§5), so two orders of the same bonds may send an H to
  different sites. Those are different final states and stay separate
  hypotheses: a few extra relaxations where the order matters, in keeping
  with the kinetic reading of a leg-by-leg search (§1).
- Determinism (old R10) holds as today. Enumeration runs in atom-id order,
  relaxations run in parallel with `keep_best` by (strain, change set), and the
  local phase sorts each level before truncating.
- **Numbered feet** (§4.1, "Foot order") give one order, so no two paths
  reach one change set and the dedupe never fires; it stays in place.
- **Not deduplicated:** translational copies, the same pattern on an
  equivalent set of sites elsewhere on the proxy. A small `anchor_reach`
  limits them. Recognising equivalent sites is deferred (§8).

### 4.8 Reference state and ranking

The pose is no longer the reference, because candidates come from many
orientations. The reference becomes the **separated** state: the adsorbate
relaxed alone plus the substrate relaxed alone (two relaxations). `strain` is
`E(candidate) − E(reference)`. The atoms are the same, so the comparison is
valid, and as before it is clean only within one bond inventory (old §11.3).
Ranking is by strain alone, as today, and the inventory stays a filter, never
a sort key.

**The leg-count and inventory filters stay, and they are how results compare
fairly** (decision 2026-10-08, confirmed against the engine). UFF has no
bond-energy term, so strain favours fewer bonds: an unfiltered search ranks a
tripod "stuck on two legs" above the same tripod on three, and with `top_n`
pruning during the search the full bindings can be evicted before the user
sees them. The answer is the one the engine already gives (`chemisorption`
module doc; `formed_bonds` and `bond_inventory` in `config.rs`), not a new
ranking rule:

- `formed_bonds` (exact count) and `bond_inventory` (exact inventory) remain
  **search settings applied during enumeration**, before any relaxation and
  before `top_n` and `energy_window`. Only hypotheses that pass them are
  candidates, so `top_n` ranks one leg count, or one inventory, at a time.
- In the geometric phase they prune levels directly: `formed_bonds = 3` relaxes
  no two-leg hypothesis; `formed_bonds = 2` skips the torus. The inventory
  filter fixes the depth (its formed bonds, less the H bonds its transfers
  form: one per broken O–H) and prunes any branch whose formed bond kinds
  already exceed the target.
- In the local phase (§4.6), a state shallower than the filter still has to be
  relaxed, because it is the **parent** of the deeper states. Such
  intermediates spend `budget` and are recorded in the debug tree, but they
  are **not candidates**: they never enter `top_n`, `energy_window` or the
  `candidates` output. The same holds for the three-leg parents that the
  local phase grows from.
- Unfiltered (both unset) stays allowed and mixes leg counts, as today. The
  panel's inventory list (`inventory_options`), the `formed_bonds` field and the
  per-level statistics show what is mixed. The reference guide must say to
  set one of the two filters before reading the ranking.

**What is relaxed** (decision 2026-10-08). A hypothesis is relaxed for one of
two reasons, and only these:

1. **It is a candidate**: the filters admit it (`formed_bonds`,
   `bond_inventory`, `max_formed_bonds`, and D8's one-foot rule for one leg).
   Whether deeper hypotheses grow from it does not matter. Unfiltered, a
   two-leg tripod binding is a candidate even though three-leg bindings extend
   it: "stuck on two legs" is a result in its own right (a kinetic trap), not
   only a step.
2. **It is a parent in the local phase** (§4.6): a three-leg or deeper state
   the next local leg is searched from. Relaxed, not a candidate unless (1)
   also holds.

So in the geometric phase nothing is relaxed only as a step: a two-leg
hypothesis that is not a candidate (e.g. `formed_bonds = 3`) only spawns
three-leg hypotheses, and those are Kabsch-seated from their own bonds (D5),
never from a relaxed two-leg geometry. The local phase is the one place where
intermediates are relaxed, because its search reads the relaxed positions.

### 4.9 Plan and budget

- **Where the pruning comes from.** Leg 1 is limited by `anchor_reach`; leg
  2 by the shell, leg 3 by the ring (§4.3, §4.4); then the mirror check
  halves the three-leg set (§4.5) and dedupe collapses binding orders
  (§4.7). The shells are thick, and for a compact adsorbate they are solid
  balls, so leg 1 is then the only real limit. How much smaller the search
  is than the old all-at-once enumeration is **measured, not claimed**: the
  spike reports the hypothesis and relaxation counts per level, against the
  old search, for the stand-in tripod and hexapod (§13.2, §13.6). `budget` bounds
  the cost either way; `clash_filter` and a `substrate_tag` on the facet are
  the levers if the counts are too high.
- The geometric phase (§4.2–4.5) is pure geometry. `plan` counts the two- and
  three-leg hypotheses **exactly** before Run, as today, and they update live
  in the panel. The relaxation count it reports is the hypotheses that will be
  relaxed under §4.8 "What is relaxed": the candidates, plus the three-leg
  parents when a local phase follows; two-leg rows that are only steps cost
  nothing. The mirror check, seating, the clash check and the transfer rule
  all run in `plan` too: they are geometry, cheap next to a relaxation.
  **But `plan` runs on every evaluation, so its time is measured in Phase 1**
  (decided 2026-10-08). The spike timed enumeration only (0.09 s for the
  hexapod's 7,500 three-leg hypotheses, §13.7). Seating and the clash check
  are dearer, especially the two-leg θ rule: 36 angles, each with a clash
  check. If `plan` on the stand-in hexapod takes noticeably longer than the
  evaluation it sits in (the threshold is set in Phase 1, a fraction of a
  second), the per-row seating, clash and mirror results move out of `plan`.
  They would then be computed when a row is expanded or selected, and by Run
  for the rows it relaxes. Enumeration, the counts and the dedupe stay in
  `plan`. Nothing visible changes, except that the clash and mirror counts
  show only after a row is expanded or after Run.
- The local phase (§4.6) depends on relaxations. The panel shows its count as
  "+ local phase" without a number; the statistics give it after Run.
- `budget` counts relaxations. The geometric phase relaxes first in plan
  order; the local phase spends what is left, level by level, and truncation
  of a level is reported. A truncated search makes no exhaustiveness claim, as
  today.

### 4.10 What the exhaustiveness claim becomes

> Every binding reachable by: leg 1 within `anchor_reach` of the posed feet;
> leg 2 on any site the foot spacing and bond lengths allow, plus
> `tolerance`; leg 3 likewise on the ring its distances to both bonded
> sites allow, plus `tolerance`; further legs within `reach` of the relaxed
> geometry; each transferred hydrogen placed by the rule of §5 when its leg
> forms, after which its acceptor is unavailable to later legs; minus the
> three-leg assignments the mirror check rejects as mirrored (§4.5), and,
> with `clash_filter` on, those whose seating clashes.

This is weaker than "every combination" in one way (it depends on the order of
binding, and the transfers are a rule, not a search: an H that took a site a
later leg could have used is never tried elsewhere), but it is stronger where
it matters: it covers orientation, which the current claim does not. It is
well defined and reproducible.

---

## 5. Transfers: one deterministic rule

Enumerating every acceptor for every donated H multiplies the search (old
§11.2 finding 3), and UFF cannot rank the alternatives anyway. In the water
case, "H on the same dimer" and "H on the next dimer" differ by ≤ 0.4 kcal/mol,
and which one wins flips with the pose (old §11.2 finding 1). So for now
(D9):

- **Rule:** when a foot bonds by donating its H (an OH leg), the H goes to the
  free site **nearest to the site the foot bonded to**, among sites with
  valence left at that moment: after the bonds and transfers of the earlier
  legs and this leg's own bond. Ties are broken by atom id. On Si(100) this is
  the dimer partner, the concerted four-centre pathway and the experimental
  answer for water.
- **Fixed when the leg is added** (decision 2026-10-08). The acceptor is
  chosen as the leg forms, in leg order, and never revisited. Later legs see
  it as used. So a relaxed parent in the local phase (§4.6) already holds its
  H's on their final sites, and a child adds only its own leg's bond and H.
  The rule reads only site positions, so in the geometric phase it runs in
  `plan` and the counts stay exact.
- **Not the foot's own site.** A site with two free valences is at distance 0
  from itself; it is excluded as its own foot's acceptor, which keeps the
  four-centre pathway the rule rests on.
- **No acceptor:** if no free site lies within `reach` of **the foot's site**
  (site to site, the same reference the rule measures from), the leg, and so
  the hypothesis, is dropped and counted as `pruned_no_acceptor`. No seated
  position takes part, so the test is not circular.
- **Seating:** the H is placed at bond length from the acceptor, in the
  acceptor's **open valence direction** — the slot guided placement offers,
  where `passivate` would put a terminator — once the leg bonds are made. Of
  several open slots it takes the one nearest the side the H comes from (the
  line from the acceptor to the H's position on its foot, after the adsorbate
  is seated, or in the relaxed parent); a bare acceptor with no slot to read
  takes that line itself. Two H on one acceptor are seated one after the
  other, so they take two slots. (Revised 2026-10-09, §18.4: the line alone
  had put the H across the acceptor's own bonds.)
- **Known limit.** A site taken by an earlier leg's H is unavailable to later
  legs, and the alternative (that H elsewhere, the site free) is not
  searched. That is right for a concerted transfer, and it is the price of
  one deterministic rule (D9); the transfer search (§8) is where it would be
  lifted.
- **Scope:** `to_substrate` only (legs donating H). `to_adsorbate`
  (abstraction of surface H) and patterns that are only transfers have no
  place in a leg-by-leg search; they are out of scope (§10 Q5).
- `max_transfers` goes away. Every OH foot that bonds transfers exactly one H.

The rule determines the bond inventory from the feet: a bonded OH foot always
brings `broken O–H, formed H–Si`. The `bond_inventory` filter therefore still
prunes during enumeration.

---

## 6. The node

### 6.1 Properties

| Property | Default (set by the Phase 0 spike, §13) | Meaning |
|---|---|---|
| `anchor_reach` | 3.5 Å | leg 1: sites within this of a posed foot |
| `tolerance` | 0.5 Å | legs 2 and 3: how far a site may lie outside the exact shell or ring, for the molecule's own flex (the bond lengths already cover every bond direction) |
| `reach` | 3.0 Å | legs 4 and later: sites within this of a relaxed foot; also the H acceptor reach (§5), measured site to site from the foot's site. Greyed out when the adsorbate has ≤ 3 feet and no transfer rule |
| `clash_filter` | **on** | prune seatings that clash (§4.5) instead of relaxing them; clashes are reported either way |

**Why one tolerance, and why 0.5 Å** (decided 2026-10-08, after the spike).
The bond lengths in each test already cover every bond direction: the old
calibration's largest needed deviation, 2.64 Å on site distances (old §8.6),
is inside `b₁ + b₂ ≈ 3.4 Å` for two Si–O bonds at tolerance 0. What is left
for the tolerance is a change in the foot spacing itself, which the tests
take from the posed, rigid molecule. On the stiff stand-in cage that needed
0.00 Å (§13.2). The tolerance stays anyway, for **non-rigid adsorbates**: a
molecule with a flexible linker between its feet can change `d₁₂` by
angströms through torsions, and without a tolerance its bindings would be
lost silently. 0.5 Å is the default margin; the sphere and the ring use the
same value, since the term means the same thing in both. Raise it for a
floppy adsorbate; the debug view's near misses (§6.5) show what that adds.

Unchanged: `adsorbate_tag`, `substrate_tag`, `max_formed_bonds` (now limits
the depth), `formed_bonds`, `bond_inventory`, `top_n`, `energy_window`,
`budget`, `max_iterations`. All of them are fingerprinted, as today.
`formed_bonds` and `bond_inventory` keep their role as enumeration-time
filters; they are what makes strains comparable (§4.8). Removed:
`max_transfers`. The name `pair_tolerance` is deliberately not reused.

### 6.2 Pins and records

- **Outputs: the `best` pin is removed** (mechadense, 2026-10-08: it is
  unnecessary). The outputs become pin 0 `candidates`
  (`[Record(ChemisorbCandidate)]`, now the primary output) and pin 1 `stats`
  (`Record(ChemisorbStats)`). Consequences:
  - The node no longer draws atoms in the viewport. A candidate is viewed by
    extracting its `structure` field downstream (e.g. an array element, then
    the record field), as any non-best candidate is today.
  - Before Run there is nothing to show either. Old §7.2 output the unrelaxed
    pose as `best` so the viewport showed the setup; now the user sees the
    pose by displaying the adsorbate and substrate inputs themselves.
  - Old §11.1's "after a run that found nothing, `best` is the relaxed
    reference" goes with it. Whether to expose the reference state (old §10
    Q3) remains open; it is less useful now that it is the separated state
    (§4.8).
  - Pin indices shift: wires saved from today's pins 1 and 2 must be moved to
    0 and 1. Following the old §11.3 precedent (one user), no file migration;
    the existing test fixtures are updated.
- Inputs unchanged. The `transfers` pin keeps its record. A `to_substrate`
  record enables the rule for that element; a `to_adsorbate` record is an
  error naming the record (§5).
- `ChemisorbCandidate` keeps its `formed_bonds` field, which **is** the leg
  count (transfers are not counted in it); no `legs` field is added (§10 Q6).
  A downstream `filter` on it shows any level of the search. It gains
  `seating_clash: Bool`, true when its start geometry clashed (§4.5); with the
  clash filter off, such candidates are relaxed and listed like any other,
  and this field shows which ones they are.
- `ChemisorbStats` gains `anchors`, `sphere_pairs`, `torus_triples`,
  `seating_clashes` (always counted), `pruned_clash` (0 unless
  `clash_filter` is on), `pruned_mirror`, `mirror_undecided` (§4.5),
  `pruned_no_acceptor` and `local_relaxed`, and drops
  `transfer_candidates`.

### 6.3 Run model

Unchanged (old §7.4): the Run button, the result stored in node data keyed by
an input fingerprint and never saved, stale fallback to the plan, CLI `run`,
background job with cancellation.

### 6.4 The old single-pose mode

**Replaced, with no compatibility mode** (decided 2026-10-08, §10 Q4). Setting
`tolerance` small and `anchor_reach` to the old
`reach` comes close to the old behaviour, and keeping two algorithms doubles
the testing and the explanation. The precedent is old §11.3: one user, no
backward compatibility.

### 6.5 Debug view

The search is a tree: each row has some legs bonded, and a row that was
relaxed (a candidate, or a local-phase parent, §4.8 "What is relaxed") has a
relaxation once the search has run. The debug view shows that tree
in the node's properties panel and draws the selected row in the viewport. It
serves the maintainer, to debug the algorithm, and users, mechadense first, to
see and steer each level and to set `anchor_reach`, the tolerance and
`reach` from what they see rather than by guessing. It is what this design
offers in place of chained nodes (§9).

**The tree in the panel.**

States (the root, a leg) and steps (a *next foot*: one foot's test from a
state) alternate (revised 2026-10-09, §18):

```
root (posed molecule)
└─ next foot O12                     anchor: 4
   └─ O12–Si45
      ├─ next foot O13               shell: 9, 3 near
      │  └─ + O13–Si61
      │     └─ next foot O14         ring: 2, 1 clash, 1 dup. hidden
      │        └─ + O14–Si70         strain 41.2
      └─ next foot O14               shell: 7
```

With **Show duplicates** on, the hidden row appears too:

```
      │     └─ next foot O14
      │        ├─ + O14–Si70         strain 41.2
      │        └─ + O14–Si52         = duplicate of …   (click jumps there)
```

- Levels: root → next foot → leg 1 → next foot → leg 2 → … A row's identity
  is its **path** of (foot, site) choices, stable for as long as the input
  fingerprint matches; a step is its state's path plus a foot. Steps group a
  state's children by foot; they are not rows of the recorded tree.
- Children are **loaded lazily**: the panel asks for a row's children only
  when it is expanded, so a tree of 10⁵ rows opens at once.
- A step shows its test's counts: the sites accepted, and among the legs
  they reach the seating clashes (pruned only when `clash_filter` is on) and
  the mirror verdicts (pruned, undecided); its near misses. A leg shows its
  strain once relaxed and its own verdict; its rejections by reason (valence,
  no acceptor) are in its tooltip.
- **Duplicates are hidden by default** (decided 2026-10-08). A bond set
  reached in several orders is recorded under every parent that reaches it,
  but by default the panel shows it once, under its **canonical** path: the
  one the search kept (§4.6, §4.7). Each step counts what it hides
  ("1 dup. hidden"), so nothing disappears unseen. A **Show
  duplicates** toggle in the panel shows the others as "= duplicate of …"
  rows that jump to the canonical one, which makes the deduplication itself
  visible when debugging. Leg orders are tried exhaustively (every foot as
  leg 1, every remaining foot as leg 2, …; with numbered feet, §4.1, each
  state has one next foot), so on the stand-in tripod at
  tolerance 1.0 the leg-3 level has about 3,600 paths for 1,760 distinct
  hypotheses (§13.2): hidden, a user browses results; shown, the maintainer
  sees how each order was merged. The toggle is a **display option only**:
  it changes neither the search, nor the recorded tree, nor the fingerprint,
  and like the selection it is panel state, not saved and not an undo step.
- **The geometric levels exist before Run**: they are the plan, which is
  cheap. A user can tune the tolerance while browsing the tree, with no
  relaxation, and Run when it looks right. Relaxed rows and strains fill in
  after Run.

**Near misses.** Sites that fell outside a level's test by up to 1 Å (an
internal constant): outside `anchor_reach`, outside the shell or the torus,
or outside the local `reach`. They are recorded per row with their foot and
miss distance, counted on the step and listed in its tooltip and in the CLI's
description. They are **not coloured** (§18): they are the sites just outside
the step's drawn shape, which shows what raising a tolerance would add.

**Output pins.** Two appended pins (pin 2 and pin 3, after `candidates` and
`stats`); each can be shown in the viewport on its own, without wiring
anything downstream:

- `debug` (`Molecule`): the selected item as a structure, with per-atom colour
  overrides set by the node itself (the mechanism `apply_style` uses). Atoms
  keep their element colours unless the item marks them:

  | Selected item | Molecule shown | Marked |
  |---|---|---|
  | root | the posed molecule | nothing |
  | a leg (a state) | relaxed when it was relaxed, else seated (a one- to three-leg row by the θ rule or Kabsch, §4.5; a local row on its replayed parent) | its bonded pairs orange; seated, its own clashing atom pairs red |
  | a next foot (a step) | its state, in the form its test read: posed under the root, seated under one or two legs, relaxed below | the foot violet; the sites its test accepted green; the state's bonded pairs orange; the rest of the substrate transparent |

  A leg with both forms can be switched to the other in the panel. The
  transparency is per-atom alpha: the ghost rendering only desaturates,
  which does nothing to a grey silicon (§18). Bonded pairs use the orange
  highlight of `cs_changed`.
- `debug_shapes` (the isosurface type the `isosurface` node outputs): the
  selected step's test shape, transparent; a state draws none. It is the
  `anchor_reach` sphere around the posed foot, the inner and outer bounds of
  the leg-2 shell, the leg-3 ring (the overlap of its two shells, drawn as one
  field: the larger of the two shell distances), or the local `reach` sphere
  around the foot's relaxed position. Each is an analytic distance field
  sampled on a grid around the shape, rendered by the existing transparent
  isosurface path. **Showing or hiding this pin is the "show shapes" checkbox**;
  no property is needed. One pin carries one type, which is why the shapes do
  not share the `debug` pin.

**The root is selected by default** (decided 2026-10-08). Without a
selection, or when the stored selection no longer matches the fingerprint
(the inputs or settings changed since), both debug pins show the **root**:
the posed molecule, unmarked, with no shape (revised 2026-10-09, §18). The
view `anchor_reach` is tuned with is a step under the root (one foot, its
anchor sites, its sphere); while one is selected, evaluation rebuilds it for
the new inputs, so it follows `anchor_reach` live with no Run. These posed
views are the only debug structures **evaluation builds itself**: the posed
inputs plus colour overrides and an analytic sphere, with no seating and no
relaxation, so they are as cheap as `plan`. Every other item is built by the
select action below.

**The run model (R12 holds).** Evaluation never relaxes anything and builds
no debug structure except the posed views above:

- **Selecting a row is an API action**, `chemisorb_debug_select(node_id,
  path)`, like Run. It builds the debug structure and the shapes and stores
  them in the node data, keyed by the input fingerprint. Evaluation only
  outputs what is stored. Reading this from `eval()` is safe for the same
  reason the stored result is (old §7.4): a shared or outdated entry can only
  mismatch the fingerprint, never produce a wrong output.
- **Relaxed rows are not all stored.** Only the best `top_n` candidates keep
  their structures (bounded memory, old §11.4). Selecting any other relaxed
  row re-relaxes it, and for a local-phase row its relaxed ancestors, inside
  the action, as a short background job. The ancestors replayed are the
  **canonical parents** (§4.6), not the clicked row's own path: a
  deduplicated row may be reached by several binding orders, and only the one
  the search kept produced its geometry. With that, and relaxation being
  deterministic, the result is the one the search produced.
- **No structures are kept for the tree.** Every debug structure is rebuilt
  from the row's data on selection: seated ones by geometry alone (the
  start geometry is a function of the change set, §4.7), relaxed ones by the replay
  above. Only the selected row's structure is held, and selecting another row
  replaces it.
- **The engine records the tree** compactly while it enumerates and relaxes:
  per row its path, counts, near misses, strain and canonical parent, no
  structures, about 3 MB per 10⁵ rows. The geometric part is recorded by `plan`; the local phase
  appends its rows during Run.
- **The selection and the debug structures are not saved and are not undo
  steps**, like the Run result. A selection is UI state, not a search
  setting, so it is not fingerprinted either.
- **Headless:** `atomcad-cli` gets a debug-select command (node name and a
  path), so an AI agent can inspect a row, render it, and report what it sees.

---

## 7. Changes against the old design

| Old | What happens |
|---|---|
| R1 one search = one pose | **changed**: the pose fixes only where leg 1 lands (`anchor_reach`); orientation is searched |
| R3 `reach` | **changed**: split into `anchor_reach`, a `tolerance` and a local `reach` |
| R5 partial binding from one bond | **changed**: from two bonds (one only for a one-foot adsorbate) |
| R6 validity | kept; plus the mirror check (prunes by default) and clash detection after seating (reported always, prunes only with the optional `clash_filter`) |
| no geometric pruning of multi-bond patterns (the removed pair tolerance; crystolecule `AGENTS.md`) | **reversed, deliberately**: the shell and ring tests prune. They differ from the removed pair tolerance in being exact at tolerance 0 for any bond direction (the bond lengths cover the >2.5 Å flex that sank it), and the debug view's near misses make the tolerance tunable. The `AGENTS.md` bullet is rewritten in Phase 3 |
| R7 ranking by strain | kept; the reference is now the separated state (§4.8) |
| R8 dedupe by bond set | kept, keyed on the change set (bonds plus transfers); exact because the start geometry is a function of it |
| R9 output, filters as search settings | kept; new statistics and the `seating_clash` field |
| §7.2 `best` output pin | **removed** (§6.2); `candidates` becomes pin 0, `stats` pin 1 |
| — | **new**: the debug view, a tree in the panel and the `debug` / `debug_shapes` pins 2 and 3 (§6.5) |
| R10 determinism | kept |
| R11 budget | **changed**: counts relaxations across both phases; local levels may truncate |
| R12 Run model | kept |
| §5.2 all-at-once enumeration | **replaced** by §4 |
| §5.3 relax with UFF pulling the feet in | **changed**: rigid seating first (§4.5) |
| old §3 "site = atom, no plane or normal" | kept: the search uses only site positions and distances; the local up is used for seating and the mirror check only |
| §6.1 transfer enumeration | **replaced** by the rule (§5); `to_adsorbate` and `max_transfers` dropped |
| §7.4 exact plan count | kept for two and three legs; legs 4+ counted after Run |
| §9 orientation sampling | **done** by this design, without sampling |
| §11.4 bounded memory | kept for candidates (`top_n`); the local-phase frontier stores positions only |

---

## 8. Deferred

- **A `seeds` input pin**: already-bonded structures to continue the search
  from (a hand-picked or filtered leg-1 result). This is the useful part of
  mechadense's chaining idea, without making chaining the main path.
- **Equivalent-site deduplication**: recognise translational copies by
  comparing local environments under a rigid motion, so `top_n` is not filled
  with one motif repeated across the proxy.
- **Transfer search** beyond the rule: enumeration or seeded sampling, if the
  rule proves wrong somewhere UFF can tell the difference.
- **A swing limit or a tilt range**, if users ask (D3).
- ~~**A bond-divergence cap**~~ **Rejected 2026-10-08 after the spike**: the
  best three-leg tripod binding diverges by 100°, so a 90° cap would prune the
  best result (§13.5). Kept below for the record. (Considered 2026-10-08,
  deferred as not worth its property, tests and explanation before the spike
  showed a need.) An
  optional `max_bond_divergence`: the largest angle `θ` between two new
  bonds of one binding. It is relative between bonds, so it needs no surface
  normal. It replaces `bᵢ + bⱼ` in the shell and ring tests by the slack
  `wᵢⱼ = |bᵢ − bⱼ| + 2·min(bᵢ, bⱼ)·sin(θ/2)` (proof: with `bᵢ ≥ bⱼ`,
  `bᵢdᵢ − bⱼdⱼ = (bᵢ − bⱼ)·dᵢ + bⱼ·(dᵢ − dⱼ)` and `|dᵢ − dⱼ| ≤ 2 sin(θ/2)`),
  which keeps both tests exact within the cap. At 90° the slack is 2.33 Å for
  Si–O instead of 3.3 Å; for the stand-in tripod that removes roughly a third
  of the three-leg hypotheses. It only ever removes hypotheses, so it is a
  speed setting, never needed for correctness: default 180° (no cap), which
  is why adding it later changes no result and breaks no file. The spike's
  divergence measurement (§10 Q2) says whether it is needed and how low it
  could go.
- Everything in old §9 that this design does not cover: dissociation,
  bond-order changes, surface rearrangements, the `custom` op-library kind.

---

## 9. Why not chain one node per leg

mechadense proposed one `chemisorb` per leg, chained: each node takes an
array of states and outputs the states with one more leg bonded. It can be
built, but:

1. **Run model.** Each node has its own Run and stored result. Re-running
   node 1 makes every later node stale, so they must be re-run in order by
   hand, and none of it can sit in a `map` body (old §11.3).
2. **Memory.** Every level's frontier becomes a node output of full
   structures, cloned by the evaluator. That brings back the store-everything
   problem old §11.4 removed.
3. **It freezes the algorithm.** With the levels as nodes, the algorithm is
   written into saved networks, and changing it later (seating, dedupe across
   levels, a global budget) breaks files.
4. **It loses the global properties**: one budget, one truncation flag, one
   exhaustiveness claim, and one ranking per leg count or inventory, chosen
   with the filters (§4.8). Strains do not compare across leg counts, so the
   aim is not a single ranking across them.
5. **The user must know the algorithm** to wire it: sphere for leg 2, torus
   for leg 3, local after.

What mechadense really needs, seeing and steering the levels, is met by the
debug view (§6.5), the `formed_bonds` field and the per-level statistics, and by
`seeds` later (§8).

---

## 10. Open questions

1. **Legs 4 and later: local relaxation or geometric?** (a) As in §4.6:
   relax after three legs, then search locally. (b) Place the remaining feet
   by the three-foot fit, take sites within `reach` of those predicted
   positions, fit all bonded feet and relax once per hypothesis. Option (b)
   keeps the whole plan exact and needs no frontier. Option (a) is safer if
   the cage flexes a lot once bonded. The spike measures how far feet 4–6 move
   when a three-leg hexapod binding relaxes: if under `reach`, choose (b).
   **Decided 2026-10-08: (a).** Feet move up to 3.26 Å in the window, past
   the 3.0 Å `reach` (§13.7).
2. **Tolerance defaults; one tolerance or two; is the cap needed?**
   *Decided 2026-10-08 (§13.9): one `tolerance`, default 0.5 Å; no
   divergence cap (§13.5).* The
   spike calibrates both tolerances and checks how many relaxations the
   shells cost. It also **measures** the bond divergence (the angle between
   two new bonds of one binding) of every relaxed tripod and hexapod
   candidate inside the energy window, and reports the largest: the data
   for deciding whether the deferred cap (§8) is worth adding, and how low
   it could safely go. Both tests are exact at tolerance 0, so both
   tolerances mean only flex.
   If the cost ever matters, a tighter test
   also exists: predict each foot's position
   along its site's dangling bond (`s + b·n`, from the passivation code's
   open directions) and test those instead. It was set aside (2026-10-08)
   because it assumes the bond direction, which is doubtful on buckled dimers
   and step edges, and because the site-centred search is simpler.
3. **Is the Kabsch seating close enough?** The relaxed results must not show
   the tangles of old §10 Q5, and must converge within `max_iterations`.
   **Decided 2026-10-08: yes** (§13.3); `max_iterations` stays at 2,000.
4. ~~Replace the single-pose mode?~~ **Decided 2026-10-08: yes**, no
   compatibility mode (§6.4).
5. **`to_adsorbate` abstraction**: dropped for now. Is there a near-term use
   (a radical foot abstracting surface H while mounting), or can it wait for
   the transfer search (§8)?
6. ~~`legs` vs `formed_bonds`?~~ **Decided 2026-10-08: keep `formed_bonds`
   only**; it is the leg count, and no `legs` field is added (§6.2).
7. **Clash threshold and the two-leg θ rule**: internal constants. The spike
   picks them; should they ever be user-visible? And when may `clash_filter`
   default to on? Proposed criterion: on the stand-in tripod and hexapod, no
   hypothesis with `seating_clash` set relaxes into the energy window. The
   same criterion confirms the mirror check's default pruning (§4.5): no
   hypothesis it prunes relaxes into the window. The spike also sets its
   abstention margin (about sin 15°) and reports how many hypotheses it
   leaves undecided. **Decided 2026-10-08** (§13.4): both criteria met;
   mirror pruning on, `clash_filter` on by default; the constants stay
   internal (0.6 × covalent sum, θ step 10°, margin sin 15°).
8. **Ethylene known answer.** The end-bridge competitor sits on sites
   ~3.84 Å apart against a 1.54 Å C–C: a difference of 2.3 Å, inside the
   `b₁ + b₂ ≈ 3.8 Å` for two C–Si bonds, so the site-centred shell keeps it
   at tolerance 0.
   The test should assert that the end-bridge is still among the hypotheses.
   **Confirmed 2026-10-08** (§13.8). The test runs with `anchor_reach` 4.5
   (the old test's reach) and asserts the low-strain end-bridge; the two
   old end-bridges that need 2.79 Å relax to 750+ kcal/mol and are not
   asserted.
9. **Debug view details** (§6.5), unverified: can atom labels show arbitrary
   text (miss distances in the viewport, not only in the row's tooltip)? How
   coarse can the grid behind the transparent shapes be before the torus
   looks bad? Neither changes the design.

---

## 11. Testing

A search algorithm fails quietly. A bug rarely crashes; it drops a hypothesis
that should have been tried, seats a molecule a little wrong, or lets the
dedupe merge two different bindings, and the output still looks like a
plausible ranked list. Nobody notices a missing result. Example tests alone
do not guard against that, because the same understanding writes both the
code and the expected values. So the suite is built on **checks that do not
share the code's reasoning**: an independent oracle, bindings planted by
construction, invariances the output must respect, and every output
re-derived from the structure it claims to describe.

Synthetic fixtures only, as before (old §8): the stand-in tripod and hexapod,
never the real tools. Tests live in the crystolecule crate's `tests/`
directory. Randomised tests use a small seeded generator in the test helpers
(SplitMix64 or similar, no new dependency) with fixed seeds, and print the
failing seed and case so a failure reproduces. The `plan`-only tests are fast
and run hundreds of cases; tests that relax use small fixtures.

### 11.1 The oracle: naive enumeration (geometric phase)

A test-only enumerator, written separately and as plainly as possible: every
tuple of up to three distinct (foot, site) pairs on a small fixture, each leg
checked against the §4.2–4.5 conditions directly (anchor, shell, torus,
valence, frozen pairs, hydrogen, the mirror check), with the §5 transfer
rule applied in tuple order, then normalized and deduplicated by change set,
with no pruning, no ordering tricks and no loop structure shared with the
engine. On every fixture and on randomised small inputs, the set of
hypotheses `plan` produces must **equal** the oracle's, both ways: nothing
missing (the silent bug) and nothing extra. The per-level counts in the
statistics and the debug tree's rows must match the oracle's too.

The oracle shares the per-leg predicates with the engine (a second copy of
the same formulas would carry the same slip), so it tests the enumeration,
pruning and dedupe around them. The predicates themselves are tested by
§11.2 and §11.3.

### 11.2 Planted bindings: the predicates are sound

Ground truth by construction. Place the adsorbate rigidly at a random pose
over a slab, pick a random bond direction `dᵢ` for each of two or three feet,
and put a site atom at `Fᵢ − bᵢ·dᵢ` (free valence, conflicting atoms moved out
of the way). That binding is geometrically exact. The search, posed near it,
must produce the planted bond set **at tolerance 0**.

- The bond directions are swept over the full range the design claims to
  cover: any direction, including bonds tilted far from the local up and
  bonds tilted in opposite directions. The shell and the ring are exact at
  tolerance 0 by the triangle inequality (§4.3, §4.4); this sweep is what
  holds the code to that claim.
- With every bond direction along the local up, the Kabsch seating puts each
  foot on its exact position (residual ≈ 0, rotation determinant +1), and the
  relaxed planted binding appears among the candidates.
- Degenerate shapes are planted on purpose: feet closer than `b₁ + b₂` or
  `b₁ + b₃` (a shell's inner bound is negative), a third foot beyond the
  `F₁F₂` segment, nearly collinear feet (the ring collapses towards the
  axis; the mirror check abstains), a site with two free valences taking two
  feet, sites exactly on a bound (the tests are `≤`).

### 11.3 Hand-worked cases

A few fixtures small enough to work out on paper, with the expected
hypotheses written into the test by hand rather than computed: a three-foot
triangle over a toy substrate of about six sites, every accepted and every
rejected tuple listed with its reason. These tie the predicates and the
oracle to human reasoning. Plus the unit cases: shell and torus membership
just inside and just outside each bound; the local up of a flat slab is its
normal, on the open side; the two-leg θ rule picks the body-up side; one-leg
bindings appear only for a one-foot adsorbate.

### 11.4 Metamorphic tests: invariances

Each changes the input in a way whose effect on the output is known exactly:

- **Rigid motion.** Rotating and translating the whole input (substrate and
  adsorbate together) gives the same bond sets, the same statistics and the
  same strains to floating-point accuracy.
- **Pose independence of seating.** Seating depends only on the bond set
  (§4.7), so two runs from different poses that share a change set give it the
  same strain to floating-point accuracy (a tolerance far below any physical
  difference; the pose enters only through rounding). A seating that leaks
  pose information breaks this at once.
- **Relabelling.** Permuting atom ids gives the same results under the
  mapping, except where atom id breaks a tie: the order among exact strain
  ties, and the transfer acceptor between equidistant sites (§5). Fixtures
  for this test avoid equidistant acceptors.
- **Foot order.** The seating of a bond set is the same whatever order its
  feet were bonded in.
- **Monotonicity.** Raising `anchor_reach`, either tolerance or `reach`
  never removes a hypothesis from the plan. A row's near misses are exactly the
  plan at the tolerance plus 1 Å minus the plan at the tolerance.
- **Blocking.** Putting an H on a site removes exactly the hypotheses that
  bond to that site; the others survive, except that one whose H went to
  that site now takes the next-nearest acceptor, or is dropped as
  `pruned_no_acceptor` if none is within `reach`.
- **Filters commute.** A search with `formed_bonds` or `bond_inventory` set
  gives exactly the unfiltered search filtered afterwards, bond sets and
  strains, with `top_n` and `budget` large enough to keep everything. This
  checks that filtering during enumeration loses nothing.
- **Budget.** A truncated run relaxes a prefix of the plan order and reports
  the truncation.
- **Determinism.** The same search on a one-thread pool and on a many-thread
  pool gives identical output: candidates, statistics and tree.

### 11.5 Every output re-derived

An `audit` helper runs on each candidate of every test that relaxes, and
recomputes from the structure instead of trusting the hypothesis:

- The bond changes, read off by comparing the candidate's bonds with the
  inputs', equal the claimed bond set plus the rule's transfers; the bond
  inventory recomputed from them equals the claimed one; `formed_bonds`
  equals the number of foot bonds.
- Every formed bond is near its rest length after relaxation (within an
  internal fraction): a bond to the wrong atom, or a seating UFF could not
  repair, shows up here.
- No atom exceeds its valence; frozen atoms did not move; no bond joins two
  frozen atoms; atoms and elements are conserved.
- The strain, recomputed as the UFF energy of the candidate structure minus
  the reference energy, equals the recorded strain.
- The statistics add up: every hypothesis is counted exactly once, as pruned
  (by reason), a duplicate or relaxed; the relaxations `plan` predicted equal
  the UFF calls made when the budget is not hit.

### 11.6 Rule and feature tests

- **Clash detection and the filter:** a body-through-substrate (upside-down)
  seating is detected; an ordinary close contact and the excluded atoms
  (feet, their sites, atoms within two bonds of a foot, hydrogens) are not;
  with `clash_filter` off, clashing hypotheses are relaxed, listed with
  `seating_clash` set and counted in `seating_clashes`, and `pruned_clash` is
  0; with it on, exactly those hypotheses are pruned and counted in
  `pruned_clash`; the property is fingerprinted.
- **Mirror check:** each planted binding of §11.2 is also planted in its
  mirrored assignment (the same sites, two feet swapped); the check calls the
  proper one proper and the mirrored one mirrored, and abstains only inside
  its margin (planted collinear feet, a body in its foot plane, a steep site
  triangle); undecided hypotheses are relaxed; with `û` rotated up to just
  under 90° from the slab normal, the verdicts do not change; pruned and
  undecided hypotheses are counted in `pruned_mirror` and
  `mirror_undecided`; on the fixtures, every mirror-pruned hypothesis would
  also be flagged by the clash check.
- **Transfer rule:** the H goes to the nearest free site to its foot's site;
  ties go to the lower atom id; a site with two free valences never takes
  its own foot's H; no free site within `reach` of the foot's site (site to
  site) drops the hypothesis, whatever the seating; `to_adsorbate` is an
  error. **Fixed per leg:** in the local phase, a child keeps every
  transfer of its parent unchanged and adds only its own leg's; a site
  taken by an earlier leg's H is never bonded by a later leg; two binding
  orders that send an H to different sites give two hypotheses with
  different change sets, and two that send it to the same site collapse;
  the transfers are fixed in `plan`, before any relaxation.
- **What is relaxed:** with `formed_bonds = 3`, no two-leg hypothesis is
  relaxed and the plan's relaxation count matches the UFF calls made;
  unfiltered, every two-leg hypothesis is relaxed and listed even when
  three-leg bindings extend it; a local-phase parent outside the filter is
  relaxed but never appears in `candidates`.
- **Local phase:** each level's hypotheses equal a naive scan of the sites
  within `reach` of each relaxed parent's free feet (the §11.1 oracle,
  extended one level); dedupe of two binding orders keeps the lower strain
  and records it as the canonical parent; level truncation is reported; after
  Run no frontier memory is held.
- Node, API, CLI and round trips as in old §8.4, for the new properties and
  statistics.

### 11.7 Known answers, the old engine, regression snapshots

- **Known answers:** water (one foot, H by the rule onto the dimer partner,
  which now holds by construction, and the guide must say so); ethylene di-σ
  against end-bridge (§10 Q8): the end-bridge is among the hypotheses.
- **Differential test against the old engine, same pose.** In Phase 0, while
  the old engine still exists, run it on the fixtures and store its bond sets
  and strains as golden data. From the same pose, with `anchor_reach` at least
  the old `reach` and the default tolerance, every old bond set of two or
  more bonds **within the energy window** must be among the new hypotheses
  (the old engine enumerated every combination, including geometrically
  impossible ones, which the shells rightly drop). Same pose, so no
  translation matching is needed. On fixtures with transfers, a missing bond
  set is a failure unless it is the known limit of §5 (an earlier leg's H
  took the site in every binding order), which the test checks and reports
  rather than fails.
- **Coverage against the old brute force** (the acceptance test of the whole
  design): the stand-in tripod in **one** run must find every pattern within
  the energy window that the 20-pose brute force of old §8.6 found, compared
  **up to the slab's lattice translations**: the test helper maps each bond
  set to a canonical translate using the fixture's lattice vectors. That is
  test code only; the engine still does not recognise equivalent sites (§8).
  The brute-force golden data is captured in Phase 0 too (the §8.6 test was
  removed, so it is rerun from git history or rewritten).
- **Regression snapshots** (`insta`): for the stand-in tripod, the hexapod,
  water and ethylene, the plan statistics and the top candidates (bond sets,
  strains rounded to 0.01 kcal/mol). Any change to the algorithm then shows
  up as a snapshot diff to review instead of slipping through.

### 11.8 Debug view

The recorded tree matches the plan and the oracle (every hypothesis is one
row's path; duplicate rows point at the canonical one); with duplicates
hidden (the default) each hypothesis is listed exactly once, under its
canonical path, and every parent's hidden count equals the duplicate rows the
toggle reveals under it; toggling changes no recorded row, no fingerprint and
no search result; the children of a row
are what its level's test accepts, and its near misses follow the
monotonicity rule of §11.4; selecting a row before Run builds the seated
structure with the documented markings, and no UFF call is made; after Run,
rows open per the seated-or-relaxed rule (a two-leg candidate with three-leg
children opens seated, a two-leg leaf candidate and every relaxed three-leg
row open relaxed, a clash-pruned row opens seated); selecting a relaxed row
outside `top_n` reproduces the strain the search recorded, including a
local-phase row reached by two binding orders whose clicked path is not the
canonical one; with no selection, and after a fingerprint mismatch, both debug pins
show the root view (posed molecule, feet, anchor sites, `anchor_reach`
spheres), built by evaluation with no UFF call and following `anchor_reach`
live; the selection
is not saved, not undoable and not fingerprinted; the CLI command matches the
API. The panel tree itself is part of the manual walkthrough.

### 11.9 Do the tests catch bugs?

Once per engine phase, by hand: for each predicate and each dedupe, seating
or transfer step, plant a deliberate bug (an off-by-one bound, `b₁` for `b₂`,
`d₁₃` for `d₂₃` in the ring's second shell, a sign flip in `σ_s`, a dedupe key
that drops the foot, a lost transfer) and
confirm that at least one test fails. A mutation no test catches means a
missing test, added before the phase closes. The mutations tried are listed
in the phase's commit message.

---

## 12. Phases

0. **Spike** (outside `main`, e.g. an ignored test on a branch). The stand-in
   tripod against the old §8.6 brute-force data: coverage, tolerance
   calibration, seating quality (Q2, Q3, Q7). The stand-in hexapod: how far
   feet 4–6 move after a three-leg relaxation (Q1). Relax the clashing
   seatings and the mirror-pruned hypotheses too and report where they land
   (Q7). Report the relaxation
   counts against the old search. The spike decides the open questions before
   anything lands on `main`. **Capture the old engine's golden data** while it
   still exists: its bond sets and strains on the fixtures from one pose, and
   the 20-pose brute force (§11.7). Report the hypothesis counts per level
   (three-leg before and after the mirror check) and the measured bond
   divergence of the relaxed candidates (Q2).
1. **Engine, geometric phase.** Feet, sphere, the two-shell ring, local up,
   the mirror check, seating, clash check, separated reference, the transfer
   rule, `plan` statistics. Promote
   `kabsch`. **Record the search tree** (rows, counts, near misses) from the
   start, since it is nearly free while enumerating and the debug view needs
   it. Tests: the oracle (§11.1), planted bindings (§11.2), hand-worked
   cases (§11.3), the metamorphic tests (§11.4), the `audit` helper on every
   relaxing test (§11.5), the rule and known answers, the old-engine
   differential and coverage tests and the snapshots (§11.7). **Time `plan`**
   on the stand-in tripod and hexapod with seating, clash and mirror checks
   included. If it is too slow for every evaluation, move those per-row
   results out of `plan`, as §4.9 describes. The mutation check (§11.9)
   closes the phase.
2. **Engine, local phase** (legs 4+, local relaxation per Q1), also
   recording its rows. Tests for the local phase and the budget, the oracle
   extended to the local levels (§11.6), the hexapod snapshots; the mutation
   check again.
3. **Node.** New properties, statistics and the `seating_clash` field, removal of
   `max_transfers` and of the `best` pin (fixtures and tests moved to the new
   pin indices), panel (the live plan count, the "+ local phase" line,
   greyed `reach`), the reference-guide page `nodes/atomic.md#chemisorb`, and
   the AGENTS.md invariants: in crystolecule "Chemisorption search", rewrite
   "No geometric pruning of multi-bond patterns" (§7), "Transfers are
   enumerated before bond forming" (now the per-leg rule, §5) and "against
   the same pose relaxed unbonded" (now the separated reference, §4.8), add
   the mirror check and the per-leg change set, and point the design-doc
   reference at this document; also the `chemisorb` bullet in
   `nodes/AGENTS.md`. The repository-side scrubbed copy of this document.
4. **Debug view** (§6.5): the tree panel with lazy loading, the
   `chemisorb_debug_select` action and its background re-relaxation, the
   `debug` and `debug_shapes` pins, the CLI command, the reference-guide
   section. Then use it to calibrate the default tolerance on the real
   tripod, outside the repository.
5. **Manual walkthrough** of the panel, Run and the debug view (maintainer),
   after a release build.

---

## 13. Phase 0 results (2026-10-08)

The spike is on atomCAD branch `chemisorption-sequential` and is not
committed yet. It consists of the ignored tests in
`rust/crates/atomcad-crystolecule/tests/crystolecule/chemisorption_sequential_spike_test.rs`,
plus one engine change: `mechanosynth::fit` and its `rigid_fit` are made
`pub` (Phase 1 promotes them properly). Run it with:

```
cargo test -p atomcad-crystolecule --release -j 4 --test crystolecule spike_ -- --ignored --nocapture --test-threads 1
```

The prototype covers the geometric phase only: anchor, shell, two-shell ring,
local up, mirror check, Kabsch seating, θ rule (10° steps), clash detection
(0.6 × covalent sum) and the separated reference. The transfer rule (§5) is
not prototyped, because no open question depends on it. Fixtures are the
§8.6 ones: the stand-in tripod and hexapod over `si100_slab(5, 11)`, with the
feet 1.8 Å above the dimer layer. Strains below are against the separated
reference. `b(O–Si)` = 1.716 Å (UFF), and the foot spacing is 5.04 Å.

### 13.1 Golden data of the old engine

The golden data is
`tests/crystolecule/chemisorption_golden/old_engine.json` (470 KB). It
records every relaxed candidate in input ids: formed bonds, site positions,
transfers, inventory, strain, absolute energy, convergence and worst bond
ratio. It covers four runs:

- the tripod 20-pose brute force: 442 relaxations, all converged, 31 s;
- the hexapod at pose 0: 127 relaxations;
- ethylene: reach 4.5, 22 relaxations;
- water: reach 4.5, H `to_substrate`, 26 relaxations.

Run the capture test again only if the fixtures change.

### 13.2 Tripod: counts, coverage, calibration

The setup has 155 sites, of which only **40 are on the top face**. The
rest are on the slab's bare side and bottom faces, which the thick shells
reach through the slab (§13.6). Counts per level, with `anchor_reach` 3.5
and both tolerances equal:

| tol | anchors | 2-leg | 3-leg | proper | mirrored | undecided | relaxed (2 + 3 accepted) |
|---|---|---|---|---|---|---|---|
| 0.0 | 6 | 209 | 677 | 283 | 279 | 115 | 607 |
| 0.5 | 6 | 263 | 1148 | 496 | 489 | 163 | 922 |
| 1.0 | 6 | 315 | 1760 | 688 | 684 | 388 | 1391 |
| 2.0 | 6 | 435 | 3890 | 1360 | 1357 | 1173 | 2968 |
| 3.0 | 6 | 565 | 6756 | 2409 | 2409 | 1938 | 4912 |

The old brute force made 442 relaxations over 20 poses, about 22 per pose.

**Coverage** is measured from one run at pose 0, against the brute force's
patterns of two or more legs, mapped up to the slab's lattice translations
(13 translations found). There are 84 distinct patterns: 44 with two legs
and 40 with three.

- **Within the window, everything is covered at tolerance 0**, at every
  `anchor_reach` tried (3.5, 4.5 and 5.5). That is 22 of 22 patterns in the
  per-leg-count window (30 kcal/mol above the best of the same leg count) and
  3 of 3 in old §8.6's per-pose window. The tolerance the windowed patterns
  need is **0.00 Å, maximum**: the bond lengths in the shell tests cover
  every flex the low-strain bindings use.
- Across all 84 patterns, including the ones hundreds of kcal/mol up,
  coverage is 44 at tolerance 0, 75 at 1.0 and 81 at 2.0. With
  `anchor_reach` 4.5 it reaches 84 at 2.0. The patterns missed at the
  defaults are all outside the window.
- The same-pose differential passes: at pose 0, both old bond sets of two or
  more legs within the window are among the new hypotheses.
- The enumerator agrees with an analytic per-pair check on every pattern, at
  every setting. This is a first check of the §11.1 oracle idea.

**The sequential search finds better bindings than the brute force did.**
Its best three-leg binding has strain **97.7**, against **116.6** for the
best three-leg binding of all 20 brute-force poses: 19 kcal/mol lower,
because orientation is now searched. The best two-leg binding (6.5) and the
best one-leg reference point match.

### 13.3 Seating quality (Q3)

- **Convergence** within 2,000 iterations: 1 of 315 two-leg and 10 of 1,076
  accepted three-leg hypotheses fail to converge (p50 about 300 iterations).
  On the hexapod, 21 of 4,200 fail.
- **No tangles in the window.** Every candidate within the window has its
  formed bonds within 1.027 × rest length. The 336 three-leg relaxations
  that stretch a formed bond beyond 1.25 × rest all rank hundreds of
  kcal/mol up.
- **Seating residuals are large but harmless:** p50 2.3 Å and maximum 3.3 Å
  for three legs. The site triangle seldom matches the foot triangle, and UFF
  absorbs the difference.
- **Seating reaches the old minimum.** For the same pose and the same sites,
  the sequential energy equals the old engine's to 0.00 kcal/mol (13
  patterns). Over translated copies of the 22 windowed patterns, the best
  translate lies within −0.8 to +2.9 kcal/mol of the brute force (p50 0.0),
  which is proxy-edge noise. So the pose-independence claim (§4.7, §11.4)
  holds in practice.

### 13.4 Mirror check and clashes (Q7)

- **Mirror check:** 684 three-leg hypotheses are pruned as mirrored, and
  **none relaxes into the window**. The closest lands **329 kcal/mol** above
  the best accepted three-leg binding. 683 of the 684 also clash when seated,
  so §11.6's "every mirror-pruned hypothesis would also be flagged by the
  clash check" holds with one exception; Phase 1 should look at that one.
  The check left 388 of 1,760 undecided at the sin 15° margin. **Prune by
  default is confirmed.**
- **Clashes:** 73 of 315 two-leg and 480 of 1,076 accepted three-leg
  seatings clash. **None lands in the window.** The closest are 757 and 277
  kcal/mol above the best. By §10 Q7's own criterion, **`clash_filter` may
  default to on**. With it on, the tripod run drops from 1,391 to 838
  relaxations. The 0.6 fraction and the 10° θ step are adequate as internal
  constants.

### 13.5 Bond divergence and the cap (Q2)

Bond divergence is the largest angle between two new bonds, measured on
relaxed candidates within the window:

| fixture | legs | p50 | max |
|---|---|---|---|
| tripod | 2 | 27° | 53° |
| tripod | 3 | 87° | **100°** |
| hexapod | 3 | 73° | 78° |

The best three-leg tripod binding diverges by 100°, so a 90° cap would
prune the best result. **The cap is not supported by the data.** Keep it
deferred (§8), or drop it.

### 13.6 Cost: what the shells admit

The shells are as thick as §4.3 predicted. At tolerance 1.0, the
geometric phase needs 1,391 tripod relaxations against 442 for the 20-pose
brute force: 126 s against 31 s. Two levers reduce this, and both are
already in the design:

- **Tolerance 0.** It covers the whole window, and gives 607 relaxations.
- **`clash_filter` on.** This takes 553 off at tolerance 1.0.

The rest of the excess comes from the fixture. 92 of the 315 two-leg
hypotheses use a side-face or bottom-face site, which the 8–9 Å shells
reach through a 1.5-cell slab. A real proxy (`proxy` caps severed bonds)
or a `substrate_tag` on the top face removes them. The reference guide
should advise tagging the facet.

### 13.7 Hexapod, legs 4 and later (Q1)

At tolerance 1, the hexapod gives 736 two-leg and 7,504 three-leg
hypotheses (3,366 proper, 3,304 mirrored, 834 undecided); enumeration takes
0.09 s. All 4,200 accepted three-leg hypotheses were relaxed (182 s). The
measurement compares each unbonded foot's position after Kabsch seating with
its position after relaxation:

| set | foot move p50 | p90 | max | site sets within 3 Å differ |
|---|---|---|---|---|
| all 4,200 | 1.09 Å | 2.66 Å | 8.25 Å | 35 % of feet |
| 20 in the window | 1.71 Å | 3.25 Å | 3.26 Å | 56 of 60 feet |

In the window, the relaxed feet's sites are a **subset** of the seated
feet's: 0 sites only near relaxed, 82 only near seated. In the window,
option (b) would therefore miss nothing and only add hypotheses. Outside the
window, both directions differ.

§10 Q1's criterion ("if under `reach`, choose (b)") **fails at the
maximum**: 3.26 Å against a 3.0 Å reach. **Recommendation: keep (a), local
relaxation**, as §4.6 has it. An option (b) with a slightly larger reach
would also work on this fixture.

### 13.8 Ethylene (Q8)

At `anchor_reach` 3.5 and tolerance 0, 10 of the old engine's 14 two-bond
bindings are hypotheses. All 4 misses are anchor-reach misses (the nearest
foot–site distance is 4.35 Å; the old run used reach 4.5):

- two are di-σ copies on more distant dimers;
- two are end-bridges that need 2.79 Å of tolerance and relax to 750–800
  kcal/mol.

**Every physical end-bridge is kept.** The known answer holds: di-σ 39.8
against end-bridge 43.2, the same as the old engine's 39.5 / 43.2.

### 13.9 Decisions (all accepted by the maintainer, 2026-10-08)

1. **Q1:** keep the local phase (a).
2. **Q2: DECIDED 2026-10-08.** The two tolerances are merged into one
   `tolerance`, **default 0.5 Å** (§6.1). It is kept, although 0 covers the
   whole window on the stand-in, because non-rigid adsorbates can change
   their foot spacing. No divergence cap.
3. **Q3:** Kabsch seating is good enough. Raising `max_iterations` is not
   needed.
4. **Q7:** keep mirror pruning on by default, and default `clash_filter` to
   **on**.
5. **Q8:** the end-bridge is among the hypotheses. The test uses
   `anchor_reach` 4.5 and asserts the physical end-bridge.
6. **`anchor_reach`:** default 3.5 Å. It suffices for coverage up to
   translation; 4.5 Å changes nothing within the window.

---

## 14. Phase 1 results (2026-10-08)

On atomCAD branch `chemisorption-sequential`, committed as e4140a57. The
engine is `rust/crates/atomcad-crystolecule/src/chemisorption/sequential/`
(`config`, `setup`, `plan`, `tree`, `evaluate`), **beside** the old engine:
the `chemisorb` node still runs the old one until Phase 3 switches it and
deletes the old code. Kabsch is promoted: `mechanosynth/fit.rs` became the
crate-level `rigid_fit.rs`, shared by mechanosynth and the seating. The
crystolecule `AGENTS.md` has the module map, the test list and a short note;
the full rewrite of its chemisorption bullets stays in Phase 3.

### 14.1 What is built

Feet and sites (§4.1), the anchor, the shell and the two-shell ring
(`Setup::pair_need`, exact at tolerance 0), the transfer rule fixed per leg
(§5), the local up, the mirror check, seating (translation / θ rule /
Kabsch), clash detection and the clash filter, the separated reference, the
filters as enumeration-time pruning, `PlanStats` with its add-up invariants,
and the search tree (rows, counts, near misses, canonical rows, a child index
ready for lazy loading). `evaluate` relaxes only candidates (§4.8) and keeps
every relaxation's strain per hypothesis (`RelaxedRow`) for the debug view.
`PlanStats::local_phase` says when a local phase would follow; nothing
relaxes parents yet (Phase 2).

Decisions taken while implementing, within the design's latitude:

- A foot with a free valence bonds with it; it donates an atom only when it
  has none of its own (an OH oxygen). The donated atom must have one single
  bond and be unfrozen. With several donatable atoms (water's two H), the one
  that moves is the one nearest its acceptor once seated.
- One-leg seating (a one-foot adsorbate) translates the posed molecule: the
  orientation is the pose's, as §6.5 already says for display.
- Bond lengths are UFF single-bond rest lengths of bare-atom types (O–Si
  1.716, C–Si 1.867 Å, as in the spike).

### 14.2 Findings

1. **The mirror check was reading rounding noise on collinear sites.** Site
   triples along one dimer row are exactly collinear; their normal is noise
   and the verdict depended on the order of the legs. About 31 of the
   spike's 279 "mirrored" verdicts at tolerance 0 were such noise. Now nearly
   collinear **feet** abstain at the sin 15° margin (an order-free triangle
   quality, `2√3·|N| / Σ|edge|²`), and seating points abstain only when
   degenerate to rounding (quality < 1e-6). With that, **every mirror-pruned
   hypothesis also clashes** on the tripod and hexapod at tolerances 0, 0.5
   and 1 (the spike's single exception was one of the noise verdicts).
   Tripod at the defaults: 459 mirrored, 223 undecided (spike: 489 and 163).
2. **Kabsch is undetermined on collinear seating points**, the same sites:
   every turn about their line fits equally well and the eigen-solver returns
   an arbitrary one. Found by the rigid-motion test (seated atoms up to 11 Å
   apart). §4.5's "with three non-collinear points it is unique" assumed the
   case away. `seat` now turns about that line by the θ rule.
3. **The θ rule as written was pose-dependent**: its θ = 0 came from a
   shortest-arc alignment that depends on the pose, so the 10° grid fell on
   different angles for two poses of the same bond set, breaking §4.7. Now
   θ = 0 is the body-straight-up turn (from the molecule and the sites only)
   and the angles are tried 0, ±10°, ±20°, …; the first clash-free one wins.
   That is still "the highest clash-free angle", since the height falls with
   |θ|, and it is 2.5× faster (item 9).
4. **Seating reads the legs sorted** (local up, fit, θ rule); in binding
   order the start geometry differed by rounding between orders.
5. **`pair_need` absorbs 1e-9 Å of rounding.** A site exactly on a shell
   bound (planted opposite bonds, or an ideal lattice) otherwise failed at
   tolerance 0 by 1e-16 Å.
6. **H transfer without an adsorbate tag makes every C–H carbon a donor
   foot**, as the old review feared: a methanol's methyl C becomes a foot.
   The tests tag the O feet. The reference guide must say so (Phase 3).
7. **Ethylene: the wider sphere reaches a fixture artefact.** At
   `anchor_reach` 4.5 the best two-leg binding (32.4 kcal/mol) bonds a dimer
   atom to a frozen, unreconstructed rim atom with two dangling bonds, 0.19 Å
   above the dimer layer, which the old engine could not reach from the
   pose. With the dimer atoms tagged as sites the known answer holds: di-σ
   39.45 against end-bridge 43.22 (old engine 39.5 / 43.2). More reason for
   the guide's "tag the facet" (§13.6).
8. **An upside-down cage clashes only a handful of times** (3 pairs in the
   test case) at 0.6 × the covalent sum, not "many times over" (§4.5). It is
   still caught, which is all the filter needs.
9. **Timing (§4.9, the Phase 1 decision).** `plan` in release, defaults,
   seating + clash + mirror included: tripod 0.04 s, hexapod 0.05 s (0.14 s
   before item 3's early stop); debug ~3 s on the hexapod. **The per-row
   results stay in `plan`.** Threshold: 0.25 s on the hexapod in release
   (`plan_is_fast_enough_for_every_evaluation`, an ignored test; it reads
   0.15 s when it shares the CPU with the relaxing snapshot test).
10. **Counts at the defaults** (tripod, `anchor_reach` 3.5, tolerance 0.5,
    clash filter on): 6 anchors, 263 two-leg, 1,148 three-leg, 952
    candidates, 235 clashing, **717 relaxations**. Hexapod: 10 / 594 /
    4,652, 3,563 candidates, 2,831 relaxations (2,315 with
    `formed_bonds = 3`). Best three-leg tripod binding 97.68 kcal/mol (spike
    97.7); all 502 three-leg relaxations converged.

### 14.3 Tests

Four files in `tests/crystolecule/` (`chemisorption_sequential_*`): 58 tests
in debug (~45 s), plus three ignored release-only ones (timing, and the
stand-in candidate snapshots, ~3 min).

- **Oracle (§11.1)**: a naive tuple enumerator sharing only the per-leg
  predicates; equal to `plan` both ways (verdicts, candidate flags, level
  counts) on the tripod (tol 0, 0.5), the hexapod, ethylene, water, a
  competing-H case and **300 random cases** (2–4 feet including OH donors,
  5–10 sites with one or two valences, random frozen atoms and settings, then
  again with an inventory filter drawn from the result).
- **Planted (§11.2)**: 200 random poses with fully random bond directions
  (two and three legs) found at tolerance 0; bonds along the body normal seat
  exactly (residual < 1e-6, det +1, local up = normal, proper); mixed O/C
  feet at both shell bounds; ethanediyl; two feet on one SiH₂; a third foot
  beyond the segment; nearly collinear feet (undecided).
- **Hand-worked (§11.3)**: a 6 Å foot triangle over five silyls, every
  accepted tuple listed by hand (3 / 15 / 7, 5 mirrored, 12 duplicates, one
  near miss of 0.267 Å), plus the unit cases.
- **Metamorphic (§11.4)**: rigid motion and relabelling (plans and stats
  identical, strains within 1e-3), seating independent of binding order (bit
  for bit) and of pose (1e-6 Å; strains 1e-3), monotonicity in all three
  reaches, near misses = what one more ångström adds, blocking a site and an
  acceptor, filters commute (plan and strains), budget prefix, one thread =
  four threads. Strain invariance uses the tripod over the real slab with a
  few tagged sites: on loose fragments (separate methoxys over separate
  silyls) relaxations end 0.5 kcal/mol apart on flat landscapes, which says
  nothing about the engine.
- **Audit (§11.5)** on every relaxing test's candidates.
- **Rules, tree, known answers (§11.6–11.8)**, and the **golden data**:
  same-pose differential (tripod 7, hexapod ≤ 3 legs 14, ethylene 8 windowed
  old bindings, all found; water: every old O–site bond found with the rule's
  acceptor, 16 of 26 old acceptors collapsed by the rule; old pure transfers
  out of scope) and **the acceptance test: one run at the defaults covers all
  144 per-pose and 138 per-leg-count windowed bindings of two or more legs of
  the 20-pose brute force**, up to the slab's lattice translations. Windows
  are per leg count (the old runs' mixed windows are almost all one-leg
  bindings). `insta` snapshots of the plan statistics (tripod, hexapod,
  ethylene, water) and of the top candidates (ethylene, water; tripod and
  hexapod release-only).

**Mutation check (§11.9)**, one planted bug at a time, all 24 caught: shell
bound off by 0.05 Å; `b₁` for `b₂` in the slack (missed at first — both feet
anchored, so the other binding order hid it; the mixed-element plant now
anchors one foot only); ring against leg 1 only; sign flip in σ_s; mirror
margin dropped; dedupe key without the foot; a lost transfer; acceptor may be
its own site; tie to the higher id; acceptor reach ignored; local up flipped;
θ rule body down; improper Kabsch; collinear seating not turned; seating in
binding order; one-leg candidates for many feet; inventory prune too strict;
valence not checked; clash fraction 0.4; clash filter ignored; frozen pairs
may bond; near-miss band halved; reference = substrate only; duplicates not
counted on the parent.

### 14.4 Left for later phases

- Phase 2: the local phase; `local_phase` is only a flag today.
- Phase 3: the node, the fingerprint of `SequentialSearch`, removing the old
  engine and the spike file (its capture test needs the old engine), the
  `AGENTS.md` bullets, the guide (tag the facet; tag OH feet with an H rule).
- §4.5's wording should be corrected for items 1, 2, 3 and 8 when the
  scrubbed copy goes into the repository.

---

## 15. Phase 2 results (2026-10-08)

On atomCAD branch `chemisorption-sequential`, committed as 64a71b1d. New file
`sequential/local.rs`; `evaluate` runs it after the geometric relaxations.

### 15.1 What is built

- **Parents in `plan`.** Every unmirrored three-leg hypothesis is a parent
  when a local phase follows (`Hypothesis::parent`); it is seated and
  relaxed even when the filters do not admit it, and the clash filter
  prunes it like a candidate. `PlanStats::parents` counts the parents that
  are not candidates, so `candidates + parents == to_relax + pruned_clash`.
  `SequentialPlan::max_legs` is the leg cap (feet, `max_formed_bonds`,
  `formed_bonds`, the inventory's legs). `plan`'s relaxation count is
  therefore exact for the geometric phase, as §4.9 says.
- **The local phase**, breadth first. A level's frontier holds, per relaxed
  state, only its strain and the positions of the movable atoms (every
  adsorbate atom and every unfrozen substrate atom, `Setup::movable`). For
  each unbonded foot, every site within `reach` of the foot's relaxed
  position (the site's relaxed position too) is a path; valence, the
  transfer rule (site positions as input, so the same rule as `plan`), the
  inventory prune and the frozen-pair rule as in the geometric phase. Near
  misses and rejections are recorded on the parent row.
- **Relax, then dedupe.** A child's start geometry is its relaxed parent
  plus its bond (and its H, seated by `Transfer::seat` from where it sits in
  the parent): `Setup::grown_structure`. Every path that needs relaxing
  (a candidate, or a parent of the next level) is relaxed; then each
  change set keeps its lowest strain (ties: the earlier row), whose row
  becomes canonical and whose parent is the canonical parent. Paths of
  the same set are marked duplicates on the tree and counted on their parent.
- **Budget.** The geometric phase spends first; each level gets what is
  left. A level sorts its paths by (parent strain, change set, row) before
  truncating, so the children of the best parents are relaxed first, and
  reports `truncated`.
- **Report.** `SearchReport` gains `tree` (the plan's tree plus the local
  rows), `local` (the local hypotheses, numbered after the plan's:
  `SearchReport::hypothesis`), and `SearchStats::{local: Vec<LevelStats>,
  local_relaxed, truncated}`. `RelaxedRow` gains `row`, so a duplicate
  path's relaxation is listed too. Nothing of the frontier survives
  `evaluate`.
- **`replay(plan, report, config, row)`**, ahead of Phase 4: rebuilds any
  row's relaxed state, a local row through its own path's parent (always a
  canonical row, since only kept states grow). The tests use it as the
  oracle's source of parent geometry; it reproduces every recorded strain
  bit for bit (asserted to 1e-9).
- Progress: each level renames the phase ("Local phase: leg 4") and adds
  its relaxations to the job total.

### 15.2 Findings

1. **A rebuilt state must apply its bond changes in the search's order.**
   A candidate's structure is rebuilt from positions after dedupe
   (`Setup::state_structure`, step by step, bond before transfer); the
   child start uses the same function for its parent, so the two agree and
   the audit's strain recomputation holds.
2. **Six-leg bindings of the stand-in are very strained.** Full slab,
   defaults, `formed_bonds = 6`: 9,682 relaxations (3,741 / 2,776 / 613 in
   levels 4–6), not truncated, **12.6 min in release**; the best six-leg
   binding is 348 kcal/mol (three-leg: 59.8). The local phase is the
   expensive part (7,130 of the relaxations); the facet tag is the lever
   (7 Å of dimers: 2,604 relaxations, best 400 kcal/mol, in the snapshot).
3. **The default budget (10,000) is close.** Unfiltered, the same run would
   relax the two-leg candidates too; it was not run to the end. The guide
   should say so (Phase 3).
4. **Local candidates have no seating**, so `seating_clash` is false for
   them; the clash filter applies to the geometric phase only.

### 15.3 Tests

New file `chemisorption_sequential_local_test.rs`: 11 tests in debug
(~50 s; a five-foot planted run is shared through a `OnceLock`), plus one
release-only test (the planted hexapod, three local levels), and a
release-only `insta` snapshot of the hexapod over a tagged Si(100) facet
(`sequential_local_hexapod`, level statistics and top candidates). Fixture:
the stand-in cage with 4–6 feet over a planted silyl under each foot
(frozen Si, free H), optionally a decoy site.

- The planted five- and six-leg bindings are found; `formed_bonds = 5`
  relaxes the three- and four-leg states as parents and lists none of them.
- **The oracle per level**: for a sample of each level's parents (replayed),
  a naive scan of sites within `reach` of the relaxed feet equals the
  recorded children, both ways, and the near misses equal what one more
  ångström adds.
- **Dedupe**: every local duplicate's strain ≥ its canonical row's; the
  duplicate's relaxation is listed under the kept hypothesis; replaying a
  duplicate and its canonical row gives each its own recorded strain; the
  canonical replay equals the listed candidate's structure.
- **Transfers per leg** (an OH foot): a child keeps its parent's steps and
  transfers unchanged and adds only its own; the acceptor is the rule's;
  no later leg bonds a site an earlier H took.
- Filters commute (`formed_bonds = 4` = the inventory `formed 4× O–Si` =
  the four-leg part of the unfiltered run); no local phase with a cap of 3
  or for a tripod; the budget truncates a level to the prefix of its order,
  and the order is by parent strain; a frozen foot over frozen sites never
  bonds; one thread = four threads (tree, relaxations, hypotheses,
  candidates bit for bit).
- The support's `audit` now checks the local statistics add up and that a
  row is relaxed once; `assert_stats_add_up` checks the parent flags.

**Mutation check (§11.9)**, one planted bug at a time (script
`chemisorption_phase2_mutate.py` next to this file, log beside it), all 15
caught: reach from the posed foot; reach off by 0.05 Å; dedupe keeps the
highest strain; dedupe key without transfers (missed at first: no fixture
had two OH feet competing for one acceptor; `competing_oh` and
`two_binding_orders_with_different_acceptors_stay_two_hypotheses` added); a
state loses its transfers; valence ignores earlier acceptors; truncation
not by parent strain; mirrored hypotheses as parents; the local phase
ignores the spent budget; local near-miss band halved; the frontier drops
the unfrozen substrate; states never grow deeper; the local inventory
misses the new bond; local duplicates not counted on the parent; frozen
pairs may bond. A first run was killed by the host for low memory and left
its mutation in the source: after an interrupted run, check the sources.

### 15.4 Left for later phases

- Phase 3: the node (`ChemisorbStats` fields `local_relaxed` and the level
  statistics, the "+ local phase" panel line), the guide (tag the facet;
  local-phase cost).
- Phase 4: the debug view on top of `replay` and `SearchReport::tree`.

---

## 16. Phase 3 results (2026-10-09)

On atomCAD branch `chemisorption-sequential`, not committed yet. The
`chemisorb` node runs the sequential search, and the old all-at-once engine is
gone.

### 16.1 What is built

**Engine.** The old engine's `plan` / `evaluate` / `search` (`enumerate.rs`'s
enumerator, `report.rs`, `ChemisorptionSearch`, `candidate_transfers`) are
deleted. What the sequential search used of it stays, renamed where the old
name no longer fit: `enumerate.rs` → `atoms.rs` (`free_valence`,
`reactive_atoms`, `change_key`), `inventory_options` moved to `inventory.rs`,
and `relax` takes a `RelaxSettings` (iterations, gradient tolerance, vdW mode)
instead of the old config. `input_fingerprint` hashes a `SequentialSearch`,
destructured so a new field is a compile error. The search keeps its
`chemisorption::sequential` path. The spike test and the old engine's tests
are deleted; the captured golden data (`chemisorption_golden/old_engine.json`)
stays, and the golden test still reads it.

**Node** (`nodes/chemisorb.rs`, `chemisorb_ops.rs`):

- Properties `anchor_reach` (3.5), `tolerance` (0.5), `reach` (3.0),
  `clash_filter` (on), beside the unchanged ones; `max_transfers` is gone.
  `max_formed_bonds` is `-1` (no cap) or at least 1, `formed_bonds` at least 1:
  a zero-leg search finds nothing, so 0 is now an error rather than a value.
- Outputs `candidates` (pin 0) and `stats` (pin 1); `best` is removed. The node
  draws nothing in the viewport. `ChemisorbCandidate` gains `seating_clash`.
  `ChemisorbStats` is the plan's counts (`feet`, `sites`, `paths`, `anchors`,
  `sphere_pairs`, `torus_triples`, `duplicates`, the pruned counts, the mirror
  and clash counts, `candidates`, `parents`, `to_relax`, `local_phase`) and the
  run's (`relaxed`, `local_relaxed`, `unconverged`, `listed`, `truncated`,
  `seconds`), plus `local: [ChemisorbLevel]`, one record per local leg. `sites`
  replaces `sites_in_reach` and `paths` replaces `considered`, since neither old
  meaning survives. The `ChemisorbTransfer` direction hint offers only
  `to_substrate`; a `to_adsorbate` record is the config's error, naming it.
- **The stored result keeps the plan beside the report** (`StoredSearch`):
  the report numbers its hypotheses after the plan's and extends its tree, and
  Phase 4's `replay` needs both. Run is therefore `plan` then `evaluate` in
  `chemisorb_ops.rs` rather than `sequential::search`, with the same progress
  phases. While a stored result matches, evaluation reuses its plan instead of
  planning again.
- **The `bond_inventory` dropdown** counts the plan's candidates (legs 1–3)
  that a run would relax. Deeper inventories are not known before Run; once a
  matching result without the inventory filter exists, its local hypotheses are
  listed too. The guide says to run once with `formed_bonds` set to reach them.
- The eval cache carries `reach_used` (a local phase follows, or a transfer
  record is wired), and the panel greys `reach` from it.

**Panel** (`chemisorb_editor.dart`): the three distances, *Prune seatings that
clash*, *Limit legs* (minimum 1); the count beside Run reads "+ local phase"
when legs 4+ follow; the statistics card shows the leg counts, candidates,
parents, mirror and clash counts and one line per local leg; a candidate row is
marked `clash` when its seating clashed. Reference guide `nodes/atomic.md#chemisorb`
rewritten; `AGENTS.md` (crystolecule "Chemisorption search", `nodes/`,
`node_data/`) rewritten as §12 Phase 3 asked, including the reversed "no
geometric pruning" rule.

### 16.2 Decisions taken while implementing

1. **`reach` is always fingerprinted**, even when nothing reads it. Whether it
   is read depends on the inputs (the number of feet), and a greyed property is
   not one a user edits; hashing it conditionally would need the setup inside
   the fingerprint.
2. **No file migration** (old §11.3 precedent): a saved `reach` lands on the
   new local `reach`, `anchor_reach` and `tolerance` take their defaults, and
   `max_transfers` is ignored on load. A test pins this.
3. **Wires saved from the old output pins are not moved** (§6.2 accepted
   this). Pins are positional, so a wire from old pin 0 (`best`) now reads
   `candidates`, one from old pin 1 (`candidates`) reads `stats`, and one from
   old pin 2 (`stats`) has no pin; type mismatches show as validation errors
   on the consumer. Not exercised on a real file: re-wire by hand.

### 16.3 Tests

- `chemisorb_node_test.rs`, 29 tests (debug, under a second of search): the
  plan before Run (one-foot •OH, three one-leg hypotheses, the paths
  invariant); the output pins and record schemas (`best` gone, the new stats
  fields, `ChemisorbLevel`); Run, staleness, undo, call sites, the eval cache;
  the filters and the dropdown on a three-foot cage (two two-leg inventories
  and one three-leg, by leg count then label, counts consistent with
  `to_relax`); water with an H record (one transfer per candidate, the rule's
  inventory, `reach_used`); `to_adsorbate` refused; a four-foot cage with
  `formed_bonds = 4` (local phase before and after Run, `local` on the pin,
  `relaxed = to_relax + local_relaxed`); text format and `.cnnd` round trips;
  an old saved node loads; the node-job tests unchanged in substance.
- `chemisorb_api_test.rs` (the panel seam) updated; Flutter
  `node_jobs_test.dart` and `chemisorb_inventory_test.dart` pass unchanged.
- Suites: crystolecule chemisorption 69 passed (4 ignored, release-only);
  `structure_designer` 3958, `structure_designer_api` 340, `integration` 116,
  no failures. Clippy 36 warnings (baseline 40), none in the changed files;
  `flutter analyze` adds nothing in the changed file.

No mutation check: §11.9 asks for one per *engine* phase, and the engine did
not change here beyond moving code.

### 16.4 Left

- Phase 4: the debug view, on `StoredSearch::{plan, report}` and `replay`.
- Phase 5 / manual: a walkthrough of the panel and Run after
  `cargo build --release`, and the Flutter smoke test (maintainer only).

---

## 17. Phase 4 results (2026-10-09)

On atomCAD branch `chemisorption-sequential`, not committed yet. The debug
view of §6.5 is built: the tree in the panel, the select action with its
background replay, the `debug` and `debug_shapes` pins, the CLI command and
the reference-guide section.

### 17.1 What is built

**Engine** (`sequential/debug.rs`, new; `local.rs` gains `replay_start`,
which `replay` now calls). Pure functions over a plan and an optional report,
nothing stored:

- `row_forms`: the forms a row has and the one it opens on, the §6.5 rule
  verbatim (posed for the root and the foot rows; seated for a one- or
  two-leg row with children; relaxed when relaxed; seated otherwise).
- `needs_relaxation`: the line between "instant" and "a job" — a relaxed row
  that is not a kept candidate (replay), and a local-phase row's seated form
  (its relaxed parent replayed, plus the new bond).
- `debug_view` / `root_view`: the row's structure with its markings in the
  decorator (colour overrides, labels, the unmarked substrate ghosted), the
  search shapes, the strain of a relaxed row, and `Marks` (bonded, feet,
  accepted / mirrored / undecided / clashing children, near misses with the
  smallest miss) for the tests and the CLI.
- `DebugShapes`, an analytic `ScalarField`: per unbonded foot the leg-2 shell
  or the leg-3 ring (the intersection of its shells), the `anchor_reach`
  spheres, the local `reach` spheres; field `max(0, SHAPE_LEVEL − d)` with `d`
  the signed distance to the union, so the isosurface at `SHAPE_LEVEL` is the
  boundary and no negative lobe is extracted.
- `row_label`, `row_path`, `find_row`: the names the panel and the CLI use.
  A path is the legs as `foot-site` combined atom ids (`12-45,13-61`, the ids
  of a candidate's `sites` field), a foot id alone for its foot row, `#N`, or
  `root`.
- `ChemisorptionError::DebugRow` for a row or form that does not exist.

**Node** (`nodes/chemisorb.rs`, `chemisorb_ops.rs`):

- Output pins 2 `debug` (`Molecule`) and 3 `debug_shapes` (`Isosurface`),
  appended. Every error reaches all four pins.
- `ChemisorbData::debug: Option<Arc<StoredDebug>>` (fingerprint + view),
  `#[serde(skip)]`, carried by `inherit_runtime_state` like `stored`.
  Evaluation outputs it while the fingerprint matches, otherwise the root view,
  which it builds itself (geometry alone).
- The eval cache carries the tree the panel reads (`ChemisorbTree`: the plan
  of this evaluation in an `Arc`, or the matching `StoredSearch`), the current
  selection and the tree key (the input fingerprint). Its `debug_row`,
  `debug_children` (duplicates only on request) and `debug_ancestors` build
  the panel's rows lazily.
- `StructureDesigner::prepare_chemisorb_debug(scope, node, DebugRowRef,
  form)`: the same guards as Run (top level, editable network), evaluates the
  inputs, takes the stored search when it matches or plans afresh, finds the
  row (by number or by path), and returns either a ready view or a
  `ChemisorbDebugWork` — a second kind of node job, labelled "Chemisorption
  debug view". Both install through `install_job_result`: no undo step, no
  dirty flag. `chemisorb_debug_select_blocking` is the CLI's form.

**API and CLI.** `get_chemisorb_debug_row` / `_children` / `_ancestors`,
`chemisorb_debug_select(scope, node, row, form) → job id or none`, and
`chemisorb_debug_select_by_name` for the HTTP server's `/debug-select`.
`atomcad-cli debug-select <node> [<path>] [--form posed|seated|relaxed]
[--show]` (and the REPL's `debug-select`) prints the row's description and its
children with their paths; `--show` displays the node and its two debug pins.

**Panel** (`chemisorb_debug_tree.dart`, under the statistics card): the lazy
tree (fetched on expand, a `ListView` of 24-px rows in a 300-px box), each row
with its label and counts (strain, the next level's accepted / mirrored /
undecided / clash / near counts, "N dup. hidden"), a tooltip with the path,
rejections and near misses; *Show duplicates*; *debug pin* / *shapes pin*
chips that toggle pins 2 and 3; under the tree the selected row with a
*Seated / Relaxed* switch when it has both, a button back to the root, and a
colour legend. A replay shows in the Run row's progress like a Run, with
Cancel; the host reports its outcome.

### 17.2 Decisions taken while implementing

1. **The panel selects by row number, the CLI by path.** §6.5 names a row by
   its path; row numbers are equivalent and hold while the fingerprint does
   (a run only appends the local rows), so the API takes the number and the
   CLI the path, which `find_row` resolves.
2. **A duplicate row shows its canonical row.** Only the path the search kept
   produced the change set's geometry; the panel's duplicate rows jump to it
   rather than showing a second view.
3. **A local-phase row's seated form is its start geometry**: the relaxed
   parent, replayed, plus the new bond and H. It needs relaxing, so it is a
   job like a relaxed replay.
4. **The shapes draw the test's envelope over site elements.** A new foot's
   bond length depends on the site element; the drawn shell uses the longest,
   so it contains the test's shell for every element (they coincide on a
   one-element substrate, which a test pins). The `reach` spheres are drawn
   only in the relaxed form, since the local phase searches from relaxed
   positions; a seated three-leg parent draws nothing.
5. **The shapes field reports a native grid** (`SHAPE_GRID` 0.3 Å) although
   it is analytic, so the extraction does not fall back to the 0.15 Å default
   spacing over shells 20 Å across. This half of §10 Q9 (how coarse before the
   ring looks bad) is the walkthrough's to judge; the value is one constant.
6. **§10 Q9's other half: atom labels carry arbitrary text**, so near misses
   are labelled in the viewport with their miss (`Si52 +0.27`), not only in
   the row tooltip. Feet and bonded atoms are labelled with their names.
7. **A view built before a Run stays after it.** It is still correct for the
   same fingerprint (the seated form); the switch offers the relaxed form. Not
   re-selecting automatically avoids starting a replay job nobody asked for.
8. **A selection survives a settings edit and its undo** (through
   `inherit_runtime_state`), as the stored result does, and a selection never
   makes a result stale (it is not fingerprinted).
9. **One job per node** still holds: a replay and a Run refuse each other
   ("A job is already running on this node"). Select is refused where Run is
   (in a body, on a read-only network).
10. **`--show` makes an ordinary display change** (undoable, saved like any
    pin display), so an agent can render the view without the GUI.

### 17.3 Findings

1. After a run, the best kept candidate is usually a two-leg binding — and a
   two-leg row with children opens **seated** by the rule, even though it is
   a listed candidate. Correct per §6.5, but surprising at first sight: the
   relaxed form is one click on the switch. Worth checking in the
   walkthrough whether the rule should prefer relaxed for a listed candidate.
2. The plan's tree already held everything the panel needs (no engine change
   to the search itself beyond `replay_start`). Before Run the panel reads the
   plan of the current evaluation, kept in an `Arc` in the eval cache.
3. On the stand-in tripod over the Si(100) slab, every site lies inside a
   drawn sphere or ring **iff** the search's own `need_against` accepts it, at
   the defaults, for all one-leg rows and a fifth of the two-leg rows (sites
   within 1e-6 Å of a bound skipped). The drawn shapes are therefore exactly
   what the search tests, which is what makes them useful for tuning.

### 17.4 Tests

- `chemisorption_sequential_debug_test.rs` (crystolecule), 13 tests, ~10 s in
  debug: each hypothesis listed once with duplicates hidden, the hidden counts
  equal the duplicates shown; every row's path finds it, element prefixes and
  errors; the forms before a run, after one (two-leg rows with children
  seated, relaxed three-leg rows relaxed, two-leg leaves relaxed with
  `formed_bonds = 2`), clash-pruned and budget-cut rows seated; a seated view
  equals the seating and marks its bonds and clashes; the root view marks the
  feet and exactly the anchor sites (from the definition, not the tree),
  ghosts the rest, draws one `anchor_reach` sphere per foot and follows
  `anchor_reach` (the near misses become anchors one ångström wider); a row
  marks its children by verdict and labels its near misses; the shapes equal
  the test (above); a tripod three-leg row draws nothing; a relaxed row
  outside top N replays to its recorded strain (1e-6), three-leg parents and
  four-leg candidates; a local row reached in two orders shows the kept one
  with its strain; a local row's seated form is `replay_start`, and its relaxed
  parent's `reach` spheres contain every child's site.
- `chemisorb_node_test.rs`, 7 new tests (36 in the file): the appended pins
  and the root view (feet, anchor spheres, `anchor_reach` live, errors on all
  four pins); the eval cache's lazy rows, hidden duplicates and ancestors;
  selecting before Run is seated, needs no job, is not an undo step and not a
  dirtying edit, and refuses the relaxed form ("Run first"); a fingerprint
  change shows the root and its undo brings the selection back; a selection
  does not make a result stale and is not saved; after Run a relaxed row
  outside top N is a job that reproduces its strain, the kept candidate shows
  its stored structure; the CLI path selects what the panel would.
- `chemisorb_api_test.rs`, 1 new test: the rows through the API seam, the
  selection reported back, `debug-select` by path, forms, `--show`.
- Flutter `test/chemisorb_debug_tree_test.dart`, 4 tests: children fetched only
  on expand, a click selects, duplicates hidden then shown and jumping to the
  canonical row, a new tree resets the expansion, the form switch and the way
  back to the root.

No mutation check: §11.9 asks for one per *engine* phase, and the search did
not change.

### 17.5 Left

- Phase 4's last item: calibrate the default `tolerance` with the debug view
  on a real adsorbate (outside this repository).
- Phase 5: the manual walkthrough after `cargo build --release` — the panel,
  Run, the tree, the shapes at `SHAPE_GRID` 0.3 Å and the colours on a real
  proxy, and finding 1 above; the Flutter smoke test (maintainer only).

## 18. Debug view revised (2026-10-09)

The first look at the Phase 4 debug view on a real adsorbate (the maintainer,
2026-10-09) found it hard to read, and the view was revised the same day. The
search, the recorded tree, the fingerprint and the run model are unchanged;
what changed is what an item of the panel stands for and what the viewport
marks. §6.5 is updated in place; this section records why.

### 18.1 What was wrong

1. **Too many colours.** Seven overrides (bonded, foot, candidate, near miss,
   mirrored, undecided, clash) replaced the element colours on most atoms near
   the adsorbate, so a reader could no longer tell an O from a Si. The
   undecided cyan was also indistinguishable from the frozen-atom rim, and the
   bonded orange from the near-miss yellow under the viewport's lighting.
2. **Verdicts painted on atoms.** *Mirrored* and *undecided* are verdicts on
   the hypothesis a (foot, site) pair reaches — the mirror check prunes a
   three-leg assignment, not a site — yet they coloured the site. The view was
   therefore unreadable without knowing the search's internals.
3. **Several feet at once.** A row marked every unbonded foot and the union of
   all their sites, and drew all their shells: under a one-leg row of the
   tripod, two feet, two overlapping shells, and no way to tell which green
   site belonged to which foot. The violet looked like "the next foot" only
   where a single foot was left.
4. **Labels.** `O25`, `Si123`, `Si52 +0.27` drawn over the structure obscured
   it.
5. **Ghosting did nothing.** The unmarked substrate was "dimmed" with the
   ghost flag, which blends the colour halfway to mid grey: a silicon is
   already grey, so nothing looked ghosted.

### 18.2 What it is now

- **States and steps alternate.** An item is a *state* — the root or a leg
  row: the bonds made so far — or a *step*, the "next foot" from a state for
  one foot. A state's children are its steps (one per foot the search tried
  next from it; the root's are the tree's foot rows), a step's children are
  the legs its test accepted. Walking down the tree replays the search one
  decision at a time. Steps are a grouping of a state's children by foot,
  `DebugItem { row, foot }`, not rows of the recorded tree; the tree and the
  search know nothing of them.
- **A state** shows its bonds orange and, seated, its own clashing atom pairs
  red; nothing else is recoloured, nothing faded, no shape.
- **A step** shows its foot violet, the sites its test accepted green
  (whatever became of the leg each reaches: that is the leg row's to say), the
  state's bonds orange, the rest of the substrate transparent (per-atom alpha,
  `FADED_ALPHA` 0.25), and its test's one shape on `debug_shapes`: the
  `anchor_reach` sphere around the posed foot, the shell, the ring, or the
  local `reach` sphere around the foot's relaxed position. The test pinning
  "a site is inside a drawn shape iff the search's test accepts it" now holds
  per foot.
- **No near-miss colour, no labels.** Near misses are the sites just outside
  the drawn shape; they stay in the step's counts and tooltip and in the CLI's
  description. An atom's name is in its hover tooltip, which now shows its id
  (`O25` = oxygen, id 25: element symbol plus the id in the search's combined
  structure, where the adsorbate's atoms come first).
- **Forms.** A step is shown in the form its test read positions from:
  posed under the root, seated under a one- or two-leg state, relaxed under a
  deeper one. A leg opens relaxed when it was relaxed and seated otherwise —
  §17.3 finding 1 (a two-leg candidate opening seated) is resolved by the
  split: its seated form is its step's. The *Seated / Relaxed* switch remains
  for legs that have both.
- **The root marks nothing** and draws no shape. The `anchor_reach` tuning
  view of §6.5 is the root's step for one foot; evaluation rebuilds a selected
  step under the root for new inputs (it is posed geometry, found again by the
  foot's atom id), so it still follows `anchor_reach` live. Every other
  selection still falls back to the root on a fingerprint change. Considered
  and rejected: the root showing every foot's sphere at once (one rule with no
  exception reads better in the reference guide; the steps are one click
  away).
- **Paths.** A step's path is its state's followed by the foot alone:
  `12` under the root (as the foot row's path was), `12-45,13` below it.

### 18.3 Tests

`chemisorption_sequential_debug_test.rs` (14 tests): states and steps
alternate and know their parents; each hypothesis is listed once with
duplicates hidden; every item's path finds it; forms before and after a run
(a relaxed two-leg row opens relaxed, its step seated); a seated state marks
only its bonds and clashes; the root marks nothing and a root step marks its
foot and exactly its anchor sites, from the definition, and follows
`anchor_reach`; a step marks every accepted site whatever the verdict and
colours no near miss; a step's shell or ring contains exactly what its foot's
test accepts; no labels and no ghosts anywhere, the unmarked substrate faded
on steps only; the replay tests on items. `chemisorb_node_test.rs`: a selected
root step follows `anchor_reach` across a fingerprint change; the eval cache
lists states and steps; the CLI selects a step by path. The API test and the
Flutter panel test (`test/chemisorb_debug_tree_test.dart`, 5 tests) follow
the items.

### 18.4 The transferred H is seated in the acceptor's open slot

The first look at a one-leg step (O24–Si203, H→Si202, seated) showed the
transferred H tilted across the Si202 dimer towards O24 and Si203. The rule
of §5 placed it at the Si–H length on the line from the acceptor towards
where the H sat on the seated molecule: a direction set by the pose, blind to
the acceptor's own bonds. That geometry is the start of every relaxation with
a transfer, not only a picture. It now goes into the acceptor's open valence
slot (guided placement, as `passivate` places terminators), the slot nearest
the side it came from, after the leg bonds are added; several transfers are
seated one at a time in a fixed order, so two H on one acceptor take two
slots and the listed order of the transfers does not matter. The seating
clash check is unaffected: it skips hydrogen. A new test checks the seat
against geometry of its own: on a three-bonded acceptor the H lies along
−Σ of the bond directions, on a two-bonded one it is tetrahedral to both.

Two consequences. The water golden snapshot's H-transfer candidate relaxes
to −2.80 kcal/mol instead of −3.21 (same bonds and acceptor, a neighbouring
minimum). And the local-phase fixture `competing_oh` had built its two extra
acceptors with their free valence pointing away from the adsorbate, into empty
space; the old rule had hidden that by putting the H on the near side. They
now point where an atom can arrive (X's sideways, out from under the planted
sites; Y's up), and the test still finds a bond set reached with two acceptor
choices.

### 18.5 Panel options, after a second look

Near misses are no longer listed in the panel at all — neither as a count in
a step's summary nor in its tooltip; the CLI's description keeps them, and
the shape shows where they are. Mirror-pruned legs are hidden by default like
duplicates, behind a **Show mirrored** toggle beside **Show duplicates**: a
display option, filtered in the node's eval cache (each step reports how many
of its listed and of its duplicate legs are mirrored, so the panel knows
whether a step still has anything to expand). The step's *mirr.* count stays.
