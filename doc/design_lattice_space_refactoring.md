# Lattice Space Refactoring

## Motivation

The current type system conflates two orthogonal concerns into a single `Geometry` vs `Atomic` divide:

1. whether atoms are materialized (have concrete positions)
2. whether the object is in lattice space or in free space

Symptoms of this conflation:

- `atom_fill` performs two independent actions at once — carving atoms *and* exiting lattice space — leaving no intermediate state where atoms exist while still anchored to a lattice.
- Duplicate nodes `atom_lmove` / `atom_lrot` exist because `lattice_move` / `lattice_rot` are locked to `Geometry`.
- `atom_move` / `atom_rot` are locked to `Atomic` when they should be locked to "free" objects.
- `atom_cut`, `atom_union` exist as hacks because boolean ops only work on `Geometry`.
- Lattice info is discarded after `atom_fill`, so lattice-aligned operations on edited structures are impossible without workarounds.

This refactoring replaces the two-type system with a **three-phase model** plus a minimal type-system extension that gives each phase distinction its own static guarantee.

## Model: Three Phases

Think of it as architecture → construction → deployment. An object in atomCAD is made of up to three independent ingredients:

- **Lattice** — an infinite repeating pattern (unit cell + motif + motif offset) defining where atoms *would* exist everywhere in space.
- **Geometry** — a bounded shape.
- **Atoms** — concrete atom positions, possibly hand-edited.

These compose into three phases:

| Phase | Ingredients | Role |
|---|---|---|
| **Blueprint** | Lattice + Geometry | *Design.* Geometry is a "cookie cutter" positioned in an infinite crystal field. Moving the cutter changes which atoms *would* be carved. This is where geometric design happens: boolean ops, fitting lattices, adjusting surface cuts. |
| **Crystal** | Lattice + Geometry + Atoms | *Construction.* `materialize` has carved atoms out of the lattice. Atoms and geometry now form a rigid body — they move together under any transform. Lattice info is retained so lattice-constrained operations remain available. |
| **Molecule** | Atoms (+ optional geometry shell) | *Deployment.* `exit_lattice` has dropped the lattice association. The object is free-floating and can be moved arbitrarily. |

The forward transitions `materialize` and `exit_lattice` are explicit; `dematerialize` and `enter_lattice` provide the inverses.

## Type System

### Concrete Object Types

| Type | Geometry | Lattice | Atoms | Atoms–Geometry coupling |
|---|---|---|---|---|
| **Blueprint** | yes | yes | no | n/a (atoms are latent) |
| **Crystal** | yes | yes | yes | rigidly coupled |
| **Molecule** | optional | no | yes | rigidly coupled |

### The `Lattice` Value Type

`Lattice` is a first-class value type carrying `unit_cell` + `motif` + `motif_offset`. It flows through the network like any other value. Promoting it to a type (rather than scattering its fields as parameters) lets a single lattice be defined once and reused across many Blueprints, extracted from existing objects with `get_lattice`, or supplied as presets (`diamond_lattice`, `lonsdaleite_lattice`, ...).

### Abstract Types

Many operations are naturally polymorphic over two of the three phases:

- atom operations (`atom_edit`, `apply_diff`, `relax`, ...) work on anything with atoms — Crystal *or* Molecule;
- lattice movement (`lattice_move`, `lattice_rot`) works on anything with a lattice — Blueprint *or* Crystal;
- free movement (`free_move`, `free_rot`) works on anything where free movement is legal — Blueprint *or* Molecule.

This yields three symmetric "two-out-of-three" abstract types, each excluding exactly one phase:

| Abstract type | Members | Excludes | Property |
|---|---|---|---|
| **`Atomic`** | Crystal, Molecule | Blueprint | has materialized atoms |
| **`LatticeBound`** | Blueprint, Crystal | Molecule | has a lattice |
| **`Unanchored`** | Blueprint, Molecule | Crystal | atoms are not locked to a lattice; free movement is legal |

```
              Blueprint
             ╱         ╲
            ╱           ╲
     LatticeBound    Unanchored
         ╱                 ╲
        ╱                   ╲
    Crystal ──── Atomic ──── Molecule
```

