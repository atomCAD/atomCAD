# Supercell Node — Design

## Goal

Add a `supercell` node that rewrites a `Structure` (lattice vectors + motif + motif offset) as an equivalent `Structure` with a larger unit cell. The physical atom positions produced by `materialize` are unchanged; only the representation changes. This unblocks downstream `motif_edit`/`atom_edit` workflows that need a supercell to work in.

## Node signature

| | |
|---|---|
| **Name** | `supercell` |
| **Category** | `OtherBuiltin` (sits next to `structure`, `motif`, `motif_sub`) |
| **Input pin 0** | `structure: Structure` (required) |
| **Input pin 1** | `diagonal: IVec3` (optional) — when connected, overrides the stored matrix with `diag(v.x, v.y, v.z)`. Common case: axis-aligned supercells like 2×2×2. |
| **Output pin 0** | `Structure` |
| **Stored data** | A 3×3 integer matrix `m: [[i32; 3]; 3]`, default `identity()` |
| **UI panel** | Three equation-style rows, one per new basis vector, plus a determinant readout. See *UI panel* below. |

### UI panel

Rather than three anonymous `IVec3` fields, the panel renders each row as an inline equation so the meaning of every integer is explicit:

```
new_a  =  [2]·a  +  [0]·b  +  [0]·c
new_b  =  [0]·a  +  [2]·b  +  [0]·c
new_c  =  [0]·a  +  [0]·b  +  [2]·c
```

Each `[n]` is an editable integer field; the `·a / ·b / ·c` labels are fixed text referring to the old lattice basis. This removes the "is the row an old-axis or a new-axis?" convention question — the user reads the panel directly.

Below the three rows, a live readout shows the determinant and resulting volume scaling:

```
det = 8    (new volume = 8 × old)
```

Rules for the readout:

- `det > 0` → render in the normal text colour.
- `det == 0` → render in red as `det = 0 (singular — rows are linearly dependent)`.
- `det < 0` → render in red as `det = −N (left-handed basis — not supported)`.

This catches singular and inverted-handedness matrices at edit time, before evaluation, and gives the user an immediate feel for how large the new cell will be.

When the `diagonal` input pin is connected, the panel grays out the stored matrix and shows the effective diagonal matrix derived from the incoming `IVec3`, with the same determinant readout. Disconnecting the pin restores editability of the stored matrix.

### Text format properties

- `a: IVec3` — row 0 of the matrix (coefficients of `old_a, old_b, old_c` in `new_a`).
- `b: IVec3` — row 1.
- `c: IVec3` — row 2.

`TextValue::IVec3` already exists (`rust/src/structure_designer/text_format/text_value.rs`), so no new text-format machinery is needed.

### Input resolution

At eval time:

1. If pin 1 (`diagonal`) is connected, build the effective matrix as `diag(v.x, v.y, v.z)` and ignore the stored matrix.
2. Otherwise, use the stored matrix.

The stored matrix always persists (we do not overwrite it when pin 1 is connected). Disconnecting the pin restores the panel values.

## Semantics

Let `M` be the effective 3×3 integer matrix, with row `i` being `[M[i][0], M[i][1], M[i][2]]`.

New basis vectors (in real space):

```
new_a = M[0][0]·a + M[0][1]·b + M[0][2]·c
new_b = M[1][0]·a + M[1][1]·b + M[1][2]·c
new_c = M[2][0]·a + M[2][1]·b + M[2][2]·c
```

New unit cell volume is `|det(M)| × old_volume`. The new motif contains exactly `|det(M)| × old_sites_count` sites. Each enumerated old cell inside the new cell contributes one tiled copy of the old motif's sites.

A `Structure` defines an infinite set of atoms by tiling its motif over every lattice cell — call this the *crystal field* of the structure. The supercell operation is a pure reparameterization: `crystal_field(supercell(S, M)) = crystal_field(S)` as sets of (position, element) pairs in real space. The two structures describe the same infinite atom pattern; they just factor it into `(cell + motif)` differently. Practical consequence: for any geometry `G`, `materialize(Blueprint { structure: S, geometry: G })` and `materialize(Blueprint { structure: supercell(S, M), geometry: G })` produce the same atom set (up to atom-id renumbering). This is the correctness invariant and the basis of the roundtrip tests.

