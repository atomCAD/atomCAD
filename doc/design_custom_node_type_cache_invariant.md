# Design: `custom_node_type` cache invariant (rename wire-loss fix)

Status: planned. Owner: TBD. Branch: `wire-drop-bug`.

Investigation/evidence: `reports/rename_wire_loss_investigation_2026-06-17.md`.
Regression tests (currently RED): `rust/tests/structure_designer/rename_wire_loss_regression_test.rs`
(fixtures `tests/fixtures/rename_wire_loss/{before,minimal}.cnnd`).

## Problem

A record-def rename silently deletes incoming wires across the *whole* project —
including networks unrelated to the rename. 127 of 357 wires were lost in the
reported case. Loss is pure deletion (no rerouting) and is confined to
**derived-layout** node types: `parameter`, `expr`, `map`, `filter`, `fold`,
`foreach`, `collect`, `product`, `record_construct`, `record_destructure`
(and `sequence`, `array_*`). Fixed-arity nodes and custom-network instances are
untouched.

## Background: the cache

Each `Node` carries `custom_node_type: Option<NodeType>` (`node_network.rs:506`),
a `#[serde(skip)]` cache of the node's *derived* signature. It is computed by
`NodeData::calculate_custom_node_type` (returns `Some` for derived-layout nodes,
`None` for static nodes) and read preferentially by
`NodeTypeRegistry::get_node_type_for_node` (`node_type_registry.rs:627`) — when
present it **shadows** the static registry type.

