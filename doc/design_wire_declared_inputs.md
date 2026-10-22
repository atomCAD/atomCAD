# Wire-declared inputs: the open port on `expr` and anonymous `record_construct`

## Status: Phases 0–5 implemented (2026-10-22); Phase 4 and Phase 5 manual walkthroughs pending

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
tab that hangs below the node while a wire is being dragged. Dropping the
wire on it appends an input of the wire's type, under a derived name,
connects the wire, and puts the cursor in the new input's name field so a
rename is one keystroke away.

The type is copied once, at the drop, and does not follow the wire afterwards.
When the source changes later, the input's type goes stale: the wire reports
a mismatch, or, for a record that gained a field, it stays valid but is too
narrow to read the new field. **Take type from wire** is the repair: one
click copies the type from the wire again. It applies to any input the node declares
itself, however that input was made, and it matters most for anonymous
records: renaming one of their fields changes their type, so every input that
copied the old type goes stale at once.

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

- the `DeclaredInputs` capability trait that both nodes implement;
- the open port on `expr`;
- an anonymous mode for `record_construct`, with an open port;
- **Take type from wire** on both nodes' declared inputs;
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
| Which nodes get an open port | `expr` (always) and `record_construct` **in anonymous mode** (its `schema` is empty). Nothing else in this doc. Both are exactly the nodes whose `NodeData::declared_inputs()` returns `Some`, so the open port and Take type from wire share one gate. |
| The abstraction | A capability trait, `DeclaredInputs`, which a node hands out through `NodeData::declared_inputs()` / `declared_inputs_mut()` (default `None`). This avoids a set of separate `NodeData` hooks with "no open port" defaults. The `Option` expresses the mode dependence directly: `record_construct` returns `Some(self)` only in anonymous mode. The orchestrators in `declared_input_ops.rs` never name a node type. |
| How the open port looks | A `+` **tab hanging below the node's bottom edge**, at the input-pin column. It is shown on every visible node with declared inputs while a wire is being dragged, and nowhere else. It is highlighted while a compatible wire hovers it. The tab is drawn **outside** the node's footprint, in a canvas layer above the nodes. So the node's size, its pin offsets and every saved layout stay as they are, and nothing reflows, mid-drag or ever. Two alternatives were rejected. An *in-node row that appears mid-drag* would change the footprint and move pins while the user is aiming. A *permanent in-node row* would make every existing `expr` one row taller: saved `.cnnd` layouts were positioned for today's heights, so they would overlap wherever a node sits tightly below an `expr`, and reflow runs only on edits, never on load. The tab may briefly cover the top of a node beneath it. That is acceptable for the length of a drag, and because the tab is on top, it wins the hit test. |
| Gesture direction | **Output → `+` only** in v1: drag from a source pin (a node output, a function pin, or a zone input) and drop on the `+`. The `+` itself is not draggable. Dragging from `+` to an output is a later addition (see "Future work"). |
| The declared type | The source pin's **resolved type at drop time**, snapshotted (`NodeTypeRegistry::resolve_output_type_scoped` for node outputs, the HOF's zone-input pin type for zone inputs). The type does not follow the wire afterwards. If upstream changes, the wire shows a type error like any other wire, or, when a record source gains a field, stays valid but too narrow (see the Take type from wire rows). A live type would make the interface depend on wiring, and undo and the text format would have to track that dependency. A stale type is repaired by Take type from wire. |
| Take type from wire — what it does | It sets a declared input's type to the **offered type** (next rows), computed from the single wire on it at the time of the click, through `DeclaredInputs::retype`. The pin id, the name, the wire and the expression text are untouched. One undo step, "Take type from wire", recorded as a `NetworkSnapshotCommand`. |
| Take type from wire — when it is offered | Only on a **stale** input. All of these must hold: (1) the node has declared inputs, and `declares_pin(pin_id)` is true; (2) exactly one wire is on the pin; (3) the source's resolved type is stale against the declared type: either it fails `can_be_converted_to`, or both are records whose resolved fields differ (next row); (4) `accepts_type(offered type)` passes. A scalar wire that is compatible but different (`Int` into a `Float` parameter) gets no offer: it is not broken. A **record** wire that is compatible but different does get one. Records have width subtyping (`can_be_converted_to_impl` in `data_type.rs`), so when a field is added to an anonymous `record_construct` that already feeds an `expr`, the wire `{a, b, c} → {a, b}` stays valid with no error. `r.c` in the expression then fails, and nothing else would repair it. This is the most common way a record input goes stale. A source that turned into something the node cannot declare, such as a `Function`, keeps the plain mismatch error and gets no offer. |
| Take type from wire — record staleness and the offered type | Two records "differ" when their resolved, canonically sorted field lists are not equal (names and `DataType`s). This is not `DataType` equality: a promoted source `Named(Box)` with exactly the declared anonymous shape is **not** stale. The offered type is the source's resolved type, with one exception. When the declared type is an anonymous record and the source is a named record, the offer is the source's **anonymous form** (`RecordType::anonymous` of its resolved fields). An anonymous declared input stays anonymous: `expr` cannot read a named record's fields, so offering the name would swap a stale type for a broken one (see "Promotion — downstream nodes"). |
| Take type from wire — where | The property panel is the primary surface: a stale row gets a warning marker and a **Use wire's type: `<type>`** button under its type selector. The node context menu's **Edit** section adds **Take types from wires**, which repairs every stale input in one step and appears only when at least one qualifies. There is no wire context menu in v1, because the canvas has none today (see "Future work"). |
| Renaming an anonymous field | The rename changes the record's type (`{a: Float}` becomes `{b: Float}`), so every input that copied the old type now mismatches. The wire is kept and reported, not dropped: `repair_node_network` drops no node-output wire over a type change (only `repair_zone_body`'s zone-input wires are type-checked, and a rename does not change a zone-input pin's type), so `validate_wires` reports the mismatch on the consumer. The renamed node's own input wire survives through the stable pin id. The repairs are Take type from wire for each stale input, or promotion, after which renames of the named def's fields go through `FieldId`s. A rename does **not** retype downstream inputs automatically, because that would be the live typing the "declared type" row rules out. |
| Which types an open port accepts | `expr`: the predicate its drag-aware adapter already applies: no abstract types, `Function`, `Iterator`, `Unit` or `None` (`ExprData::adapt_for_drag_source`), factored into a shared `ExprData::accepts_parameter_type`. `record_construct`: any type a record def field may hold (see Phase 2), and never `None`, `Unit` or `AnyFunction`. While a wire with a rejected type hovers the `+`, the tab is drawn in a refused style with the reason as a one-line label beside it. A tooltip would not work here, because Flutter shows no tooltips during a drag. |
| The derived name | The source **node's name** (`custom_name`, e.g. `gap` or `sphere1`). When the source node has more than one output pin, or the source is a zone input, the **pin name** is used instead; if that is already taken, `<node>_<pin>`. The name is sanitized to `[A-Za-z_][A-Za-z0-9_]*`, made to avoid reserved words, and deduplicated as `name`, `name_2`, `name_3`, … (details under "Name derivation"). **Superseded for `expr` by Phase 5:** a node with a name pool (`expr`: `x, y, z, a, …`) takes the next free pool name instead, and the source-derived name is offered as a one-click rename. This rule still names `record_construct` fields. |
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
| Undo of a schema change | One `NetworkSnapshotCommand` ("Set record schema"), **not** the `SetNodeDataCommand` that `set_record_construct_data` records today (through `set_node_network_data_scoped`). That command restores node data only. The wires a switch drops are lost when the pins are re-derived, so undo would bring back field `a` without its wire. The setter becomes a `StructureDesigner` method: snapshot the network, write the data, re-derive the node type (which drops the wires that do not carry over), snapshot again, push. A named → named switch has the same gap today, and this change closes it too. |
| Def references in local fields | A local field's type can name a def. Dropping a `Box` output on the `+` declares a `Record(Named("Box"))` field, so this is common. Every place that finds or rewrites record-def names in node data must therefore see local field types, not only `schema`. Those places are: `collect_record_refs_in_node`, which feeds the delete and namespace-delete reference checks, the library dependency closure / `uses`, frozen-node detection, the invariant checker and cross-document paste; `rewrite_record_names_in_node`, for def rename; and `canonicalize::canonicalize_node_data`, which their doc comments require to stay in sync. These are hand-maintained downcast chains, and `record_construct` reaches them only through the trailing `record_schema_ref` fallback. A downcast branch for it is not allowed: `record_node_hook_test.rs` forbids downcasting to a record node outside its own files. So the fallback itself reads the local field types, through a record-node hook, `NodeData::local_record_fields` / `local_record_fields_mut` (default `None`). The fallback skips an empty `schema` and covers every `fields[*].data_type`. Without it, renaming `Box` leaves a dangling `Named("Box")` in the field, the delete reference check does not see the node, and a linked def named only by a field is missing from `uses` and never freezes the node. |
| Text format | No new syntax beyond a `fields` property mirroring `expr`'s `parameters`: `r = record_construct { fields: [{name: "a", data_type: Float}], a: x }`. The open port is a GUI gesture; in text, declaring is already just writing. |
| Reserved names | `expr`: the lexer's keywords, plus `expression` and `parameters` (its property keys). `record_construct`: `schema`, `fields` and `literal_values`. Each node supplies its list through `DeclaredInputs::reserved_names`. When a derived name hits one, `_` is appended. |

