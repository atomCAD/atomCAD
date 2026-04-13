# Structure Space Refactoring

## Motivation

The current type system conflates two orthogonal concerns into a single `Geometry` vs `Atomic` divide:

1. whether atoms are materialized (have concrete positions)
2. whether the object is in structure space or in free space

Symptoms of this conflation:

- `atom_fill` performs two independent actions at once — carving atoms *and* exiting structure space — leaving no intermediate state where atoms exist while still anchored to a structure.
- Duplicate nodes `atom_lmove` / `atom_lrot` exist because `structure_move` / `structure_rot` are locked to `Geometry`.
- `atom_move` / `atom_rot` are locked to `Atomic` when they should be locked to "free" objects.
- `atom_cut`, `atom_union` exist as hacks because boolean ops only work on `Geometry`.
- Structure info is discarded after `atom_fill`, so structure-aligned operations on edited structures are impossible without workarounds.

This refactoring replaces the two-type system with a **three-phase model** plus a minimal type-system extension that gives each phase distinction its own static guarantee.

## Model: Three Phases

Think of it as architecture → construction → deployment. An object in atomCAD is made of up to three independent ingredients:

- **Structure** — an infinite repeating pattern (lattice vectors + motif + motif offset) defining where atoms *would* exist everywhere in space.
- **Geometry** — a bounded shape.
- **Atoms** — concrete atom positions, possibly hand-edited.

These compose into three phases:

| Phase | Ingredients | Role |
|---|---|---|
| **Blueprint** | Structure + Geometry | *Design.* Geometry is a "cookie cutter" positioned in an infinite crystal field. Moving the cutter changes which atoms *would* be carved. This is where geometric design happens: boolean ops, fitting structures, adjusting surface cuts. |
| **Crystal** | Structure + Geometry + Atoms | *Construction.* `materialize` has carved atoms out of the structure. Atoms and geometry now form a rigid body — they move together under any transform. Structure info is retained so structure-constrained operations remain available. |
| **Molecule** | Atoms (+ optional geometry shell) | *Deployment.* `exit_structure` has dropped the structure association. The object is free-floating and can be moved arbitrarily. |

The forward transitions `materialize` and `exit_structure` are explicit; `dematerialize` and `enter_structure` provide the inverses.

## Type System

### Concrete Object Types

| Type | Geometry | Structure | Atoms | Atoms–Geometry coupling |
|---|---|---|---|---|
| **Blueprint** | yes | yes | no | n/a (atoms are latent) |
| **Crystal** | optional | yes | yes | rigidly coupled |
| **Molecule** | optional | no | yes | rigidly coupled |

### The `Structure` Value Type

`Structure` is a first-class value type carrying `lattice_vecs` + `motif` + `motif_offset`. It flows through the network like any other value. Promoting it to a type (rather than scattering its fields as parameters) lets a single structure be defined once and reused across many Blueprints, extracted from existing objects with `get_structure`, or supplied as presets (`diamond`, `lonsdaleite`, ...).

### Abstract Types

Many operations are naturally polymorphic over two of the three phases:

- atom operations (`atom_edit`, `apply_diff`, `relax`, ...) work on anything with atoms — Crystal *or* Molecule;
- structure movement (`structure_move`, `structure_rot`) works on anything with a structure — Blueprint *or* Crystal;
- free movement (`free_move`, `free_rot`) works on anything where free movement is legal — Blueprint *or* Molecule.

This yields three symmetric "two-out-of-three" abstract types, each excluding exactly one phase:

| Abstract type | Members | Excludes | Property |
|---|---|---|---|
| **`Atomic`** | Crystal, Molecule | Blueprint | has materialized atoms |
| **`StructureBound`** | Blueprint, Crystal | Molecule | has a structure |
| **`Unanchored`** | Blueprint, Molecule | Crystal | atoms are not locked to a structure; free movement is legal |

```
              Blueprint
             ╱         ╲
            ╱           ╲
    StructureBound    Unanchored
         ╱                 ╲
        ╱                   ╲
    Crystal ──── Atomic ──── Molecule
```

Abstract types exist only as **pin constraints**. No runtime value is ever of an abstract type: every runtime value is concretely Blueprint, Crystal, or Molecule. Conversion is uniform — each concrete phase implicitly converts to any abstract type containing it, and there are no implicit downcasts from abstract to concrete.

