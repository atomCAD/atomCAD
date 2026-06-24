# Design: Identity vs. Naming — record defs referenced by id

**Status:** Proposal / implementation brief. Phase 0 (the invariant checker,
`doc/design_identity_vs_naming_phase0.md`) is **shipped**. This document
specifies the **one** refactor we intend to implement in the near term:
**referencing named record type defs by a stable id instead of by name.**
**Scope (near-term):** `data_type.rs` (`RecordType`), `node_type_registry.rs`
(record-def storage, rename, the rewrite walk), the three record nodes'
schema/target fields, the serialization boundary, and the type-rendering paths.
**Explicitly out of scope** (see §9): node-type ids, parameter/argument ids,
and record-*field* renaming.

> **Representation decision (the spine of this doc):** a record reference in
> memory carries **identity, never a name**. Concretely
> `RecordType::Named(RecordRef)` with
> `RecordRef::Resolved(RecordDefId) | RecordRef::Dangling(String)` — a **sum**
> (exactly one of id *or* orphan-name), never a struct holding both. The on-disk
> and on-screen *name* is **derived from the registry** at the boundary, the way
> atomCAD already derives every other on-disk shape from its in-memory editing
> types (the `Serializable*` DTO family). See §4.2.

---

## 1. The principle, and the one place we apply it now

> **References store identity. Names are a derived/presentation view — never the
> binding.**

We have repeatedly shipped the same bug: a **record rename leaves stale
`Record(Named(old))` references** scattered through the graph, because the rename
is implemented as a hand-maintained walk that rewrites every embedded copy of the
name, and that walk has drifted out of sync with reality (the `closure` / `apply`
/ `collect` omissions — see
`rename_wire_loss_regression_test::record_rename_rewrites_closure_and_collect_type_fields`).

The root cause is that **a record def's name is used as its identity**. A
`DataType` references a record def via `RecordType::Named(String)`; the registry
is keyed by that same string. So renaming the def forces the system to *find and
rewrite every place that used the old name*. That walk
(`rewrite_record_name_in_registry`) is:

- **fragile** — correctness depends on a hand-maintained downcast list being
  *exhaustive*, and nothing enforces it (it has silently missed node types
  twice); and
- **needlessly global** — a pure relabel touches the whole document.

A record def's name is a **safe** thing to demote to a pure label, because
**records are structurally typed**: `Named("Box")` and `Named("Crate")` with the
same fields are already interchangeable, and a named def is interchangeable with
a matching anonymous record. The name does **not** gate type compatibility (only
the *fields* do). So making the def name a mutable label with no binding power
changes no typing semantics — it only deletes the rewrite walk.

> **Why not also node-type names, parameter positions, record *fields*?**
> Those are deliberately deferred or rejected — see §9.

---

## 2. What this deletes, and what it moves (the honest payoff)

When the refactor lands:

- **`rewrite_record_name_in_registry`** (`node_type_registry.rs:~1342` site +
  the ~154-line downcast chain) — **deleted**.
- **`NodeTypeRegistry::rename_record_type_def`** + `_unchecked`'s rewrite call —
  the rename body collapses from "move key + rewrite the whole registry" to "set
  the def's `name` field + update one reverse-index entry" (~30 lines → ~6). The
  rename undo command shrinks to a name+index swap (no graph snapshot).
- **The three-way hand-maintained downcast list** (the rewriter's `&mut` list,
  the read enumerator `collect_record_refs_in_node`, and `canonicalize_node_data`
  that must "stay in sync") collapses to **one** shared traversal used by the
  serialization-boundary conversion (§4.3). The drift bug class is structurally
  eliminated.
- **The per-rename `repair_all_networks` cascade** for renames goes away (renames
  no longer change any pin layout — only field edits do, which keep their
  existing `repair` path).

