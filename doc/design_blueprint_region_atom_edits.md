# Atomistic Edits Specified by a `Blueprint` Volume

## Status: Draft

## GitHub issue

[atomCAD/atomCAD#371](https://github.com/atomCAD/atomCAD/issues/371) — "add features for atomistic edits specified by `Blueprint` volume" (umbrella). Absorbs the design originally written for [#346](https://github.com/atomCAD/atomCAD/issues/346) ("improve on surface modification features: blueprint specified regions & disentanglement from materialize node"), which is now **Part B** of this document.

> This file was previously `doc/design_materialize_regions.md` (the `materialize.regions` design). It has been generalized: the same idea — *restrict an operation to a user-drawn `Blueprint` volume* — applies to many atomic edits, of which per-region materialization is the one structurally-special case. The renamed doc keeps that design intact as Part B and adds the broader composable-operation mechanism as Part A.

## Depends on

- `doc/design_optional_type.md` — the `Optional[T]` data type. **Only Part B depends on it** (it provides the set-vs-inherit semantics of the materialize region record's fields). Part A (the composable region pin) needs no new type machinery — a plain optional `Blueprint` pin suffices.
- `doc/design_atom_replace_rules_input.md` Phase A — built-in record-def infrastructure (`built_in_record_type_defs`, `lookup_record_type_def`), shipped. Used by Part B.

## Motivation

Many atomic edits naturally want to be **scoped to a region of space** the user draws as an ordinary volume, rather than applied globally. The issue lists, among others:

- Limiting **surface passivation / reconstruction** to a volume (the original #346 driver).
- **Freezing / unfreezing** atoms (or any atom-metadata edit) within a volume.
- **Relaxation** of a sub-volume, achieved by freezing the atoms outside it.
- **Element replacement** by volume.
- **Localized strain-field** application (a more exotic future idea).
- …probably more.

atomCAD already has the right primitive for "a volume the user drew": a **`Blueprint`**, whose `geo_tree_root` is an SDF built from the exact same nodes users already know (`half_space`, `cuboid`, `sphere`, CSG combinations). The job of this design is to make that volume *select which atoms an operation touches*, using one consistent membership rule across every operation.

### The two shapes of the problem

There are two structurally different cases, and they get two different mechanisms:

| | **Composable operations** (Part A) | **`materialize`** (Part B) |
|---|---|---|
| Examples | `atom_replace`, `add_hydrogen`, `remove_hydrogen`, `infer_bonds`, `freeze`/`unfreeze` | `materialize` (Blueprint → Crystal) |
| Can run more than once? | **Yes** — each acts on already-materialized atoms and returns the same phase, so they chain | **No** — single, irreversible Blueprint → Crystal conversion |
| "Multiple regions" achieved by… | putting **multiple nodes in sequence**, each with its own region | one node carrying an **ordered array** of region records, resolved per-position in a single pass |
| Mechanism | one optional **`region: Blueprint`** input pin | optional **`regions: [Record(MaterializeRegion)]`** input pin (painter's algorithm) |
| Needs `Optional[T]`? | No | Yes (record fields are per-field optional) |

The asymmetry is essential, not incidental. `materialize`'s five settings (`passivate`, `rm_single`, `surf_recon`, `invert_phase`, `rm_unbonded`) are consumed **simultaneously** inside one `fill_lattice` pass, so per-region overrides must be assembled *before* the pass and resolved per query point. Every other operation runs on atoms that already exist, returns the same phase it received, and is therefore freely stackable — so a single region pin per node, chained, is both sufficient and simpler.

The shared kernel of both parts is identical: **membership of a point in a region = the region Blueprint's `geo_tree_root` SDF evaluated at that point is `≤ margin`.** Part B already specifies that rule in full (§B4); Part A reuses it verbatim.

---

# Part A — Composable region-gated operations

## A1. The `region` input pin

Each region-gating operation node gains **one optional input pin**:

| Name | Type | Required | Disconnected behavior |
|---|---|---|---|
| `region` | `Blueprint` | No | operation applies to **all** atoms (exactly today's behavior) |

- **Type is `Blueprint`** for the same reason as Part B's `volume` field: there is no bare 3D-geometry type at the network level, and every 3D shape node already outputs a `Blueprint`. Only the region Blueprint's `geo_tree_root` is consumed; its `structure` is **ignored** (documented; see A7).
- **No `Optional[T]` dependency.** Like `atom_replace.rules`, a disconnected pin evaluates to `NetworkResult::None`, which the node reads as "no region → operate on everything". This is plain optional-pin plumbing (`evaluate_arg`, not `evaluate_arg_required`), so Part A can ship independently of `doc/design_optional_type.md`.
- The pin is always the **last** input pin on each node, so existing wires/positions are unaffected and `.cnnd` files need no migration (the pin appears unconnected on existing nodes).

## A2. Membership and margin

Membership reuses Part B §B4 unchanged:

> A point `p` belongs to a region iff `region_geo_tree.implicit_eval_3d(p) ≤ margin`.

- **`DEFAULT_REGION_MARGIN = 0.1 Å`** (the same constant defined in `lattice_fill` for Part B; Part A imports it rather than redefining). Rationale carried over: it sits an order of magnitude above fill/float noise and an order of magnitude below the smallest relevant interlayer spacing, so a region built by reusing a boundary-coincident half-space robustly captures the intended surface atoms without grabbing the layer below.
- Unlike materialize (where surface atoms land *on* the boundary by construction), post-materialize atoms usually sit comfortably inside or outside a region, so margin matters less here — but a user who reuses the materialize geometry as a region hits the same knife-edge, so the same default protects them. See Open Question 1 for whether Part A should expose a per-node margin override.

## A3. Shared helper: `map_atomic_in_region`

Today every atom op funnels its mutation through `map_atomic` (`evaluator/atom_op.rs`), which applies a closure to the `AtomicStructure` inside a `Crystal`/`Molecule` while preserving the concrete phase. Part A adds a sibling that additionally carries a region predicate:

```rust
/// Like `map_atomic`, but the closure is told which atoms are in-region.
/// `region == None` → every atom is in-region (today's behavior).
/// Membership = region SDF at the atom position ≤ margin (batched).
pub fn map_atomic_in_region<F>(
    input: NetworkResult,
    region: Option<&GeoNode>,
    margin: f64,
    f: F,
) -> NetworkResult
where
    F: FnOnce(AtomicStructure, &dyn Fn(u32) -> bool) -> AtomicStructure,
{ ... }
```

- Membership is precomputed once per eval via `BatchedImplicitEvaluator` over all atom positions (we now have concrete positions, so batching is cheap and parallel), yielding a `HashSet<atom_id>` or a closure `in_region: Fn(atom_id) -> bool` handed to `f`.
- `region == None` short-circuits to "all in-region" — `map_atomic_in_region(input, None, _, |s, _| …)` is exactly `map_atomic`, so the helper subsumes the old one and each op keeps a single code path.
- Keeping the membership logic in one helper means each op node's change is: add the optional pin, read it, call `map_atomic_in_region` instead of `map_atomic`, and consult the predicate inside the existing per-atom loop. No op re-implements SDF/margin/coordinate handling.
- **Evaluate at the atom's raw real-space position** — `region_geo_tree.implicit_eval_3d(atom.position) ≤ margin`. `geo_tree_root` is already in absolute real (Å) coordinates: it is built that way by the shape nodes (`sphere`/`cuboid`/etc. convert their integer lattice-unit parameters to real coordinates via `lattice_vecs.ivec3_lattice_to_real(...)` at build time) and evaluated that way by `fill_lattice` and by `patch_build::extract_patch_tile` / `patch_latticefill`. Follow that precedent — **do not** divide by `unit_cell_size`. The one node that does (`atom_cut`, `cutter_geo_tree_root.implicit_eval_3d(&(atom.position / unit_cell_size))`) is **currently buggy** — a stale pre-lattice-space-refactoring convention that survives only because users don't exercise that node; it will be fixed soon and is **not** a pattern to copy. Mirroring it here would mis-scale every region by the unit-cell size (~3.567 Å for diamond).

## A4. Per-node rollout

The following existing nodes gain the `region` pin and gate their per-atom work on the predicate:

| Node | In-region effect | Out-of-region atoms |
|---|---|---|
| `atom_replace` | apply replacement rules only to in-region atoms | pass through unchanged |
| `add_hydrogen` | passivate dangling bonds only on in-region atoms | left as-is |
| `remove_hydrogen` | strip H only from in-region atoms | left as-is |
| `infer_bonds` | (re)infer bonds touching at least one in-region atom (one-endpoint-inside; see below) | bonds between two out-of-region atoms untouched |

Each is a localized change: the membership predicate is checked at the top of the existing per-atom iteration (e.g. `atom_replace` already loops `for (atom_id, atom) in structure.iter_atoms()` — add `if !in_region(*atom_id) { continue; }`).

**Which atom's membership counts.** Every operation tests membership on the position of the **existing atom it acts on** — the heavy/host atom, which is the stable side of any bond or passivation:

- `atom_replace` tests the atom being replaced.
- `add_hydrogen` tests the dangling-bond (host) atom; the new H is placed wherever the bond template puts it, regardless of where that lands relative to the region.
- `remove_hydrogen` tests the heavy atom the H is bonded to — an H sitting just outside the boundary is still stripped if its host is in-region.
- `infer_bonds` (re)infers a bond when **at least one endpoint** is in-region (one-endpoint-inside), so a surface atom gets its bonds even to a neighbor just outside.

Newly created atoms are never themselves membership-tested: the batched predicate (A3) covers only the atoms present when the node is entered.

Phase types are preserved automatically by `map_atomic_in_region` (Crystal-in → Crystal-out, Molecule-in → Molecule-out), so the nodes' `OutputPinDefinition::single_same_as("molecule")` typing is unchanged.

## A5. New nodes: `freeze` / `unfreeze`

Per the issue discussion (@mechadense: prefer many single-function nodes over one combined node), atom freezing becomes its own pair of region-gated metadata-edit nodes:

- **`freeze`** — sets the frozen flag (`Atom` bit 2, `set_frozen(true)`) on in-region atoms.
- **`unfreeze`** — clears it on in-region atoms.

Both are `HasAtoms`-polymorphic (`single_same_as("molecule")`), stateless (`NoData`-style), and use `map_atomic_in_region`. With the `region` pin disconnected they freeze/unfreeze **all** atoms (consistent with every other op). They are atom-metadata edits, exactly the category the issue body calls out ("freezing atoms (or any atom metadata edits)"), and keeping them single-purpose lets a user compose a richer metadata editor as a custom network if desired.

This pair is folded into this document rather than split out, because freeze is just another region-gated atom op and shares all of Part A's machinery.

## A6. `relax` honors the frozen flag

The minimizer (`crystolecule/simulation/minimize.rs`) **already supports frozen atoms** by zeroing their gradient components, and `minimize_energy()` — the entry point the `relax` node calls — **already collects the frozen atoms itself**: it walks the topology, picks out every atom with `is_frozen()`, and passes those indices to both the force-field construction and the optimizer (`crystolecule/simulation/mod.rs`). So `relax` honors the frozen flag today with **no node change required**.

> **Historical note.** An earlier draft of this section claimed `relax` was buggy because it called `minimize_energy()` with an *empty frozen list* and that Phase A3 needed to fix `relax.eval` to collect `is_frozen()` indices. That was already false by the time Phase A3 was implemented (2026-06-29): `minimize_energy` had since been refactored from taking an explicit `frozen` argument to deriving the frozen set internally from the atom flags. The supposed bug never reached users. Phase A3 therefore added only a regression test (`relax_holds_frozen_atom_fixed`) locking in the existing behavior, and made no change to `relax`. This note is kept so anyone returning to this design isn't misled by the original claim.

`relax` gains **no `region` pin**. Because a frozen atom stays in the force field (it still pulls on its mobile neighbors) while being held fixed, the existing `freeze`/`unfreeze` nodes (A5) already compose with `relax` to constrain which atoms move — no region-aware variant of `relax` is needed.

## A7. Coordinate frame, validation, serialization

- **Coordinate frame.** Atoms arrive at each node already in their current model-space positions (movement nodes bake transforms into atom positions and `geo_tree` transforms — see the three-phase model). The region is tested against atoms **as they arrive at that node**, so the region-authoring subgraph must sit *after* the same movement that positioned the atoms. Document this; it is the same mental model as Part B (region built in the same real space as the thing it acts on). Both the atom positions and the region `geo_tree_root` are in **absolute real (Å) coordinates**, so membership is a direct `implicit_eval_3d(atom.position)` with no unit-cell rescaling — see §A3 (and note the `atom_cut` bug called out there: it divides by `unit_cell_size`, which is wrong and slated to be fixed; do not replicate it).
- **Region Blueprint `structure` ignored.** Only `geo_tree_root` is consumed (mirrors Part B §B1 and Open Question 5).
- **Validation.** The `region` wire is ordinary `Blueprint`-type checking — no new validator rule. A region whose volume is disjoint from the structure is a well-defined no-op (nothing in-region). An empty/`None` region is today's behavior.
- **Serialization.** No `.cnnd` migration: each new pin appears unconnected on existing nodes; no version bump. The two new node types (`freeze`, `unfreeze`) are additive registrations.
- **Undo.** No new commands — region connections and the new nodes are covered by the existing wire/add-node machinery.

## A8. UI

- The `region` pin renders as an ordinary optional input pin; no editor.
- Optional subtitle nicety (parallel to `atom_replace`): when `region` is connected, a node may show a `(regional)` subtitle hint. Low priority.
- Regions are authored entirely with existing nodes (`half_space`, `cuboid`, CSG) — no new editors.

---

# Part B — Per-Region Materialization Settings (`materialize.regions`)

> This is the original #346 design, unchanged in substance. Section numbers are prefixed `B` to disambiguate from Part A; cross-references that previously read "§4" now read "§B4", etc.

`materialize` converts a Blueprint into a Crystal via `fill_lattice`, controlled by five booleans — `passivate`, `rm_single`, `surf_recon`, `invert_phase`, `rm_unbonded` — that today apply **globally** to the entire structure. Issue #346 asks for crystolecule parts with *different* surface treatments on different regions/zones: e.g. a reconstructed and/or depassivated top surface with ordinary passivation everywhere else, specified by simple volume proxies (typically a half-space through the top surface). The issue also asks to make the materialization process "less magic and more user-space exposed".

This design keeps `materialize` as the single Blueprint → Crystal conversion point and adds an optional **`regions`** input pin: an ordered array of region records, each pairing a Blueprint volume with per-field-optional settings overrides. The specification is assembled from ordinary nodes (`half_space`, CSG, `record_construct`, array nodes), so it is plain, inspectable, parametric node-network data — that is the "user-space exposed" part — while the algorithms that genuinely require fill-time data (crystallographic addresses, motif templates) stay inside materialization.

### Why a flat ordered array, not a tree

A tree of volumes with child-overrides-parent semantics was considered (it is the natural mental model: a root for all of space, children refining sub-volumes). Rejected as the *representation* for two reasons:

1. **Recursive record types are unrepresentable.** The record-type-def dependency graph is acyclicity-enforced (`NodeTypeRegistry::check_no_cycle`), so a `RegionNode = {…, children: [RegionNode]}` def is rejected by construction. A tree would need a dedicated opaque spec type + combinator nodes — significant machinery.
2. **The flat array is equally expressive.** Resolving a point against a tree returns its deepest containing node; flattening the tree depth-first yields an ordered list where "last containing region wins" gives the identical answer. With `Optional` fields, per-field resolution additionally recovers the tree's *inheritance*: a region that sets only `surf_recon` transparently inherits everything else (see Semantics). If a genuine tree builder is ever wanted, it can compile down to this array — the semantics are forward-compatible.

## Design

### B1. Built-in `MaterializeRegion` record type

Registered in `NodeTypeRegistry::new()` next to `ElementMapping`, with all the standard built-in-def properties (reserved name, immutable, not serialized, discoverable through `lookup_record_type_def` and the unified type dropdowns):

```text
MaterializeRegion = {
    volume:       Blueprint,
    margin:       Optional[Float],   // membership tolerance in Å; unset → DEFAULT_REGION_MARGIN
    passivate:    Optional[Bool],
    rm_single:    Optional[Bool],
    surf_recon:   Optional[Bool],
    invert_phase: Optional[Bool],
    rm_unbonded:  Optional[Bool],    // remove zero-bond (lone) atoms; mirrors materialize's rm_unbonded (#363)
}
```

Authored field order above drives the `record_construct` pin layout. Per `doc/design_optional_type.md` (Core Decision 2: `Optional` is a record-field modifier, never a pin type), **the construct input pins are plain `T`** — `volume: Blueprint`, `margin: Float`, the five settings `Bool` — *not* `Optional[…]`. `Optional` lives only in the def's field declarations above; it never appears on a pin or wire. The field's Optional-ness drives behavior at `record_construct::eval`: `volume` is the one **required** pin (unwired → the whole record collapses to `None`); the six `Optional` fields are **not required** (an unset one stays an explicit `None` in the record rather than collapsing it).

The three settings states this design needs (§Motivation: force-on / force-off / inherit) map directly onto the construct node's per-field input, with no `Optional` value ever on a wire:

| State | How the user expresses it | Field value in the emitted record |
|---|---|---|
| Force on | wire `true`, or set the literal to `true` | `Bool(true)` |
| Force off | wire `false`, or set the literal to `false` | `Bool(false)` |
| Inherit | leave the pin unwired **and** the literal unset | `None` |

"Inherit" is the **absence** of both a wire and a `literal_values` entry (resolution order is unchanged: wired > literal > `None` — see `doc/design_optional_type.md` §5). The construct node's literal editor therefore needs a **clearable / tri-state** affordance for these fields (true / false / unset for the `Bool`s; value / unset for `margin`), defaulting to unset. A freshly-added region record thus inherits everything until the user sets a field.

Note Part B never uses a `record_destructure` node: `materialize` parses the region records directly in Rust (§B6, reading each field as "value or `None`"), so the destructure-emits-plain-`T` rule from the Optional design does not bear on this path at all — Part B depends only on (a) `Optional` as a field-declaration type and (b) `record_construct`'s optional-field collapse exemption.

**`volume` is a Blueprint** because that is what every 3D shape node outputs — there is no bare 3D-geometry type at the network level. Users build region volumes with the exact nodes they already know (`half_space`, `cuboid`, `sphere`, CSG combinations), in the same real space as the Blueprint being materialized. Only the region Blueprint's `geo_tree_root` is consumed; its `structure` is **ignored** (documented; see Open Question 5 for the warn-on-mismatch alternative).

### B2. Pin signature

| Index | Direction | Name | Type | Required | Notes |
|---|---|---|---|---|---|
| 0 | Input | `shape` | `Blueprint` | Yes | unchanged |
| 1 | Input | `passivate` | `Bool` | No | unchanged (overrides stored bool) |
| 2 | Input | `rm_single` | `Bool` | No | unchanged |
| 3 | Input | `surf_recon` | `Bool` | No | unchanged |
| 4 | Input | `invert_phase` | `Bool` | No | unchanged |
| 5 | Input | `rm_unbonded` | `Bool` | No | unchanged (added in #363; remove zero-bond atoms) |
| 6 | Input | **`regions`** | **`[Record(Named("MaterializeRegion"))]`** | **No** | new |
| — | Output | (pin 0) | `Crystal` | — | unchanged |

### B3. Semantics: root + per-field painter's algorithm

- The node's **effective settings** (stored booleans, individually overridden by the wired bool pins exactly as today) form the **root**: they apply to all of space. Note the contrast with `atom_replace.rules`, where a connected pin *replaces* the stored data — here the regions **layer on top of** the root; the node's own settings stay meaningful (and the side-panel checkboxes stay enabled when `regions` is connected).
- Regions apply **in array order**. For a query point `p` and a settings field `f`: walk the regions **last → first**; the first (i.e. latest-in-array) region that *contains* `p` **and** has `f` set supplies the value; if none does, the root supplies it. Resolution is per field, so a region that sets only `surf_recon: true` changes nothing else — unset fields are transparently inherited from earlier matching regions and ultimately from the root.
- A region **contains** `p` iff `region_sdf(p) ≤ margin` (see §B4).
- Disconnected `regions` pin / empty array → exactly today's behavior. A region whose settings are all unset is a no-op.

### B4. Boundary tolerance (`margin`)

The knife-edge case is the *default* usage, not a corner case: `fill_lattice` places surface atoms at `sdf ≈ 0` of the main geometry (inclusion test `sdf ≤ CRYSTAL_SAMPLE_THRESHOLD = 0.01`), and users will build region volumes by reusing the very half-spaces and primitives that bound the Blueprint — so the critical atoms sit numerically *on* the region boundary, where unbiased membership would be decided by floating-point noise.

Therefore membership uses a positive tolerance:

```rust
// lattice_fill, next to CRYSTAL_SAMPLE_THRESHOLD so the relationship is visible
/// Default region-membership tolerance in Å. A point belongs to a region if
/// its SDF is ≤ this margin. Sits an order of magnitude above the fill
/// threshold + float noise (CRYSTAL_SAMPLE_THRESHOLD = 0.01) and an order of
/// magnitude below the smallest relevant interlayer spacing (diamond (100)
/// layer separation a/4 ≈ 0.89 Å), so it robustly captures the surface atoms
/// a boundary-coincident region aims at without grabbing the layer below.
const DEFAULT_REGION_MARGIN: f64 = 0.1;
```

This is the single constant **shared with Part A** (A2).

Per-region override via the `margin` field (`Optional[Float]`, unset → default). **Negative margins are allowed** — they shrink the effective region, e.g. to deliberately exclude the boundary layer. No clamping.

Margin solves *membership* (in-or-out of one region); it does not solve *tie-breaking* — two adjacent regions whose margins meet at a shared face now have a guaranteed overlap band there, and the **array order** decides inside it, deterministically. Both rules are needed; the doc/UI copy should state them together.

### B5. The pipeline today (recap) and the per-position changes

`fill_lattice` (`crystolecule/lattice_fill/fill_algorithm.rs`) runs, in order:

1. **Fill** — recursive box subdivision; main-geometry SDF batch-evaluated at candidate motif sites; atoms placed where `sdf ≤ 0.01`; per-atom depth (−sdf) stored on the atom; crystallographic address recorded in `PlacedAtomTracker`.
2. **Bonds** — created from motif bond templates via the tracker.
3. **`rm_unbonded`** (gated; default on) — `remove_lone_atoms`: single pass removing zero-bond atoms (removing a 0-bond atom can't create new 0-bond atoms, so no waves needed). Made a flag in #363; was unconditional before.
4. **`rm_single`** (gated) — `remove_single_bond_atoms`: waves of ≤1-bond removals until fixpoint (monotone → terminates).
5. **`surf_recon`** (gated; near-cubic zincblende diamond/Si only, via `get_reconstruction_params`) — per-atom classification (depth ≤ `BULK_DEPTH_THRESHOLD = 0.5`, exactly 2 bonds, axis-aligned) → primary selection via truth tables over crystallographic addresses (**`invert_phase`** XOR-ed in, per atom) → partner via offset table → `apply_dimer_reconstruction` per validated pair (**`passivate` is already a per-dimer parameter** here: it selects geometry constants and adds the two tilted H atoms, flagging the carbons).
6. **`passivate`** (gated) — `hydrogen_passivate`: per tracked atom not flagged in step 5, compare actual vs. motif-expected bonds; per missing motif bond, place an H along the template direction.

Every boolean is already consumed at per-atom or per-feature granularity — the flags only gate loops — so per-position settings are a plumbing change, not an algorithmic redesign.

**New plumbing** (all in `lattice_fill`):

```rust
/// One region, extracted from a MaterializeRegion record by the node layer.
pub struct RegionSpec {
    pub geometry: GeoNode,
    pub margin: f64,                 // resolved: record value or DEFAULT_REGION_MARGIN
    pub passivate: Option<bool>,
    pub rm_single: Option<bool>,
    pub surf_recon: Option<bool>,
    pub invert_phase: Option<bool>,
    pub rm_unbonded: Option<bool>,
}

// LatticeFillConfig gains:  pub regions: Vec<RegionSpec>,   // empty = today's behavior

/// Per-field painter's resolution against root + regions.
struct SettingsResolver<'a> { root: &'a LatticeFillOptions, regions: &'a [RegionSpec] }
impl SettingsResolver<'_> {
    /// Walk regions last → first; membership = sdf(p) ≤ margin; first region
    /// with the field set wins; early-exit when all five fields are filled;
    /// remaining fields fall back to root.
    fn resolve_at(&self, p: DVec3) -> LatticeFillOptions { ... }
    /// `root.f || any region sets Some(true)` — replaces the old global gates.
    fn enabled_anywhere(&self, field) -> bool { ... }
}
```

**Per-step changes:**

- Steps 1–2: **unchanged**. (Regions affect surface treatment only; atom placement is still driven solely by the main geometry. Per-region *placement* effects — doping — are a future extension, see §B10.)
- **Step 3 (`rm_unbonded`)**: gate becomes `enabled_anywhere`; removal filters candidates to atoms whose `resolve_at(position).remove_unbonded_atoms` is true. Implemented as a predicate variant in `atomic_structure_utils` (`remove_lone_atoms_filtered(structure, eligible: &dyn Fn(DVec3) -> bool)`), the existing function being the always-true case — same pattern as step 4. Single pass (a zero-bond atom's removal never lowers another atom's bond count), so no fixpoint concern.
- **Step 4 (`rm_single`)**: gate becomes `enabled_anywhere`; the removal waves filter candidates to atoms whose `resolve_at(position).remove_single_bond_atoms` is true — in both the initial scan and the neighbor re-check. Implemented as a predicate variant in `atomic_structure_utils` (`remove_single_bond_atoms_filtered(structure, recursive, eligible: &dyn Fn(DVec3) -> bool)`) so the utils module stays independent of `lattice_fill`; the existing function becomes the always-true case. Monotone → still terminates.
- **Step 5 (`surf_recon`)**: runs when `enabled_anywhere`. Classification (`process_atoms`) is unchanged except `is_primary_dimer_atom` receives the **per-atom** `invert_phase` resolved at that atom's position. The apply loop gates each validated pair on `resolve_at(midpoint of the two atom positions)`: skip if `surf_recon` is off there; the same lookup supplies `passivate` for the dimer's H atoms (already a per-dimer parameter of `apply_dimer_reconstruction`). The existing motif/cell gating (`get_reconstruction_params`) is untouched — a `surf_recon: true` region on a non-supported structure is a no-op, same as the global flag today.
- **Step 6 (`passivate`)**: runs when `enabled_anywhere`; the global gate moves inside the loop — per tracked atom, resolve once at the atom's position and skip its dangling-bond scan if `passivate` is false there. (Decision point = the **existing atom's** position, not the would-be H position: the atom is the stable side of the dangling bond.)

**Boundary effects** (all deterministic; document, don't fight):

- Overlapping margins of adjacent regions → array order decides in the overlap band.
- `invert_phase` differing across a region boundary → atoms straddling it may classify both-primary or both-secondary → no dimer forms there; those atoms fall through to ordinary passivation (if enabled at their position).
- `rm_single` removing an atom in an enabled region can leave a 1-bond neighbor in a disabled region; the neighbor stays and is passivated later. Well-defined.

### B6. `materialize` node changes

`eval` reads pin 6 with `evaluate_arg` (not `_required`):

- `NetworkResult::None` → `regions: vec![]` (today's behavior).
- `NetworkResult::Array(items)` → `parse_regions_from_records(items)`, mirroring `atom_replace::parse_rules_from_records`: per item, `volume` must extract to a Blueprint payload (its `geo_tree_root` is taken; structure ignored), `margin` a `Float` or `None` (→ `DEFAULT_REGION_MARGIN`), the five settings `Bool` or `None`. Any malformed item → `NetworkResult::Error` with the item index in the message.
- Anything else → `NetworkResult::Error` naming the actual type.

`MaterializeData` storage, `get_text_properties` / `set_text_properties`, and the parameter-element machinery are unchanged. There is **no `regions` text property** — like `atom_replace.rules`, wired region data is recomputed per eval, never stored.

### B7. UI

- The side-panel checkboxes stay **enabled** when `regions` is connected — the node settings are the root, not a shadowed default (contrast with the `atom_replace` graying convention; the semantic difference is deliberate and worth a one-line annotation in the panel: *"Regions override these settings inside their volumes."*).
- No subtitle change (`materialize` has none today).
- No new editors: regions are authored with `record_construct` + array nodes.

### B8. Performance

Region SDF lookups happen only at **surface decision points** — ≤1-bond candidates (step 4), depth ≤ 0.5 Å classification survivors (step 5), dangling-bond atoms (step 6) — a small fraction of the structure, against typically simple region geometry (a few half-spaces/primitives). Direct `GeoNode::implicit_eval_3d` per query is expected to be fine. If profiling ever disagrees: memoize `resolve_at` per atom id (steps 5/6 query the same atoms), or batch through `BatchedImplicitEvaluator` per region. Not designed in up front. (Part A, by contrast, queries *every* atom once, so it batches from the start — see A3.)

### B9. Validation, edge cases, compatibility

- Type checking of the `regions` wire is ordinary record-array validation; no new validator rules. Runtime parse errors are localized `NetworkResult::Error`s (non-blocking by the existing litmus test — the evaluator handles them cleanly per node).
- Empty array / all-fields-unset regions / region volume disjoint from the structure → no-ops.
- `.cnnd` migration: none. The new pin appears unconnected on existing `materialize` nodes; no version bump. `MaterializeRegion` joins the reserved built-in namespace with the same pre-existing-user-def caveat handled for `ElementMapping` (see that design's Open Question 1 — same resolution applies).
- Undo: no new commands (wire connections and stored bools are covered by existing machinery).

### B10. Future extensions (explicit non-goals now)

- **Per-region `parameter_element_values`** (region-based doping/element substitution *at fill time*). Powerful, but placement-affecting: the lookup point is the fill/flush loop, with real performance weight (every candidate site, not just surface atoms). Deferred; the record field can be added to the built-in def later at low cost (built-in defs are not serialized; `repair_node_network` refreshes construct pin layouts). (Note: the *post-materialize* element-replacement-by-volume case is already covered by Part A's `atom_replace` region pin — this future item is specifically about replacement during materialization.)
- **Standalone surface-reconstruction node** (the issue's full "disentangled pipeline"): rejected for now. Reconstruction and template passivation consume the `PlacedAtomTracker` (crystallographic addresses) and motif templates, which exist only during materialization — a post-`Crystal` node would need addresses carried in `CrystalData` or re-derived from positions, both significant and fragile after atom edits. The regions mechanism delivers the user-visible benefit without that cost.
- **Tree-builder combinator node** compiling to the ordered array, if flat arrays ever prove unwieldy in practice.

---

# Phasing

Phases are grouped by the two parts; Part A and Part B are largely independent (Part A does **not** depend on `Optional[T]`; Part B does).

## Part A phases

### Phase A1 — Region engine for atom ops (helper + first consumer)

1. `map_atomic_in_region` in `evaluator/atom_op.rs` (batched membership, `region == None` fast path), reusing `DEFAULT_REGION_MARGIN`.
2. Add the optional `region` pin to **`atom_replace`** and route its loop through the predicate (first consumer, proves the seam end-to-end).

Tests (`rust/tests/structure_designer/`): disconnected `region` → output identical to today; cuboid Crystal + half-space region → only in-region atoms replaced; boundary-coincident region captures `margin`-near atoms; region disjoint from structure → no-op; phase preservation (Crystal-in/out, Molecule-in/out). Serialization (mirrors Part B's `.cnnd` coverage): an existing pre-pin `.cnnd` loads with the new `region` pin unconnected (backward-compat, no migration); a network with a *wired* `region` pin round-trips unchanged.

**Verification note (manual).** *Status: implemented; automated coverage green (`rust/tests/structure_designer/atom_replace_region_test.rs`).* In the running app: materialize a diamond slab, wire it into an `atom_replace` with rule `C→Si`, and display the result. Add a `half_space` whose plane cuts through the slab and wire it into `atom_replace`'s `region` pin — confirm only the atoms on the in-region side recolour to silicon while the rest stay carbon; disconnect `region` and confirm all atoms recolour (global behaviour returns). Nudge the half-space so its plane sits exactly on a layer of atoms and confirm that layer is still captured (the 0.1 Å margin).

### Phase A2 — Roll out to the remaining ops

`add_hydrogen`, `remove_hydrogen`, `infer_bonds` gain the `region` pin via the same helper, each gating on the host-atom membership rule fixed in A4 (`infer_bonds` = one-endpoint-inside).

Tests: per-node regional behavior + disconnected-pin equivalence; `infer_bonds` one-endpoint-inside policy; `remove_hydrogen` strips an out-of-boundary H whose host is in-region; `add_hydrogen` places a passivating H even when it lands *outside* the region, so long as its host atom is in-region (the "newly created atoms are never membership-tested" invariant, A4). **Composition** (the load-bearing Part A claim that "multiple regions = chained nodes"): two region-gated ops in sequence — e.g. `atom_replace(region A) → atom_replace(region B)` with overlapping and disjoint A/B — apply independently and their effects accumulate exactly as separate single-region passes would.

**Verification note (manual).** *Status: implemented; automated coverage green (`rust/tests/structure_designer/region_atom_ops_test.rs`).* In the app, repeat the A1 region walkthrough for each rolled-out node: `add_hydrogen` (only in-region dangling bonds gain H — and an H placed *across* the boundary still appears, since its host is in-region), `remove_hydrogen` (only H bonded to in-region hosts is stripped, including an H whose own position is just outside the region), `infer_bonds` (a bond forms when at least one endpoint is in-region). Then chain two region-gated `atom_replace` nodes with overlapping and with disjoint regions and confirm the effects accumulate identically to two separate single-region passes — this is the load-bearing "multiple regions = chained nodes" claim, so it is worth eyeballing directly.

### Phase A3 — `freeze` / `unfreeze` nodes ( + `relax` already honors frozen)

*Status: implemented 2026-06-29 (`rust/src/structure_designer/nodes/freeze.rs`, tests `rust/tests/structure_designer/freeze_test.rs`).*

1. Register `freeze` / `unfreeze` node types (region-gated metadata edits via `map_atomic_in_region`).
2. ~~`relax.eval` collects `is_frozen()` topology indices and passes them to `minimize_energy`.~~ **Not needed** — `minimize_energy` already derives the frozen set from the atom flags (see §A6). Phase A3 added only a regression test, not a `relax` change.

Tests: `freeze`/`unfreeze` set/clear bit 2 in-region (and globally when disconnected); `relax` holds frozen atoms fixed while moving free ones. Node registration/serialization: a network containing `freeze`/`unfreeze` nodes (with and without a wired `region`) round-trips through `.cnnd`. Composition: `freeze(region A) → freeze(region B)` leaves the union of A and B frozen (chained metadata edits accumulate).

**Verification note (manual).** In the app, drop a `freeze` node with a `half_space` region on a structure and confirm — via the frozen-atom rendering/flag — that only in-region atoms are frozen, and that `unfreeze` clears them; with `region` disconnected, confirm both act on all atoms. Then wire `freeze → relax` and run relaxation: the frozen atoms must hold their positions while their mobile neighbours move and settle (`relax` honors the flag via `minimize_energy`, as it already did before this phase).

## Part B phases

(Unchanged from the original #346 plan; require `doc/design_optional_type.md` Phases 1–2 for the type + `record_construct` optional-field behavior. The **tri-state literal editor** (set/unset per field) lands in that doc's Phase 3; until then the three states are still reachable by wiring a `bool`/`float` node for "force" and leaving the pin unwired for "inherit", so Part B's Phases B1–B2 do not block on it.)

### Phase B1 — Region engine in `lattice_fill` (no node changes)

*Status: implemented 2026-06-29. `RegionSpec` / `SettingsResolver` in `lattice_fill/config.rs`; `LatticeFillConfig.regions` (default empty); filtered cleanup variants in `atomic_structure_utils.rs`; per-position gates wired into steps 3–6 of `fill_algorithm.rs` (+ `surface_reconstruction.rs` / `hydrogen_passivation.rs` take the resolver). Tests: `rust/tests/crystolecule/lattice_fill_regions_test.rs` (9 tests). No node-network surface yet — that lands in Phase B2.*

1. `RegionSpec`, `DEFAULT_REGION_MARGIN`, `SettingsResolver` (resolve_at / enabled_anywhere), `LatticeFillConfig.regions` (default empty).
2. Step-3 predicate variant `remove_lone_atoms_filtered` and step-4 variant `remove_single_bond_atoms_filtered` in `atomic_structure_utils`; wire the per-position gates into steps 3–6 per §B5.

Tests (`rust/tests/crystolecule/`, pure — configs constructed directly, no node network): empty-regions equivalence with today's snapshots; cuboid blueprint + boundary-coincident half-space region per setting (`passivate` off on top only; `surf_recon` on top only; `rm_single` regional; `rm_unbonded` regional → lone atoms kept outside the region but stripped inside; `invert_phase` regional → dimer row phase flips inside, no dimer at the seam); margin behavior (default captures `sdf ≈ 0` atoms; negative margin excludes the boundary layer); overlap + array-order override; per-field inheritance (region setting only one field).

**Verification note (manual).** Phase B1 is pure `lattice_fill` plumbing with **no node-network surface**, so there is nothing to drive from the UI yet — manual verification is the automated `tests/crystolecule/` suite above. For an ad-hoc spot-check, construct a `LatticeFillConfig` with a single boundary-coincident `RegionSpec` in a scratch test or binary and diff the produced `Crystal` against the global-flag baseline (e.g. `passivate` off on the top region only should leave exactly the top surface's dangling bonds unterminated).

### Phase B2 — `MaterializeRegion` def + `regions` pin

1. Register the built-in def (§B1) in `NodeTypeRegistry::new()`; reserved-name guards come free from the `atom_replace.rules` Phase-A infrastructure (add the mirror tests).
2. `materialize` node: pin 6 (§B2), optional in `get_parameter_metadata`; `eval` parsing (§B6).

Tests (`rust/tests/structure_designer/`): disconnected pin → snapshot-identical output; wired regions drive per-position settings end-to-end through the node; malformed item → indexed `Error`; node-snapshot update for the new pin; `.cnnd` round-trip of a network with a wired regions chain; reserved-name tests (`add/rename/update/delete_record_type_def("MaterializeRegion")` rejected; network named `MaterializeRegion` rejected).

**Verification note (manual).** In the app, build a region record with `record_construct(MaterializeRegion)` — a `half_space` through the top surface into `volume`, `surf_recon: true` (others unset) — wrap it in a one-element array, and wire it into `materialize.regions`. Confirm the top surface reconstructs/depassivates per the region while the rest of the slab keeps the node's root settings, and that the side-panel checkboxes stay **enabled** (regions layer on top of the root, they don't shadow it). Feed a deliberately malformed array element and confirm a localized, index-named node error rather than a blank viewport. Finally confirm the reserved-name guards from the UI: `MaterializeRegion` cannot be created/renamed-to as a record def or network.

### Phase B3 — UI polish & docs

1. Side-panel annotation when `regions` is connected (§B7).
2. Reference guide / tutorial section: building a region spec (half-space through the top surface → `record_construct` → `materialize.regions`), the root + painter's mental model, margin semantics.

**Verification note (manual).** Walk through, on a diamond slab: a depassivated top surface; a reconstructed-top-only surface; two overlapping regions whose array order decides the result in the overlap band; a negative margin demonstrating boundary-layer exclusion; and the checkbox-root behaviour with `regions` connected (the panel checkboxes stay live and carry the *"Regions override these settings inside their volumes."* annotation). This phase is itself the UI/docs polish, so its manual walkthrough doubles as the acceptance check.

---

# Open Questions

**Part A**

1. **Per-node margin override.** Part A uses the shared `DEFAULT_REGION_MARGIN` with no per-node knob. Should the region-gated nodes also expose a `margin` (stored field and/or pin), like `MaterializeRegion.margin`? Proposed: **defer** — post-materialize atoms rarely sit on a region boundary, so the default suffices; add a `margin` field only if a real case needs it. (Cheap to add later.)
2. **Invert / "outside the region".** Achievable today by authoring (flip the `half_space`, `diff` from a big box). Proposed: rely on geometry for v1; an `invert: Bool` convenience can be added to the region mechanism later if authoring proves annoying.

**Part B** (carried over)

3. **Dimer decision point.** §B5 proposes the **midpoint** of the two dimer atoms (least sensitive to which atom is "primary"). Alternative: the primary atom's position (simpler, slightly biased). Either is deterministic; pick one and document it.
4. **Reserve a `params` field now?** Per-region parameter-element values (§B10) would extend the def later. Since built-in defs are not serialized and `repair_node_network` refreshes construct pin layouts on def change, adding the field later is cheap — proposed: **defer**, don't reserve.
5. **Region-volume structure mismatch.** The region Blueprint's `structure` is ignored (§B1, A7). Should a mismatch with the materialized Blueprint's structure produce a warning badge? Proposed: no for v1 — it would make `half_space`-on-default-structure regions (the common case) noisy; revisit if users get confused.
6. **Default margin value.** 0.1 Å per §B4's two-scales argument. Confirm against real lattices beyond diamond/Si (the bound that matters is the smallest interlayer spacing among supported structures).
