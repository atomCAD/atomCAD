# Design: `array` literal node + record-field editor hints

## Motivation

Two gaps, discovered together while designing style rules
(`doc/design_style_rules.md`), that share one root cause — *generic* record
editing is currently both verbose and visually poor:

1. **There is no array literal.** Building an `Array[Record]` value (an
   `atom_replace` rule set, a `StyleRule` list) costs one `record_construct`
   node **per element** plus a `sequence` collector — N+1 nodes, with the
   schema re-picked on every `record_construct`. The `expr` node closes the
   gap only for primitives (`[1, 2, 3]`): record width subtyping requires
   every destination field present in the source
   (`data_type.rs::can_be_converted_to_impl`, record arm), and expr record
   literals cannot express an unset `Optional` field — so expr can never
   feed a pin typed against an all-`Optional`-fields def like `StyleRule`.
2. **Generic editors render an atomic number as a bare int box.** A
   dedicated per-schema node could show an element dropdown (atom_replace's
   inline rules editor does exactly that, hard-coded to `ElementMapping`),
   but the whole point of record-typed rules is that *generic* machinery
   (`record_construct`, and the `array` node below) authors them — and
   generic machinery only knows the field is `Int`.

This document adds the two complementary pieces:

- **Part A — field editor hints**: an optional, purely presentational
  annotation on record-def fields (`Element`, `Color`, `Enum`, `Range`)
  that lets any generic editor pick the right widget.
- **Part B — the `array` node**: a one-node array literal — pick an element
  type, then add/remove/reorder/edit elements inline, with record elements
  edited through the same hint-aware field rows `record_construct` uses.

Part A is independently useful (it upgrades `record_construct` today, for
`ElementMapping` immediately and `StyleRule` when it lands) and Part B
consumes it, so A lands first. Neither part changes the type system, the
evaluator's value model, or any existing node's behavior.

## Part A — Field editor hints

### The mechanism

```rust
// node_type_registry.rs, next to RecordField
/// A purely presentational annotation on a record-def field: which widget a
/// generic literal editor should render. NEVER consulted by subtyping,
/// conversion, validation, or eval — the field's DataType alone governs
/// wires and values. See the invariant below.
pub enum FieldEditorHint {
    /// Int fields: atomic-number element dropdown (`SelectElementWidget`).
    Element,
    /// Vec3 fields: 0–1 RGB color editor.
    Color,
    /// String fields: fixed-choice dropdown. Entries are trimmed, non-empty,
    /// duplicate-free; the list is non-empty.
    Enum(Vec<String>),
    /// Float or Int fields: slider between min and max (min < max).
    Range { min: f64, max: f64 },
}

// RecordField gains:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub hint: Option<FieldEditorHint>,
```

**The invariant, stated once and firmly: hints are cosmetic.** A hint never
gates a wire connection, never converts a value, never validates at eval,
never appears in `can_be_converted_to` / `can_be_structurally_converted_to`,
and never changes what `record_construct` emits. The wire value is a plain
`Int`/`Vec3`/`String`/`Float`; a wired-in value outside an `Enum` list or
`Range` bounds flows through exactly as today and is judged only by the
consuming node's own eval-time validation (e.g. `apply_style` validating
`render_style` strings). The moment a hint changes behavior it becomes a
shadow type system — that is the line this design draws. (External
precedent: JSON Schema's `format` annotation and uiSchema layers — primitive
type + presentation annotation, kept strictly apart.)

### Applicability

A hint is **well-formed** only on a matching field type, checked *through*
an `Optional[…]` wrapper (the hint describes the inner value):

| Hint | Valid on |
|---|---|
| `Element` | `Int` / `Optional[Int]` |
| `Color` | `Vec3` / `Optional[Vec3]` |
| `Enum(…)` | `String` / `Optional[String]` |
| `Range{…}` | `Float`, `Int`, or their `Optional`s |

Ill-formed hints are rejected at every def construction site, exactly like
the `Optional[Optional[…]]` restrictions: `add_record_type_def` /
`update_record_type_def` (and the `with_edits` variant) error with a clear
message; registry validation guards `.cnnd` files that smuggle in a
mismatched hint (the hint is **dropped with a load warning**, not a load
failure — cosmetic data must never brick a project file); built-in defs are
constructed in code through the same checked constructor. `Enum` lists must
be non-empty with trimmed, non-empty, duplicate-free entries; `Range`
requires `min < max`.

### Persistence

