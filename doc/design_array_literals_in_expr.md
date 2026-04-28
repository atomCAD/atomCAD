# Array Literals and Multiline Editing in the Expression Node

## Scope

This document is split into two phases that ship independently:

- **Phase 1 — Array literal syntax in the expression language.** Adds `[a, b, c]` non-empty array literals plus `[]T` empty-array annotations to the expr language. Verifies an expr node with zero parameters works as a pure literal node. Solves the core motivating problem (no way to construct an `Array[T]` inline today).
- **Phase 2 — Multiline-friendly expression editor.** UI improvements to the expr node's text area: bounded dynamic height, a `Ctrl+Enter` apply shortcut, and confirmed-correct keyboard handling for newline-rich expressions. Becomes especially valuable once Phase 1 lands and users start writing array literals with one element per line.

A separate, dedicated `array` literal node type with a per-element grid UI may be developed at some future point if real usage shows the expr-based path is inadequate for high-volume hand-typed lists. That node is **not** part of this document and is mentioned only to record that the option exists.

## Phase 1 — Array literal syntax

### Motivation

Several use cases require feeding an `Array[T]` into a downstream node:

- **List of defect positions** (the immediate motivating case): a defect-application node wants `Array[IVec3]`. Today the user has no way to construct one inline — every position would need its own node, which is impractical.
- **List of computed values**: `Array[Float]` derived from parameters or arithmetic.
- **Empty list as a "no-op" input**: e.g. "no defects today" while keeping the wire connected.

The runtime already supports arrays end-to-end — `DataType::Array(Box<DataType>)` and `NetworkResult::Array(Vec<NetworkResult>)` exist, and nodes like `range` / `map` produce/consume them. The missing piece is an **inline literal syntax**, and the natural place to put it is the expression language used by the `expr` node, which already has dynamic input parameters and type inference.

### Design choice rationale

We considered three approaches (extend expr / new dedicated array node / hybrid) and picked **extend expr** because:

1. It reuses everything the `expr` node already provides — dynamic parameters, type inference, error display, undo/redo, text-format roundtrip, validation pipeline.
2. It does not add a new node type (the user explicitly wanted to limit node-type proliferation).
3. It composes cleanly: pure-literal arrays (`[ivec3(1,2,3), ivec3(4,5,6)]`), wired-parameter arrays (`[p1, p2, p3]`), and mixed-compute arrays (`[a*2, a*3, a*4]`) all fall out of the same construct.
4. Implementation is small and localized to `rust/src/expr/`.

The single-line text field will become awkward past ~30 hand-typed elements; a future dedicated array node could address that with a grid UI. It is deliberately out of scope here, since the expr-based path covers all current concrete use cases.

### Syntax

#### Non-empty array literal

```
[expr1, expr2, ..., exprN]
```

Element type is inferred via existing type-promotion rules (see "Element type unification" below). Trailing comma is **not** allowed (consistent with function-call argument syntax in the rest of the language).

Examples:

```
[1, 2, 3]                                  // Array[Int]
[1, 2.0, 3]                                // Array[Float] (Int promoted)
[ivec3(1,2,3), ivec3(4,5,6)]               // Array[IVec3]
[ivec3(1,2,3), vec3(0.5, 0.5, 0.5)]        // Array[Vec3]  (IVec3 promoted)
[a, b, c]                                  // Array[T] where T is the unified type of params a, b, c
[a*2 + 1, a*3 + 1, a*4 + 1]                // Array[Int] (assuming a: Int)
[]                                         // ERROR — empty literal needs a type, e.g. []Int
[1, vec3(0,0,0)]                           // ERROR — incompatible element types
```

#### Empty typed array literal

```
[]TypeExpr
```

Where `TypeExpr` is a concrete DataType expression (see "Element types" below). The leading `[]` is the empty-literal marker; the trailing `TypeExpr` declares the element type using the same type-expression grammar used elsewhere.

Examples:

```
[]IVec3              // Array[IVec3], empty
[]Float              // Array[Float], empty
[]Structure          // Array[Structure], empty
[]Crystal            // Array[Crystal], empty
[][IVec3]            // Array[Array[IVec3]], empty
[][[Int]]            // Array[Array[Array[Int]]], empty
```

The two grammars are kept distinct on purpose: `[]` is a value-level marker (an empty array), while `[K]` in type position is the array-type constructor. They compose cleanly without overlap — `[][K]` reads "empty array, of type Array of K" — and parameter names that happen to match type names (`structure: Structure`) are no longer a syntactic hazard.

#### Disambiguation rule

There is no genuine ambiguity. The parser applies a single zero-cost lookahead after consuming `[`:

1. **Next token is `]`** → empty-array literal: consume `]`, parse a `TypeExpr`, emit `Expr::EmptyArray(type)`.
2. **Anything else** → element list: parse `expr (, expr)* ]`.

For example:

| Input | Parses as | Why |
|-------|-----------|-----|
| `[]Int` | empty `Array[Int]` | `[` then `]` → empty marker; `Int` is the type |
| `[][IVec3]` | empty `Array[Array[IVec3]]` | `[` then `]` → empty marker; `[IVec3]` is the type |
| `[]Int` followed by stray tokens | parse error after the type | normal recovery |
| `[a]` (a:Int param) | 1-element `Array[Int]` | `[` then `a` (not `]`) → element list |
| `[Int]` | 1-element array — element is `Var("Int")` | `[` then `Int` (not `]`) → element list; fails validation if no `Int` variable in scope |
| `[1, 2, 3]` | 3-element `Array[Int]` | element list |
| `[[]Int]` | 1-element `Array[Array[Int]]` containing one empty `Array[Int]` | outer `[` then `[` → element list; first element is `[]Int` |
| `[[1, 2]]` | 1-element wrapping `Array[Int]` | element list |
| `[ivec3(1,2,3)]` | 1-element `Array[IVec3]` | element list |

There is no speculative parsing, no save/restore, and no failure mode where the parser silently picks the wrong interpretation. `[]T` always means empty, `[…]` (anything else) is always element-list.

#### Element types

The set of allowed element types is **any concrete DataType**, parsed by a small recursive type-expression parser used only after the `[]` empty-literal marker:

```
TypeExpr := TypeName             // primitive or domain type, e.g. Int, IVec3, Structure
          | "[" TypeExpr "]"     // recursive: array of TypeExpr
```

Note: the `[T]` form for "array of T" matches the UI display convention for nested array types. There is no `Array` keyword in the source syntax — the brackets themselves convey "array of." Nested types read naturally: `[[IVec3]]` is "array of array of IVec3" in type position. This grammar appears only after `[]` in source — the standalone bracket form is never confused with an expression.

`TypeName` is any DataType identifier the system already recognises — primitives (`Int`, `Float`, `Bool`, `Vec2`, `Vec3`, `IVec2`, `IVec3`, `Mat3`, `IMat3`), domain types (`Structure`, `Blueprint`, `Crystal`, `Molecule`, `Atomic`, `Geometry`, `LatticeVecs`, `DrawingPlane`, …) — except for the explicitly rejected names below.

**Element-type policy: default-allow.** Anything `DataType::from_string` accepts is allowed as an element type by default; only the rejections enumerated below are excluded. This is deliberate — trying to curate "useful" element types is overengineering, and the coverage test (see Testing → Coverage test for element-type eligibility) ensures any newly added `DataType` variant lands as an explicit accept-or-reject decision rather than slipping in silently. The reasoning behind the principle: the expr language *can* produce values of any DataType via parameters (a user with `a: Structure` can write `[a, a, a]` to produce `Array[Structure]`), so whatever element type a non-empty literal can yield, the empty annotation should be able to declare.

**Rejected as element types:**

- The abstract supertypes `HasAtoms`, `HasStructure`, `HasFreeLinOps`. These are documented as input-pin-only types; an array's element type must be concrete (no abstract variants exist at runtime).
- `None`. Sentinel, not a real value type.
- `Function(...)`. Closures are not first-class in the expr language, so a non-empty literal cannot produce one — and the proposed `parse_type_expr` grammar (`Ident | [TypeExpr]`) cannot reach a function type anyway. Listed explicitly so the rejection is documented if `parse_type_expr` is ever widened to accept parens.

The rejections share a common shape — sentinels, abstracts, and types unreachable at runtime — rather than being a hand-picked list of "uninteresting" concretes. Everything else (primitives, vectors, matrices, all three phase types, `Structure`, `Motif`, `LatticeVecs`, `DrawingPlane`, `Geometry2D`, `String`, and future concrete domain types) is allowed without further enumeration.

**No parameter-shadowing hazard.** Type names are only parsed as types in the position immediately after `[]`. Everywhere else (including inside an element list `[a, b, c]`), an identifier resolves as a normal expression — so naming a parameter `Structure` and writing `[Structure]` produces a 1-element array containing the parameter, exactly as expected. The earlier `[T]`-as-empty-array proposal had a silent footgun here; the `[]T` form removes it.

#### Nested arrays

Both nested empty annotations (`[][IVec3]` for empty `Array[Array[IVec3]]`) and nested non-empty literals (`[[ivec3(1,2,3)], [ivec3(4,5,6)]]`) are supported. They fall out naturally:

- `Expr::Array` is already a recursive enum variant, so non-empty nesting needs no parser change beyond the array-literal rule itself.
- The recursive `TypeExpr` grammar handles nested empty annotations.
- Unification extends with one case (see "Element type unification").
- No ambiguity: the `[]` value-marker and the `[K]` type form are syntactically disjoint, so every nested form has a single parse.

### Element type unification

For a non-empty array literal `[e1, e2, ..., eN]`, the element type is computed by reducing the element types pairwise:

- Start with `T = type_of(e1)`.
- For each subsequent element `ei`: `T = unify(T, type_of(ei))`, or fail if no common type exists.

Unification rules (matching existing conditional-expression and arithmetic promotion rules in `Expr::validate`):

| Pair                            | Result            |
| ------------------------------- | ----------------- |
| Same types                      | The type          |
| `Int` + `Float`                 | `Float`           |
| `IVec2` + `Vec2`                | `Vec2`            |
| `IVec3` + `Vec3`                | `Vec3`            |
| `IMat3` + `Mat3`                | `Mat3`            |
| `Bool` + `Int`                  | `Int`             |
| `Array[T1]` + `Array[T2]`       | `Array[unify(T1, T2)]` (recursive) |
| Anything else                   | Error             |

For non-promotable concrete types (`Structure`, `Crystal`, `Molecule`, etc.), only the "same types" rule applies — `[crystal_a, crystal_b]` unifies to `Array[Crystal]`, but `[crystal_a, molecule_b]` is rejected even though both flow through `HasAtoms`-typed input pins, because abstract supertypes are not valid concrete element types. Users wanting to mix must convert explicitly via separate nodes.

Reusing the existing promotion logic (already present in `types_compatible` and the conditional-branch unification at `rust/src/expr/expr.rs:300-308`) means no new type-system rules — just a helper that exposes the unified type, not just a yes/no compatibility check, plus the recursive Array case.

Error message format: *"array element {i} has type {Ti}, incompatible with prior element type {T}"*.

### Implementation

#### File-by-file changes

##### 1. `rust/src/expr/lexer.rs`

Add two tokens to the `Token` enum:

```rust
LBracket,    // [
RBracket,    // ]
```

Add to the lexer's `next_token` match: `'['` → `LBracket`, `']'` → `RBracket`. (Placement: near the `LParen` / `RParen` handling.)

##### 2. `rust/src/expr/expr.rs`

Add two new variants to `Expr`:

```rust
pub enum Expr {
    // ... existing variants ...
    Array(Vec<Expr>),               // non-empty array literal: [e1, e2, ...]
    EmptyArray(DataType),           // typed empty array: []Type
}
```

Add validation arms:

```rust
Expr::EmptyArray(t) => {
    // Already validated to be one of the supported types at parse time;
    // wrap and return.
    Ok(DataType::Array(Box::new(t.clone())))
}
Expr::Array(elements) => {
    // elements is non-empty by construction
    let mut unified = elements[0].validate(variables, functions)?;
    for (i, e) in elements.iter().enumerate().skip(1) {
        let ti = e.validate(variables, functions)?;
        unified = unify_array_element_types(&unified, &ti)
            .map_err(|_| format!(
                "array element {} has type {}, incompatible with prior element type {}",
                i, ti, unified
            ))?;
    }
    Ok(DataType::Array(Box::new(unified)))
}
```

Add evaluation arms:

```rust
Expr::EmptyArray(_) => NetworkResult::Array(vec![]),
Expr::Array(elements) => {
    let mut out = Vec::with_capacity(elements.len());
    for e in elements {
        let v = e.evaluate(variables, functions);
        if let NetworkResult::Error(_) = v {
            return v;  // short-circuit on error, matching call/binop pattern
        }
        out.push(v);
    }
    NetworkResult::Array(out)
}
```

Add a private helper `unify_array_element_types(a: &DataType, b: &DataType) -> Result<DataType, ()>` implementing the table above. (Returning the unified type rather than just a bool, unlike the existing `types_compatible`.)

If unification produces a wider type than the elements actually have (e.g. unifying `Int` and `Float` to `Float`), the runtime values will be heterogeneous (`Int(1)`, `Float(2.0)`). This matches existing behavior for conditional expressions, where `if c then 1 else 2.0` validates to `Float` but evaluates to either `Int(1)` or `Float(2.0)`. Downstream consumers of `Array[T]` should already tolerate this via the existing array-element conversion machinery in `DataType::can_be_converted_to`.

##### 3. `rust/src/expr/parser.rs`

Add a primary-expression branch for `LBracket` in `parse_bp`:

```rust
Token::LBracket => {
    self.parse_array_literal()?
}
```

Implement `parse_array_literal` with a single zero-cost lookahead:

```rust
fn parse_array_literal(&mut self) -> Result<Expr, String> {
    // LBracket already consumed.
    if matches!(self.peek(), Token::RBracket) {
        // []TypeExpr — empty typed-array literal.
        self.bump(); // consume `]`
        let t = self.parse_type_expr()?;
        return Ok(Expr::EmptyArray(t));
    }

    // Element list: parse expr (,expr)* ]
    let mut elements = Vec::new();
    loop {
        let e = self.parse_bp(0)?;
        elements.push(e);
        match self.peek() {
            Token::Comma => { self.bump(); continue; }
            Token::RBracket => { self.bump(); break; }
            other => {
                return Err(format!(
                    "expected ',' or ']' in array literal, got {:?}", other
                ));
            }
        }
    }
    Ok(Expr::Array(elements))
}

fn parse_type_expr(&mut self) -> Result<DataType, String> {
    match self.bump() {
        Token::LBracket => {
            let inner = self.parse_type_expr()?;
            match self.bump() {
                Token::RBracket => Ok(DataType::Array(Box::new(inner))),
                other => Err(format!("expected ']' to close array type, got {:?}", other)),
            }
        }
        Token::Ident(name) => {
            parse_concrete_type_name(&name)
                .ok_or_else(|| format!("unknown or non-concrete type '{}'", name))
        }
        other => Err(format!("expected type name or '[', got {:?}", other)),
    }
}
```

The grammar separates value and type positions cleanly: `[]` is the value-level empty-array marker, and `[K]` is the type-level array-of-K constructor. They never overlap because `parse_type_expr` is only called after `[]` has already been consumed. There is no speculative parse, no save/restore, and no semantic ambiguity to resolve.

`parse_concrete_type_name` is a free function returning `Option<DataType>` for any concrete DataType. The canonical string-to-DataType helper already exists at `rust/src/structure_designer/data_type.rs:225` (`DataType::from_string`) and enumerates every type name. Reuse it directly so the variant table is not forked:

```rust
fn parse_concrete_type_name(name: &str) -> Option<DataType> {
    let dt = DataType::from_string(name).ok()?;
    match dt {
        DataType::None
        | DataType::HasAtoms
        | DataType::HasStructure
        | DataType::HasFreeLinOps => None,
        _ => Some(dt),
    }
}
```

This keeps the standalone walker in the expr parser (only the small `[T]` recursion in `parse_type_expr`) without duplicating the type-name table. When a new `DataType` variant is added in the future, it lights up automatically here — no second parser to update. The coverage test under "Validation" below ensures any new variant forces an explicit accept-or-reject decision.

##### 4. `rust/src/structure_designer/nodes/expr.rs`

Update the embedded help-text in `get_node_type()` to mention array literal syntax. Add a section after "Vector Operations":

```
### Array Literals

- `[expr1, expr2, ...]` — non-empty array literal; element type inferred via
  existing promotion rules; recursive nesting supported.
- `[]TypeExpr` — empty array of given element type. TypeExpr is a primitive or
  domain type name (e.g. Int, IVec3, Structure), or `[TypeExpr]` for nested
  array types.

Examples:
  [1, 2, 3]                          // Array[Int]
  [1, 2.0]                           // Array[Float] (Int promoted)
  [ivec3(1,2,3), ivec3(4,5,6)]       // Array[IVec3]
  []IVec3                            // empty Array[IVec3]
  []Structure                        // empty Array[Structure]
  [][IVec3]                          // empty Array[Array[IVec3]]
  [][[Int]]                          // empty Array[Array[Array[Int]]]
  [[]Int]                            // 1-element Array[Array[Int]] containing one empty Array[Int]

The leading `[]` marks an empty-array literal; the trailing TypeExpr declares the
element type. The abstract supertypes HasAtoms, HasStructure, HasFreeLinOps are
not accepted as element types. Type-name identifiers are only interpreted as
types in the position immediately after `[]`, so naming a parameter after a type
(e.g. `structure: Structure`) is safe.
```

##### 5. Verify zero-parameter expr works as a literal

The user's secondary request: *"the expr node should allow for zero input parameters so it can be used as an input literal."* Reading the existing code:

- `ExprData.parameters: Vec<ExprParameter>` — `Vec` permits empty.
- `ExprEditorState._removeParameter` (`lib/structure_designer/node_data/expr_editor.dart:109`) deletes one parameter at a time; nothing prevents deleting the last one. The "no parameters defined" placeholder text already exists in the editor (line 211–222).
- `eval()` (`rust/src/structure_designer/nodes/expr.rs:142-163`) iterates over parameters; an empty loop is fine.
- `calculate_custom_node_type` produces zero input pins for an empty parameter vec.