But be honest about what *moves* rather than disappears. Deleting the rename walk
means embedded refs are never rewritten on rename, so **a resolved ref no longer
carries a usable name at all** — and therefore every place that renders a record
*name* (serialization, text format, FFI labels, type-mismatch messages) must
**resolve `id → name` through the registry** instead of reading it off the type.
That rendering change is **not optional and not specific to this representation**:
even if we kept a name embedded in the ref, it would be *stale the instant a
rename runs* (the rename is O(1) and never visits it), so authoritative name
rendering has to go through the registry either way. This refactor simply makes
that truth explicit. The net is therefore **roughly LOC-neutral** (§10) — the win
is **correctness and architecture**, not line count: the silent-drift class is
gone, the live graph holds **zero** duplicated names, and a rename is reflected
everywhere instantly because nothing caches the old name.

---

## 3. The reference sites (today) and the target

| # | Reference site | Today | Target (in memory) | On disk / text |
| - | -------------- | ----- | ------------------ | -------------- |
| 1 | embedded type ref | `RecordType::Named(String)` in any `DataType` | `RecordType::Named(RecordRef)` (id, or orphan-name if dangling) | **name** (unchanged, readable) |
| 2 | `record_construct.schema` | `String` | `RecordRef` | **name** |
| 3 | `record_destructure.schema` | `String` | `RecordRef` | **name** |
| 4 | `product.target` | `String` | `RecordRef` | **name** |
| 5 | registry storage | `HashMap<String, RecordTypeDef>` | unchanged key + `id` field + `id→name` reverse index | n/a |

Anonymous records (`RecordType::Anonymous(Vec<(String, DataType)>)`) are **not
touched** — they have no def to rename, and they remain name-keyed structural
schemas (§4.8).

---

## 4. The design in detail

### 4.1 The id, its allocation, and the registry indices

```rust
/// Document-scoped stable surrogate key for a named record def. Allocated once,
/// never reused within a document, remapped on cross-document import.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct RecordDefId(pub u64);
```

`RecordTypeDef` gains an `id`:

```rust
pub struct RecordTypeDef {
    pub id: RecordDefId,          // NEW — stable identity
    pub name: String,             // now a pure mutable label
    pub fields: Vec<(String, DataType)>,
}
```

`NodeTypeRegistry` **keeps the existing `record_type_defs: HashMap<String,
RecordTypeDef>` name-keyed map** (so the ~16 `lookup_record_type_def(name)` call
sites are unchanged) and adds a reverse index plus an allocator:

```rust
pub record_def_name_by_id: HashMap<RecordDefId, String>, // id → current name
pub next_record_def_id: u64,                             // monotonic; floor recomputed on load
```

> **On the two registry maps (the "is this redundant?" question).** This is the
> *benign* kind of duplication, and it is the only place names are duplicated at
> all. The single source of truth is the `RecordTypeDef` (it owns both `id` and
> `name`); `record_type_defs` (name→def) and `record_def_name_by_id` (id→name)
> are just two lookup directions over that one object, updated **together in the
> one `rename` method**, and the whole thing is O(number of defs) — a few dozen,
> not graph-scale. That is standard bidirectional indexing, not drift-prone
> duplication. The dangerous, graph-scale duplication — a name copied into every
> embedded ref — is exactly what §4.2 removes.

- **Allocation invariant** (mirrors the F1 `next_param_id` discipline): on add,
  hand out `next_record_def_id` then bump; on load, recompute
  `next_record_def_id = max(existing id) + 1`; never recycle.
- **Built-in record defs** (`built_in_record_type_defs`, e.g. `ElementMapping`)
  get **fixed, well-known ids** assigned at registry construction. Never renamed.
- Accessors: `lookup_record_type_def_by_id(id) -> Option<&RecordTypeDef>` (chains
  through `record_def_name_by_id` then `record_type_defs`),
  `record_def_id(name) -> Option<RecordDefId>`,
  `record_def_name(id) -> Option<&str>`.

A thin **`RecordNameResolver`** view (`record_def_id(name)` / `record_def_name(id)`)
is the only capability the serialization/rendering boundary needs; the registry
implements it. Threading a narrow resolver — rather than the whole registry — keeps
the boundary honest about what it touches.

