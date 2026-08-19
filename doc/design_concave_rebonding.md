# Concave-Corner Rebonding

## Status: Implemented on `surf-rec-fix` — awaiting chemistry review (Lukas). §5 and §14 are the parts that need a chemist's eye.

The pass is `rust/crates/atomcad-crystolecule/src/lattice_fill/concave_rebond.rs`, its tests are
`tests/crystolecule/concave_rebond_test.rs`. Workspace: 5146 tests green, no new clippy warnings.

## Depends on / relates to

- **`doc/surface_reconstructions.md`** — the (100) 2×1 dimer reconstruction this extends. Its
  `## Potential problems and mitigations` section is still `TODO`; this is its first entry.
- **`lattice_fill`** (`rust/crates/atomcad-crystolecule/src/lattice_fill/`) — the pipeline the new
  pass joins, after `hydrogen_passivate`.
- **`doc/design_blueprint_region_atom_edits.md`** — `RegionSpec` / `SettingsResolver`, which the new
  setting joins as a per-region-overridable field.
- **`doc/design_halogen_passivation.md`** — terminators are not always H; the criterion must scale to
  F/Cl/Br/I.
- **`doc/design_surface_patches.md`** — the hand-authored alternative (§12), and the escape hatch if
  this pass is ever not enough.

Background: reported by Lukas/Mark on a Si(100) T-centre model, 2026-08-19.

## 1. Motivation

On a Si(100) surface each surface atom has two bonds down into the bulk and two dangling bonds up.
Reconstruction pairs each surface atom with an in-plane neighbour and forms a **dimer bond**,
consuming one dangling bond per atom, so each ends up with exactly one terminator (monohydride).

At a **concave corner** — a (100) terrace meeting an ascending (111) wall — the terrace can narrow to
a strip one atom row wide. Those atoms have no partner left on the terrace, so no dimer forms, both
dangling bonds survive, and passivation puts **two terminators on each**: the "SiH2 / SiCl2 row" in
the report. That is not what happens physically. One of the two dangling bonds points straight at a
dangling bond on the wall atom, and the two terminators end up ~1.4 Å apart (H) or ~0.5 Å apart (Cl)
— a hard steric clash. The real surface resolves it by dropping both terminators and letting the two
host atoms bond directly across the corner. This is the textbook **rebonded step edge**.

The rewrite is **coordination-neutral**: each host loses one terminator bond and gains one host–host
bond. No valence bookkeeping changes, and no new single-bond atoms can appear.

## 2. What the current code does, and why nothing fires

`fill_lattice` (`lattice_fill/fill_algorithm.rs:160-208`) runs: `remove_lone_atoms_filtered` →
`remove_single_bond_atoms_filtered` → `reconstruct_surface` → `hydrogen_passivate`.

- `classify_atom_surface_orientation` (`surface_reconstruction.rs:238`) *does* classify the
  leftover-row atom correctly — it has exactly two bonds, both pointing into the bulk along one axis,
  so it comes out as e.g. `Surface001`.
- `process_atoms` (`:814`) then looks up the dimer partner's crystallographic address in
  `PlacedAtomTracker`, and the pair is only applied if the partner classifies to the *same* facet.
  Either half can fail — the partner site may be absent from the tracker, or present but classified
  differently — and in both cases **no `DimerPair` is emitted and the atom is silently dropped**.
  There is no "unpaired" bucket and no fallback rule.

  The fixture built for §10 exercises the second, less obvious path, and it is worth recording
  because it is the one the report actually hit: the last terrace row before the wall picks up a
  *third* lattice bond going up into the base of the (111) wall.
  `classify_atom_surface_orientation` requires exactly two bonds, so that row is filed as `Unknown` —
  and its would-be partner, the next row back, is then left with no valid partner and stays
  dihydride. The unpaired row is therefore one row *behind* the step edge, not on it.
- `hydrogen_passivate` walks the *motif* bond lists, finds both lattice neighbours missing, and
  passivates both.

Two findings worth stating explicitly, because they contradict plausible first guesses:

- **There is no existing clash test to reorder.** No distance/overlap check exists anywhere in the
  fill pipeline. (`weld_coincident_atoms` is unrelated — it fuses *exactly coincident* atoms for the
  patch system.) Reordering the existing steps cannot fix this.
