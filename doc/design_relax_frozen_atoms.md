# Design: frozen-aware relax limits and interaction filtering

**Problem report:** relax node fails with "175400 atoms > 2000 atoms limit"
on a structure where almost all atoms are frozen. The user only wants to
snap a small unfrozen region (elongated bonds on an Si SPM apex) to
equilibrium; the frozen bulk should not count against the limit.

## Motivation

`MAX_RELAX_ATOMS = 2000` (`relax.rs`, duplicated as `MAX_MINIMIZE_ATOMS` in
`crystolecule/simulation/mod.rs`) counts **total** atoms, frozen included.
The limit predates the `freeze` node and exists to keep the synchronous
UFF minimization from blocking the UI thread (issue #271). Since then the
pipeline has become mostly frozen-aware:

- The L-BFGS minimizer zeroes frozen gradients and normalizes its
  convergence RMS over **free coordinates only** (`minimize.rs`) — correct
  and cheap regardless of frozen count.
- The vdW term skips frozen–frozen pairs at pair-list build time in both
  `VdwMode`s (`uff/mod.rs::from_topology_with_frozen`). With the default
  `use_vdw_cutoff = true` preference (6 Å spatial grid), the per-iteration
  vdW cost scales with free atoms plus their 6 Å frozen shell.

Three gaps remain, and they are what this design fixes:

1. **Bonded terms are not frozen-filtered.** Bonds, angles, torsions, and
   inversions are enumerated and parameterized for the entire structure,
   and every `energy_and_gradients` call iterates all of them. A term
   whose atoms are *all* frozen exerts exactly zero force on every free
   atom and contributes only a constant energy offset — pure waste. For a
   175k-atom Si slab that is ~350k bonds, ~1M angles, and ~3M torsions
   evaluated on every L-BFGS iteration and line-search probe: tens of
   seconds to minutes of dead compute per relax, synchronously on the
   evaluator thread.
2. **The cutoff pair-list rebuild scans all atoms.** In `Cutoff` mode the
   vdW neighbor list is rebuilt every `CUTOFF_REBUILD_INTERVAL = 10`
   energy evaluations (`build_cutoff_pairs`): the spatial grid is rebuilt
   from all positions and the neighbor scan iterates **all** atoms — the
   frozen–frozen skip happens per candidate pair, *inside* the scan, so
   it does not shrink the scan itself. At 175k atoms that is an
   O(N_total·k) sweep (~10⁷ candidate pairs) repeated ~100–250 times
   over a 500-iteration run — a recurring total-atom cost that can
   dominate the relax even after the bonded terms are filtered.
3. **The atom-count limit is frozen-oblivious**, so the fast paths above
   are unreachable: the error fires before any frozen-aware code runs.

There is also a latent trap that blocks a naive "just count free atoms"
fix: with the cutoff preference **off**, `VdwMode::AllPairs` makes
`MolecularTopology::from_structure` enumerate all N²/2 nonbonded pairs
*before* the frozen filter is applied (`topology.rs::
enumerate_nonbonded_pairs` is not frozen-aware; the filter happens later
in `UffForceField`). At 175k atoms that is ~1.5×10¹⁰ pair structs
(hundreds of GB) — instant OOM. Lifting the limit without guarding this
path would turn an error message into a crash.

## Design summary

Three coordinated changes, all in the Rust backend. No API/FRB/Flutter
changes — the new errors surface through the existing node-error path.

1. **Filter fully-frozen bonded interactions** when building
   `UffForceField`, mirroring what vdW already does, and **make the cutoff
   pair-list rebuild O(N_free)** by caching the frozen atoms' spatial grid
   once and rebuilding only the free grid (frozen atoms never move).
   Neither changes where any free atom ends up (see invariants below).
2. **Count free atoms against the 2000 limit**; add a separate, much
   larger total-atom cap for O(N) setup cost and memory.
3. **Error on AllPairs mode above 2000 total atoms**, with a message
   telling the user to enable the vdW cutoff preference.

After this, the user's scenario (a few hundred free atoms in a 175k-atom
structure, default preferences) relaxes in seconds: per-iteration cost is
proportional to the free region plus its interaction shell. The only
residual O(N_total) cost is a single setup pass — topology enumeration,
UFF typing, and building the cached frozen grid — all done **once** per
relax. There is no recurring O(N_total) cost: the periodic cutoff rebuild
touches only the free atoms (the frozen grid is cached, see Change 1), and
per-iteration energy/gradient evaluation touches only the filtered
free-region interaction terms.