### 4.2 The in-memory reference: a sum, not a struct

The crux. A resolved reference carries **only** identity. A reference that failed
to resolve at load time carries **only** its orphan name (so it round-trips and
produces a good error). Never both — so there is nothing to keep in sync.

```rust
/// A reference to a named record def. Exactly one of two states; they never
/// coexist, so there is no id/name pair to drift.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecordRef {
    /// Bound to a registry def by stable id. Carries NO name — the display /
    /// on-disk name is derived from the registry (`record_def_name(id)`).
    Resolved(RecordDefId),

    /// A load-time (or hand-edited-file) reference whose name matched no def.
    /// Keeps the orphan name verbatim for round-trip + error messages; never
    /// acquires an id. Surfaces as today's dangling-ref type error.
    Dangling(String),
}

pub enum RecordType {
    Named(RecordRef),                      // was: Named(String)
    Anonymous(Vec<(String, DataType)>),    // unchanged
}
```

**Equality/hashing derive cleanly and are correct by construction:** two
`Resolved` refs are equal iff their ids match; two `Dangling` refs iff their
orphan names match; a `Resolved` is never equal to a `Dangling`. `DataType`
derives `PartialEq`/`Eq`/`Hash` (used in `ValidationContext` keys and dedup) and
keeps working with **no hand-written impls** — a `u64` and a `String` both have
sound derives, and there is no name-shadow field to accidentally fold into
identity. (This deletes the "must hand-write id-only `Eq`/`Hash`" hazard a
prior `{ id, name }` draft carried.)

Constructors: `RecordRef::resolved(id)`, `RecordRef::dangling(name)`. The parser
and the deserialize path produce a transitional name-only form that the
post-load resolve pass (§4.3) turns into `Resolved` or `Dangling`.

### 4.3 The serialization & rendering boundary: derive the name, don't store it

Names are derived from the registry at exactly two kinds of boundary — when
bytes/characters cross in or out, and when a name is shown to the user. atomCAD
already concentrates "on-disk shape ≠ in-memory shape" in its `Serializable*`
DTO family and the `to_string`/`from_string` type-string seam; this slots in
there.

**(a) Load (`name → RecordRef`).** The `.cnnd` deserializer already produces a
name-shaped reference; the conversion functions resolve it:
`record_def_id(name)` → `Resolved(id)` if found, else `Dangling(name)`. Two
sub-paths:

- **Signature types** (`SerializableParameter.data_type` /
  `SerializableOutputPin.data_type` / zone pins) already round-trip via
  `DataType::from_string` (see `node_type_to_serializable`). Give them a
  resolver-aware sibling `DataType::from_type_string(s, &resolver)` that emits
  `Resolved`/`Dangling` for the record arm. (Plain `from_string` stays for
  contexts with no resolver and yields `Dangling`, which the next validate pass
  re-resolves.)
- **Node-data blob** (`SerializableNode.data: serde_json::Value`, produced by the
  per-node-type `node_data_loader`): see the `SaveContext` note below.

**(b) Save (`RecordRef → name`).** The mirror: `Resolved(id)` →
`record_def_name(id)` (always current — a rename updated the registry, so this is
fresh by construction); `Dangling(name)` → its preserved orphan name. Same two
sub-paths (`to_type_string(&resolver)` for signatures; `node_data_saver` for the
blob).

**(c) Render (`RecordRef → name`, on screen).** The text-format serializer
(`text_format/serializer.rs` renders `DataType` via `to_string()`), the FFI type
labels, `node_type_introspection`, and type-mismatch error messages must render
the **current** name. These switch to `to_type_string(&resolver)` (or a small
`DataType::display_with(&resolver)` helper). The bare `impl fmt::Display for
DataType` stays total for registry-less contexts (`Debug`, internal asserts,
logs) but renders a **resolved** record as `Record(#<id>)` — explicitly a
debug-only form, never written to `.cnnd` or shown in the UI — while a
**dangling** record still renders `Record(<name>)` from its own payload.

