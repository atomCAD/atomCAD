# Crystal / Molecule Split and Abstract Phase Types

## Context

This document specifies the implementation plan for the type-system split that separates the current `DataType::Atomic` into two concrete phase types (`Crystal`, `Molecule`) plus three abstract "two-out-of-three" supertypes (`Atomic`, `StructureBound`, `Unanchored`), together with a new output-pin mechanism (`PinOutputType::SameAsInput`) that lets polymorphic nodes preserve the concrete input type at the output.

It is step 6 of the broader lattice-space refactoring. The motivation, user-facing model, node catalog, and payload-struct shapes are defined in the parent design document:

- [`design_lattice_space_refactoring.md`](design_lattice_space_refactoring.md) — read sections "Type System", "Nodes", and Appendix A/B before implementing.

This document is self-contained for implementation: read it plus the relevant `AGENTS.md` files (project root, `rust/AGENTS.md`, `rust/src/structure_designer/AGENTS.md`) and the code locations cited below.

## Preconditions (already in tree)

Steps 1–5 of the parent refactoring have landed:

- `DataType::Geometry` renamed to `Blueprint`; `DataType::UnitCell` renamed to `LatticeVecs`.
- `DataType::Structure` and `NetworkResult::Structure` exist. `Structure` carries `lattice_vecs + motif + motif_offset`.
- `BlueprintData { structure: Structure, geo_tree_root: GeoNode }` is the Blueprint payload.
- Primitives (`cuboid`, `sphere`, `extrude`, ...) take a `Structure` input (defaulting to diamond) and output Blueprint.
- `AtomicStructure` has had `frame_transform` removed.
- `NetworkResult::Atomic(AtomicStructure)` is still a single concrete runtime variant.

Nothing in Flutter / FRB / `APIDataTypeBase` changes in this step — that is deferred to the final migration step.

## Target model (recap)

Three concrete runtime phase types:

| Concrete type | Has `Structure` | Has atoms | Has geometry |
|---|---|---|---|
| **Blueprint** | yes | no | yes (required) |
| **Crystal**   | yes | yes | optional |
| **Molecule**  | no  | yes | optional |

Three abstract supertypes, each excluding exactly one concrete phase:

| Abstract | Members | Excludes |
|---|---|---|
| **`Atomic`** | Crystal, Molecule | Blueprint |
| **`StructureBound`** | Blueprint, Crystal | Molecule |
| **`Unanchored`** | Blueprint, Molecule | Crystal |

Invariants:

- Abstract types exist only as pin *constraints*. No runtime `NetworkResult` value is ever of an abstract type.
- Each concrete phase implicitly converts (upcasts) to any abstract type containing it. There is no implicit downcast.
- A polymorphic node declared over an abstract input preserves the concrete input type on its output, so e.g. `Crystal → atom_edit → structure_move` remains a well-typed chain with Crystal at every boundary.

Type-preservation is achieved by introducing:

```rust
pub enum PinOutputType {
    Fixed(DataType),
    SameAsInput(String),   // mirrors this input pin's resolved concrete type
}
```

During wire validation the input pin's concrete type is resolved first; the output pin is then treated as that concrete type for all downstream validation. At runtime nothing special happens: the node receives a concrete `NetworkResult::Crystal(..)` or `::Molecule(..)`, operates on the inner `AtomicStructure`, and re-wraps in the same variant.

## Scope of this step

### In scope

- New `DataType` variants: `Crystal`, `Molecule`, `StructureBound`, `Unanchored`. Redefinition of the existing `Atomic` variant as abstract (retain the name; existing `.cnnd` strings stay valid).
- New `NetworkResult` variants: `Crystal(CrystalData)`, `Molecule(MoleculeData)`. Removal of `NetworkResult::Atomic(..)` construction; `DataType::Atomic` is now abstract-only.
- New payload structs `CrystalData`, `MoleculeData` (shapes below, matching parent doc Appendix B).
- New `PinOutputType` enum; `OutputPinDefinition.data_type` changes to `PinOutputType`.
- Extension of the wire-validation / conversion rules to handle abstract upcasting and `SameAsInput` resolution.
- Migration of the 19 atom-operation node definitions to use `DataType::Atomic` input + `PinOutputType::SameAsInput` output, re-wrapping the concrete variant at evaluation time.
- Update of `atom_fill` to output `Crystal`, wrapping its carved atoms together with the `Structure` sourced from the Blueprint input.
- Test / snapshot updates.

