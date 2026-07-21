# Viewable Bodies of 0-ary Closures

**Issue:** [#409](https://github.com/atomCAD/atomCAD/issues/409) — "make
contents of 0-ary closures viewable"

**Related designs:** `design_closures.md` (the `closure` node + `ZoneClosure`),
`design_zones.md` / `design_zones_ui.md` (inline bodies, scope stacks, capture
wires), `design_custom_closure_kind.md` (0-arity `Custom` closures),
`design_multi_output_pins.md` (per-pin display, `NodeDisplayState`),
`design_function_pin_roles.md` (precedent for relaxing function-related
display suppression).

## Motivation

Grouping nodes inside a `closure` body is a natural way to organize a network,
but today it costs all viewability: every node inside any zone body has its
visibility (eye) toggle hidden, and scene generation only ever iterates the
top-level network's `displayed_nodes`. mechadense's point: for a **0-ary**
closure this trade-off is unnecessary — with no parameters there is no unknown
iteration value, so the body's nodes are fully determined and could render in
the 3D viewport like any top-level node.

The suppression exists because a body node may reference the enclosing zone's
zone-input pins (`element`, `acc`), whose values only exist per invocation
(`NetworkEvaluationContext::current_zone_input` panics without a pushed
frame). But that is the *only* obstacle:

- **Body-local wires** (`source_scope_depth == 0`) evaluate normally.
- **Capture wires** (`depth >= 1`, `NodeOutput` source) are resolved by
  `NetworkEvaluator::resolve_incoming_wire` by walking the **live network
  stack** — no frozen capture cache required. Evaluating a body node against
  the stack `[top-level, body]` resolves captures against exactly the same
  sources, in the same context, that `build_inline_closure` freezes at the
  closure's own eval. Captured values are therefore *not* a blocker;
  scene-time values match invocation-time values by construction.
- **Zone-input references** cannot exist in a body whose entire enclosing
  chain has zero zone-input pins (validation rule 3 enforces this, and a
  violation flips `valid`, which blocks evaluation). Because scene
  evaluation of a body node is the first path ever to evaluate body wires
  *without* a pushed zone frame — where `current_zone_input` panics — Phase
  2 also adds a defensive non-panicking floor for the desync case (see
  Design §3).

This is exactly how the eager HOFs (`fold`/`foreach`) already evaluate body
nodes against a real containing-network stack. No new evaluation machinery is
needed (one small defensive guard aside — Design §3); the work is in the
display pipeline, which is structurally top-level-only today.

## The eligibility rule

> A body node is **scene-evaluable** iff every zone-owning ancestor in its
> scope chain is a `closure` node with **zero parameters** (zero zone-input
> pins on its resolved custom node type).

Consequences:

- A 0-ary closure at the top level: body nodes eligible. A 0-ary closure
  nested inside another 0-ary closure: eligible (recursively).
- A 0-ary closure inside a `map` body: **not** eligible — the `map`'s
  `element` is still unknowable at scene time.
- `map` / `filter` / `fold` / `foreach` bodies are never eligible (their types
  always declare ≥ 1 zone-input pin). This matches the issue explicitly
  factoring ">0-ary viewability" out as a separate, harder problem.
- The preset `ClosureKind`s (`Map`/`Filter`/`Fold`/`Foreach`) all have ≥ 1
  parameter; only `ClosureKind::Custom` with empty `param_names` is 0-ary.

We call a scope path satisfying the rule an **eligible chain**.

## Arity changes: derive, don't mutate (dormant display flags)

When the user adds a parameter to a previously 0-ary closure, its body (and
every nested body under it) must stop rendering and lose its eye toggles. We
do **not** actively clear the body networks' `displayed_nodes` maps. Instead,
stored display flags in a body are treated as **dormant** whenever the chain
is ineligible:

- Scene generation collects displayed nodes **only** from eligible chains, so
  the moment arity goes 0 → 1 every affected body's scene entries drop on the
  next refresh.
- The Flutter eye condition is derived from chain eligibility, so the toggles
  disappear at the same moment.
- If the user flips arity back to 0 (or undoes), the previous display states
  reappear instead of being lost.

This avoids revocation bookkeeping entirely: `set_closure_data` routes through
the generic `set_node_network_data_scoped`, whose undo command does not
snapshot the body network, so a hard clear would require a new compound
command. Derivation sidesteps that and mirrors an existing pattern — an
`f`-overridden HOF body is suppressed by derivation (the scope resolver flags
it), not by mutating body state.

Dormant flags serialize with the body network (harmless; `displayed_nodes` is
already part of `NodeNetwork` serialization) and simply reactivate when the
chain becomes eligible again.

## Goals

- Nodes inside an eligible chain get working per-pin eye toggles; their
  displayed pins render in the 3D viewport exactly like top-level displayed
  nodes (geometry, atoms, hover values, errors).
- Captured values are supported: a body node reading a top-level source via a
  capture wire renders with the live value of that source.
- Toggles are undoable and persisted (`.cnnd` round-trip).
- Adding a parameter to a closure in the chain hides the eyes and stops
  rendering the whole body subtree, by derivation; removing it restores the
  previous display state.
- Behavior of ineligible bodies is unchanged (no eye, no rendering).

## Non-Goals

- **>0-ary viewability** (choosing witness values for parameters, "probe"
  inputs, etc.) — explicitly out of scope, per the issue.
- No automatic display policy inside bodies. Top-level networks run
  `apply_node_display_policy`; body display stays fully manual. A node newly
  added to a body is not auto-displayed.
- No gadgets / direct editing (atom_edit etc.) for body nodes. The gadget
  path keys off the top-level `active_node_id` and stays top-level-only.
- No viewport click-to-activate for body nodes. `viewport_pick` filters its
  candidate set to top-level refs (Design §1); the bare-node-id Flutter
  surfaces (disambiguation overlay, scroll-to-node, solo-hide) are
  untouched. Scoped pick UX is a possible follow-up, not part of this
  feature.
- No text-format surface for body display flags beyond what `NodeNetwork`
  serialization already persists.
- No change to closure/HOF evaluation semantics, capture freezing, or
  `run_closure_once`.

## Design

### 1. Scene keying: `u64` → `NodeRef` (the structural prerequisite)

`StructureDesignerScene.node_data` is `HashMap<u64, NodeSceneData>`, and
per-body `next_node_id` counters mean a body node and a top-level node
routinely share a numeric id. Displaying body nodes therefore requires
re-keying the scene by the existing scope-aware
`NodeRef { scope_path, node_id }` (`node_network.rs`), which is already
`Hash + Eq` and already keys `node_errors` / `node_output_strings` inside
`NodeSceneData`.

Touched key spaces (all in the scene/refresh layer — this re-keying phase
touches nothing in the evaluator; the feature's only evaluator change is the
`ZoneInput` floor in §3):

| Site | Change |
|------|--------|
| `StructureDesignerScene.node_data` | `HashMap<NodeRef, NodeSceneData>` |
| `invisible_node_cache` (LRU) | keyed by `NodeRef`; `move_to_cache` / `restore_from_cache` / `invalidate_cached_nodes` / `update_cached_displayed_pins` take `NodeRef` |
| `StructureDesignerChanges.visibility_changed` | `HashSet<NodeRef>` |
| `refresh_full` / `refresh_partial` loops | iterate `(NodeRef, NodeDisplayType)` |
| `scene_tessellator.rs` `is_active` check | compare against `NodeRef::top(active_node_id)` |
| Active-node scene lookups (`get_selected_node_interactive_pin`, `common_api.rs` drawing-plane read, etc.) | `node_data.get(&NodeRef::top(active_id))` |
| Viewport hit-testing (`structure_designer.rs` pick paths) | iterate as today (`.values()` mostly); internal plumbing carries the hit `NodeRef`; the click-to-activate candidate set filters to top-level refs (see below) |

Most consumers iterate `.values()` and are agnostic to the key. The refactor
is mechanical and behavior-preserving on its own — before Phase 2 every key in
the map is `NodeRef::top(..)`.

Viewport picking consequence (decision): the click-to-activate surface on
the Flutter side — `viewport_pick`'s FRB result types, the disambiguation
overlay, `scrollToNode(nodeId)`, the solo-eye "hide others" action — is
keyed by **bare node id**, and id collisions are the load-bearing motivation
for this refactor: routing a body node's id through those surfaces would
scroll to / toggle the wrong (top-level) node. Extending all of that to
scope chains is real UI work this feature does not need. So: **body-node
scene entries are excluded from the click-to-activate candidate set** —
`viewport_pick` considers only top-level refs, and clicking displayed body
geometry falls through to the existing no-hit / active-node behavior.
Displayed body geometry is visible but not click-activatable. Full scoped
pick UX (pick results carrying scope chains, scoped disambiguation /
scroll-to-node / solo-hide) is deferred — see Non-Goals.

### 2. Collecting displayed refs (eligibility-gated)

A new helper on `StructureDesigner` (or free function beside it):

```rust
/// Walk the network and collect every displayed node as a scope-aware ref:
/// the top-level `displayed_nodes`, plus — recursively — the
/// `displayed_nodes` of every body reachable through an ELIGIBLE chain
/// (each ancestor is a `closure` node whose resolved custom node type has
/// zero zone-input pins). Ineligible bodies contribute nothing (their
/// stored flags are dormant).
fn collect_displayed_node_refs(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> Vec<(NodeRef, NodeDisplayType)>
```

Eligibility of one hop is checked with:

```rust
/// True iff `node` is a `closure` whose resolved custom node type declares
/// zero zone-input pins (ClosureKind::Custom with no params).
fn is_zero_ary_closure(node: &Node, registry: &NodeTypeRegistry) -> bool
```

Reading the **resolved** custom node type (not `ClosureData` directly) keeps
this in lock-step with what the body's wires may legally reference — the same
source of truth validation uses.

### 3. Scoped scene generation

`generate_scene` gets a scope-aware variant (the existing signature delegates
with an empty path):

```rust
pub fn generate_scene_scoped(
    &mut self,
    network_name: &str,
    node_ref: &NodeRef,
    ...
) -> NodeSceneData
```

Differences from the top-level path:

- **Stack construction:** walk `node_ref.scope_path` from the top-level
  network down through each closure node's `zone`, pushing a
  `NetworkStackElement { node_network: body, node_id: closure_id }` per hop.
  Evaluation of the target node then runs against this stack; capture wires
  (`depth >= 1`) resolve by the existing stack walk in
  `resolve_incoming_wire`. No evaluator changes beyond the defensive
  `ZoneInput` floor below.
- **Eval-scope keying:** push `context.push_eval_scope(closure_id)` per hop
  (mirroring `run_closure_once`) so `node_errors` / `node_output_strings`
  key under the correct `NodeRef`, and pop after. Hover values and error
  badges for body nodes then work through the existing scope-keyed lookups.
- **`from_selected_node`:** read from the body network's selection
  (`body.is_node_selected(node_id)`), consistent with scoped selection.
- **Displayed pins:** read from the body network's
  `get_displayed_pins(node_id)`.
- A missing/malformed chain (closure deleted, zone missing, chain no longer
  eligible) returns `NodeSceneData::new(NodeOutput::None)` — same convention
  as today's missing-node path.
- **Defensive `ZoneInput` floor (the one evaluator change):** a body node is
  evaluated here with **no zone frame pushed**, so a `ZoneInput` wire
  reaching `context.current_zone_input` would panic — a crash path that does
  not exist today (body nodes currently only evaluate inside
  `run_closure_once`, which always pushes the frame). An eligible chain
  "cannot" contain such wires, but that guarantee is *derived*: it rests on
  validation rule 3 having run and on `is_zero_ary_closure` reading a fresh
  `custom_node_type` cache — and both premises are known-fragile (refresh
  paths never validate; body-node type caches go stale across undo restores,
  see the closure⇄network-conversion pitfall). Phase 2 therefore converts
  the `ZoneInput` arm of `resolve_incoming_wire` to a fallible lookup
  (`try_current_zone_input`) that returns a localized
  `NetworkResult::Error("zone input referenced outside an invocation")` when
  the frame is missing, instead of panicking. Closure-invocation paths are
  unaffected (the frame is always pushed there); a `debug_assert` may remain
  to catch regressions on those paths.

Evaluation cost: a displayed body node is one more `generate_scene` call per
refresh, identical in kind to a top-level displayed node. The central skip
rule for Unit-returning nodes applies unchanged.

### 4. Refresh integration

**Full refresh** (`refresh_full`): replace the `network.displayed_nodes`
iteration with `collect_displayed_node_refs` — in **both** loops: the
input-cache-clearing loop at the top (resolve body nodes' `NodeData` via
`find_node_data_at_scope` so displayed body nodes get their input caches
cleared too) and the `generate_scene_scoped` loop. Selected-node unit-cell
tracking compares against `NodeRef::top(active_node_id)`.

**Partial refresh** (`refresh_partial`):

- Step 1/3 (visibility changes): `visibility_changed` is now
  `HashSet<NodeRef>`; cache moves/restores key by `NodeRef`. The "is this ref
  currently displayed?" check (which decides move-to-cache vs.
  restore-from-cache) must resolve the ref's **scope network** and read *its*
  `displayed_nodes` — today's code reads the top-level map by bare id.
  Restoring a body node from the invisible cache is only valid if its chain
  is still eligible — check before restoring, else drop the cache entry.
