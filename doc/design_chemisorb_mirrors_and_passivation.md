# Design: mirrored bindings and passivated sites in `chemisorb`, clash-aware `passivate`

Status: **reviewed 2026-10-15; ready for implementation.** Phases 1–3 are
agent work; Phase 4 is the maintainer's.

Three additions, requested by mechadense for a chemist who runs `chemisorb`
as a brute-force search and wants to be shown, not told, what the pruning
rules drop:

1. **`chemisorb` → `include_mirrored`** (bool, default off). Relax the
   three-leg assignments the mirror check would prune, instead of pruning
   them.
2. **`chemisorb` → `passivate_sites`** (bool, default off). Before a
   candidate is relaxed, cap every site it does not bond to with H, so the
   binding is relaxed on a passivated surface with only its own sites bare.
3. **`passivate` → `skip_clashes`** (bool, default off, with
   `clash_factor`). Leave an open valence uncapped when its terminator would
   sit in a bad steric clash.

Features 2 and 3 are independent. `passivate_sites` places its H **without**
the clash rule (D8): it caps the start structure, where the molecule is
usually still far from where the relaxation takes it, so a clash measured
there says little about the relaxed binding. The rule lives in
`hydrogen_passivation.rs` as a general-purpose option, and today the
`passivate` node is its only caller.

Background: `doc/design_chemisorption_sequential.md` (the search; §4.5
seating and the mirror check, §4.6 the local phase, §4.8 the reference
state, §13.4 the Phase 0 evidence on mirrored hypotheses) and
`doc/design_hydrogen_passivation.md` (placement).

---

## 1. Motivation

### 1.1 Mirrored assignments

For three legs on three sites, the 3! assignments of feet to sites split into
two halves. Seen from the open side of the surface, three go round the sites
in the same direction as the feet go round the molecule, and a rotation seats
them body-up. The other three go round the opposite way. A rotation can seat
them only with the body inside the substrate; only a reflection, which gives
the mirror-image molecule, seats them body-up. The plan prunes the second
half (`Mirror::Mirrored`, `setup.rs`) and keeps the ones it cannot decide.

The rule is sound for a molecule whose feet keep their arrangement. Phase 0
measured it (§13.4 of the sequential design): no mirror-pruned tripod
hypothesis relaxed to within 329 kcal/mol of the best accepted binding. Two
reasons still justify a switch:

- **A floppy adsorbate can realise a reversed assignment.** Feet on long,
  flexible linkers can cross, giving a crossed-arm conformer. The prune
  assumes this cannot happen, and only a relaxation shows whether it does.
- **The prune is invisible.** A user enumerating every combination sees
  4,896 three-leg hypotheses and 2,658 relaxations, and has to take the gap
  on trust (mechadense's run on 2026-10-15: 3 feet, 18 sites, 2,238
  mirror-pruned, 420 undecided). Relaxing them turns that trust into a
  result. Together with `unique`, it also shows which mirrored starts end in a
  proper binding.

What the user pictured is "keep the molecule upright, declare the three bonds,
and let UFF pull." That is what a mirrored hypothesis's start geometry
becomes (§3.2).

### 1.2 Passivated sites

The substrate is relaxed with every site bare. On a reactive surface (a
depassivated patch with a handful of radicals) that is the wrong environment
for the sites a binding does *not* use. In reality those sites are capped, and
their H atoms crowd the molecule. With all of them bare, UFF sees neither the
crowding nor the H–H contacts. `passivate_sites` relaxes each candidate on the
surface it would actually meet: its own sites bare, every other site capped.

### 1.3 Clash-aware passivation

`passivate` places a terminator on every open valence along the ideal
direction and never looks at what is already there. On an unreconstructed
(100) dihydride, at a concave corner, or next to a bound adsorbate, that
produces H–H or H–heavy-atom contacts far inside van der Waals contact. A
relaxation cannot repair such a contact without large distortion. The new
option skips those terminators and leaves the host's valence open. This is
the right result for a host whose open bond points into something.

---

## 2. Decisions

- **D1. `include_mirrored` is a search setting.** It changes what is
  relaxed, so it is fingerprinted, and changing it makes a result stale.
  Default off: the evidence for the prune (§13.4) stands, and the default
  search does not change.
