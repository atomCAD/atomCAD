# Insert node on wire, and dissolve node

## Status: design draft (2026-10-23, revised after review), not implemented

## Motivation

atomCAD networks are mostly chains: blueprint → `structure_move` →
`materialize` → `passivate` → `relax` → `export_atoms`. The most common edit
to such a chain is **"put a step in between"**: a `structure_move` before
`materialize`, a `relax` after `passivate`, an `expr` that scales a value on
its way into a pin. Today that takes five or six actions:

1. select the wire, press Del;
2. open the Add Node popup, place the node;
3. draw the wire into it, then the wire out of it;
4. usually, drag neighbours apart, because the gap under a wire is narrower
   than a node.

This doc makes it one gesture: **right-click a wire → Insert node…**, pick a
node type from a list already filtered to what fits there, and the node is
spliced in. It is one undo step, the node is placed between its neighbours,
and the drawing widens if there is no room. In later phases two drag gestures
reach the same operation: dropping an unconnected node onto a wire, and
dropping a network from the user-types panel onto a wire.

The inverse is **Dissolve**: right-click a node → **Dissolve**, or
Ctrl+Delete. The node is removed and its input wire is joined to whatever its
output fed, so the chain closes over the gap. It is offered only when there is
exactly one way to do it.

Along the way the design fixes the existing **wire-drop-in-empty-space** flow,
which has the problem insertion must avoid (Phase 0).

### Scope

In scope:

- a Rust *plan* query: the valid ways a node type can be spliced into a given
  wire, computed without changing the network;
- a Rust *commit* operation: add the node, wire the source into it, re-point
  the old wire's slot at it, and place it, as one undo step;
- the same plan/commit split for the wire-drop-in-empty-space flow, and a
  shared arrangement dialog;
- three entry points for insertion: the wire context menu, dragging an
  unconnected node onto a wire, dropping a user-types-panel network onto a
  wire;
- **Dissolve**, on the node context menu and on Ctrl+Delete.

Out of scope:

- inserting into several selected wires at once;
- "insert and keep the old wire too" (fan-out);
- a text-format / CLI verb. An AI edit can already express an insertion as a
  rewire, and `ai_text_edit` already lays it out incrementally.

## Resolved decisions

