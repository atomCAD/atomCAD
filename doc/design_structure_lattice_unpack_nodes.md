# Design: `structure_unpack`, `lattice_vecs_unpack`, `lattice_vecs_params` nodes

## Motivation

`Structure` and `LatticeVecs` are built-in (non-`Record`) data types, so there is
no way to read their fields back out of a value once constructed. A user needs to
obtain the **three lattice basis vectors of a structure**; today that is impossible
because the `structure` and `lattice_vecs` constructor nodes are one-directional.

If these types were user `Record`s, the answer would be `record_destructure`. Since
they are built-ins, we add the equivalent **destructure nodes by hand** — the inverse
of the existing constructors. This keeps the system compositional: instead of a
single-purpose "get basis vectors of a structure" shortcut, the headline use case
falls out of chaining two general-purpose inverses:

```
Structure ─[structure_unpack]→ lattice_vecs ─[lattice_vecs_unpack]→ a, b, c
```

We add **three** nodes:

| Node | Inverse of | Input | Outputs |
|---|---|---|---|
| `structure_unpack` | `structure` | `structure: Structure` | `lattice_vecs: LatticeVecs`, `motif: Motif`, `motif_offset: Vec3` |
| `lattice_vecs_unpack` | `lattice_vecs` (vector form) | `lattice_vecs: LatticeVecs` | `a: Vec3`, `b: Vec3`, `c: Vec3` |
| `lattice_vecs_params` | `lattice_vecs` (crystallographic form) | `lattice_vecs: LatticeVecs` | `a: Float`, `b: Float`, `c: Float`, `alpha: Float`, `beta: Float`, `gamma: Float`, `lengths: Vec3`, `angles: Vec3` |

### Why split the two `lattice_vecs` inverses

`LatticeVecs` carries **two redundant representations** of the same cell (basis
vectors *and* lengths/angles). `lattice_vecs_unpack` exposes the literal stored
basis vectors (true unpack); `lattice_vecs_params` exposes the crystallographic
parameter view (conceptually "derive cell parameters"). Splitting keeps each node
small (3 pins / 8 pins) instead of one 11-pin monster, and is conceptually honest.
The names sort adjacent to their constructors in the alphabetical add-node palette
(`lattice_symop`, `lattice_vecs`, `lattice_vecs_params`, `lattice_vecs_unpack`).

### Naming

Word: **`unpack`** (chosen over `destructure` for length, over `ds`/abbreviations
for legibility — the codebase spells everything out). Order is **noun-first**
(`structure_unpack`, not `unpack_structure`) to match `record_destructure` and to
keep palette adjacency with the constructors. `lattice_vecs_params` uses the
standard crystallographer term "lattice parameters".

> Note on the `a`/`b`/`c` pin-name reuse: on `lattice_vecs_unpack` these are the
> **Vec3 basis vectors**; on `lattice_vecs_params` the same letters are the **Float
> lengths**. This is unambiguous in context (the node names differ) and matches the
> letters used by the `lattice_vecs` constructor and crystallographic convention.
> Pin names `lengths`/`angles` carry the same three values packed as `Vec3`
> (`lengths = (a,b,c)`, `angles = (α,β,γ)`) for convenient single-wire access.

## Data model facts these nodes rely on

From `crystolecule/unit_cell_struct.rs` — `UnitCellStruct` stores **both** views and
keeps them consistent on every construction path:

- `UnitCellStruct::new(a, b, c)` recomputes lengths/angles from the basis vectors.
- `LatticeVecsData::to_unit_cell_struct()` / `UnitCellStruct::from_parameters(...)`
  set vectors and params together.
- The `lattice_vecs` node's `eval` always produces one of those two, so a
  `NetworkResult::LatticeVecs(UnitCellStruct)` reaching our nodes is **guaranteed**
  to have `cell_length_*` / `cell_angle_*` consistent with `a`/`b`/`c`.

⇒ `lattice_vecs_params` does **no geometry** — it reads the stored fields directly.
Angles are in **degrees** (matching `LatticeVecsData` and `UnitCellStruct`):
`alpha = b∠c`, `beta = a∠c`, `gamma = a∠b`.

From `crystolecule/structure.rs` — `Structure { lattice_vecs: UnitCellStruct, motif:
Motif, motif_offset: DVec3 }`. Relevant `NetworkResult` variants
(`evaluator/network_result.rs`): `Structure`, `LatticeVecs(UnitCellStruct)`,
`Motif(Motif)`, `Vec3(DVec3)`, `Float(f64)`.

