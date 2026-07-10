# Design: Diff Output Pins for `relax` & Co. (Issue #295)

**Issue:** https://github.com/atomCAD/atomCAD/issues/295 (mechadense)

## Problem Statement

`atom_edit` is currently the only node that exposes its effect as a **diff** (output pin 1).
Users who build stamp/template workflows with `sequence`, `atom_composediff`, and
`apply_diff` cannot include the effect of `relax` (or other atom-manipulating nodes) in a
composed diff.

The motivating workflow: relax a **small mockup proxy** of a tool tip, take the *diff* of
that relaxation, and `apply_diff` it onto the **full-size SPM tip** — avoiding threading
the monster structure (including thousands of frozen atoms) through the `relax` node
entirely.

Requested nodes (issue names updated to post-lattice-space-refactoring names):

| Issue name | Current node | Priority |
|---|---|---|
| relax | `relax` | ★ critical |
| atom_(l)move / atom_(l)rot | `free_move`, `free_rot`, `structure_move`, `structure_rot` | ★ |
| atom_replace | `atom_replace` | ★ |
| atom_cut | `atom_cut` | ★ |
| lattice_move on non-atomic input | `structure_move`/`free_move` fed a `Blueprint` | empty diff |
| atom_union | — | explicitly **not** wanted |

## Key Insight

All the consumer-side infrastructure already exists and is battle-tested
(`crystolecule/atomic_structure_diff.rs`: `apply_diff`, `compose_two_diffs`,
`compose_diffs`; nodes `apply_diff`, `atom_composediff`, `sequence`; the `atom_edit`
two-pin precedent `[result, diff]`).

The **only missing primitive** is the inverse operation: *deriving* a diff from a
(before, after) pair of structures. `atom_edit`'s diff is authored incrementally by its
tools; nothing computes one after the fact.

All target nodes mutate a clone of their input **in place** (`relax` passes
`&mut atoms` into `minimize_energy`; `atom_replace`/`atom_cut` mutate inside
`map_atomic`/`map_atomic_in_region`; movement nodes bake transforms into positions;
`AtomicStructure` deletion keeps surviving slots stable), so **atom ids correspond 1:1
between before and after** (minus deletions, plus additions). This makes extraction a
simple, exact, id-keyed comparison — no positional matching heuristics at extraction
time. Positional matching only happens later, inside `apply_diff`, exactly as it does
for `atom_edit` diffs today.

## Correctness Invariant (drives the whole test story)

For any `before` structure and any `after` produced from it by an id-stable mutation:

```
apply_diff(before, extract_diff(before, after), tol) ≡ after        (roundtrip)
```

where `≡` is *structural equivalence*: equal multisets of (position, element, durable
flags) atoms and equal bond multisets — **not** id equality, because `apply_diff`
re-assigns ids. Every phase's tests are anchored on this executable spec.

Tests use `apply_diff`'s standard tolerance (0.1 Å). The invariant is insensitive to
the exact value because extracted anchors are *exact* base positions — matching
distance is 0 — so any tolerance small enough not to conflate distinct atoms works.

A secondary invariant ties extraction into the existing composition machinery:

```
apply_diff(apply_diff(base, d1), d2) ≡ apply_diff(base, compose_two_diffs(d1, d2))
```

must keep holding when `d1`/`d2` are *extracted* (not authored) diffs.

---

## 1. The Extraction Primitive: `extract_diff`

**Location:** `rust/src/crystolecule/atomic_structure_diff.rs` (next to `apply_diff` /
`compose_two_diffs` — same "fundamental operation on atomic structures" rationale).

```rust
/// Derives a diff (is_diff = true) such that applying it to `before`
/// reproduces `after`. Atoms are correlated BY ID: callers must guarantee
/// that `after` was produced from `before` by in-place mutation (surviving
/// atoms keep their ids). All target nodes satisfy this.
///
/// `position_epsilon`: an atom whose position moved by no more than this
/// (and whose element/durable flags are unchanged) is treated as untouched
/// and omitted from the diff. Pass 0.0 for exact comparison.
pub fn extract_diff(
    before: &AtomicStructure,
    after: &AtomicStructure,
    position_epsilon: f64,
) -> AtomicStructure
```

