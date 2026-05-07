# String Template Literals in the Expression Node

## Motivation

The motivating use case is **systematic file-path assembly** for batch export. The
`product` node produces an `Iter[Record(target)]` whose elements differ along
several authored axes (e.g. species × size × dose). Mapping that iterator over a
sub-network ends in `export_xyz`, which takes a `String` `file_name` input. To
make each variant land in its own file, the user needs an inline way to build
that path string from the record's fields:

```
`output/${variant.species}_size${variant.size}_dose${variant.dose}.xyz`
```

The runtime already supports strings end-to-end — `DataType::String`,
`NetworkResult::String`, and the `string` node all exist, and `expr` parameters
of type `String` validate as `Var` of type `String`. The missing pieces are
**any way to write a literal string inside an expr** and **any way to compose
strings**. Both gaps close with one feature.

We considered three approaches and picked **template literals only** because:

1. Backtick template literals cover both gaps in one syntactic form. A literal
   with no `${…}` is just a plain string; a literal with interpolations
   composes strings cleanly. No second concatenation operator needed.
2. The implementation is small and localized to `rust/src/expr/`. The runtime
   already produces `NetworkResult::String`; we only need to construct one.
3. It composes with everything already in the language: member access on
   records (`v.species`), arithmetic inside `${…}`, conditional expressions,
   etc.

A separate `"…"` plain-string literal form, a `+` overload for string
concatenation, formatting/padding helpers, and string member functions
(`.length`, `.to_lower`, etc.) are all deliberately **out of scope**. Each is a
trivially additive follow-up if real usage demands it. See "Out of scope" at
the end.

## Syntax

```
TemplateLiteral := "`" ( Text | Interpolation )* "`"
Text             := any character except backtick, '$' followed by '{', or '\\'
                 |  EscapeSequence
Interpolation    := "${" expr "}"            // expr is the existing expr grammar
EscapeSequence   := "\\`"  | "\\\\" | "\\$"  // backtick, backslash, dollar
                 |  "\\n"  | "\\t"  | "\\r"  // common whitespace
```

**Examples (covering the surface):**

```
``                                                // empty string ""
`hello world`                                     // "hello world"
`${x}`                                            // String value of x
`prefix-${x}`                                     // "prefix-" + str(x)
`${a}${b}`                                        // adjacent interpolations, no separator
`${variant.species}_size${variant.size}.xyz`      // record-field access inside ${}
`cost: $5`                                        // literal "$" — '$' not followed by '{' is literal
`literal \${x}`                                   // literal "${x}" — the \$ disables interpolation
`a backtick: \``                                  // literal "a backtick: `"
`line1\nline2`                                    // two lines, embedded newline
`line1
line2`                                            // also two lines — raw newlines are allowed
```

### Stringification rules

Every `${expr}` is validated to produce one of: `String`, `Int`, `Float`,
`Bool`. Anything else (including `Vec3`, `Record`, `Array`, `Iter`, the
phase types, etc.) is a validation error.

**Why this restricted set:** the path use case needs exactly these. Including
vectors/records would force a formatting policy choice (`(1, 2, 3)` vs
`{x: 1, y: 2, z: 3}` vs `1,2,3`) that has no obvious right answer; arrays and
iterators raise the same question plus delimiter choices. Users who need
those today can pull components out (`${v.x}_${v.y}_${v.z}`); a future
`format(value, spec)` builtin can extend the set when a real use case picks a
formatting answer.

Runtime stringification:

| Type      | Format                                        | Examples           |
|-----------|-----------------------------------------------|--------------------|
| `String`  | passthrough, no quotes                        | `hello`            |
| `Int`     | `i32::Display` (decimal, no padding)          | `42`, `-7`         |
| `Float`   | `f64::Display`, **finite values only** — full precision, no trailing zeros | `1`, `3.14`, `0.1`, `-7.5` |
| `Bool`    | `true` / `false`                              | `true`             |

We deliberately do **not** reuse `NetworkResult::to_display_string`, which
formats `Float` with `{:.6}` (six decimals always) — that would turn
`dose_0.1.xyz` into `dose_0.100000.xyz`. Path-friendly Float formatting wants
Rust's default `Display`, which trims trailing zeros.

Non-finite Float values (`NaN`, `+inf`, `-inf`) are **rejected at evaluation
time** with `NetworkResult::Error`. The motivating use case is filename
construction, where `dose_NaN.xyz` or `dose_inf.xyz` is filesystem-hostile
(case-folding collisions, confusing bug reports, broken downstream tools).
Finiteness isn't a type-level property, so this stays a runtime check rather
than a validation check — a Float `${…}` is still type-valid, it just may
fail at evaluation if the upstream value is non-finite.

### Type rule

A template literal always validates to `DataType::String` and evaluates to
`NetworkResult::String`. This holds even when the template body is empty
(``\``) or pure-text with no interpolations.