User defs serialize the hint with the field (serde default `None`,
`skip_serializing_if` keeps hint-free saves byte-identical to today's
output). **No `.cnnd` version bump, no migration** — old files load with no
hints; new hint-free files are unchanged. Built-in defs are never
serialized, as before.

### FRB / Flutter surface

- New FRB mirror `APIFieldEditorHint` (same four variants).
- `APILiteralField` (`structure_designer_api_types.rs`) gains
  `hint: Option<APIFieldEditorHint>`, populated by
  `get_record_construct_fields` from the resolved def. The *other* consumer
  of `APILiteralField` — `CustomNodeEditor`'s parameter rows — has no record
  def behind it, so its hint is always `None` and nothing changes there.
- `APIRecordSchemaData`'s field rows gain the hint (so the schema editor can
  display and edit it — Phase 2).
- `lib/structure_designer/node_data/literal_fields_editor.dart`: the widget
  dispatch (currently a `switch (field.dataType)` at ~line 160) gains a
  pre-check — if `field.hint` is present, render the hint widget; otherwise
  fall through to the existing type switch. Unknown/future hint values fall
  through too (graceful degradation is free because the type switch remains
  complete).

Hint widgets:

- `Element` → the existing `lib/common/select_element_widget.dart`
  (`SelectElementWidget`), already used by atom_replace's inline rules
  editor. Reuse, zero new UI.
- `Enum` → a plain `DropdownButton` over the entries. A stored value that is
  *not* in the list (wire-era leftovers, def edits) is shown as an extra,
  visually-flagged entry rather than silently discarded — picking any real
  entry replaces it.
- `Range` → slider + numeric field pair (the `xray_editor` composition),
  clamped to the hint bounds *in the UI only*.
- `Color` → **no color widget exists in `lib/` today**; add a minimal
  `lib/common/color_field_widget.dart` (swatch preview + three 0–1 float
  fields, composed from existing input widgets). A richer picker is a
  drop-in upgrade later — the hint seam doesn't change.

### Annotations shipped with this design

- **`ElementMapping.from` / `.to` → `Element`** — the immediate payoff:
  `record_construct(schema: ElementMapping)` literals get element dropdowns,
  matching the convenience of atom_replace's bespoke inline editor.
- **`StyleRule`** (when `doc/design_style_rules.md` lands) declares four:
  `element → Element`, `color → Color`, `alpha → Range{0,1}`,
  `render_style → Enum(["ball_and_stick", "space_filling", "default"])` —
  the Enum hint turns that free-typed string into a dropdown, making the
  eval-time string validation a backstop instead of the first line of
  defense.
- `MaterializeRegion` needs none (its `Optional[Bool]`/`Optional[Float]`
  fields already render sensibly).

### Limitation (stated, not solved)

Hints cover **statically knowable** affordances only. A `tag: String` field
ideally offers the *upstream structure's* tag names — runtime context that a
`record_construct` sitting anywhere in the graph does not have (it is not
connected to the styled structure). No static annotation can express that;
such fields stay free text. Context-dependent suggestion plumbing is out of
scope here and belongs to the consuming node's eval-cache pattern if ever
pursued.

## Part B — the `array` node

### Node specification

| | |
|---|---|
| **Name** | `array` |
| **Category** | same as `sequence` (`OtherBuiltin`) |
| **Input pins** | **none** (literal-only — see Decision 1) |
| **Output** | `Array[element_type]` via `calculate_custom_node_type` |
| **Data** | `ArrayData { element_type: DataType, elements: Vec<TextValue> }` |
| **Defaults** | `element_type: Int`, `elements: []` |
| **Subtitle** | `3 × Int`-style count + element type |

### Decision 1 — literal-only elements (no per-element pins)

Elements are stored literals, period. A computed element means using
**`sequence`** (which stays exactly as it is — the two nodes are
complements: `array` = literal data, `sequence` = wired data; mixing is
`array_concat` / `array_append` territory).

Rationale: per-element input pins would make every add/remove/reorder a
**structural** edit — pin-count changes, wire stability across removal,
stable per-element ids, whole-network-snapshot undo — the full
`switch`/`zip_with` lane machinery. Literal-only, the node has *no input
pins at all*: every element edit is a pure node-data mutation with plain
`SetNodeDataCommand` undo, and the entire hazard class vanishes. Hybrid
wired-overrides-literal elements are the documented v2 if demand appears;
nothing in v1's storage forecloses it.

