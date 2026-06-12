# `Optional[T]` Data Type

## Status: Draft

First consumer: per-region materialization settings (`doc/design_materialize_regions.md`), which needs record fields that distinguish "set to a value" from "unset / inherit". This feature is deliberately specified as a standalone type-system building block because the need is general: any future record schema or node property with override/inherit semantics will reuse it.

## Motivation

atomCAD's type system has no way to express "a `T`, or nothing". The gap shows up wherever a record field or pin needs three-valued semantics:

- **Override vs. inherit.** `MaterializeRegion` (see `doc/design_materialize_regions.md`) wants each settings field to mean "force on", "force off", or "not specified here — inherit from the enclosing scope". A plain `Bool` cannot express the third state.
- **Future shaped inputs.** Any built-in record def carrying optional knobs (tolerances, overrides, annotations) hits the same wall, and so does any node that wants an explicitly-nullable property flowing along a wire.

Workarounds considered and rejected:

- **Sentinel encodings** (`Int` with `-1` = unset, `String` enums `"inherit"/"on"/"off"`): stringly/magic-number typed, invisible to the type checker, and each field invents its own convention.
- **A dedicated `OptionalBool` primitive**: barely cheaper than the general feature (it still touches the `DataType` enum, conversion rules, type parser, and the Flutter type selector) and is a dead end the moment a record wants `Optional[Float]` — which `MaterializeRegion.margin` already does.

So: bite the bullet, add `Optional[T]` — but with a representation choice that makes the bullet small.

## Core Decision: Nullable Union, Not a Wrapper

`Optional[T]` is a **nullable** type — its values are either an ordinary `T` value or the null — **not** a `Some(T)`-wrapped container. This single decision is what keeps the feature cheap, because the value layer already has the null:

- `NetworkResult::None` exists today as the canonical "no value" (it is what a disconnected input pin evaluates to, and `evaluate_or_default` handles it throughout the evaluator).
- Under the nullable design, the runtime representation of an `Optional[T]` value is *either a plain `T`-shaped `NetworkResult` or `NetworkResult::None`*. **No new `NetworkResult` variant, no wrapping, no unwrapping.**
- Consequently `T → Optional[T]` is an **identity at the value layer** — exactly a *tag-only widening*, the class of conversion the record system already accepts at field positions via `is_tag_only_widening` (`data_type.rs`). The new rules slot into the existing predicate instead of fighting it, and `record_destructure` keeps passing payloads through unchanged.
- The `record_construct` UX falls out for free: an `Optional[T]` field maps to an *optional input pin*; a disconnected pin already evaluates to `NetworkResult::None`, which **is** the null. No "none literal" node is needed for v1.

The price of untagged null is that nesting is meaningless: a `None` inside `Optional[Optional[T]]` cannot distinguish "no outer value" from "outer value present, inner is none". Nesting is therefore **forbidden** (see Restrictions) — an acceptable trade; no anticipated consumer needs it.

## Design

### 1. Type representation

```rust
// data_type.rs
pub enum DataType {
    ...
    /// Nullable `T`: the value is either an ordinary `T` or
    /// `NetworkResult::None`. No wrapper at the value layer.
    /// `Optional[Optional[_]]`, `Optional[Iter[_]]`, `Optional[Unit]` and
    /// `Optional[None]` are ill-formed (rejected at every construction
    /// site). See `doc/design_optional_type.md`.
    Optional(Box<DataType>),
    ...
}
```

`Display` emits `Optional[T]` (consistent with `Iter[T]` / `[T]`); `DataType::from_string` / the type parser (`parse_builtin_type`) gain the matching arm. A `T?` sugar is *not* added in v1 (see Open Questions).

### 2. Conversion rules

In `can_be_converted_to_impl` (ordered before the catch-all):