- **D2. A mirrored hypothesis is seated upright on two of its legs, and the
  third bond is declared stretched** (§3.2). It is never seated by a
  reflection, which would relax the enantiomer: a different molecule for a
  chiral adsorbate, and a relabelled proper binding for an achiral one. It is
  never seated by the proper Kabsch fit either, because that puts the body in
  the substrate (683 of 684 such seatings clashed in Phase 0).
- **D3. Mirrored hypotheses are ordinary hypotheses once included.** They
  are candidates if the filters admit them, local-phase parents if the
  adsorbate has four or more feet, and are subject to `clash_filter` like
  any other seating. Each candidate carries a `mirrored` flag.
- **D4. `passivate_sites` caps the open valence the binding leaves on its
  sites.** The hosts are exactly the sites of the search: substrate atoms with
  a free valence that pass `substrate_tag`. Each gets H on every valence still
  open **after** the candidate's legs and transfers are bonded. An unused site
  is capped in full. A site the candidate uses keeps its leg, and any second
  valence it has left is capped too (maintainer decision 2026-10-15; rare in
  practice, since it needs a site with two free valences). Untagged substrate
  atoms and the adsorbate are never touched; the adsorbate's unbonded feet
  stay radicals, because they are part of the binding as found.
- **D4a. H only.** The passivant is hydrogen. A halogen-terminated surface
  would reuse `passivant_element`, but nobody has asked for one.
- **D5. Only candidate relaxations are passivated.** A local-phase parent is
  relaxed bare, because the next leg bonds to a site that passivation would
  have capped. A hypothesis that is both a candidate and a parent is relaxed
  twice, once each way, and the plan counts both (§4.3).
- **D6. The reference stays atom-conserving.** A passivated candidate's strain
  is measured against the adsorbate relaxed alone plus the substrate **with
  the same H atoms** relaxed alone. The reference therefore depends on how
  many H each host received; it is built from that `(host, count)` list
  alone, and one reference relaxation is run per distinct list in a search
  (§4.4).
- **D7. The clash rule is van der Waals based, greedy, and lives in
  `hydrogen_passivation.rs`.** A terminator is skipped when it would sit
  closer than `clash_factor × (r_vdW(terminator) + r_vdW(other))` to any atom
  other than its host and the host's bonded neighbours, or to a terminator
  this pass already placed. Hosts are visited in atom-id order. Covalent radii
  are the wrong scale here: they put an H–H clash below 0.6 Å, which would
  let through every dihydride clash this option exists to catch.
