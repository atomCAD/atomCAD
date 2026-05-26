# Structural `Function` and `Iter` Types on `APIDataType` — First-Class Type Picker Branches

## Scope

Today `DataType::Function(...)` and `DataType::Iterator(...)` round-trip
through `APIDataType` only via the `APIDataTypeBase::Custom` free-form text
escape hatch (text like `(Int, Bool) -> String` or `Iter[Int]` typed into a
text field, parsed by `DataType::from_string`). Every other concrete
`DataType` variant has a first-class `APIDataTypeBase` enum value and a
dedicated `DataTypeInput` branch — Function and Iter are the conspicuous
exceptions.

This document designs the additive change that makes both types
**structurally** representable on the API surface and authorable from
`DataTypeInput` as recursive branches, without touching anything else
about the type system, the evaluator, the validator, the `.cnnd` format, or
the text format.

The work is one Rust-side API expansion (mostly bookkeeping) plus two
Flutter additions (the Iter branch, and a `FunctionTypeInput` widget that
nests inside `DataTypeInput`). The `Custom...` text option stays as a fallback
for future unsupported types (today: `RecordType::Anonymous`).

**Load-bearing invariant**: `APIDataType` carries no on-disk state. It is
the FRB shuffle struct between Rust `DataType` (canonical, serialized to
`.cnnd` via `to_string` / `from_string`) and the Flutter widget tree.
Adding fields to `APIDataType` does **not** change `.cnnd`, the text format,
the evaluator, the `NetworkResult` payloads, or `can_be_converted_to`. All
existing call sites are unaffected; new sites read the new fields when they
need to (i.e. only when `data_type_base` is `Function` or `Iter`).

In scope:
- Two new `APIDataTypeBase` variants: `Function` and `Iter`.
- One new field on `APIDataType`: `children: Vec<APIDataType>`. Empty for
  every existing variant; non-empty only for `Function` / `Iter`.
- Rust-side converter changes (`api_data_type_to_data_type` /
  `data_type_to_api_data_type` in
  `rust/src/api/structure_designer/structure_designer_api.rs`).
- Flutter: a recursive `Iter` branch on `DataTypeInput` (just an inner
  `DataTypeInput` for the element type) and a new `FunctionTypeInput`
  widget (parameter list with add/remove + return-type sub-picker) nested
  as a `Function` branch on `DataTypeInput`.
- Updating the Rust→API conversion so that a `DataType::Function(...)` or
  `DataType::Iterator(...)` flowing back to the UI is rendered as the new
  structural variant rather than `Custom` — so a user who once typed
  `"Iter[Int]"` into the text box sees it as the structural Iter form on
  next paint (one-shot upgrade, no migration needed).

Out of scope:
- Anonymous records (`RecordType::Anonymous`). They remain text-only; the
  expression language is their only authoring path (existing convention,
  `doc/design_record_types.md`).
- Nested-array UI (`Array[Array[T]]`). The parser supports it; the UI
  doesn't surface it cleanly. Independently useful, separately small,
  deferred to its own follow-up (see §"Open questions / deferred").
- Context-aware picker restriction (e.g. hiding `Iter[T]` when the slot is
  a record-field type or an array element type, where the substrate
  rejects it anyway). The substrate already rejects at wire / eval time;
  picker stays permissive (consistent with how it treats every other
  illegal-in-context choice today).
- Combinators (`compose`, `flip`) — deferred separately in
  `doc/design_closures.md`.

## Build/test contract

| Must pass | When |
|---|---|
| `cd rust && cargo test --test structure_designer --test integration` green; new round-trip tests for Function/Iter through the API converters; clippy clean; `flutter_rust_bridge_codegen generate` runs as part of this phase since `src/frb_generated.rs` will not compile against the new `APIDataType` shape until regen; **`flutter analyze` clean** (Phase 1 owns the mechanical Dart-literal migration so the project builds end-to-end at the Phase 1 commit boundary — no new editor widgets yet) | Phase 1 |
| `flutter analyze` clean for touched files; `DataTypeInput` round-trips Function/Iter without dropping into the `Custom...` text branch; closure editor still works (its `_CustomParamRow` picks up the new branches "for free" via its inner `DataTypeInput`); manual walkthrough — drop a `parameter` node, set type to `Iter[(Int) -> Float]` via the picker (no text), connect a matching wire, observe a healthy graph | Phase 2 |