Region-select on the relax node (floated in the report) is **not** added:
region-gated freezing is already expressible as `freeze` (whole structure)
→ `unfreeze` with a `region: Blueprint` input → `relax`, which composes
better and keeps freezing visible in the viewport. This design makes that
existing workflow actually work past 2000 total atoms.

## Invariant: trajectory identity

The filtering must not change where any free atom ends up. Three facts
make the all-frozen filter safe:

- A bonded term's gradient contributions land only on its participant
  atoms. If all participants are frozen, every contribution is zeroed by
  `zero_frozen` anyway; the term's energy is a constant of the
  optimization (frozen positions never change).
- The convergence RMS is already computed over free coordinates only, so
  removing zero-gradient terms does not perturb the stopping criterion.
- The line search compares energies of candidate steps; a constant offset
  cancels in the Armijo comparison (`e_new <= energy + c1·step·dg`).

**Reported energy changes** by that constant offset: the result becomes
"energy of all interactions involving at least one free atom". This is
accepted (not compensated): frozen–frozen vdW pairs are *already* dropped
today, so the reported energy already stops being "total UFF energy" the
moment anything is frozen. The relax placard message keeps its format.

**The cutoff-rebuild change is *set*-identical rather than
bitwise-identical**: the two-grid scan produces exactly the same vdW pair
set as today's full scan (every non-excluded pair within the build radius
with ≥1 free endpoint), but pairs can land in the list in a different
order, perturbing floating-point summation in the last bits. Physically
meaningless, but it means bit-for-bit positional identity is only
guaranteed in `AllPairs` mode — the Phase 1 trajectory test runs there,
and cutoff-mode parity is asserted on the pair set instead.

## Change 1: frozen-aware interaction filtering

**Location: `uff/mod.rs` (`from_topology_with_frozen` and
`build_cutoff_pairs`), plus one small `SpatialGrid::from_positions_subset`
helper in `spatial_grid.rs`.** The `MolecularTopology` interaction lists
are left untouched, for two reasons that must not regress:

- **The typer needs full connectivity.** `from_topology_with_frozen`
  builds per-atom bond lists from `topology.bonds` (Step 1) and feeds them
  to `assign_uff_types_with_overrides`. Frozen boundary atoms participate
  in mixed interactions, and their UFF type (sp2 vs sp3, rest lengths,
  force constants) is derived from their *complete* bond environment.
  Filtering `topology.bonds` would silently mistype boundary atoms.
- **Torsion force-constant scaling counts siblings.**
  `compute_torsion_params` divides each torsion's force constant by the
  number of torsions about the same central bond (RDKit's
  `scaleForceConstant`), counted over the hybridization-filtered list. A
  central bond can host both mixed (free end atom) and all-frozen
  torsions; dropping the all-frozen ones before counting would inflate the
  surviving torsions' force constants and change forces on free atoms —
  silently breaking the trajectory-identity invariant.

Concretely, with `frozen_flags: Vec<bool>` built once at the top of the
constructor (it is currently built twice, inside each vdW arm — hoist it):

| Term | Filter point |
|---|---|
| `bond_params` | in the Step 4 map: skip when `frozen[idx1] && frozen[idx2]` |
| `angle_params` | in the Step 5 `filter_map`: skip when all of idx1/2/3 frozen |
| `torsion_params` | in `compute_torsion_params`, **after** the count-and-scale second pass: `retain` torsions with ≥1 free participant. Counting stays on the unfiltered `raw_torsions`. |
| `inversion_params` | in `compute_inversion_params`: skip when all of idx1/2/3/4 frozen |
| vdW | already filtered — unchanged |

`compute_torsion_params` and `compute_inversion_params` gain a
`frozen: &[bool]` parameter. `from_topology_with_vdw_mode` (the no-frozen
wrapper) passes an all-false slice and is behavior-identical.

