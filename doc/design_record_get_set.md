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
| Path identity | Each path segment is stored by **`FieldId`** (`doc/design_record_field_identity.md`) when its record type is a named def, and by name only inside an anonymous record type, which has no ids. A field rename anywhere along the path keeps the path (and its wire); a delete-and-add does not. |
| `record_set` semantics | **Update only.** Every path must exist in the schema; the output type is the schema. No adding fields, no retyping fields. |
| Value conversion on `record_set` | Allowed **at the wire**, like on any pin: the row's pin is typed with the field's type, so an `Int` wired into a `Float` field converts. This does not contradict the "no value-converting widenings inside records" rule (`doc/design_record_types.md` principle 3) — that rule exists because `record_destructure` has nowhere to convert; `record_set`'s pin is that place. |
| Unwired `record_set` row | Uses its stored literal if it has one, otherwise **leaves the field unchanged** (a no-op row). No warning: an unwired row is also how a row becomes a parameter when the node is used through its function pin (see "Node shape"). |
| Overlapping `record_set` paths (`p` and `p.x`) | Allowed; rows apply **top to bottom**. "Replace `p`, then adjust `p.x`" is well-defined and useful. |
| Duplicate paths | Rejected at edit time on both nodes. |
| What a path may step through | Record-typed fields only (named or anonymous record types). **Not** `Array`, **not** `Optional[Record]` in v1. The *leaf* may be any type, including `Optional[T]`. |
| A path that stops resolving | The row is kept in the node data as a **dangling row**: no pin, a non-blocking warning, shown as broken in the panel. Its wire is dropped, just as `record_construct` / `record_destructure` drop a deleted field's wire. Every by-name edit path (text format, API) can name a dangling row, so editing other rows never forces a decision about it (see "Row editing"). |
| A path through a def under a missing library mount | **Not** a dangling row: the node is frozen, like a node whose schema is under the missing mount (see "Frozen nodes and nested paths"). |

## Node shape

### `record_get`

| Pin | Index | Type | Notes |
|---|---|---|---|
| `record` | input 0 | `Record(Named(schema))` | fixed, `id: None` |
| `<derived name>` × N | output 0 … N−1 | the leaf field's pin type | one per **resolving** row, `OutputPinDefinition::id = row.id` |

With no resolving row (no schema, no rows, or only dangling rows) there is
one placeholder output pin `result: None`, the same
`output_pins.len() >= 1` invariant `record_destructure` keeps.

Output pin 0 is the first resolving row, so in the text format a bare `g`
means the first path. A node's function pin (`-1`) returns its pin-0 output,
so a one-row `record_get` is also the "pluck" idiom: its `-1` wired into
`map.f` turns a stream of `Box` into a stream of `Float` (`map` yields an
`Iter`; `collect` it for an array).

### `record_set`

| Pin | Index | Type | Notes |
|---|---|---|---|
| `record` | input 0 | `Record(Named(schema))` | fixed, `id: None` |
| `<derived name>` × N | input 1 … N | the leaf field's pin type | one per resolving row, `Parameter.id = row.id` |
| output | 0 | `Record(Named(schema))` | `OutputPinDefinition::single_fixed` |

`record` must stay pin 0: `map.f` matches on a function's *leading* parameter
(`AnyFunction { leading_params: [element_type] }`), so `record_set`'s `-1` pin
wired into `map.f` is the "update every record in the stream" idiom. Under
the default function-pin partition, the unwired row pins become the
closure's later parameters, and wired rows are captured.

Per-pin function-pin roles (`Node::function_pin_roles`,
`doc/design_function_pin_roles.md`) are keyed by **input-pin index**, and
today nothing moves them when the layout changes. That is a latent bug for
`switch` and `zip_with` already, but `record_set` makes it likely: its row
pins shift without the user touching the node, whenever a def edit elsewhere
dangles a row and removes its pin. A `Delayed` or `Supplied` role would then
silently move to a different field. Phase 0 therefore makes
`Node::set_custom_node_type` remap `function_pin_roles` with the **same**
old-index → new-index map it already builds for `arguments` (id match first,
then name), and drop the roles of pins that disappeared. A role dropped with
a dangling row's pin is not restored if the row is revived; undoing the def
edit restores it with the snapshot.

"Leaf field's pin type" is `DataType::record_field_pin_type()` — an
`Optional[T]` leaf is a plain `T` pin, the convention `record_construct` and
`record_destructure` already follow (`doc/design_optional_type.md` §5).

## Data model

The code splits by layer:

| Module | Contents | Why there |
|---|---|---|
| `record_path.rs` (crate root, beside `data_type.rs`) | `PathSegment`, `resolve_path`, `ResolvedPath`, `DanglingReason`, `resolve_names` (names → segments), `PathSpec`, the path text syntax (parse / format with backtick quoting and the `!` dangling marker) | Type-level logic over record defs. Its callers include registry passes, `library_links` and the API (field picker, dangling reasons), not only nodes. |
| `nodes/record_path_rows.rs` | `RecordPathRow`, `derived_pin_names`, `merge_rows`, the shared row-to-pins builder both nodes' hooks call | Node-level: pin layout and row editing, shared by the two nodes. |
| `nodes/variadic_ids.rs` | `merge_ids_by_key`, the generic three-pass id merge | Shared with `switch` (see "Row editing"). |
| `evaluator/network_result.rs` | `extract_record_path`, `with_record_path_replaced` | Runtime record access lives beside `extract_record_field` / `record()` (see "Runtime semantics"). |
| `nodes/record_get.rs`, `nodes/record_set.rs` | the node data and `NodeData` impls | |

