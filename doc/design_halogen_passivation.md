# Design: Halogen Passivation (issue #405)

**Issue:** [#405 — generalize `add_hydrogen` to `add_halogen` or separate extra
node subsuming the function](https://github.com/atomCAD/atomCAD/issues/405)

> Halogen passivations are commonly needed. Replacing hydrogen with a desired
> halogen with the `atom_replace` node is possible but needs a subsequent
> minimization causing not necessarily reproducible starting atom positions.

## 1. Motivation

Today the only way to get a halogen-terminated surface is `add_hydrogen` (or
`materialize` with `passivate`) followed by `atom_replace` H→F/Cl/Br/I. That
leaves every halogen sitting at the **H bond length** (e.g. 1.09 Å for C–H
instead of ~1.35 Å for C–F), so a `relax` pass is mandatory, and the relaxed
positions depend on optimizer trajectory — not reproducible as a canonical
starting geometry.

The fix is to let the passivation step place the terminating atom **directly at
the correct host–terminator bond length along the ideal bond direction**. The direction
machinery is already element-agnostic; only the element and the bond length are
hardcoded to hydrogen.

## 2. Current state (survey)

There are **three** places that place passivating hydrogens:

| Site | File | Used by | Bond length logic |
|---|---|---|---|
| General passivation | `crystolecule/hydrogen_passivation.rs` (`add_hydrogens[_filtered]`) | `add_hydrogen` node; atom_edit panel "Add H" action | `XH_BOND_LENGTHS` table (C,N,O,Si,P,S,B,Ge) + covalent-radii-sum fallback |
| Lattice-fill passivation | `crystolecule/lattice_fill/hydrogen_passivation.rs` (`hydrogen_passivate`) | `materialize` node via `fill_lattice` | `C_H_BOND_LENGTH = 1.09` special case + covalent-radii-sum fallback |
| Surface reconstruction | `crystolecule/lattice_fill/surface_reconstruction.rs` (`add_hydrogen_passivation`) | `materialize` with `surf_recon` | `C_H_BOND_LENGTH` / `SI_H_BOND_LENGTH` constants (via `SurfaceReconstructionParams.h_bond_length`) |

⚠ Naming collision: `C_H_BOND_LENGTH` exists **twice** — once in
`lattice_fill/hydrogen_passivation.rs` (site 2) and once in
`surface_reconstruction.rs` (site 3). They are separate constants (both 1.09);
don't confuse them when editing.

Shared observations:

- **Direction computation is element-agnostic.** The general path derives open
  directions from hybridization geometry (`compute_open_directions`); the
  lattice-fill path derives them from the missing motif site's direction. Neither
  cares what atom is placed at the end.
- **Valence bookkeeping is unaffected by the passivant.** Halogens are
  monovalent like H — one terminating atom per open valence slot / dangling
  bond, single bond. `needed = max_bonds - current` and the motif dangling-bond
  scan both stay untouched.
- **The bond length is the only chemistry that changes.** It is computed in
  three places whose H values differ *deliberately by context* (molecular
  table / bulk radii-sum fallback / surface-reconstruction-specific constants
  — see D2); they are not duplicates of one another.
- **The passivation flag (Atom flags bit 1) is set asymmetrically across the
  sites.** The general path (and the atom_edit panel action) flags the
  **terminator H** it adds; surface reconstruction flags the two dimer
  **hosts** — not its own H terminators; lattice-fill flags **nothing**. The
  flag has exactly two readers (`add_hydrogens_filtered`'s
  `skip_already_passivated` host check and lattice-fill's per-atom skip), and
  both consume the same single meaning: *"this host was handled by surface
  reconstruction — don't passivate it again."* The terminator-marking done by
  the general path is informational only — a flagged terminator is inert at
  both readers (it is element 1 / saturated, so it is skipped for other
  reasons anyway).

## 3. Design decisions

### D1. Allowed passivants: H, F, Cl, Br, I

The passivation element is restricted to the monovalent set
`{1, 9, 17, 35, 53}`. This keeps the algorithm structurally identical (one atom,
one single bond per open slot). Any other atomic number is rejected at eval time
with a localized `NetworkResult::Error` (no validator rule needed — the eval
error is the surfacing, per the non-blocking-validation litmus test).

Multi-atom caps (–OH, –CH₃, –CF₃ …) are explicitly **out of scope** (§8) — they
need orientation decisions (dihedral of the cap) and are a different feature.

### D2. One shared bond-length primitive — for halogens only; H stays per-site

**Research finding (2026-07).** The three sites' H bond lengths differ
*deliberately, by chemical context* — they are not duplication to clean up:

- The general path's `XH_BOND_LENGTHS` are **molecular** equilibrium values
  (silane Si–H = 1.48 Å) — right for passivating arbitrary molecules.
- Surface reconstruction's `SI_H_BOND_LENGTH = 1.50` matches the
  **surface-specific** value: DFT gives Si–H ≈ 1.51 Å on the Si(100)-2×1:H
  monohydride, longer than silane. The surface/molecular split is real in both
  directions — on Si(100)-2×1:Cl, surface EXAFS measures Si–Cl = 1.95 ± 0.04 Å
  vs ~2.02 Å molecular.
- The lattice-fill covalent-radii-sum fallback (Si–H = 1.11 + 0.31 = 1.42 Å)
  is the least literature-grounded of the three, but it is what existing
  `.cnnd` outputs reproduce.

So hydrogen is **not** unified: for `passivant == 1` each site short-circuits
to its existing H code path, byte-for-byte — the `XH_BOND_LENGTHS` table, the
lattice-fill `C_H_BOND_LENGTH` special case + radii-sum fallback, and the
surf-recon `C_H`/`SI_H` constants all stay in place, untouched. H outputs
cannot shift, **by construction** (no "identical table" argument needed). The
shared primitive covers the halogens only. Add to
`crystolecule/atomic_constants.rs`:

```rust
/// Equilibrium single-bond length host–halogen in Å (molecular values).
/// `halogen` ∈ {F, Cl, Br, I}; hydrogen is handled by each call site's
/// existing per-context path (see doc/design_halogen_passivation.md, D2).
/// Falls back to covalent-radii sum.
pub fn halogen_bond_length(host: i16, halogen: i16) -> f64
```

Table (rounded molecular experimental values; same provenance style as the
existing `XH_BOND_LENGTHS` — Calculla / Wikipedia / NIST CCCBDB):

| host \ halogen | F | Cl | Br | I |
|---|---|---|---|---|
| C (6)  | 1.35 | 1.77 | 1.94 | 2.14 |
| Si (14)| 1.60 | 2.02 | 2.16 | 2.44 |
| Ge (32)| 1.70 | 2.10 | 2.30 | 2.51 |
| B (5)  | 1.31 | 1.75 | 1.87 | 2.10 |
| N (7)  | 1.36 | 1.75 | — | — |
| O (8)  | 1.42 | 1.70 | — | — |
| P (15) | 1.57 | 2.03 | 2.20 | — |
| S (16) | 1.56 | 2.05 | — | — |

(`—` = fall back to covalent-radii sum. Exact values to be double-checked
against CCCBDB during implementation.)

Using the *molecular* halogen values at all three sites — including surface
reconstruction, where the measured surface bond can be a few percent shorter
(the Si–Cl case above) — is a documented approximation refined by `relax`,
same class as the H-calibrated dimer geometry in D6.

### D3. Node shape: rename `add_hydrogen` → `passivate` (RECOMMENDED)

The issue offers "generalize `add_hydrogen` to `add_halogen` or separate extra
node". Options considered:

- **(a) Rename to `passivate`, add an `element` property + pin** *(recommended)*.
  "add_halogen" would be chemically wrong for the H default (H is not a
  halogen); "passivate" names the operation, matches the `materialize.passivate`
  pin vocabulary, and stays truthful for every element choice. The repo has
  done exactly this rename before (`export_xyz` → `export_atoms`, migration
  v6→v7), so the churn is known and bounded: `migrate_v7_to_v8.rs` rewrites
  `node_type_name`, `SERIALIZATION_VERSION` bumps 7→8.
- **(b) Keep the name `add_hydrogen`, add the element property/pin.** No
  migration, but a node named add_hydrogen placing fluorine is actively
  misleading in a saved network.
- **(c) Separate `add_halogen` node.** Duplicates the whole node for a
  one-field difference; two nodes to keep in sync forever. Rejected.

`remove_hydrogen` is **not** renamed or generalized (D7).

### D4. Element selection: stored property + optional appended pin

Both nodes follow the established wired-overrides-stored precedence
(`evaluate_or_default`):

- Stored property, `i16`, serde-default `1` (hydrogen) — named `element` on
  `passivate`, `passivation_element` on `materialize` (text property
  `passiv_elem`, see D10). Old files and the empty `{}` node data load
  unchanged, **no data migration needed** for the property itself.
- New **optional Int input pin, appended as the last pin**
  (arguments are positional — appending keeps existing wires valid):
  - `passivate` (né add_hydrogen): pins `[molecule, region, element]`. This
    knowingly breaks the region-gated-op convention that `region` is the last
    pin — positional wire compatibility wins; do **not** "fix" the order.
  - `materialize`: pins `[shape, passivate, rm_single, surf_recon,
    invert_phase, rm_unbonded, regions, passiv_elem]` (index 7).
- A wired pin makes the passivant network-computable (e.g. `map` over
  `[9, 17, 35]` to generate a halogenation series — exactly the
  reproducible-parametrization use case behind the issue).

### D5. Passivation flag: preserve each site's exact current behavior

`Atom` flags bit 1 (`is_hydrogen_passivation` / `set_atom_hydrogen_passivation`)
is set asymmetrically today (see §2), and its only *consumed* meaning is
"host handled by surface reconstruction." The invariant to preserve is
therefore: **surf_recon-handled hosts stay flagged, and both readers keep
keying on the flag.** Terminator marking inherits whatever each site already
does:

- General path: flags its terminators — now halogen terminators too (same
  line of code, element-independent).
- Surface reconstruction: keeps flagging the dimer hosts; keeps *not*
  flagging its terminators.
- Lattice-fill: keeps flagging nothing.

No new flag bits, no new semantics, no behavior change. Flagging terminators
uniformly at all sites was considered and rejected for now: it is functionally
inert at both readers, but it would be an unforced change to `materialize`
outputs (the bit is visible to downstream atom_edit diffs and roundtrips
through their serialization) with no current consumer benefiting — same
conservative principle as the D2 bond-length resolution. See §8 for the
follow-up. (Optionally rename the accessors to `is_passivation` in a
mechanical follow-up; the flag bit itself must not move.)

### D6. Surface reconstruction: thread the element through

When `surf_recon` and passivation are both on, dimer construction currently
adds H at fixed constants. The question was whether halogen termination should
be supported there at all, or whether reconstruction is a hydrogen-only
phenomenon.

**Surface-science analysis.** The 2×1 dimer reconstruction is driven by the
bare substrate, not the adsorbate: an ideal (100) termination leaves two
dangling bonds per surface atom, and dimerization halves that count (the clean
surface reconstructs on its own, with buckled dimers). A monovalent terminator
merely caps the one remaining dangling bond per dimer atom, giving the
symmetric mono-X 2×1 phase — hydrogen is just the most common X. The genuinely
hydrogen-specific phase is the **unreconstructed dihydride 1×1** (two H per
surface atom): only H is small enough to attempt it, and even H is badly
strained there on diamond (see the relax non-convergence investigation).
Halogens have no di-halide analog at all, so for halogens the 2×1 mono-halide
is essentially the *only* full-coverage (100) termination — reconstruction is
*more* necessary with halogens, not less.

Experimentally known analogs on the substrates atomCAD's reconstruction
supports (diamond + silicon (100)): **Si(100)-(2×1):Cl and :Br** (textbook
halogen-etching systems; symmetric unbuckled dimers), **Ge(100)-(2×1):Cl/:Br**,
and **C(100)-(2×1):F** (fluorinated diamond, stable at full monolayer with some
F–F strain). Diamond + Cl is marginal; diamond + Br/I is sterically unrealistic
at full coverage — that responsibility stays with the user, consistent with how
the dihydride (100) strain case is treated today.

**Rejected alternatives:** (a) keeping dimer termination H-only while the rest
of the surface gets the halogen produces an unrequested mixed H/X surface;
(b) erroring on `surf_recon` + element≠H breaks down under regions — settings
resolve per *position*, so a region can set `passiv_elem = F` where
`surf_recon` inherits `true`, and rejecting that would need cross-field
validation over the region array while blocking chemically legitimate Si-Cl
structures.

**Decision:**

- The per-dimer passivation branch places the configured element: halogen
  terminator bonds use `halogen_bond_length(host, elem)` (molecular values —
  the measured surface bond can differ a few percent, see the Si–Cl note in
  D2); H terminators keep the existing surface-specific
  `SurfaceReconstructionParams.h_bond_length` constants unchanged (D2). The
  terminator **angle from the surface normal keeps the H-calibrated
  `h_angle_from_normal_degrees` constant (24°)** for every element — only the
  bond *length* is element-dependent; the angle is part of the same documented
  approximation as the dimer geometry below.
- Under regions, the element is resolved **at the same position where the
  existing per-dimer `hydrogen_passivation` bool is already resolved** — no
  new resolution policy for dimers straddling a region boundary; whatever
  position that code path uses today decides both fields.
- The **dimer geometry itself is unchanged** (target dimer bond lengths /
  vertical displacement stay the H-calibrated constants). Passivated dimers
  are symmetric regardless of terminator; the dimer length differs only by a
  few percent under a halogen (e.g. Si–Si ~2.37 Å under H vs ~2.40 Å under
  Cl), so this is a documented approximation refined by `relax` — the output
  is a deterministic, chemically sensible starting point.
- The reference guide notes the steric caveat for heavy halogens on diamond.

### D7. No `remove_halogen`

`remove_hydrogen` stays as-is (strip element 1). Halogen stripping is already
expressible and discoverable as `atom_replace` with target **Delete**
(`to = 0`), per element. The reference guide gets a sentence pointing this out.

### D8. Per-region passivant in `materialize.regions`

`MaterializeRegion` (built-in record def) gains an optional field
`passiv_elem: Optional[Int]` (unset = inherit). `RegionSpec` gains
`passiv_elem: Option<i16>`; `SettingsResolver::resolve_at` resolves it with the
same per-field painter's algorithm as the five booleans, and
`LatticeFillOptions` carries the resolved `passivation_element: i16`. This
enables e.g. "fluorinate only this face" — a strong use case for surface
functionalization. Since the resolver is already per-field, this is mechanical.

The new field carries the **`FieldEditorHint::Element` editor hint** so that
`record_construct(schema: MaterializeRegion)` renders an element dropdown
(`SelectElementWidget`) instead of a bare int box — same convenience the
`ElementMapping` built-in def already provides for `atom_replace` rules. The
hint is valid on `Optional[Int]` (it describes the inner type through the
`Optional` wrapper; see `FieldEditorHint::validate_for`). This requires
switching `MaterializeRegion`'s registration from `from_named_fields` to
`from_hinted_fields` (the other fields pass `None`).

Per the field-hint invariant (`doc/design_array_node_and_field_hints.md` Part
A), the hint is **cosmetic only**: the dropdown offers the full element list,
not the restricted passivant set, and a chemically invalid pick (e.g. O) flows
through as a plain Int and is judged solely by the D1 eval-time check, which
surfaces the localized error naming the allowed set. There is no Enum-style
element-subset hint, and inventing one would cross the "hints never gate
values" line that design draws.

Note: built-in record defs are not serialized, so adding a field needs **no**
`.cnnd` migration; existing region records simply don't set it (the
record_construct optional-field collapse handles unset).

### D9. atom_edit panel "Add H" action stays H-only

`atom_edit/hydrogen_passivation.rs` (the panel action writing into the diff)
keeps calling `add_hydrogens` with the default element. Generalizing that UI
(an element dropdown next to the Add H button) is a small, independent
follow-up; not blocking this feature since the node-level path covers the
workflow.

### D10. Text format: atomic number Int

Text properties use the atomic number as an `Int` (`element` on `passivate`,
`passiv_elem` on `materialize`), consistent with the pin type and with
`StyleRule.element`. (Symbol strings would read nicer but would be the only
symbol-typed element property in the text format; not worth the asymmetry.)

## 4. Phase 1 — crystolecule core

1. **`atomic_constants.rs`**: add `halogen_bond_length(host, halogen)`
   (table from D2 + covalent-radii-sum fallback) and
   `pub const ALLOWED_PASSIVANTS: [i16; 5] = [1, 9, 17, 35, 53];` with an
   `is_allowed_passivant(i16) -> bool` helper. Note the two domains are
   deliberately different: `ALLOWED_PASSIVANTS` is the D1 *validation* set
   and **includes H**; `halogen_bond_length` covers **halogens only** — call
   sites branch on `passivant == 1` first (D2) and never pass H to it.
2. **`hydrogen_passivation.rs`**:
   - `AddHydrogensOptions` gains `passivant_element: i16` (default `1` via the
     existing `Default` impl).
   - `add_hydrogens_filtered`: bond length = `lookup_xh_bond_length(host)`
     when the passivant is H (unchanged), `halogen_bond_length(host,
     options.passivant_element)` otherwise; `structure.add_atom(options.
     passivant_element, pos)`. `XH_BOND_LENGTHS` stays where it is (D2).
   - Rename `AddHydrogensResult.hydrogens_added` → `atoms_added` (the name
     would be a lie for F; few call sites, caller messages updated). The
     function/struct names (`add_hydrogens*`, `AddHydrogensOptions`) stay —
     renaming the whole module's API is churn without payoff, matching the
     accessor-rename deferral in D5/§8.
   - Host-skip rule: skip a host when `host == 1` **or** `host ==
     options.passivant_element`. Today's `atomic_number == 1` check is the
     `passivant == H` instance of exactly this rule (never cap an atom with
     its own element — a lone H must not become H₂), so for H passivation the
     behavior is **byte-identical to today**: a *lone* F atom still gets
     H-capped to HF, terminal halogens are saturated (max 1 bond) and skipped
     by the valence check regardless. What the rule adds for halogen
     passivants: a lone F is not capped with another F (no F₂ "passivation"),
     and the longstanding "H is a terminator, never a host" invariant holds
     for every passivant. A lone Cl under passivant F does get capped (ClF) —
     exotic but consistent with "cap every open valence", and only reachable
     with a bare unbonded halogen in the input.
3. **`lattice_fill/config.rs`**: `LatticeFillOptions` gains
   `passivation_element: i16`; `RegionSpec` gains `passiv_elem: Option<i16>`;
   `SettingsResolver::resolve_at` resolves it per-field (one more
   `have_passiv_elem` slot).
4. **`lattice_fill/hydrogen_passivation.rs`**: `hydrogen_passivate_dangling_bond`
   takes the resolved element from the per-position options; H keeps the
   existing `C_H_BOND_LENGTH` special case + radii-sum fallback unchanged,
   halogens use `halogen_bond_length` (D2); `add_atom(elem, pos)`.
5. **`lattice_fill/surface_reconstruction.rs`**: per-dimer passivation places
   the resolved element; H keeps the existing `h_bond_length` params
   constants, halogens use `halogen_bond_length` (D6); dimer geometry
   constants untouched.
6. **Tests** (`rust/tests/crystolecule/`):
   - `atomic_constants` (new module or alongside an existing one):
     `halogen_bond_length` unit tests — table hits (C–F 1.35, Si–Cl 2.02)
     plus an `—`-cell fallback (e.g. N–Br → covalent-radii sum).
   - `hydrogen_passivation_test.rs`: F/Cl passivation of a methane-like C
     fragment → bond length assertions (1.35 / 1.77 ± 1e-6); determinism
     (two runs → identical positions); host-skip rule: lone F + passivant F →
     untouched, lone F + passivant H → still H-capped (today's behavior),
     lone H → never passivated; halogen terminator has the passivation flag
     set (D5, general path flags its terminators).
   - **Per-site H pinning (the D2 guard).** Assert the exact per-site H bond
     length **on a silicon host** at each site: general path Si–H = 1.48
     (`XH_BOND_LENGTHS`), lattice-fill Si–H = 1.42 (radii sum), surf_recon
     Si–H = 1.50 (`SI_H_BOND_LENGTH`). Silicon is the element where the
     three sites *differ* — C coincides everywhere (1.09), so C-only tests
     would pass even if an implementer "helpfully" unified the H paths and
     silently violated D2. (This replaces any "unchanged vs today"
     comparison, which is unimplementable once the refactor lands — the
     per-site constants *are* the golden values.)
   - `lattice_fill_test.rs`: fill a small diamond sphere with
     `passivation_element = 9` → every terminator is F at C–F length; count
     matches the H run; terminators are **unflagged** (D5, lattice-fill
     flags nothing).
   - **Surf-recon halogen test** (`lattice_fill_test.rs`, next to the
     existing surf_recon coverage): silicon slab with `surf_recon = true`
     and `passivation_element = 17` → dimer terminators are Cl at
     `halogen_bond_length(Si, Cl)` = 2.02; dimer geometry (dimer bond
     length, vertical displacement) **identical to the H run** — pins the
     "dimer constants untouched" half of D6; dimer host atoms flagged,
     terminators unflagged (D5).
   - Region resolution: region overriding `passiv_elem` only (inherit the
     booleans) via `lattice_fill_regions_test.rs`.

## 5. Phase 2 — nodes + serialization

1. **Rename node** (D3): `nodes/add_hydrogen.rs` → `nodes/passivate.rs`
   (`NodeType.name = "passivate"`, updated description/summary); registry +
   `nodes/mod.rs`. `serialization/migrate_v7_to_v8.rs` rewrites
   `node_type_name: "add_hydrogen"` → `"passivate"` (model:
   `migrate_v6_to_v7.rs`); `SERIALIZATION_VERSION` 7→8.
2. **`passivate` node data**: `PassivateData { element: i16 }` with
   `#[serde(default = ...1)]`; appended optional `element: Int` pin;
   `evaluate_or_default` for precedence; eval-time D1 validation (error names
   the allowed set); `get_subtitle` returns the element symbol when ≠ H (e.g.
   "F") so the network reads correctly at a glance; text properties
   `element: Int`; `get_parameter_metadata` marks the pin optional.
3. **`materialize`**: `MaterializeData.passivation_element: i16`
   (`serde(default = 1)`, added to both the struct and
   `MaterializeDataDeserialized`); appended optional `passiv_elem: Int` pin
   (index 7) via `evaluate_or_default`; D1 validation; text property
   `passiv_elem`; regions parser reads the new record field
   (`parse_optional_int_field` sibling of `parse_optional_bool_field`, with the
   D1 check per item).
4. **`MaterializeRegion` built-in record def** (`node_type_registry.rs`): add
   `("passiv_elem", Optional[Int], Some(FieldEditorHint::Element))` and switch
   the registration to `RecordTypeDef::from_hinted_fields` (D8).
5. **Node snapshots**: `cargo insta review` for the renamed node type + new
   pins.
6. **Tests** (`rust/tests/structure_designer/`): materialize with stored F +
   wired `passiv_elem` pin override; passivate node F on a molecule, stored
   **and** via wired `element` pin override (both nodes' precedence paths);
   migration fixture `add_hydrogen` → `passivate` roundtrip (fixture under
   `tests/fixtures/`) — the fixture must include an `add_hydrogen` **nested
   inside a zone body** (migrations are raw-JSON passes; body recursion is
   the classic miss); text-format roundtrip of both new properties; invalid
   element (e.g. 8) → localized error, network stays evaluable — tested at
   **all three** D1 surfaces: `passivate` stored/pin, `materialize`
   stored/pin, and a `MaterializeRegion` record with `passiv_elem = 8` (the
   regions parser's per-item check is separate code from the pin check).

## 6. Phase 3 — API + Flutter + docs

1. **API**: new `get_passivate_data` / `set_passivate_data` in
   `api/structure_designer/` — **must take `scope_path`** like all node-data
   accessors; setter goes through the standard undo path (`SetNodeData`) and
   re-validates. `materialize`'s existing get/set data API gains the element
   field. Run `flutter_rust_bridge_codegen generate` (if a new Rust api module
   file is created it must be added to `flutter_rust_bridge.yaml` `rust_input`
   or codegen silently skips it).
2. **Flutter editors**:
   - New `lib/structure_designer/node_data/passivate_editor.dart`: a small
     fixed dropdown of the five allowed passivants (H / F / Cl / Br / I) — a
     restricted list is better UX than the full ~100-element
     `SelectElementWidget` where most entries would error. Standard
     wired-pin-disables-editor pattern for the `element` pin (opacity +
     IgnorePointer + italic annotation). Register in `node_data_widget.dart`
     (there is currently **no** editor for add_hydrogen — this is a new case).
   - `materialize_editor.dart`: same restricted dropdown next to the
     `passivate` checkbox; wired `passiv_elem` pin disables it (this one *is*
     replace-semantics, unlike the annotate-only `regions` pattern);
     the existing "Regions override these settings…" annotation already covers
     the per-region element.
3. **Reference guide** (`doc/reference_guide/nodes/atomic.md`): rename the
   add_hydrogen section to `passivate`, document the element choice, the pin,
   determinism rationale, the surf_recon interaction (D6 approximation), and
   the "remove halogens via atom_replace → Delete" note (D7); update the
   materialize section (new property/pin + region field).
4. Manual walkthrough (thin editor UI — no integration test mandated):
   passivate a diamond slab with F via checkbox, via wired pin, via region
   record; verify dropdown disable states.

## 7. Testing summary

- Core (Phase 1): `halogen_bond_length` table + fallback; halogen bond
  lengths, determinism, host-skip rule; **per-site H pinning on Si** (the D2
  guard — asserts 1.48 / 1.42 / 1.50 at the three sites); **surf_recon
  halogen** (Cl terminators + dimer geometry identical to H run, the D6
  guard); **flag invariants** (general-path terminators flagged, lattice-fill
  terminators unflagged, surf_recon hosts flagged — the D5 guard).
- Nodes (Phase 2): precedence (stored vs wired, both nodes), D1 rejection at
  all three surfaces (passivate, materialize, region record), migration incl.
  zone-body nesting, text roundtrip, snapshots.
- UI (Phase 3): manual walkthrough only (thin editor UI convention).

## 8. Out of scope / future

- **Multi-atom caps** (–OH, –NH₂, –CH₃, –CF₃): needs cap-orientation
  (dihedral) policy; a different feature.
- **Per-call element in the atom_edit "Add H" panel action** (D9) — small
  follow-up.
- **Mixed passivation within one node call** beyond regions (e.g.
  probabilistic F/H mixes) — expressible today by chaining region-gated calls.
- **Renaming the `is_hydrogen_passivation` accessors** to `is_passivation` —
  mechanical, optional.
- **Unifying terminator marking across all three sites** (flag every
  passivation terminator, including lattice-fill's and surf_recon's) — do
  this if a future feature needs a reliable "placed by passivation" marker
  (a strip-passivation op, styling passivation atoms). Functionally inert at
  both current readers (D5), so it can ship independently at any time.
- Dimer-geometry recalibration for halogen-terminated reconstructions (D6
  keeps H-calibrated dimer constants).

## 9. Decision log

1. **D3 naming** — DECIDED: rename `add_hydrogen` → `passivate` with the
   v7→v8 migration.
2. **D6** — DECIDED: thread element through surface reconstruction; analysis
   in D6 (halogen mono-X 2×1 phases are experimentally real on Si/Ge and for F
   on diamond; the unreconstructed dihydride is the hydrogen-only special case,
   not the reconstruction).
3. **D7** — DECIDED: `remove_hydrogen` gains no element filter;
   `atom_replace` → Delete covers halogen stripping.
4. **D5 flag behavior** — DECIDED (2026-07-20, flag-consumer audit): the flag
   is set asymmetrically today and only the "surf_recon-handled host" meaning
   is consumed; each site keeps its exact current flag behavior (general path
   extends terminator flagging to halogens; lattice-fill and surf_recon
   terminators stay unflagged). Uniform terminator marking deferred to §8.
5. **D2 hydrogen bond lengths** — DECIDED (2026-07-20, literature check):
   the per-site H values are context-specific (DFT Si–H ≈ 1.51 Å on
   Si(100)-2×1:H vs 1.48 Å in silane; surface Si–Cl 1.95 Å vs ~2.02 Å
   molecular), so H keeps each site's existing computation untouched and the
   shared primitive `halogen_bond_length` covers F/Cl/Br/I only.
6. **Host-skip rule** — DECIDED (2026-07-20): `host == 1 || host ==
   passivant`, the exact generalization of today's H-only check — H
   passivation stays byte-identical (a lone F still gets H-capped), and
   self-capping (F→F₂) is excluded for halogen passivants.