**Verification task:** add a Rust integration test creating an expr node with empty `parameters: vec![]` and `expression: "[ivec3(1,2,3), ivec3(4,5,6)]"`, evaluate it, assert the output is `NetworkResult::Array([IVec3, IVec3])`. Confirm this also roundtrips through `.cnnd` save/load.

If the editor UI feels cramped at zero parameters (e.g. the "Add Parameter" button placement), make a small layout polish — but this is not a blocker.

### Testing

New test file: `rust/tests/expr/array_literal_test.rs`, registered in `rust/tests/expr.rs`.

#### Lexer (in `rust/tests/expr/`)

Add cases for `[` and `]` tokenization in the existing lexer test file (or `array_literal_test.rs`).

#### Parser

| Input | Expected |
| ----- | -------- |
| `[1, 2, 3]` | `Expr::Array([Int(1), Int(2), Int(3)])` |
| `[ivec3(1,2,3), ivec3(4,5,6)]` | `Expr::Array([Call(...), Call(...)])` |
| `[]IVec3` | `Expr::EmptyArray(DataType::IVec3)` |
| `[]Structure` | `Expr::EmptyArray(DataType::Structure)` |
| `[][IVec3]` | `Expr::EmptyArray(Array(IVec3))` |
| `[][[Int]]` | `Expr::EmptyArray(Array(Array(Int)))` |
| `[[]Int]` | `Expr::Array([EmptyArray(Int)])` (1-element outer, inner is `[]Int`) |
| `[]` (no type follows) | parse error (expected type after `[]`) |
| `[]Foo` | parse error (unknown type) |
| `[]HasAtoms` | parse error (abstract supertype rejected) |
| `[]None` | parse error (sentinel rejected) |
| `[1, ]` | parse error (trailing comma) |
| `[1; 2]` | parse error (bad separator) |
| `[1, 2` (no closing bracket) | parse error |
| `[Int, x]` | element list — `Int` becomes `Var("Int")`, fails validation as unknown variable |
| `[Structure]` (with no `Structure` parameter) | element list — `Var("Structure")`, fails validation as unknown variable |
| `[Structure]` (with parameter `Structure: Structure`) | 1-element `Array[Structure]` containing the parameter |
| `[[1, 2], [3, 4]]` | nested non-empty: `Expr::Array([Expr::Array([Int(1), Int(2)]), Expr::Array([Int(3), Int(4)])])` |
| `[[]Int, []Int]` | 2-element outer, each inner is `EmptyArray(Int)` |

#### Validation

| Expression | Variables | Expected output type |
| ---------- | --------- | -------------------- |
| `[1, 2, 3]` | — | `Array[Int]` |
| `[1, 2.0, 3]` | — | `Array[Float]` |
| `[1, 2.0, 3.0]` | — | `Array[Float]` |
| `[true, false]` | — | `Array[Bool]` |
| `[ivec3(1,2,3), ivec3(4,5,6)]` | — | `Array[IVec3]` |
| `[ivec3(1,2,3), vec3(0.5,0.5,0.5)]` | — | `Array[Vec3]` |
| `[a, b]` | a: Int, b: Int | `Array[Int]` |
| `[a, b]` | a: Int, b: Float | `Array[Float]` |
| `[a, b]` | a: IVec3, b: Vec3 | `Array[Vec3]` |
| `[a, a, a]` | a: Structure | `Array[Structure]` |
| `[a, b]` | a: Crystal, b: Crystal | `Array[Crystal]` |
| `[a, b]` | a: Crystal, b: Molecule | error (no concrete unification) |
| `[1, vec3(0,0,0)]` | — | error |
| `[a]` | a: Int | `Array[Int]` |
| `[]IVec3` | — | `Array[IVec3]` |
| `[]Float` | — | `Array[Float]` |
| `[]Structure` | — | `Array[Structure]` |
| `[][IVec3]` | — | `Array[Array[IVec3]]` (empty annotation) |
| `[][[Int]]` | — | `Array[Array[Array[Int]]]` (empty annotation) |
| `[[a], [b]]` | a: IVec3, b: IVec3 | `Array[Array[IVec3]]` |
| `[[a], [b]]` | a: Int, b: Float | `Array[Array[Float]]` (recursive unification) |
| `[[]Int, []Int]` | — | `Array[Array[Int]]` (2-element outer; each inner is empty `Array[Int]`) |
| `[[]Int]` | — | `Array[Array[Int]]` (1-element outer; inner is empty `Array[Int]`) |