```rust
// record_path.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathSegment {
    /// The field's stable identity when the containing record type is a
    /// named def. `None` inside an anonymous record type, which has no
    /// `FieldId`s — resolved by `name` there.
    #[serde(default)]
    pub field_id: Option<u64>,
    /// The field's name when the row was last edited. Used to resolve a
    /// segment with no `field_id`, and as the **identity of a dangling row**
    /// (its warning message, its `!path` text form, its API form). Never used
    /// to resolve a segment that has an id, and not kept up to date on
    /// renames: current names always come from `resolve_path`. In
    /// particular, a schema change does not resolve live rows by these names
    /// (see "Schema change").
    pub name: String,
}

// nodes/record_path_rows.rs
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
    defs: RecordDefs<'_>,
) -> Result<ResolvedPath, DanglingReason>;

pub struct ResolvedPath {
    pub names: Vec<String>,   // current names, after any rename
    pub leaf_type: DataType,  // declared type (may be Optional[T])
}

pub enum DanglingReason {
    /// A def the path needs (the schema, or a nested def) is not in `defs`.
    DefMissing(String),
    /// A segment's field is not in its record type any more.
    FieldGone { depth: usize },
    /// An inner segment's field is no longer a record type.
    NotARecord { depth: usize },
}

/// The inverse direction, for the text format and the API, where paths
/// arrive as names: resolves `names` against `schema` and returns the
/// segments with their current `FieldId`s, or the depth of the first name
/// that doesn't resolve.
pub fn resolve_names(
    schema: &str,
    names: &[String],
    defs: RecordDefs<'_>,
) -> Result<Vec<PathSegment>, usize>;
```

`RecordDefs` is introduced in Phase 0: a `Copy` view over the user and
built-in def maps with one `lookup(name)` (user first, then built-in). The
registry hands it out as `registry.record_defs()`, and the cache populator,
which can't borrow the whole registry while it holds `&mut node_networks`,
builds it from its two map borrows. It can also carry an optional **overlay**
map, consulted before the other two; only `recorded_layout` uses it, to lay
recorded interfaces of unloaded library defs over the live ones (see "Frozen
nodes and nested paths"). It replaces the
`(record_type_defs, built_in_record_type_defs)` parameter pair that the
existing `build_node_type_for_schema_with_defs` functions pass around and
that would otherwise spread to every new function here.

Walking a segment: if the current record type is `Named(def)`, find the field
by `field_id` through `defs.lookup`; if it is `Anonymous(fields)`, find it by
`name`. (`RecordType::resolve_fields` can't be reused for the named case: it
projects fields down to `(name, type)` and drops the `FieldId`.) Every
segment except the last must land on a field whose declared type is
`Record(_)` — not `Optional`, not `Array`. The failure cases are the three
`DanglingReason`s. A deleted schema def is `DefMissing`: for a live node it
makes every row dangling.

Because a nested path reaches into defs other than `schema` (`Box.p` →
`Point.x`), an edit to `Point` changes a `record_get` on `Box`. That already
works: `repair_node_network` re-derives *every* node's `custom_node_type` on a
def change, not just nodes naming the edited def (see the "Why every node"
comment in that pass). Phase 1 tests 6 and 7 cover it.

## Pin name derivation

One helper, `record_path_rows::derived_pin_names(resolved: &[Option<ResolvedPath>],
reserved: &[&str]) -> Vec<Option<String>>`, used by both nodes'
`build_record_node_type` (the Phase 0 hook), and therefore by the text-format
serializer and parser, which resolve pin names through the custom node type.

- **Base name:** the current segment names joined with `_` — `p.x` → `p_x`.
  Field names are not necessarily identifiers (`z-shift` is a legal field
  name; so is one containing `.`), so the joined name is not necessarily one
  either. The text format already backtick-quotes such pin names
  (`format_identifier`; the serializer comment calls out record field pins
  for exactly this reason). No sanitizing.
- **Collisions between rows.** Underscore joining can collide: path `a.b`
  and a top-level field named `a_b` both produce `a_b`. Dedup works like
  `switch`'s: walk the rows in order; the first holder of a name keeps it,
  and later holders append `__2`, `__3`, …. Deterministic given the rows,
  so serialize → parse agree.
- **Reserved names (`record_set` only).** `record_set`'s row pins are inputs,
  and input pins share the text format's `{ … }` block with everything else
  in it: the fixed `record` pin, the node's own property keys (`schema`,
  `paths`), and the special properties (`visible`, `on`, `pin_roles`). A
  derived name equal to one of these is treated as already taken, so it
  gets `__2`. `record_get`'s row pins are outputs, which the text format
  only ever names as `g.<pin>`, outside the block, so `record_get` reserves
  nothing.
- Dangling rows get no name and take no part in the dedup.

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

The record walking is not written in the nodes. Two methods go on
`NetworkResult` in `evaluator/network_result.rs`, beside
`extract_record_field` and `record()`, so the canonical-order and
binary-search details stay in one file:

```rust
/// Follows `path` through nested records. `None` if any step is missing
/// or not a record.
pub fn extract_record_path(&self, path: &[&str]) -> Option<&NetworkResult>;

/// A copy of `self` with the field at `path` replaced by `value`
/// (inserted if the leaf is missing). `Err(depth)` if a record part-way
/// down is missing or not a record.
pub fn with_record_path_replaced(
    &self,
    path: &[&str],
    value: NetworkResult,
) -> Result<NetworkResult, usize>;
```

Later callers can use them too: a `trace` for these nodes, or `expr` field
access if it is ever allowed on named records.

### `record_get`

1. No resolvable schema → `EvalOutput::single(None)` (placeholder pin).
2. `evaluate_arg(0)`: `None` → `None` on every pin; `Error` → that error on
   **every** pin (the multi-output rule in `nodes/AGENTS.md`).
3. For each resolving row, `extract_record_path` with the row's current
   segment names. A missing field or a `None` part-way down → `None` on that
   pin. (Records pass through without projection, so the runtime value may
   also carry extra fields; these are ignored.)

### `record_set`

1. No resolvable schema → `None`.
2. `evaluate_arg(0)`: `None` → `None`; `Error` → forward verbatim. With no
   resolving rows, the input passes through unchanged.
3. For each resolving row, in order, pick the value: a wired pin wins; else
   the row's `literal` if it converts to the pin type (`TextValue::to_network_result`);
   else skip the row (no-op). This is `record_construct`'s
   `wired > literal > fallback` rule, with "leave the field unchanged" as the
   fallback. `record_construct::eval` has the rule inline today; Phase 0 moves
   it into a shared helper both nodes call (`wired_or_literal(node,
   param_index, literal, pin_type, …) -> Option<NetworkResult>`, `None` meaning
   "unwired, no usable literal"), so the precedence can't drift.