- Step 2/4 (data changes): `affected_by_data_changes` is already a
  `HashSet<NodeRef>`. The current code only intersects **top-level** refs with
  displayed nodes (body dirtiness is lifted to the enclosing HOF by the
  synthetic body→HOF edge). Extend the intersection to scoped refs: an
  affected `NodeRef` that appears in `collect_displayed_node_refs` output
  joins `nodes_needing_evaluation`.
- **Cache invalidation must cover scoped refs too** (this is exactly the
  "stale restore" failure mode Phase 0 exists to catch): Step 2 currently
  filters the affected set with `is_top_level()` before calling
  `invalidate_cached_nodes`. Drop that filter and pass the full scoped
  affected set — a **hidden** body node does not appear in
  `collect_displayed_node_refs` output, but its invisible-cache entry must
  still be invalidated when a captured upstream source changes, or
  hide → edit the captured `float` → show restores stale geometry. (The
  `clear_input_cache` walk over the affected set is already scope-aware via
  `find_node_data_at_scope` — keep it that way.)
- **Ancestor→body propagation already exists:** `walk_scope_reverse_deps`
  (`node_network.rs`) inserts a *source → destination* edge for every wire
  in a body node's `arguments`, **including capture wires** (`depth >= 1`
  resolves to the ancestor scope via `resolve_wire_source`). So the flow-in
  edge *ancestor source node → body node* — what makes "edit the captured
  `float`, displayed body node re-renders" work under partial refresh — is
  present today. Phase 2 only needs the capture-liveness regression test
  below; no new edges, no fallback needed.
