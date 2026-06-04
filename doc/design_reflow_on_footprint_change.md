# Design: Reflow Neighbours on Node Footprint Growth

## Summary

When an edit makes a node's **rendered footprint grow in place** — without the
user dragging anything — the surrounding nodes should be pushed out of the way so
the grown node does not overlap them. Today only two operations do this:
`inline_custom_node` and `convert_instance_to_closure`, both via
`node_inlining::make_space_for_inline`. Every other in-place growth leaves
neighbours overlapping.

This document designs a single reusable **reflow** primitive and the undo support
for it, then phases it into the three operations that need it:

- **(A)** A higher-order-function (HOF) node (`map` / `filter` / `fold` /
  `foreach`) in `CollapseMode::Auto` **expands** when its `f` pin is
  **disconnected** (the wire deleted, or its source node deleted): it flips from a
  compact regular-node footprint to a full body region.
- **(B)** An HOF **expands** when its collapse mode is set to `Expanded` (from
  compact) via `set_collapse_mode`.
- **(C)** Adding / pasting / duplicating a node **inside a zone body** grows the
  body past its stored size, which grows the enclosing HOF's rendered footprint
  **in its parent network** — and that growth can cascade up several scope levels.

The reflow primitive is the spatial half of the design; the larger half is making
those neighbour moves **undoable in the same single step as the triggering edit**,
across multiple scopes, **without** resorting to whole-network snapshots.

## Non-goals / cases that need nothing

- **Shrinking** operations need no reflow: connecting `f` (HOF goes compact),
  collapsing, and `extract_closure_to_network` (large closure → small instance)
  all leave a harmless gap. Pulling neighbours *inward* would be surprising and is
  explicitly out of scope.
- **Zone resize drag** (`set_zone_size`) is user-driven — the user is already
  dragging the handle and controls placement. Out of scope (could be revisited).
- **Property edits that add input pins** (e.g. an `expr` gaining a parameter, a
  `record_construct` schema gaining a field) grow a node's height by ~22px/pin.
  Real but small; out of scope for v1 (the same primitive could be applied later).

---

## Background — what already exists

### `make_space_for_inline` (`node_inlining.rs`)

```rust
pub fn make_space_for_inline(
    network: &mut NodeNetwork,
    instance_id: u64,
    anchor: DVec2,           // top-left of the growing node (its position)
    original_size: DVec2,    // footprint before growth
    content_size: DVec2,     // footprint after growth
    node_sizes: &HashMap<u64, DVec2>,
) -> DVec2                    // returns the applied delta
```

It keeps the growing node's top-left fixed and shifts every other node in
`network` that lies in the lower-right sweep band by `delta = max(0, content -
original)`, using a size-aware "completely above / completely left" guard. It
operates on **one** network level and **mutates positions in place** — it does not
report *which* nodes moved.

### Recursive size estimation (`node_inlining.rs`)

`estimate_node_size_in_network(node, registry)` returns a node's true rendered
size, recursing into zone bodies via `rendered_body_size` (mirrors Flutter's
`_computeBodySize`). `estimate_network_node_sizes(network, registry)` maps every
node id to its size. `instance_size(node, registry)` is the single-node form.
These already exist (added with the closure-conversion fix) and are the sizing
authority for reflow.

### The undo machinery (`undo/`)

- `UndoCommand` trait: `description()`, `undo(ctx)`, `redo(ctx)`,
  `refresh_mode()`. `UndoRefreshMode ∈ { Lightweight, NodeDataChanged(Vec<u64>),
  Full }`.
- **No command-grouping mechanism exists.** Each `push_command` is one undo step.
  `suppress_recording`/`resume_recording` only *prevent* recording.
- Position changes are captured one of two ways today:
  - **Snapshot style** — whole-network `SerializableNodeNetwork` (`inline`,
    `convert_to_closure`) or whole-body `ZoneBodySnapshot` (`EditZoneBodyCommand`).
    Captures every position in its scope for free, but stores the whole network.
  - **Explicit-move style** — `MoveNodesCommand { network_name, scope_path,
    moves: Vec<(u64, DVec2, DVec2)>, description }`, `Lightweight` refresh,
    **scope-aware** (resolves via `ctx.network_in_scope_mut`).

This design deliberately uses the **explicit-move style** — it stores only the ids
and positions that actually changed, never a whole-network copy.