4. A picked value that is `Error` → forward it as the output. A picked `None`:
   on an `Optional[T]` leaf, store an explicit `None` (this is how a user
   *unsets* an optional field); on a required leaf, the output collapses to
   `None` — `record_construct`'s all-or-nothing rule.
5. Write the value with `with_record_path_replaced`. A record part-way down
   that is missing at runtime (only reachable with a malformed value, since
   the static type guarantees it) → a localized `Error` naming the path. A
   missing *leaf* field is simply inserted.

Untouched fields — declared or extra — are copied through, so the width
subtyping pass-through rule (`doc/design_record_types.md`, principle 5)
holds: a `{x, y, z, tag}` value entering a `record_set` on `{x, y, z}` comes
out still carrying `tag`.

The record is cloned once per row on the path (records are small); no
attempt is made to share structure.

## Row editing

`record_path_rows::merge_rows(old_rows, new_paths, schema, defs,
next_row_id)` is shared by both nodes' `set_text_properties` and by the
`StructureDesigner`-level setters. New paths arrive as **names** (from the
text format or the API), and each one says whether it names a live path or
an existing dangling row:

```rust
// record_path.rs (beside the text syntax that parses into it)
pub enum PathSpec {
    /// A path that must resolve against the schema now.
    Live(Vec<String>),
    /// An existing dangling row, named by its stored segment names
    /// (`PathSegment::name`). Never resolved.
    Dangling(Vec<String>),
}
```

A dangling row can't be named by a live path: by definition its names don't
resolve. Without `Dangling`, every by-name edit of a node that has one would
either be rejected or would silently delete the dangling row, and the text
format's `query` → `--replace` would stop being a no-op. The text format
writes `Dangling` as a `!`-prefixed path (see "Text format"); the API carries
it as a flag on the row (Phase 4). Both come from what the getter / serializer
emitted for the row, so echoing a node's rows back unchanged is always a
no-op.

`merge_rows` turns each `Live` spec into segments with `resolve_names`, then
applies `switch`'s value-keyed id merge (`doc/design_switch_node.md`,
"Case-list editing") with the path as the key, since paths are unique per
node.

The merge is **not** copied from `SwitchData::merge_cases`. Phase 0
extracts it into `nodes/variadic_ids.rs`:

```rust
/// The three-pass id merge: key match, then positional fallback, then mint.
/// Returns one id per element of `new`. Rejects nothing: callers check
/// duplicates and emptiness first.
pub fn merge_ids_by_key<K>(
    old: &[(Option<u64>, K)],
    new: &[K],
    same_key: impl Fn(&K, &K) -> bool,
    next_id: &mut u64,
) -> Vec<u64>;
```

`merge_cases` becomes a wrapper over it, and so does `merge_rows`.
`switch`'s existing merge tests (the `[1, 2] → [3, 1]` steal case among them)
then cover both nodes. (`zip_with`'s `merge_lane_types` is a positional merge
with no key, so it stays as it is.)

`merge_rows` does, in order:

1. **Checks.** A `Live` name that doesn't resolve, an empty path, or a
   duplicate `Live` path is an error, and the node is unchanged.