- **Nothing ever adds atoms to blunt a concave edge.** `fill_lattice` only places motif atoms whose
  SDF ≤ threshold; every cleanup step only removes atoms.

## 3. Model

One new pass, `rebond_concave_clashes`, in a new module `lattice_fill/concave_rebond.rs`:

```
for each pair of clashing terminators (tA on host A, tB on host B), closest first:
    delete tA; delete tB; add_bond(A, B, 1)
```

No atom is moved. Relaxation handles geometry — the shortcut bond is created at the hosts' unrelaxed
second-neighbour separation (~3.84 Å in Si), which is long for a Si–Si bond and is expected to pull
in during minimisation.

## 4. Pipeline placement

The pass runs **after `hydrogen_passivate`**, as a new step 5 in `fill_lattice`'s cleanup block.

It must be after, not before. `hydrogen_passivate` decides "is this bond dangling?" from the *motif*
(`bonds_by_site1_index` / `bonds_by_site2_index` + tracker lookups), **not** from the atom's actual
bonds. Adding the shortcut bond earlier would therefore not stop passivation from also placing the
terminator, and the host would end up 5-coordinate. Running after passivation is self-correcting and
needs no change to the passivation logic at all.

## 5. The pair criterion  ← **needs chemistry review**

A candidate pair `(tA, A, tB, B)` is accepted iff **all** of the following hold:

1. `tA` and `tB` are terminators (§6), `A ≠ B`, and `A`/`B` are not already bonded.
2. **At least one of `A`, `B` is an unpaired {100} surface atom** (§7).
3. **Clash:** `|tA − tB| ≤ CLASH_FRACTION × (vdw(tA) + vdw(tB))`, with `CLASH_FRACTION = 0.75`.
4. **Host separation:** `|A − B| ≤ 1.05 × a/√2`, with `a` the actual cubic cell parameter. This is
   the second-neighbour distance — the hosts must be plausible bonding partners. Stops the pass from
   stitching a narrow trench shut.
5. **Facing:** the two dangling bonds must point at each other —
   `dot(normalize(tA − A), normalize(B − A)) ≥ 0.5` and the symmetric test for B (≤ 60° each).

Distances below, with a surface dangling bond sitting 54.75° off the normal. **Note the Si–H bond
length: 1.42 Å, not 1.50.** `SI_H_BOND_LENGTH = 1.50` is surf_recon's own constant for the
terminators it places on dimers; the danglers that matter here are passivated by
`hydrogen_passivate_dangling_bond`, which for a non-carbon host falls through to the *covalent radii
sum* (Si 1.11 + H 0.31). Two different Si–H lengths in the same pipeline is exactly the kind of
detail that makes a hand-derived threshold wrong — the first draft of this table used 1.50 and came
out 0.13 Å short.

| case | host separation | terminator–terminator | fraction of vdW sum | test 3 fires? |
|---|---|---|---|---|
| Si(100), unpaired pair, H (1.42 Å) | 3.840 Å | **1.521 Å** (measured) | 0.634 | yes |
| Si(100), unpaired pair, Cl (2.02 Å) | 3.840 Å | **0.541 Å** | 0.149 | yes |
| C(100), unpaired pair, H (1.09 Å) | 2.522 Å | **0.742 Å** | 0.309 | yes |
| *nearest legitimate contact, reconstructed Si(100)* | 4.591 Å | **2.869 Å** (measured) | 1.195 | no |
| Si(111) monohydride | 3.840 Å | 3.840 Å | 1.600 | no |
| two H on the *same* dihydride Si | — | 2.319 Å | 0.966 | no — test 1 rejects (`A = B`) |

The two measured rows come from the §10 fixture. They settle what the first draft left open — the
closest legitimate contact between adjacent *successfully reconstructed* dimers, which is the
tightest non-clash on the surface and is awkward to derive on paper.

**For hydrogen the separation is clean and wide.** Real clashes sit at 0.634 of the vdW sum; the
nearest legitimate contact is at 1.195. The 0.75 threshold has ~1.2× headroom above the clashes and
~1.6× margin below the legitimate contact, with nothing in between — the measured spectrum jumps
straight from 1.521 Å to 2.869 Å with no intermediate population.

### Tests 4 and 5 are load-bearing, not belt-and-braces