### Parsing model

The lexer scans the entire template literal as a **single token** containing
its parsed structure. There is no lexer-mode state that bleeds out:

```rust
pub enum Token {
    // ... existing variants ...
    /// Whole template literal, scanned in one go from `` ` `` to matching `` ` ``.
    /// On success, carries the parsed parts; each interpolation's *raw inner
    /// source* is preserved as a string and the parser re-tokenizes and
    /// parses it on demand. On failure, carries a structured error that the
    /// parser converts to a user-facing message — the lexer never falls back
    /// to `Token::Eof` to signal a malformed template.
    Template(Result<Vec<TokenTemplatePart>, TemplateLexError>),
}

pub enum TokenTemplatePart {
    Text(String),     // already-decoded literal text (escapes resolved)
    Expr(String),     // raw inner source of one ${...} (without the `${` `}`)
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplateLexError {
    /// Reached end of input before the closing backtick.
    Unterminated,
    /// Inside `${...}`, reached end of input before the matching `}`.
    UnterminatedInterpolation,
    /// `${}` — the interpolation has no source.
    EmptyInterpolation,
    /// `\X` where X is not one of the supported escapes.
    UnknownEscape(char),
    /// Backtick encountered inside `${...}`. Nested template literals are
    /// intentionally unsupported (see **Out of scope**).
    NestedTemplateNotSupported,
}
```

The lexer uses a small character-by-character routine that tracks brace depth
when it enters `${…}` so that nested record literals (`${ {x: 1} }`) and
unbalanced text don't prematurely terminate the interpolation. A backtick
inside `${…}` is rejected — nested template literals are intentionally not
supported (see **Out of scope**). The routine is contained — it does not
invoke the main `next_token` loop recursively.

The parser handles `Token::Template(parts)` by mapping each part:

```rust
pub enum Expr {
    // ... existing variants ...
    /// String template literal. `parts` is a flat list of literal text
    /// segments and embedded expressions, in source order. Adjacent literal
    /// segments are merged at lex time. Empty `parts` means the empty
    /// string literal `` ` ` ``.
    Template(Vec<TemplatePart>),
}

pub enum TemplatePart {
    Text(String),
    Expr(Box<Expr>),
}
```

For each `TokenTemplatePart::Expr(src)`, the parser calls the existing
`parse(&src)` recursively. This re-uses the whole expression grammar
(arithmetic, member access, calls, records, arrays) without writing a single
new parser path.

### Disambiguation

