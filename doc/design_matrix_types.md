# Matrix Data Types (`IMat3`, `Mat3`) — Design

## Goal

Add two new first-class pin data types to the node network:

- **`IMat3`** — a 3×3 integer matrix, `[[i32; 3]; 3]`.
- **`Mat3`** — a 3×3 floating-point matrix, `glam::DMat3` (f64 throughout, matching the existing `Vec3 = DVec3` convention).

These types are needed so that:

1. The `supercell` node can accept an **arbitrary** integer matrix from an input pin, not just a diagonal case routed through `IVec3`. Non-axis-aligned rebases (e.g. FCC primitive → conventional) become wirable from computed values, e.g. an `expr` node or a symmetry-operation pipeline.
2. The `expr` node gains a linear-algebra vocabulary — matrix × vector, matrix × matrix, transpose, determinant, inverse — that is useful for any future node that operates on orientations, change-of-basis, or rigid transforms.
3. Downstream nodes (future `motif_edit` orientation, rotation-matrix construction, lattice symmetry operations) have a natural carrier type for 3×3 transforms.

`Mat4` / `IMat4` are **out of scope** for this design; atomCAD's transform nodes currently bake rotations directly into atom positions, and no current workflow needs an affine 4×4 on a pin. Adding them later follows the same template.

## Design decisions

### D1. Row-major convention, matching `supercell`

Matrices are stored and displayed row-major: `m[i][j]` is row `i`, column `j`. Row `i` is the `i`-th new basis vector (for `supercell`) or output component (for matrix × vector).

`matrix × vector`: `(M · v)[i] = Σ_j m[i][j] · v[j]`. This matches the `supercell` design doc's `new_a = M[0][0]·a + M[0][1]·b + M[0][2]·c` formulation exactly — readers of one doc will read the other without convention whiplash.

### D2. Storage

- **`IMat3`** — `[[i32; 3]; 3]`. Plain array, matching `SupercellData::matrix`. No `glam` integer-matrix type exists.
- **`Mat3`** — `glam::DMat3` (f64, 3×3). Note that `glam::DMat3` is **column-major** internally. We store it as-is and do row-major ↔ column-major conversion only at the boundary (text-format serializer, `expr` member access, UI). Keeping the in-memory type native to `glam` lets us reuse `DMat3::determinant`, `DMat3::inverse`, `DMat3::transpose`, and `DMat3 * DVec3`. The extra mental load is one clear seam, not a scattered convention question.

### D3. `IMat3 ↔ Mat3` conversions

Mirrors the existing `IVec3 ↔ Vec3` rule in `DataType::can_be_converted_to` (`rust/src/structure_designer/data_type.rs:173`). The integer→float direction is lossless; the float→integer direction truncates (matches existing `Vec3 → IVec3` precedent at line 175, and `TextValue::as_ivec3` at line 231).

The truncation-on-downcast behavior is a **known footgun**. We surface it via the usual route — no extra warning — because the user clearly asked for an `IMat3` output pin by wiring into an `IMat3` input. If the user wants rounding instead, they write `to_imat3_round(m)` — not provided in phase 1 (see the edge-cases table).

### D4. `IVec3 → IMat3` is NOT a free conversion

Unlike `IVec3 ↔ Vec3`, we do **not** auto-promote `IVec3` to a diagonal `IMat3`. Rationale: the IVec3→diagonal mapping is a specific semantic choice (diagonal placement), not a value-preserving representation change. Making it implicit hides the semantics; forcing the user to route through an `imat3_diag` node makes the intent visible on the graph. Symmetric to how we don't auto-promote `Int` to `Vec3`.

Consequence: the current `supercell` node's `diagonal: IVec3` pin does not become "free" once `IMat3` exists. See §"Supercell integration" for how we migrate.

### D5. Constructors are separate nodes, not one node with a mode toggle

The user suggested a "maybe one constructor with a boolean property toggling rows vs. columns." I'd push back: a boolean property that silently reinterprets three input vectors is a subtle failure mode. Two distinct nodes (`imat3_rows`, `imat3_cols`) are more discoverable and produce clearer serialized text. The on-graph name is the documentation.