| # | Decision | Rationale |
|---|---|---|
| D1 | **Plan, then commit.** Rust answers "which arrangements are valid" without changing anything; the dialog (if any) is shown; then one commit call performs the whole splice. | Insertion deletes a wire. A flow that changes the network first and asks second (today's wire-drop flow, `_handleWireDropInEmptySpace` in `node_network.dart`) leaves a half-done network when the user cancels. |
| D2 | **An arrangement is a pair** `(input, output pin)` and validity is decided **per pair**. | Output types are often `SameAsInput(...)`: which outputs fit the destination can depend on which input took the wire. Two independent pickers could offer a combination that does not type-check. |
| D3 | **The dialog is a flat list of valid pairs.** No dialog when there is exactly one pair. Each row names only the side that varies (`base` / `sub`; `result` / `diff`); when both vary, `base → result`. One click commits; Up/Down/Enter/Esc work. | Every choice stays one click, an invalid combination is never shown, and it is the shape the existing `PinSelectionDialog` already has. Both sides varying is rare. |
| D4 | **The node's function pin (`-1`) is never offered as the output.** | Inserting means the wire's value flows *through* the node. A wire that carries a function is still insertable, through a node whose ordinary input takes a function (a partial `apply`, say). |
| D5 | **The new wire into the destination takes the old wire's slot** in `Argument::incoming_wires`. | Array pins concatenate their wires in order. Appending would silently reorder a `union`'s inputs. |
| D6 | **The inserted node lives in the scope where the user right-clicked, clamped** to lie between the source's scope and the destination's effective scope. | A capture wire runs through the outer canvas and into a body. Inserting outside the body runs the node once; inside, once per element (`node_networks.md` → Cost model). Where the user clicked is the honest signal of which they meant. |
| D7 | **A new node is placed by `layout::layout_incremental`**, run over a pre-insert snapshot exactly as `ai_text_edit` does. | Step 4 of `doc/design_incremental_layout.md` already places a block with one upstream and one downstream anchor between the two, and shifts the downstream half-plane right when there is no room. The guide's promise ("the neighbours slide right together") then holds for this gesture too. |
| D8 | **One undo step**: a `NetworkSnapshotCommand` taken over the top-level network, labelled "Insert node" (or "Dissolve node"). | One snapshot covers the wires, the node, the layout moves and any body edits, as the declared-input operations already do. |
| D9 | **`expr` inserts as an identity pass-through**: the wire declares input `x` through the open port, and the expression is set to `x`. The plan applies the identity too, not only the commit. It reaches `expr` through a `DeclaredInputs` hook, `make_pass_through`, never by naming the node type. | The network evaluates exactly as before; the user then edits `x` into `2 * x`. An empty expression (a fresh `expr`'s default since `doc/design_wire_declared_inputs.md` Phase 5) has no output type, so a plan that resolved outputs without the identity would find no arrangement and D10 would drop `expr` from the list. The declared-input orchestrators never name a node type (`nodes/AGENTS.md` → Declared inputs), and the splice is one of them. |
| D10 | **The Add Node list for an insertion is exact.** A type is listed only if the plan query finds at least one arrangement for it. The only pruning before the plan is the *input-side* test the Add Node popup already uses; there is no static output-side check. | A listed type that then cannot be spliced is a dead end with no explanation. A static output check cannot be both cheap and sound: `SameAsInput` may mirror an input other than the one the wire enters, and drag adapters change output types. The plan is cheap enough (two-node scratch network) to run on every input-side survivor. |
| D11 | **Wire anchors are retargeted whenever a wire's source is replaced in its slot**, by the one primitive that does the replacing (`replace_wire_source`). A comment anchored to the spliced wire follows it to the downstream half; on Dissolve, a comment anchored to a consumer wire follows it to the re-pointed wire. | A `WireAnchor` records `source_node_id` beside the destination slot, and `resolve` matches on it. Both operations change the source of a wire whose slot they keep, so without a rewrite the anchor dangles and the repair pass drops it. Putting the rewrite inside the primitive means no caller can forget it. |
| D12 | **Read-only (linked) networks refuse both operations.** The insertion gestures are inert, like every other canvas edit (`lib/structure_designer/AGENTS.md` → Linked libraries); Dissolve is absent from the menu and Ctrl+Delete says why. | — |
| D13 | **Dissolve is offered only when it has exactly one answer**: exactly one candidate input wire (D14). It never opens a dialog. When it does not apply, the menu item is hidden, not greyed out. | A dissolve that needs a choice is rare, and is easier to do by hand (Delete, then rewire) than to explain in a dialog. Hiding matches how the menu treats its other gated items (`canFactor`, `canConvertToClosure`). |
| D14 | **A candidate is a wire, not an input pin**, and only wires whose source fits every consumer count. | Counting wired *pins* would hide Dissolve on a `structure_move` whose `translation` is wired from a `vec3`, the most common shape. That Vec3 wire cannot feed `materialize`, so it is not a candidate and the answer is unique. An array pin with three wires (`union`) is three candidates: not offered. |
| D15 | **Ctrl+Delete dissolves the selected node, and explains a refusal.** When the node cannot be dissolved, nothing changes and a transient snackbar gives the reason (`Can't dissolve diff: 2 input wires could replace it (base, sub)`). | A menu can hide an item that does not apply; a key cannot. A shortcut that silently does nothing reads as a broken key. The reason comes from the same query that hides the menu item, so the two cannot disagree. Blender's Ctrl+X is Cut here; Ctrl+Delete reads as "a stronger Delete", which it is. |

## Code layout

The Rust side has three layers. Each calls only the layers above it in
this table, never the ones below.

| Where | What | Depends on |
|---|---|---|
| `splice_plan.rs` (new, in `atomcad-structure-designer`) | The **pure planner**: `plan_arrangements` and its types, and the D6 scope-and-depth function `insertion_scope`. No `StructureDesigner`, no live network. | `NodeTypeRegistry`, the `NodeData` / `DeclaredInputs` traits |
| `splice_ops.rs` (new, same crate) | The **operations**, as `impl StructureDesigner`: the wire-drop and insert plans and commits, `insertable_node_type_names`, the dissolve plan and commit, the `replace_wire_source` primitive, and the refusal enums. Plans resolve the live types and call the planner; the insert commit runs the one private `splice`. | the planner, `add_node_core`, `declared_input_ops` |
| `rust/src/api/structure_designer/` | Thin FRB wrappers, the `insertable_node_types` view builder, and the API twins `APIArrangement`, `APINodeSource` and `APIDraggedPin` (with `From` both ways). Refusals cross as their `Display` text. | the operations |

Three existing pieces are lifted out or moved to serve them:

- `create_node_data` and `add_node_core` (`structure_designer.rs`, Phase 0),
  lifted out of the two add paths. One behaviour changes, on purpose: a node
  added in a body is now adapted to the drag source, as at top level;
- `accepts_drag_source` (`node_type_registry.rs`), moved out of a closure in
  `view_builders.rs` unchanged;
- `WireAnchor` moves from `nodes/comment.rs` to `node_network.rs`, beside
  `Wire`, unchanged. It is the canonical identity of a wire, used here far
  more than by comments. The name and serde shape stay the same, and
  `APIWireAnchor` already mirrors it.

## Insertion: the operation

### Naming a wire

Every API addresses a wire as **its storage scope plus a `WireAnchor`**:
destination node, argument kind (`External` or `ZoneOutput`), slot, and
source node id. That is the identity the model already uses for comments,
and it is minimal: within one slot `Argument` holds at most one wire per
source node, so the source pin and depth are properties of the wire, not its
name. `WireAnchor::resolve` assembles the full `Wire`. Its
`destination_param_id` precedence keeps the address valid when a
dynamic-arity destination reorders its pins. Flutter already builds anchors
from the canvas (as `APIWireAnchor`), so the address needs no new FRB type.

Body-return wires (`ZoneOutput`) are insertable: a `map` body's
`mul → result` is as natural a place for an extra step as any. Their storage
scope is the HOF's own network, where `zone_output_arguments` lives.

### Choosing the insertion scope (D6)

Depth conventions first, because the two source kinds count differently. A
wire stored in scope `T` with depth `k` names its source in scope
`T[..len(T) − k]`, the *frame* scope. For a `NodeOutput` source that frame is
where the source node lives. For a `ZoneInput` source it is the network that
contains the HOF; the validator requires `k ≥ 1` (`network_validator.rs`,
rule 3), because the value is visible only inside the HOF's body.

Let `src` be the shallowest scope where the source value is visible: the
frame scope for a `NodeOutput` source, `frame ++ [hof id]` (the body) for a
`ZoneInput` source. Let `dst` be the destination's effective scope
(`dest scope ++ [dest id]` for a `ZoneOutput` wire, else the dest scope).
Every valid scope `S` satisfies `src ⊑ S ⊑ dst` (prefix order). Given the
clicked scope `C` from `ScopeResolver.findContainingScope`:

- `S` = the longest prefix of `C` that is also a prefix of `dst`;
- if that is shorter than `src`, `S = src`.

The two new wires' depths follow from the convention:

- **into the new node** (stored in `S`): `len(S) − len(frame)`, which for a
  `ZoneInput` source is `len(S) − len(src) + 1`, never 0;
- **out of the new node** (into the old wire's slot, whose depth is counted
  from `dst`, the body for a `ZoneOutput` wire): `len(dst) − len(S)`.

The pure function, `splice_plan::insertion_scope(source kind, frame, dst, C)
-> InsertionScope { scope: S, depth_in, depth_out }`, returns `S` and both
depths. It is unit-tested on its own (Phase 1).

### The planner (`splice_plan.rs`)

Every plan in this design asks one question: which pairs (D2) does a
candidate node offer between a value arriving and a type it must produce? A
wire drop off an output has only the arriving side, a wire drop off an input
only the produced side, and an insertion both. One pure function answers all
of them:

```rust
pub struct SpliceSource {
    pub data_type: DataType,
    /// What an open port names a new input after (`declared_inputs::NameSource`).
    pub name: NameSource,
}

pub enum InputChoice {
    Pin(usize),
    /// The open port: a new declared input, named by
    /// `next_declared_input_name` (D9).
    OpenPort,
}

/// `name` is the pin's name; for `OpenPort`, the name the new input will get.
pub struct ArrangementInput  { pub choice: InputChoice, pub name: String }
pub struct ArrangementOutput { pub pin: i32, pub name: String, pub data_type: DataType } // pin ≥ 0 (D4)

/// One valid pair. A side is `None` exactly when the plan had nothing on
/// that side (a wire drop); an insertion's arrangements have both.
pub struct Arrangement {
    pub input: Option<ArrangementInput>,
    pub output: Option<ArrangementOutput>,
}
// All four types derive `Clone, Debug, PartialEq`: the commit compares the
// caller's arrangement against a fresh plan.

/// The node being planned for: a new node of a type, or an existing one.
pub struct PlannedNode { pub type_name: String, pub data: Box<dyn NodeData> }

impl PlannedNode {
    /// Default data, adapted to the drag (`create_node_data`, Phase 0).
    /// `None` for an unknown type. An insertion passes the source type
    /// `FromOutput`; a wire drop passes the dragged pin's type and direction.
    pub fn new_of_type(registry: &NodeTypeRegistry, type_name: &str,
                       drag: Option<&DragSource>) -> Option<PlannedNode>;
    /// A clone of an existing node's data, not adapted (Phase 4).
    pub fn existing(node: &Node) -> PlannedNode;
}

pub fn plan_arrangements(
    registry: &NodeTypeRegistry,
    node: &PlannedNode,
    source: Option<&SpliceSource>,
    target: Option<&DataType>,
    pass_through: bool, // insertion: apply `make_pass_through` (D9)
) -> Vec<Arrangement>;
```

`plan_arrangements` builds a **two-node scratch network**, and never sees
the user's:

1. **The scratch network.** A fresh `NodeNetwork` holding the planned node and,
   when there is a source, a stub: `nodes::value::add_typed_value_node`
   declared as the source type. Its value is irrelevant; only the declared
   output type is ever read.
2. **Each input.** The inputs tried are every input pin whose declared type
   the source converts to (`DataType::can_be_converted_to`), and the open port
   if the planned node's `declared_inputs()` `accepts_type` the source. For
   each, the planner takes a fresh copy of the scratch network (two nodes, so
   it is cheap) and wires the stub into that input.
   - **Open port:** append the declared input through the `DeclaredInputs`
     trait. With `pass_through`, call `make_pass_through` on the new input.
     Then re-derive the node type
     (`NodeTypeRegistry::populate_custom_node_type_cache_with_types`).
   - **Then** run the derived-layout post-passes the validator runs
     (`update_apply_pin_layouts_for_network`,
     `update_map_pin_layouts_for_network` and the other `f`-derived ones), so
     that an `apply` or `map` takes its layout from a function-typed source.
   - **No source** (a wire drop off an input): this step runs once, with
     nothing wired.
3. **Each output.** Each non-function output pin is resolved with
   `resolve_output_type`. With a target, the pair is kept if the output
   converts to it (`DataType::can_be_converted_to`). Without one, every
   output is kept. Arrangements are ordered by input (pins in pin order, then
   the open port), then by output pin. The dialog shows them in this order.

The scratch network is local, so neither a capture nor a `ZoneInput` source
needs an ancestor chain: the caller resolved the type and the stub carries it.
Being pure, the planner is tested without a designer (Phase 0).

**`DeclaredInputs::make_pass_through(&mut self, pin_id: u64) -> bool`** is
the D9 hook. `pin_id` is the id `append` returned. Like the trait's other
methods it edits the node's data only, and the caller re-derives the node
type afterwards. The default does nothing and returns `false`. `expr` sets
its expression to that input's name and returns `true`.

### The insert plan (`splice_ops.rs`)

```rust
pub enum NodeSource { New(String), Existing(u64) }

impl StructureDesigner {
    pub fn plan_insert_on_wire(
        &self,
        wire_scope: &[u64],
        wire: &WireAnchor,
        clicked_scope: &[u64],
        node: &NodeSource,
    ) -> Result<Vec<Arrangement>, SpliceRefusal>;
}
```

`wire_scope` is the wire's storage scope. `clicked_scope` is `C`, the scope
the user clicked in (or, for a drag, the dragged node's scope); Rust derives
`S` from it. The method does the live half and hands the rest to the
planner:

1. refuse a read-only network (`ReadOnly`, D12), so a linked network offers
   nothing;
2. resolve `wire` (`WireAnchor::resolve`, else `WireNotFound`) and
   `insertion_scope(…, clicked_scope)` (D6), giving `S`;
3. resolve the source type in the live network
   (`resolve_output_type_scoped` with the chain from `get_scope_ancestors`;
   for a `ZoneInput` source, the HOF's zone-input pin type), the source's
   `NameSource` (built from `open_port_name_source`, made `pub(crate)`), and
   the destination pin's declared type;
4. build the `PlannedNode`:
   - `New(type)`: `new_of_type` with the source type `FromOutput`, else
     `UnknownNodeType`;
   - `Existing(id)`: `existing`. Checked in this order:
     - the node must be in `clicked_scope` (its own scope), else
       `NodeNotFound`;
     - it must have no wires, else `NodeHasWires`;
     - `S` must equal `clicked_scope`, else `WrongScope`. `S` differs only
       when D6 clamps, that is, when the node's scope lies outside
       `src ⊑ … ⊑ dst`. An example is a top-level node dragged over a wire
       drawn inside a body. The node would have to change scope, which a
       splice never does. A capture wire is accepted from either end, since
       both the outer scope and the body lie within the range;
5. call `plan_arrangements` with both sides and `pass_through: true`.

Nothing is touched, so the plan needs no undo bookkeeping and can be called
freely. An empty list is a valid answer ("cannot be spliced here"). A
`SpliceRefusal` (see the commit) means the request itself is bad. A type
that is not `allowed_in_zone_body` gets an empty plan when `S` is a body. The
insertion scope is known here, so the Rust side filters, not Flutter.

### The insertable-types query (D10)

Split across the crate boundary, because the grouped view type is an api type:

```rust
// atomcad-structure-designer
impl StructureDesigner {
    pub fn insertable_node_type_names(&self, wire_scope: &[u64],
                                      wire: &WireAnchor, clicked_scope: &[u64])
        -> Result<Vec<String>, SpliceRefusal>;
}

// rust/src/api/structure_designer/view_builders.rs
pub fn insertable_node_types(designer: &StructureDesigner, wire_scope: &[u64],
                             wire: &WireAnchor, clicked_scope: &[u64])
    -> Vec<APINodeCategoryView>; // empty on a refusal
```

The candidates are exactly the types the Add Node popup offers for an output
drag of the source type. That test, today a closure inside
`view_builders::get_compatible_node_types` (static match, else declared
inputs that `accepts_type`, else a verified adapter), moves down into the
domain crate as `node_type_registry::accepts_drag_source(node_type, source,
direction, registry)`, and both callers use it. No output-side pre-check runs
(D10). The live half (steps 1–3 of the insert plan) runs **once**. Each
candidate type then gets only `PlannedNode::new_of_type` and
`plan_arrangements`, and the list is the types with a non-empty answer. The view builder only
groups, with `get_compatible_node_types`'s grouping.

### The commit

```rust
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SpliceRefusal {
    #[error("this network is read-only (linked library)")] ReadOnly,
    #[error("the wire no longer exists")]                  WireNotFound,
    #[error("the scope no longer exists")]                 ScopeNotFound,
    #[error("unknown node type {0}")]                      UnknownNodeType(String),
    #[error("the node is not in the insertion scope")]     NodeNotFound,
    #[error("only an unconnected node can be dropped onto a wire")] NodeHasWires,
    #[error("the node cannot reach this wire from its scope")] WrongScope,
    #[error("this scope cannot see the dragged pin")]      ScopeNotVisible,
    #[error("the network changed; try again")]             StaleArrangement,
    #[error("the inserted node's output no longer fits")]  ResultMismatch,
}

impl StructureDesigner {
    /// Returns the id of the node now on the wire.
    pub fn insert_node_on_wire(
        &mut self,
        wire_scope: &[u64],
        wire: &WireAnchor,
        clicked_scope: &[u64],
        node: &NodeSource,
        arrangement: &Arrangement,
    ) -> Result<u64, SpliceRefusal>;
}
```

`New` and `Existing` are one entry point and one private `splice`. The
difference is a single step, and two copies of the rest would drift apart.

1. Re-run the plan (which refuses first on a read-only network, and on
   every other bad request), and require `arrangement` to be in it, else
   `StaleArrangement`. The UI's copy may be stale if something changed
   between the menu and the click. A refusal changes nothing and pushes no
   undo step.
2. Snapshot the top-level network for undo, and take the layout pre-state
   (`text_format::snapshot_node_positions`, `layout::collect_all_wires`).
3. `New` only: add the node with `add_node_core` (Phase 0), adapted to the
   source type in any scope and recording nothing. Its position is a
   placeholder; step 6 places it.
4. Connect the source to the chosen input in `S`, with `depth_in` from D6.
   - `InputChoice::Pin`: `NodeNetwork::connect_wire`.
   - `InputChoice::OpenPort`: `append_declared_input_and_connect` (made
     `pub(crate)`; it already pushes nothing), then `make_pass_through` on
     the new pin and `rederive_node_type` (D9).
5. `replace_wire_source(wire_scope, wire, new node's chosen output,
   depth_out)`: the downstream half takes the old wire's slot (D5) and its
   anchors (D11). The old wire is never removed and re-added, so there is no
   intermediate state in which the slot is empty.
6. `layout::layout_incremental` with the step-2 pre-state (D7).
7. Check the result. Resolve the chosen output in the live network (with the
   scope chain) and require it to still convert to the destination type. The
   planner built its answer on a scratch network, and this is the check that
   the two agree. On a mismatch, restore the step-2 snapshot and refuse with
   `ResultMismatch`, with no undo step. This should never fire; the corpus run
   pins plan/commit agreement.
8. Revalidate, apply the display policy for the dirty set (as `connect_nodes`
   does), make the node the selected, active node, and push one
   `NetworkSnapshotCommand` "Insert node" (D8).

### The rewiring primitive

```rust
impl StructureDesigner {
    /// Replace the source of `wire` (stored in `wire_scope`) in its slot,
    /// keeping its position in the slot's `incoming_wires` (D5), and retarget
    /// every comment in `wire_scope` anchored to it (D11). Returns the
    /// wire's new address. On `Err` nothing has changed.
    pub(crate) fn replace_wire_source(
        &mut self,
        wire_scope: &[u64],
        wire: &WireAnchor,
        source_node_id: u64,
        source_pin: SourcePin,
        source_scope_depth: u8,
    ) -> Result<WireAnchor, SourceAlreadyInSlot>;
}

/// The slot already holds a wire from the new source node.
pub struct SourceAlreadyInSlot;

/// Read-only twin, for plans: would `replace_wire_source` accept replacing
/// the wire from `replaced_source` with one from `new_source` in `argument`?
pub(crate) fn slot_accepts_source(argument: &Argument, replaced_source: u64,
                                  new_source: u64) -> bool;
```

This is the only place in the design that edits an existing wire. Both
operations re-point a wire through it, and neither touches `incoming_wires`
or comment anchors directly. It enforces the per-source uniqueness invariant
of `Argument` itself. Dissolve's `union(a, a)` check is
`slot_accepts_source`, not a rule of its own. Insert can never trip it,
since its new source is a node created for the purpose (or, for
`Existing`, one with no wires).

Comment anchors are scope-local, and a wire's anchors live in its storage
scope: for a body-return wire, that is the HOF's network, beside
`zone_output_arguments`. So `wire_scope` is the only scope searched.

## Insertion: Flutter

### The arrangement dialog

`PinSelectionDialog` (`add_node_popup.dart`) is generalized into an
**`ArrangementDialog`** that takes rows `{label, typeLabel, color}` and
returns the chosen index. Row labels follow D3. The pin dot uses the canvas's
own type colours (`DATA_TYPE_COLORS` / `getDataTypeColor` in
`node_network.dart`); the current dialog's private `_getDataTypeColor` matches
type names that no longer exist (`geometry`, `atomic`), so most pins render
grey. The dialog takes keyboard focus: Up/Down move, Enter commits, Esc
cancels. It stays a `DraggableDialog`. The row-label rule is a pure function,
unit-tested in `test/`.

### Entry point 1: the wire context menu

Today the canvas's `_handleSecondaryTapDown` opens the background menu
(Add Node / Paste) on anything that is not a node. With this design it first
hit-tests wires with `NodeNetworkPainter.findWireAtPosition`. On a hit it
selects the wire and opens a wire menu instead:

- **Insert node…** → take `C` (`ScopeResolver.findContainingScope` at the
  click), open the Add Node popup over `insertable_node_types(wire_scope,
  wire, C)`, plan the chosen type, show the dialog if the plan has more than
  one row, commit. Flutter never computes `S`; it passes `C`, and Rust
  derives `S`.
- **Delete wire** → the existing delete path.

`WireHitResult` gains the destination argument kind, which it drops today,
so that it can build the `APIWireAnchor` every call takes (source node,
destination node, kind, slot).
`_isSelectableWire` keeps excluding `ZoneOutput` wires from *selection*; the
insertion hit test accepts them.

### Entry point 2: dragging an unconnected node onto a wire

Dragging a **single** node that has **no wires at all** over a wire
highlights the wire if the node can splice there without changing scope (the
plan succeeds and is non-empty). Releasing over it splices. The rules:

- Only a wireless node qualifies, and only in a single-node drag. Splicing a
  node that already has wires would have to decide what happens to them.
- The highlight test runs in the drag fast path (`node_network/AGENTS.md` →
  Performance invariants): on each `dragRepaint` tick the overlay painter
  hit-tests the dragged node's title-bar centre against the scope's wires. It
  never calls `notifyListeners`.
- Whether a wire *can* take the node is the plan query, asked once per wire
  the drag hovers and remembered for the rest of that drag: one plan per wire
  actually touched, not per tick.
- On release the move commits as it does today, as its own undo step. The
  splice then runs as a second step, with the dialog if the plan is ambiguous.
  Ctrl+Z undoes the splice and leaves the node where it was dropped;
  cancelling the dialog has the same result.
- The node stays where the user dropped it: it is a kept node, not an added
  one, so `layout_incremental` does not place it. Only the layout's
  backward-wire repair (Step 6 of the layout design) runs, which makes room
  if the drop left a wire running backwards.
- The plan and the commit are the same calls with `NodeSource::Existing(id)`,
  and `clicked_scope` is the node's own scope. The plan's checks
  (`NodeNotFound`, `NodeHasWires`, `WrongScope`) are listed under the insert
  plan, step 4. Commit step 3 is skipped. Step 6 is the same
  `layout_incremental` call, which for a kept node amounts to the repair
  above. The planned node is `PlannedNode::existing`, a clone of the node's
  **current** data, not adapted, since the user has already configured it.
  The node has no wires, so nothing else is needed to type it.

### Entry point 3: dropping a network from the user-types panel onto a wire

`_buildUserTypeDropTarget` gains an `onMove` that hit-tests wires under the
drag and highlights one the network can splice into (same plan query, same
per-drag memo, with `C` taken at the pointer). Dropping on a highlighted wire
runs the insert flow with `NodeSource::New(network name)`: plan → dialog if
ambiguous → commit. Dropping elsewhere
places the node as today.

## Dissolve

Remove node `N` and connect the value that flowed *into* it straight to
whatever `N`'s outputs fed.

### When it is possible (D13, D14)

The **consumers** are the wires leaving `N` from its non-function output pins,
in any scope (captures into bodies below `N` included). The **candidates** are
the wires into `N`'s inputs (each wire of an array pin counted separately)
whose source type converts (`DataType::can_be_converted_to`) to the pin type
of **every** consumer.

Dissolve is possible when none of the checks below fails. They run in this
order and the first failure is the refusal reason. The message is what
Ctrl+Delete shows (D15), with `N` replaced by the node's display name: its
`custom_name`, else its type name. The same rule names the consumer and the
source in the last row.

| Check fails when | Message | Why it is a refusal |
|---|---|---|
| the network is read-only | `Can't dissolve N: this network is read-only (linked library)` | D12 |
| there is no consumer | `Can't dissolve N: nothing uses its output — use Delete` | With no consumer, Dissolve is just Delete. |
| something consumes `N`'s function pin (`-1`) | `Can't dissolve N: it is used as a function` | That consumer wants `N` *as a function*; no input wire can stand in for it. |
| `N` is its network's return node | `Can't dissolve N: it is the network's return node` | The return node is the custom network's interface. Moving it is a separate decision. |
| there is no candidate | `Can't dissolve N: no input wire fits where its output goes` | — |
| there is more than one candidate | `Can't dissolve N: 2 input wires could replace it (base, sub)`. The list names each pin with candidates in pin order, and a pin holding several reads `3 wires into shapes`, as in `Can't dissolve union: 3 input wires could replace it (3 wires into shapes)` | D13, D14 |
| a consumer's slot would get two wires from the candidate's source node (`slot_accepts_source` is false) | `Can't dissolve N: union would receive a twice` (consumer, then source) | `Argument::incoming_wires` holds at most one wire per source node, so `union(a, move(a))` → `union(a, a)` cannot be represented. |

The messages are written for a snackbar: one line, the node's name first, and
a hint where there is an obvious alternative.

The type check covers only the pins the consumers feed. A consumer whose own
output type is `SameAsInput` may change type, and something further downstream
may then mismatch. That surfaces as an ordinary validation error, as after any
rewire, and one Ctrl+Z undoes it.

Scope is never an obstacle. The candidate's source is visible in `N`'s scope,
and every consumer's effective scope (D6's `dst`) is `N`'s scope or deeper, so
the source is visible to them too. A `ZoneInput` source is visible because
`N` sits inside that body.

### The query and the operation

```rust
pub struct DissolvePlan {
    /// The one input wire into `N` whose source is kept (stored in `N`'s scope).
    pub candidate: Wire,
    /// (storage scope, wire) for each wire to re-point. A body-return
    /// consumer is stored in the HOF's network, like any `ZoneOutput` wire.
    pub consumers: Vec<(Vec<u64>, WireAnchor)>,
}

/// One variant per row of the checks table, in check order. `Display` is
/// the snackbar text, with the node's name filled in.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DissolveRefusal {
    #[error("Can't dissolve {node}: this network is read-only (linked library)")]
    ReadOnly { node: String },
    #[error("Can't dissolve {node}: nothing uses its output — use Delete")]
    NoConsumer { node: String },
    #[error("Can't dissolve {node}: it is used as a function")]
    UsedAsFunction { node: String },
    #[error("Can't dissolve {node}: it is the network's return node")]
    ReturnNode { node: String },
    #[error("Can't dissolve {node}: no input wire fits where its output goes")]
    NoCandidate { node: String },
    #[error("Can't dissolve {node}: {candidates}")]
    SeveralCandidates { node: String, candidates: CandidateList },
    #[error("Can't dissolve {node}: {consumer} would receive {source} twice")]
    DuplicateSource { node: String, consumer: String, source: String },
    #[error("Can't dissolve: the node no longer exists")]
    NodeNotFound,
}

impl StructureDesigner {
    pub fn plan_dissolve_node(&self, scope: &[u64], node_id: u64)
        -> Result<DissolvePlan, DissolveRefusal>;
    pub fn dissolve_node(&mut self, scope: &[u64], node_id: u64)
        -> Result<(), DissolveRefusal>;
}
```

`CandidateList` holds `(pin name, wire count)` per pin with candidates, in
pin order. Its `Display` renders the checks table's shape:
`{total} input wires could replace it ({pin}, {pin}, …)`, where a pin with
more than one wire reads `{n} wires into {pin}`. Tests match on variants and
their fields. Each message's wording is
tested once, through `Display`. The FRB wrappers return the `Display` text,
since Dart only shows it.

`plan_dissolve_node` is read-only. `dissolve_node` re-plans and, if the plan
now fails, returns that refusal and pushes no undo step. Otherwise it:

1. snapshots the top-level network;
2. calls `replace_wire_source` for each consumer: the candidate's
   `source_node_id` and `source_pin`, in the consumer's own slot (D5), with its
   anchors (D11). The new `source_scope_depth` is **the consumer's current
   depth plus the candidate's depth**. The consumer's depth counts up from its
   storage scope to `N`'s scope, and the candidate's counts from `N`'s scope
   to the source's frame scope, so the sum counts the whole way. That holds
   for a `ZoneInput` source too, whose depth is measured to the network
   containing the HOF (D6's convention). The plan has already checked every
   slot with `slot_accepts_source`, so a `SourceAlreadyInSlot` here means the
   plan and the commit disagree. The snapshot is restored, and the refusal is
   `DuplicateSource`;
3. deletes `N`, and its body if it has one;
4. revalidates, applies the display policy, and pushes one
   `NetworkSnapshotCommand` "Dissolve node" (D8).

Nothing else moves: a deleted node leaves a hole
(`doc/design_incremental_layout.md` D4). A comment anchored to `N` loses its
anchor by the existing dangling-anchor rule. A comment anchored to a consumer
wire keeps it, through `replace_wire_source`. A comment anchored to the
candidate wire loses it, because that wire ran into `N` and is deleted with
it.

### Flutter

**Dissolve (Ctrl+Del)** goes in the node context menu's **Edit** section,
after *Cut*. Like *Factor out to Subnetwork…*, it is gated on a value computed
before `showMenu` (`model.canDissolveNode(nodeId, scopeChain)`, a thin wrapper
over `plan_dissolve_node(...).is_ok()`), so the item is absent rather than
greyed out. It appears only when a single node is selected.

**Ctrl+Delete** is handled in `node_network.dart`'s key handler, ahead of the
plain Delete/Backspace branch, which would otherwise delete the node. It acts
on the selection in the active scope (`activeScopeChain`):

- exactly one node selected → `model.dissolveNode(...)`; on a refusal the
  message goes to `showTransientSnackBar` and the network is untouched;
- no node selected, several nodes, or only wires → `Select one node to
  dissolve`.

Ctrl+Delete never falls through to deleting. A refusal is information, not a
failure, so it uses the plain transient snackbar rather than
`showErrorSnackBar` (`lib/AGENTS.md` → Showing an Error Message). The menu
item never needs a message, because it only appears when the plan succeeds.

## Phases

Each phase lists its own tests. Rust tests go in
`rust/crates/atomcad-structure-designer/tests/structure_designer/`.

### Phase 0 — plan-first wire drop, and the shared dialog

Today the wire-drop-in-empty-space flow creates the node, then asks which pin,
then connects. That is two undo steps, and cancelling the dialog leaves an
orphan node. When dragging off an input, the top-level shortcut
`get_compatible_pins_for_auto_connect` also only ever considers output pin 0.

- Rust, the add primitives both flows build on:
  - `create_node_data(type, drag_source, registry)` is the adapter call and
    its strict verification, lifted out of `add_node_with_drag_source`
    unchanged. It is pure, and the scratch plans use it.
  - `add_node_core(scope, type, position, drag_source) -> u64` adds a node
    with that data in **any** scope and initializes its custom-node-type
    cache. It does nothing else: no undo push, no display policy, no
    validation, no reflow. `add_node_with_drag_source` and `add_node_scoped`
    become `add_node_core` plus their existing bookkeeping. As a result the
    body path now adapts to the drag source, which it skips today
    (`add_node_scoped`'s "not exercised inside an HOF body in U2" comment):
    a wire dropped in a body makes the same node it makes at top level.
- Rust: `splice_plan.rs`, the pure planner (`plan_arrangements`, `PlannedNode`,
  `Arrangement` and its parts), lands here because the wire drop is its first
  caller. `DeclaredInputs::make_pass_through` lands with it (default and
  `expr`), although only the insertion passes `pass_through: true`.
- Rust, in `splice_ops.rs`, the wire drop:

  ```rust
  /// The pin a wire was dragged from.
  pub enum DraggedPin {
      /// A node output or an HOF's zone-input pin (`element`, `acc`).
      Output { node_id: u64, pin: SourcePin },
      /// An input slot, of either argument kind.
      Input { node_id: u64, kind: ArgumentKind, index: usize },
  }

  impl StructureDesigner {
      pub fn plan_wire_drop(&self, pin_scope: &[u64], pin: &DraggedPin,
                            drop_scope: &[u64], node_type: &str)
          -> Result<Vec<Arrangement>, SpliceRefusal>;
      /// Returns the new node's id.
      pub fn add_node_and_connect(&mut self, pin_scope: &[u64],
                                  pin: &DraggedPin, drop_scope: &[u64],
                                  node_type: &str, position: DVec2,
                                  arrangement: &Arrangement)
          -> Result<u64, SpliceRefusal>;
  }
  ```

  - **The plan.** `pin_scope` is where the dragged pin's node lives;
    `drop_scope` is where the new node will (`C` at the drop point; no clamp,
    the user chose it). The plan refuses `ReadOnly`, then `ScopeNotVisible`.
    Dragging off an output, `drop_scope` must lie at or below the source's
    `src` (D6's definition). Dragging off an input, it must lie at or above
    the input's `dst`. It then resolves the dragged pin's type (and, off an
    output, its `NameSource`) once in the live network. Finally it calls the
    planner with that type as the source (off an output) or the target (off
    an input), `PlannedNode::new_of_type` adapted in the same direction, and
    `pass_through: false`. Every non-function output is covered, not only
    pin 0.
  - **The commit.** `add_node_and_connect` re-plans (`StaleArrangement` if
    `arrangement` is no longer in it). It then adds the node (`add_node_core`)
    and makes the one wire with D6's depth convention: off an output, into
    the chosen input (or the open port); off an input, from the chosen output
    into that slot. Either way it goes through `NodeNetwork::connect_wire`,
    so an already-wired slot behaves exactly as it does in today's drop. It
    pushes **one** `NetworkSnapshotCommand` "Add node".
- FRB: the `APIArrangement` and `APIDraggedPin` twins (with `From` both
  ways). `APIArrangement` is used by every plan and commit from here on.
- Flutter: `PinSelectionDialog` becomes `ArrangementDialog` (see *The
  arrangement dialog*), with its pure row-label function.
  `_handleWireDropInEmptySpace` plans, shows `ArrangementDialog` if the choice
  is ambiguous, and commits; cancelling leaves the network untouched. The
  plan replaces both the Rust `getCompatiblePinsForAutoConnect` shortcut and
  the Flutter-side `_findCompatiblePinsCrossScope` probe.
- `node_network/AGENTS.md` → Interaction Model: the Auto-connect line
  describes the plan-first flow. The crate's `src/AGENTS.md`:
  - `splice_plan.rs` and `splice_ops.rs` join the directory map;
  - one line saying that a composite operation adds nodes through
    `add_node_core` inside its own snapshot, never through the recording add
    paths.

  `nodes/AGENTS.md` → Declared inputs names the `make_pass_through` hook.

*Tests (Rust, `splice_plan_test.rs`, the planner alone, no designer):*
- Each side alone and both together give the expected arrangements for
  `diff` (fixed pins), `structure_move` (`SameAsInput`), `expr` (open port,
  with and without `pass_through`) and `map` (adapted, function-typed `f`).
- An arrangement never has output pin −1.
- A `PlannedNode::existing` is not re-adapted.
- `make_pass_through` is `false` and changes nothing on a node without the
  override.

*Tests (Rust, `wire_drop_plan_test.rs`):*
- **Refusals.** A drop scope that cannot see the dragged pin is refused with
  `ScopeNotVisible`, and a stale arrangement with `StaleArrangement`; both
  change nothing.
- **The plan's answers.**
  - Dragging off a Blueprint output, the plan for `diff` lists `base` and
    `sub`.
  - Dragging off `relax`'s `HasAtoms` input, the plan for `atom_edit` lists
    `result` and `diff`, which today's pin-0-only shortcut cannot offer.
  - The plan never mutates (the shared oracle in Testing).
- **`add_node_and_connect`.**
  - It is one undo step, and the undo/redo round trip holds.
  - A drop inside a body produces a depth-0 wire stored on the body node.
  - A drop of an outer node's output into a body (a capture) produces a
    depth-1 wire stored on the body node.
  - An open-port target declares its input.
  - It joins the read-only alphabet.
- **`add_node_core`.**
  - It pushes no undo step.
  - In a body, a type-parameterized node (`map` from an `Array[Int]` output)
    gets the same adapted data as the same drop at top level.
- **The add-path refactor.**
  - The existing add-node tests pass unchanged.
  - `drag_adapter_test.rs` (api harness) passes unchanged.

*Tests (Dart):* the row-label rule (one side varies / both vary).

### Phase 1 — Rust insertion: plan, types, commit

- **Moves.** `WireAnchor` moves to `node_network.rs` (imports updated; serde
  shape unchanged, so comment fixtures round-trip as before).
  `accepts_drag_source` moves down, and `get_compatible_node_types` switches
  to it.
- **In `splice_plan.rs`:** `insertion_scope` (D6).
- **In `splice_ops.rs`:**
  - `plan_insert_on_wire`, `insertable_node_type_names`, and the
    `insert_node_on_wire` commit over the private `splice`, with
    `NodeSource::New` (`Existing` is accepted from Phase 4);
  - `replace_wire_source` / `slot_accepts_source`;
  - `SpliceRefusal`.
- **FRB.** The wrappers and the `insertable_node_types` view builder in
  `rust/src/api/structure_designer/`, with an `APINodeSource` twin. Wires
  cross as the existing `APIWireAnchor`.

*Tests (Rust, `splice_plan_test.rs`):* `insertion_scope` as a table: (source
kind, frame, `dst`, `C`) → (`S`, both depths). It covers a `NodeOutput` and a
`ZoneInput` source, a capture clicked outside and inside the body, a
body-return wire, and clicks clamped on both sides (shallower than `src`,
off the `dst` chain).

*Tests (Rust, `insert_node_on_wire_test.rs`):*
- **Plans.**
  - `structure_move` between a blueprint and `materialize` gives one
    arrangement (`input` → `result`).
  - `passivate` between `materialize` and `structure_move` type-checks
    through `SameAsInput` (Crystal in, Crystal out).
  - `relax` after `exit_structure` resolves its output to Molecule.
  - `diff` on a Blueprint wire yields two arrangements.
  - The function pin is never an output.
  - A type whose output cannot reach the destination has an empty plan and
    is absent from `insertable_node_type_names`.
  - `expr` is listed on a Float wire.
  - `apply` on a function-typed wire takes its layout from the source.
  - A type-parameterized node has a non-empty plan on a body-return wire.
  - A `parameter` is never listed for a body scope.
  - The plan never mutates (shared oracle).
- **Commits.**
  - The `structure_move` insert wires the source to `input` and `result` to
    `materialize`.
  - Inserting into the second wire of a three-wire `union` keeps the
    concatenation order.
  - A body-return insert lands the node in the body.
  - A capture insert clicked outside the body puts the node outside, and
    clicked inside puts it inside. The stored depths match the D6 table.
    Inserting an `expr` both ways, the network evaluates as before; the
    identity expression makes that comparison exact.
  - An insert on a body's `element` wire gives the node's input depth 1.
  - An `expr` insert evaluates identically to the network without it.
  - A comment anchored to the wire is anchored to the downstream half
    afterwards (its `source_node_id` is the new node).
  - A stale arrangement is refused with `StaleArrangement`, a missing wire
    with `WireNotFound`, and an unknown type with `UnknownNodeType`; each
    with no undo step and no change.
  - It joins the read-only alphabet.
  - The undo/redo round trip holds.
- **`replace_wire_source`.** It keeps the wire's position among the slot's
  `incoming_wires`, moves exactly the comments anchored to that wire (a
  comment on a sibling wire in the same slot keeps its own anchor), and
  refuses a source the slot already holds with `SourceAlreadyInSlot`,
  changing nothing. `slot_accepts_source` agrees with it on every case.
- **Layout** (against the layout oracle and the incremental-layout rules,
  not coordinates). The node lands between its neighbours. With no room, the
  downstream half-plane shifts right while the upstream chain stays put.
- **The corpus run, insert half** (`insert_dissolve_corpus_test.rs`, method in
  Testing). Plan/commit agreement is tested here, over real drawings, rather
  than on a hand-picked fixture.

### Phase 2 — Flutter: the wire context menu

The wire menu, `WireHitResult` carrying the destination argument kind (so it
can build an `APIWireAnchor`), and the insertion flow through
`ArrangementDialog`. Reference guide: `doc/reference_guide/ui.md` →
*Manipulating nodes and wires* gets an **Insert a node into a wire** entry.
`node_network/AGENTS.md` describes the wire menu beside the node context menu
section.

*Tests:* none automated beyond Phase 0's Dart unit test. This is thin editor
UI over Phase 1's tested calls, verified by the Phase 6 walkthrough.

### Phase 3 — Dissolve

`plan_dissolve_node`, `dissolve_node`, the FRB wrappers, the Edit-section menu
item with its pre-`showMenu` gate, and the Ctrl+Delete shortcut with its
refusal snackbar. Reference guide: a **Dissolve a node** entry beside *Insert
a node into a wire*, listing the shortcut. The shortcut also joins the
keyboard list in `node_network/AGENTS.md` → Interaction Model.

*Tests (Rust, `dissolve_node_test.rs`):*
- **Refusals.** One test per row of the checks table, each asserting its
  `DissolveRefusal` variant and fields. One table-driven test checks every
  variant's `Display` text against the checks table. Order is tested: a
  read-only return node reports `ReadOnly`. `dissolve_node` returns the
  refusal `plan_dissolve_node` gives, with no undo step and no change.
  - A `diff` with both inputs wired from Blueprints is refused.
  - A three-input `union` is refused.
  - A node with no consumers, a consumed function pin, or the return node is
    refused.
  - `union(a, move(a))` is refused.
  - A consumer whose pin cannot take the candidate's type blocks the dissolve.
- **Successes.**
  - `structure_move` between a blueprint and `materialize`, with
    `translation` wired from a `vec3`, dissolves to blueprint →
    `materialize` (D14).
  - A node feeding two consumers re-points both, each in its own slot.
  - A node feeding a capture into a `map` body re-points the capture, with
    the depth recomputed.
  - A node inside a body fed by the body's `element` re-points its consumer
    to `element` with depth 1, and depth 2 for a consumer one body deeper.
    Each evaluates as before the dissolve.
  - A comment anchored to a consumer wire is still anchored afterwards, to
    the re-pointed wire.
  - Dissolving a `map` deletes its body.
  - No other node moves.
  - Dissolving right after an insert restores the pre-insert network
    exactly, wiring and anchors alike. Positions are excluded, since layout
    may have moved neighbours.
  - The plan never mutates (shared oracle), and the undo/redo round trip
    holds.
  - It joins the read-only alphabet.
- **The corpus run, dissolve half**, plus the insert-then-dissolve round trip
  over the corpus (method in Testing).

### Phase 4 — drag an unconnected node onto a wire

`NodeSource::Existing` accepted by the plan and the commit (Rust), the
fast-path highlight, the per-drag plan memo, and release handling. The
guide's insertion entry gains the drag gesture.

*Tests (Rust, in `insert_node_on_wire_test.rs`):*
- Splicing an existing wireless node offers the arrangements its current
  data allows. An unconfigured node of a type-parameterized type may offer
  fewer than inserting a new one, which adapts.
- A node with any wire is refused with `NodeHasWires`, and a node not in
  `clicked_scope` with `NodeNotFound`.
- A top-level node over a wire inside a body is refused with `WrongScope`. A
  capture wire is accepted from a node at either end (outside, or in the
  body).
- The node keeps its dropped position unless the backward-wire repair moves
  it.
- The splice is one undo step, separate from the move. The round trip holds,
  and it joins the read-only alphabet.

*Tests (Dart):* the per-drag plan memo (a pure class with an injected
planner) asks once per wire across many hover ticks, and forgets everything
when the drag ends. This is the one piece of the fast path whose failure
would be silent: a plan per tick still works, only slowly.

### Phase 5 — drop a network from the user-types panel onto a wire

`onMove` hit testing in `_buildUserTypeDropTarget`, the highlight, and the
insert flow with the type preselected. The guide's insertion entry gains the
panel drop.

*Tests:* none beyond Phase 1's (the commit is the same call).

### Phase 6 — manual walkthrough

Human-only, per `AGENTS.md` (agents do not run the Flutter smoke test):

- right-click a wire between a blueprint and `materialize`, insert
  `structure_move`: one click, no dialog, neighbours slide right, one Ctrl+Z
  restores everything;
- insert `diff` on a Blueprint wire: the dialog shows `base` / `sub` with
  real pin colours, the keyboard works, Esc leaves the wire intact;
- insert `expr` on a Float wire: the result is unchanged until you edit `x`;
- insert on a capture wire outside a `map` body, then inside it, and confirm
  where the node lands;
- insert on a `map` body's return wire;
- right-click the inserted `structure_move` → **Dissolve**: the chain closes,
  nothing moves, one Ctrl+Z brings it back; right-click a two-input `diff`
  and confirm **Dissolve** is absent;
- select the `diff` and press Ctrl+Delete: the node stays and the snackbar
  names `base` and `sub`; select a dissolvable node and press Ctrl+Delete: it
  dissolves; with two nodes selected, Ctrl+Delete deletes nothing and asks for
  one node; plain Delete still deletes;
- drag a fresh unconnected node over several wires: only compatible ones
  highlight, the canvas stays smooth in a 100-node network, release splices
  and the node stays where it was dropped;
- drop a custom network from the panel onto a wire;
- wire-drop in empty space: cancel the pin dialog and confirm no node is left
  behind; one Ctrl+Z removes the node and its wire together;
- run the Flutter smoke test (`flutter test integration_test/`).

## Testing

**Where tests live.** Everything about the operations tests the domain
functions in `atomcad-structure-designer` (`insertable_node_type_names`, not
its view builder), so it lives in that crate's `structure_designer` harness,
each file registered in `tests/structure_designer.rs`:

- `splice_plan_test.rs`: the pure planner and `insertion_scope`, with no
  designer;
- `wire_drop_plan_test.rs`, `insert_node_on_wire_test.rs`,
  `dissolve_node_test.rs`: the operations;
- `insert_dissolve_corpus_test.rs`: the corpus run.

Refusals are asserted as `SpliceRefusal` / `DissolveRefusal` variants,
never as strings. The wording is tested once, in the `Display` table test.
The FRB wrappers and the view builder are thin and untested, per
`rust/AGENTS.md`. The one api-side test that matters is
`rust/tests/structure_designer_api/drag_adapter_test.rs`, and it is a
regression net, not a new test: `accepts_drag_source` moves out from under
it, so it must stay green unchanged.

**Shared oracles**, in one support module used by the operation and corpus
files:

- *No mutation*: a plan or refusal leaves the serialized network
  byte-identical, the undo stack the same depth, and the dirty flag
  unchanged. The serialized form alone would miss a stray undo push.
- *Round trip*: after the operation, undo restores the serialized network
  that was there before, and redo restores the one after it.
- *Invariants*: `invariants::check_document_invariants` reports nothing fatal
  after every successful operation and after every undo and redo. That is
  what catches a wire with the wrong depth or a duplicate source in one
  argument before evaluation does.
- *Layout*: where a placement is what is tested, `layout_oracle` and the
  `doc/design_incremental_layout.md` rules, never hard-coded coordinates.

"Serialized" means the network's `.cnnd` JSON. The dissolve-after-insert
comparison drops node positions from it first (Phase 3).

**Read-only.** Each new mutating entry point (`add_node_and_connect`,
`insert_node_on_wire` with each `NodeSource`, and `dissolve_node`) joins the
mutation alphabet in `library_links_readonly_test.rs`. That gives it both
halves for free: refused, with nothing changed, on linked content, and
working on the local twin.

**The corpus run** (`insert_dissolve_corpus_test.rs`) is the net for what
hand-written fixtures cannot see. That covers plan/commit agreement across
every node type, real drawings with real overlaps, and wire shapes nobody
thought to write a case for. It loads `demolib/baselib_with_demos.cnnd`
through the real loader, as `layout_corpus_test.rs` does, and honours the
same `ATOMCAD_LAYOUT_CORPUS` override for the private file.

- *Insert half (Phase 1).* Over every wire of every network, plan
  `insertable_node_type_names`. Then, from a seeded sample capped at a few
  hundred (wire, type, arrangement) triples, commit, run the four oracles,
  and undo. Every commit must succeed (`ResultMismatch` never occurs), and
  every planned type must commit. The sample is seeded and logged, so a
  failure names its triple.
- *Dissolve half (Phase 3).* Over every node, `plan_dissolve_node`. For each
  node it accepts, dissolve, run the oracles, and undo. No refusal may be
  `NodeNotFound`, and no commit may return a `DuplicateSource` its plan
  accepted.
- *Round trip (Phase 3).* For each sampled insert, dissolving the inserted
  node restores the pre-insert network, positions aside.

A failure here is reproduced as a named test in the operation's own file
before it is fixed. This file is a net, not a workbench, like the layout
corpus test.

**Not automated.** The Flutter side is thin. Its two pieces of pure logic are
unit-tested in Dart: the dialog row labels (Phase 0) and the per-drag plan
memo (Phase 4). Everything else, from the menus and the keyboard to the drag
highlight, its smoothness and the snackbars, is the Phase 6 walkthrough,
together with the Flutter smoke test, which only the maintainer runs.

## Open questions

None. (Dragging a node onto a capture wire, once open, is settled by D6: see
Entry point 2.)
