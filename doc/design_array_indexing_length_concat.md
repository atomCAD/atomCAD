# Array Indexing, Length, and Concatenation

## Scope

Array literals already exist (see `doc/design_array_literals_in_expr.md`). This document adds the four most basic array primitives that are still missing, in both the expression language (the `expr` node) and as dedicated nodes:

| Primitive       | expr syntax        | Node type        |
| --------------- | ------------------ | ---------------- |
| Element access  | `arr[i]`           | `array_at`       |
| Length          | `len(arr)`         | `array_len`      |
| Concatenation   | `concat(a, b)`     | `array_concat`   |
| Append element  | `append(arr, e)`   | `array_append`   |

Together these cover read-only random access, size queries, combination, and single-element extension — the minimum set needed to do anything non-trivial with arrays once they exist.

Explicitly out of scope (deferred):

- **`repeat(value, count)`** (constant-fill array). Workable today via `range` + `map`; revisit only if hand usage shows the workaround is awkward.
- **Slicing / sub-array** (`arr[a..b]`).
- **Higher-order array operators** (`filter`, `reduce`, `any`, `all`). `map` already exists as a node.
- **Index of element / contains check.**
- **Mutating operations** — arrays are values, never mutated in place.
- **Multi-dimensional indexing in one step** (`arr[i, j]`). Use chaining: `arr[i][j]`.
- **Negative indexing / wrap / clamp** — see "Out-of-bounds policy" below.

## Design decisions

| Question                                              | Decision                                                                                  |
| ----------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| expr indexing syntax                                  | `arr[i]` postfix, highest precedence (alongside member access and call)                   |
| expr length function name                             | `len(arr)` — short, no ambiguity inside expr (vector magnitudes use `length2` / `length3`) |
| expr concat function name                             | `concat(a, b)` — function call, not an operator                                           |
| Node names                                            | `array_at`, `array_len`, `array_concat`, `array_append` — type-prefixed, matching the `atom_*` / `imat3_*` precedent for the global node namespace |
| Index type                                            | `Int` only. Nested arrays require chaining (`a[i][j]`)                                    |
| Out-of-bounds policy                                  | **Error.** Consistent with `inv3` on a singular matrix and with the existing array-literal "incompatible element" error. Users wanting a defaulted access can write `if i >= 0 && i < len(a) then a[i] else fallback` |
| Concat element-type rule                              | The two arrays must agree on element type after the standard promotion rules — same algorithm used by array-literal element unification |
| `array_len`, `array_concat`, `array_at`, `array_append` configuration | A user-selected `element_type` property, exactly like `sequence`. The element type fully determines the input and output pin types; no polymorphism. `array_len` outputs `Int` regardless |
| Concat empty-array handling                           | `concat([]T, [1,2,3])` ≡ `[1,2,3]`. No special case — falls out of unification |
| expr append function name                             | `append(arr, elem)` — function call. Rejected `push` (mutation connotation; our arrays are values) and `add` (too generic in the global node namespace) |
| Append element-type rule                              | Same as concat: arg 0's element type and arg 1's type unify under the standard promotion rules. Node form has no cross-promotion (wire-time conversion handles it). |

### Naming asymmetry rationale

The expr language has its own scoped vocabulary; the node graph is a shared global namespace. Inside expr, `len` is unambiguous — array length is the only "size of a container" query the language has. In the node graph, `len` would collide visually with anything else that might one day query a length, so the type prefix earns its keep there. This same split is already in flight: the language has `length2`/`length3` (vector magnitudes, suffix-by-dimension), and there is no `length` node — node-side analogues live behind named primitives. Carrying that pattern to arrays gives `len` (expr) and `array_len` (node), which is consistent with how the language and the node graph are already drawn.

### Out-of-bounds policy

`a[i]` and `array_at` produce an evaluation error on `i < 0` or `i >= len(a)`. Error message: `array index {i} out of bounds for array of length {n}`.

This is consistent with `inv3(m)` on a singular matrix (also evaluation error) and with array-literal element-type mismatch errors. Users who want a default fallback can write the check explicitly:

```
if i >= 0 && i < len(a) then a[i] else fallback
```

Adding a clamp / wrap / default-value mode as a property on the `array_at` node is **not** part of this design. If the explicit-conditional pattern proves cumbersome in real use, revisit then.

---

## Phase 1 — Indexing in expr (`arr[i]`)

### Syntax

`arr[index_expr]` where `arr` is any expression of type `Array[T]` and `index_expr` is any `Int`-typed expression. The result has type `T`.

Precedence: same as function call and member access (level 1, highest). Indexing chains naturally with `.member` and `(args)`:

```
points[i].x                  // ((points[i]).x) — element first, then member
data[i][j]                   // ((data[i])[j]) — chained for nested arrays
positions[2 * k + 1]         // arbitrary Int expression as index
[1, 2, 3][i]                 // index a literal — legal but odd
```

### Parser

Indexing is a postfix operator parsed at the same level as function-call and member-access in the existing Pratt loop. The `[` token is already lexed (used for array literals). The disambiguation rule is **position-based**:

- `[` in **prefix position** (start of an expression) → array literal (`Expr::Array` / `Expr::EmptyArray`, already implemented).
- `[` in **postfix position** (immediately after a parsed expression) → index operation (`Expr::Index`).