- Structural changes (node/wire add/delete, closure deletion, arity change
  via `set_closure_data`) already trigger revalidation + refresh paths that
  end in full re-evaluation of affected nodes; the eligibility gate in
  `collect_displayed_node_refs` is what makes newly-ineligible scene entries
  disappear. `refresh_full` starts from a fresh `StructureDesignerScene`, so
  stale scoped entries cannot linger there; for partial refresh, any change
  to a closure node's data (arity!) must mark its body subtree's scene
  entries for removal — simplest correct rule: when `data_changed` contains a
  `closure` node at ref `{scope_path: p, node_id: c}`, drop scene entries
  (live + cached) whose `scope_path` starts with the prefix `p ++ [c]` —
  i.e. the closure's whole body subtree.

### 5. Display toggles + undo

`set_node_display_scoped` already flips the body network's flag for non-empty
paths but currently skips undo ("deferred to U4") and change tracking. It is
brought to parity with the top-level path:

- Insert `NodeRef::scoped(scope_path, node_id)` into `visibility_changed`.
- Push a `SetNodeDisplayCommand` extended with a `scope_path: Vec<u64>` field;
  its `undo`/`redo` resolve the network through
  `get_scope_network_mut(scope_path)`. Same for a new
  `toggle_output_pin_display_scoped(scope_path, node_id, pin_index)` and an
  extended `SetOutputPinDisplayCommand`.
