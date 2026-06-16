# `IMat2` Type + `plane_tiling_vectors` Helper — Design

## Status: Draft (ready to implement)

## Goal

Two additions:

1. **`IMat2`** — a 2×2 integer matrix data type (`[[i32; 2]; 2]`), a strict 2D mirror of the existing `IMat3` (`doc/design_matrix_types.md`), with the `imat2_rows` / `imat2_cols` / `imat2_diag` constructor nodes.
2. **`plane_tiling_vectors`** — a helper node that turns a Miller-indexed `DrawingPlane` plus a 2×2 integer **superlattice** into the `Array[IVec3]` consumed by `patch_build.tiling_vectors` (see `doc/design_surface_patches.md` §4). The superlattice is entered in the node UI as a 2×2 grid (supercell-style) and is overridable by an optional `IMat2` pin.

This is the implementation behind the "ergonomic vectors" convenience referenced in `doc/design_surface_patches.md`; it lives here so that doc stays focused on patches.

## Decisions

- **Mirror `IMat3` exactly.** Everything about `IMat2` follows `doc/design_matrix_types.md` with 3→2 and `IVec3`→`IVec2`: row-major storage (`m[i][j]` = row `i`, col `j`), row `i` = the `i`-th basis/output vector (D1), plain-array storage (D2), separate `rows`/`cols`/`diag` constructor nodes rather than a mode toggle (D5), identity-by-default so an unwired constructor is the identity constant (D6).
- **`IMat2` only — no float `Mat2`.** `IMat3` has a `Mat3` partner because float 3×3s have consumers (orientations, `expr` linear algebra); nothing needs a float 2×2. So **drop every `Mat2`/`IMat2 ↔ Mat2` arm** the `IMat3` template has — `IMat2`'s only conversion is identity. (Symmetric to that doc scoping out `Mat4`/`IMat4`.) If a float 2×2 is ever needed, it mirrors trivially.
- **No `IVec2 → IMat2` auto-promotion** (D4 analog): wire through `imat2_diag` for the diagonal case.
- **Superlattice rows are the tiling vectors.** In `plane_tiling_vectors`, the 2×2's **rows** are the two superlattice vectors expressed in the `(u_axis, v_axis)` basis — same "each row is a new basis vector" convention as `supercell`. Diagonal `n×m`: rows `(n,0)`,`(0,m)`; √3×√3 R30°: rows `(2,1)`,`(-1,1)`; c(2×2): `(1,1)`,`(1,-1)`.
- **No `.cnnd` migration** — new type, new nodes, additive only.

## Touch-point checklist (mirror the `IMat3` source, drop `Mat3`)

Each row cites the `IMat3` code to copy. Adding the `DataType::IMat2` variant breaks every exhaustive `match` on `DataType`; the compiler enumerates them — give each the `IMat3`-analogous arm.

| Area | File | Change (mirror of) |
|---|---|---|
| Value struct | `rust/src/util/imat2.rs` **(NEW)** + `util/mod.rs` | mirror `util/imat3.rs`: `IMat2 { cols: [IVec2; 2] }`, `new`/`identity`/`mul(IMat2·IVec2)`/`mul_imat2`/`as_dmat2` |
| DataType | `structure_designer/data_type.rs` | `IMat2` enum variant; `Display` arm; `from_string`/parser arm. **No** `IMat2↔Mat2` `can_be_converted_to` arms (identity only). |
| Eval value | `evaluator/network_result.rs` | `IMat2([[i32;2];2])` variant; `infer_data_type` arm; `extract_imat2`; `to_display_string` arm. **No** `convert_to` Mat2 arms. |
| Constructors | `nodes/imat2_rows.rs`, `imat2_cols.rs`, `imat2_diag.rs` **(NEW)** | exact mirror of `imat3_{rows,cols,diag}.rs` (2 rows/cols; `imat2_diag` stores `v: IVec2`). Text props `a`,`b` as `TextValue::IVec2`. |
| Registration | `nodes/mod.rs`, `node_type_registry.rs` | `pub mod` + `add_node_type(...)` for the 3 new nodes (and the helper, below) |
| Text format | `text_format/text_value.rs`, `parser.rs`, `serializer.rs` | `TextValue::IMat2([[i32;2];2])` + serde + `as_imat2`/`from_imat2` + `infer_data_type` + `to_network_result` (identity only); parser accepts `((a,b),(c,d))` for `IMat2` target; serializer emits it |
| FFI types | `api/common_api_types.rs` | `APIIMat2 { m: [[i32;2];2] }` (mirror `APIIMat3`) |
| API types | `api/structure_designer/structure_designer_api_types.rs` | `IMat2` in `APIDataTypeBase`; `APIIMat2RowsData`/`ColsData`/`DiagData` (`APIIVec2` fields); `APIPlaneTilingVectorsData` (below); `APILiteralValue::IMat2` |
| API fns | `api/structure_designer/structure_designer_api.rs` | `get/set_imat2_{rows,cols,diag}_data` + `get/set_plane_tiling_vectors_data` — mirror `get/set_imat3_*` and `get/set_supercell_data` (each takes `scope_path: Vec<u64>`) |
| Flutter type | `lib/inputs/data_type_input.dart`, `type_editor_dialog.dart` | `case APIDataTypeBase.iMat2: return 'IMat2';`; pin colour (mirror `IMat3`) |
| Flutter editors | `lib/structure_designer/node_data/imat2_{rows,cols,diag}_editor.dart`, `plane_tiling_vectors_editor.dart` **(NEW)** + `node_data_widget.dart` | reuse the 2×2 grid pattern of `supercell_editor.dart` / the `IntMatrixCell` in `matrix_cell.dart` |
| Flutter model | `lib/structure_designer/structure_designer_model.dart` | `setImat2*Data` / `setPlaneTilingVectorsData` (forward `propertyEditorScopeChain`), mirror `setSupercellData` |
| FRB | — | `flutter_rust_bridge_codegen generate` after the API changes |