### Out of scope (later steps)

- Phase-transition nodes (`materialize`, `dematerialize`, `exit_structure`, `enter_structure`).
- Movement nodes (`structure_move`/`structure_rot`/`free_move`/`free_rot`) and deletion of `atom_lmove`/`atom_lrot`/`atom_move`/`atom_rot`/`atom_trans`.
- `.cnnd` migration script, new `APIDataTypeBase` variants, FRB regeneration, Dart UI updates.
- `get_structure`, `set_structure`, preset structure nodes.

## Current code — surgical targets

All paths relative to repo root.

### Core enums and dispatch

| Concern | File | Notable lines |
|---|---|---|
| `DataType` enum + `Display` + `from_string` / `parse_builtin_type` + `can_be_converted_to` | `rust/src/structure_designer/data_type.rs` | enum `11–30`; Display `32–74`; parse `176–426` (builtin `246–262`); conversion `90–153` |
| `NetworkResult` + payloads + `infer_data_type` + `extract_*` + `convert_to` + Display | `rust/src/structure_designer/evaluator/network_result.rs` | `BlueprintData` `94–145`; enum `156–177`; `infer_data_type` `182–201`; extractors `217–369`; `convert_to` `377–445`; display `540–650` |
| `OutputPinDefinition`, `NodeType` | `rust/src/structure_designer/node_type.rs` | `16–30`, `34–89` |
| Wire validation, topological walk, output-type propagation for custom networks | `rust/src/structure_designer/network_validator.rs` | conversion call `~164`; `update_network_output_type` `488–524` |
| `is_valid_connection` / `auto_connect_wire` | `rust/src/structure_designer/node_network.rs` (`~570`), `rust/src/structure_designer/structure_designer.rs` (`~1920+`) | |
| Evaluator dispatch on `Atomic` variant | `rust/src/structure_designer/evaluator/network_evaluator.rs` | `~461`, `~577` |
| `AtomicStructure` payload (unchanged structurally) | `rust/src/crystolecule/atomic_structure/mod.rs` | `79–97` |

### Nodes to migrate (19)

All under `rust/src/structure_designer/nodes/`. Each currently declares `DataType::Atomic` as input *and* output (except where noted). Input pin name varies per file — use the actual pin name (`atoms`, `diff`, `input`, ...) when constructing `PinOutputType::SameAsInput(<name>)`.

Atom operations that stay polymorphic (Atomic in, SameAsInput out):

1. `add_hydrogen.rs`
2. `apply_diff.rs` (two Atomic inputs: `atoms` + `diff`; output mirrors `atoms`)
3. `atom_cut.rs`
4. `atom_move.rs`
5. `atom_replace.rs`
6. `atom_rot.rs`
7. `atom_trans.rs`
8. `edit_atom/edit_atom.rs` (input at `~636`, output at `~639`)
9. `infer_bonds.rs`
10. `lattice_move.rs`
11. `lattice_rot.rs`
12. `relax.rs`
13. `remove_hydrogen.rs`

Array-input atom operations (use `SameAsArrayElements` per OQ1):

14. `atom_union.rs` (input `Array[Atomic]`, output `SameAsArrayElements("input")`)
15. `atom_composediff.rs` (input `Array[Atomic]`, output `SameAsArrayElements("input")`)

Atom-valued sinks / sources:

16. `export_xyz.rs` — input `Atomic`, no atom-valued output.
17. `sequence.rs` — `element_type: DataType::Atomic` at `~55–60`. Per OQ1, becomes type-preserving: its output's resolved element type is the unified concrete kind of its element inputs (all Crystal → `Array[Crystal]`, all Molecule → `Array[Molecule]`, mixed → validation error).
18. `import_xyz.rs` — currently outputs `Atomic`; update to `Fixed(DataType::Molecule)` (XYZ carries no structure).
19. `import_cif.rs` — currently outputs `Atomic`; see Open Question 2.

