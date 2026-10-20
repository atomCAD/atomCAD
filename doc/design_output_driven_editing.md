# Design: Output-Driven Editing — Trace a Viewport Hit Back to Its Nodes

## Problem

A design of any size is a long chain of nodes: shapes combined by booleans,
materialized into a crystal, edited, moved, relaxed. When the user looks at the
result and wants to change *this* facet or *this* atom, they have to work out
which of dozens of upstream nodes is responsible — by reading the network, or
by toggling visibility until something changes.

The existing click-to-activate (`doc/design_click_to_activate_node.md`) answers
only "which **displayed** node drew this". The node that actually needs editing
is almost always further upstream and not displayed.

## Goal

Right-click on content in the viewport opens a menu listing the nodes that
contributed to the thing under the cursor, nearest-to-the-viewport first, each
labelled with what it did ("filled", "moved", "shape", …). Choosing a row
selects that node and scrolls the node network editor to it.

This is the first and simplest **output-driven editing** feature: the user
starts from the result and lands on the node, keeping the full power of the
network behind it. Later features (edit gadgets from the viewport, write-back
to wired parameters, viewport tools that insert nodes) build on the same trace;
they are out of scope here.

### Example

`union(sphere, intersect(half_space ×4))` → `materialize`. The user right-clicks
an atom on a face of the tetrahedron:

```
materialize      filled
union            passed
intersect        passed
half_space_3     shape
```

The `sphere` and the other three half spaces are **pruned**: the atom does not
come from them, so they are not listed. Clicking an atom on the sphere's surface
instead lists `materialize`, `union`, `sphere`, and the whole `intersect` branch
is pruned.

## Design overview

The trace is computed **on demand**, at right-click time. Nothing is recorded
during normal evaluation.

- Each node may implement one optional trait method, `NodeData::trace`, which
  answers a *local* question: "this thing is in my output — where did it come
  from in my inputs, and what did I do to it?"
- A generic **trace engine** walks the network upstream, calling `trace` node
  by node. It owns everything non-local: wires, array pins, custom-node
  instances, deduplication, and the fallback for nodes that do not implement
  `trace`.
- Values the nodes need (their inputs, their own output) come from the ordinary
  evaluator, run with the eval memo installed for the duration of one trace, so
  each upstream node is evaluated at most once per right-click.

### Why not record provenance during evaluation

Carrying a source-node id on every atom and every `GeoNode` was considered and
rejected:

- `Atom` is held at 64 bytes by a size test; a per-atom field costs memory on
  million-atom structures, and every id-renumbering site (`add_atomic_structure`,
  `apply_diff`, `fill_lattice`) would have to carry it.
- A source tag on `GeoNode` either enters the content hash (identical shapes
  stop deduplicating in the CSG cache) or does not (a cache hit can return the
  wrong tag).
- It records only "who created this", not "who moved / edited / cut it".
- It taxes every evaluation for a question asked on a right-click.

## The trait

All trace types live in a new module `atomcad-structure-designer/src/trace/`.

```rust
/// What is being traced: one thing visible in a node's output pin.
pub enum TraceProbe {
    /// An atom. `id` is valid in the output being traced *only*; `position`
    /// is carried so the engine can re-sync by position (see Fallback).
    Atom { id: u32, position: DVec3 },
    /// A point of geometry. `on_surface` is true for a ray hit on a displayed
    /// surface, false for an atom centre handed down by `materialize`.
    Point { position: DVec3, on_surface: bool },
}

/// What the node did to the probed thing. Shown as the row's label.
pub enum Contribution {
    /// The node is where the thing comes from: "filled", "added", "shape",
    /// "imported", "passivated".
    Created(&'static str),
    /// The node changed it: "moved", "edited", "relaxed", "replaced".
    Modified(&'static str),
    /// The thing went through unchanged; the node only routed it
    /// (union, booleans, an atom_edit that did not touch this atom).
    Passed,
    /// Set by the engine for a node without a `trace` implementation.
    /// Never returned by a node.
    Unknown,
}

pub struct TraceStep {
    pub contribution: Contribution,
    /// Where the thing came from. Empty = this node is the origin.
    pub upstream: Vec<TraceLink>,
}

pub struct TraceLink {
    /// Input pin index.
    pub pin: usize,
    /// Index into the pin's merged array value, for array pins. The engine
    /// maps it to the wire that delivered that element.
    pub element: Option<usize>,
    /// The probe rewritten into that input's terms: the input's atom id, an
    /// inverse-transformed point.
    pub probe: TraceProbe,
}
```