## Declared inputs, in general

### The `DeclaredInputs` trait

A node whose input list is its own data implements one capability trait and
hands it out through two `NodeData` accessors. Every operation in this doc,
the open-port drop and Take type from wire alike, reaches the node only
through it:

```rust
// declared_inputs.rs (crate root)

/// A node whose input pins are declared by its own data, one pin per
/// declared input: `expr`'s parameters, an anonymous `record_construct`'s
/// fields. Declared inputs are addressed by **pin id**, the id the node puts
/// on the pin's `Parameter` when it builds its node type
/// (`calculate_custom_node_type` for `expr`, `build_record_node_type` for
/// `record_construct`). Ids survive renames, reorders and the re-derivation
/// of the node type; indices do not.
///
/// Every mutating method edits the node's data only. The caller re-derives
/// the node type (`Node::set_custom_node_type`), revalidates and records
/// undo; see `declared_input_ops.rs`.
pub trait DeclaredInputs {
    /// Type gate shared by the open port (append) and Take type from wire
    /// (retype). `Err` carries the user-facing reason. It is shown beside a
    /// refused `+` tab, and the retype offer is withheld.
    fn accepts_type(&self, data_type: &DataType) -> Result<(), String>;

    /// Names a derived name must avoid beyond the current input names:
    /// keywords and property keys.
    fn reserved_names(&self) -> &'static [&'static str];

    /// The name to use when the source offers none: `"x"` for `expr`,
    /// `"field"` for `record_construct`.
    fn fallback_name(&self) -> &'static str;

    /// Whether `pin_id` names one of the declared inputs. It is `true` for
    /// every input pin of the two nodes in this doc. It is a method rather
    /// than an invariant so that a future node can mix fixed pins with
    /// declared ones.
    fn declares_pin(&self, pin_id: u64) -> bool;

    /// Append a declared input. `name` is already derived, sanitized and
    /// deduplicated. Returns the new input's pin id. Errors when
    /// `accepts_type` refuses `data_type`.
    fn append(&mut self, name: String, data_type: DataType) -> Result<u64, String>;

    /// Retype the declared input `pin_id` in place. The name, the id and the
    /// position are kept. Errors when the pin is not declared or
    /// `accepts_type` refuses `data_type`.
    fn retype(&mut self, pin_id: u64, data_type: DataType) -> Result<(), String>;
}

// node_data.rs, in the NodeData trait
/// The node's declared inputs, when its input list is its own data. `None`
/// (the default) means the node's inputs are fixed by its type or by a def.
/// May depend on the data: `record_construct` answers `Some` only in
/// anonymous mode. `Some` is also what gives the node the open port (the
/// `+` tab shown during a wire drag).
fn declared_inputs(&self) -> Option<&dyn DeclaredInputs> { None }
fn declared_inputs_mut(&mut self) -> Option<&mut dyn DeclaredInputs> { None }
```

