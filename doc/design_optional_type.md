# `Optional[T]` Data Type

## Status: Draft

First consumer: per-region materialization settings (`doc/design_blueprint_region_atom_edits.md`, Part B), which needs record fields that distinguish "set to a value" from "unset / inherit". This feature is deliberately specified as a standalone type-system building block because the need is general: any future record schema or node property with override/inherit semantics will reuse it.

## Motivation

atomCAD's type system has no way for a **record field** to express "a `T`, or nothing". (Pins do not need this — every pin is *already* implicitly nullable, since a disconnected input evaluates to `NetworkResult::None`. The gap is specifically at record fields, the one place the system enforces presence.) It shows up wherever a record field needs three-valued semantics:

- **Override vs. inherit.** `MaterializeRegion` (see `doc/design_blueprint_region_atom_edits.md` §B1) wants each settings field to mean "force on", "force off", or "not specified here — inherit from the enclosing scope". A plain `Bool` cannot express the third state.
- **Future shaped inputs.** Any built-in record def carrying optional knobs (tolerances, overrides, annotations) hits the same wall.

Workarounds considered and rejected:

- **Sentinel encodings** (`Int` with `-1` = unset, `String` enums `"inherit"/"on"/"off"`): stringly/magic-number typed, invisible to the type checker, and each field invents its own convention.
- **A dedicated `OptionalBool` primitive**: barely cheaper than the general feature (it still touches the `DataType` enum, conversion rules, type parser, and the Flutter type selector) and is a dead end the moment a record wants `Optional[Float]` — which `MaterializeRegion.margin` already does.

So: bite the bullet, add `Optional[T]` — but with a representation choice that makes the bullet small.

## Core Decision 1: Nullable Union, Not a Wrapper

`Optional[T]` is a **nullable** type — its values are either an ordinary `T` value or the null — **not** a `Some(T)`-wrapped container. This single decision is what keeps the feature cheap, because the value layer already has the null:

- `NetworkResult::None` exists today as the canonical "no value" (it is what a disconnected input pin evaluates to, and `evaluate_or_default` handles it throughout the evaluator).
- Under the nullable design, the runtime representation of an `Optional[T]` value is *either a plain `T`-shaped `NetworkResult` or `NetworkResult::None`*. **No new `NetworkResult` variant, no wrapping, no unwrapping.**
- Consequently `T → Optional[T]` is an **identity at the value layer** — the payload (or `None`) is unchanged.

## Core Decision 2: `Optional` Is a Record-Field Modifier, Never a Pin Type

The required-vs-optional distinction is **only meaningful at record fields**, because that is the only place in the system that enforces value presence: `record_construct::eval` collapses the whole record to `None` the moment any field resolves to `None`. **Everywhere else — every input pin, every output pin — a value is already implicitly nullable.** A disconnected input evaluates to `NetworkResult::None`, and `convert_to` passes `None` through unchanged on any pin. So an `Optional[T]` *tag on a pin* would carry no information the evaluator does not already assume, while actively creating dead-end ("trapped") values: a v1 `Optional[T]` pin output could only flow into another `Optional` pin or a `Unit` discard, because there is no unwrap/coalesce/`none`-literal in v1.

Therefore:

- **`Optional[T]` appears only in record-field *declarations*** (and the record-subtyping that compares them). It is never the type of a pin and never travels on a wire.
- **`record_construct`** input pins for a field of declared type `Optional[T]` are typed plain **`T`**. The optional behavior is driven by the *field type in the record def*, which `eval` already has in hand: a field declared `Optional[T]` whose pin resolves to `None` is **kept** as an explicit `None` in that slot instead of collapsing the record. "This pin may be left unwired" is conveyed by pin metadata (`get_parameter_metadata`), not by the pin's type.
- **`record_destructure`** output pins are typed plain **`T`**, *not* `Optional[T]`. This is exactly identity at the value layer — an `Optional[T]` field already stores "a `T`-shaped value or `None`," so projecting it onto a `T` output pin passes the payload (or `None`) straight through, and every downstream consumer handles `None` exactly as it must for any pin. No value is trapped.

