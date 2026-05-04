# Record Types Design

## Status: Draft

## Motivation

atomCAD's node networks pass structured data through typed pins, but every value today is either a primitive (`Int`, `Float`, `Vec3`, …), a domain object (`Blueprint`, `Crystal`, …), or a homogeneous `Array[T]`. There is no way to bundle a small set of named, heterogeneously-typed values into a single value that travels along one wire.

Concrete motivating use case: a `product` higher-order node that takes N input arrays and emits the cartesian product as `Array[Record(Foo)]` where `Foo` is a user-declared record type. This is unbuildable today — the only multi-valued output is `Array[T]`, which forces all elements to share a type. More broadly, records let users compose structured payloads (e.g. defect descriptors, parameter bundles, intermediate results in a fold) without having to fan them out into N parallel parameter pins or N parallel arrays.

Two design tenets shape every decision below:

- **Records are structurally subtyped.** Compatibility between two record types is computed from their field schemas, not from their names. Two record types with different names but compatible schemas are interchangeable for assignability.
- **Records are extensible (width subtyping).** A record value with fields `{x: Int, y: Int, z: Int}` is assignable to a pin declared `{x: Int, y: Int}`. The extra field `z` rides along at runtime; the consumer is just contractually allowed to read fewer fields than the producer carries.

Records can be **named** (declared in the project's top-level type table) or **anonymous** (no name). The user creates and edits named record defs from the same UI list as custom node networks. Anonymous records arise inside expression-language code where there is no natural name to attach (e.g. an `expr` literal `{x: 1, y: 2}` produces an anonymous record value; the `expr` node's inferred output type is then an anonymous record type). Anonymous records are not user-declared at the project level — they only exist as the type of a value or expression.

Why have names at all when typing is structural? Names are not part of the type system — compatibility is decided by field shape — but they pay for themselves three other ways:

- **User cognition.** Thinking about and talking about "a `Point`" is easier than reading and re-reading `{x: Int, y: Int}` every time the same shape appears. A name turns a recurring schema into a noun the user (and reviewers, and documentation) can refer to.
- **Simpler UI.** When a record type is needed on a node property or pin, picking from a dropdown of named defs is materially less work than building the schema inline every time. Most users will name the handful of records they reuse and never touch the inline form.
- **Bulk edits via cascading propagation.** When the user changes a named def's schema (renames a field, retypes one, adds a new one), every reference in every network in the project sees the new schema immediately — node properties, pin types, custom-network interfaces — and incompatible wires are automatically repaired. This works because each `DataType::Record` instance references the def by name; resolving the reference returns the current schema. Anonymous records have no equivalent — editing one schema doesn't touch any other — which is exactly right when the schemas were never meant to be the same thing in the first place.

Naming is therefore a UX and tooling concession layered on top of a structurally-typed core, not a constraint on the type system itself.

## Design Principles

1. **`DataType::Record` is either a name reference or an inline anonymous schema, never both.** Named records carry only the def's name; the schema is resolved via the registry on demand. Anonymous records (e.g. expr literal types) carry their fields inline. A rename still walks every `DataType` to rewrite the old name to the new (the same fix-up pattern that handles renaming a node network), but field-level edits to a def need no propagation — every reference resolves to the current schema automatically.
2. **Names don't gate compatibility.** Subtyping is purely structural. Two named records `Foo` and `Bar` with compatible schemas are assignable in either direction up to the width-subtyping rule below.
3. **Width + structural depth subtyping; only tag-only widenings at field level.** `Record(R1) <: Record(R2)` iff every field declared in `R2`'s schema is present in `R1`'s schema with a structurally-compatible type. Depth recursion preserves width subtyping inside nested records and element-wise compatibility inside arrays. At each leaf field position only **tag-only widenings** are accepted — exact equality, plus the concrete-to-abstract phase upcasts (`Crystal → HasAtoms`, `Molecule → HasFreeLinOps`, …) that require no runtime value conversion. **Value-converting widenings** (`Int ↔ Float`, `IVec* ↔ Vec*`, `IMat3 ↔ Mat3`, `LatticeVecs → DrawingPlane`) are *not* applied inside record fields, at any nesting depth. Rationale: pass-through (principle 5) requires that a destructure read the runtime payload as-is; value-converting widenings would force a per-field coercion, while tag-only widenings need none (the runtime variant already satisfies the abstract type — `HasAtoms` never appears at runtime). If a user wants a `Float` field built from an `Int` source, they insert an explicit conversion node before `record_construct`. Value-converting promotion can be added at field level later as a non-breaking extension if the explicit-conversion cost proves painful. The empty record `{}` is the top of this lattice — every record is assignable to it.
4. **No type names at runtime.** `NetworkResult::Record` carries fields only — no name. Names live only on the type side, where they identify the source def for propagation. At evaluation time the name has no role.
5. **Pass-through coercion.** When a value with fields `{x, y, z}` flows into a pin declared `{x, y}`, the runtime value is unchanged. The destination type declares what the consumer is allowed to read, not what the value must contain. (Same precedent as abstract supertypes like `HasAtoms`, which never appear at runtime.)
6. **Canonical field order in anonymous `RecordType` and `NetworkResult::Record`; authored order on `RecordTypeDef`.** Anonymous record types and runtime record values store fields **sorted by name** (canonical form). This makes derived `PartialEq` / `Hash` correct, makes serialization deterministic, and lets subtyping merge two sorted field lists in linear time. The top-level `RecordTypeDef` keeps fields in **authored order** — this is what node pin layouts and the schema editor display. Subtyping against a named record sorts the def's authored fields on demand into a canonical view; records are small enough that this is cheap, and we add memoization only if profiling shows it matters.
7. **No cycles.** A record's fields may contain other record types (nesting is allowed and common — `Box = { p: Point }`), but the dependency graph among named record defs must be acyclic. A def cannot reference itself, directly or transitively. This is validated at edit time, and lets `RecordTypeDef.fields` hold real `DataType` values directly — no stub or forward-declaration scheme is needed to break cycles.
8. **One namespace for user types.** Named record defs and named node networks live in the same name space and the same UI list, both treated as "user-defined types in the project."
9. **Records aren't viewport-displayable.** A pin of record type carries no geometry/atoms; users destructure it before displaying anything.

## Current System Summary

| Concern | Location |
|---|---|
| Type representation | `rust/src/structure_designer/data_type.rs` — single `DataType` enum |
| Wire-time compatibility | `DataType::can_be_converted_to` (data_type.rs:112-202) |
| Pin connection gate | `NodeNetwork::can_connect_nodes` (node_network.rs:525-568) |
| Runtime values | `rust/src/structure_designer/evaluator/network_result.rs` — `NetworkResult` enum |
| Top-level user-defined types | `NodeTypeRegistry::node_networks` (node_type_registry.rs:88-92) |
| .cnnd network serialization | `serialization/node_networks_serialization.rs` |
| Expression language types | `rust/src/expr/expr.rs` — `Expr::validate`, `unify_array_element_types` |
| Parametric node properties | e.g. `array_at::ArrayAtData::calculate_custom_node_type` |
| Custom-network rename propagation | `rename_node_network` and the `repair_node_network` pass — model for record-type propagation |