> **Why the render change is unavoidable (not a wart of this representation).** A
> rename is O(1) and never visits embedded refs, so *any* cached name on a ref
> would be stale until the next save. Authoritative names therefore *must* be
> pulled from the registry by id at render time. The `{ id, name }` struct a
> prior draft proposed would have shown **stale** names in the UI after a rename
> until a save/reload cycle. The id-only ref makes the only correct behaviour the
> only available one.

**The `SaveContext` thread (the node-data blob).** Per-node data is serialized by
the fn-pointer `node_data_saver` / `node_data_loader` on `NodeType`, which run
serde on the concrete data struct (`RecordConstructData`, `ExprData`, `MapData`,
…) and have **no registry**. They already take a non-serde context parameter —
`design_dir: Option<&str>` — for exactly this reason (serde can't reach external
state). Generalize that one parameter into a context struct:

```rust
struct SaveContext<'a> {
    design_dir: Option<&'a str>,
    record_names: &'a dyn RecordNameResolver,
}
```

Widen the saver/loader signature from `(…, design_dir)` to `(…, ctx)` — a
one-time, mechanical touch across the ~47 node types, of which only the
**record-bearing** ones (`record_construct` / `record_destructure` / `product`
for their schema/target field; `expr` / `map` / `filter` / `fold` for embedded
`DataType`s that may be `Record(Named)`) actually read `record_names`. Those
savers convert `Resolved(id) → name` on save and `name → Resolved/Dangling` on
load; everyone else ignores the new field. Bundling into a struct means the *next*
"savers need context X" never re-touches all 47 again.

Both directions are driven by **one** `walk_record_refs_in_*_mut` enumerator (the
single authoritative downcast list) — the read-only `collect_record_refs_in_node`
becomes a thin wrapper over it. So there is now exactly **one** ref-traversal list
total, exercised on both load and save; a node type missing from it fails
**loudly** (its refs never resolve → they sit `Dangling` → the §7 invariant and
the existing dangling-ref error fire at load), rather than silently corrupting on
a future rename.

### 4.4 The three schema/target fields

`record_construct.schema`, `record_destructure.schema`, and `product.target`
become `RecordRef` (replacing the bare `String`). They are already visited by
`collect_record_refs_in_node` under `RecordRefSite::Schema`, so the unified `_mut`
enumerator covers them on both boundaries (§4.3). Their `eval` / pin-layout code
that currently does `registry.lookup_record_type_def(&self.schema)` switches on
the ref: `Resolved(id) => lookup_record_type_def_by_id(id)`, `Dangling(_) =>`
empty-schema fallback (preserving the current "unset / unknown schema" behaviour).

### 4.5 Updates inside `data_type.rs` (mechanical, ~one file)

Every `RecordType::Named(name)` site changes to bind the `RecordRef`. The notable
ones:

- **`resolve_fields`** (the hot resolution): `Named(RecordRef::Resolved(id)) =>
  registry.lookup_record_type_def_by_id(id)`; `Dangling(_) => None` (dangling, as
  today).
- **`can_be_converted_to` record arm:** the `Named(s) == Named(d)` short-circuit
  becomes `Resolved(a) == Resolved(b)` by id (cleaner and faster). The structural
  fallback (resolve both to canonical field lists, match fields by name) is
  **unchanged** — field matching stays name-based, which is correct (§9).
- **`fmt::Display`** → `Record(#<id>)` for `Resolved` (debug-only), `Record(<name>)`
  for `Dangling`; the user-facing/round-trip name comes from
  `to_type_string(&resolver)` (§4.3 (c)).
- **`from_string` / parser:** constructs the name-only transitional form; the
  resolver-aware `from_type_string` (or the post-parse resolve step) turns it into
  `Resolved`/`Dangling`.
