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
- **Bulk edits via cascading propagation.** When the user changes a named def's schema (renames a field, retypes one, adds a new one), the change cascades to every reference in every network in the project — node properties, pin types, custom-network interfaces — and incompatible wires are automatically repaired. This is only possible because each `DataType::Record` instance carries the def's name, so the system can find every reference by name and rewrite it. Anonymous records have no equivalent — editing one schema doesn't touch any other — which is exactly right when the schemas were never meant to be the same thing in the first place.

Naming is therefore a UX and tooling concession layered on top of a structurally-typed core, not a constraint on the type system itself.

## Design Principles

1. **`DataType::Record` carries both name and field schema inline.** A name is optional metadata; the field schema is what type compatibility is computed from. The schema is duplicated alongside the name so that a renamed/modified named def can be propagated to every `DataType` instance referencing it (the same fix-up pattern that handles renaming a node network).
2. **Names don't gate compatibility.** Subtyping is purely structural. Two named records `Foo` and `Bar` with compatible schemas are assignable in either direction up to the width-subtyping rule below.
3. **Width + depth subtyping with field-type promotion.** `Record(R1) <: Record(R2)` iff every field declared in `R2`'s schema is present in `R1`'s schema with a compatible type, recursively, using the existing `DataType::can_be_converted_to` rules (so `IVec3 → Vec3` and `Int → Float` work field-wise).
4. **No type names at runtime.** `NetworkResult::Record` carries fields only — no name. Names live only on the type side, where they identify the source def for propagation. At evaluation time the name has no role.
5. **Pass-through coercion.** When a value with fields `{x, y, z}` flows into a pin declared `{x, y}`, the runtime value is unchanged. The destination type declares what the consumer is allowed to read, not what the value must contain. (Same precedent as abstract supertypes like `HasAtoms`, which never appear at runtime.)
6. **Canonical field order in `RecordType` and `NetworkResult::Record`; authored order on `RecordTypeDef`.** Cached schemas inside `DataType::Record` and runtime record values store fields **sorted by name** (canonical form). This makes derived `PartialEq` / `Hash` correct, makes serialization deterministic, and lets subtyping merge two sorted field lists in linear time. The top-level `RecordTypeDef` keeps fields in **authored order** — this is what node pin layouts and the schema editor display. Conversion is a single sort when constructing a `RecordType` from a def.
7. **No cycles.** A record's fields may contain other record types (nesting is allowed and common — `Box = { p: Point }`), but the dependency graph among named record defs must be acyclic. A def cannot reference itself, directly or transitively. This is validated at edit time, and lets the type representation stay flat — `fields` is always inlined, never a stub.
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
pub struct RecordType {
    /// `Some(name)` if this references a registered record type def;
    /// `None` for an anonymous record (e.g. expr literal type, expr output type).
    pub name: Option<String>,