Backtick is currently an unused character in the expr language, so there is no
disambiguation to perform. The lexer adds one new prefix-character branch
(`` ` ``); everything else stays.

`$` outside a backtick literal is also unused today and stays so — `$` is only
meaningful inside template text, where bare `$` is literal and `${` opens an
interpolation.

### Errors

Lex/parse errors detectable at lex time:

- Unterminated template literal (`` `abc `` with no closing backtick) →
  *"unterminated template literal"*.
- Unterminated interpolation (`` `${a` `` end of input mid-expression) →
  *"unterminated `${...}` in template literal"*.
- Empty interpolation (`` `${}` ``) → *"empty `${}` in template literal"*.
- Unknown escape `\X` → *"unknown escape sequence `\X` in template literal"*.
- Nested template literal inside `${…}` (`` `${`inner`}` ``) → *"nested
  template literals are not supported inside `${...}`"*.

Parse errors inside a `${…}` (e.g. `` `${a +}` ``) bubble up from the recursive
`parse(&src)` call, prefixed with *"in template interpolation `${...}`: "*
where `...` is the raw source of the failing interpolation. This lets the
user identify *which* `${…}` failed even when several appear in the same
template (e.g. `` `${a}_${b +}_${c}` `` reports *"in template interpolation
`${b +}`: …"*).

Validation errors (only one new shape):

- *"template interpolation `${…}` must produce String, Int, Float, or Bool;
  got T"* where `T` is the inferred type of the inner expression.

Runtime errors (only one new shape):

- *"template interpolation produced non-finite Float (V); refusing to embed
  in string"* where `V` is the offending value (`NaN`, `inf`, or `-inf`).
  Finiteness is value-level, not type-level, so this lands at evaluation time.

## Implementation

### File-by-file changes

#### 1. `rust/src/expr/lexer.rs`

Add the new token variants and the error enum:

```rust
pub enum Token {
    // ... existing ...
    Template(Result<Vec<TokenTemplatePart>, TemplateLexError>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenTemplatePart {
    Text(String),
    Expr(String),  // raw inner source of one ${...}
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplateLexError {
    Unterminated,                 // missing closing backtick
    UnterminatedInterpolation,    // missing closing `}` in `${...}`
    EmptyInterpolation,           // `${}`
    UnknownEscape(char),          // `\X` where X is unknown
    NestedTemplateNotSupported,   // backtick inside `${...}`
}
```

Add a prefix-character branch in `next_token` for `` ` `` that calls a new
`scan_template_literal()` method. The scanner walks character-by-character,
maintaining a brace-depth counter inside `${…}`:

```rust
fn scan_template_literal(&mut self) -> Token {
    // opening backtick already consumed
    let mut parts: Vec<TokenTemplatePart> = Vec::new();
    let mut text = String::new();

    loop {
        match self.bump() {
            None      => return Token::Template(Err(TemplateLexError::Unterminated)),
            Some('`') => break,                          // close
            Some('\\') => {
                let escaped = match self.bump() {
                    Some('`')  => '`',
                    Some('\\') => '\\',
                    Some('$')  => '$',
                    Some('n')  => '\n',
                    Some('t')  => '\t',
                    Some('r')  => '\r',
                    Some(c)    => return Token::Template(
                        Err(TemplateLexError::UnknownEscape(c))),
                    None       => return Token::Template(
                        Err(TemplateLexError::Unterminated)),
                };
                text.push(escaped);
            }
            Some('$') if self.peek() == Some('{') => {
                self.bump(); // consume '{'
                if !text.is_empty() {
                    parts.push(TokenTemplatePart::Text(std::mem::take(&mut text)));
                }
                // capture inner expression source up to matching '}'
                match self.scan_interpolation_inner() {
                    Err(e) => return Token::Template(Err(e)),
                    Ok(ref s) if s.trim().is_empty() =>
                        return Token::Template(Err(TemplateLexError::EmptyInterpolation)),
                    Ok(inner) => parts.push(TokenTemplatePart::Expr(inner)),
                }
            }
            Some(c) => text.push(c),                    // including raw '\n' (multi-line allowed)
        }
    }
    if !text.is_empty() {
        parts.push(TokenTemplatePart::Text(text));
    }
    Token::Template(Ok(parts))
}
```

Every error path returns `Token::Template(Err(_))` carrying a specific
`TemplateLexError`; the lexer never silently produces `Token::Eof` for an
in-flight template. The parser branch (below) translates each variant into
the user-facing message documented in **Errors**.

`scan_interpolation_inner` returns `Result<String, TemplateLexError>` and is
a flat counter loop — no recursion, no state stack:

```rust
fn scan_interpolation_inner(&mut self) -> Result<String, TemplateLexError> {
    let mut buf = String::new();
    let mut depth: u32 = 0;            // unmatched `{` seen since `${`
    loop {
        match self.bump() {
            None                    => return Err(TemplateLexError::UnterminatedInterpolation),
            Some('}') if depth == 0 => return Ok(buf),
            Some('}')               => { depth -= 1; buf.push('}'); }
            Some('{')               => { depth += 1; buf.push('{'); }
            Some('`')               => return Err(TemplateLexError::NestedTemplateNotSupported),
            Some(c)                 => buf.push(c),
        }
    }
}
```

Brace-depth tracking lets `${ {x: 1} }` work. Rejecting `` ` `` keeps the
scanner trivially flat by ruling out nested template literals (see
**Out of scope**). The expr language has no plain-string literal and no
backslash-using token, so no other character requires special treatment
inside `${…}`.

#### 2. `rust/src/expr/expr.rs`

Add the new AST variant:

```rust
#[derive(Debug, Clone)]
pub enum Expr {
    // ... existing ...
    Template(Vec<TemplatePart>),
}

#[derive(Debug, Clone)]
pub enum TemplatePart {
    Text(String),
    Expr(Box<Expr>),
}
```

Add validation:

```rust
Expr::Template(parts) => {
    for part in parts {
        if let TemplatePart::Expr(e) = part {
            let t = e.validate(variables, functions)?;
            match t {
                DataType::String | DataType::Int | DataType::Float | DataType::Bool => {}
                other => return Err(format!(
                    "template interpolation `${{...}}` must produce String, Int, Float, or Bool; got {}",
                    other
                )),
            }
        }
    }
    Ok(DataType::String)
}
```

Add evaluation:

```rust
Expr::Template(parts) => {
    let mut out = String::new();
    for part in parts {
        match part {
            TemplatePart::Text(s) => out.push_str(s),
            TemplatePart::Expr(e) => {
                let v = e.evaluate(variables, functions);
                if let NetworkResult::Error(_) = v {
                    return v;       // short-circuit, matches existing pattern
                }
                match v {
                    NetworkResult::String(s)                    => out.push_str(&s),
                    NetworkResult::Int(n)                       => out.push_str(&n.to_string()),
                    NetworkResult::Float(f) if f.is_finite()    => out.push_str(&f.to_string()),
                    NetworkResult::Float(f)                     => return NetworkResult::Error(format!(
                        "template interpolation produced non-finite Float ({}); refusing to embed in string",
                        f
                    )),
                    NetworkResult::Bool(b)                      => out.push_str(&b.to_string()),
                    other => return NetworkResult::Error(format!(
                        "template interpolation produced non-stringable value of type {:?}",
                        other.infer_data_type()
                    )),
                }
            }
        }
    }
    NetworkResult::String(out)
}
```

The runtime fallback `other =>` exists for defense-in-depth — validation
already rejects non-stringable types — but it costs nothing and makes the
match exhaustive.

Add a `to_prefix_string` arm so `Expr::Template` round-trips through the
existing debug formatter:

```rust
Expr::Template(parts) => {
    let body = parts.iter().map(|p| match p {
        TemplatePart::Text(s) => format!("(text {:?})", s),
        TemplatePart::Expr(e) => format!("(expr {})", e.to_prefix_string()),
    }).collect::<Vec<_>>().join(" ");
    format!("(template {})", body)
}
```

#### 3. `rust/src/expr/parser.rs`

Add a primary-expression branch for `Token::Template(parts)`:

```rust
Token::Template(Err(e)) => return Err(format_template_lex_error(&e)),
Token::Template(Ok(parts)) => {
    let mut ast_parts: Vec<TemplatePart> = Vec::with_capacity(parts.len());
    for p in parts {
        match p {
            TokenTemplatePart::Text(s) => ast_parts.push(TemplatePart::Text(s)),
            TokenTemplatePart::Expr(src) => {
                let inner = parse(&src).map_err(|e|
                    format!("in template interpolation `${{{}}}`: {}", src, e)
                )?;
                ast_parts.push(TemplatePart::Expr(Box::new(inner)));
            }
        }
    }
    Expr::Template(ast_parts)
}
```

with the lex-error formatter:

```rust
fn format_template_lex_error(e: &TemplateLexError) -> String {
    match e {
        TemplateLexError::Unterminated =>
            "unterminated template literal".to_string(),
        TemplateLexError::UnterminatedInterpolation =>
            "unterminated `${...}` in template literal".to_string(),
        TemplateLexError::EmptyInterpolation =>
            "empty `${}` in template literal".to_string(),
        TemplateLexError::UnknownEscape(c) =>
            format!("unknown escape sequence `\\{}` in template literal", c),
        TemplateLexError::NestedTemplateNotSupported =>
            "nested template literals are not supported inside `${...}`".to_string(),
    }
}
```

Empty interpolations (`${}`) are caught at lex time and surface as
`Token::Template(Err(EmptyInterpolation))`; the parser does not need a
separate `parse("")` check. The `"in template interpolation: …"` wrapper
above only kicks in for *non-empty* interpolations whose body fails to parse
(e.g. `` `${a +}` ``).

No changes to `infix_binding_power` — template literals are pure primary
expressions.

#### 4. `rust/src/structure_designer/nodes/expr.rs`

Update the embedded help text in `get_node_type()` to document template
literals. Add a section after "Array Literals":

```
### String Template Literals

Build a String value with optional interpolation:

  `text-only literal`                      // String
  `${x}`                                   // stringification of x
  `prefix-${x}-suffix`                     // mixed
  `${a.species}_${a.size}.xyz`             // record-field interpolation

Interpolation `${expr}` accepts String, Int, Float, or Bool. Anything else
is a validation error.

Stringification:
  String → passthrough
  Int    → decimal (e.g. -7, 42)
  Float  → Rust default Display (e.g. 0.1, 3.14, 1, inf, NaN)
  Bool   → true / false

Escapes: \` \\ \$ \n \t \r. Bare $ not followed by { is literal, so
`cost: $5` works without escaping. To write a literal ${...}, escape the
dollar: `\${literal}`.
```

#### 5. No changes required elsewhere

- The `string` node still works as before (a constant String source).
  Template literals simply become the *expr-side* way to construct strings.
- `export_xyz` and other nodes that take a `String` input pin require no
  change; they'll consume the expr's `String` output through the standard
  pin-conversion machinery.
- No FRB regen is needed — expression syntax is data inside
  `ExprData.expression` and crosses the FFI boundary as a single string.

## Testing

New test file: `rust/tests/expr/template_literal_test.rs`, registered in
`rust/tests/expr.rs`.

### Lexer

| Input | Expected token (Template parts shown structurally) |
|-------|------------------------------------------------------|
| ``\`hello\``` | `[Text("hello")]` |
| ``\`\``` | `[]` (empty) |
| ``\`${x}\``` | `[Expr("x")]` |
| ``\`a${x}b\``` | `[Text("a"), Expr("x"), Text("b")]` |
| ``\`${a}${b}\``` | `[Expr("a"), Expr("b")]` (no empty Text between) |
| ``\`cost: $5\``` | `[Text("cost: $5")]` (bare `$` is literal) |
| ``\`\\${x}\``` | `[Text("${x}")]` (escape disables interpolation) |
| ``\`\\`back\\``` | `[Text("`back`")]` (escaped backticks) |
| ``\`a\\nb\``` | `[Text("a\nb")]` (escape produces newline) |
| ``\`a\nb\``` (raw newline) | `[Text("a\nb")]` (multi-line allowed) |
| ``\`${ {x: 1} }\``` | `[Expr(" {x: 1} ")]` (brace depth tracked) |
| ``\`abc`` (no closing) | error: unterminated template literal |
| ``\`${x`` (no closing brace) | error: unterminated `${...}` |
| ``\`\\q\``` | error: unknown escape |
| ``\`${}\``` | error: empty `${}` |
| ``\`${\`inner\`}\``` | error: nested template literals not supported |

### Parser

For each lexer success case above, verify the AST shape matches: `Text(_)` ↔
`TemplatePart::Text(_)`, `Expr(src)` ↔ `TemplatePart::Expr(parsed)`. Spot
checks:

| Input | Expected AST |
|-------|--------------|
| ``\`${x + 1}\``` | `Template([Expr(Binary(Var("x"), Add, Int(1)))])` |
| ``\`${v.species}\``` | `Template([Expr(MemberAccess(Var("v"), "species"))])` |
| ``\`${x +}\``` | parse error: *"in template interpolation: …"* |

### Validation

| Expression | Variables | Expected output type |
|------------|-----------|----------------------|
| ``\`hello\``` | — | `String` |
| ``\`\``` | — | `String` (empty) |
| ``\`${x}\``` | x: String | `String` |
| ``\`${x}\``` | x: Int | `String` |
| ``\`${x}\``` | x: Float | `String` |
| ``\`${x}\``` | x: Bool | `String` |
| ``\`${x}\``` | x: Vec3 | error (non-stringable) |
| ``\`${x}\``` | x: Record(...) | error (non-stringable) |
| ``\`${x}\``` | x: Array[Int] | error (non-stringable) |
| ``\`${v.species}_${v.size}.xyz\``` | v: Record({species: String, size: Int}) | `String` |
| ``\`${if x > 0 then 1 else 2}\``` | x: Int | `String` |

### Evaluation

| Expression | Variables | Expected `NetworkResult` |
|------------|-----------|--------------------------|
| ``\`hello\``` | — | `String("hello")` |
| ``\`\``` | — | `String("")` |
| ``\`${x}\``` | x: Int(42) | `String("42")` |
| ``\`${x}\``` | x: Float(0.1) | `String("0.1")` |
| ``\`${x}\``` | x: Float(1.0) | `String("1")` (no trailing zero) |
| ``\`${x}\``` | x: Float(NaN) | `Error(...)` (non-finite rejected) |
| ``\`${x}\``` | x: Float(f64::INFINITY) | `Error(...)` (non-finite rejected) |
| ``\`${x}\``` | x: Float(f64::NEG_INFINITY) | `Error(...)` (non-finite rejected) |
| ``\`${x}\``` | x: Bool(true) | `String("true")` |
| ``\`${x}\``` | x: String("abc") | `String("abc")` |
| ``\`a${x}b\``` | x: Int(7) | `String("a7b")` |
| ``\`${a}${b}\``` | a: Int(1), b: Int(2) | `String("12")` |
| ``\`${v.species}_${v.size}.xyz\``` | v: Record({species: "Si", size: 5}) | `String("Si_5.xyz")` |
| ``\`\\${x}\``` | x: Int(7) | `String("${x}")` (literal, no interpolation) |
| ``\`cost: $5\``` | — | `String("cost: $5")` |

### Integration / roundtrip

In `rust/tests/structure_designer/`:

- Create an expr node with `expression: ``\`${v.species}_${v.size}.xyz\``` and
  one parameter `v: Record(Named("Variant"))` (with a registered
  `RecordTypeDef` declaring `species: String, size: Int`). Validate, evaluate
  with a wired upstream record. Assert `NetworkResult::String` with the
  expected concatenation.
- Save the network to `.cnnd`, reload, re-validate. Confirm the expression
  string survives roundtrip and validates to `String`.
- Wire the expr's output to an `export_xyz` node's `file_name` input. Assert
  connection is accepted by `DataType::can_be_converted_to`.

### Coverage test

Add a single test enumerating all `DataType` variants and asserting whether
each is allowed as a `${…}` interpolation result:

```rust
#[test]
fn template_interpolation_accepts_only_documented_stringable_types() {
    // accepted: String, Int, Float, Bool
    // rejected: everything else (one assertion per variant)
}
```

When a new `DataType` variant is added in the future, this test fails and
forces an explicit accept-or-reject decision rather than letting the new
variant silently slip into (or out of) template eligibility.

## Out of scope

- **Nested template literals (`` `${`inner-${x}`}` ``).** A backtick inside
  `${...}` is rejected at lex time. Allowing nesting would force
  `scan_interpolation_inner` to track recursive template state and
  per-level escapes; the flat counter loop above stays trivially small
  because backtick is a hard error. The same string can be built with
  adjacent interpolations (`` `prefix-${x}-suffix` `` instead of
  `` `${`prefix-${x}-suffix`}` ``), so no expressive power is lost.
  Trivially additive later by promoting the loop into recursive descent if
  a real use case appears.
- **Plain `"…"` string literals.** Backticks alone cover both pure-string and
  interpolation cases; adding `"…"` is a small additive change but adds a
  second token, second AST variant, and parallel test surface for no current
  use case. Trivially additive later if visual cleanliness demands it.
- **`+` overload for string concatenation.** Templates compose strings inline
  (``\`${a}${b}\``); a `+` overload introduces a special case in
  `Expr::Binary` validation/evaluation with no expressive gain.