---

## The decisive structural fact

For each trigger, the question is: **does the triggering command's snapshot
already cover the network where reflow moved nodes?**

| Case | Trigger's current command | Where reflow moves nodes | Auto-captured? |
|---|---|---|---|
| **A**, top-level (f-wire or f-source-node deleted) | `DeleteWiresCommand` / `DeleteNodesCommand` — no positions | siblings in the same top-level network | ❌ |
| **A**, inside a body | `EditZoneBodyCommand` — whole-body before/after | siblings in the **same body** | ✅ (whole-body *after*-snapshot, taken at push time) |
| **B**, top-level or body | `SetCollapseModeCommand` — stores only `old_mode`/`new_mode` | siblings in the HOF's own network/body | ❌ |
| **C** (in-body add/paste grows the body) | `EditZoneBodyCommand` — snapshots **only that body** | siblings of the **enclosing HOF** = the **parent** scope (and up) | ❌ (moves land *outside* the snapshotted body) |

**A-in-a-body is already free.** `push_zone_body_command` takes a *fresh*
after-snapshot at push time, so as long as reflow runs *before* it, the moved
body-sibling positions ride along. No new undo work for that sub-case.

Everything else needs the moves explicitly bundled into the same undo step:
B (its own scope), A-top (top level), C (ancestor scopes the cascade reached).

---

## New primitives

### 1. `CompositeCommand` (`undo/commands/composite.rs`)

