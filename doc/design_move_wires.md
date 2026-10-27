# Moving wires between pins, and edge auto-pan (issue #435)

## Status: Design (2026-10-27); not implemented

## Motivation

To change where an existing wire goes, the user deletes it and draws it again
from its other end. That is cheap when both ends are on screen and painful
when one is not: wire `A.out → B.in` with A a screen away (or further than the
farthest zoom level shows), and the user wants A's value to go into C, next to
B, instead. Today they must reach A's output. The workarounds reported in
#435 are moving nodes back and forth, or a relay `expr` that does nothing.

Some rewiring already works without reaching the far end. A drop onto a
connected single input **replaces** its wire (`NodeNetwork::connect_nodes`),
so *feeding B from a nearby D* is a drag between D and B. What is missing is
the other half: moving a wire's end **away from** the pin it is on, while its
far end stays put. Array inputs have neither half: a drop adds a wire, so
replacing one is a delete followed by a draw.

This doc adds one gesture: **picking up the wires of a pin and dropping them
on another pin.** It also adds **edge auto-pan**: while a wire, a node or a
selection rectangle is dragged, holding the pointer near the canvas edge, or
past it, pans the canvas that way. That attacks the same "far end" problem
from the other side, for new wires and node drags too. It is an independent
phase (Phase 3) and can land before or after the move gesture.

### The rule

- **Ctrl+drag on a connected pin moves all of its wires** to the pin it is
  dropped on. This works on every pin kind: input, array input, output, the
  function pin, zone pins.
- **A plain drag starts a new wire**, as today, except on a **connected
  single input**, where a plain drag moves its wire too. A new wire from
  there could only replace the one already in it, which the reverse drag
  already does.
- Ctrl+drag on an unconnected pin is a plain drag.

| Pin | Plain drag | Ctrl+drag |
|---|---|---|
| unconnected (any) | new wire | new wire |
| single input, connected | **move its wire** | **move its wire** |
| array input, connected | new wire (adds one) | **move all its wires** |
| output / function pin / zone input, connected | new wire (fans out) | **move all its wires** |

### Scope

In scope:

- a Rust *plan* query that says whether, and why not, the wires of one pin
  can move to another, and a Rust *commit* that moves them as one undo step;
- the drag in the node network editor: picking up, the rubber bands, target
  acceptance, the drop, Escape;
- edge auto-pan during wire drags (new and moved), node drags and rectangle
  selection;
- the reference guide.

Out of scope:

- moving **one** wire of several (one of an array input's wires, one
  consumer of an output). Delete and redraw it. Grabbing a wire near its end
  can be added later if this turns out to be needed;
- dropping moved wires on an open-port `+` tab
  (`doc/design_wire_declared_inputs.md`). The tabs do not appear during a
  move;
- proximity snapping of the drop to the nearest compatible pin. It helps the
  same "far end" problem, but it is a separate issue;
- edge auto-pan for the resize handles (HOF bodies, comment notes), the
  comment-anchor handle, and drags that come in from the user-types panel.
  Panel drags start outside the canvas, so the canvas's pointer listener
  never sees them;
- a text-format / CLI verb. An AI edit expresses a rewire directly.

## Resolved decisions

