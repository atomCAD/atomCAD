# Math and programming nodes

← Back to [Reference Guide hub](../../atomCAD_reference_guide.md)

## int

Outputs an integer value.

![](../../atomCAD_images/int.png)

## float

Outputs a float value. 

![](../../atomCAD_images/float.png)

## ivec2

Outputs an IVec2 value.

![](../../atomCAD_images/ivec2.png)

## ivec3

Outputs an IVec3 value.

![](../../atomCAD_images/ivec3.png)

## vec2

Outputs a Vec2 value.

![](../../atomCAD_images/vec2.png)

## vec3

Outputs a Vec3 value.

![](../../atomCAD_images/vec3.png)

## imat3_rows

Outputs an `IMat3` (3×3 integer matrix) built from three row vectors.

**Input pins** (all optional, default to identity rows)

- `a: IVec3` — row 0 (default `(1, 0, 0)`)
- `b: IVec3` — row 1 (default `(0, 1, 0)`)
- `c: IVec3` — row 2 (default `(0, 0, 1)`)

**Stored property**

- 3×3 integer grid that supplies the row defaults when an input pin is unwired. Default is identity, so an unwired `imat3_rows` is the identity constant.

The subtitle shows `det = N` for the resolved matrix, or `det = ?` when any row is wired (the determinant cannot be precomputed).

## imat3_cols

Same as `imat3_rows` but the three input vectors are interpreted as **columns** instead of rows: `m[i][j] = col_j[i]`.

## imat3_diag

Outputs a diagonal `IMat3` from a single `IVec3`.

**Input pin**

- `v: IVec3` (optional, default `(1, 1, 1)`)

The result is `diag(v.x, v.y, v.z)`. This is the node to use when wiring an `IMat3` input pin (for example `supercell.matrix`) for the simple axis-aligned case.

## mat3_rows

Floating-point counterpart of `imat3_rows`: outputs a `Mat3` (3×3 float matrix) from three `Vec3` row vectors. Defaults are the float identity rows.

## mat3_cols

Floating-point counterpart of `imat3_cols`: three `Vec3` columns → `Mat3`.

## mat3_diag

Floating-point counterpart of `imat3_diag`: `Vec3 → Mat3` (`diag(v.x, v.y, v.z)`).

## imat2_rows