## Core operation

The logic lives in `rust/src/crystolecule/supercell.rs` as a pure function, so it is trivially testable without the node graph:

```rust
pub fn apply_supercell(
    structure: &Structure,
    matrix: &[[i32; 3]; 3],
) -> Result<Structure, SupercellError>;
```

The node's `eval()` is a thin wrapper that resolves the effective matrix, calls `apply_supercell`, and wraps the result as `NetworkResult::Structure` (or `Error` on validation failure).

## Algorithm

Let `N = |det(M)|`.

**Coordinate transforms used below.** Let `A_old` be the matrix whose columns are the old basis vectors `a, b, c`, and `A_new` likewise for the new basis. Under the rows convention (`new_a = M[0][0]·a + M[0][1]·b + M[0][2]·c`, etc.), we have `A_new = A_old · Mᵀ`. Therefore for any point `x`:

- Old→new fractional coords: `f_new = M⁻ᵀ · f_old` (equivalently `(M⁻¹)ᵀ`).
- Translation by `k ∈ ℤ³` new cells in old-integer coords: `Mᵀ · k` (because shifting by `k_i·new_i` adds `k_i · M[i]`, i.e. row `i` of `M` weighted by `k_i` — which equals `Mᵀ · k` read as a column vector).

These are the two linear maps the algorithm uses; everywhere you see `M⁻ᵀ` or `Mᵀ·k` below, it comes from these identities.

### 1. Validate

- `det(M) == 0` → `SupercellError::Degenerate`.
- `det(M) < 0` → `SupercellError::InvertedHandedness` (left-handed basis; downstream code assumes right-handed).
- `N * structure.motif.sites.len() > MAX_SUPERCELL_SITES` → `SupercellError::TooLarge` (cap TBD — see Open Questions).

### 2. Compute new lattice vectors

```rust
let new_a = m00*a + m01*b + m02*c;
let new_b = m10*a + m11*b + m12*c;
let new_c = m20*a + m21*b + m22*c;
let new_lattice_vecs = UnitCellStruct::new(new_a, new_b, new_c);
```

`UnitCellStruct::new` already recomputes lengths/angles from basis vectors.

### 3. Enumerate site copies and build the new motif

Enumerating whole old cells and blindly copying all their sites is **wrong** for non-diagonal `M`: the new cell's faces are slanted planes in old-integer space, so individual motif sites near an old cell's boundary can fall into or out of the new cell independently of their host cell's origin. We instead enumerate `(old_cell, old_site_idx)` *pairs* and per-site check containment.

Step 3a — AABB of the new cell in old-integer space:

```rust
fn new_cell_aabb(m: &[[i32; 3]; 3]) -> (IVec3, IVec3) {
    let rows = [IVec3::from(m[0]), IVec3::from(m[1]), IVec3::from(m[2])];
    let corners: [IVec3; 8] = [
        IVec3::ZERO,
        rows[0], rows[1], rows[2],
        rows[0] + rows[1], rows[0] + rows[2], rows[1] + rows[2],
        rows[0] + rows[1] + rows[2],
    ];
    // componentwise min/max over the 8 corners → (aabb_min, aabb_max)
}
```

Step 3b — scan the expanded AABB, per-site containment check, build sites and index map in one pass:

```rust
let (aabb_min, aabb_max) = new_cell_aabb(m);
// Expand by -1 on the low side to catch cells whose sites protrude up to +1
// into the new cell. Upper bound stays tight because site.position < 1
// componentwise, so a cell at `aabb_max + 1` can never contribute.
let scan_min = aabb_min - IVec3::ONE;
let scan_max = aabb_max;

// M⁻ᵀ maps old fractional coords to new fractional coords (see "Coordinate
// transforms" above). Precompute once.
let minv_t: DMat3 = dmat3_from_i32(m).inverse().transpose();

let mut site_map: HashMap<(IVec3, usize), usize> = HashMap::new();
let mut new_sites = Vec::new();

for pz in scan_min.z..=scan_max.z {
    for py in scan_min.y..=scan_max.y {
        for px in scan_min.x..=scan_max.x {
            let p = IVec3::new(px, py, pz);
            for (s_idx, site) in structure.motif.sites.iter().enumerate() {
                let old_pos = p.as_dvec3() + site.position;
                let new_frac = minv_t * old_pos;
                if in_unit_cube_half_open(new_frac, EPS) {
                    site_map.insert((p, s_idx), new_sites.len());
                    new_sites.push(Site {
                        atomic_number: site.atomic_number,
                        position: snap_unit(new_frac),
                    });
                }
            }
        }
    }
}

debug_assert_eq!(
    new_sites.len(),
    det_abs(m) as usize * structure.motif.sites.len(),
);
```

Two epsilon-dependent helpers:

- `in_unit_cube_half_open(v, eps)` accepts `v` iff each component is in `[−eps, 1 − eps)`. A site sitting on the lower face of the new cell is included; one on the upper face is excluded (it belongs to the `+new-a` / `+new-b` / `+new-c` neighboring cell). This is the standard half-open `[0, 1)` rule with a tolerance.
- `snap_unit(v)` snaps near-0 and near-`1` components to exactly 0 so the recorded fractional coords are clean on boundaries.

Together these give a consistent no-double-counting inclusion rule for sites on new-cell faces. The debug assertion catches epsilon-policy regressions.

### 4. Rebuild bonds

For each old bond `(site_1, site_2, multiplicity)` — the motif parser enforces `site_1.relative_cell == IVec3::ZERO` — iterate over every copy of `site_1` that ended up inside the new cell:

```rust
let mut new_bonds = Vec::new();

for bond in &structure.motif.bonds {
    debug_assert!(bond.site_1.relative_cell == IVec3::ZERO);
    let s1_idx = bond.site_1.site_index;
    let s2_idx = bond.site_2.site_index;
    let s2_rel = bond.site_2.relative_cell;
    let s2_frac = structure.motif.sites[s2_idx].position;

    for (&(p_1, s_idx), &new_site_1_idx) in &site_map {
        if s_idx != s1_idx { continue; }

        // Endpoint 2's old cell and atom position in old-integer space.
        let end_2_cell: IVec3 = p_1 + s2_rel;
        let end_2_atom_pos: DVec3 = end_2_cell.as_dvec3() + s2_frac;

        // Reduce into an interior copy + new-cell translation.
        // Use the *atom* position, not the cell origin: when M⁻ᵀ has large
        // entries, adding s2_frac can push across an integer boundary and
        // flip which new cell the atom belongs to.
        let new_cell_idx: IVec3 = dvec3_floor(minv_t * end_2_atom_pos);
        // Translation by `new_cell_idx` new cells is Mᵀ·new_cell_idx in old-integer
        // coords (see "Coordinate transforms" above).
        let end_2_reduced: IVec3 = end_2_cell - m_transpose_mul_ivec3(m, new_cell_idx);

        let new_site_2_idx = site_map[&(end_2_reduced, s2_idx)];

        new_bonds.push(MotifBond {
            site_1: SiteSpecifier { site_index: new_site_1_idx, relative_cell: IVec3::ZERO },
            site_2: SiteSpecifier { site_index: new_site_2_idx, relative_cell: new_cell_idx },
            multiplicity: bond.multiplicity,
        });
    }
}

debug_assert_eq!(
    new_bonds.len(),
    structure.motif.bonds.len() * det_abs(m) as usize,
);
```

Correctness argument: `end_2_reduced.as_dvec3() + s2_frac` equals `end_2_atom_pos − Mᵀ·new_cell_idx`, whose `M⁻ᵀ`-image is in `[0, 1)³` by the floor definition — so the reduced copy is guaranteed to be in `site_map` from step 3. Each old bond produces exactly `|det(M)|` new bonds (one per `site_1` copy), matching the correct total.