| # | Decision | Rationale |
|---|---|---|
| D1 | **The gesture rule is the table above.** No Alt variant, and no right-drag. | One rule, one modifier. The only thing a plain drag from a connected single input did that the reverse drag cannot was a drop in empty space creating a replacement upstream node, which is rare and is a delete and a drag by hand. Right-press on a node opens its context menu, Shift+right-drag pans, and Flutter's `Draggable` answers only the primary button. |
| D2 | **A move takes all of the pin's wires, or none.** Every wire must be valid at the new pin, or nothing changes and the target does not accept the drop. | A move that silently leaves some wires behind is worse than one that refuses. |
| D3 | **A moved wire is judged like a fresh connect to the new pin.** It uses the same type rule (`DataType::can_be_converted_to`), the same scope rule (`doc/design_zones_ui.md` §"Computing source_scope_depth at wire creation"), and the same zone-input rule (today enforced in Flutter's `_zoneInputSourceInScope`). The only additions are D4's wire count and D6's duplicate rule, which a single new wire cannot hit. Cycles are not refused, because a plain connect does not refuse them either: validation flags them. | The user should not have to learn a second set of rules. |
| D4 | **Input-side targets.** A single input accepts a move of exactly one wire. If it already holds a wire, that wire is displaced, as on any drop onto a connected single input. An array input appends the moved wires after its own, in their original order. | Array pins concatenate in order (`doc/design_insert_node_on_wire.md` D5). Appending keeps the target's existing order, and the moved wires keep theirs. |
| D5 | **Output-side moves re-source every consumer.** Each wire keeps its slot, and its position in that slot, and only its source changes, through `StructureDesigner::replace_wire_source`. | The consumers' arguments, including array order, are what the user built. A move changes only where the value comes from. |
| D6 | **A move never makes a wire vanish, apart from the one a single input displaces (D4).** When a target slot already holds a wire from the moved wire's source node, the move is refused (`DuplicateSource`), not merged. | `Argument::incoming_wires` holds at most one wire per source node. A plain connect merges into the existing wire. For a move, merging would delete a wire the user is holding, which D2 promises never happens. Dissolve refuses the same case the same way. |
| D7 | **Dropping in empty space, or on the starting pin, does nothing.** So does Escape during the drag, or a release over an invalid target. | A missed drop is a common outcome, as `node_network/AGENTS.md` notes for comment anchors, and a miss must not delete wires. The Add Node popup does not fit a drag that can hold several wires. |
| D8 | **One Rust command, plan then commit, one undo step.** `plan_move_wires` is read-only. `move_wires` re-plans, refuses if the plan refuses, and records one `NetworkSnapshotCommand` over the top-level network, labelled "Move wire" or "Move wires". | One snapshot covers wires in any body, displaced wires, comment anchors and adoption. A delete followed by a connect would be two undo steps and two refreshes, and the first would leave a half-done network if the second were refused. |
| D9 | **Rust owns every rule.** Unlike `can_connect_wire_scoped`, the plan checks the scope and zone-input rules itself rather than leaving them to Flutter. Flutter decides only the gesture (D1). | The plan must be authoritative, or a target can accept the drop and the commit refuse it. |
| D10 | **Comment anchors follow the wires.** Output-side: `replace_wire_source` already retargets them (`doc/design_insert_node_on_wire.md` D11). Input-side: a new sibling primitive, `replace_wire_destination`, moves one wire to a new slot. It also rewrites the destination fields of the comment anchors on that wire, provided the wire stays in the same storage scope. A wire moved into another scope loses its anchor, because the comment cannot come with it. A displaced wire's anchors are dropped by `drop_dangling_anchors`. | A note about a wire should still be about that wire after a move. Putting the rewrite in the primitive means no caller can forget it. |
| D11 | **Read-only (linked) networks: the pick-up is inert**, like every other canvas edit gesture (`lib/structure_designer/AGENTS.md` → Linked libraries). Rust refuses too. | — |
| D12 | **Edge auto-pan runs during wire drags (new and moved), node drags and rectangle selection**, and only while the pointer is inside an edge band of the canvas or past its edge. Its speed grows with how deep the pointer is in the band, and with how far past the edge it is, up to a cap. It is in screen pixels per second, so it feels the same at every zoom level. | These are the drags whose target can be off screen. A band rather than a hard edge lets the user control the speed. Past the edge has to count too, or a fast pointer flies over the band and stops panning. |
| D13 | **The dragged thing stays under the cursor while the canvas pans.** Each pan step moves dragged nodes by the same logical amount (through `dragSelectedNodes` / `dragNodePosition`) and shifts a selection rectangle's start corner by the same screen amount. A wire's free end needs nothing, because it is stored in canvas-local pixels. | Without it, a node dragged into the band slides away from the pointer and the selection rectangle stretches the wrong way. |
| D14 | **A wire release never acts on a pin that is no longer under the cursor.** If the canvas panned since the last pointer move, and the pin under the cursor now differs from the one `Draggable` last hit-tested (either one may be "no pin"), the release does nothing: no connect, no move, no Add Node popup. | `Draggable` re-checks its targets only on pointer moves. A release after panning with a still pointer would otherwise connect to the pin that *was* under the cursor before the pan, which may be off screen by now. |
| D15 | **Auto-pan is view state.** It is not undoable. The viewport is persisted once, when the drag ends (`_syncCanvasViewportToKernel`), as after a manual pan. | Same status as every other pan and zoom. |

## Rust: the operation

New module `rust/crates/atomcad-structure-designer/src/wire_move_ops.rs`, as
`impl StructureDesigner`, beside `splice_ops.rs`, whose shape it copies.

### Naming a pin

```rust
/// One pin of one node, addressed the way Flutter's `PinReference` addresses it.
pub struct PinAddress {
    /// Scope of the pin's owner node.
    pub scope: Vec<u64>,
    pub node_id: u64,
    pub side: PinSide,
}

pub enum PinSide {
    /// A wire destination: an external input, or an HOF's zone-output pin.
    Input { kind: ArgumentKind, index: usize },
    /// A wire source: a regular output (`-1` = function pin) or a zone input.
    Output(SourcePin),
}
```

### The wires of a pin

- **Input pin**: the `incoming_wires` of the slot, in order. They are stored
  in the owner's scope for both kinds. The effective scope that depths count
  from is the owner's scope, plus the owner itself for `ZoneOutput`.
- **Output pin**: every wire, in the owner's scope or any body below it,
  whose source resolves to exactly this `(node, SourcePin)`. That is
  `splice_ops::collect_consumers` with a pin filter, and with `ZoneInput`
  sources matched as well. Dissolve today matches all of a node's
  `NodeOutput` pins. Generalise that function rather than copying it, and
  move it to wherever both modules can reach it.

### The plan

```rust
pub struct WireMovePlan {
    /// The wires that will move, as `(storage scope, anchor)`, in order.
    pub wires: Vec<(Vec<u64>, WireAnchor)>,
    /// The wire a single input will displace (D4), if any.
    pub displaced: Option<(Vec<u64>, WireAnchor)>,
}

pub fn plan_move_wires(&self, from: &PinAddress, to: &PinAddress)
    -> Result<WireMovePlan, WireMoveRefusal>;
```

The checks run in the order of `WireMoveRefusal`'s variants, and the first
that fails is the answer:

```rust
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum WireMoveRefusal {
    ReadOnly,
    PinNotFound,
    /// `from` has no wires.
    NoWires,
    /// One is an input and the other an output.
    DirectionMismatch,
    /// `from == to`.
    SamePin,
    /// More than one wire, dropped on a single input (D4).
    TooManyWires { count: usize },
    /// A wire's source would not be in scope at the new destination, or a new
    /// source would not be in scope at a consumer (D3).
    OutOfScope { wire: String },
    /// A wire's type does not convert to the new pin (D3).
    TypeMismatch { wire: String, from_type: String, to_type: String },
    /// A target slot already reads the source node (D6).
    DuplicateSource { wire: String },
}
```

`wire` is a display label such as `cube → union.shapes`. The canvas does not
show the reason. A refused pin simply does not take the drop, exactly as an
incompatible pin does not take a new wire today (`PinWidget` returns a bare
`bool` from `onWillAcceptWithDetails` and shows nothing). The variants and
their `Display` text exist for the tests, and for any later surface that
wants to explain a refusal. Giving a reason during a drag is a separate
improvement, for new wires and moves together.

Per side:

- **Input → input.** For each moved wire, the source's scope must be a
  prefix of the new destination's effective scope. If the source is a zone
  input, the body that zone input feeds (the HOF's scope plus the HOF) must
  be a prefix too. The new depth is the
  length of the new effective scope minus the length of the source's scope.
  The type check is `can_be_converted_to(source type, new pin type)`, with
  the source type resolved as `can_connect_wire_scoped` resolves it. The new
  slot must not already read any moved wire's source node, unless that is
  the wire being displaced.
- **Output → output.** For each consumer, the new source's scope must be a
  prefix of the consumer's effective scope, with the zone-input rule as
  above. The new depth is the length of the consumer's effective scope minus
  the length of the new source's scope. The type check is
  `can_be_converted_to(new source type, consumer pin type)`, and the
  consumer's slot must not already read the new source node, unless that
  node is the old source as well (moving between two outputs of one node).

### The commit

```rust
pub fn move_wires(&mut self, from: &PinAddress, to: &PinAddress)
    -> Result<(), WireMoveRefusal>;
```

1. Re-plan, then snapshot the top-level network for undo.
2. Move the wires:
   - **input side**: if the target is a single input, remove its displaced
     wire. Then call `replace_wire_destination` (D10) on each moved wire, in
     order. It removes the wire from the old slot, appends it to the new one
     with its new depth, and retargets the comment anchors on it when the
     storage scope is unchanged. Like `replace_wire_source`, it finds those
     anchors before it changes anything;
   - **output side**: `replace_wire_source` on every consumer.
3. Drop the moved wires from `selected_wires`, and `drop_dangling_anchors` in
   every touched scope.
4. Run the passes a connect runs after it adds a wire: record-reader
   adoption (`connect_adoption` / `install_connect_adoption`) for each new
   wire, `rederive_node_type` where a derived pin layout depends on the wire
   (`apply`, `map`), footprint reflow for bodies whose owner's footprint
   changed, `mark_node_data_changed_scoped` for every old and new
   destination, `apply_node_display_policy` over the touched top-level nodes,
   `validate_active_network`, `set_dirty`, `mark_full_refresh`. Today these
   passes are spread across `connect_nodes`, `connect_nodes_scoped` and
   `connect_wire_scoped`. Factor them into one helper that those paths and
   this one share; do not add a fourth copy.
5. Push the `NetworkSnapshotCommand` (D8).

### API

In `rust/src/api/structure_designer/structure_designer_api.rs`, following the
dissolve pair:

- `pin_wires(pin: APIPinAddress) -> Vec<APIWireEnd>`: the wires a pick-up
  would hold. Each one carries the wire's identity (the fields the painters
  key wires by, so they can hide it) and its **fixed** end (the pin that
  stays where it is, which the rubber band is drawn from). It is read-only
  and called once per drag start. Its length is also the "is this pin
  connected" answer, so Flutter never re-derives a pin's consumers across
  scopes.
- `move_wires_refusal(from, to) -> Option<String>`: the plan as a gate.
  `None` means the target accepts. The text is not shown (see above).
- `move_wires(from, to) -> Option<String>`: the commit, then
  `refresh_structure_designer_auto`.

`APIPinAddress` and `APIWireEnd` are api twins with `From` impls, per
`rust/AGENTS.md`.

## Flutter: the drag

### Starting

`PinWidget` (`node_network/node_widget.dart`) keeps its single `Draggable`.
In `onDragStarted` it calls `sd_api.pinWires` and decides the mode with a
pure function:

```dart
enum WireDragMode { newWire, move }

WireDragMode wireDragMode({
  required bool isOutput,
  required bool multi,
  required int wireCount,
  required bool ctrl,
}) {
  if (wireCount == 0) return WireDragMode.newWire;
  if (ctrl) return WireDragMode.move;
  if (!isOutput && !multi) return WireDragMode.move;
  return WireDragMode.newWire;
}
```

`ctrl` is `HardwareKeyboard.instance.isControlPressed`, read once, at the
start. Pressing or releasing Ctrl mid-drag does not change the mode.

A `newWire` drag is today's path, unchanged. A `move` drag sets
`model.wireMove`, a `WireMove { from, ends, cursor, cancelled }`, through a
`beginWireMove` that is gated on `activeNetworkReadOnly` like `dragWire`. It
leaves `draggedWire` null, so the open-port tabs (which listen to
`wireDragSource`) stay hidden and `onDragEnd` does not open the Add Node
popup.

### During

- `onDragUpdate` moves `wireMove.cursor` and bumps `dragRepaint`. That is the
  drag fast path (`node_network/AGENTS.md` → Performance invariants): no
  `notifyListeners`.
- The painters skip the held wires, matched by key against `wireMove`, and
  draw one rubber band from each fixed end to the cursor, in the wire's own
  colour.
- Each pin's `DragTarget.onWillAcceptWithDetails` asks
  `model.canMoveWiresTo(pin)` while a move is on. The answer comes from a
  `PerDragPlanMemo` (`splice_drop_memo.dart`) keyed by the target pin, so
  Rust is asked once per pin per drag. A rejected target just does not
  accept, like an incompatible pin during a new-wire drag.
- **Escape** sets `wireMove.cancelled`. The rubber bands disappear, the held
  wires are drawn again, and the release does nothing (D7). Flutter's
  `Draggable` cannot be cancelled from outside, so the flag is what the drop
  path reads.

### Ending

- Dropping on an accepting pin calls `model.moveWires(target)` →
  `sd_api.moveWires` → `refreshFromKernel`. The one full notify of the
  gesture happens here.
- Any other end (empty space, an invalid target, Escape, the starting pin)
  clears `wireMove` and changes nothing.

## Flutter: edge auto-pan

Independent of the move gesture: nothing here depends on `wireMove` beyond
listing it among the drags that pan.

### Where the pointer comes from

The canvas's `Listener` (`NodeNetworkState._handlePointerMove`) sees every
pointer move of a drag that started on the canvas, including moves past its
bounds, because Flutter routes a pointer's moves to whatever it hit-tested at
pointer-down. Wire drags (`Draggable` on a pin), node drags (`GestureDetector`
on the node) and rectangle selection all start on the canvas, so one listener
covers them. That is also why panel drags are out of scope.

`_handlePointerMove` records the last canvas-local pointer position while one
of the D12 drags is active. Whether one is active is read from state the model
and the canvas already hold: `draggedWire`, `wireMove`, whether a node drag is
in progress, and `_selectionRectStart`. The model already tracks the dragged
node ids for the drag fast path (`_draggedNodeIds`); it needs a public getter
for this.

### The speed

A pure function, so it can be unit-tested:

```dart
/// Pan velocity, in screen px/s, for a pointer at [pointer] (canvas-local)
/// on a canvas of [size]. It points toward the edge the pointer is at, and
/// is zero outside the edge band.
Offset edgePanVelocity(Offset pointer, Size size);
```

Per axis: zero while the pointer is more than `EDGE_PAN_BAND` (≈ 30 px) inside
the edge, rising linearly to `EDGE_PAN_MAX_SPEED` at the edge and beyond,
reached a little past it. The constants live beside the other canvas
constants. The values are tuned in the walkthrough, not by the tests.

### The ticker

`NodeNetworkState` gains a `Ticker` (`SingleTickerProviderStateMixin`), which
runs only while a D12 drag is active **and** the velocity is non-zero. It
starts from `_handlePointerMove` and stops when the velocity drops to zero or
the drag ends. Each tick:

1. `logicalStep = velocity * dt / scale`.
2. `setState` sets `_panOffset -= logicalStep`. The content slides away from
   the edge the pointer is at, and what lies beyond that edge comes into
   view. This is the same full-canvas rebuild a middle-mouse pan already
   does on every pointer move, so the cost is existing cost, and the rebuild
   redraws wires and rubber bands against the moved pins.
3. D13: a node drag gets `+logicalStep` pushed into the dragged nodes,
   through the existing model drag methods. Those apply
   `_clampZoneBodyDragDelta`, so a body node still cannot be pushed out of
   its body. A selection rectangle's start corner moves with the content (by
   `-logicalStep * scale` in screen space), and the rectangle is recomputed
   from it and the last pointer position.