The analogous float constructors (`mat3_rows`, `mat3_cols`) follow the same pattern.

### D6. Identity and zero are constants, not constructor nodes

`imat3_identity()` / `mat3_identity()` / `imat3_zero()` / `mat3_zero()` can be expressed by the existing `imat3_rows` / `mat3_rows` node with default inputs (identity: `(1,0,0), (0,1,0), (0,0,1)`; zero: three `(0,0,0)` vectors). Default values for the stored matrix are identity, so an unwired `imat3_rows` node is already "the identity constant." No separate nodes needed.

### D7. Expr scope: minimal-but-useful, not comprehensive

Matrix support in `expr` is worth adding because it lets users compose matrix expressions inline (`a * b + c * identity`), which is the pain point saved constructor nodes *don't* solve. Starting set, chosen for high-leverage-per-line-of-code:

Naming rule: `expr`'s function registry is `HashMap<String, FunctionSignature>` — one signature per name, no overload resolution. We follow the existing `dot2` / `dot3` / `idot2` / `idot3` / `length2` / `length3` convention and use distinct names per input type. Node-constructor names (`mat3_rows`, `imat3_rows`, …) reappear verbatim as expr functions.

- `mat3_rows(a, b, c)` / `imat3_rows(a, b, c)` — row-vector constructors.
- `mat3_cols(a, b, c)` / `imat3_cols(a, b, c)` — column-vector constructors.
- `mat3_diag(v)` / `imat3_diag(v)` — diagonal constructors.
- `transpose3(m)` / `itranspose3(m)`, `det3(m)` / `idet3(m)`, `inv3(m)` — transpose, determinant, inverse. `inv3` exists only for `Mat3` (integer inverse would need a rational type).
- `to_mat3(m)` / `to_imat3(m)` — explicit IMat3 ↔ Mat3 casts. Implicit casting is already covered by `DataType::can_be_converted_to`; the explicit form makes the conversion (especially the truncating downcast) visible in the expression.
- Binary `*`: `Mat3 × Mat3 → Mat3`, `Mat3 × Vec3 → Vec3`, and the IMat3 analogues. This is operator dispatch in `arithmetic_op`'s per-type match, not function-name overloading. The `*` on `Mat3` × `Vec3` is **not commutative**; `Vec3 × Mat3` is rejected (a separate `transpose_mul3` would make that explicit later if anyone needs it).
- Binary `+`, `-` component-wise on matching matrix types. No scalar × matrix yet — not needed by supercell and simple to add later.
- Member access: `.m00` through `.m22` returning scalar. No `.row(i)` / `.col(i)` — lack of integer indexing in `expr` means `row` would need nine variants, not worth it.

Explicitly deferred:

- Rotation-matrix construction (`rotation_x(angle)` etc.) — belongs in a dedicated rotation node where angles and axes are first-class.
- Eigen-decomposition, SVD, rank — no use case yet.
- Integer matrix inverse (no `iinv3`) — would need a rational type.
- `Mat3 × Float` scalar multiply in `expr` — add when the first use case lands.

### D8. Supercell integration: replace `diagonal` pin with `matrix: IMat3`

The current `supercell` node has two input pins: `structure` and `diagonal: IVec3`. Adding matrix types makes the `diagonal` pin redundant — users wire through `imat3_diag` instead. We replace the pin outright rather than keeping both:

- The node just shipped (commits 21d5af87, a9656c78, 24ad4913); there is no installed base of user files with `diagonal` connections to worry about.
- Two override pins (`diagonal` and `matrix`) creates a "which wins?" question the node docs would have to answer.
- The migration is trivial: `.cnnd` files that wired the `diagonal` pin can be migrated by inserting an `imat3_diag` node between the source and the supercell, or by a one-shot serialization migration.

## Type registration

Every new `DataType` touches a fixed set of files. Listing them explicitly so the implementation PR has no blind spots.