- The cached-`displayed_pins` fast-path update in `toggle_output_pin_display`
  keys by `NodeRef`.
- API layer: `set_node_display` already takes `scope_path`;
  `toggle_output_pin_display` gains it (per the scope-aware API rule in
  `rust/AGENTS.md`). FRB regen required.
- Toggling display in an **ineligible** scope is permitted and simply stores
  a dormant flag (the UI never offers it; API robustness). No error.

**Phasing note:** the node-level `SetNodeDisplayCommand` extension lands in
**Phase 2**, together with the change tracking — not in Phase 3. Two
reasons: the project rule that persisted mutations must be undoable (the
flag serializes into `.cnnd` and marks the design dirty), and LIFO
consistency — body state is also restored wholesale by whole-body snapshot
commands (`EditZoneBodyCommand` and friends), which is only correct if
*every* intervening body mutation is itself a command. A non-undoable
display toggle interleaved between snapshot commands would be silently
reverted by undoing an unrelated body edit, desyncing the eye icon from the
scene. Phase 3 then adds the per-pin toggle command and the API surface.

Undo's existing full-refresh path (`apply_node_display_policy` +
`validate_active_network` + full refresh) needs no change — display policy
never touches body networks, and the refresh collection re-derives
everything.

### 6. Flutter UI