On `NodeData`:

```rust
/// Output-driven editing (`doc/design_output_driven_editing.md`): `probe` is
/// something in this node's output pin `pin`. Say what this node did to it
/// and where in the inputs it came from. Read inputs and outputs through
/// `ctx`. `None` = not supported; the engine falls back to "possibly
/// contributed" and continues through every phase-typed input.
fn trace(
    &self,
    _pin: usize,
    _probe: &TraceProbe,
    _ctx: &mut TraceContext,
) -> Option<TraceStep> {
    None
}
```

`trace` may read the node's **persisted** data (`atom_edit`'s diff, a move's
stored translation) but never runtime caches. Subnetwork node data is shared
by every call site, so a runtime cache can hold another instance's state.

### `TraceContext`

The node's only window onto the evaluator:

```rust
impl TraceContext<'_> {
    /// The evaluated value on input pin `pin`, the same value `eval` sees.
    /// Array pins return the merged `NetworkResult::Array`.
    pub fn input(&mut self, pin: usize) -> NetworkResult;
    /// This node's own evaluated output on `pin`.
    pub fn output(&mut self, pin: usize) -> NetworkResult;
    /// δ for "does this child decide the surface here" ties
    /// (see Geometry booleans). One value for every node.
    pub fn surface_tolerance(&self) -> f64;
}
```

Internally it wraps `NetworkEvaluator`, the network stack, `node_id`, the
registry and a throwaway `NetworkEvaluationContext`, and calls `evaluate_arg`
and the node's own `eval` the way the evaluator does. The node never sees the
stack, wires or scopes.

## The engine

`StructureDesigner::trace_hit(hit) -> Vec<TraceRow>` walks upstream
depth-first from the hit.

**Starting the trace.** The hit comes from the per-node raycast (see Phase 2):
a displayed `NodeRef`, the pin, and either an atom (id, position) or a surface
point. The engine re-evaluates that node's pin. It does not trust ids in the
decorated display output, so it resolves an atom hit by position against the
real `NetworkResult`. A bond hit is traced as its endpoint nearer to the hit
point.

**Each step.**

1. Call `node.trace(pin, probe, ctx)`.
2. Emit a row: node, contribution, scope.
3. For each returned link, resolve `(pin, element)` to the incoming wire.
   - Array pins mirror `evaluate_arg`'s merge: wires sorted by source node id,
     each value converted to an array and concatenated.
   - The engine evaluates each wire's value, which is a memo hit, to find which
     wire element *i* falls in.
4. Continue at the source node's output pin with the link's probe.

If a wire delivered a whole array, the element-within-wire index is dropped.
The source node (`array`, `map`, …) is traced with the bare probe, where the
fallback's position re-sync (below) usually recovers it.

**Custom-node instances.** The engine enters the subnetwork at its return node
and keeps tracing. At a `parameter` node it steps back out to the instance's
matching input pin in the parent network, the same stack discipline `eval`
uses.

The engine emits a row for the instance itself, then the rows traced inside
it as that row's children. Rows therefore form a **tree**, and nested custom
nodes nest deeper. The instance row's contribution is the strongest one among
its children, in the order `Created` > `Modified` > `Passed` > `Unknown`, with
that child's label. "filled" on a `slab` instance says the atom was made
inside it. Each instance is its own subtree: a network instantiated twice on
the path appears twice.

**Opaque constructs.** HOFs, closures, `apply`, and wires crossing a zone
boundary (`source_scope_depth > 0`) are treated as nodes without `trace` in v1.

**Dedup and bounds.** A visited set keyed by (`NodeRef`, pin, probe kind,
rounded position) stops diamond-shaped DAGs from repeating. A row cap (e.g.
200) guards against pathological networks; hitting it adds a final
"… more" row.

**Row order.** Rows are listed in walk order: downstream first, branches
depth-first in input-pin order. That reads as a path from the viewport back
to the origin.

### Fallback for nodes without `trace`

`trace` returned `None`:

1. Emit the row with `Contribution::Unknown`, shown as "possibly contributed".
2. **No phase-typed inputs**: the node is an origin, labelled
   `Created("source")`. Leaves (`sphere`, `cuboid`, `half_space`, `import_xyz`,
   `import_cif`, …) therefore need no implementation. A node counts as having
   phase-typed inputs if one of its declared input pin types is `Blueprint`,
   `Crystal`, `Molecule`, `HasAtoms`, `HasStructure`, `HasFreeLinOps`, or an
   array of one of these.
3. **Otherwise**, continue into every phase-typed input with a re-synced probe:
   - **Atom**: look for an atom within 0.1 Å of the probe position in that
     input (`get_atoms_in_radius`). If found, the probe is precise again, with
     the input's atom id. Many unsupported nodes do not move atoms (`passivate`,
     `tag`, `freeze`, `apply_style`, `infer_bonds`, …), so the trace usually
     survives them unharmed.
   - **Point**: keep the point if the input's geometry contains it, or for an
     `on_surface` probe, if it is within δ of the input's surface.
   - **No match**: the probe is **lost**. Every node upstream of that input
     through phase-typed pins is listed as `Unknown` without calling its
     `trace`. With no probe, nothing can be pruned, so listing everything is
     the honest answer.

## Node implementations

### Movement nodes

`structure_move`, `structure_rot`, `free_move`, `free_rot`, `structure_invert`,
`geo_trans`:

- **Atom**: the id is preserved, so link to pin 0 with the same id. The
  position comes from the input atom.
- **Point**: link to pin 0 with the inverse-transformed point. To avoid
  duplication, factor the transform out of `eval` into a helper used by both
  `eval` and `trace`.
- Reports `Modified("moved")`.

### `relax`

Same id, link to pin 0. Reports `Modified("relaxed")`.

### `atom_union`

Element 0 is the output's base, so its ids are unchanged. Every later element
is appended through `add_atomic_structure`, which returns the old→new id map.
Replay the merge with `ctx.input(0)`'s elements to find which element produced
the probed id and its id there. Reports `Passed`.

### `atom_edit`

Re-run `apply_diff(base, &self.diff, tolerance)` on `ctx.input(0)` and read
`DiffProvenance.sources[id]`:

| `AtomSource` | Contribution | Link |
|---|---|---|
| `DiffAdded` | `Created("added")` | none |
| `DiffMatchedBase { base_id, .. }` | `Modified("edited")` | pin 0, `base_id` |
| `BasePassthrough(base_id)` | `Passed` | pin 0, `base_id` |

### `materialize`

- Reports `Created("filled")`, or `Created("passivated")` for an atom with the
  hydrogen-passivation flag.
- Links to pin 0 (the Blueprint) with
  `Point { position, on_surface: false }`. This is where the trace crosses from
  atoms to geometry.
- For a passivating hydrogen, use the position of the atom it caps. The H sits
  just outside the geometry; its parent sits inside, like every other filled
  atom.

### Geometry booleans

`union`, `intersect`, `diff` (3D):

> **Follow the children whose own SDF equals the combined SDF at the point,
> within δ.**

That is, follow the children whose surface decides the shape there:

- `union` keeps children within δ of the minimum.
- `intersect` keeps children within δ of the maximum.
- `diff` computes `max(base, −cutter_i)` and keeps whichever terms are within
  δ of the result.

All three report `Passed`. Ties are what make this work at edges and corners:
an atom on a tetrahedron face yields one half space, an edge atom two, a
vertex atom three. A deep-interior atom yields the nearest face.

The heuristic lives in the three nodes' `trace` methods, not in the engine.
Two shared helpers in `trace/` keep each implementation to a few lines:

```rust
/// Indices of `values` within `delta` of `combined`.
pub fn deciding_children(values: &[f64], combined: f64, delta: f64) -> Vec<usize>;
/// Signed distance of one geometry value at `point` (3D), via `implicit_eval_3d`.
pub fn sdf_at(value: &NetworkResult, point: DVec3) -> Option<f64>;
```

`ctx.surface_tolerance()` supplies δ. v1 uses a fixed 0.5 Å. Whether δ should
follow the lattice's nearest-neighbour distance is left to the Phase 4
walkthrough.

### Not in v1

`extrude` and the 2D booleans are left out. `extrude` falls back and, having no
phase-typed inputs, becomes an origin. The 2D booleans can later reuse the
helpers above.

