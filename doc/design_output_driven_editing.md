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
contributed to the thing under the cursor, downstream first (the displayed
node at the top, the origin at the bottom), each labelled with what it did ("filled", "moved", "shape", …). Choosing a row
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
half_space_3     shape
intersect        passed
union            passed
materialize      filled
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
  each upstream node is evaluated at most once per right-click. The trace keeps
  every value it fetches in its own store for the rest of the trace, so each
  value is copied out of the memo at most twice (see `TraceContext`).

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
    /// An atom. A `Some` id is valid in the value being traced *only*.
    ///
    /// In a link a node returns, `id: None` means **"match by position"**:
    /// the engine looks the atom up by `position` in that input, and if
    /// nothing is there it drops the link silently. The thing did not come
    /// from that input, which is pruning, not loss. A node that knows the
    /// exact id (`atom_edit`) passes it; one that only knows the atom did not
    /// move (`atom_union`) passes `None` and lets the engine do the lookup.
    Atom { id: Option<u32>, position: DVec3 },
    /// A point of geometry. `on_surface` is true for a ray hit on a displayed
    /// surface, false for an atom centre handed down by `materialize`.
    ///
    /// `delta` is the tie tolerance δ that the geometry booleans use to decide
    /// which children decide the shape at this point (see Geometry booleans).
    /// The node that creates the `Point` sets it, and every later step passes
    /// it on unchanged (the movement nodes are rigid, so it holds in any
    /// frame).
    Point { position: DVec3, on_surface: bool, delta: f64 },
}