The piece foreshadowed by `undo/AGENTS.md` ("safety valve for future compound
operations"). Bundles N child commands into one undo step:

```rust
pub struct CompositeCommand {
    pub commands: Vec<Box<dyn UndoCommand>>,
    pub description: String,
}

impl UndoCommand for CompositeCommand {
    fn description(&self) -> &str { &self.description }
    fn undo(&self, ctx) { for c in self.commands.iter().rev() { c.undo(ctx); } }
    fn redo(&self, ctx) { for c in &self.commands { c.redo(ctx); } }
    fn refresh_mode(&self) -> UndoRefreshMode {
        // strongest child wins: Full > NodeDataChanged(∪ ids) > Lightweight
        combine_refresh_modes(self.commands.iter().map(|c| c.refresh_mode()))
    }
}
```

Notes:
- `undo` runs children in **reverse**, `redo` in **forward** order — the standard
  composite convention. In practice `MoveNodesCommand` sets *absolute* positions,
  so its order relative to the primary command is immaterial; reverse-on-undo is
  kept for correctness with any future order-dependent child.
- `combine_refresh_modes` folds the children: any `Full` ⇒ `Full`; else union all
  `NodeDataChanged` id-lists (a `Lightweight` contributes nothing); else
  `Lightweight`. This lives in `undo/mod.rs` next to `UndoRefreshMode`.
- A composite with a single child is never constructed — callers push the bare
  child when reflow produced no moves (see "Wiring" below).

### 2. The reflow helper — returns its moves

`make_space_for_inline` mutates in place; reflow must additionally **report what
moved** so the caller can record `MoveNodesCommand`s. Add a `StructureDesigner`
method (it needs the registry and scope walking):

```rust
/// One reflow step at `scope_path` for `node_id`, which has just grown from
/// `old_sizes[0]`. Re-estimates its new size; if it grew, makes space in its own
/// network and records the moves. If that network is itself a zone body whose
/// own footprint grew past its stored size, recurses one scope up with the
/// enclosing HOF as the node — the cascade. Returns one entry per scope that
/// actually moved nodes (empty if nothing grew).
///
/// CONTRACT: `node_id` MUST be a member of `network(scope_path)`. For in-body
/// growth that has no in-place growth at `scope_path` itself (Case C), the caller
/// starts one scope up — see "Undo wiring per case".
pub fn reflow_for_footprint_change(
    &mut self,
    scope_path: &[u64],          // network containing the node that grew
    node_id: u64,                // the grown node — MUST be a member of network(scope_path)
    old_sizes: &[DVec2],         // pre-edit footprints, captured before the edit (see below)
) -> Vec<ScopedMoves>;

pub struct ScopedMoves {
    pub scope_path: Vec<u64>,                  // network the moves apply to
    pub moves: Vec<(u64, DVec2, DVec2)>,       // (id, old_pos, new_pos)
}
```

Algorithm (iterative up the scope chain):

```
let mut out  = vec![];
let mut path = scope_path;          // network containing `node_id`
let mut nid  = node_id;
let mut step = 0;                    // index into old_sizes
loop {
    let old   = old_sizes[step];                   // pre-edit footprint, captured by caller
    let net   = get_scope_network(path)?;          // nid is a member of net (the contract)
    let new   = estimate_node_size_in_network(net.nodes[nid], registry);
    let delta = max(0, new - old);
    if delta == 0 { break; }                       // growth absorbed; stop

    // record sibling positions before, make space, diff to (id, old_pos, new_pos)
    let before = positions of net.nodes (excluding nid);
    let sizes  = estimate_network_node_sizes(net, registry);
    make_space_for_inline(net_mut, nid, net.nodes[nid].position, old, new, sizes);
    let moves  = before.filter(pos changed).map(|(id, o)| (id, o, net.nodes[id].position));
    if !moves.is_empty() { out.push(ScopedMoves { path, moves }); }

    if path.is_empty() { break; }                  // reached top-level; done

    // Cascade one scope up: net (a body) grew, so its owning HOF grows in the
    // parent network. The enclosing HOF is path.last(); its parent is path[..len-1].
    nid   = path[len-1];
    path  = path[..len-1];
    step += 1;                                      // old_sizes[step] = this HOF's pre-edit size
}
return out;
```

Every `old` size must be the footprint **before** the edit, because by the time
reflow runs the bodies have already grown — they cannot be re-estimated after the
fact. The caller captures them up front into `old_sizes` (one cheap
`instance_size` each): `old_sizes[0]` is `node_id`'s pre-edit size; `old_sizes[k]`
(k ≥ 1) is the pre-edit size of the ancestor HOF `scope_path[len-k]` reached after
the k-th cascade step. The slice need only be as long as the cascade can actually
climb; for Case A/B with a top-level HOF it is just `[old_size]` and the loop runs
once.

> **Why iterative, not a whole re-layout.** Each step only touches the lower-right
> sweep band in one network; a node whose growth is fully absorbed by its body's
> existing slack stops the cascade immediately (`delta == 0`). This keeps reflow
> proportional to the actual disturbance, not the network size.

---

## Undo wiring per case

General rule: **bundle a `MoveNodesCommand` only for the scopes the primary
command's snapshot does not already cover.** When `reflow_for_footprint_change`
returns no moves, push the bare primary command (no composite).

### Case B — `set_collapse_mode` (simplest)

`structure_designer.rs::set_collapse_mode`. Capture the HOF's pre-flip
footprint(s) before changing the mode; after, run
`reflow_for_footprint_change(scope_path, hof_id, &old_sizes)`. Here `hof_id` is a
member of `network(scope_path)`, so the contract holds directly.

```
push Composite[
    SetCollapseModeCommand { .. },            // existing, Lightweight
    MoveNodesCommand per ScopedMoves,         // Lightweight, scope-aware
]
```

Refresh stays **Lightweight** (collapse + moves are both presentational — no
re-eval). When the HOF is top-level the helper runs once (`old_sizes = [old_size]`,
the cascade breaks at the empty path). When the HOF is itself nested in a body the
flip grows that body and the cascade climbs exactly as in Case C, so capture the
ancestor sizes too. This case is the integration test-bed for the primitives.

### Case A — `f`-pin disconnect

`structure_designer.rs::delete_selected_scoped`. The HOF whose `f` got
disconnected lives in the **same network as the deleted wire/node**:

- **Top-level** (the `DeleteWiresCommand` / `DeleteNodesCommand` paths). The set
  of affected HOFs is a *post*-deletion fact (`resolve_body_collapsed` flips only
  once the `f` pin is unwired), but each compact `old_size` must be read *before*
  the deletion. So the steps are ordered explicitly:

  1. **Compute the deletion's effect without applying it** — the wires that will
     be removed: the selected `deleted_wires`, plus (for a node deletion) every
     wire incident to the deleted nodes.
  2. **Predict the flips:** filter the destination ends of those wires to
     collapsable HOFs in `Auto` mode that are currently collapsed and whose `f`
     pin will be left unwired by the deletion. These are the HOFs that will expand.
  3. **Snapshot their compact `old_size`** (still pre-deletion).
  4. **Apply the deletion,** then run reflow for each predicted HOF in the
     top-level network and bundle:
     ```
     push Composite[ DeleteWiresCommand | DeleteNodesCommand,  MoveNodesCommand… ]
     ```

  If step 2 yields no HOFs, push the bare `DeleteWiresCommand` / `DeleteNodesCommand`.
- **In a body** (the `EditZoneBodyCommand` path, `delete_selected_scoped`'s
  `scope_path` non-empty branch): apply the same predict-then-snapshot ordering.
  Here the expanding HOF *is* a member of the edited body, so reflow is called with
  `scope_path` = that body and `node_id` = the HOF — the contract holds and reflow
  starts *inside* the body (unlike Case C). Run it **before**
  `push_zone_body_command`: the moves within that body are captured by the fresh
  after-snapshot — **no composite needed for the body scope**. Only if the HOF's
  expansion grew the body past its stored size does the cascade reach an ancestor
  scope; bundle a `MoveNodesCommand` for each such ancestor (composite with the
  `EditZoneBodyCommand`).

> Deleting an `f`-**source node** disconnects `f` via the node's wire landing in
> `DeleteNodesCommand.deleted_wires`; it is the same trigger and the same handling
> as deleting the wire directly.

### Case C — in-body growth cascade

The four body-scoped structural orchestrators that grow a body —
`add_node_scoped`, `paste_nodes` (body branch), `duplicate_node_scoped`, and the
body `connect_nodes_scoped` paths — all finish with `push_zone_body_command`.

Adding / pasting / duplicating a node does **not** grow an existing in-body node
*in place*, so reflow produces **no moves inside the edited body itself**. What
grows is the body's *owning HOF*, whose footprint expands in the **parent**
network. The cascade therefore starts one scope up — at `parent =
scope_path[..len-1]` with `node_id = scope_path[len-1]` (the HOF that owns the
edited body), which satisfies the contract (that HOF *is* a member of `parent`).
Capture that HOF's pre-edit footprint (and any further ancestors the cascade may
reach) **before** the edit:

```
let before_body  = snapshot_zone_body(scope_path);
let hof_id       = scope_path[len-1];                        // owns the edited body
let parent       = scope_path[..len-1];                      // network the HOF lives in
let old_sizes    = capture_ancestor_sizes(parent, hof_id);   // pre-edit: [hof_old, grandparent_old, …]
... perform the body edit ...                                // body now contains the new node
let scoped_moves = reflow_for_footprint_change(parent, hof_id, &old_sizes);
// Every scoped_moves entry lands in `parent` or higher — none in `scope_path`.
let edit_cmd  = build EditZoneBodyCommand(before_body, after = snapshot now);
let move_cmds = scoped_moves.map(MoveNodesCommand);          // all ancestor scopes
if move_cmds.is_empty() { push edit_cmd }
else { push Composite[ edit_cmd, move_cmds… ] }
```

The added node itself rides the `EditZoneBodyCommand` after-snapshot (taken at
push time), so the order is "edit → reflow → push". The ancestor
`MoveNodesCommand`s use the ancestor `scope_path`s (shorter chains than
`scope_path`), and `network_in_scope_mut` resolves them on undo/redo.

This path also subsumes the **residual cascade in `convert_instance_to_closure`**:
when a closure is created inside a body and the body grows, the same ancestor
reflow applies. Fold that call site into Phase 3.

---

## Edge cases & pitfalls

- **Capture `old_size` before mutating.** Every case re-estimates *after* the
  edit; the *before* size must be snapshotted first. For the cascade, snapshot all
  ancestor HOF sizes along `scope_path` up front.
- **`delta == 0` stops the cascade.** A body with enough slack to absorb the
  growth produces no moves; reflow returns early and the bare primary command is
  pushed. Do not assume reflow always moves something.
- **No double-capture.** Never bundle a `MoveNodesCommand` for a scope already
  inside a snapshot-style primary command (the in-body scope of an
  `EditZoneBodyCommand`). It would be redundant (both set the same absolute
  positions) and clutter the step. Bundle only uncovered scopes.
- **Refresh strength.** `combine_refresh_modes` must promote to the strongest
  child so e.g. a deletion's `Full`/`NodeDataChanged` is not downgraded to the
  move's `Lightweight`.