These never collide because the parser knows which position it is in.

### AST

Add one variant:

```rust
pub enum Expr {
    // ... existing variants ...
    Index(Box<Expr>, Box<Expr>),    // arr[index]
}
```

### Validation

```rust
Expr::Index(arr, idx) => {
    let arr_ty = arr.validate(variables, functions)?;
    let idx_ty = idx.validate(variables, functions)?;
    let elem_ty = match arr_ty {
        DataType::Array(inner) => *inner,
        other => return Err(format!("cannot index into non-array type {:?}", other)),
    };
    if !matches!(idx_ty, DataType::Int) {
        return Err(format!("array index must be Int, got {:?}", idx_ty));
    }
    Ok(elem_ty)
}
```

Note: only `Int` is accepted — not `Bool`, not `Float`. Even though `Bool` promotes to `Int` in arithmetic, semantically indexing by a boolean is almost certainly a bug, so reject it.

### Evaluation

The arm checks `Error` only and lets everything else (incl. type mismatches that should already be caught by validation) fall through to a typed-error result. This matches the convention used by every other `Expr::*` evaluation arm (`Unary`, `Binary`, `Conditional`, `MemberAccess`, `Array`, `Call`).

`NetworkResult::None` does not need a dedicated arm because it cannot reach this layer: the expr node converts unconnected/`None` parameter values into error outputs in `ExprData::eval` (`rust/src/structure_designer/nodes/expr.rs`, lines ~152–158) before they enter the `variables` HashMap, and no `Expr::*` evaluation arm produces `None`. The `_ => Error("indexing non-array value")` fall-through is therefore reachable only on a real type bug, not on missing-input propagation.

```rust
Expr::Index(arr, idx) => {
    let arr_v = arr.evaluate(variables, functions);
    if let NetworkResult::Error(_) = arr_v { return arr_v; }
    let idx_v = idx.evaluate(variables, functions);
    if let NetworkResult::Error(_) = idx_v { return idx_v; }

    let elements = match arr_v {
        NetworkResult::Array(v) => v,
        _ => return NetworkResult::Error("indexing non-array value".into()),
    };
    let i = match idx_v {
        NetworkResult::Int(n) => n,
        _ => return NetworkResult::Error("array index must be Int".into()),
    };
    if i < 0 || (i as usize) >= elements.len() {
        return NetworkResult::Error(format!(
            "array index {} out of bounds for array of length {}",
            i, elements.len()
        ));
    }
    elements.into_iter().nth(i as usize).unwrap()
}
```

### Help text

Update the embedded help text in `rust/src/structure_designer/nodes/expr.rs` to mention indexing in a new "Array Access" section directly after the existing "Array Literals" section:

```
**Array Access:**

- `arr[i]` — element access; `i` is an Int expression. Out-of-bounds is an
  evaluation error. For nested arrays, chain: `arr[i][j]`.
```

### Phase 1 tests

New file `rust/tests/expr/array_index_test.rs`, registered in `rust/tests/expr.rs`.

**Parser:**

| Input              | Expected AST                                      |
| ------------------ | ------------------------------------------------- |
| `a[0]`             | `Index(Var("a"), Int(0))`                         |
| `a[i + 1]`         | `Index(Var("a"), Binary(Var("i"), Add, Int(1)))`  |
| `a[i][j]`          | `Index(Index(Var("a"), Var("i")), Var("j"))`      |
| `a[i].x`           | `MemberAccess(Index(Var("a"), Var("i")), "x")`    |
| `[1, 2, 3][0]`     | `Index(Array([Int(1), Int(2), Int(3)]), Int(0))`  |
| `a[]`              | parse error (empty index)                         |
| `a[1, 2]`          | parse error (no comma in index)                   |
| `a[1`              | parse error (unclosed)                            |

**Validation:**

| Expression | Variables                   | Expected output type      |
| ---------- | --------------------------- | ------------------------- |
| `a[0]`     | a: Array[Int]               | `Int`                     |
| `a[i]`     | a: Array[Float], i: Int     | `Float`                   |
| `a[i]`     | a: Array[IVec3], i: Int     | `IVec3`                   |
| `a[i][j]`  | a: Array[Array[Int]], i,j:Int | `Int`                   |
| `a[i].x`   | a: Array[IVec3], i: Int     | `Int` (member access on IVec3) |
| `a[0]`     | a: Int                      | error (non-array)         |
| `a[true]`  | a: Array[Int]               | error (Bool index)        |
| `a[1.0]`   | a: Array[Int]               | error (Float index)       |
| `a[i]`     | a: Array[Structure], i: Int | `Structure`               |

**Evaluation:**

| Expression       | Variables                              | Expected `NetworkResult`              |
| ---------------- | -------------------------------------- | ------------------------------------- |
| `a[0]`           | a: Array([Int(10), Int(20), Int(30)])  | `Int(10)`                             |
| `a[2]`           | a: Array([Int(10), Int(20), Int(30)])  | `Int(30)`                             |
| `a[i + 1]`       | a: Array([Int(10), Int(20), Int(30)]), i: Int(1) | `Int(30)`                  |
| `a[i][j]`        | a: Array([Array([Int(1), Int(2)]), Array([Int(3), Int(4)])]), i: Int(1), j: Int(0) | `Int(3)` |
| `a[-1]`          | a: Array([Int(10)])                    | `Error("array index -1 out of bounds for array of length 1")` |
| `a[3]`           | a: Array([Int(10), Int(20), Int(30)])  | `Error("array index 3 out of bounds for array of length 3")`  |
| `a[0]`           | a: Array([])                           | `Error("array index 0 out of bounds for array of length 0")`  |

