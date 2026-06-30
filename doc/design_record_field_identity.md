# Design: Record field identity — stable `FieldId` for wire stability across rename/reorder

**Status:** Proposal / implementation brief. Closes issue **#377** ("renaming a
record parameter field drops wires from delayed closure inputs to such field
inputs, non-locally!"). Reproduced live 2026-06-30 at three altitudes (top-level
`record_construct`, a delayed function in another network via `map.f`, and the
literal inline-closure-body case from the title).

**Scope:** the three record nodes (`record_construct`, `record_destructure`,
`product`), their pin-layout builder (`build_node_type_for_schema_with_defs`),
`RecordTypeDef` storage + a per-def id allocator, the `update_record_type_def`
API + Flutter `SchemaEditor`, and `RecordConstructData.literal_values` re-keying.
**Explicitly out of scope:** record-*def* identity (`RecordDefId`, owned by
`doc/design_identity_vs_naming.md`), structural type compatibility, and anonymous
records — all left exactly as they are (§3, §8).

> **Representation decision (the spine of this doc):** a record **field** carries
> a **stable `FieldId` (editing identity)** that is **orthogonal to its name
> (structural type identity)**. The name keeps gating type compatibility
> (records stay structurally typed, by name); the `FieldId` gates *wire
> preservation* on the construct/destructure/product nodes. A rename changes the
> name and keeps the `FieldId`; a delete+add changes the `FieldId`. That single
> distinction is the entire fix.

---

## 1. The bug, in one paragraph

`record_construct` exposes one input pin per field of its chosen def. Those pins
are built with **`id: None`** (`nodes/record_construct.rs:240`, in
`build_node_type_for_schema_with_defs`). Wire preservation in
`NodeNetwork::set_custom_node_type` (`node_network.rs:720-780`) prefers an
**id match** and falls back to a **name match**; with `id: None` only the name
match is available. Renaming field `b → bb` therefore looks identical to
"delete `b`, add `bb`": the new pin `bb` matches no old pin, `old_index` is
`None`, and the wire feeding that pin is **dropped**. Because `repair_node_network`
recurses into zone bodies (`repair_zone_body → repair_node_network`), the same
drop fires on `record_construct` nodes buried inside lazy `map`/`filter`/`fold`/
`foreach`/`closure` bodies — a wire vanishes in a body the user never opened,
while they were editing a *type definition* elsewhere. That is the "non-local!"
in the title.

The `Parameter.id` field already documents its own purpose: *"Persistent
identifier for wire preservation across renames"* (`node_type.rs:12`). Record
fields are the one pin source that declines to supply it. **This doc supplies it.**

---

## 2. The principle — and the §9 objection it must answer

`doc/design_identity_vs_naming.md` §9 **originally rejected** record-field
renaming as a relabel (the prior text, quoted here as the objection this doc
answers; §9 has since been amended to defer to this doc — see the resolution
below):

> *"A record field name is **structural type identity**, not a label:
> `can_be_converted_to` matches record fields by name, and anonymous records
> carry only names with no def to host an id. So renaming a field genuinely
> produces a different type (`{x: Int}` ≠ `{y: Int}`)…"*

That paragraph is **correct about types and wrong about wires** — it conflates two
different bindings:

1. **Record-VALUE compatibility (downstream).** Whether a `Record(...)` value
   flowing along a wire satisfies a consumer pin. This is *correctly* name-based:
   `{x: Int}` and `{y: Int}` are genuinely different record types, and a consumer
   expecting `{x: Int}` *should* stop matching after the rename. **This doc does
   not touch it.** `can_be_converted_to` stays name-based and structural.

2. **Field-INPUT-pin wires (on `record_construct`/`product`).** The wire we are
   dropping does **not** carry a record value — it carries the *ingredient* being
   assembled into one field. Its validity depends only on that **field's type**,
   not its name. Renaming `a:Int → aa:Int` leaves the pin's type `Int`, so an
   `Int` source still fits. Dropping that wire is **not** type-justified; it is a
   pure identity-tracking failure.