Outputs an `IMat2` (2×2 integer matrix, row-major) built from two row vectors. The `IMat2` type exists mainly to describe an in-plane **superlattice** for the [`plane_tiling_vectors`](#plane_tiling_vectors) node; there is no floating-point `Mat2` counterpart.

**Input pins** (both optional, default to identity rows)

- `a: IVec2` — row 0 (default `(1, 0)`)
- `b: IVec2` — row 1 (default `(0, 1)`)

**Stored property**

- A 2×2 integer grid that supplies the row defaults when an input pin is unwired. Default is identity, so an unwired `imat2_rows` is the identity (the conventional `1×1` cell).

## imat2_cols

Same as `imat2_rows` but the two input vectors are interpreted as **columns** instead of rows.

## imat2_diag

Outputs a diagonal `IMat2` from a single `IVec2`: the result is `diag(v.x, v.y)`.

**Input pin**

- `v: IVec2` (optional, default `(1, 1)`)

## plane_tiling_vectors

Turns a Miller-indexed `DrawingPlane` plus a 2×2 integer superlattice into the `Array[IVec3]` tiling vectors consumed by [`patch_build`](./atomic.md#patch_build)'s `tiling_vectors` pin. This is the ergonomic way to produce surface-patch tiling vectors without hand-solving the in-plane crystallography. (2D-surface case; 1D-edge and 3D-twin patches feed `tiling_vectors` directly.)

The plane supplies the in-plane lattice basis vectors `u_axis`, `v_axis`. Each **row** of the superlattice gives one tiling vector as an integer combination of `u` and `v` (row 0 = `a`, row 1 = `b`):

| Superlattice | Rows | Pattern |
|---|---|---|
| `1×1` | `(1,0)`, `(0,1)` | conventional cell (identity) |
| `n×m` (diagonal) | `(n,0)`, `(0,m)` | rectangular supercell |
| √3×√3 R30° | `(2,1)`, `(-1,1)` | e.g. Si(111) |
| c(2×2) | `(1,1)`, `(1,-1)` | centred |

**Input pins**

- `plane: DrawingPlane` — supplies the in-plane lattice basis. Must be built from the same crystal (`UnitCellStruct`) you later pass as `patch_build`'s `lattice`.
- `superlattice: IMat2` (optional) — overrides the node's stored 2×2 matrix when wired (typically from an `imat2_*` node).

**Stored property**

- A 2×2 integer superlattice grid (rows `a`, `b`), editable inline. Default identity. The subtitle shows `det = N` (or `det = ?` when the `superlattice` pin is wired).

**Output (single pin)**

- `Array[IVec3]` — the two tiling vectors. Degenerate (linearly dependent) rows are not rejected here; `patch_build`'s linear-independence check reports them.

## bool

Outputs a Bool value (`true` or `false`).

## string

Outputs a String value.

![](../../atomCAD_images/string.png)

## expr

![](../../atomCAD_images/expr_node_props.png)

You can type in a mathematical expression and it will be evaluated on its output pin.
The input pins can be dynamically added on the node editor panel, you can select the name and data type of the input parameters.

The expr node supports scalar arithmetic, vector operations, conditional expressions, and a comprehensive set of built-in mathematical functions.

**Expression Language Features:**

**Literals**

- integer literals (e.g., `42`, `-10`)
- floating point literals (e.g., `3.14`, `1.5e-3`, `.5`)
- boolean values (`true`, `false`)

**Arithmetic Operators:**

- `+` - Addition (also concatenates two `String` values; see *String Template Literals* below)
- `-` - Subtraction  
- `*` - Multiplication
- `/` - Division
- `%` - Modulo (integer remainder, only works on integers)
- `^` - Exponentiation
- `+x`, `-x` - Unary plus/minus

**Comparison Operators:**
- `==` - Equality
- `!=` - Inequality
- `<` - Less than
- `<=` - Less than or equal
- `>` - Greater than
- `>=` - Greater than or equal

**Logical Operators:**
- `&&` - Logical AND
- `||` - Logical OR
- `!` - Logical NOT

**Conditional Expressions:**

```
if condition then value1 else value2
```
Example: `if x > 0 then 1 else -1`

**Vector Operations:**

*Vector Constructors:*

- `vec2(x, y)` - Create 2D float vector
- `vec3(x, y, z)` - Create 3D float vector
- `ivec2(x, y)` - Create 2D integer vector
- `ivec3(x, y, z)` - Create 3D integer vector

*Member Access:*
- `vector.x`, `vector.y`, `vector.z` - Access vector components

*Vector Arithmetic:*
- Vector + Vector (component-wise)
- Vector - Vector (component-wise)
- Vector * Vector (component-wise)
- Vector * Scalar (scaling)
- Scalar * Vector (scaling)
- Vector / Scalar (scaling)

*Type Promotion:*

Integers and integer vectors automatically promote to floats and float vectors when mixed with floats.

**Vector Math Functions:**
- `length2(vec2)` - Calculate 2D vector magnitude
- `length3(vec3)` - Calculate 3D vector magnitude
- `normalize2(vec2)` - Normalize 2D vector to unit length
- `normalize3(vec3)` - Normalize 3D vector to unit length
- `dot2(vec2, vec2)` - 2D dot product
- `dot3(vec3, vec3)` - 3D dot product
- `cross(vec3, vec3)` - 3D cross product
- `distance2(vec2, vec2)` - Distance between 2D points
- `distance3(vec3, vec3)` - Distance between 3D points

**Integer Vector Math Functions:**

- `idot2(ivec2, ivec2)` - 2D integer dot product (returns int)
- `idot3(ivec3, ivec3)` - 3D integer dot product (returns int)
- `icross(ivec3, ivec3)` - 3D integer cross product (returns ivec3)

**Matrix Operations:**

The `Mat3` and `IMat3` types are 3×3 matrices, stored row-major (`m[i][j]` is row `i`, column `j`).

*Matrix Constructors:*

- `mat3_rows(a, b, c)` / `imat3_rows(a, b, c)` — build a matrix from three row vectors.
- `mat3_cols(a, b, c)` / `imat3_cols(a, b, c)` — build a matrix from three column vectors.
- `mat3_diag(v)` / `imat3_diag(v)` — diagonal matrix from a single vector.

*Arithmetic:*

- `Mat3 + Mat3`, `Mat3 - Mat3` — component-wise addition / subtraction (and the `IMat3` analogues).
- `Mat3 * Mat3` — standard matrix product.
- `Mat3 * Vec3` — matrix × vector (row-major: `result[i] = Σ_j m[i][j] · v[j]`). The reverse `Vec3 * Mat3` is rejected.
- The integer analogues `IMat3 * IMat3` / `IMat3 * IVec3` work identically. `IVec3` and `IMat3` operands promote to their float counterparts when mixed with floats, just like the scalar/vector promotion rule.

*Member Access:*

- `m.m00`, `m.m01`, … `m.m22` — access the nine entries of a `Mat3` (returns `Float`) or `IMat3` (returns `Int`). `.mIJ` is row `I`, column `J`.

*Matrix Functions:*

- `transpose3(m)` / `itranspose3(m)` — transpose.
- `det3(m)` — determinant (`Mat3 → Float`).
- `idet3(m)` — determinant (`IMat3 → Int`).
- `inv3(m)` — inverse (`Mat3 → Mat3`); returns an error for a singular matrix (`|det| < 1e-12`). No integer counterpart — an integer inverse would need a rational type.
- `to_mat3(m)` / `to_imat3(m)` — explicit `IMat3 ↔ Mat3` casts (the float→int direction truncates).

**Array Literals:**

- `[expr1, expr2, ..., exprN]` — non-empty array literal. The element type is inferred from the elements using the same promotion rules as other expressions (e.g. mixing `Int` and `Float` produces `Array[Float]`, mixing `IVec3` and `Vec3` produces `Array[Vec3]`). Trailing commas are not allowed. Nesting is supported (`[[1, 2], [3, 4]]`).
- `[]TypeExpr` — empty array of the given element type. The leading `[]` marks the literal as empty; the trailing `TypeExpr` declares the element type. `TypeExpr` is either a concrete type name (e.g. `Int`, `IVec3`, `Structure`, `Crystal`) or `[InnerTypeExpr]` for nested array types.

Type-name identifiers are only interpreted as types in the position immediately after `[]`. Everywhere else (including inside an element list `[a, b, c]`), an identifier resolves as a normal expression — so naming a parameter after a type (`structure: Structure`) is safe and `[structure]` is a 1-element array containing that parameter.

The abstract supertypes (`HasAtoms`, `HasStructure`, `HasFreeLinOps`), the `None` sentinel, and function types are not accepted as element types.

Examples:

```
[1, 2, 3]                           // Array[Int]
[1, 2.0, 3]                         // Array[Float]  (Int promoted)
[true, false, true]                 // Array[Bool]
[ivec3(1,2,3), ivec3(4,5,6)]        // Array[IVec3]
[ivec3(1,2,3), vec3(0.5,0.5,0.5)]   // Array[Vec3]   (IVec3 promoted)
[a, b, c]                           // Array[T] where T is the unified type of a, b, c
[a*2 + 1, a*3 + 1, a*4 + 1]         // Array[Int]    (assuming a: Int)
[]IVec3                             // empty Array[IVec3]
[]Structure                         // empty Array[Structure]
[][IVec3]                           // empty Array[Array[IVec3]]
[][[Int]]                           // empty Array[Array[Array[Int]]]
[[1, 2], [3, 4]]                    // Array[Array[Int]]
[[]Int, []Int]                      // 2-element Array[Array[Int]], each inner empty
```

A common use is constructing an `Array[IVec3]` literal of defect positions inline, which can then be fed to a downstream node consuming an array. An `expr` node with zero parameters can be used as a pure literal node for this purpose.

**Array Access:**

- `arr[i]` — element access; `i` is an `Int` expression. Indexing has the same precedence as function call and member access (highest), so it chains naturally:
  - `arr[i].x` — index then read a member.
  - `arr[i][j]` — chain for nested arrays.
  - `[1, 2, 3][0]` — index a literal.
- Out-of-bounds (`i < 0` or `i` past the end) produces an evaluation error. The index must be an `Int`; `Bool` and `Float` are rejected at validation time.
- `len(arr)` — number of elements in `arr`. Returns `Int`. Works on any `Array[T]`, including empty arrays (`len([]Int)` is `0`).
- `concat(a, b)` — concatenate two arrays. The result element type is the unification of the two element types under the standard promotion rules (e.g. `concat([]Int, [1,2,3])` is `[1,2,3]`; `concat([1,2], [3.5])` is `Array[Float]`). For more than two arrays, nest: `concat(a, concat(b, c))`.
- `append(arr, elem)` — return a new array with `elem` appended at the end. The result element type is the unification of `arr`'s element type and `elem`'s type under the standard promotion rules (so `append([1,2], 3.5)` is `Array[Float]`). Chain calls to append multiple elements: `append(append([1], 2), 3)`.

**Record Literals and Field Access:**

The expression language can construct and read [record values](#record-types) inline.

- `{name1: expr1, name2: expr2, ..., nameN: exprN}` — non-empty record literal. Field names must be distinct; trailing commas are not allowed (consistent with array literals). Each value expression is type-checked independently; the literal's type is an inline anonymous record `{name1: T1, name2: T2, ..., nameN: TN}` whose fields participate in structural subtyping like any other record type. There is no type name on the literal, so a literal flowing into a pin declared `Record(Foo)` matches by structural compatibility — the anonymous schema must be width-compatible with `Foo`'s schema (extras allowed; missing fields rejected).
- `r.<field>` — field access on a record. If `r`'s static type is a record type, `.<field>` is resolved against that record's schema and produces the field's declared type. The receiver type disambiguates record fields from vector / matrix members: a record with a field named `x` does **not** conflict with the `Vec3.x` rule, because the parser checks the receiver type and only falls back to the vector/matrix rules when the receiver is not a record.
- **Type expressions** in `expr` parameter type positions accept record types in two forms:
  - A bare identifier — `Foo` — resolves first as a built-in type, then as a named record def in the project, then as an error if neither matches. So an `expr` parameter typed `Foo` (where `Foo` is a record def) accepts `Record(Foo)` values.
  - An inline `{x: Int, y: Int}` literal in type position produces an anonymous record type. The same type-identifier scoping rule that gates `[]TypeExpr` applies: type-name identifiers are only interpreted as types in type-position contexts, so `{x: Int}` inside an element-list expression is a value literal, not a type expression.

Examples:

```
{x: 1, y: 2}                        // anonymous Record({x: Int, y: Int})
{x: 1, y: 2.0}                      // anonymous Record({x: Int, y: Float})
{x: 1, y: 2, label: "p"}            // mixed-type record value
{outer: {inner: 1}}.outer.inner     // nested record literal + chained access
{x: 1, y: 2}.x + 1                  // field access in arithmetic (Int)
[{x: 1, y: 2}, {x: 3, y: 4}]        // array of records (anonymous schema)
```

**String Template Literals:**

Backtick-delimited literals (`` `…` ``) build a `String` value with optional inline interpolation. They cover both the pure-string case (no interpolations) and string composition (one or more interpolations), which is the easiest path from "I have these record fields" to "I have a per-variant filename." The motivating use case is systematic file-path assembly for batch export — e.g. mapping a `product` stream of variant records into an `export_xyz.file_name` pin.

- `` `text` `` — a literal `String`. Empty `` `` `` is the empty string.
- `` `${expr}` `` — interpolation. `expr` is the full expression grammar — arithmetic, member access, conditionals, function calls, record literals, etc. all work inside `${…}`.
- Adjacent interpolations need no separator: `` `${a}${b}` `` concatenates two values directly.
- A bare `$` not followed by `{` is literal, so `` `cost: $5` `` works without escaping.

**Stringification rules.** Each `${expr}` is validated to produce one of `String`, `Int`, `Float`, or `Bool`; anything else (including `Vec3`, `Record`, `Array`, `Iter`, the phase types) is rejected at validation time. To stringify a compound type, pull components out (`` `${v.x}_${v.y}_${v.z}` ``).

| Type     | Format                                                                  | Examples                       |
|----------|-------------------------------------------------------------------------|--------------------------------|
| `String` | passthrough, no quotes                                                  | `hello`                        |
| `Int`    | decimal, no padding                                                     | `42`, `-7`                     |
| `Float`  | Rust default `Display` — full precision, trims trailing zeros            | `1`, `3.14`, `0.1`, `-7.5`     |
| `Bool`   | `true` / `false`                                                        | `true`                         |

Non-finite Floats (`NaN`, `+inf`, `-inf`) are rejected at evaluation time — they would produce filesystem-hostile filenames like `dose_NaN.xyz`.

**Escapes.** `\` `\\ \$ \n \t \r` are recognized inside template text. Use `\$` to write a literal `${...}` without triggering interpolation; use `` \` `` to embed a backtick. Raw newlines inside the literal are also allowed (the body can span multiple lines).

**`+` for string concatenation.** Two `String` values can be glued with `+` — `` `hello ` + `world` `` produces `"hello world"`. The operator is **strict**: only `String + String` is accepted. Mixing in `Int`, `Float`, `Bool`, or any other type is rejected at validation time; for mixed-type composition use a template literal (`` `count: ${n}` ``), which auto-stringifies `Int`/`Float`/`Bool` per the *Stringification rules* table above. `+` is left-associative, so `` `a` + `b` + `c` `` is `"abc"`. Largely redundant with templates, but provided for users who prefer the operator form.

**Out of scope.** Nested template literals are rejected at lex time — a backtick inside `${…}` is an error. Use adjacent interpolations (`` `prefix-${x}-suffix` ``) instead. Plain `"…"` string literals and format specifiers (zero-padding, width, precision) are not provided; templates compose strings inline well enough that these have not been needed.

Examples:

```
`hello world`                                    // "hello world"
`${x}`                                           // String value of x
`prefix-${x}`                                    // "prefix-" + str(x)
`${a}${b}`                                       // adjacent interpolations
`${variant.species}_size${variant.size}.xyz`     // record-field interpolation
`cost: $5`                                       // literal "$" — '$' not followed by '{' is literal
`literal \${x}`                                  // literal "${x}" — \$ disables interpolation
`a backtick: \``                                 // literal "a backtick: `"
`line1\nline2`                                   // embedded newline (escape)
`line1
line2`                                           // also two lines — raw newline is allowed
`${if x > 0 then 1 else 2}`                      // any expr inside ${...}
```

**Mathematical Functions:**

- `sin(x)`, `cos(x)`, `tan(x)` - Trigonometric functions
- `sqrt(x)` - Square root
- `abs(x)` - Absolute value (float)
- `abs_int(x)` - Absolute value (integer)
- `floor(x)`, `ceil(x)`, `round(x)` - Rounding functions

**Operator Precedence (highest to lowest):**
1. Function calls, member access, parentheses
2. Unary operators (`+`, `-`, `!`)
3. Exponentiation (`^`) - right associative
4. Multiplication, division, modulo (`*`, `/`, `%`)
5. Addition, subtraction (`+`, `-`)
6. Comparison operators (`<`, `<=`, `>`, `>=`)
7. Equality operators (`==`, `!=`)
8. Logical AND (`&&`)
9. Logical OR (`||`)
10. Conditional expressions (`if-then-else`)

**Example Expressions:**
```
2 * x + 1                           // Simple arithmetic
x % 2 == 0                          // Check if x is even (modulo)
if x % 2 > 0 then -1 else 1         // Conditional with modulo
vec3(1, 2, 3) * 2.0                // Vector scaling  
length3(vec3(3, 4, 0))              // Vector length (returns 5.0)
if x > 0 then sqrt(x) else 0       // Conditional with function
dot3(normalize3(a), normalize3(b))  // Normalized dot product
sin(3.14159 / 4) * 2               // Trigonometry
vec2(x, y).x + vec2(z, w).y        // Member access
distance3(vec3(0,0,0), vec3(1,1,1)) // 3D distance
```

## Iterator types

An **iterator type** `Iter[T]` represents a lazily-evaluated stream of `T` values. Iterators travel along wires the same way arrays do, but downstream nodes pull elements one at a time rather than allocating the full payload upfront. This is the backbone of the `range → map → filter → fold` pipeline: a million-element `range` followed by a `map` and a `fold` keeps only one element alive at a time, regardless of stream length.

`range`, `map`, `filter`, and `product` are the four iterator producers — their output pins are `Iter[T]`, not `Array[T]`. `fold` is an iterator consumer; it walks the stream to a single accumulator value. `collect` is the explicit bridge from `Iter[T]` back to `Array[T]`.

**Implicit conversions**

- `Array[T] → Iter[T]` is allowed and applied automatically at wire time. The array is wrapped as a stream that yields each element in order; element-level conversions (`Int → Float`, `IVec3 → Vec3`, …) run eagerly at wrap time, just like the `Array[S] → Array[T]` rule. A literal `[1, 2, 3]` flowing into a `fold.xs` pin keeps working with no edit.
- `T → Iter[T]` is the single-element broadcast: a scalar value flowing into an `Iter[T]` pin is wrapped as a one-element stream. Mirrors the existing `T → Array[T]` rule.
- `Iter[T] → Iter[T]` is the identity passthrough — the walker is handed through unchanged.

**Explicit `collect`**

The reverse direction `Iter[T] → Array[T]` is **not** an implicit conversion: turning a fused stream into a fully materialized array is exactly the operation iterators are designed to avoid, so the conversion is rejected at wire-time validation. To feed an iterator into an array consumer (`array_at`, `array_len`, `array_concat`, `array_append`, `sequence`, or any other pin declared `Array[T]`), insert a `collect` node between them. The error message on the rejected wire points at `collect`.

**Restrictions**

- Lazy element conversion across iterator boundaries (`Iter[S] → Iter[T]` with `S ≠ T`) is not implicit; insert a `map` with the conversion, or `collect` and rebuild explicitly.
- `Iter[T]` cannot appear as a record field type, and cannot be carried across a higher-order-function body's boundary as a capture (the same walker would be reused across per-iteration invocations and get corrupted). Both restrictions point users at `collect`.
- Iterator-typed top-level parameters (CLI/API-bound) are not accepted; pass an `Array[T]` instead.

**Display**

A node whose displayed pin output is `Iter[T]` produces **no** viewport output — materialization is the consumer's job, and the iterator is, by design, potentially unbounded or expensive to drain. To inspect elements of a stream, wire it into a `collect` node (with an optional limit) and display the `collect`. The `collect` node reports the live element count or "stopped at limit N" in its node-graph subtitle.

## range

Produces a lazily-evaluated stream of integers (`Iter[Int]`) starting from an integer value and having a specified step between them. The number of integers in the stream is set by the `count` property. The stream materializes one element at a time when downstream nodes pull from it; chaining `range → map → fold` keeps live-element memory at O(1) regardless of `count`. To consume the stream as an `Array[Int]`, insert a `collect` node after `range` — the `Iter[Int] → Array[Int]` boundary is rejected at validation, with an error pointing at `collect`.

![](../../atomCAD_images/range_node_props.png)

## sequence

Collects a fixed number of inputs into an ordered array. Use `sequence` when you want to build an array from inputs that come from different upstream nodes and you care about their order, or when you want each element to appear on its own labeled pin in the network — `range`, `map`, `filter`, and `product` produce iterator streams from rules, but `sequence` lets you wire up the elements explicitly one at a time, and the result is an `Array[T]` rather than an `Iter[T]`.

## array_at

Reads one element from an array at a given integer index. The expression-language equivalent is `arr[i]`.

**Properties**

- `Element type` — the element type of the input array (and of the output). All array element types accepted by `sequence` are accepted here.

**Input pins**

- `array: Array[ElementType]` — the array to read from.
- `index: Int` — the zero-based index.

**Behavior**

If either input is unconnected, the output is `None` (propagates as a missing-input). Otherwise the node returns the element at `index`. An `index < 0` or `index >= len(array)` produces an evaluation error of the form `array index {i} out of bounds for array of length {n}`.

For nested arrays, chain two `array_at` nodes (`arr[i][j]` becomes two nodes connected in series).

## array_len

Returns the length of the input array as an `Int`. The expression-language equivalent is `len(arr)`.

**Properties**

- `Element type` — the element type of the input array. The output is always `Int` regardless. This property is required because the input pin must be typed; pick the element type that matches the array you are wiring in.

**Input pins**

- `array: Array[ElementType]` — the array to measure.

**Behavior**

If `array` is unconnected, the output is `None` (propagates as a missing-input). Otherwise the node returns the number of elements in the array as an `Int`. Empty arrays produce `Int(0)`.

![TODO(image): a `sequence` node configured with element type Blueprint and three input pins, with three different geometry nodes wired into pins 0, 1, 2](TODO)

**Properties**

- `Element type` — the type of every input pin and of the output array's elements (e.g. `Int`, `Blueprint`, `Crystal`, …). All input pins share this type.
- `Count` — number of input pins (minimum 1). Each pin is named by its index (`0`, `1`, `2`, …) and the output is `[ElementType]` with elements in pin-index order.

**Behavior**

The output is the array of values from connected pins, in pin-index order. Unconnected pins are skipped (they do not contribute a `None` element). For element-typed pins, each pin can also accept array-typed input thanks to the standard array conventions, but typically each pin carries a single value.

This node is also how the `Display array outputs` workflow is built up by hand: feed several outputs you want to view side-by-side into a `sequence` node, mark its output pin as displayed, and the array's elements render together in the viewport.

## array_concat

Concatenates two arrays of the same element type into a single array. The expression-language equivalent is `concat(a, b)`.

**Properties**

- `Element type` — the element type shared by both inputs and the output. Unlike the expression-level `concat`, the node form does not perform cross-element promotion: both input pins are typed `Array[ElementType]`, so the standard wire-time array-element conversion rules already handle compatibility (e.g. wiring an `Array[IVec3]` into an `Array[Vec3]` pin promotes element-wise).

**Input pins**

- `a: Array[ElementType]` — left array.
- `b: Array[ElementType]` — right array, appended after `a`.

**Behavior**

If either input is unconnected, the output is `None` (propagates as a missing-input). Otherwise the node returns a new array containing every element of `a` followed by every element of `b`, preserving order. Empty arrays are handled with no special case: `concat([], [1, 2])` is `[1, 2]`.

To concatenate three or more arrays, chain `array_concat` nodes (e.g. wire `array_concat(a, b)` into the `a` pin of a second `array_concat` whose `b` pin is `c`).

## array_append

Appends one element to the end of an array, returning a new array. The expression-language equivalent is `append(arr, elem)`.

**Properties**

- `Element type` — the element type of the input array and of the appended element. Unlike the expression-level `append`, the node form does not perform cross-element promotion: the element pin is typed `ElementType` and the array pin is typed `Array[ElementType]`, so the standard wire-time conversion rules already handle compatibility (e.g. wiring an `Array[IVec3]` into an `Array[Vec3]` pin promotes element-wise).

**Input pins**

- `array: Array[ElementType]` — the array to extend.
- `element: ElementType` — the element to append.

**Behavior**

If either input is unconnected, the output is `None` (propagates as a missing-input). Otherwise the node returns a new array containing every element of `array` followed by `element`, preserving order. Appending to an empty array produces a length-1 array.

To append multiple elements, chain `array_append` nodes (or wire an `array_concat` node when the right-hand operand is itself an array).

## collect

Materializes a lazy iterator (`Iter[T]`) into an array (`Array[T]`) by exhausting the stream. This is the explicit escape hatch when a downstream array consumer really does want the whole vector, and — because `Iter[T]` pins are not displayable in their own right — also the place where you ask atomCAD to *show* you elements of a stream. Iterators are produced by the stream-fusing nodes `range`, `map`, `filter`, and `product`; an `Array[T]` source wired into `collect.iter` is also accepted thanks to the implicit `[T] → Iter[T]` wire conversion (in which case `collect` is a no-op pass-through).

**Properties**

- `Element type` — the element type T. Drives both the iterator-input pin (`Iter[T]`) and the array-output pin (`Array[T]`).
- `Limit elements` (checkbox + spinbox, optional) — when checked, caps the number of elements collected. Default 100 on first check. When the cap is reached, `collect` stops pulling from the walker and the resulting array contains exactly that many elements. When unchecked, `collect` exhausts the stream.

**Input pins**

- `iter: Iter[ElementType]` — the iterator to drain. Accepts an `Array[ElementType]` source via the implicit `[T] → Iter[T]` wire conversion (eagerly wrapped) and a single `ElementType` value via the single-element broadcast rule.
- `limit: Int` (optional) — when wired, **overrides** the stored `Limit elements` setting at evaluation time. Use this to drive the cap from a parameter or computed value (e.g. a `slider` upstream). When the pin is disconnected or evaluates to `None`, the stored value (if any) takes effect. Negative values produce an evaluation error.

**Behavior**

If `iter` is unconnected the output is `None` (propagates as a missing input). Otherwise `collect` pulls elements from the walker until it ends *or* the effective limit is reached, accumulating them into a new array in iteration order. An iterator that yields an `Error` value mid-stream causes `collect` to abort and propagate that error; subsequent elements are not pulled.

The node's pin subtitle reports the materialization outcome — `(N elements)` when the walker exhausted, or `(stopped at limit N)` when the cap was reached with more elements still pending.

Without a limit there is no built-in size cap. If you wire a 10⁹-element iterator into `collect` with no limit you will run out of memory — that is the contract: `collect` is the explicit, expensive step that turns a fused stream back into a fully materialized array.

## Higher-order function nodes (`map`, `filter`, `fold`, `foreach`)

`map`, `filter`, `fold`, and `foreach` are the four **higher-order function** (HOF) nodes — each one walks a stream of values and runs a per-element body on every element. Unlike a regular node, an HOF has an **inline body region** inside the node itself: a small editable canvas with its own nodes and wires that defines the per-element computation. The inline body is the default way to author that computation.

Each HOF *also* exposes an optional **`f` input pin**: wire a `Function` value (typically a [`closure`](#closure) node's output) into it and that function drives the HOF instead of its inline body. When `f` is connected the inline body is hidden in the editor and ignored at evaluation; disconnect `f` and the inline body returns. This is what lets one authored body be reused across several HOFs. `filter`, `fold`, and `foreach` accept exact-arity `f` sources; `map.f` additionally accepts **any** `Function` whose parameter list starts with the input element type and auto-partializes the excess parameters (the source's tail becomes part of the streamed output element type). See *[Function values and closures](#function-values-and-closures)* below.

A `map` node placed in a network looks like this — the rectangle in the middle is the body region:

```
┌── map ──────────────────────────────────────────────┐
│ xs ●─────────┐                       ┌── ● Iter[U]  │   ← external pins (outside the body)
│              │                       │              │
│              ▼                       ▲              │
│        ┌─────────────────────────────┐              │
│        │                             │              │
│ element●─── [ + 1 ] ────────────●result              │   ← inner pins (inside the body)
│        │                             │              │
│        └─────────────────────────────┘              │
│              (translucent body region)              │
└─────────────────────────────────────────────────────┘
```

Every HOF body has two kinds of inside-facing pins:

- **Zone-input pins** (inner-left, facing into the body): sources that supply per-iteration values to body nodes. `map` / `filter` / `foreach` have one — `element` — the current iteration value. `fold` has two — `acc` (the running accumulator) and `element`.
- **Zone-output pin** (inner-right, facing into the body): the destination that receives the body's per-iteration return value. `map`'s is `result` (the transformed element), `filter`'s is `result` (the `Bool` predicate decision), `fold`'s is `new_acc` (the next accumulator), `foreach`'s is `out` (whose value is discarded).

You build the body by clicking into the body region (which makes it the *active scope*) and adding nodes there the same way you add nodes to a top-level network — right-click in the body, or drag a wire from a pin and drop on body empty space. Every node you add lives in **that body's scope**, with its own selection, undo, and copy/paste set. Wires between body nodes work like ordinary wires. The body region grows automatically as you add content and can also be dragged larger from its bottom-right corner.

**Captures.** A wire that starts from a pin **outside** the body and ends on a pin **inside** the body is a **capture** — it crosses the body's boundary and carries an outer-scope value into the per-iteration evaluation. Captures are how the inline-body model replaces the "extra parameters bound at function-pin wiring" mechanism: rather than pre-binding parameters of a function value, you just drag a wire from any outer node's output pin straight into a body node's input pin. The wire is drawn as a normal bezier visibly crossing the body's translucent edge; a small dot marks the boundary crossing. A capture from a deeper scope into a doubly-nested body crosses two boundaries and gets one marker per crossing.

Nested HOFs work the same way recursively: a `map` placed inside another `map`'s body renders its own inline body region; a capture from the outer-outer scope into the inner body crosses two boundaries.

**Authoring tips.**

- Bodies start out empty and immediately fail validation (every zone-output pin needs at least one incoming wire). The fix is to wire something into the zone-output pin; until you do, the HOF and the offending body node show a red error border. This is intentional — a freshly placed HOF with no body would have no per-element computation to run.
- Keyboard shortcuts (Delete, Ctrl+C/X/V/D) operate on the **active body** — the body whose interior you most recently clicked into. Clicking on the top-level canvas (outside any HOF) makes the top level active again.
- The inline body is the default authoring surface; wiring the optional `f` pin overrides it with a reusable `Function` value (see [`closure`](#closure)). The body region is hidden while `f` is connected and reappears when it is disconnected.
- `expr` is a convenient body-internal node: a single-parameter `expr` named `x` wired from `element` is the typical shape for a `map` body that computes `x + 1`, `x * 2`, etc.

## map

Takes a stream of values (`xs: Iter[T]`), runs the body on every element, and produces a transformed stream (`Iter[U]`) — the body's `result` zone-output value for each input element. The transformation is **lazy**: the body runs one element at a time, only when a downstream consumer pulls from `map`'s output. Wire an `Array[T]` into `xs` and the implicit `Array[T] → Iter[T]` conversion handles the wrapping automatically; wire `map`'s output into an `Array[U]` consumer and you'll need an explicit `collect` to materialize the result.

**Properties**

- `Input type` — the element type T of the input stream (drives the type of the body's `element` zone-input pin).
- `Output type` — the element type U of the output stream (drives the type of the body's `result` zone-output pin).

**External pins**

- Input `xs: Iter[InputType]` — the stream to transform. Accepts an `Array[InputType]` source via the implicit `[T] → Iter[T]` wire conversion.
- Input `f: Function(InputType, *)` *(optional)* — any function value whose parameter list **starts with** `InputType`. When wired, it drives the transform instead of the inline body. The `*` marks "any tail allowed": a `(InputType) → U` source slots in normally; a higher-arity source like `(InputType, K) → U` is **auto-partialized** — each element of the stream produces a partially-applied closure of type `Function((K,), U)` carrying that element as its first bound argument. See *[Function values and closures](#function-values-and-closures)* and the worked example below.
- Output `Iter[OutputType]` — the transformed stream. When `f` is connected, `OutputType` is **derived** from the wired source's signature (the partial-application tail's return, or the source's full return when the arities match), and the stored `Output type` property is shown as a read-only display in the editor.

**Body (inline)**

- Zone-input `element: InputType` — the current iteration value (inner-left source).
- Zone-output `result: OutputType` — the body's per-iteration return value (inner-right destination). Must have at least one incoming wire.

![](../../atomCAD_images/map_node_props_viewport.png)

To see the map node in action please check out the *Pattern* demo [in the demos document](../../../samples/demo_description.md).

In the Pattern demo, `map`'s input type is `Int` and output type is `Blueprint` — so the body's `element` pin is `Int` and the `result` pin is `Blueprint`. Inside the body, an `Int → Blueprint` chain (a `cuboid` whose position is driven by `element`, for instance) wires into `result`. To parameterize the body — e.g. a `gap` value that the body uses to space the cuboids — drop a `float` (or any other) node in the **outer** scope and drag a capture wire from it into the relevant body-internal node's input. The capture wire is the inline-body equivalent of the old "extra function parameter" mechanism.

**Auto-partialization example.** A `closure` of kind `Custom` with parameters `(x: Float, y: Float)` and body `expr: x * y` has type `Function((Float, Float) → Int)`. Wire it directly into the `f` pin of a `map` whose `Input type` is `Float` (e.g. driven by `range(3) → collect`), and `map.output_type` is derived to `Function((Float,), Int)` — `map` produces a stream of partially-applied closures, one per `xs` element, each with that element bound as its `x`. Downstream you can pipe this `Iter[Function((Float,), Int)]` into a second `map` whose body calls `apply` on each closure to finish the computation. No nested-`closure` ladder, no inline body required.

## filter

Returns a stream containing the elements of `xs` for which the body's `result` zone-output was `true`, preserving order. The filter is **lazy**: the body runs one element at a time, only when a downstream consumer pulls from `filter`'s output, and rejected elements are skipped without buffering.

**Properties**

- `Element type` — the element type T of the input and output streams.

**External pins**

- Input `xs: Iter[ElementType]` — the stream to filter. Accepts an `Array[ElementType]` source via the implicit `[T] → Iter[T]` wire conversion.
- Input `f: Function((ElementType) -> Bool)` *(optional)* — when wired, this predicate drives the filter instead of the inline body. Exact-arity: `filter` does not auto-partialize (its output type is fixed `Bool`, so there is nothing to absorb extra arguments into); only `map` accepts higher-arity sources via the starts-with rule.
- Output `Iter[ElementType]` — the kept-elements stream.

**Body (inline)**

- Zone-input `element: ElementType` — the current iteration value.
- Zone-output `result: Bool` — the predicate decision. Must have at least one incoming wire.

**Behavior**

If `xs` is unconnected the node produces an error. With `xs` wired, downstream pulls from the output stream advance the upstream `xs` walker until the body returns `true` for an element, then yield that element; consumers see only the kept elements, in their original order. An empty `xs` produces an empty stream; the body is never run. If the body returns anything other than `Bool`, the stream yields `Error("filter: f returned non-Bool")` and then ends — same fuse semantics as the rest of the iterator pipeline. The same applies if any required input inside the body is unwired and propagates as `None` — the predicate result is non-`Bool`.

A typical filter body is one `expr` node with an `Int` parameter named `x` wired from `element`, computing `x % 2 == 0` (keep evens) into `result`.

## fold

Reduces `xs` to a single value by repeatedly running the body with `(acc, element)`, starting from `init`, left-to-right. With body B:

- `fold(<empty stream>, init)  ==  init`
- `fold(<a, b, c>, init)       ==  B(B(B(init, a), b), c)`

`fold` is the primary **iterator consumer**: it drains the input stream one element at a time, so a `range → map → filter → fold` pipeline keeps memory at O(1) regardless of stream length. The output is a single accumulator value, not an iterator.

**Properties**

- `Element type` — the element type T of the input stream (drives the type of the body's `element` zone-input pin).
- `Accumulator type` — the accumulator and output type Acc (drives the types of both `acc` zone-input and `new_acc` zone-output). Acc may differ from T; body-internal pin connections use the same `Int ↔ Float` (and similar) conversions that any other pin connection does, so e.g. folding an `Iter[Float]` into an `Int` accumulator works because Float→Int truncation is already a supported pin conversion.

**External pins**

- Input `xs: Iter[ElementType]` — the stream to reduce. Accepts an `Array[ElementType]` source via the implicit `[T] → Iter[T]` wire conversion, so the legacy `[1, 2, 3] → fold` shape keeps working with no edit.
- Input `init: AccumulatorType` — the initial accumulator value.
- Input `f: Function((AccumulatorType, ElementType) -> AccumulatorType)` *(optional)* — when wired, this function drives the reduction instead of the inline body. Exact-arity: `fold` does not auto-partialize (its output type is fixed at `AccumulatorType`); only `map` accepts higher-arity sources.
- Output `AccumulatorType` — the final accumulator after the stream exhausts.

**Body (inline)**

- Zone-input `acc: AccumulatorType` — the running accumulator at this step.
- Zone-input `element: ElementType` — the current iteration value.
- Zone-output `new_acc: AccumulatorType` — the next accumulator value. Must have at least one incoming wire.

**Behavior**

If `xs` or `init` is unconnected, the node produces an error. With everything wired, an empty `xs` returns `init` unchanged (the body is never run). Otherwise the node walks `xs` left-to-right, replacing the accumulator with `body(acc, element)` at each step, and returns the final accumulator value. If the body errors on any iteration, the error propagates immediately and remaining elements are not pulled from the stream.

A summation body is one `expr` with parameters `a: Int` and `x: Int` (wired from `acc` and `element` respectively) computing `a + x` into `new_acc`. `fold` is the universal aggregator: sum, product, min, max, "all true", "any true", and chained CSG (e.g. unioning a list of blueprints) are all special cases.

## foreach

Side-effect counterpart of `map`: walks a stream of values and runs the body on every element for its side effect, discarding each return value. The output type is `Unit`, so `foreach` is gated by the [Execute action](../ui.md#execute-action-side-effect-nodes) — on a normal display pass the central skip rule short-circuits the node entirely without pulling a single element from `xs`, even when the upstream iterator would have been a million elements long. The motivating use case is **batch export**: a `product` node fans variants into a stream, and a `foreach` whose body wires `element` into an `export_xyz` node writes one file per variant when the user invokes Execute.

**Property**

- `Input type` — the element type T of the input stream (drives the type of the body's `element` zone-input pin).

**External pins**

- Input `xs: Iter[InputType]` — the stream to walk. Accepts an `Array[InputType]` source via the implicit `[T] → Iter[T]` wire conversion.
- Input `f: Function((InputType) -> Unit)` *(optional)* — when wired, this function drives the per-element side effect instead of the inline body. Exact-arity: `foreach` does not auto-partialize (its output is fixed `Unit`); only `map` accepts higher-arity sources.
- Output `Unit` — not displayable; the only point of wiring `foreach` is its side effect under Execute.

**Body (inline)**

- Zone-input `element: InputType` — the current iteration value.
- Zone-output `out: Unit` — the body's per-iteration return value (discarded). Because the universal `T → Unit` widening applies at the body's return position, the body can end in *any* node — `export_xyz` (the natural fit), `print` (returns `String`, widened to `Unit`), or even a pure data computation whose value is silently discarded. Must have at least one incoming wire.

**Behavior**

- **Display passes (no Execute):** zero work. The central skip rule prevents `eval` from running on any all-Unit-output node when `execute = false`, so neither `xs` nor the body is touched. This is what makes a `product → foreach` pipeline cheap during normal editing.
- **Execute passes:** drains `xs` left-to-right; for each element, runs the body and discards the result. **Fail-fast on errors:** if the body returns an error for any element, `foreach` halts immediately and surfaces that error as its output. This matches `fold` and `collect`'s mid-stream error semantics — silently producing a partial result set is the worst of all worlds for batch operations.

`map` keeps its data semantics; the `map`-with-`export_xyz`-in-the-body pattern still works under Execute (the flag propagates through the higher-order-function machinery), but `foreach` is the recommended primitive for batch export because of the display-pass short-circuit. A `map`-only pipeline produces an `Iter[Unit]` whose elements are only realized when the iterator is *consumed* — and you'd typically consume it by displaying a `collect` for inspection. `foreach` skips that ceremony: it consumes the stream itself and is the natural sink for "do something for every element."

## Function values and closures

A **function value** (type `Function((P0, P1, …) -> R)`) is a computation captured as a value that can travel along a wire and be called later. It is the same bundle an HOF's inline body represents — a body, its captured outer-scope values, and the wire delivering its result — detached from any single call site. Function pins and wires render in **amber**.

Two roles meet around a function value:

- The **[`closure`](#closure)** node *produces* one. It owns an inline zone body exactly like an HOF, but instead of consuming the body inline it exposes it on a `Function`-typed output pin.
- An HOF's **`f`** pin, the **[`apply`](#apply)** node, and a subnetwork's `Function` output *consume* one.

This buys three things the inline-body model alone cannot express:

- **Reuse** — author one body in a `closure` and wire it into several HOFs' `f` pins, instead of redrawing the same body at each call site.
- **Function factories** — a subnetwork can compute and return a `closure` configured by its inputs (e.g. a `(k: Int) -> Function` network whose returned `closure` captures `k` and adds it).
- **Single-value application** — `apply` calls a function once, on one argument set, outside any iteration. It also supports **partial application** — wire a prefix of the function's arguments and `apply` returns a new function value carrying the remaining (unwired) parameters.

**Currying equivalence (function types are canonical).** Function types are stored in a **flat** canonical form: `(A, B, C) → D`, `(A) → ((B, C) → D)`, `(A, B) → ((C) → D)`, and `(A) → (B) → (C) → D` are **the same type** — the multi-arg flat form. All four notations parse the same way and compare identical. Where one form ends and the next begins is decided at the call site by how many arguments you supply, not by how the value was authored.

**Capture-freeze timing.** A `closure`'s captures (values wired in from outside its body) are frozen when the `closure` node is evaluated — at its *definition site*, the standard closure semantics. A `closure` placed *outside* a `fold` freezes its captures once and shares them across every iteration (reuse without recomputation); a `closure` placed *inside* a `fold` body re-freezes per outer iteration, snapshotting that iteration's values.

**Compatibility.** A function value flows into a `Function`-typed pin when the two function types match **structurally**: same arity (after canonical flattening), with each parameter type and the return type pairwise convertible (the usual leaf conversions like `Int → Float` apply). The four HOFs' `f` pins differ in how much flexibility they grant:

- **`map.f`** has type `Function(InputType, *)` — any function value whose parameter list **starts with** `InputType` is accepted; extra parameters become a partial-application tail. See the `map` section for the worked example.
- **`filter.f`** / **`fold.f`** / **`foreach.f`** are exact-arity. Their output types are constrained (`Bool` / `AccumulatorType` / `Unit`), so a partial result has nowhere to go.
- **`apply.f`** has type `Function*` — accepts *any* function value, of any shape; argument pins materialize from the wired source's signature.

**Restrictions.** Function values cannot be array elements, record fields, or `Iter[T]` elements, and `Iter[T]` values cannot be captured into a closure. A v1 closure has exactly one result. Closures cannot reference themselves, so recursion is not expressible.

## closure

Exposes its inline zone body as a first-class `Function` value on its output pin, rather than consuming the body inline the way an HOF does. Wire the output into an HOF's `f` pin (reuse across call sites), into an [`apply`](#apply) node (call it once, or partially apply it), or into a subnetwork's return node (a function factory). Captures are frozen once, when the `closure` node is evaluated — see *[Function values and closures](#function-values-and-closures)* above.

**Property**

- `Kind` — a shape template that fixes the arity and decides, per pin, whether each type is **free** (you pick a `DataType`) or **fixed/derived** (supplied by the system). The five kinds are the four HOF body shapes plus a fully-flexible `Custom`:

  | Kind | parameters | result |
  |---|---|---|
  | `(T) -> U` *(map-like)* | `T` (named `element`) | free `U` (named `result`) |
  | `(T) -> Bool` *(filter-like)* | `T` (named `element`) | fixed `Bool` (named `result`) |
  | `(A, T) -> A` *(fold-like)* | `A` (named `acc`), `T` (named `element`) | derived `= A` (named `new_acc`) |
  | `(T) -> Unit` *(foreach-like)* | `T` (named `element`) | fixed `Unit` (named `out`) |
  | `(P0, P1, …, Pn) -> R` *(`Custom`)* | arbitrary count and naming (including **0**), each independently typed | free `R` |

  The four preset kinds are the natural match for the four HOFs' `f` pins. `Custom` is the general case: it accepts any number of parameters (including zero), with user-chosen names and types, and a free return type. A 0-parameter Custom closure (a **thunk**) has type `() → R` and is rendered with a `() → R` title. Pair Custom with `apply` for partial application or for calling functions whose shapes don't match any HOF; Custom closures also flow into `map.f` via the "starts-with" rule whenever their first parameter matches the input element type.

  The Node Properties panel shows a kind dropdown above one or two type pickers for the preset kinds, or a list of named-parameter rows + a return-type picker for `Custom`. Changing the kind restructures the node's zone pins through the standard repair pass.

**Output (single pin)**

- `Function((params) -> result)` — the function value. Multi-parameter return types are stored in canonical flat form (see *[Function values and closures](#function-values-and-closures)* on currying equivalence).

**Body (inline)**

- Zone-input pins (inner-left): one per parameter — the preset kinds use `element` and `acc` to match the matching HOF; `Custom` uses the user-supplied parameter names. A 0-parameter Custom closure has no zone-input pins.
- Zone-output pin (inner-right): the result — `result`, `new_acc`, or `out` by preset kind, or `result` for `Custom`. Must have at least one incoming wire (an empty body fails validation, like any HOF body).

The body is authored exactly like an HOF body: click into the region to make it the active scope, add nodes, and drag capture wires across the boundary. Captures are ordinary capture wires drawn into the body — they are *not* part of the shape, so the kind/type editor only ever describes parameters and result.

A `closure` can be promoted into a reusable named subnetwork (and the reverse) via the right-click **Extract to Network…** / **Convert to Closure** operations — see [Convert between a closure and a named network](../ui.md#manipulating-nodes-and-wires).

## apply

Calls a function value, and either runs it to completion (full application) or partially applies it (returning a new function value that still needs the rest of its arguments). Where the four HOFs run a function across a stream, `apply` runs it once on a single argument frame. Two motivating uses:

- **Full application** — call the output of a function-factory subnetwork. `apply(make_adder(5), 10)` yields `15`.
- **Partial application** — `apply(g, 2)` where `g: (Float, Float) → Float` yields a `Function((Float,) → Float)` that still needs its second argument. Chaining `apply` nodes lets you fill in arguments one or several at a time and is the flat (non-nested) counterpart to a ladder of nested `closure` definitions.

**Properties**

`apply` has no user-set kind or shape property. Its pin layout is **entirely derived from the connected `f`**: when nothing is wired into `f`, only the `f` pin is shown. The moment a `Function` source is wired into `f`, argument pins materialize matching that source's canonical (flat) signature.

**Input pins**

- `f: Function*` — **required**. Declared type accepts any function value, of any shape. Unlike an HOF, `apply` has no inline body to fall back on, so a disconnected `f` is a validation error.
- One argument pin per parameter of the wired source's canonical (flat) signature, typed to that parameter's type. **Arg pins are optional and must be wired as a contiguous prefix.** Wire `arg0…arg_{k-1}` and leave the tail (`arg_k…arg_{N-1}`) unwired to partially apply; wire all of them to fully evaluate. Wiring a non-prefix (e.g. `arg1` while `arg0` is unwired) is a validation error — partial application is positional from the left.

**Output (single pin)**

- **Full eval (`k == N`)**: the function's return type `R`. When `R` is `Unit`, `apply` is gated by the [Execute action](../ui.md#execute-action-side-effect-nodes) — calling an effectful function is itself an effect.
- **Partial application (`k < N`)**: `Function(<unwired parameter types>, R)` — a new function value bundling the wired arguments and the still-needed ones. Downstream consumers (another `apply`, an HOF's `f` pin, or a `Function`-typed pin on a subnetwork's return) consume it like any other function value.
- **Thunk force (`N == 0`, `k == 0`)**: when `f` is a 0-parameter Custom closure (a `() → R` thunk), no argument pins appear and the output is `R` — `apply` simply runs the body.

## print

A **debug node** for surfacing intermediate values into the [Console panel](../ui.md#console-panel) without breaking the wire. Passes its `text` input through unchanged on the output, and as a side effect appends a timestamped entry to a per-CAD-instance log buffer that the Console panel renders. Output type is `String` (not `Unit`), so the central skip rule does **not** apply: by default `print` fires on every evaluation that reaches it, including normal display passes — which is exactly what you want when you're trying to figure out what's flowing through a wire.

**Property**

- `execute_only: Bool` (default `false`) — when `true`, the buffer push fires only under an Execute pass; display passes still pass `text` through but do not append. Useful when the print is part of a batch-export pipeline and you only want one entry per element per Execute, not one per upstream edit.

**Input pin**

- `text: String` — the value to log. Wire any sub-network ending in a `String` (an `expr` template literal is a common pattern) into this pin; combine with record `record_destructure` to print specific fields of a record stream.

**Output (single pin)**

- `String` — the same value as `text`, unchanged. Insert `print` mid-chain without affecting downstream behavior.

The Console panel shows entries chronologically with a `[HH:MM:SS]` timestamp, the source `network / node-label`, and a ▶ marker on entries from Execute passes (so you can tell display-pass and Execute-pass prints apart). See the [Console panel](../ui.md#console-panel) section for toggling visibility, the autoscroll toggle, and the Clear button.

> **Tip.** `print` inside a `foreach` body fires once per element (lazy iteration), and the entries arrive in the same order the body was invoked. Combine with `execute_only = true` to keep the Console quiet during edits and only see the per-element trace when you actually run the batch.

## Record types

A **record type** bundles a fixed set of named, heterogeneously-typed fields into a single value that travels along one wire. Records are the CAD equivalent of a struct: rather than fan a small payload out into N parallel pins (or N parallel arrays), you declare the shape once and the network passes records through unchanged.

Define record types from the **User Types** panel on the left. Each project keeps its named record defs alongside its custom node networks; both kinds share one namespace and show up in the same list. A new def starts with zero fields — empty record types are valid (`{}` is the top of the record subtype lattice). Use the **+ Add field** button to append a field; each field row has a name, a type, a drag-handle for reorder, and a delete button. The order you author fields in is the order pins appear on `record_construct`, `record_destructure`, and `product` nodes that reference the def.

Records are **structurally subtyped** — compatibility between two record types is decided by their field shape, not by their names. A `Record(Foo)` whose schema is `{x: Int, y: Int}` is interchangeable with an inline anonymous `{x: Int, y: Int}` everywhere. They are also **width-subtyped**: a value with fields `{x, y, z}` flows freely into any pin that only declares `{x, y}`, and the extra `z` rides along at runtime untouched. Field-level subtyping accepts only *tag-only widenings* — exact equality plus the concrete-to-abstract phase upcasts (`Crystal → HasAtoms`, `Molecule → HasFreeLinOps`, …). Value-converting widenings such as `Int → Float` or `IVec3 → Vec3` are **not** applied inside record fields; insert an explicit conversion node before `record_construct` if you need one.

Record-typed pins render in a single neutral color (no per-name hashing — the visual reflects structural compatibility, not the def name). Hovering a record-typed pin shows the resolved schema in the def's authored field order.

Record defs may freely contain other record types as field types, but the dependency graph among defs must be acyclic. `Tree = { children: [Record(Tree)] }` is rejected; build recursive shapes by linking records via integer IDs in arrays instead.

Some node types ship with **built-in record defs** — schemas baked into the application that you don't have to author yourself. Two examples ship today:

- `ElementMapping = {from: Int, to: Int}` — the element type of `atom_replace`'s optional `rules` input.
- `Patch = {tile: Molecule, tiling_vectors: Array[IVec3], cut_volume: Blueprint}` — the surface-reconstruction patch produced by [`patch_build`](./atomic.md#patch_build) and consumed by [`patch_latticefill`](./atomic.md#patch_latticefill). Because a patch is a plain record of existing types, you can `record_destructure` one to swap its tile or tiling vectors with ordinary nodes.

Built-in defs share one namespace with user defs and participate in the same bare-identifier lookup (so `ElementMapping` and `Patch` work as type expressions in `expr` parameters and in the schema dropdowns), but they cannot be edited or deleted from the User Types panel, and the User Types panel will reject attempts to create, rename, or delete a user def with the same name.

## record_construct

Bundles N input values into a single record value of the target schema.

**Property**

- `Schema` — the name of a record type def in the project's User Types panel. The dropdown lists every existing def alphabetically; pick *Edit definition…* to jump to the schema editor for the selected def. New defs are created from the User Types panel, not from this dropdown.

**Input pins**

- One pin per field of the chosen schema, named after the field and typed to the field's declared type. Pins appear in the def's **authored field order** (the order shown in the schema editor) — they do not re-sort alphabetically.

**Output (single pin)**

- `Record(Schema)`.

**Behavior**

When you select a `record_construct` node, the **Node Properties** panel shows the schema dropdown followed by one inline editor row per field whose type is a simple editable type (`Bool`, `Int`, `Float`, `String`, the `Vec`/`IVec` vector types, and the `Mat3`/`IMat3` matrices) — the same set as for [custom node parameters](../node_networks.md#editing-custom-node-parameters). Fields of other types (`Blueprint`, `Crystal`, arrays, nested records, …) stay wire-only and do not appear in the panel.

For each field, a value wired into the corresponding input pin takes precedence over the value set inline; if neither is supplied, the output is `None` (propagates as a missing-input). Unlike custom node parameters, `record_construct` fields have no *default* layer to fall back to. Otherwise the node assembles a record value carrying every field. Editing the schema (renaming a field, retyping one, adding or removing fields) immediately re-derives this node's pin layout; wires whose source type no longer matches the corresponding field's type are disconnected, and inline values whose field was renamed or retyped become inert (they linger in the saved file but are ignored at eval).

If the `Schema` property is empty (`— No schema chosen —`) or names a deleted def, the node's output type is dangling and downstream wires are disconnected by the network's repair pass.

## record_destructure

Splits a record value into its constituent fields, one per output pin.

**Property**

- `Schema` — the name of a record type def. Same dropdown as `record_construct`, including the *Edit definition…* affordance.

**Input pin**

- `record: Record(Schema)`.

**Output pins**

- One pin per field of the chosen schema, named after the field and typed to the field's declared type. Pins appear in the def's **authored field order**.

**Behavior**

Reads each field by name. Because compatibility is width-subtyped, the runtime record may carry extra fields beyond the declared schema; those extras are simply not surfaced as output pins. If the input record happens to be missing a declared field (an unreachable case under pass-through, but defensive), the corresponding output pin emits `None`.

## product

Cartesian product of N input streams into an `Iter[Record(Target)]`. Use `product` to enumerate every combination of inputs as a structured payload — the motivating use case for record types, and the easiest path from "I have these N axes of variation" to "I have a stream of records I can `map` or `filter` over." Like the other iterator nodes, `product` is **lazy**: the cartesian space is never materialized; downstream pulls advance the rightmost axis one step at a time, with mixed-radix carries up the axes as they exhaust.

**Property**

- `Target` — the name of a record type def. The target's field list drives both the input pin layout and the output element type. Same dropdown as the other record nodes, with the *Edit definition…* affordance.

**Input pins**

- One pin per field of the target def, named after the field and typed `Iter[FieldType]`. Pins appear in the target's **authored field order**. Each pin accepts an `Array[FieldType]` source via the implicit `[T] → Iter[T]` wire conversion.

**Output (single pin)**

- `Iter[Record(Target)]`.

**Behavior**

For `Target = { f_0: T_0, …, f_{N-1}: T_{N-1} }` and inputs `xs_0: Iter[T_0], …, xs_{N-1}: Iter[T_{N-1}]`, the output stream yields the cartesian product:

```
{f_0: a_0, …, f_{N-1}: a_{N-1}}   for each (a_0, …, a_{N-1}) in xs_0 × … × xs_{N-1}
```

The **rightmost field varies fastest** (matches the natural reading of nested for-loops). The output cardinality is `∏ |xs_i|`; if any input stream is empty, the output stream is empty. If any input pin is unconnected, the output is `None`. To materialize the full enumeration as an `Array[Record(Target)]`, wire `product` into a `collect` node — note that this is what costs gigabytes for large products, and is precisely the cost the lazy stream is designed to avoid.