This is a pure per-iteration win. The `MolecularTopology` lists still
materialize all-frozen interactions transiently (at 175k Si atoms, the
torsion list is roughly 100 MB, freed when the topology drops). That
transient bound is what the total-atom cap in Change 2 protects; pushing
the filter down into topology enumeration is a *future* optimization and
must solve the two hazards above first (documented here so nobody trips
over them).

**Cutoff pair-list rebuild uses a two-grid (cached-frozen) scan.**
`build_cutoff_pairs` (also used for the initial list at construction)
currently iterates every atom, applies the frozen–frozen skip per
candidate pair, and rebuilds the whole spatial grid from all positions
every `CUTOFF_REBUILD_INTERVAL = 10` energy evaluations. Two coupled facts
let us drop both the scan *and* the grid rebuild from O(N_total) to
O(N_free): only free atoms can move, and only pairs with ≥1 free endpoint
survive the frozen–frozen skip.

- **Cache the frozen grid.** Frozen atoms never move, so their cell
  assignments are valid for the whole relax. Build a `frozen_grid`
  (containing only frozen atoms) **once** at force-field construction and
  store it on the `Cutoff` strategy, alongside a `free_indices: Vec<usize>`
  built once (deriving it from a per-atom `frozen` flag on each rebuild
  would itself be O(N_total)).
- **Rebuild only the free grid.** On each rebuild, build a small
  `free_grid` from just the free atoms (O(N_free)). For each free atom
  `i`, scan **both** grids: `free_grid` with the `j > i` dedup (free–free
  pairs found once, from their lower index), and `frozen_grid` with
  unconditional acceptance (every neighbor there is frozen, so the
  `j > i || frozen[j]` rule is trivially satisfied — free–frozen pairs
  found once, from their free center). Frozen–frozen pairs are never
  scanned. The existing exclusion check applies unchanged in both scans.

The resulting pair set is provably identical to today's (and to a
single-grid free-only scan); only the list order can differ (see the
invariant section). The one-time `frozen_grid` build is O(N_frozen) — the
same linear cost the topology/typing pass already pays — but it is paid
**once** instead of ~100–250 times across a 500-iteration run.

`SpatialGrid` gains a `from_positions_subset(positions, indices,
cell_size)` constructor (the existing `from_positions` becomes the
all-indices case); both grids read live coordinates from the shared
`positions` array by index, so the frozen grid needs no position copy and
frozen atoms' cached cells stay correct as free atoms move. The per-atom
`frozen: Vec<bool>` flag previously stored on the `Cutoff` strategy is no
longer needed there — the free/frozen partition is now structural (two
grids + `free_indices`).

**Who benefits automatically:** `minimize_energy` (relax node) and
`atom_edit`'s batch minimize in FreezeBase mode — both already route
through `from_topology_with_frozen`. The continuous-minimization drag path
constructs force fields the same way and inherits both filters too.

## Change 2: free-atom limit + total-atom cap