Polymorphic operations declared over an abstract type **preserve the concrete input type at the output**: a Crystal fed into `atom_edit` comes out as a Crystal; a Molecule comes out as a Molecule. This is essential — without it, the chain `Crystal → atom_edit → structure_move` would collapse to an abstract type partway through and the `structure_move` (which needs Crystal) could no longer connect. See **Appendix A** for the implementation mechanism.

## Nodes

### Structure Construction and Manipulation

- **`structure`** — Unified constructor / modifier node (copy-with pattern). All four inputs are optional:
  - `structure` (Structure) — base structure to modify. If unconnected, constructs from scratch.
  - `lattice_vecs` (LatticeVecs) — if connected, overrides the lattice vectors. Default: diamond lattice vectors.
  - `motif` (Motif) — if connected, overrides the motif. Default: diamond motif.
  - `motif_offset` (Vec3) — if connected, overrides the motif offset. Default: zero vector.

  When `structure` is connected, unconnected fields pass through from the base. When `structure` is unconnected, unconnected fields use the defaults above. Output: `Structure`.
- **Preset structures** — `diamond`, `lonsdaleite`, `silicon`, ... No inputs, output `Structure`.
- **`get_structure`** — `StructureBound → Structure`. Extracts the structure from a Blueprint or Crystal.

### Primitives

`cuboid`, `sphere`, `extrude`, ... — output **Blueprint**. Each primitive has an optional `Structure` input; if unconnected, a default (diamond) is used.

### Blueprint Modifiers

- **`set_structure`** — `(Blueprint, Structure) → Blueprint`. Replaces the Blueprint's structure info. **Geometry is preserved unchanged.** This is how geometry designed in one structure is transitioned into another — for aligning crystals from different structures using a shared shape, or seeing what a diamond-designed shape looks like filled with lonsdaleite.

### Boolean Ops

`union`, `intersect`, `diff` — `(Blueprint × Blueprint) → Blueprint`. Both inputs must share compatible `lattice_vecs`; otherwise an error is raised. The user reconciles incompatible structures with `set_structure`.

### Movement

All four movement nodes are polymorphic over an abstract input type; each preserves the concrete type at its output.

| Node | Input | Blueprint | Crystal | Molecule |
|---|---|---|---|---|
| **`structure_move`**, **`structure_rot`** | `StructureBound` | moves geometry only (repositions the cutter) | moves atoms + geometry together | BLOCKED (type error) |
| **`free_move`**, **`free_rot`** | `Unanchored` | moves geometry only, **amber warning** (cutter is off-structure) | BLOCKED (type error) | moves everything |

Key rules:

- **Blueprint** — geometry moves; the latent atoms stay anchored to the structure. Free moves are legal but flagged.
- **Crystal** — everything moves together, structure-constrained only. For free movement, the user must `exit_structure` first.
- **Molecule** — everything moves together, freely.

### Phase Transitions

```
          materialize              exit_structure
Blueprint ──────────→ Crystal ──────────────→ Molecule
          ←──────────          ←──────────────
         dematerialize         enter_structure
```

- **`materialize`** — `Blueprint → Crystal`. Carves atoms according to geometry ∩ structure. Parameterless.
- **`dematerialize`** — `Crystal → Blueprint`. Discards atoms, returns to the blueprint. **Destructive: atom edits are lost.**
- **`exit_structure`** — `Crystal → Molecule`. Drops structure info; geometry is kept as a shell.
- **`enter_structure`** — `(Molecule, Structure) → Crystal`. Re-associates a free object with a structure. Used for repackaging.

### Atom Operations

`atom_edit`, `apply_diff`, `atom_composediff`, `atom_replace`, `add_hydrogen`, `remove_hydrogen`, `relax`, `infer_bonds`, `passivate` — all accept `Atomic` and preserve the concrete input type (Crystal stays Crystal, Molecule stays Molecule).

- **`atom_union`** — `Array[Atomic] → Atomic`. Merges atoms from multiple inputs; raises an error if nuclei are implausibly close. The output concrete type is the "meet" of the input types: all Crystal (with compatible structures) → Crystal; all Molecule → Molecule; mixed Crystal + Molecule → Molecule. Geometry, if present on inputs, is unioned; if only some inputs carry geometry the result keeps the union of those that do.