    /// Cached field schema in **canonical (sorted-by-name) order**. Always
    /// populated. For named records, this is the canonicalized schema of the
    /// def at the time the `RecordType` was constructed — kept in sync with
    /// the registry by the propagation pass on rename / update. Empty record
    /// `Empty = {}` is just `vec![]`. Cycles are forbidden, so no stub form
    /// is needed.
    pub fields: Vec<(String, DataType)>,
}
```

Construction invariants (enforced by the constructors):
- Field names within `fields` are distinct.
- Field order is canonical (sorted ascending by field name).

Helpers (constructors sort the input):

```rust
impl RecordType {
    pub fn anonymous(mut fields: Vec<(String, DataType)>) -> Self {
        fields.sort_by(|(a, _), (b, _)| a.cmp(b));
        RecordType { name: None, fields }
    }
    pub fn named(name: String, mut fields: Vec<(String, DataType)>) -> Self {
        fields.sort_by(|(a, _), (b, _)| a.cmp(b));
        RecordType { name: Some(name), fields }
    }
}
```

Because both `RecordType` and `NetworkResult::Record` (below) canonicalize on construction, the derived `PartialEq` and `Hash` impls are structurally correct out of the box: two `RecordType`s with the same name and the same fields-as-a-set compare equal regardless of how the user authored field order.

### Subtyping

Extend `can_be_converted_to` with one branch:

```rust
(DataType::Record(src), DataType::Record(dst)) => {
    // Same-name short-circuit (avoids walking fields when names match).
    if src.name.is_some() && src.name == dst.name {
        return true;
    }
    // Both field lists are canonical (sorted by name). Walk dst forward,
    // advancing src to find each dst field by linear merge.
    let mut si = 0;
    for (dst_field, dst_ty) in &dst.fields {
        while si < src.fields.len() && src.fields[si].0.as_str() < dst_field.as_str() {
            si += 1;
        }
        if si == src.fields.len() || src.fields[si].0 != *dst_field {
            return false;  // dst requires a field src doesn't have
        }
        if !can_be_converted_to(&src.fields[si].1, dst_ty) {
            return false;
        }
        si += 1;
    }
    true
}
```

This delivers **width subtyping** (extra fields on `src` are allowed), **depth subtyping** (each field type checked recursively), and **field-type promotion** (because the recursive call goes through the same `can_be_converted_to`). Cached `fields` are used directly — no registry needed at pin-connect time. The linear-merge walk is O(N + M) thanks to canonical ordering. Termination is trivial: the no-cycle invariant means `DataType` values are finite trees, so recursion bottoms out.

Equality (`==`) is defined by the derived `PartialEq` on `RecordType` — name-and-field-equal — but is *not* the relation used at pin-connect time. Pin-connect uses `can_be_converted_to`, which ignores names and applies subtyping.

`can_be_converted_to` does not need `&NodeTypeRegistry` for record subtyping (cached fields are sufficient). Other code paths that *construct* `DataType::Record` from a name (e.g. when the user picks a record type in a node property) need the registry to fill in the cached fields, but that's a construction-time concern, not a subtyping-time concern.

### Pass-through, not projection

Runtime values are *never* projected. If a record value with fields `{x, y, z}` flows into a pin declared `Record(Bar)` where `Bar = {x, y}`, the runtime payload is unchanged — it still carries `z`. Consumers (the `record_destructure` node, the `r.x` expression) see only what their declared schema lets them read. This:

- Avoids data loss on intermediate edges.
- Sidesteps the question of how to project through `Array[Record(...)]`.
- Matches the precedent set by `HasAtoms` — declared types and runtime values are not constrained to be identical.

`infer_data_type(value)` for a record value returns `DataType::Record(RecordType { name: None, fields: Some(...) })` — an anonymous schema reflecting the value's actual fields. Validation does not compare declared and inferred types directly; it goes through `can_be_converted_to`.

## Runtime Values

```rust
// network_result.rs