Because `Optional` never reaches a wire, **no wire-level conversion rules are needed for it** (see §2): the entire `S → Optional[T]` / `Optional[S] → Optional[T]` machinery in `can_be_converted_to_impl`, its strict-broadcast mirror, and the `convert_to(value, Optional[T])` runtime arm all disappear. The only conversion handling Optional needs lives at **record-field positions during record subtyping**, which is its natural home (`can_be_structurally_converted_to`).

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

Because `Optional` never appears on a pin, there are **no arms in `can_be_converted_to_impl`** (the wire-level entry) and **no `convert_to` runtime arm** for it. The only place an `Optional[T]` is compared against another type is **record subtyping** — when one record's field is `Optional`-typed and the corresponding field in the other record is `Optional` or bare. That comparison goes through the field-level structural predicate `can_be_structurally_converted_to` (which already carries `&NodeTypeRegistry`, so it can resolve `Optional[Record(Named)]` inner types correctly).

Extend the **field-level** predicate (`can_be_structurally_converted_to`, *not* the registry-free `is_tag_only_widening`) with two arms, mirroring how it already recurses through `Array`:

| Field-position rule | Condition |
|---|---|
| `Optional[S] → Optional[T]` | `can_be_structurally_converted_to(S, T)` |
| `S → Optional[T]` (S not Optional) | `can_be_structurally_converted_to(S, T)` — promoting a present value to a maybe-present one |

Notes:

- The reverse, `Optional[S] → T` at a field position, is **rejected** — a maybe-present value cannot satisfy a field that requires presence. (`T → Unit` discard still applies if it ever reached here, but `Unit` is not a record field type.)
- These stay **strictly tag-only** at the leaf, exactly like bare fields: `{x: Int} → {x: Optional[Float]}` is rejected (value-converting), while `{x: Crystal} → {x: Optional[HasAtoms]}` is accepted (tag-only), and `{x: Record(A)} → {x: Optional[Record(B)]}` follows the ordinary record structural rule because the predicate has the registry. Putting these arms in `is_tag_only_widening` instead would be wrong: that function is registry-free and has no record arm, so it would spuriously reject `Optional`-wrapped record/array inner types that the bare field accepts.
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

- `NetworkResult::None` inhabits every `Optional[T]`. `infer_data_type()` of `None` remains `DataType::None`. Since `Optional` is never a pin type, nothing resolves an output pin *to* `Optional`; it appears only in record-field declarations.
- No `convert_to` arm is needed: `Optional` values only ever sit *inside* a record field, where the stored payload (or `None`) is held verbatim, and they leave a record onto a plain `T` pin via `record_destructure` with no conversion. The existing `NetworkResult::None => return self` arm in `convert_to` already keeps `None` as `None` on the destructure output pin.

### 5. Records integration

- **`record_construct`**: a field declared `Optional[T]` gets an input pin typed plain **`T`** (the value/wire layer never sees `Optional`). `get_parameter_metadata` marks that pin **not required** so the UI/`describe` know it may be left unwired. At `eval`, the field's *declared* type drives collapse behavior (see below): an Optional field that resolves to `None` is **kept** as an explicit `None`; a required field that resolves to `None` collapses the record. Emitted records always carry **all** fields in canonical order — "unset" is an explicit `None` value, never a missing field, so record subtyping and the destructure passthrough invariant are untouched.
- **`record_destructure`**: the field's output pin is typed plain **`T`** (not `Optional[T]`) and passes the stored value through unchanged (`None` or payload). This is identity at the value layer and means a destructured field flows into any ordinary `T` consumer — no trapped values.
- **`product`**: no special case — `product` consumes `Iter[T]` per field as today; the target record's field being `Optional` is invisible to the value/wire layer.
- Pin layouts re-derive through the existing `repair_node_network` machinery when a def gains/loses Optional-ness. Flipping a field between `T` and `Optional[T]` does **not** change the pin's *type* (both are `T`), so wires are not disconnected; only the construct pin's required-ness metadata and the eval-time collapse behavior change.