### The drop

D14: when a pan happened since the last pointer move, every wire release first
compares the pin `Draggable` chose (none, for a drop in empty space) with
`resolver.findPinAtScreenPosition(releasePosition)`. That is a small new
helper over the resolver's existing pin positions. On a mismatch the release
does nothing. This covers all three paths: `PinWidget.onAcceptWithDetails`,
the empty-space drop in `onDragEnd`, and Phase 2's move drop.

D15: when a drag that panned ends, `_syncCanvasViewportToKernel` runs once.

## Phases

### Phase 1: Rust move operation

- `wire_move_ops.rs`: `PinAddress`, `pin_wires`, `plan_move_wires`,
  `move_wires`, `WireMoveRefusal` with its `Display`.
- `replace_wire_destination` beside `replace_wire_source`, and the shared
  post-connect helper (step 4 of the commit), with the existing connect
  paths moved onto it.
- `collect_consumers` generalised to one pin, with `ZoneInput` sources, and
  dissolve moved onto the shared version.
- The three FRB functions and their api twins; codegen.

Tests, in `atomcad-structure-designer/tests/structure_designer/move_wires_test.rs`
(registered in `tests/structure_designer.rs`), reusing the no-mutation,
round-trip and invariants oracles of `splice_support.rs`:

- each source/target combination: a single input to a single input; an array input with three wires to an empty array input
  (order kept), to an array input that already has wires (appended after
  them), and to a single input (`TooManyWires`); an output with consumers
  in two scopes (top level and a `map` body capture) to another output;
  the function pin's consumers; a zone-input pin's readers inside its own
  body;