- **`walk_data_type_record_names` / `_mut`:** the closure type changes from `&str`
  / `&mut String` to `&RecordRef` / `&mut RecordRef`. Recursion arms unchanged.
  (Keep the two in byte-for-byte sync as the existing comment demands.)

This is the bulk of the mechanical edit (~38 match sites), all inside one file,
compiler-driven once the enum payload changes.

### 4.6 Rename becomes O(1)

```rust
pub fn rename_record_type_def(&mut self, old: &str, new: &str)
    -> Result<(), RecordTypeDefError>
{
    // …existing built-in / not-found / collision guards, unchanged…
    let mut def = self.record_type_defs.remove(old).unwrap();
    def.name = new.to_string();
    self.record_def_name_by_id.insert(def.id, new.to_string()); // reverse index
    self.record_type_defs.insert(new.to_string(), def);
    Ok(()) // NO rewrite_record_name_in_registry, NO repair_all_networks cascade
}
```

Embedded refs hold `id`; they are unaffected. Every render and every save resolves
`id → name` fresh, so the new name appears **immediately and everywhere** — no
stale-name window, no walk. `rename_record_type_def_unchecked` (undo/batch) loses
its rewrite call identically; the rename undo command shrinks to "swap the def's
name + reverse-index entry."

> **Field edits** (`update_record_type_def`) are unaffected and keep their
> `repair_node_network` pass: changing a def's *fields* still re-derives
> `record_construct`/`destructure`/`product` pin layouts and disconnects
> now-incompatible wires (an id-stable ref automatically sees the new schema).

### 4.7 Import / paste-from-library (remap)

Importing a `.cnnd` library brings in record defs with *their* document's ids.
On import: allocate fresh local ids for each imported def, build an
`imported_id → local_id` map, and remap every imported `RecordRef::Resolved(id)`
through it. **Name-based merge/conflict stays the policy at the boundary:** if an
imported def's name matches an existing local def, merge or rename-on-conflict per
the existing import rules — names remain authoritative *across documents*. A
`Dangling` import stays dangling unless its name happens to match a local def, in
which case the import resolve binds it. Because in-memory refs are already
id-based post-load, the remap is a single pass over the imported subgraph using
the same `_mut` enumerator. This is the one new sharp edge and earns a dedicated
test (§8).

### 4.8 Anonymous records: unchanged

`RecordType::Anonymous` stays `Vec<(String, DataType)>`, name-keyed, no id.
Structural subtyping between a `Named(Resolved(id))` and an anonymous record
resolves the named side to its fields and matches **by field name** against the
anonymous side. Field-level matching therefore stays name-based on both sides —
the correct, intended semantics (§9).

---

## 5. Implementation phases (each independently shippable)

> Each phase compiles and passes the suite. Land in order; stop after any phase
> and the system is strictly better.

**Phase R1 — ids on defs + registry index (no behaviour change).**
Add `RecordDefId`, `RecordTypeDef.id`, `record_def_name_by_id`,
`next_record_def_id`, the allocator + floor-on-load, built-in fixed ids, the
`*_by_id` / `record_def_id` / `record_def_name` accessors, and the
`RecordNameResolver` view. `RecordType::Named` still carries a `String`.
Everything else unchanged. Pure addition; wire "every def has a unique stable id,
recomputed on load, never recycled" into the Phase 0 invariant checker
(`DuplicateRecordDefId`, `RecordDefIdFloor`).

**Phase R2 — `RecordRef` sum + the boundary (resolve / refresh / render).**
Change `RecordType::Named(String) → Named(RecordRef)` and the three
schema/target fields. Add the unified `walk_record_refs_in_*_mut` enumerator. Add
`DataType::from_type_string` / `to_type_string(&resolver)` and route the signature
seam through them. Introduce `SaveContext` and widen the `node_data_saver` /
`node_data_loader` signature; convert id↔name in the record-bearing savers. Switch
the render sites (text format, FFI labels, introspection, error messages) to the
resolver-aware path; leave bare `Display` as the `#id` debug fallback. Update
`data_type.rs` (§4.5) and the three record nodes (§4.4). At this point the
rewriter still exists but is **dead on rename** — verify by routing rename through
the new O(1) body while keeping `rewrite_record_name_in_registry` only until R3.