#### Current `record_construct` semantics & the required change

Today (pre-Optional) `record_construct::eval` is **all-or-nothing**. For each field it resolves a value as **wired pin > stored UI literal > `None`** (a wired pin wins; an unwired pin uses its stored literal if that literal still coerces to the field type; otherwise the field is `None`). It then short-circuits: the moment *any* field resolves to `NetworkResult::None`, the whole node returns `NetworkResult::None` (and any field resolving to `Error` short-circuits to that error). Only if **every** field obtained a non-`None` value does it emit the record. So a record is emitted iff, for every field, either its input pin supplies a non-`None` value or its UI literal is filled.

This short-circuit is what `Optional[T]` fields must opt out of. The Phase 2 change makes the `None`-collapse **conditional on the field type**:

- A **non-Optional** (required) field that resolves to `None` still collapses the whole record to `None`, exactly as today.
- An **Optional[T]** field that resolves to `None` is **kept** as an explicit `None` value in that field's slot and does **not** collapse the record (consistent with the §5 rule that emitted records always carry all fields).

`Error` propagation is unchanged — an `Error` in any field, Optional or not, still short-circuits to that error. The not-required flag surfaced by `get_parameter_metadata` (Optional field ⇒ pin may be unwired) is cosmetic — it feeds the `describe` text command and pin styling, not a validation gate — and this eval-level exemption is the one that actually matters; keep them in sync so the UI doesn't mislead.

**Literal vs. `None` on an unwired Optional field.** Today `record_construct::eval` resolves each field **wired > stored literal > `None`**, where "stored literal" means a present entry in `literal_values` (keyed by field name). **This order is kept unchanged for Optional fields** — the only eval change is the collapse exemption above. "Unset / inherit" is therefore represented by the **absence** of a `literal_values` entry *and* no wire, which already falls through to `None`. This is more expressive than forcing unwired-⇒-`None`: a user can still express "force this value" inline by typing a literal (e.g. `freeze: true` / `freeze: false`) **without** wiring a constant node — exactly the three-state UX the `MaterializeRegion` use case wants (force-on / force-off / inherit). The work is on the UI side: an Optional field's literal editor must offer a **clearable / tri-state** affordance (a true "unset" that removes the map entry) rather than a plain `Bool`/`Float` box that always holds a value. A freshly-added `record_construct` defaults Optional fields to *no* literal entry (⇒ `None` ⇒ inherit).

### 6. Out of scope for v1

- **`expr` language**: no `none` literal, no null-coalescing operator, and `Optional` is rejected in expr type positions. Nodes consuming Optionals do so in Rust (`NetworkResult::None` match arms). Revisit when an expr-level use case appears.
- **A `none`-producing node**: a disconnected optional pin covers every known construction path. Add a literal node only when a use case demands wiring an explicit null *through* a network.
- **Text-format property values**: no node stores Optional-typed *properties* yet, so `TextValue` is unchanged. The `Optional[T]` *type syntax* (for record-def field declarations) is in scope.
- **Optional on pins**: out of scope by construction — `Optional` is a record-field modifier only (see Core Decision). If a future need arises to flow an explicitly-nullable value *along a wire* (distinct from the implicit nullability every pin already has), that is when an unwrap / coalesce / `none`-literal story gets designed.

### 7. API / Flutter

- `APIDataTypeBase` gains an `Optional` variant represented like `Iter`: one entry in `children` carrying the inner type (the existing recursive-children mechanism; the outermost-`array` bool is orthogonal and unchanged). Needed because record-field types cross the FFI boundary in the `SchemaEditor`.
- `DataTypeInput` gains an "Optional" toggle alongside the existing array/iter affordances, with the nesting guards from §3 — but it is **only meaningful inside the record `SchemaEditor`** (declaring a field type). It does not appear as a pin-type affordance, since `Optional` is never a pin type.
- Pin tooltips / type strings: pins are always `T`, so nothing renders `Optional` outside the schema editor. Record-field type strings flow through `Display` and need no special handling.