## The `plane_tiling_vectors` helper node

The only non-mirror content. Category `MathAndProgramming`, `public: true`.

```rust
pub struct PlaneTilingVectorsData { pub matrix: [[i32; 2]; 2] }   // default identity [[1,0],[0,1]]
```

| Pin | Type | Req | Role |
|---|---|---|---|
| `plane` | `DrawingPlane` | Yes | Supplies `u_axis, v_axis: IVec3` (the in-plane lattice vectors `DrawingPlane` already derives from its Miller index). |
| `superlattice` | `IMat2` | No | Overrides the stored 2×2 when wired (supercell pattern). |
| → (out) | `Array[IVec3]` | — | `[ m[0][0]·u + m[0][1]·v,  m[1][0]·u + m[1][1]·v ]` |

**Eval:** evaluate `plane` (required; error if unwired/wrong type) → read `u_axis`, `v_axis`. Read the effective matrix `m` from the `superlattice` pin if connected (`NetworkResult::IMat2`), else `self.matrix` — same branch shape as `supercell.rs` eval. Emit `NetworkResult::Array(vec![IVec3(m[0][0]·u + m[0][1]·v), IVec3(m[1][0]·u + m[1][1]·v)])`. Do **not** error on `det(m) == 0`; the two (then dependent) vectors flow to `patch_build`, whose existing linear-independence check on `tiling_vectors` reports it.

**Text properties / subtitle / editor:** mirror `supercell`. Text props `a`, `b` as `TextValue::IVec2` (the two rows). `get_subtitle` shows `det = N`, or `det = ?` when the `superlattice` pin is connected. The editor mirrors `SupercellEditor` with a **2×2** grid (rows `vec1`, `vec2`; columns labelled `·u`, `·v`), a determinant readout, the stored grid disabled when the pin is wired, and a one-line hint that `plane` supplies `u/v`.

**API:** `APIPlaneTilingVectorsData { a: APIIVec2, b: APIIVec2 }` (rows 0/1) + `get/set_plane_tiling_vectors_data(scope_path, node_id, …)`, mirroring `APISupercellData` and its get/set.

**Caveat — conventional cells.** `DrawingPlane` derives `u_axis/v_axis` from the unit cell as-is. For a conventional (centred) cell they are the *conventional* in-plane vectors, so an identity superlattice is the conventional `(1×1)`, not the textbook primitive surface cell. Tiling correctness is unaffected (any integer in-plane basis tiles and welds), but the superlattice numbers are read against the conventional cell — so the editor should surface the resolved `u_axis/v_axis`. The `plane` must be built from the **same `UnitCellStruct`** as `patch_build.lattice`.

## Optional follow-on: `expr` support (deferred)

For full parity with `IMat3`'s `expr` vocabulary (`doc/design_matrix_types.md` §"expr node support", D7), a later phase can add `imat2_rows/cols/diag`, `transpose2`/`itranspose2`, `idet2`, `IMat2 × IVec2` / `IMat2 × IMat2` `*`, component-wise `+`/`-`, and `.m00`..`.m11` member access (returning `Int`). **Not required** for the patch helper — deferred until there's friction, and explicitly lower priority than the type + nodes + helper above.

## Phases

1. **Core type** — `util/imat2.rs`, `DataType::IMat2`, `NetworkResult::IMat2`, `TextValue::IMat2` + parser/serializer, `APIIMat2`, FRB. Tests: `rust/tests/structure_designer/imat2_types_test.rs` (conversion identity, text round-trip, parser). Exit: green + FRB regen + clean `flutter analyze`.
2. **Constructor nodes** — `imat2_{rows,cols,diag}` + API get/set + editors. Tests: `imat2_nodes_test.rs` (defaults=identity, wired rows/cols/diag, `.cnnd` round-trip, snapshots).
3. **Helper node** — `plane_tiling_vectors` + API + editor. Tests: `plane_tiling_vectors_test.rs` (identity → `[u,v]`; diagonal `(2,1)` scaling; non-diagonal √3×√3 vectors; `superlattice` pin override).
4. **(Deferred) `expr` support** — as above.

## Risks

Low — same as `doc/design_matrix_types.md`: additive enum variants with compiler-enumerated match arms, against a complete `IMat3`/`supercell` template. Dropping `Mat2` removes the one subtle seam (row-major-over-column-major) entirely, since integer matrices store row-major directly.
