# Design: equivalent chemisorption results, and the `compare_atoms` node

Status: **reviewed 2026-10-13 (correctness, architecture, tests, clarity);
ready for implementation.**

Vocabulary used throughout: the search **keeps** up to `top_n` candidates
(its stored result); `unique` **merges** a kept candidate into an earlier
equal one; what remains is **listed**. A kept candidate that was merged is a
**copy**; the one it was merged into is its **representative**.

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

- **It would have a typing problem.** Such a node needs `items` to accept both
  structures and records carrying one, and the type system has no unions;
  each workaround (`filter`-style per-instance element type, record width
  subtyping) has a cost.
- **One checkbox, not a node and its wiring** — the verbosity of
  `array_at` / `sequence` wiring is what mechadense asked to get away from.
- **`chemisorb` knows its own clusters.** It writes the `cs_local` tag it
  compares on (§5.2) and reports `copies` on the candidate record, which keeps
  `rank`, `strain` and `bonds` beside each listed binding.

Making `top_n` count *distinct* bindings — merging during the search — was
considered and rejected (D7): it needs either an arbitrary oversized pool or
a store of every candidate's cluster plus replays, and it turns the
comparison settings into search settings, so every tolerance change costs a
Run. Instead `top_n` counts candidates before merging, its default rises to
50 so copies do not crowd out the distinct bindings (D13), and a user who
still gets too few raises it.

What is given up: deduplicating arrays from other producers. The core lives
in `atomcad-crystolecule` (§3), so a `unique_atoms` node can be added on it
if someone asks; `compare_atoms` already covers pairwise checks.

---

## 2. Decisions

| # | Decision |
|---|---|
| D1 | One shared core in `atomcad-crystolecule` (§3) serves `chemisorb`'s `unique` and the `compare_atoms` node, so the two cannot disagree. |
| D2 | Equal means: **a motion of the allowed kind and a one-to-one matching of atoms, same element, every matched pair within `tolerance` (Å) after the least-squares fit of that matching** (Kabsch; for `translation`, the centroid difference; for `none`, the identity). The max deviation, never the RMS, decides — as in mechanosynth placement. Optionally bonds too (presence and order). |
| D3 | The motion is one choice: `none` / `translation` / `rigid` / `rigid_mirror`. Not two checkboxes: rotation without translation has no meaning without a fixed origin. |
| D4 | The motion is **proposed, then verified**: the centroids and principal axes propose it first, anchor triples whenever those fail (§3.4); a refined fit follows; the final check decides, on the least-squares fit of the matching it found. A proposal can only cause a missed merge, never a wrong one: **"equal" is exact by construction, "not equal" is very likely but not proven** (§3.5). Handedness and foot assignment need no rule of their own: the mirror image of a chiral adsorbate fails under `rigid` (and is merged under `rigid_mirror`, as intended); an asymmetric adsorbate with its feet swapped fails under every motion over two *inequivalent* sites, and is merged when a symmetry of the cluster swaps the two sites — then it is the same binding. |
| D5 | Each comparison has a **work cap**, sized to the cost budget (§3.7), not to a round number. Exceeding it makes the pair *not equal* and counts it **undecided**, reported. It may leave two copies apart; it never merges two different bindings, never hangs, and bounds the cost of one pair to tens of milliseconds. |
| D6 | `chemisorb` compares candidates on their **local cluster**: the adsorbate plus `local_hops` bonds into the substrate, with H atoms as terminators — included, never walked through (§5.2). The cluster is also written as tags (`cs_adsorbate`, `cs_local`), so `compare_atoms` can be pointed at exactly what `unique` compared. |
| D7 | **`top_n` counts candidates before merging.** The search is unchanged: `keep_best` keeps the best `top_n` by (strain, change set), the energy window cuts them, and merging runs on that list. The listed result is therefore at most `top_n` long and often shorter; a user who wants more distinct bindings raises `top_n`. Copies of one binding have nearly the same strain, so they sit next to each other in the ranking and can fill most of a small `top_n` on their own; hence D13. |
| D8 | `unique`, `unique_tolerance` and `local_hops` are **output settings**: they act on the stored result, never on what the search keeps, so they are not `SequentialSearch` fields, are not fingerprinted, and changing one re-evaluates the node in milliseconds without making a stored result stale. |
| D9 | **Grouping is greedy in rank order**: each kept candidate is merged into the first listed candidate it equals, else it is listed. The kept list does not depend on the order relaxations finished in (D7), so neither does the grouping — tolerance equality is not transitive, and this fixed order is what makes the answer deterministic. |
| D10 | **`unique` is on by default** (`rigid_mirror`). A ranked list of copies is never what a search is run for. A saved node without the new fields loads with the defaults (`#[serde(default)]`), so existing `chemisorb` nodes list fewer candidates after their next Run (stored results are not saved, so nothing on disk goes stale). |
| D11 | Both run at evaluation time: `compare_atoms` as any node, `unique` when `chemisorb` builds its outputs from the stored result. Eval memoization reruns them only when an input or setting changes. |
| D12 | **One module, `structure_compare/`, at the crate's top level** (beside `rigid_fit.rs`), holds all of the comparison: `compare_structures` and `Mismatch` move into it from `mechanosynth/compare.rs` (`mechanosynth` re-exports them; nothing in them is mechanosynth-specific), with a companion `compare_structures_matched` that also returns the matching (§3.4). Not `atomic_structure/`: that directory is the container and its primitive queries (`matching.rs` stays there), and a comparison is an algorithm over two structures, like the rigid fit. |
| D13 | **`top_n`'s default rises from 10 to 50** for new `chemisorb` nodes; saved nodes keep their value. `top_n` bounds only what `keep_best` holds — every candidate is relaxed whatever it is — so the cost is memory for 40 more kept candidates, not search time. With 10, the best motif repeated over the proxy's equivalent sites could leave one or two distinct bindings listed, which is the requester's complaint in another form. The walkthrough (§6, Phase 4) checks the number on a realistic proxy. |
| D14 | **What is chemisorption-specific lives in `chemisorption/distinct.rs`**, not in the node: the two tags, the hop rule, and "group a `SearchReport`'s candidates". The `chemisorb` node converts its settings, calls one function and builds records from the answer, as it already does for the search. The engine's search (`sequential/`) is not touched. |