Further candidates, in rough order of value: `atom_replace`, `atom_cut`,
`passivate`, `remove_hydrogen`, the `apply_diff` node, `supercell`,
`dematerialize`.

## Rust API

In `rust/src/api/structure_designer/structure_designer_api.rs`:

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn trace_viewport_hit(ray_origin: APIVec3, ray_direction: APIVec3) -> APITraceResult;
```

```rust
pub struct APITraceResult {
    /// Empty = nothing hit.
    pub rows: Vec<APITraceRow>,
}

pub struct APITraceRow {
    /// Same addressing as `APINetworkUsage`, so Dart can `jumpToNode`.
    pub host_network: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub node_label: String,
    /// Tree level: 0 = the top network; children of a custom-node instance
    /// row are one deeper. Rows are in pre-order, so a row's children follow
    /// it directly.
    pub depth: u32,
    pub contribution: APIContributionKind, // Created | Modified | Passed | Unknown
    /// "filled", "moved", "shape", "possibly contributed", …
    pub label: String,
}
```

If several displayed nodes are hit within click-to-activate's overlap epsilon,
v1 traces only the nearest, which is the same choice click-to-activate makes
for an unambiguous hit. Hits are restricted to top-level displayed nodes, as
in `viewport_pick`.

## Flutter

### Gesture

Right button down already starts an orbit (Shift: pan) in
`cad_viewport.dart:284`. That stays. A right **click** is recognized on pointer
up when the pointer moved less than a small slop (≈ 4 logical px) since
pointer down. The base class then calls a new overridable
`onSecondaryClick(Offset)`. The structure designer viewport overrides it:
build the ray (`getRayFromPointerPos`), call `trace_viewport_hit`, and show the
menu at the click position. An empty result shows no menu.

### Menu

A popup in the style of the click-to-activate disambiguation overlay:

- **Columns**: one row per trace row, showing the node label and, right-aligned,
  the contribution label. `Passed` and `Unknown` rows are dimmed.
- **Tree**: a custom-node instance is a parent row, and the nodes traced
  inside its network are its children, indented by `depth`. Each parent has an
  expand/collapse chevron and starts expanded, since trace lists are short.
- **Click a row**:
  `graphModel.jumpToNode(row.hostNetwork, row.scopePath, row.nodeId)`, the path
  Find Usages already uses. It selects the node and scrolls the editor.
  Because of the tree, the user chooses where to land:
  - **Click a parent (instance) row**: land on the instance in the network
    being viewed. The user stays in place, and the viewport keeps showing the
    design that was right-clicked.
  - **Click a child row**: land on that node inside the custom network. The
    active network switches, so the viewport then shows that network's
    display.
- **Dismissal**: clicking away or pressing Escape closes the menu with no change.

All rows are shown. Filtering by contribution, for example a dropdown in the
menu header, is a later UI-only change: the category is already on every row.

## Open questions

1. **δ.** Fixed 0.5 Å, or derived from the lattice? See Geometry booleans.
2. **Overlapping displayed nodes.** v1 traces the nearest only. Tracing each
   overlapping node in its own section is a possible follow-up.

## Performance

A trace re-evaluates the displayed node's upstream chain once, with the memo
installed, plus the `trace` calls themselves. For CSG and editing chains this
is about one refresh. Upstream `relax`, `chemisorb` or `mechanosynth` nodes
can make it take seconds. v1 accepts that; Phase 1 measures it on the demo
library. Only if that measurement shows a problem is caching considered, for
example keeping the last refresh's memo.

## Phased implementation plan

### Phase 1 — Engine, trait and fallback (Rust)

- `trace/` module with `TraceProbe`, `Contribution`, `TraceStep`, `TraceLink`,
  `TraceContext`, `deciding_children` and `sdf_at`.
- `NodeData::trace` with its default `None`.
- `StructureDesigner::trace_hit`, covering:
  - start-of-trace re-sync and bond-to-atom
  - the step loop
  - wire and array-element resolution
  - custom-node enter and exit through `parameter`
  - opaque HOFs and zone wires
  - dedup and the row cap
  - the fallback with position re-sync and the lost-probe listing
- Extend `PerNodeRayHit` (or add a sibling query) so the raycast also reports
  pin, hit kind, atom id and hit point. Today it reports only `node_ref` and
  distance.
- Measure trace time on the demo library networks and record the numbers in
  this doc.

Tests (`atomcad-structure-designer/tests/`), all with no node implementations
yet, so the fallback is exercised:

- A leaf with no phase inputs becomes an origin.
- An unsupported pass-through node (e.g. `tag`) keeps the probe precise by
  position re-sync.
- An unsupported node that moves atoms loses the probe, and everything
  upstream is listed `Unknown`.
- On a multi-wire array pin, element *i* resolves to the right wire.
- The trace enters a custom node, reaches its `parameter`, and comes back out
  to the instance's input. The instance row comes first, with the strongest
  contribution among its children. The inner rows follow it one level deeper,
  with the right `host_network` and `scope_path`. A nested custom node nests
  one level further.
- A diamond-shaped DAG lists each node once.

### Phase 2 — Viewport gesture, menu and API (Flutter + FRB)

- `trace_viewport_hit` and its API types; FRB codegen.
- The right-click slop in `cad_viewport.dart`, `onSecondaryClick`, and the menu
  in the structure designer viewport, wired to `jumpToNode`.
- Reference guide: `doc/reference_guide/ui.md`, viewport navigation. Document
  that right-drag still orbits, that a right click opens the contributing-nodes
  menu, and what the labels mean.

Tests: an API-level test that a ray through a known atom of a fixture network
returns the expected rows. The menu itself is verified manually
(walkthrough: right-drag still orbits; right click opens the menu; a row lands
on its node; a custom-node parent row lands on the instance in place, and its
child row lands inside the custom network; collapse/expand works). The Flutter smoke test is
a pending manual step for the maintainer.

### Phase 3 — Atomic nodes

`trace` for `structure_move`, `structure_rot`, `free_move`, `free_rot`,
`structure_invert`, `relax`, `atom_union`, `atom_edit` and `materialize`,
including the passivating-H rule. Factor out the movement transform helpers.

Tests, one per node, on small fixture networks:

- A moved atom traces through the move node with the right upstream id.
- `atom_union` element attribution, for an atom from element 0 and one from a
  later element.
- `atom_edit`: an added atom stops at the node; an edited atom continues to its
  base atom; an untouched atom is `Passed`.
- `materialize` hands a `Point` probe to the Blueprint. A passivating H is
  probed at its parent's position.

### Phase 4 — Geometry nodes

`trace` for `union`, `intersect`, `diff` and `geo_trans`, plus the inverse-point
path of the movement nodes on Blueprints.

Tests:

- The sphere ∪ cube example: an atom on the sphere surface lists `sphere` and
  prunes the cube.
- The tetrahedron example: face, edge and vertex atoms list one, two and three
  half spaces.
- `diff`: an atom on a cut face traces to the cutter; one on an uncut face
  traces to the base.
- A surface ray hit on a displayed Blueprint, as opposed to an atom, gives the
  same result.

Manual walkthrough, which also settles open question 1 (δ).

## Testing

Trace tests build small networks in Rust, issue `trace_hit` with a known probe,
and assert the row list: node names, contributions and order. Use the existing
network-building test helpers rather than new `.cnnd` fixtures where possible,
since each new fixture also updates two corpus snapshots. The UI is verified by
manual walkthrough. Tests for each phase are listed in that phase.

## Files to modify

| Area | Files |
|---|---|
| Trait | `rust/crates/atomcad-structure-designer/src/node_data.rs` |
| Engine | new `rust/crates/atomcad-structure-designer/src/trace/`, `structure_designer.rs` (`trace_hit`, raycast hit details) |
| Nodes | `nodes/{structure_move,structure_rot,free_move,free_rot,structure_invert,relax,atom_union,materialize,union,intersect,diff,geo_trans}.rs`, `nodes/atom_edit/atom_edit_data.rs` |
| API | `rust/src/api/structure_designer/structure_designer_api.rs`, `structure_designer_api_types.rs` |
| Flutter | `lib/common/cad_viewport.dart`, `lib/structure_designer/structure_designer_viewport.dart` |
| Docs | `doc/reference_guide/ui.md`; `rust/crates/atomcad-structure-designer/src/AGENTS.md` (a short "Output tracing" section: the trait, the fallback, "read only persisted data in `trace`") |