| Rule | Condition | Runtime effect |
|---|---|---|
| `Optional[S] → Optional[T]` | `S → T` (recursive, non-top-level) | null passes through as null; payload converts per inner rule |
| `S → Optional[T]` (S not Optional) | `S → T` (recursive, non-top-level) | payload converts per inner rule |
| `Optional[S] → T` (T not Optional) | **rejected** | — (this is the whole point; the one exception is the universal `T → Unit` discard, which keeps applying) |

Notes:

- `S → Optional[T]` deliberately uses the **full** conversion rule, not just tag-only, because wires need it: a user dropping an `int` node onto a `record_construct` pin typed `Optional[Float]` expects the ordinary `Int → Float` conversion to fire. `convert_to` handles the payload exactly as it would for a bare `T` destination, and maps `NetworkResult::None → NetworkResult::None`.
- **Record field positions stay strict automatically.** The field-level path goes through `is_tag_only_widening`, which is extended with the tag-only subset only: `S → Optional[T]` and `Optional[S] → Optional[T]` where `S == T` or `is_tag_only_widening(S, T)`. So `{x: Int} → {x: Optional[Float]}` is rejected at a field position (value-converting), while `{x: Crystal} → {x: Optional[HasAtoms]}` is accepted (tag-only) — consistent with how bare fields behave today.
- The strict drag-adapter variant (`can_be_strictly_converted_to`) mirrors the same two arms with its own recursion, so scalar broadcast cannot leak in through an Optional element type.
- The generic broadcasts compose without special cases: `S → Array[Optional[T]]` works iff `S → Optional[T]`, etc.
- `Optional` participates in `canonicalize_data_type` (recurse into the inner type) and in the record-rename `DataType` walks.

### 3. Restrictions (ill-formed Optionals)

Rejected at **every** type-construction site:

- `Optional[Optional[T]]` — untagged null makes nesting ambiguous (see Core Decision).
- `Optional[Iter[…]]` — iterators are already banned from record fields and closure captures; an optional lazy walker has no meaningful semantics.
- `Optional[Unit]` and `Optional[None]` — degenerate.

Enforcement sites:

1. The text-format / `from_string` type parser (parse error with message).
2. The Flutter `DataTypeInput` (the Optional toggle is disabled/cleared when the inner type is itself Optional/Iter/Unit, and vice versa).
3. Registry validation of record type defs (`validate_registry`-style pass + `add_record_type_def` / `update_record_type_def`), guarding `.cnnd` files that smuggle in ill-formed shapes.

### 4. Runtime

No evaluator changes are required for the type itself:

- `NetworkResult::None` inhabits every `Optional[T]`. `infer_data_type()` of `None` remains `DataType::None`; nothing resolves an output pin *to* `Optional` implicitly — Optional types appear only where declared (pins, record fields).
- `convert_to(value, Optional[T])`: `None` stays `None`; anything else converts against `T`.
- A displayed output pin of type `Optional[T]` renders nothing in the viewport (same as other primitives) — no scene-builder change.

### 5. Records integration

- **`record_construct`**: a field typed `Optional[T]` becomes an **optional** input pin (`get_parameter_metadata` derives required-ness from the field type). Unwired pin → the emitted record stores `NetworkResult::None` for that field. Emitted records always carry **all** fields in canonical order — "unset" is an explicit `None` value, never a missing field, so record subtyping and the destructure passthrough invariant are untouched.
- **`record_destructure`**: the field's output pin is typed `Optional[T]` and passes the stored value through unchanged (`None` or payload).
- **`product`**: no special case — an `Optional[T]` field consumes `Iter[Optional[T]]` like any other field type, via the generic rules.
- Pin layouts re-derive through the existing `repair_node_network` machinery when a def gains/loses Optional-ness; wires that become incompatible (e.g. `Optional[T]` source into a now-bare-`T` pin) are disconnected by the ordinary repair pass.

### 6. Out of scope for v1