### 1.1 Atom classification (by id)

| Case | Emitted into diff |
|---|---|
| id in both, position moved > ε **or** element changed **or** durable flags changed | *Modified*: atom with `after`'s element/position/metadata, `anchor = before.position` |
| id in both, nothing (durably) changed | Nothing — passes through at apply time. (May later be materialized as an UNCHANGED marker if a bond entry needs it as an endpoint, see 1.3) |
| id only in `before` (deleted) | *Delete marker*: `DELETED_SITE_ATOMIC_NUMBER` at `before.position`, `anchor = before.position` |
| id only in `after` (added) | *Pure addition*: atom with `after`'s state, **no anchor** |

**Durable flags mask.** Flag comparison and the flags written onto diff atoms must mask
out the transient bits of `Atom.flags`: bit 0 (selected) and bit 5 (display-ghost).
Durable bits — hydrogen_passivation (1), frozen (2), hybridization override (3–4),
patch-ghost (6) — participate in the comparison and are carried into the diff (a
`freeze` node's entire effect *is* a flags change). Add a
`pub const DURABLE_FLAGS_MASK: u16` next to the flag accessors in
`atomic_structure/atom.rs` so the mask has one definition.

### 1.2 Frozen atoms fall out for free

`minimize_energy` holds frozen atoms *exactly* fixed, so under id-keyed exact
comparison they are "untouched" and never enter the diff. This is precisely
mechadense's requirement: the relax diff of a mockup with thousands of frozen boundary
atoms contains only the movable atoms.

### 1.3 Bond classification

Compare the canonical bond sets `{(min_id, max_id) → order}` of `before` and `after`,
restricted to pairs where both endpoints exist in the respective structure:

| Case | Emitted into diff |
|---|---|
| Bond only in `before`, both endpoints survive into `after` | `BOND_DELETED` entry between the endpoints' diff representatives |
| Bond only in `after` | Bond entry with its order |
| Bond in both, order changed | Bond entry with `after`'s order (override) |
| Bond in both, order unchanged, neither endpoint modified | Nothing (base bond passes through) |
| Bond in both, order unchanged, **but an endpoint is a modified atom** | Nothing — `apply_diff` step 3a already re-adds the base bond when the diff has no bond between two matched atoms |
| Bond whose `before` endpoint was deleted | Nothing — `apply_diff` drops bonds to deleted base atoms automatically |

**Endpoint representatives.** A bond entry needs both endpoints to exist as atoms in
the diff structure. The representative of an endpoint id is: its modified/added diff
atom if one was emitted; otherwise an **UNCHANGED marker** (`UNCHANGED_ATOMIC_NUMBER`
at `before.position`, `anchor = before.position`) created on demand and memoized per
endpoint id. This is the same referencing pattern issue #386 established.

### 1.4 Determinism & complexity guarantee

Iterate atoms and bonds in ascending id order so the emitted diff is deterministic
(stable diff atom ids, stable serialization, snapshot-friendly). No caching anywhere —
this is a pure function computed on demand (per the project's no-speculative-caching
policy).

**`extract_diff` must be O(n_atoms + n_bonds), and the id-keyed design makes this
free:** ids are slot indices (`id = index + 1`), so before/after pairing is an O(1)
indexed lookup inside one linear sweep (which also yields ascending-id order with no
sort); bonds are inline per atom (bounded degree), so the canonical
`(min_id, max_id) → order` set comparison is O(n_bonds) using `FxHashMap` (not an
ordered map — no hidden log factor); UNCHANGED-marker memoization is hash-map O(1).
There is deliberately **no positional matching at extraction time** — the O(n)
grid-backed matcher runs only inside `apply_diff`, as it always has. The dominant
per-eval cost is the node-side before-snapshot clone (§2.1), also linear.

### 1.5 Id-stability precondition — verified per node

The by-id comparison assumes surviving atoms keep their ids from `before` to `after`.
Verified against the current implementations:

- `relax`: `minimize_energy` takes `&mut` and only writes positions — ids untouched.
- Movement nodes: bake the transform into positions in place — ids untouched.
- `atom_replace`: element swaps via `set_atom_atomic_number` (in-place field write,
  `atomic_structure/mod.rs`); rule-deletions via `delete_atom` — survivors keep ids.
- `atom_cut`: **delete-only** (`cut_atomic_structure` collects ids outside the cutter
  SDF and calls `delete_atom`; never adds or rebuilds) — the simplest case, its diff
  is purely delete markers.
- `delete_atom` itself vacates the atom's slot (`atoms[id-1] = None`) with no
  compaction or renumbering, so deletion can never shift another atom's id.

The one theoretical hazard is **freed-id recycling**: if a node both deleted an atom
and added a new one in the same eval, `add_atom` could hand the new atom a recycled id
and the pair would masquerade as a single "modified" atom. No node in this plan
both deletes and adds (relax/movement: move only; atom_replace: swap/delete only;
atom_cut: delete only). Any future node adopting the diff pin (e.g. a passivation
pass that deletes then adds) must re-check this first; the per-node id-stability
assertion tests are the guard.

### 1.6 Non-goals

- **No positional matching at extraction time.** Id correspondence is a precondition.
  A standalone `atom_extractdiff(before, after)` *node* was considered and rejected:
  its correctness would silently depend on id stability across arbitrary user-wired
  chains, which is invisible to users. Per-node diff pins keep the assumption inside
  each node where it is locally verifiable.
- **No changes to `apply_diff` / `compose_two_diffs` semantics.**

---

## 2. Node-Side Pattern (identical for every node)

Using `relax` as the template:

1. **Output pins** (in `get_node_type()`):

   ```rust
   output_pins: vec![
       OutputPinDefinition::same_as_input("result", "molecule"),
       OutputPinDefinition::fixed("diff", DataType::Molecule),
   ],
   ```

   Same two-pin shape as `atom_edit` (`atom_edit_data.rs:2440-2451`) — with one
   deliberate difference: atom_edit's pin 0 is `same_as_input_or_default("result",
   "molecule", DataType::Molecule)`, while each node here keeps its **existing** pin-0
   resolution (relax today is `single_same_as("molecule")`, i.e. plain `same_as_input`
   with no disconnected-fallback — just split into the two-element vec). The diff pin
   is always a `Molecule` regardless of the input phase — diffs are free-floating atom
   sets, and this matches `atom_edit` / `atom_composediff` conventions
   (`Array[HasAtoms]` inputs accept it). The `same_as_input` pin name varies per node:
   `"molecule"` for `relax`/`atom_replace`/`atom_cut`, `"input"` for the four movement
   nodes — keep each node's existing input-pin name.

2. **Eval**: snapshot before mutating, extract after, return both pins:

   ```rust
   let before = atoms_ref.clone();          // or a cheaper snapshot, see §2.1
   /* ... existing mutation (minimize_energy etc.) ... */
   let mut diff = extract_diff(&before, atoms_ref, /*ε=*/ 0.0);
   diff.decorator_mut().show_anchor_arrows = true;   // same as atom_composediff
   EvalOutput::multi(vec![
       wrapper,                                       // pin 0, unchanged semantics
       NetworkResult::Molecule(MoleculeData { atoms: diff, geo_tree_root: None }),
   ])
   ```

   **Error paths:** return the error on **both** pins —
   `EvalOutput::multi(vec![err.clone(), err])`. Note this deliberately does *not*
   copy `atom_edit`, whose error arms return `EvalOutput::single(error)`; that is
   evaluator-safe (`EvalOutput::get` maps an out-of-range pin to
   `NetworkResult::None`, `node_data.rs:75-80`) but silently degrades the pin-1
   error to `None` for diff consumers. New two-pin nodes should propagate the real
   error on both pins; Phase 2 tests 6–7 assert it.

3. **Nothing else.** No `.cnnd` migration (output pins live on the `NodeType`, not in
   files; the new pin is index 1, existing pin-0 wires are untouched). No Flutter work
   (pins render from the node type; the global pin-0-only display default means no new
   viewport clutter; do **not** override `default_display_all_output_pins` — a diff
   *does* draw viewport geometry). No FRB regen for the pattern itself (no API type
   changes) — the one exception is the relax-specific `diff_min_move` property (§2.2).
   Text format `.pinname` references already support multi-output pins.

### 2.1 Cost note

The before-snapshot is a full clone of the input atoms, held only for the duration of
`eval`. For `relax` this is noise next to minimization. For the cheap nodes
(`atom_replace`, movement) it doubles transient memory of one eval; acceptable, and
`EvalOutput` has no per-pin laziness to exploit anyway. If profiling ever shows this
matters, a positions-only snapshot suffices for movement nodes — do not build that
speculatively.

### 2.2 `relax`-specific: `diff_min_move` pruning property

Minimization nudges *every* non-frozen atom at least infinitesimally, so the relax diff
contains essentially all non-frozen atoms. That is semantically correct and already
delivers the frozen-atom win. As a pruning knob, `relax` gets a stored property
`diff_min_move: f64` (default `0.0`), passed as `position_epsilon` to `extract_diff`
and exposed via `get_text_properties` / `set_text_properties` + the Flutter relax
editor's property panel. Documented caveat: pruning makes "apply the diff" differ from
"relax directly" by up to `min_move` per atom. Default keeps exact behavior.

Making the property editable from Flutter requires new API accessors: `relax_api.rs`
today exposes only `get_relax_message()` (`RelaxData` is an empty struct with no data
API). Phase 2 adds `get_relax_data` / `set_relax_data` (taking `scope_path` like every
sibling node-data accessor, per `rust/AGENTS.md`) followed by
`flutter_rust_bridge_codegen generate`. This is the plan's **only** FRB regeneration
(see §5).

### 2.3 Empty-diff semantics for non-atomic inputs

`structure_move` / `structure_rot` / `free_move` / `free_rot` accept `Blueprint` inputs
(no atoms). Per the issue, the diff pin then yields an **empty diff**
(`AtomicStructure::new_diff()` with zero atoms) rather than an error, so stamp
templates can be written generically. (`apply_diff` with an empty diff is a clean
no-op; `atom_composediff` composes it away.)

### 2.4 Movement nodes: the diff is atoms-only (documented lossiness)

The movement nodes move more than atoms: `free_move`/`free_rot` on a Molecule also
transform the optional `geo_tree_root`, and `structure_move`/`structure_rot` on a
Crystal move atoms and geometry rigidly together within the structure frame. A diff
can only represent the **atomic** component. Applying a movement diff to another
structure therefore moves its atoms but not its geometry/structure — for a Crystal
target this weakens the atoms⇄geometry rigid coupling, and downstream
geometry-dependent nodes (`atom_cut`, `dematerialize`, surface patches) will see the
un-moved geometry. This is inherent to the diff model and matches the issue's intent
(the atoms are what stamp workflows need); it must be stated in the node descriptions
(`description` field of the four movement node types mentions the diff pin captures
atom motion only).

### 2.5 Diff-of-a-diff inputs: unsupported

Feeding a structure that is itself a diff (`is_diff = true`, e.g. `atom_edit` pin 1)
into these nodes is not supported — delete/UNCHANGED markers carry pseudo-elements
(0 / −1) that already confuse downstream ops today (e.g. UFF typing in `relax`).
This feature neither improves nor worsens that; `extract_diff` documents that its
inputs are expected to be non-diff structures, and no special handling is added.

---

## 3. Phased Implementation Plan

Each phase compiles, passes `cargo clippy` + `cargo test -j 4`, and lands its own
tests. Tests live under `rust/tests/` (never inline), registered in the parent test
crate files, mirroring source layout per `rust/AGENTS.md`.

### 3.0 Mandatory per-node roundtrip test (non-negotiable)

**Every node that gains a diff pin — in this plan or any future adoption — MUST land a
node-level roundtrip test:**

```
apply_diff(node_input_atoms, diff_pin_value) ≡ result_pin_value
```

(the `apply_diff` *function*, called directly on the node's evaluated pin values),
evaluated through the real node in a real network (the `value`-node harness pattern
from `apply_diff_node_test.rs`), for **each atomic input phase the node accepts** —
Molecule *and* Crystal for `relax`/`atom_replace`/`atom_cut`, Molecule only for
`free_move`/`free_rot`, Crystal only for `structure_move`/`structure_rot` (each
movement pair type-errors on the other phase). This is the executable definition of "the
diff pin is correct" at node granularity; a node phase without it is incomplete
regardless of what else it tests.

To make this a one-liner per node, Phase 2 introduces a shared helper in the
structure_designer test crate (e.g. `tests/structure_designer/diff_test_support.rs`,
`#[path]`-registered like its sibling test modules):