**Phase R3 — delete the walk.**
Remove `rewrite_record_name_in_registry`, drop its call from
`rename_record_type_def` / `_unchecked`, drop the now-unneeded
`repair_all_networks` *on rename* (keep it on field-edit/delete). Collapse the
read enumerator to share the R2 traversal. Simplify the rename undo command.

**Phase R4 — import remap + hardening.**
Implement §4.7 (fresh-id remap on import/paste-from-library), the dedicated remap
test module, and the dangling-ref round-trip test.

---

## 6. Migration / backward compatibility

**No `.cnnd` format change.** On disk, record references remain names
(`Record(Name)` in type strings; `"schema": "Name"` on the record nodes). Old
files load unchanged: deserialize yields names, the load resolve assigns
`Resolved`/`Dangling`, save writes names back. A name that doesn't resolve
(hand-edited or genuinely dangling file) becomes `Dangling(name)` and surfaces as
today's dangling-ref type error, with the original name preserved in the message
and on re-save. **No version bump, no migration shim, no determinism worry.**

---

## 7. Interaction with the Phase 0 invariant checker

Add to `invariants.rs`:
- `DuplicateRecordDefId` (Tier 1) — two defs share an id.
- `RecordDefIdFloor` (id-counter floor, like `ParamIdFloor`: fatal in
  `check_document_invariants`, excluded from the hot debug assert).
- `UnresolvedRecordRef` — a `RecordRef::Dangling(name)` whose `name` *does*
  resolve in the registry (i.e. a resolve pass was skipped or an enumerator site
  was missed). This catches a missed boundary traversal loudly. (A `Dangling`
  whose name does **not** resolve is a legitimate user-reachable dangling ref and
  stays accounted-for, not fatal.)

The existing `UnresolvedRecordName` / schema checks keep working (they consult
`lookup_record_type_def` by name, which still exists).

---

## 8. Test plan

1. **Rename no longer rewrites embedded refs:** the existing
   `rename_wire_loss_regression_test` (esp.
   `record_rename_rewrites_closure_and_collect_type_fields`) must pass with the
   rewriter **deleted** — proving id-stability, not exhaustive-walk correctness.
2. **Rename reflects everywhere instantly:** rename `Foo→Bar`, then *without
   saving* assert the rendered type strings (text format / FFI label path) read
   `Bar` — proving render-through-registry, not a cached name.
3. **Round-trip:** save→load a network whose nodes embed `Record(Foo)` in
   `parameter` / `expr` / `map` / `closure` / `apply` / `collect` / record nodes;
   assert all refs re-resolve to the same id and the on-disk names match the def's
   current name.
4. **Rename then save:** rename `Foo→Bar`, save, assert every embedded reference
   serialized as `Bar`, and reload resolves them to one id.
5. **Dangling round-trip:** a ref to a missing def stays `Dangling`, keeps its name
   across save/load, and surfaces a dangling-ref error (no panic, no silent drop).
6. **Field edit still repairs:** `update_record_type_def` changing a field type
   still disconnects now-incompatible record-node wires (unchanged path).
7. **Import remap:** import a library whose defs' ids collide numerically with
   local ids; assert no cross-contamination and name-based merge/rename-on-conflict.
8. **Phase 0 invariants** extended (§7) run across the property/round-trip axis.

---

## 9. Non-goals (explicit, with rationale)

- **Node-type ids (`node_type_name` → `TypeId`).** Deferred indefinitely. It is
  the *most pervasive* change in the codebase: `node_type_name` is on every
  `Node`, the registry is name-keyed (80+ lookup sites), it is compared against
  string literals in ~35 places, and it is a serialization / text-format / FFI
  token (~180 edit sites total). The network-rename cascade it would delete
  (`apply_rename_core` rewriting `node_type_name`) is real but **not fragile**
  (one field, one walk) — unlike the record rewriter's drifting 17-type list. Low
  value-to-cost; treat as a separate future effort if ever justified.