Records plug into all of the above; nothing is replaced.

## Type System Changes

### `DataType::Record`

```rust
// data_type.rs

pub enum DataType {
    // ... existing variants ...
    Record(RecordType),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecordType {
    /// References a registered record type def by name. The schema is
    /// resolved via `NodeTypeRegistry::record_type_defs` at use time. A
    /// reference whose name is missing from the registry is *dangling* and
    /// is treated as a type error wherever it appears.
    Named(String),

    /// Inline anonymous record (e.g. expr literal type, expr output type).
    /// Fields are in **canonical (sorted-by-name) order**; field names are
    /// distinct. The empty record `{}` is `Anonymous(vec![])`.
    Anonymous(Vec<(String, DataType)>),
}
```

Construction invariants for `Anonymous` (enforced by the constructor):
- Field names within `fields` are distinct.
- Field order is canonical (sorted ascending by field name).

Helpers:

```rust
impl RecordType {
    pub fn named(name: String) -> Self {
        RecordType::Named(name)
    }

    pub fn anonymous(mut fields: Vec<(String, DataType)>) -> Self {
        fields.sort_by(|(a, _), (b, _)| a.cmp(b));
        RecordType::Anonymous(fields)
    }

    /// Resolve to the canonical field schema. For `Named`, looks up the def
    /// in the registry and returns its fields in canonical (sorted) order;
    /// returns `None` if the name is dangling. For `Anonymous`, returns the
    /// inline fields (already canonical).
    pub fn resolve_fields<'a>(
        &'a self,
        registry: &'a NodeTypeRegistry,
    ) -> Option<Cow<'a, [(String, DataType)]>> {
        match self {
            RecordType::Anonymous(fs) => Some(Cow::Borrowed(fs)),
            RecordType::Named(n) => registry.record_type_defs.get(n).map(|def| {
                let mut canonical = def.fields.clone();
                canonical.sort_by(|(a, _), (b, _)| a.cmp(b));
                Cow::Owned(canonical)
            }),
        }
    }
}
```

Because `RecordType::Anonymous` canonicalizes on construction, the derived `PartialEq` and `Hash` impls on `RecordType` are structurally correct out of the box: two anonymous records with the same fields-as-a-set compare equal regardless of how the user authored field order. `NetworkResult::Record` likewise canonicalizes, so its derived `PartialEq` is structural for the same reason (no `Hash` is required on `NetworkResult`). Two `RecordType::Named(n)` values are equal iff `n` matches; `Named` and `Anonymous` are never equal (use `can_be_converted_to` for structural compatibility).

### Subtyping

`can_be_converted_to` takes a `&NodeTypeRegistry` parameter (passed through to recursive calls) so that `Named` references can be resolved. The new record branch:

```rust
(DataType::Record(src), DataType::Record(dst)) => {
    // Same-name short-circuit: two `Named(n)` references resolve to the
    // same def, hence the same fields, by definition.
    if let (RecordType::Named(s), RecordType::Named(d)) = (src, dst) {
        if s == d {
            return true;
        }
    }
    // Resolve both sides to canonical field lists. A dangling reference
    // (missing from the registry) is incompatible with anything.
    let Some(src_fields) = src.resolve_fields(registry) else { return false; };
    let Some(dst_fields) = dst.resolve_fields(registry) else { return false; };
    // Both lists are canonical. Walk dst forward, advancing src by linear merge.
    let mut si = 0;
    for (dst_field, dst_ty) in dst_fields.iter() {
        while si < src_fields.len() && src_fields[si].0.as_str() < dst_field.as_str() {
            si += 1;
        }
        if si == src_fields.len() || src_fields[si].0 != *dst_field {
            return false;  // dst requires a field src doesn't have
        }
        // Strict structural compare on field types — no scalar promotion.
        if !can_be_structurally_converted_to(&src_fields[si].1, dst_ty, registry) {
            return false;
        }
        si += 1;
    }
    true
}
```

The field-level check uses `can_be_structurally_converted_to`, a strict variant of `can_be_converted_to`. It accepts **tag-only widenings** (identity plus concrete-to-abstract phase upcasts) and rejects value-converting widenings (`Int ↔ Float`, `IVec* ↔ Vec*`, `IMat3 ↔ Mat3`, `LatticeVecs → DrawingPlane`), single-value-to-array broadcasting, and function partial application — anything that would force a runtime value conversion at a destructure pin.

To keep the rule in one place, the existing tag-only edges in `data_type.rs` (the `Crystal → HasAtoms`, `Molecule → HasAtoms`, etc. block at `data_type.rs:188-197`) are extracted into a small predicate:

```rust
/// True when `src` widens to `dst` without any runtime value
/// conversion. Today: identity, plus concrete phase types upcasting to
/// their abstract supertypes — these are pure tag-level widenings and
/// the runtime variant doesn't change. Distinct from
/// `can_be_converted_to`, which also accepts value-converting
/// widenings (Int↔Float, IVec3↔Vec3, …).
pub fn is_tag_only_widening(src: &DataType, dst: &DataType) -> bool {
    if src == dst {
        return true;
    }
    matches!(
        (src, dst),
        (DataType::Crystal,   DataType::HasAtoms)
      | (DataType::Crystal,   DataType::HasStructure)
      | (DataType::Molecule,  DataType::HasAtoms)
      | (DataType::Molecule,  DataType::HasFreeLinOps)
      | (DataType::Blueprint, DataType::HasStructure)
      | (DataType::Blueprint, DataType::HasFreeLinOps)
    )
}
```

`can_be_converted_to` is refactored so its abstract-upcast arm calls `is_tag_only_widening` (mechanical change, no behavior delta). The strict variant then reuses the same predicate at leaf positions:

```rust
/// Like `can_be_converted_to`, but at leaf positions accepts only
/// tag-only widenings (identity plus concrete-to-abstract phase
/// upcasts) — never value-converting widenings such as Int→Float or
/// IVec3→Vec3. The no-promotion guarantee is cooperative: the record
/// arm below delegates to `can_be_converted_to`, whose record arm in
/// turn recurses through *this* function for field types. Keep the two
/// record arms in sync — if either side changes its field-level
/// dispatch, scalar promotion can leak into records.
fn can_be_structurally_converted_to(
    src: &DataType,
    dst: &DataType,
    registry: &NodeTypeRegistry,
) -> bool {
    match (src, dst) {
        // Records: same width + depth structural rule as the record arm
        // of `can_be_converted_to` (which itself uses the strict variant
        // for field-level checks, so this is safe to delegate).
        (DataType::Record(_), DataType::Record(_)) => can_be_converted_to(src, dst, registry),
        // Arrays: element-wise, stays strict.
        (DataType::Array(s), DataType::Array(d)) => can_be_structurally_converted_to(s, d, registry),
        // Leaf position: identity + concrete→abstract phase upcasts
        // only. No value-converting widenings, no broadcast, no
        // function partial application.
        _ => is_tag_only_widening(src, dst),
    }
}
```

This delivers **width subtyping** (extra fields on `src` are allowed) and **structural depth subtyping** (each field type checked recursively under the strict no-promotion variant). The linear-merge walk is O(N + M) thanks to canonical ordering. Termination is trivial: the no-cycle invariant means resolving a chain of `Named` references bottoms out, and `DataType` values are finite trees.

