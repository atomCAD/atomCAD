# Wire-declared inputs: the open port on `expr` and anonymous `record_construct`

## Status: Draft (2026-10-21)

## Motivation

Some nodes **declare their own input list**: `expr` has a parameter list, and
`record_construct`'s inputs are the fields of the record it builds. Today,
adding an input to one of them is a two-step chore:

1. Open the property panel and declare the input: a name, a type (picked from
   the type selector) and its position.
2. Go back to the canvas and draw the wire you meant to draw in the first
   place.

The wire already carries most of the declaration:

| A declaration needs | The wire supplies it |
|---|---|
| a type | **exactly** — the source pin's resolved type |
| a position | **exactly** — appended at the end, in drop order |
| a name | **approximately** — the source node's name, or its pin name |

So the first step is redundant. This doc makes **connecting declare the
input**. A node that declares its own inputs gets an **open port**: a `+`
row under its last input pin. Dropping a wire on it appends an input of the
wire's type, under a derived name, connects the wire, and puts the cursor in
the new input's name field so a rename is one keystroke away.

The record half also makes a second change: **structure first, name later**.
Today a record must be designed before it can be built: create a def in the
user-types panel, add fields, pick types, then place a `record_construct`.
Here a `record_construct` with no schema is an **anonymous record** whose
fields are declared by wires. Once the bundle proves worth keeping, **Name
this record…** promotes it to a named def in one step, keeping every wire.
This fits the type system as it already is. `doc/design_record_types.md`
calls names "a UX and tooling concession layered on top of a
structurally-typed core". The anonymous record is the honest type of "these
five things", and the name is added when sharing, cascading renames or
`record_get` / `record_set` make it worth having.

### Scope

In scope:

- the open port on `expr`;
- an anonymous mode for `record_construct`, with an open port;
- promoting an anonymous `record_construct` to a named def, or pointing it at
  an existing def with the same shape;
- the parser fix that lets anonymous record types round-trip through type
  strings (a prerequisite, Phase 0).

**Out of scope — explicitly:**

- **Custom node networks.** No open port on custom-node instances, and no
  parameter is created by dropping a wire anywhere. An instance is a call
  site: adding an input from it edits the shared definition and gives every
  other instance an unwired pin. Inside a definition, the existing **Promote
  to Parameter** command (`undo/commands/promote_to_parameter.rs`) already
  covers the inside-out case. This doc changes neither.
- **Named-mode `record_construct`.** Its fields belong to the def, which is
  shared, so it has no open port. Adding a field to a def stays a def edit.
- `record_get` / `record_set` / `record_destructure` on anonymous records.
  They keep requiring a named `schema`; promotion is the way there.
- Field access on named records inside `expr` (`r.x` on a `Record(Foo)` still
  errors, `expr/expr.rs`, `Expr::MemberAccess`).

## Resolved decisions