| File | Change |
|---|---|
| `rust/src/structure_designer/data_type.rs` | Add `IMat3`, `Mat3` variants to `DataType` enum (line 11). Update `Display` impl (line 36). Update `can_be_converted_to` with `IMat3 ↔ Mat3` rules (line 164). Update `DataTypeLexer::tokenize` and `DataTypeParser::parse_data_type` (line 215+) to recognize the new type names. |
| `rust/src/structure_designer/evaluator/network_result.rs` | Add `IMat3([[i32; 3]; 3])` and `Mat3(DMat3)` variants (line 220). Update `infer_data_type` (line 247). Update `convert_to` with float↔int matrix conversion. Add `extract_imat3` / `extract_mat3` helpers matching the `extract_ivec3` / `extract_vec3` pattern. Update CLI `from_string` parser (line 729+) if matrices need CLI-literal support (optional — see Phase 1 notes). |
| `rust/src/structure_designer/text_format/text_value.rs` | Add `TextValue::IMat3([[i32; 3]; 3])` and `TextValue::Mat3([[f64; 3]; 3])` (note: `[[f64; 3]; 3]` at the `TextValue` layer, not `DMat3`, for plain JSON shape; conversion to `DMat3` happens in `to_network_result`). Update `Serialize` / `Deserialize` impls (lines 27, 83). Update `as_*` helpers, `from_*` constructors, `inferred_data_type`, `to_network_result` (lines 181+). |
| `rust/src/structure_designer/text_format/parser.rs` | Extend the vector literal parser to accept matrix literals — see §"Text-format literal syntax". |
| `rust/src/structure_designer/text_format/serializer.rs` | Serialize `TextValue::IMat3` / `TextValue::Mat3` back to text. |
| `rust/src/api/structure_designer/` | Add `APIIMat3` / `APIMat3` FFI types. Generated Dart bindings follow from `flutter_rust_bridge_codegen generate`. |
| `lib/structure_designer/data_type.dart` (or equivalent) | Add Dart-side `DataType` enum entries to mirror the Rust enum. Used by pin-rendering and hit-testing code. |

Touchpoints per type: ~7 files. Not onerous, but comprehensive — a missed file usually surfaces as a "type not supported" runtime error somewhere unexpected.

## Text-format literal syntax

`TextValue::Vec3(1.0, 2.0, 3.0)` serializes as `(1.0, 2.0, 3.0)` in the text format. Matrices follow the same principle: nested tuples, row-major:

```
m = imat3_rows(
  a: (2, 0, 0),
  b: (0, 2, 0),
  c: (0, 0, 2),
)

-- or inline:
m = supercell(structure: diamond, matrix: ((2, 0, 0), (0, 2, 0), (0, 0, 2)))
```

The `((…), (…), (…))` literal is parsed as `TextValue::IMat3` when the target type is `DataType::IMat3`, `TextValue::Mat3` when `DataType::Mat3`, and fails when the target is `TextValue::Array(TextValue::IVec3)` unless the outer context disambiguates. This is the usual type-directed-parsing mechanism `TextValue::to_network_result` already uses.

Implementation: the existing vector-tuple parser handles `(a, b, c)` with scalar elements. Extend it to recognize nested tuples `((a, b, c), …)` and produce `TextValue::IMat3` / `TextValue::Mat3` based on the requested target type (`to_network_result(&self, expected_type: &DataType)` in `text_value.rs:373`).

## Constructor nodes

### `imat3_rows` — integer matrix from three row vectors

| | |
|---|---|
| **Name** | `imat3_rows` |
| **Category** | `MathAndProgramming` |
| **Inputs** | `a: IVec3` (optional, default `(1,0,0)`), `b: IVec3` (optional, default `(0,1,0)`), `c: IVec3` (optional, default `(0,0,1)`) |
| **Output** | `IMat3` |
| **Stored data** | `IMat3Data { matrix: [[i32; 3]; 3] }` — default is identity. When an input pin is connected it overrides the corresponding row. |
| **Subtitle** | Compact `det = N` or `det = ?` when any row is wired (matches `supercell`). |
| **Text properties** | `a`, `b`, `c` as `TextValue::IVec3`. Same shape as the `supercell` node's text properties — deliberate. |