Structural mutation:

- `atom_fill.rs` — output at `~430`, construction at `~286`. Change output to `Fixed(DataType::Crystal)`; wrap result in `CrystalData` carrying the `Structure` extracted from the Blueprint input's `BlueprintData.structure`.

## Implementation sub-steps

Each sub-step is a natural stopping point: the code should compile and the existing test suite (minus snapshots regenerated at the end) should pass.

### 6.1 — DataType variants and conversion rules

File: `rust/src/structure_designer/data_type.rs`.

- Add `Crystal`, `Molecule`, `StructureBound`, `Unanchored` to the `DataType` enum. Keep `Atomic` in place; its *meaning* changes from "concrete atomic structure" to "abstract supertype of Crystal and Molecule".
- Update `Display`, `from_string`, and `parse_builtin_type` for the four new names. `Atomic`'s string remains `"Atomic"` for backward compatibility with existing `.cnnd` files.
- Extend `can_be_converted_to(src, dst)` with:
  - `Crystal → Atomic`, `Crystal → StructureBound`, `Crystal → Crystal`
  - `Molecule → Atomic`, `Molecule → Unanchored`, `Molecule → Molecule`
  - `Blueprint → StructureBound`, `Blueprint → Unanchored`, `Blueprint → Blueprint` (existing identity)
  - `Atomic → Atomic`, `StructureBound → StructureBound`, `Unanchored → Unanchored`
  - No abstract → concrete conversion. No cross-abstract conversion (e.g., `StructureBound → Unanchored`).
- Add a helper `DataType::is_abstract(&self) -> bool` returning true for `Atomic`, `StructureBound`, `Unanchored`. This is used as a debug-assertion hook in 6.2 and a validation guard in 6.4.

### 6.2 — Payload structs and NetworkResult variants

File: `rust/src/structure_designer/evaluator/network_result.rs`.

Add payload structs (parent doc Appendix B):

```rust
pub struct CrystalData {
    pub structure: Structure,
    pub atoms: AtomicStructure,
    pub geo_tree_root: Option<GeoNode>,
}

pub struct MoleculeData {
    pub atoms: AtomicStructure,
    pub geo_tree_root: Option<GeoNode>,
}
```

- Add variants `NetworkResult::Crystal(CrystalData)` and `NetworkResult::Molecule(MoleculeData)`.
- Remove `NetworkResult::Atomic(..)`. Producers are updated in 6.6/6.7; consumers in 6.5.
- `infer_data_type`: `Blueprint → Blueprint`, `Crystal → Crystal`, `Molecule → Molecule`. Debug-assert the returned concrete type is not `is_abstract()`.
- Replace `extract_atomic() -> Option<&AtomicStructure>` with one that accepts both Crystal and Molecule (returning a borrow of `CrystalData.atoms` or `MoleculeData.atoms`). Add `extract_crystal` and `extract_molecule` for call sites that need the full concrete payload.
- Update `convert_to` for any runtime coercions that previously touched `Atomic`.
- Update Display / `to_detailed_string` for the new variants (two insta snapshots regenerate in 6.8).

### 6.3 — PinOutputType

File: `rust/src/structure_designer/node_type.rs`.

```rust
pub enum PinOutputType {
    Fixed(DataType),
    SameAsInput(String),            // mirror a single input pin's resolved concrete type
    SameAsArrayElements(String),    // mirror the element type of an Array[..] input pin (OQ1)
}
```

- Change `OutputPinDefinition.data_type: DataType` to `data_type: PinOutputType`.
- Provide constructors: `OutputPinDefinition::fixed(name, DataType)`, `OutputPinDefinition::same_as_input(name, input_pin_name)` plus convenience `::single_fixed` / `::single_same_as(..)` mirroring the existing `single` helper.
- Add a resolution helper:
  ```rust
  fn resolve_output_type(
      &self,
      node: &Node,
      network: &NodeNetwork,
      registry: &NodeTypeRegistry,
  ) -> DataType;
  ```
  which, for `Fixed(t)`, returns `t`; for `SameAsInput(name)`, looks up the upstream wire feeding `name`, asks the registry for the source node's output-pin concrete type (recursively resolving), and returns it. If no wire is connected, returns the declared abstract type of that input pin (so downstream validation sees the abstract type and rejects anything that requires concreteness — consistent with "abstract cannot downcast").
