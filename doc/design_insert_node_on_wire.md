# Insert node on wire, and dissolve node

## Status: design draft (2026-10-23), not implemented

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
- a Rust *commit* operation: delete the wire, add the node, wire both sides,
  place it, as one undo step;
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
| D9 | **`expr` inserts as an identity pass-through**: the wire declares input `x` through the open port, and the expression is set to `x`. | The network evaluates exactly as before; the user then edits `x` into `2 * x`. An empty expression (a fresh `expr`'s default since `doc/design_wire_declared_inputs.md` Phase 5) would put the destination in error. |
| D10 | **The Add Node list for an insertion is exact.** A type is listed only if the plan query finds at least one arrangement for it. A cheap two-sided static check prunes the candidates first. | A listed type that then cannot be spliced is a dead end with no explanation. |
| D11 | **Comment anchors need no handling when inserting.** A comment anchored to the wire ends up on the downstream half. | Wire anchors are keyed by the destination slot (`destination_param_id`, `doc/design_wire_annotations.md`), and the downstream half keeps that slot. |
| D12 | **Read-only (linked) networks refuse both operations.** The insertion gestures are inert, like every other canvas edit (`lib/structure_designer/AGENTS.md` → Linked libraries); Dissolve is absent from the menu and Ctrl+Delete says why. | — |
| D13 | **Dissolve is offered only when it has exactly one answer**: exactly one candidate input wire (D14). It never opens a dialog. When it does not apply, the menu item is hidden, not greyed out. | A dissolve that needs a choice is rare, and is easier to do by hand (Delete, then rewire) than to explain in a dialog. Hiding matches how the menu treats its other gated items (`canFactor`, `canConvertToClosure`). |
| D14 | **A candidate is a wire, not an input pin**, and only wires whose source fits every consumer count. | Counting wired *pins* would hide Dissolve on a `structure_move` whose `translation` is wired from a `vec3`, the most common shape. That Vec3 wire cannot feed `materialize`, so it is not a candidate and the answer is unique. An array pin with three wires (`union`) is three candidates: not offered. |
| D15 | **Ctrl+Delete dissolves the selected node, and explains a refusal.** When the node cannot be dissolved, nothing changes and a transient snackbar gives the reason (`Can't dissolve diff: two input wires could replace it (base, sub)`). | A menu can hide an item that does not apply; a key cannot. A shortcut that silently does nothing reads as a broken key. The reason comes from the same query that hides the menu item, so the two cannot disagree. Blender's Ctrl+X is Cut here; Ctrl+Delete reads as "a stronger Delete", which it is. |

## Insertion: the operation

### Naming a wire

The API addresses a wire the way the model stores it: the storage scope path
plus a `Wire` (`node_network.rs`): `source_node_id`, `source_pin`
(`NodeOutput { pin_index }` or `ZoneInput { pin_index }`),
`source_scope_depth`, `destination_node_id`, `destination_argument_kind`
(`External` or `ZoneOutput`), `destination_argument_index`. Body-return wires
(`ZoneOutput`) are insertable: a `map` body's `mul → result` is as natural a
place for an extra step as any.

### Choosing the insertion scope (D6)

Let `src` be the scope the source value lives in (the storage scope minus
`source_scope_depth` frames; for a `ZoneInput` source, the HOF's own body),
and `dst` the destination's effective scope (`dest scope ++ [dest id]` for a
`ZoneOutput` wire, else the dest scope). Every valid scope `S` satisfies
`src ⊑ S ⊑ dst` (prefix order). Given the clicked scope `C` from
`ScopeResolver.findContainingScope`:

- `S` = the longest prefix of `C` that is also a prefix of `dst`;
- if that is shorter than `src`, `S = src`.

The two new wires get their `source_scope_depth` recomputed for `S`, the same
way `connectPins` computes it today. A `ZoneInput` source always puts `S` in
that body or deeper, which the rule already guarantees.

### The plan query

```rust
pub struct InsertArrangement {
    /// Input slot the wire's source connects to. `None` = the node's open
    /// port (a new declared input, D9).
    pub input_pin: Option<usize>,
    /// The input pin's name; for the open port, the name the new input gets.
    pub input_name: String,
    pub output_pin: i32,        // never -1 (D4)
    pub output_name: String,
    pub output_type: DataType,  // resolved, for the dialog row
}

impl StructureDesigner {
    pub fn plan_insert_on_wire(
        &self,
        wire_scope: &[u64],
        wire: &Wire,
        insert_scope: &[u64],
        node_type_name: &str,
    ) -> Result<Vec<InsertArrangement>, String>;
}
```

It works on a **scratch clone** of the network at `insert_scope`
(`NodeNetwork` is `Clone`; bodies are `Arc`s, so the clone is cheap). On the
clone it:

1. adds the node as `add_node_scoped` does, with the wire's source type as the
   `DragSource` (`DragDirection::FromOutput`), so type-parameterized nodes
   adapt exactly as on a wire drop;
2. for each input slot that accepts the source, and for the open port if the
   node has one, connects the source and resolves each non-function output pin
   with `resolve_output_type_scoped`;
3. keeps the pairs whose output converts to the destination pin's type
   (`DataType::can_be_converted_to`), in pin order.

Nothing in the registry is touched, so the query needs no undo bookkeeping and
can be called freely. An empty list is a valid answer ("cannot be spliced
here"); `Err` means the wire, the scope or the node type does not exist.

### The insertable-types query (D10)

```rust
pub fn insertable_node_types(&self, wire_scope: &[u64], wire: &Wire,
                             insert_scope: &[u64]) -> Vec<APINodeCategoryView>;
```

A static pass comes first: the type must have some input that accepts the
source type (`static_match` with `FromOutput`, or
`declared_inputs().accepts_type`) **and** some output whose *declared* type
could convert to the destination type, treating `SameAsInput` as the source
type and abstract output types as permissive. The survivors then get the exact
`plan_insert_on_wire`, and the list is the types with a non-empty plan.
Grouping and the `allowed_in_zone_body` filter are `get_compatible_node_types`'s.

### The commit

```rust
pub fn insert_node_on_wire(
    &mut self,
    wire_scope: &[u64],
    wire: &Wire,
    insert_scope: &[u64],
    node_type_name: &str,
    arrangement: &InsertArrangement,
) -> Result<u64, String>;
```

1. Re-run the plan and require `arrangement` to be in it. The UI's copy may be
   stale if something changed between the menu and the click. A refusal
   changes nothing and pushes no undo step.
2. Snapshot the top-level network for undo, and take the layout pre-state
   (`text_format::snapshot_node_positions`, `layout::collect_all_wires`).
3. Remove the wire, remembering its slot.
4. Add the node with the drag source. Its position is a placeholder; step 6
   places it.
5. Connect the source to the chosen input. For an open-port arrangement
   (`input_pin: None`) that is `add_input_from_wire`'s internal step,
   `append_declared_input_and_connect`, without its own undo push; for an
   `expr`, then set the expression to the new input's name (D9). Connect the
   chosen output to the destination **at the remembered slot** (D5).
6. `layout::layout_incremental` with the step-2 pre-state (D7).
7. Revalidate, apply the display policy for the dirty set (as `connect_nodes`
   does), make the new node the selected, active node, and push one
   `NetworkSnapshotCommand` "Insert node" (D8).

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

- **Insert node…** → resolve `S` (D6) from the click, open the Add Node popup
  over `insertable_node_types`, plan the chosen type, show the dialog if the
  plan has more than one row, commit.
- **Delete wire** → the existing delete path.

`WireHitResult` gains the source pin kind and the destination argument kind,
which it drops today, so the commit can name the wire exactly.
`_isSelectableWire` keeps excluding `ZoneOutput` wires from *selection*; the
insertion hit test accepts them.

### Entry point 2: dragging an unconnected node onto a wire

Dragging a **single** node that has **no wires at all** over a wire in its own
scope highlights the wire if the node can splice there. Releasing over it
splices. The rules:

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
- The commit is `insert_existing_node_on_wire`, taking a node id instead of a
  type name. It shares steps 1–3, 5 and 7 of the commit; its step 6 is the
  same `layout_incremental` call, which for a kept node amounts to the repair
  above.

### Entry point 3: dropping a network from the user-types panel onto a wire

`_buildUserTypeDropTarget` gains an `onMove` that hit-tests wires under the
drag and highlights one the network can splice into (same plan query, same
per-drag memo). Dropping on a highlighted wire runs the insert flow with the
type already chosen: plan → dialog if ambiguous → commit. Dropping elsewhere
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
order and the first failure is the refusal reason; the message is what
Ctrl+Delete shows (D15), with `N` replaced by the node's name.

| Check fails when | Message | Why it is a refusal |
|---|---|---|
| the network is read-only | `Can't dissolve N: this network is read-only (linked library)` | D12 |
| there is no consumer | `Can't dissolve N: nothing uses its output — use Delete` | With no consumer, Dissolve is just Delete. |
| something consumes `N`'s function pin (`-1`) | `Can't dissolve N: it is used as a function` | That consumer wants `N` *as a function*; no input wire can stand in for it. |
| `N` is its network's return node | `Can't dissolve N: it is the network's return node` | The return node is the custom network's interface. Moving it is a separate decision. |
| there is no candidate | `Can't dissolve N: no input wire fits where its output goes` | — |
| there is more than one candidate | `Can't dissolve N: two input wires could replace it (base, sub)`, naming the candidates' pins; for one array pin, `(3 wires into shapes)` | D13, D14 |
| a consumer's slot would get two wires from the candidate's source node | `Can't dissolve N: union would receive x twice` | `Argument::incoming_wires` holds at most one wire per source node, so `union(a, move(a))` → `union(a, a)` cannot be represented. |

The messages are written for a snackbar: one line, the node's name first, and
a hint where there is an obvious alternative.

The type check covers only the pins the consumers feed. A consumer whose own
output type is `SameAsInput` may change type, and something further downstream
may then mismatch. That surfaces as an ordinary validation error, as after any
rewire, and one Ctrl+Z undoes it.

Scope is never an obstacle. The candidate's source is visible in `N`'s scope,
and every consumer lives in `N`'s scope or deeper, so it is visible to them
too. A `ZoneInput` source is visible because `N` sits inside that body.

### The query and the operation

```rust
pub struct DissolvePlan {
    pub candidate: Wire,                  // the input wire whose source is kept
    pub consumers: Vec<(Vec<u64>, Wire)>, // (storage scope, wire) to re-point
}

impl StructureDesigner {
    /// `Err` is the user-facing refusal message from the checks table.
    pub fn plan_dissolve_node(&self, scope: &[u64], node_id: u64)
        -> Result<DissolvePlan, String>;
    pub fn dissolve_node(&mut self, scope: &[u64], node_id: u64)
        -> Result<(), String>;
}
```

`plan_dissolve_node` is read-only. `dissolve_node` re-plans and, if the plan
now fails, refuses with that message and pushes no undo step. Otherwise it:

1. snapshots the top-level network;
2. re-points each consumer wire at the candidate's source **in its own slot**
   (D5). The new `source_scope_depth` is the wire's current depth plus the
   depth of `N`'s scope minus the depth of the source's scope;
3. deletes `N`, and its body if it has one;
4. revalidates, applies the display policy, and pushes one
   `NetworkSnapshotCommand` "Dissolve node" (D8).

Nothing else moves: a deleted node leaves a hole
(`doc/design_incremental_layout.md` D4). A comment anchored to `N` loses its
anchor by the existing dangling-anchor rule. A comment anchored to a consumer
wire keeps it, because the slot is unchanged. A comment anchored to the
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

- Rust: `plan_wire_drop(scope, source pin, node type)` returns the pin
  options, computed on a scratch clone like the insert plan, and covers every
  non-function output when dragging off an input. `add_node_and_connect(...)`
  performs the add and the connect (or the open-port declaration) and pushes
  **one** `NetworkSnapshotCommand` "Add node".
- Flutter: `_handleWireDropInEmptySpace` plans, shows `ArrangementDialog` if
  the choice is ambiguous, and commits; cancelling leaves the network
  untouched. The plan replaces both the Rust `getCompatiblePinsForAutoConnect`
  shortcut and the Flutter-side `_findCompatiblePinsCrossScope` probe.
- `node_network/AGENTS.md` → Interaction Model: the Auto-connect line
  describes the plan-first flow.

*Tests:* the plan for `diff` from a Blueprint output lists `base` and `sub`;
from an `atom_edit` input drag it lists `result` and `diff`; the plan never
changes the network (serialized before = after); `add_node_and_connect` is one
undo step and undo restores the exact prior network; a body-scope drop and a
capture drop produce the same wires today's flow does; an open-port target
declares its input. Dart: the row-label rule (one side varies / both vary).

### Phase 1 — Rust insertion: plan, types, commit

`plan_insert_on_wire`, `insertable_node_types`, `insert_node_on_wire`, the D6
scope rule as a pure function, and the `expr` identity (D9). FRB API in
`rust/src/api/structure_designer/`, with an API twin for `InsertArrangement`.

*Tests:* inserting `structure_move` between a blueprint and `materialize`
gives one arrangement and wires the source to `input` and `result` to
`materialize`; `passivate` between `materialize` and `structure_move`
type-checks through `SameAsInput` (Crystal in, Crystal out); inserting `relax`
after `exit_structure` resolves its output to Molecule; a type whose output
cannot reach the destination is absent from `insertable_node_types` and its
plan is empty; `diff` on a Blueprint wire yields two arrangements; the
function pin is never an output; inserting into the second wire of a
three-wire `union` keeps the concatenation order; a body-return wire is
insertable and the node lands in the body; a capture wire clicked outside the
body puts the node outside (depths recomputed), clicked inside puts it inside;
an `expr` insert evaluates identically to the network without it; a stale
arrangement is refused with no undo step; the insert is one undo step and redo
reproduces it; the node lands between its neighbours and, with no room, the
downstream half-plane shifts right while the upstream chain stays put; a
comment anchored to the wire is anchored to the downstream half afterwards; a
read-only (linked) network refuses.

### Phase 2 — Flutter: the wire context menu

The wire menu, `WireHitResult` carrying both pin kinds, and the insertion flow
through `ArrangementDialog`. Reference guide: `doc/reference_guide/ui.md` →
*Manipulating nodes and wires* gets an **Insert a node into a wire** entry.
`node_network/AGENTS.md` describes the wire menu beside the node context menu
section.

*Tests:* none automated beyond Phase 0's Dart unit test (thin editor UI;
`feedback_manual_test_for_editor_ui`). See the Phase 6 walkthrough.

### Phase 3 — Dissolve

`plan_dissolve_node`, `dissolve_node`, the FRB wrappers, the Edit-section menu
item with its pre-`showMenu` gate, and the Ctrl+Delete shortcut with its
refusal snackbar. Reference guide: a **Dissolve a node** entry beside *Insert
a node into a wire*, listing the shortcut. The shortcut also joins the
keyboard list in `node_network/AGENTS.md` → Interaction Model.

*Tests (Rust):* a `structure_move` between a blueprint and `materialize`, with
`translation` wired from a `vec3`, dissolves to blueprint → `materialize`
(D14); a `diff` with both inputs wired from Blueprints is refused; a `union`
with three inputs is refused; a node with no consumers, a consumed function
pin, or the return node is refused; `union(a, move(a))` is refused; a consumer
whose pin cannot take the candidate's type blocks the dissolve; a node feeding
two consumers re-points both, each in its own slot; a node feeding a capture
into a `map` body re-points the capture with the depth recomputed; a node
inside a body whose input is the body's `element` re-points its consumer to
`element`; dissolving a `map` deletes its body; the plan never changes the
network; the dissolve is one undo step and undo restores the exact prior
network; no other node moves; dissolving right after an insert restores the
pre-insert wiring; each refusal returns its own message from the checks
table, in check order (a read-only return node reports read-only);
`dissolve_node` refuses with the same message `plan_dissolve_node` gives and
pushes no undo step.

### Phase 4 — drag an unconnected node onto a wire

`insert_existing_node_on_wire` (Rust), the fast-path highlight, the per-drag
plan memo, and release handling. The guide's insertion entry gains the drag
gesture.

*Tests (Rust):* splicing an existing wireless node offers the same
arrangements as inserting a new node of its type; a node with any wire is
refused; the node keeps its dropped position unless the backward-wire repair
moves it; the splice is one undo step, separate from the move.

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
  behind; one Ctrl+Z removes the node and its wire together.

## Testing

Rust integration tests drive `StructureDesigner` directly. The plan queries
are checked for non-mutation by serializing the network before and after.
Undo is checked by comparing the serialized network after undo with the one
before the edit. Where a layout rule is what is being tested, assertions
compare positions against the `doc/design_incremental_layout.md` rules rather
than hard-coded coordinates. The Flutter side is thin and is verified by the
Phase 6 walkthrough; its only pure logic (the dialog row labels) has a Dart
unit test.

## Open questions

- **Dragging a node onto a capture wire.** The dragged node already lives in
  one scope, so D6's clamp either accepts that scope or refuses. The proposal
  is to refuse, i.e. not highlight the wire.