| Question | Decision |
|---|---|
| Which nodes get an open port | `expr` (always) and `record_construct` **in anonymous mode** (its `schema` is empty). Nothing else in this doc. |
| How the open port looks | A **permanent** `+` row below the last input pin, drawn subdued, and highlighted while a compatible wire hovers it. It is not a ghost that appears only during a drag: a row that appears mid-drag would change the node's footprint, move every pin offset below it, and trigger reflow, all while the user is aiming. A permanent row keeps the layout stable and makes the feature visible. |
| Gesture direction | **Output → `+` only** in v1: drag from a source pin (a node output, a function pin, or a zone input) and drop on the `+`. The `+` itself is not draggable. Dragging from `+` to an output is a later addition (see "Future work"). |
| The declared type | The source pin's **resolved type at drop time**, snapshotted (`NodeTypeRegistry::resolve_output_type_scoped` for node outputs, the HOF's zone-input pin type for zone inputs). The type does not follow the wire afterwards. If upstream changes, the wire shows a type error like any other wire. A live type would make the interface depend on wiring, and undo and the text format would have to track that dependency. |
| Which types an open port accepts | `expr`: the predicate its drag-aware adapter already applies: no abstract types, `Function`, `Iterator`, `Unit` or `None` (`ExprData::adapt_for_drag_source`), factored into a shared `ExprData::accepts_parameter_type`. `record_construct`: any type a record def field may hold (see Phase 2), and never `None`, `Unit` or `AnyFunction`. While a wire with a rejected type hovers the `+`, the row is drawn in a refused style with the reason as a one-line label beside it. A tooltip would not work here, because Flutter shows no tooltips during a drag. |
| The derived name | The source **node's name** (`custom_name`, e.g. `gap` or `sphere1`). When the source node has more than one output pin, or the source is a zone input, the **pin name** is used instead; if that is already taken, `<node>_<pin>`. The name is sanitized to `[A-Za-z_][A-Za-z0-9_]*`, made to avoid reserved words, and deduplicated as `name`, `name_2`, `name_3`, … (details under "Name derivation"). |
| After the drop | The destination node becomes the selected node, and its property panel focuses the new input's name field with the text selected. Typing renames it; anything else keeps the derived name. Stable ids mean a rename never drops the wire. |
| Undo | **One step** per drop ("Add input from wire"): the input declaration, the wire, and any reflow of neighbours the taller node causes. It is recorded as a whole-network snapshot: a new generic `NetworkSnapshotCommand { network_name, description, before, after }`, the pattern `PromoteToParameterCommand` already uses, so redo never depends on the order of re-deriving a pin and re-adding a wire. |
| Pin position | Always **appended**. No existing pin index shifts, so wires, function-pin roles (keyed by input index) and the text format's positional state are untouched. |
| Anonymous `record_construct` storage | `RecordConstructData` gains a local `fields` list (`id`, `name`, `data_type`) and a `next_field_id`. Mode is decided by `schema`: non-empty → named mode, where `fields` must be empty; empty → anonymous mode. |
| Anonymous pin ids | `ANON_FIELD_PIN_ID_BASE \| field.id`, with the base `1 << 62`. `FieldId`s are counters allocated from a per-file floor, so in practice they never reach that range. As a result `Node::set_custom_node_type`'s id-first matching can never pair an anonymous field with an unrelated def field on a mode switch. It falls through to **name** matching, which is the intended correspondence (promotion keeps names; picking a schema matches by name). |
| Anonymous field types | Never `Optional[T]`: every anonymous field is required. The type comes from a pin, and `Optional` is never a pin type. A promoted def can make a field optional later in the schema editor. |
| Output type in anonymous mode | `Record(Anonymous(fields))`, canonically sorted as `RecordType::anonymous` requires. Pin order is authored order. With no fields it is `{}`, and eval produces the empty record. This changes how existing files behave: a saved `record_construct` with an empty `schema` used to produce a dangling `Named("")` output and `None`. It now loads as an anonymous node with no fields, which outputs `{}`. |
| Literals in anonymous mode | Same storage as named mode: `literal_values`, keyed by field name. Renaming an anonymous field moves its literal key. This keeps the literal-editor core of `doc/design_record_construct_property_panel.md` shared by both modes. |
| Promotion — what it creates | A new def with the anonymous fields in authored order, the same types, no hints, and fresh `FieldId`s (`claim_field_ids`). The node's `schema` becomes the def name and `fields` is cleared. Wires survive through name matching, and literals survive because they are keyed by name. |
| Promotion — reuse instead of create | The dialog first lists every def (user and linked) whose field set is **exactly** the anonymous one: the same names with equal types, with no `Optional` fields and no extra fields. Choosing one just sets `schema` (pins reorder to that def's authored order; wires are kept by name). A wider def is not offered: its extra required fields would be unwired, and the record would become `None`. |
| Promotion — downstream nodes | Not touched. Downstream wires stay valid because the named type has exactly the anonymous type's shape. In particular, a downstream `expr` whose parameter was declared with the anonymous type, for example by dropping this node's output on its `+`, keeps that type, so `r.a` in it keeps working. Retyping such a parameter to the new name would *break* it, since `expr` cannot read fields of a named record. |
| Switching mode from the schema dropdown | Picking a def on an anonymous node is allowed. Local fields are cleared. Wires and literals carry over by name where the def has a same-named field; the other wires are dropped (the usual repair rule), and so are the other literals. A literal is never left behind to reappear when a field of the same name is added later. Picking "none" on a named-mode node leaves an anonymous node with **no** fields, under the same literal rule. Copying the def's fields into local ones (un-promotion) is future work. |
| Text format | No new syntax beyond a `fields` property mirroring `expr`'s `parameters`: `r = record_construct { fields: [{name: "a", data_type: Float}], a: x }`. The open port is a GUI gesture; in text, declaring is already just writing. |
| Reserved names | `expr`: the lexer's keywords, plus `expression` and `parameters` (its property keys). `record_construct`: `schema`, `fields` and `literal_values`. When a derived name hits one, `_` is appended. |

## The open port, in general

A node opts in through four new `NodeData` hooks. The defaults mean "no
port":

```rust
// node_data.rs
/// Whether this node currently shows an open input port (the `+` row).
/// May depend on the data — `record_construct` has one only in anonymous mode.
fn has_open_input_port(&self) -> bool { false }

/// Append a wire-declared input. `name` is already derived, sanitized and
/// deduplicated against the node's current input names; `data_type` is the source's
/// resolved type. Returns the new input's **pin id**, so the caller can find
/// its index after the node type is re-derived. Errors when the type is not
/// acceptable (the same predicate `accepts_open_port_type` reports).
fn append_open_port_input(&mut self, name: String, data_type: DataType) -> Result<u64, String> {
    Err("no open input port".into())
}

/// Type gate for the drop, consulted on hover (Flutter `onWillAccept`) and
/// again by the orchestrator. `Err` carries the user-facing reason.
fn accepts_open_port_type(&self, data_type: &DataType) -> Result<(), String> {
    Err("no open input port".into())
}

/// Names the derived name must avoid, beyond the node's current input
/// names (keywords, property keys).
fn open_port_reserved_names(&self) -> &'static [&'static str] { &[] }
```

The orchestrator is `StructureDesigner::add_input_from_wire`, which takes the
same source description as `connect_wire`:

```rust
pub fn add_input_from_wire(
    &mut self,
    dest_scope_path: &[u64],
    dest_node_id: u64,
    source_node_id: u64,
    source_pin: SourcePin,        // NodeOutput { pin_index } | ZoneInput { pin_index }
    source_scope_depth: u32,      // capture wires across zone boundaries, as connect_wire
) -> Result<AddedInput, String>   // AddedInput { pin_index, pin_id, name }
```

Steps 1–4 only check; nothing changes until step 5, so every refusal leaves
the network untouched and pushes no undo step.

1. Refuse when the network is read-only (linked library) or the destination
   has no open port.
2. Resolve the source's type (the "declared type" rule in the decision
   table). Refuse `None`.