`ExprData` and `RecordConstructData` implement `DeclaredInputs` and return
`Some(self)` (anonymous mode only, for the latter). Nothing on the open-port
or Take type from wire paths checks for these two node types by name (the
record-def reference walkers are a separate matter; see "Def references in
local fields"): `NodeView.has_open_input_port` is
`declared_inputs().is_some()`, and the orchestrators below take whatever the
accessor returns.

A capability trait rather than flat `NodeData` hooks: the methods are only
meaningful together, the open port and the retype share the gate, and a node
without declared inputs implements nothing. It does not need four defaults
that each return "no open port".

### Shared pieces

`declared_input_ops.rs` (an `impl StructureDesigner` file, like
`library_link_ops.rs`) holds both orchestrators and the one helper they share:

```rust
/// The resolved type of the source a wire describes. A node output resolves
/// through `NodeTypeRegistry::resolve_output_type_scoped`, which follows
/// `SameAsInput` up the chain. A zone input resolves to the HOF's zone-input
/// pin type at `source_scope_depth`. The function pin resolves to the
/// node's function signature. It must give the same answer as the type
/// `validate_wires` checks a wire against (its `ValidationContext::resolve`),
/// so an offer and the validator can never disagree about a mismatch. If the
/// two paths cannot share code, a test pins them together.
fn resolve_wire_source_type(
    &self,
    dest_scope_path: &[u64],
    source_node_id: u64,
    source_pin: SourcePin,
    source_scope_depth: u8,
) -> Option<DataType>
```

### Adding an input: the open port

The orchestrator is `StructureDesigner::add_input_from_wire`, which takes the
same source description as `connect_wire`:

```rust
pub fn add_input_from_wire(
    &mut self,
    dest_scope_path: &[u64],
    dest_node_id: u64,
    source_node_id: u64,
    source_pin: SourcePin,        // NodeOutput { pin_index } | ZoneInput { pin_index }
    source_scope_depth: u8,       // capture wires across zone boundaries, as connect_wire
) -> Result<AddedInput, String>   // AddedInput { pin_index, pin_id, name }
```

Steps 1–4 only check; nothing changes until step 5, so every refusal leaves
the network untouched and pushes no undo step.

1. Refuse when the network is read-only (linked library) or the destination's
   `declared_inputs()` is `None`.
2. `resolve_wire_source_type`. Refuse `None`, both an unresolvable source and
   a `None`-typed one.
3. `accepts_type`.
4. Derive the name (below), passing `reserved_names()` and `fallback_name()`.
5. Snapshot the network, and capture pre-edit footprints for reflow.
6. `declared_inputs_mut()?.append`, re-derive the node's custom type
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

*Phase 5 note:* this is how a node **without** a name pool names a new input
(`record_construct`). `expr` now has a pool and takes letters; this
derivation still computes the name its rename chip offers. See Phase 5.

`declared_inputs.rs`, beside the trait, holds a pure function so it can be
tested without a network:

```rust
pub struct NameSource<'a> {
    pub node_name: Option<&'a str>,   // source node's custom_name
    pub pin_name: Option<&'a str>,    // source output / zone-input pin name
    pub prefer_pin: bool,             // source node has > 1 output pin, or a zone input
}

pub fn derive_open_port_name(
    src: &NameSource,
    taken: &HashSet<String>,          // current input names on the destination
    reserved: &[&str],                // DeclaredInputs::reserved_names
    fallback: &str,                   // DeclaredInputs::fallback_name
) -> String
```

It takes the two node-supplied lists as plain arguments rather than a
`&dyn DeclaredInputs`, so its tests need no node.

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

### Repairing a type: Take type from wire

Two `StructureDesigner` methods in `declared_input_ops.rs`, one read and one
write. Both compute the offers the same way:

```rust
/// One entry per input pin of the node, by pin index: `Some(type)` when Take
/// type from wire is offered for that pin, carrying the type it would set.
/// Empty when the node has no declared inputs. Read-only: no refresh, no undo.
pub fn wire_type_offers(&self, scope_path: &[u64], node_id: u64) -> Vec<Option<DataType>>;

/// Retype each listed input to its offered type, in one undo step.
pub fn take_types_from_wires(
    &mut self,
    scope_path: &[u64],
    node_id: u64,
    pin_indices: &[usize],
) -> Result<(), String>;
```

`wire_type_offers` walks the node's input pins. For pin index `i`, with pin
id `id` (from the node type's `Parameter`), it offers `Some(t)` exactly when
all four conditions of the decision table hold:

1. `declared_inputs()` is `Some` and `declares_pin(id)`;
2. the argument at `i` has exactly one incoming wire;
3. `resolve_wire_source_type` of that wire gives `s`, and `s` is stale
   against `declared`: `can_be_converted_to(s, declared)` is false, or both
   are records whose resolved field lists differ;
4. the offered type `t` is `s`, or `s`'s anonymous form when `declared` is
   an anonymous record and `s` a named one, and `accepts_type(&t)` is `Ok`.

`take_types_from_wires` never trusts types sent by the caller. It recomputes
the offers and works all or nothing:

1. Refuse when the network is read-only, or any listed index has no offer.
   An empty list is refused too. Nothing has changed yet, and no undo step
   is pushed.
2. Snapshot the network.
3. For each listed pin, `declared_inputs_mut()?.retype(id, t)`. Retyping
   never changes a node's footprint, so no reflow is needed.
4. Re-derive the node's custom type (`set_custom_node_type`). The pin ids are
   unchanged, so every wire stays on its pin.
5. Revalidate and mark the design dirty. Snapshot again, push one
   `NetworkSnapshotCommand { description: "Take type from wire", .. }` with
   `Full` refresh. The node's output type can change (an `expr` reading
   `r.b`), and that can clear or raise errors downstream.

A record input can be offered while its wire carries **no** error (the width
case in the decision table). So the offers are a superset of the validator's
mismatch errors, not the same set. The test that pins
`resolve_wire_source_type` to the validator states it that way.

Both node surfaces go through `take_types_from_wires`: the panel button with
one index, the context-menu item with every offered index.

API: `get_wire_type_offers(scope_path, node_id) -> Vec<Option<APIDataType>>`
and `take_types_from_wires(scope_path, node_id, pin_indices) -> Result<(),
String>`, both scoped like every other node-property API.

## `expr`

The data model doesn't change. `ExprData` implements `DeclaredInputs`, and
`declared_inputs()` always returns `Some(self)`. A pin's id is its
`ExprParameter::id`, which `calculate_custom_node_type` already copies onto
the `Parameter`. That field is an `Option`, so Phase 1 makes sure every
loaded parameter has one: if a legacy file can still produce an id-less
parameter, the loader gives it `max_id + 1`, the same rule as `append`.
`declares_pin` matches ids only, so an id-less parameter would otherwise be
silently unrepairable.

- `accepts_type` is `accepts_parameter_type`, factored out of
  `adapt_for_drag_source`, so drag-aware add and the open port share one
  predicate.
- `reserved_names`: the lexer's keywords plus `expression` and `parameters`.
  `fallback_name`: `"x"`. (Phase 5 adds `pi` to the reserved names and a name
  pool, `x, y, z, a, …`, so a new parameter is named by letter.)
- `append` pushes `ExprParameter { id: max_id + 1, name, data_type,
  data_type_str: None }`, the same id rule `set_text_properties` uses.
- `retype` replaces `data_type` on the parameter with that id and clears its
  `data_type_str`, so the stored text form cannot disagree with the new type.
- Both re-run `parse_and_validate`, which updates the cached `output_type`
  inside `ExprData`. That is still a data-only edit, as the trait requires.

The expression text is **never** edited. A new parameter is unused until the
user writes it in. That's a cheap state (no error), and guessing an edit to
the user's expression is not. A retype that breaks a member access (`r.a`
after the field became `b`) shows up as the expression's own error, which
the user fixes in the text.

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
- **`DeclaredInputs`:** `declared_inputs()` is `Some(self)` while `schema` is
  empty, and `None` in named mode. A def's fields are retyped in the schema
  editor, never from a wire. A pin's id is `ANON_FIELD_PIN_ID_BASE |
  field.id`, so `declares_pin` checks the base bit and looks up the low bits.
  `accepts_type` is the field-type predicate of Phase 2, `reserved_names` is
  `schema`, `fields` and `literal_values`, and `fallback_name` is `"field"`.
  `append` allocates from `next_field_id`. `retype` replaces the field's
  `data_type` and drops the field's literal. A retype only happens on a
  wired field, whose literal is not in use, and only to a different type,
  which the literal was not written for. The rule is the mode switch's: a
  literal is never kept where it could reappear with the wrong type.
  Undo brings it back.
- **Schema changes** go through a `StructureDesigner` method that records a
  `NetworkSnapshotCommand`, so undo restores the wires a switch drops (see
  "Undo of a schema change").
- **Record-def references:** `collect_record_refs_in_node`,
  `rewrite_record_names_in_node` and `canonicalize::canonicalize_node_data`
  read the local field types through the `NodeData::local_record_fields[_mut]`
  hook in their record-node fallback, which also skips an empty `schema` (see
  "Def references in local fields"). A field type naming a def that does not
  exist is reported on the node as a warning (`record_node_problems`).

### Promotion

```rust
/// Exact-shape matches for an anonymous `record_construct`: every def whose
/// fields are the same name set with equal types and no `Optional`. Sorted by
/// name. Includes linked defs, not built-in ones. Empty for a node with no
/// fields, which promotion refuses too (otherwise every empty def would be
/// listed for every fresh node).
pub fn matching_record_defs_for(&self, scope_path: &[u64], node_id: u64) -> Vec<String>;

