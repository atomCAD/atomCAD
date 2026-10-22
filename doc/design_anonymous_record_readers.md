# Anonymous records on the record readers: `record_destructure`, `record_get`, `record_set` — and named fields in `expr`

## Status: Design — not implemented (2026-10-22)

## Motivation

`doc/design_wire_declared_inputs.md` made anonymous records easy to **build**.
An anonymous `record_construct` grows a field per wire dropped on its `+`
tab, and `expr` has always produced anonymous records (`{a: x, b: y}`). But
they are hard to **take apart**. Of the nodes that read a record, only `expr`
accepts an anonymous one:

| Reader | Named record | Anonymous record |
|---|---|---|
| `record_destructure` | yes (`schema`) | **no** |
| `record_get` | yes (`schema`) | **no** |
| `record_set` | yes (`schema`) | **no** |
| `expr` (`r.x`) | **no** | yes |

The three record nodes take their pins from a named def (`schema`). Their drag
adapters (`adapt_for_drag_source`) return `None` for an anonymous source,
because there is no def to name. Dragging an `expr` output into empty space
and picking `record_destructure` therefore gives a node whose input is
`Record(Named(""))`: dangling, and refusing every wire. The wire-declared-
inputs doc left this out of scope and named promotion ("Name this record…")
as the way through. Promotion is a fine way to **commit** to a type. It is a
poor price for **looking inside** a record you just made.