The linear scan over `site_map` per bond is `O(|bonds| · |det(M)| · |sites|)`, fine for realistic motifs. If this ever shows up in a profile, add a `HashMap<usize, Vec<(IVec3, usize)>>` index keyed by site index.

Finally, recompute `bonds_by_site1_index` / `bonds_by_site2_index` from `new_bonds`, matching how the motif parser does it.

### 5. Assemble

```rust
let new_structure = Structure {
    lattice_vecs: new_lattice_vecs,
    motif: Motif {
        parameters: structure.motif.parameters.clone(),
        sites: new_sites,
        bonds: new_bonds,
        // bonds_by_site1_index / bonds_by_site2_index rebuilt from new_bonds
    },
    motif_offset: snap_unit(minv_t * structure.motif_offset),
};
```

`M⁻ᵀ · old_motif_offset` re-expresses the offset in the new basis: a fractional shift measured in old-lattice units becomes the same physical shift measured in new-lattice units. This is the only line in the algorithm that references `motif_offset` — steps 3b and 4 are offset-free because a constant translation factors out of the per-site and per-bond math. `snap_unit` cleans up near-0 / near-1 noise; it does not reduce arbitrary values mod 1.

## Where the code lives

```
rust/src/crystolecule/
├── supercell.rs                     # NEW: pub fn apply_supercell + SupercellError + helpers
└── mod.rs                           # add `pub mod supercell;`

rust/src/structure_designer/nodes/
├── supercell.rs                     # NEW: SupercellData + NodeData impl + get_node_type()
├── mod.rs                           # add `pub mod supercell;`
└── (node_type_registry.rs)          # register supercell::get_node_type()

rust/tests/crystolecule/
└── supercell_test.rs                # NEW: unit tests for apply_supercell
                                     # (register in rust/tests/crystolecule.rs)

doc/
└── design_supercell_node.md         # this file
```

## Reuse analysis — what from `crystolecule::lattice_fill` can we reuse?

Short answer: **almost nothing, and forcing reuse would make both sides worse.** Here's the honest breakdown.

`lattice_fill` is geared toward a *different* problem: "fill a geometry-bounded region with atoms by SDF sampling." Its primary loop:

1. Recursive AABB subdivision driven by SDF half-diagonal bounds.
2. Conservative cell/AABB overlap tests with epsilons tuned for SDF noise.
3. Batched implicit-evaluator for GPU-ish SDF calls.
4. Output is a Cartesian `AtomicStructure`, plus a `PlacedAtomTracker`, plus passivation and surface reconstruction passes.

`supercell` needs none of this:

- No geometry / no SDF — we know exactly which cells to enumerate from `M` alone.
- No AABB subdivision — the region is tiny and exact.
- Output is a *new* `Motif` (fractional coords), not an `AtomicStructure` (Cartesian).
- No passivation, no reconstruction, no tracker.

**What I considered reusing and rejected:**

- `fill_algorithm::calculate_unit_cell_aabb` (a parallelepiped → AABB helper). Would be reusable, but the supercell algorithm needs the AABB in *old-integer* space of the new cell's corners, which is the integer-matrix analogue — 6 lines inline, no dependency worth introducing.
- `LatticeFillConfig` / `fill_lattice`. These are fundamentally tied to `GeoNode`, SDF, and `AtomicStructure`. Supercell has no geometry.
- `PlacedAtomTracker`. Maps `(motif_pos: IVec3, site_index: usize) → atom_id`. Our `site_map` maps `(old_cell: IVec3, site_index: usize) → new_site_index`. Same shape, different type — and ours is 1 line of `HashMap` insertion inside the enumeration loop. Extracting a generic tracker type wouldn't remove meaningful code.