pub enum NetworkResult {
    // ... existing variants ...
    Record(Vec<(String, NetworkResult)>),  // canonical (sorted by name); no type name
}
```

Runtime values carry no type name. The name lives only on `DataType::Record`, where it serves the propagation mechanism. Field order is canonical — same rationale as `RecordType.fields`: structural equality of values works under derived `PartialEq`, hashing is consistent, and serialization is deterministic. The constructor sorts on creation:

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
    /// the canonical (sorted) order used inside `RecordType` and `NetworkResult`.
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

### Operations and propagation

The defining feature of this design: cached `fields` inside every `DataType::Record { name: Some(N), fields: Some(...) }` instance must stay consistent with the registry's `RecordTypeDef[N]`. We maintain this invariant by **propagation passes** keyed by name, modeled on `rename_node_network` / `repair_node_network`.

- `add_record_type_def(def)` — validates: name not already taken (against `record_type_defs`, `node_networks`, and built-ins); field names within the def are distinct; **the def's transitive references do not form a cycle** (i.e., its fields do not, directly or via other named records, contain a reference back to itself).
- `delete_record_type_def(name)` — removes the def. Walks all networks; every `DataType::Record { name: Some(name), .. }` becomes a dangling reference, reported as a validation error. Mirrors how a deleted custom network is handled.
- `rename_record_type_def(old, new)` — updates the registry key, then walks every `DataType` (in node properties, parameter types, pin types, the return node's output type, anywhere a `DataType` appears) and rewrites `RecordType { name: Some(old), .. }` to `RecordType { name: Some(new), .. }`. Cached `fields` are unaffected by rename.
- `update_record_type_def(name, new_fields)` — first runs the cycle check (`new_fields` and the rest of the registry must remain acyclic); if that passes, replaces the def's field list (preserving the user's authored order on the def), then walks every `DataType::Record { name: Some(name), .. }` instance and overwrites the cached `fields` with the new schema *in canonical order* (the propagation routine sorts as it copies). After propagation, run `repair_node_network` on every affected network: nodes' `calculate_custom_node_type` is re-evaluated, pin layouts refresh from the def's authored order, wires now incompatible with the new schema are disconnected.

Three places need to be walked by these passes:
1. **Node property data** (the `NodeData` blob for `record_construct`, `record_destructure`, `product`, etc., which stores a `RecordType`).
2. **Computed node types** (cached pin types on individual nodes, which include record types).
3. **Custom network interfaces** (input parameter types and return-node output type).

The walk is a single recursive descent over `DataType` (similar to existing `DataType` visitors); each leaf record reference is checked against the affected name and rewritten in place.

### Repair on schema change

When a record def's fields change, the propagation pass above rewrites cached `fields` everywhere. After that:
- `record_destructure` nodes re-derive their output pins from the new schema; wires connected to pins that no longer exist are disconnected (via `repair_output_pin_wires`).
- `record_construct` and `product` nodes re-derive their input pins; wires whose source type no longer satisfies the new field type are disconnected.
- General pin-type compatibility is rechecked across all wires; failing wires are disconnected.

Reuse and extend `repair_node_network`. Evaluation caches are invalidated, so runtime values produced before the change are not observed by downstream consumers post-change.

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
        {
          "name": "p",
          "type": {
            "Record": {
              "name": "Point",
              "fields": [
                { "name": "x", "type": "Int" },
                { "name": "y", "type": "Int" }
              ]
            }
          }
        }
      ]
    }
  ],
  ...
}
```

`DataType::Record` serializes as a `RecordType` struct with `name` and `fields` fields. Every reference to a named record (in record-def fields, in node properties, in pin types) serializes the full cached schema — accepting some on-disk redundancy in exchange for keeping deserialization independent of cross-section ordering.

The `record_type_defs` field on the project root uses `#[serde(default)]`, so older .cnnd files without it load with an empty `HashMap`. No version bump and no migration code: this is a purely additive change in the same vein as past additive field additions. (Versioning is reserved for changes that require explicit migration logic.)

A consistency check on load: walk every `DataType::Record { name: Some(N), .. }` and verify the cached `fields` matches the canonical form of `record_type_defs[N].fields`. If not (e.g. a hand-edited file, or drift from a different code version), prefer the registry and run the update-propagation pass. The cycle check is also re-run on load.

## No Recursive Definitions

Record types may freely contain other record types as field types — `Box = { p: Point }`, `Triangle = { a: Point, b: Point, c: Point }`, `Group = { items: Array[Item] }` — but the dependency graph among named record defs must be acyclic. A def cannot reference itself, directly or transitively. Concretely:

- ✗ `Tree = { children: Array[Record(Tree)] }` — direct self-reference.
- ✗ `A = { b: Record(B) }`, `B = { a: Record(A) }` — mutually recursive.
- ✓ `Point = { x: Int, y: Int }`, `Segment = { start: Point, end: Point }`, `Path = { segments: Array[Segment] }` — finite chain.

Validation is a cycle check on the named-record dependency graph: when adding a def or updating an existing def's fields, walk the new fields and collect every `RecordType { name: Some(N), .. }` reference, then DFS those names through the registry. If the DFS revisits the def being validated, reject the edit with a clear error message ("`Tree` would reference itself via …"). The check is O(V + E) on the dependency graph.

This restriction lets the type representation stay flat — `RecordType.fields` is always inlined, never a stub — and lets `can_be_converted_to` recurse on `DataType` directly without a visited-set. If the user actually needs a recursive shape (e.g. a tree), they can build it with arrays of records that are linked by an integer ID stored as a field, rather than by direct type reference. We can revisit this restriction later if the workaround proves painful in practice.

## New Nodes

The three nodes below treat field order asymmetrically: **pin layout follows the def's authored order** (so `Date = {year, month, day}` shows pins in that order, not alphabetical), while **pin types and emitted runtime values use canonical order** (so the underlying `RecordType` and `NetworkResult::Record` are sorted-by-name internally). The conversion is local to each node — it iterates the def's authored fields for layout and re-sorts when constructing/reading values.