- the displaced wire on a connected single-input target, with its anchor
  dropped;
- every refusal variant, each with the no-mutation oracle. `OutOfScope`
  includes a zone input moved out of its body, and a top-level output's
  consumers moved to a body node's output; `DuplicateSource` covers both
  sides;
- two outputs of one node (`atom_edit` `result` → `diff`): accepted, not a
  `DuplicateSource`;
- comment anchors: a note on a moved wire follows it on both sides; a wire
  moved into another scope loses it;
- adoption: moving a record wire into an anonymous record reader adopts as a
  connect would;
- one undo step: undo restores the pre-move network, redo the post-move one,
  and the undo stack grows by exactly one;
- the refusal wording, once, in a `Display` table test.

`move_wires` joins the mutation alphabet in `library_links_readonly_test.rs`.
The existing dissolve and connect tests must stay green unchanged, as the net
for the two refactors.

### Phase 2: Flutter drag

- `wireDragMode` in its own small file under `node_network/`, unit-tested in
  `test/wire_drag_mode_test.dart` (every cell of the rule table).
- `WireMove` state on the model, `beginWireMove` / `updateWireMove` /
  `cancelWireMove` / `moveWires` / `canMoveWiresTo`, the memo, the Escape
  handling, and the painters (hidden wires and rubber bands).