The §9 rejection reasoned entirely about (1) and let (2) be collateral. The fix
is to give (2) its own identity — exactly as the def-level doc gave record *defs*
an identity (`RecordDefId`) while leaving their *name* as the structural,
user-facing label. **`FieldId` is to a field what `RecordDefId` is to a def:**
the editing identity, separate from the structural name.

**Why this is safe (the three guards that make §9's worries moot):**

- **Construct/destructure/product always reference a *named* def.** They carry a
  `schema`/`target` string; there is no way to construct or destructure an
  *anonymous* record with these nodes. So a `FieldId` only ever has to live on the
  fields of a **named** def — the "anonymous records have no def to host an id"
  objection never reaches the wire-dropping path.
- **`FieldId` never enters `can_be_converted_to`.** Type compatibility is computed
  exactly as today (resolve named → fields, match fields **by name**). The id is
  invisible to the type system; it only steers `set_custom_node_type`'s argument
  re-association.
- **Genuinely-incompatible wires still drop.** If a rename makes the *output*
  record structurally incompatible with some downstream consumer, that wire drops
  via the unchanged name-based `can_be_converted_to` — independently of the
  `FieldId` machinery.

**Resolution in the sibling doc (done):** `design_identity_vs_naming.md` §9 has
been amended — the record-*field* bullet no longer "rejects" field renaming
wholesale; it now distinguishes the **structural-type axis** (field name, stays
name-based there) from the **wire-identity axis** (addressed here) and points to
this doc. See §11.

---

## 3. Correct behavior, enumerated

The schema editor commits a new field list. Each field edit must resolve as:

| Edit | Wires on the field's pins (construct input / destructure output / product input) |
| --- | --- |
| **Rename** (`b → bb`, type unchanged) | **Preserved** — top-level *and* inside every closure/HOF body. |
| **Reorder** | **Preserved** — wires follow the field, not the slot index. |
| **Retype, still-compatible** (`Int → Float` at the pin) | **Preserved** — the wire is kept by id and normal wire validation re-checks the source against the new pin type (an `Int` source still converts to a `Float` pin). |
| **Retype, incompatible** | **Preserved in `arguments`, surfaced as a validation type error** — *unchanged* behaviour. The repair layer does not drop type-mismatched wires (only missing-source / bad-pin-index wires); type mismatch is a blocking validation error, consistent with every other mistyped wire. The `FieldId` fix preserves the wire by id exactly as name-matching does today, so validation still flags it — no behaviour change. (An earlier draft wrongly said "dropped"; the red-first guard test proved otherwise — §7.) |
| **Delete** the field | **Dropped.** |
| **Add** a field | New, unwired pin. |

Today *rename* and *reorder* both wrongly collapse into "dropped"/"mis-wired,"
because the backend receives a bare `Vec<(name, type)>` and cannot tell a rename
from a delete+add, nor a reorder from independent edits. **Supplying the field's
identity with each edit is the missing information.**

---

## 4. The design

### 4.1 `FieldId` and the per-def allocator

Mirror the landed `next_param_id` discipline (`closure_network_conversion.rs:896-909`,
guarded by the Phase 0 invariants `invariants.rs:219-247`):

```rust
/// Def-scoped stable surrogate key for a record field. Allocated once from the
/// owning def's counter, never reused within that def, never reordered.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct FieldId(pub u64);
```

`RecordTypeDef` carries the id alongside each field and a monotonic counter:

```rust
pub struct RecordTypeDef {
    pub name: String,
    pub fields: Vec<RecordField>,   // was: Vec<(String, DataType)>
    pub next_field_id: u64,         // NEW — monotonic; floor recomputed on load
    // (RecordDefId from design_identity_vs_naming.md lands independently)
}

pub struct RecordField {
    pub id: FieldId,        // NEW — editing identity
    pub name: String,       // structural type identity (unchanged role)
    pub data_type: DataType,
}
```

- **Allocation invariant** (verbatim from `next_param_id`): on add, hand out
  `next_field_id` then bump; on load (and for legacy/migrated defs, §6), recompute
  `next_field_id = max(existing field id) + 1`; **never recycle**.
- **Built-in defs** (`built_in_record_type_defs`, e.g. `ElementMapping`) allocate
  ids at registry construction; they are never renamed, so any deterministic
  scheme works.
- `fields` keeps storing **authored order** — pin layouts still follow it (§4.3).
  The `FieldId` is *not* the order; reorder permutes `fields` while ids stay put.

> **Why a per-def counter, not a document-global one.** Field identity only needs
> to be unique *within its def* — that is the scope in which `set_custom_node_type`
> compares old vs. new pin lists. Per-def matches the `next_param_id` precedent
> (per-network) exactly, and keeps the Phase 0 invariant local and cheap.

### 4.2 The edit becomes a diff-with-identity

This is the indispensable piece. `update_record_type_def` must receive, per row,
**which existing field it is** (or that it is new). Widen the API field type:

```rust
// structure_designer_api_types.rs
pub struct APIRecordTypeField {
    pub id: Option<u64>,   // NEW: Some(field_id) = existing field; None = new field
    pub name: String,
    pub data_type: APIDataType,
}
```

`update_record_type_def(name, fields)` then computes the diff against the current
def by **id**:

- `Some(id)` present now **and** before → the field survives; **keep its id**,
  apply its (possibly new) name/type/position.
- `None` → a new field; **allocate** `next_field_id`.
- an id present before but **absent** now → deleted (its wires drop, correctly).

The Flutter **`SchemaEditor`** already holds each field as a persistent UI row; it
reads ids from `get_record_type_def` (extend `APIRecordTypeDef`/`APIRecordTypeField`
to surface `id`) and **echoes the row's id on commit**. A freshly added row sends
`None`. Renaming text in a row keeps the row's id → backend sees a rename. This
also makes **swaps** (`a→b`, `b→a` simultaneously) correct, which a "match by
previous name" heuristic could never handle.

> **The API stays a whole-list replace** (not a stream of rename/add/delete RPCs).
> One atomic commit with per-row identity expresses *any* combination of
> rename + reorder + retype + add + delete, and resolves each correctly. This is
> strictly more robust than a dedicated `rename_record_field` verb (§9).

### 4.3 Stamp the id onto the pins (the one-line core)

`build_node_type_for_schema_with_defs` (`nodes/record_construct.rs:219-249`,
shared by `record_destructure` and `product`) changes the field→pin map from
`id: None` to the field's id:

```rust
custom.parameters = def.fields.iter()
    .map(|f| Parameter {
        id: Some(f.id.0),                       // was: None
        name: f.name.clone(),
        data_type: f.data_type.record_field_pin_type(),
    })
    .collect();
```

That is the load-bearing change. With ids present on both the pre-update cached
`custom_node_type` and the post-update one, `set_custom_node_type`'s existing
**id-first** matching (`node_network.rs:720-780`) preserves the wire across a
rename or reorder with **no change to the wire layer**, and the body recursion
that caused the non-local drop now *preserves* non-locally instead. Incompatible
retypes and deletes still drop, via the unchanged downstream validation.

### 4.4 `record_destructure` outputs and `product` — the symmetric cases

- **`record_construct` inputs** and **`product` inputs** are `Parameter`s → fixed
  directly by §4.3 (id-keyed argument matching). This closes the **reported** bug.
- **`record_destructure` outputs** are a quieter, second defect. Output wires are
  stored on the *consumer* as `(source_node, output_pin_index)` and keyed by
  **index**, so a pure rename leaves them intact — but a **reorder** silently
  re-points every output wire at the wrong field's value. To make outputs honor
  field identity too, `OutputPinDefinition` needs an optional identity (a parallel
  `id: Option<u64>`), and the output-wire repair (`repair_output_pin_wires`) must
  remap by id when the source is a record node. This is a strictly additive,
  later phase (§5, R3); the rename fix (R1/R2) does not require it.

### 4.5 The `literal_values` casualty

`RecordConstructData.literal_values: HashMap<String, TextValue>` is keyed by field
**name** (`nodes/record_construct.rs:31`). A rename orphans a stored default
exactly like a wire (the map is "orphan-tolerant" — silently ignored). Two options:

- **(chosen) Rewrite keys on the rename diff.** `update_record_type_def` already
  knows the per-id `old_name → new_name` map; walk every `record_construct` bound
  to this def (via `walk_all_nodes_mut`, so **bodies are covered**) and rename its
  `literal_values` keys. No format change, no per-node id storage.
- (alternative) Re-key `literal_values` by `FieldId`. Cleaner long-term but forces
  a `.cnnd` format change; deferred unless we adopt persisted field ids anyway.

The walk must use `walk_all_nodes_mut`, **not** a bare `network.nodes` loop — the
same body-recursion lesson that the non-local wire drop teaches.

### 4.6 What does **not** change

- `DataType::can_be_converted_to` — record fields still match **by name**;
  `FieldId` is invisible to it.
- `RecordType::Anonymous` — untouched; anonymous records never reach construct/
  destructure/product and need no field ids.
- The `.cnnd` on-disk shape (§6).
- Record-*def* identity — `RecordDefId` and the rename-rewrite deletion remain the
  separate concern of `design_identity_vs_naming.md`. The two ids compose cleanly
  (def id = which def; field id = which field within it) but ship independently.

---

## 5. Implementation phases (each independently shippable)

> Each phase compiles and passes the suite; land in order; stop after any and the
> system is strictly better. Tests live **with the phase that makes them pass**.
> Tests tagged **[RED-FIRST]** are bug regressions to be **written against current
> `main` and observed failing before the phase's code change**, then observed green
> after — see §7 for the methodology. Tests tagged **[GUARD]** must stay green
> *before and after* (they protect correct behaviour from the fix).

**Phase R1 — ids on fields + allocator (no behaviour change).**
Introduce `FieldId`, `RecordField`, `RecordTypeDef.next_field_id`, the allocator +
floor-on-load, built-in ids. Migrate `Vec<(String,DataType)>` call sites to
`RecordField` (mechanical, compiler-driven). `build_node_type_for_schema_with_defs`
**still** emits `id: None` (so no wire-behaviour change yet). Add Phase 0
invariants `DuplicateFieldId` / `FieldIdFloor` (mirror `DuplicateParamId` /
`next_param_id` floor). Pure addition — nothing user-visible was broken, so there
is no red-first regression here; tests are forward-looking unit tests:

- `DuplicateFieldId` fires when two fields of one def share an id; `FieldIdFloor`
  fires when `next_field_id <= max(field id)` (mirror the param-id invariant tests
  in `invariants` coverage).
- **Allocator discipline:** add then delete then re-add fields; assert ids are
  never recycled. Round-trip a def through save/load; assert `next_field_id` is
  recomputed to `max(id)+1`.

**Phase R2 — turn it on (closes #377).**
Flip §4.3 to `id: Some(f.id.0)`. Widen `APIRecordTypeField.id` +
`update_record_type_def` diff-by-id; surface ids through `get_record_type_def`;
update the Flutter `SchemaEditor` to carry and echo per-row ids. Implement §4.5
literal re-key. Tests (write the [RED-FIRST] set **first**, watch them fail on
current code, then make this change and watch them pass):

- **[RED-FIRST] #377 closure-body regression** — the reported case: a
  `record_construct` inside a `map` body with the body's `element` source pin
  wired into a field; rename the field; assert the body wire **survives** and the
  body still evaluates the full record. *Red today: wire dropped non-locally.*
- **[RED-FIRST] top-level rename** — a wired top-level `record_construct` field;
  rename it; assert the wire survives and `eval` still yields the complete record
  (today it collapses to `None`). *Red today.*
- **[RED-FIRST] cross-network delayed function** — `record_construct` lives in
  network B, consumed via `map.f` in network A; rename a field; assert B's
  internal wire survives though B was never edited. *Red today.*
- **[RED-FIRST] name swap** — rename `a→b` and `b→a` in one commit; assert each
  wire follows its `FieldId`. *Red today: name-matching silently **mis-wires** the
  swap (wrong-but-not-dropped), the most insidious variant.*
- **[RED-FIRST] `literal_values` follow rename** — a field with a stored default
  (no wire), one top-level and one inside a body; rename; assert the literal
  survives under the new name. *Red today: orphaned/lost.*
- **[GUARD] field delete drops only its wire** — deleting field `a` drops `a`'s
  wire while `b`'s survives (and the orphaned wire is **not** migrated onto a
  surviving/new field). This is the real over-reach guard for the fix.
- **[GUARD] incompatible retype is unchanged** — retype a wired field to an
  incompatible type; the wire stays in `arguments` and validation flags a type
  error (it is **not** dropped at the repair layer — see the §3 table). Asserts
  the fix doesn't accidentally start dropping it.
- **[GUARD] downstream structural compatibility unchanged** — a consumer typed
  `Record(Anonymous{a:Int})` fed by `record_construct(Pair{a:Int})`; rename
  `a→aa`; assert *that output* wire drops (genuinely incompatible record value)
  **while** the construct's *input* wire survives — proving `FieldId` did not leak
  into `can_be_converted_to`.

**Phase R3 — destructure output identity (the reorder tail).**
Add `OutputPinDefinition` identity + id-aware `repair_output_pin_wires` for record
nodes (§4.4). Tests:

- **[RED-FIRST] destructure reorder mis-wire** — wire each `record_destructure`
  output to a distinct sink, reorder the def's fields, assert each sink still
  receives **its** field's value. *Red today: index-keyed output wires re-point to
  the wrong field after a reorder (silent).*
- **[GUARD] construct-input reorder** — reordering fields preserves
  `record_construct` input wires. (Already green pre-fix via name-matching; this
  documents that R3 keeps it green now via id-matching.)

**Phase R4 — undo + hardening.**
Ensure `UpdateRecordTypeDefCommand` round-trips ids. Tests:

- **ids + `next_field_id` survive undo/redo** of a field rename/reorder/add/delete
  (mirror the `next_param_id` undo test; restore the counter on undo).
- **Permutation property test** — across random sequences of
  rename/reorder/retype/add/delete, assert the invariant: *a wire is preserved iff
  its field's `FieldId` persists **and** the pin type stays compatible.*

---

## 6. Migration / backward compatibility

**No `.cnnd` format change** (matching the def-id doc's stance). On disk, defs
remain `{ "name": ..., "fields": [{"name":.., "type":..}] }` and record nodes keep
`"schema": "Name"`. On load, assign `FieldId`s deterministically in **authored
order** and set `next_field_id = len`. Wires reattach by name at load (their only
available key in the old file), and from that point forward carry id stability for
the session. Because ids are assigned, not read, **no version bump, no shim, no
determinism worry**. (Should we later re-key `literal_values` by id — §4.5
alternative — *that* would require persisting field ids and a format bump; it is
deliberately not in this plan.)

---

## 7. Test methodology (red-first for the regressions)

Tests are specified **inside the phases** (§5), each tagged **[RED-FIRST]** or
**[GUARD]**. The working order for a bug-fix phase (R2, R3):

1. **Write the [RED-FIRST] tests first**, against current `main`, in
   `rust/tests/structure_designer/` (per the repo's "tests in `rust/tests/`, never
   inline" rule). Run them and **observe red** — this proves the test actually
   exercises the bug and would catch a regression, not merely that it passes for an
   unrelated reason. The #377 closure-body test in particular must reproduce the
   exact reported failure (a body wire dropped by a rename done elsewhere).
2. **Land the phase's code change**, then **observe green** on the same tests, with
   no edit to the test bodies (editing a test to make it pass forfeits the proof).
3. **[GUARD] tests** are written alongside and stay green throughout — they fail
   only if the fix *over-reaches* (e.g. starts preserving a wire that a genuine
   type incompatibility should drop, or leaks `FieldId` into `can_be_converted_to`).

A short per-test note recording the observed red message (and the green after) goes
in the PR description, so the red→green transition is auditable. R1's tests are
ordinary forward-looking unit tests (no pre-existing behaviour to break, so no
red-first step); R4's permutation property test is written with R4.

**Already written and observed red (against current `main`):**
`rust/tests/structure_designer/record_field_rename_wire_loss_test.rs` holds the
first three R2 [RED-FIRST] regressions —
`top_level_field_rename_preserves_input_wire`,
`rename_does_not_drop_wire_in_other_network` (the non-local case), and
`field_name_swap_preserves_wires_by_identity` (the insidious mis-wire) — all
**failing** as designed, plus `guard_field_delete_drops_only_its_wire`
**passing**. These exercise the `StructureDesigner::update_record_type_def(name,
Vec<(String, DataType)>)` path.

> **API-signature caveat.** R2 widens that method (and `APIRecordTypeField`) to
> carry per-field ids (§4.2). When it lands, these tests' **call sites** adapt to
> the new signature (e.g. tagging which row is the renamed field) but their
> **assertions stay byte-identical** — adapting a deliberately-changed signature
> is not "editing the test to make it pass." Keep the assertions frozen across the
> red→green transition.

---

## 8. Non-goals (explicit)

- **Record-def identity / the rename-rewrite deletion** — owned by
  `design_identity_vs_naming.md` (R1–R4 there). Composes with this; ships
  separately.
- **Changing structural typing to id-based.** Rejected. Records are structurally
  typed by field name and stay so; cross-def and named↔anonymous compatibility all
  depend on it. `FieldId` is an editing concern only.
- **A `.cnnd` format change / persisted field ids.** Avoided (§6). Reconsider only
  if `literal_values`-by-id is adopted.
- **Field ids on anonymous records.** Unnecessary — they never reach the three
  record nodes.

---

## 9. Why not the cheaper `rename_record_field` verb?

A dedicated `rename_record_field(def, old, new)` that remaps wires + literal keys
by name via `walk_all_nodes_mut` would close the *rename* case with less surface
area. It is rejected as the destination because:

- it fixes rename **only**, leaving reorder (and the destructure-output mis-wire)
  broken;
- it forces the UI to special-case rename vs. every other field edit, where today
  (and under this design) the editor just commits a field list;
- it cannot express compound edits (rename + reorder + add in one commit), and
  mis-handles swaps.

It remains a reasonable **interim stopgap** if R2 must be deferred, but the
`FieldId` design subsumes it with essentially no extra wire-layer code (§4.3).

---

## 10. Honest cost

| Area | Δ |
| --- | --- |
| `FieldId` + `RecordField` + `next_field_id` + allocator/floor + accessors | **+45** |
| Migrate `Vec<(String,DataType)>` → `Vec<RecordField>` call sites (mechanical) | **+30** |
| `build_node_type_for_schema_with_defs`: `id: None → Some(f.id.0)` | **+1** |
| `APIRecordTypeField.id` + `update_record_type_def` diff-by-id + `get_record_type_def` surface | **+40** |
| Flutter `SchemaEditor` carry/echo per-row id | **+20** |
| `literal_values` re-key on rename (walk_all_nodes_mut) | **+20** |
| Phase 0 invariants (`DuplicateFieldId`, `FieldIdFloor`) | **+25** |
| (R3) `OutputPinDefinition` id + `repair_output_pin_wires` remap | **+40** |
| **Net** | **≈ +220 source lines** |

The wire layer (`set_custom_node_type`) and the type system (`can_be_converted_to`)
are **untouched** — the cost is concentrated in plumbing a field identity that the
`Parameter.id` contract already expected to exist. The justification is
**correctness**: the entire rename/reorder-drops-wires class (the reported #377
plus its non-local closure-body and reorder siblings) is eliminated **by
construction**, because the field a wire was attached to is now tracked by identity
rather than by a name the user is free to change.

---

## 11. Relationship to existing docs

- `doc/design_identity_vs_naming.md` — the def-level sibling. **Its §9 record-field
  bullet has been amended** (and its §1 cross-reference updated): field renaming is
  no longer "rejected" wholesale — structural typing stays name-based there, and
  the wire-identity axis is addressed by this doc (§2).
- `doc/design_identity_vs_naming_phase0.md` — the shipped invariant checker; §4.1/§5
  extend it with `DuplicateFieldId` / `FieldIdFloor`, modeled on its
  `DuplicateParamId` / param-id-floor checks.
- `doc/design_parameter_wire_stability.md` — the `next_param_id` discipline this
  doc mirrors field-for-field (allocate-then-bump, floor-on-load, never recycle,
  restore-on-undo).
- `doc/design_record_types.md` — defines the record/field model and the
  structural-by-name typing this doc preserves.