All limit logic consolidates into `minimize_energy`
(`crystolecule/simulation/mod.rs`) — single source of truth. `relax.rs`
**deletes** its duplicated pre-check and `MAX_RELAX_ATOMS`; its existing
`Err(msg) → NetworkResult::Error(msg)` arm surfaces the new messages
unchanged. (`minimize_energy`'s messages are already user-facing.)

Constants in `simulation/mod.rs`:

```rust
/// Maximum number of *unfrozen* atoms `minimize_energy` will accept.
/// Free atoms drive the per-iteration cost (issue #271); frozen atoms
/// are excluded from all interaction terms that don't touch a free atom.
pub const MAX_MINIMIZE_FREE_ATOMS: usize = 2000;   // replaces MAX_MINIMIZE_ATOMS

/// Maximum *total* atoms (frozen included). Bounds the O(N) topology
/// build, UFF typing, and the transient interaction-list memory.
pub const MAX_MINIMIZE_TOTAL_ATOMS: usize = 500_000;
```

`500_000` rationale: setup stays O(N) in time and the transient torsion
list stays under ~1 GB worst case (dense 4-coordinated lattices) —
tolerable on desktop, and 3× the reported use case. It can be raised
later once topology-level filtering lands.

Checks at the top of `minimize_energy`, in order (free count via one
`iter_atoms` pass over `!atom.is_frozen()`):

1. `num_atoms > MAX_MINIMIZE_TOTAL_ATOMS` →
   > "Structure has {num_atoms} atoms, which exceeds the total
   > minimization limit of {MAX_MINIMIZE_TOTAL_ATOMS} (frozen atoms
   > included). Reduce the structure size."
2. `num_free > MAX_MINIMIZE_FREE_ATOMS` →
   > "Structure has {num_free} unfrozen atoms, which exceeds the
   > minimization limit of {MAX_MINIMIZE_FREE_ATOMS} free atoms. Freeze
   > the atoms that should not move (freeze node, optionally with a
   > region input) or reduce the structure size."
3. AllPairs guard — see Change 3.

Ordering puts the hard structural cap first, then the actionable "freeze
more" advice, then the mode advice; each message names its own remedy.

## Change 3: AllPairs guard above 2000 atoms

Per decision: **error, don't silently switch modes.** When the user has
explicitly disabled the cutoff preference, overriding it behind their back
would make the preference a lie; instead the error tells them exactly
which switch to flip.

In `minimize_energy`, after the free-atom check:

```rust
if matches!(vdw_mode, VdwMode::AllPairs) && num_atoms > MAX_MINIMIZE_FREE_ATOMS {
    return Err(format!(
        "Structure has {} atoms; minimizing more than {} atoms requires \
         the van der Waals distance cutoff. Enable 'Use vdW distance \
         cutoff for energy minimization' in Preferences (Simulation \
         section) and try again.",
        num_atoms, MAX_MINIMIZE_FREE_ATOMS
    ));
}
```

The wording matches the actual checkbox label in
`preferences_window.dart`. The threshold is **total** atoms, not free
atoms, because the O(N²) enumeration in
`MolecularTopology::from_structure` happens before any frozen filtering —
frozen atoms fully participate in the blowup. (Making the AllPairs
enumeration frozen-aware — only pairs with ≥1 free endpoint, O(N_free·N)
— was considered and rejected: cutoff mode is strictly better at that
scale, it is the default, and keeping AllPairs a small-structure exact
mode keeps its semantics simple.)

Note the guard sits below the total cap and free-atom check, so it only
fires for structures that are otherwise admissible — i.e. exactly the
"large but mostly frozen, cutoff disabled" case.

## What deliberately does not change

- **No relax `region` pin** — compose `freeze`/`unfreeze` (which already
  take an optional `region: Blueprint`) with `relax`.
- **No topology-level filtering** (hazards documented under Change 1).
- **No async/background minimization** — the limits still exist precisely
  because relax runs synchronously in the evaluator; that is a separate,
  larger project.
- **No `MinimizationConfig` / convergence changes** — the RMS is already
  free-only.
- **No atom_edit limit changes** — its minimize paths have no atom cap
  today and gain the Phase-1 speedup for free; adding caps there is out
  of scope.
- **No .cnnd / serialization impact** — nothing persisted changes.

## Implementation phases

### Phase 1 — interaction filtering (`uff/mod.rs`)

- Hoist `frozen_flags` construction to the top of
  `from_topology_with_frozen`; reuse in both vdW arms.
- Apply the four filter points from the table above; thread
  `frozen: &[bool]` into `compute_torsion_params` /
  `compute_inversion_params`.
- Rework the cutoff pair-list build to the two-grid (cached-frozen) scan:
  add `SpatialGrid::from_positions_subset`; on the `Cutoff` strategy store
  a `frozen_grid` and `free_indices` built once at construction and drop
  the per-atom `frozen: Vec<bool>` field; make the rebuild build only a
  `free_grid` (O(N_free)) and scan both grids per free atom (`free_grid`
  with the `j > i` dedup, `frozen_grid` with unconditional acceptance).
  Both the construction-time build and the periodic rebuild go through this
  one function, so a single change covers both.
- Tests (`rust/tests/crystolecule/simulation/`, extend
  `minimize_test.rs` / `uff_force_field_test.rs`):
  - Construct the same topology twice — no frozen vs. partial frozen —
    and assert the surviving interaction params are **byte-identical**
    (esp. torsion force constants around a central bond that hosts both
    mixed and all-frozen torsions: the count-before-filter rule).
  - Frozen boundary atom typing parity: a frozen atom bonded only to
    frozen atoms still gets its full-connectivity UFF type.
  - Trajectory identity (`AllPairs` mode): minimize a small molecule
    with a frozen appendage; free-atom final positions match the
    unfiltered reference within 1e-10. The reference run must minimize
    the **same constrained problem**: build its force field via
    `from_topology_with_vdw_mode` (all-false flags → filtering inert,
    every bonded term retained) but pass the **same frozen index list**
    to `minimize_with_force_field` — the frozen mask lives at the
    minimizer level, independent of force-field construction. (A
    reference with no frozen atoms at all would let the appendage move
    and converge elsewhere; comparing free-atom positions against it is
    meaningless.) The reference FF also retains the frozen–frozen vdW
    pairs the filtered FF drops; those act only between frozen atoms, so
    the trajectories still match exactly.
  - Cutoff pair-set parity: on a mixed frozen/free structure, the pair
    list from the two-grid scan — compared as a *sorted set* of
    `(idx1, idx2)` index pairs — equals a brute-force reference (all
    non-excluded pairs within the build radius with ≥1 free endpoint).
    List order is allowed to differ, which is why the strict trajectory
    test above runs in `AllPairs` mode. Assert parity holds **across a
    rebuild** too: displace a free atom, force a rebuild, and re-compare —
    catches a stale `frozen_grid` or a free/frozen partition bug.

    **Why set-parity is the test of record for the two-grid
    optimization.** This optimization changes *only which vdW pairs land
    in the list* — it touches neither the bonded terms nor the
    energy/gradient consumption loop. The vdW energy/gradient is an
    order-independent sum over the pair list, so an identical pair *set*
    implies an identical energy and gradient (up to floating-point
    summation order), which in turn implies an identical trajectory. Thus
    "the two-grid scan yields the correct pair set, including after a
    rebuild" is the complete correctness condition for this change:
    there is no separate end-to-end cutoff-trajectory equality test
    because a cutoff run's final positions are not bit-reproducible across
    pair orderings (which is exactly why the strict 1e-10 trajectory test
    lives in `AllPairs` mode). Comparing against a brute-force *ground
    truth* — rather than diffing an "optimization on vs. off" pair of code
    paths — is deliberately stronger: it validates the output is correct,
    not merely unchanged from a prior implementation that could itself be
    wrong.
  - All-atoms-frozen edge case: all param vectors empty, energy 0,
    converges immediately (zero free coordinates), no movement, no panic.

### Phase 2 — limits (`simulation/mod.rs`, `relax.rs`)

- Rename `MAX_MINIMIZE_ATOMS` → `MAX_MINIMIZE_FREE_ATOMS`, add
  `MAX_MINIMIZE_TOTAL_ATOMS`, implement the three ordered checks.
- Delete `relax.rs`'s own limit check and `MAX_RELAX_ATOMS`; audit other
  `MAX_MINIMIZE_ATOMS` / `MAX_RELAX_ATOMS` references (tests, docs).
- Tests:
  - `>2000` free atoms → free-atom error (message mentions freezing).
  - Large total / few free (e.g. materialize a slab, freeze all,
    unfreeze a pocket) with `VdwMode::Cutoff` → succeeds; frozen atoms
    verified unmoved; runtime sanity (converges).
  - Same structure with `VdwMode::AllPairs` → the preference-suggesting
    error.
  - Total > `MAX_MINIMIZE_TOTAL_ATOMS` → total-cap error (unit-test the
    check directly rather than allocating 500k atoms: factor the checks
    into a `fn check_minimize_limits(num_atoms, num_free, vdw_mode)`
    tested exhaustively, called by `minimize_energy`).
  - Relax-node-level test through the evaluator: frozen bulk over the old
    limit evaluates successfully (regression for the reported bug).