/// What the node did to the probed thing. Shown as the row's label.
pub enum Contribution {
    /// The node is where the thing comes from: "filled", "added", "shape",
    /// "source", "passivated".
    Created(&'static str),
    /// The node changed it: "moved", "edited", "relaxed", "replaced".
    Modified(&'static str),
    /// The thing went through unchanged; the node only routed it
    /// (union, booleans, an atom_edit that did not touch this atom).
    Passed,
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
    /// The probe rewritten into that input's terms: the input's atom id (or
    /// `None` to match by position), an inverse-transformed point.
    pub probe: TraceProbe,
}
```

`Contribution` holds only what a node can say. The engine's extra "possibly
contributed" state lives in the row type (`RowContribution`, see The engine),
so a node cannot return it.

On `NodeData`:

```rust
/// Output-driven editing (`doc/design_output_driven_editing.md`): `probe` is
/// something in this node's output pin `pin` (in element `element` of it,
/// when the pin's value is an array). Say what this node did to it and where
/// in the inputs it came from. Read inputs and outputs through `ctx`.
///
/// `None` = "I cannot answer this": the node does not trace at all, or it
/// does not handle this probe kind on this pin (a `Point` reaching
/// `atom_edit` from a click on a Crystal's geometry shell, any probe on a
/// `diff` output pin). Either way the engine falls back to "possibly
/// contributed" and continues through every phase-typed input.
fn trace(
    &self,
    _pin: usize,
    _element: Option<usize>,
    _probe: &TraceProbe,
    _ctx: &TraceContext,
) -> Option<TraceStep> {
    None
}
```

The output-side `element` mirrors `TraceLink::element` on the input side. With
it, array plumbing nodes (`array`, `array_at`, `array_concat`) can implement
`trace` trivially, and an index is never dropped at a wire that carries a
whole array.

`trace` may read the node's **persisted** data (`atom_edit`'s diff, a move's
stored translation) but never runtime caches. Subnetwork node data is shared
by every call site, so a runtime cache can hold another instance's state.

### `TraceContext`

The node's only window onto the evaluator:

```rust
impl TraceContext<'_> {
    /// The evaluated value on input pin `pin`, the same value `eval` sees.
    /// Array pins return the merged `NetworkResult::Array`.
    pub fn input(&self, pin: usize) -> Rc<NetworkResult>;
    /// This node's own evaluated output on `pin`.
    pub fn output(&self, pin: usize) -> Rc<NetworkResult>;
}
```

Internally it wraps `NetworkEvaluator`, the network stack, `node_id`, the
registry and a throwaway `NetworkEvaluationContext`, and calls `evaluate_arg`
and the node's own `eval` the way the evaluator does. The node never sees the
stack, wires or scopes.

**Shared, not copied.** The methods take `&self` and return `Rc`s, so a node
can hold several values at once — the booleans hold their children and their
combined output together, `atom_edit` holds its base while it reads its
output. (Returning `&NetworkResult` from `&mut self` would forbid that.)

The values come from a **trace-lifetime store**, shared by the engine and
every `TraceContext` of the trace. It lives in a `RefCell` and holds two kinds
of entry, each filled on first use:

- **Output entries**, keyed by (frame, network, node, output pin): a node's
  evaluated output (the memo's `lookup_pin`, or `eval` on a miss).
  `ctx.output` reads them, and so does the engine when it resolves wires (step
  3 of Each step). When the trace moves on to a source node, its
  `ctx.output` is the entry the engine already filled.
- **Input entries**, keyed by (frame, network, node, input pin): the value of
  `evaluate_arg`, which applies the pin's conversion and merges array pins.
  `ctx.input` reads them. They are separate from the sources' output entries
  because `evaluate_arg` builds a new value; re-implementing its conversion to
  share entries is exactly what this design avoids.

The store is needed because the memo does not lend either: `eval_memo::lookup`
/ `lookup_pin` return a clone, and `CrystalData` and `MoleculeData` hold a
plain `AtomicStructure`, so every memo hit deep-copies the atoms. With the
store, a value is copied **at most twice per trace**, once as its consumer's
input and once as its own node's output, however many times the engine and the
nodes ask for it. The store is dropped when the trace ends; nothing is cached
across traces. "Frame" is the `TraceFrame`, see The engine.

The memo is installed for the trace **without a byte budget**: an eviction in
the middle of a trace would re-run whatever produced the evicted value, which
could be a `relax`. `StructureDesigner::with_eval_context` cannot be used as
is: it installs the memo with the user's preference budget, installs nothing
when the memo is switched off, and stores the pass's counts in
`last_memo_counts`, which the profiling UI shows as the last refresh's. The
trace gets a sibling, `with_trace_eval_context`, that shares its setup but
always installs `eval_memo::install(Some(usize::MAX))` (`None` means "memo
off"), runs with `execute = false`, and returns the counts from
`eval_memo::take()` to the caller for `TraceStats` instead of writing
`last_memo_counts`.

A trace walks one upstream cone, so the peak is that cone's values, held once
in the memo and up to twice in the store. Phase 1 measures it.

## The engine

The engine lives in `trace/engine.rs`. It walks upstream depth-first from the
hit and returns a tree:

```rust
pub enum RowContribution {
    Traced(Contribution),
    /// The node has no `trace`, or the probe was lost upstream of it:
    /// "possibly contributed". Only the engine creates this.
    Unknown,
}

/// Where a node sits during the trace: the chain of custom-node instances
/// entered from the top network, outermost first. Empty = the top network.
/// Each entry is (the network that holds the instance, the instance node).
pub type TraceFrame = Vec<(String, NodeRef)>;

pub struct TraceRow {
    pub frame: TraceFrame,
    pub node: NodeRef,
    pub network: String,
    pub contribution: RowContribution,
}

pub struct TraceNode {
    pub row: TraceRow,
    /// Rows traced inside a custom-node instance. Empty for any other node.
    pub children: Vec<TraceNode>,
}
```

The tree has one kind of nesting only: **custom-node instances**. It is not a
data-flow tree. Within one network, rows are a flat list in walk order (see
Row order), and a branch point such as `union` does not get its branches as
children.

```rust
pub struct TraceReport {
    /// The top network's rows in walk order, each with its instance subtree.
    pub rows: Vec<TraceNode>,
    pub stats: TraceStats,
}

/// What the trace cost. Read by the Phase 1 measurement and by tests; the API
/// drops it.
pub struct TraceStats {
    /// Entries filled in the trace store (see `TraceContext`): one per
    /// distinct input or output entry the trace touched.
    pub store_fills: usize,
    /// The memo's own counts for the trace (`eval_memo::take`).
    pub memo: MemoCounts,
}
```

`StructureDesigner::trace_hit(hit) -> TraceReport` only delegates to the
engine. `structure_designer.rs` is already over 11 000 lines, so it should not
grow the logic.

**Starting the trace.** The hit comes from the per-node raycast, extended in
Phase 1 to report hit details:
a displayed `NodeRef`, the pin, and either an atom (id, position) or a surface
point. The engine re-evaluates that node's pin. It does not trust ids in the
decorated display output, so it resolves an atom hit by position against the
real `NetworkResult`. A bond hit is traced as its endpoint nearer to the hit
point. A surface hit starts as `Point { on_surface: true, delta: 0.1 Å }`
(see Geometry booleans).

**Each step.**

1. Call `node.trace(pin, element, probe, ctx)`. If it returns `None`, apply
   the Fallback (below) instead of steps 2–4.
2. Emit a row: frame, network, node, contribution.
3. For each returned link, resolve `(pin, element)` to the incoming wire.
   - Array pins mirror `evaluate_arg`'s merge: wires sorted by source node id,
     each value converted to an array and concatenated.
   - The engine reads each wire's source value from the store's output
     entries to find which wire element *i* falls in.
   - If that wire delivered a whole array, the element's index *within* that
     array becomes the source's output `element`. If it delivered one value,
     the source's `element` is `None`.
4. Resolve a match-by-position atom probe (`id: None`) against the source's
   output (that element of it, for an array): an atom within 0.1 Å gives the
   id. If there is none, drop the link silently.
5. Continue at the source node's output pin with the link's probe and element.
   A source that is a custom-node instance or a `parameter` node is handled as
   below.

**Custom-node instances.** Custom networks are dispatched by the evaluator
itself (`network_evaluator.rs`, the `registry.node_networks` branch), not
through `NodeData`, so entering one is an evaluator rule. The engine enters
the subnetwork at its return node and keeps tracing. At a `parameter` node it
steps back out to the instance's matching input pin in the parent network.

Both moves use the evaluator's own frame helpers, which are made crate-visible
for this, rather than re-implementing the stack discipline in `trace/`. When
the evaluator's rules change, the trace changes with them.

The engine emits a row for the instance itself, then the rows traced inside
it as that row's children. Rows therefore form a **tree**, and nested custom
nodes nest deeper. The instance row's contribution is the strongest one among
its children, in the order `Created` > `Modified` > `Passed` > `Unknown`, with
that child's label. "filled" on a `slab` instance says the atom was made
inside it. Each instance is its own subtree: a network instantiated twice on
the path appears twice.

**Opaque constructs.** HOFs, closures, `apply`, and wires crossing a zone
boundary (`source_scope_depth > 0`) are treated as nodes without `trace` in v1.

**Dedup and bounds.** A visited set keyed by (`TraceFrame`, network,
`NodeRef`, pin, element, probe kind, rounded position) stops diamond-shaped
DAGs from repeating. `NodeRef` alone is not enough: its `scope_path` is the
*zone* path inside HOF bodies and says nothing about the network, and node ids
restart in every network, so node 5 of a custom network and node 5 of its
parent would share a key. The frame keeps two instances of one network apart,
which is what lets each instance be its own subtree (above), while a diamond
*inside* one instance still collapses. A row cap (e.g. 200) guards against
pathological networks; hitting it adds a final "… more" row.

**Row order.** Rows are listed in walk order: downstream first, branches
depth-first in input-pin order. That reads as a path from the viewport back
to the origin. The menu shows the reverse (see Flutter → Menu).

### Fallback for nodes without `trace`

`trace` returned `None`:

1. **No phase-typed inputs**: the node is an origin, and the trace stops. The
   label comes from the type of the output pin being traced:
   - Blueprint: `Created("shape")`, e.g. `sphere`, `cuboid`, `half_space`.
   - Crystal or Molecule: `Created("source")`, e.g. `import_xyz`,
     `import_cif`.

   Leaves therefore need no implementation. A node counts as having
   phase-typed inputs if one of its declared input pin types is `Blueprint`,
   `Crystal`, `Molecule`, `HasAtoms`, `HasStructure`, `HasFreeLinOps`, or an
   array of one of these.
2. **Otherwise**, emit the row with `RowContribution::Unknown`, shown as
   "possibly contributed", and continue into every phase-typed input (every
   element of an array input) with a probe re-synced **to that input's type**.
   The probe kind need not match the input: an unsupported node can turn
   geometry into atoms or atoms into geometry, so the re-sync converts first.
   - **Atomic input** (`Crystal`, `Molecule`, or an abstract atomic type),
     either probe kind: look for an atom within 0.1 Å of the probe position
     (`get_atoms_in_radius`). If found, the probe becomes a precise `Atom` with
     the input's atom id. Many unsupported nodes do not move atoms
     (`passivate`, `tag`, `freeze`, `apply_style`, `infer_bonds`, …), so the
     trace usually survives them unharmed. If no atom is there and the probe is
     a `Point`, fall back to the input's geometry shell: keep the `Point` if
     `geo_tree_root` is `Some` and passes the Blueprint test below.
   - **Blueprint input**: an `Atom` probe becomes
     `Point { position, on_surface: false, delta }`, with `delta` from
     `atom_point_delta` (see `materialize`). The point is kept if the geometry
     contains it, or, for an `on_surface` probe, if it is within the probe's
     `delta` of the surface.
   - **No match**: the probe is **lost**. Every node upstream of that input
     through phase-typed pins is listed as `Unknown` without calling its
     `trace`. With no probe, nothing can be pruned, so listing everything is
     the honest answer.

   This is different from a node's match-by-position link, which is dropped
   when it finds nothing. A node that returns such a link knows the atom did
   not move, so "not there" means "not from here". A node without `trace`
   promises nothing, so "not there" can mean "moved".

## Node implementations

### Movement nodes

`structure_move`, `structure_rot`, `free_move`, `free_rot`, `structure_invert`,
and `geo_trans` (geometry only, so it handles `Point` probes only):

- **Atom**: the id is preserved, so link to pin 0 with `Some(id)`. The
  position comes from the input atom.
- **Point**: link to pin 0 with the inverse-transformed point; `on_surface`
  and `delta` pass through unchanged. To avoid duplication, factor the
  transform out of `eval` into a helper used by both `eval` and `trace`.
- Reports `Modified("moved")`.

The Atom path is Phase 3 and the Point path is Phase 4. Until a path exists,
`trace` returns `None` for that probe kind and the fallback runs.

### `relax`

Same id, link to pin 0. Reports `Modified("relaxed")`.

### `atom_union`

The union does not move atoms. Return one match-by-position link
(`id: None`) per element of pin 0. The engine keeps the element that has the
atom and drops the rest. Reports `Passed`.

The node does not replay the merge to recover exact ids. That would rebuild
the merged structure on every trace, and it would copy
`add_atomic_structure`'s id-assignment rule into a node. If exact ids are ever
needed, the helper belongs in `atomcad-crystolecule`, next to
`add_atomic_structure`, which defines that rule.

### `atom_edit`

Re-run `apply_diff(base, &self.diff, tolerance)` on `ctx.input(0)` and read
`DiffProvenance.sources[id]`:

| `AtomSource` | Contribution | Link |
|---|---|---|
| `DiffAdded` | `Created("added")` | none |
| `DiffMatchedBase { base_id, .. }` | `Modified("edited")` | pin 0, `Some(base_id)` |
| `BasePassthrough(base_id)` | `Passed` | pin 0, `Some(base_id)` |

### `materialize`

- Reports `Created("filled")`, or `Created("passivated")` for a passivating
  terminator: an H or halogen (#405) with exactly one bond (`is_terminator`).
  The test is structural because the `is_hydrogen_passivation` flag cannot
  answer it: lattice fill flags none of its terminators, and surface
  reconstruction flags the dimer *hosts* (`doc/design_halogen_passivation.md`
  D5). See Phase 3 as built.
- Links to pin 0 (the Blueprint) with
  `Point { position, on_surface: false, delta }`. This is where the trace
  crosses from atoms to geometry.
- For a passivating terminator, use the atom it caps (its one bonded
  neighbour) for both `position` and `delta`. The terminator sits just outside
  the geometry; its parent sits inside, like every other filled atom.
- `delta` comes from a shared helper in `trace/`, also used by the fallback:

  ```rust
  /// δ for a Point probe made from an atom centre: the length of the atom's
  /// shortest bond to an atom that is not a terminator; for an atom with no
  /// such bond, `crystal_bond_length(z, z)`, else 1.5 Å.
  pub fn atom_point_delta(atoms: &AtomicStructure, atom_id: u32) -> f64;
  ```

  Why a bond length: filled atom centres do not sit on the surface. Each one
  lies somewhere between 0 and about one layer spacing inside every face near
  it, and the layer spacing is bounded by the bond length. Two faces meeting at
  an edge can therefore be up to a bond length apart in signed distance at the
  edge's outermost atom, and a smaller δ would keep only one of them.

### Geometry booleans

`union`, `intersect`, `diff` (3D):

> **Follow the children whose own SDF equals the combined SDF at the point,
> within δ.**

That is, follow the children whose surface decides the shape there:

- `union` (one array pin, `shapes`) keeps the elements within δ of the
  minimum.
- `intersect` keeps the elements within δ of the maximum.
- `diff` has **two** array pins, `base` and `sub`, each unioned before the
  difference (`diff.rs`). With `B = min_j base_j` and `S = min_i sub_i`, the
  combined value is `max(B, −S)`. If `B` is within δ of it, follow the `base`
  elements within δ of `B` (pin 0, element *j*). If `−S` is within δ of it,
  follow the `sub` elements within δ of `S` (pin 1, element *i*). Both sides
  can be followed at an edge where a cut meets an uncut face.

Every link carries the element index, so the trace lands on the one shape
that decides the point, not on the whole pin. δ is the probe's own `delta`
(see `TraceProbe::Point`).

All three report `Passed`. Ties are what make this work at edges and corners:
an atom on a tetrahedron face, well away from its edges, yields one half
space; the outermost atom on an edge two; the atom at a vertex three. A
deep-interior atom yields the nearest face. Because δ is a bond length (see
`materialize`), a face atom within about one bond length of an edge also
yields both of that edge's half spaces. That is acceptable: the atom is
close to both faces, and editing either moves it.

The heuristic lives in the three nodes' `trace` methods, not in the engine.
Two shared helpers in `trace/` keep each implementation to a few lines:

```rust
/// Indices of `values` within `delta` of `combined`.
pub fn deciding_children(values: &[f64], combined: f64, delta: f64) -> Vec<usize>;
/// Signed distance of one geometry value at `point` (3D), via `implicit_eval_3d`.
pub fn sdf_at(value: &NetworkResult, point: DVec3) -> Option<f64>;
```

δ has two sources, set where the `Point` is created:

- **Atom centres** (`materialize`, and the fallback's atom → point
  conversion): `atom_point_delta`, a bond length.
- **Surface ray hits** on a displayed Blueprint: a fixed 0.1 Å. The point is
  on the tessellated surface, so the children that decide it agree to within
  the gap between the mesh and the SDF. δ only has to cover that gap.

### Not in v1

`extrude` and the 2D booleans are left out. `extrude` falls back and, having no
phase-typed inputs, becomes an origin. The 2D booleans can later reuse the
helpers above.

Further candidates, in rough order of value: `atom_replace`, `atom_cut`,
`passivate`, `remove_hydrogen`, the `apply_diff` node, `supercell`,
`dematerialize`. Most of them are taken up, with the "wired upstream" /
"trace stops here" row kinds, by `doc/design_trace_coverage.md`.

## Rust API

In `rust/src/api/structure_designer/structure_designer_api.rs`:

```rust
/// Step 1: which displayed node does the right-click trace from?
#[flutter_rust_bridge::frb(sync)]
pub fn trace_viewport_pick(ray_origin: APIVec3, ray_direction: APIVec3) -> APITracePick;

/// Step 2: trace the hit of this ray on the given displayed node.
#[flutter_rust_bridge::frb(sync)]
pub fn trace_viewport_hit(
    ray_origin: APIVec3,
    ray_direction: APIVec3,
    node_id: u64,
) -> APITraceResult;
```

```rust
pub enum APITracePick {
    NoHit,
    /// One displayed node was hit (or one is clearly nearest): trace it.
    Traced(APITraceResult),
    /// Several displayed nodes were hit within the overlap epsilon: the user
    /// picks one, then Dart calls `trace_viewport_hit` with its id.
    Disambiguation { candidates: Vec<APICandidateNode> },
}

pub struct APITraceResult {
    /// Empty = nothing hit.
    pub rows: Vec<APITraceRow>,
}

pub struct APITraceRow {
    /// Same addressing as `APINetworkUsage`, so Dart can `jumpToNode`.
    /// `host_network` is `TraceRow::network`; `scope_path` is the row's
    /// `NodeRef::scope_path`, always empty in v1 because the trace never
    /// enters a zone body (see Opaque constructs).
    pub host_network: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub node_label: String,
    /// Tree level: 0 = the top network; children of a custom-node instance
    /// row are one deeper. Rows are in pre-order, so a row's children follow
    /// it directly.
    pub depth: u32,
    pub contribution: APIContributionKind, // Created | Modified | Passed | Unknown
    /// The `Created` / `Modified` label ("filled", "moved", "shape", …),
    /// "passed" for `Passed`, "possibly contributed" for `Unknown`.
    pub label: String,
}
```

**Split between the layers.**
- **The engine's tree is domain data.** Flattening it into pre-order rows with
  a `depth` is a transport shape for Dart, so that conversion lives in
  `api/structure_designer/view_builders.rs`, next to the other projections of
  domain types.
- **Deciding which displayed nodes a ray hits is domain logic.** Today
  `OVERLAP_EPSILON` and the "hits within ε of the nearest" filter are written
  out twice in the API layer: in `query_hovered_atom_info` and in
  `viewport_pick`. Rather than a third copy, this work moves the rule down
  into `StructureDesigner::pick_displayed_candidates(ray, privilege_active)`.
  All three API functions call it and only map its result to
  `APICandidateNode` / `APITracePick`.

**Overlapping displayed nodes** are handled the way left-click handles them.
`trace_viewport_pick` uses the same overlap rule (hits within
`OVERLAP_EPSILON` of the nearest) and the same `APICandidateNode`. A single hit is
traced straight away. Overlapping hits return the candidates, and only the one
the user picks is traced, so a right-click never pays for tracing nodes the
user did not choose. There are two differences from left-click:

- **The active node is not privileged.** Left-click treats "the active node
  is among the overlapping hits" as the user having already chosen it. For a
  trace, the overlapping outputs are equally plausible, so the candidates are
  always offered.
- **Gadgets are ignored.** A right-click traces content and never hits a
  gadget handle.

Hits are restricted to top-level displayed nodes, as in `viewport_pick`.

## Flutter

### Gesture

Right button down already starts an orbit (Shift: pan) in
`cad_viewport.dart:284`. That stays. A right **click** is recognized on pointer
up when the pointer moved less than a small slop (≈ 4 logical px) since
pointer down. The base class then calls a new overridable
`onSecondaryClick(Offset)`. The structure designer viewport overrides it:
build the ray (`getRayFromPointerPos`) and call `trace_viewport_pick`.

- `NoHit` shows nothing.
- `Traced` shows the trace menu at the click position.
- `Disambiguation` first shows the existing disambiguation overlay
  (`_DisambiguationOverlay`) at the same position. Choosing a candidate calls
  `trace_viewport_hit(ray, candidate.nodeId)` and replaces the overlay with the
  trace menu.
  - The overlay gets a mode in which a row means "trace from this node". The
    Solo button is hidden there, because hiding nodes is not what the user
    asked for.
  - Clicking away dismisses it, as on left-click.

### Menu

A popup in the style of the click-to-activate disambiguation overlay:

- **Columns**: one row per trace row, showing the node label and, right-aligned,
  the contribution label. `Passed` and `Unknown` rows are dimmed.
- **Order**: downstream first, top to bottom: the displayed node first, the
  origin last — the engine's walk order, which the flattening in
  `view_builders.rs` keeps. (An earlier build-up order, origin first, was
  dropped: with an instance row as the header above its children, a reader
  would expect the parent below its children too, an unusual tree.) An
  instance row stays above its children, which start at the return node.
  Branches stay grouped: `union(A, B)` lists `union`, then A's chain, then
  B's. Since the row cap cuts the far-upstream end, the "… more" line is at
  the bottom.
- **Hover**: every row highlights under the pointer. A row of the network
  being viewed (depth 0) also highlights its node in the node network editor,
  until the pointer leaves the row or the menu closes. Rows inside an instance
  live in a network the editor is not showing, so they highlight only in the
  menu.
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

1. **Surface-hit δ.** Is 0.1 Å enough to cover the gap between the mesh and
   the SDF on coarse tessellations? See Geometry booleans; the Phase 4
   walkthrough settles it. (The atom-centre δ is decided: two bond lengths,
   see Phase 4 as built. One bond length, as first written here, was too
   small.)

## Performance

A trace re-evaluates the displayed node's upstream chain once, with the memo
installed and unbudgeted, plus the `trace` calls themselves. On top of that it
pays up to two deep copies of every value it visits, when the value moves from
the memo into the trace store (see `TraceContext`). Memory peaks at up to
three times the cone's values: once in the memo, up to twice in the store.

For CSG and editing chains this is about one refresh. Upstream `relax`,
`chemisorb` or `mechanosynth` nodes can make it take seconds, and a
million-atom chain pays a copy of the structure per node visited. v1 accepts
that; Phase 1 measures time and peak memory on the demo library. Only if that
measurement shows a problem are the alternatives considered, for example
keeping the last refresh's memo, or sharing atom payloads by `Arc` so that a
memo hit stops being a deep copy.

## Phased implementation plan

### Phase 1 — Engine, trait and fallback (Rust)

- `trace/` module with `TraceProbe`, `Contribution`, `TraceStep`, `TraceLink`,
  `TraceContext` (`Rc` values from the trace-lifetime store),
  `deciding_children`, `sdf_at` and `atom_point_delta`.
- `StructureDesigner::with_trace_eval_context` (unbudgeted memo, counts
  returned instead of written to `last_memo_counts`).
- `NodeData::trace` with its default `None`.
- `trace/engine.rs` returning a `TraceReport` (the `TraceNode` tree with
  `RowContribution`, plus `TraceStats`), and a thin
  `StructureDesigner::trace_hit` that delegates to it. The engine covers:
  - start-of-trace re-sync and bond-to-atom
  - the step loop
  - wire and array-element resolution, both directions
  - match-by-position links, dropped when nothing matches
  - custom-node enter and exit through `parameter`, via the evaluator's own
    frame helpers
  - opaque HOFs and zone wires
  - dedup keyed on the `TraceFrame`, and the row cap
  - the fallback with type-directed re-sync (atom ↔ point conversion,
    geometry-shell fallback) and the lost-probe listing
- Extend `PerNodeRayHit` (or add a sibling query) so the raycast also reports
  pin, hit kind, atom id and hit point. Today it reports only `node_ref` and
  distance.
- Move the overlap rule out of the API layer into
  `StructureDesigner::pick_displayed_candidates`. Switch `viewport_pick` and
  `query_hovered_atom_info` to it with no change in behaviour. **No test covers
  the rule today** (nothing under `rust/tests/` or the crates' `tests/`
  exercises `raytrace_per_node`, `viewport_pick` or `query_hovered_atom_info`),
  so its characterization tests below are written and passing **before** the
  move, against the behaviour as it is.
- Measure trace time and peak memory on the demo library networks and record
  the numbers in this doc.

Tests (`atomcad-structure-designer/tests/structure_designer/trace_engine_test.rs`).
Engine behaviour that needs a node to *return* something (links with
elements, match-by-position links, reading the context) uses the test-only
`ScriptedTraceNode` (see Testing). Fallback tests use `tag`, which v1 never
traces, and the test-only `UntracedMoveNode`, so Phases 3–4 cannot change their
outcome.

- *Origins.* A leaf with no phase inputs becomes an origin, labelled "shape"
  for a Blueprint output and "source" for a Crystal or Molecule output.
- *Fallback, kept.* An unsupported node that does not move atoms (`tag`) keeps
  the probe precise by position re-sync: the leaf upstream of it is reached
  with a probe and labelled "source", not listed `Unknown` by the lost-probe
  path.
- *Fallback, lost.* An unsupported node that moves atoms (the test-only
  `UntracedMoveNode`) loses the probe. Everything upstream through
  phase-typed pins is listed `Unknown`, without calling `trace` on any of it
  (a `ScriptedTraceNode` upstream records that it was not called).
- *Fallback, converted.* An Atom probe reaching the test-only
  `UntracedFillNode` (Blueprint input, atomic output) is converted to a
  `Point`. Upstream of it, a `union` of two shapes (itself still a fallback
  node in Phase 1) keeps only the shape that contains the point, instead of
  the probe being lost. A `Point` link from a `ScriptedTraceNode` into a
  Crystal input with no atom at the position is kept by the geometry shell
  when `geo_tree_root` is `Some`, and lost when it is `None`.
- *Array elements.* A `ScriptedTraceNode` links to element *i* of an array pin
  fed by three wires, one of which delivers a two-element array. Each *i*
  lands on the right wire, and the two elements of the array wire arrive at
  that source as output `element` 0 and 1.
- *Match by position.* A `ScriptedTraceNode` returns `id: None` links to two
  inputs; only the input that has an atom at the position is followed. The
  other is dropped, not listed as `Unknown`.
- *Custom nodes.* The trace enters a custom node, reaches its `parameter`, and
  comes back out to the instance's input. The instance row comes first, with
  the strongest contribution among its children. The inner rows follow as its
  children, with the right `network` and `TraceFrame`. A nested custom node
  nests one level further.
- *Dedup.* A diamond-shaped DAG lists each node once. Two instances of the
  same custom network on the path both appear, each with its own inner rows. A
  node inside the custom network whose id equals a parent-network node's id on
  the path is not deduped against it.
- *Opaque constructs.* A `map` (HOF) on the path and a wire crossing a zone
  boundary are both treated as nodes without `trace`.
- *Row cap.* A chain longer than the cap ends in a "… more" row.
- *Start of trace.* An atom hit is re-resolved by position against the real
  `NetworkResult`, and a bond hit is traced from the endpoint nearer the hit
  point.
- *Context and cost.* A `ScriptedTraceNode` holds `ctx.input(0)` and
  `ctx.output(0)` at once. In a chain where every node reads its input and
  its output several times, `stats.store_fills` equals the number of
  distinct store entries (one input and one output entry per node), and a
  counting node upstream (the `CounterUnitNodeData` pattern) evaluates once
  per trace. A trace leaves `last_memo_counts` unchanged.
- *Raycast details.* The extended per-node raycast reports pin, hit kind, atom
  id and hit point for an atom, a bond and a Blueprint surface.
- *Overlap rule, before the move.* `pick_displayed_candidates`'
  characterization tests, written against today's API-layer behaviour and
  kept green across the move: no hit; one hit; two hits within
  `OVERLAP_EPSILON` give two candidates; two hits farther apart give the
  nearest; with `privilege_active`, an active node among overlapping hits is
  chosen alone, and without it all candidates are returned.

#### Phase 1 as built

**Measurement** (`trace_measurement_test.rs`, release, every top-level
displayed node of every network in `samples/*.cnnd`, 191 traces; "refresh"
is a full refresh of the same network for comparison):

| | median | max |
|---|---|---|
| trace time | 0.5 ms | ~220 ms (`flexure-delta-robot` / `flexidelta-frame` #54; its refresh: ~250 ms) |
| memo peak | 0.12 MB | 14.6 MB (same trace) |
| trace store | 0.09 MB | 11.5 MB (same trace) |

A trace costs about one refresh of its cone or less, and memory stays well
under "three times the cone". No alternative from Performance is needed.
Two traces hit the 200-row cap (`flexidelta-frame` #54 and
`flexure-delta-robot` #16): in Phase 1 almost no node has `trace`, so a probe
lost early lists the whole upstream network. Phases 3–4 should shrink these.

**Where the build differs from the text above:**

- **Array inputs in the fallback.** The probe is re-synced against each
  element. Elements that match are followed. Elements that do not match are
  pruned, as long as at least one element matched. The probe counts as lost
  only when no element matches. This is what *Fallback, converted* needs
  (`union` keeps only the shape containing the point).
- **The row cap** is `TraceReport::truncated`, not a row. Phase 2's
  flattening appends the "… more" row.
- **A `parameter`** gets its own `passed` row inside the instance. When the
  call site leaves the pin unwired, the trace continues into the parameter's
  own default pin.
- **`query_hovered_atom_info`** does not call `pick_displayed_candidates`.
  Its reference distance is the nearest *atom* hit, not the nearest hit, and
  it is not restricted to top-level nodes. It shares `OVERLAP_EPSILON` and
  `overlapping_hits` instead. `viewport_pick` does call
  `pick_displayed_candidates`.
- **The overlap-rule characterization tests** target
  `pick_displayed_candidates` directly. The API functions need the GPU-backed
  `CADInstance` and cannot run in a test, so they could not be pinned down
  before the move.
- **Iterator chains are cut at the consumer.** `collect`, `map` and
  `filter` have no phase-typed inputs (`Iter[T]`, `Function`), so a
  `collect` over molecules becomes an origin labelled "source". A HOF with a
  phase-typed pin (`fold`'s `init`) falls back as described. The *Opaque
  constructs* test uses `fold`. The zone-wire case cannot be reached, because
  the trace never enters a body.
- `TraceStats` also reports `store_bytes`, for the measurement.

### Phase 2 — Viewport gesture, menu and API (Flutter + FRB)

- `trace_viewport_pick`, `trace_viewport_hit` and their API types; FRB codegen.
- Flattening the `TraceNode` tree into pre-order `APITraceRow`s, in
  `view_builders.rs`.
- The right-click slop in `cad_viewport.dart` and `onSecondaryClick`.
- The disambiguation overlay's trace mode, with no Solo button.
- The trace menu in the structure designer viewport, wired to `jumpToNode`.
- Reference guide: `doc/reference_guide/ui.md`, viewport navigation. Document
  that right-drag still orbits, that a right click opens the contributing-nodes
  menu, and what the labels mean.

Tests:

- **Flattening** (`rust/tests/structure_designer_api/`): a hand-built
  `TraceNode` tree with two nested instances flattens to pre-order rows. Each
  parent comes directly before its children, `depth` increases by one per
  instance, and inner rows carry the custom network as `host_network`. The
  instance row stays in the viewed network.
- **API tests** (`rust/tests/structure_designer_api/`, network built in code):
  - A ray through a known atom returns `Traced` with the expected rows.
  - A ray that misses everything returns `NoHit`.
  - A ray through two overlapping displayed nodes returns `Disambiguation`
    even when one of them is the active node.
  - `trace_viewport_hit` on the chosen candidate returns that node's rows, and
    on the other candidate that node's rows.
  - Gadgets are ignored: a ray through a gadget handle in front of an atom
    traces the atom.
  - `viewport_pick` and `query_hovered_atom_info` still pass the Phase 1
    characterization cases, now through the API.
- **Manual walkthrough** for the menu: right-drag still orbits; a right click
  opens the menu; overlapping outputs show the candidate overlay first,
  without Solo; a row lands on its node; a custom-node parent row lands on the
  instance in place, and its child row lands inside the custom network;
  collapse and expand work; Escape and clicking away dismiss. The click-versus-
  drag slop lives in the Flutter gesture code, so it is checked here, not in a
  Rust test. The Flutter smoke test is a pending manual step for the
  maintainer.

#### Phase 2 as built

**Where the build differs from the text above:**

- **The row cap is `APITraceResult::truncated`, not a row.** A "… more" row
  would carry a node address and be clickable; the menu draws a footer line
  instead.
- **The pick steps are domain functions** in `trace/pick.rs`
  (`trace_viewport_pick` → `TracePick`, `trace_viewport_hit`), reached through
  thin `StructureDesigner` methods. The FFI functions are one call each into
  `view_builders::trace_pick_view` / `trace_hit_view`.
- **`viewport_pick` and `query_hovered_atom_info` moved into view builders
  too** (`viewport_pick_view`, `hovered_atom_info_view`), with no change in
  behaviour. The API functions need the GPU-backed `CADInstance`, so this was
  the only way to run the "still pass through the API" cases in a test.
- **A right press does not orbit until it leaves the slop.** "Right button
  down starts an orbit" would let a click turn the camera by up to 4 px ×
  0.02 rad before the button comes up, and the traced ray would then miss what
  the user clicked. The orbit (or Shift-pan) starts at the first move past the
  slop, so a click leaves the camera exactly where it was.
- **Escape and a right click on the barrier also dismiss the disambiguation
  overlay**, in both its modes (the left-click mode had neither).
- The jump is `StructureDesignerModel.jumpToNode(hostNetwork, scopeChain,
  nodeId)`, the spine shared by Find Usages and error navigation.
- The menu widget (`lib/structure_designer/trace_menu.dart`) takes plain data
  and callbacks and has a unit test (`test/trace_menu_test.dart`: folding, row
  click, dismissal). The gesture and the jump are left to the walkthrough.
- No real node has a `trace` yet, so until Phases 3–4 the menu mostly shows
  "possibly contributed" rows ending in a "shape" or "source" origin.

### Phase 3 — Atomic nodes

`trace` for `structure_move`, `structure_rot`, `free_move`, `free_rot`,
`structure_invert` (Atom probes only), `relax`, `atom_union`, `atom_edit` and
`materialize`, including the passivating-terminator rule. Factor out the
movement transform helpers.
Also the array plumbing nodes `array`, `array_at` and `array_concat`, which
map an output `element` to an input element.

Tests (`trace_atomic_nodes_test.rs`), one or more per node, on small networks
built in code:

- Each movement node, and `relax`: a moved atom traces through the node as
  `Modified` with the right upstream id, and the row above it is not
  `Unknown`.
- `atom_union` element attribution by position: an atom from element 0 and one
  from a later element each list only their own branch. The other elements'
  links are dropped, not listed as `Unknown`.
- An `atom_union` fed by one wire from an `array` node: the output element
  index carries through the `array` node to the right input.
- `atom_edit`: an added atom stops at the node; an edited atom continues to its
  base atom; an untouched atom is `Passed`.
- `materialize` hands a `Point` probe to the Blueprint, with `delta` from
  `atom_point_delta`. A passivating H, and a halogen terminator, are probed at
  their parent's position. `atom_point_delta` itself: the shortest bond to a
  non-passivating neighbour for a bulk atom, and the `crystal_bond_length`
  fallback for an unbonded one.
- `array_at` and `array_concat` map an output element to the right input
  element (the `array` case is covered with `atom_union` above).

The Phase 1 tests are unchanged by this phase: their fallback cases use `tag`
and test-only nodes, none of which gets a `trace` in v1.

#### Phase 3 as built

Tests: `trace_atomic_nodes_test.rs` (18). The Phase 1 tests are unchanged;
one Phase 2 API test (`gadgets_are_ignored_by_the_trace`) now expects
`mover moved` where `free_move` used to fall back.

**Where the build differs from the text above:**

- **Terminators are recognised by structure, not by the passivation flag.**
  The text assumed `materialize`'s terminators carry `is_hydrogen_passivation`.
  They do not: lattice fill flags no terminator, and surface reconstruction
  flags the two dimer *hosts* (`doc/design_halogen_passivation.md` D5; only the
  general `passivate` path flags its terminators). Keyed on the flag,
  `materialize` would never say "passivated", would probe a dimer host at its
  first neighbour, and `atom_point_delta` would count the 1.09 Å C–H bond and
  skip the hosts' C–C bonds. Both now use `trace::is_terminator`: an atom of a
  passivant element (`ALLOWED_PASSIVANTS`) with exactly one bond. This changes
  the Phase 1 helper `atom_point_delta` too.
- **`array` has no input pins** (it holds literal values only), so it has
  nothing to map an element to. The node that gathers wired values into an
  array is **`sequence`**, and it got the `trace` instead: output element *k*
  is the *k*-th *connected* pin, since `eval` skips unwired ones. The
  `atom_union`-through-an-array test uses `sequence`.
- **No movement transform helper yet.** The Atom path needs no transform: the
  id survives, so the position is read from the input atom with that id. The
  five movers and `relax` share one helper, `trace::trace_same_atom_id`.
  Factoring the transform out of `eval` belongs with the Point path in
  Phase 4, its only user.
- **Nested arrays are not traced.** `sequence` with an array element type, and
  `array_at` / `array_concat` asked about an element *inside* the element they
  route, return `None` (the fallback): `TraceLink` holds one element index.
- `atom_edit` reads its tolerance from the wired `tolerance` pin, else the
  stored value, as `eval` does, and returns `None` in motif mode.
- Test support: the recording source can output a Crystal or a Blueprint
  (`SourceKind`) and records the probes it receives (`add_probe_source`), so
  the tests assert the upstream id, position and δ, not just the rows.

### Phase 4 — Geometry nodes

`trace` for `union`, `intersect`, `diff` and `geo_trans`, plus the inverse-point
path of the movement nodes on Blueprints.

Tests (`trace_geometry_nodes_test.rs`). Atoms are chosen **by query** on the
materialized output, never by id or hard-coded position, because both depend
on the fill. A shared helper picks the atom with the largest projection
along a direction, optionally restricted to atoms more than a distance from a
set of planes.

- `deciding_children` and `sdf_at` on plain numbers and on a sphere value: ties
  within δ, and none outside it.
- The sphere ∪ cube example: an atom on the far side of the sphere lists
  `sphere` and prunes the cube.
- The tetrahedron example: a face atom more than two bond lengths from any
  edge lists one half space; the outermost atom along an edge's direction two;
  the outermost atom along a vertex's direction three.
- `diff`: an atom on a cut face traces to the cutter; one on an uncut face
  traces to the base. With two `base` and two `sub` elements, each atom traces
  to its own element on each side, not to the whole pin.
- `geo_trans` and a movement node on a Blueprint: the inverse-transformed
  point still selects the right child upstream of the transform.
- A surface ray hit on a displayed Blueprint, as opposed to an atom, gives the
  same rows as an atom near that spot, with `delta` 0.1 Å.
- The top-of-doc example, end to end: for a face atom more than two bond
  lengths from any edge of the tetrahedron and from the sphere, the four rows
  exactly as shown.

The Phase 1 *Fallback, converted* test still passes: it traces through
`UntracedFillNode`, and `union` now prunes by its own `trace` with the same
result.

Manual walkthrough, which also settles open question 1 (surface-hit δ).

#### Phase 4 as built

Tests: `trace_geometry_nodes_test.rs` (14). The tetrahedron tests run over six
off-centre placements and check every face, edge and vertex in each; the
movement-node test covers `geo_trans` and all five movers. Phase 1's
*Fallback, converted* test passes unchanged.

**Where the build differs from the text above:**

- **The atom-centre δ is two bond lengths, not one**
  (`trace::ATOM_POINT_DELTA_BONDS`; `atom_point_delta` scales the bond length
  it finds). The argument under `materialize` does not hold: it assumes a
  filled atom sits less than one layer spacing below each face near it, and
  that the layer spacing is bounded by the bond length. A diamond (111) layer
  period is a/√3 = 2.06 Å (4/3 of a bond), and single-bond removal makes an
  edge recede further. Measured at the outermost atom of a tetrahedron edge
  over 24 off-centre placements, the two faces' signed distances differ by up
  to **1.98 Å** with single-bond removal and **2.45 Å** without (the default),
  against δ = 1.545 Å, so most edge atoms listed one half space and vertex
  atoms one of three. With δ = 3.09 Å they list two and three. The price is
  the one the text already accepts, at a larger distance: a face atom lists
  the neighbouring face too until it is about δ plus its own depth (≈ 4 bond
  lengths, 6.2 Å) from the edge, so the face-atom tests ask for four bond
  lengths where the text says two. Centred on a lattice atom, diamond's
  tetrahedral symmetry makes all four faces cut alike and hides the problem,
  which is why the tests move the tetrahedron off centre. This also changes
  the Phase 3 tests of `atom_point_delta` and `materialize`.
- **The transform helpers are pin readers.** Each mover and `geo_trans` has
  one helper that resolves its parameter pins (`motion_args`,
  `rotation_args`, `pivot_args`, `translation_arg`, `geometry_transform`),
  taking `arg: &mut dyn FnMut(usize) -> NetworkResult`: `eval` passes a
  closure over `evaluate_arg`, `trace` one over `TraceContext::input_value`.
  The wired-pin-else-stored-field rule is `network_evaluator::value_or_default`,
  factored out of `evaluate_or_default`. The shared `trace::trace_moved_point`
  builds the `Modified("moved")` step from the mapped point.
- **The movers' Point path is not limited to Blueprints.** A `Point` on a
  Crystal or Molecule (the fallback's geometry-shell case) is mapped back the
  same way; its geometry moves with the atoms.
- **The booleans fold the children's SDFs** (min, max, `max(B, −S)`) rather
  than reading `ctx.output(0)`: the same value, by the definitions in
  `atomcad-geo-tree`, with no output entry in the trace store. A `diff` with
  `sub` unwired follows its base only.
- Phase 1's measurement was not re-run; the two traces that hit the row cap
  should now be shorter.

## Testing

Tests for each phase are listed in that phase. This section is the method.

- **Where.** Engine and node tests live in
  `atomcad-structure-designer/tests/structure_designer/` (`trace_*_test.rs`,
  registered in that harness's module list). API and flattening tests live in
  `rust/tests/structure_designer_api/`. Run the harnesses explicitly
  (`cargo test -p atomcad-structure-designer --test structure_designer trace_`,
  `cargo test --test structure_designer_api trace_`), because `cargo test
  --workspace` stops at the first failing harness.
- **Input.** Tests build small networks in code and call `trace_hit` with a
  constructed hit (node, pin, atom or surface point), so engine and node tests
  do not depend on the raycast. The raycast is tested on its own (Phase 1) and
  end to end through the API (Phase 2). Use the existing network-building
  helpers rather than new `.cnnd` fixtures; a new fixture also updates two
  corpus snapshots, in the same change.
- **Assertion.** A shared helper renders a `TraceReport` as indented text, one
  line per row: node name, contribution label, children indented. Tests
  compare it to a literal, like the example at the top of this doc, so a
  failure shows the whole trace.
- **Test-only nodes.** Following `execute_flag_test.rs`
  (`CounterUnitNodeData`), the tests register their own `NodeData` types in
  the registry:
  - `ScriptedTraceNode` returns a `TraceStep` given by the test, reads
    `TraceContext` the way the test says, and records whether `trace` was
    called. It exercises engine paths that no real node takes in Phase 1, and
    it keeps those tests independent of real nodes' `trace`.
  - A counting node records its `eval` calls, for the evaluated-once
    assertions.
  - `UntracedMoveNode` translates its atomic input and keeps the default
    `trace`: the fallback's "moved, so lost" subject.
  - `UntracedFillNode` takes a Blueprint and outputs a Molecule with one atom
    at a position the test gives, and keeps the default `trace`: the
    fallback's atom → point conversion subject. `materialize` cannot serve,
    because it gets a `trace` in Phase 3.
- **Fallback subjects.** The "kept" case uses a real node v1 never traces,
  `tag`, so at least one fallback test runs on production code; the "lost"
  case uses `UntracedMoveNode`, because every real mover is on the v1 list or
  may keep some atoms in place. Neither depends on Phases 3–4. Whoever later
  gives `tag` a `trace` swaps in another untraced node that keeps atoms in
  place.
- **No wall-clock assertions** (as in `eval_memo_test.rs`). Cost is asserted
  through `TraceStats` and eval counts. Time and peak memory are recorded by
  the Phase 1 measurement, not tested.
- **UI** is verified by manual walkthrough (Phase 2). The Flutter smoke test
  is the maintainer's manual step.

## Files to modify

| Area | Files |
|---|---|
| Trait | `rust/crates/atomcad-structure-designer/src/node_data.rs` |
| Engine | new `rust/crates/atomcad-structure-designer/src/trace/` (`mod.rs` types and helpers, `engine.rs`); `evaluator/network_evaluator.rs` (crate-visible frame helpers for custom-network enter and `parameter` exit); `structure_designer.rs` (thin `trace_hit`, `with_trace_eval_context`, `pick_displayed_candidates`, raycast hit details) |
| Nodes | `nodes/{structure_move,structure_rot,free_move,free_rot,structure_invert,relax,atom_union,materialize,union,intersect,diff,geo_trans,array,array_at,array_concat}.rs`, `nodes/atom_edit/atom_edit_data.rs` |
| API | `rust/src/api/structure_designer/structure_designer_api.rs` (new functions; `viewport_pick` and `query_hovered_atom_info` switched to `pick_displayed_candidates`), `structure_designer_api_types.rs`, `view_builders.rs` (tree → rows) |
| Untouched | `atomcad-geo-tree` (uses the public `ImplicitGeometry3D`), `atomcad-crystolecule` (reuses `apply_diff` / `DiffProvenance`) |
| Tests | new `rust/crates/atomcad-structure-designer/tests/structure_designer/trace_{engine,atomic_nodes,geometry_nodes}_test.rs` (plus a shared helper module for the test-only nodes and the trace renderer), registered in `tests/structure_designer.rs`'s module list; new trace and flattening tests under `rust/tests/structure_designer_api/` |
| Flutter | `lib/common/cad_viewport.dart`, `lib/structure_designer/structure_designer_viewport.dart` |
| Docs | `doc/reference_guide/ui.md`; `rust/crates/atomcad-structure-designer/src/AGENTS.md` (a short "Output tracing" section: the trait, the fallback, "read only persisted data in `trace`") |