## Implementation shape (all three nodes)

These are **stateless, fixed-pin** nodes — strictly simpler than `record_destructure`
(which is dynamic). They follow the empty-data-struct pattern of `nodes/structure.rs`
(`StructureData {}` + `generic_node_data_saver`/`loader`), **not** the
registry-cache path:

- No stored state, no text properties, no `get_subtitle`.
- `calculate_custom_node_type` returns `None` (pins are static).
- `output_pins` are declared statically with `OutputPinDefinition::fixed(name, ty)`.
- `eval` returns `EvalOutput::multi(vec![...])`.

### Eval skeleton (`structure_unpack`)

```rust
fn eval(&self, network_evaluator, network_stack, node_id, registry, _decorate, context) -> EvalOutput {
    let arg = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
    match arg {
        // No input wired: emit None on every pin (non-blocking; downstream just
        // gets None — mirrors record_destructure). Do NOT default to diamond;
        // a user wanting diamond defaults wires a `structure` node.
        NetworkResult::None => EvalOutput::multi(vec![NetworkResult::None; 3]),
        NetworkResult::Error(_) => {
            let e = arg; EvalOutput::multi(vec![e.clone(), e.clone(), e])
        }
        NetworkResult::Structure(s) => EvalOutput::multi(vec![
            NetworkResult::LatticeVecs(s.lattice_vecs),
            NetworkResult::Motif(s.motif),
            NetworkResult::Vec3(s.motif_offset),
        ]),
        _ => {
            let e = NetworkResult::Error("structure_unpack: expected a Structure".into());
            EvalOutput::multi(vec![e.clone(), e.clone(), e])
        }
    }
}
```

`lattice_vecs_unpack`: extract `UnitCellStruct` (via `extract_unit_cell` or a match),
emit `Vec3(uc.a)`, `Vec3(uc.b)`, `Vec3(uc.c)`.

`lattice_vecs_params`: emit, in pin order, `Float(uc.cell_length_a)`,
`Float(uc.cell_length_b)`, `Float(uc.cell_length_c)`, `Float(uc.cell_angle_alpha)`,
`Float(uc.cell_angle_beta)`, `Float(uc.cell_angle_gamma)`,
`Vec3((cell_length_a, cell_length_b, cell_length_c))`,
`Vec3((cell_angle_alpha, cell_angle_beta, cell_angle_gamma))`.

### Registration

- `nodes/mod.rs`: add `pub mod structure_unpack;` etc. (alphabetical).
- `node_type_registry.rs` `create_built_in_node_types()`: add the three
  `ret.add_node_type(..._get_node_type());` calls next to `structure`/`lattice_vecs`.
- **Category:** `NodeTypeCategory::OtherBuiltin` (same as `structure`/`lattice_vecs`),
  so they group with their constructors rather than under Math/Programming.
- `public: true`.

### Validation / repair

Nothing new. Fixed pins, no zones, no parameters-as-interface. The standard
`validate_network` wire type-checking handles a wrong-typed input. (No-input is the
clean `None`-passthrough above, so this stays non-blocking.)

## Why there is no Flutter work

`NodeView` is built generically from `output_pins`, and multi-output pin rendering /
eye toggles / per-pin wire offsets already exist (multi-output Phase 4). Because
these nodes are stateless, there is **no property editor to write** — the property
panel is simply empty. The drag-aware add-node popup will surface them for the right
drag source automatically: their input pin type is fixed (not property-driven), so
the default static-pin compatibility check already matches and **no
`adapt_for_drag_source` is needed**.

⇒ Verification only (no Dart code): drop each node, wire a structure/lattice source,
confirm pins render and wires connect, toggle a pin's eye.

## Text format

The `.pinname` multi-output reference syntax already exists (multi-output Phase 5),
so referencing these nodes' pins works with no parser/serializer change:

```
s = structure { }
lv = structure_unpack { structure: s }          // pin 0 (lattice_vecs) implied
m  = motif_edit { motif: structure_unpack.motif } // named pin
abc = lattice_vecs_unpack { lattice_vecs: lv }
ax  = lattice_vecs_unpack.a                        // basis vector a
prm = lattice_vecs_params { lattice_vecs: lv }
beta = lattice_vecs_params.beta
```

Stateless nodes serialize with an empty body `{ }` plus their wired inputs; no
`get_text_properties` needed.

---

## Phased plan

Each phase compiles and tests green on its own. Tests go in `rust/tests/` (never
inline), mirroring source layout, registered in `rust/tests/structure_designer.rs`.