```rust
/// Evaluates `node_id`'s pin 0 and pin 1, applies the pin-1 diff to
/// `input_atoms`, and asserts structural equivalence with the pin-0 result.
fn assert_node_diff_roundtrip(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    input_atoms: &AtomicStructure,
)
```

Every per-node test listed in Phases 2–4 as "roundtrip" is an invocation of this
helper (plus whatever node-specific assertions the bullet adds). The relax roundtrip
must run with `diff_min_move = 0.0` — pruning intentionally breaks exact roundtrip
(§2.2), which is itself asserted separately (Phase 2 test 4).

### Phase 1 — `extract_diff` core primitive + equivalence test harness

**Code**
- `extract_diff(before, after, position_epsilon)` in
  `rust/src/crystolecule/atomic_structure_diff.rs` per §1.
- `DURABLE_FLAGS_MASK` constant in `atomic_structure/atom.rs`.
- Test-support helper `assert_structures_equivalent(a, b)` implementing the
  id-insensitive `≡` from the invariant (greedy position+element+flags matching with
  tiny tolerance, then bond multiset comparison mapped through the matching).
  **Location matters:** the `crystolecule` and `structure_designer` test crates are
  separate binaries and cannot import from each other, so put it in a shared file —
  `rust/tests/test_support/structure_equivalence.rs` — and `#[path]`-include that
  file as a module from *both* `rust/tests/crystolecule.rs` (Phase 1) and
  `rust/tests/structure_designer.rs` (Phase 2+). If an equivalent helper already
  exists in `compose_diffs_test.rs`, move it there instead of duplicating.