2. **Dangling specs.** Each `Dangling` spec claims the first unclaimed old
   dangling row whose stored names equal it, in order, and that row is kept
   **verbatim** (id, segments, literal). A `Dangling` spec that matches no
   old dangling row is dropped with a warning ("dangling path `p.x` not
   found"), not an error: a pasted or retyped text block may name a row that
   no longer exists, and there is nothing to revive. `Dangling` specs never
   take part in the passes below, and old dangling rows they claim are
   withheld from them.
3. **Path match** (`merge_ids_by_key` pass 1, `Live` specs only). A new path
   equal to an old row's path keeps that row's id. Handles removal and
   reorder.
4. **Positional fallback** (pass 2). A still-unmatched `Live` path inherits
   the id of the old row at its index, if no earlier step consumed that id.
   Indices count every spec and every old row, dangling ones included, so a
   path edited in place lines up with its old row even when a dangling row
   sits above it. Handles editing a row's path in place: the wire follows.
5. **Mint** (pass 3). Anything still without an id takes `next_row_id`.
6. **Literals** (`record_set` only). Each live row keeps the literal of the
   old row whose id it inherited, but only if the literal still converts to
   the row's leaf type.

The passes must run separately; `switch`'s doc explains why a single pass
steals ids (`[1, 2] → [3, 1]`).

Two live paths are equal (`same_key`) when their segments match by
`field_id` where both sides have one, and by name otherwise. An old dangling
row that no `Dangling` spec claimed takes part in passes 3–4 like any other:
a `Live` path matching it by key revives its id. Old rows claimed by nothing
are removed, dangling or not — that is how the panel's delete button on a
broken row works.

### Schema change

Picking a different `schema` re-resolves each row **by name** against the
new def (field ids are per-def, so ids from the old def mean nothing there)
and rewrites the segments' `field_id`s and stored names. The names used are
the row's **current** names: `resolve_path` against the *old* schema, which
still exists during the edit. They are not the stored `PathSegment::name`s,
which go stale on renames — after `Point.x` → `px`, a row's stored name is
still `x`, and resolving `x` against the new schema would wrongly dangle the
row or bind it to some other field that happens to be called `x`. Only a row
that is already dangling under the old schema falls back to its stored
names. Rows keep their ids. Rows whose names don't resolve become dangling.
This is an explicit user edit, so it goes through the same undoable command
as a row edit.

The by-name callers get this for free: the serializer and the getter emit
current names, so a text block or API call that changes `schema` and
re-sends the paths already carries them.

### Undo

Row edits and schema changes drop or remap wires, which a node-data snapshot
cannot restore. Use the existing **`NodeStructureEditCommand`**
(`undo/commands/node_structure_edit.rs`, shared by `zip_with` and `switch`),
with descriptions "Edit record_get paths" / "Edit record_set paths".
`StructureDesigner::set_record_get_data(scope_path, node_id, schema, paths:
Vec<PathSpec>)` / `set_record_set_data(…)` follow `set_switch_data` as
written: a read-only pre-check that runs the whole edit on a clone, a no-op
check, `snapshot_network` before, mutate the node data, then
`finish_node_structure_edit` (repair, validate, refresh, snapshot after,
push). Setting a single
literal is a pure node-data edit (no wire fallout). Like
`set_record_construct_literal`, it is an **API-only** setter that clones the
node data, changes the row's literal and goes through
`set_node_network_data_scoped`, which brings the ordinary `SetNodeData`
undo with it. There is no `StructureDesigner` op for it.

The text path pushes nothing of its own (`TextEditNetworkCommand` covers it).

## Dangling rows

A row whose path stops resolving — a field on the path deleted, an inner
field retyped away from a record, the schema def deleted — stays in the node
data and gets no pin. Repair then drops its wire exactly as it drops a deleted
field's wire on `record_construct` / `record_destructure`.

The node carries one **non-blocking** warning per dangling row
(`ValidationError::warning`), for example "path `p.x` no longer exists in
`Box`", built from the stored segment names. `NodeData::get_data_error`
can't produce it, because it gets no registry and can't resolve a path. So
the warning comes from `network_validator`, through the hook method
`record_node_problems` (see "Registry and repair integration"). The node's
other rows keep working, which is why the warning doesn't block the node
(`structure_designer/AGENTS.md`, "is the node's output still useful?").

A dangling row never resolves again by itself: a re-added field has a new
`FieldId`, and that is the point of field identity. The panel shows the row
as broken, with a delete button. Editing *other* rows leaves it alone: the
panel and the text format send it back as a `Dangling` spec, and `merge_rows`
keeps it verbatim (see "Row editing"). Undoing the def edit that dangled it
brings everything back, pins and wires: `UpdateRecordTypeDefCommand` restores
every affected network from a snapshot taken before the edit.

A def that is missing because its **library mount didn't load** is not a
deletion, and does not dangle a row: the node freezes instead (see "Frozen
nodes and nested paths"). The validator pass skips frozen nodes, which
already get their one frozen-node error from `validate_zones_recursive`.

Retyping only the **leaf** is not dangling: the pin is re-typed, and repair
drops the wire if it no longer fits.

## Registry and repair integration

### Today: name-keyed special cases

The three existing record nodes get their pins from the registry, not from
their own data, so `calculate_custom_node_type` can't build them. Instead,
six places list them by name with `downcast_ref` chains, or reach them
through a function that does:

- `NodeTypeRegistry::populate_custom_node_type_cache_with_types`
- `NodeTypeRegistry::resolve_drag_candidate_type`
- `library_links::record_node_schema` (the frozen-node rule)
- `library_links::recorded_layout`
- `collect_record_refs_in_node` (record-def rename, the namespace-delete
  reference check, cross-document paste, and `invariants.rs`'s
  unresolved-schema check, which goes through it)
- `rewrite_record_names_in_node`

(`canonicalize.rs` also mentions them, but only in a comment: schema names
are strings, so there is nothing to canonicalize.)

Adding two arms to each would grow every list from three to five. Leaving
one out isn't caught by the compiler, and some misses do damage: for
example, `recorded_layout` ends in
`_ => product::build_node_type_for_target_with_defs(…)`, so a record node it
doesn't know about silently gets **`product`'s pin layout**.

### Phase 0: a `NodeData` hook

Before either node exists, move the three existing nodes onto a trait hook
and make every site above generic:

```rust
// node_data.rs
/// For nodes whose pins come from a record def rather than from their own
/// data (`record_construct`, `record_destructure`, `product`,
/// `record_get`, `record_set`). The default (`None`) means "not a record
/// node".
fn record_schema_ref(&self) -> Option<&str> { None }

/// Rewrites the schema name (record-def rename, namespace move,
/// cross-document paste). Called only when `record_schema_ref` is `Some`.
fn set_record_schema_ref(&mut self, _name: String) {}

/// Builds the node's pins from the defs. `None` only under
/// `MissingDefs::Refuse` when a def the node needs is missing (see
/// "Frozen nodes and nested paths"); the caller then keeps whatever layout
/// the node has.
fn build_record_node_type(
    &self,
    _base: &NodeType,
    _defs: RecordDefs<'_>,
    _missing: MissingDefs,
) -> Option<NodeType> {
    None
}

/// Per-node warnings that need the defs to compute (one per dangling row).
/// Read by `network_validator`. Only `record_get` / `record_set` return any.
fn record_node_problems(&self, _defs: RecordDefs<'_>) -> Vec<String> {
    Vec::new()
}

/// The names of the defs, other than the schema, that the node's rows walk
/// into — including a def that is missing (the name a `DefMissing` carries),
/// where that row's walk stops. Read by `library_links::unresolved_mount_ref`
/// so that a nested def under a missing mount freezes the node. Only
/// `record_get` / `record_set` return any.
fn record_path_defs(&self, _defs: RecordDefs<'_>) -> Vec<String> {
    Vec::new()
}

pub enum MissingDefs {
    /// A live node: a missing def is a dangling row (or an empty layout,
    /// for the three existing nodes, as today). A def under an unloaded
    /// library mount never gets here: the node is frozen instead.
    Dangle,
    /// A frozen node being rebuilt from a recorded interface: refuse.
    Refuse,
}
```

| Site | After Phase 0 |
|---|---|
| `populate_custom_node_type_cache_with_types` | `build_record_node_type(…, Dangle)`; if it returns `Some`, install it; otherwise the usual `calculate_custom_node_type` path |
| `resolve_drag_candidate_type` | the same call |
| `record_node_schema` | `node.data.record_schema_ref()` |
| `recorded_layout` | builds a `RecordDefs` from the live defs overlaid with the recorded entries (see "Frozen nodes and nested paths") and calls `build_record_node_type(…, Refuse)` — the `_ =>` fallthrough is gone |
| `collect_record_refs_in_node` / `rewrite_record_names_in_node` | a `Schema` site from `record_schema_ref` / `set_record_schema_ref` (so `invariants.rs` follows with no change of its own) |
| `unresolved_mount_ref` | additionally checks every name from `record_path_defs(registry.record_defs())` against `name_under_mount_unresolved_record` (empty for the three existing nodes, so no behavior change) |

`record_node_problems` has no existing caller. It arrives with Phase 1 as a
new pass in `network_validator`.

`record_path_defs` is deliberately **not** folded into
`collect_record_refs_in_node`: that function has no defs to walk paths with,
and its other callers (rename, reference check, paste) only care about the
schema — a nested def is reached through the schema's own field types, which
those passes already see on the def.

`canonicalize.rs` needs nothing. The node-name string checks in
`library_refresh` (`"record_destructure"`) are about output reconciliation by
`FieldId`, which is particular to that one node. They stay.

After Phase 0, `record_get` and `record_set` need **no** arm anywhere: they
implement the hook's methods, and that's all.

### Frozen nodes and nested paths

Today a record node is frozen (`doc/design_library_linking.md` §8) only when
its **schema** is under a library mount that didn't load, and
`recorded_layout` rebuilds it from the recorded interface of that **one**
def. A nested path breaks both halves of that:

- **Detection.** A `record_get` on a *local* `Box` whose field
  `p: lib::Point` comes from a missing mount has a resolvable schema, so the
  node isn't frozen. Built with `MissingDefs::Dangle`, row `p.x` hits
  `DefMissing`, dangles, and repair drops its wire. Saving then loses the
  wire for good, although nothing was deleted — the library is just missing.
  `record_destructure` never had this problem, since it never reaches into a
  nested def.
- **Layout.** A `record_get` on a linked `Box` with path `p.x` also needs
  `Point`. Built from `Box` alone, every nested row would look dangling and
  lose its pin.

Rule:

- **A nested def under a missing mount freezes the node**, exactly as the
  schema does. `unresolved_mount_ref` checks the names from
  `record_path_defs` (Phase 0) as well as the schema, so such a node lands in
  repair's `frozen` set: its arguments are never touched, and its output
  wires are skipped by the R3 remap and the slot-index check. No wire is
  dropped.
- **`recorded_layout` builds its `RecordDefs` from the live defs overlaid
  with the recorded interface of every def under an unresolved mount** that
  the `uses` table records, not just the schema's. This covers both a linked
  schema with linked nested defs and a local schema with a linked nested def
  (so the `schema != name → None` early return goes away for record nodes:
  the unresolved name may be a nested def's).
- It calls the builder with `MissingDefs::Refuse`. A row whose
  `resolve_path` reports `DefMissing` makes the builder return `None`;
  `FieldGone` and `NotARecord` still give dangling rows, because the
  recorded interface really does lack that field.
- On `None`, `install_recorded_layout` returns `false`, and the frozen
  branch keeps the cached layout, which is the existing fallback. If there
  is no cached layout yet, the frozen branch builds one as it does today;
  a nested row may then lack its pin, but the node is frozen, so its wires
  are kept and come back with their pins when the library loads.

A live node is always built with `MissingDefs::Dangle`, and with the rule
above a live node's `DefMissing` can only be a real deletion (a def not
under any unresolved mount), so its rows dangle.

### Other places that need nothing

- `rekey_record_field_literals` (re-keys literals on a field rename) needs
  **no** arm: `record_set` literals are stored per row id, not per field
  name. That is deliberate, so nobody should "fix" it by adding one.
- `library_refresh` reconciles `record_destructure` outputs and
  `record_construct` inputs by `FieldId` across a library change. The new
  nodes' pins carry **row ids**, which are local to the node, so there is
  nothing to reconcile. The paths' `FieldId`s point into the library's def,
  and `claim_field_ids` keeps a file's field ids unique across its history.
  So a field the library renamed keeps resolving, and a field it deleted and
  re-added gets a new id and the row goes dangling. Phase 1 tests both.

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
- A **dangling row** is written with a leading `!` and its stored names:
  `paths: ["!p.x", "size"]`, parsed as `PathSpec::Dangling`. A first segment
  whose name itself starts with `!` is written in backticks
  (`` "`!a`.b" ``), so the marker is unambiguous. This is what keeps a
  `query` → `--replace` round trip of a node with a dangling row a no-op;
  without it the row could only be rejected or silently deleted. A dangling
  `record_set` row has no pin, so its literal is not printed; it survives
  because `merge_rows` keeps the claimed row verbatim.
- `record_set` literals are emitted by `get_text_properties` under the row's
  **derived pin name** (`p_x: 3.0`), so they read like any pin literal. The
  text editor accepts a key as a literal when it is in
  `get_text_properties`'s list (`apply_literal_properties`), so these keys
  are accepted. `set_text_properties` must apply `schema`, then `paths`, and
  only then resolve the literal keys against the **new** derived names: one
  text block can change all three.
- A row that is wired **and** has a stored literal prints only the wire: the
  serializer leaves out a stored property whose pin is connected. A
  `--replace` round trip therefore drops that hidden literal. This is the
  existing behavior for every pin-backed property, and the doc accepts it.
- A key that names neither a property nor a current row pin gets the
  existing "Unknown property" warning.

## Serialization and healing

`generic_node_data_saver` / `loader`. These are new node types, so there is no
`.cnnd` migration. Loader healing, as in `switch`:

- a missing or zero `next_row_id` → max(row ids)+1;
- a row loaded with `id: None` gets a fresh id;
- duplicate paths: later duplicates are dropped. The loader has no defs, so
  "duplicate" means identical stored segments (`PathSegment`'s `PartialEq`,
  ids and names), which is never a false positive;
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

## Testing conventions

All new tests live in the owning crate's `tests/` directory, never inline.
Node-level tests go in
`crates/atomcad-structure-designer/tests/structure_designer/` and must be
registered as a `#[path = "structure_designer/<file>.rs"] mod <file>;` entry
in `tests/structure_designer.rs`, or they silently never run. API-level tests
go in the root package's `rust/tests/structure_designer_api/`. `.cnnd`
round-trip tests live in the node test files and are named `*_cnnd_roundtrip`,
so `cargo test cnnd_roundtrip` selects them (there is no single
`cnnd_roundtrip_test.rs`). Each test below is in the phase that builds what
it checks; a phase's gate runs everything from the earlier phases too.

## Phase 0 — Refactors (no behavior change except one bug fix)

**Deliverables**

- `RecordDefs<'a>` in `node_type_registry.rs` (a `Copy` view, `lookup`,
  the optional overlay, `NodeTypeRegistry::record_defs()`). The three existing
  `build_node_type_for_schema_with_defs` / `build_node_type_for_target_with_defs`
  functions take it instead of the two maps.
- The `NodeData` hook (`record_schema_ref`, `set_record_schema_ref`,
  `build_record_node_type` with `MissingDefs`, and `record_node_problems` /
  `record_path_defs` with their empty defaults), implemented by `record_construct`,
  `record_destructure` and `product`. Every site in the "After Phase 0"
  table switches to the hook, and `recorded_layout`'s `_ =>` fallthrough
  goes away.
- `nodes/variadic_ids.rs::merge_ids_by_key`, with `SwitchData::merge_cases`
  rewritten on top of it.
- `wired_or_literal`, the shared literal-precedence helper, used by
  `record_construct::eval`.
- `Node::set_custom_node_type` remaps `function_pin_roles` alongside
  `arguments` (see "Node shape", `record_set`). This is a behavior change,
  but only a fix: today a role on a `switch` case pin lands on the wrong pin
  after a case-list edit.
- `unresolved_mount_ref` consults the new `record_path_defs` hook, and
  `recorded_layout` builds its defs as live defs overlaid with the recorded
  interfaces (see "Frozen nodes and nested paths"). Both are no-ops for the
  three existing nodes.
- `nodes/AGENTS.md`, **Records** bullet: a record node is one that
  implements the hook, and no site may list record nodes by name.
  `structure_designer/AGENTS.md`'s "keep the downcast lists in sync" notes
  (on `collect_record_refs_in_node` and friends) shrink accordingly.

**Automated tests.** The existing suites are the gate: record types
(`record_types_*`, `record_field_identity_undo_test.rs`), `switch_test.rs`,
`zip_with_test.rs`, the function-pin suites (`function_pin_test.rs`,
`text_format_pin_roles_test.rs`), the library-linking suites
(`library_links_*`, including the fuzz harness), `clipboard_test.rs`, and the
node snapshots, all unchanged, except where test 3 below pins the role fix.
Add:

1. A frozen `record_destructure` / `product` still gets its own layout from
   `recorded_layout`. This pins the fallthrough bug that the refactor
   removes.
2. A test that greps the crate sources for downcasts to
   `RecordConstructData`, `RecordDestructureData` and `ProductData` outside
   their own files, and fails if it finds any. It has one allowed exception:
   `rekey_record_field_literals`, which edits `record_construct`'s
   name-keyed literals and is not a record-node list. The API layer is a
   different crate and isn't scanned. The precedent for a source-scanning
   test is the renderer's `shader_test.rs`, which walks
   `env!("CARGO_MANIFEST_DIR")/src`. This keeps the name lists from coming
   back.
3. Function-pin roles follow their pin: on a `switch` with cases `[1, 2, 3]`
   and a `Delayed` role on case `2`'s pin, removing case `1` moves the role
   with case `2`; removing case `2` drops it. A `zip_with` lane-type edit
   that removes a lane drops that lane's role. Undo restores the roles.
4. `merge_ids_by_key` directly, beside the `switch` tests that now cover it
   indirectly: key match, positional fallback, mint, the `[1, 2] → [3, 1]`
   no-steal case, and that `next_id` only grows.

**Gate:** `cd rust && cargo fmt && cargo clippy && cargo test -j 4`, plus the
`structure_designer_api` and `integration` harnesses explicitly.

## Phase 1 — Path core + `record_get`

**Deliverables**

- `record_path.rs` (crate root): `PathSegment`, `resolve_path` with
  `DanglingReason`, `resolve_names`, `PathSpec`, and path parsing and
  formatting for the text format (backtick quoting, the `!` marker).
- `nodes/record_path_rows.rs`: `RecordPathRow`, `derived_pin_names` (dedup;
  the reserved-name list is a parameter, empty for `record_get`), and the
  row-to-pins builder.
- The `network_validator` pass that turns `record_node_problems` into
  non-blocking warnings, skipping frozen nodes.
- `NetworkResult::extract_record_path`.
- `nodes/record_get.rs`: `RecordGetData`, `NodeData` impl (the Phase 0 hook,
  `record_path_defs`, `eval`, `get_subtitle`, `adapt_for_drag_source`, a
  minimal `get/set_text_properties` with `schema` and `paths`), and the
  placeholder pin. The minimal setter **replaces the rows wholesale**: it
  resolves each `Live` path with `resolve_names`, mints fresh ids, and
  rejects bad paths and any `!` (`Dangling`) spec. That is enough for the
  tests below to build nodes through the text format; `merge_rows` replaces
  it in Phase 3. The getter already prints dangling rows as `!path`, so a
  node with a dangling row serializes; parsing that back arrives in
  Phase 3.
- Registration (`nodes/mod.rs`, `create_built_in_node_types`). No other
  integration sites, thanks to Phase 0.
- Re-bless node-type snapshots; update registry count/list assertions.

**As implemented** (two things this document did not anticipate):

- `NodeData::get_text_properties` / `set_text_properties` get no registry, so
  they cannot print current names or resolve paths. Phase 1 added
  `get_text_properties_with_defs` / `set_text_properties_with_defs` (taking a
  `RecordDefs`, defaulting to the defs-free pair), and the serializer and the
  editor call those. Phase 2/3 setters belong there too.
- A row through a **missing** def (as opposed to a field that is gone) is
  written as a live path, not `!path`: on a frozen node the def is only
  unavailable, and the node must survive `query` → `--replace`. The setter
  resolves such a path with `resolve_names_allowing_missing_defs`, keeping the
  segments from the missing def on as name-only (`field_id: None`), which
  resolve by name once the def is back. A live node whose def was really
  deleted therefore also prints its rows without `!`.

**Automated tests** — new
`crates/atomcad-structure-designer/tests/structure_designer/record_get_test.rs`:

1. Top-level and two-level paths read the right values; output pins are
   named `size`, `p_x`.
2. `None` / `Error` input → `None` / that error on every pin.
3. An `Optional[T]` leaf: a plain `T` pin, and an unset value passes `None`.
4. A path through an **anonymous** record type inside a named def resolves
   by name.
5. Pin-name dedup: path `a.b` beside top-level field `a_b` gives `a_b` and
   `a_b__2`. A field named `schema` keeps its name on `record_get` (nothing
   is reserved on outputs).
6. Field rename on the **nested** def (`Point.x` → `Point.px`): the pin
   renames to `p_px`, and the downstream wire survives.
7. Field reorder in the nested def: no change to pins or wires. Field delete
   → dangling row, no pin, one **non-blocking** warning naming the path, and
   the downstream wire is dropped; the node's other rows still evaluate.
   Undoing the def edit restores pin and wire. Deleting the schema def
   makes every row dangling and leaves the placeholder pin. Retyping an
   inner field away from a record (`Box.p: Point` → `Float`) dangles `p.x`
   (`NotARecord`) but leaves `size` alone.
8. Output-pin id remap (the R3 remap reaches `record_get`). Rows
   `[p.x, size]`, both wired downstream. Delete `Point.x`: `size` moves from
   pin 1 to pin 0 and its wire follows. (A control case, rows
   `[size, p.x]`: `size` stays on pin 0.)
9. Def rename and namespace move rewrite `schema`; deleting a def referenced
   only by a `record_get` is caught by the reference check. In
   `clipboard_test.rs`: a cross-document paste of a `record_get` on `Box`
   with row `p.x` resolves to the same fields in the target document when
   its `Box` has the same interface, and is refused when it does not.
10. A row in a **zone body** survives a def edit (the `walk_all_nodes` class).
11. Library linking: a renamed library field keeps the path; a deleted and
    re-added one makes the row dangling; the schema under a missing mount
    freezes the node.
12. Frozen node with a nested path: with the library missing, a
    `record_get` on a linked `Box` with row `p.x` keeps its pin and its
    downstream wire. Test it both with `Point` recorded in `uses` and
    without (the cached-layout fallback).
13. Nested def under a missing mount, **local** schema: a local `Box` with
    `p: lib::Point`, a `record_get` on `Box` with rows `[p.x, size]`, both
    wired downstream. Load with the library missing: the node is frozen (no
    dangling-row warning), both wires survive, and saving and reloading with
    the library back restores both pins with their wires. A control case: a
    *local* `Point` that is really deleted dangles `p.x` and drops its
    wire.
14. `extract_record_path`: a hit, a miss part-way down, and a non-record
    part-way down.
15. Drag-aware add from a `Record(Named)` output produces a `record_get` with
    the schema set; from an anonymous record output, no schema.
16. Path text syntax, directly on `record_path.rs`: parse and format round
    trip for plain paths, a segment containing `.` (backticks), a first
    segment starting with `!` (backticks), and the `!` dangling marker;
    malformed input (unclosed backtick, empty segment) is an error.
    `resolve_names` returns the depth of the first name that fails;
    `resolve_path` returns each of the three `DanglingReason`s.

**Gate:** `cd rust && cargo fmt && cargo clippy && cargo test -j 4` (and the
`structure_designer_api` / `integration` harnesses explicitly).

## Phase 2 — `record_set`

**Deliverables**

- `nodes/record_set.rs`: `RecordSetData` / `RecordSetRow`, `NodeData` impl
  including the Phase 0 hook.
- `NetworkResult::with_record_path_replaced`.
- `eval` as specified under "Runtime semantics", using `wired_or_literal`.
- Registration (`nodes/mod.rs`, `create_built_in_node_types`); re-bless
  node-type snapshots; update registry count/list assertions.

**As implemented:**

- **A leaf retype keeps an incompatible wire and flags it**; it does not drop
  it, contrary to test 8 below and to "Dangling rows" above. That is the
  codebase's rule for every record node: `record_construct`'s incompatible
  retype is pinned by
  `record_field_rename_wire_loss_test.rs::guard_incompatible_retype_preserves_wire_but_flags_error`
  (a blocking type error on the node; the user fixes the wire). Test 8 checks
  that instead.
- The text properties are the Phase 1 minimum, as on `record_get`: `schema`
  and `paths` (rows replaced wholesale, no literals). Literal keys arrive in
  Phase 3, so the literal tests set `RecordSetRow::literal` on the node data
  directly. Phase 3 heads-up: `apply_literal_properties` computes the known
  keys (`text_prop_names`) from the node's data *before* the block is applied,
  so a block that creates the rows and sets `p_x: 3.0` in one go will likely
  get a spurious "Unknown property" warning (the literal is still passed on).
- The row helpers in `record_path_rows.rs` take any `R: AsRef<RecordPathRow>`,
  so `record_set` passes its `RecordSetRow`s without copying.
  `record_set::RESERVED_PIN_NAMES` is the reserved list (`record`, `schema`,
  `paths`, `visible`, `on`, `pin_roles`).
- `with_record_path_replaced`'s `Err(depth)` names the value at
  `path[..depth]` that is missing or not a record (0 = the input itself).

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
7. Function pin: `record_set`'s `-1` into `map.f` updates every element of
   an `Array[Box]`, with the result typed `Iter[Box]`. Wired rows are
   captured. With one row left unwired, `map` auto-partializes it, and the
   output is `Iter` of a one-parameter function (`function_pin_dispositions`,
   `doc/design_currying.md`).
8. Leaf retype in the def: the pin is retyped, and an incompatible wire is
   dropped by repair.
9. The tests from Phase 1 that apply to input pins: nested rename keeps the
   wire (by `Parameter.id`); a nested delete gives a dangling row; a frozen
   `record_set` with a nested row keeps its input wire, including the
   local-schema case of Phase 1 test 13.
10. `with_record_path_replaced`: replace at depth 1 and 2, insert a missing
    leaf, `Err(depth)` on a missing middle record; sibling and extra fields
    are untouched.
11. Reserved names: rows for fields named `record`, `schema` and `visible`
    get pins `record__2`, `schema__2`, `visible__2`.
12. Function-pin roles on row pins (the case that motivated the Phase 0
    fix): rows `[p.x, size]`, a `Delayed` role on `size`. Delete `Point.x`:
    the role stays on `size`, now at pin 1 instead of 2, and `map.f`'s
    synthesized function still takes `size` as a parameter.
13. Drag-aware add produces a `record_set` with the schema set from either
    direction (dragging off a record output, or from a record input).

**Gate:** as Phase 1.

## Phase 3 — Row editing, undo, text format, serialization

**Deliverables**

- `record_path_rows::merge_rows` (a wrapper over `merge_ids_by_key`), and
  schema-change re-resolution.
- `StructureDesigner::set_record_get_data` / `set_record_set_data`, both
  taking `scope_path`. (The literal setter is API-only and comes in
  Phase 4.)
- Full `get/set_text_properties` (the `schema` → `paths` → literals order),
  loader healing.

**Automated tests** — the two node test files (row editing, the
`*_cnnd_roundtrip` tests), plus `undo_test.rs`, `text_format_test.rs` and the
round-trip corpus:

1. Remove the middle row of three: the survivors' wires follow their ids,
   and the removed row's wire drops. This must be checked on `record_get`
   **outputs** as well as `record_set` inputs, through the
   `StructureDesigner` setters. Undo restores rows and wires exactly, redo
   reapplies the edit, and re-sending the unchanged rows pushes no command.
2. Edit a row's path in place: the wire survives (positional fallback); a
   `record_set` literal survives if it still converts and is dropped if not.
3. No positional steal: `[a, b] → [c, a]` keeps `a`'s wire.
4. Removing the highest-id row and adding one doesn't reuse the id.
5. Duplicate paths, empty paths and names that don't resolve are rejected;
   the node is unchanged.
6. Schema change: rows re-resolve by current name and keep their ids;
   non-resolving rows go dangling. Undo restores the old schema, rows and
   wires exactly (`normalize_json`), including for a body-internal node.
   Stale names: rename `Point.x` → `px`, then switch the schema to a `Box2`
   whose `p` has fields `px` and `x`. The row binds to `px`, not `x`.
7. Dangling rows in by-name edits. With rows `[p.x, size]` and `p.x`
   dangling: re-sending `["!p.x", "size", "p.y"]` adds `p.y` and leaves the
   dangling row and `size`'s wire untouched (and a `record_set`'s dangling
   literal survives); re-sending `["size"]` deletes the dangling row; a
   `"!q"` naming no dangling row is dropped with a warning. A `Live` path
   that revives a dangling row by key keeps its id.
8. Text: parse the canonical example and the wires land correctly; the
   serialize → `--replace` → serialize round trip is byte-equal, including a
   dedup-suffixed name, a backtick-quoted segment, a `` `!a` `` segment and
   a **dangling row** (`"!p.x"`); one block that changes `schema`, `paths`
   and a literal applies all three. A wired row with a stored literal prints
   only the wire.
9. `.cnnd` round-trip with wired rows, a literal and a dangling row; healing
   of id-less rows, a zero counter and duplicate paths.
10. Add the canonical example to the text-format round-trip corpus (a new
    `.cnnd` fixture → two corpus snapshots; use `add_value_node` /
    `add_typed_value_node`).
11. Both `StructureDesigner` setters on a **body-internal** node through
    `scope_path`: the edit lands in the body, and undo restores it.

**Gate:** as Phase 1. The full `cargo test -j 4` already includes the
`cnnd_roundtrip` and `node_snapshots` tests; if a snapshot changes, review it
with `cargo insta review` rather than blessing blindly.

## Phase 4 — API, Flutter UI, documentation

**Deliverables**

- API (`rust/src/api/structure_designer/`): `get_record_get_data` /
  `set_record_get_data`, `get_record_set_data` / `set_record_set_data`, and
  a literal set/clear pair, all `#[frb(sync)]` and all taking **`scope_path`**.
  Paths cross the API as `APIRecordPath { names: Vec<String>, dangling:
  bool }`, the API form of `PathSpec`. The getter reports each row with its
  current names (stored names for a dangling row), its derived pin name, its
  leaf type, and, for a dangling row, `dangling: true` and the reason. The
  setter takes the same struct, so the panel edits rows by sending the list
  it got back with its change applied: a dangling row it re-sends is kept, one
  it leaves out is deleted. Ids never cross the API, and the path setters
  wrap the Phase 3 ops. The literal pair copies `set_record_construct_literal` /
  `clear_record_construct_literal` (`set_node_network_data_scoped`, so the
  ordinary `SetNodeData` undo comes with it), keyed by row index. Each
  `record_set` row reports its literal part as an **`APILiteralField`** built
  with the existing `text_value_to_api_literal` / `data_type_to_simple_param_type`
  helpers, so the Flutter side can use the existing literal editor as it is.
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
- `nodes/AGENTS.md`: add both nodes to the **Records** bullet.
  `structure_designer/AGENTS.md` (Record Type Defs): name `record_path.rs`
  as the one path resolver and `extract_record_path` /
  `with_record_path_replaced` as the runtime pair.

**Automated tests** — new
`rust/tests/structure_designer_api/record_get_set_api_test.rs`, registered in
`rust/tests/structure_designer_api.rs`:

1. The getters report each row's current names, derived pin name and leaf
   type; a dangling row comes back with `dangling: true`, its stored names
   and its reason.
2. Echo is a no-op: feeding a getter's rows straight back to the setter,
   with a dangling row among them, changes nothing and pushes no undo
   command. Leaving the dangling row out deletes it.
3. The literal pair, keyed by row index: set, then clear, each undoable
   (`SetNodeData`); a literal that doesn't convert to the row's leaf type is
   rejected. The row's `APILiteralField` reflects the stored / unset / wired
   state.
4. All of the above on a **body-internal** node through `scope_path`.

**Gate:** `cargo fmt`, then `flutter_rust_bridge_codegen generate`, then
`cargo clippy && cargo test -j 4` (with the `structure_designer_api` harness
explicitly), `flutter analyze` (no new issues over the baseline), and
`dart format` on the touched files.

**Manual checks**

1. The Flutter editors are thin UI over the tested API and are checked by
   hand, not by widget tests.
2. Manual walkthrough (the maintainer's, together with the Flutter smoke
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