### 8. Serialization & compatibility

- `DataType` is serde-serialized; the new variant round-trips with no migration (old `.cnnd` files cannot contain it).
- Standard forward-compat caveat: a project saved with `Optional` types will not load in older builds. No version bump — same policy as previous additive `DataType` variants (`Unit`, `Iterator`, matrices).

## Phasing

### Phase 1 — Core type & field-position subtyping (Rust)

1. `DataType::Optional` variant + `Display` + type-parser arm (incl. ill-formed-nesting rejection).
2. Field-position subtyping arms in `can_be_structurally_converted_to` (§2). **No** changes to `can_be_converted_to_impl`, no strict-variant mirror, no `convert_to` arm — `Optional` never reaches a wire.
3. `canonicalize_data_type` recursion + record-rename walk coverage.
4. Registry validation of record defs against §3.

Tests (`rust/tests/structure_designer/`): field-position matrix (`{x: Crystal} → {x: Optional[HasAtoms]}` accepted, `{x: Int} → {x: Optional[Float]}` rejected, `{x: Record(A)} → {x: Optional[Record(B)]}` follows the record structural rule, `{x: Optional[T]} → {x: T}` rejected, `{x: Optional[S]} → {x: Optional[T]}` per inner); parser round-trip incl. rejection of the four ill-formed shapes.

### Phase 2 — Records & nodes integration

1. `record_construct`: input pin for an `Optional[T]` field typed plain `T`; `get_parameter_metadata` marks it not-required; eval keeps `None` for Optional fields (driven by declared field type) and collapses for required fields; emitted records carry all fields. Literal resolution order is unchanged (§5) — the tri-state literal editor is Phase 3 UI work.
2. `record_destructure`: output pin typed plain `T`; unchanged passthrough of `None`/payload.
3. `repair_node_network` interaction: flipping a field's Optional-ness keeps the pin type `T` (no wire disconnect) but refreshes the construct pin's required-ness/eval behavior; add coverage.
4. `.cnnd` round-trip with a user def containing Optional fields.

Tests: construct-with-unwired-optional-pin (no literal) → destructure → `None` flows into a plain `T` consumer; required field unwired → record collapses; unwired Optional field **with** a stored literal → that literal value (resolution order unchanged); roundtrip fixture; flip Optional-ness on a def in use.

### Phase 3 — Flutter UI + API

1. `APIDataTypeBase::Optional` + conversion in both directions (`flutter_rust_bridge_codegen generate`).
2. `DataTypeInput` "Optional" toggle + nesting guards, surfaced **only in the `SchemaEditor`** (record-field declaration), not as a pin affordance.
3. `record_construct` literal editor: for an Optional field, a **clearable / tri-state** literal affordance (true / false / unset for `Optional[Bool]`; value / unset for `Optional[Float]`), where "unset" removes the `literal_values` entry. Default = unset.
4. `flutter analyze` clean; manual pass: declare a record def with `Optional[Bool]` / `Optional[Float]` fields, build construct/destructure chains, leave optional construct pins unwired (unset) and confirm the destructured `T` output reads `None` and feeds an ordinary consumer; set a literal and confirm it overrides.

## Open Questions

1. **`T?` sugar.** Should the parser/Display additionally accept (or emit) `Float?`? Proposed: no for v1 — one canonical spelling (`Optional[Float]`), consistent with `Iter[T]`; sugar can be added later without breaking round-trips only if Display keeps emitting the canonical form.
2. **`Optional[Function(...)]`.** No known hazard (function values are ordinary `NetworkResult`s) and no known consumer. Proposed: allow, but flag here so a reviewer confirms.
3. **expr-language support.** When expr eventually grows record-literal interplay with optional fields, it will need a `none` literal and probably a coalescing form (`a ?? b`). Deliberately deferred.