**Tests** — new `rust/tests/crystolecule/extract_diff_test.rs` (register in
`rust/tests/crystolecule.rs`):

*Directed unit tests* (each asserts both the emitted diff's exact shape — atom kinds,
anchors, markers, bond entries — and the roundtrip invariant):
1. Identical structures → empty diff.
2. Single atom moved → one anchored atom; untouched atoms absent.
3. Atom moved by less than / more than `position_epsilon` (pruning boundary, ε=0 and
   ε>0).
4. Element changed in place (atom_replace shape) → anchored atom, same position.
5. Durable flag changed (frozen bit — the `freeze` node shape) → anchored atom;
   transient flag changed (selected, display-ghost) → **empty diff**.
6. Atom deleted → delete marker at old position; bonds to it emit nothing; roundtrip
   drops the bonds.
7. Atom added (with a bond to an untouched atom) → pure addition, no anchor; bond entry
   with an UNCHANGED marker endpoint.
8. Bond deleted between two untouched atoms → `BOND_DELETED` between two UNCHANGED
   markers.
9. Bond order changed → override entry.
10. Bond between two *moved* atoms, order unchanged → no bond entry, yet roundtrip
    preserves the bond (exercises `apply_diff` step 3a pass-through).
11. Mixed everything-at-once structure (move + replace + delete + add + bond add/del)
    → roundtrip.