- **Eligibility flag on the view:** `build_node_view`'s zone recursion
  computes chain eligibility top-down (parent eligible && node is a 0-ary
  closure) and surfaces it as `ZoneView.body_scene_evaluable: bool`. Flutter
  never re-derives arity rules; Rust is the single source of truth.
- **Eye rendering:** `node_widget.dart`'s output-pin row currently hides the
  eye for any `scopeChain.isNotEmpty` node. New condition: show the eye area
  when the node's containing body chain is `body_scene_evaluable` (threaded
  down the widget tree / scope resolver alongside the existing
  collapsed/f-overridden body flags). Unit-typed pins keep their existing
  no-eye rule.
- **Toggle wiring:** the eye and per-pin eye handlers pass the node's scope
  chain to `set_node_display` / `toggle_output_pin_display`.
- Hover values and error badges for body nodes already work (scope-keyed
  `node_output_strings` / `node_errors`); displayed body nodes populate them
  through `generate_scene_scoped`'s eval-scope pushes.
- **Collapse interaction (decision):** a collapsed body's nodes keep
  rendering in the 3D viewport if displayed — collapse is a network-canvas
  concern, not a scene concern (same as the top level, where the viewport
  shows displayed nodes regardless of canvas viewport). The eyes are simply
  not visible while collapsed because the body content isn't rendered.

### 7. Edge cases

- **Id collisions:** a body node and a top-level node with the same id can
  both be displayed; `NodeRef` keying keeps their scene entries, caches, and
  change tracking distinct. This is the load-bearing reason for Phase 1.