## Concept

### What `APIDataType` is, today

```rust
pub struct APIDataType {
    pub data_type_base: APIDataTypeBase,
    pub custom_data_type: Option<String>,
    pub array: bool,
}
```

A flat triple. `array: bool` handles one level of `Array[T]` wrapping at
the outermost level. `custom_data_type` is the free-form escape hatch for
the long tail: `Iter[T]`, `(T) -> U` / `(T0, T1) -> R`, `[[T]]`, anonymous
records, anything else `DataType::from_string` accepts that doesn't have a
first-class enum entry.

`DataTypeInput` (`lib/inputs/data_type_input.dart`) renders this as:
- A `DropdownButtonFormField<APIDataTypeBase>` over all variants.
- A `Record` branch with a named-def dropdown (separately authored).
- A `Custom...` branch with a `StringInput` (free-form text → Rust parser).
- An `Array` checkbox, hidden when base is `Custom` (Custom owns its own
  array semantics inside the text).

The `Custom...` escape hatch works (the Rust parser is solid) but is poor
UX for the two cases this change targets, both of which are growing in
prominence with iterators (`doc/design_iterators.md`) and closures
(`doc/design_closures.md`, `doc/design_custom_closure_kind.md`).

### What this change adds

| Aspect | Today | After |
|---|---|---|
| `APIDataTypeBase` | 26 variants incl. `Custom` | 28: + `Function`, + `Iter` |
| Nested structure on `APIDataType` | None — packed into `custom_data_type: String` | One `children: Vec<APIDataType>` field, semantically driven by the base |
| `Iter[T]` authoring | Type `"Iter[T]"` into Custom box | Dedicated branch: inner `DataTypeInput` for `T` |
| `Function(T0, T1, ..., Tn-1) -> R` authoring | Type `"(T0, T1) -> R"` into Custom box | Dedicated `FunctionTypeInput`: parameter list (add/remove) + return-type picker |
| `Array[Array[T]]` | Outer flag + inner Custom string | Unchanged (deferred) |
| Anonymous records | `Custom...` text only (expr language) | Unchanged |

### Children encoding (one field, two interpretations)

`children: Vec<APIDataType>` is interpreted *locally to the base variant*:

| Base | `children` length | Meaning |
|---|---|---|
| `Iter` | `1` | `children[0]` is the element type `T`. |
| `Function` | `N + 1` (N ≥ 0) | `children[0..N]` are parameter types in order, `children[N]` is the return type. |
| All other bases | `0` | No children. |

The Function encoding mirrors `ClosureData::type_args` for `ClosureKind::Custom`
(`doc/design_custom_closure_kind.md`) — `[params..., return]` with the
rightmost slot as the return type. Same mental model, same indexing.

A function with zero parameters is `children.len() == 1` (just the return
type). A zero-arg function value `() -> R` is a "thunk"; arity 0 is
legal on the picker even though the Custom-closure UI defers it
(`doc/design_custom_closure_kind.md` §"Open questions").

Empty `children` for an `Iter` or `Function` base is **not** a legal API
value — the API→Rust converter (`api_data_type_to_data_type`, see below)
rejects it with a hard error. The editor maintains the invariant at a
single point: the `DataTypeInput` dropdown-change handler seeds default
`children` *at the moment* the user switches the base to `Iter` or
`Function`, so an empty-`children` Iter/Function value is never sent to
Rust (see §"Defaults" and §"DataTypeInput — new branches"). The Rust→API
converter likewise always produces non-empty `children` for these bases
(`Iter` ⇒ 1 child, `Function` ⇒ ≥ 1 child).

### Why uniform children, not named slots

Two reasons:

1. **One new FRB field, not three.** The alternative is
   `element_type: Option<Box<APIDataType>>` + `parameter_types: Vec<APIDataType>`
   + `output_type: Option<Box<APIDataType>>`. That's three fields, two of
   which are unused for every variant. Uniform `children: Vec<APIDataType>`
   is one field and stays at zero length for every existing variant.