- Update all existing readers of `OutputPinDefinition.data_type` to call `resolve_output_type` when they need the resolved concrete type, or to pattern-match on `PinOutputType` where that is more natural. Affected sites (known so far): `node_type_registry.rs`, `network_validator.rs`, `network_evaluator.rs` (display metadata path), scene conversion in `rust/src/api/structure_designer/...`, and any Flutter-facing type reporting. The FFI surface still reports a single `DataType` per pin — convert via `resolve_output_type` before crossing the API boundary.

### 6.4 — Wire validation and output-type propagation

Files: `rust/src/structure_designer/network_validator.rs`, `rust/src/structure_designer/node_network.rs`, `rust/src/structure_designer/structure_designer.rs`.

- During `validate_node_network`, walk nodes in topological order. For each node, after its inputs are resolved, compute and cache each output pin's *resolved concrete* `DataType`. Downstream wire checks read this cache instead of recomputing. (A plain `HashMap<(NodeId, PinIndex), DataType>` on a local `ValidationContext` is sufficient; do not persist it on the network.)
- `is_valid_connection` and `auto_connect_wire`:
  - Source pin's resolved concrete type is read from the cache if available, otherwise recomputed via `resolve_output_type`.
  - Destination pin's declared type may be abstract. Use `DataType::can_be_converted_to(source, dest)`.
- `update_network_output_type()` (currently at `network_validator.rs:488–524`) does not need structural changes: it clones the return node's `output_pins` onto the network's `NodeType`. If a return node's output is `SameAsInput`, the enclosing custom network will *also* expose `SameAsInput` on its synthesised output pin, which naturally re-resolves when the custom network itself is wired into a larger graph.
- Add a validation-time guard: reject any `NetworkResult` produced by evaluation whose `infer_data_type()` is abstract (debug-assert; should be impossible).

### 6.5 — Evaluator dispatch

File: `rust/src/structure_designer/evaluator/network_evaluator.rs`.

- At every former `if let NetworkResult::Atomic(s) = ..` site (`~461`, `~577`), match on `Crystal(..) | Molecule(..)` and extract the inner `AtomicStructure`. Carry the originating variant forward when re-wrapping.
- For array flattening in polymorphic Array-input nodes, see Open Question 1.

### 6.6 — `atom_fill` migration

File: `rust/src/structure_designer/nodes/atom_fill.rs`.

- Output pin changes from `DataType::Atomic` to `PinOutputType::Fixed(DataType::Crystal)`.
- In `evaluate`, read the Blueprint input's `BlueprintData`, clone its `structure`, run the existing carving algorithm unchanged, and return:
  ```rust
  NetworkResult::Crystal(CrystalData {
      structure,
      atoms: result.atomic_structure,
      geo_tree_root: Some(blueprint.geo_tree_root.clone()),
  })
  ```
- Keep all existing inputs and parameters (`shape`, `motif`, `m_offset`, `passivate`, `rm_single`, `surf_recon`, `invert_phase`). The node-level motif/offset knobs remain until phase 7 unifies them into `Structure`.

### 6.7 — Migrate polymorphic atom-op nodes

For each of nodes 1–13 above:

- Input pin: unchanged (still `DataType::Atomic`, now abstract).
- Output pin: `PinOutputType::SameAsInput("<input_pin_name>")` where `<input_pin_name>` is the pin the output should mirror (`atoms` for most; `atoms` not `diff` for `apply_diff`).
- `evaluate`: match on both concrete variants, operate on the inner `AtomicStructure`, re-wrap in the same variant. Introduce a shared helper to cut 19× boilerplate:

```rust
// rust/src/structure_designer/evaluator/atom_op.rs (new)
pub fn map_atomic(input: NetworkResult, f: impl FnOnce(AtomicStructure) -> AtomicStructure)
    -> NetworkResult
{
    match input {
        NetworkResult::Crystal(mut c) => { c.atoms = f(c.atoms); NetworkResult::Crystal(c) }
        NetworkResult::Molecule(mut m) => { m.atoms = f(m.atoms); NetworkResult::Molecule(m) }
        other => NetworkResult::Error(format!(
            "atom op received non-atomic input: {:?}", other.infer_data_type())),
    }
}
```

Import and XYZ/CIF import:

- `import_xyz.rs`: output `PinOutputType::Fixed(DataType::Molecule)`; construct `NetworkResult::Molecule(MoleculeData { atoms, geo_tree_root: None })`.
- `import_cif.rs`: output `PinOutputType::Fixed(DataType::Molecule)` (OQ2). Add a TODO comment at the pin declaration noting that CIF's crystal lattice/motif info is discarded and this should emit `Crystal` with an extracted `Structure` once phase-transition nodes land.
- `export_xyz.rs`: input stays `Atomic`; no output-pin change needed.

Array-input atom operations (`atom_union`, `atom_composediff`): see Open Question 1.

### 6.8 — Tests and snapshots

- Update all tests that construct or match `NetworkResult::Atomic(..)` to use `Crystal`/`Molecule` with the matching payload. The survey counted ~52 reference sites; the majority are node evaluation tests under `rust/tests/structure_designer/` and `rust/tests/crystolecule/`.
- Regenerate affected insta snapshots via `cargo insta review`. Expected: display-string snapshots that mentioned `Atomic` now say `Crystal` or `Molecule`.
- Add new unit tests:
  1. Conversion matrix — every pair in the 7×7 `DataType` grid over the phase types; assert `can_be_converted_to` exactly matches the table in this document.
  2. `PinOutputType::SameAsInput` resolution — `atom_fill` (Crystal) → `add_hydrogen` → `infer_bonds`: assert the final output pin's resolved concrete type is `Crystal` and the runtime variant is `Crystal`.
  3. `import_xyz` (Molecule) → `add_hydrogen` → `export_xyz`: assert Molecule is preserved end-to-end.
  4. Validation rejects an abstract type appearing as a *runtime* result (debug assertion).
  5. Any array-input meet rule fixed in Open Question 1.

### 6.9 — Flutter / FRB

No changes. `APIDataTypeBase` still has a single `Atomic` variant; the `resolve_output_type` helper collapses `Crystal`/`Molecule` to `Atomic` for the Flutter boundary until the final migration step updates the API.

## Payload shape reference

```rust
// Blueprint (already exists, unchanged in this step)
pub struct BlueprintData {
    pub structure: Structure,
    pub geo_tree_root: GeoNode,
}

// New in this step
pub struct CrystalData {
    pub structure: Structure,
    pub atoms: AtomicStructure,
    pub geo_tree_root: Option<GeoNode>,
}

pub struct MoleculeData {
    pub atoms: AtomicStructure,
    pub geo_tree_root: Option<GeoNode>,
}

pub enum NetworkResult {
    // ... existing variants unchanged except Atomic is removed
    Blueprint(BlueprintData),
    Crystal(CrystalData),
    Molecule(MoleculeData),
    // ...
}
```

## Invariants after this step

1. No `NetworkResult` value ever carries an abstract `DataType`. Every runtime atomic value is exactly `Crystal` or `Molecule`. This extends recursively: no resolved pin type is abstract at any nesting depth inside `Array[..]` either.
2. Every output pin has either a `Fixed` concrete type, a `SameAsInput`, or a `SameAsArrayElements` that, when resolved against a valid graph, yields a concrete type. If resolution fails (unconnected input, mixed-phase array), the node is flagged invalid and downstream wires are treated as disconnected.
3. `can_be_converted_to` never permits abstract → concrete. It permits concrete → abstract only along the membership edges listed above.
4. `atom_fill` is the only producer of `Crystal` after this step. `import_xyz` and `import_cif` are the producers of `Molecule` (the latter with a TODO — see OQ2).
5. The Flutter API and `.cnnd` file format are unchanged.

## Open questions (to be resolved before starting)

### OQ1 — Array[Atomic] handling for `atom_union` and `atom_composediff` — **RESOLVED**