**What from `UnitCellStruct` we *do* reuse (and it's clean):**

- `UnitCellStruct::new(a, b, c)` to compute lengths/angles from the new basis.
- Nothing else — the lattice↔real helpers aren't in our hot path because supercell is purely an integer-lattice operation.

### Should we refactor `lattice_fill` first? **No.**

I looked for shared vocabulary and found very little. The only two concepts that overlap — "enumerate cells inside a region" and "map `(cell, site)` to an atom/site" — are implemented differently on each side (SDF-conservative vs. exact-integer; atom_id vs. site_index). A shared abstraction would be tortured. Copy-the-shape, share-nothing is the honest call.

If at some later point `lattice_fill` gains an exact-enumeration mode (e.g., for crystalline-interior regions where SDF subdivision is wasteful), *then* we could factor out a shared cell-enumeration helper. Not now.

## Edge cases & error handling

| Case | Handling |
|---|---|
| `det(M) == 0` | Return `SupercellError::Degenerate("rows are linearly dependent")`. Surfaces as `NetworkResult::Error`. |
| `det(M) < 0` | Return `SupercellError::InvertedHandedness`. Left-handed new basis would break downstream geometry code that assumes right-handedness. |
| Identity matrix (`det = 1`, diagonal) | Passes through unchanged (roundtrip test). |
| Non-diagonal matrix with `det = 1` | Equivalent-lattice rebasing. Works the same way; just no volume change. Useful case. |
| Diagonal pin connected with a zero component (e.g., `(2, 0, 2)`) | Effective matrix is singular → `SupercellError::Degenerate`. Clear error. |
| Diagonal pin connected with negative components | Effective matrix has `det < 0` (or still positive if two negatives) → handled by validation. |
| Motif site exactly on a new-cell face | `snap_unit` with ε = 1e-9 maps the face-1 coord to 0 and the face-0 coord stays at 0, so each boundary site is counted once. |
| Very large `det(M)` | Cap at `MAX_SUPERCELL_SITES` (default 10 000 — matches order-of-magnitude of realistic motifs). Emit `SupercellError::TooLarge`. Cap is a debug-UX guardrail, not a correctness constraint. |
| `motif_offset != zero` | `new_motif_offset = snap_unit(Minv · old_motif_offset)` at assembly (step 5). Steps 3b and 4 are offset-free. |

## Implementation plan

Three phases, each independently mergeable, each landing its own tests. Phase 1 is pure Rust math with no node-graph dependency. Phase 2 exposes it through the node system using the generic text-properties panel. Phase 3 adds the custom UI from the *UI panel* section above.

### Phase 1 — Core algorithm (`crystolecule::supercell`)

**Deliverables:**
- `SupercellError` enum: `Degenerate`, `InvertedHandedness`, `TooLarge`.
- `apply_supercell(structure: &Structure, matrix: &[[i32; 3]; 3]) -> Result<Structure, SupercellError>`.
- Private helpers: `new_cell_aabb`, `det_abs`, `in_unit_cube_half_open`, `snap_unit`, `dvec3_floor`, `m_transpose_mul_ivec3` (returns `Mᵀ · v` as `IVec3`), `dmat3_from_i32`.
- Use `i64` internally for AABB bounds and bond-reduction arithmetic (see Open Question 2).
- Register `pub mod supercell;` in `rust/src/crystolecule/mod.rs`.

**Tests** (`rust/tests/crystolecule/supercell_test.rs`, registered in `rust/tests/crystolecule.rs`):
1. Identity matrix — `apply_supercell(s, I)` is `approximately_equal` to `s`.
2. Diagonal `(2, 2, 2)` on cubic diamond — verify site/bond counts (8×) and spot-check several new fractional coords.
3. FCC primitive → conventional (`det = 4`) — site count, volume, and atom-set equality via `materialize`.
4. Bond fidelity — `materialize(supercell(s, M))` and `materialize(s)` have the same edge set (up to atom relabeling) over a small fill box.
5. Double supercell — `apply_supercell(apply_supercell(s, A), B) ≈ apply_supercell(s, B·A)`.
6. Non-zero `motif_offset` — verify `new_motif_offset ≈ Minv · old_motif_offset` and atom-set equivalence with `materialize`.
7. Validation errors — singular matrix, negative determinant, oversized cell.

**Exit criterion:** all tests green; `cargo clippy` clean on the new file.

### Phase 2 — Node, Rust side (`structure_designer::nodes::supercell`)

At the end of this phase the node is fully usable: users can place it, edit nine integers in the default text-properties panel, and wire the `diagonal` pin. The polished UI comes in phase 3.

**Deliverables:**
- `SupercellData { matrix: [[i32; 3]; 3] }` with serde (default: identity).
- `NodeData` impl:
  - `eval` — resolve effective matrix (diagonal pin overrides stored matrix), call `apply_supercell`, wrap as `NetworkResult::Structure` or propagate error.
  - `get_text_properties` / `set_text_properties` — three `TextValue::IVec3` keyed `a`, `b`, `c`.
  - `get_subtitle` — compact `det = N` (or error indicator).
  - `clone_box`.
- `get_node_type()` with inputs `structure: Structure` (required) and `diagonal: IVec3` (optional); single output `Structure`; category `OtherBuiltin`.
- Register in `nodes/mod.rs` and `node_type_registry.rs::create_built_in_node_types`.
- Regenerate FFI bindings: `flutter_rust_bridge_codegen generate`.

**Tests** (`rust/tests/structure_designer/supercell_node_test.rs`, registered in `rust/tests/structure_designer.rs`):
1. Default node passes a structure through unchanged (identity matrix).
2. `set_text_properties` on `a, b, c` — eval uses the stored matrix.
3. Diagonal pin connected — stored matrix is ignored; effective matrix is `diag(v.x, v.y, v.z)`.
4. Error propagation — singular matrix surfaces as `NetworkResult::Error` with a readable message.
5. `.cnnd` roundtrip — configured supercell node saves and reloads with matrix intact (extend `cnnd_roundtrip_test.rs`).
6. Node-type snapshot — `get_node_type()` matches an `insta` snapshot (extend `node_snapshot_test.rs`).

**Exit criterion:** tests green; FFI regenerated; `flutter analyze` reports no new issues beyond the 68 pre-existing ones.

### Phase 3 — UI panel (Flutter)

Replace the generic 9-integer panel with the equation-style layout described in the *UI panel* section.

**Deliverables:**
- Custom panel widget: three rows rendered as `new_a = [n]·a + [n]·b + [n]·c`, one editable integer per `[n]`.
- Live determinant readout `det = N (new volume = N × old)`, red with explanatory text when `det ≤ 0`.
- When the `diagonal` input pin is connected: gray the nine-integer grid, show the effective diagonal matrix derived from the incoming `IVec3`, keep the determinant readout live.

**Tests:**
- Flutter widget test (`integration_test/`): matrix editing, determinant color state transitions, pin-connected gray-out behavior.
- Manual smoke test: place node, edit matrix, wire `diagonal` pin, confirm visuals track the math.

**Exit criterion:** widget test green; manual smoke test passes; no new `flutter analyze` warnings.

## Open questions

1. **`MAX_SUPERCELL_SITES` value.** 10 000 seems safe; FCC primitive → conventional of a 20-site motif × 4 = 80 sites, nowhere near the limit. A user who types `(100, 100, 100)` by mistake should get an error, not a 30-second hang. Open to bumping to 100 000 if legitimate workflows need it.

2. **Integer overflow on large `M`.** Matrix rows are `IVec3` (i32). A matrix like `diag(1000, 1000, 1000)` has `det = 10⁹`, fits in i64 but not i32. The enumeration AABB and bond-reduction arithmetic should use i64 internally, then cast. Cheap and clean — not worth deferring.

3. **UI: should rows be clickable to reset to identity?** Deferred. Default ctor uses identity; text edits make the matrix user-controlled.

## Risks

None serious. The math is standard, the existing `Motif`/`UnitCellStruct` types already model everything we need, and there is no architectural entanglement with SDF/geometry code paths. The only place precision can bite us is the boundary epsilon — that is a solved problem (snap with 1e-9 at `[0, 1)` faces) and we verify via the `new_sites.len() == det(M) × motif.sites.len()` debug assertion.
