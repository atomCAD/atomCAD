# Relaxed Node Names

## Status: Draft

## Motivation

[Issue #298](https://github.com/atomCAD/atomCAD/issues/298): the current naming rule
for node networks (and per-node custom names) is the lexer's bare-identifier rule
— effectively `[A-Za-z0-9_]` plus what `char::is_alphanumeric()` admits. This
blocks crystallography/library naming patterns the users actually want, e.g.:

- `lib.x_rect▭□▯{100}_positive`
- `lib.hexirod_[0001]30°`

The literal request in the issue ("blacklist instead of whitelist; allow anything
but backtick") cannot be applied to the bare-identifier form because several of
those characters are syntax in the text format (`.` is the multi-output pin
separator, `{}` delimit property blocks, `[]` arrays, `()` tuples, `=` is
assignment, `:` separates keys, `,` separates items, whitespace is a token
boundary).

The fix is to separate **what we store** from **how we serialize** — relax the
storage rule, and add backtick-quoted identifiers to the text format.

## Goals

- Allow nearly any character in a node-network name and a per-node custom name.
- Support those names losslessly in the text format.
- Keep the existing bare-identifier serialization for the common case (so
  existing files round-trip unchanged).
- Keep the existing backtick-in-prose convention for rename auto-update in
  comments and network descriptions (no change in that area).

## Non-Goals

- Renaming or restructuring built-in node-type names. They stay simple
  identifiers.
- Quoting pin names in `node.pin` references. Pin names come from
  `OutputPinDefinition.name`, are author-controlled, and have always been
  simple identifiers. No need.
- Flutter UI redesign. Existing text fields already accept arbitrary Unicode;
  only a small input validator (reject backtick + control chars) is needed.

## Design

### 1. Allowed characters in stored names

A single validator, used for both network names and per-node custom names:

```rust
// rust/src/structure_designer/identifier.rs (new)

pub fn is_valid_user_name(s: &str) -> Result<(), InvalidNameReason> {
    if s.is_empty() { return Err(Empty); }
    for c in s.chars() {
        if c == '`' { return Err(ContainsBacktick); }
        if c.is_control() { return Err(ContainsControl); }
    }
    if s.starts_with(char::is_whitespace) || s.ends_with(char::is_whitespace) {
        return Err(EdgeWhitespace);
    }
    Ok(())
}
```

Rationale for each exclusion:

- **Empty** — would produce `` `` `` `` which is meaningless.
- **Backtick** — reserved as the text-format quoting delimiter and as the
  rename auto-update marker in prose. Exclusion makes the literal-string
  scan/replace in `undo/commands/rename_helpers.rs` correct unconditionally.
- **Control characters (incl. `\n`, `\r`, `\t`)** — round-trip hazards in
  serialization, copy/paste, UI display, and diffs.
- **Leading/trailing whitespace** — invisible in most UIs; almost always a
  paste accident rather than intent.

Everything else is fair game: dots, dashes, parens, brackets, braces, degree
sign, Unicode shapes, Unicode letters, spaces, mathematical symbols, etc.

The validator is called from:

- `StructureDesigner::add_node_network_with_name`
- `StructureDesigner::rename_node_network`
- `StructureDesigner::factor_selection_into_subnetwork`
- The path that mutates `Node.custom_name` (currently
  `StructureDesigner::set_node_custom_name` and equivalents). `Node.custom_name`
  is `Option<String>`; `None` is the existing "no override, use default"
  sentinel and must remain valid. The validator runs on the inner string of a
  `Some(_)` only — clearing the custom name (assigning `None`) bypasses
  validation.
- The text-format `NetworkEditor` when materializing names from parsed input.

### 2. Backtick-quoted identifiers in the text format

A node-name token has two forms:

```
bare         := <existing rule: leading letter or `_`, then alphanumeric/`_`>
quoted       := `` ` `` <one or more chars, none of which is `` ` ``> `` ` ``
identifier   := bare | quoted
```