- **Format specifiers / padding.** Real future need (zero-padded frame
  numbers like `frame_001.xyz`), but it's a separate feature with its own
  design surface (specifier grammar, named vs positional args). Easiest
  follow-up: add a small `pad_left(s, len, ch)` builtin function and let
  users write ``\`frame_${pad_left(\`${n}\`, 3, \`0\`)}.xyz\```. No template
  syntax change needed.
- **String member functions** (`.length`, `.to_lower`, `.contains`, etc.).
  Add when a use case appears; the runtime carries the value already, so
  follow-ups land as new builtin functions rather than language work.
- **Stringification of compound types** (Vec3, Record, Array, Iter). Rejected
  at validation today; add a future `format(value, spec)` builtin or a
  `to_string` conversion if the policy choices ever pay off.
- **Multi-line editor ergonomics in `expr_editor.dart`.** Multi-line template
  bodies with embedded newlines are technically fine in the lexer (raw `\n`
  is allowed in template text), but the existing 12-line cap from the array-
  literal Phase 2 design (`design_array_literals_in_expr.md`) already covers
  this; no further UI work is required.

## Implementation checklist

1. [ ] Lexer: add `Token::Template(Result<Vec<TokenTemplatePart>, TemplateLexError>)`,
       the `TemplateLexError` enum, and the
       `scan_template_literal` / `scan_interpolation_inner` routines
       (`rust/src/expr/lexer.rs`). Wire `format_template_lex_error` in the
       parser so each `TemplateLexError` variant produces the message listed
       under **Errors**.
2. [ ] AST: add `Expr::Template(Vec<TemplatePart>)` and `TemplatePart`
       (`rust/src/expr/expr.rs`).
3. [ ] Validation: add the `Expr::Template` arm with the four-type stringable
       check.
4. [ ] Evaluation: add the `Expr::Template` arm with stringification by
       Rust `Display` (not `to_display_string`'s `{:.6}`); reject non-finite
       Float values at runtime with `NetworkResult::Error`.
5. [ ] `to_prefix_string` arm for round-trip debug formatting.
6. [ ] Parser: add the `Token::Template` primary-expression branch that
       recursively `parse(&src)`s each interpolation source
       (`rust/src/expr/parser.rs`).
7. [ ] Update expr node help text
       (`rust/src/structure_designer/nodes/expr.rs`).
8. [ ] Tests: lexer, parser, validation, evaluation in
       `rust/tests/expr/template_literal_test.rs` (registered in
       `rust/tests/expr.rs`).
9. [ ] Tests: integration / roundtrip / wire-into-`export_xyz` in
       `rust/tests/structure_designer/`.
10. [ ] Coverage test for stringable-type eligibility.
11. [ ] `cd rust && cargo fmt && cargo clippy && cargo test`.
12. [ ] No FRB regen needed (no API surface changes).
13. [ ] Manual smoke: in the running app, build a `product` → `map` pipeline
        terminating in `export_xyz`, set the `file_name` via an `expr` node
        with a template literal that interpolates the variant record's
        fields, run, and confirm the variants land in distinct files.