- **`expr` language**: no `none` literal, no null-coalescing operator, and `Optional` is rejected in expr type positions. Nodes consuming Optionals do so in Rust (`NetworkResult::None` match arms). Revisit when an expr-level use case appears.
- **A `none`-producing node**: a disconnected optional pin covers every known construction path. Add a literal node only when a use case demands wiring an explicit null *through* a network.
- **Text-format property values**: no node stores Optional-typed *properties* yet, so `TextValue` is unchanged. The `Optional[T]` *type syntax* (for record-def declarations and pin types) is in scope.

### 7. API / Flutter

- `APIDataTypeBase` gains an `Optional` variant represented like `Iter`: one entry in `children` carrying the inner type (the existing recursive-children mechanism; the outermost-`array` bool is orthogonal and unchanged).
- `DataTypeInput` gains an "Optional" toggle alongside the existing array/iter affordances, with the nesting guards from §3. The record `SchemaEditor` inherits it via `DataTypeInput` with no extra work.
- Pin tooltips / type strings flow through `Display` and need no special handling.

### 8. Serialization & compatibility

- `DataType` is serde-serialized; the new variant round-trips with no migration (old `.cnnd` files cannot contain it).
- Standard forward-compat caveat: a project saved with `Optional` types will not load in older builds. No version bump — same policy as previous additive `DataType` variants (`Unit`, `Iterator`, matrices).

## Phasing

### Phase 1 — Core type & conversions (Rust)

1. `DataType::Optional` variant + `Display` + type-parser arm (incl. ill-formed-nesting rejection).
2. Conversion rules in `can_be_converted_to_impl`, the strict variant, and the `is_tag_only_widening` extension (§2).
3. `convert_to` runtime arm (§4).
4. `canonicalize_data_type` recursion + record-rename walk coverage.
5. Registry validation of record defs against §3.

Tests (`rust/tests/structure_designer/`): conversion matrix (`T → Optional[T]`, `Int → Optional[Float]` wire-accepted, `Optional[T] → T` rejected, `Optional[Crystal] → Optional[HasAtoms]` accepted, `{x: Int} → {x: Optional[Float]}` field-rejected, `{x: Crystal} → {x: Optional[HasAtoms]}` field-accepted, `Optional[T] → Unit` accepted); parser round-trip incl. rejection of the four ill-formed shapes; `convert_to` null/payload behavior.

### Phase 2 — Records & nodes integration

1. `record_construct`: optionality of pins derived from field types; unwired → `None` field value; emitted records carry all fields.
2. `record_destructure`: `Optional[T]` pin typing + unchanged passthrough.
3. `repair_node_network` interaction: def edits that flip Optional-ness refresh pin layouts and disconnect incompatible wires (existing machinery; add coverage).
4. `.cnnd` round-trip with a user def containing Optional fields.

Tests: construct-with-unwired-optional-pin → destructure → `None` comes out; subtyping through wires; roundtrip fixture; repair on def edit.

### Phase 3 — Flutter UI + API

1. `APIDataTypeBase::Optional` + conversion in both directions (`flutter_rust_bridge_codegen generate`).
2. `DataTypeInput` toggle + nesting guards; `SchemaEditor` verification.
3. `flutter analyze` clean; manual pass: declare a record def with `Optional[Bool]` / `Optional[Float]` fields, build construct/destructure chains, wire an `int` into an `Optional[Float]` pin, leave optional pins unwired.

## Open Questions

1. **`T?` sugar.** Should the parser/Display additionally accept (or emit) `Float?`? Proposed: no for v1 — one canonical spelling (`Optional[Float]`), consistent with `Iter[T]`; sugar can be added later without breaking round-trips only if Display keeps emitting the canonical form.
2. **`Optional[Function(...)]`.** No known hazard (function values are ordinary `NetworkResult`s) and no known consumer. Proposed: allow, but flag here so a reviewer confirms.
3. **expr-language support.** When expr eventually grows record-literal interplay with optional fields, it will need a `none` literal and probably a coalescing form (`a ?? b`). Deliberately deferred.