- **Parameter / argument ids (positional wire destinations).** Dropped. The acute
  bug (parameter reorder/add jumbling wires across networks) is **already fixed**
  by the landed F1–F6 of `doc/design_parameter_wire_stability.md` (regression
  tests green). Removing it leaves no open bug.

- **Record *field* renaming.** Rejected as a relabel-style change. A record field
  name is **structural type identity**, not a label: `can_be_converted_to` matches
  record fields **by name**, and anonymous records (`{x: Int}`) carry *only* names
  with no def to host an id. So renaming a field genuinely produces a different
  type (`{x: Int}` ≠ `{y: Int}`) — correctly handled today as a *schema edit*
  (`update_record_type_def` → `repair_node_network`), not a free rename.

---

## 10. Honest cost / line tally

The earlier draft claimed a net deletion. With the rendering boundary surfaced
(§2, §4.3 (c)) that is **no longer true**, and pretending otherwise would
mis-sell the change. The accounting:

| Area | Δ |
| --- | --- |
| Delete `rewrite_record_name_in_registry` + downcast chain | **−154** |
| Rename method/undo collapse (rewrite call + cascade gone) | **−25** |
| Add `RecordDefId`, registry `id`/reverse-index/allocator/accessors/`RecordNameResolver` | **+50** |
| `RecordRef` sum type (derives — **no** manual `Eq`/`Hash`) | **+12** |
| Unified `walk_record_refs_*_mut` (replaces 3 lists with 1) | **+10 net** |
| Load resolve + save refresh, folded into DTO conversion | **+35** |
| `SaveContext` + saver/loader signature widen (~47 mechanical sites) | **+30** |
| `to_type_string`/`from_type_string` + render sites switched to resolver | **+55** |
| `data_type.rs` Named-payload edits (~38 sites, mechanical) | **+25** |
| 3 record nodes: `String` schema/target → `RecordRef` | **+15** |
| Import remap (Phase R4) | **+30** |
| **Net** | **≈ +80 to +110 source lines** |

So: a **modest net increase in LOC**, concentrated in mechanical signature/render
churn. The justification is **not** line count — it is:

1. the fragile, twice-broken, three-way-synchronized rename walk is gone, and the
   entire "stale `Record(Named(old))` after rename" bug class is eliminated **by
   construction** (a missed traversal site now fails loudly at load, not silently
   on a later rename);
2. the live graph holds **zero** duplicated record names — identity and naming are
   cleanly separated, with the name derived from the one source of truth (the
   registry) at every boundary; and
3. a rename is reflected **instantly and everywhere**, because nothing anywhere
   caches the old name.

The blast radius is concentrated in `data_type.rs`, the record registry, the
serialization conversion functions, and the type-render sites — and crucially, **the
`.cnnd` format, the text format on disk, and every name-based *lookup* site are
untouched.**

---

## 11. Relationship to existing docs

- `doc/design_identity_vs_naming_phase0.md` — the shipped invariant checker; §7
  here extends it with record-def-id invariants.
- `doc/design_parameter_wire_stability.md` — the tactical fix for the
  parameter-order bug; §9 here explains why the strategic argument-id change is
  dropped (F1–F6 already contain the acute bug).
- `doc/design_record_types.md` — defines the record/field model this doc id-keys;
  the structural-typing semantics in §9 ("fields match by name") come from there.
- `doc/design_custom_node_type_cache_invariant.md` — its "Non-goals" named the
  deeper fix; this doc delivers it for the record-name axis (and explains in §9
  why the node-type axis is deferred).
- The `Serializable*` DTO family (`serialization/node_networks_serialization.rs`)
  and the `node_data_saver`/`design_dir` precedent are the house pattern §4.3
  builds on: in-memory editing types never derive `Serialize`; on-disk shape is
  produced by explicit, context-carrying conversion functions.