#### Coverage test for element-type eligibility

Add a single test that enumerates every `DataType` variant and asserts `parse_concrete_type_name` either accepts it or appears in the documented rejection set (`None`, `HasAtoms`, `HasStructure`, `HasFreeLinOps`, `Function(_)`). Purpose: when a new `DataType` variant is added in the future, this test fails and forces an explicit accept-or-reject decision rather than letting the new variant silently slip into (or out of) array eligibility through `DataType::from_string`.

```rust
#[test]
fn every_concrete_datatype_is_array_eligible_or_explicitly_rejected() {
    // For each known DataType, call parse_concrete_type_name on its display
    // form and assert the result matches the documented rejection set.
}
```

#### Evaluation

| Expression | Variables | Expected `NetworkResult` |
| ---------- | --------- | ------------------------ |
| `[1, 2, 3]` | — | `Array([Int(1), Int(2), Int(3)])` |
| `[a*2, a*3]` | a: Int(5) | `Array([Int(10), Int(15)])` |
| `[ivec3(1,2,3), ivec3(4,5,6)]` | — | `Array([IVec3(1,2,3), IVec3(4,5,6)])` |
| `[a, a, a]` | a: Structure(s) | `Array([Structure(s), Structure(s), Structure(s)])` |
| `[[1, 2], [3, 4]]` | — | `Array([Array([Int(1), Int(2)]), Array([Int(3), Int(4)])])` |
| `[]Int` | — | `Array([])` |
| `[]IVec3` | — | `Array([])` |
| `[]Structure` | — | `Array([])` |
| `[][IVec3]` | — | `Array([])` (empty outer Array of inner Array[IVec3]) |
| `[[]Int, []Int]` | — | `Array([Array([]), Array([])])` (length-2 outer; each inner empty) |

#### Integration / roundtrip

In `rust/tests/structure_designer/`:

- Create an expr node with `expression: "[ivec3(1,2,3), ivec3(4,5,6)]"`, no parameters. Validate, evaluate. Assert `NetworkResult::Array` with two `IVec3` elements.
- Save the network to `.cnnd`, reload, re-validate. Assert the expression survives roundtrip and validates to `Array[IVec3]`.
- Wire the expr's output to a downstream node expecting `Array[IVec3]`. Assert connection is accepted by `DataType::can_be_converted_to`.

### Out of scope

- **Type-annotated non-empty literals** (e.g. `[1, 2, 3] : Array[Float]`). Not needed: existing inference already handles non-empty cases.
- **Tuple-literal shorthand** for vectors (e.g. `[(1,2,3), (4,5,6)]` for `[ivec3(1,2,3), ivec3(4,5,6)]`). Would require parens to overload as tuple constructors, conflicting with grouping. The verbose `ivec3(...)` form is acceptable.
- **Dedicated `array` literal node** with a per-element grid UI. May be designed later if the single-text-field expr approach proves inadequate at scale; not covered by this document.
- **Array indexing / length / slicing operations.** Out of scope — those would be language extensions that operate on arrays once they exist, but downstream array consumers (`map`, defect-application nodes) handle iteration themselves.

### Implementation checklist

1. [ ] Lexer: add `LBracket` / `RBracket` tokens (`rust/src/expr/lexer.rs`).
2. [ ] AST: add `Expr::Array` and `Expr::EmptyArray` variants (`rust/src/expr/expr.rs`).
3. [ ] Validation: add unification helper + arms for both new variants.
4. [ ] Evaluation: add arms for both new variants.
5. [ ] Parser: add primary-expression branch for `[` with single-lookahead disambiguation (`]` after `[` → empty-array marker, else element list), plus recursive `parse_type_expr` (`rust/src/expr/parser.rs`).
6. [ ] `parse_concrete_type_name` helper — thin wrapper around `DataType::from_string` (`rust/src/structure_designer/data_type.rs:225`) that rejects `None`, `HasAtoms`, `HasStructure`, `HasFreeLinOps`, and `Function(_)`. Do not duplicate the variant table.
7. [ ] Update expr node help text (`rust/src/structure_designer/nodes/expr.rs`).
8. [ ] Tests: lexer, parser, validation, evaluation in `rust/tests/expr/array_literal_test.rs`.
9. [ ] Tests: roundtrip and zero-parameter expr in `rust/tests/structure_designer/`.
10. [ ] Run `cd rust && cargo fmt && cargo clippy && cargo test`.
11. [ ] No FRB regen needed (no API surface changes — expression syntax is data inside `ExprData.expression`).
12. [ ] Manual smoke: in the running app, create an expr node, delete its default parameter, type `[ivec3(1,2,3), ivec3(4,5,6)]`, confirm output type displays as `Array[IVec3]`.