### Import / Export

- **`import_cif`** → `Blueprint`. Via multi-output pins, also exposes the extracted `Structure` for downstream reuse.
- **`import_xyz`** → `Molecule`. XYZ carries no structure info.
- **`export_xyz`**, **`export_mol`** — input `Atomic` (works on Crystal or Molecule).

## Example Pipelines

**Simple crystal, released for free transport:**
```
diamond ──┐
cuboid ───┴─→ materialize → exit_structure → free_move
```

**Edit a defect, then shift the whole crystal in-structure:**
```
diamond ──┐
cuboid ───┴─→ materialize → atom_edit → structure_move(1,0,0) → exit_structure
```
`structure_move` moves atoms + geometry together; the defect rides along.

**Share one structure across multiple primitives (guaranteeing compatibility for boolean ops):**
```
diamond ──┬──→ cuboid ──┐
          └──→ sphere ──┴─→ union → materialize
```

**Reuse a shape in a different structure via `set_structure`:**
```
diamond ─────→ cuboid → structure_move(align) ──┐
lonsdaleite ───────────────────────────────────┴─ set_structure → materialize
```
The exact same geometry carves lonsdaleite instead of diamond.

**Match an imported crystal's structure via `get_structure`:**
```
import_cif("quartz.cif") ──→ get_structure ──┐
cuboid ──────────────────────────────────────┴─→ set_structure → materialize
```

**Repackage a free molecule into a new structure:**
```
diamond ──────────────────────────────────┐
... → exit_structure → free_move ─────────┴─→ enter_structure → materialize
```

## What This Replaces

| Old | New |
|---|---|
| `atom_lmove`, `atom_lrot` | `structure_move`, `structure_rot` on `StructureBound` |
| `atom_move`, `atom_rot` | `free_move`, `free_rot` on `Unanchored` |
| `atom_cut` | `diff` on `Blueprint` followed by `materialize` |
| `atom_union` | `atom_union` on `Atomic` (kept, now polymorphic over Crystal/Molecule) + `union` on `Blueprint` for geometric booleans |
| `atom_fill` | `materialize` (pure carving) + `Structure` data type + structure constructors / modifiers / `set_structure` |

## Open Questions

- **Mismatch levels within structure space** — three dissonance levels exist (fully space-group aligned, structure aligned but space-group broken, fully structure-broken); how should they be visually communicated?
- **Passivation and surface reconstruction settings** — parameters on `materialize` or separate Blueprint modifier nodes?
- **Migration path** ?

---

## Appendix A: Implementation of Type-Preserving Polymorphic Pins

This appendix describes how the abstract-type polymorphism is implemented in the Rust backend. It is aimed at contributors working on the structure designer type system and evaluator. Nothing here affects the user-facing behavior described in the main document.

### Type-Preserving Output Pins

An abstract input type alone would lose concrete type information downstream: after `Crystal → atom_edit → ???`, the chain's type would collapse to `Atomic`, and a subsequent `structure_move` (which needs Crystal) could no longer connect.

To preserve concreteness, output pins are extended to allow mirroring an input pin:

```rust
pub enum PinOutputType {
    Fixed(DataType),
    SameAsInput(String),  // mirrors this input pin's resolved concrete type
}
```

Polymorphic operations declare both input and output using the abstract type and `SameAsInput("input")`:

```
atom_edit:      Atomic         →  SameAsInput("input")
structure_move: StructureBound →  SameAsInput("input")
free_move:      Unanchored     →  SameAsInput("input")
```

During wire validation, the concrete type flowing into the input pin is resolved first; the output pin is then treated as that concrete type for all downstream validation. Thus `Crystal → atom_edit → structure_move` validates cleanly as `Crystal → Crystal → Crystal`.

At runtime nothing special happens: the node receives a concrete `NetworkResult::Crystal(..)` or `NetworkResult::Molecule(..)`, mutates the inner data, and returns the same variant. The wrapper passes through automatically.

### Why This Implementation