- **D8. `chemisorb` does not use the clash rule** (maintainer decision
  2026-10-15, superseding the first draft's "always on"). `passivate_sites`
  caps on the start structure, and in most seatings the molecule there is
  several Å from where it relaxes to (§8.0), so a skip decided there removes
  H where the molecule *was*, not where it ends up. Every open valence is
  capped. An H that starts inside the molecule is expected to be pushed out
  by the relaxation; whether it is sometimes trapped under the molecule
  instead is what the walkthrough checks (Phase 4). Without skips, the H a candidate gets depends only on
  which sites it uses, which also keeps `unique` and the reference keys
  (§4.4) stable.
- **D9. The passivating H atoms are tagged `cs_passivant`** in every
  candidate structure of a search with the setting on, so an `apply_style`
  rule can show or hide them. A structure holds at most 32 tag names
  (`MAX_TAGS`), and chemisorb already writes three (`cs_changed`,
  `cs_adsorbate`, `cs_local`). With the setting on it writes four, so the
  inputs may carry at most 28 instead of 29. Inputs over the limit fail the
  way they do today: `TagError::LimitReached`, reported as an error, never a
  panic.

---

## 3. `include_mirrored`

### 3.1 Plan

`SequentialSearch` gains `include_mirrored: bool`. In `plan.rs`, `found`
computes the mirror verdict exactly as today. The verdict prunes only when the
setting is off:

```rust
let pruned = mirror == Some(Mirror::Mirrored) && !self.config.include_mirrored;
let candidate = !pruned && admits(…);
let parent = !pruned && self.local_phase && depth == GEOMETRIC_LEGS;
```

`found` returns `pruned` where it returns the verdict today, so the caller's
`extend` gate reads the same value. The geometric depth cap is three legs, so
this matters only for consistency, but the two must not diverge. `Hypothesis::mirror` keeps the verdict either way, so a candidate
knows it was mirrored.

Statistics: `pruned_mirror` keeps meaning *pruned* and is 0 with the setting
on. A new `PlanStats::mirrored_kept` counts the Mirrored verdicts that were
kept. `pruned_mirror + mirrored_kept` is then the number of Mirrored verdicts
whatever the setting.

### 3.2 Seating a mirrored hypothesis

A new `Setup::seat_mirrored(steps)` seats a Mirrored set of legs, and `plan`
calls it instead of `seat` for a Mirrored hypothesis it keeps. `seat` itself
does not change. Everything downstream (`evaluate`, `replay_start`, the debug
view) reads the seating stored on the hypothesis and calls `seat` only when
none is stored. With the setting off, a pruned mirrored row therefore still
shows the proper Kabsch seating in the debug view, with the body in the
substrate, which is the picture of why it was pruned. `seat_mirrored`:

1. For each of the three pairs of legs, in sorted leg order, compute the
   two-leg seating that `seat` already does for two legs: align the foot axis
   with the seating axis, then apply the θ rule (body up, first clash-free
   turn).
2. For each pair, measure the third foot's distance from its seating point.
   Keep the pair with the smallest distance; ties go to the earlier pair.
3. The seating is that pair's rotation and translation. `residual` is the
   third foot's distance, typically several Å, and the panel shows it. The
   clashes and the moved transfer atoms are computed with **all three** legs,
   exactly as for any seating.

The start structure then declares all three bonds (`start_structure` already
adds every step's bond whatever the geometry), so the third leg starts as a
stretched spring and UFF pulls. Everything in the choice is a function of the
sorted legs, so "the start geometry is a function of the change set", and
with it the change-set dedupe, still holds.

`seat` should be split into a placement half (rotation, translation) and a
finishing half (residual, clashes, moved) so that `seat_mirrored` reuses
both without copying.

**Plan time.** Today a mirrored hypothesis is never seated. With the setting
on, each one costs three two-leg θ searches. The plan runs on every
evaluation, and `plan_is_fast_enough_for_every_evaluation` guards 0.25 s on
the hexapod. Phase 2 measures the cost with the setting on. If it breaks the
guard, follow the sequential design's §4.9 remedy: move per-row seating out of
`plan`, without narrowing the enumeration.

### 3.3 Outputs, panel, debug view

- `ChemisorbCandidate` gains `mirrored: Bool`. New fields are **appended**
  to the record.
- The candidate list marks a mirrored row `mirr.`, and its hover adds "seated
  on two legs; third bond started at N Å".
- The statistics line "Mirror: pruned / undecided" becomes
  "Mirror: pruned / kept / undecided".
- Debug view: with the setting on, mirrored rows are ordinary rows, not
  greyed, and their next-foot items say `mirr. (kept)`. **Show mirrored** only
  lists pruned rows, of which there are none, so it is disabled with a tooltip
  saying why. A mirrored row's seated form is the two-leg seating above; its
  relaxed form behaves like any other row's.
- `unique` needs no change. A mirrored start that relaxes into a proper
  binding merges into it, and the representative's `copies` counts it; the
  debug tree already marks such a row "copy of rank N". Expect few merges for
  a rigid adsorbate. Seated on two legs, it usually cannot swing its third
  foot across, and it ends strained instead. A mirrored candidate that stays
  unmerged *and* ranks well is the interesting case the option exists to
  show.

---

## 4. `passivate_sites`

### 4.1 What is capped

For a candidate, passivation runs on its **start structure**, where the legs
and transfers are already bonded. It calls `add_hydrogens_filtered` with
`eligible = |id| site_ids.contains(&id)` (every `Setup::sites` id) and
`clash: None` (D8). Because the bonds are already there, the open slots it
finds on a site are exactly what the binding left. An unused site gets all of
its slots, a used site its leftover slots (none in the common case), and an
acceptor whatever the transferred atom did not take. No list of used sites
is needed.

- The number of H per host is `open_valence_slots` (the passivation code's
  count), not `free_valence` (the search's count). The two agree for sp³
  Si/C/Ge hosts, the case this is for. Where they differ (a radical carbon
  typed sp²), the placement code is the one that knows the directions, so it
  decides. The reference guide says so.
- No H is skipped, even one that starts inside the molecule (D8). The
  candidate records how many H were placed.
- `bond_inventory` and the `bonds` field do not change. The passivating
  Si–H bonds are not bonds the binding forms.
- **`skip_already_passivated` must be `false` here.** A site made by
  deleting a surface H keeps the hydrogen-passivation flag. On mechadense's
  network all 18 sites carry it, so the default options place **no H at
  all** (measured 2026-10-15). `add_terminators_counted` (§4.4) must not
  skip flagged hosts either.

### 4.2 Where it happens

- **Geometric phase** (`evaluate.rs`). For a hypothesis that is a candidate:
  `start_structure` → passivate → relax. If it is also a parent, a second
  relaxation from the bare `start_structure` supplies the frontier positions.
  A parent-only hypothesis is relaxed bare, as today.
- **Local phase** (`local.rs`). With the setting on, a level runs in two
  batches:
  1. Relax the paths that need a **state**, i.e. every path when the level is
     not the deepest, bare and exactly as today. Dedupe by change set on the
     bare strain, which gives the winners and the next frontier.
  2. Relax each **kept candidate** passivated, from its winning path's start
     geometry (the relaxed parent plus the new leg, then passivated).

  At the deepest level, step 1 has no paths that need a state. The candidates
  are relaxed passivated directly, and the dedupe uses the passivated strain.
  This asymmetry is deliberate: above the deepest level the winning path
  must be the one whose bare state the next level grows from, while at the
  deepest level nothing grows further. With the setting off, a level is
  unchanged: one batch.
- **Replay** (`replay`, `replay_start`). The relaxed form of a candidate's row
  is the passivated relaxation; a parent-only row's is the bare one. A row
  that is both shows the passivated relaxation, and its hover says the deeper
  rows grew from the bare one. `replay` takes the row's role from its
  hypothesis and passivates exactly as the search did, so it reproduces the
  recorded strain. The existing debug-view test that replay equals the search
  must pass with the setting on.

### 4.3 Counting and budget

A hypothesis that is a candidate *and* a parent costs two relaxations when
`passivate_sites` is on. Today `PlanStats` counts such a hypothesis once, in
`candidates` (`parents` counts parent-only hypotheses), and `to_relax` is a
list of hypotheses. With the setting on:

- `PlanStats` gains `twice`: the queued (not clash-pruned) hypotheses that
  are both candidate and parent.
- `PlanStats::to_relax` counts **relaxations**: the queued hypotheses plus
  `twice`. The plan's list of queued hypotheses stays a list of hypotheses.
- The invariant becomes
  `candidates + parents + twice == to_relax + pruned_clash` (untruncated).
- `budget` counts relaxations. A hypothesis is queued only if all of its
  relaxations fit in what is left; otherwise the plan is truncated there.
  A hypothesis is never half relaxed.

The count next to Run already shows `to_relax`, so it is correct by
construction. In the local phase, `LevelStats::to_relax` counts both
batches. The panel's "+ local phase" note is unchanged. Reference relaxations
(§4.4) are not counted against the budget, as the separated reference's two
relaxations are not counted today, but the progress total includes them.

With `formed_bonds` set to the deepest leg count, or for any adsorbate with
three feet or fewer, no hypothesis is both a candidate and a parent. Then
`twice` is 0 and the option costs only the reference relaxations.

### 4.4 The reference

The strain of a passivated candidate is

`E(candidate) − E(adsorbate alone) − E(substrate + the candidate's H, alone)`

where the second substrate is the input substrate carrying **the same H
atoms as the candidate**: the same number on the same hosts. The atoms match
the candidate's exactly, which is the property §4.8 of the sequential design
rests on.

**The reference is a function of its key, never of the candidate that first
needed it.** Its key is the list of `(host, count)` pairs the candidate's
passivation placed, sorted by host id. Its start structure is built from the
key alone: on the bare input substrate, each host gets the **first `count`**
of the slots the placement rule gives it there, and the result is relaxed.
On a used site the bare substrate has more open slots than the candidate's
start structure did, which is why the count is part of the key. Copying the
candidate's H positions instead would make the reference depend on which
candidate reached a key first, and that order varies with the thread count.
`hydrogen_passivation.rs` gains a small `add_terminators_counted(structure,
&[(host, count)], element)` for this, built on the same direction code.

With no clash rule (D8), the key depends only on which sites the candidate
uses and what valence each has left, so it varies between candidates only
because they use different sites. `evaluate` therefore keeps the reference
relaxations of one search in a map from key to `(energy, StrainTerms)`.
This is not a speculative cache. Without it, one search runs one reference
relaxation per candidate instead of one per key: for mechadense's run, 2,658
instead of at most 816 (one per unordered site triple). The map lives on
`SearchReport`, because `replay` needs the same references. It is small: two
numbers and a short list per entry.

**The references are relaxed before the candidates that need them.** A
candidate's strain is needed the moment its relaxation finishes (`keep_best`
applies `top_n` and the window while relaxing), so the map must already hold
its key. The key is cheap to compute: passivate the start structure, read
`(host, count)`, and drop the structure. So:

1. Before the geometric phase's batch, compute the key of every queued
   candidate. Collect the distinct keys, sorted.
2. Relax those references in parallel and fill the map.
3. Relax the candidates.

Each local level does the same for its kept candidates before its
passivated batch, relaxing only keys not already in the map. No thread waits
on another for a reference, nothing is relaxed twice, and the progress total
is known before each batch starts.

`SearchReport::reference_energy` and `reference_terms` stay as the bare
reference and are used by every bare relaxation. A passivated candidate's
`terms` are taken against its own reference.

**Comparability.** As today, strains compare cleanly within one bond
inventory. Each candidate's reference carries exactly its H, so each strain
is still the cost of forming that candidate's bonds.

### 4.5 Fingerprint, outputs, panel

- `input_fingerprint` hashes both new `SequentialSearch` fields; its
  destructuring makes a forgotten field a compile error.
- `ChemisorbCandidate` gains, appended: `passivants: Int` (H placed), 0 with
  the setting off.
- The candidate structures carry `cs_passivant` on the placed H (D9).
- The panel gets two checkboxes in the search settings: *Include mirrored
  assignments* and *Passivate unused sites before relaxing*. They sit under
  *Prune seatings that clash*. A hover on a row adds "N H placed".
- Text format: `include_mirrored: true`, `passivate_sites: true`. Both are
  written only when true, like the existing optional settings, so saved
  networks and the round-trip corpus are unchanged.

---

## 5. `passivate` → `skip_clashes`

### 5.1 The rule (core)

`AddHydrogensOptions` gains `clash: Option<f64>`, the factor. `None` means
today's behaviour, bit for bit. With a factor `f`:

- A candidate terminator at `p` on host `h` is **rejected** if, for any atom
  `a` within reach, `|p − a| < f · (r_vdW(passivant) + r_vdW(a))`, where `a`
  ranges over every atom of the structure except `h` and `h`'s bonded
  neighbours, plus every terminator this pass has already **accepted**.
- Hosts are visited in atom-id order, slots in the order
  `compute_open_directions` returns them. The first terminator of a clashing
  pair is kept, the second skipped. Greedy and deterministic: a dihydride row
  keeps one H of each clashing pair rather than none.
- Excluding the host's neighbours (the terminator's 1–3 partners) is
  necessary. On carbon an H sits about 2.15 Å from the host's neighbouring C,
  which is inside any useful fraction of the C–H van der Waals sum.
- The neighbour scan uses the structure's spatial grid. Accepted terminators
  are kept in a small grid of their own, so the pass stays linear.

`AddHydrogensResult` gains `skipped: usize`. The two-step analyse-then-mutate
structure of `add_hydrogens_filtered` stays: the clash test runs in the
analysis step, against the structure plus the accepted list.

`DEFAULT_PASSIVATION_CLASH_FACTOR` is a public constant next to the function,
proposed **0.7**: H–H 1.68 Å, H–C 2.08 Å, H–Si 2.37 Å with the table's radii.
Phase 1 confirms it against the fixtures in its tests; the tests, not this
paragraph, define the acceptable bracket.

### 5.2 The node

`PassivateData` gains `skip_clashes: bool` (serde default false) and
`clash_factor: f64` (serde default `DEFAULT_PASSIVATION_CLASH_FACTOR`, must
be > 0; a non-positive value is an eval error naming the property). No new
pins. The eval-cache message becomes "Added N H atoms, skipped M (steric
clash)" when the option is on.