Eval: for each row, resolve via `evaluate_or_default` (mirror of `ivec3.rs:48`). Combine the three rows into `[[i32; 3]; 3]` and emit `NetworkResult::IMat3(matrix)`.

### `imat3_cols` — integer matrix from three column vectors

Identical to `imat3_rows` except the evaluator stores the three input vectors as columns: `m[i][j] = col_j[i]`. Stored default is still identity.

A single `imat3` node with a `columns: bool` property was considered and rejected (D5).

### `imat3_diag` — integer diagonal matrix from a single vector

| | |
|---|---|
| **Name** | `imat3_diag` |
| **Inputs** | `v: IVec3` (optional, default `(1,1,1)`) |
| **Output** | `IMat3` |
| **Stored data** | `IMat3DiagData { v: IVec3 }` — default `(1,1,1)` (identity). |
| **Text properties** | `v` as `TextValue::IVec3`. |

Eval: produces `diag(v.x, v.y, v.z)` — exactly replicating the `supercell.diagonal` pin's current semantics. This node is what users wire into `supercell.matrix` when they want the axis-aligned case.

### `mat3_rows`, `mat3_cols`, `mat3_diag`

Structural copies of the three `imat3_*` nodes with `Vec3` inputs and `Mat3` output. `mat3_diag` takes a `Vec3` (or an `IVec3` via the standard numeric coercion).

All six constructor nodes are category `MathAndProgramming`.

## `expr` node support

### Validator changes (`rust/src/expr/expr.rs`)

1. `validate()` for `BinOp::Add | BinOp::Sub`: add rows for `(Mat3, Mat3) → Mat3` and `(IMat3, IMat3) → IMat3` at the arithmetic branch (line 88).

2. `validate()` for `BinOp::Mul`: add rows for `(Mat3, Mat3) → Mat3`, `(Mat3, Vec3) → Vec3`, `(IMat3, IMat3) → IMat3`, `(IMat3, IVec3) → IVec3`. The existing vector-promotion rules (`IVec3 × Vec3 → Vec3`, etc.) should extend: `(IMat3, Mat3)` and `(IMat3, Vec3)` promote to `(Mat3, Mat3)` and `(Mat3, Vec3)` respectively.

3. `validate()` for `MemberAccess`: add rows for `Mat3.m00` through `Mat3.m22` → `Float`, and `IMat3.m00` through `IMat3.m22` → `Int` (line 280).

4. `types_compatible`: add `IMat3 ↔ Mat3` for equality comparisons, mirroring `IVec3 ↔ Vec3` at line 434.

### Evaluator changes

1. `arithmetic_op`: add `(NetworkResult::Mat3, NetworkResult::Mat3)` etc. to the match, computing the component-wise result for `+`, `-` and the standard matrix product for `*`. Use `DMat3::mul_mat3` and `DMat3::mul_vec3` — glam handles column-major internally; our row-major API exposure is only at construction and member access.

2. `MemberAccess` evaluation (line 394): add the 18 `.m00`..`.m22` cases (9 for each of `Mat3`, `IMat3`). These are straightforward `NetworkResult::Float(m[i][j])` / `NetworkResult::Int(m[i][j])` arms. Since we store `Mat3` as column-major `DMat3`, the member accessor does the row↔column swap: `.m01` returns `dmat3.col(1).x` — i.e., `.mIJ` returns `dmat3.col(J)[I]`. Convention is asserted in a one-line helper with a unit test covering all nine entries.

### Validator function registrations (`rust/src/expr/validation.rs`)

Per D7's naming rule, every signature gets its own name (the registry — `FUNCTION_SIGNATURES: HashMap<String, FunctionSignature>` at `validation.rs:26` — does not support overload resolution). Register the following:

| Function | Signature | Notes |
|---|---|---|
| `mat3_rows(a, b, c)` | `(Vec3, Vec3, Vec3) → Mat3` | Matches node name. |
| `imat3_rows(a, b, c)` | `(IVec3, IVec3, IVec3) → IMat3` | Matches node name. |
| `mat3_cols(a, b, c)` | `(Vec3, Vec3, Vec3) → Mat3` | Matches node name. |
| `imat3_cols(a, b, c)` | `(IVec3, IVec3, IVec3) → IMat3` | Matches node name. |
| `mat3_diag(v)` | `Vec3 → Mat3` | Matches node name. |
| `imat3_diag(v)` | `IVec3 → IMat3` | Matches node name. |
| `transpose3(m)` | `Mat3 → Mat3` | |
| `itranspose3(m)` | `IMat3 → IMat3` | |
| `det3(m)` | `Mat3 → Float` | |
| `idet3(m)` | `IMat3 → Int` | |
| `inv3(m)` | `Mat3 → Mat3` | Float-only. Returns `Error("inv3: singular matrix")` when `|det| < 1e-12`. |
| `to_mat3(m)` | `IMat3 → Mat3` | Explicit upcast — same result as the implicit conversion via `DataType::can_be_converted_to`, but visible in the expression. |
| `to_imat3(m)` | `Mat3 → IMat3` | Explicit truncating downcast. |

Unifying the paired names (a single `transpose(m)` that dispatches on `Mat3` vs `IMat3`, etc.) would require changing the registry to `HashMap<String, Vec<FunctionSignature>>` and belongs in a separate design doc covering the existing `dot2` / `dot3` / `length2` / `length3` set at the same time.

## Supercell integration

Replace `diagonal` with `matrix: IMat3`. Changes to `rust/src/structure_designer/nodes/supercell.rs`:

- Pin 1 becomes `matrix: IMat3` (optional). Default to stored matrix when unwired.
- Eval: `NetworkResult::IMat3(m) => use m as effective_matrix`. Remove the `NetworkResult::IVec3 => diag(…)` branch.
- Subtitle: when pin is wired, show `det = ?` (current behavior when the override pin is connected).
- `get_parameter_metadata`: update description for the new pin name.
- `.cnnd` migration: `.cnnd` files that wired the old `diagonal` pin either load with a broken connection plus a warning (user rewires through an `imat3_diag` node) or get a one-shot serialization migration that inserts `imat3_diag` automatically. Since the node just shipped there is essentially nothing to migrate — the "warn-and-let-user-fix" path is fine.

## Flutter UI

The Flutter side needs:

1. **Pin rendering** — update `lib/structure_designer/data_type.dart` and the pin-color map so `IMat3` and `Mat3` pins render with a distinct colour. Mirror the existing `IVec3` / `Vec3` scheme (desaturated-integer vs. saturated-float).
2. **Constructor-node editor panels** — each of `imat3_rows`, `imat3_cols`, `imat3_diag` (and float counterparts) gets an editor. The `rows` / `cols` panels can reuse the existing `SupercellEditor` 3×3 grid layout (`lib/structure_designer/node_data/supercell_editor.dart`) with the equation-style labelling swapped out for neutral `row_a = […]` or `col_x = […]` headings. The `diag` panel is a single IVec3 / Vec3 editor.
3. **Supercell panel updates** — per D8 the override pin is renamed from `diagonal: IVec3` to `matrix: IMat3`. In `SupercellEditor` this means renaming every user-visible "diagonal" reference (pin label, the "diagonal pin is connected" messaging that disables the stored-matrix editor) to "matrix". The 3×3 grid that edits the stored override itself does not change: `SupercellData::matrix` is already `[[i32; 3]; 3]`, so only the pin's type flipped, not the stored representation.

No new pin-kind widget is needed if we keep the generic IVec3/Vec3 editor approach. If we later want a polished "matrix constant" node, its UI would go in phase 5.

## Where the code lives