Wires are positional: a wire's destination is its index in `node.arguments:
Vec<Argument>`; only the *source* has a stable id. So any rebuild of `arguments`
that doesn't preserve index alignment loses/moves wires.

## Root cause

`Option<NodeType>` overloads `None` to mean two different things:

- **(A) Static node — no derived type.** Permanent, correct. `get_node_type_for_node`
  falls back to the registry type.
- **(B) Stale / not-yet-computed.** Transient and *invalid* for a derived node:
  readers mis-type it, and the repair path destroys its wires.

The bug is a sequence that makes state (B) observable to a `refresh_args=true`
consumer:

1. Record rename → `rewrite_record_name_in_registry` (`node_type_registry.rs:2576`)
   rewrites embedded type-name references in node data, then **unconditionally**
   sets `node.custom_node_type = None` for *every* node in *every* network
   ("clear it defensively", `:2685`).
2. `repair_all_networks` → `repair_node_network` re-derives each cache with
   `refresh_args = true` (`:2032`, all node types except `apply`).
3. `set_custom_node_type(Some(new), refresh_args=true)` (`node_network.rs:717`)
   computes `can_preserve` from the *old* cache. With the old cache now `None`,
   `can_preserve = false`, so it takes the rebuild branch; the wire-copy block is
   guarded by `if let Some(old) = self.custom_node_type` (also `None`), so nothing
   is copied and `self.arguments` is replaced with empties. **Wires gone.**

Static nodes survive because `calculate_custom_node_type` returns `None`, so
`set_custom_node_type(None, _)` never touches `arguments`. Custom-network
instances survive because they miss the `built_in_types` lookup in the populator.

There is a **second instance of the same anti-pattern**: `canonicalize.rs:66`
(`canonicalize_network`) also does an unconditional defensive
`node.custom_node_type = None`. It is safe *today* only because canonicalize is
currently followed by the load path's `refresh_args=false` repopulate; it becomes
the same catastrophe the day anything runs `canonicalize → repair_all_networks`.

### Why the defensive clear exists

`rewrite_record_name_in_registry` is mid-iteration over
`registry.node_networks.values_mut()` and cannot also borrow
`registry.built_in_node_types` / `record_type_defs` to recompute in place —
borrow conflict. The cheap workaround was "null it, let the later pass rebuild."

### Verified: in-place recompute is safe

For every derived-layout node, `calculate_custom_node_type(&self, base)` depends
**only** on the node's own `data` + the static base type (enforced by the
signature — it cannot read other networks). The 3 record nodes additionally read
`record_type_defs`, which the caller renames *before* the rewrite walk. `apply` /
`map.f` layouts depend on a wired source but are derived by network-level
post-passes, not the per-node clear. Therefore recomputing a node's cache in
place, immediately after rewriting its own data, is order-independent and
coherent. The split-borrow tool already exists:
`populate_custom_node_type_cache_with_types` is a static method taking the type
maps as separate params (`node_type_registry.rs:699`).

### Key insight: these ops are structure-preserving

A record *rename* (and `canonicalize`) change type-name strings inside pins; they
never change a node's pin **count or order**. So `arguments` are always still
positionally valid, and the correct rebuild contract is `refresh_args=false`
("types follow, wires stay"), not the destructive reshape (`true`).

## The invariant

> For any node where `calculate_custom_node_type(..)` returns `Some`,
> `custom_node_type` is `Some` and coherent with the node's `data` + registry
> at every point a consumer can observe it.

State (B) for a derived node must never be observable.

## How the changes relate (two orthogonal axes)

The fix has two independent axes. For these *structure-preserving* ops, either
axis alone stops the wire loss — but they guarantee different things, so we apply
both:

- **Axis A — *when* the cache is made valid:** clear-then-repair-later (current)
  vs recompute in place (Change 2).
- **Axis B — *what* the recompute does to `arguments`:** reshape
  (`refresh_args=true`) vs preserve positionally (`refresh_args=false`).

|                              | B=true (reshape)                                                   | B=false (preserve)                                                  |
| ---------------------------- | ------------------------------------------------------------------ | ------------------------------------------------------------------- |
| **A: clear + later repair**  | wires LOST, invariant violated — *current bug*                     | wires kept, but cache still observably `None` in the window         |
| **A: recompute in place**    | wires kept (old cache still `Some` ⇒ `can_preserve`), invariant ok | wires kept, invariant ok — **Change 2** (bottom-right)              |

Change 2 picks the bottom-right cell: recompute in place (fixes the *invariant* —
the cache is never observably `None`) with `refresh_args=false` (the
intent-correct, robust reshape contract for a structure-preserving op — it does
not *rely* on `can_preserve` happening to return true). Change 1 hardens the
top-left cell so that even a future stray `None`-cache + `refresh_args=true`
repair degrades to "wires kept" instead of "wires lost".

## Short-term plan

Three independent changes (apply in any order). Together: state (B) is
*eliminated* at the source (Change 2), *tolerated harmlessly* if it ever recurs
(Change 1), and *caught loudly* in tests if it does (Change 3).

### Change 1 — Tolerance net: `set_custom_node_type` never drops wires

File: `node_network.rs`, `set_custom_node_type` (`:717`).

When `refresh_args == true` and the rebuild branch is taken but
`self.custom_node_type` is `None`, **preserve `arguments` positionally** instead
of emptying them. Concretely: if there is no old node type to copy from, keep the
existing `self.arguments` (resizing to `new_node_type.parameters.len()` by
pushing empty `Argument`s when too few, truncating when too many) rather than
discarding them. On a consistent graph the arguments are already correctly
ordered, so positional preservation is correct; this mirrors the existing `apply`
positional-preservation carve-out, generalized.

Outcome: a `None` cache at repair time can no longer drop wires.

### Change 2 — Eliminate the bad state: recompute in place instead of clearing

This is the invariant guarantee. (An alternative considered was making the
defensive clear *conditional* — only clear nodes that reference the renamed name.
Recompute-in-place is strictly stronger: it never leaves the cache in state (B)
at all, so we do that instead.)

Replace each unconditional `custom_node_type = None` defensive clear with an
in-place recompute using the static split-borrow populator and
`refresh_args = false` (structure-preserving):

- File: `node_type_registry.rs`, `rewrite_record_name_in_registry` (`:2576`).
  Destructure the registry so `built_in_node_types`, `record_type_defs`,
  `built_in_record_type_defs` are borrowed alongside `&mut node_networks`, then
  after rewriting a node's own data call
  `Self::populate_custom_node_type_cache_with_types(built_in_node_types,
  record_type_defs, built_in_record_type_defs, node, /*refresh_args=*/ false)`
  in place of `node.custom_node_type = None;` (`:2685`).
- File: `canonicalize.rs`, `canonicalize_network` (`:66`). Same treatment. Note
  this function currently only receives `&mut NodeNetwork`; it will need the type
  maps threaded in (or be called from a context that has them). If threading the
  maps is awkward here, the acceptable fallback is to keep the clear **but** rely
  on Change 1 for safety and ensure no `canonicalize → repair(refresh_args=true)`
  path exists; document that constraint at the call site.

After Change 2, the `repair_all_networks` that follows a record rename becomes a
no-op for these nodes (`can_preserve` is true: old cache present and matching).

### Change 3 — Enforce the invariant: assertion

Add a `debug_assert!` that fires when a derived node is observed with a `None`
cache (i.e. `node.custom_node_type.is_none()` but
`node.data.calculate_custom_node_type(base).is_some()`). Cheapest high-value
step; converts this whole class from silent data loss into a loud test failure.
Keep it `debug_assert!` so release builds are unaffected.

**Placement matters — do NOT put it in `get_node_type_for_node`
(`node_type_registry.rs:625`) naively.** That path is called during the
legitimate transient window (post-deserialize, pre-`initialize_custom_node_types_for_network`),
where derived nodes correctly still hold `None`; a bare assert there would panic
spuriously in load paths/tests. Preferred site: a dedicated invariant check at
the **end of `validate_network`** (and/or at the end of
`initialize_custom_node_types_for_network` / `repair_node_network`), i.e. only at
points where initialization is guaranteed complete. Walk all nodes (use
`walk_all_nodes`, which recurses into zone bodies) and assert the invariant per
node. The deferred 3-state enum removes this fragility entirely (the transient
becomes the named `Uncomputed` variant, distinct from `Static`).

### Tests

All run under `cargo test` (debug, so `debug_assert!` is active). New tests go in
`rust/tests/structure_designer/rename_wire_loss_regression_test.rs` unless noted;
register any new module in `tests/structure_designer.rs`.

**(T1) Existing RED → GREEN (Change 2, forward record rename).** The two RED tests
— `record_def_rename_does_not_drop_unrelated_wires` (verbatim 462 KB file; covers
`parameter`/`expr`/`collect`/`record_destructure` breadth) and
`minimal_record_rename_drops_parameter_default_wire` (minimal, *unrelated* victim)
— must turn GREEN. Keep the two `*_present_after_load` sanity tests.

**(T2) Node that *references* the renamed def (Change 2 — rewrite + record-node
populator branch).** *Gap:* T1's minimal case only covers an unrelated victim; the
"node actually using the renamed record" path (and the special
`record_construct`/`record_destructure` populator branch) is only covered by the
opaque big file. Add a small fixture (e.g. `minimal_record_ref.cnnd`) with a
`record_construct` whose `schema = "R"` and a wire into one of its field pins.
Rename `R → R2` and assert: (a) the field wire survives, **and** (b) the node's
`schema` (and any embedded `Record(Named(..))`) is now `R2`. Generate it the same
way `minimal.cnnd` was generated (build via `StructureDesigner` + `add_record_type_def`
+ `add_node` + `connect_nodes`, then `save_node_networks_as`).

**(T3) Undo/redo (Change 2 covers this path via the command).** `RenameRecordTypeDefCommand::{undo,redo}`
both route through `rename_record_type_def_unchecked` + `repair_all_networks`, so
they re-run the fixed path. Test: load a designer with a wired derived node →
`rename_record_type_def("R","R2")` → assert wires intact → `undo()` → assert wires
intact (and def name is `R` again) → `redo()` → assert wires intact.

**(T4) `canonicalize` site (Change 2, second site).** *Gap:* there is no natural
`canonicalize → repair` path (canonicalize is only called on load, followed by the
`refresh_args=false` repopulate), so the test must compose the dangerous sequence
by hand: build/load the minimal network, call
`canonicalize::canonicalize_network(&mut net)` directly, then
`registry.repair_node_network(&mut net)` (or `repair_all_networks`), and assert the
`parameter` default wire survives. (If the Change 2 fallback is taken for this site
instead of threading the maps, this test still must pass — via Change 1.)

**(T5) Change 1 direct unit tests (the grow/shrink branches are NOT reachable via
T1–T4).** *Gap:* rename/canonicalize are structure-preserving (pin count never
changes), so they only ever exercise the "equal count" case; the resize logic has
no other coverage. Test `Node::set_custom_node_type` directly (Node fields are
`pub`; construct a `Node` with a simple `NodeData`, `arguments` carrying marker
wires, and `custom_node_type = None`, then call with `refresh_args = true` and a
new `NodeType` of the chosen parameter count):
  - equal count → all wires preserved at their indices;
  - grow (new count > current args) → existing wires preserved in place, new slots empty;
  - shrink (new count < current args) → prefix preserved, tail dropped;
  - **non-regression:** `custom_node_type = Some(old)` with params reordered/renamed →
    existing copy-by-id/name behaviour unchanged (a wire moves to its matched slot).
    Guards against Change 1 altering the with-old-cache path.

**(T6) Non-regression — structure-*changing* ops must still reshape/disconnect.**
*Gap:* Change 1 generalizes the `apply` positional-preserve carve-out; if it
over-applies it could suppress legitimate wire disconnection. Ensure existing
`update_record_type_def` / `delete_record_type_def` / custom-network-interface-change
tests still pass. If none asserts that retyping a record field to an *incompatible*
type disconnects the now-invalid wire, add one. This is the guard that
"preserve positionally" did not become "never disconnect anything."

**(T7) Change 3 assertion.** The `*_present_after_load` and full suite run in debug,
so they confirm the assertion does **not** false-fire during the legitimate
post-deserialize/pre-init transient (validating placement). Optionally add a
white-box `#[should_panic]` test that forces a derived node to `custom_node_type =
None`, calls the invariant-check entry point, and expects a panic — proving the
assert actually guards.

Full suite: `cd rust && cargo test`.

## Deferred: replace `Option<NodeType>` with an explicit 3-state enum

The `Option` is the structural reason (A) and (B) are confusable. Replace it with:

```rust
enum NodeTypeCache {
    /// Node's signature is the static registry type; no per-node derivation.
    Static,
    /// Derived signature, coherent with `data` + registry.
    Derived(NodeType),
    /// Not yet computed. Valid only transiently (post-deserialize, pre-init);
    /// never observable by a type-resolution consumer.
    Uncomputed,
}
```

This makes the dangerous transient a *named* variant that every reader must
handle, so the compiler forces the conversation a bare `None` let everyone skip.
`get_node_type_for_node` matches `Static`/`Derived` and `debug_assert!`s it never
sees `Uncomputed`. The defensive clears become `Uncomputed` (loudly distinct from
`Static`), and the construction/deserialize defaults start `Uncomputed`.

Blast radius (from the audit): ~9 write sites and ~6 read sites of
`custom_node_type` (`set_custom_node_type`, the populators at
`node_type_registry.rs:727/743/760/771/1555/1820`, the clone-through sites in
`node_inlining.rs:166` / `selection_factoring.rs:447` / `node_network.rs:1087/2122`,
and the readers `get_node_type_for_node:627`, `set_custom_node_type:720/746`,
`node_type_registry.rs:1673`). Mechanical but wide; do it as its own change once
the short-term fixes are green.

## Non-goals (separately tracked)

- **Positional → id-keyed wire destinations** (store `dest_param_id` on wires so
  any `arguments` rebuild re-binds by id). The deeper fix for this whole bug
  family (also the `next_param_id` recycling bug,
  `project_parameter_wire_stability`). Large; out of scope here.
- **Splitting `repair` into structure-preserving vs structure-changing** so
  `refresh_args` isn't a hardcoded `true` at `repair_node_network:2032`. The
  `refresh_args=false` in Change 2 sidesteps this locally; the general refactor
  is deferred.
- **Rename undo → snapshot-based** (currently replay-based, so it cannot restore
  wires the repair pass dropped). Tracked separately.