/// Create def `def_name` from the node's anonymous fields and point the node at it.
pub fn promote_record_construct(
    &mut self, scope_path: &[u64], node_id: u64, def_name: &str,
) -> Result<(), String>;
```

`promote_record_construct`:

1. Refuse when the node is not in anonymous mode, has no fields, or the
   network is read-only.
2. Validate `def_name`: `identifier::is_valid_user_name`, not under (or
   above) a linked mount (`library_links::name_conflicts_with_mount`, checked
   first for a clearer message), and not taken (`name_is_taken`, which also
   covers built-ins). All checks run before anything changes.
3. Snapshot the network. Build the `RecordTypeDef`, `claim_field_ids`, and
   add it through the **non-pushing** registry call.
4. Set `schema = def_name` and clear `fields`; keep `literal_values` (all of
   them still name a field) and `next_field_id` (it never decreases). Re-derive
   the node type: the pins now carry `FieldId`s, none of which can equal a
   `BASE | id` pin id, so every wire matches by name. Reflow neighbours if the
   node grew: a named node shows its schema as a subtitle, an anonymous one
   does not.
5. Revalidate, mark the design dirty and request a full refresh. Snapshot the
   network again, then push
   `CompositeCommand [AddRecordTypeDefCommand, NetworkSnapshotCommand]`
   described as "Name record". Undo restores the network, then removes the
   def; redo adds the def, then restores the promoted network.

"Use existing def" is the schema setter, the dropdown's own path, which now
records a `NetworkSnapshotCommand` (see "Undo of a schema change"). It needs
no command of its own.

## Flutter

- **`NodeView` / API:** `has_open_input_port: bool` per node, which is
  `declared_inputs().is_some()`.
- **Open-port tab layer:** while `model.draggedWire` is non-null, the canvas
  shows one extra layer above the nodes. It holds a `+` tab for each visible
  node with `has_open_input_port`, placed just below the node's bottom edge
  at the input-pin column. The layer must live in the canvas's own stack,
  not inside the node widget: Flutter does not hit-test a child outside its
  parent's bounds, so a tab hanging out of the node widget would never
  receive the drop. The layer rebuilds only when `draggedWire` changes
  between null and non-null: twice per drag, never per tick, which keeps
  "drag ticks never notify" intact (`project_canvas_drag_performance`). Tab
  positions come from the node positions and sizes the canvas already has.
- **Each tab** is a `DragTarget<PinReference>`. Its `onWillAcceptWithDetails`
  applies the scope-prefix and zone-input rules of `canConnectPins`, then
  calls `can_add_input_from_wire`, keeping the refusal reason in widget
  state. `onAcceptWithDetails` calls `model.addInputFromWire(...)`. The
  builder draws the accepted highlight from `candidateData`, and the refused
  style plus the reason label from `rejectedData`. Both are per-target and
  need no `notifyListeners`. `addInputFromWire` clears `draggedWire` the way
  `connectPins` does. Otherwise the source pin's `onDragEnd` would also
  treat the drop as an empty-space drop and open the add-node popup.
- **Node height:** unchanged. The tab is outside the footprint, so neither
  `getNodeSize()` nor the Rust estimators that mirror it count it. Only a
  real appended input grows the node, as any new pin does, and the drop's
  reflow handles that.
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
- **Take type from wire, in the panel:** both editors fetch
  `getWireTypeOffers(scopePath, nodeId)` alongside their data and index it by
  row. A row with an offer gets a warning marker and, under its type
  selector, a **Use wire's type: `<type>`** button. The type is printed with
  the same formatter as the type selector, so `{b: Float}` reads as the
  wire's tooltip does. The button calls
  `model.takeTypesFromWires(nodeId, [index], scopeChain:
  propertyEditorScopeChain)`. The offers are re-fetched on every rebuild of
  the panel. They are cheap (one type resolution per wired input), and a
  cached copy would go stale exactly when upstream changes, which is the
  case they exist for.
- **Take type from wire, in the node context menu:** `_handleContextMenu`
  computes the offers before `showMenu`, as it does for the other gated
  items, and adds **Take types from wires** to the **Edit** section when any
  entry is `Some`. It calls `model.takeTypesFromWires(nodeId, offeredIndices,
  scopeChain: <the clicked node's scope chain>)`, like the menu's other
  node operations, so it acts on the clicked node even when the property
  panel shows another one.

## Phases

Each phase leaves `main` green: `cargo test -j 4` (the `structure_designer_api`,
`integration` and `renderer_api` harnesses run explicitly), `cargo clippy`
with no new warnings, `flutter analyze` at baseline, and `flutter test` (the
Dart unit tests in `test/`, not the smoke test).

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

Text-format test (beside the existing text-format round-trip tests): an
`expr` whose parameter is typed `{a: Float, p: {x: Int}}` survives `query` →
`--replace` as a no-op. The text format does not use `from_string` here. It
reads `{…}` as an object and converts it in `TextValue::to_data_type`, so
this path already works; the test keeps the open port from relying on it
untested. That arm builds `RecordType::Anonymous(fields)` directly, skipping
the canonical sort of `RecordType::anonymous`, so the test also loads a
hand-written `{b: Int, a: Int}` and requires it to equal `{a: Int, b: Int}`.
If it does not, Phase 0 fixes the arm.

API test (`rust/tests/structure_designer_api/`): an `expr` with an
anonymous-typed parameter survives a `set_expr_data` round-trip of its
parameter list. Docs: the reference-guide sentence becomes true; nothing to
change there beyond an example.

### Phase 1 — declared inputs core, the open port and Take type from wire, on `expr`

- `declared_inputs.rs`: the `DeclaredInputs` trait and
  `derive_open_port_name`. The `declared_inputs` / `declared_inputs_mut`
  accessors on `NodeData`.
- `declared_input_ops.rs`: `resolve_wire_source_type`,
  `add_input_from_wire`, `wire_type_offers`, `take_types_from_wires`.
- `NetworkSnapshotCommand` (`undo/commands/network_snapshot.rs`). Implemented by
  renaming the existing `NodeStructureEditCommand` (the `zip_with` / `switch`
  structural edits), which already had exactly this shape.
- `ExprData` implements `DeclaredInputs`, and `accepts_parameter_type` is
  factored out of `adapt_for_drag_source`.
- API: `add_input_from_wire`, `can_add_input_from_wire`,
  `get_wire_type_offers`, `take_types_from_wires`, and
  `has_open_input_port` on the node view. Run `cargo fmt`, then FRB codegen.

Tests (`tests/structure_designer/declared_inputs_test.rs` and
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
- undo: the drop is one step, checked with `assert_undo_redo_roundtrip`
  (`undo_test.rs`). It compares every network, serialized, before, after
  undo, and after redo, so it catches stray changes anywhere, not only on
  the edited node. This covers the top-level case and the capture-wire case;
- refresh after undo/redo: `NetworkSnapshotCommand`'s `Full` refresh leaves
  derived state current. After undoing a drop, the `expr`'s `output_type`
  and the network's validation errors equal a freshly validated copy. When
  the edited network is used as a custom node in another network, and the
  `expr` feeds its return node, the instance's output type in the parent
  follows the undo and the redo;
- `NetworkSnapshotCommand` itself, in `undo_test.rs`, as the undo
  `AGENTS.md` asks for a new command: undo/redo restore the snapshot exactly,
  including an edit inside a zone body;
- a later rename through `set_expr_data` keeps the wire (id preserved);
- offers: an `expr` parameter declared `Float` and fed by a `Vec3` output is
  offered `Vec3`. One fed by `Int` is **not** offered (compatible). An
  unwired parameter is not offered. A source that resolves to a `Function`
  is not offered (refused by `accepts_type`). The offers agree with
  `validate_wires` over a small table of source/declared pairs, which pins
  `resolve_wire_source_type` to the validator. For a non-record pair, a pin
  is offered exactly when it carries a mismatch error and `accepts_type`
  passes. For a record pair, it is offered exactly when it carries a
  mismatch error **or** the resolved fields differ, and `accepts_type`
  passes;
- record width (sources are `expr` record literals): a parameter declared
  `{a: Float}` fed by `{a: Float, b: Float}` carries no error and **is**
  offered `{a: Float, b: Float}`. Taking it lets `p.b` validate. A named
  source whose fields equal the declared anonymous type is **not** offered.
  A named source wider than the declared anonymous type is offered in its
  anonymous form, never as the name;
- offers through a polymorphic chain (`SameAsInput`): the offered type is
  the resolved concrete type;
- `take_types_from_wires` retypes the parameter, keeps its id, name and
  wire, leaves the expression text unchanged, and clears the mismatch error.
  The `expr`'s output type follows (e.g. `x * 2` changes from `Float` to
  `Vec3`), and so does the evaluated value: re-evaluating after the retype
  yields a `Vec3`, so no memoized result outlives the type change. Two stale
  inputs taken together give one undo step (`assert_undo_redo_roundtrip`);
- refusals: an index with no offer, an empty list, read-only network. Each
  leaves the network unchanged and pushes no undo step. A list that mixes an
  offered and a non-offered index changes nothing.

API tests (`rust/tests/structure_designer_api/`). These pin what the Flutter
side relies on but cannot check itself:

- **the hover check agrees with the drop.** Over one table of sources
  (accepted, abstract, `Function`, `None`-typed, read-only network,
  destination without a port, capture across a zone), `can_add_input_from_wire`
  returns `None` exactly when `add_input_from_wire` succeeds. A tab that
  lights up green must never refuse the drop, and a refused one must never
  have accepted it;
- `AddedInput.pin_index` is the new parameter's row in `get_expr_data`.
  `pendingNameFocus` keys on it;
- `get_wire_type_offers` returns one entry per input pin, in the editor's
  row order;
- `has_open_input_port` is `true` on `expr` and `false` on a sample of other
  nodes, including a custom-node instance and `record_construct`. Phase 2
  extends this: `true` on an anonymous `record_construct`, `false` in named
  mode.

Docs: `crates/atomcad-structure-designer/src/nodes/AGENTS.md` gets a short
"Declared inputs" note. A node whose input list is its own data implements
`DeclaredInputs` and returns it from `declared_inputs()`, and that is all it
takes to get the open port and Take type from wire; no orchestrator names a
node type. Pins are addressed by id. No user-visible change yet, so no
reference-guide change.

### Phase 2 — anonymous `record_construct`

- Data model, `ANON_FIELD_PIN_ID_BASE`, anonymous node-type branch,
  factored eval, text-format `fields`, the `DeclaredInputs` impl (`Some`
  only in anonymous mode, `retype` dropping the field's literal), the
  `FromInput` anonymous adaptation, rename moving literal keys, and a mode
  switch dropping the literals that do not carry over.
- The schema setter as a `StructureDesigner` method recording a
  `NetworkSnapshotCommand` ("Set record schema"), replacing the
  `set_node_network_data_scoped` path of `set_record_construct_data`.
- The local field types in `collect_record_refs_in_node`,
  `rewrite_record_names_in_node` and `canonicalize_node_data`, through the
  `NodeData::local_record_fields[_mut]` hook (a downcast is forbidden by
  `record_node_hook_test.rs`). The schema setter and the field-list setter
  live in `record_construct_ops.rs`, which that test lists as the node's own.
- Invariant checker (`invariants.rs`): extend `DuplicateFieldId` and
  `FieldIdFloor` to a `record_construct`'s local `fields` / `next_field_id`,
  and add a Tier 1 check that a named-mode node has empty `fields`.
  `validate_network` debug-asserts these on every edit, so every test in
  this doc exercises them for free, and `check_document_invariants` covers
  save → load.
- Correct the doc comment on `build_node_type_for_schema`. It says mismatched
  downstream wires "get disconnected by network validation", but
  `validate_wires` only reports them, and this design relies on that.
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
- the drag-aware-add sequence Phase 4 wires up, at API level: add a
  zero-field `record_construct` from a `FromOutput` drag, then
  `add_input_from_wire` from the dragged source. The result is a one-field
  anonymous record whose field has the derived name and is wired. Flutter
  only picks this call instead of a pin index, so the behavior is tested
  here;
- `set_record_construct_fields`: rename keeps the wire (pin id) and moves
  the literal key; reorder keeps every wire on its field; delete drops the
  field's wire and literal; add takes the next id from `next_field_id`,
  never reusing a deleted field's id. Each is one undo step
  (`assert_undo_redo_roundtrip`);
- invariants: a duplicated local field id, a `next_field_id` at or below the
  largest local id, and a named-mode node with non-empty `fields` are each
  reported (`invariants_test.rs`);
- `declared_inputs()` is `Some` in anonymous mode and `None` in named mode,
  and `has_open_input_port` follows it through the API;
  a named-mode node has no open port and no offers, even with a mismatched
  wire on a field;
- **the rename scenario, end to end:** anonymous node `r` with field `a`,
  its output dropped on an `expr` `+` (parameter declared `{a: Float}`,
  expression `p.a`). Rename `a` to `b`. `r`'s input wire is kept. The
  `expr`'s wire is kept and carries a mismatch error. `wire_type_offers`
  offers `{b: Float}`. Take it: the mismatch clears, and the expression's
  `p.a` now reports its own error. Edit the expression to `p.b`: it
  evaluates;
- **the add-a-field scenario, end to end:** anonymous node `r` with field
  `a`, its output dropped on an `expr` `+` (parameter declared
  `{a: Float}`). Drop another output on `r`'s `+`, giving field `b`. The
  `expr`'s wire carries **no** error, and `wire_type_offers` still offers
  `{a: Float, b: Float}`. Take it: `p.b` in the expression validates and
  evaluates;
- `take_types_from_wires` on an anonymous field: the field is retyped, its
  pin id and wire are kept, and its literal is dropped. Undo restores the
  type and the literal;
- mode switch: an anonymous node with fields `a` and `b` (wired, `a` also
  with a literal) picks def `Foo { b, c }`. `b`'s wire is kept, `a`'s wire
  and literal are dropped, and no wire lands on `c`. Run this a second time
  with local ids chosen to collide numerically with `Foo`'s `FieldId`s,
  which is the case the id base exists for. The switch passes
  `assert_undo_redo_roundtrip`: undo restores `a`, its literal and both
  wires in one step (the assertion that fails under `SetNodeDataCommand`),
  and redo drops them again. A named → named switch that drops a wire
  passes it too;
- def references in local fields: an anonymous field typed `Box` (from
  dropping a `Box` output on the `+`). Renaming def `Box` to `Crate`
  rewrites the field to `Crate`, and its wire stays valid. The delete
  reference check for `Box` finds the node. `collect_record_refs_in_network`
  lists `Box`. If `Box` goes missing anyway (a hand-edited file), the
  invariant checker's `UnresolvedRecordName` is accounted for by a
  validation error, not silent. An empty `schema` is never reported as a reference. With a
  field typed by a linked library's def, removing that def from the library
  and refreshing freezes the node, and its wires survive refresh and
  reopen (`library_links` tests);
- legacy: a saved `record_construct` with an empty `schema` loads as an
  anonymous node with no fields, outputting `{}`;
- text format: `query` → `--replace` of an anonymous node is a no-op
  (round-trip corpus rule), including a field whose type is itself an
  anonymous record (Phase 0's parser); `fields` + `a: x` builds the node
  from text;
- `.cnnd` round-trip with a new fixture holding an anonymous node. Build its
  source values with `add_value_node` / `add_typed_value_node`. The new
  fixture adds two corpus snapshots, to be accepted with `cargo insta review`.
  A named-mode file re-serializes byte-identically;
- drag adapter: `FromInput` from `{a: Float}` adapts to anonymous mode with
  field `a`; the existing named-mode cases are unchanged.

Docs: the `record_construct` section of
`reference_guide/nodes/math_programming.md` (anonymous mode and its text
form; the `+` tab is documented in Phase 4, where it appears); the records
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
  step; redo reinstates both. `assert_undo_redo_roundtrip` compares networks
  only, so the test also asserts the def registry: no `Box` after undo, and
  `Box` with the same `FieldId`s after redo;
- refusals: name taken, linked namespace, not anonymous, no fields. Each
  leaves no def and no undo step;
- matching: an exact def is listed; a def with an extra field, a missing
  field, a different type or an `Optional` field is not; linked defs are
  listed;
- "use existing": pins reorder to the def's authored order and wires follow
  by name.

### Phase 4 — Flutter: the `+` tab, dropping, and the panels

- The open-port tab layer (shown while a wire is dragged), each tab's
  `DragTarget`, hover check, refused style and reason label, clearing
  `draggedWire` on a tab drop, and `pendingNameFocus`.
- Drag-aware add: auto-connect to a newly created `record_construct` (zero
  fields, from a `FromOutput` drag) goes through `addInputFromWire` instead
  of a pin index.
  *Implementation note:* this also needed a backend change the phases above
  missed. The add-node popup's filter (`get_compatible_node_types`) offers a
  node only when a pin of its default type, or its drag adapter, matches the
  source, and a fresh `record_construct` has neither for a `FromOutput` drag
  — so it was never listed. The filter now also offers, for an output drag,
  any node whose default data has declared inputs that `accepts_type` the
  source. The old test that `record_construct` never surfaces from an output
  was updated to the new rule.
- `expr` editor: consume `pendingNameFocus`.
- `record_construct` editor: anonymous field list, Add field, Name this
  record… dialog with the "Same shape as" list, and the context-menu entry.
- Take type from wire: the per-row marker and **Use wire's type** button in
  both editors, the **Take types from wires** context-menu item, and
  `model.takeTypesFromWires`.

Tests. The tab layer's placement and eligibility depend only on data, so
they live in a pure function, roughly `openPortTabs(nodes, scopeChain, zoom)
→ [(nodeId, Rect)]`. It gets a unit test in `test/open_port_tabs_test.dart`,
following `test/mechanosynth_popup_layout_test.dart`; agents run these with
`flutter test`:

- only nodes with `hasOpenInputPort` in the visible scope get a tab, and a
  node inside a collapsed HOF body gets none;
- each tab rect sits directly below its node's bottom edge, at the
  input-pin column, and scales with the zoom level;
- the tab does not change the node: `getNodeSize()` returns the same size
  for a node whether `hasOpenInputPort` is true or false.

Hit-test priority, drag feedback and panel focus need a running app, so they
are checked by a manual walkthrough (`feedback_manual_test_for_editor_ui`):

1. Drop three outputs on an `expr` `+`: three parameters with derived names,
   name field focused each time; rename one; the wire stays.
2. Hover an abstract-typed or a function output over an `expr` `+`: the tab
   shows the refused style and the reason, and the drop does nothing.
3. Drag an output into empty space → `record_construct`: one-field anonymous
   record. Drop two more on its `+`. Wire its output into a new `expr`
   (`r.<field>` works).
4. Name this record… → `Box`: the def appears in the user-types panel; no
   wire moved; Ctrl+Z undoes it in one step, and Ctrl+Y redoes it.
5. A second anonymous node with the same shape: the dialog offers `Box`
   under "Same shape as"; Use → pins reorder, wires stay.
6. Inside a `map` body, drop a captured outer value on an `expr` `+`.
7. Open an existing file with tightly stacked `expr` nodes: nothing moved,
   nothing overlaps. Start a wire drag: `+` tabs appear under every `expr`
   and anonymous `record_construct` in view, and none under other nodes.
   They disappear on a drop or a cancel. Dropping on a tab adds the input
   without opening the add-node popup. A tab covering the top of the
   node beneath it still takes the drop.
8. Take type from wire: in the network from step 3, rename a field of the
   `record_construct`. The downstream `expr` turns red. Select it: the
   stale parameter row shows the marker and **Use wire's type: `{…}`**.
   Click it: the error moves into the expression text; fix the member
   access and the node evaluates. Ctrl+Z restores the old type. Make two
   inputs stale and use **Take types from wires** from the node's context
   menu: both are fixed, and one Ctrl+Z undoes both. On a healthy node the
   menu item is absent. Then drop one more output on the
   `record_construct`'s `+`: the downstream `expr` stays error-free, but its
   parameter row shows the marker and offers the wider type. Take it, and
   `r.<new field>` works.
9. Schema switch: on an anonymous node with two wired fields, pick a def
   that shares only one of them; one wire drops. Ctrl+Z brings back both
   wires and the local fields.
10. Flutter smoke test (`flutter test integration_test/`): **human-only**, run
   by the maintainer.

Docs: `reference_guide/node_networks.md` gets a short "open port" paragraph
(where it appears, what a drop does, renaming after); the `expr` and
`record_construct` sections mention the `+` tab, Name this record… and Take
type from wire, with the rename and add-a-field cases as its examples; and
`lib/structure_designer/AGENTS.md` notes that the open-port tabs live in a
canvas layer outside the node footprints, and why: hit testing, and saved
layouts that must not move.

### Phase 5 — `expr` starts empty; one naming rule; wires declare in text

Added after Phases 0–4 were in use. With the open port, a fresh `expr`'s
default parameter (`x: Int`, body `x`) was mostly in the way: `Int` was
usually the wrong type, and the first input was wired differently from every
later one.

- **A fresh `expr` has no parameters and the body `0`** (`node_data_creator`).
  There is one default, used by the GUI and the text format alike. The body is
  a constant so the new node evaluates and shows no error.
- **The text format: a wire declares a missing input.** `y: src` on a node with
  declared inputs that has no input `y` appends one named `y`, of `src`'s
  resolved type, then wires it (`network_editor.rs`,
  `declare_input_from_wire`). It works for both nodes in this document,
  through the trait. The rules:
  - Only a single reference declares. A literal carries no type, and an array
    is several wires.
  - **Explicit wins.** A statement that sets the node's
    `DeclaredInputs::declaring_property` (`parameters` / `fields`) declares
    nothing more; a wire to another name stays the warning it was.
  - The name is the key as written: a non-identifier key (`@0`) is not a
    declaration attempt, and a reserved one is refused with its reason.
  - Declarations happen only in the retry pass (`wire_deferred_connections`),
    after every ordinary wire is made, so a polymorphic source resolves
    through its own inputs. The pass repeats while a round makes progress,
    which resolves chains of `expr`s written in any order.

  This keeps the old spelling `expr { expression: "x * 2", x: val }` working,
  now with `val`'s type rather than `Int`, and it resolves
  `doc/research/expr-parameter-auto-inference.md`.
- **One naming rule for a new input.** `DeclaredInputs::name_pool` (default
  empty) and `declared_inputs::next_declared_input_name`:
  - A node with a pool takes the first pool name that is neither taken nor
    reserved, then `x_2`, `y_2`, … once the pool runs out (`next_pool_name`).
    `expr`'s pool is `x, y, z, a, b, …, w`: an `expr` parameter is a variable
    in a formula, so a letter reads better than `sphere1`.
  - A node without a pool (`record_construct`, whose field names are data) is
    named after the source, as before.
  - Every path goes through it: the open port, `expr`'s drag adapter (which
    still gives `x` with identity body `x`) and the panel's **Add Parameter**
    (`StructureDesigner::next_declared_input_name`, API
    `next_declared_input_name`), replacing a Dart-side letter list.
  - The text format is the exception by design: there the name is what was
    written.
- **The source's name stays one click away.** `AddedInput::source_name` is
  the source-derived name when it differs from the one given. The panel shows
  it as a **Name after source** chip under the focused name field
  (`NameSuggestionChip`, `model.nameSuggestion`); the chip is dropped once
  another node is shown, the name is typed, or it is used. A user setting and
  a two-zone tab (drop on the left for a letter, on the right for the source's
  name) were considered and rejected: a hidden switch that changes what a
  gesture does, and a target too small to split.
- **The tab previews the name.** `preview_add_input_from_wire` runs the drop's
  checks and returns the name (or the refusal), and an accepted hover draws
  the name beside the tab. `can_add_input_from_wire` remains, as its `err()`.
- `pi` (the expression language's constant) joins `expr`'s reserved names.

Tests: `tests/structure_designer/wire_declared_text_test.rs` (the fresh node,
the text-format declaration and each of its rules, the chain, an incremental
append, `record_construct`, a `query` → `--replace` no-op, the add-button
name, the hover preview) and the pool table in `open_port_name_test.rs`. The
open-port tests now expect letters on `expr` and keep the source-naming cases
on `record_construct`.

Manual walkthrough:

1. Drop a fresh `expr`: no input pins, it shows `0`, no error.
2. Drag a wire over its `+` tab: the label beside it reads `x`. Drop: parameter
   `x` of the wire's type, name focused, and a **Name after source: `<node>`**
   chip under it. Click the chip: the parameter is renamed.
3. Drop a second wire: `y`. Rename `x` to `radius` and drop a third: `x` again.
4. **Add Parameter** names the next free letter.
5. Drag an output into empty space and pick `expr`: `x` of that type, body `x`.
6. Drop on an anonymous `record_construct`'s `+`: the label shows the source's
   name, and no chip appears.
7. Select another node and come back: the chip is gone.

## Testing

Behavior is tested at the layer that owns it:

- **Pure functions:** name derivation (Rust), and the open-port tab
  placement (Dart, `test/`, which agents run with `flutter test`).
- **Orchestrators and `DeclaredInputs` impls:** through `StructureDesigner`
  in the crate's `tests/structure_designer/`. Each new test file is
  registered as a `mod` in `tests/structure_designer.rs`, or it silently
  never runs.
- **Undo:** every undoable operation in this doc goes through
  `assert_undo_redo_roundtrip`, which compares every network, serialized,
  rather than the fields a test thought to check. Promotion adds a registry
  assertion, because the helper does not compare defs.
- **The FFI surface:** in `rust/tests/structure_designer_api/`, where the
  hover check (`can_add_input_from_wire`) is pinned to the drop, and indices
  to editor rows.
- **The invariant checker:** extended to local fields in Phase 2.
  `validate_network` debug-asserts it after every edit, so stale but
  still-wired inputs are checked throughout the suite: each one must be
  accounted for by a validation error, never silent.
- **Persistence:** the existing `.cnnd` round-trip corpus, `check_document_invariants`
  across save → load, and the text-format `query` → `--replace` no-op rule,
  including anonymous record types. Phase 0 covers both the type-string
  parser the panel and API use and the text format's own path.
- **Snapshots:** no snapshot records the `record_construct` node type, so
  the rewritten description changes none. The Phase 2 fixture adds one entry
  to the integration harness's validation-corpus snapshot.

Every phase runs the gate listed under "Phases". The `structure_designer_api`,
`integration` and `renderer_api` harnesses are named explicitly because
`--workspace` stops at the first failing harness. Phase 4 is the first phase
to add Dart unit tests to `flutter test`. Only drag feedback, hit-test
priority and panel focus are left to the Phase 4 walkthrough, along with the
Flutter smoke test, which is human-only.

## Future work

- **Drag from `+`** to a source output: the same operation started from the
  other end.
- **A wire context menu**, with Take type from wire on a mismatched wire into
  a declared input. The canvas has no wire menu today: a right-click reaches
  the background or a node menu. When it gets one, the item calls the same
  `take_types_from_wires` with one index. No new backend is needed.
- **Rename following into consumers:** retyping downstream inputs
  automatically when an anonymous field is renamed. This would make declared
  types live, which the "declared type" decision rules out. Promotion gives
  cascading renames through `FieldId`s instead.
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
  updates. Designed in `doc/design_anonymous_record_readers.md`, together
  with named-record field access in `expr`.