```
rust/src/structure_designer/
├── data_type.rs                      # +IMat3, +Mat3 variants + parsing/conversion
├── evaluator/
│   └── network_result.rs             # +IMat3/Mat3 variants + extractors
├── text_format/
│   ├── text_value.rs                 # +IMat3/Mat3 variants + serde
│   ├── parser.rs                     # +nested-tuple literal parsing
│   └── serializer.rs                 # +matrix serialization
└── nodes/
    ├── imat3_rows.rs                 # NEW
    ├── imat3_cols.rs                 # NEW
    ├── imat3_diag.rs                 # NEW
    ├── mat3_rows.rs                  # NEW
    ├── mat3_cols.rs                  # NEW
    ├── mat3_diag.rs                  # NEW
    ├── supercell.rs                  # MODIFIED: diagonal pin → matrix pin
    ├── mod.rs                        # register the six new modules
    └── (node_type_registry.rs)       # register the six new get_node_type()s

rust/src/expr/
├── expr.rs                           # +BinOp rules, +MemberAccess rules
└── validation.rs                     # +function signatures, +implementations

rust/src/api/structure_designer/
└── (api types file)                  # +APIIMat3, +APIMat3

lib/structure_designer/
├── data_type.dart                    # +IMat3, +Mat3 cases
└── node_data/
    ├── imat3_rows_editor.dart        # NEW (reuse supercell_editor layout)
    ├── imat3_cols_editor.dart        # NEW
    ├── imat3_diag_editor.dart        # NEW
    ├── mat3_rows_editor.dart         # NEW
    ├── mat3_cols_editor.dart         # NEW
    ├── mat3_diag_editor.dart         # NEW
    └── supercell_editor.dart         # MODIFIED if Option A

rust/tests/
├── structure_designer/
│   ├── matrix_types_test.rs          # NEW: data_type + network_result + text_value
│   ├── imat3_nodes_test.rs           # NEW: the six constructor nodes
│   └── supercell_node_test.rs        # UPDATED for new matrix pin
└── expr/
    └── matrix_expr_test.rs           # NEW: binary ops, member access, functions

doc/
└── design_matrix_types.md            # this file
```

## Algorithm notes (the few that aren't obvious)

### Determinant and inverse precision

`DMat3::determinant` returns `f64`; `DMat3::inverse` computes the inverse via cofactor expansion and returns garbage when the matrix is singular (glam does not check). `expr`'s `inv3` implementation should check `determinant.abs() < 1e-12` and return `NetworkResult::Error("inv3: singular matrix")` explicitly — mirrors what `sqrt` does for negative input (validation.rs:221).

### Row-major API over column-major storage (Mat3 only)

`DMat3` stores columns. Our public API is row-major. Three touchpoints:

1. **Constructor `mat3_rows(a, b, c)`** — builds `DMat3::from_cols(col_0, col_1, col_2)` where `col_j = DVec3::new(a[j], b[j], c[j])`. I.e., transposing at construction. Unit test: `mat3_rows((1,2,3),(4,5,6),(7,8,9))` has `.m01 == 2` and `.m10 == 4`.
2. **`.mIJ` member access** — returns `m.col(J)[I]` (inverse of construction).
3. **Text format** — `TextValue::Mat3` is `[[f64; 3]; 3]` in row-major. Converting to `DMat3` transposes; converting back also transposes. Isolated in `to_network_result` and the `Mat3 → TextValue` serializer path.

`IMat3` has no such ambiguity — we store `[[i32; 3]; 3]` row-major directly.

### Matrix × vector

`DMat3 * DVec3` in glam computes `result[i] = Σ_j col_j[i] * v[j]`. Because of the transpose done at construction, this matches our row-major semantics (`result[i] = Σ_j m[i][j] * v[j]`). The `IMat3 * IVec3` implementation in `expr`'s evaluator writes out the three-component sum directly — nine multiplies, no glam.

## Edge cases & error handling

| Case | Handling |
|---|---|
| `inv3(m)` with `|det| < 1e-12` | `NetworkResult::Error("inv3: singular matrix")`. |
| `Mat3 * Vec3` type-promotion of `IVec3` | Upcast `IVec3 → Vec3` by the existing rule; result is `Vec3`. Symmetric with `Vec3 * Float`. |
| `IMat3 * Vec3` | Result is `Vec3` (IMat3 upcasts to Mat3). Follows existing `IVec3 + Vec3 → Vec3` promotion. |
| User writes `(1.5, 2, 3)` into an `IMat3` row in the text format | `TextValue::Float(1.5)` coerces to `Int` by truncation (matches `as_int` at text_value.rs:197). The IMat3 row becomes `(1, 2, 3)`. Possibly surprising but consistent. |
| `.mIJ` where `I` or `J` is not `0`, `1`, or `2` | Validation error: `"Type Mat3 does not have member 'm33'"`. Existing `MemberAccess` error path. |
| `supercell` receives singular matrix via wire | Current behavior: `SupercellError::Degenerate`. Still works — the `apply_supercell` validator runs regardless of matrix source. |
| `Mat3 → IMat3` downcast with non-integer components | Truncate (consistent with `Vec3 → IVec3`). A user who wants rounding uses `to_imat3_round`, which we do not provide in phase 1. |
| Expr `vec * mat` (reversed) | Rejected at validation: `"Arithmetic operation Mul not supported for types Vec3 and Mat3"`. Clear message. A `transpose_mul3` function can be added later if useful. |