(One edit is still structural in effect: changing `element_type` retypes
the **output** pin and can drop outgoing wires — see Decision 3.)

### Decision 2 — literal-capable element types

`element_type` must be **literal-capable**, defined by a new predicate
(`is_literal_capable(dt, registry)`) as:

- the simple types the literal panel already edits — the
  `APISimpleParamType` set: `Bool`, `Int`, `Float`, `String`, `IVec2`,
  `IVec3`, `Vec2`, `Vec3`, `IMat3`, `Mat3`; or
- `Record(Named(def))` whose every field, looked at through its
  `Optional[…]` wrapper, is one of those simple types.

Excluded, deliberately: structural types (`HasAtoms`, `Blueprint`,
`Structure`, …) and `Function`/`Iter`/`Unit` — no literal form exists (this
is why `MaterializeRegion`, whose `volume` is a `Blueprint`, is *not*
authorable here; `ElementMapping` and `StyleRule` are); `IMat2` — absent
from `APISimpleParamType`/`APILiteralValue` today, add both together if
wanted; **nested arrays** and **record-typed record fields** — each would
recurse the element editor into a list-in-list / group-in-group UI, real
scope creep for no current consumer; `Record(Anonymous)` — pin-type
convention reserves anonymous records for expr-inferred types.

The predicate is enforced in `set_text_properties`, the node-data API
setter, and the Flutter type picker filter (the `DataTypeInput` used by the
`sequence`/`zip_with` editors, filtered).

### Storage & eval

Element storage mirrors `record_construct.literal_values`, one level up:

- simple `element_type` → each element is the matching `TextValue`;
- record `element_type` → each element is a
  `TextValue::Object(Vec<(String, TextValue)>)` holding entries **only for
  set fields** — an absent entry is "unset", exactly the semantics of a
  missing `literal_values` key on `record_construct`.

`eval` (no inputs to evaluate) converts each element to a `NetworkResult`:

- simple → the same literal-coercion path `record_construct::eval` applies
  to unwired fields (so a whole-number literal coerces into a `Float`
  element, etc.);
- record → `NetworkResult::record(...)` with **all** fields in canonical
  order: a set field coerces to the field type; an unset `Optional` field
  becomes an explicit `NetworkResult::None` (the emit-all-fields invariant
  from `doc/design_optional_type.md`); an unset **required** field, or any
  uncoercible literal, is a **localized `NetworkResult::Error` naming the
  element index and field** (`array[2].from is unset`) — nothing is
  partially emitted.

An empty `elements` list evaluates to a valid empty `Array` (matching the
"empty rules array passes through" conventions downstream).

**Stale literals are preserved, never silently dropped.** Changing
`element_type` (or a record def gaining/retyping fields) leaves stored
literals verbatim; mismatches surface as the localized eval errors above,
and the editor flags the offending rows and offers clearing. This is the
same no-silent-data-loss stance as switch-case healing.

### Decision 3 — undo classes