Why tag-only widenings only? Pass-through coercion (next section) requires that a destructure read the runtime payload as-is — there is no place in the destructure node to perform a per-field conversion. Allowing `Record({x: Int}) <: Record({x: Float})` would mean a `Float`-declared destructure receives an `Int` runtime value, a real bit-level mismatch the destructure cannot fix. Tag-only widenings sidestep this trap: a `HasAtoms`-declared field receiving a `Crystal` runtime variant is *exactly* the precedent set by top-level `HasAtoms` pins — abstract supertypes never appear at runtime, and the destructure simply hands the concrete variant through. Rejecting value-converting widenings while admitting tag-only ones lets records freely participate in the existing phase-type polymorphism (e.g. a `{ atoms: HasAtoms, label: String }` record built from any of `Crystal`, `Molecule`) without compromising pass-through. If a user wants to feed an `Int` into a `Float` field, they insert an explicit `int_to_float` node before `record_construct`. Field-level value-converting promotion can be added later as a non-breaking extension if the explicit-conversion cost proves painful.

Equality (`==`) is the derived `PartialEq` on `RecordType` — `Named(n) == Named(n)`, anonymous-equal-anonymous by field-set, named never equal to anonymous — and is *not* the relation used at pin-connect time. Pin-connect uses `can_be_converted_to`, which resolves names and applies structural subtyping (so a `Named` and an `Anonymous` with compatible schemas are interchangeable).

Threading `&NodeTypeRegistry` through `can_be_converted_to` is necessary because resolving a `Named` reference requires looking up the def. Most call sites already have access to the registry in surrounding context; the few that don't need it threaded.

### Pass-through, not projection

Runtime values are *never* projected. If a record value with fields `{x, y, z}` flows into a pin declared `Record(Bar)` where `Bar = {x, y}`, the runtime payload is unchanged — it still carries `z`. Consumers (the `record_destructure` node, the `r.x` expression) see only what their declared schema lets them read. This:

- Avoids data loss on intermediate edges.
- Sidesteps the question of how to project through `Array[Record(...)]`.
- Matches the precedent set by `HasAtoms` — declared types and runtime values are not constrained to be identical.

`infer_data_type(value)` for a record value returns `DataType::Record(RecordType::Anonymous(fields))` — an anonymous schema reflecting the value's actual fields. Validation does not compare declared and inferred types directly; it goes through `can_be_converted_to`.

## Runtime Values

```rust
// network_result.rs

pub enum NetworkResult {
    // ... existing variants ...
    Record(Vec<(String, NetworkResult)>),  // canonical (sorted by name); no type name
}
```

Runtime values carry no type name. The name lives only on `DataType::Record`, where it serves the propagation mechanism. Field order is canonical — same rationale as `RecordType.fields`: structural equality of values works under derived `PartialEq`, and serialization is deterministic. The constructor sorts on creation:

```rust
impl NetworkResult {
    pub fn record(mut fields: Vec<(String, NetworkResult)>) -> NetworkResult {
        fields.sort_by(|(a, _), (b, _)| a.cmp(b));
        NetworkResult::Record(fields)
    }

    pub fn extract_record_field(&self, name: &str) -> Option<&NetworkResult> {
        if let NetworkResult::Record(fs) = self {
            fs.binary_search_by(|(n, _)| n.as_str().cmp(name))
                .ok()
                .map(|i| &fs[i].1)
        } else {
            None
        }
    }
}
```

Field lookup uses binary search on the sorted field list. `convert_to` for a record value is identity if subtyping holds (checked statically); the runtime value passes through unchanged. No projection, no copy, no field reordering.

## Top-Level Storage

### Registry

```rust
// node_type_registry.rs

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordTypeDef {
    pub name: String,
    /// **Authored order** — the order the user typed/edited fields in. Drives
    /// the schema editor's display and the input/output pin layout on
    /// `record_construct`, `record_destructure`, and `product`. Distinct from
    /// the canonical (sorted) order used inside `RecordType::Anonymous` and
    /// `NetworkResult::Record`. Subtyping sorts this on demand into a
    /// canonical view.
    pub fields: Vec<(String, DataType)>,
}

pub struct NodeTypeRegistry {
    pub built_in_node_types: HashMap<String, NodeType>,
    pub node_networks: HashMap<String, NodeNetwork>,
    pub record_type_defs: HashMap<String, RecordTypeDef>,  // NEW
    pub design_file_name: Option<String>,
}
```

### Namespace

`record_type_defs` and `node_networks` share a single user-type namespace. Adding either rejects names that collide with the other or with built-in types. The Flutter "user types" panel shows both kinds in one tree, with a kind icon. (UI section below.)

### Operations

Because `RecordType::Named(N)` stores only a name, field-level edits to a def need no propagation — every reference resolves through the registry on every lookup. The only operation that walks `DataType` instances is rename, and it rewrites names, not schemas.

- `add_record_type_def(def)` — validates: name not already taken (against `record_type_defs`, `node_networks`, and built-ins); field names within the def are distinct; **the def's transitive references do not form a cycle** (i.e., its fields do not, directly or via other named records, contain a reference back to itself).
- `delete_record_type_def(name)` — removes the def. Every `RecordType::Named(name)` reference now resolves to `None` (dangling) and is reported as a validation error wherever it appears. Run `repair_node_network` on every network so wires that depended on the resolved schema are disconnected.
- `rename_record_type_def(old, new)` — updates the registry key, then (a) walks every `DataType` (in parameter types, pin types, the return node's output type, anywhere a `DataType` appears) and rewrites `RecordType::Named(old)` to `RecordType::Named(new)` via a single recursive descent over `DataType`; and (b) walks every `record_construct` / `record_destructure` / `product` node and rewrites its `schema`/`target` property string from `old` to `new` (these properties hold a bare name, not an embedded `RecordType`).
- `update_record_type_def(name, new_fields)` — runs the cycle check (`new_fields` and the rest of the registry must remain acyclic); if that passes, replaces the def's field list (preserving the user's authored order). No `DataType` *rewrite* walk is needed — every `Named(name)` reference automatically sees the new schema. A repair walk is still required: run `repair_node_network` on every affected network so `record_construct` / `record_destructure` / `product` re-derive pin layouts and wires now incompatible with the new schema are disconnected.

The rename walk needs to visit:
1. **Node property data** for `record_construct` / `record_destructure` / `product` — each stores the def name as a bare `String` (`schema` or `target`); rewrite the field if it equals `old`.
2. **`DataType` locations**: parameter types, pin types, the return node's output type, and any other `DataType` reachable from the project root — rewrite every embedded `RecordType::Named(old)` to `RecordType::Named(new)`.
3. **Custom network interfaces** (input parameter types and return-node output type) are reached as part of (2).

Computed/derived pin types do not need to be rewritten by the rename pass — they are recomputed on the next type-resolution call from the (now-renamed) inputs.

### Repair on schema change