### `record_construct`

**Property:**
- `Schema: RecordType` — a named record type chosen from the project's record defs. Anonymous schemas are not exposed here in v1.

**Inputs:** one parameter pin per field of the def, named after the field, typed to the field's `DataType`. **Pin order matches the def's authored order** (looked up by name in the registry at type-resolution time).

**Output (single pin):** `Record(Schema)`.

**Type resolution:** `calculate_custom_node_type` reads the def's authored `fields` from the registry, builds the parameter list in that order, and sets `output_pins[0].data_type = DataType::Record(self.schema.clone())` (the cached `Schema` itself stores fields canonically). Modeled on `array_at::calculate_custom_node_type`. If `Schema.name` becomes dangling after a def deletion, the node's output type is an error sentinel.

**Eval:** reads each input parameter (in pin/authored order), constructs `NetworkResult::record(fields)` which sorts into canonical order. Missing-input behavior matches other constructors: if any required field input is unconnected, the output is `None`.

### `record_destructure`

**Property:**
- `Schema: RecordType` — a named record type chosen from the project's record defs.

**Inputs:** one pin `record: Record(Schema)`.

**Outputs (multi-pin):** one pin per field of the def, typed to the field's `DataType`, named after the field. Uses the multi-output pin infrastructure from the multi-output-pins design. **Pin order matches the def's authored order.**

**Type resolution:** reads the def's authored fields from the registry, sets `parameters[0].data_type = DataType::Record(Schema)` (the cached `Schema` is canonical), sets `output_pins` to one `OutputPinDefinition` per field in authored order. Wires-into-removed-pins handled by `repair_output_pin_wires` after schema-change propagation.

**Eval:** reads the input record (canonical order internally) and emits `EvalOutput::multi(...)` with one entry per output pin in pin/authored order — fields are looked up by name (binary search on the canonical input). Pass-through coercion means the runtime record may carry extra fields beyond `Schema`; we ignore them. Fields declared in `Schema` but missing from the runtime value (an unreachable case under pass-through, but defensive code is cheap) emit `None` on the corresponding pin.

### `product`

**Property:**
- `Target: RecordType` — a named record type. The target's field list drives the node's input pin layout *and* the node's output element type.

**Inputs:** one pin per field of the def, named after the field, typed `Array[FieldType_i]`. **Pin order matches the def's authored order** (read from the registry at type-resolution time).

**Output (single pin):** `Array[Record(Target)]`.

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
- `{x: Int, y: Int}` — inline anonymous record-type literal in type position. Produces `RecordType { name: None, fields: Some(...) }`.

The "type identifier" position remains restricted to immediately after `[]` or as a parameter type — same scoping rule the array-literal design already documents.

## Flutter UI

### Generic Type Selector — Record Branch

The existing type-selector widget (used wherever a `DataType` is picked: `array_at.element_type`, `sequence.element_type`, `map.input_type` / `output_type`, `filter.element_type`, `fold.element_type` / `accumulator_type`, `expr` parameter types, `array_concat`/`array_append`/`array_len` element types) gains a new top-level option: **Record**.

Selecting Record reveals two sub-modes:

- **Named** — a dropdown listing all `RecordTypeDef`s in the project, plus a `+ New record type…` item that opens a small modal: enter a name → an empty record def is created and immediately selected. Produces `RecordType { name: Some, fields: Some(<cached>) }`.
- **Anonymous** — an inline schema editor: a list of `(name, type)` rows with add/remove/reorder. The type cell of each row recurses into the same generic type selector. Produces `RecordType { name: None, fields: Some(<inline>) }`.

In v1, only the **Named** sub-mode is exposed in node property UIs (`record_construct`, `record_destructure`, `product`). The **Anonymous** sub-mode is reserved for `expr` parameter type pickers (where anonymous records are natural). This keeps the bulk of the UI simple while not constraining the type system.

The widget is shared across every site listed above. Implementation: one Flutter widget, used everywhere `DataType` is selected.

### Per-Node Property UI

**`record_construct`, `record_destructure`, and `product`** — each has a single property: the named record type (`Schema` or `Target`). UI is the Named sub-mode of the Record dropdown above, with two affordances:

- *Edit definition…* — opens the schema editor for the selected def in the user-types panel.
- *New record type…* — same as the dropdown's `+ New` item, for convenience when the user realizes mid-build that they need a new type.

There are no per-pin field-name controls on `product` (the def supplies the names) and no inline schema editor on `record_construct` / `record_destructure`.

### Top-Level User-Types Panel

The existing left-side panel that lists custom node networks becomes a unified "User Types" panel with two kinds of entries:

- Node networks (existing).
- Record type defs (new).

A single tree/list view with a kind icon per row (function-arrow icon for networks, brace icon for records). Both kinds support: rename, delete, "go to definition" (opens the appropriate editor — graph view for networks, schema editor for records).

Schema editor for a record def: a list of `(field_name, type)` rows with add/remove/reorder. The type cell uses the generic type selector (which can recurse into another `Record(...)` selection — including a self-reference for recursive defs, which produces a stub at the cycle point). Editing fires the propagation pass on save.

A new-record dialog asks for a name and creates an empty-fields def. Empty record types are valid (top in the subtype lattice — every record is `<: Empty`).

### Wire and Pin Rendering

A pin of type `Record(_)` is rendered with a single neutral "record" color (no per-name hashing, since structurally compatible record types can have different names and we want the visual to reflect compatibility, not identity). Hover tooltip shows the name (when present) and the resolved field list (`Point { x: Int, y: Int }` for named, `{x: Int, y: Int}` for anonymous).

## Subtyping Examples

(`Point = {x: Int, y: Int}`, `Point3 = {x: Int, y: Int, z: Int}`, `PointF = {x: Float, y: Float}`, `Box = {p: Point3}`, `BoxXY = {p: Point}`.)

| Source | Destination | Result |
|---|---|---|
| `Record(Point3)` | `Record(Point)` | ✓ width |
| `Record(Point)` | `Record(Point3)` | ✗ missing `z` |
| `Record(Point)` | `Record(PointF)` | ✓ depth + Int→Float |
| `Record(Box)` | `Record(BoxXY)` | ✓ depth + width |
| `Array[Record(Point3)]` | `Array[Record(Point)]` | ✓ array elt-wise |
| `Record(Foo)` where `Foo = {x: Int, y: Int}` | `Record({x: Int, y: Int})` (anonymous) | ✓ structural — names ignored |
| `Record({x: Int, y: Int})` (anonymous) | `Record(Point)` | ✓ structural |

## Migration

No migration of existing project files and no cnnd version bump. `record_type_defs` is absent in pre-record cnnd files; `#[serde(default)]` on the field produces an empty `HashMap`. Pre-record files contain no `DataType::Record` or `NetworkResult::Record` instances either, so deserialization is unaffected.

No existing nodes change behavior. `DataType::Record` is a new variant; every existing match on `DataType` is exhaustive over the old variants and a `_ => unreachable!()` arm becomes wrong — those will need to be expanded. (Use `cargo check` to find them.) Same for `NetworkResult` matches.

## Phasing

Each phase is shippable on its own. Phases 1–4 are the core; 5–9 layer features.

### Phase 1 — Type and value plumbing