## Implementation plan

Five phases. Phases 1–2 are independently mergeable; phase 3 depends on phase 1. Phase 4 depends on phase 1. Phase 5 depends on phases 2 and 3.

### Phase 1 — Core types

**Deliverables:**

- `DataType::IMat3`, `DataType::Mat3` + Display + parse + `can_be_converted_to` rules.
- `NetworkResult::IMat3([[i32; 3]; 3])`, `NetworkResult::Mat3(DMat3)` + `infer_data_type` + `convert_to` + `extract_imat3` / `extract_mat3`.
- `TextValue::IMat3([[i32; 3]; 3])`, `TextValue::Mat3([[f64; 3]; 3])` + serde + `as_imat3` / `as_mat3` / `from_imat3` / `from_mat3` + `to_network_result`.
- Text-format parser accepts `((a,b,c), (d,e,f), (g,h,i))` for `IMat3` / `Mat3` targets.
- FFI types `APIIMat3` / `APIMat3` + codegen.
- Flutter `data_type.dart` enum updates + pin colour.

**Tests** (`rust/tests/structure_designer/matrix_types_test.rs`):

1. `DataType::IMat3 ↔ Mat3` conversions + parsing round-trips.
2. `NetworkResult::IMat3` extractors + convert_to path.
3. `TextValue` serde round-trip for both variants.
4. Parser accepts the nested-tuple literal for both target types.

**Exit:** all tests green; FRB regenerated; `flutter analyze` has no new issues.

### Phase 2 — Constructor nodes

**Deliverables:** `imat3_rows`, `imat3_cols`, `imat3_diag`, `mat3_rows`, `mat3_cols`, `mat3_diag` — six nodes. Each with `get_node_type()`, `NodeData` impl, registered in `node_type_registry.rs` and `nodes/mod.rs`.

**Tests** (`rust/tests/structure_designer/imat3_nodes_test.rs`):

1. Default `imat3_rows` outputs identity.
2. `imat3_rows` with stored matrix — outputs stored value.
3. `imat3_rows` with wired `a` input — row 0 comes from wire, rows 1–2 from stored.
4. `imat3_cols` with three wired IVec3 inputs — produces the column-composed matrix.
5. `imat3_diag` with wired IVec3 — produces `diag(v.x, v.y, v.z)`.
6. `mat3_*` equivalents (three representative tests).
7. `.cnnd` roundtrip — serialize/deserialize each constructor with non-default matrix.
8. `get_node_type()` snapshot tests for all six (extends `node_snapshot_test.rs`).

**Exit:** tests green; FRB regenerated; no new `flutter analyze` warnings.

### Phase 3 — Supercell integration

**Deliverables:** Replace `diagonal: IVec3` pin with `matrix: IMat3`. Update eval, subtitle, parameter metadata. Update `lib/structure_designer/node_data/supercell_editor.dart` to reflect the renamed pin (scope per the Flutter UI section — pin label + "pin is connected" branch). The editor rename ships in this phase, not Phase 5: without it the app would reference a pin that no longer exists.

**Tests:** extend `rust/tests/structure_designer/supercell_node_test.rs`:

1. Matrix pin unwired — stored matrix used (existing test, rename).
2. Matrix pin wired with full IMat3 — effective matrix is the wired value.
3. Matrix pin wired to `imat3_diag((2,2,2))` — equivalent to the old diagonal-pin case.
4. Matrix pin wired to singular matrix — error surfaces through existing validation.

