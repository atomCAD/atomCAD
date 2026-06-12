# Per-Region Materialization Settings (`materialize.regions`)

## Status: Draft

## Depends on

- `doc/design_optional_type.md` — the `Optional[T]` data type (prerequisite; provides the set-vs-inherit semantics of the region record's fields).
- `doc/design_atom_replace_rules_input.md` Phase A — built-in record-def infrastructure (`built_in_record_type_defs`, `lookup_record_type_def`), shipped.

GitHub issue: [atomCAD/atomCAD#346](https://github.com/atomCAD/atomCAD/issues/346) — "improve on surface modification features: blueprint specified regions & disentanglement from materialize node".

## Motivation

`materialize` converts a Blueprint into a Crystal via `fill_lattice`, controlled by four booleans — `passivate`, `rm_single`, `surf_recon`, `invert_phase` — that today apply **globally** to the entire structure. Issue #346 asks for crystolecule parts with *different* surface treatments on different regions/zones: e.g. a reconstructed and/or depassivated top surface with ordinary passivation everywhere else, specified by simple volume proxies (typically a half-space through the top surface). The issue also asks to make the materialization process "less magic and more user-space exposed".

This design keeps `materialize` as the single Blueprint → Crystal conversion point and adds an optional **`regions`** input pin: an ordered array of region records, each pairing a Blueprint volume with per-field-optional settings overrides. The specification is assembled from ordinary nodes (`half_space`, CSG, `record_construct`, array nodes), so it is plain, inspectable, parametric node-network data — that is the "user-space exposed" part — while the algorithms that genuinely require fill-time data (crystallographic addresses, motif templates) stay inside materialization.

### Why a flat ordered array, not a tree

A tree of volumes with child-overrides-parent semantics was considered (it is the natural mental model: a root for all of space, children refining sub-volumes). Rejected as the *representation* for two reasons:

1. **Recursive record types are unrepresentable.** The record-type-def dependency graph is acyclicity-enforced (`NodeTypeRegistry::check_no_cycle`), so a `RegionNode = {…, children: [RegionNode]}` def is rejected by construction. A tree would need a dedicated opaque spec type + combinator nodes — significant machinery.
2. **The flat array is equally expressive.** Resolving a point against a tree returns its deepest containing node; flattening the tree depth-first yields an ordered list where "last containing region wins" gives the identical answer. With `Optional` fields, per-field resolution additionally recovers the tree's *inheritance*: a region that sets only `surf_recon` transparently inherits everything else (see Semantics). If a genuine tree builder is ever wanted, it can compile down to this array — the semantics are forward-compatible.

## Design

### 1. Built-in `MaterializeRegion` record type

Registered in `NodeTypeRegistry::new()` next to `ElementMapping`, with all the standard built-in-def properties (reserved name, immutable, not serialized, discoverable through `lookup_record_type_def` and the unified type dropdowns):

```text
MaterializeRegion = {
    volume:       Blueprint,
    margin:       Optional[Float],   // membership tolerance in Å; unset → DEFAULT_REGION_MARGIN
    passivate:    Optional[Bool],
    rm_single:    Optional[Bool],
    surf_recon:   Optional[Bool],
    invert_phase: Optional[Bool],
}
```

Authored field order above drives the `record_construct` pin layout: `volume` is the only required pin; everything else is optional (unwired → unset, per `doc/design_optional_type.md` §5).

**`volume` is a Blueprint** because that is what every 3D shape node outputs — there is no bare 3D-geometry type at the network level. Users build region volumes with the exact nodes they already know (`half_space`, `cuboid`, `sphere`, CSG combinations), in the same real space as the Blueprint being materialized. Only the region Blueprint's `geo_tree_root` is consumed; its `structure` is **ignored** (documented; see Open Question 3 for the warn-on-mismatch alternative).

### 2. Pin signature

| Index | Direction | Name | Type | Required | Notes |
|---|---|---|---|---|---|
| 0 | Input | `shape` | `Blueprint` | Yes | unchanged |
| 1 | Input | `passivate` | `Bool` | No | unchanged (overrides stored bool) |
| 2 | Input | `rm_single` | `Bool` | No | unchanged |
| 3 | Input | `surf_recon` | `Bool` | No | unchanged |
| 4 | Input | `invert_phase` | `Bool` | No | unchanged |
| 5 | Input | **`regions`** | **`[Record(Named("MaterializeRegion"))]`** | **No** | new |
| — | Output | (pin 0) | `Crystal` | — | unchanged |

### 3. Semantics: root + per-field painter's algorithm

- The node's **effective settings** (stored booleans, individually overridden by the wired bool pins exactly as today) form the **root**: they apply to all of space. Note the contrast with `atom_replace.rules`, where a connected pin *replaces* the stored data — here the regions **layer on top of** the root; the node's own settings stay meaningful (and the side-panel checkboxes stay enabled when `regions` is connected).
- Regions apply **in array order**. For a query point `p` and a settings field `f`: walk the regions **last → first**; the first (i.e. latest-in-array) region that *contains* `p` **and** has `f` set supplies the value; if none does, the root supplies it. Resolution is per field, so a region that sets only `surf_recon: true` changes nothing else — unset fields are transparently inherited from earlier matching regions and ultimately from the root.
- A region **contains** `p` iff `region_sdf(p) ≤ margin` (see §4).
- Disconnected `regions` pin / empty array → exactly today's behavior. A region whose settings are all unset is a no-op.

### 4. Boundary tolerance (`margin`)

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

Per-region override via the `margin` field (`Optional[Float]`, unset → default). **Negative margins are allowed** — they shrink the effective region, e.g. to deliberately exclude the boundary layer. No clamping.

Margin solves *membership* (in-or-out of one region); it does not solve *tie-breaking* — two adjacent regions whose margins meet at a shared face now have a guaranteed overlap band there, and the **array order** decides inside it, deterministically. Both rules are needed; the doc/UI copy should state them together.

### 5. The pipeline today (recap) and the per-position changes

`fill_lattice` (`crystolecule/lattice_fill/fill_algorithm.rs`) runs, in order:

1. **Fill** — recursive box subdivision; main-geometry SDF batch-evaluated at candidate motif sites; atoms placed where `sdf ≤ 0.01`; per-atom depth (−sdf) stored on the atom; crystallographic address recorded in `PlacedAtomTracker`.
2. **Bonds** — created from motif bond templates via the tracker.
3. **Lone-atom cleanup** — always.
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
}

// LatticeFillConfig gains:  pub regions: Vec<RegionSpec>,   // empty = today's behavior

/// Per-field painter's resolution against root + regions.
struct SettingsResolver<'a> { root: &'a LatticeFillOptions, regions: &'a [RegionSpec] }
impl SettingsResolver<'_> {
    /// Walk regions last → first; membership = sdf(p) ≤ margin; first region
    /// with the field set wins; early-exit when all four fields are filled;
    /// remaining fields fall back to root.
    fn resolve_at(&self, p: DVec3) -> LatticeFillOptions { ... }
    /// `root.f || any region sets Some(true)` — replaces the old global gates.
    fn enabled_anywhere(&self, field) -> bool { ... }
}
```

**Per-step changes:**

- Steps 1–3: **unchanged**. (Regions affect surface treatment only; atom placement is still driven solely by the main geometry. Per-region *placement* effects — doping — are a future extension, see §10.)
- **Step 4 (`rm_single`)**: gate becomes `enabled_anywhere`; the removal waves filter candidates to atoms whose `resolve_at(position).remove_single_bond_atoms` is true — in both the initial scan and the neighbor re-check. Implemented as a predicate variant in `atomic_structure_utils` (`remove_single_bond_atoms_filtered(structure, recursive, eligible: &dyn Fn(DVec3) -> bool)`) so the utils module stays independent of `lattice_fill`; the existing function becomes the always-true case. Monotone → still terminates.
- **Step 5 (`surf_recon`)**: runs when `enabled_anywhere`. Classification (`process_atoms`) is unchanged except `is_primary_dimer_atom` receives the **per-atom** `invert_phase` resolved at that atom's position. The apply loop gates each validated pair on `resolve_at(midpoint of the two atom positions)`: skip if `surf_recon` is off there; the same lookup supplies `passivate` for the dimer's H atoms (already a per-dimer parameter of `apply_dimer_reconstruction`). The existing motif/cell gating (`get_reconstruction_params`) is untouched — a `surf_recon: true` region on a non-supported structure is a no-op, same as the global flag today.
- **Step 6 (`passivate`)**: runs when `enabled_anywhere`; the global gate moves inside the loop — per tracked atom, resolve once at the atom's position and skip its dangling-bond scan if `passivate` is false there. (Decision point = the **existing atom's** position, not the would-be H position: the atom is the stable side of the dangling bond.)

**Boundary effects** (all deterministic; document, don't fight):

- Overlapping margins of adjacent regions → array order decides in the overlap band.
- `invert_phase` differing across a region boundary → atoms straddling it may classify both-primary or both-secondary → no dimer forms there; those atoms fall through to ordinary passivation (if enabled at their position).
- `rm_single` removing an atom in an enabled region can leave a 1-bond neighbor in a disabled region; the neighbor stays and is passivated later. Well-defined.

### 6. `materialize` node changes

`eval` reads pin 5 with `evaluate_arg` (not `_required`):

- `NetworkResult::None` → `regions: vec![]` (today's behavior).
- `NetworkResult::Array(items)` → `parse_regions_from_records(items)`, mirroring `atom_replace::parse_rules_from_records`: per item, `volume` must extract to a Blueprint payload (its `geo_tree_root` is taken; structure ignored), `margin` a `Float` or `None` (→ `DEFAULT_REGION_MARGIN`), the four settings `Bool` or `None`. Any malformed item → `NetworkResult::Error` with the item index in the message.
- Anything else → `NetworkResult::Error` naming the actual type.

`MaterializeData` storage, `get_text_properties` / `set_text_properties`, and the parameter-element machinery are unchanged. There is **no `regions` text property** — like `atom_replace.rules`, wired region data is recomputed per eval, never stored.

### 7. UI

- The side-panel checkboxes stay **enabled** when `regions` is connected — the node settings are the root, not a shadowed default (contrast with the `atom_replace` graying convention; the semantic difference is deliberate and worth a one-line annotation in the panel: *"Regions override these settings inside their volumes."*).
- No subtitle change (`materialize` has none today).
- No new editors: regions are authored with `record_construct` + array nodes.

### 8. Performance

Region SDF lookups happen only at **surface decision points** — ≤1-bond candidates (step 4), depth ≤ 0.5 Å classification survivors (step 5), dangling-bond atoms (step 6) — a small fraction of the structure, against typically simple region geometry (a few half-spaces/primitives). Direct `GeoNode::implicit_eval_3d` per query is expected to be fine. If profiling ever disagrees: memoize `resolve_at` per atom id (steps 5/6 query the same atoms), or batch through `BatchedImplicitEvaluator` per region. Not designed in up front.

### 9. Validation, edge cases, compatibility

- Type checking of the `regions` wire is ordinary record-array validation; no new validator rules. Runtime parse errors are localized `NetworkResult::Error`s (non-blocking by the existing litmus test — the evaluator handles them cleanly per node).
- Empty array / all-fields-unset regions / region volume disjoint from the structure → no-ops.
- `.cnnd` migration: none. The new pin appears unconnected on existing `materialize` nodes; no version bump. `MaterializeRegion` joins the reserved built-in namespace with the same pre-existing-user-def caveat handled for `ElementMapping` (see that design's Open Question 1 — same resolution applies).
- Undo: no new commands (wire connections and stored bools are covered by existing machinery).

### 10. Future extensions (explicit non-goals now)

- **Per-region `parameter_element_values`** (region-based doping/element substitution). Powerful, but placement-affecting: the lookup point is the fill/flush loop, with real performance weight (every candidate site, not just surface atoms). Deferred; the record field can be added to the built-in def later at low cost (built-in defs are not serialized; `repair_node_network` refreshes construct pin layouts).
- **Region/mask pins on post-materialize atom ops** (`add_hydrogen`, `remove_hydrogen`, `atom_replace`, `relax`): restrict an operation to atoms inside a Blueprint volume. Independent, cheap, and covers workflows regions-in-materialize cannot (e.g. region-restricted relaxation to fix steric clashes at treatment boundaries; `relax`'s frozen-atom support maps naturally onto masking). Own mini-design when picked up.
- **Standalone surface-reconstruction node** (the issue's full "disentangled pipeline"): rejected for now. Reconstruction and template passivation consume the `PlacedAtomTracker` (crystallographic addresses) and motif templates, which exist only during materialization — a post-`Crystal` node would need addresses carried in `CrystalData` or re-derived from positions, both significant and fragile after atom edits. The regions mechanism delivers the user-visible benefit without that cost.
- **Tree-builder combinator node** compiling to the ordered array, if flat arrays ever prove unwieldy in practice.

## Phasing

Phases R1–R3 require `doc/design_optional_type.md` Phases 1–2 (R2 also benefits from its Phase 3 for authoring UX, but does not hard-depend on it — the built-in def needs no schema editor).

### Phase R1 — Region engine in `lattice_fill` (no node changes)

1. `RegionSpec`, `DEFAULT_REGION_MARGIN`, `SettingsResolver` (resolve_at / enabled_anywhere), `LatticeFillConfig.regions` (default empty).
2. Step-4 predicate variant `remove_single_bond_atoms_filtered` in `atomic_structure_utils`; wire the per-position gates into steps 4–6 per §5.

Tests (`rust/tests/crystolecule/`, pure — configs constructed directly, no node network): empty-regions equivalence with today's snapshots; cuboid blueprint + boundary-coincident half-space region per setting (`passivate` off on top only; `surf_recon` on top only; `rm_single` regional; `invert_phase` regional → dimer row phase flips inside, no dimer at the seam); margin behavior (default captures `sdf ≈ 0` atoms; negative margin excludes the boundary layer); overlap + array-order override; per-field inheritance (region setting only one field).

### Phase R2 — `MaterializeRegion` def + `regions` pin

1. Register the built-in def (§1) in `NodeTypeRegistry::new()`; reserved-name guards come free from the Phase-A infrastructure (add the mirror tests).
2. `materialize` node: pin 5 (§2), optional in `get_parameter_metadata`; `eval` parsing (§6).

Tests (`rust/tests/structure_designer/`): disconnected pin → snapshot-identical output; wired regions drive per-position settings end-to-end through the node; malformed item → indexed `Error`; node-snapshot update for the new pin; `.cnnd` round-trip of a network with a wired regions chain; reserved-name tests (`add/rename/update/delete_record_type_def("MaterializeRegion")` rejected; network named `MaterializeRegion` rejected).

### Phase R3 — UI polish & docs

1. Side-panel annotation when `regions` is connected (§7).
2. Reference guide / tutorial section: building a region spec (half-space through the top surface → `record_construct` → `materialize.regions`), the root + painter's mental model, margin semantics.

Manual verification: depassivated-top-surface walkthrough on a diamond slab; reconstructed-top-only; two overlapping regions demonstrating order; negative margin demonstrating boundary-layer exclusion; checkbox-root behavior with regions connected.

## Open Questions

1. **Dimer decision point.** This doc proposes the **midpoint** of the two dimer atoms (least sensitive to which atom is "primary"; with default margins, whole pairs near a face land robustly on one side). Alternative: the primary atom's position (simpler, slightly biased). Either is deterministic; pick one and document it.
2. **Reserve a `params` field now?** Per-region parameter-element values (§10) would extend the def later. Since built-in defs are not serialized and `repair_node_network` refreshes construct pin layouts on def change, adding the field later is cheap — proposed: **defer**, don't reserve.
3. **Region-volume structure mismatch.** The region Blueprint's `structure` is ignored (§1). Should a mismatch with the materialized Blueprint's structure produce a warning badge? Proposed: no for v1 — it would make `half_space`-on-default-structure regions (the common case) noisy; revisit if users get confused.
4. **Default margin value.** 0.1 Å per §4's two-scales argument. Confirm against real lattices beyond diamond/Si (the bound that matters is the smallest interlayer spacing among supported structures).
