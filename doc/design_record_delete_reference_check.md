# Design: Block record-type-def deletion when still referenced

Status: proposed
Related: `doc/design_record_types.md`, `doc/design_record_types` phase notes, `AGENTS.md` (Record Type Defs; `walk_all_nodes`)

## 1. Problem

When a record definition `A` has a field whose type is another record `B`, and the user
deletes `B`, `A` is left in an erroneous state: the UI still shows `A` containing a field of
type `B`, but `B` no longer exists (a dangling `Record(Named("B"))`). This is inconsistent
with **node-network** deletion, which already refuses to delete a network that is referenced
by nodes elsewhere and shows the user a *"Cannot Delete Network"* dialog listing the
referencing networks.

Today, record-def deletion never blocks. `StructureDesigner::delete_record_type_def`
(`rust/src/structure_designer/structure_designer.rs:2296`) removes the def unconditionally and
then calls `NodeTypeRegistry::repair_all_networks`
(`node_type_registry.rs:1451`) to disconnect wires whose pin types now dangle. Crucially,
`repair_all_networks` only touches **wires in node networks** — it does **not** repair **other
record defs** that use the deleted type as a field. That is exactly why the `A`-contains-`B`
case is left dangling.

### Goal

Make record-def deletion **consistent with network deletion**: refuse to delete a record type
that is still referenced — by **another record def** *or* by **a node in any network** — and
surface a message that tells the user **exactly what** references it. Remove the silent
auto-repair path for this operation.

### Non-goals

- Changing network deletion (already correct — it is the model we mirror).
- Changing record **rename** (already updates references in place via `walk_data_type_record_names_mut`).
- Cascading deletes ("delete B and everything using it") — out of scope; we block instead.

## 2. Current behavior (for reference)

| Operation | Referenced-check | On conflict |
|---|---|---|
| Delete **network** | `check_delete_references` (`structure_designer.rs:1856`) walks all networks (incl. HOF bodies via `walk_all_nodes`), matches `node.node_type_name` | Returns `Err(String)`; Flutter shows *"Cannot Delete Network"* dialog |
| Delete **record def** | **none** | Deletes, then `repair_all_networks` disconnects dangling wires; **other record defs left dangling** |

Flutter already has the counterpart dialog for records: `node_networks_action_bar.dart`
`_handleDeleteActive` shows *"Cannot Delete Record Type Def"* with `result.errorMessage` — but
it never fires today because the backend never returns an error for a referenced def.

## 3. Design

### 3.1 What counts as a reference — the detection predicate

A record type named `B` is referenced by a `DataType` iff `B` appears anywhere in that type's
tree. The type tree is already walked, exhaustively and in one place, by:

```
walk_data_type_record_names(t: &DataType, f: &mut impl FnMut(&str))   // data_type.rs:243
```

It recurses through `Array`, `Iterator`, `Optional`, `Function` (params + return),
`AnyFunction { leading_params }`, and nested `Record::Anonymous` fields, and calls `f(name)`
for every `Record(Named(name))` leaf. Its recursion arms are required to stay byte-for-byte
aligned with `walk_data_type_record_names_mut` (the comment at `data_type.rs:236` enforces
this), so reusing it means our reference check cannot drift from the rename logic — the two
share a single definition of "mentions record `B`".

**Predicate:**

```
fn data_type_mentions(t: &DataType, target: &str) -> bool {
    let mut hit = false;
    walk_data_type_record_names(t, &mut |name| if name == target { hit = true });
    hit
}
```

This single predicate covers every nested shape: `B`, `Array[B]`, `Optional[B]`, `Iter[B]`,
`(B) -> Int`, `{ p: B }` (anonymous), and any composition thereof.

### 3.2 Where references live — the scan sites

There are exactly two families of locations that can hold a `DataType`, and both must be scanned.

**(a) Other record defs.** For every `RecordTypeDef` in
`NodeTypeRegistry::record_type_defs` **except the target itself**, scan each
`field.data_type` (`RecordField`, `node_type_registry.rs:127`). Built-in defs
(`built_in_record_type_defs`) are application-supplied and cannot reference user records, so
they need not be scanned (a self-check assertion is fine but not required).

**(b) Nodes in every network.** For every network in `node_networks`, and **recursing into
every HOF zone body** via `walk_all_nodes` (`node_network.rs:2188` — a bare
`nodes.values()` loop silently skips body nodes; see `AGENTS.md`), scan **the node's type
signature**: every `DataType` in the node's