- **Closure deleted / body converted to network / inlined:** the body ceases
  to exist; refresh re-derives the displayed set, and the closure-node
  `data_changed`/structural rules drop stale scoped entries. On conversion to
  a custom network, whatever the existing conversion serialization does with
  the body's `displayed_nodes` (carry into the new network's top level, or
  drop) is acceptable — a custom network's *instances* are not
  scene-addressable per instance, so no correctness issue either way; do not
  add special handling.
- **Copy/paste:** top-level copy already clears `displayed_nodes` on the
  clipboard; display flags inside a copied closure's body travel with the
  body serialization. Pasted-body flags are dormant-or-active per the pasted
  chain's eligibility — no special handling.
- **Effect nodes in a body:** Unit-only outputs are skipped by the central
  skip rule; displaying them renders nothing, same as top level.
- **Iterator-typed pins in a body:** same as top level — no viewport output;
  hover shows the type; `collect` inside the body to preview.
- **`closure` node's own eye:** unchanged — the closure node itself is a
  top-level (or body) node whose pin-0 `Function` value displays as before.

## Implementation plan

Phases 0–4. Each new test file must be registered in
`rust/tests/structure_designer.rs` per the testing convention in
`rust/AGENTS.md`.

### Phase 0 — Refresh-pipeline characterization tests (pre-refactor)

The paths Phase 1 rewires are among the least-tested in the codebase: only a
handful of test files drive `StructureDesigner::refresh` at all, and nothing
covers the invisible-cache lifecycle end-to-end (`move_to_cache` /
`restore_from_cache` / `invalidate_cached_nodes` /
`update_cached_displayed_pins`). "Existing tests stay green" is therefore a
weak gate exactly where the re-keying refactor is riskiest. This phase pins
down today's behavior **before any refactoring**, against the current `u64`
API.

The re-keying refactor has two *silent* failure modes these tests exist to
catch:

1. **Silent cache miss** — inconsistent key construction makes
   `restore_from_cache` quietly fail; the node re-evaluates, output stays
   correct, no test fails — but the fast visibility-toggle path is dead.
2. **Stale restore** — a missed `invalidate_cached_nodes` entry means
   hide → edit upstream → show renders **stale geometry**.

To assert "did it re-evaluate?", wire a `print` node (`execute_only: false`,
so it fires on every evaluation) into the probed chain and count
`take_print_log()` entries — no test-only hooks needed.

**Tests (`rust/tests/structure_designer/refresh_pipeline_test.rs`),** driven
through `StructureDesigner::refresh` with the designer's pending changes (the
same entry point the API layer uses), *not* through direct evaluator calls:

- *Hide → cache → show → restore:* a displayed node with viewport output;
  toggle display off (scene entry leaves `node_data`, cached-node count
  rises); toggle back on → entry restored **without re-evaluation** (print
  counter unchanged) and with identical output.
- *Invalidation:* hide a node, change an upstream node's stored data through
  the normal mutation path, show again → re-evaluated (print counter rises)
  and the output reflects the new upstream value.
- *Pin display while hidden:* hide a multi-output node, toggle one of its
  pins, show → the restored entry's `displayed_pins` is correct
  (`update_cached_displayed_pins` path).
- *Selection change:* select/deselect a displayed node → previous + current
  selection re-evaluate (Step 4.5) and `from_selected_node` flips.
- *Active-node lookups:* `get_selected_node_interactive_pin` and
  selected-node unit-cell tracking return the same values before and after a
  partial refresh.

During Phase 1 these tests are updated **mechanically — key type only**. Any
other edit needed to keep them green is a red flag that behavior drifted.

### Phase 1 — Scene re-keying by `NodeRef` (behavior-preserving refactor)