The full extension is two enum additions — one `DataType` variant per abstract type, plus `PinOutputType::SameAsInput` — and a handful of conversion rules in `can_be_converted_to`. Full static safety is preserved, no runtime downcasts are needed, and the evaluator is untouched. The existing `update_network_output_type()` machinery already propagates output types from inputs for custom networks, so the "computed output type" pattern has precedent in the codebase.

### Alternatives Considered and Rejected

- **Full parametric polymorphism** (`T where T: HasAtoms` with type variables and constraint solving) — more general but significantly more infrastructure; overkill for three abstract types.
- **Row polymorphism** (structural types like `{has_atoms, has_structure, has_geometry}`) — theoretically most elegant, but a paradigm shift from the current flat-enum `DataType`; too invasive.
- **Merge Crystal and Molecule into a single runtime-tagged type** — simplest typing, but sacrifices static safety of `structure_move` vs `free_move`, defeating the three-phase model.
- **Plain abstract type with runtime downcast** (`Atomic → Crystal` allowed with runtime check) — breaks the "errors caught at wire time" guarantee, defers problems to evaluation.

### Migration Notes

- The existing `DataType::Atomic` variant can be redefined from "the concrete atomic-structure type" to "the abstract supertype of Crystal and Molecule" rather than renamed, easing migration.
- Two new `DataType` variants are introduced: `Crystal` and `Molecule` (concrete), `StructureBound` and `Unanchored` (abstract). The existing `Geometry` variant is renamed to `Blueprint`. The existing `UnitCell` variant is renamed to `LatticeVecs` and the `unit_cell` node is renamed to `lattice_vecs`.
- All current atom-operation node definitions need their input/output pin declarations updated to `Atomic` + `SameAsInput("input")`.
- Existing `.cnnd` files using the old node set require format conversion — in particular, `atom_fill` nodes must be replaced with a `Structure` source feeding into the primitive + a `materialize` node.

---

## Implementation Strategy

Work proceeds on a **feature branch**. There is no business requirement to release incrementally, and the changes are too deeply interconnected for gradual backward-compatible migration on main — adding a single `DataType` variant triggers exhaustive-match errors in ~15–20 locations, node renames would require maintaining parallel registrations, and the shim code for each intermediate step would be throwaway. A single `.cnnd` migration script is written at the end.

### Phases

Each phase is a natural stopping point where the code compiles and tests pass.

1. **Rename Geometry → Blueprint.** Pure rename across all ~33 node files, evaluator dispatch, serialization, display system. Behavior is identical; only the type name changes.

2. **Rename UnitCell → LatticeVecs.** Rename `DataType::UnitCell` to `LatticeVecs`, `NetworkResult::UnitCell` to `LatticeVecs`, and the `unit_cell` node to `lattice_vecs`. Pure rename, same pattern as phase 1.

3. **Add Structure value type.** New `DataType::Structure` and `NetworkResult::Structure` variants. Constructor/modifier node (`structure`) only. Preset nodes (`diamond`, `lonsdaleite`, `silicon`, ...) are deferred — an empty `structure` node already defaults to diamond. `get_structure` and `set_structure` are also deferred: they are naturally typed over the `StructureBound` abstract type, which does not exist until phase 5.

4. **Structure input on primitives.** Add optional `Structure` input to primitive nodes (cuboid, sphere, extrude, ...). If unconnected, a default (diamond) is used. Primitives now output `Blueprint` carrying the structure.

5. **Split Atomic into Crystal / Molecule + abstract types.** Redefine `Atomic` as abstract (supertype of Crystal and Molecule). Introduce `StructureBound` and `Unanchored` abstract types, `can_be_converted_to` rules, and `PinOutputType::SameAsInput` so that polymorphic nodes preserve the concrete input type at the output. Update ~23 atom-operation nodes to use `Atomic` + `SameAsInput`. `atom_fill` now outputs `Crystal`.

6. **Phase transitions and movement nodes.** `materialize` / `dematerialize` / `exit_structure` / `enter_structure`, `structure_move` / `structure_rot` / `free_move` / `free_rot`. Remove old duplicates (`atom_lmove`, `atom_lrot`, `atom_move`, `atom_rot`).

7. **Migration script + Flutter API.** `.cnnd` file converter (rename DataType strings, rename node_type_name strings, restructure `atom_fill` into `structure` source + `materialize`). Update `APIDataTypeBase`, regenerate FRB bindings, update Dart UI.
