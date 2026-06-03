# Design: Inline a Custom Node

## Summary

Add an **Inline** operation that is the structural inverse of *Factor Selection into
Subnetwork*. Right-clicking a custom-network instance (a node whose `node_type_name`
resolves to a user network) offers **Inline**, which replaces that single node with a
copy of the contents of its custom network, spliced into the parent network in place.

The named custom-network definition is **left untouched** in the registry — other
instances of it may exist. Inlining only mutates the **parent** network (or the zone
body) that contained the instance.

## Background: the existing factor-OUT feature

`rust/src/structure_designer/selection_factoring.rs` implements the collapse direction:

- `analyze_selection_for_factoring` — finds wires crossing the selection boundary.
  External inputs become `parameter` nodes; the single external output becomes the
  return.
- `create_subnetwork_from_selection` — builds a new `NodeNetwork`, copies selected
  nodes with **id remapping** and positions offset by `-center`, creates `parameter`
  nodes, rewires internal connections, sets `return_node_id`.
- `replace_selection_with_custom_node` — adds a custom node instance (referenced purely
  by `node_type_name`), wires it, deletes the originals.
- Orchestrator `StructureDesigner::factor_selection_into_subnetwork`
  (`structure_designer.rs`) snapshots before/after and pushes `FactorSelectionCommand`
  (three `SerializableNodeNetwork` snapshots: source-before, source-after, subnetwork).

Inlining runs this logic **in reverse**, but is simpler in one important way: it touches
only one network, so its undo is a single before/after snapshot rather than a
two-network command.

## Structural correspondence

For a custom node instance `I` whose `node_type_name` resolves to network `N`:

| Instance `I` (in parent)        | Network `N` (definition)                         |
|---------------------------------|--------------------------------------------------|
| input pin `p`                   | the `parameter` node with `param_index == p`     |
| output pin `i`                  | output pin `i` of `N.return_node_id` (multi-out) |
| (the instance node itself)      | all non-`parameter` nodes of `N`                 |

Inlining therefore:

1. Copies every **non-`parameter`** node of `N` into the parent (fresh ids, shifted
   into place).
2. Splices the parent's incoming wires on `I`'s input pin `p` through to whatever inside
   `N` consumed `parameter` node `p`.
3. Splices `N`'s return source through to whatever in the parent consumed `I`'s
   output pin(s).
4. Deletes `I`.

## Node placement ("make space")

The inlined content is generally larger than the single node it replaces. We make room
with a simple, predictable algorithm: keep the original node's **upper-left corner**
fixed and push the lower-right region of the parent outward by exactly the extra space
the content needs.

```text
anchor        = I.position                      // top-left, stays fixed
original_size = (NODE_WIDTH, estimate_node_height(I, ...))   // node_layout.rs
content_size  = bounding box of the inlined (non-parameter) nodes of N
delta         = max(DVec2::ZERO, content_size - original_size)   // componentwise

right_edge  = anchor.x + original_size.x
bottom_edge = anchor.y + original_size.y

for every other node n in the same scope (n != I):
    if n.position.x > right_edge:  n.position.x += delta.x
    if n.position.y > bottom_edge: n.position.y += delta.y
```

Notes:

- A node in the **lower-right quadrant** (past both edges) is shifted on both axes.
- Each node is measured by its **top-left `position`** (the field stored on `Node`;
  regular nodes have no per-node width/height — only HOFs carry `body_width`/
  `body_height`). It is compared against the **original node's right/bottom edges**, so
  nodes that merely overlap the original vertically or horizontally don't move.
- Node sizes come from `node_layout::estimate_node_height` / `NODE_WIDTH`, which mirror
  the Flutter `getNodeSize()` formula (`TITLE_HEIGHT + max(in*22, out*22, 25) +
  subtitle + PADDING`). `content_size` is computed from the bounding box of the copied
  nodes' positions, each expanded by its estimated size.