Examples of equivalent text:

```
my_sphere = sphere { radius: 5 }
`my_sphere` = sphere { radius: 5 }            # quoted form for an unreserved name — legal
`lib.x_rect▭□▯{100}_positive` = sphere { ... }
result = union { a: `lib.x_rect▭□▯{100}_positive`, b: my_box }
```

Quoted form is **always legal**, even when not required. The serializer chooses
the form via the `needs_quoting` predicate (§3).

Every grammar position that currently consumes a node name accepts both forms.
Concretely:

Within an assignment statement:

1. **LHS** — the node's custom name.
2. **Node-type position** (after `=`) — the type name. Custom networks become
   node types, so a network with a relaxed name appears here in quoted form.
3. **Reference position** — value of an input property (`a: foo`, `a: foo.diff`)
   and inside arrays/tuples. Quoted form: `` a: `lib.x_rect`.diff ``. Note the
   pin part stays bare; only the name side needs quoting.

In other statements:

4. **Function reference** — `@network_name`. Quoted form: `` @`my.network` ``.
5. **`output` and `delete` statements** — `` output `lib.x_rect` ``,
   `` delete `lib.x_rect` ``.

A qualified reference is `identifier ('.' bare_pin)?` — the pin half is always
bare, since pin names are author-controlled and remain simple identifiers
(see Non-Goals).

### 3. The `needs_quoting` predicate

Used by the serializer to decide which form to emit. Defined in terms of the
lexer so it cannot drift:

```rust
pub fn needs_quoting(s: &str) -> bool {
    !lexes_as_single_bare_identifier(s)
}
```

The implementation calls the existing `Lexer` over `s`, requires that it produce
exactly one `Token::Identifier(t)` followed by EOF with `t == s`, and returns
false in that case. Anything else — multi-token splits, keyword collisions
(any token the lexer recognizes as a non-`Identifier` token, e.g. `true`,
`false`, `output`, `delete`, `description`, `summary`), leading digit, embedded
reserved character — is `true`.

By delegating to the lexer rather than maintaining a parallel character
classification, the predicate is automatically correct as the lexer evolves
(new keywords, new reserved characters).

The serializer always emits the bare form when `!needs_quoting(name)`, and the
quoted form otherwise. Files that contain only existing names round-trip
byte-identically.

### 4. Comments and network descriptions

No change. The existing convention — wrap a name reference in backticks inside
prose so `apply_rename_core` in `undo/commands/rename_helpers.rs` can find and
update it — keeps working unchanged. Because stored names cannot contain
backticks (§1), the literal `` format!("`{}`", old_name) `` → `` format!("`{}`",
new_name) `` replace remains safe and unambiguous regardless of which other
characters the name contains.

## Implementation Plan

Single phase; no need to split. Order matters because tests at each step
depend on the previous one.

### Step 1: Validator

- Add `rust/src/structure_designer/identifier.rs` with `is_valid_user_name`
  and `InvalidNameReason`.
- Wire it into `add_node_network_with_name`, `rename_node_network`,
  `factor_selection_into_subnetwork`, and node-rename paths.
- Surface errors through the existing API result types.
- Tests: `rust/tests/structure_designer/identifier_test.rs` — boundary
  cases (empty, backtick, `\n`, leading space, valid Unicode shapes,
  long Unicode strings, `.`, `{`, `}`, etc.).

### Step 2: Lexer — quoted identifier token

`rust/src/structure_designer/text_format/parser.rs`:

- Add a `` ` `` arm in the lexer that reads until the next `` ` `` and emits
  `Token::Identifier(content)`. Error on EOF before the closing backtick
  (unterminated quoted identifier) and on an empty quoted token `` `` ``
  (forbidden by §1; the lexer must agree, otherwise an empty identifier could
  enter the AST through the text format and bypass the validator).
  An embedded backtick is not a possible input — the second backtick is by
  definition the closer — so no rule is needed for it.
