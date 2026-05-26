# Custom Closure Shapes — User-Specified Function Types for `closure` / `apply`

## Scope

This document designs a fifth `ClosureKind` — **`Custom`** — that lets the user
specify the function shape of a `closure` (or `apply`) node directly: arbitrary
arity, an arbitrary type per parameter, an arbitrary return type, and authored
parameter names. The four existing preset kinds (`Map`, `Filter`, `Fold`,
`Foreach`) are kept exactly as-is — they remain the recommended choice when the
goal is to drop the resulting `Function` value into a matching HOF's `f` pin by
construction. `Custom` is the escape hatch for everything else: function-factory
subnetworks whose output type isn't one of the four HOF shapes, calling sites
driven by `apply`, and any future combinator that needs an arbitrary signature.

The change is purely additive in shape — no preset's storage layout, evaluator
behavior, validator pass, or repair path is altered. The only new mental-model
item is that `Custom` separates *parameter names* from *parameter types*: the
four presets carry names statically on the kind, but a user-authored signature
needs to carry both. We resolve this with one new sibling field on
`ClosureData` / `ApplyData` (a parallel `Vec<String>`), populated only for
`Custom`.

**Load-bearing invariant**: parameter names are a *local UI labeling concern
only*. They are consumed when labeling the closure's zone-input pins and the
apply node's arg pins, and nowhere else. They MUST NOT be read into
`DataType::Function(FunctionType { parameter_types, output_type })` — the
function type is purely structural (see `FunctionType` in `data_type.rs`,
which has no name field) and wire compatibility (`can_be_converted_to`) is
purely structural too. Two closures with the same param types and return type
but different param names are the same function value.

In scope:
- The new `ClosureKind::Custom` variant and the helper-method changes it
  forces (`param_types` / `return_type` / `param_names` / `result_name` /
  `num_type_args` / `function_type`).
- The new `param_names: Vec<String>` field on `ClosureData` / `ApplyData`
  (and their API mirrors), plus the encoding of `type_args` for `Custom`.
- The Flutter editor extension (the Custom branch of `ClosureShapeEditor`,
  composing existing `DataTypeInput` widgets — no new generic type-selector
  widget).
- Kind transitions (preset ⇄ Custom data preservation).
- `.cnnd` backward compatibility for the new field.
- Text-format syntax for the `Custom` kind.

Out of scope:
- A recursive `Function(...)` *branch* on `DataTypeInput` itself. A parameter
  whose type is `Function(...)` is reachable in v1 only through
  `DataTypeInput`'s existing `Custom...` text escape hatch (parsed by
  `DataType::from_string`). Adding a dedicated Function branch is an
  independently useful follow-up — see §"Open questions / deferred".
- Iterator-typed parameters: same situation, same escape hatch. Note also
  that `Iter[T]` cannot be captured into a closure anyway
  (`doc/design_iterators.md`) so this is a narrow loss.
- Combinator nodes (`compose`, `flip`) — already deferred in
  `doc/design_closures.md`.
- A name-collision policy beyond local-identifier validation on each row
  (the body, not the editor, is the authority on what names mean in scope).

## Build/test contract

| Must pass | When |
|---|---|
| `cd rust && cargo test` green; new tests for `ClosureKind::Custom` round-trip, helpers, custom-typing of closure/apply pins, and back-compat .cnnd load | Phase 1 |
| `cd rust && cargo clippy` clean | Phase 1 |
| `flutter_rust_bridge_codegen generate` succeeds | Phase 2 |
| `flutter run` launches; switching kinds preserves overlap; Custom branch adds/removes parameter rows; closure body re-shapes on every change; an `apply` driven by a Custom-shape closure runs end-to-end | Phase 3 |
| `flutter analyze` clean for the touched files | Phase 3 |

## Concept

### What a "kind" is, today