- The inlined content is placed starting at `anchor`, spanning
  `[anchor, anchor + content_size]`. Because nodes below the original shift down by
  `delta.y = content_height - original_height`, they always end up below
  `anchor.y + content_height` (the content's bottom), so no vertical overlap occurs.

## Scopes: the splice must be scope-aware

`N`'s top-level nodes do **not** all live in a single flat layer. Any of them can be a
higher-order-function node (`map` / `filter` / `fold` / `foreach` / `closure`) carrying an
inline `zone` body, nested to arbitrary depth. Body wires can reference scopes above their
own via `IncomingWire::source_scope_depth` (see `node_network.rs`):

- `depth == 0` — source in the same network.
- `depth == d ≥ 1` — source lives `d` ancestor frames up. Used by `NodeOutput` **captures**
  *and* `ZoneInput` **iteration-value references** (the latter carries the enclosing HOF's
  id in `source_node_id`).

The instance's top-level content (`N`'s top-level nodes) is spliced into **one** scope —
the *instance scope* (the network resolved by `scope_path`). Inlining adds **no** new scope
level. The crucial consequence: **a body wire at nesting `k` below the instance scope
references the instance scope iff `source_scope_depth == k`.** Everything keyed to the
instance scope — captures of `N`'s top-level nodes, captures of `N`'s `parameter` nodes, and
external references *to* the instance node `I` itself — appears at exactly that
`depth == k` gate, and only there. Shallower wires (`depth < k`) point at intermediate
copied bodies whose ids are preserved by the verbatim body clone, so they are left alone.
(For a self-contained `N`, `depth > k` cannot occur — `N`'s bodies cannot reference above
`N`'s own top level.)

This is the **same** gate the paste path already uses:
`node_network::remap_body_wires_to_pasted_scope` (called from `copy_nodes_from`) remaps
`source_node_id`s only where `source_scope_depth == nesting`, descending into every body and
covering both `NodeOutput` and `ZoneInput` sources and both `arguments` and
`zone_output_arguments`. Inlining reuses that exact traversal skeleton; it only adds a
**parameter-splice** branch to the per-wire classification (and a symmetric **return-splice**
on the consumer side). A flat-only splice (the obvious "rewire top-level consumers of the
parameter / instance" implementation) silently breaks every capture of a parameter or of the
instance that lives inside an HOF body — a routine pattern (a custom network whose `map` body
uses one of the network's parameters as a constant).

## Module layout — `rust/src/structure_designer/node_inlining.rs`

Three helpers:

```rust
// (1) The placement algorithm above.
fn make_space_for_inline(
    network: &mut NodeNetwork,
    instance_id: u64,        // excluded from the shift
    anchor: DVec2,
    original_size: DVec2,
    content_size: DVec2,
) -> DVec2;                  // the delta actually applied (for tests)

// (2) Copy N's non-`parameter` node STRUCTURE into the instance scope: fresh
//     ids, shifted positions, verbatim body `Arc`s. Touches NO wires — all
//     wire fix-up happens in (3).
fn copy_content_into(
    target: &mut NodeNetwork,
    source: &NodeNetwork,                // N
    anchor: DVec2,
    content_min: DVec2,                  // top-left of source content bbox
) -> HashMap<u64, u64>;                  // old_id -> new_id (parameter nodes absent)

// (3) All wire fix-up, scope-aware. Performs the two descents below + deletes
//     the instance.
fn splice_inline_boundary(
    target: &mut NodeNetwork,
    instance_id: u64,
    source: &NodeNetwork,                // N — read params + return node
    id_mapping: &HashMap<u64, u64>,
);
```

### `copy_content_into` details

- Allocates fresh ids from `target.next_node_id` for each non-`parameter` node of `source`,
  building `id_mapping: old_id -> new_id`.
- New position = `anchor + (old.position - content_min)`, so the content's top-left
  lands exactly on `anchor`.
- Clones the whole `Node` (incl. `data`, `custom_node_type`, `zone`,
  `zone_output_arguments`, `body_*`, `collapse_mode`). HOF nodes inside `N` keep their
  `Arc<NodeNetwork>` body verbatim. **No wires are rewired here** — the copied nodes'
  `arguments`, `zone_output_arguments`, and all nested body wires still carry `N`'s original
  ids on exit. `splice_inline_boundary` is the single place that fixes every wire, so the
  id-classification happens exactly once, against `N`'s original id space (this avoids a
  subtle hazard: a freshly allocated `new_id` can numerically collide with one of `N`'s old
  parameter ids, so any two-pass "remap, then match parameters by id" scheme could
  misclassify).

(The per-node custom-type cache for the copied content is repopulated by the orchestrator
after the splice — see step 7 below.)

### `splice_inline_boundary` details

Let `param_id(p)` be `N`'s `parameter` node with index `p`, and let `instance_wires(p)` be
the instance's incoming wires on input pin `p` (read from `I.arguments[p]` *before* any
mutation; preserved **verbatim** — `source_pin` shape and `source_scope_depth` — so an
instance that itself lives in a zone body keeps its captures / zone-input refs). If pin `p`
is unconnected, `instance_wires(p)` falls back to `param_id(p)`'s own default `Argument`
wires, with their `source_node_id` remapped through `id_mapping` (a parameter default, when
present, references a node *inside* `N`); if that is also empty, `instance_wires(p)` is empty
and consumer wires are dropped.

**Descent A — fix the copied content (parameters + internal references).** Walk every copied
node, tracking nesting `k` relative to the instance scope (`k = 0` for the copied top-level
nodes; `k = 1` for each of their bodies; deeper bodies `k = 2, …`). For each wire `w` (in
`arguments`, and in `zone_output_arguments` for the same node) **with `w.source_scope_depth == k`**,
classify by `w.source_node_id` (an original `N` id — copied-node ids and parameter ids are
disjoint, so this is unambiguous):

- **`w.source_node_id == param_id(p)`** → replace `w` with `instance_wires(p)`, each wire's
  depth shifted by `k`: `new_depth = k + iw.source_scope_depth`. (At `k = 0` this is the
  instance wires verbatim — the flat case. At nesting `k`, the same physical source must be
  reached from `k` frames deeper, hence `+ k`.) Multiple instance wires on a multi-input pin
  replicate; an empty `instance_wires(p)` drops `w`.
- **`w.source_node_id ∈ id_mapping`** → keep `w`, set `source_node_id = id_mapping[old]`
  (pin and depth unchanged). This is exactly `remap_body_wires_to_pasted_scope`'s action.
- **otherwise** → drop (cannot happen for a valid self-contained `N`).

For `k = 0`, every top-level copied-node `arguments` wire has `depth == 0` and is processed
(`N`'s top-level nodes cannot capture, so all their wires are local). Their
`zone_output_arguments` reference body-internal nodes (preserved ids) and are not at
`depth == 0` in the `S0` sense, so they are untouched — matching the paste traversal, which
likewise never rewrites a top-level HOF's own `zone_output_arguments`.

**Descent B — repoint consumers of the instance to the return node.** Determine the return
source: `(id_mapping[N.return_node_id], pin i)`, preserving pin index for multi-output
passthrough; if `N` has no `return_node_id`, the return source is *none*. Walk the instance
scope and all its bodies, **skipping the freshly copied nodes (do not descend into their
bodies either)** — copied content comes from `N` and can never reference `I` — tracking
nesting `k`. For each wire `w` with
`w.source_scope_depth == k` **and `w.source_node_id == instance_id` and `w.source_pin ==
NodeOutput { i }`**: if a return source exists, set `source_node_id` to the return node id
(keep pin `i`, keep depth — the return node now occupies `I`'s former scope position, so no
depth shift); otherwise drop `w`. This handles both flat consumers (`k = 0`) and a sibling
HOF body that captured the instance's output (`k ≥ 1`).

**Delete the instance.** After Descent B no wire references `I`, so deletion is just
`target.displayed_nodes.remove(&instance_id)` + `target.nodes.remove(&instance_id)`.

> **Assumption (shared with the paste path):** `zone_output_arguments` wires are body-local
> (`depth == 0` in their body's frame) — a zone-output pin reads a body-internal source, not
> a raw outer capture. Under this invariant, Descent A's inclusion of `zone_output_arguments`
> is a no-op at `k ≥ 1` (their `depth == 0 ≠ k`), exactly as in
> `remap_body_wires_to_pasted_scope`. We include them for parity; if the invariant were ever
> relaxed (capture wired directly to a zone-output pin), inline and paste would need the same
> follow-up — out of scope here.

### Display state and names

- Copied nodes inherit their display state from `N` (round-trip faithful with factoring,
  which copied display state into the subnetwork). *Alternative considered:* only the
  return node inherits the instance's display type and the rest stay hidden — simpler but
  loses fidelity. We go with inheriting from `N`.
- `custom_name` collisions between copied nodes and existing parent nodes are
  de-duplicated (suffix bump), mirroring `make_names_unique`'s spirit.

## Orchestrator

```rust
// structure_designer.rs
pub fn inline_custom_node(
    &mut self,
    scope_path: Vec<u64>,
    node_id: u64,
) -> Result<(), String>;
```

Per `rust/AGENTS.md` ("Addressing Nodes Across Scopes"), this is **scope-aware**: the
target network is resolved via `get_scope_network_mut(&scope_path)` (empty = top-level
active network; non-empty = the chain of HOF node ids down to a body).

Steps:

1. Resolve the instance node in the target scope. Error if not found.
2. Verify it is a custom-network instance: `registry.is_custom_node_type(node_type_name)`.
   This single check is the whole gate — built-ins, HOFs, `apply`, and `closure` are not
   custom types, so they are rejected automatically (no separate reject list needed). Error
   "Only custom network nodes can be inlined".
3. Clone the definition `N = registry.node_networks[node_type_name].clone()` (we read from it
   while mutating the target).
4. Compute `content_min` / `content_size` from `N`'s non-`parameter` nodes.
5. Snapshot for undo (see below).
6. `make_space_for_inline(...)`, `copy_content_into(...)`, `splice_inline_boundary(...)`.
7. `registry.initialize_custom_node_types_for_network(target)` — repopulate per-node
   custom-type caches for the copied content (descends into bodies; as
   `create_subnetwork_from_selection` does).
8. `validate_active_network()` — required; refresh paths do not validate
   (`project_refresh_does_not_validate`).
9. `is_dirty = true; mark_full_refresh()`.

## Undo

Inline mutates exactly one network, so it needs a single before/after snapshot — no need
for the three-snapshot `FactorSelectionCommand`. Branch on scope:

- **Top-level** (`scope_path` empty): before/after `SerializableNodeNetwork` snapshot of
  the active network. Either add a small `InlineNodeCommand` modeled on
  `TextEditNetworkCommand` (both are "parent network before → after"), or reuse the
  text-edit command pattern directly. `refresh_mode = Full`.
- **Inside a zone body** (`scope_path` non-empty): use
  `StructureDesigner::snapshot_zone_body` / `push_zone_body_command`
  (`EditZoneBodyCommand`), which snapshot the body `NodeNetwork` plus the HOF's
  `zone_output_arguments`.

## API layer

`rust/src/api/structure_designer/structure_designer_api.rs`:

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn inline_custom_node(scope_path: Vec<u64>, node_id: u64) -> InlineResult;
```

`InlineResult` is a new type in `structure_designer_api_types.rs`, modeled on
`FactorSelectionResult` but without `new_node_id` (inline produces many nodes, not one):

```rust
pub struct InlineResult {
    pub success: bool,
    pub error: Option<String>,
}
```

On `Ok` the wrapper calls `refresh_structure_designer_auto(cad_instance)` and returns
`{ success: true, error: None }`; on `Err` it returns `{ success: false, error: Some(msg) }`
(shown as a snackbar, like the factor dialog). A read-only
`can_inline_node(scope_path, node_id) -> bool` (just `is_custom_node_type` on the resolved
node) can gate the menu item, mirroring `get_factor_selection_info`.

## Flutter UI

`lib/structure_designer/node_network/node_widget.dart`:

- In `_handleContextMenu`, the `if (isCustomNode)` block (~line 1620, next to
  *Go to Definition*) gains an **`'inline'`** `PopupMenuItem` labeled *"Inline"*.
- A dispatch branch in the `.then((value) {...})` handler calls a thin model method
  `model.inlineCustomNode(node)` which calls the API with the node's scope chain
  (`propertyEditorScopeChain` / the node's scope path) and then `refreshFromKernel()`.

No dialog is needed (unlike factoring, which collects a name + param names) — inline
takes no user input.

## Edge cases

- **Multi-output custom node** — instance pin `i` ↔ return node pin `i`; handled by
  preserving the pin index in the return splice (Descent B).
- **Custom network nodes that are themselves HOFs** with `zone` bodies — the `Arc` body is
  cloned as-is and body-internal ids are preserved, **but** any body wire reaching `N`'s top
  level (`source_scope_depth == nesting`) is fixed by Descent A: a capture of a copied node
  is remapped through `id_mapping`; a capture of a `parameter` node is spliced to the
  instance's input wires (depth shifted by the nesting). Without this, nested captures of
  `N`'s top-level nodes dangle. This is the core correctness reason the splice descends into
  bodies rather than touching only the top level.
- **Inlining inside a zone body** — captures (`source_scope_depth ≥ 1`) and zone-input
  references on the instance's input wires are preserved verbatim by `instance_wires(p)`, and
  the per-nesting depth shift (`k + iw.source_scope_depth`) keeps them resolving against the
  correct enclosing scope from inside any nested copied body.
- **Sibling HOF body captures the instance's output** — Descent B descends into the instance
  scope's other bodies and repoints `(I, pin i)` captures to the return node at their own
  nesting depth, not just flat top-level consumers.
- **Unconnected instance input** — `instance_wires(p)` falls back to the `parameter` node's
  default `Argument` wires (remapped through `id_mapping`), or drops the consumer wire if the
  default is also empty.
- **No return node in `N`** — all consumers of the instance's output (flat and nested) are
  dropped.
- **`custom_name` collisions** — de-duplicated on copy.

## Testing

Tests in `rust/tests/structure_designer/` (new `node_inlining_test.rs`, registered in
`tests/structure_designer.rs`), mirroring `selection_factoring`'s coverage:

- Basic inline (single-param, single-output) — node count, wiring, instance removed.
- Multi-parameter inline — each input pin spliced to the correct parent source.
- Multi-output inline — each output pin's consumers spliced to the correct return pin.
- Unconnected input — default fallback / dropped wire.
- Inline inside a zone body — scope resolution + capture/zone-input preservation.
- **Nested capture of a parameter** — `N` contains an HOF whose body uses a `parameter` as a
  constant; assert the body wire is spliced to the instance's pin source with
  `source_scope_depth == k + d_I` (Descent A). Cover `k = 1` and `k = 2`, and an instance
  whose own pin wire is itself a capture (`d_I ≥ 1`).
- **Nested capture of a copied node** — body capture of one of `N`'s top-level nodes is
  remapped through `id_mapping` (and *not* misclassified when a `new_id` collides with an old
  parameter id).
- **Sibling HOF body captures the instance output** — Descent B repoints the deep capture to
  the return node; verify depth is preserved.
- `make_space_for_inline` placement — assert exact post-shift positions, incl. the
  lower-right both-axes case and the no-move (overlapping) case.
- Undo/redo round-trip — top-level (`InlineNodeCommand`) and body
  (`EditZoneBodyCommand`); the network is byte-identical to before after undo
  (`normalize_json` for HashMap ordering).
- Reject non-custom-node inline (built-in, HOF, `apply`).

## Implementation phases

Each phase compiles, is independently testable, and leaves the test suite green. Phases 1–3
are Rust-only; phase 4 wires up FFI + Flutter. The scope-aware splice (the heart of the
feature) is built and fully tested at top level in phase 2; phase 3 only changes how the
target network is resolved and snapshotted, because the splice is scope-relative.

**Phase 1 — Placement + structural copy (pure helpers, no orchestrator).**

- `make_space_for_inline` and `copy_content_into` in the new `node_inlining.rs`.
- `copy_content_into` clones `N`'s non-`parameter` nodes with fresh ids, shifted positions,
  and verbatim body `Arc`s; returns `id_mapping`. Touches no wires.
- Tests: `make_space_for_inline` placement (lower-right both-axes shift, no-move/overlap
  case, exact applied delta); `copy_content_into` node count, `id_mapping` completeness
  (parameter nodes absent), positions, body `Arc` preserved.
- Done when: both helpers are unit-tested in isolation; nothing is wired into
  `StructureDesigner` yet.

**Phase 2 — Scope-aware boundary splice + top-level orchestrator.**

- `splice_inline_boundary` with the full **Descent A** (parameter-splice + copied-node
  remap, recursing into bodies at the `depth == k` gate) and **Descent B** (instance-output
  → return node, recursing), then instance deletion.
- `StructureDesigner::inline_custom_node` for **empty `scope_path` only**: resolve the
  instance, gate on `is_custom_node_type`, clone `N`, compute the content bbox, snapshot,
  call the three helpers, `initialize_custom_node_types_for_network`,
  `validate_active_network`, set dirty + `mark_full_refresh()`.
- `InlineNodeCommand` (before/after `SerializableNodeNetwork`) registered in
  `undo/commands/mod.rs`.
- Tests: basic single-param/single-output; multi-parameter; multi-output; unconnected input
  (default fallback / drop); no-return; reject non-custom; **nested capture of a parameter**
  (`k = 1` and `k = 2`); **nested capture of a copied node** (incl. the `new_id` ↔ old
  parameter-id collision); **sibling HOF body captures the instance output**; undo/redo
  round-trip (`normalize_json`).
- Done when: top-level inline is fully correct, **including an `N` that contains nested HOF
  bodies**. (The `d_I ≥ 1` instance-wire path is implemented here; it is exercised
  end-to-end in phase 3.)

**Phase 3 — Inline inside a zone body.**

- Orchestrator handles a non-empty `scope_path`: resolve the target via
  `get_scope_network_mut(&scope_path)`; snapshot/undo via `snapshot_zone_body` /
  `push_zone_body_command` (`EditZoneBodyCommand`). The splice algorithm is unchanged.
- Tests: inline inside a zone body — scope resolution + capture/zone-input preservation; an
  instance whose own input wire is a capture (`d_I ≥ 1`), asserting the spliced
  `source_scope_depth == k + d_I`; body undo round-trip.
- Done when: inline works in any scope.

**Phase 4 — API + Flutter UI.**

- `inline_custom_node(scope_path, node_id) -> InlineResult` (+ optional `can_inline_node`)
  in `structure_designer_api.rs`; run `flutter_rust_bridge_codegen generate`.
- `node_widget.dart` context-menu item + dispatch; `inlineCustomNode` model method calling
  the API then `refreshFromKernel()`.
- Manual walkthrough (`flutter run`): inline a plain top-level instance, an instance whose
  `N` contains nested HOFs, and an instance living inside a zone body; undo each.
- Done when: the feature is usable from the UI.

## Files touched

- **New:** `rust/src/structure_designer/node_inlining.rs`
- **New:** `rust/tests/structure_designer/node_inlining_test.rs` (+ register in
  `rust/tests/structure_designer.rs`)
- `rust/src/structure_designer/mod.rs` — declare the module.
- `rust/src/structure_designer/structure_designer.rs` — `inline_custom_node` orchestrator
  + undo snapshot wiring.
- `rust/src/structure_designer/undo/commands/inline_node.rs` — `InlineNodeCommand` (or reuse
  the `text_edit_network.rs` snapshot pattern) + register in `undo/commands/mod.rs`.
- `rust/src/api/structure_designer/structure_designer_api.rs` — `inline_custom_node`
  (+ optional `can_inline_node`).
- `rust/src/api/structure_designer/structure_designer_api_types.rs` — `InlineResult` type.
- Regenerate FFI: `flutter_rust_bridge_codegen generate`.
- `lib/structure_designer/node_network/node_widget.dart` — menu item + dispatch.
- `lib/structure_designer/.../structure_designer_model.dart` — `inlineCustomNode` method.