Re-key `StructureDesignerScene.node_data`, the invisible LRU cache, and
`StructureDesignerChanges.visibility_changed` by `NodeRef`; update
`refresh_full` / `refresh_partial`, `scene_tessellator`, active-node scene
lookups, and the pick paths' returned ids (`NodeRef` internally; API surface
unchanged — every ref is top-level at this point, so `.node_id` is a lossless
projection at the API boundary). `generate_scene` callers pass
`NodeRef::top(id)`.

No user-visible change. The acceptance gate is the full existing suite plus
Phase 0's characterization tests, all green with at most mechanical
(key-type-only) edits.

Refactor discipline (what keeps this safe):

- **No `u64` convenience overloads** and no `impl From<u64> for NodeRef`
  shims: the key-type change must be a compile error at every call site so
  the compiler enumerates them. After that, the entire review surface is the
  set of sites that *construct* a `NodeRef` — a wrong `scope_path` is the one
  mistake the compiler cannot catch.
- `raytrace_per_node_test.rs` injects `node_data` by key and asserts hit node
  ids; it is the existing coverage for the pick paths. Update it mechanically.
- The `scene_tessellator.rs` `is_active` comparison is a one-line change in
  the GPU-adjacent display crate; per project convention it is verified
  manually (the existing `rust/tests/display/` tests catch compile-level
  drift).

**Tests (`rust/tests/structure_designer/`):**

- `scene_noderef_keying_test.rs` — container-level tests of the re-keyed
  scene (production code creates only `NodeRef::top` refs until Phase 2;
  these tests exercise scoped refs directly on the data structures):
  - Scene insert/lookup with two `NodeSceneData` entries whose refs share
    `node_id` but differ in `scope_path` — both retrievable, no clobbering.
  - `move_to_cache` / `restore_from_cache` round-trip with a scoped ref;
    restoring one of two colliding-id entries restores the right one.
  - `invalidate_cached_nodes` with a scoped ref leaves the colliding
    top-level entry's cache intact.
- Phase 0's `refresh_pipeline_test.rs` green after mechanical key-type
  updates only.
- Existing suite (`cargo test`) green, including `node_snapshots` and
  `cnnd_roundtrip` (serialization is untouched by this phase — assert that by
  running them, not by inspection).

### Phase 2 — Backend: eligibility, scoped scene generation, refresh

`is_zero_ary_closure`, `collect_displayed_node_refs`,
`generate_scene_scoped` (stack construction + eval-scope pushes + the
defensive `ZoneInput` floor from Design §3), `refresh_full` /
`refresh_partial` integration including the scoped-ref cache invalidation
(drop the `is_top_level` filter feeding `invalidate_cached_nodes`) and the
capture-liveness regression test against the existing flow-in edges in
`build_scope_reverse_dependency_map`, closure-`data_changed` scoped-entry
eviction, and eligibility-checked cache restoration. `viewport_pick` filters
its click-to-activate candidate set to top-level refs (Design §1).
`set_node_display_scoped` feeds `visibility_changed` **and** pushes the
scope-extended `SetNodeDisplayCommand` (see the §5 phasing note; per-pin
toggles and the API surface remain Phase 3).

**Tests (`rust/tests/structure_designer/`), `zero_ary_closure_display_test.rs`:**

- *Eligibility:* 0-ary `Custom` closure at top level → eligible; 0-ary inside
  0-ary → eligible; 0-ary inside a `map` body → not eligible; preset-kind
  (`Map`) closure → not eligible; `Custom` with one param → not eligible.
- *Evaluation:* body node (`sphere` fed by a captured top-level `float`
  radius) displayed via `set_node_display_scoped`; full refresh produces a
  scene entry keyed `NodeRef::scoped([closure_id], node_id)` with the
  expected `NodeOutput`; capture value matches the top-level source's value.
- *Capture liveness:* change the captured `float`'s stored value; partial
  refresh re-evaluates the displayed body node (asserts the
  ancestor→body dependency edge works, not just full refresh).