**Roundtrip / integration** (`rust/tests/structure_designer/`):

- Create an expr node with `expression: "a[i]"` and parameters `a: Array[IVec3]`, `i: Int`. Save to `.cnnd`, reload, re-validate. Confirm the indexed-access expression survives roundtrip.

### Phase 1 implementation checklist

1. [ ] AST: add `Expr::Index(Box<Expr>, Box<Expr>)` (`rust/src/expr/expr.rs`).
2. [ ] Validation: add arm for `Expr::Index`.
3. [ ] Evaluation: add arm for `Expr::Index`.
4. [ ] Parser: add postfix handling for `LBracket` after a parsed expression (`rust/src/expr/parser.rs`). Reuses the existing Pratt structure used by member access and function call.
5. [ ] Help text update in `rust/src/structure_designer/nodes/expr.rs`.
6. [ ] Tests in `rust/tests/expr/array_index_test.rs` (parser, validation, evaluation).
7. [ ] Roundtrip test in `rust/tests/structure_designer/`.
8. [ ] `cd rust && cargo fmt && cargo clippy && cargo test`.
9. [ ] Reference-guide doc update: add an "Array Access" subsection to `doc/reference_guide/nodes/math_programming.md` under the expr-node section.

---

## Phase 2 — `array_at` node

### Behavior

Reads one element from an array at a given integer index.

**Properties**

- `Element type` — the element type of the input array. All array element types accepted by `sequence` are accepted here.

**Input pins**

- `array: Array[ElementType]` — the array.
- `index: Int` — the zero-based index.

**Output pin**

- `out: ElementType` (declared via `OutputPinDefinition::single_fixed(self.element_type.clone())`, mirroring the way `sequence` types its array output from the configured `element_type`).

**Behavior**

If `array` or `index` is unconnected, evaluation yields `NetworkResult::None` (matching how other nodes treat missing required inputs — see `sequence` for the unconnected-pin convention). Otherwise: same out-of-bounds rule as the expr operator — produce an evaluation error on `i < 0` or `i >= len`.

### Implementation sketch

New file `rust/src/structure_designer/nodes/array_at.rs`. Mirror `sequence.rs` for the `element_type` property, text-format properties, and registration. The `eval` function:

1. Evaluate `array` arg → expect `NetworkResult::Array(items)`.
2. Evaluate `index` arg → expect `NetworkResult::Int(i)`.
3. Bounds-check, return `EvalOutput::single(items[i].clone())` or an error result.

`calculate_custom_node_type` follows the `sequence` pattern: builds parameters from the configured `element_type`, uses `OutputPinDefinition::single_fixed(self.element_type.clone())` for the output.

### Phase 2 tests

New file `rust/tests/structure_designer/array_at_test.rs`, registered in `rust/tests/structure_designer.rs`.

| Scenario                                                                 | Expected                          |
| ------------------------------------------------------------------------ | --------------------------------- |
| `array_at` with element_type `Int`, array=`[10,20,30]`, index=`1`        | `Int(20)`                         |
| `array_at` with element_type `IVec3`, array=`[ivec3(1,2,3), ivec3(4,5,6)]`, index=`0` | `IVec3(1,2,3)`        |
| `array_at` with element_type `Crystal`, array of two crystals, index=`1` | second `Crystal` value             |
| Index `-1`                                                               | error result                      |
| Index past end                                                           | error result                      |
| Empty array, index `0`                                                   | error result                      |
| Unconnected `array` pin                                                  | `None` (evaluation propagates)    |
| Unconnected `index` pin                                                  | `None`                            |

Plus:

- **Snapshot test** — add `array_at` to `rust/tests/structure_designer/node_snapshot_test.rs` so the node-type definition is locked in.
- **Text-format roundtrip** — verify properties (`element_type`) and pin wiring serialize and deserialize via the existing `text_format_test.rs` pattern. Add a case for `array_at` with `element_type: Structure` and one with `element_type: Int`.
- **`.cnnd` roundtrip** — add a fixture under `rust/tests/fixtures/` containing an `array_at` node and assert load → save → reload is stable.

### Phase 2 implementation checklist

1. [ ] `rust/src/structure_designer/nodes/array_at.rs`.
2. [ ] Register in `rust/src/structure_designer/nodes/mod.rs`.
3. [ ] Register in `rust/src/structure_designer/node_type_registry.rs::create_built_in_node_types()`.
4. [ ] Tests in `rust/tests/structure_designer/array_at_test.rs`.
5. [ ] Snapshot in `rust/tests/structure_designer/node_snapshot_test.rs` + run `cargo insta review`.
6. [ ] Text-format and `.cnnd` roundtrip cases.
7. [ ] Reference-guide entry for `array_at` in `doc/reference_guide/nodes/math_programming.md`.
8. [ ] No FRB regen (no API surface change beyond new node type, which surfaces through existing node-type machinery).

---

