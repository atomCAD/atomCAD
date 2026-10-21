# `record_get` / `record_set` — read and update record fields by path

## Motivation

Records nest (`Box = { p: Point, size: Float }`, `Point = { x: Float, y:
Float, z: Float }`), but the only tools for working with them are
`record_construct` (build a whole record from one pin per field) and
`record_destructure` (split a record into one pin per top-level field). Two
common jobs are therefore expensive:

- **Changing one field.** Setting `box.p.x` today means destructuring `Box`,
  destructuring `Point`, constructing a new `Point` and constructing a new
  `Box` — four nodes, and every untouched sibling field (`p.y`, `p.z`, `size`)
  wired through by hand. The cost grows with depth (two nodes per level) and
  with the width of every record on the path. If the def later gains a field,
  every such hand-built "update" needs another wire.
- **Reading a deep field.** Reading `box.p.x` needs one `record_destructure`
  per level. The expression language does not help: `r.p.x` works only on
  **anonymous** records — on a named record `expr` validation rejects field
  access outright ("record name resolution requires a registry",
  `expr/expr.rs`, `Expr::MemberAccess`) — and a `Record(Named)` pin is what
  every user-declared def produces.

This adds two nodes:

- **`record_get`** — one `record` input; one output pin per user-chosen
  *path* (`p.x`, `size`, …) into the record, at any depth.
- **`record_set`** — one `record` input; one input pin per user-chosen path,
  each assigning the value of that field (wired or an inline literal); one
  output, the updated record. Every field not named by a path passes through
  untouched — including extra runtime fields beyond the schema.

`record_destructure` stays as it is: the "show me every top-level field" node.
`record_get` is the "just these, at any depth" node.

## Resolved decisions

| Question | Decision |
|---|---|
| Pin naming | **Path segments joined with `_`**: `p.x` → pin `p_x`, `size` → `size`. Deterministic dedup on collision (below). |
| How the record type is known | A **`schema` property** naming a record def, exactly like `record_destructure` — set automatically by drag-aware add. Pins don't vanish when the input wire is disconnected. Anonymous records are not supported in v1. |
| Path identity | Each path is stored as a list of **`FieldId`s** (`doc/design_record_field_identity.md`), not names. A field rename anywhere along the path keeps the path (and its wire); a delete-and-add does not. |
| `record_set` semantics | **Update only.** Every path must exist in the schema; the output type is the schema. No adding fields, no retyping fields. |
| Value conversion on `record_set` | Allowed **at the wire**, like on any pin: the row's pin is typed with the field's type, so an `Int` wired into a `Float` field converts. This does not contradict the "no value-converting widenings inside records" rule (`doc/design_record_types.md` principle 3) — that rule exists because `record_destructure` has nowhere to convert; `record_set`'s pin is that place. |
| Unwired `record_set` row | Uses its stored literal if it has one, otherwise **leaves the field unchanged** (a no-op row, with a non-blocking warning). |
| Overlapping `record_set` paths (`p` and `p.x`) | Allowed; rows apply **top to bottom**. "Replace `p`, then adjust `p.x`" is well-defined and useful. |
| Duplicate paths | Rejected at edit time on both nodes. |
| What a path may step through | Record-typed fields only (named or anonymous record types). **Not** `Array`, **not** `Optional[Record]` in v1. The *leaf* may be any type, including `Optional[T]`. |
| A path that stops resolving | The row is kept in the node data as a **dangling row**: no pin, a non-blocking warning, shown as broken in the panel. Same outcome for the wire as `record_construct` / `record_destructure` give a deleted field. |

## Node shape

### `record_get`

| Pin | Index | Type | Notes |
|---|---|---|---|
| `record` | input 0 | `Record(Named(schema))` | fixed, `id: None` |
| `<derived name>` × N | output 0 … N−1 | the leaf field's pin type | one per **resolving** row, `OutputPinDefinition::id = row.id` |

With no resolving row there is one placeholder output pin `result: None`, the
same `output_pins.len() >= 1` invariant `record_destructure` keeps.