## Phase 2 — Multiline-friendly expression editor

### Motivation

The existing expr node text field caps at 3 visible lines (`maxLines: 3, minLines: 1` in `lib/structure_designer/node_data/expr_editor.dart:148-160`). With Phase 1 landed, users will routinely write expressions that span many lines — for example, a list of 50 defect positions formatted one element per line:

```
[
  ivec3(1, 0, 0),
  ivec3(0, 1, 0),
  ivec3(0, 0, 1),
  ...
]
```

The 3-line cap forces an awkward internal scroll, hiding most of the expression at any moment. Phase 2 raises the cap and adds a deliberate keyboard shortcut for committing changes without losing focus.

This phase ships independently of Phase 1 and benefits any long single-line expression (not just array literals), but the strongest motivation is the Phase-1 use case.

### Parser/lexer status — no changes needed

`rust/src/expr/lexer.rs:73-80` skips whitespace via `char::is_whitespace()`, which matches all Unicode whitespace including `\n`, `\r`, `\t`, and space. The lexer drops whitespace before producing tokens, so the parser never sees newlines:

```rust
while let Some(c) = self.peek() {
    if c.is_whitespace() {
        self.i += 1;
    } else {
        break;
    }
}
```

`[1,\n2,\n3]` is lexically identical to `[1, 2, 3]`. Phase 2 is a pure UI change.

### Text area height

**Decision:** bounded dynamic — `minLines: 1, maxLines: 12`.

- Auto-grows from 1 line up to 12 visible lines as the user types newlines.
- Beyond 12 lines, internal scrolling kicks in.
- Costs zero vertical space when the expression is short; comfortably accommodates 50-element array literals laid out one per line; degrades gracefully past that.

Avoid `maxLines: null` (unbounded growth) — a 500-element list would push the rest of the editor pane off-screen. The 12-line cap is a starting point; tune in code review if it feels off.

A fullscreen pop-out dialog for very large expressions is a possible future enhancement but is **not** part of Phase 2. Defer until someone actually hits the limit.

### Keyboard handling

#### Target platform

atomCAD is a desktop application shipped on Windows, macOS, and Linux; mobile is not a near-term target. Keyboard handling is designed for desktop only — no `textInputAction` ceremony, no mobile-specific submit paths — but **must respect the platform-native modifier convention** (Cmd on macOS, Ctrl on Windows/Linux). See "Apply shortcut" below for how this is handled.

#### Current behavior on Windows desktop (verified)

The existing field is configured with `keyboardType: TextInputType.multiline` and `textInputAction: TextInputAction.done`, plus an `onFieldSubmitted` callback and a focus listener (`expr_editor.dart:35-40`). What actually happens:

- **Enter** → inserts a newline (multiline behavior takes precedence).
- **Shift+Enter** → also inserts a newline (no special handling).
- **`onFieldSubmitted` never fires on desktop.** `TextInputAction.done` is a mobile-keyboard concept; on Windows there is no system-provided "done" key for a multiline field. The callback is dead code.
- **Submit happens only via focus loss** — the focus listener detects blur and calls `_updateExpressionFromText`.

#### Decisions

1. **Keep Enter = newline.** This is the current behavior and matches IDE/code-editor convention (the field is code, not chat). Inverting to "Enter submits" would surprise users mid-array-literal and constantly truncate their input.
2. **Don't special-case Shift+Enter.** Same as Enter. No reason to introduce a distinction users would have to learn.
3. **Keep submit-on-focus-loss.** Already works; touch nothing.
4. **Add a platform-native "apply now" shortcut: Ctrl+Enter on Windows/Linux, Cmd+Enter on macOS.** Lets keyboard-driven users commit a long expression without tabbing or clicking out. Implement via a `CallbackShortcuts` widget wrapping the `TextFormField` with **both** activators bound to the same callback — Flutter only fires the binding whose modifier is actually pressed, so listing both is safe on every platform and matches each user's muscle memory. The callback calls `_updateExpressionFromText(_expressionController.text)` and consumes the event so it does not also insert a newline.
5. **Optional `Esc` to revert.** Set the controller text back to the last applied `widget.data?.expression`, blur the focus node. Low priority — include only if it falls out cheap; otherwise defer.

### Implementation

#### File: `lib/structure_designer/node_data/expr_editor.dart`

The change is localized to the `TextFormField` in `build()` (currently lines 148-164) and a small wrapper around it.

##### 1. Raise the line cap

```dart
maxLines: 12,
minLines: 1,
```