Abstract types exist only as **pin constraints**. No runtime value is ever of an abstract type: every runtime value is concretely Blueprint, Crystal, or Molecule. Conversion is uniform — each concrete phase implicitly converts to any abstract type containing it, and there are no implicit downcasts from abstract to concrete.

### Type-Preserving Output Pins

An abstract input alone would lose concrete type information downstream. After `Crystal → atom_edit → ???`, the chain's type would collapse to `Atomic`, and a subsequent `lattice_move` (which needs Crystal) could no longer connect.

To preserve concreteness, output pins are extended to allow mirroring an input pin:

```rust
pub enum PinOutputType {
    Fixed(DataType),
    SameAsInput(String),  // mirrors this input pin's resolved concrete type
}
```

Polymorphic operations declare both input and output using the abstract type and `SameAsInput("input")`:

```
atom_edit:    Atomic       →  SameAsInput("input")
lattice_move: LatticeBound →  SameAsInput("input")
free_move:    Unanchored   →  SameAsInput("input")
```

During wire validation, the concrete type flowing into the input pin is resolved first; the output pin is then treated as that concrete type for all downstream validation. Thus `Crystal → atom_edit → lattice_move` validates cleanly as `Crystal → Crystal → Crystal`.

At runtime nothing special happens: the node receives a concrete `NetworkResult::Crystal(..)` or `NetworkResult::Molecule(..)`, mutates the inner data, and returns the same variant. The wrapper passes through automatically.

### Design Rationale

The full extension is two enum additions — one `DataType` variant per abstract type, plus `PinOutputType::SameAsInput` — and a handful of conversion rules. Full static safety is preserved, no runtime downcasts are needed, and the evaluator is untouched. The existing `update_network_output_type()` machinery already propagates output types from inputs for custom networks, so the "computed output type" pattern has precedent.

Richer alternatives (parametric polymorphism with type variables, row polymorphism with structural records) would work but add substantial machinery for little gain. Simpler alternatives (one merged type with runtime flags, or abstract types with runtime downcasts) sacrifice the static safety that motivates the three-phase model in the first place.

## Nodes

### Lattice Construction and Manipulation

- **`lattice`** — Generic constructor. Inputs: `unit_cell`, `motif`, `motif_offset`. Output: `Lattice`.
- **Preset lattices** — `diamond_lattice`, `lonsdaleite_lattice`, `silicon_lattice`, ... No inputs, output `Lattice`.
- **`get_lattice`** — `LatticeBound → Lattice`. Extracts the lattice from a Blueprint or Crystal.
- **`set_motif`**, **`set_unit_cell`**, **`set_motif_offset`** — Modify one field of a `Lattice`, returning a new `Lattice`.

### Primitives

`cuboid`, `sphere`, `extrude`, ... — output **Blueprint**. Each primitive has an optional `Lattice` input; if unconnected, a default (diamond) is used.

### Blueprint Modifiers

- **`set_lattice`** — `(Blueprint, Lattice) → Blueprint`. Replaces the Blueprint's lattice info. **Geometry is preserved unchanged.** This is how geometry designed in one lattice is transitioned into another — for aligning crystals from different lattices using a shared shape, or seeing what a diamond-designed shape looks like filled with lonsdaleite.

### Boolean Ops

`union`, `intersect`, `diff` — `(Blueprint × Blueprint) → Blueprint`. Both inputs must share a compatible `unit_cell`; otherwise an error is raised. The user reconciles incompatible lattices with `set_lattice`.

### Movement

All four movement nodes are polymorphic via an abstract input and `SameAsInput("input")` output:

| Node | Input | Blueprint | Crystal | Molecule |
|---|---|---|---|---|
| **`lattice_move`**, **`lattice_rot`** | `LatticeBound` | moves geometry only (repositions the cutter) | moves atoms + geometry together | BLOCKED (type error) |
| **`free_move`**, **`free_rot`** | `Unanchored` | moves geometry only, **amber warning** (cutter is off-lattice) | BLOCKED (type error) | moves everything |