Output pin 0 is the first row, so in the text format a bare `g` means the
first path. A one-row `record_get` is therefore also the "pluck" idiom: its
function pin (`-1`) wired into `map.f` turns `Array[Box]` into `Array[Float]`.

### `record_set`

| Pin | Index | Type | Notes |
|---|---|---|---|
| `record` | input 0 | `Record(Named(schema))` | fixed, `id: None` |
| `<derived name>` × N | input 1 … N | the leaf field's pin type | one per resolving row, `Parameter.id = row.id` |
| output | 0 | `Record(Named(schema))` | `OutputPinDefinition::single_fixed` |

`record` must stay pin 0: `map.f` matches on a function's *leading* parameter
(`AnyFunction { leading_params: [element_type] }`), so `record_set`'s `-1` pin
wired into `map.f` is the "update every record in the array" idiom.

"Leaf field's pin type" is `DataType::record_field_pin_type()` — an
`Optional[T]` leaf is a plain `T` pin, the convention `record_construct` and
`record_destructure` already follow (`doc/design_optional_type.md` §5).

## Data model

New shared module `nodes/record_path.rs` (both nodes use it), plus
`nodes/record_get.rs` and `nodes/record_set.rs`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathSegment {
    /// The field's stable identity when the containing record type is a
    /// named def. `None` inside an anonymous record type, which has no
    /// `FieldId`s — resolved by `name` there.
    #[serde(default)]
    pub field_id: Option<u64>,
    /// The field's name when the row was last edited. Used to resolve
    /// segments with no `field_id`, and in the "no longer exists" message
    /// of a dangling row. Not used to resolve a segment that has an id.
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordPathRow {
    /// Hidden stable identity: the pin id. Wires survive path edits,
    /// reorders, and removals of other rows.
    #[serde(default)]
    pub id: Option<u64>,
    pub path: Vec<PathSegment>, // never empty
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordGetData {
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub rows: Vec<RecordPathRow>,
    /// Monotonic id source; never max(existing)+1
    /// (`doc/design_parameter_wire_stability.md`).
    #[serde(default)]
    pub next_row_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordSetData {
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub rows: Vec<RecordSetRow>,
    #[serde(default)]
    pub next_row_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordSetRow {
    #[serde(flatten)]
    pub row: RecordPathRow,
    /// Stored literal, consulted when the pin is unwired. Lives on the row,
    /// **not** in a name-keyed map: name-keyed literals are the casualty
    /// `doc/design_record_field_identity.md` §4.5 had to repair on
    /// `record_construct`.
    #[serde(default)]
    pub literal: Option<TextValue>,
}
```

A fresh node has an empty `schema` and no rows. Zero rows is legal (a
`record_get` shows the placeholder pin; a `record_set` is an identity).

### Path resolution

One function, the single source of truth for both nodes and for every pass
that needs a row's pin:

```rust
/// Resolves `path` against `schema`. Returns the current segment names and
/// the leaf's declared type, or why the path no longer resolves.
pub fn resolve_path(
    schema: &str,
    path: &[PathSegment],
    record_type_defs: &HashMap<String, RecordTypeDef>,
    built_in_record_type_defs: &HashMap<String, RecordTypeDef>,
) -> Result<ResolvedPath, DanglingReason>;

pub struct ResolvedPath {
    pub names: Vec<String>,   // current names, after any rename
    pub leaf_type: DataType,  // declared type (may be Optional[T])
}
```

It takes the two def maps, not `&NodeTypeRegistry`, for the same reason
`record_construct::build_node_type_for_schema_with_defs` does: the cache
populator calls it under a concurrent `&mut node_networks` borrow.

Walking a segment: if the current record type is `Named(def)`, find the field
by `field_id` (looked up user-first then built-in, as
`lookup_record_type_def` does); if it is `Anonymous(fields)`, find it by
`name`. Every segment except the last must land on a field whose declared
type is `Record(_)` — not `Optional`, not `Array`. Failure reasons: the schema
does not resolve; a segment's field is gone; an inner segment's field is no
longer a record.

Because a nested path reaches into defs other than `schema` (`Box.p` →
`Point.x`), an edit to `Point` changes a `record_get` on `Box`. That already
works: `repair_node_network` re-derives *every* node's `custom_node_type` on a
def change, not just nodes naming the edited def (see the "Why every node"
comment in that pass). Phase 1 has a test for it.

## Pin name derivation

One helper, `record_path::derived_pin_names(resolved: &[Option<ResolvedPath>],
reserved: &[&str]) -> Vec<Option<String>>`, used by both nodes'
`build_node_type_for_schema_with_defs`, and therefore by the text-format
serializer and parser, which resolve pin names through the custom node type.

- **Base name:** the current segment names joined with `_` — `p.x` → `p_x`.
  Field names are not necessarily identifiers (`z-shift` is a legal field
  name; so is one containing `.`), so the joined name is not necessarily one
  either. The text format already backtick-quotes such pin names
  (`format_identifier`; the serializer comment calls out record field pins
  for exactly this reason). No sanitizing.
- **Collisions.** Underscore joining can collide: path `a.b` and a top-level
  field named `a_b` both produce `a_b`. A derived name can also collide with a
  reserved name — the fixed `record` pin, the node's own text-property keys
  (`schema`, `paths`), and the text format's special properties (`visible`,
  `on`, `pin_roles`). Dedup works like `switch`'s: walk the rows in order; the
  first holder of a name keeps it, and later collisions (and any collision
  with a reserved name) append `__2`, `__3`, ….
  Deterministic given the rows, so serialize → parse agree.
- Dangling rows get no name and take no part in the dedup.

`record_get`'s reserved list is `schema`, `paths` plus the special properties;
output pin names never share a namespace with the `record` input pin, so
`record` is reserved only on `record_set`, where the row pins are inputs too.

Wire identity never depends on these names: input wires are rebuilt by
`node_network.rs::set_custom_node_type`, which matches parameters by id when
both sides carry one (what `switch` / `zip_with` rely on). Output wires are
remapped by output-pin id in `repair_node_network`. That remap (the R3 block
built for `record_destructure`, `record_old_pin_ids`) keys on any output pin
with `id.is_some()`, not on `record_destructure` by name, so `record_get`'s
row-id output pins are covered without changes. **Phase 1 checks this
claim with a test**, since the block's comment says "only
`record_destructure` per-field output pins carry ids".

## Runtime semantics

### `record_get`

1. No resolvable schema → `EvalOutput::single(None)` (placeholder pin).
2. `evaluate_arg(0)`: `None` → `None` on every pin; `Error` → that error on
   **every** pin (the multi-output rule in `nodes/AGENTS.md`).
3. For each resolving row, walk the runtime value with
   `NetworkResult::extract_record_field` segment by segment. A missing field
   or a `None` part-way down → `None` on that pin. (Records pass through
   without projection, so the runtime value may also carry extra fields;
   these are ignored.)

### `record_set`

1. No resolvable schema → `None`.
2. `evaluate_arg(0)`: `None` → `None`; `Error` → forward verbatim.
3. For each resolving row, in order, pick the value: a wired pin wins; else
   the row's `literal` if it converts to the pin type (`TextValue::to_network_result`);
   else skip the row (no-op). The record_construct rule
   `wired > literal > fallback` holds, with "leave the field unchanged" as the
   fallback.
4. A picked value that is `Error` → forward it as the output. A picked `None`:
   on an `Optional[T]` leaf, store an explicit `None` (this is how a user
   *unsets* an optional field); on a required leaf, the output collapses to
   `None` — `record_construct`'s all-or-nothing rule.
5. Write the value: rebuild each record along the path with the one field
   replaced (`NetworkResult::record` re-sorts into canonical order). A record
   part-way down that is missing at runtime (only reachable with a malformed
   value, since the static type guarantees it) → a localized `Error` naming
   the path. A missing *leaf* field is simply inserted.

Untouched fields — declared or extra — are copied through, so the width
subtyping pass-through rule (`doc/design_record_types.md`, principle 5)
holds: a `{x, y, z, tag}` value entering a `record_set` on `{x, y, z}` comes
out still carrying `tag`.

The record is cloned once per row on the path (records are small); no
attempt is made to share structure.

## Row editing

`record_path::merge_rows(old, new_paths, next_row_id)` — shared by both
nodes' `set_text_properties` and the `StructureDesigner`-level setters. It is
`switch`'s value-keyed merge (`doc/design_switch_node.md`, "Case-list
editing"), with the *path* as the key, because paths are unique per node:

1. Reject duplicates and empty paths (error, node unchanged).
2. **Path-match pass** — a new path equal to an old row's path keeps that
   row's id (and, on `record_set`, its literal). Handles removal and reorder.
3. **Positional-fallback pass** — a still-unmatched new path inherits the id
   of the old row at its index if that id wasn't consumed in pass 2. Handles
   editing a row's path in place: the wire follows. On `record_set` the
   literal is kept only if it still converts to the new leaf type.
4. **Mint pass** — anything still without an id takes `next_row_id`.

The passes must run separately; `switch`'s doc explains why a single pass
steals ids (`[1, 2] → [3, 1]`).

Two paths are equal when their segments match by `field_id` where both
sides have one, and by name otherwise.

### Schema change

Picking a different `schema` re-resolves each row **by name** against the
new def (field ids are per-def, so ids from the old def mean nothing there)
and rewrites the segments' `field_id`s. Rows that don't resolve become
dangling. This is an explicit user edit, so it goes through the same
undoable command as a row edit.

### Undo

Row edits and schema changes drop or remap wires, which a node-data snapshot
cannot restore. Use the existing **`NodeStructureEditCommand`**
(`undo/commands/node_structure_edit.rs`, shared by `zip_with` and `switch`),
with descriptions "Edit record_get paths" / "Edit record_set paths".
`StructureDesigner::set_record_get_data(scope_path, node_id, schema, paths)` /
`set_record_set_data(…)` follow `set_switch_data` as written: a read-only
pre-check, a no-op check, `snapshot_network` before, mutate, validate/refresh,
snapshot after, and push only if the snapshots differ. Setting a single
literal is a pure node-data edit (no wire fallout) and uses the ordinary
node-data path, as `set_record_construct_literal` does.

The text path pushes nothing of its own (`TextEditNetworkCommand` covers it).

## Dangling rows

A row whose path stops resolving — a field on the path deleted, an inner
field retyped away from a record, the schema def deleted — stays in the node
data and gets no pin. Repair then drops its wire exactly as it drops a deleted
field's wire on `record_construct` / `record_destructure`. The node carries a
**non-blocking** warning (`ValidationError::warning`, through
`NodeData::get_data_error` or the validator — whichever
`record_construct` uses for a dangling schema today), for example
"path `p.x` no longer exists in `Box`", built from the stored segment names.
The node's other rows keep working, which is why the warning doesn't block
the node (`structure_designer/AGENTS.md`, "is the node's output still
useful?").

A dangling row never resolves again by itself: a re-added field has a new
`FieldId`, and that is the point of field identity. The panel shows the row
as broken, with a delete button. Undoing the def edit brings everything
back, because the def edit's own undo restores the def, and repair
re-derives the pins.

Retyping only the **leaf** is not dangling: the pin is re-typed, and repair
drops the wire if it no longer fits.

## Registry and repair integration

The three existing record nodes are special-cased by name in several places
because their pins come from the registry, not from node data. Each place
gets a `record_get` and a `record_set` arm:

- `NodeTypeRegistry::populate_custom_node_type_cache_with_types` and
  `resolve_drag_candidate_type` — call the new
  `build_node_type_for_schema_with_defs`.
- `library_links::record_node_schema` (the frozen-node rule: a `schema` under
  an unresolved mount freezes the node) and `library_links::recorded_layout`.
- `collect_record_refs_in_node` / `rewrite_record_names_in_node` (record-def
  rename, the namespace-delete reference check, cross-document paste) — the
  `schema` string, as a `Schema` site. Paths need no rewrite: they hold ids,
  plus names that are only fallbacks.
- `canonicalize::canonicalize_node_data`'s downcast list (it must stay in
  sync with the two above).
- `invariants.rs`'s dangling-schema check.
- `library_refresh` reconciles `record_destructure` outputs and
  `record_construct` inputs by `FieldId` across a library change. The new
  nodes' pins carry **row ids**, which are local to the node, so there is
  nothing to reconcile. The paths' `FieldId`s point into the library's def,
  and `claim_field_ids` keeps a file's field ids unique across its history.
  So a field the library renamed keeps resolving, and a field it deleted and
  re-added gets a new id and the row goes dangling. Phase 1 tests both.

When a grep for `"record_destructure"` turns up a further site during
implementation, it gets an arm too; the list above is the set known at
design time.

## Text format

```
g = record_get { schema: "Box", paths: ["p.x", "size"], record: b }
s = record_set { schema: "Box", paths: ["p.x", "size"], record: b, p_x: 3.0, size: g.size }
```

- `paths` is a `TextValue::Array` of `TextValue::String`. Segments are
  separated by `.`. A segment that itself contains `.` is written in
  backticks: `` "`a.b`.c" ``. Field names can't contain a backtick
  (`identifier::is_valid_user_name`), so the quoting is unambiguous.
  Ids never appear in the text format; the parser resolves names against the
  schema and runs `merge_rows`.
- `record_set` literals are emitted by `get_text_properties` under the row's
  **derived pin name** (`p_x: 3.0`), so they read like any pin literal. The
  text editor accepts a key as a literal when it is in
  `get_text_properties`'s list (`apply_literal_properties`), so these keys
  are accepted. `set_text_properties` must apply `schema`, then `paths`, and
  only then resolve the literal keys against the **new** derived names: one
  text block can change all three.
- A key that names neither a property nor a current row pin gets the
  existing "Unknown property" warning.

## Serialization and healing

`generic_node_data_saver` / `loader`. These are new node types, so there is no
`.cnnd` migration. Loader healing, as in `switch`:

- a missing or zero `next_row_id` → max(row ids)+1;
- a row loaded with `id: None` gets a fresh id;
- duplicate paths: later duplicates are dropped;
- an empty path: the row is dropped.

## Misc node behaviors

- **`get_subtitle`**: the schema name, as on `record_destructure`.
- **`adapt_for_drag_source`**: adopt a `Record(Named(n))` source's name as the
  schema when `n` resolves. `record_get`: `FromOutput` (dragging off a record
  output). `record_set`: both directions (its input and its output are the
  record). Anonymous records → `None`.
- **Display**: `record_get` keeps the default pin-0-only display. Its outputs
  are usually scalars or vectors, which draw nothing, so it could opt into
  `default_display_all_output_pins` like the unpack nodes. It doesn't,
  because a path can end on a `Crystal` field, and showing every such pin at
  once is the clutter that default exists to prevent.
- **Category**: `MathAndProgramming`. Descriptions mention "field", "path",
  "nested", "update", "get", "set" so registry search finds them.

## Out of scope (deferred)

- **Anonymous records** (from `expr` literals): no def to name in `schema`.
  Would need the schema stored inline, or taken from the input wire's type.
- **Adding fields or changing a field's type** (`record_extend` /
  `record_merge`): the output would be an anonymous record type, which runs
  into the same gap.
- **Paths through arrays or `Optional`** (`atoms[2].x`): `map` +
  `record_get` covers arrays; stepping into an `Optional[Record]` needs a rule
  for what `record_set` does when the inner record is unset.
- **Expression-language field update** (`r with { p.x: 3.0 }`): the language
  layer is frozen. (Making `expr`'s `r.x` work on named records would be a
  separate, small change — the validator would need the registry.)

---

## Phase 1 — Path core + `record_get`

**Deliverables**

- `nodes/record_path.rs`: `PathSegment`, `RecordPathRow`, `resolve_path`,
  `derived_pin_names` (with dedup and reserved names), path parsing and
  formatting for the text format (backtick quoting).
- `nodes/record_get.rs`: `RecordGetData`, `NodeData` impl (`eval`,
  `get_subtitle`, `adapt_for_drag_source`, a minimal
  `get/set_text_properties` with `schema` and `paths`),
  `build_node_type_for_schema_with_defs`, and the placeholder pin.
- Registration (`nodes/mod.rs`, `create_built_in_node_types`), plus every
  arm listed in "Registry and repair integration" for `record_get`.
- Re-bless node-type snapshots; update registry count/list assertions.

**Automated tests** — new
`crates/atomcad-structure-designer/tests/structure_designer/record_get_test.rs`:

1. Top-level and two-level paths read the right values; output pins are
   named `size`, `p_x`.
2. `None` / `Error` input → `None` / that error on every pin.
3. An `Optional[T]` leaf: a plain `T` pin, and an unset value passes `None`.
4. A path through an **anonymous** record type inside a named def resolves
   by name.
5. Pin-name dedup: path `a.b` beside top-level field `a_b`; a field named
   `schema`, and one named `visible`, get `__2`.
6. Field rename on the **nested** def (`Point.x` → `Point.px`): the pin
   renames to `p_px`, and the downstream wire survives.
7. Field reorder in the nested def: no change to pins or wires. Field delete
   → dangling row, no pin, warning, and the downstream wire is dropped.
   Undoing the def edit restores pin and wire.
8. Output-pin id remap: rows `[size, p.x]`, both wired downstream; the
   nested field `p.x` is deleted, so `size` stays on pin 0. Then rows
   `[p.x, size]` with `p.x` deleted: `size`'s wire moves from pin 1 to pin 0
   (the R3 remap reaches `record_get`).
9. Def rename and namespace move rewrite `schema`; deleting a def referenced
   only by a `record_get` is caught by the reference check.
10. A row in a **zone body** survives a def edit (the `walk_all_nodes` class).
11. Library linking: a renamed library field keeps the path; a deleted and
    re-added one makes the row dangling; the schema under a missing mount
    freezes the node.
12. Drag-aware add from a `Record(Named)` output produces a `record_get` with
    the schema set.

**Gate:** `cd rust && cargo fmt && cargo clippy && cargo test -j 4` (and the
`structure_designer_api` / `integration` harnesses explicitly).

## Phase 2 — `record_set`

**Deliverables**

- `nodes/record_set.rs`: `RecordSetData` / `RecordSetRow`, `NodeData` impl,
  `build_node_type_for_schema_with_defs`, the integration arms.
- `eval` as specified under "Runtime semantics".

**Automated tests** — new `record_set_test.rs`:

1. Set a top-level field and a two-level field; untouched fields, declared
   and **extra runtime fields**, pass through.
2. `Int` wired into a `Float` leaf converts.
3. Unwired row with a literal → the literal; unwired without one → field
   unchanged.
4. Rows `p` then `p.x`: the second applies on top of the first; in reverse
   order, `p` wins.
5. `None` on a required leaf collapses the output; `None` on an `Optional`
   leaf stores an explicit `None`.
6. An `Error` on the record or on a row value is forwarded verbatim (not
   re-wrapped).
7. Function pin: `record_set`'s `-1` into `map.f` updates every element of an
   `Array[Box]`; the row pins left unwired become the closure's later
   parameters (`function_pin_dispositions`), and the result type is
   `Array[Box]`.
8. Leaf retype in the def: the pin is retyped, and an incompatible wire is
   dropped by repair.
9. The tests from Phase 1 that apply to input pins: nested rename keeps the
   wire (by `Parameter.id`); a nested delete gives a dangling row.

**Gate:** as Phase 1.

## Phase 3 — Row editing, undo, text format, serialization

**Deliverables**

- `record_path::merge_rows` (three passes), schema-change re-resolution.
- `StructureDesigner::set_record_get_data` / `set_record_set_data` and a
  literal setter for `record_set` rows, all taking `scope_path`.
- Full `get/set_text_properties` (the `schema` → `paths` → literals order),
  loader healing.

**Automated tests** — the two node test files, plus `undo_test.rs`,
`text_format_test.rs`, `cnnd_roundtrip_test.rs`:

1. Remove the middle row of three: the survivors' wires follow their ids,
   and the removed row's wire drops. This must be checked on `record_get`
   **outputs** as well as `record_set` inputs.
2. Edit a row's path in place: the wire survives (positional fallback); a
   `record_set` literal survives if it still converts and is dropped if not.
3. No positional steal: `[a, b] → [c, a]` keeps `a`'s wire.
4. Removing the highest-id row and adding one doesn't reuse the id.
5. Duplicate and empty paths are rejected; the node is unchanged.
6. Schema change: rows re-resolve by name; non-resolving rows go dangling.
   Undo restores the old schema, rows and wires exactly (`normalize_json`),
   including for a body-internal node.
7. Text: parse the canonical example and the wires land correctly; the
   serialize → `--replace` → serialize round trip is byte-equal, including a
   dedup-suffixed name and a backtick-quoted segment; one block that changes
   `schema`, `paths` and a literal applies all three.
8. `.cnnd` round-trip with wired rows, a literal and a dangling row; healing
   of id-less rows, a zero counter and duplicate paths.
9. Add the canonical example to the text-format round-trip corpus (a new
   `.cnnd` fixture → two corpus snapshots; use `add_value_node` /
   `add_typed_value_node`).

**Gate:** full `cargo test -j 4`, including `cargo test cnnd_roundtrip` and
`cargo test node_snapshots`.

## Phase 4 — API, Flutter UI, documentation

**Deliverables**

- API (`rust/src/api/structure_designer/`): `get_record_get_data` /
  `set_record_get_data`, `get_record_set_data` / `set_record_set_data`, and
  a literal set/clear pair, all `#[frb(sync)]` and all taking **`scope_path`**.
  Paths cross the API as `Vec<Vec<String>>` (segment names); each row reports
  its derived pin name, its leaf type, and whether it is dangling (with the
  reason). Ids never cross the API, and the setters wrap the Phase 3 ops.
  The picker needs a def's fields one level at a time; use the existing
  record-def getters, and add a field-tree getter only if recursing from
  Dart proves awkward.
- `flutter_rust_bridge_codegen generate` (after `cargo fmt`).
- `lib/structure_designer/node_data/record_get_editor.dart` and
  `record_set_editor.dart`, registered in `node_data_widget.dart`. Each has
  the schema dropdown (`record_def_dropdown.dart`), one row per path showing
  the path, the type and a delete button (broken rows in an error style), and
  an "Add path" button that opens a **tree picker** of the schema's fields.
  Record-typed fields expand; picking any field adds a row. `record_set` rows
  also get the inline literal editor from `literal_fields_editor.dart`, with
  the same stored / unset / wired states `record_construct` shows.
- Reference guide: `doc/reference_guide/nodes/math_programming.md` — new
  `## record_get` and `## record_set` sections after `record_destructure`,
  and a line in "Record types" pointing to them for nested access.
- `nodes/AGENTS.md`: add both nodes to the **Records** bullet, and note
  `record_path.rs` as the one path resolver.

**Checks**

1. A Rust test that drives the setters on a **body-internal** node through
   `scope_path`.
2. `flutter analyze` (no new issues over the baseline), `dart format` on the
   touched files.
3. Manual walkthrough (the maintainer's, together with the Flutter smoke
   test): build `Box`/`Point`, read `p.x` with `record_get`, set `p.x` with a
   literal and then with a wire, rename `Point.x` in the schema editor and
   watch the pin rename with its wire intact, delete `Point.x` and see the
   broken row, then Ctrl+Z; drive `record_set` through `map.f` over an array
   of boxes.

---

## Open questions

1. **Should `record_destructure` grow deep paths instead of a new node?** The
   design keeps it separate: destructure's job is "all top-level fields",
   and turning it into "chosen paths" changes what an existing node shows.
   Revisit if users find two read nodes confusing.
2. **Anonymous records** are the main limitation, since `expr` produces them.
   If they turn out to be common, the next step is a "schema from input
   wire" mode, at the cost of pins depending on the wire.