##### 2. Wrap the field with `CallbackShortcuts` for the apply shortcut

Bind both Ctrl+Enter and Cmd+Enter to the same callback. Listing both activators is safe on every platform — only the one whose modifier is pressed will fire, and each platform's users get their native convention (Ctrl on Windows/Linux, Cmd on macOS):

```dart
CallbackShortcuts(
  bindings: {
    const SingleActivator(LogicalKeyboardKey.enter, control: true):
        () => _updateExpressionFromText(_expressionController.text),
    const SingleActivator(LogicalKeyboardKey.enter, meta: true):
        () => _updateExpressionFromText(_expressionController.text),
  },
  child: TextFormField(
    controller: _expressionController,
    focusNode: _expressionFocusNode,
    decoration: const InputDecoration(...),
    maxLines: 12,
    minLines: 1,
    keyboardType: TextInputType.multiline,
    // textInputAction and onFieldSubmitted dropped — both are dead on desktop.
  ),
),
```

(Extracting the callback into a local `void apply() => _updateExpressionFromText(_expressionController.text);` is a fine readability tweak if the duplication bothers reviewers; it does not change behavior.)

Desktop submit paths after this change: focus-loss (existing focus listener) and Ctrl+Enter / Cmd+Enter (new shortcut). Together they cover every keyboard-driven and pointer-driven workflow on every platform we ship to.

`CallbackShortcuts` only fires when the wrapped subtree has focus, which is exactly what we want — the shortcut is active whenever the user is typing in the field.

##### 3. (Optional) Esc to revert

If included, add another binding:

```dart
const SingleActivator(LogicalKeyboardKey.escape):
    () {
      _expressionController.text = widget.data?.expression ?? '';
      _expressionFocusNode.unfocus();
    },
```

### Testing

Phase 2 is mostly UI behavior; the value comes from manual smoke testing on every desktop platform we ship (Windows, macOS, Linux), with widget tests for the keyboard shortcut on both modifier paths.

#### Manual smoke

Run the full sequence on Windows/Linux using **Ctrl+Enter**, and on macOS using **Cmd+Enter**:

1. Open an expr node, paste an array literal with 30 elements one per line. Confirm the field grows up to ~12 lines, then scrolls.
2. Place cursor mid-expression, press Enter — confirm a newline is inserted, no submission.
3. Press the platform-native apply shortcut (Ctrl+Enter on Win/Linux, Cmd+Enter on macOS) — confirm the expression is applied (the output type updates immediately, no need to click outside) and that no stray newline was inserted.
4. (If Esc included) Modify the expression, press Esc — confirm the field reverts to the last applied value and loses focus.
5. Click outside the field — confirm the existing focus-loss submit path still works (no regression).

#### Widget test (Flutter)

Add a test in `integration_test/` or a new widget test file covering:

- `Ctrl+Enter` triggers the apply callback with the field's current text.
- `Cmd+Enter` (Meta+Enter) triggers the apply callback with the field's current text. Drive this directly via the test framework — no need to set `debugDefaultTargetPlatformOverride`, since we want both bindings live everywhere.
- Plain `Enter` does not trigger the apply callback (it inserts a newline instead).

### Out of scope

- **Syntax highlighting** for the expr language. Significant work; defer until a strong signal it would matter.
- **Auto-formatting** (e.g. one-element-per-line normalization on save). Out of scope — users may prefer their own layout, and the lexer ignores layout anyway.
- **Pop-out fullscreen editor.** Possible future enhancement once the 12-line bounded cap proves inadequate.
- **Bracket matching / autocomplete.** IDE-grade affordances are not pursued here.

### Implementation checklist

1. [ ] Bump `maxLines` from 3 to 12 in `expr_editor.dart`.
2. [ ] Wrap the `TextFormField` in `CallbackShortcuts` with **both** `Ctrl+Enter` and `Cmd+Enter` (`meta: true`) bindings calling `_updateExpressionFromText` — Ctrl is the Windows/Linux convention, Cmd is the macOS convention, and atomCAD ships on all three.
3. [ ] Remove `textInputAction: TextInputAction.done` and `onFieldSubmitted` from the field. Both are dead on desktop, and atomCAD is desktop-only.
4. [ ] (Optional) Add `Esc` binding that reverts the controller and unfocuses.
5. [ ] Manual smoke per the test plan above — run on Windows/Linux (Ctrl+Enter) and macOS (Cmd+Enter).
6. [ ] (Optional) Widget test covering both `Ctrl+Enter` and `Cmd+Enter` apply paths.
7. [ ] `flutter analyze` clean; `dart format lib/structure_designer/node_data/expr_editor.dart`.