---

## 3. The core (`atomcad-crystolecule`)

Pure functions over `AtomicStructure`, no node types, in one module (D12):

```
crates/atomcad-crystolecule/src/
├── structure_compare/
│   ├── mod.rs        # CompareSettings, Motion, Comparison; compare(); re-exports
│   ├── exact.rs      # compare_structures / compare_structures_matched, Mismatch (moved)
│   ├── cluster.rs    # Cluster (§3.2)
│   ├── invariants.rs # the pre-checks (§3.3)
│   ├── motion.rs     # proposals, verification (§3.4)
│   └── group.rs      # group_in_order (§3.6)
├── rigid_fit.rs      # + symmetric_eigen_3 (below)
└── chemisorption/
    └── distinct.rs   # the chemisorb side (§5, D14)
```

Two things it reuses rather than duplicates:

- **The fit** is `rigid_fit::rigid_fit`, and a found motion is reported as
  its `Fit` (`r` may be improper under `rigid_mirror`); no new transform type.
- **The principal axes** need a symmetric 3×3 eigendecomposition, which the
  crate does not have: `rigid_fit` has a private cyclic-Jacobi solver for the
  4×4 of Horn's method, and `atomcad-geo-tree`'s 3×3 helper is private and
  returns the smallest eigenvalue only. Generalize the Jacobi solver in
  `rigid_fit.rs` to return all eigenpairs (`symmetric_eigen_3`, and the 4×4
  path on the same code), keeping its fixed sweep order and count — the
  determinism the grouping (§3.6) relies on comes from there.

`AGENTS.md` for the crate gets the new module in its map, and its "Position
matching" paragraph the new home of `compare_structures`.

### 3.1 Settings

```rust
pub enum Motion { None, Translation, Rigid, RigidMirror }

pub struct CompareSettings {     // Default:
    pub motion: Motion,           // RigidMirror
    pub tolerance: f64,           // 0.15 Å, max per-atom deviation after the fit
    pub compare_bonds: bool,      // true
    pub max_proposals: usize,     // 200, per pair (D5, §3.4)
    pub proposals: Proposals,     // both paths (§3.4)
}

/// Both on by default and in every caller; turning one off exists for the
/// tests that check the two paths against each other (Phase 1 tests, §6).
pub struct Proposals { pub principal_axes: bool, pub anchors: bool }
```