Panel (`passivate_editor.dart`): a *Skip terminators that clash* checkbox,
and under it the factor field, greyed while the box is unticked. The API twin
`APIPassivateData` gains both fields, and the getter and setter keep their
`scope_path` parameter.

Text format: `skip_clashes: true, clash_factor: 0.7`, written only when
`skip_clashes` is true.

The other callers of `add_hydrogens` (`atom_edit`'s one-click passivation,
`materialize`, `lattice_fill`, `proxy_cut`, and `chemisorb`'s
`passivate_sites`, D8) pass `clash: None`.

---

## 6. Phases

### Phase 1 — Clash-aware passivation (core + `passivate` node)

Build §5: the `clash` option and `skipped` count in
`hydrogen_passivation.rs`, the constant, the two node properties, the API
twin, the panel, the text format, and the `passivate` section of
`doc/reference_guide/nodes/atomic.md`.

Tests (`hydrogen_passivation_test.rs`, `passivate_node_test.rs`):

- `clash: None` is bit-identical to today on every existing passivation
  fixture: same atoms, positions and ids.
- Si(100) 2×1 monohydride and Si(111) 1×1: **zero skipped** at the default
  factor.
- Unreconstructed Si(100) 1×1 dihydride and C(100) 1×1 dihydride: skipped
  > 0, and the output is checked by a brute-force O(n²) oracle written
  independently of the implementation, in both directions:
  - *Sound:* no two non-excluded atoms are inside the rule.
  - *Not over-eager:* every skipped slot, added alone to the output, would
    clash with something in it. Without this half, a pass that skips every
    terminator would pass the test.