- *Collision:* top-level node and body node with the same numeric id both
  displayed; both scene entries present and carry their own outputs.
- *Dormancy:* with a displayed body node, `set_closure_data` to arity 1 →
  refresh drops the scoped scene entry (live and cached); back to arity 0 →
  entry reappears without re-toggling (flag survived).
- *Nested dormancy:* displayed node in an inner 0-ary closure whose *outer*
  closure gains a param → inner entry also drops.
- *Cache eviction on closure edit:* display a body node, hide it (entry moves
  to the invisible cache), change the closure's data (arity 0 → 1) → the
  cached subtree entry is evicted; revert to arity 0 and show → the node is
  re-evaluated fresh (no stale restore; assert via the Phase 0 print-counter
  technique), output correct.
- *Stale restore guard (hidden body node):* display a body node, hide it
  (entry moves to the invisible cache), change the captured source's stored
  value, show again → re-evaluated (print counter rises) and the output
  reflects the new value — no stale restore.
- *`ZoneInput` panic floor:* a body whose enclosing closure claims zero
  params but which still holds a `ZoneInput` wire (constructed directly —
  the desync state validation would normally block) → scene evaluation of
  the affected node yields a localized `NetworkResult::Error`, no panic.
- *Display-toggle undo (node level):* toggle a body node's display on →
  undo → flag cleared in the body network and scene entry gone; redo →
  restored. Toggling to the current state pushes no command. The undo does
  not disturb a colliding-id top-level node's display state.
- *Errors/hover:* a body node with an eval error surfaces it under the
  scoped `NodeRef` via `get_node_error` / `get_node_output_strings`.
- *Deletion:* deleting the closure node removes the scoped scene entries.

### Phase 3 — Pin-toggle undo + API surface + persistence

(The node-level `SetNodeDisplayCommand` extension landed in Phase 2 — see
the §5 phasing note.) `SetOutputPinDisplayCommand` gains `scope_path`; new
`toggle_output_pin_display_scoped`; API `toggle_output_pin_display` gains
`scope_path` (FRB regen). Ineligible-scope toggles store dormant flags
without error.

**Tests:**

- `zero_ary_closure_display_undo_test.rs`
  (`rust/tests/structure_designer/`):
  - Per-pin toggle on a multi-output body node: toggle → undo → redo
    (displayed_pins set round-trips). Node-level toggle undo (including
    colliding-id isolation and the no-op rule) is covered by the Phase 2
    test.
  - Pin-toggle no-op: toggling a pin to its current state pushes no command
    (parity with top-level behavior).
- Serialization: extend the roundtrip coverage (`cnnd_roundtrip` style) with
  a `.cnnd` containing a closure whose body has `displayed_nodes` entries —
  save → load → flags preserved (dormant and active alike).

### Phase 4 — Flutter UI + reference guide

`ZoneView.body_scene_evaluable` (computed in `build_node_view` recursion, FRB
regen), eye-area condition in `node_widget.dart`, scoped toggle handlers,
model plumbing (`refreshFromKernel` → `notifyListeners`). Reference-guide
update.

**Tests / verification:**

- Rust: a backend test (new case in `zero_ary_closure_display_test.rs`)
  asserting `build_node_view` sets `body_scene_evaluable` for an eligible
  chain and clears it after adding a param (view-building is backend logic;
  test it in Rust, not in a widget test).
- Flutter: `flutter analyze` clean (no new warnings over the baseline);
  manual walkthrough per the project's thin-editor-UI convention —
  toggle eyes inside a 0-ary closure, see geometry appear; add a parameter
  in the ClosureShapeEditor, eyes vanish and geometry disappears; remove
  the parameter, both return; undo/redo across the sequence; verify a
  collapsed body still renders its displayed geometry.
- Reference guide: update `doc/reference_guide/node_networks.md` (closures
  section) — bodies of parameter-less closures have working visibility
  toggles; adding a parameter hides them (and why); state is remembered.