## Phase 3 — Length in expr (`len`)

### Syntax

`len(expr)` — function call returning `Int`. The argument must be of type `Array[T]` for any `T`.

```
len([1, 2, 3])              // 3
len([]Int)                  // 0
len(positions)              // length of an Array[IVec3] parameter
len(a[i])                   // length of a nested array element (a: Array[Array[T]])
```

### Implementation

**Decision: special-case `len` inside `Expr::Call::validate`; register the runtime impl in `FUNCTION_IMPLEMENTATIONS` like the other built-ins.**

Rationale. The cost is asymmetric between the two registries:

- `FunctionSignature` (in `rust/src/expr/validation.rs`) is `{ parameter_types: Vec<DataType>, return_type: DataType }` — fixed types only. Expressing "any `Array[*]`" requires turning `parameter_types` into something like `Vec<ArgKind>` with a `Fixed(DataType) | AnyArray` enum, updating `Expr::Call::validate`'s `types_compatible` dispatch, and touching all ~40 existing `FunctionSignature::new(...)` sites in `create_standard_function_signatures`. That is a partial generics system used by exactly one function.
- `EvaluationFunction = Box<dyn Fn(&[NetworkResult]) -> NetworkResult + Send + Sync>` already places **no** type constraint on arguments, so a polymorphic `len` implementation drops into `FUNCTION_IMPLEMENTATIONS` with no surgery.

Concretely:

1. In `Expr::Call::validate` (currently in `rust/src/expr/expr.rs` around line 251), add a guard at the top of the arm that intercepts `name == "len"` *before* the registry lookup: arity must be 1, the single argument's validated type must be `DataType::Array(_)`, return `DataType::Int`. Otherwise fall through to the existing registry-driven path. Do **not** add an entry for `len` to `FUNCTION_SIGNATURES` — the special case fully replaces it.
2. Register `len` in `FUNCTION_IMPLEMENTATIONS` alongside `length2`, `length3`, etc.: match `args[0]` on `NetworkResult::Array(items)` → `NetworkResult::Int(items.len() as i32)`; mismatch returns an error result (defensive — validation should already have caught it).

No new runtime arm in `Expr::Call::evaluate` is needed; the standard registry path handles the call once the implementation is registered.

### Help text

Append to the "Array Access" section of the expr-node help text:

```
- `len(arr)` — number of elements in `arr`. Returns Int. Works on arrays of
  any element type, including empty arrays (`len([]Int)` is 0).
```

### Phase 3 tests

Append to `rust/tests/expr/array_length_test.rs` (new file):

**Validation:**

| Expression           | Variables                   | Expected output type |
| -------------------- | --------------------------- | -------------------- |
| `len([1, 2, 3])`     | —                           | `Int`                |
| `len([]Int)`         | —                           | `Int`                |
| `len(a)`             | a: Array[IVec3]             | `Int`                |
| `len(a[i])`          | a: Array[Array[Int]], i: Int | `Int`               |
| `len(5)`             | —                           | error (non-array)    |
| `len([1, 2], [3])`   | —                           | error (wrong arity)  |

**Evaluation:**

| Expression                         | Expected                |
| ---------------------------------- | ----------------------- |
| `len([1, 2, 3])`                   | `Int(3)`                |
| `len([]Int)`                       | `Int(0)`                |
| `len([[1,2], [3,4,5]])`            | `Int(2)`                |
| `len([ivec3(1,2,3), ivec3(4,5,6)])` | `Int(2)`               |

### Phase 3 implementation checklist

1. [ ] Add `name == "len"` guard at the top of `Expr::Call::validate` (`rust/src/expr/expr.rs`).
2. [ ] Register `len` runtime impl in `FUNCTION_IMPLEMENTATIONS` (`rust/src/expr/validation.rs`).
3. [ ] Help-text update in `rust/src/structure_designer/nodes/expr.rs`.
4. [ ] Tests in `rust/tests/expr/array_length_test.rs`.
5. [ ] `cd rust && cargo fmt && cargo clippy && cargo test`.
6. [ ] Reference-guide doc update.

---

## Phase 4 — `array_len` node

### Behavior

Returns the length of the input array as an `Int`.

**Properties**

- `Element type` — the element type of the input array. (Required because the input pin must be typed; same convention as `sequence` and `array_at`.)

**Input pin**

- `array: Array[ElementType]`

**Output pin**

- `out: Int`

**Behavior**

If `array` is unconnected, evaluate to `NetworkResult::None`. Otherwise, return `Int(len)`.

### Implementation sketch

New file `rust/src/structure_designer/nodes/array_len.rs`. Same shape as `array_at` minus the index input. Output pin is `OutputPinDefinition::single_fixed(DataType::Int)` (not polymorphic — length is always `Int`).

### Phase 4 tests

New file `rust/tests/structure_designer/array_len_test.rs`:

| Scenario                                                       | Expected                  |
| -------------------------------------------------------------- | ------------------------- |
| element_type `Int`, array=`[1,2,3]`                            | `Int(3)`                  |
| element_type `IVec3`, array of length 5                        | `Int(5)`                  |
| element_type `Int`, empty array                                | `Int(0)`                  |
| Unconnected `array` pin                                        | `None`                    |
| Snapshot of node-type definition                               | locked in via insta       |
| Text-format roundtrip                                          | passes                    |
| `.cnnd` roundtrip                                              | passes                    |