That comfortable picture is a **hydrogen-only** picture, and assuming it generalises would be the
easiest way to break this feature. The passivant is user-selectable (`passiv_elem`, and per-region at
that), and a halogen changes the balance in both directions at once: the terminator reaches ~0.6 Å
further out from its host (Si–Cl 2.02 Å vs Si–H 1.42 Å), *and* the threshold grows with its van der
Waals radius. Legitimate contacts shrink while the bar rises.

Measured on the two control fixtures, over the whole allowed set
(`dump_control_contact_spectrum_by_passivant`):

| passivant | test-3 threshold | closest legitimate contact | pairs test 3 flags (flat slab / (111) bevel) |
|---|---|---|---|
| H | 1.80 Å | — none reaches the threshold | 0 / 0 |
| F | 2.19 Å | — none reaches the threshold | 0 / 0 |
| Cl | 2.73 Å | 2.217 Å | **98 / 70** |
| Br | 2.79 Å | 2.069 Å | **98 / 70** |
| I | 3.06 Å | 1.790 Å | **98 / 70** |

So on a plain silicon slab with chlorine passivation, the distance test alone would propose ~98
spurious bonds. Every one of them is rejected by test 4 (hosts 4.591 Å apart against a 4.03 Å cap)
**and** independently by test 5 (facing 0.413 against a 0.5 floor) — both of which are purely
geometric and therefore *element-independent*, which is exactly why they hold as the terminator gets
bulkier.

**Do not "simplify" this criterion to the distance test.** For H and F that simplification looks
harmless and passes every hydrogen test; for Cl/Br/I it fuses a hundred surface atom pairs on an
ordinary slab. The control tests run over all five passivants for this reason.

Test 5 is what distinguishes "two dangling bonds pointing at each other across a concave corner"
from "two terminators brushing past each other on a flat face". On (111) all danglers are parallel to
the surface normal, so `dot` with the in-plane host direction is 0 and the rule can never fire there,
independent of any distance threshold.

## 6. Identifying terminators

A terminator is an atom that (a) has exactly one bond, (b) is of an element in `ALLOWED_PASSIVANTS`
(H/F/Cl/Br/I), and (c) **is not recorded in the `PlacedAtomTracker`**.

Test (c) is the exact discriminator and the reason no new bookkeeping is needed: `record_atom` is
called for every motif-placed atom and for nothing else, so *tracked = lattice atom, untracked =
terminator added by passivation*. Without it, a motif that legitimately contains a monovalent halogen
(a salt, an organic crystal) would have its own lattice atoms misread as terminators. Building the
`FxHashSet<u32>` of tracked ids is one O(n) pass.

The `host` of a terminator is its single bonded neighbour, which is by construction a tracked atom.

## 7. The unpaired-surface-atom set — and why it *is* the gate

`reconstruct_surface` changes its return type from `usize` to:

```rust
pub struct SurfaceReconstructionOutcome {
    pub dimer_count: usize,
    pub unpaired_surface_atoms: FxHashSet<u32>,
}
```

An atom joins `unpaired_surface_atoms` iff it was classified as a {100} surface atom **and**
`resolver.resolve_at(pos).reconstruct_surface` is true at its own position **and** it did not end up
in an applied dimer. All three conditions matter — see D3.

Criterion 2 of §5 (at least one host in this set) is not just a narrowing heuristic, it is what makes
the pass structurally safe. On an *unreconstructed* (100) face every adjacent pair of surface atoms
has terminators 1.39 Å apart (this is exactly why ideal Si(100)-1×1 dihydride is unstable). An
ungated clash rule would silently dimerize the whole face — performing reconstruction the user turned
off. Because the set is empty wherever reconstruction did not run, that cannot happen: with
`surf_recon` off globally, `reconstruct_surface` is not called at all and the pass is a no-op.

## 8. Determinism and safety

- Candidate pairs are collected, then **sorted by `(distance, min(tA,tB), max(tA,tB))`** and accepted
  greedily. Never iterate a `HashMap` to pick winners — ordering must not depend on hash order.
- Each terminator is consumed at most once (`consumed: FxHashSet<u32>`); a host may take part in
  several rebonds, which is correct at a corner where two danglers face two different partners.