When a record def's fields change (or it is deleted), `repair_node_network` runs on every affected network:
- `record_destructure` nodes re-derive their output pins from the new schema; wires connected to pins that no longer exist are disconnected (via `repair_output_pin_wires`).
- `record_construct` and `product` nodes re-derive their input pins; wires whose source type no longer satisfies the new field type are disconnected.
- General pin-type compatibility is rechecked across all wires; failing wires are disconnected.

Reuse and extend `repair_node_network`. Evaluation caches are invalidated, so runtime values produced before the change are not observed by downstream consumers post-change.

**Field renames are not specially handled.** A rename `x → xx` is treated as "remove field `x`, add field `xx`" by the repair pass — wires connected to the old `x` pin are disconnected, and the new `xx` pin starts unconnected. The codebase preserves wires across *custom-network parameter* renames via a stable `param_id` (see `repair_call_sites_for_network` in `network_validator.rs`), but record fields have no equivalent identity beyond their name, and adding one would be a sizable layered change (id-tagged fields in `RecordTypeDef`, serialization, schema-editor plumbing) for what is mostly an occasional schema-authoring nuisance. We accept the re-wiring cost; field IDs can be retrofitted later as a non-breaking change if users complain.

### .cnnd Serialization

Add a `record_type_defs` array at the project root, alongside the existing `node_networks`:

```jsonc
{
  "version": <current>,
  "node_networks": [ ... ],
  "record_type_defs": [
    {
      "name": "Point",
      "fields": [
        { "name": "x", "type": "Int" },
        { "name": "y", "type": "Int" }
      ]
    },
    {
      "name": "Box",
      "fields": [
        { "name": "p", "type": { "Record": { "Named": "Point" } } }
      ]
    }
  ],
  ...
}
```

`DataType::Record` serializes as a `RecordType` enum: `{"Named": "Point"}` for named references, `{"Anonymous": [...fields...]}` for inline anonymous schemas. References to named records carry only the name — no schema duplication.

The `record_type_defs` field on the project root uses `#[serde(default)]`, so older .cnnd files without it load with an empty `HashMap`. No version bump and no migration code: this is a purely additive change in the same vein as past additive field additions. (Versioning is reserved for changes that require explicit migration logic.)

On save, the serializer emits `record_type_defs` entries sorted ascending by `name`, so the on-disk JSON is deterministic across saves of the same project despite the in-memory `HashMap` having non-deterministic iteration order. (The example above is shown in pedagogical order — `Point` before its consumer `Box` — for readability; an actual save would emit `Box` first.)

Load order is straightforward: deserialize `record_type_defs` and `node_networks` independently from JSON (both are just sections of the document), then validate. Validation re-runs the cycle check on the registry and walks every `DataType::Record::Named(N)` reference, flagging any whose `N` is missing from `record_type_defs` (defensive against hand-edited files). There is no cached-fields reconciliation step because there are no cached fields.

## No Recursive Definitions

Record types may freely contain other record types as field types — `Box = { p: Point }`, `Triangle = { a: Point, b: Point, c: Point }`, `Group = { items: Array[Item] }` — but the dependency graph among named record defs must be acyclic. A def cannot reference itself, directly or transitively. Concretely:

- ✗ `Tree = { children: Array[Record(Tree)] }` — direct self-reference.
- ✗ `A = { b: Record(B) }`, `B = { a: Record(A) }` — mutually recursive.
- ✓ `Point = { x: Int, y: Int }`, `Segment = { start: Point, end: Point }`, `Path = { segments: Array[Segment] }` — finite chain.

Validation is a cycle check on the named-record dependency graph: when adding a def or updating an existing def's fields, walk the new fields and collect every `RecordType::Named(N)` reference, then DFS those names through the registry. If the DFS revisits the def being validated, reject the edit with a clear error message ("`Tree` would reference itself via …"). The check is O(V + E) on the dependency graph.

This restriction guarantees that `resolve_fields` and `can_be_converted_to` terminate without a visited-set: every chain of `Named` references eventually bottoms out, and `DataType` values are finite trees. If the user actually needs a recursive shape (e.g. a tree), they can build it with arrays of records that are linked by an integer ID stored as a field, rather than by direct type reference. We can revisit this restriction later if the workaround proves painful in practice.

## New Nodes

The three nodes below treat field order asymmetrically: **pin layout follows the def's authored order** (so `Date = {year, month, day}` shows pins in that order, not alphabetical), while **emitted runtime values use canonical order** (`NetworkResult::Record` is sorted by name internally). The conversion is local to each node — it iterates the def's authored fields for layout and re-sorts when constructing values.

### `record_construct`

**Property:**
- `schema: String` — the name of a record type def in the project's registry. Anonymous schemas are not exposable as a node property in v1, so the property stores just the name; it is wrapped as `RecordType::Named(self.schema.clone())` at use time. An empty string means "no schema chosen yet."

**Inputs:** one parameter pin per field of the def, named after the field, typed to the field's `DataType`. **Pin order matches the def's authored order** (looked up by name in the registry at type-resolution time).

**Output (single pin):** `Record(schema)`.

**Type resolution:** `calculate_custom_node_type` reads the def's authored `fields` from the registry, builds the parameter list in that order, and sets `output_pins[0].data_type = DataType::Record(RecordType::Named(self.schema.clone()))`. Modeled on `array_at::calculate_custom_node_type`. If `schema` is dangling (empty, or the named def has been deleted), `resolve_fields` returns `None`, so the node's output type fails subtyping against any consumer; `repair_node_network` then disconnects the now-incompatible wires.

**Eval:** reads each input parameter (in pin/authored order), constructs `NetworkResult::record(fields)` which sorts into canonical order. Missing-input behavior matches other constructors: if any required field input is unconnected, the output is `None`.

### `record_destructure`

**Property:**
- `schema: String` — the name of a record type def in the project's registry, wrapped as `RecordType::Named(self.schema.clone())` at use time. An empty string means "no schema chosen yet."

**Inputs:** one pin `record: Record(schema)`.

**Outputs (multi-pin):** one pin per field of the def, typed to the field's `DataType`, named after the field. Uses the multi-output pin infrastructure from the multi-output-pins design. **Pin order matches the def's authored order.**

**Type resolution:** reads the def's authored fields from the registry, sets `parameters[0].data_type = DataType::Record(RecordType::Named(self.schema.clone()))`, sets `output_pins` to one `OutputPinDefinition` per field in authored order. Wires-into-removed-pins handled by `repair_output_pin_wires` after schema-change repair.

**Eval:** reads the input record (canonical order internally) and emits `EvalOutput::multi(...)` with one entry per output pin in pin/authored order — fields are looked up by name (binary search on the canonical input). Pass-through coercion means the runtime record may carry extra fields beyond the schema; we ignore them. Fields declared in the schema but missing from the runtime value (an unreachable case under pass-through, but defensive code is cheap) emit `None` on the corresponding pin.

### `product`

**Property:**
- `target: String` — the name of a record type def in the project's registry, wrapped as `RecordType::Named(self.target.clone())` at use time. The target's field list drives the node's input pin layout *and* the node's output element type. An empty string means "no target chosen yet."

**Inputs:** one pin per field of the def, named after the field, typed `Array[FieldType_i]`. **Pin order matches the def's authored order** (read from the registry at type-resolution time).