2. **Consistent encoding with `ClosureData::type_args`.** The Custom-closure
   editor (`doc/design_custom_closure_kind.md`) uses `[params..., return]`
   for its type_args; the same indexing for Function children lets a future
   refactor share code between the Custom-closure body editor and
   `FunctionTypeInput`.

The cost is the local-to-the-base interpretation rule, which is one line
of comment per field reader. Worth it.

### Why keep `Custom...`

After this change, every type the Rust parser accepts that doesn't have a
first-class API surface — anonymous records today; whatever new variants
appear later — still needs *some* way to be typed in by hand. `Custom...`
remains that escape hatch. Removing it would require committing to keeping
`DataTypeInput` exhaustively in sync with the Rust parser, which is a
strong claim. The agreed direction is: `Custom...` exists as a fallback;
the Rust→API converter prefers structural variants over `Custom` for any
type that *does* have first-class API surface.

## Data model

### Rust API types

`rust/src/api/structure_designer/structure_designer_api_types.rs`:

```rust
pub enum APIDataTypeBase {
    None, Bool, String, Int, Float,
    Vec2, Vec3, IVec2, IVec3, IMat3, Mat3,
    LatticeVecs, DrawingPlane, Geometry2D,
    Blueprint, HasAtoms, Crystal, Molecule,
    HasStructure, HasFreeLinOps, Motif, Structure, Unit,
    Record,
    /// `Iter[T]`: `children = [T]`.
    Iter,
    /// `Function((p0, p1, ..., pN-1) -> R)`:
    /// `children = [p0, p1, ..., pN-1, R]`.
    Function,
    Custom,
}

pub struct APIDataType {
    pub data_type_base: APIDataTypeBase,
    pub custom_data_type: Option<String>,
    pub array: bool,
    /// Recursive children, interpretation driven by `data_type_base`. See
    /// the per-variant table in `doc/design_structural_function_and_iter_types.md`.
    pub children: Vec<APIDataType>,
}
```

That's the entire data-model change. Every existing API site that
constructs an `APIDataType` adds `children: vec![]` (the compiler
enforces). Every API consumer that matches on `data_type_base` adds two
new arms (`Iter`, `Function`); the existing fallback `_ =>
APIDataTypeBase::Custom` covers everything else as today.

### FFI / Flutter mirror

FRB regenerates `APIDataType` with a new `List<APIDataType> children`
field. The const Dart constructor on `APIDataType` gains the parameter.
Existing call sites that pass `const APIDataType(...)` need to add
`children: const []`; non-const sites need `children: <APIDataType>[]`.
The analyzer enforces (the new field is required); sub-twenty sites,
all in node editors. This is one mechanical pass.

### Converters

`rust/src/api/structure_designer/structure_designer_api.rs`:

#### Rust → API (`data_type_to_api_data_type`)

Today this routes anything that isn't a flat variant or a `Named` record
to `APIDataTypeBase::Custom` with `custom_data_type: Some(data_type.to_string())`.
After this change, two new arms run *before* the existing `_ => Custom`
fallback:

```rust
DataType::Iterator(element) => {
    return APIDataType {
        data_type_base: APIDataTypeBase::Iter,
        custom_data_type: None,
        array: is_array,
        children: vec![data_type_to_api_data_type(element.as_ref())],
    };
}
DataType::Function(func) => {
    let mut children: Vec<APIDataType> = func
        .parameter_types
        .iter()
        .map(data_type_to_api_data_type)
        .collect();
    children.push(data_type_to_api_data_type(func.output_type.as_ref()));
    return APIDataType {
        data_type_base: APIDataTypeBase::Function,
        custom_data_type: None,
        array: is_array,
        children,
    };
}
```

`DataType::Record(RecordType::Anonymous(_))` still falls through to
`Custom` (out of scope).

#### API → Rust (`api_data_type_to_data_type`)

Two new match arms, each calling the converter recursively on
`api_data_type.children` and wrapping with the outer `Array` if
`api_data_type.array` is set:

```rust
APIDataTypeBase::Iter => {
    let element = api_data_type.children.first()
        .ok_or_else(|| "Iter type requires one child".to_string())?;
    let inner = api_data_type_to_data_type(element)?;
    let base = DataType::Iterator(Box::new(inner));
    return Ok(if api_data_type.array { DataType::Array(Box::new(base)) } else { base });
}
APIDataTypeBase::Function => {
    if api_data_type.children.is_empty() {
        return Err("Function type requires at least one child (the return type)".into());
    }
    let n = api_data_type.children.len() - 1;
    let parameter_types: Result<Vec<_>, _> = api_data_type.children[..n]
        .iter().map(api_data_type_to_data_type).collect();
    let output_type = api_data_type_to_data_type(&api_data_type.children[n])?;
    let base = DataType::Function(FunctionType {
        parameter_types: parameter_types?,
        output_type: Box::new(output_type),
    });
    return Ok(if api_data_type.array { DataType::Array(Box::new(base)) } else { base });
}
```

`Custom` keeps its current behavior (parse the text). A previously-typed
`"Iter[Int]"` text input round-trips as Custom on the first read but, after
any state-refresh that re-runs `data_type_to_api_data_type`, surfaces as
the structural Iter form. No migration needed; the next paint upgrades the
user's view.

### Defaults

The defaults live in exactly one place: the `DataTypeInput`
dropdown-change handler (today at `lib/inputs/data_type_input.dart:43`).
When the user picks a new base from the dropdown, the handler constructs
the outgoing `APIDataType` and is responsible for seeding `children`:

| New base | Seeded `children` | Resulting type |
|---|---|---|
| `Iter` | `[APIDataType(Float)]` | `Iter[Float]` |
| `Function` | `[APIDataType(Float), APIDataType(Float)]` | `(Float) -> Float` |
| all others | `const []` | unchanged from today |

`(Float) -> Float` matches the closure-editor default; `Float` matches
the existing "free slot defaults to Float" convention.

Switching *away* from `Iter`/`Function` to a flat base drops `children`
back to `const []` (the seeding rule re-fires every dropdown change; old
children for the prior base are discarded). Switching between `Iter`
and `Function` likewise replaces the seed — there is no carry-over.

Because the seeding happens at the dropdown-change boundary, the inner
branches in `DataTypeInput` (the `Iter[T]` `DataTypeInput`, the
`FunctionTypeInput`) can rely on `children` already having the
appropriate shape for the current base; `_childAt(0)` / `_functionParams()`
/ `_functionReturn()` are total functions, not partial.

### Validation

No new validation. The substrate's existing rules carry through
unchanged:
- `Iter[T] → Array[T]` is rejected (requires explicit `collect` —
  `can_be_converted_to`, `doc/design_iterators.md`).
- `Iter[T]` cannot be a record field type (rejected on record-def update).
- Captures cannot carry `Iter[T]` into a closure body (existing rule).
- `Function` arity / param / return mismatch on wire → existing
  `can_be_converted_to` rejection.

The picker stays permissive. A user *can* pick `Iter[Iter[T]]` for a
parameter type; the substrate will reject any wire that doesn't make
sense, the same way it does today for nonsensical `Custom...` text.

## Editor (Flutter)

All changes live in `lib/inputs/data_type_input.dart` (existing widget) +
one new sibling file for the Function helper widget.

### `DataTypeInput` — new branches

Two changes inside the same `build()` Column:

**Dropdown-change handler — seed `children` on switch.** The existing
handler (`onChanged:` of the `DropdownButtonFormField<APIDataTypeBase>`,
`lib/inputs/data_type_input.dart:43`) gains the seeding step from
§"Defaults":

```dart
List<APIDataType> seededChildren;
if (newValue == APIDataTypeBase.iter) {
  seededChildren = [_defaultFloat()];
} else if (newValue == APIDataTypeBase.function) {
  // Arity 1: one param + return = two children, both Float.
  seededChildren = [_defaultFloat(), _defaultFloat()];
} else {
  seededChildren = const [];
}
widget.onChanged(APIDataType(
  dataTypeBase: newValue,
  customDataType: customDataType,
  array: newValue == APIDataTypeBase.custom ? false : widget.value.array,
  children: seededChildren,
));
```

