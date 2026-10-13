# Design: equivalent chemisorption results, and the `compare_atoms` node

Status: **proposed, 2026-10-13, unreviewed.**

Two pieces on one shared core:

- **`chemisorb` lists distinct bindings only.** A new `unique` output setting
  (on by default) merges the kept candidates that are the same binding up to
  a rigid motion of their local cluster; each listed candidate says how many
  copies it stood for. It runs on the stored result, after the search, so
  changing it never needs a re-run.
- **`compare_atoms`**, a generic node that compares two atomic structures up
  to a tolerance and, optionally, a rigid motion. It is how a user finds out
  why two candidates were (or were not) merged — and it is the structure
  comparison node the project has wanted anyway.

Requested by mechadense after using the sequential chemisorption search
(`doc/design_chemisorption_sequential.md`):

> A lot of locally equivalent binding patterns in the results when I only
> want to have unique ones in the end for running simulations on these. […]
> I think I have seen mirrored ones beside shifted ones too.

---

## 1. Motivation

The search's results contain three kinds of repeats, and the search removes
none of them today:

1. **Exact duplicates.** A symmetric adsorbate with its feet swapped over the
   same sites relaxes to the same structure with the atoms permuted. The
   change-set dedupe cannot see this: `foot1–siteA, foot2–siteB` and
   `foot1–siteB, foot2–siteA` are different change sets. Numbered feet fix
   the binding *order*, not which site each foot takes, so they do not
   prevent it.