- `PinWidget`: mode decision at drag start, acceptance and drop routed by
  mode.
- Reference guide: `doc/reference_guide/ui.md`, the wire paragraph next to
  "Left-click and drag from an output pin to an input pin", gains the rule
  table in prose and a line on Escape. `node_networks.md` needs nothing.
- `node_network/AGENTS.md`: the interaction-model list gains the move
  gesture, and a short note that a move is a separate drag state from
  `draggedWire`, which is why the open-port tabs and the empty-space popup
  stay out of it.

### Phase 3: edge auto-pan

Independent of Phases 1–2: it can land first, and then covers new-wire drags,
node drags and rectangle selection. Phase 2 adds the move drag to its list
whenever it lands.

- `edgePanVelocity` and its constants, unit-tested in
  `test/edge_pan_test.dart`: zero in the interior; each edge and each corner
  (both axes at once); the cap is reached and held past the edge; symmetric
  for opposite edges.
- The ticker, the per-drag compensation (D13), the release check (D14) and
  the viewport sync (D15) in `NodeNetworkState`.
- Reference guide: `doc/reference_guide/ui.md`, the canvas navigation
  section, gains one sentence, along the lines of "while dragging a wire, a
  node or a selection rectangle, hold the pointer near the canvas edge to
  pan".