**Output (single pin):** `Array[Record(target)]`.

**Eval:** for `Target = { f_0: T_0, …, f_{N-1}: T_{N-1} }` and inputs `xs_0 : Array[T_0]`, …, `xs_{N-1} : Array[T_{N-1}]`, the output is the cartesian product:

```
[ {f_0: a_0, …, f_{N-1}: a_{N-1}} | a_0 in xs_0, …, a_{N-1} in xs_{N-1} ]
```

Iteration order: **rightmost field varies fastest** (matches the natural reading of nested for-loops). If any `xs_i` is empty, the output is empty. Length is `∏ |xs_i|`.

The node's pin layout *is* the chosen def: the user declares the record type once at the top level and `product` reads its layout from there. Output is a real named type that downstream destructure / construct nodes can refer to without coordinating field names by hand.

## Expression Language Extensions

Phase 7 of the rollout. Optional — node-level work is independently useful.

### Record literal

```
{x: 1, y: 2, z: 3.0}
```

No type name on the literal. Parses to `Expr::RecordLiteral(Vec<(String, Expr)>)` preserving the user's source order in the AST (so error messages can point back to the original spelling). Validation runs each value expression, collects field types, and produces a type of `DataType::Record(RecordType::anonymous(...))` — an anonymous record type whose `fields` are canonicalized (sorted) inside `RecordType`. The runtime value emitted at evaluation is likewise built via `NetworkResult::record(...)` and stored canonically. Field names must be distinct; trailing comma not allowed (consistent with array literals).

The literal's anonymous type then participates in subtyping like any other record type: when the literal flows into a pin or a function parameter declared `Record(Foo)` and the structural compatibility holds, the connection is allowed. Width subtyping applies — the literal may declare more fields than the destination, and the extras pass through at runtime.

This works without bidirectional inference because the type system already has first-class anonymous record types. The literal does not need to know its destination type to be well-formed.

### Field access

```
r.x
```

Already legal grammar (used for vector and matrix members). Parser today special-cases `.x`, `.y`, `.z` for vectors and `.m00`–`.m22` for matrices. Generalize: if the receiver type is `Record(_)`, look the field up in the resolved schema; else fall back to vector/matrix rules.

### Type expressions in `expr` parameters

Since `DataType::Record` natively supports both named and anonymous forms, type expressions can freely produce either:

- `Foo` — identifier in type position resolves first as a built-in type, then as a named record def, then as an error.
- `{x: Int, y: Int}` — inline anonymous record-type literal in type position. Produces `RecordType::Anonymous(...)`.

The "type identifier" position remains restricted to immediately after `[]` or as a parameter type — same scoping rule the array-literal design already documents.

## Flutter UI

### Generic Type Selector — Record Branch

The existing type-selector widget (used wherever a `DataType` is picked: `array_at.element_type`, `sequence.element_type`, `map.input_type` / `output_type`, `filter.element_type`, `fold.element_type` / `accumulator_type`, `expr` parameter types, `array_concat`/`array_append`/`array_len` element types) gains a new top-level option: **Record**.

Selecting Record shows a dropdown listing every `RecordTypeDef` in the project, sorted alphabetically by name. The dropdown produces `DataType::Record(RecordType::Named(N))`. If the project has no record defs yet, the dropdown is empty and the user creates the def first from the user-types panel, then comes back to this widget — the type selector itself never creates defs.

Picking a type should not have side effects beyond setting that one type. Creating a record def, in contrast, swaps the main editing area away from whatever the user is currently editing (network → schema editor) and is therefore a deliberate, panel-driven action. Keeping these two actions separate avoids surprising context switches.

The widget exposes only named records. Anonymous record types exist in the type system (they are produced by expression-language record literals and inline `{...}` schemas in `expr` type positions — see Phase 7), but the UI never asks the user to construct one inline: any record they pick from this widget gets a name and lives in the user-types panel. This keeps the widget's UX simple (one row, one dropdown) and routes all record-schema authoring through the schema editor.

The widget is shared across every site listed above. Implementation: one Flutter widget, used everywhere `DataType` is selected.

### Per-Node Property UI