- **Element content edits** (set/clear a literal, set/clear a record
  element's field, add / remove / reorder elements): pure node-data →
  standard `SetNodeDataCommand` via `set_node_network_data_scoped`
  ("persisted mutations must be undoable" — satisfied by the standard
  path).
- **`element_type` change**: retypes the output pin, so downstream wires can
  be dropped by the repair pass. Undo must restore those wires, so this one
  setter pushes the whole-network-snapshot **`NodeStructureEditCommand`** —
  the same command `switch` case edits and `zip_with` lane edits use for
  exactly this reason.

### Text format

`get_text_properties` / `set_text_properties`:

- `element_type` — `TextValue::DataType` (as `sequence`);
- `elements` — `TextValue::Array` of the element `TextValue`s
  (`TextValue::Object` per record element), riding the existing serializer.

This makes the node fully text-format authorable — a rule set is one
statement — which matters for the AI-integration path (the atomcad skill
edits networks through the text format). `set_text_properties` rejects a
non-literal-capable `element_type` with a clear error and validates that
`elements` is an array, but keeps individual literals raw (eval reports
per-element problems, per the stale-literal rule).

### Node-data API

Mirrors `record_construct`'s granular literal API
(`get_record_construct_fields` / `set_record_construct_literal` /
`clear_record_construct_literal`) rather than shipping whole blobs — all
thin, `#[frb(sync)]`, **scope_path-taking** (hard rule), all triggering
refresh + undo per Decision 3:

```text
get_array_node_data(scope_path, node_id) -> APIArrayNodeData
  // element_type + per-element rows: simple -> Option<APILiteralValue>;
  // record  -> Vec<APILiteralField>  (name, type, stored value, hint —
  //            the SAME row type record_construct's editor consumes,
  //            so the hint plumbing from Part A applies for free)
set_array_element_type(scope_path, node_id, ...)        // NodeStructureEditCommand
add_array_element(scope_path, node_id, index)
remove_array_element(scope_path, node_id, index)
move_array_element(scope_path, node_id, from, to)
set_array_element_literal(scope_path, node_id, index, APILiteralValue)
clear_array_element_literal(scope_path, node_id, index)
set_array_element_field_literal(scope_path, node_id, index, field, APILiteralValue)
clear_array_element_field_literal(scope_path, node_id, index, field)
```

Existing converters `text_value_to_api_literal` / `api_literal_to_text_value`
(`structure_designer_api.rs`) handle every value crossing; nothing new at
the value layer. Run `flutter_rust_bridge_codegen generate`.

### Flutter editor

`lib/structure_designer/node_data/array_editor.dart`:

- header: element-type `DataTypeInput` filtered to literal-capable types;
- element list: one row (simple types — the hint-aware literal widget) or
  one expandable group of field rows (record types — visually the
  `record_construct` literal section, per element) each with remove and
  move-up/down affordances; an add button appends;
- rows whose stored literal no longer coerces show a warning affordance
  with a clear action (per the stale-literal rule);
- `Optional` record fields get the same tri-state *set / unset* affordance
  as `record_construct`.

### Drag-aware add

`adapt_for_drag_source`: `FromInput` (dragging backwards from a consumer pin
expecting `Array[T]`) → strict-peel the element type and accept iff
literal-capable, else `None`; `FromOutput` → `None` (the node has no input
pins). This makes `array` surface in the drag-add popup for exactly the pins
it can feed — including `atom_replace.rules` and the future
`apply_style.rules`.

### What does not change

`sequence`, `array_append`, `array_concat`, expr array literals, the
single-value → Array broadcast conversion, and `record_construct` itself all
stay as they are. The `array` node is additive; no migration, no version
bump (new node type, unconnected everywhere by definition).

## Phases

Each phase lands green on `cargo fmt && cargo clippy && cargo test -j 4`
(and `flutter analyze` where Dart is touched) with the automated tests
listed. Part A = Phases 1–2, Part B = Phases 3–4; Phase 2 can slip behind 3
if built-in-only hints are acceptable for a while.

---

### Phase 1 — hints core + `ElementMapping` annotation

**Implementation**

- `FieldEditorHint` + `RecordField.hint` (serde default) + the checked
  constructor / applicability validation at every def mutation site +
  load-time drop-with-warning for ill-formed hints.
- Annotate `ElementMapping.from`/`.to` with `Element`.
- FRB: `APIFieldEditorHint`; `APILiteralField.hint` populated in
  `get_record_construct_fields`; codegen.
- Flutter: hint pre-check in `literal_fields_editor.dart`; widgets —
  `SelectElementWidget` reuse (Element), dropdown (Enum), slider+field
  (Range), new `lib/common/color_field_widget.dart` (Color).

**Automated tests** (`rust/tests/structure_designer/`, extending the
record-def test files)

- Each valid (hint, type) combination accepted, including through
  `Optional[…]`; each mismatch rejected by `add_record_type_def` /
  `update_record_type_def` with a clear error; `Enum` list rules and
  `Range{min<max}` enforced.
- Serde: a user def with hints round-trips through `.cnnd`; a hint-free
  save is byte-identical to the pre-feature format; an old file (no `hint`
  keys) loads; a hand-corrupted file with an ill-formed hint loads with the
  hint dropped.
- `lookup_record_type_def("ElementMapping")` exposes the `Element` hints;
  `get_record_construct_fields` carries them into `APILiteralField`.
- `flutter analyze` clean over baseline.

**Manual verification** — `flutter run`: a `record_construct` with schema
`ElementMapping` renders element dropdowns for the `from`/`to` literals;
wiring a pin disables the row exactly as today; a schema without hints is
pixel-identical to before.

---

### Phase 2 — schema-editor hint UI (user defs)

**Implementation**

- `APIRecordSchemaData` field rows carry the hint; the user-types panel's
  `SchemaEditor` gains a per-field hint control — a dropdown filtered to
  the hints valid for the field's type (per §Applicability), with an
  entry-list sub-editor for `Enum` and min/max fields for `Range`.
- Setter path revalidates applicability Rust-side (the UI filter is
  convenience, not the gate) and returns the error into the panel.

**Automated tests** — API-level: update a user def adding each hint kind →
round-trips through get; an invalid combination returns a clear error and
leaves the def unchanged; def edits that retype a hinted field re-run
applicability (retyping `Int`→`String` under an `Element` hint is rejected
until the hint is cleared or changed).

**Manual verification** — `flutter run`: declare a user def with an `Enum`
hint; its `record_construct` literal shows the dropdown; renaming/editing
the def keeps hints attached to their fields (stable `FieldId`s).

---

### Phase 3 — `array` node backend

**Implementation**

- `nodes/array.rs`: `ArrayData`, `is_literal_capable`,
  `calculate_custom_node_type` (output `Array[element_type]`), `eval` per
  §Storage & eval, subtitle, text properties, `adapt_for_drag_source`.
  Register in `nodes/mod.rs` + `node_type_registry.rs`.

**Automated tests** — new `rust/tests/structure_designer/array_node_test.rs`:

- Eval per simple type (each `APISimpleParamType` member); whole-number
  literal coerces into a `Float` element; empty list → empty `Array`.
- Record elements: set fields land typed and canonical-ordered; unset
  `Optional` field → explicit `None` in the emitted record; unset required
  field → localized error naming index and field; uncoercible literal →
  localized error; downstream consumption (wire an
  `Array[Record(ElementMapping)]` from the node into `atom_replace.rules`
  and assert the replacement applies — the end-to-end this exists for).
- `element_type` guard: structural / nested-array / anonymous-record types
  rejected by `set_text_properties`.
- Stale literals: retype `Int → Vec3` with elements present → data
  preserved, eval errors localize per element.
- Text-format round-trip: primitives, record objects, exotic strings
  (quotes, non-ASCII), unset-vs-set `Optional` fields distinguished.
- Node-type snapshots (`cargo test node_snapshots` + `cargo insta review`).

**Manual verification** — `flutter run`: node addable, output wires into
`Array`-typed pins, subtitle tracks count/type, text-format panel
round-trips a hand-written element list. No property editor yet (Phase 4)
— expected.

---

### Phase 4 — `array` node API, Flutter editor, reference guide

**Implementation**

- The §Node-data API surface (granular setters; `set_array_element_type` on
  `NodeStructureEditCommand`, everything else on `SetNodeDataCommand`);
  codegen.
- `array_editor.dart` per §Flutter editor, registered in the
  property-panel dispatch.
- Reference guide: new `doc/reference_guide/nodes/array.md` (element-type
  domain and why structural types are excluded, literal-vs-`sequence`
  guidance, stale-literal behavior, record-element editing) + node index
  link; the record-types / user-types guide page gains a short "editor
  hints" section (what each hint renders, that hints never affect wires or
  values).