- `has_bond_between(A, B)` guards against duplicate bonds.
- Neighbour search is `structure.get_atoms_in_radius(&tA.position, r)` with `r` the largest possible
  clash distance — a small constant. Same cost class as `weld_coincident_atoms`; negligible even on
  the 1.07M-atom nanobeam.
- The pass cannot create single-bond atoms (§1, coordination-neutral), so it does not need to re-run
  `remove_single_bond_atoms`.
- Idempotent: after it runs, no accepted pair remains (both terminators are gone).

## 9. Decisions

- **D1 — placement after passivation, not before.** Forced by `hydrogen_passivate` deriving
  dangling-ness from the motif rather than from actual bonds (§4).
- **D2 — clash expressed as a fraction of the van der Waals radius sum**, not an absolute distance
  and not a covalent-radius multiple. It is the physically correct statement of "steric clash", and
  it scales across C/Si hosts and H/F/Cl/Br/I terminators with one constant. Absolute thresholds do
  not: the same geometry gives 1.52 Å in Si–H and 0.74 Å in C–H.

  Scaling the *threshold* is necessary but not sufficient. Because a bulky terminator also reaches
  further out, the distance test on its own stops discriminating for Cl/Br/I — see §5. The criterion
  is a conjunction of three tests precisely so the two element-independent ones carry the halogen
  case.
- **D3 — "unpaired" means classified *and* reconstruction-enabled-at-its-position *and* not dimerized.**
  Dropping the middle condition breaks regional reconstruction: with `surf_recon` true only inside a
  region, `reconstruct_surface` still runs and still classifies atoms *outside* the region, and those
  atoms are dihydride by the user's choice. They must not be rebonded.
- **D4 — terminators identified by absence from `PlacedAtomTracker`** (§6), not by element alone and
  not by a new flag. Exact, zero new state, and correct for halogen-bearing motifs.
- **D5 — no new setting; the pass is part of `surf_recon`.** An earlier draft proposed a `rebond`
  boolean (node-data field + appended pin + `MaterializeRegion` field) as an opt-out. It was
  dropped, because its justification did not survive contact with the serde-default question.

  Both existing `materialize` settings pick their serde default explicitly to keep old files
  behaving as before — `remove_unbonded_atoms` is `#[serde(default = "default_true")]` *"to preserve
  the historical hardcoded behavior"*, `passivation_element` is `1` *"so old files load unchanged"*.
  This would be the first setting where preserving old behaviour (`false`) and the behaviour we
  actually want (`true`) pull in opposite directions. Defaulting to `true` changes old designs
  anyway, which makes the option worthless as compatibility protection — the only load-bearing
  argument it had. Defaulting to `false` honours the house rule but leaves every existing model
  producing chemically wrong structures until someone manually ticks a box.

  With that gone, what remains is speculative: per-region independent control, and a switch for the
  case where the §5 criterion misfires. There is no scenario for wanting reconstruction *without*
  rebonding other than "the heuristic is wrong", and the answer to that is to fix the criterion. §7
  already provides the gating for free, so folding in costs zero gating code, and
  `MaterializeRegion.rebond` would be permanent public schema. Adding the option later, if evidence
  demands it, is the same day of work — with evidence about what actually needs controlling.
- **D6 — no atom repositioning.** Explicitly out of scope per Lukas; the shortcut bond is created at
  the unrelaxed second-neighbour separation and relaxation closes it.
- **D7 — the pass lives in `atomcad-crystolecule`, on plain `AtomicStructure`.** No node-network
  types, so it is testable directly (§10).

## 10. Test plan

All tests are plain Rust in `rust/crates/atomcad-crystolecule/tests/crystolecule/concave_rebond_test.rs`,
building geometry with `GeoNode::half_space` + `intersection_3d` and calling `fill_lattice` — the
pattern already used at `lattice_fill_test.rs:347`. No `.cnnd`, no node network, no UI.

**Fixture.** A 6×6×3-cell Si slab minus the material that is both above `z = 2a` and on the near
side of the plane `x + y + z = SUM_CUT`, giving a (100) terrace with an ascending (111) wall whose
step edge runs along [1-10]. `SUM_CUT` only matters modulo the lattice; `sweep_cut_offsets` (kept in
the file, `#[ignore]`d) shows one-cell-wide plateaus, and the fixture takes the midpoint of the
plateau that leaves a whole row unpaired — 9 clashes, the largest crop in the sweep.