### Phase 4 implementation checklist

1. [ ] `rust/src/structure_designer/nodes/array_len.rs`.
2. [ ] Register in `nodes/mod.rs` and `node_type_registry.rs`.
3. [ ] Tests in `rust/tests/structure_designer/array_len_test.rs`.
4. [ ] Snapshot + insta review.
5. [ ] Text-format and `.cnnd` roundtrip cases.
6. [ ] Reference-guide entry in `math_programming.md`.

---

## Phase 5 — Concat in expr (`concat`)

### Syntax

`concat(a, b)` — function call. Both arguments must be `Array[*]`. The result is an `Array[U]` where `U` is the unified element type computed by the same promotion rules used by array-literal element unification (see `design_array_literals_in_expr.md`).

```
concat([1, 2], [3, 4])              // Array[Int],   [1, 2, 3, 4]
concat([1, 2], [3.0, 4.0])          // Array[Float], promotion across the boundary
concat([]Int, [1, 2, 3])            // Array[Int],   [1, 2, 3]
concat(a, b)                        // a: Array[T], b: Array[T] → Array[T]
concat([ivec3(1,2,3)], [vec3(0.5, 0.5, 0.5)])  // Array[Vec3] (IVec3 promoted)
```

Two-argument form only. Multi-argument concat (`concat(a, b, c, d)`) is a small extension we may add later if usage shows it would help; for the initial drop, two-argument is simpler and `concat(concat(a, b), c)` is always available.

### Validation

```rust
// Inside the special-case for concat in Expr::Call validation
let a_ty = args[0].validate(...)?;
let b_ty = args[1].validate(...)?;
let elem_a = match a_ty {
    DataType::Array(t) => *t,
    other => return Err(format!("concat arg 0 must be Array, got {:?}", other)),
};
let elem_b = match b_ty {
    DataType::Array(t) => *t,
    other => return Err(format!("concat arg 1 must be Array, got {:?}", other)),
};
let elem = unify_array_element_types(&elem_a, &elem_b)
    .map_err(|_| format!(
        "concat arguments have incompatible element types: {} and {}",
        elem_a, elem_b
    ))?;
Ok(DataType::Array(Box::new(elem)))
```

`unify_array_element_types` already exists from the array-literals work — reuse it directly.

### Evaluation

`concat` is registered in `FUNCTION_IMPLEMENTATIONS` like `length2`, `length3`, etc. Argument-evaluation error-propagation is already handled by `Expr::Call::evaluate` (`rust/src/expr/expr.rs:411-428`) — it short-circuits on any `Error` arg before invoking the registered impl. So the impl receives `&[NetworkResult]` whose entries are guaranteed non-`Error`, and only needs to dispatch on `Array` vs. anything else (the latter is defensive — validation already rejected non-array args):

```rust
"concat".to_string(),
Box::new(|args: &[NetworkResult]| -> NetworkResult {
    if args.len() != 2 {
        return NetworkResult::Error("concat() requires exactly 2 arguments".to_string());
    }
    let mut out = match &args[0] {
        NetworkResult::Array(v) => v.clone(),
        _ => return NetworkResult::Error("concat() requires array arguments".to_string()),
    };
    match &args[1] {
        NetworkResult::Array(v) => out.extend(v.iter().cloned()),
        _ => return NetworkResult::Error("concat() requires array arguments".to_string()),
    }
    NetworkResult::Array(out)
})
```

`None` does not need a dedicated arm here for the same reason as in Phase 1's `Expr::Index`: the expr node converts unconnected inputs to error outputs before any `Expr::*` evaluation runs, and no `Expr::*` arm produces `None`. The `_ =>` fall-through covers genuine type bugs only.

### Help text

Append to the "Array Access" section:

```
- `concat(a, b)` — concatenate two arrays. The result element type is the
  unification of the two element types under the standard promotion rules
  (e.g. `concat([]Int, [1,2,3])` is `[1,2,3]`; `concat([1,2], [3.0])` is
  `Array[Float]`).
```

### Phase 5 tests

New file `rust/tests/expr/array_concat_test.rs`:

**Validation:**

| Expression                                                | Expected output type |
| --------------------------------------------------------- | -------------------- |
| `concat([1, 2], [3, 4])`                                  | `Array[Int]`         |
| `concat([1, 2], [3.0, 4.0])`                              | `Array[Float]`       |
| `concat([]Int, [1, 2, 3])`                                | `Array[Int]`         |
| `concat([]Int, []Int)`                                    | `Array[Int]`         |
| `concat([ivec3(1,2,3)], [vec3(1.0, 2.0, 3.0)])`           | `Array[Vec3]`        |
| `concat([1, 2], [vec3(1,2,3)])`                           | error (incompatible) |
| `concat([1, 2], 3)`                                       | error (non-array)    |
| `concat([1, 2])`                                          | error (arity)        |
| `concat([1, 2], [3], [4])`                                | error (arity)        |
| `concat([[1,2]], [[3,4]])`                                | `Array[Array[Int]]`  |

**Evaluation:**