`_defaultFloat()` is a private helper returning
`APIDataType(dataTypeBase: APIDataTypeBase.float, customDataType: null, array: false, children: const [])`.

**Two new inner branches.** Alongside the existing Record / Custom
branches in the same `Column`:

```dart
// Iter[T] branch.
if (widget.value.dataTypeBase == APIDataTypeBase.iter)
  Padding(
    padding: const EdgeInsets.only(top: 8.0),
    child: DataTypeInput(
      label: 'Element Type',
      value: _childAt(0),
      onChanged: (newElement) => widget.onChanged(_withChildren([newElement])),
    ),
  ),

// Function((p0,...,pN-1) -> R) branch.
if (widget.value.dataTypeBase == APIDataTypeBase.function)
  Padding(
    padding: const EdgeInsets.only(top: 8.0),
    child: FunctionTypeInput(
      parameterTypes: _functionParams(),
      outputType: _functionReturn(),
      onChanged: (params, ret) =>
          widget.onChanged(_withChildren([...params, ret])),
    ),
  ),
```

The inner branches assume `children` is well-shaped for the current
base — guaranteed by the dropdown-change seeding plus the structural
Rust→API converter (see §"Children encoding"). The helpers do not need
to defend against an empty `children`.

Where `_childAt(i)`, `_withChildren(...)`, `_functionParams()`,
`_functionReturn()` are private helpers in the state class that mirror the
encoding rules in §"Data model".

The Array checkbox stays for all bases *except* `Custom` (as today), and
applies to the outermost level: `Array[Iter[T]]` is allowed (the array
flag wraps the structural inner type), the substrate decides whether it's
valid wire-side.

### `FunctionTypeInput` (new widget)

A new private widget in `lib/inputs/function_type_input.dart`:

```
┌── Function type ───────────────────────────────────────┐
│  Parameters                                            │
│  ┌──────────────────────────────────────────────┐     │
│  │ [DataTypeInput: Float                ] 🗑     │     │
│  │ [DataTypeInput: Int                  ] 🗑     │     │
│  └──────────────────────────────────────────────┘     │
│  + Add parameter                                       │
│  Return Type                                           │
│  [DataTypeInput: Float                            ]   │
└────────────────────────────────────────────────────────┘
```

Shape: a `Column` with one row per parameter (a single `DataTypeInput`
plus a delete `IconButton`), an "Add parameter" button, a divider, and a
"Return Type" `DataTypeInput`. The delete button is enabled at all
arities including 1 — function arity 0 (a thunk, `() -> R`) is a legal
type even though the Custom-closure UI defers authoring zero-arg
closures (`doc/design_custom_closure_kind.md` §Open questions).

The interface:

```dart
class FunctionTypeInput extends StatelessWidget {
  final List<APIDataType> parameterTypes;
  final APIDataType outputType;
  final void Function(List<APIDataType> params, APIDataType output) onChanged;
}
```

This widget is reusable: it knows nothing about closures. The Custom
closure-kind editor (`closure_editor.dart`) intentionally does **not**
migrate to this widget in v1 — its rows additionally carry parameter
*names* (a closure-only concern; function types have no names per the
load-bearing invariant in `doc/design_custom_closure_kind.md`). A
follow-up could share a `_ParamRow` base widget between the two, but it's
not on the critical path and is left as a §"Open questions" item.

### Dropdown labels

Add two entries to the kind dropdown label table in `_getDataTypeBaseDisplayName`:

```dart
case APIDataTypeBase.iter:
  return 'Iter[T]';
case APIDataTypeBase.function:
  return 'Function(args…) → R';
```

Lowercase / camelCase forms (`iter`, `function`) are what FRB will emit
from the Rust enum names (`Iter`, `Function`), matching every other
variant — no naming surprise.

## Repair, validation, evaluation

These layers don't see `APIDataType` — they operate on Rust `DataType`,
which is unchanged. The only adjacent path is the eventual
`api_data_type_to_data_type` call site (most of the property setters);
since those still produce the same `DataType` values they did before, no
downstream change is needed.

## Serialization (`.cnnd`)