- **`MoveNodesCommand` excludes the grown node.** `make_space_for_inline` already
  skips `instance_id`; the diff therefore never lists the grown node itself, only
  its neighbours. Good — the grown node's position is unchanged.
- **Selection is not undoable** (existing invariant) — reflow touches only
  positions, never selection.
- **Flutter side is automatic.** Positions are authoritative in Rust; the
  `ScopeResolver` re-derives layout each frame from node positions, so no Flutter
  change is required — the neighbours simply render in their new spots after the
  refresh.

---

## Implementation phases

Each phase is independently shippable and testable. Tests go in
`rust/tests/structure_designer/` (new `reflow_test.rs`, registered in
`tests/structure_designer.rs`), plus additions to `undo_test.rs`.

### Phase 0 — Primitives (no behaviour change)

- `undo/commands/composite.rs`: `CompositeCommand` + register in
  `commands/mod.rs`.
- `undo/mod.rs`: `combine_refresh_modes`.
- `StructureDesigner::reflow_for_footprint_change` + `ScopedMoves` (in
  `structure_designer.rs`, leaning on the existing `node_inlining` sizing fns).
- Tests: `CompositeCommand` undo/redo order + `combine_refresh_modes` table;
  `reflow_for_footprint_change` on a hand-built network returns the expected
  `(id, old, new)` moves and the correct cascade across two body levels (pure,
  no undo).

### Phase 1 — Case B (`set_collapse_mode`)

- Wire reflow + composite into `set_collapse_mode`. Single scope when the HOF is
  top-level; a nested HOF cascades like Case C, so pass the ancestor sizes.
- Tests: Auto/Collapsed→Expanded pushes neighbours; undo restores both the mode
  **and** the neighbour positions in one step; redo re-applies; Expanded→Collapsed
  (shrink) moves nothing and pushes only the bare `SetCollapseModeCommand`.

### Phase 2 — Case A (`f`-pin disconnect)

- Top-level `delete_selected_scoped`: detect HOFs that lost `f` and flipped to
  expanded, capture compact `old_size`, reflow, wrap `DeleteWires`/`DeleteNodes`
  in a composite.
- Body `delete_selected_scoped`: run reflow before `push_zone_body_command`;
  assert the in-body case needs no composite (covered by the after-snapshot), and
  the ancestor cascade (if any) is bundled.
- Tests: delete the `f` wire feeding a compact `map` next to a downstream node →
  neighbour pushed, single-step undo restores wire **and** position; delete the
  `f`-source node variant; body-scope variant (auto-covered).

### Phase 3 — Case C (in-body growth cascade)

- Wire reflow into `add_node_scoped`, `paste_nodes` (body), `duplicate_node_scoped`,
  body `connect_nodes_scoped`; capture the owning HOF's + ancestor sizes pre-edit;
  start reflow at the **parent** scope (`node_id` = the body-owning HOF); bundle the
  ancestor `MoveNodesCommand`s with the `EditZoneBodyCommand`.
- Fold the `convert_instance_to_closure` residual ancestor cascade into the same
  helper call.
- Tests: add a node inside a `map` body that grows it → the `map`'s sibling in the
  parent network is pushed; two-level nesting cascades to the grandparent; one
  single-step undo restores the body edit and all ancestor positions; a body with
  slack (`delta == 0`) pushes nothing.

---

## Files touched

- **New:** `rust/src/structure_designer/undo/commands/composite.rs`
  (+ register in `undo/commands/mod.rs`).
- **New:** `rust/tests/structure_designer/reflow_test.rs`
  (+ register in `rust/tests/structure_designer.rs`).
- `rust/src/structure_designer/undo/mod.rs` — `combine_refresh_modes`.
- `rust/src/structure_designer/structure_designer.rs` —
  `reflow_for_footprint_change` + `ScopedMoves`; reflow/composite wiring in
  `set_collapse_mode` (P1), `delete_selected_scoped` (P2),
  `add_node_scoped` / `paste_nodes` / `duplicate_node_scoped` /
  `connect_nodes_scoped` / `convert_instance_to_closure` (P3).
- `rust/tests/structure_designer/undo_test.rs` — per-phase single-step undo/redo
  coverage.

No Flutter, FFI, or serialization changes — reflow only moves nodes, and the
Flutter `ScopeResolver` already re-derives layout from positions each frame.