| Expression                            | Expected                                |
| ------------------------------------- | --------------------------------------- |
| `concat([1, 2], [3, 4])`              | `Array([Int(1), Int(2), Int(3), Int(4)])` |
| `concat([]Int, [1, 2])`               | `Array([Int(1), Int(2)])`               |
| `concat([]Int, []Int)`                | `Array([])`                             |
| `concat([1], concat([2], [3]))`       | `Array([Int(1), Int(2), Int(3)])`        |
| `len(concat([1, 2], [3, 4]))`         | `Int(4)` (composes with `len`)          |
| `concat([1, 2], [3.0])[2]`            | `Float(3.0)` (composes with indexing — element 2, with promotion) |

### Phase 5 implementation checklist

1. [ ] Add `name == "concat"` guard at the top of `Expr::Call::validate` (alongside the `len` guard from Phase 3) — same rationale: the polymorphic argument shape doesn't fit `FunctionSignature`'s fixed-types model, but the runtime registry accepts polymorphic impls without changes.
2. [ ] Register `concat` runtime impl in `FUNCTION_IMPLEMENTATIONS`.
3. [ ] Reuse `unify_array_element_types` from the array-literal phase (used in the validate guard).
4. [ ] Help-text update.
5. [ ] Tests in `rust/tests/expr/array_concat_test.rs`.
6. [ ] Reference-guide doc update.

---

## Phase 6 — `array_concat` node

### Behavior

Concatenates two arrays of the same element type into a single array.

**Properties**

- `Element type` — the element type for both inputs and the output.

Note: unlike the expr-level `concat`, the node form does **not** perform cross-element promotion. Both input pins are typed `Array[ElementType]`, so the standard `DataType::can_be_converted_to` rules at wire-time already handle compatibility — wiring an `Array[IVec3]` into an `Array[Vec3]` pin is accepted via the existing array-element promotion path. Doing further unification inside the node would be redundant.

**Input pins**

- `a: Array[ElementType]`
- `b: Array[ElementType]`

**Output pin**

- `out: Array[ElementType]`

**Behavior**

If either input is unconnected, evaluation yields `NetworkResult::None`. This matches `array_at` and `array_len`: missing required inputs propagate as `None` rather than being silently substituted with a default. Users who want "append an optional second tail" can wire an explicit empty-array literal into the unused pin.

### Implementation sketch

New file `rust/src/structure_designer/nodes/array_concat.rs`. The `eval` function:

1. Evaluate both args. If either is `None`, return `EvalOutput::single(NetworkResult::None)`. Errors propagate the usual way.
2. Append `b`'s items to `a`'s items.
3. Return `EvalOutput::single(NetworkResult::Array(combined))`.

Output pin can use `OutputPinDefinition::single_fixed(DataType::Array(Box::new(self.element_type.clone())))` since the node's stored `element_type` already determines the array's element type concretely.

### Phase 6 tests

New file `rust/tests/structure_designer/array_concat_test.rs`:

| Scenario                                              | Expected                      |
| ----------------------------------------------------- | ----------------------------- |
| element_type `Int`, a=`[1,2]`, b=`[3,4]`              | `[1, 2, 3, 4]`                |
| element_type `Int`, a=`[]`, b=`[1, 2]`                | `[1, 2]`                      |
| element_type `Int`, a unconnected, b=`[1, 2]`         | `None`                        |
| element_type `Int`, both unconnected                  | `None`                        |
| element_type `Int`, a=`[]`, b unconnected             | `None`                        |
| element_type `IVec3`, two non-empty arrays            | concatenation                 |
| element_type `Crystal`, two arrays of crystals        | concatenation preserves order |
| Snapshot of node-type definition                      | locked in via insta           |
| Text-format roundtrip                                 | passes                        |
| `.cnnd` roundtrip                                     | passes                        |

### Phase 6 implementation checklist

1. [ ] `rust/src/structure_designer/nodes/array_concat.rs`.
2. [ ] Register in `nodes/mod.rs` and `node_type_registry.rs`.
3. [ ] Tests in `rust/tests/structure_designer/array_concat_test.rs`.
4. [ ] Snapshot + insta review.
5. [ ] Text-format and `.cnnd` roundtrip cases.
6. [ ] Reference-guide entry in `math_programming.md`.

---

## Phase 7 — Append in expr (`append`)

### Syntax

`append(arr, elem)` — function call. The first argument must be `Array[T]` for any `T`; the second argument is any `U` that unifies with `T` under the standard promotion rules. The result is `Array[unify(T, U)]`.

```
append([1, 2], 3)                               // Array[Int],   [1, 2, 3]
append([1, 2], 3.0)                             // Array[Float], promotion across the boundary
append([]Int, 5)                                // Array[Int],   [5]
append([ivec3(1,2,3)], vec3(0.5, 0.5, 0.5))     // Array[Vec3]   (IVec3 promoted)
append(append([1], 2), 3)                       // Array[Int],   [1, 2, 3]   (chains)
```

`append` is the array-element companion to `concat` (which takes two arrays). Wrapping a single element in a literal just to concat — `concat(arr, [elem])` — is verbose enough that a dedicated primitive earns its place.

### Validation

Same approach as `concat` from Phase 5: special-case `name == "append"` in `Expr::Call::validate`, before the registry lookup. Arity must be 2; arg 0 must be `DataType::Array(_)`; arg 1 may be any type. Element type unification reuses `unify_array_element_types`.