Unchanged. `.cnnd` stores Rust `DataType` via `to_string()` /
`from_string()` (the same parser that the `Custom...` escape hatch uses).
Round-tripping a type through `APIDataType` is purely an in-memory FFI
shuffle. No fixture migration; no `#[serde(default)]` lever needed.

## Text format

Unchanged. The text format mirrors Rust `DataType`, not `APIDataType`.
`Iter[T]` and `(T0, T1) -> R` already round-trip today.

## "Custom..." escape hatch interaction

After this change, when the user opens a property that currently shows
`Custom: Iter[Int]`, the next paint (after any state refresh that re-runs
`data_type_to_api_data_type`) renders it as the structural Iter branch
with an inner `DataTypeInput` showing `Int`. The transition is invisible
beyond "their UI is now better"; no user action is required.

A user who *types* `"(Int) -> Float"` (or `"Int -> Float"`) into Custom
continues to get the same result they got before (the Rust parser accepts
it; eval sees the Function value); on next refresh it's promoted to the
structural branch.

`Custom...` continues to host the long tail (anonymous records, anything
new the Rust parser accepts that this design doesn't surface). Future
designs that add another structural branch (e.g. nested arrays, see
§"Open questions") follow the same pattern — add an `APIDataTypeBase`
variant, add a converter arm, add a `DataTypeInput` branch.

## Phases

Two phases, each green on the contract row above. Phase 1 is a
self-contained end-to-end-green deliverable: the Rust API change, FRB
regen, *and* the mechanical Dart literal migration land together so that
both `cargo test` and `flutter analyze` are clean at the Phase 1 commit
boundary — the project builds, just without the new editor surfaces.
Phase 2 is the Flutter editor.

### Phase 1 — Rust API + FRB regen + Dart literal migration + tests

In order:

- Add `Iter` and `Function` variants to `APIDataTypeBase`.
- Add `children: Vec<APIDataType>` to `APIDataType`.
- Update `data_type_to_api_data_type` to emit structural Iter/Function
  (recursive) before the `_ => Custom` fallback.
- Update `api_data_type_to_data_type` with the two new arms.
- Update every Rust call site that constructs an `APIDataType` literal
  to include `children: vec![]` (the compiler enforces; sub-twenty sites).
- Run `flutter_rust_bridge_codegen generate` — without it,
  `src/frb_generated.rs`'s `APIDataType` literals are missing the new
  field and the crate stops compiling.
- Migrate every `const APIDataType(...)` / `APIDataType(...)` call site
  in `lib/` to add `children: const []` / `children: <APIDataType>[]`.
  Analyzer-driven mechanical pass; sub-twenty sites, all in node editors.
  No editor wiring yet — these literals all correspond to existing flat
  bases (`Float`, `Int`, `Bool`, named `Record`, etc.), so `const []` is
  correct per the per-variant table in §"Children encoding".

New tests in `rust/tests/structure_designer/data_type_test.rs` (the
existing home for `DataType` tests — keep them together):

- `iter_int_roundtrip` — `DataType::Iterator(Box<Int>)` →
  structural API form → `DataType::Iterator(Box<Int>)`.
- `function_arity1_roundtrip` — `(Int) -> Float`.
- `function_arity0_roundtrip` — `() -> Float` (thunk).
- `function_arity3_roundtrip` — `(Int, Bool, Vec3) -> String`.
- `nested_iter_of_function_roundtrip` — `Iter[(Int) -> Float]`.
- `array_of_iter_roundtrip` — `array: true` + Iter base.
- `custom_text_iter_promotes_on_back_conversion` — start from a
  `Custom`-base `APIDataType { custom_data_type: Some("Iter[Int]"), .. }`,
  convert API → Rust → API, observe the result is the structural Iter
  variant, not Custom.

End state: `cargo clippy` clean, `cargo test --test structure_designer
--test integration` green, **`flutter analyze` clean**. The new
`children` field reaches the Dart side via the regenerated bindings and
every existing Dart literal compiles; no Flutter widget reads `children`
yet (that's Phase 2). The Phase 1 commit is a deployable, fully-building
intermediate state.

### Phase 2 — Flutter editor

Phase 1 left the project building; Phase 2 is purely additive editor
work:

- Extend `_getDataTypeBaseDisplayName` in
  `lib/inputs/data_type_input.dart` with cases for `iter` and `function`
  (`'Iter[T]'`, `'Function(args…) → R'`). The switch is exhaustive over
  `APIDataTypeBase` and will not analyze-clean without these.
- New file `lib/inputs/function_type_input.dart` implementing
  `FunctionTypeInput` per §"Editor".
- Extend `lib/inputs/data_type_input.dart`:
  - Two new `if` branches in `build()` (Iter, Function), wired to inner
    `DataTypeInput` / `FunctionTypeInput`.
  - Helper methods on the state class for `children` reads/writes.
- The `Custom...` branch stays exactly as it is; the Record dropdown,
  array checkbox, and all preset bases are untouched.
- Manual walkthrough added to `lib/structure_designer/node_data/AGENTS.md`:
  *(a)* drop a `parameter` node, switch its type to `Iter[Float]` via
  the picker (no text); *(b)* switch a parameter to `(Int, Bool) -> String`
  via the structural Function branch; *(c)* drop a `closure` set to
  Custom kind, observe its parameter rows expose the new Iter/Function
  branches via their inner `DataTypeInput`.
- `flutter analyze` clean for touched files.

## Reuse map

**Reused unchanged:**
- `DataType::from_string` (Rust parser) — the `Custom...` text path still
  goes through it.
- The Record-def dropdown (`_RecordDefDropdown`).
- The closure editor (`closure_editor.dart`) — its `_CustomParamRow`'s
  type slot is a `DataTypeInput`, so Iter/Function become available
  inside Custom-closure parameter types for free.
- `.cnnd` serialization and the text format.

**Reused with small changes:**
- `APIDataTypeBase` (two new variants).
- `APIDataType` (one new field; mechanical migration of every literal
  site).
- `api_data_type_to_data_type` / `data_type_to_api_data_type` (two new
  arms each).
- `DataTypeInput` (two new branches; two new dropdown labels).

**New from scratch:**
- `FunctionTypeInput` widget in `lib/inputs/function_type_input.dart`.
- New round-trip tests appended to
  `rust/tests/structure_designer/data_type_test.rs`.

**Deleted:** nothing. `Custom...` stays.

## Open questions / deferred

1. **Nested-array UI (`Array[Array[T]]`).** The parser supports it; the
   UI doesn't surface it structurally. Easiest path: when `array: true`
   is checked on a base whose underlying type already wraps another
   `Array`, surface a nested inner `DataTypeInput` instead of routing
   through Custom. Deferred — distinct enough in scope and small enough
   on its own to land separately.
2. **Sharing `FunctionTypeInput` with the Custom-closure editor.** The
   closure editor's `_CustomParamRow` is structurally a parameter row
   that *additionally* carries a name field. After this change, the
   "row minus name" is exactly `FunctionTypeInput`'s row. A future
   refactor could extract a shared `_ParamRow` with an optional name
   slot, leaving `FunctionTypeInput` to use the no-name form and
   `closure_editor.dart` to use the with-name form. Skipped in v1 — the
   duplication is light and the closure editor's UX may diverge further
   from a bare function-type picker as features land.
3. **Context-aware picker restriction.** A record-field-type slot
   *should* hide `Iter[T]` (the substrate rejects it). Same for
   array-element slots. Skipped in v1: the picker has always been
   permissive, substrate rejection is the source of truth, and adding
   context-awareness on a per-slot basis is a separate UX project
   (likely involving a `restrict_to:` set passed into `DataTypeInput`).
4. **Removing `Custom...`.** Once `Iter` and `Function` are structural,
   the remaining users of the text escape hatch are anonymous-record
   literal types (only authorable from the expression language anyway)
   and the very long tail of "future types the picker hasn't caught up
   on yet". Removing the escape hatch would require committing to keep
   the picker exhaustively synced with the Rust parser, which is a
   stronger contract than we want today. Keep it.
5. **Default Function arity choice.** v1 defaults to arity 1
   (`(Float) -> Float`) matching the existing closure default. A future
   tweak could default to arity 0 (a thunk, `() -> Float`) on the theory
   that "Add parameter" is a more discoverable action than "Remove
   parameter". Trivial to flip later — one constant.