- `node_network/AGENTS.md`: the interaction-model list gains edge auto-pan,
  and the performance-invariants section notes that the ticker's `setState`
  is the one deliberate per-frame rebuild during a drag, and why (the whole
  canvas moves).

### Phase 4: manual walkthrough

Human-only, per `AGENTS.md`:

- a wire from a far-away node into B: drag B's input onto C's input. The
  wire now ends at C and B is unconnected, without ever scrolling to the
  far node. One Ctrl+Z restores it;
- a plain drag from a connected single input onto an incompatible pin: the
  pin does not take it, and the release changes nothing;
- `union` with three wires: a plain drag adds a fourth; Ctrl+drag onto
  another `union`'s array input moves all three, in order; Ctrl+drag onto a
  single input is not accepted;
- Ctrl+drag a node's output onto an alternative node's output: every
  consumer, including capture wires into a body, now reads the alternative;
- Escape mid-drag, a drop in empty space, and a drop back on the starting
  pin: nothing changes, and no Add Node popup appears;
- a comment anchored to a moved wire still points at it;
- a linked (read-only) network: Ctrl+drag does nothing;
- the canvas stays smooth during the drag in a 100-node network;
- edge auto-pan: drag a new wire toward each edge and each corner. The canvas
  pans, faster the deeper the pointer goes, and stops when the pointer comes
  back inside. Connect to a pin that was off screen when the drag began. Do
  the same with a moved wire, a dragged node (it stays under the cursor, and
  inside its body for a body node) and a selection rectangle (its start
  corner stays on the content it started on). Release while panning with a
  still pointer: nothing connects to a pin that has scrolled away. At each
  zoom level the speed feels the same. After switching networks and back,
  the view is where the drag left it. Tune the band and speed constants
  here;
- run the Flutter smoke test (`flutter test integration_test/`).

## Testing

The operation is tested at the domain level in `atomcad-structure-designer`'s
`structure_designer` harness, with refusals asserted as `WireMoveRefusal`
variants, never as strings. The FRB wrappers are thin and untested, per
`rust/AGENTS.md`. The Flutter side's two pieces of pure logic, the mode
decision and the edge-pan velocity, are Dart unit tests. Everything else (the
rubber bands, target acceptance, Escape, the ticker, compensation,
smoothness, the tuning) is the Phase 4 walkthrough.

## Open questions

None.