`tolerance` must be in `(0, MAX_TOLERANCE]`, `MAX_TOLERANCE = 0.3 Å`, a
config error otherwise. The constraint is the final check's nearest-atom
matching (§3.4, step 3), which pairs only atoms of the same element. With
the right motion, an atom's partner is within `tolerance`, and any other
atom of its element is at least `d − 3·tolerance` away, `d` the shortest
same-element distance in A (B's can be `2·tolerance` shorter). The pairing is
therefore unambiguous when `4·tolerance < d`. For most bonds `d` is at least
~1.2 Å (C=C 1.34, geminal H–H ~1.8, Si–Si 2.35), so 0.3 Å qualifies. A
cluster with a shorter same-element pair (H₂, N₂) can, near the limit, lose
a pairing — a missed merge, never a wrong one (§3.5).

Default tolerance: **0.15 Å**. Two relaxations that reach the same minimum
from different start geometries agree only to the minimizer's convergence,
and at the edge of a local cluster a shifted copy sits in a slightly
different finite-slab environment. To be confirmed in the walkthrough (§6,
Phase 4; open question in §7).

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
  rigid motion, mirror included, leaves its eigenvalues unchanged. The
  least-squares fit of an equal pair (D2) puts the centroids together, so
  each centred position moves by `|dᵢ| ≤ tolerance`, and with `bᵢ = aᵢ + dᵢ`,
  `‖M_A − M_B‖ ≤ E = Σ (2·tolerance·|aᵢ| + tolerance²)`, taking `aᵢ` from A.
  By Weyl's inequality no eigenvalue moves by more than `E`, so eigenvalues
  differing by more than `E` reject the pair — exact. The same tensor gives
  the principal axes (§3.4).

Pairs are binned by element counts and, with bonds, bond counts first (one
hash of both), then the label multisets and eigenvalues are compared, then
the distance lists (O(N²), ~10 µs at N = 150). Only pairs that pass reach
the motion search. The bond counts reject differently bonded pairs before
any motion is tried, instead of at the final `compare_structures`.

### 3.4 The motion search

Instead of trying positions and orientations blindly, the motion is first
read off the clusters themselves; the search is the fallback.

- **`none`**: no search. The identity is the one proposal; it goes straight
  to step 3 of the verification below (no pairing, no refinement, no refit).
- **`translation`**: **one proposal**, `t = c_A − c_B`, the difference of
  the centroids. It is the least-squares translation of every matching
  (D2), so if any matching is within tolerance under it, this is the motion
  that shows it. It goes through the verification below; no other proposal
  and no fallback.
- **`rigid` / `rigid_mirror`, the principal-axes path.** The eigenvectors of
  each cluster's second-moment tensor (§3.3) are axes fixed to the cluster.
  Rotating B's axes onto A's (about the centroids) aligns the two, up to the
  sign of each axis: **4 proper proposals**, plus 4 improper ones with
  `rigid_mirror`. Each goes through the verification below. If one passes,
  the pair is equal.
- **If none passes, the anchor search always runs.** The axes can be off:
  two equal or nearly equal eigenvalues leave the axes in their plane
  undetermined (a cluster with a threefold axis, a ring, a near-spherical
  cage), and even well-separated ones turn under the tolerance. No
  reliability bound is used to skip the fallback — a wrong bound there would
  turn into silently missed merges, and the pairs that get this far have
  passed every pre-check, so they are nearly identical and rare.
- **The anchor search.** Three anchors in A — the first from the
  rarest label (the rarest element without bonds), the other two chosen to
  make a well-conditioned triangle (far apart, far from collinear). In B,
  candidate triples are built atom by atom from atoms with the **same
  label**, keeping only those whose distances to the already-chosen anchors
  agree within `2·tolerance`. Each triple gives a proper fit with
  `rigid_fit`; with `rigid_mirror`, also the improper one (A onto B reflected
  through its centroid). Labels and the distance filter keep this far from
  O(N³) even for symmetric clusters; in a molecule most labels are unique.

Equal pairs of the local clusters `chemisorb` compares are usually found on
the principal axes: the hop-2 substrate atoms break the adsorbate's own
symmetry, and a site on Si(100) 2×1 has at most a mirror plane. The anchor
search is for the rest, and for every unequal pair that reaches it.

**Verification, for every proposed motion:**

1. **Pair.** Apply the motion to B and pair each atom of A with the nearest
   unclaimed atom of B of the **same element** (with bonds compared, the
   same label), within a loose `3·tolerance`: a fit on three anchors is only
   as good as three atoms, and a frame from principal axes only as good as
   the eigenvectors. A different element is never a partner (D2), which
   keeps C–H neighbours from being confused at any tolerance.
2. **Refine.** If every atom is paired, refit on all pairs (`rigid_fit`, same
   handedness), re-apply, and pair again. Repeat until the pairing stops
   changing, at most 3 rounds. A few wrong pairs among many right ones skew
   a fit only slightly, so the next round's pairing corrects them — the
   iterative-closest-point refinement.
3. **Decide** in two parts. First `compare_structures_matched(a, b_moved,
   tolerance)` (D12), its matching restricted to same-element partners (a
   flag; the mechanosynth callers keep today's unrestricted matching), bonds
   included when asked. If that passes, **refit on the matching it returned**
   — the least-squares motion of the allowed kind (`rigid_fit`, same
   handedness; the centroid translation for `translation`; nothing for
   `none`) — and require every pair of that matching within `tolerance`
   under the refit. Stop at the first motion that passes both.

   The refit is what makes the verdict D2 rather than "within tolerance under
   some motion": refinement stops after 3 rounds whether or not the pairing
   has settled, and its pairing (by label) need not be the matching the
   check finds (by element), so the motion it ends on need not be the
   least-squares fit of that matching. The bond check needs no repeat: it
   reads only the matching. The refit is also what the eigenvalue pre-check's
   bound (§3.3) assumes — centroids together.

A cluster whose atoms are all collinear leaves the turn about their line
undetermined and irrelevant; two anchors suffice. A planar cluster needs no
special case: its mirror image is reachable by a proper motion, which `rigid`
already finds.

After `max_proposals` (default **200**) proposed motions the pair is
**undecided** (D5). The default is sized to time, not to a count that feels
safe: a proposal that pairs most atoms before failing costs up to 3
refinement rounds of ~150 nearest-atom queries, ~50–100 µs, so 200 bound a
pair at ~10–20 ms. The Phase 1 timing test (§6) measures the per-proposal cost and
sets the final number; a typical pair needs the 4–8 principal-axes proposals
and a few anchor triples, so the cap is reached only by highly symmetric
clusters.

```rust
pub struct Comparison {
    pub equal: bool,
    pub undecided: bool,
    pub rejected_by: Option<PreCheck>, // a pre-check (§3.3) decided; no motion was tried
    pub max_deviation: f64,            // of the best fit found; NaN when none was
    pub rmsd: f64,                     // likewise
    pub transform: Option<Fit>,        // rigid_fit::Fit, B onto A; None for `none`
                                       // and when no fit was found
    pub mismatches: Vec<Mismatch>,     // against the best fit
}

/// Which pre-check rejected a pair; `Display` gives a one-line reason
/// ("element counts differ: C 6/5", "bond counts differ: C–Si single 3/2", …).
pub enum PreCheck { ElementCounts, BondCounts, Labels, Eigenvalues, Distances }
```

The "best fit" of an unequal pair is the motion that matched the most atoms,
ties by smaller max deviation, so a near miss is reported against the
alignment a user would pick by eye. Exactly one of these holds: `equal`
(mismatches empty); `rejected_by` is set (mismatches empty — the reason is
the pre-check's); or neither, and `mismatches` is non-empty against the best
fit (with `undecided` set if the cap was reached).

### 3.5 What is guaranteed

- **"Equal" is exact.** It is only ever reported when the final check found
  a same-element matching with every pair within `tolerance` under the
  least-squares fit of that very matching (and the bonds agreeing, when
  compared) — which is D2 itself. However the motion was
  proposed, and whatever the pairing went through on the way, a wrong merge
  is impossible.
- **"Not equal" is very likely, not proven.** It means no proposal led the
  refinement to a passing pairing. The pre-checks are exact (§3.3), and
  `translation` is exact (its one proposal is the least-squares
  translation). For rigid motions, a missed equal pair would need every
  label-consistent anchor triple, and the principal axes, to start the
  refinement too far off to recover; well-conditioned triangles and the
  refinement make that rare, but nothing here proves it impossible.
  Exceeding the work cap is reported as *undecided* rather than hidden.
- The guards on the second point are the tests: a brute-force oracle over
  every matching on small clusters, and known answers on larger ones
  (Phase 1 tests, §6). A missed merge costs a duplicate in the
  list, which `compare_atoms` can then explain.

### 3.6 Grouping

`group_in_order(clusters, settings) -> Grouping { listed, class_of, copies,
compared, undecided }`: for each cluster in the given order, compare it with
the clusters listed so far in its element-count bin; the first equal one (by
index) is its representative; else it is listed itself. `class_of[i]` is the
index of `i`'s representative (itself if listed), `copies[i]` the number of
clusters merged into `i`, `compared` / `undecided` the pair counts. The
comparisons of one cluster against the listed ones run in parallel and the
**lowest-index** equal one is chosen, so scheduling cannot change the answer.

### 3.7 Cost

Estimated, not measured (Phase 1 and 2 timing tests, §6, measure them):

| | |
|---|---|
| cluster size N (`local_hops` 2, H included) | 30–150 atoms |
| a rejected proposed motion | ~1 µs (first atom unpaired) to ~100 µs (fails after refinement) |
| a pair, typical / symmetric | 10–100 µs / ~1 ms |
| a pair at the work cap (D5) | ~10–20 ms, then *undecided* |
| grouping 200 kept candidates | well under 50 ms typical |

Fast enough to rerun on every change of an output setting (D11).

---

## 4. The `compare_atoms` node

Category: atomic structure. A pure function of its inputs:
`atomcad-structure-designer/src/nodes/compare_atoms.rs`, a thin wrapper over
`structure_compare::compare` — cutting the tagged clusters, calling, and
turning the `Comparison` into pins. `CompareReport` is a built-in record type
registered in `node_type_registry.rs`, as the `Chemisorb*` records are.
`Motion` is node data here and in `chemisorb`, so it derives `serde` in
`structure_compare` (the precedent is `field::Colormap`); the node adds no
twin.

| Pin | Type | |
|---|---|---|
| in `a` | `HasAtoms` | |
| in `b` | `HasAtoms` | |
| out `equal` | `Bool` | |
| out `aligned` | `Molecule` | all of `b`, moved onto `a` by the best fit (`b` unmoved when there is none): overlay it on `a` to see the comparison |
| out `report` | record `CompareReport` | `max_deviation`, `rmsd` (Float, NaN without a fit), `undecided` (Bool), `rejected_by` (String, the pre-check's reason, empty if none), `mismatches: Array[String]` (each `Mismatch`'s `Display`) |

Properties: `tag` (empty = all atoms; the same tag on both sides), `motion`
(default `rigid_mirror`), `tolerance` (default 0.15 Å), `compare_bonds`
(default **on**, as `unique` always compares bonds). The defaults are
`unique`'s, so a node with only `tag` set compares as `unique` does.

To find out why two candidates were or were not merged: switch `unique` off
on the `chemisorb` node (instant, D8) so every kept candidate is listed, take
two of them (`array_at` on `candidates`, then the record's `structure`
field) into `a` and `b`, set `tag` to `cs_local`, and set `motion` and
`tolerance` to `unique`'s values (the defaults already match `unique`'s
defaults); keep `compare_bonds` on. `equal` then agrees with `unique`'s
verdict, `aligned` shows the fit, and `report` names the pre-check or the
atoms that broke it. With `motion = none`, `compare_bonds` on and an empty
tag it is the structural equality the mechanosynth generators verify with.

Errors (node errors, not a `false`): `tolerance` outside `(0, 0.3]`; a
non-empty `tag` that no atom of `a`, or no atom of `b`, carries.

Text format: `compare_atoms { tag: "cs_local", motion: rigid_mirror,
tolerance: 0.15, compare_bonds: true }`, the `motion` values spelled as in
D3.

---

## 5. `chemisorb`

### 5.1 Settings

Three new settings and panel controls, **not** `SequentialSearch` fields
(D8): `input_fingerprint` never sees them, and the engine's search knows
nothing of them. They are one domain struct, `DistinctSettings` in
`chemisorption/distinct.rs` (serde, validated there like
`SequentialSearch::validate`), held by `ChemisorbData` beside its
`SequentialSearch` — so the node data mirrors the split between what is
searched and what is listed:

```rust
pub struct DistinctSettings {
    pub unique: Option<Motion>, // None = off; Some(Motion::None) is shown as "exact"
    pub tolerance: f64,         // `unique_tolerance`, validated as in §3.1
    pub local_hops: usize,
}
```

Every field `#[serde(default)]` (D10). The comparison it runs is
`CompareSettings { motion, tolerance, compare_bonds: true, ..default }`.

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
  substrate. The atoms are `Setup::adsorbate_atoms` — ids in the plan's
  combined structure, which are every candidate's ids too.
- **`cs_local`**: depends on `local_hops`. Seeds are the `cs_adsorbate`
  atoms. Expand `local_hops` times along bonds; an H reached is included but
  not expanded from (a terminator); seed H atoms *are* expanded from, so a
  transferred H reaches its acceptor at hop 1. Finally add every H bonded to
  an included atom.

Both are written **at output**, by `distinct.rs`, on a clone of each kept
candidate's structure — the clone `candidate_record` makes today moves into
`distinct()`, so there is still one clone per candidate — whether `unique`
is on or off, so `compare_atoms` can use them
either way. Not in `evaluate` beside `CHANGED_TAG`: that tag is written in two
places (the geometric phase in `evaluate.rs` and the local phase in
`local.rs`), a third tag there would change every stored candidate and the
engine's snapshot tests, and D14 keeps the engine untouched. The plan is
stored with the result (`StoredSearch::plan`), so the setup is at hand. The
debug view's rebuilt structures carry neither tag; they show tree rows, not
listed candidates.

The hop walk is the multi-source BFS `proxy_cut::bond_distances` already does
with H as riders; generalize it there (a seed may start expanded, for the
transferred H) rather than writing a second one in `distinct.rs`.

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

1. Take the kept candidates — the best `top_n` by (strain, change set),
   already cut to the energy window, in rank order.
2. Clone each, write both tags (§5.2), and cut its `cs_local` cluster.
3. With `unique` on, group the clusters in rank order (§3.6, with the
   comparison of §5.1). A copy is dropped from `candidates` and counted on
   its representative.
4. List the rest, keeping their search `rank`, so a gap in the ranks shows
   where copies were merged.

Steps 1–3 are one call in `chemisorption/distinct.rs`:

```rust
pub fn distinct(setup: &Setup, report: &SearchReport, settings: &DistinctSettings)
    -> Result<Distinct, ChemisorptionError>; // Err: the tag limit (§5.2)

pub struct Distinct {
    pub candidates: Vec<DistinctCandidate>, // one per kept candidate, rank order
    pub merged: usize,
    pub undecided: usize,                   // pairs that hit the work cap
}
pub struct DistinctCandidate {
    pub structure: AtomicStructure,  // tagged clone
    pub copy_of: Option<usize>,      // representative's index; None if listed
    pub copies: usize,               // merged into this one; 0 for a copy
}
```

The node calls it with `&stored.plan.setup` and `&stored.report`, and builds
records, stats and its eval cache from the answer (D14).

`top_n` and the energy window are the search's and are applied before
grouping; `unique` only reduces what they kept. `off` lists exactly what is
listed today, plus the two tags.

A merged candidate is still a relaxed candidate in the search tree; with
`unique` on, the debug view's tooltip on its row (`Candidate::row`) says
"copy of #rank" with the representative's rank, so it is not mistaken for
one that fell outside `top_n`.

What the panel shows follows the same answer, through the existing path:
`ChemisorbEvalCache.rows` (the panel's candidate list) holds the listed rows
only, each with its `copies`, and the "copy of" rank rides on
`ChemisorbDebugRowView`, which `chemisorb_api.rs` already serves the debug
view. Their API twins in `structure_designer_api_types.rs` grow to match —
`APIChemisorbRow` (`copies`), `APIChemisorbDebugRow` (copy of),
`APIChemisorbStats` (`merged`, `unique_undecided`) and `APIChemisorbData`
(the three settings) — so Phase 2 includes a
`flutter_rust_bridge_codegen generate`.

### 5.4 Outputs

- Candidate record: new field `copies: Int` — how many kept candidates this
  one stood for, itself excluded (0 with `unique` off or no copy found).
  Copies the search did not keep are not counted.
- Stats record: the existing `listed` now counts candidates *after*
  merging (today it is the kept count, `rows.len()`); new `merged`
  (candidates merged into an earlier one) and `unique_undecided` (pairs that
  hit the work cap). `listed + merged` is the kept count. With `unique` off,
  `listed` is the kept count, as today, and the other two are 0.

The new fields are appended; no pin changes. The pin readout of `candidates`
shows "6 of 20 (14 copies merged)" when `unique` merged anything.

---

## 6. Phases

Rules for every phase:

- Run the suite as `cargo test -j 4` from `rust/` (or `-p <crate>` while
  iterating), then `cargo fmt` (never `--all`) and `cargo clippy`; from
  Phase 2 on also `flutter analyze` from the repository root.
- Tests go in the owning crate's `tests/` directory, declared in its
  `tests/<crate>.rs` beside their siblings. No `#[cfg(test)]` modules.
- **Randomized tests** use a seeded `SplitMix64` (moved in Phase 1 out of
  `chemisorption_sequential_support.rs` into a shared `test_rng.rs` of the
  same test crate) and a fixed case count, and **assert their own
  coverage**: a property test over random pairs that are almost never equal
  would pass with every check deleted, so each one asserts that both
  verdicts (and, for a pre-check, both outcomes) occurred at least a stated
  number of times.
- **Timing tests** are `#[ignore = "timing; run in release"]` and print their
  numbers, as `plan_is_fast_enough_for_every_evaluation` does.
- The Dart layer gets no automated tests; it gets `flutter analyze` and the
  manual walkthrough. The Flutter smoke test is the maintainer's to run: an
  implementing agent lists it as pending and does not run it.
- After any text-format change, the round-trip corpus test must still show
  `query → --replace` as a no-op.

### Phase 1 — Core: `structure_compare`

**Files.** `crates/atomcad-crystolecule/src/structure_compare/` (new; `pub
mod` in `lib.rs`; layout in §3), `mechanosynth/compare.rs` (removed;
`mechanosynth/mod.rs` re-exports from the new home), `rigid_fit.rs`
(`symmetric_eigen_3`), `tests/crystolecule/structure_compare_test.rs` and
`rigid_fit_test.rs` (new), `test_rng.rs` (new, `SplitMix64` moved), the
crate's `AGENTS.md` (module map; the "Position matching" paragraph names the
new home of `compare_structures`). No UI.

**Deliverables.** Everything in §3: `Motion`, `CompareSettings`,
`Proposals`, `Comparison`, `PreCheck`, `Cluster`, the pre-checks as `pub`
functions of `invariants.rs`, the proposals and verification,
`compare_structures_matched` and the same-element flag, `group_in_order`.

**Automated tests — against a brute-force oracle**
(`structure_compare_test.rs`).

**The oracle is D2, literally.** `brute_force_equal(a, b, settings)`: every
same-element bijection (with bonds compared, also agreeing on every bond);
for each, the least-squares fit of the allowed kind — `rigid_fit` proper,
and improper too under `rigid_mirror`; the centroid difference for
`translation`; identity for `none` — and its max deviation against
`tolerance`. Equal iff some bijection passes. Clusters of at most 8 atoms
(≤ 8! bijections, far fewer per element).

**The pair generator** draws clusters of 2–3 elements, optionally bonded,
that satisfy §3.1's precondition (every same-element distance above
`4·tolerance`), and pairs them so that both verdicts are common: B = A under
a random motion of each kind, shuffled, jittered by up to `1.5·tolerance`;
A's mirror image; A with one atom moved by about `tolerance`; one bond
changed; an unrelated cluster with the same element counts.

On every generated pair, for every motion kind:

- **The pipeline agrees with the oracle.** "Equal" with the oracle false
  would be a wrong merge; "not equal" with the oracle true is a missed merge
  on a small cluster. Both fail the test.
- **The verdict is D2.** For every "equal", the least-squares fit of the
  returned matching has every pair within `tolerance`.
- **Every pre-check is exact.** Each one (element counts, distance list, bond
  counts, labels, eigenvalues), called directly: none rejects an
  oracle-equal pair, and each rejects some pairs (not vacuous).
- **Principal axes and anchors agree.** `CompareSettings::proposals` runs
  each path alone: the anchor path alone agrees with the oracle, and the
  principal-axes path alone never says equal where the oracle does not.

**Outside the precondition** (same-element pairs closer than
`4·tolerance`), the generator runs again: "equal" still implies the oracle —
the guarantee that does not depend on it — while missed merges are allowed
and counted, not failed.

**Automated tests — known answers on larger clusters**
(`structure_compare_test.rs`). Beyond brute-force size, the expected verdict
comes from the construction:

- **Invariance.** B is a random 30–150-atom cluster A under a random motion,
  shuffled, jittered by up to `tolerance`. The expected verdict is
  **computed, not assumed**: the least-squares fit of the known matching, and
  its max deviation against `tolerance` (D2). A jitter bound alone does not
  decide it — the fit spreads the jitter, so one atom can end up farther off
  than its own jitter (translation, three atoms jittered `+0.9t`, `−0.9t`,
  `−0.9t` along x: the fit shifts by `−0.3t` and leaves the first `1.2t`
  off). Both verdicts occur. Each motion kind rejects the motions it does not
  allow (`translation` a rotated copy, `rigid` the mirror image of a chiral
  cluster).
- **Chirality and asymmetry.** A chiral cluster vs its mirror image: equal
  under `rigid_mirror`, not under `rigid`. An asymmetric molecule with two
  feet swapped over two *inequivalent* sites: not equal under any motion.
  The same swap over the two atoms of a symmetric dimer (related by the
  dimer's twofold axis): equal under `rigid` — it is the same binding.
- **Bonds.** Same positions, one bond order different: equal without bonds,
  not with them — rejected by the bond counts (`rejected_by =
  BondCounts`), before any motion is tried. Same bond counts, bonds on
  different atoms: rejected by the final check.
- **Degenerate shapes.** Collinear and planar clusters; a ring and a
  threefold cluster, slightly jittered (equal or near-equal eigenvalues):
  each equal to a moved copy of itself, found by the anchor fallback when
  the axes fail.
- **Refinement recovers swaps.** A proposal deliberately rotated by a few
  degrees, so that the first pairing swaps two H atoms, still verifies after
  refinement.
- **Same-element pairing.** A C–H pair closer than `3·tolerance` after a
  rough proposal is never paired across elements.
- **`translation`** finds a shifted, jittered copy with its single
  proposal, and rejects a rotated one.
- **Undecided.** An equal pair with degenerate principal axes (the ring
  above), so the first proposal fails, with `max_proposals = 1`: comes back
  undecided and not equal. (A generic symmetric pair would not do: its first
  principal-axes proposal can pass, and the test would see "equal".) Across
  all tests, `undecided` never comes with `equal`.
- **The three outcomes** (§3.4): every `Comparison` is exactly one of equal,
  rejected by a pre-check, or unequal with mismatches. A copy with one atom
  displaced by `3·tolerance` reports the copy's motion as `transform` and
  one mismatch naming that atom.

**Automated tests — the pieces underneath.**

- **`rigid_fit_test.rs`** (new; the fit has been tested only through its
  callers): `symmetric_eigen_3` returns orthonormal eigenvectors that
  reconstruct the matrix, sorted eigenvalues, repeated eigenvalues handled,
  and bit-identical output on repeated calls. The existing mechanosynth and
  chemisorption tests stay green on the generalized solver.
- **`compare_structures`** after the move: the existing mechanosynth tests
  pass unchanged through the re-export; `compare_structures_matched`
  returns a matching consistent with its mismatch list; the same-element
  flag off reproduces today's behaviour.
- **Grouping** (`group_in_order`): the same answer in rayon pools of 1, 2
  and 8 threads; each class keeps its first member; a non-transitive chain
  (A equals B, B equals C, A not C) groups as {A, B}, {C}; `class_of` and
  `copies` agree; undecided pairs are counted.

**Timing** (`#[ignore]`, release):

- Grouping 200 hexapod-sized clusters at hop 2, `rigid_mirror`: under
  0.25 s.
- One pair driven to the work cap (a highly symmetric cluster against a near
  miss): under 20 ms; the measured per-proposal cost sets `max_proposals`
  (§3.4).

### Phase 2 — `chemisorb` lists distinct bindings

**Files.** `crates/atomcad-crystolecule/src/chemisorption/distinct.rs`
(new; `pub mod` and re-exports in `chemisorption/mod.rs`), `proxy_cut.rs`
(`bond_distances` generalized, §5.2), `chemisorption/sequential/config.rs`
(the `top_n` default, D13), `tests/crystolecule/chemisorption_distinct_test.rs`
(new); `crates/atomcad-structure-designer/src/nodes/chemisorb.rs` (settings
in `ChemisorbData`, output building from `Distinct`, record and stats
fields, eval cache, readout), `chemisorb_node_test.rs`;
`rust/src/api/structure_designer/structure_designer_api_types.rs` and
`chemisorb_api.rs` (the twins of §5.3), FRB regeneration; the Flutter
`chemisorb` panel (three controls, the `copies` column, the debug-view
tooltip). Docs: the `chemisorb` entry in
`doc/reference_guide/nodes/atomic.md`, and the crate `AGENTS.md`'s
chemisorption section (output settings are not search settings; tags are
written at output).

**Deliverables.** §5 in full. The engine's search (`sequential/`) is not
touched apart from the `top_n` constant, so its tests (oracle, brute force,
golden data) are unaffected; tests that relied on the old default pin 10
explicitly.

**Fixtures.** A real search: a one-foot adsorbate (silyl, from the support
file) on `si100_slab`, so every interior dimer atom is a site and
translational and mirror copies are among the kept candidates (`top_n`
large enough to keep them all). The transferred-H cases use a second search
with an H transfer rule (`transfers: [H → substrate]`), as the existing
transfer tests set one up.

**Automated tests — the crate side** (`chemisorption_distinct_test.rs`; D14
makes this testable without a node):

- **`cs_adsorbate`** is on exactly the adsorbate's atoms, including a
  transferred H on the substrate.
- **The local cluster:** seeds, the H rule, a transferred H reaching its
  acceptor at hop 1, `local_hops = 0` equal to the adsorbate. The existing
  `proxy_cut_test.rs` stays green on the generalized `bond_distances`.
- **Merges are sound.** Every merged pair's sites are related by a surface
  lattice translation or (under `rigid_mirror`) a mirror of the slab —
  checked independently, from the site coordinates and the slab's cell.
- **Merges happen.** Two interior sites related by a lattice translation,
  both with their hop-2 clusters clear of the slab edge, are one listed
  candidate; under `rigid_mirror` also a mirror pair.
- **Listed candidates are distinct.** No two listed candidates are equal by
  the core at the same settings, and `listed + merged` equals the kept count.
- **`off` is today.** With `unique: off` every kept candidate is listed, in
  order, with the two tags added and `copies` 0.
- **Determinism.** The same answer from results searched at 1, 2 and 8
  threads.
- **Tag limit.** A substrate already carrying 31 tags gives the tag error,
  not a panic.
- **Settings validation:** `unique_tolerance` outside `(0, 0.3]` is a
  config error.

**Automated tests — the node** (`chemisorb_node_test.rs`, only what the node
adds):

- **Not fingerprinted.** Changing `unique`, `unique_tolerance` or
  `local_hops` after a Run does **not** make the stored result stale, and
  the output changes accordingly.
- **The node reflects `Distinct`:** the `candidates` pin, `stats` (`listed +
  merged` is the kept count) and the eval cache's rows agree with it.
- **Defaults and persistence:** a new node's `top_n` is 50 and a loaded
  node's is unchanged; a node saved before this change loads with the
  `DistinctSettings` defaults; text-format round trip of the three new
  properties.
- **Snapshots:** the node-type snapshots (`node_snapshots`, the new record
  fields) and the `candidates` snapshots under the default settings change;
  review them with `cargo insta review`.

**Timing** (`#[ignore]`, release): `distinct()` on the hexapod fixture's
stored result, printed beside the plan's time, so an output rebuild can be
compared with an evaluation.

### Phase 3 — The `compare_atoms` node

**Files.** `crates/atomcad-structure-designer/src/nodes/compare_atoms.rs`
(new; registered in `nodes/mod.rs` and `node_type_registry.rs`, with the
`CompareReport` record type), `tests/structure_designer/compare_atoms_test.rs`
(new); its property panel; a new entry in
`doc/reference_guide/nodes/atomic.md`.

**Deliverables.** §4 in full, including the text format.

**Automated tests** (`compare_atoms_test.rs`):

- Pins and report on a known equal pair, a known unequal pair (one
  mismatch, `aligned` moved by the best fit) and a pre-check rejection
  (`rejected_by` set, `aligned` = `b` unmoved).
- **Agrees with `unique`:** on Phase 2's fixture with `unique` off, with only
  `tag` set to `cs_local`, `equal` agrees with `unique`'s verdict on every
  pair of kept candidates.
- Errors: a tolerance out of range; a non-empty tag missing from `a`, and
  from `b`.
- Text-format round trip of the four properties; the node-type snapshot of
  the new node, reviewed with `cargo insta review`.

### Phase 4 — Walkthrough (maintainer, by hand)

An implementing agent lists this as pending, together with the Flutter smoke
test, and runs neither. With mechadense's results: `local_hops` 1 vs 2,
tolerance 0.1 / 0.15 / 0.25, `rigid` vs `rigid_mirror`; look at every pair
hop 1 merges and hop 2 keeps apart, using `compare_atoms`. Count the
distinct bindings listed at `top_n` 10 and 50 on a realistic proxy: if 50
still lists only a handful, D13's default is too low (or D7 needs
revisiting).

---

## 7. Open questions

- **Default tolerance** (0.15 Å) — to be set from the walkthrough.
- **Show the merged copies?** A user may want to see where the copies sat
  (e.g. to pick the one farthest from the slab edge for a simulation).
  Switching `unique` off shows them all; a per-candidate list of its copies'
  ranks on the record would be cheap. Not proposed until asked for.
- **A `unique_atoms` node** for arrays from other producers, on the same
  core — if asked for.