Key rules:

- **Blueprint** — geometry moves; the latent atoms stay anchored to the lattice. Free moves are legal but flagged.
- **Crystal** — everything moves together, lattice-constrained only. For free movement, the user must `exit_lattice` first.
- **Molecule** — everything moves together, freely.

### Phase Transitions

```
          materialize              exit_lattice
Blueprint ──────────→ Crystal ──────────────→ Molecule
          ←──────────          ←──────────────
         dematerialize         enter_lattice
```

- **`materialize`** — `Blueprint → Crystal`. Carves atoms according to geometry ∩ lattice. Parameterless.
- **`dematerialize`** — `Crystal → Blueprint`. Discards atoms, returns to the blueprint. **Destructive: atom edits are lost.**
- **`exit_lattice`** — `Crystal → Molecule`. Drops lattice info; geometry is kept as a shell.
- **`enter_lattice`** — `(Molecule, Lattice) → Crystal`. Re-associates a free object with a lattice. Used for repackaging.

### Atom Operations

`atom_edit`, `apply_diff`, `atom_composediff`, `atom_replace`, `add_hydrogen`, `remove_hydrogen`, `relax`, `infer_bonds`, `passivate` — all declared as `Atomic → SameAsInput("input")`. Each preserves its concrete input type (Crystal stays Crystal, Molecule stays Molecule).

### Import / Export

- **`import_cif`** → `Blueprint`. Via multi-output pins, also exposes the extracted `Lattice` for downstream reuse.
- **`import_xyz`** → `Molecule`. XYZ carries no lattice info.
- **`export_xyz`**, **`export_mol`** — input `Atomic` (works on Crystal or Molecule).

## Example Pipelines

**Simple crystal, released for free transport:**
```
diamond_lattice ──┐
cuboid ───────────┴─→ materialize → exit_lattice → free_move
```

**Edit a defect, then shift the whole crystal in-lattice:**
```
diamond_lattice ──┐
cuboid ───────────┴─→ materialize → atom_edit → lattice_move(1,0,0) → exit_lattice
```
`lattice_move` moves atoms + geometry together; the defect rides along.

**Share one lattice across multiple primitives (guaranteeing compatibility for boolean ops):**
```
diamond_lattice ──┬──→ cuboid ──┐
                  └──→ sphere ──┴─→ union → materialize
```

**Reuse a shape in a different lattice via `set_lattice`:**
```
diamond_lattice ─────→ cuboid → lattice_move(align) ──┐
lonsdaleite_lattice ──────────────────────────────────┴─ set_lattice → materialize
```
The exact same geometry carves lonsdaleite instead of diamond.

**Match an imported crystal's lattice via `get_lattice`:**
```
import_cif("quartz.cif") ──→ get_lattice ──┐
cuboid ────────────────────────────────────┴─→ set_lattice → materialize
```

**Repackage a free molecule into a new lattice:**
```
diamond_lattice ──────────────────────────┐
... → exit_lattice → free_move ───────────┴─→ enter_lattice → materialize
```

## What This Replaces

| Old | New |
|---|---|
| `atom_lmove`, `atom_lrot` | `lattice_move`, `lattice_rot` on `LatticeBound` |
| `atom_move`, `atom_rot` | `free_move`, `free_rot` on `Unanchored` |
| `atom_cut` | `diff` on `Blueprint` followed by `materialize` |
| `atom_union` | `union` on `Blueprint` |
| `atom_fill` | `materialize` (pure carving) + `Lattice` data type + lattice constructors / modifiers / `set_lattice` |

## Open Questions

- **Mismatch levels within lattice space** — three dissonance levels exist (fully space-group aligned, lattice aligned but space-group broken, fully lattice-broken); how should they be visually communicated?
- **Boolean ops across incompatible lattices** — allow with `set_lattice` reconciliation, or hard-error?
- **Passivation and surface reconstruction settings** — parameters on `materialize` or separate Blueprint modifier nodes?
- **Implicit `materialize`** — can it eventually become implicit once the explicit phase model is in place?
- **Migration path** ?