`expr` has the opposite gap. `r.x` on a `Record(Named(Foo))` is a validation
error (`expr/expr.rs`, `Expr::MemberAccess`: "record name resolution requires
a registry"). So promoting a record takes a capability away from every `expr`
reading it. That is why the wire-declared-inputs doc needed its "an anonymous
declared input stays anonymous" exception.

This document closes both gaps:

1. **`record_destructure`** gets an anonymous mode. Its type is taken from the
   wire: by drag-placing it from an anonymous output, or by wiring a record
   into an unset node. The user never edits the field list by hand.
2. **`record_get` / `record_set`** get the same anonymous mode. Their path rows
   start from an anonymous root type, which the path resolver can already
   walk.
3. **`expr`** resolves field access on named records through the record defs.

Parts 1 and 2 share one mechanism. Part 3 is independent of them.

### Scope

In scope:

- a capability trait, `RecordTypedInput`, for "a node whose pins are derived
  from the record type of its `record` input". `record_destructure`,
  `record_get` and `record_set` implement it;
- an **unset** state whose input is `{}` and accepts any record;
- **adopting** the record type of a wire: on drag-place, on the first wire
  into an unset or uncommitted node, and on demand through **Take type from
  wire**;
- remapping output wires across a type change, by pin id first and then by
  field name;
- turning `record_destructure`'s schema change into an undoable network
  snapshot, which keeps its output wires;
- named-record field access in `expr`.

**Out of scope:**

- **Live typing.** The adopted type is a snapshot, exactly as in
  `doc/design_wire_declared_inputs.md` ("The declared type"). When the
  upstream shape changes, the wire goes stale and **Take type from wire**
  repairs it. Pins never follow the wire on their own.
- **`product`.** It is a record node too, but its record type is its
  **output**: there is no record wire to take a type from. Anonymous
  `product` would need a field editor, so it is a different design.
- **Editing the field list** of an anonymous reader. The fields always
  describe the input type. Changing them by hand would only make the input
  disagree with its wire.
- **Promotion from a reader.** "Name this record…" stays on
  `record_construct`. A reader in anonymous mode can switch to a def of the
  same shape from its schema dropdown.
- **The custom-node-type cache refactors** a defs-aware `expr` might suggest
  beyond the one hook Part 3 adds.

## Resolved decisions

| Question | Decision |
|---|---|
| Which nodes | `record_destructure`, `record_get`, `record_set`: the three record nodes that **read** a record through an input pin named `record` at pin 0. `record_construct` is the constructor and already has its anonymous mode. `product` is out of scope (see Scope). |
| Modes | Each reader is in one of three modes, decided by its data. **Named**: `schema` is set, and `fields` must be empty; this is today's behaviour. **Anonymous**: `schema` is empty and `fields` is non-empty. **Unset**: both are empty. This mirrors `record_construct`, where an empty `schema` is anonymous mode. The difference is that a reader with no fields has nothing to read, so it is the unset state, not the empty record. |
| Storage | The reader's data gains `fields: Vec<LocalRecordField>` and `next_field_id: u64`. These are the same type and the same allocator rules as `record_construct`'s. `LocalRecordField` and its id allocation move out of `nodes/record_construct.rs` into a shared module (`nodes/local_record_fields.rs`), since four nodes now use them. Because the existing `NodeData::local_record_fields` / `local_record_fields_mut` hook carries the fields, the record-def reference walkers (`collect_record_refs_in_node`, `rewrite_record_names_in_node`, `canonicalize_node_data`), frozen-node detection and the invariant checker (`check_local_record_fields`, including `NamedRecordWithLocalFields`) cover the readers with **no change**. A local field typed `Record(Named("Box"))` therefore renames, freezes and blocks deletion like any other reference. |
| The input type | Named: `Record(Named(schema))`, unchanged. Anonymous: `Record(RecordType::anonymous(fields))`. Unset: `Record(Anonymous([]))`, i.e. `{}`. Records have width subtyping (`can_be_converted_to_impl`, `data_type.rs`), so **every resolvable record converts to `{}`**, named or anonymous. An unset reader therefore accepts any record wire with no new abstract type. Today the unset input is `Record(Named(""))`, which accepts nothing. The change is safe for existing files: an unset reader could never have been wired. |
| Output pins | Named: unchanged (one pin per def field, carrying its `FieldId`; `record_get`'s rows carry row ids). Anonymous `record_destructure`: one pin per local field in authored order, carrying `ANON_FIELD_PIN_ID_BASE \| field.id`, the same id space `record_construct` uses for its input pins. Unset: the `result: None` placeholder, as today. |
| Field order of an adopted anonymous type | `RecordType::anonymous` sorts fields canonically, so an anonymous **type** has no authored order. The pins of an adopted anonymous destructure therefore come out in canonical (alphabetical) order. One exception: when the source is an anonymous `record_construct` and the wire is direct, its authored order is used, read through `local_record_fields` on the source node, so `{b, a}` built in that order destructures as `b, a`. Any other source gives alphabetical order. Pin order is cosmetic: wires are kept by id. |
| The capability | A trait, `RecordTypedInput`, handed out through `NodeData::record_typed_input()` / `record_typed_input_mut()` (default `None`). It has three methods: `record_input_type(&self) -> Option<RecordType>` (`None` when unset), `set_record_input_type(&mut self, t: &RecordType, defs: RecordDefs) -> Result<(), String>`, and `is_record_input_pin(&self, index: usize) -> bool` (pin 0 on all three). This follows `DeclaredInputs` exactly: the orchestrators in a new `record_input_type_ops.rs` never name a node type, and `record_node_hook_test.rs` keeps forbidding downcasts outside each node's own files. It is a separate trait rather than part of `DeclaredInputs` because the readers have no open port, and `declared_inputs().is_some()` is the open port's gate. |
| What adopting a type does | `set_record_input_type` with a **named** type: `schema = name`, `fields` cleared (named mode). With an **anonymous** type: `schema` cleared, and `fields` rebuilt from the type's fields. A field keeps its local id when a current field has the **same name**; otherwise it gets a fresh id. There is no positional fallback: unlike a rename in `record_construct`'s panel, a field that disappears upstream while another appears is not the same field. `record_get` / `record_set` additionally re-resolve their rows against the new root by current names. That is what a schema change does today (`merge_rows`); a row whose path no longer exists becomes a dangling row with its usual warning. Adopting `{}` (an empty record source) leaves the node unset. A field whose type the node cannot hold (`RecordConstructData::accepts_field_type`) refuses the whole adoption, and the reason is reported. |
| Named source → named mode | When the wire carries `Record(Named(Box))`, the reader adopts **`Box`**, not its anonymous shape. Named mode gives `FieldId` identity, so a later field rename in `Box` follows into the reader's pins and wires. Unlike `expr` before Part 3, the readers have no trouble with named records. So the wire-declared-inputs rule "an anonymous declared input stays anonymous" does **not** apply here: the offered type is always the source's own type. |
| When a type is adopted | (1) **Drag-place**: `adapt_for_drag_source` returns a reader already set to the source's type, named or anonymous, through the same `set_record_input_type`. (2) **Wiring an uncommitted reader** (next row). (3) **Take type from wire**, on a stale input. Nothing else changes the type. In particular, upstream edits never do. |
| Uncommitted readers adopt on connect | A reader is **uncommitted** when no wire touches it except the one into `record`: no wire out of any output pin, including the function pin `-1`, and for `record_set`, no wire into a row pin. Connecting a record wire into an uncommitted reader's `record` pin adopts the wire's type in the same step. The connect gate (`NodeNetwork::can_connect_nodes` and its scoped and Flutter-hover callers) accepts **any resolvable record** on that pin while the node is uncommitted, so a reader can be pointed at a different record until something depends on its outputs. An unset reader is always uncommitted. This covers both of the user's cases: a reader dropped on its own takes the first record wired into it, and a reader whose outputs are not used yet can be repurposed by wiring a different record into it. A **committed** reader keeps its type: a wire that does not convert is refused as today, and a convertible but different one is accepted and becomes stale (Take type from wire). Two alternatives were rejected. *Adopting only on an unset node* would make repurposing mean "pick none from the dropdown first", a step that only exists to work around the rule. *Always re-adopting on connect* would silently drop the downstream wires of fields the new record lacks. |
| Take type from wire | The existing operation, extended. `StructureDesigner::wire_type_offers` also offers pin 0 of a node with `record_typed_input`, when exactly one wire is on it and the source's resolved type is stale against the input type. Stale has the same meaning as for declared inputs (`offered_type` / `records_differ` in `declared_input_ops.rs`, factored into a shared `is_stale_record_wire` predicate): the source fails to convert, or both are records whose resolved, canonically sorted fields differ. A **wider** source is therefore stale. An anonymous source that gained field `c` offers a destructure the new pin, and nothing else would surface it, because the wire stays valid under width subtyping. `take_types_from_wires` dispatches by hook: a declared-input pin is retyped as today, and the record input goes through the adoption path below. The Flutter side reads `wire_type_offers` per pin, so the node context menu's **Take types from wires** item works with no Dart change. |
| Output wires across a type change | An adoption can change, add and remove output pins. The R3 remap in `repair_node_network` keys wires by pin id, but it runs only during repair and captures the "before" layout from the cached `custom_node_type`. An adoption that re-derived the node type first would capture the after layout. So the adoption path captures the reader's old output pins `(id, name)` **before** installing the new data, re-derives the node type, and then calls a remap helper factored out of the R3 block (`remap_record_output_wires(network, node_id, old_pins)`). The helper matches **by id first, then by name**. Ids alone cover an anonymous → anonymous adoption (ids kept by name), a field rename inside one def, and a reorder. The name fallback covers mode switches, where the id spaces differ: a named-mode `FieldId` never equals an anonymous `ANON_FIELD_PIN_ID_BASE \| id`, and `FieldId`s of different defs are distinct counters from the per-file floor. So `Box{a,b}` → `{a,b,c}` keeps the wires on `a` and `b`. A wire whose field is gone is dropped. `repair_node_network`'s own R3 pass stays id-only: on a def edit, a deleted field must not be re-pointed by name at a newly added one. |
| Undo | Every adoption is **one** `NetworkSnapshotCommand`. Drag-place is already covered by the add-node step. **Connect + adopt** replaces the `ConnectWireCommand` with a snapshot labelled "Connect wire" (top level), because `ConnectWireCommand` restores the wire only and its undo would leave the adopted pins behind. Inside a zone body, `connect_nodes_scoped` already records the whole body (`EditZoneBodyCommand`), so the adoption just happens inside it. Take type from wire is labelled "Take type from wire". The **schema dropdown** of `record_destructure` becomes a `StructureDesigner` op, `set_record_destructure_schema`, labelled "Set record schema": snapshot, install, re-derive, remap, reflow, snapshot. Today `set_record_destructure_data` goes through `set_node_network_data_scoped` and records a `SetNodeDataCommand`, which neither remaps output wires nor restores them on undo. The same gap `record_construct` had before Phase 2 of the wire-declared-inputs doc. Phase 1 starts with a test that confirms the current behaviour. |
| Reflow | Adoption changes the output-pin count, and therefore the footprint. The adoption path calls `capture_footprint_chain` / `reflow_for_footprint_change`, as `add_input_from_wire` does, and the moves land inside the after-snapshot. This differs from declared inputs, where "retyping never changes a node's footprint". |
| Picking from the schema dropdown on an anonymous reader | Allowed: switches to named mode. Output wires are kept by name (the remap's name fallback) and the rest are dropped. Picking **none** makes the node unset: fields are cleared, all output wires are dropped, and the input becomes `{}`. The dropdown never lists the anonymous type itself. That type is shown read-only beneath the dropdown (see Flutter). |
| `record_set`'s output | `Record(root)`: `Record(Named(schema))` in named mode, the anonymous root type in anonymous mode. Update only, as today: every path must exist in the root. |
| Text format | `record_destructure`, `record_get` and `record_set` gain a `fields` property, written exactly like `record_construct`'s: `d = record_destructure { fields: [{name: "a", data_type: Float}, {name: "b", data_type: Vec3}], record: r }`. `query` writes `fields` for an anonymous reader and `schema` for a named one, so `query` → `--replace` stays a no-op (the round-trip corpus). **Wires declare in text**, as Phase 5 of the wire-declared-inputs doc does for `expr`: a `record_destructure` statement that sets neither `schema` nor `fields` and wires `record:` to a single reference adopts that source's type in the retry pass (`wire_deferred_connections`), after the source has resolved. An explicit `schema` or `fields` wins. `record_get` / `record_set` with `paths` must state `schema` or `fields`, because the paths are resolved against the root when the statement is applied. A `paths` without a root is an error naming both properties. Deferring path resolution to the retry pass is possible later. |
| `expr`: named field access | `Expr::validate` gains a record-field resolver (`RecordDefs`), and `Expr::MemberAccess` on `Record(Named(n))` resolves `n` through it. An unknown def is an error naming it, and an unknown field is the same error the anonymous case gives. Runtime needs no change: a named record's value is a `NetworkResult::Record` whose fields are read by name today. |
| `expr`: where the defs come from | `ExprData::parse_and_validate` is called from places with no registry: the loader, `set_text_properties`, the drag adapter. A new mutable hook, `NodeData::refresh_with_record_defs(&mut self, defs)` (default no-op), is called by `populate_custom_node_type_cache_with_types` **before** `calculate_custom_node_type`. `expr` implements it by re-validating its cached AST against the defs, which sets `output_type` and `error`. The registry-less call sites still validate, and treat a named member access as **pending** rather than an error. The populate pass then settles it. `repair_node_network` repopulates every node after a def edit, so retyping `Foo.x` from `Float` to `Int` re-derives the type of every `expr` reading `r.x`. A plain `&self` hook was rejected because `build_record_node_type` cannot write the `error` the node displays. |
| `expr`: an `Optional` field | `r.x` on an `Optional[T]` field has type `T` (`DataType::record_field_pin_type`, the same projection `record_destructure`'s pins use). An unset field evaluates to `None`, as on a destructure pin, and flows through the expression the way a `None` parameter does today. |
| `expr`: the "anonymous stays anonymous" exception | Kept. With Part 3 it is no longer needed for correctness, but it remains a reasonable default: an `expr` parameter is a local variable and has no reason to tie itself to a def. Revisiting it is a one-line change in `offered_type`. |

## The shared mechanism: `RecordTypedInput`

```rust
/// A node whose pins are derived from the record type of its `record` input
/// (`doc/design_anonymous_record_readers.md`): `record_destructure`,
/// `record_get`, `record_set`. The type is a snapshot, taken from the wire on
/// drag-place, on connect while the node is uncommitted, and by Take type
/// from wire. Mutating methods edit the data only; the caller re-derives,
/// remaps output wires, reflows, revalidates and records undo
/// (`record_input_type_ops.rs`).
pub trait RecordTypedInput {
    /// `Some(Named(schema))`, `Some(anonymous(fields))`, or `None` when unset.
    fn record_input_type(&self) -> Option<RecordType>;

    /// Adopt `t`: a named type sets `schema` and clears `fields`; an anonymous
    /// one clears `schema` and rebuilds `fields`, keeping ids by name. Path
    /// rows re-resolve by name. `{}` makes the node unset. Errors (nothing
    /// changed) when a field's type is one the node cannot hold.
    fn set_record_input_type(&mut self, t: &RecordType, defs: RecordDefs<'_>)
        -> Result<(), String>;

    /// The `record` pin: index 0 on all three readers.
    fn is_record_input_pin(&self, index: usize) -> bool { index == 0 }
}
```

The operations live in `record_input_type_ops.rs`, which names no node type:

- `adopt_record_input_type(scope_path, node_id, t) -> Result<(), String>`
  captures the old output pins `(id, name)` and the footprint chain, calls
  `set_record_input_type` on a clone of the data, installs it, re-derives
  (`rederive_node_type`), runs `remap_record_output_wires`, and reflows. It
  records **no** undo: each caller below wraps it in its own snapshot.
- `is_uncommitted_record_reader(network, node_id) -> bool`: no wire out of
  the node (scan every argument in the network for a source equal to the
  node), and none into a pin for which `is_record_input_pin` is false.
- The connect hook: after `connect_nodes` / `connect_nodes_scoped` has made
  the wire, if the destination has `record_typed_input`, the pin is the
  record pin, the node was uncommitted before the connect, and the resolved
  source type differs from `record_input_type()`, call
  `adopt_record_input_type` on the source's resolved type. At top level, the
  connect's undo step becomes a snapshot taken before the wire was made.
- `wire_type_offers` / `take_types_from_wires` gain the record-pin branch
  described in "Resolved decisions".

The connect gate change is the one place outside these ops that needs
care. `NodeNetwork::can_connect_nodes` asks whether the source converts to
the pin type. For a record pin on an uncommitted reader it must instead ask
whether the source is a resolvable record. The hover highlighting in Flutter
calls the same predicate through the API, so it follows automatically.

## `record_destructure`

Data:

```rust
pub struct RecordDestructureData {
    #[serde(default)]
    pub schema: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<LocalRecordField>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub next_field_id: u64,
}
```

`build_record_node_type` gets a third branch:

- named mode: unchanged;
- anonymous mode: input `Record(anonymous(fields))`, and one output per field
  with id `ANON_FIELD_PIN_ID_BASE | id`;
- unset: input `{}` and the placeholder output.

Anonymous mode needs no defs and never refuses under `MissingDefs::Refuse`.
`eval` reads fields by name in both modes, so the anonymous branch is the
named branch's loop over `fields` instead of `def.fields`. `get_subtitle`
stays `None` in anonymous mode, as on `record_construct`. `record_node_problems`
reports local field types that name a missing def, as `record_construct`
already does; the check moves into the shared module.

`adapt_for_drag_source` accepts an anonymous source too, and for both kinds
calls `set_record_input_type` on a fresh data value. The order exception
(an anonymous `record_construct` source keeps its authored order) is applied
by the drag-place and connect callers, which can see the source node. The
adapter cannot see it, because it gets a type, not a node. For drag-place,
the create-time plumbing reorders the fields after the node is placed and
wired.

API: `APIRecordDestructureData { schema: String, fields: Vec<APIRecordLocalField> }`
replaces the shared `APIRecordSchemaData` for this node (the fields are
read-only). `set_record_destructure_data` is replaced by
`set_record_destructure_schema(scope_path, node_id, schema)`, which calls
the snapshot op.

## `record_get` and `record_set`

The path machinery already walks anonymous records. `record_path::record_fields`
handles `RecordType::Anonymous`, and a `PathSegment` inside an anonymous type
has `field_id: None` and resolves by name. Only the **root** is hard-wired to
a def: `walk(schema: &str, …)` begins with `defs.lookup(schema)`. The change
is to make the root a type:

- `walk`, `resolve_path`, `resolve_names*`, `path_defs`, `display_names`
  and the helpers in `nodes/record_path_rows.rs` (`row_pins`,
  `dangling_row_problems`, `rows_path_defs`, `merge_rows`, `rebase_rows`,
  `paths_text_value`, `row_spec`, …) take a `&RecordType` root instead of
  `schema: &str`. The root's fields come from `record_fields`, so a root
  segment of an anonymous root has no id and resolves by name, like every
  anonymous segment below it. This is a wide but mechanical change.
- The node supplies the root from its mode: `Named(schema)` or
  `anonymous(fields)`. An unset node has no root and its path picker is empty.
- `DanglingReason::DefMissing` stays the reason for a missing **named** root.
  An anonymous root cannot be missing.

**Row identity on an anonymous root.** Rows below an anonymous root are by
name all the way down to the first named def, and by `FieldId` inside it. A
field renamed upstream in the anonymous part therefore makes the row stale,
not renamed. The wire goes stale, Take type from wire re-adopts, and the row
whose name is gone becomes a dangling row with its warning. That is the same
outcome a schema change has today. It is the cost of not promoting, and the
reference guide says so.

`record_set`'s output type follows the root (see "Resolved decisions"). Its
row literals live on the rows, so they survive an adoption wherever the row
does.

API: `APIRecordPathNodeData` gains `fields: Vec<APIRecordLocalField>`, and
the path-picker tree (`APIRecordFieldNode`) is built from the root type
instead of the schema name.

## `expr`: field access on named records

Today:

```rust
RecordType::Named(name) => Err(format!(
    "Cannot resolve field '{}' on named record `{}` in this context — \
     record name resolution requires a registry. …", member, name)),
```

After:

- `Expr::validate(&self, variables, functions, defs: Option<RecordDefs<'_>>)`.
  With `Some(defs)`, a `MemberAccess` on `Record(Named(n))` looks up `n`,
  finds the field by name, and returns `field.data_type.record_field_pin_type()`.
  With `None`, the registry-less call sites, it returns a **pending** marker
  instead of an error. `parse_and_validate` turns a pending result into "no
  output type yet" without an error message.
- `NodeData::refresh_with_record_defs(&mut self, defs: RecordDefs<'_>)`, a
  default no-op, is called first in `populate_custom_node_type_cache_with_types`.
  `ExprData` re-validates its cached `expr` against `defs`, which sets
  `output_type` and `error`. `motif` and `motif_sub` also call
  `parse_and_validate`, but on their own languages, and are unaffected.
- `expr`'s parameter types already reach the record-def walkers, so a def
  rename already rewrites them, and the repair pass then re-validates against
  the renamed def.

Consequences:

- A promoted record can be read with `r.x`, so promotion no longer costs
  `expr` anything.
- `Take type from wire` on an anonymous `expr` parameter still offers the
  anonymous form of a named source (kept, see "Resolved decisions"). An
  `expr` whose parameter **is** named, for example one declared by dropping a
  `Box` output on its `+`, now just works.
- The error text for a missing field on a named record names the def:
  "Record `Box` has no field 'q'".

## Flutter

- **`record_destructure_editor.dart`.**
  - The `RecordDefDropdown` stays.
  - In anonymous mode, a read-only list of the fields sits under it, under the
    caption **Type from wire**, one `name: Type` line per field.
  - In unset mode, a hint replaces the list: *"Wire a record into `record`, or
    pick a schema."*
  - When `wireTypeOffers` has an entry for pin 0, the shared `WireTypeOffer`
    widget (`wire_type_offer.dart`) shows its **Use wire's type: `<type>`**
    button under the list. This is the widget `expr` and `record_construct`
    already use.
- **`record_get_editor.dart` / `record_set_editor.dart`** get the same three
  pieces above their path rows. `record_path_rows_editor.dart` takes its tree
  from the root-type API.
- **The node context menu** needs no change: **Take types from wires** is
  driven by `wireTypeOffers`.
- **The canvas** needs no change. A reader's pins come from its node type,
  and the connect-gate change reaches the hover highlighting through the
  existing `can_connect` API.

## Phases

Every test named in a phase is written in that phase. Run
`cargo test -j 4 --test structure_designer` (and `--test integration` where
the round-trip corpus is touched), plus `flutter analyze`. The Flutter smoke
test is the maintainer's.

### Phase 1 — the mechanism and `record_destructure` (backend)

1. Confirm the current schema-switch behaviour with a test: switch a wired
   `record_destructure` from `Box{a,b}` to `Foo{b,c}` through today's setter,
   and record what happens to the consumer wire on `b`, and what undo restores.
   Keep the test. It is rewritten to the new expectations in step 6.
2. Move `LocalRecordField` and its allocator to `nodes/local_record_fields.rs`.
   `record_construct` keeps its behaviour, and its tests are untouched.
3. `RecordTypedInput` and the two `NodeData` hooks; `RecordDestructureData`'s
   new fields, three modes, `build_record_node_type` branches and `eval`; the
   `{}` unset input; `local_record_fields` on the reader.
4. `record_input_type_ops.rs`: `adopt_record_input_type`,
   `is_uncommitted_record_reader`, and `remap_record_output_wires`, factored
   out of the R3 block with a name fallback. The R3 call site stays id-only.
5. The drag adapter for anonymous sources. Connect-time adoption at top level
   and in zone bodies, with the snapshot undo. The connect gate for
   uncommitted readers.
6. `set_record_destructure_schema` as a snapshot op, replacing the
   `SetNodeDataCommand` path.
7. Take type from wire: the pin-0 branch in `wire_type_offers` /
   `take_types_from_wires`, and the shared `is_stale_record_wire` predicate.
8. Text format: the `fields` property, `query` output, and wires-declare in
   the retry pass.
9. API: `APIRecordDestructureData` and `set_record_destructure_schema`;
   `cargo fmt`, then `flutter_rust_bridge_codegen generate`.

Tests (`tests/structure_designer/record_destructure_anonymous_test.rs`, plus
the files named):

- Unset input is `{}`. Every named and anonymous record can connect; nothing
  else can.
- Drag-place from an `expr` `{a: x, b: y}` output gives two pins `a`, `b`.
  From an anonymous `record_construct` built `b, a`, the pins come out `b, a`.
  From a `Box` output, named mode.
- Connecting into an unset node adopts the type, and one undo removes the wire
  **and** the pins. Redo restores both.
- Repurposing: an uncommitted anonymous reader rewired from `{a, b}` to
  `{b, c}` re-adopts. A committed one (output `a` wired) refuses a
  non-convertible record and accepts a wider one without re-adopting.
- Take type from wire after the upstream `record_construct` gains `c`: pin `c`
  appears, and the wires on `a` and `b` stay (ids kept by name). After the
  upstream renames `b` → `q`: the wire on `b` is dropped and `q` appears.
- Mode switches keep wires by name: anonymous `{a, b}` → `Box{a, b}` through
  the dropdown; named → anonymous through Take type from wire.
- **None** from the dropdown leaves the node unset, with every output wire
  gone. Undo restores them all.
- The step-1 test, rewritten: `Box` → `Foo` keeps the wire on `b`, drops the
  one on `a`, and undo restores both.
- A local field typed `Record(Named(Box))`: renaming `Box` rewrites it,
  deleting `Box` is blocked by the reference check, and a linked `Box` under
  an unloaded mount freezes the node (extend `record_node_hook_test.rs`'s
  hook coverage and the library-linking freeze tests).
- Invariants: `NamedRecordWithLocalFields` fires for a reader with both.
- Text: `query` → `--replace` no-op for named, anonymous and unset readers;
  `record: src` with no `fields` declares from the wire; explicit `fields`
  wins. Add one anonymous reader to the round-trip corpus. Per
  `new_cnnd_fixture_updates_corpus_snapshots`, that updates two snapshots.

### Phase 2 — `record_destructure` in Flutter, and the guide

- `record_destructure_editor.dart`: mode display, field list, unset hint,
  `WireTypeOffer`.
- `model.setRecordDestructureSchema` replaces `setRecordDestructureData`.
- `doc/reference_guide/nodes/math_programming.md`, `record_destructure`: the
  three modes, adoption on drag-place and on connect while uncommitted, Take
  type from wire, the text form. Remove "anonymous records have no def" from
  the other pages that say it (grep `doc/reference_guide/`).

Manual walkthrough:

1. Drag from an `expr` producing `{a: x, b: y}` into empty space and pick
   `record_destructure`. Two output pins appear, and the panel lists the type
   under **Type from wire**.
2. Place a `record_destructure` from the node list. Its panel shows the unset
   hint. Wire a `Box` into it: named mode, `Box` in the dropdown.
3. With nothing wired out, wire an anonymous record into the same node. It
   switches to that type. Wire one of its outputs, then try a `Box` again:
   refused.
4. Add a field to the upstream anonymous `record_construct`. The panel shows
   **Use wire's type**. Click it: the new pin appears, and the existing wires
   stay. The node context menu offers the same.
5. Pick **none** from the dropdown, then undo. Pins and wires come back.

### Phase 3 — `record_get` and `record_set`

- Generalize the root in `record_path.rs` and `nodes/record_path_rows.rs`.
  Named-root behaviour must stay byte-identical: the existing
  `record_get_test.rs`, `record_set_test.rs` and `record_path_rows_test.rs`
  pass untouched.
- `RecordTypedInput` on both nodes. Their data gains `fields` /
  `next_field_id`. Adoption re-resolves rows by name. `record_set`'s output
  follows the root. Drag adapters for anonymous sources.
- Text: `fields`, and the "paths need a root" error.
- API: `APIRecordPathNodeData.fields`, and the path tree from the root type.
- Flutter: the two editors and `record_path_rows_editor.dart`.
- Reference guide: `record_get`, `record_set`, including the row-identity cost
  of an anonymous root.

Tests (`record_get_anonymous_test.rs`):

- A path `p.x` on an anonymous root `{p: Point, s: Float}`, where `Point` is
  named: the row survives a rename of `Point.x`, by `FieldId` below the root.
  It becomes dangling when the root field `p` is renamed upstream and the type
  is re-adopted.
- `record_set` on an anonymous root outputs that anonymous type, and its
  literal survives a re-adoption that keeps its row.
- A `record_get` used through its function pin in `map.f` is committed, so
  connecting a different record is refused.
- Round trip: `query` → `--replace` no-op for anonymous `record_get` /
  `record_set`; a `paths` without a root errors.

Manual walkthrough: drag-place `record_get` from an anonymous record whose
field `p` is a `Point`. **Add path** shows `p` expandable to `x, y, z`.
Choose `p.x`. Then rename upstream `p` → `q` and take the type from the wire:
the row turns into a broken row with its warning.

### Phase 4 — `expr` reads named records

Independent of Phases 1–3. It can go first.

- `Expr::validate` with `Option<RecordDefs>`, the pending marker, and
  `NodeData::refresh_with_record_defs` called from the populate pass.
- The error texts.
- Reference guide: the `expr` section on member access. Remove the "pass the
  record through a `record_destructure` first" advice wherever it appears.

Tests (`expr_named_record_access_test.rs`):

- `r.size` on `Record(Named(Box))` types as `Box.size`'s type and evaluates.
- Retyping `Box.size` re-derives the `expr`'s output type through repair.
- A missing field names the def.
- An `Optional` field reads as `T` and yields `None` when unset.
- Loading a `.cnnd` whose `expr` reads a named field gives the right output
  type without any edit (the load path's populate settles the pending result).

Manual walkthrough: promote an anonymous `record_construct` that feeds an
`expr` reading `r.a`. Then retype that `expr`'s parameter to the named record
from its type dropdown: `r.a` still evaluates.

## Testing

Each phase lists its own tests, written in that phase. The method is the
crate's usual one:

- integration tests in `rust/crates/atomcad-structure-designer/tests/`,
  driven through `StructureDesigner` (the same ops the API calls);
- undo checked by undo → compare snapshot → redo → compare snapshot;
- text-format behaviour checked through the `query` → `--replace` round trip;
- the Flutter side checked by `flutter analyze` and the manual walkthroughs
  above.

## Future work

- **Deferred path resolution in text:** let `record_get` / `record_set`
  statements omit the root and resolve `paths` after the retry pass adopts
  the wire's type.
- **"Match a def" on a reader:** offer the defs whose fields exactly match an
  anonymous reader's type, as `matching_record_defs_for` does for
  `record_construct`, as a one-click switch to named mode.
- **Anonymous `product`:** needs a field editor for its output type. A
  different design.
- **Dropping the "anonymous stays anonymous" exception** in `offered_type`,
  once Phase 4 has been in use.