- Greedy order: of a clashing pair, the terminator on the lower-id host is
  kept; renumbering the hosts swaps which one survives.
- The 1–3 exclusion: a methyl-terminated carbon chain passivates fully at the
  default factor.
- A terminator that would sit inside an unrelated molecule placed above the
  surface is skipped; moving the molecule away restores it.
- Node: serde-default loading of an old `{}` / `{"element":1}` payload; a
  non-positive factor is an error; both properties round-trip through the
  text format and `.cnnd` when `skip_clashes` is on, and nothing is written
  when it is off; the round-trip corpus stays a no-op.

Record the measured nearest-pair distances of the four surface fixtures in
this document (a new §8.1), and adjust the default factor if 0.7 does not
separate them.

### Phase 2 — `include_mirrored`

Build §3: the setting, the `pruned` gate, `mirrored_kept`, `seat_mirrored`, the candidate field, the panel and debug-view changes, the
fingerprint, the text format, the reference guide (*How a binding is built*,
*Properties*, the statistics list) and the crystolecule `AGENTS.md` rule
"Mirror check vs clash filter" ("pruned always" becomes "pruned unless
`include_mirrored`").

Tests (`chemisorption_sequential_test.rs`, `…_relax_test.rs`,
`…_debug_test.rs`, `chemisorb_node_test.rs`):

- Off: plan, stats and snapshots unchanged (the existing golden and insta
  tests are the check).
- On: `pruned_mirror == 0` and `pruned_mirror(off) == mirrored_kept(on)`.
  `to_relax(on) == to_relax(off) + mirrored_kept` holds only when every
  three-leg hypothesis is relaxed, so the fixture sets that up: clash filter
  off, no `formed_bonds` / `bond_inventory` filter that rejects three legs,
  and not truncated.
- The oracle (§11.1 of the sequential design) gains the setting: with it on,
  every three-leg assignment within the shells is a hypothesis, and the
  count equals the naive enumeration's.
- Mirrored seating: body up (centroid above the seating plane along the local
  up), exactly two feet at their seating points, the third foot's residual
  equal to the minimum over the three pairs; the result is invariant under
  binding order (permute the steps).
- A tripod relaxation from a mirrored start converges or is marked
  unconverged, never errors. Its `mirrored` flag is set.
- An achiral tripod with three equivalent feet: every mirrored candidate is
  either merged by `unique` into a proper one, or its strain is above the
  best proper candidate's in the same inventory. For an achiral molecule a
  mirrored start cannot reach a binding the proper search lacks, so a
  mirrored candidate that ranks first and stays unmerged means a bug. The
  test does **not** require every one to merge: a rigid tripod seated on two
  legs usually cannot swing its third foot across, and it ends strained
  instead.
- Replay and debug view: with the setting on, a mirrored relaxed row's
  `replay` reproduces its recorded strain, and its seated form is the
  `seat_mirrored` seating. With it off, a pruned mirrored row's seated form
  is unchanged (the proper Kabsch seating).
- Plan time with the setting on, under the existing timing guard.
- Node: the property round-trips through the text format and `.cnnd` and is
  written only when true; a node saved without it loads with it off; toggling
  it makes a stored result stale and undo restores it; `mirrored` is the
  record's last field so far (appended).

### Phase 3 — `passivate_sites`

Build §4: the setting; the capped-host rule; passivated candidate
relaxations in both phases; the two-batch local level; the per-key
reference map, relaxed before each batch; replay; `twice`; the candidate fields and `cs_passivant`; the
panel; the fingerprint; the text format; the reference guide (*Properties*,
*How candidates are ranked*, the output-record list, the tag paragraph);
`AGENTS.md` (*What is relaxed* gains D5; the tag count).

Tests (`chemisorption_sequential_relax_test.rs`, `…_local_test.rs`,
`…_debug_test.rs`, `chemisorption_distinct_test.rs`,
`chemisorb_node_test.rs`):

- Off: unchanged (golden + snapshots).
- On, geometric phase: after the pass, every site's open valences are
  capped, with no H beyond them; atom count = combined + `passivants`.
- Nothing to cap: on a fixture where every candidate uses all of its sites'
  valence (e.g. two sites, a two-legged adsorbate, `formed_bonds = 2`), the
  setting changes nothing. The same candidates, structures and strains come
  out with it on and off, and `passivants == 0`. This metamorphic check pins
  that the passivated path and its reference reduce to the bare ones.
- Sites made by deleting H keep the hydrogen-passivation flag and are still
  capped (§4.1). Build the fixture with `atom_edit` H deletions, as
  mechadense's network does.
- Leftover valence: a hand-built site with two free valences that bonds one
  foot carries exactly one H on the other valence, and an acceptor site with
  a valence left after its transferred H gets that one capped.
- Atom conservation: for each candidate, the reference's atoms (adsorbate +
  substrate + H) match the candidate's by element, count and host, and
  recomputing the reference independently, without the map, gives the same
  energy.