3. `accepts_open_port_type`.
4. Derive the name (below).
5. Snapshot the network, and capture pre-edit footprints for reflow.
6. `append_open_port_input`, re-derive the node's custom type
   (`set_custom_node_type`), find the new pin's index **by `pin_id`**, and
   connect the wire through the same internal path `connect_wire` uses.
7. Reflow neighbours if the node grew
   (`doc/design_reflow_on_footprint_change.md`), revalidate, mark the design
   dirty, then snapshot again and push one `NetworkSnapshotCommand
   { network_name, description: "Add input from wire", before, after }` with `Full` refresh.

The API takes the same arguments as `connect_wire` (`dest_scope_path`,
`source_node_id`, `source_pin: APISourcePin`, `source_scope_depth`,
`dest_node_id`) and has two functions:

- `add_input_from_wire(...) -> Result<APIAddedInput, String>` performs the
  drop.
- `can_add_input_from_wire(...) -> Option<String>` is the sync hover check:
  `None` means accepted, `Some(reason)` means refused.

### Name derivation

`open_port_name.rs` (crate root, beside `record_path.rs`) holds a pure
function so it can be tested without a network:

```rust
pub struct NameSource<'a> {
    pub node_name: Option<&'a str>,   // source node's custom_name
    pub pin_name: Option<&'a str>,    // source output / zone-input pin name
    pub prefer_pin: bool,             // source node has > 1 output pin, or a zone input
}

pub fn derive_open_port_name(
    src: &NameSource,
    taken: &HashSet<String>,          // current input names on the destination
    reserved: &[&str],
    fallback: &str,                   // "x" for expr, "field" for record_construct
) -> String
```

The rules:

1. **Base.** A function pin (`-1`) uses the node name. Otherwise, if
   `prefer_pin` is set, use the pin name, or `<node>_<pin>` if the pin name
   is taken. For a zone input, `<node>` is the HOF, e.g. `map1_element`.
   Otherwise use the node name. If nothing is available, use `fallback`.