```rust
// Inside the special-case for append in Expr::Call validation
let arr_ty = args[0].validate(...)?;
let elem_ty = args[1].validate(...)?;
let arr_elem = match arr_ty {
    DataType::Array(t) => *t,
    other => return Err(format!("append arg 0 must be Array, got {:?}", other)),
};
let unified = unify_array_element_types(&arr_elem, &elem_ty)
    .map_err(|_| format!(
        "append element type {} is incompatible with array element type {}",
        elem_ty, arr_elem
    ))?;
Ok(DataType::Array(Box::new(unified)))
```

Same rationale as `len` and `concat` for not adding `append` to `FUNCTION_SIGNATURES`: the polymorphic argument shape (any `Array[T]` plus any `U` unifiable with `T`) does not fit `FunctionSignature`'s fixed-types model, while the runtime registry accepts polymorphic impls without changes.

### Evaluation

Register `append` in `FUNCTION_IMPLEMENTATIONS` alongside `concat`, `length2`, etc. Argument-evaluation error-propagation is already handled by `Expr::Call::evaluate` — it short-circuits on any `Error` arg before invoking the registered impl — so the impl receives `&[NetworkResult]` whose entries are guaranteed non-`Error`, and only needs to dispatch on `Array` for arg 0 (the latter is defensive — validation already rejected non-array arg 0):

```rust
"append".to_string(),
Box::new(|args: &[NetworkResult]| -> NetworkResult {
    if args.len() != 2 {
        return NetworkResult::Error("append() requires exactly 2 arguments".to_string());
    }
    let mut out = match &args[0] {
        NetworkResult::Array(v) => v.clone(),
        _ => return NetworkResult::Error("append() first arg must be an array".to_string()),
    };
    out.push(args[1].clone());
    NetworkResult::Array(out)
})
```

`None`-propagation rationale matches Phase 5's `concat` and Phase 1's `Expr::Index`: the expr node converts unconnected inputs to error outputs before any `Expr::*` arm runs, and no `Expr::*` arm produces `None`. The `_ =>` fall-through covers genuine type bugs only.

### Help text

Append to the "Array Access" section of the expr-node help text:

```
- `append(arr, elem)` — return a new array with `elem` appended at the end.
  The result element type is the unification of `arr`'s element type and
  `elem`'s type under standard promotion rules (so `append([1,2], 3.0)` is
  `Array[Float]`).
```

### Phase 7 tests

New file `rust/tests/expr/array_append_test.rs`, registered in `rust/tests/expr.rs`.

**Validation:**

| Expression                                       | Variables                | Expected output type |
| ------------------------------------------------ | ------------------------ | -------------------- |
| `append([1, 2], 3)`                              | —                        | `Array[Int]`         |
| `append([1, 2], 3.0)`                            | —                        | `Array[Float]`       |
| `append([]Int, 5)`                               | —                        | `Array[Int]`         |
| `append([ivec3(1,2,3)], vec3(1.0, 2.0, 3.0))`    | —                        | `Array[Vec3]`        |
| `append([[1,2]], [3,4])`                         | —                        | `Array[Array[Int]]`  |
| `append(a, x)`                                   | a: Array[IVec3], x: IVec3 | `Array[IVec3]`      |
| `append([1, 2], ivec3(1,2,3))`                   | —                        | error (incompatible) |
| `append(5, 3)`                                   | —                        | error (non-array)    |
| `append([1, 2])`                                 | —                        | error (arity)        |
| `append([1], 2, 3)`                              | —                        | error (arity)        |

**Evaluation:**

| Expression                            | Expected                                                         |
| ------------------------------------- | ---------------------------------------------------------------- |
| `append([1, 2], 3)`                   | `Array([Int(1), Int(2), Int(3)])`                                |
| `append([]Int, 5)`                    | `Array([Int(5)])`                                                |
| `append(append([1], 2), 3)`           | `Array([Int(1), Int(2), Int(3)])`                                |
| `len(append([1, 2], 3))`              | `Int(3)` (composes with `len`)                                   |
| `append([1, 2], 3)[2]`                | `Int(3)` (composes with indexing)                                |
| `concat(append([1], 2), [3, 4])`      | `Array([Int(1), Int(2), Int(3), Int(4)])` (composes with `concat`) |

### Phase 7 implementation checklist

1. [ ] Add `name == "append"` guard at the top of `Expr::Call::validate` (`rust/src/expr/expr.rs`), alongside the `len` and `concat` guards.
2. [ ] Register `append` runtime impl in `FUNCTION_IMPLEMENTATIONS` (`rust/src/expr/validation.rs`).
3. [ ] Reuse `unify_array_element_types` in the validate guard.
4. [ ] Help-text update in `rust/src/structure_designer/nodes/expr.rs`.
5. [ ] Tests in `rust/tests/expr/array_append_test.rs`.
6. [ ] `cd rust && cargo fmt && cargo clippy && cargo test`.
7. [ ] Reference-guide doc update.

---

## Phase 8 — `array_append` node

### Behavior

Appends one element to the end of an array.

**Properties**

- `Element type` — both the array's element type and the appended element's type. (Same convention as `array_at` / `array_len` / `array_concat`.)

**Input pins**

- `array: Array[ElementType]`
- `element: ElementType`