- The reference is a function of its key: two candidates with the same key
  (including a used site with leftover valence) share one reference
  relaxation, and the energy does not change with the thread count.
- A hand-built case where a site next to the bound molecule's body gets an H
  inside the molecule: the H is placed, and the relaxation converges or is
  marked unconverged, never errors.
- `twice`: a four-footed fixture with `formed_bonds` unset has `twice > 0`
  and runs exactly `to_relax` relaxations; with `formed_bonds = 4`, `twice
  == 0`.
- Budget counts relaxations: for every budget from 0 to the untruncated
  relaxation count, the relaxations run never exceed the budget, no
  hypothesis is relaxed only once when it needs two, and the plan is marked
  truncated exactly when the budget is below the untruncated count.
- References are relaxed up front: the reference map holds exactly one entry
  per distinct key among the relaxed candidates, and the progress total
  announced before a batch equals the relaxations that batch runs,
  references included.
- Local phase: the frontier positions with the setting on equal those with
  it off (states are bare); the deepest-level dedupe uses passivated strain.
- Replay: every relaxed row's `replay` reproduces the recorded strain with
  the setting on (the existing replay-equals-search test, run in both
  settings).
- Tags: `cs_passivant` is on exactly the placed H. With the setting on,
  inputs carrying 29 tag names give the tag-limit error, not a panic (the
  existing `the_tag_limit_is_an_error_not_a_panic`, adjusted by one). With
  the setting off, 29 still works.