2. **Shifted copies.** The same binding on an equivalent set of sites
   elsewhere on the slab. The sequential design leaves these on purpose
   (§4.7 "Not deduplicated: translational copies") and defers the fix (§8
   "Equivalent-site deduplication … so `top_n` is not filled with one motif
   repeated across the proxy"). This document addresses it after the search:
   the copies are merged in what is listed, while `top_n` still counts the
   candidates kept before merging (D7).
3. **Mirrored copies.** Two proper bindings related by a mirror plane of the
   surface. These are *not* what the search's mirror check prunes: that
   check removes assignments no rotation of the molecule can reach. Both
   copies here are reachable.

The debug view's *Show duplicates* and *Show mirrored* toggles are unrelated:
they show tree rows the search already keeps out of the results.

### Why inside `chemisorb`, not a separate `unique_atoms` node

A separate node reducing an array of structures was designed first and
dropped:

- **No typing problem.** A downstream node needs `items` to accept both
  structures and records carrying one, and the type system has no unions;
  each workaround (`filter`-style per-instance element type, record width
  subtyping) has a cost.
- **One checkbox, not a node and its wiring** — the verbosity of
  `array_at` / `sequence` wiring is what mechadense asked to get away from.
- **The node knows its own clusters.** It writes the `cs_local` tag it
  compares on (§5.2) and reports `copies` on the candidate record, which keeps
  `rank`, `strain` and `bonds` beside each listed binding.

Making `top_n` count *distinct* bindings — merging during the search — was
considered and rejected (D7): it needs either an arbitrary oversized pool or
a store of every candidate's cluster plus replays, and it turns the
comparison settings into search settings, so every tolerance change costs a
Run. Instead `top_n` counts candidates before merging, and a user who gets
too few distinct bindings raises it.

What is given up: deduplicating arrays from other producers. The core lives
in `atomcad-crystolecule` (§3), so a `unique_atoms` node can be added on it
if someone asks; `compare_atoms` already covers pairwise checks.

---

## 2. Decisions

| # | Decision |
|---|---|
| D1 | One shared core in `atomcad-crystolecule` (§3) serves `chemisorb`'s `unique` and the `compare_atoms` node, so the two cannot disagree. |
| D2 | Equal means: **a motion of the allowed kind and a one-to-one matching of atoms, same element, every matched pair within `tolerance` (Å) after the least-squares fit of that matching** (Kabsch; for `translation`, the centroid difference). The max deviation, never the RMS, decides — as in mechanosynth placement. Optionally bonds too (presence and order). |
| D3 | The motion is one choice: `none` / `translation` / `rigid` / `rigid_mirror`. Not two checkboxes: rotation without translation has no meaning without a fixed origin. |
| D4 | The motion is **proposed, then verified**: the centroids and principal axes propose it directly where they are well defined, anchor triples where they are not (§3.4); a full fit refines it; `compare_structures` decides. The verification makes the answer exact up to the tolerance, so an asymmetric adsorbate with swapped feet, or the mirror image of a chiral one, fails without a rule of its own. |
| D5 | Each comparison has a **work cap**. Exceeding it makes the pair *not equal* and counts it **undecided**, reported. It may leave two copies apart; it never merges two different bindings and never hangs. |
| D6 | `chemisorb` compares candidates on their **local cluster**: the adsorbate plus `local_hops` bonds into the substrate, H not counting as a hop (§5.2). The cluster is also written as tags (`cs_adsorbate`, `cs_local`), so `compare_atoms` can be pointed at exactly what `unique` compared. |
| D7 | **`top_n` counts candidates before merging.** The search is unchanged: `keep_best` keeps the best `top_n` by (strain, change set), the energy window cuts them, and merging runs on that list. The listed result is therefore at most `top_n` long and often shorter; a user who wants more distinct bindings raises `top_n`. |
| D8 | `unique`, `unique_tolerance` and `local_hops` are **output settings**: they act on the stored result, never on what the search keeps, so they are not `SequentialSearch` fields, are not fingerprinted, and changing one re-evaluates the node in milliseconds without making a stored result stale. |
| D9 | **Grouping is greedy in rank order**: each candidate joins the first listed candidate it equals, else it is listed. The kept list does not depend on the order relaxations finished in (D7), so neither does the grouping — tolerance equality is not transitive, and this fixed order is what makes the answer deterministic. |
| D10 | **`unique` is on by default** (`rigid_mirror`). A ranked list of copies is never what a search is run for. Existing `chemisorb` nodes list fewer candidates after their next Run (stored results are not saved, so nothing on disk goes stale). |
| D11 | Both run at evaluation time: `compare_atoms` as any node, `unique` when `chemisorb` builds its outputs from the stored result. Eval memoization reruns them only when an input or setting changes. |
| D12 | `compare_structures` moves out of `mechanosynth/` into `atomic_structure/compare.rs`, beside `matching.rs`; `mechanosynth` re-exports it. Nothing in it is mechanosynth-specific. |

---

## 3. The core (`atomcad-crystolecule`)

`atomic_structure/compare.rs` (the moved `compare_structures` and `Mismatch`)
plus `structure_compare.rs` (cluster, invariants, motion search, grouping).
Pure functions over `AtomicStructure`, no node types.

### 3.1 Settings

```rust
pub enum Motion { None, Translation, Rigid, RigidMirror }

pub struct CompareSettings {
    pub motion: Motion,
    pub tolerance: f64,       // Å, max per-atom deviation after the fit
    pub compare_bonds: bool,
    pub max_candidates: usize // per pair, D5
}
```

`tolerance` must be in `(0, MAX_TOLERANCE]`, `MAX_TOLERANCE = 0.5 Å`, a
config error otherwise. Above about half the shortest interatomic distance
(~1.0 Å for X–H) nearest-atom matching is no longer unambiguous and the
greedy matcher in `compare_structures` could pair the wrong atoms.

Default tolerance: **0.15 Å**. Two relaxations that reach the same minimum
from different start geometries agree only to the minimizer's convergence,
and at the edge of a local cluster a shifted copy sits in a slightly
different finite-slab environment. To be confirmed in the walkthrough (§8).

### 3.2 The cluster

`Cluster::from_atoms(structure, ids)` / `Cluster::from_tag(structure, tag)`:
the chosen atoms as a small structure of their own — positions, elements,
and the bonds *between* them only. Bonds leaving the cluster are dropped:
what they say about the environment is included by choosing more atoms.

A tag no atom carries is an error, as `chemisorb`'s reactive tags are: an
empty cluster would make every pair equal.

### 3.3 Invariants: rejecting pairs cheaply

Per cluster, computed once:

- **Element counts.** Equal counts are necessary under every motion.
- **The sorted list of all pairwise distances.** A rigid motion, mirror
  included, preserves every distance; a matching within `tolerance` changes
  each by at most `2·tolerance`; and sorting is 1-Lipschitz in the max norm.
  So if the two sorted lists differ anywhere by more than `2·tolerance`, the
  pair cannot be equal. This is **exact as a rejection**, never a guess.
- **Bond counts by kind** (only when bonds are compared): the number of
  bonds for each unordered element pair and bond order, e.g. `C–Si single:
  3`. The bonds compared are those between cluster atoms (§3.2), and a
  matching that agrees on every bond maps them one to one, kind for kind, so
  equal counts are necessary. Exact as a rejection, like the other two, and
  O(bonds) to compute.
- **Atom labels** (only when bonds are compared): each atom's element, its
  bond count within the cluster, and the sorted elements of its bonded
  neighbours. A matching that agrees on every bond maps each atom to one
  with the same label, so the multisets of labels must be equal. The labels
  are reused by the anchor search (§3.4). Without bonds compared they are
  not used at all: bonds may then differ, and so may labels.
- **The sorted eigenvalues of the second-moment tensor.** With `c` the
  centroid and `aᵢ = xᵢ − c`, the tensor is `M = Σ aᵢ aᵢᵀ` (unweighted). A
  rigid motion, mirror included, leaves its eigenvalues unchanged. A matching
  within `tolerance` moves each centred position by `|dᵢ| ≤ 2·tolerance`
  (its own deviation plus the centroid's), so `‖M_A − M_B‖ ≤ E =
  Σ (2|aᵢ||dᵢ| + |dᵢ|²) ≤ Σ (4·tolerance·|aᵢ| + 4·tolerance²)`, taking `aᵢ`
  from A; by Weyl's inequality no eigenvalue moves by more than `E`. So
  eigenvalues differing by more than `E` reject the pair — exact, and the
  same tensor serves the principal-axes path (§3.4).

Pairs are binned by element counts and, with bonds, bond counts first (one
hash of both), then the label multisets and eigenvalues are compared, then
the distance lists (O(N²), ~10 µs at N = 150). Only pairs that pass reach
the motion search. The bond counts reject differently bonded pairs before
any motion is tried, instead of at the final `compare_structures`.

### 3.4 The motion search

Instead of trying positions and orientations blindly, the motion is first
read off the clusters themselves; the search is the fallback.

- **`none`**: no search. `compare_structures(a, b, tolerance)`.
- **`translation`**: **one candidate**, `t = c_A − c_B`, the difference of
  the centroids. It is the least-squares translation of every matching
  (D2), so if any matching is within tolerance under it, this is the motion
  that shows it. No search and no fallback.
- **`rigid` / `rigid_mirror`, the principal-axes path.** The eigenvectors of
  each cluster's second-moment tensor (§3.3) are axes fixed to the cluster.
  Rotating B's axes onto A's (about the centroids) aligns the two, up to the
  sign of each axis: **4 proper candidates**, plus 4 improper ones with
  `rigid_mirror`. Each goes through the verification below.
- **When the axes are not well defined.** Two equal or nearly equal
  eigenvalues leave the axes in their plane undetermined — a cluster with a
  threefold axis, a ring, a near-spherical cage. Under the matching's
  perturbation an eigenvector turns by at most `sin θ ≤ E / gap` (Davis–Kahan,
  `gap` the distance to the nearest other eigenvalue, `E` from §3.3). The
  path is **reliable** when `2·r_max·E / gap ≤ tolerance` for every axis
  (`r_max` the farthest atom from the centroid): the two frames then differ
  by less than the loose matching tolerance of step 1 below can absorb.
  - Reliable, and a candidate verifies: equal.
  - Reliable, and none verifies: **not equal**, without a fallback — an
    equal pair would have been found.
  - Not reliable: try the candidates anyway (cheap), and if none verifies,
    fall back to the anchor search.
- **The anchor search (fallback).** Three anchors in A — the first from the
  rarest label (the rarest element without bonds), the other two chosen to
  make a well-conditioned triangle (far apart, far from collinear). In B,
  candidate triples are built atom by atom from atoms with the **same
  label**, keeping only those whose distances to the already-chosen anchors
  agree within `2·tolerance`. Each triple gives a proper fit with
  `rigid_fit`; with `rigid_mirror`, also the improper one (A onto B reflected
  through its centroid). Labels and the distance filter keep this far from
  O(N³) even for symmetric clusters; in a molecule most labels are unique.

The local clusters `chemisorb` compares usually take the fast path: the
hop-2 substrate atoms break the adsorbate's own symmetry, and a site on
Si(100) 2×1 has at most a mirror plane. The fallback is for the rest, and it
is what keeps the answer exact (D4) whatever the cluster.

**Verification, for every proposed motion:**

1. Apply it to B and match to A with nearest-unclaimed matching at a loose
   tolerance (`3·tolerance`): a fit on three anchors is only as good as three
   atoms, and a frame from principal axes only as good as the eigenvectors.
2. If every atom is matched, **refit on all matched pairs** (`rigid_fit`, same
   handedness) and re-apply.
3. Decide with `compare_structures(a, b_moved, tolerance)` (bonds included
   when asked). Stop at the first motion that passes.

A cluster whose atoms are all collinear leaves the turn about their line
undetermined and irrelevant; two anchors suffice. A planar cluster needs no
special case: its mirror image is reachable by a proper motion, which `rigid`
already finds.

After `max_candidates` (default 10,000) proposed motions the pair is
**undecided** (D5).

```rust
pub struct Comparison {
    pub equal: bool,
    pub undecided: bool,
    pub max_deviation: f64,          // of the best fit found
    pub rmsd: f64,
    pub transform: Option<Isometry>, // B onto A; None for `none`
    pub mismatches: Vec<Mismatch>,   // against the best fit; empty iff equal
}
```

The "best fit" of an unequal pair is the motion that matched the most atoms,
ties by smaller max deviation, so a near miss is reported against the
alignment a user would pick by eye.

### 3.5 Grouping

`group_in_order(clusters, settings) -> Grouping { kept, class_of, copies,
compared, undecided }`: for each cluster in the given order, compare it with
the kept ones in its element-count bin, in order; the first equal one is its
class; else it is kept. The comparisons of one cluster against the kept ones
run in parallel and the **lowest-index** equal one is chosen, so scheduling
cannot change the answer.

### 3.6 Cost

Estimated, not measured (§7 adds the measurement):

| | |
|---|---|
| cluster size N (`local_hops` 2, H included) | 30–150 atoms |
| a rejected proposed motion | ~1 µs |
| a pair, typical / symmetric worst | 10–100 µs / ~1 ms |
| grouping 200 kept candidates | well under 50 ms typical |

Fast enough to rerun on every change of an output setting (D11).

---

## 4. The `compare_atoms` node

Category: atomic structure. A pure function of its inputs.

| Pin | Type | |
|---|---|---|
| in `a` | `HasAtoms` | |
| in `b` | `HasAtoms` | |
| out `equal` | `Bool` | |
| out `aligned` | `Molecule` | all of `b`, moved onto `a` by the best fit: overlay it on `a` to see the comparison |
| out `report` | record `CompareReport` | `max_deviation`, `rmsd`, `undecided`, `mismatches: Array[String]` (each `Mismatch`'s `Display`) |

Properties: `tag` (empty = all atoms; the same tag on both sides), `motion`,
`tolerance`, `compare_bonds`.

To find out why two candidates were or were not merged: switch `unique` off
on the `chemisorb` node (instant, D8) so every kept candidate is listed, take
two of them (`array_at` on `candidates`, then their `structure`), and set
`tag` to `cs_local`, `motion` and `tolerance` as `unique` had them. `equal`
then agrees with `unique`'s verdict, `aligned` shows the fit, and `report`
names the atoms that broke it. With `motion = none`, `compare_bonds` on and
an empty tag it is the structural equality the mechanosynth generators verify
with.

Errors: tolerance out of range, a tag no atom carries on either side.

---

## 5. `chemisorb`

### 5.1 Settings

Three new `ChemisorbData` fields and panel controls, **not**
`SequentialSearch` fields (D8): `input_fingerprint` never sees them, and the
engine's search knows nothing of them.

| Setting | Default | |
|---|---|---|
| `unique` | `rigid_mirror` | `off` / `exact` (= motion `none`) / `translation` / `rigid` / `rigid_mirror` |
| `unique_tolerance` | 0.15 Å | the comparison tolerance (§3.1) |
| `local_hops` | 2 | the cluster compared, and tagged `cs_local` (§5.2) |

Bonds are always compared: two clusters with the same positions and
different bonds are different bindings.

### 5.2 The local cluster and the two tags

- **`cs_adsorbate`**: every atom that came from the `adsorbate` input. A
  transferred H came from the adsorbate, so it keeps the tag on the
  substrate. Added in `evaluate`, beside `CHANGED_TAG` (`cs_changed`): it
  depends on no setting.
- **`cs_local`**: added when the node builds a candidate's output record
  (the structure is cloned there already), since it depends on `local_hops`.
  It is written whether `unique` is on or off, so `compare_atoms` can use it
  either way. Seeds are the `cs_adsorbate` atoms. Expand `local_hops`
  times along bonds; an H reached is included but not expanded from (a
  terminator); seed H atoms *are* expanded from, so a transferred H reaches
  its acceptor at hop 1. Finally add every H bonded to an included atom.

The `cs_local` atoms are the cluster `unique` compares (D6). Hop 1 is the
adsorbate plus every substrate atom the binding touched. Hop 2 adds each
site's back-bond neighbours and dimer partner, and with the H rule their
passivation — whether the neighbouring dimer atom is a radical, which hop
counting alone would put at hop 3. Hop 1 cannot tell site types apart when
the sites' arrangement is the same (a dimer atom and a step-edge atom compare
equal), the common case with one or two legs; hence the default 2.

Two more tags count against `MAX_TAGS` (32); an input near the limit gets the
existing tag error.

### 5.3 Where it runs: at output

The search and its stored result are untouched (D7). When the node builds
its outputs from a stored result:

1. Take the stored candidates — the best `top_n` by (strain, change set),
   already cut to the energy window, in rank order.
2. Tag each with `cs_local` (§5.2) and cut its cluster.
3. With `unique` on, group them in rank order (§3.5; the setting's motion
   and tolerance, bonds on). Each candidate that equals an earlier one is
   dropped from `candidates` and counted on it.
4. List the rest, keeping their search `rank`, so a gap in the ranks shows
   where copies were merged.

So the window is applied before grouping: a distinct binding outside it is
gone even if a copy of it would have been inside. `top_n` and the window
are the search's; `unique` only reduces what they kept. `off` lists exactly
what is listed today, plus the `cs_local` tag.

A merged candidate is still a relaxed candidate in the search tree; with
`unique` on, the debug view's tooltip on its row says "copy of #rank", so it
is not mistaken for one that fell outside `top_n`.

### 5.4 Outputs

- Candidate record: new field `copies: Int` — how many kept candidates this
  one stood for, itself excluded (0 with `unique` off or no copy found).
  Copies the search did not keep are not counted.
- Stats record: `listed` (candidates after merging), `merged` (candidates
  merged into an earlier one), `unique_undecided` (pairs that hit the work
  cap). With `unique` off, `listed` equals the kept count and the other two
  are 0.

Both are appended fields; no pin changes. The pin readout of `candidates`
shows "6 of 20 (14 copies merged)" when `unique` merged anything.

---

## 6. Phases

1. **Core.** Move `compare_structures` (D12, re-exported from
   `mechanosynth`); `structure_compare.rs` with the cluster, invariants, the
   three searches, verification, grouping. Tests (§7). No UI.
2. **`chemisorb`.** `cs_adsorbate` in `evaluate`; the three output settings
   (node data, panel), `cs_local` and grouping in the output building, the
   record and stats fields, the readout, the debug-view tooltip, the
   reference-guide section. The engine's search is not touched, so its
   tests (oracle, brute force, golden data) are unaffected; node-level tests
   and snapshots of the `candidates` output under the default settings are
   reviewed and accepted.
3. **`compare_atoms`.** Node, panel, `CompareReport` record, text format,
   reference guide.
4. **Walkthrough** (maintainer, with mechadense's results): `local_hops` 1 vs
   2, tolerance 0.1 / 0.15 / 0.25, `rigid` vs `rigid_mirror`; look at every
   pair hop 1 merges and hop 2 keeps apart, using `compare_atoms`.

---

## 7. Tests

In `atomcad-crystolecule/tests/crystolecule/structure_compare_test.rs`:

- **Invariance.** A random cluster equals itself under a random translation,
  rotation, and (for `rigid_mirror` only) reflection, atoms shuffled, every
  position jittered by up to `0.9·tolerance`. Each motion kind rejects the
  motions it does not allow (`translation` a rotated copy, `rigid` the mirror
  image of a chiral cluster).
- **The invariant filters are exact.** On small random clusters, no pair the
  distance-list test or the bond-count test rejects is equal by brute force
  over all motions. Without bonds compared, the bond counts reject nothing.
- **Chirality and asymmetry.** A chiral cluster vs its mirror image: equal
  under `rigid_mirror`, not under `rigid`. An asymmetric molecule with two
  feet swapped over the same two sites: not equal under any motion.
- **Bonds.** Same positions, one bond order different: equal without bonds,
  not with them — rejected by the bond counts, before any motion is tried.
  Same bond counts, bonds on different atoms: rejected by the final check.
- **Collinear and planar clusters** compare correctly.
- **Principal axes agree with the search.** Over random clusters and pairs
  (including near-degenerate ones: threefold and ring-like clusters, slightly
  jittered), the verdict with the principal-axes path enabled equals the
  verdict of the anchor search alone. In particular, no pair the path calls
  "reliable, not equal" is equal by the anchor search.
- **The eigenvalue and label pre-checks are exact** (in the same brute-force
  test as the other pre-checks).
- **`translation`** finds a shifted, jittered copy with its single
  candidate, and rejects a rotated one.
- **Undecided.** With `max_candidates = 1` a symmetric pair comes back
  undecided, never equal.
- **Grouping** is deterministic under varied thread counts; each class keeps
  its first member; `class_of` and `copies` agree.
- **Timing.** Grouping 200 hexapod-sized clusters at hop 2, `rigid_mirror`:
  under 0.25 s in release.

In the sequential-search tests (`chemisorption_sequential_*`):

- **`cs_adsorbate`** is on exactly the adsorbate's atoms, including a
  transferred H on the substrate.
- **The local cluster** (the hop rule, exposed by the core or the engine for
  the node to call): seeds, the H rule, a transferred H reaching its acceptor
  at hop 1, `local_hops = 0` equal to the adsorbate.

In `atomcad-structure-designer` tests (`chemisorb_node_test.rs` and new
ones):

- **Shifted copies merge.** On a slab fixture, a binding and its copy under
  a lattice translation of the slab that keeps both clusters inside it are
  one listed candidate with `copies ≥ 1`; under `rigid_mirror`, also its
  mirror copy across a mirror plane of the surface.
- **Listed candidates are distinct.** With `unique` on, no two listed
  candidates are equal by the core at the node's settings, and `listed +
  merged` equals the kept count.
- **`off` is today.** With `unique: off` the listed candidates are the
  stored ones, in order, with the `cs_local` tag added and `copies` 0.
- **Not fingerprinted.** Changing `unique`, `unique_tolerance` or
  `local_hops` after a Run does **not** make the stored result stale, and
  the output changes accordingly.
- **Determinism.** Same output from results searched at 1, 2 and 8 threads.
- `compare_atoms`' pins and report; text-format round trips of both nodes'
  new properties; `chemisorb`'s new record and stats fields.

---

## 8. Open questions

- **Default tolerance** (0.15 Å) — to be set from the walkthrough.
- **Show the merged copies?** A user may want to see where the copies sat
  (e.g. to pick the one farthest from the slab edge for a simulation).
  Switching `unique` off shows them all; a per-candidate list of its copies'
  ranks on the record would be cheap. Not proposed until asked for.
- **A `unique_atoms` node** for arrays from other producers, on the same
  core — if asked for.