**Decision: mixed Crystal+Molecule arrays are a validation error. Same-kind arrays propagate their kind.**

This supersedes the parent design doc's "mixed → Molecule" meet rule: that rule is a silent type coercion that discards structure information. The stricter rule forces the user to be explicit about phase transitions and keeps the type system honest.

Mechanism:

- Add `PinOutputType::SameAsArrayElements(String)` — the string names an `Array[Atomic]` input pin. Resolution at validation time:
  1. Resolve the concrete *element* type of the array flowing into the named pin (see below).
  2. `Array[Crystal]` element type → output is `Crystal`.
  3. `Array[Molecule]` element type → output is `Molecule`.
  4. Element type still abstract after resolution (no upstream, mixed, etc.) → node is flagged invalid; downstream wires are treated as disconnected.
- `atom_union` and `atom_composediff` use `SameAsArrayElements("input")` (or the actual pin name).
- The runtime `evaluate` also debug-asserts that every array element carries the same concrete variant; this should be unreachable in a valid graph.

For this to work, every pin whose type is `Array[Atomic]` must also resolve its element type concretely — otherwise the validator has no concrete info to feed rules 2/3. This is a strengthening of invariant 1: **no resolved pin type is abstract, at any nesting depth inside `Array[..]` either.** The implication:

- **`sequence` node must become type-preserving.** Currently declares `element_type: DataType::Atomic` (abstract). After this step, its output pin's resolved element type is the unified concrete kind of its element inputs — all Crystal → `Array[Crystal]`, all Molecule → `Array[Molecule]`, mixed → validation error (same rule as `atom_union`). Any future array-literal nodes over atomic values follow the same rule.
- **Custom networks** whose return pin is `Array[Atomic]` inherit this automatically, because `update_network_output_type` already clones the return node's resolved output pins.

Practical consequence: if a user wires a Crystal and a Molecule into `atom_union` (or any array pin over atoms), they see a validation error on the offending node and must insert an explicit phase-transition step — silent "downgrade to Molecule" never happens.

### OQ2 — `import_cif` output type — **RESOLVED**

**Decision: `import_cif` outputs `Fixed(DataType::Molecule)` for this step, with a TODO to revisit.**

Rationale:

- Today `AtomicStructure` carries no structure field, so `import_cif` already discards the CIF's lattice/motif information at the type level. Relabeling the current output as `Molecule` changes the label but not the information content — no regression.
- Keeps this step focused on the type split. The proper redesign (CIF naturally wants to emit `Crystal` with the extracted `Structure`, or perhaps a `Structure`-only output when just the pattern is needed) interacts with phase-transition nodes that don't exist yet.
- Avoids keeping a transitional `NetworkResult::Atomic` variant alive purely to postpone one node's migration.

The parent design doc's sketch (`import_cif → Blueprint` with a Structure side-pin) is not obviously correct: a Blueprint requires a bounded geometry and a CIF doesn't naturally carry a cookie-cutter shape. That decision belongs to a later step.

Implementation note: add a TODO comment at the output-pin declaration in `import_cif.rs` reading approximately: *"Emitting Molecule discards the CIF's crystal lattice/motif information. Revisit once phase-transition nodes land; likely should emit `Crystal` with the extracted `Structure`."*

### OQ3 — Runtime guard on abstract types — **RESOLVED**

**Decision: return `NetworkResult::Error(..)` in release builds too, not just a debug assertion.**

If evaluation ever produces a value whose `infer_data_type()` is abstract, that is a bug in a polymorphic node's `evaluate` (it failed to re-wrap its result in the correct concrete variant). The validator's post-evaluation check surfaces this as a node-level error rather than silently corrupting downstream state. Consistent with how the evaluator already reports other kinds of ill-typed results.

## Estimated footprint

- 7 core files edited: `data_type.rs`, `network_result.rs`, `node_type.rs`, `node_type_registry.rs`, `network_validator.rs`, `node_network.rs`, `network_evaluator.rs`.
- 19 node files edited; 1 new helper module (`atom_op.rs`).
- ~15–25 test files touched; 2–3 insta snapshots regenerated.
- Zero Flutter / FRB changes.