- Determinism across thread counts with the setting on (the existing thread
  test, run in both settings).
- `unique` still merges: the existing "merges = the slab's site orbits" test
  in `chemisorption_distinct_test.rs`, run with the setting on. The passivant
  H are part of the compared cluster, so this pins that symmetric bindings
  receive symmetric H.
- Node: the property round-trips through the text format and `.cnnd` and is
  written only when true; a node saved without it loads with it off; toggling
  it makes a stored result stale; `passivants` is appended after `mirrored`.

### Phase 4 — Walkthrough (maintainer, by hand)

On mechadense's network: run with `include_mirrored` on and read the
`mirr.` rows and their merges. Run with `passivate_sites` on and look at the
`cs_passivant` H around a few bindings, in particular the best-fitting ones
that start with an H inside the tool (§8.0): it should be pushed out, not
trapped under the molecule. On a dihydride surface, compare
`passivate` with and without `skip_clashes`. Not an agent task; the Flutter
smoke test stays a manual step.

---

## 7. Open questions

None. The four questions of the first draft were settled by the maintainer
on 2026-10-15:

- `chemisorb` does not use the clash rule at all (D8; revised the same day
  from "used, factor not exposed").
- A used site's leftover valence is capped (D4).
- H is the only passivant (D4a).
- The adsorbate's unbonded feet stay radicals (D4).

