# Design: Identity vs. Naming vs. Position (the reference architecture)

**Status:** Proposal / architecture north-star. Not yet implemented.
**Scope:** Cross-cutting. Touches `node_network`, `node_type`, `data_type`,
`node_type_registry`, `network_validator`, `serialization`, the text format, and
the FFI/Flutter boundary.
**Supersedes-by-subsuming:** the "Non-goals" of
`doc/design_custom_node_type_cache_invariant.md` ("positional → id-keyed wire
destinations") and the long-term half of `doc/design_parameter_wire_stability.md`
(F1–F6 are the tactical fix; this doc is the strategic end-state).

---

## 0. Why this document exists

We have hit the same class of bug repeatedly:

- **Record rename drops wires across unrelated networks** (the
  `custom_node_type` cache-invariant bug, then the `closure`/`apply`/`collect`
  rewrite-walk omission — two separate failures of *the same* hand-maintained
  walk list).
- **Adding/reordering a parameter jumbles wires in calling networks**
  (`design_parameter_wire_stability.md`: a recycled `param_id` after load).
- **Repair passes that "guess" how to realign wires** after a layout change,
  and silently drop or mis-route them.

Each of these is a symptom of one root architectural confusion. This document
names that confusion, states the principle that removes it, and lays out an
incremental, safe migration.

---

## 1. The confusion: three concepts stored as one

The system conflates three distinct concepts:

| Concept      | What it should be                              | Mutates when…           |
| ------------ | ---------------------------------------------- | ----------------------- |
| **Identity** | a stable, opaque id, allocated once, never reused | never                |
| **Name**     | a mutable, human-facing label                  | the user renames        |
| **Position** | a derived ordering for layout & evaluation     | the user reorders       |

Today, **names and positions are used *as* identity**:

- A wire's destination is the **index** of its `Argument` in
  `node.arguments: Vec<Argument>` — *position is identity*.
- A `DataType` references a record def via `RecordType::Named(String)`, a node
  references its type via `node_type_name: String`, a record field is referenced
  by field name — *name is identity*.

Because name and position are not stable, every rename and every reorder forces
the system to *find and rewrite every place that used the old name/position*.
That rewrite is:

- **brittle** — it is a hand-maintained list of node-data downcasts
  (`rewrite_record_name_in_registry`) that has drifted out of sync with reality
  twice; and
- **lossy** — the positional realignment (`repair_call_sites_for_network`,
  `repair_network_arguments`, the `set_custom_node_type(refresh_args=true)`
  rebuild) guesses, and a wrong guess silently deletes or mis-routes a wire.

---

## 2. The principle

> **References store identity. Names and positions are derived/presentation
> views — never the binding.**

Apply it and three expensive, dangerous operations become trivial:

- **Rename** → mutate one `name` field on the definition. **No reference
  rewrite, no cross-network walk.** Deletes `rewrite_record_name_in_registry`,
  `collect_record_refs_in_network`, the `rename_node_network` rewrite walk, and
  the obligation to keep all three in sync with
  `canonicalize::canonicalize_node_data`.
- **Reorder** parameters / fields / output pins → a **no-op for every wire**,
  because wires point at ids, not slots. Only the derived positional view is
  recomputed. Deletes `repair_call_sites_for_network`'s index translation.
- **Wire repair** → shrinks to a single clean rule: *drop a wire iff its
  endpoint id no longer exists, or the endpoint types are no longer
  compatible.* No index realignment, no guessing.

---

## 3. The two axes (separate blast radius)

The principle is one, but the work splits along two axes with very different
cost, so we track them separately.

### Axis A — name-based cross-references (the *rename* pain)

Records, record fields, and node types are referenced by **name strings embedded
throughout the graph** (inside `DataType`s on `parameter`/`expr`/`map`/`closure`/
`collect`/… node data, inside `record_construct.schema`, inside every network's
parameter/output-pin signature). Rename ⇒ rewrite every embedded copy.

This is the walk in `rewrite_record_name_in_registry`. Its fragility is
structural: correctness depends on the downcast list being **exhaustive**, and
nothing enforces that. It has silently missed node types twice.

### Axis B — position-based bindings (the *reorder* pain)

A wire's destination is its index in `node.arguments`. Insert / remove / reorder
a parameter and the indices shift.

**Important nuance (from `design_parameter_wire_stability.md`):** positional
storage *works correctly* as long as the parameter's `param_id` is unique and
persistent — the validator reconciles old→new positions by id. The actual
corruption there was an **identity-allocation** failure (`next_param_id` reset to
`1` on load → a recycled id → the reconciler matched the wrong parameter). So
Axis B is really: *we already have ids on the definition side, but (a) the wire
itself doesn't store the destination id, and (b) id allocation isn't airtight.*

---

## 4. The reference classes and the id each needs

| # | Reference site            | Today                                | Proposed                              | Eval-hot? |
| - | ------------------------- | ------------------------------------ | ------------------------------------- | --------- |
| 1 | wire → destination param  | index into `arguments: Vec<Argument>`| `dest_param_id: ParamId` on the wire  | **yes**   |
| 2 | wire → source pin         | `SourcePin::NodeOutput{pin_index}`   | `OutputPinId`                         | yes       |
| 3 | node → its type           | `node_type_name: String`             | `TypeId` (name moves to the def)      | no        |
| 4 | `DataType` → record def   | `RecordType::Named(String)`          | `RecordDefId` (interned, see §6)      | no        |
| 5 | record value / pin → field| field **name**                       | `FieldId`                             | partially |

Source **nodes** are already id-keyed (`IncomingWire::source_node_id: u64`) — and
that is precisely why the source side never suffered the rename bug. We are
extending the discipline that already works for source nodes to the other five
sites.

---

## 5. Keeping evaluation fast: id-keyed truth, index-keyed eval view

The constraint "evaluation must stay fast" is real: the evaluator reads
`arguments[i]` in a hot loop and must not pay a hash lookup per argument. The
resolution reuses a pattern the code already relies on for `custom_node_type`:

- **Source of truth is id-keyed.** The wire carries `dest_param_id`; a record
  value is field-id-keyed.
- **The eval view stays a plain `Vec`.** `arguments: Vec<Argument>` becomes a
  **derived cache**, materialized in the node type's current parameter order. It
  is recomputed only when the layout changes — which is *exactly* when
  `calculate_custom_node_type` already re-runs. The hot loop remains a `Vec`
  index; **zero per-eval hashing.**

So "reorder" means "recompute the derived index view." Wires never move because
they were never keyed on the index in the first place. Same idea for records:
store the payload field-id-keyed, materialize a fixed-order tuple per def when an
eval actually needs positional access.

---

## 6. The external boundary stays name-based (by design)

Internally by id, externally by full name. This is not a compromise — it is the
correct separation, and it is where names *should* remain authoritative:

- The **`.cnnd` human/AI text format**, the **FFI / Flutter UI**, and
  **library import** stay name-based. We add a thin **name↔id resolution layer at
  the boundary**: a per-document **name table**. Parse/import resolves names →
  ids; serialize/display resolves ids → names.
- **Import disambiguation by full name falls out for free.** Ids are unique only
  *within a document*. Importing a library **remaps** its ids to fresh local ids,
  and uses **full names** to decide "same logical type → merge" vs. "name clash →
  rename-on-conflict." This is the one place names *must* stay authoritative, and
  it is external — exactly where we want them.
- **Built-ins** get fixed, well-known ids; they are never renamed.

Net effect: human naming is **decoupled** from internal identity. Renaming a
record, field, parameter, network, or pin is a pure UI/label operation with no
graph-wide consequences.

---

## 7. The one new global invariant: id allocation

Ids are **document-scoped surrogate keys**. The whole architecture rests on one
rule:

> **Never recycle an id within a document. Remap on every cross-document move
> (import / paste-from-library / duplicate-network).**

The parameter-wire bug *was* a recycling failure, so allocation must be
structured so recycling is impossible:

- **Prefer document-level monotonic allocators** (one per id-class) over
  per-network counters. The current per-network `next_param_id` and per-body
  `next_node_id` are the exact shape that produced both the recycling bug *and*
  the "body id numerically collides with a top-level id" pitfall documented in
  `rust/AGENTS.md`. A single per-document allocator per class is the simplest
  thing that cannot recycle. (A composite `(scope, local_counter)` key also
  works but is more moving parts.)
- **Always recompute the floor on load** as a backstop
  (`next_id = max(existing) + 1`). `design_parameter_wire_stability.md` F1
  already does this for `param_id`; generalize the discipline to every id-class.
- **Remap-on-import** becomes the single new sharp edge. It gets its own
  invariant-test module (see Phase 0).

---

## 8. What this deletes

When the migration is complete, these disappear or collapse to a one-liner:

- `rewrite_record_name_in_registry` — gone. Record rename = set
  `record_type_defs[id].name`.
- `collect_record_refs_in_network` — gone (or reduced to "ids referencing this
  def," a trivial id scan with no node-type knowledge).
- the `rename_node_network` reference-rewriting walk — gone.
- the obligation to keep the three node-data downcast lists
  (`canonicalize_node_data`, the rewriter, the collector) in sync — gone.
- `repair_call_sites_for_network`'s positional old→new translation — gone (wires
  carry `dest_param_id`).
- `set_custom_node_type(refresh_args=true)`'s lossy `arguments` rebuild — reduces
  to "recompute the derived index view; keep id-keyed bindings."
- `repair_network_arguments` count/index pad-truncate — reduces to "drop wires
  whose `dest_param_id` no longer exists."

---

## 9. Incremental, safe rollout

Every phase is independently shippable. Each follows the same safe shape:

> **add id alongside name (dual representation) → migrate existing files
> deterministically → flip readers to id → delete the name-walk.**

You can stop after any phase and the system is strictly better, never
half-broken.

### Phase 0 — Invariant checkers + property tests *(do first; cheap; de-risks all the rest)*

> **Implementation spec:** `doc/design_identity_vs_naming_phase0.md` — a
> self-contained, hand-to-an-assistant brief for this phase (violation catalogue,
> checker signatures, wiring point, test plan, staged rollout). The summary below
> is the rationale; that doc is the build instructions.

This phase ships *no* representation change. It makes every later phase safe and
catches today's bugs loudly.

- A debug invariant run after every structural mutation (end of
  `validate_network`, alongside the existing cache-invariant `debug_assert`):
  **no wire dangles; every type / record / field reference resolves.** Converts
  the entire silent-corruption class into a loud failure *today*.
- The property suite from `design_parameter_wire_stability.md` F5, generalized:
  for arbitrary sequences of `{add, remove, reorder, rename, retype}` over
  params / fields / record defs / node types, **crossed with**
  `{fresh, after-load, after-duplicate, after-import}`, assert every surviving
  wire preserves its `(source-identity, destination-identity)` pair and no wire
  changes which parameter/pin it feeds. This is the **acceptance bar** for every
  later phase.

### Phase 1 — Parameter-id-keyed wire destinations *(Axis B)*

Highest value, self-contained, already half-designed. Land
`design_parameter_wire_stability.md` F1–F6 (several already done), then:

- store `dest_param_id` on the wire/argument;
- make `Parameter.id` non-`Option` (permanent, allocated once);
- make the positional `arguments` a derived view of the id-keyed binding (§5).

Kills the parameter-order repair class.

### Phase 2 — Record-def ids *(Axis A — deletes the walk we keep re-fighting)*

The most pervasive change, because `DataType` is everywhere. Do it by
**interning, not by reshaping the enum**:

- Keep `RecordType::Named(String)` as the *serialized / text-format* form.
- At load, **intern each name to a `RecordDefId`** in the document name table;
  in-memory references carry the id (e.g. a resolved-id cache on the `DataType`,
  or a parallel `RecordType::Ref(RecordDefId)` used in memory only).
- Rename = set one `name` field; the walk disappears.

Interning is far smaller blast radius than rewriting `DataType`'s variants across
serialization, text format, FFI, and every walk.

### Phase 3 — Record field ids

Same interning approach, scoped to **named** defs. Anonymous records stay
structural (no ids — nothing to rename). Field rename and field reorder become
no-ops for `record_construct` / `record_destructure` / `product` wiring.

### Phase 4 — Node-type ids + import remap

Node stores `TypeId`; registry keyed by id; name becomes a def field; import
remaps ids and dedups/renames by full name (§6). Built-ins get fixed ids.

### Phase 5 — Output-pin ids

Smallest remaining piece. Matters mainly for `record_destructure` multi-output
reorder and any future multi-output node whose pins can be reordered.

---

## 10. Honest cost / risk

- **Biggest cost is Phases 2–3.** `DataType`, the text format, and the FFI all
  touch record/field names. The interning mitigation (string form on the wire
  format, ids in memory) is what makes it tractable. The name table must be
  threaded where `&NodeTypeRegistry` already is — which is most resolution sites,
  so the threading largely exists already.
- **Migration determinism.** Assigning ids to existing files must be
  deterministic and stable, or every re-save churns the file. Rule: assign ids
  from current order/name on first load; persist thereafter; never re-derive.
- **Remap-on-import** is the new global invariant and the most likely home for a
  future bug. It earns a dedicated test module.
- **Anonymous records** need no ids and get no special-casing beyond "ids apply
  to named defs only."

---

## 11. If the full migration is too expensive: the 80/20

If only two things ship, ship these — cheap, and they kill most of the pain:

1. **Phase 0 invariant checker.** Turns the entire silent-corruption class into
   loud test/assert failures immediately, independent of any representation
   change.
2. **Phase 2 interning.** Record (and later type) rename becomes O(1) and the
   fragile, twice-broken hand-maintained walk lists are deleted — *without*
   reshaping `DataType`.

Axis B (parameter order) is already substantially contained once
`design_parameter_wire_stability.md` F1–F6 land, so the full `dest_param_id`
change is the cleanest *eventual* state but the lowest *marginal* pain today.

---

## 12. Relationship to existing docs

- `doc/design_parameter_wire_stability.md` — the **tactical** fix for Axis B's
  acute bug (recycled `param_id`). Its F5 property suite is this doc's Phase 0
  acceptance bar; its F1/F6 floor-recompute is this doc's §7 generalized.
- `doc/design_custom_node_type_cache_invariant.md` — the tactical fix for one
  Axis A failure mode (stale `custom_node_type` cache during a rename). Its
  "Non-goals" explicitly name "positional → id-keyed wire destinations" as the
  deeper fix; this doc is that deeper fix, generalized to all five reference
  classes.
- `doc/design_record_types.md` — defines the record/field model this doc
  proposes to id-key.