A `ClosureKind` is a *shape template*: it fixes the arity, decides per pin
whether the type is **free** (user picks via `DataTypeInput`, filled from
`type_args`) or **fixed/derived** (the system supplies it: `Bool`, `Unit`, or
`= acc`), and fixes the parameter / result pin **names** statically. The four
preset kinds are exactly the four HOF body shapes, which is what makes a
`closure` of a given kind drop into the matching HOF's `f` pin by construction
(`doc/design_closures.md` §"Editor (Flutter) changes").

### What `Custom` adds

The minimum needed to author an arbitrary function signature inside the same
`{ kind, type_args }` model:

| Aspect | Preset kinds | `Custom` |
|---|---|---|
| Arity | Fixed by kind | Authored (≥ 1; see below) |
| Per-param type | From `type_args[i]` (filled in by 1–2 free slots) | From `type_args[0..N]` |
| Return type | Fixed (`Bool`, `Unit`), derived (`= acc`), or free (`Map`'s `U`) | From `type_args[N]` (always free) |
| Param names | Static on the kind (`"element"`, `"acc"`) | Authored, in a sibling `Vec<String>` |
| Result name | Static on the kind (`"result"`, `"new_acc"`, `"out"`) | Static `"result"` |

`Custom` has no fixed/derived pins: every type is free, including the return
type. This is why a `Custom` closure will not drop into a preset HOF's `f` pin
"by construction" — it can still drop in *if* the resolved `Function(...)` type
happens to match (`can_be_converted_to` is structural), but matching is the
exception, not the rule.

The arity is bounded below by 1 (a zero-arg "thunk" closure would be unusual
and is deferred — see §"Open questions / deferred").

### Why this is a kind, not a separate node

Two reasons:

1. **`closure` and `apply` share the same `{ kind, type_args }` data already**,
   differing only in inward vs. outward expansion. Custom-shape closures need
   exactly the same dual treatment (inward for the producer, outward for the
   consumer), so reusing the shared model is what's natural.
2. **The four presets remain the dominant case**. Authoring a Custom shape
   should be a one-click switch from a preset, with as much data preserved as
   possible — not a separate node-type lookup and re-wiring exercise.

## Data model

### `ClosureKind` (Rust)

```rust
pub enum ClosureKind {
    Map, Filter, Fold, Foreach,
    /// Arbitrary `(p0, p1, ..., pN-1) -> R`. Param types live at
    /// `ClosureData::type_args[0..N]`, the return type at `type_args[N]`.
    /// Arity N is derived from the parallel `ClosureData::param_names`
    /// length, **not** from `type_args` length (which can be transiently
    /// shorter or longer during editing — same convention as presets).
    Custom,
}
```

### `ClosureData` / `ApplyData`

A single new sibling field:

```rust
pub struct ClosureData {
    pub kind: ClosureKind,
    pub type_args: Vec<DataType>,
    /// Authored parameter names. **Empty for preset kinds** (which read
    /// names from `ClosureKind::param_names`'s static table) and length-N
    /// for `Custom` (where N is the arity). Same shape on `ApplyData`.
    #[serde(default)]
    pub param_names: Vec<String>,
}
```

`#[serde(default)]` is the **only** thing keeping older `.cnnd` files
loadable, so it is non-negotiable. (`closure` and `apply` only landed recently,
so the back-compat surface is small — but new fields on existing node-data
structs must always carry `default` per the existing additive-field convention
on serializable node-data structs in this codebase.)

The "free vs. fixed" interpretation of `type_args` is centralized in the
helper methods on `ClosureKind` so call sites (`calculate_custom_node_type` on
both nodes, plus the editor) do not need to know the encoding:

```rust
impl ClosureKind {
    /// How many `type_args` entries the kind expects. Preset arms are
    /// constant; `Custom` reads its arity from `param_names.len()`, so the
    /// answer depends on the caller's data — hence the `param_names` slice.
    pub fn num_type_args(&self, param_names: &[String]) -> usize {
        match self {
            Self::Map | Self::Fold => 2,
            Self::Filter | Self::Foreach => 1,
            Self::Custom => param_names.len() + 1,
        }
    }

    pub fn param_types(&self, type_args: &[DataType], param_names: &[String]) -> Vec<DataType> {
        match self {
            Self::Map | Self::Filter | Self::Foreach => vec![arg(type_args, 0)],
            Self::Fold => vec![arg(type_args, 0), arg(type_args, 1)],
            Self::Custom => (0..param_names.len()).map(|i| arg(type_args, i)).collect(),
        }
    }

    pub fn return_type(&self, type_args: &[DataType], param_names: &[String]) -> DataType {
        match self {
            Self::Map => arg(type_args, 1),
            Self::Filter => DataType::Bool,
            Self::Fold => arg(type_args, 0),
            Self::Foreach => DataType::Unit,
            Self::Custom => arg(type_args, param_names.len()),
        }
    }

    /// The names used as zone-input pin labels (closure) or arg-pin labels
    /// (apply). Local UI concern only — **never read by the function type**.
    /// Returns owned `Vec<String>` rather than `&[&str]` so the Custom arm
    /// can return its authored names; this is called once per node update,
    /// not in a hot loop, so the allocation is irrelevant.
    pub fn param_names(&self, param_names: &[String]) -> Vec<String> {
        match self {
            Self::Fold => vec!["acc".into(), "element".into()],
            Self::Map | Self::Filter | Self::Foreach => vec!["element".into()],
            Self::Custom => param_names.to_vec(),
        }
    }

    pub fn result_name(&self) -> &'static str {
        match self { Self::Fold => "new_acc", Self::Foreach => "out", _ => "result" }
    }

    pub fn function_type(&self, type_args: &[DataType], param_names: &[String]) -> FunctionType {
        FunctionType {
            parameter_types: self.param_types(type_args, param_names),
            output_type: Box::new(self.return_type(type_args, param_names)),
        }
    }
}
```

`arg(type_args, i)` continues to default to `DataType::None` for out-of-range
indices — the same transient-state fallback presets already use during editing.

Both `calculate_custom_node_type` impls (`closure.rs:143`, `apply.rs:61`)
already iterate the resolved `params: Vec<DataType>` and zip with names from
`self.kind.param_names()`. The change is mechanical: thread `&self.type_args`
and `&self.param_names` through the helper calls so the Custom arm has the
data it needs.

### `Default` for `ClosureData` / `ApplyData`

Unchanged. The default remains map-like `(Float) -> Float` with
`param_names: vec![]` — a fresh node lands on a preset, not on `Custom`.

### API mirrors

`APIClosureKind` gains `Custom`. `APIClosureData` and `APIApplyData` gain
`param_names: Vec<String>`. The converters in
`structure_designer_api.rs:317–344` are extended with the `Custom` arm and the
new field; no other API surface changes.

`NodeView.function_type` (the title-bar pretty-print) needs no change —
`FunctionType`'s `Display` impl already prints `(p0, p1, ...) -> R`.

## Param name validation

Authored parameter names are *identifiers in the closure body's scope*. The
existing identifier validation utility (`lib/structure_designer/identifier_validation.dart`)
is reused with the standard rule set: non-empty, starts with a letter or `_`,
remaining chars `[A-Za-z0-9_]`. The validator runs per-row inline (red border +
helper text) the same way `parameter_node`'s name field does today; an invalid
row is **not** persisted (the editor holds the last valid value and surfaces the
error locally).

Within one `Custom` shape, duplicate names are also rejected (each is an inline
error on the second-and-later row). Cross-scope clashes (a `Custom`-shape param
shadowing an outer-scope identifier) are *allowed* — name resolution in zone
bodies is lexical, and presets already permit such shadowing (an outer `element`
shadowed by an inner `map`'s `element`).

Defaults for new rows: `"arg0"`, `"arg1"`, … incrementing past the highest
existing `argN` to avoid an immediate duplicate error.

## Kind transitions

`_changeKind` in `ClosureShapeEditor` already preserves overlap between presets
by slot index. The Custom-kind extensions:

**Preset → Custom**: synthesize `param_names` from the preset's static table
and re-encode `type_args` as `[params..., return]`. Reading each preset row:
the param types come from `ClosureKind::param_types(preset_type_args)`, the
return type from `ClosureKind::return_type(preset_type_args)`, and they are
concatenated.

| From preset (preset `type_args`) | Param types | Return type | Synthesized `param_names` | Synthesized `type_args` |
|---|---|---|---|---|
| `Map` `[T, U]` | `[T]` | `U` | `["element"]` | `[T, U]` |
| `Filter` `[T]` | `[T]` | `Bool` | `["element"]` | `[T, Bool]` |
| `Fold` `[A, T]` | `[A, T]` | `A` (derived) | `["acc", "element"]` | `[A, T, A]` |
| `Foreach` `[T]` | `[T]` | `Unit` | `["element"]` | `[T, Unit]` |

(Note on Fold: the preset's return type is derived `= acc`, so it materializes
into the new `type_args` as a *copy* of `A` at the return slot. Subsequent
edits to either are independent — Custom has no derived pins.)

**Custom → Preset**: take `type_args[0..N]` (with N = preset's free-slot
count), drop `param_names`, drop any extra `type_args` entries. This is lossy
by design — same as today's slot-count change between presets. The dropdown
commits silently on selection (as today); undo handles the regret case. See
deferred §"Open questions" Q4 for a possible later confirmation dialog.

## Editor (Flutter)

All changes live in `lib/structure_designer/node_data/closure_editor.dart`. No
new shared widget; no changes to `DataTypeInput`.

### Kind dropdown

Add `APIClosureKind.custom` to the items list. Its glyph entry:

```
'(args…) → R   · custom'
```

(Three plain spaces before `·` match the existing preset glyphs' alignment,
e.g. `'(T) → U   · map-like'`.)

### Custom branch render

When `kind == APIClosureKind.custom`, the editor body replaces the
1–2-`DataTypeInput`-with-result-line layout with:

```
┌── Kind: (args…) → R · custom ─────────────────────────────┐
│  Parameters                                               │
│  ┌───────────────────────────────────────────────────┐   │
│  │ [name: arg0] [DataTypeInput: Float           ] 🗑 │   │
│  │ [name: arg1] [DataTypeInput: Vec3            ] 🗑 │   │
│  └───────────────────────────────────────────────────┘   │
│  + Add parameter                                          │
│                                                           │
│  Return Type                                              │
│  [DataTypeInput: Float                                ]   │
└───────────────────────────────────────────────────────────┘
```

Concretely:

- A `Column` holding one `_CustomParamRow` per param, each consisting of:
  - A short `TextField` (~120 logical px wide) bound to `param_names[i]`,
    with the identifier validator.
  - The existing `DataTypeInput` bound to `type_args[i]`.
  - An `IconButton(Icons.delete_outline)` removing the row. Disabled when
    `param_names.length == 1` (we don't allow zero-arg closures in v1; see
    open questions).
- A `TextButton.icon(Icons.add, 'Add parameter')` appending a row.
- A divider.
- A label "Return Type" above a `DataTypeInput` bound to
  `type_args[param_names.length]`.

Every edit (add / remove / rename / retype) reconstructs the full
`(kind, typeArgs, paramNames)` tuple and calls
`onChanged(kind, typeArgs, paramNames)` — adding a third argument to the
callback. Both call sites (`closure` and `apply` in `node_data_widget.dart`)
forward the new arg into `model.setClosureData` / `setApplyData`. The
name-`TextField` commit cadence follows whatever the existing
`parameter_node` name field uses (don't invent a third pattern).

### What's hidden

The preset's read-only result-label `Padding` (currently shown for `Filter` /
`Fold` / `Foreach`) is omitted in the Custom branch since the return type is
itself a `DataTypeInput`.

## Repair, validation, evaluation

Each of these "just works" because the changes are purely in how the shape is
described, not in how it is used:

- **Structural pin change.** Switching kinds, adding a row, removing a row,
  or retyping a row triggers `calculate_custom_node_type` → the existing
  `set_node_network_data_scoped` path → `refresh_structure_designer_auto` →
  the existing `repair_node_network` / `repair_zone_body` (closure) /
  arguments-only wire-retention (apply) routines. Wires whose source/dest
  types no longer match get disconnected; the rest stay.
- **Validator.** `validate_zones_recursive` and the closure-validator rules
  (`function_input_pin_connected`, "zone-output wire required") run against
  the resolved param/return types and the resolved pin set. A `Custom`-shape
  body with N params just has N zone-input pins to wire into, same checks.
- **Evaluator.** `obtain_closure` / `build_inline_closure` / `run_closure_once`
  / the `Walker::MapZone`/`FilterZone` lazy step are all generic over the
  `ZoneClosure`'s param/return types. No code changes.
- **Undo.** `set_node_network_data_scoped` already snapshots the full
  `NodeData`; the new `param_names` field rides along automatically.

## Serialization (`.cnnd`)

`#[serde(default)]` on the new field is the only back-compat lever needed. A
forward-compat note: an old build loading a new file with `Custom`-kind data
will fail at `ClosureKind` deserialization (no `Custom` variant) — same break
shape as every other newly added node-type variant. This is acceptable because
`.cnnd` is already a same-build artifact format (no documented
cross-build-version contract).

## Text format

The text format mirrors the data model. The Custom kind needs to round-trip
three things in addition to `kind`: the ordered list of (name, type) pairs
for the params, and the return type. One illustrative shape:

```
my_closure = closure {
  kind: custom,
  params: [(input: Float), (mask: Bool)],
  return: Vec3,
  // body wires authored separately, as today
}
```

The exact bracketing/punctuation (`[(name: Type), ...]` vs. an anonymous-
record-style `{ input: Float, mask: Bool }` vs. some third option) is left to
the implementer to align with the existing `text_format/` parser
conventions — the requirement is just that it round-trips param order and
both names and types.

For preset kinds, the existing terse syntax (`kind: map`, `T: Float`,
`U: Vec3`) is unchanged.

Backward compatibility: a stored shape with `kind` ∈ `{map, filter, fold,
foreach}` and no params/return keys parses to a preset as today; a
`kind: custom` with missing params or return is a parse error.

## Drop-in semantics — explicit non-goal

A `Custom`-kind closure produces a `Function(...)` value with a user-authored
signature; it does NOT match any preset HOF's `f` pin "by construction." It
will still drop in *if* the signature happens to be structurally compatible
with the HOF's required type (which is exactly the existing
`can_be_converted_to` check) — but matching is the exception. The natural
consumers of a `Custom`-shape closure are:

1. **`apply`**. An `apply` node's stored kind can also be `Custom`, with the
   same arity/types; the closure's `Function` value flows into `apply.f` and
   the per-param arg pins are populated to match.
2. **Subnetwork function-factory outputs.** A custom network whose return
   node has type `Function(...)` is unconstrained — `Custom` is what makes
   such factories author-able from the UI.

This is fine, and worth calling out in the node description for both `closure`
and `apply`: *"Use one of the four preset kinds when the function will drive a
matching HOF; use Custom for `apply` and function-factory subnetworks."*

## Phases

Three short phases. Each ends green on the contract row above it.

1. **Rust + tests.** Add `ClosureKind::Custom`, the `param_names: Vec<String>`
   field on `ClosureData`/`ApplyData` (with `#[serde(default)]`), update the
   six `ClosureKind` helper methods and both `calculate_custom_node_type` impls
   to take/thread `param_names`, and extend
   `api_closure_kind_to_closure_kind` / `closure_kind_to_api_closure_kind` /
   the API structs. New tests in `closures_test.rs`:
   `custom_kind_calculate_node_type` (4–5 arities × closure/apply),
   `custom_kind_repair_on_param_remove`,
   `custom_kind_cnnd_roundtrip`,
   `preset_to_custom_data_preservation`,
   `cnnd_back_compat_loads_old_closure_data` (an older fixture without
   `param_names`).
2. **FRB regen.** `flutter_rust_bridge_codegen generate`; smoke-build Flutter.
   No editor changes yet — the Custom item simply does not appear in the
   dropdown because the editor only iterates the four preset values today.
3. **Flutter editor.** Extend the kind dropdown to iterate the full enum,
   add the Custom branch (`_CustomParamRow`, the add/remove buttons, the
   return-type `DataTypeInput`), wire the new tuple through
   `setClosureData` / `setApplyData`, and add a manual walkthrough to
   `lib/structure_designer/node_data/AGENTS.md`'s smoke list:
   *(a)* drop a `closure`, switch to Custom, add two params, observe zone
   pins resize; *(b)* drop an `apply`, switch to Custom, attach the closure
   via `f`, run the network; *(c)* switch back to Map, observe param drop
   + undo restores it.

## Reuse map

**Reused unchanged:**
- The shared closure/apply data model (`{ kind, type_args }` plus the new
  sibling field).
- `obtain_closure`, `build_inline_closure`, `run_closure_once`, both walker
  variants, every per-node `eval`.
- `ZoneClosure`, `NetworkResult::Function`, every validator and repair pass.
- `DataTypeInput`, every other input widget.
- The `closure` / `apply` Add Node popup entries (no new node).

**Reused with small changes:**
- `ClosureKind`'s six helper methods (new Custom arms; signatures take
  `param_names`).