**Exit:** tests green; migration note added to release notes or whatever-we-use-for-breaking-changes.

### Phase 4 — Expr support

**Deliverables:**

- `BinOp` validation + evaluation for `Mat3 × Mat3`, `Mat3 × Vec3`, integer analogues, and the `+` / `-` component-wise forms.
- `MemberAccess` for `.m00`..`.m22` on both types.
- Function registrations: `mat3_rows`, `imat3_rows`, `mat3_cols`, `imat3_cols`, `mat3_diag`, `imat3_diag`, `transpose3`, `itranspose3`, `det3`, `idet3`, `inv3`, `to_mat3`, `to_imat3`.

**Tests** (`rust/tests/expr/matrix_expr_test.rs`):

1. Binary `*`: identity matrix × vector = vector. Matrix × matrix = composed.
2. `.m11` returns the centre element.
3. `transpose3(imat3_rows(a,b,c))` matches `imat3_cols(a,b,c)`.
4. `det3(identity) == 1`, `det3(singular) == 0`.
5. `inv3(singular)` returns error, `inv3(m) * m` ≈ identity for non-singular `m`.
6. `to_imat3(mat3_diag((1.7, 2.3, 3.5)))` truncates to `(1, 2, 3)`.
7. Error paths: `Vec3 * Mat3` rejected; `transpose3(Vec3)` rejected.

**Exit:** tests green.

### Phase 5 — Flutter UI polish

**Deliverables:**

- Editor widgets for the six constructor nodes (reuse the 3×3 grid layout from supercell_editor).
- Widget test covering editor interactions for at least one integer and one float constructor.

**Exit:** widget test green; manual smoke test passes (create each constructor, wire into supercell, verify downstream molecule matches expected).

Phase 5 depends on Phase 2 (the six node types must exist to build editors for them) and on Phase 3 (the smoke test wires constructors into the supercell's new `matrix` pin).

## Open questions

1. **Should `Mat3` use `DMat3` (f64) or `Mat3` (f32)?** Recommend `DMat3` for consistency with `Vec3 = DVec3` (`NetworkResult::Vec3(DVec3)`). atomCAD's atom positions and lattice math run in f64; matching reduces silent precision loss at type boundaries.

2. **Overload resolution for `transpose` / `det`?** Phase 4 ships distinct names (`transpose3` / `itranspose3`). Unified `transpose(m)` that dispatches on `m`'s type is nicer but needs the validator's function registry to support overloads. Out of scope for this design. If the answer later is yes, it also subsumes `dot2/3`, `length2/3`, etc. — that's a separate cleanup.

3. **Matrix decomposition functions in `expr` (SVD, eig)?** Deferred. No current use case; they're heavy to implement; `glam` does not provide them. If needed, `nalgebra` is the usual path.

4. **IMat3 matrix literal in `expr`?** Currently `expr` has no matrix literal syntax — users call `imat3_rows(ivec3(1,0,0), ivec3(0,1,0), ivec3(0,0,1))`. A literal like `[[1,0,0],[0,1,0],[0,0,1]]` would be nicer but requires parser changes in `expr/parser.rs`. Defer until there's friction.

## Risks

Low. The existing `IVec3` / `Vec3` infrastructure is a direct template — most changes are additive enum variants with obvious match arms. The one non-trivial seam is the row-major API over column-major `DMat3` storage; isolating that to `mat3_rows` construction, `.mIJ` access, and text-format serialization (three well-defined places) with unit tests on each keeps the convention from leaking.

The `expr` scope (matrix × matrix, matrix × vector, `+`, `-`, member access, 13 new functions) is the largest single chunk. If it starts sprawling, ship phase 4 in two sub-phases: 4a (binary ops + member access) and 4b (functions).

## Summary

Two new `DataType`s (`IMat3`, `Mat3`), six new constructor nodes, one minor supercell edit, moderate `expr` extension, six small Flutter editors. Touches ~15 Rust files and ~8 Dart files. Estimated effort: ~1 week for a careful implementation with tests, one reviewer. No architectural risks; the templates are all in place.