12. Frozen atoms held fixed among moved neighbours → absent from diff (§1.2).
13. Determinism: extracting twice yields byte-identical serialization.

*Property-style randomized tests* (fixed-seed `StdRng`, ~100 iterations): generate a
random bonded structure, apply a random id-stable mutation script (moves, element
swaps, flag flips, deletions, additions, bond edits), then assert the roundtrip
invariant. This is the workhorse that catches bond-encoding corner cases.

*Composition interplay*: two sequential random mutation scripts `s1`, `s2`;
`d1 = extract_diff(base, s1(base))`, `d2 = extract_diff(s1(base), s2(s1(base)))`;
assert `apply_diff(base, compose_two_diffs(d1, d2)) ≡ s2(s1(base))`.

**Exit criteria:** all above green; no behavior change anywhere else in the app.

### Phase 2 — `relax` diff output (the critical node)

**Code**
- `relax.rs`: two-pin `output_pins`, before-snapshot, `extract_diff`, `EvalOutput::multi`,
  error arms return two-pin errors, `diff_min_move` property (§2.2) with
  serde-defaulted field (no `.cnnd` migration — default `0.0` on old files) + text
  properties.
- API: `get_relax_data` / `set_relax_data` in
  `rust/src/api/structure_designer/relax_api.rs` (with `scope_path`, §2.2), then
  `flutter_rust_bridge_codegen generate`.