2. **Sanitize.** Every character outside `[A-Za-z0-9_]` becomes `_`. Collapse
   runs of `_`, and trim leading and trailing ones. Prefix `_` if the name
   starts with a digit. Empty → `fallback`. Relaxed node names
   (`doc/design_relaxed_node_names.md`) such as `hexirod_[0001]30°` become
   `hexirod_0001_30`.
3. **Reserved.** If the name is in `reserved`, append `_`.
4. **Deduplicate.** `name`, then `name_2`, `name_3`, … until it is not in
   `taken`. This is the same suffix scheme as `NodeNetwork::unique_name_for`.

The case of the name is left as it is.

## `expr`

The data model doesn't change. `append_open_port_input` pushes
`ExprParameter { id: max_id + 1, name, data_type, data_type_str: None }`, the
same id rule `set_text_properties` uses, and re-runs `parse_and_validate` so
the output type stays current. The expression text is **not** edited: the
new parameter is unused until the user writes it in. That's a cheap state
(no error), and guessing an edit to the user's expression is not.

## Anonymous `record_construct`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordConstructData {
    #[serde(default)]
    pub schema: String,
    /// Anonymous mode only (`schema` empty): the locally declared fields, in
    /// authored order. Must be empty while `schema` is set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<LocalRecordField>,
    /// Allocator floor for `LocalRecordField::id`; never decreases.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub next_field_id: u64,
    #[serde(default)]
    pub literal_values: HashMap<String, TextValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalRecordField {
    pub id: u64,
    pub name: String,
    pub data_type: DataType, // never Optional
}

/// Anonymous field pins carry `ANON_FIELD_PIN_ID_BASE | field.id`.
pub const ANON_FIELD_PIN_ID_BASE: u64 = 1 << 62;
```

`skip_serializing_if` keeps named-mode `.cnnd` output byte-identical to
today, so existing files and corpus snapshots don't churn.

- **Node type** (`build_record_node_type`, `build_node_type_for_schema_with_defs`):
  in anonymous mode, one `Parameter { id: Some(BASE | f.id), name, data_type }`
  per field, and output `Record(RecordType::anonymous(..))`. The anonymous
  branch needs no defs, so it never freezes under `MissingDefs::Refuse`.
- **Eval:** the existing loop, factored to walk a list of `(name, pin_type,
  is_optional)` that either mode supplies. Anonymous fields are all required,
  so an unwired field with no literal makes the record `None`. That is
  today's rule for a required field.
- **Subtitle:** the schema name in named mode; nothing in anonymous mode (the
  pins already show the fields).
- **Drag-aware add** (`adapt_for_drag_source`): `FromInput` from an
  **anonymous** record type now adapts too, into anonymous mode with those
  fields. Dragging off an `expr` parameter typed `{a: Float, b: Int}` and
  choosing `record_construct` gives a matching builder. `FromOutput` stays
  unadapted (zero fields). Instead, the auto-connect step targets the open
  port (Phase 4), so "drag an output into empty space → `record_construct`"
  creates a one-field record, named by the same derivation.
- **Text format:** `fields` property as in the decision table. On set, ids
  are preserved by name first, then by position, exactly as
  `ExprData::set_text_properties` does for parameters. Properties are applied
  before inputs, so `a: x` finds its pin.
- **Rename** (panel / API) moves the field's `literal_values` key. Delete
  drops it.

### Promotion

```rust
/// Exact-shape matches for an anonymous `record_construct`: every def whose
/// fields are the same name set with equal types and no `Optional`. Sorted by
/// name. Includes linked defs.
pub fn matching_record_defs_for(&self, scope_path: &[u64], node_id: u64) -> Vec<String>;