- input **parameters** (`Parameter.data_type`),
- **output pins** (`PinOutputType::Fixed(dt)` — `SameAsInput` carries no literal),
- **zone-input pins** and **zone-output pins** (for HOF/closure nodes).

**Why scan the signature rather than per-node data fields?** Record names are introduced into a
node only as a `Record(Named(_))` that appears in that node's *computed* signature:
`record_construct`'s output pin is `Record(Named(schema))`; `record_destructure`'s input pin is
`Record(Named(schema))`; `product`'s output is `Array[Record(Named(target))]`; `expr`
parameters become input pins; `map`/`collect`/`array_at`/`parameter`/`closure`/`apply` all
surface their stored `DataType`/`type_args` in pins. Scanning the signature is therefore
**complete** *and* **future-proof**: a new node type that mentions a record in any pin is caught
with zero new code, whereas enumerating per-node data fields (the table in §5) would need a new
arm for every future record-bearing node and silently miss any that is forgotten — the exact
bug class this feature exists to kill.

The node's signature is its computed `NodeType`. At delete time the target def still exists (we
check *before* removing it), so every node's `NodeType` resolves normally. Bind to the existing
per-node `NodeType` accessor used elsewhere in the registry; do not hand-roll pin extraction.

### 3.3 New method: `check_record_delete_references`

Add to `StructureDesigner` (mirrors `check_delete_references`,
`structure_designer.rs:1856`):

```
fn check_record_delete_references(&self, target: &str) -> Result<(), RecordTypeDefError>
```

- Collect references as human-readable strings, e.g.:
  - record def: `"record type 'A' (field 'inner')"`
  - node: `"network 'Foo' (node 3: record_construct)"`, and when the node lives in a nested
    zone body, append `" [in a nested body]"`.
- If none, `Ok(())`.
- Otherwise `Err(RecordTypeDefError::Referenced { name, references })` where `references` is
  the collected list. Preserve discovery order but de-duplicate identical strings.

### 3.4 New error variant + message

Add to `RecordTypeDefError` (`node_type_registry.rs:273`):

```rust
#[error("cannot delete record type '{name}' because it is still referenced by:\n{}", .references.join("\n"))]
Referenced { name: String, references: Vec<String> },
```

Resulting user message (example — the exact `A`-contains-`B` scenario):

```
cannot delete record type 'B' because it is still referenced by:
record type 'A' (field 'inner')
network 'Foo' (node 3: record_construct)
```

### 3.5 Wire into `delete_record_type_def`

In `StructureDesigner::delete_record_type_def` (`structure_designer.rs:2296`), immediately after
the existing `NotFound` guard:

```rust
self.check_record_delete_references(name)?;
```

Because a clean pass now guarantees **no** network or def references the target, the subsequent
auto-repair is dead weight and should be **removed**:

- Drop the `repair_all_networks()` call (nothing dangles → no wires to disconnect).
- Drop `snapshot_all_networks_for_record_def_change(...)` and simplify
  `DeleteRecordTypeDefCommand` to no longer carry `affected_network_snapshots` — deletion no
  longer mutates any network, so undo only needs to re-insert the def and restore
  `active_record_def_name`. (This is a recommended simplification, not strictly required for
  correctness; if it complicates the undo change, the snapshots may be retained as an
  empty-effect no-op. See §6 for the test impact either way.)

The `Referenced` error propagates unchanged through the API layer to Flutter's existing
*"Cannot Delete Record Type Def"* dialog.

### 3.6 Flutter

No new UI is required — `node_networks_action_bar.dart` `_handleDeleteActive` already renders
`result.errorMessage` in the dialog. The message is multi-line; confirm the dialog body renders
newlines (wrap in a scrollable `Text` if long). **FFI note:** if the API surfaces the delete
result as a plain `{ success, errorMessage: String }`, adding a `RecordTypeDefError` variant is
message-only and needs **no** codegen. If `RecordTypeDefError` crosses FFI as a *typed* enum,
run `flutter_rust_bridge_codegen generate` after adding the variant. Verify which during
implementation.

## 4. Edge cases

- **Two-level (the reported bug):** `A { inner: B }`, delete `B` → blocked, message names `A`.
- **Nested in a container:** `A { xs: Array[B] }`, `Optional[B]`, `Iter[B]` → blocked (walker recurses).
- **Nested in an anonymous record:** `A { p: { q: B } }` → blocked (walker recurses anonymous fields).
- **Nested in a function type:** `A { f: (B) -> Int }` → blocked.
- **Reference only inside a HOF body:** a `record_construct` for `B` inside a `map` body → blocked (proves `walk_all_nodes` recursion).
- **Multiple references:** all listed in the message.
- **Deleting the container is fine:** deleting `A` when nothing references `A` succeeds even though `A` itself references `B` (referencing others never blocks *your own* deletion — we skip the target when scanning defs).
- **Unreferenced def:** deletes normally; undo restores it.
- **Built-in def:** unchanged — still rejected earlier (`is_built_in_record_type_def`).
- **Self-reference:** impossible (cycle check on add/update), and the target def is skipped during the def scan regardless.