- Flutter: add the `diff_min_move` field to the relax property editor
  (`lib/structure_designer/node_data/relax_editor.dart`; thin edit; per project policy
  verified by manual walkthrough, no integration test mandated).
- `cargo insta review` for `node_snapshots` (relax node type changed).

**Tests** — new `rust/tests/structure_designer/relax_diff_output_test.rs` (register in
`rust/tests/structure_designer.rs`), using the `value`-node harness pattern from
`apply_diff_node_test.rs`:
1. **Node-level roundtrip (§3.0):** small strained molecule → `relax`;
   `assert_node_diff_roundtrip` with `diff_min_move = 0.0`, for both Molecule and
   Crystal inputs.
2. **Mockup→monster (the issue's workflow):** build `monster = mockup ∪ far-away
   extra atoms` at identical coordinates; wire `relax(mockup)` pin 1 into an
   `apply_diff` node whose base is `monster`; assert relaxed-mockup atoms are at their
   relaxed positions and the extra atoms are untouched
   (`stats.orphaned_tracked_atoms == 0`).
3. **Frozen exclusion:** mockup with frozen boundary; assert no frozen atom appears in
   the diff (count diff atoms == count non-frozen atoms with ε=0).
4. `diff_min_move` pruning: with a large ε the diff is empty; pin 0 unaffected.
5. Pin 1 value is `Molecule` with `is_diff() == true` and `show_anchor_arrows` set,
   for both `Crystal` and `Molecule` inputs; pin 0 phase-preservation unchanged.
6. Error input (e.g. non-atomic) → both pins are `Error` (no panic on pin-1
   consumers).
7. Over-atom-limit input → both pins are `Error`. (The limit check lives inside
   `minimize_energy` — `check_minimize_limits`, `crystolecule/simulation/mod.rs` —
   and surfaces through relax's minimize-`Err` arm; relax.rs itself has no separate
   limit path. The limit logic is already covered by
   `crystolecule/simulation/minimize_test.rs`, untouched; this test only asserts the
   two-pin propagation of that arm.)

**Exit criteria:** mechadense's workflow is functional end-to-end for `relax`.

### Phase 3 — Movement nodes (`free_move`, `free_rot`, `structure_move`, `structure_rot`)

**Code:** apply the §2 pattern to all four. Atomic input → diff of the rigid motion
(every atom anchored). `Blueprint` input → **empty diff** (§2.3). Watch `free_rot`'s
degenerate-axis early return (zero axis → input passthrough before any mutation): it
must be converted to the two-pin shape with an empty diff, not left as
`EvalOutput::single`. Snapshot review.

**Tests** — new `rust/tests/structure_designer/movement_diff_output_test.rs`:
1. Per node: the mandatory §3.0 roundtrip (`assert_node_diff_roundtrip`) on each
   atomic phase the node accepts — a Molecule for `free_move`/`free_rot`, a Crystal
   for `structure_move`/`structure_rot` (the `free_*` pair rejects Crystal and the
   `structure_*` pair rejects Molecule) — four nodes, no exceptions.
2. Per node: Blueprint input → pin 1 is an empty diff (not an error), pin 0 unchanged.
3. `free_rot`: rotation about a non-origin pivot roundtrips (catches
   transform-composition mistakes).
4. Composability: `free_move` diff then `relax` diff composed via `atom_composediff` ≡
   sequential application (extends the Phase 1 interplay test to real node outputs).
5. `free_rot` degenerate (zero/unnormalizable) axis: this early-return path skips the
   mutation flow entirely today — it must still yield two pins: pin 0 the input
   unchanged, pin 1 an **empty diff** (not `None`).

### Phase 4 — `atom_replace` and `atom_cut`

**Code:** same pattern. Both route their mutation through `map_atomic` /
`map_atomic_in_region`, which consume the input `NetworkResult`; clone the atoms out of
the input value *before* invoking the map helper, then extract against the result's
atoms. (Do not extend the `map_atomic` seam unless a third caller appears.) Snapshot
review.

**Tests** — new `rust/tests/structure_designer/atom_op_diff_output_test.rs`:
1. `atom_replace`: the mandatory §3.0 roundtrip, plus: element swaps → diff of
   anchored same-position atoms with new elements; count == number of replaced atoms
   only.
2. `atom_replace` with a deletion rule (`to == 0` routes through `delete_atom`) →
   diff contains delete markers; roundtrip holds.
3. `atom_replace` with the `region` pin wired → diff contains only in-region
   replacements; out-of-region atoms absent from the diff.
4. `atom_cut`: the mandatory §3.0 roundtrip, plus: survivors' ids stable
   (regression-guard the §1.5 verification explicitly: assert surviving atoms in
   `after` keep `before` ids — if this ever fails, the precondition broke and
   extraction must not silently degrade); diff contains exactly one delete marker per
   cut atom, no entries for cut bonds.
5. Mockup→monster for `atom_cut`: apply the cut diff to a superset, only the
   corresponding atoms disappear.

### Deferred — remaining atom ops (NOT part of this plan)

`add_hydrogen`, `remove_hydrogen`, `infer_bonds`, `freeze`, `unfreeze` would get the
same pattern nearly for free (`extract_diff` already handles additions, deletions,
bond-only diffs, and flags-only diffs — each has a directed Phase 1 test), but they
are not in the issue's ★ list and are **explicitly out of scope: do not implement
them as part of this plan.** Any future adoption must follow §2, land the mandatory
§3.0 roundtrip test, and first re-check the §1.5 id-recycling hazard (these are the
first candidates that could combine delete and add in one eval). `atom_union`
intentionally gets nothing, ever (per the issue).

---

## 4. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Bond-encoding corner cases in `extract_diff` (the concentrated-risk spot) | Phase 1's directed tests 6–10 + seeded randomized roundtrip property tests |
| A node silently violates the id-stability precondition (now or in a future refactor) | Explicit id-stability assertions in Phase 4 tests; roundtrip node tests fail loudly if correspondence breaks |
| User applies a *translated* mockup's diff → anchors match nothing → silent near-no-op (`orphaned_tracked_atoms` skips) | Inherent to the positional diff model (same as `atom_edit` diffs today). Out of scope here; consider surfacing `apply_diff`'s `orphaned_tracked_atoms` stat as a node badge/subtitle in a follow-up issue |
| Diff computed even when pin 1 is unconsumed | Accepted: `EvalOutput` has no per-pin laziness; cost is one clone + linear scan, dominated by the op itself |
| Transient flags (selection, display-ghost) leaking into diffs → spurious "modified" atoms | `DURABLE_FLAGS_MASK` + Phase 1 test 5 |
| Movement diffs silently drop the geometry/structure component of the motion | Inherent to the atoms-only diff model (§2.4); stated in node descriptions |
| A node eval has multiple return paths (errors, `atom_cut`'s no-cutters passthrough, `free_rot`'s degenerate-axis passthrough, relax's minimize-`Err` arm — where the atom-limit rejection surfaces) and one keeps returning `EvalOutput::single` → pin 1 consumers see a stale/missing value | Per-node tests assert pin 1 on *every* return path (Phase 2 tests 6–7 style, Phase 3 test 5); review checklist item per node phase |
| Snapshot churn | `cargo insta review` is an explicit step in every node phase |

## 5. Explicit Non-Changes

- `apply_diff`, `compose_two_diffs`, `atom_edit` — untouched.
- No `.cnnd` version bump, no migration. The diff pins themselves need no FRB
  regeneration; the single regen in the plan is Phase 2's `diff_min_move` accessors
  (§2.2).
- No new Flutter widgets except the one `diff_min_move` field on the relax editor.
- No caching of extracted diffs.