/// Create def `def_name` from the node's anonymous fields and point the node at it.
pub fn promote_record_construct(
    &mut self, scope_path: &[u64], node_id: u64, def_name: &str,
) -> Result<(), String>;
```

`promote_record_construct`:

1. Refuse when the node is not in anonymous mode, has no fields, or the
   network is read-only.
2. Validate `def_name` as the user-types panel does (not taken, not under a
   linked namespace, a valid qualified name).
3. Snapshot the network. Build the `RecordTypeDef`, `claim_field_ids`, and
   add it through the **non-pushing** registry call.
4. Set `schema = def_name` and clear `fields`; keep `literal_values` (all of
   them still name a field) and `next_field_id` (it never decreases). Re-derive
   the node type: the pins now carry `FieldId`s, none of which can equal a
   `BASE | id` pin id, so every wire matches by name.
5. Revalidate, mark the design dirty and request a full refresh. Snapshot the
   network again, then push
   `CompositeCommand [AddRecordTypeDefCommand, NetworkSnapshotCommand]`
   described as "Name record". Undo restores the network, then removes the
   def; redo adds the def, then restores the promoted network.

"Use existing def" is the ordinary schema setter (the dropdown's own path,
`SetNodeDataCommand`), so it needs no new command.

## Flutter

- **`NodeView` / API:** `has_open_input_port: bool` per node, from the hook.
- **Node widget:** when set, one extra input row below the last pin, holding
  a `+` glyph in the pin column. It is a `DragTarget<PinReference>`. Its
  `onWillAcceptWithDetails` applies the scope-prefix and zone-input rules of
  `canConnectPins`, then calls `can_add_input_from_wire`, keeping the refusal
  reason in widget state. `onAcceptWithDetails` calls
  `model.addInputFromWire(...)`. The builder draws the accepted highlight
  from `candidateData`, and the refused style plus the reason label from
  `rejectedData`. Both are per-target and need no `notifyListeners`, which
  keeps "drag ticks never notify" intact
  (`project_canvas_drag_performance`).
- **Node height:** the extra row counts as one input row in Flutter's
  `getNodeSize()` **and** in the Rust estimators that mirror it
  (`node_layout::estimate_node_height` callers, `layout/common.rs::node_height`),
  or auto-layout and reflow will place the node one row short.
- **After the drop:** `addInputFromWire` selects the destination node and
  sets `pendingNameFocus = (nodeId, inputIndex)` from the result's
  `pin_index`. It is keyed by index, not pin id, because `APIExprParameter`
  carries no id; on both nodes an input's index is also its row in the
  editor's list. The `expr` editor and the `record_construct` editor's field
  list consume it on the next build: they focus that row's name field and
  select its text.
- **`record_construct` editor**, anonymous mode: a field list styled like
  the `expr` parameter list (name, type selector, delete, reorder, and an
  **Add field** button, default type `Float`), the existing per-field literal
  editors, and a **Name this record…** button. The same action is on the
  node's context menu.
- **Name this record… dialog:** a name field and, when
  `matching_record_defs_for` is non-empty, a "Same shape as:" list. Each
  entry is a **Use** button that points the node at that def instead.

## Phases

Each phase leaves `main` green: `cargo test -j 4` (the `structure_designer_api`,
`integration` and `renderer_api` harnesses run explicitly), `cargo clippy`
with no new warnings, and `flutter analyze` at baseline.

### Phase 0 — anonymous record types parse from strings

`DataType::from_string` rejects `{a: Float}` (`data_type.rs`,
`parse_builtin_type`: "Anonymous record syntax … is not parsed here"), yet
that is exactly how `Display` prints an anonymous record. The API's
`APIDataType` has no structural variant for anonymous records, so they fall
through to `Custom` with that string, and parsing it back fails. The
consequence today: an `expr` parameter whose type is an anonymous record
(easy to get through drag-aware add from an `expr` record literal) turns into
`None` as soon as the user edits *any* parameter in the panel. The reference
guide also claims `expr` parameter types accept `{x: Int, y: Int}`
(`reference_guide/nodes/math_programming.md`), which is not true today. The
open port would make anonymous-typed inputs common, so this must be fixed
first.

- Add `{`, `}`, `:` and `,` to the type lexer where they're missing, and
  parse `{}` and `{name: Type, …}` (nested and arrayed: `[{a: Int}]`,
  `{p: {x: Float}}`) into `RecordType::anonymous`.
- Duplicate field names are an error. Field names follow the identifier
  rule. The printer (`Display` for `RecordType::Anonymous`) never quotes
  names, so the parser needs no quoting either.

Tests (`crates/atomcad-structure-designer/tests/structure_designer/anonymous_record_type_parse_test.rs`):

- parse ↔ `Display` round-trip for `{}`, a flat record, a nested record, an
  array of records, and a function type with a record parameter;
- duplicate-name rejection;
- field order canonicalizes (`{b: Int, a: Int}` == `{a: Int, b: Int}`).

API test (`rust/tests/structure_designer_api/`): an `expr` with an
anonymous-typed parameter survives a `set_expr_data` round-trip of its
parameter list. Docs: the reference-guide sentence becomes true; nothing to
change there beyond an example.

### Phase 1 — the open port core, and `expr`

- `NodeData` hooks; `open_port_name.rs`; `StructureDesigner::add_input_from_wire`
  and `NetworkSnapshotCommand` (`undo/commands/network_snapshot.rs`); `ExprData` implements the hooks and
  `accepts_parameter_type` is factored out of `adapt_for_drag_source`.
- API: `add_input_from_wire`, `can_add_input_from_wire`,
  `has_open_input_port` on the node view. Run `cargo fmt`, then FRB codegen.

Tests (`tests/structure_designer/open_port_test.rs` and
`tests/structure_designer/open_port_name_test.rs` in the crate, plus
`rust/tests/structure_designer_api/`):

- name derivation table: plain node name; multi-output → pin name;
  pin-name collision → `<node>_<pin>`; zone input → pin name; function pin →
  node name; relaxed-name sanitization; leading digit; reserved word (`if` →
  `if_`); dedup `_2` / `_3`; empty → fallback;
- `add_input_from_wire` on `expr`: the parameter is appended with the
  source's type, the wire is connected at the new last index, earlier wires
  are untouched, the expression text is unchanged, and the result carries
  the derived name;
- a polymorphic source (`SameAsInput`) declares the **resolved** type;
- refusals: abstract source type, `Function` source, `None`-typed source,
  read-only network, destination without a port. Each leaves the network
  unchanged and pushes no undo step;
- capture wire: source in the parent scope, destination inside a `map` body;
- undo restores parameter list and wires in one step; redo reapplies;
- a later rename through `set_expr_data` keeps the wire (id preserved).

### Phase 2 — anonymous `record_construct`

- Data model, `ANON_FIELD_PIN_ID_BASE`, anonymous node-type branch,
  factored eval, text-format `fields`, the hooks, the `FromInput` anonymous
  adaptation, rename moving literal keys, and a mode switch dropping the
  literals that do not carry over.
- Field-type predicate: reuse the one the schema editor / def validator
  applies to a field type. If none exists as a function, add
  `is_valid_record_field_type` beside `validate_field_optionals` and have
  both use it. (Whether `Iter[T]` belongs in a record is decided there, once,
  for both paths.)
- API: getters and setters for the local field list, and
  `set_record_construct_fields` (rename / retype / delete / reorder / add),
  following `set_expr_data`'s id-preservation rules.

Tests (`tests/structure_designer/record_construct_anonymous_test.rs`,
API tests, cnnd round-trip):

- eval: wired fields build `Record(Anonymous)` with the right values; an
  unwired field with no literal gives `None`; a literal is used when
  unwired; zero fields give `{}`;
- the output wires into an `expr` parameter typed with the same anonymous
  type, and `r.a` evaluates;
- `add_input_from_wire` appends a field; refusals as in Phase 1;
- mode switch: an anonymous node with fields `a` and `b` (wired, `a` also
  with a literal) picks def `Foo { b, c }`. `b`'s wire is kept, `a`'s wire
  and literal are dropped, and no wire lands on `c`. Run this a second time
  with local ids chosen to collide numerically with `Foo`'s `FieldId`s,
  which is the case the id base exists for. Undo of the switch restores `a`
  and both wires;
- legacy: a saved `record_construct` with an empty `schema` loads as an
  anonymous node with no fields, outputting `{}`;
- text format: `query` → `--replace` of an anonymous node is a no-op
  (round-trip corpus rule); `fields` + `a: x` builds the node from text;
- `.cnnd` round-trip with a new fixture holding an anonymous node. Build its
  source values with `add_value_node` / `add_typed_value_node`. The new
  fixture adds two corpus snapshots, to be accepted with `cargo insta review`.
  A named-mode file re-serializes byte-identically;
- drag adapter: `FromInput` from `{a: Float}` adapts to anonymous mode with
  field `a`; the existing named-mode cases are unchanged.

Docs: the `record_construct` section of
`reference_guide/nodes/math_programming.md` (anonymous mode and its text
form; the `+` row is documented in Phase 4, where it appears); the records
bullet in
`crates/atomcad-structure-designer/src/nodes/AGENTS.md` (anonymous mode, the
pin-id base invariant).

### Phase 3 — promotion

- `matching_record_defs_for`, `promote_record_construct`, the composite
  undo, and the API pair.

Tests:

- promotion creates def `Box` with the fields in authored order, the same
  types and fresh `FieldId`s; the node is in named mode; every input wire,
  every downstream wire and every literal survive; eval output is equal
  before and after;
- a downstream `expr` declared with the anonymous type still validates and
  evaluates `r.a`;
- undo removes the def and restores the anonymous node with its wires in one
  step; redo reinstates both;
- refusals: name taken, linked namespace, not anonymous, no fields. Each
  leaves no def and no undo step;
- matching: an exact def is listed; a def with an extra field, a missing
  field, a different type or an `Optional` field is not; linked defs are
  listed;
- "use existing": pins reorder to the def's authored order and wires follow
  by name.

### Phase 4 — Flutter: the `+` row, dropping, and the panels

- `+` row and `DragTarget`, hover check, refused style and reason label,
  node-height accounting on both sides, `pendingNameFocus`.
- Drag-aware add: auto-connect to a newly created `record_construct` (zero
  fields, from a `FromOutput` drag) goes through `addInputFromWire` instead
  of a pin index.
- `expr` editor: consume `pendingNameFocus`.
- `record_construct` editor: anonymous field list, Add field, Name this
  record… dialog with the "Same shape as" list, and the context-menu entry.

Tests: a Rust test that the estimators count the open-port row (an `expr`
with *n* parameters estimates as *n*+1 rows; named-mode `record_construct`
as *n*). The UI itself is checked by a manual walkthrough
(`feedback_manual_test_for_editor_ui`):

1. Drop three outputs on an `expr` `+`: three parameters with derived names,
   name field focused each time; rename one; the wire stays.
2. Hover an abstract-typed or a function output over an `expr` `+`: the row
   shows the refused style and the reason, and the drop does nothing.
3. Drag an output into empty space → `record_construct`: one-field anonymous
   record. Drop two more on its `+`. Wire its output into a new `expr`
   (`r.<field>` works).
4. Name this record… → `Box`: the def appears in the user-types panel; no
   wire moved; Ctrl+Z undoes it in one step, and Ctrl+Y redoes it.
5. A second anonymous node with the same shape: the dialog offers `Box`
   under "Same shape as"; Use → pins reorder, wires stay.
6. Inside a `map` body, drop a captured outer value on an `expr` `+`.
7. Auto-layout a network containing these nodes: no overlaps (height
   accounting).
8. Flutter smoke test (`flutter test integration_test/`): **human-only**, run
   by the maintainer.

Docs: `reference_guide/node_networks.md` gets a short "open port" paragraph
(where it appears, what a drop does, renaming after); the `expr` and
`record_construct` sections mention the `+` row and Name this record…; and
`lib/structure_designer/AGENTS.md` notes that the `+` row counts in node
height on both sides.

## Testing

Rust behaviour is tested at the layer that owns it: name derivation as a
pure function, the orchestrator and node behaviour through
`StructureDesigner` in the crate's `tests/`, and the FFI surface in
`rust/tests/structure_designer_api/`. Persistence uses the existing `.cnnd`
round-trip corpus and the text-format `query` → `--replace` no-op rule. If
the `record_construct` node-type description changes, `node_snapshots`
needs a `cargo insta review`. The Flutter canvas and panel work is verified
by the Phase 4 walkthrough.

## Future work

- **Drag from `+`** to a source output: the same operation started from the
  other end.
- **Pack selection into record:** select several nodes and choose "Pack
  outputs into record". This creates an anonymous `record_construct` with
  one field per selected node's primary output, which is the bulk form of
  dropping each one on the `+`. It is a loop over `add_input_from_wire`
  inside one undo step.
- **Un-promote ("Detach from def"):** copy a named-mode node's def fields
  into local fields. This is the inverse of promotion, for forking a one-off
  variant.
- **Anonymous schemas on `record_get` / `record_set` / `record_destructure`**
  ("schema from input wire", `doc/design_record_get_set.md` open question 2).
  It would make promotion optional rather than required for deep reads and
  updates.