## 5. Node-data reference sites (test-matrix cross-check)

The §3.2 signature scan is the *mechanism*; this table is the **checklist the tests must
exercise** to prove each real record-bearing node type is actually covered by that scan. Each
row = one blocked-deletion test.

| Node | Data field holding the record ref | Surfaces in signature as |
|---|---|---|
| `record_construct` | `schema: String` | output pin `Record(Named(schema))` |
| `record_destructure` | `schema: String` | input pin `Record(Named(schema))` |
| `product` | `target: String` | output pin `Array[Record(Named(target))]` |
| `expr` | `parameters[].data_type: DataType` | input pins |
| `array_at` | `element_type: DataType` | input/output pins |
| `map` | `input_type`/`output_type: DataType` | regular + zone pins |
| `collect` | `element_type: DataType` | input/output pins |
| `parameter` | `data_type: DataType` | output pin |
| `closure` / `apply` | `type_args: Vec<DataType>` | function-typed pins |

## 6. Testing (automated)

New test file `rust/tests/structure_designer/record_delete_reference_check_test.rs`, registered
in `rust/tests/structure_designer.rs` with a `#[path = ...] mod ...;` entry (per `AGENTS.md`,
tests live in `rust/tests/`, never inline).

Each test builds a `StructureDesigner`, adds the def(s)/network(s), calls
`delete_record_type_def("B")`, and asserts on the result + resulting state.

**Blocked cases** — assert `Err(RecordTypeDefError::Referenced { .. })`, that `B` is **still
present** in `record_type_defs`, and that the message string contains the expected reference
substring:

1. `B` is a field type of def `A` (the reported bug) — message contains `record type 'A'` and the field name.
2. `Array[B]` field in `A`.
3. `Optional[B]` field in `A`.
4. `Iter[B]` field in `A`.
5. `{ p: B }` anonymous-record field in `A`.
6. `(B) -> Int` function-typed field in `A`.
7. `record_construct` node with schema `B` in a network — message contains the network name.
8. `record_destructure` node with schema `B`.
9. `product` node with target `B`.
10. `expr` node with a parameter of type `B`.
11. `record_construct` for `B` **inside a `map` zone body** (proves body recursion).
12. Multiple references at once (def `A` + a network node) — message lists **both**.

**Allowed cases** — assert `Ok(())` and that `B` is **gone** from `record_type_defs`:

13. `B` with no references → deletes.
14. Delete `A` where `A { inner: B }` and nothing references `A` → deletes (`A` referencing `B` must not block `A`'s own deletion).
15. Delete an unreferenced `B`, then **undo** → `B` restored (guards the §3.5 undo-command simplification).

**Unchanged behavior:**

16. Deleting a built-in def still returns the existing error (not `Referenced`).

**Regression audit (important):** the new block changes the outcome of any *existing* test that
deletes a record def while a reference still exists. Grep the current
`record_types_phase*_test.rs` and undo tests for `delete_record_type_def` and update any that
relied on the old delete-and-repair behavior (they must now either remove the reference first or
assert the new `Referenced` error). Also update `DeleteRecordTypeDefCommand` undo tests if
§3.5's snapshot removal is taken.

Run: `cd rust && cargo test --test structure_designer record_delete_reference`.

## 7. Implementation checklist

1. `RecordTypeDefError::Referenced { name, references }` variant + `#[error]` message (`node_type_registry.rs`).
2. `StructureDesigner::check_record_delete_references(&self, target) -> Result<(), RecordTypeDefError>` (`structure_designer.rs`), using `walk_data_type_record_names` + `walk_all_nodes`, scanning def fields and node signatures per §3.2.
3. Call it at the top of `delete_record_type_def`; remove `repair_all_networks` + network snapshots; simplify `DeleteRecordTypeDefCommand` (§3.5).
4. New test file + registration; implement the §6 matrix; run the regression audit.
5. Verify Flutter dialog renders the multi-line message; run `flutter_rust_bridge_codegen generate` **iff** the error crosses FFI as a typed enum (§3.6).
6. `cargo fmt && cargo clippy && cargo test`; `flutter analyze`.