- Add `needs_quoting(&str) -> bool` (and the lower-level
  `lexes_as_single_bare_identifier`) in the same module, exported.

### Step 3: Parser — accept quoted form everywhere `identifier` appears

The parser already consumes `Token::Identifier(_)` in all the relevant
positions (LHS, type, reference, `@func`, `output`, `delete`). Once the lexer
emits `Token::Identifier` for the quoted form, the parser changes are
near-zero — only positions that special-case identifier text (e.g., keyword
collision checks via the lexer's keyword path) need to make sure quoted
identifiers are not interpreted as keywords. Since keywords are produced by
the bare-identifier path (`"output" => Token::Output`), the quoted path
naturally bypasses them. Verify with tests.

### Step 4: Serializer — emit quoted form when needed

`rust/src/structure_designer/text_format/network_serializer.rs`:

- Add a small helper `format_identifier(s: &str) -> Cow<str>` that calls
  `needs_quoting` and returns either `s` borrowed or `` `s` `` owned.
- Replace the raw `format!("{} = {}", name, ...)` and similar sites with the
  helper. This includes node-name LHS, node-type RHS (custom networks),
  reference positions in `format_reference`, the `@network_name` form, and
  the `output`/`delete` statements.
- The pin-name half of `name.pin` references stays bare.

### Step 5: Flutter UI input validation

`lib/structure_designer/...` rename and factor-out dialogs:

- Validate with the same rule as the Rust validator (mirror in Dart):
  non-empty, no backtick, no control chars, no edge whitespace.
- Show inline error messages.

### Step 6: Tests

- `rust/tests/structure_designer/text_format_test.rs`:
  - Lex/parse a quoted identifier in each position (LHS, type, ref, `@func`,
    `output`, `delete`).
  - Round-trip: serialize a network with a relaxed name and re-parse it,
    asserting structural equality.
  - Round-trip stability: a network with only existing-style names emits
    byte-identical text before and after the change.
  - `needs_quoting` table-driven tests covering: bare-safe names, names with
    each reserved char, keyword collisions (`true`, `output`, etc.), names
    starting with a digit.
- `rust/tests/structure_designer/cnnd_roundtrip_test.rs`: a fixture with a
  relaxed name round-trips through `.cnnd` JSON unchanged.
- `rust/tests/structure_designer/undo_test.rs`: rename to and from a relaxed
  name; rename auto-update of backtick references in a network description
  works for relaxed names.
- `rust/tests/structure_designer/identifier_test.rs`: validator coverage
  (Step 1).

## Drawbacks / Risks

- **Lexer/parser surface area grows slightly.** Mitigated by sharing
  `needs_quoting` with the serializer so the two cannot drift.
- **Pin-reference grammar with quoted names** (`` `name`.pin ``) is a tiny
  bit harder to read than `name.pin`. Acceptable — only appears when the
  user opted into a relaxed name.
- **Possible UI confusion** if a user types backticks expecting them to
  delimit a name. The Flutter validator's error message should explicitly
  mention this ("backticks are reserved; just type the name").
- **Backtick scanning in prose remains heuristic.** A backtick-quoted name
  appearing inside a string literal in a description (legal in JSON) would
  be rewritten on rename. This is the existing behavior; not introduced by
  this change. Worth flagging in case it later needs a more precise
  walker.

## Open Questions

- Should `needs_quoting` also force quoting for names that are *visually*
  ambiguous (e.g., contain combining marks, RTL overrides, zero-width
  characters)? Not for v1; revisit if it becomes a real problem.
- Do we want a CLI/text-format escape for a literal backtick inside a name?
  No — names cannot contain backticks (§1). Listed here so the answer is in
  writing.

## Migration

None needed. Existing files contain only bare-identifier names by
construction; the serializer continues to emit bare form for them, so they
round-trip byte-identically. Old readers loading new files written with
relaxed names will fail to parse the quoted form — acceptable; this is a
forward-compatibility break by design.