**Output pin**

- `out: Array[ElementType]` via `OutputPinDefinition::single_fixed(DataType::Array(Box::new(self.element_type.clone())))`.

**Behavior**

If either input is unconnected, evaluation yields `NetworkResult::None`. This matches `array_at`, `array_len`, and `array_concat` — missing required inputs propagate as `None` rather than being silently substituted with a default.

Like `array_concat`, the node form does **not** perform cross-element promotion. Both pins are typed against `element_type`, so wire-time `DataType::can_be_converted_to` rules already handle compatible types (e.g. an `Array[IVec3]` value flowing into an `Array[Vec3]` pin via the existing array-element promotion path). Doing further unification inside the node would be redundant.

### Implementation sketch

New file `rust/src/structure_designer/nodes/array_append.rs`. Mirrors `array_concat.rs` (Phase 6) with the second input pin typed as `ElementType` instead of `Array[ElementType]`. The `eval` function:

1. Evaluate both args. If either is `None`, return `EvalOutput::single(NetworkResult::None)`. Errors propagate the usual way.
2. Clone `array`'s items, push `element`.
3. Return `EvalOutput::single(NetworkResult::Array(combined))`.

### Phase 8 tests

New file `rust/tests/structure_designer/array_append_test.rs`, registered in `rust/tests/structure_designer.rs`.

| Scenario                                                       | Expected                          |
| -------------------------------------------------------------- | --------------------------------- |
| element_type `Int`, array=`[1,2]`, element=`3`                 | `[1, 2, 3]`                       |
| element_type `Int`, array=`[]`, element=`5`                    | `[5]`                             |
| element_type `IVec3`, array of one ivec3 + ivec3               | length-2 array, order preserved   |
| element_type `Crystal`, array of crystals + crystal            | append preserves order            |
| Unconnected `array` pin                                        | `None`                            |
| Unconnected `element` pin                                      | `None`                            |
| Both unconnected                                               | `None`                            |

Plus:

- **Snapshot test** — add `array_append` to `rust/tests/structure_designer/node_snapshot_test.rs`.
- **Text-format roundtrip** — verify `element_type` property and pin wiring serialize/deserialize via the existing `text_format_test.rs` pattern. Add a case for `array_append` with `element_type: Int` and one with `element_type: IVec3`.
- **`.cnnd` roundtrip** — fixture under `rust/tests/fixtures/` containing an `array_append` node; assert load → save → reload is stable.

### Phase 8 implementation checklist

1. [ ] `rust/src/structure_designer/nodes/array_append.rs`.
2. [ ] Register in `rust/src/structure_designer/nodes/mod.rs`.
3. [ ] Register in `rust/src/structure_designer/node_type_registry.rs::create_built_in_node_types()`.
4. [ ] Tests in `rust/tests/structure_designer/array_append_test.rs`.
5. [ ] Snapshot in `node_snapshot_test.rs` + `cargo insta review`.
6. [ ] Text-format and `.cnnd` roundtrip cases.
7. [ ] Reference-guide entry for `array_append` in `doc/reference_guide/nodes/math_programming.md` (next to `array_concat`).
8. [ ] No FRB regen (no API surface change beyond new node type).

---

## Reference-guide updates (cumulative)

Each phase touches `doc/reference_guide/nodes/math_programming.md`:

- **Phases 1 / 3 / 5 / 7** add to the "Array Access" subsection in the expr-node section (extending the existing "Array Literals" section).
- **Phases 2, 4, 6, 8** add four new top-level entries — `array_at`, `array_len`, `array_concat`, `array_append` — between `sequence` and `map` to keep array-related nodes co-located.

Snippet for the expr-node "Array Access" subsection (final form after all four expr phases):

```markdown
**Array Access:**

- `arr[i]` — element access; `i` is an Int expression. Out-of-bounds is an
  evaluation error. Chain for nested arrays: `arr[i][j]`.
- `len(arr)` — number of elements; returns Int. Works on any `Array[T]`.
- `concat(a, b)` — concatenate two arrays. The result element type is the
  unification of the two element types under standard promotion rules.
- `append(arr, elem)` — return a new array with `elem` appended at the end.
  The result element type is the unification of `arr`'s element type and
  `elem`'s type under standard promotion rules.
```

## Out of scope (recap)

- `repeat(value, count)`.
- Slicing.
- `filter`, `reduce`, `any`, `all`.
- `index_of` / `contains`.
- Negative indexing or wrap/clamp out-of-bounds modes.
- Multi-argument `concat(a, b, c, ...)`.
- `prepend(arr, elem)` / `array_prepend` and `insert(arr, i, elem)` / `array_insert`. Workarounds (`concat([elem], arr)`, two `concat` calls) are tolerable for now. Revisit if real usage shows otherwise.
- A separate "any array" abstract type that would let `array_len` / `array_concat` / `array_append` skip the `element_type` property. The element-type property matches `sequence`'s convention; introducing an abstract array type just to remove one property is not worth the type-system complexity.

## Open question

**`array_concat` two-input vs. configurable-N.** `sequence` already takes a configurable count of element pins. We could similarly let `array_concat` take N input arrays. Recommendation: ship two-input first (matches the expr `concat(a, b)` form, simpler UI). If users start chaining 3+ `array_concat` nodes, revisit.