## 8. Phase results

### 8.0 Pre-implementation check of the clash factor (2026-10-15)

*Superseded for `chemisorb` by D8: the probe below is why `passivate_sites`
does not use the rule. Its numbers still inform the `passivate` default.
Without the rule, 30 of the 78 best-fitting seatings start with an H about
0.8 Å from a tool H. The walkthrough checks that those relax sensibly.*

Run on mechadense's network (`CHEMISORB-on-gemcut`: a 216-atom gemcut
Si(100) proxy with 18 bare sites, and a 28-atom tool with 3 O feet). A
temporary probe (since deleted) planned the search with that network's
settings, which gave 2,658 seatings, as in the app. On each seated start
structure it placed H on every unused site the way §4.1 does (15 per
candidate, 39,870 in all), and for each H recorded the smallest
`distance / (r_vdW(H) + r_vdW(partner))` over the atoms the rule checks.

- **The surface never clashes with itself.** With all 18 sites capped, the
  closest contact of any H is 1.21 of the van der Waals sum. The capping H
  never clash with each other or with the substrate in any candidate.
  Every contact below 1.0 is with the tool.
- **The well-fitting seatings show a clean gap.** No seating of this tool
  fits its sites within 0.5 Å: the best residual is 0.91 Å and the median
  4.4 Å, because the tool's foot spacing matches few site triples and
  `tolerance: 20` admits them all. Among the 78 seatings with a residual up
  to 1.5 Å, 30 have one H at 0.33–0.37 of the sum (0.8 Å from a tool H, so
  plainly inside the molecule). Every other H in that group is at 0.9 or
  more. Any factor between 0.4 and 0.9 gives the same result there.
- **The poorly fitting seatings form a continuum.** In the other 2,580
  (feet 1.5–9.6 Å from their sites), the ratios spread evenly from 0.1 to
  1.0. Skipped H by factor, over all candidates: 0.5 → 2,598, 0.6 → 3,846,
  **0.7 → 5,016 (in 2,496 candidates)**, 0.8 → 6,036. No factor separates
  them naturally, because the start geometry is far from where these
  candidates end up.

**What this means for the factor.** The one natural gap in the data
(0.4–0.9, among the seatings that resemble a binding) contains 0.7, so 0.7
stays the proposed default for `passivate`. In UFF terms, a contact at 0.7 of
the van der Waals sum costs roughly 15–30 kcal/mol before relaxing (H–C,
H–O, H–H), large but repairable. Contacts below about 0.5 are overlaps that
relaxation cannot repair sensibly.

**What this means for `chemisorb`** (D8). For the poorly fitting majority,
no factor helps. A skip there is decided on a rigid start several Å from
where the relaxation goes, so it removes H where the molecule *was*. That is
why `passivate_sites` caps everything. If the walkthrough shows trapped H
matter, the remedy is a different design, not a factor: relax bare, cap on
the relaxed geometry, and relax again, at twice the cost.