**Automated tests**

- API-level: each setter round-trips through the `StructureDesigner`-level
  path; undo/redo restores element content edits (SetNodeDataCommand) and
  an `element_type` change **including a dropped outgoing wire**
  (NodeStructureEditCommand — the test that fails if Decision 3 regresses
  to plain data undo).
- `flutter analyze` clean over baseline.

**Manual verification** — `flutter run`, the headline walkthrough: create
`array` of `ElementMapping`, add three rules through element dropdowns
(Part A visibly composing with Part B), reorder them, wire into
`atom_replace.rules`, watch the viewport; undo/redo through content edits
and a type change; edit the node **inside a zone body** (scope chain);
drag from `atom_replace.rules` into empty space and confirm `array`
surfaces pre-configured in the add popup.

---

## Explicitly out of scope

- Hybrid per-element input pins (wired-overrides-literal) — documented v2;
  requires the stable-id lane machinery.
- Nested arrays and record-typed record fields as elements; `IMat2`
  literals; anonymous-record elements.
- Context-dependent field suggestions (upstream tag names etc.) — hints are
  static by design.
- Hints influencing anything but widget choice — permanently, by the Part A
  invariant, not deferred.
- Inline literal editing on `Array`-typed *pins* of other nodes (an
  editor-side idea occasionally floated; this design keeps literals in a
  dedicated node).
- Retro-fitting atom_replace's bespoke inline rules editor onto the generic
  machinery — worth doing eventually (delete code, gain hints), but a
  separate cleanup.