- `ClosureData` / `ApplyData` (one new field).
- `APIClosureKind` / `APIClosureData` / `APIApplyData` and their converters.
- `closure::calculate_custom_node_type` / `apply::calculate_custom_node_type`
  (thread `param_names`).
- `ClosureShapeEditor` (new Custom branch).
- The text-format parser/serializer for `closure` / `apply` (new
  `params:`/`return:` keys).

**New from scratch:**
- One private widget in `closure_editor.dart` (`_CustomParamRow`).
- A handful of new tests in `closures_test.rs`.

**Deleted:** nothing.

## Open questions / deferred

1. **Zero-arg closures.** A `Custom` shape with `arity == 0` is a "thunk":
   a `Function(() -> R)` value. The substrate supports it (`param_types` is a
   `Vec`, the body has zero zone-input pins) — the question is whether the
   UX justifies a `+ Add parameter`-only state (no rows visible, just the
   return-type picker). Deferred: not blocking, easy to enable later by
   relaxing the "delete disabled at length == 1" rule and seeding a fresh
   `Custom` with at least one row.
2. **`Function(...)` branch on `DataTypeInput`.** Today, a Custom-shape
   parameter that *is itself* a function is reachable only through
   `DataTypeInput`'s `Custom...` text escape hatch. A recursive Function
   branch (parameter list + return-type picker, structurally identical to the
   Custom-kind editor itself) is the natural follow-up. It is independently
   useful (it would also benefit `expr` parameter pickers and record-field
   type pickers) and warrants its own short doc.
3. **A "Result" name field for Custom.** Today, the result pin name is fixed
   to `"result"` for non-Fold/Foreach presets, and `Custom` follows the same
   default. Letting the user author a result name is mechanically free
   (small editor field; no Rust change beyond reading from a third sibling
   field). Skipped in v1 because the result pin's name appears only in the
   zone-output pin label, which is rarely worth customizing. Trivially
   add-able later if asked for.
4. **Kind-switch confirmation dialog.** The current `_changeKind` commits
   silently. The lossy Custom → preset switch could surface a
   `showDraggableAlertDialog` confirm ("Switching to Map will drop N−1
   parameters and the explicit return type. Continue?"). Skipped in v1 — undo
   handles the regret case, and silent commit matches the existing UX
   pattern; revisit if user feedback says otherwise.