**`record_construct`, `record_destructure`, and `product`** — each has a single property: the record def name (`schema` or `target`, a `String`). UI is the Record dropdown described above (it picks a name from the project's record defs), with one affordance:

- *Edit definition…* — selects the bound def in the user-types panel and switches the main editing area to that def's schema editor (i.e., it activates the def, the same way clicking a network in the panel activates it). Disabled when the property is empty or dangling.

There are no per-pin field-name controls on `product` (the def supplies the names) and no inline schema editor on `record_construct` / `record_destructure`. New defs are created from the user-types panel, not from inside a node property — same rationale as for the type selector dropdown.

### Top-Level User-Types Panel

The existing left-side panel that lists custom node networks becomes a unified "User Types" panel with two kinds of entries:

- Node networks (existing).
- Record type defs (new).

A single tree/list view with a kind icon per row (function-arrow icon for networks, brace icon for records). Both kinds support: rename, delete, and activation. Selecting an entry makes it the active item; the main editing area then renders the appropriate editor for that kind.

A new-record dialog asks for a name and creates an empty-fields def. Empty record types are valid (top in the subtype lattice — every record is `<: Empty`).

### Schema Editor (where a record def is edited)

**Location.** The schema editor occupies the same region as `NetworkEditorTabs` (the Graph/Text tabs in `lib/structure_designer/main_content_area.dart`) — i.e., the bottom panel of the resizable main content area, beside/below the 3D viewport. The choice of editor is driven by the active item in the user-types panel:

- Active item is a node network → main area shows `NetworkEditorTabs` (Graph + Text).
- Active item is a record def → main area shows the record schema editor.

There is only one main editing area; switching the active item swaps which editor fills it. The viewport above is unchanged (record defs have nothing to render in 3D, so the viewport simply shows whatever displayed nodes were last active, or stays empty). No tabs are needed inside the schema editor in v1 — there is no text representation of a record def.

**Layout.** Top-to-bottom inside the editor panel:

1. A header strip with the def's name (read-only here; rename is done from the user-types panel context menu) and a small kind icon.
2. A scrollable list of field rows, in the def's **authored order** (this is the order that drives `record_construct` / `record_destructure` / `product` pin layouts; canonical sorting is an internal detail and never surfaced).
3. A trailing `+ Add field` button that appends a row with a placeholder name and a default type.

Each field row contains:

- A drag handle on the left for reorder (drag-and-drop within the list — reorders the def's authored field order).
- A name text input. Validated as a non-empty identifier via the existing `identifier_validation.dart` rules; duplicates within the def show a red ring and a tooltip ("Field `x` is already declared"). The input commits on blur or Enter.
- A type cell that uses the generic type selector widget. For the Record branch, this is the named-only dropdown described above; the type cell may select another named record def (so `Box = { p: Point }` is built by picking `Point` here).
- A delete-row button on the right.

**Save semantics.** Edits commit to the underlying def on every successful change (mirroring how node property editors apply changes immediately) — there is no Apply button. A "successful change" means the validation gate below passed:

- Field names are non-empty distinct valid identifiers.
- The new field list does not introduce a cycle in the named-record dependency graph (the cycle check from "No Recursive Definitions" — `add_record_type_def` / `update_record_type_def` already enforces it; the UI calls into that path).

If the user picks a type in the type cell that would close a cycle (e.g. editing `A` and selecting `B` where `B` already references `A`), the dropdown rejects the choice and shows a snackbar/toast: ``"`A` would reference itself via `B → A`."``. The same message comes back from the registry's cycle check, so the rule lives in one place. The dropdown filters out obviously-cyclic candidates up-front (the def being edited, plus any def that transitively references it) so the offending choice is usually not even presented.

Each successful commit fires `update_record_type_def`: the def's authored field list is replaced, then `repair_node_network` runs on every affected network — `record_construct` / `record_destructure` / `product` pin layouts re-derive, and now-incompatible wires are disconnected. (No reference rewriting is needed because `Named(N)` references resolve through the registry on every lookup.) The operation is wrapped in an `UpdateRecordTypeDefCommand` (snapshot-based, same shape as `RenameNetwork`/`DeleteNetwork`) so a single edit is one undo step.

**Adding the first field to an empty def.** A freshly created def has zero fields and is rendered with just the header and the `+ Add field` row. The first add is a normal commit — no special case.

**Navigation.** A breadcrumb-style "← Back" affordance is unnecessary because the user-types panel is always visible on the left; switching back is one click. The "Edit definition…" affordance on `record_construct` / `record_destructure` / `product` (described above) is the standard way to jump from a node property to the corresponding def.

### Wire and Pin Rendering

A pin of type `Record(_)` is rendered with a single neutral "record" color (no per-name hashing, since structurally compatible record types can have different names and we want the visual to reflect compatibility, not identity). Hover tooltip shows the name (when present) and the resolved field list (`Point { x: Int, y: Int }` for named, `{x: Int, y: Int}` for anonymous).

Tooltip rendering of a record-typed pin (and previews of record runtime values flowing through such a pin) resolves field order from the pin's declared type — authored order via `RecordTypeDef` lookup for named records, alphabetical for anonymous — not from the canonical storage order of the underlying `RecordType` / `NetworkResult::Record`. Canonical order is an internal invariant for equality, hashing, and linear-merge subtyping; it is not what the user sees. The same principle applies anywhere fields are displayed (error messages, debug prints): if a name is in scope, sort by the def's authored order at render time.

## Subtyping Examples

(`Point = {x: Int, y: Int}`, `Point3 = {x: Int, y: Int, z: Int}`, `PointF = {x: Float, y: Float}`, `Box = {p: Point3}`, `BoxXY = {p: Point}`, `Tagged = {a: Crystal, label: String}`, `Abstract = {a: HasAtoms, label: String}`.)

| Source | Destination | Result |
|---|---|---|
| `Record(Point3)` | `Record(Point)` | ✓ width |
| `Record(Point)` | `Record(Point3)` | ✗ missing `z` |
| `Record(Point)` | `Record(PointF)` | ✗ value-converting widening rejected at field level (`Int` → `Float`) |
| `Record(Box)` | `Record(BoxXY)` | ✓ depth + width |
| `Array[Record(Point3)]` | `Array[Record(Point)]` | ✓ array elt-wise |
| `Record(Tagged)` | `Record(Abstract)` | ✓ tag-only widening at field level (`Crystal → HasAtoms`) |
| `Array[Record(Tagged)]` | `Array[Record(Abstract)]` | ✓ tag-only widening through array |
| `Record({a: Molecule})` | `Record({a: HasFreeLinOps})` | ✓ tag-only widening (anonymous) |
| `Record(Foo)` where `Foo = {x: Int, y: Int}` | `Record({x: Int, y: Int})` (anonymous) | ✓ structural — names ignored |
| `Record({x: Int, y: Int})` (anonymous) | `Record(Point)` | ✓ structural |

## Migration

No migration of existing project files and no cnnd version bump. `record_type_defs` is absent in pre-record cnnd files; `#[serde(default)]` on the field produces an empty `HashMap`. Pre-record files contain no `DataType::Record` or `NetworkResult::Record` instances either, so deserialization is unaffected.

No existing nodes change behavior. `DataType::Record` is a new variant; every existing match on `DataType` is exhaustive over the old variants and a `_ => unreachable!()` arm becomes wrong — those will need to be expanded. (Use `cargo check` to find them.) Same for `NetworkResult` matches.

## Phasing

Each phase is shippable on its own. Phases 1–4 are the core; 5–9 layer features.

### Testing conventions

Rust phases (1–4, 7, 8) ship automated tests in `rust/tests/structure_designer/` (per AGENTS.md — never inline `#[cfg(test)]`). Use `cargo insta`-style snapshot tests (alongside `node_snapshots`) wherever a pin layout is derived from a record def, so authored-order vs canonical-order regressions are caught. Any new undo command follows the do/undo/redo snapshot-equality pattern from `rust/tests/structure_designer/undo_test.rs`. `.cnnd` round-trip tests live alongside the existing `cnnd_roundtrip` family.

Flutter phases (5, 6, 9) have **no automated tests** — the project does not test the Flutter UI in CI. Each of these phases lists a `Manual verification:` section instead, enumerating the click-paths an implementor should walk through before declaring the phase done.

### Phase 1 — Type and value plumbing

- Add `DataType::Record(RecordType)` with the enum shape above (`Named(String)` | `Anonymous(Vec<...>)`). Anonymous constructor canonicalizes fields (sort by name) and enforces invariants.
- Add `NetworkResult::Record(...)` (no type name); constructor canonicalizes fields.
- `infer_data_type` on a record value returns `RecordType::Anonymous(...)` (already canonical because the value's fields are).
- Thread `&NodeTypeRegistry` through `can_be_converted_to`; add a same-name short-circuit for `Named(n) → Named(n)` (full subtyping comes in Phase 4).
- Update all match-on-`DataType` and match-on-`NetworkResult` sites.

Tests (automated, `rust/tests/structure_designer/`):
- Equality of two anonymous `RecordType`s constructed from differently-ordered field lists yields `==` and equal `Hash`.
- `Named` vs `Anonymous` distinction in equality (`Named(n)` never equal to `Anonymous(...)` even when the resolved fields match).
- Same-name short-circuit in `can_be_converted_to` (`Named(n) → Named(n)` returns true without touching the registry).
- `infer_data_type` on a `NetworkResult::Record` value returns `DataType::Record(RecordType::Anonymous(...))` with fields in canonical order, regardless of construction order.
- `NetworkResult::record(...)` and `extract_record_field` round-trip: build with shuffled fields, look up each by name, and confirm canonical storage via direct inspection.
- Threading `&NodeTypeRegistry` through `can_be_converted_to` does not regress any existing non-record subtyping case (re-run the existing `can_be_converted_to` test set).

No subtyping, no UI, no nodes in this phase.

### Phase 2 — Top-level defs, operations, and serialization

- Add `NodeTypeRegistry::record_type_defs` (HashMap).
- `add` / `delete` / `rename` / `update` operations with namespace-collision checks against `node_networks` and built-ins.
- **Cycle check** on add and update: walk the def's transitive references and reject if it references itself. O(V + E) DFS on the named-record dependency graph.
- **Rename walker**: a `DataType` walker that visits every record reference in every network's nodes, properties, and parameter/return types, rewriting `Named(old)` to `Named(new)`. Modeled on `rename_node_network`. No corresponding walker is needed for field updates — references resolve through the registry, so a field change is visible immediately everywhere.
- **`repair_node_network` extension**: on def deletion or field update, walk affected networks and disconnect now-incompatible wires; refresh `record_construct` / `record_destructure` / `product` pin layouts.
- Serializable form for record defs and for `RecordType` (named references serialize as just the name; no cached schema).
- On-load validation: re-run the cycle check; flag any `Named(N)` reference whose `N` is missing from the registry. No cached-fields reconciliation step.
- `RenameRecordTypeDef`, `DeleteRecordTypeDef`, `UpdateRecordTypeDef`, `AddRecordTypeDef` undo commands (snapshot-based, same shape as `RenameNetwork`/`DeleteNetwork`).

Tests (automated, `rust/tests/structure_designer/`):

*Registry operations:*
- Add / rename / delete a def; namespace-collision rejection against `node_networks`, built-in types, and existing record defs.
- Cycle rejection: direct self-reference (`Tree = { children: Array[Record(Tree)] }`); mutual recursion (`A → B → A`); transitive chain (`A → B → C → A`); cycle introduced via `Array[Record(...)]` and via nested record fields.
- Field update is visible at every reference site **without** any explicit walk — assert by reading a `Record(N)` pin's resolved schema before and after `update_record_type_def(N, ...)`.

*Rename walker — assert rewrite at every site:*
- Inside parameter types of a custom network.
- Inside pin types on built-in nodes (e.g. `array_at.element_type` set to `Record(Old)`).
- Inside the return-node output type of a custom network.
- Inside `record_construct.schema` / `record_destructure.schema` / `product.target` (bare-name properties, not embedded `RecordType`).
- Nested inside `Array[Record(Old)]` and inside another record def's field type (`Box = { p: Record(Old) }`).
- Negative case: a network that does not reference `Old` is byte-identical before and after the rename.

*Repair on schema change / deletion:*
- After `update_record_type_def`: wires whose source type no longer satisfies a renamed/retyped field are disconnected; `record_construct` / `record_destructure` / `product` pin layouts re-derive in authored order.
- After `delete_record_type_def`: every `Named(N)` reference is now dangling; `repair_node_network` disconnects every wire that depended on it; on-load validation flags each dangling reference with a clear error.

*Serialization (`.cnnd` round-trip):*
- Round-trip a project with nested defs (`Point`, `Box = { p: Point }`); fields preserve authored order on disk; def list emitted sorted by name.
- **Backward compat** — load a fixture `.cnnd` saved before this feature (no `record_type_defs` key); `#[serde(default)]` produces an empty registry; the rest of the project loads identically. Add a fixture under `rust/tests/fixtures/` alongside the existing pre-record fixtures.
- Hand-edited file with a dangling `Named(N)` reference: load succeeds, validation reports the dangling reference, no panic.
- Hand-edited file with a cyclic def: load reports the cycle through the on-load cycle check.

*Undo (one test per command, do→undo→redo snapshot equality, modeled on `rust/tests/structure_designer/undo_test.rs`):*
- `AddRecordTypeDefCommand`.
- `DeleteRecordTypeDefCommand` — verify cross-network wire-disconnection side effects are undone too (snapshot the affected networks before delete and after undo).
- `RenameRecordTypeDefCommand` — verify all rewrite sites from the rename-walker matrix above are restored on undo.
- `UpdateRecordTypeDefCommand` — verify field-update repair side effects (disconnected wires, re-derived pin layouts) are restored on undo.

### Phase 3 — `record_construct` and `record_destructure`

- New node types in `rust/src/structure_designer/nodes/`.
- Parametric properties via `calculate_custom_node_type` (modeled on `array_at`). Cached `Schema.fields` consumed directly.
- `record_destructure` uses multi-output pins.
- Schema-change repair: when `update_record_type_def` propagates new fields, walk these nodes and refresh their pin layout; disconnect now-incompatible wires via `repair_node_network`.

Tests (automated, `rust/tests/structure_designer/`):
- Construct/destructure round-trip: build a `Point` via `record_construct`, feed into `record_destructure`, assert each output pin equals the corresponding input.
- Nested-def construct: `Box = { p: Point }` built from a `record_construct(Point)` whose output feeds the `p` input of `record_construct(Box)`.
- Missing-input propagation: any unconnected required input on `record_construct` makes the output `None`; downstream `record_destructure` emits `None` on every pin.
- Pass-through on destructure: a runtime record carrying extra fields beyond the destructure's schema is not projected — the destructure ignores the extras and emits the declared fields by name (binary search on canonical input).
- Dangling schema: set `schema = ""` and set `schema = "DeletedDef"`; both cases cause `resolve_fields` to return `None`, the node's output type fails subtyping against any consumer, and `repair_node_network` disconnects downstream wires.
- **Field-rename-as-remove+add semantics** (deliberate per design): rename `x → xx` on the def; assert wires connected to the old `x` pin on every `record_destructure` are disconnected and the new `xx` pin starts unconnected. This locks in the chosen semantics over a stable field-id alternative.
- **Pin-layout snapshot tests** (`cargo insta`, alongside `node_snapshots`): serialize the `record_construct` / `record_destructure` node view for a def with deliberately non-alphabetical authored order (e.g. `Date = {year, month, day}`); assert pins appear in authored order, not canonical order. Add a second snapshot after `update_record_type_def` reorders fields, asserting the layout follows.
- Schema-change wire repair end-to-end: build `record_construct(Point)` → `record_destructure(Point)` with both connected; retype `Point.y` from `Int` to `Vec3`; assert the `y` wire on `record_construct` is disconnected (incompatible source) and the `record_destructure.y` output pin re-derives with the new type.

### Phase 4 — Subtyping

- Extract the existing concrete-to-abstract phase upcast block in `data_type.rs` into an `is_tag_only_widening(src, dst)` predicate; refactor `can_be_converted_to`'s abstract-upcast arm to call it (no behavior change).
- Add the full record branch to `can_be_converted_to` (width + structural depth, tag-only widenings at field level).
- Add `can_be_structurally_converted_to` (the strict variant used for field-level checks; leaf positions delegate to `is_tag_only_widening`).
- Width subtyping at pin connect time.
- Element-wise subtyping in arrays of records falls out of the existing `Array` recursion.

Tests (automated, `rust/tests/structure_designer/`):
- Every row of the subtyping table in the Examples section as a parameterized test, including the value-converting-widening rejection (`Record(Point) → Record(PointF)`) **and** the tag-only widening acceptance (`Record({a: Crystal}) → Record({a: HasAtoms})`, plus the array-of-records variant `Array[Record(Tagged)] → Array[Record(Abstract)]`).
- Anonymous-named compatibility in both directions.
- `is_tag_only_widening` direct unit tests covering each phase-upcast edge plus identity.
- Refactor regression: re-run the full pre-existing `can_be_converted_to` test set after the `is_tag_only_widening` extraction and confirm zero behavior delta on non-record types.
- Empty record `{}` is top of the lattice — every record is assignable to it, including across named/anonymous and through arrays.

### Phase 5 — Flutter type-selector record branch

- Generic Record branch in the type selector widget: dropdown of named `RecordTypeDef`s. New defs are created from the user-types panel (Phase 6); the type selector itself does not create them.
- Used at every existing type-selector site.
- The widget exposes named records only; anonymous record types are reachable only from the expression language (Phase 7), not from the Flutter UI.

Manual verification (no automated UI tests in this project):
- Open a project with at least two record defs. At each existing type-selector site (`array_at.element_type`, `sequence.element_type`, `map.input_type` / `output_type`, `filter.element_type`, `fold.element_type` / `accumulator_type`, `expr` parameter type, `array_concat` / `array_append` / `array_len` element types), confirm the **Record** branch appears, the dropdown lists every def alphabetically, and selecting a def sets the pin type to `Record(Named(...))`.
- Open a project with no record defs: the **Record** branch shows an empty dropdown and offers no inline-create affordance.
- Pick a record type at one site and connect a wire whose source is a structurally-compatible record — confirm the wire is allowed (subtyping wired in via Phase 4).
- Anonymous records are not offered anywhere in the type-selector UI (sanity check that the widget hasn't grown an inline schema editor).

### Phase 6 — Flutter user-types panel

- Unified list/tree of networks + record defs.
- Schema editor for a def.
- Rename / delete / new-record actions, hooked to the API operations from Phase 2.

Manual verification (no automated UI tests in this project):
- Panel shows networks and record defs in one tree with distinct kind icons.
- Create a new record def via the panel: prompts for a name; rejects collisions with existing networks, existing record defs, and built-in type names; the new def opens in the schema editor with zero fields.
- Add fields via `+ Add field`: name validation rejects duplicates (red ring + tooltip) and empty/invalid identifiers; type cell uses the generic type selector, including the Record branch.
- Reorder fields by dragging — confirm the change is reflected in the pin layout of any existing `record_construct` / `record_destructure` / `product` referencing this def (open one in the network editor side-by-side).
- Edit a field's type to one that breaks compatibility with an existing wire — confirm the wire is disconnected after commit.
- Attempt to introduce a cycle (edit `A`, pick `B` in a type cell where `B` already references `A`) — the choice is rejected with the toast message; the dropdown filters out the cyclic candidates up-front.
- Rename a def via the panel context menu — confirm every reference (parameter types, pin types, return-node output type, `record_construct` / `record_destructure` / `product` properties) updates live; existing wires remain connected.
- Delete a def — confirm `repair_node_network` disconnects every dependent wire across every network and that any `Record(Named(N))` reference now shows as a type error.
- Undo / redo each of: create def, delete def, rename def, field add / remove / reorder / retype. After undo, the project state and every affected network is byte-identical to the pre-edit state (cross-check against Phase 2 automated undo tests, which cover the Rust side).
- Switching the active item in the panel between a network and a record def swaps the main editing area between `NetworkEditorTabs` and the schema editor; the 3D viewport is unchanged.

### Phase 7 — Expression language

- `Expr::RecordLiteral { fields }` and parser support for `{x: 1, y: 2}` (no type-name prefix; produces an anonymous `RecordType`).
- Generalize `.<ident>` field access to records.
- Type-expression grammar accepts both named identifiers and inline `{...}` schemas.

Tests (automated, `rust/tests/structure_designer/` and `rust/src/expr/` integration tests):
- Round-trip `{x: 1, y: 2}.x + 1` (parse → validate → evaluate).
- Nested literals (`{outer: {inner: 1}}.outer.inner`).
- Type-checking of literal fields against destination pin schemas (width subtyping — extras flow through, missing fields rejected).
- Subtyping of literal into a named-record pin (width only — scalar promotion at field level is rejected, see Phase 4).
- Anonymous-record type expression in `expr` parameter position (`{x: Int, y: Int}`) parses and matches structurally against a named def with the same shape.
- Field-name conflict with vector members: `r.x` on a record-typed receiver resolves to the record field (not the vector rule); error message on the confused case (e.g. `.x` on a `Vec3` typed as a record) is clear (covers Open Question 1).

### Phase 8 — `product` node

- `target: String` property (a record def name; wrapped as `RecordType::Named(_)` at use time).
- Pin layout derived from the resolved target def.
- Output: `Array[Record(target)]`.

Tests (automated, `rust/tests/structure_designer/`):
- 2-field product (smallest non-trivial case).
- 3-field product — assert iteration order is **rightmost field varies fastest** (Open Question 2 lock-in).
- Empty-input product on any axis: output is empty.
- Large product (cardinality math): `|out| == ∏ |xs_i|`.
- Product whose target def has a field of type `Array[T]` (records of arrays, *not* recursive defs — cycles are rejected; see "No Recursive Definitions").
- Dangling `target` (empty string or a deleted def): output type fails subtyping; downstream wires are disconnected by `repair_node_network`.
- **Pin-layout snapshot test** (`cargo insta`): node view for a `product` whose target has non-alphabetical authored order — assert input pins follow authored order; assert output type is `Array[Record(Named(target))]`.

### Phase 9 — Polish

- Pin/wire rendering for record types (single neutral color, hover tooltip with resolved schema).
- "Edit definition…" affordance on `record_construct` / `record_destructure` / `product`.
- Reference guide entry under `doc/reference_guide/nodes/`.

Manual verification (no automated UI tests in this project):
- Record-typed pins render in the single neutral "record" color regardless of def name; two structurally-compatible pins with different names use the same color (visual reflects compatibility, not identity).
- Hover tooltip on a named-record pin shows `Name { field: Type, ... }` with fields in **authored order** (not canonical). Hover tooltip on an anonymous-record pin shows `{field: Type, ...}` alphabetically.
- Hover preview of a runtime record value flowing through such a pin renders fields in the same order as the tooltip (authored when the pin is named, alphabetical otherwise).
- "Edit definition…" affordance on `record_construct` / `record_destructure` / `product`: enabled when the property is a valid def name; disabled when empty or dangling. Clicking it activates that def in the user-types panel and swaps the main editing area to the schema editor.
- Reference guide entry under `doc/reference_guide/nodes/` exists and renders correctly in the in-app docs viewer (if present).

## Open Questions

1. **Field-name conflicts with reserved identifiers.** A field literally named `x` conflicts with the vector-member rule when records and vectors both use `.x`. Field access on a `Record` resolves through the schema (no ambiguity at static type), but the user-facing error message when they confuse the two should be clear.
2. **`product` axis order.** Documented as "rightmost field varies fastest." Worth confirming before users build muscle memory; flipping later would be a breaking semantic change.
3. **Hand-edited .cnnd cycles or dangling references.** A user could hand-edit a .cnnd file and smuggle in a cycle that was rejected at edit time, or a `Named(N)` reference where `N` is not in `record_type_defs`. Phase 2's on-load validation re-runs the cycle check (rejecting cyclic defs with a load error) and flags every dangling reference. Worth checking that this matches the project's conventions for divergent-data recovery.