- Add `DataType::Record(RecordType)` with the struct shape above. Constructors canonicalize fields (sort by name) and enforce invariants.
- Add `NetworkResult::Record(...)` (no type name); constructor canonicalizes fields.
- `infer_data_type` on a record value returns an anonymous `RecordType` (already canonical because the value's fields are).
- Add a same-name short-circuit in `can_be_converted_to` for record types (full subtyping comes in Phase 4).
- Update all match-on-`DataType` and match-on-`NetworkResult` sites.

Tests: equality of two `RecordType`s constructed from differently-ordered field lists yields `==` and equal `Hash`; anonymous-vs-named distinction in equality; dangling references reported as errors. No subtyping, no UI, no nodes.

### Phase 2 — Top-level defs, propagation, and serialization

- Add `NodeTypeRegistry::record_type_defs` (HashMap).
- `add` / `delete` / `rename` / `update` operations with namespace-collision checks against `node_networks` and built-ins.
- **Cycle check** on add and update: walk the def's transitive references and reject if it references itself. O(V + E) DFS on the named-record dependency graph.
- **Propagation pass**: a `DataType` walker that visits every record reference in every network's nodes and pin types, applied on rename and on field updates. Modeled on `rename_node_network` / `repair_node_network`.
- .cnnd schema bump and migration. Serializable form for record defs and for `RecordType` (always with cached fields).
- On-load consistency check: cached fields are reconciled against the registry; mismatches trigger a propagation pass. The cycle check is also re-run on load (defensive against hand-edited files).
- `RenameRecordTypeDef`, `DeleteRecordTypeDef`, `UpdateRecordTypeDef`, `AddRecordTypeDef` undo commands (snapshot-based, same shape as `RenameNetwork`/`DeleteNetwork`).

Tests: register, rename, delete; serialization roundtrip with nested defs (`Box = { p: Point }`); cycle rejection (direct and mutual); namespace collision rejection; propagation correctness (rename and update both refresh cached fields throughout a network).

### Phase 3 — `record_construct` and `record_destructure`

- New node types in `rust/src/structure_designer/nodes/`.
- Parametric properties via `calculate_custom_node_type` (modeled on `array_at`). Cached `Schema.fields` consumed directly.
- `record_destructure` uses multi-output pins.
- Schema-change repair: when `update_record_type_def` propagates new fields, walk these nodes and refresh their pin layout; disconnect now-incompatible wires via `repair_node_network`.

Tests: construct/destructure roundtrip; schema-change wire repair; missing-input propagation; nested-def construct (e.g. `Box` containing a `Point`).

### Phase 4 — Subtyping

- Add the full record branch to `can_be_converted_to` (width + depth + field-type promotion).
- Width subtyping at pin connect time.
- Element-wise subtyping in arrays falls out of the existing `Array` recursion.

Tests: subtyping table from the Examples section; anonymous-named compatibility in both directions.

### Phase 5 — Flutter type-selector record branch

- Generic Record branch in the type selector widget (Named sub-mode + Anonymous sub-mode).
- Used at every existing type-selector site.
- Node property UIs use Named sub-mode only.

### Phase 6 — Flutter user-types panel

- Unified list/tree of networks + record defs.
- Schema editor for a def.
- Rename / delete / new-record actions, hooked to the API operations from Phase 2.

### Phase 7 — Expression language

- `Expr::RecordLiteral { fields }` and parser support for `{x: 1, y: 2}` (no type-name prefix; produces an anonymous `RecordType`).
- Generalize `.<ident>` field access to records.
- Type-expression grammar accepts both named identifiers and inline `{...}` schemas.

Tests: roundtrip `{x: 1, y: 2}.x + 1`; nested literals; type-checking of literal-with-mixed-promotions; subtyping of literal into a named-record pin.

### Phase 8 — `product` node

- `Target` property (a named `RecordType`).
- Pin layout derived from the resolved target def.
- Output: `Array[Record(Target)]`.

Tests: 2-field product, 3-field product, empty-input product (any axis), large product (cardinality math), product into a recursive def's array field.

### Phase 9 — Polish

- Pin/wire rendering for record types (single neutral color, hover tooltip with resolved schema).
- "Edit definition…" / "New record type…" affordances on `record_construct` / `record_destructure` / `product`.
- Reference guide entry under `doc/reference_guide/nodes/`.

## Open Questions

1. **Empty record `Empty = {}`.** Allowed by construction (a def with zero fields). Top of the subtype lattice (every record is `<: Empty`). No design action needed; just document it.
2. **Field-name conflicts with reserved identifiers.** A field literally named `x` conflicts with the vector-member rule when records and vectors both use `.x`. Field access on a `Record` resolves through the schema (no ambiguity at static type), but the user-facing error message when they confuse the two should be clear.
3. **`product` axis order.** Documented as "rightmost field varies fastest." Worth confirming before users build muscle memory; flipping later would be a breaking semantic change.
4. **Cached-fields divergence on hand-edited .cnnd.** A user could hand-edit a .cnnd file and end up with cached `fields` that disagree with the def, or with a cycle that was rejected at edit time but smuggled in by hand. Phase 2 handles both with an on-load reconciliation pass: cached fields are refreshed from the registry; the cycle check is re-run; cyclic defs are rejected with a load error. Worth checking that this matches the project's conventions for divergent-data recovery.