The clash detector in the test file is a deliberate **re-implementation** of the §5 criterion rather
than a call into the production pass, so the tests check the structure instead of agreeing with the
implementation by construction.

**Red until the pass lands** (all three currently fail for the right reason):

- `concave_corner_leaves_no_terminator_clash` — the headline: no clashing terminator pair may
  survive. Finds 9 today (7 between the unpaired row and the wall-base row, 2 more between adjacent
  unpaired atoms near the corner tip).
- `concave_corner_creates_one_rebond_per_clash` — nine resolved clashes must leave nine same-layer
  Si–Si bonds at the unrelaxed ~3.84 Å separation. Finds 0 today. (A same-layer Si–Si bond is an
  exact discriminator: the lattice has none, so every one was added by reconstruction — dimers pulled
  in to 2.34 Å, rebonds left at 3.84 Å per D6.)
- `halogen_passivant_clash_also_resolved` — same corner with Cl (guards D2's scaling).

**Green now, and must stay green:**

- `concave_corner_rebonding_is_coordination_neutral` — no Si exceeds four bonds. Catches the pass
  adding the bond without removing the terminator, or consuming one terminator twice.
- `surf_recon_off_leaves_the_surface_untouched` — with `surf_recon` off, *no* same-layer Si–Si bond
  may exist. This is the §7 structural gate stated as an observable: any such bond would mean the
  pass silently dimerized a surface the user chose to leave unreconstructed.
- `flat_slab_has_no_clashes_for_any_passivant` — a plain slab (six {100} faces, only convex edges)
  presents no clashes; the pass has no work to do on ordinary geometry.
- `bevelled_111_facet_has_no_clashes_for_any_passivant` — a {111}-bevelled corner presents no
  clashes (test 5).

Both controls, and `halogen_passivant_clash_also_resolved`, loop over the **whole**
`ALLOWED_PASSIVANTS` set rather than testing hydrogen and calling it done. §5 explains why that is
not padding: with Cl/Br/I the distance test alone flags ~98 legitimate contacts on the plain slab,
so these are the tests that prove tests 4 and 5 are doing the rejecting. A hydrogen-only control
suite would go green on a criterion that is badly broken for halogens.

Two controls from the first draft were **not** written. `regional_surf_recon_respects_region` (D3)
needs a `RegionSpec` fixture and belongs alongside the D3 code. `narrow_trench_not_stitched` (test 4)
cannot be built faithfully — lattice quantisation offers no trench width that trips test 3 while
failing test 4. Test 4 is exercised incidentally instead: the nearest legitimate contact in the §5
table is rejected by it.

**Global invariants**, added to the shared lattice-fill test helpers and asserted across the *existing*
fill corpus, not just the new fixture: no atom exceeds its valence, and no two terminators on
different hosts are closer than the clash threshold. These are cheap, and the second one would have
caught this bug as a symptom long ago.

**Corpus fixture.** Separately, a `.cnnd` reproduction goes in `rust/tests/fixtures/concave_rebond/`.
`validation_corpus_test.rs` walks `tests/fixtures/**/*.cnnd` recursively, so it joins both the
validation and eval-outcome snapshots automatically. It is authored through the atomCAD skill against
a running instance (not hand-written JSON), and it exists mainly so Lukas can open and eyeball the
exact geometry being claimed. Land it as its own commit — it churns
`integration__validation_corpus_test__validation_corpus.snap` on the way in.

## 11. Risks and compatibility

- **Existing projects change output.** Any model with a concave corner gains bonds and loses
  terminator atoms.

  This was expected to churn the snapshot suites. It did **not**: the full workspace run is 5146
  passed / 0 failed with `integration__validation_corpus_test__validation_corpus.snap` untouched and
  no `.snap.new` produced. So no committed fixture or sample exercises a concave corner with
  reconstruction on — which is a smaller blast radius than predicted, but it also means **the corpus
  gives this change no coverage at all**. The `.cnnd` fixture in §10 is what closes that gap, and it
  is worth more than the estimate here suggested.
- **Downstream `atom_edit` diffs anchor by position.** `atomic_structure_diff` matches atoms via
  `get_atoms_in_radius` with a tolerance. Deleting terminators inside `materialize` means a hand edit
  downstream that referenced one of those atoms loses its anchor. This is the sharp edge of the whole
  change and, per D5, **there is no opt-out**. Affected models are by construction exactly the ones
  that were wrong before, but the failure mode is silent, so it must be called out in the changelog
  and to Lukas directly. The mitigations are procedural, not code: this lands on a branch, it is not
  merged to main until Lukas has re-verified his models against it, and the snapshot diffs are read
  rather than blanket-accepted.
- **Threshold calibration** (§5) is the one genuinely uncertain number, isolated to one constant and
  pinned by one negative-control test.

## 12. Alternatives considered

- **Crystallographic corner rules** — a second lookup table alongside `DIMER_PARTNER_OFFSETS`, keyed
  on (orientation, site, wall facet). Chemically principled and deterministic, but requires
  enumerating every concave facet pairing ((100)/(111), (100)/(110), (100)/(100), …). The geometric
  criterion handles all of them uniformly and catches corners nobody has enumerated. Rejected as
  combinatorially open-ended for no accuracy gain.
- **Suppressing individual dangling bonds before passivation** — cleaner in principle, but requires
  per-dangling-bond state on the host (a set of consumed site specifiers) because the passivation
  early-out is per atom. Rejected as more invasive than the post-pass for the same result.
- **A hand-authored `patch`** (`doc/design_surface_patches.md`) — a rebonded step edge is expressible
  today as a tile with one tiling vector along the step (`validate_tiling_vectors` supports 1–3, see
  `patch.rs:86`). This remains the right answer for reconstructions too exotic to codify, and is the
  recommended workaround until this lands.

## 13. Surface area (D5)

Because there is no new setting, the change is confined to `atomcad-crystolecule` plus one
documentation line. No new pin, no node-data field, no `MaterializeRegion` field, no FRB regen, no
Flutter work, no serde-default question, no `.cnnd` version bump.

| layer | change |
|---|---|
| `lattice_fill/concave_rebond.rs` | new module — the pass |
| `lattice_fill/mod.rs` | `pub mod` + re-export |
| `lattice_fill/surface_reconstruction.rs` | `reconstruct_surface` returns `SurfaceReconstructionOutcome` (§7) |
| `lattice_fill/fill_algorithm.rs` | call the pass after `hydrogen_passivate`; feed it the unpaired set |
| `LatticeFillStatistics` | `concave_rebonds: i32` |
| Reference guide | one paragraph under `surf_recon` in `doc/reference_guide/nodes/atomic.md` — the behaviour is user-visible even though no control is |

Deferred, not part of this change: a non-blocking `ValidationError::warning()` on `materialize`
reporting terminator clashes the pass could not resolve. Cheap and useful as a diagnostic, but a
separate concern from the fix.

## 14. Open questions for review

1. **Is the shortcut bond the right chemistry for *this* corner?** The design assumes the rebonded
   step edge. Confirm that a (100)/(111) concave corner with a one-row terrace should rebond rather
   than, say, lose the leftover row entirely.
2. **`CLASH_FRACTION = 0.75`** — now measured rather than estimated: for hydrogen, clashes at 0.634
   of the vdW sum, nearest legitimate contact at 1.195, nothing in between (§5). What still needs a
   chemist is whether the *chemistry* agrees that the 0.634 contact should bond and the 1.195 one
   should not. And specifically for the halogens: with Cl the nearest legitimate contact is 2.217 Å
   — genuinely tight for two chlorines — so is a Cl-passivated Si(100) dimer surface even physical
   at that coverage, or should the model be telling the user something there?
3. **Should a host be allowed to take part in more than one rebond?** §8 currently allows it. At a
   corner where a single atom faces two different partners this seems right, but it is the case most
   likely to produce a surprise.
4. **No opt-out at all — is that right?** D5 drops the proposed `rebond` boolean, so every existing
   design with a concave corner changes output on load, with no switch to turn it back. The
   counter-argument is §11's silent diff-anchor breakage. The alternative, if that is too aggressive,
   is a boolean with *split* defaults — `#[serde(default)]` → `false` so old files freeze,
   `node_data_creator` → `true` so new nodes get the fix — which honours the house serde rule and
   makes the fork visible in the UI, at the cost of a day of plumbing, a permanent
   `MaterializeRegion` field, and existing models staying wrong until manually opted in.