### Phase 1 — `lattice_vecs_unpack` (Rust)
- `nodes/lattice_vecs_unpack.rs`: empty data struct, fixed 3 Vec3 output pins
  (`a`,`b`,`c`), `eval` reading `UnitCellStruct.a/b/c`.
- Register in `nodes/mod.rs` + `node_type_registry.rs`.
- Tests (`rust/tests/structure_designer/unpack_nodes_test.rs`, new, registered):
  - `lattice_vecs` (diamond default) → `lattice_vecs_unpack` → a=(3.567,0,0), etc.
  - A non-orthogonal cell (basis vectors overridden via wired `vec3` inputs) →
    correct a/b/c passthrough.
  - No input wired → all pins `None`.
- **Deliverable:** basis vectors extractable from any `LatticeVecs`.

### Phase 2 — `lattice_vecs_params` (Rust)
- `nodes/lattice_vecs_params.rs`: empty data struct, fixed 8 output pins
  (`a`,`b`,`c`,`alpha`,`beta`,`gamma` as Float; `lengths`,`angles` as Vec3), `eval`
  reading stored `cell_length_*` / `cell_angle_*`, packing the two Vec3s.
- Register in both files.
- Tests (same test module):
  - Diamond default → lengths (3.567×3) + angles (90×3); `lengths`/`angles` Vec3s match.
  - A triclinic cell via `UnitCellStruct::from_parameters(a,b,c,α,β,γ)` with distinct
    non-90 angles → verifies `alpha=b∠c`, `beta=a∠c`, `gamma=a∠b` mapping and degrees.
  - No input wired → all 8 pins `None`.
- **Deliverable:** crystallographic parameters readable from any `LatticeVecs`.

### Phase 3 — `structure_unpack` (Rust)
- `nodes/structure_unpack.rs`: empty data struct, fixed 3 output pins
  (`lattice_vecs: LatticeVecs`, `motif: Motif`, `motif_offset: Vec3`), `eval` per
  the skeleton above.
- Register in both files.
- Tests (same module):
  - Diamond `structure` → unpack → `lattice_vecs` round-trips through a `structure`
    node back to an equivalent structure; `motif_offset` = zero.
  - **End-to-end headline chain:** `structure` → `structure_unpack` →
    `lattice_vecs_unpack` → assert the three basis vectors.
  - Non-Structure input on the pin → `Error` on all pins; no-input → `None` on all pins.
- **Deliverable:** the full Structure → basis-vectors use case works.

### Phase 4 — Serialization, snapshots, palette, manual UI check
- `.cnnd` roundtrip test (`cnnd_roundtrip_test.rs`): a network using all three nodes
  survives save/load with wires intact.
- Text-format roundtrip test (`text_format_test.rs`): the `.pinname` examples above
  parse → serialize → reparse identically.
- **insta snapshots:** the node snapshot test
  (`tests/structure_designer/nodes/node_snapshots_test.rs`) does **not** enumerate
  registered node types — it loads specific fixture `.cnnd` files and snapshots the
  *displayed nodes within them*. Simply registering the three nodes produces **no**
  pending snapshots. To get snapshot coverage, add a small fixture network that uses
  all three nodes (with a displayed pin), wire it into the test, then run
  `cargo insta review` and accept the new snapshot. Skip this bullet if the Phase 1–3
  unit tests already cover eval output (they do) and no fixture is added.
- **Update node-count / name-list assertions (verify, don't assume):** the only
  registry-size assertion today is `built_in_node_types.len() > 20`
  (`node_type_registry_test.rs:234`), a `>` check that adding nodes will **not**
  break — so nothing actually needs bumping. Still, re-grep before committing in case
  new exact-count/name-list assertions have landed:
  `rg -n "add_node_type|built_in_node_types\.len|node type" rust/tests`.
- Manual Flutter walkthrough (no Dart change expected): add each node from the popup,
  wire it, confirm multi-pin rendering, eye toggles, and wiring; confirm the
  drag-from-`Structure`-output popup surfaces `structure_unpack`.
- Run the full gate: `cargo fmt && cargo clippy && cargo test`, `flutter analyze`.

## Out of scope / non-goals
- No reverse `lattice_vecs_params → LatticeVecs` constructor from 6 scalars (the
  `lattice_vecs` node already constructs from params; this doc is destructure-only).
- No single combined 11-pin node (deliberately split — see "Why split").
- No expr-language member access on `Structure`/`LatticeVecs` (could be a future
  convenience; nodes cover the need now).
```
