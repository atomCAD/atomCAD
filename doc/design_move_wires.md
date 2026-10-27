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
  there could only replace the one already in it.
- **A wire held by its input end can be dropped on an output as well as on
  an input.** On an input, the wire moves there (its source stays). On an
  output, the wire stays on its input and that output becomes its source.
  This is what a drag from a connected input onto an output does today, so
  that gesture keeps working (D16).
- Ctrl+drag on an unconnected pin is a plain drag.

| Pin | Plain drag | Ctrl+drag |
|---|---|---|
| unconnected (any) | new wire | new wire |
| single input, connected | **move its wire** (onto an input), or **re-source it** (onto an output) | same as plain |
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
- what a drag past the canvas edge looks like and does: the canvas clips its
  painting at its bounds, and a wire released outside the canvas does nothing
  (D18, D19);
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
| D1 | **The gesture rule is the table above.** No Alt variant, and no right-drag. | One rule, one modifier. A plain drag from a connected single input did two things today. A drop on an output replaced the wire's source, and D16 keeps that. A drop in empty space created a replacement upstream node; that one is given up, because it is rare and is a delete and a drag by hand. Right-press on a node opens its context menu, Shift+right-drag pans, and Flutter's `Draggable` answers only the primary button. |
| D2 | **A move takes all of the pin's wires, or none.** Every wire must be valid at the new pin, or nothing changes and the target does not accept the drop. | A move that silently leaves some wires behind is worse than one that refuses. |
| D3 | **A moved wire is judged like a fresh connect to the new pin.** It uses the same type rule (`DataType::can_be_converted_to`), the same scope rule (`doc/design_zones_ui.md` §"Computing source_scope_depth at wire creation"), and the same zone-input rule (today enforced in Flutter's `_zoneInputSourceInScope`). The only additions are the wire counts of D4 and D16 and the duplicate rule of D6, which a single new wire cannot hit. Cycles are not refused, because a plain connect does not refuse them either: validation flags them. | The user should not have to learn a second set of rules. |
| D4 | **Input-side targets.** A single input accepts a move of exactly one wire. If it already holds a wire, that wire is displaced, as on any drop onto a connected single input. An array input appends the moved wires after its own, in their original order. | Array pins concatenate in order (`doc/design_insert_node_on_wire.md` D5). Appending keeps the target's existing order, and the moved wires keep theirs. |
| D5 | **Output-side moves re-source every consumer.** Each wire keeps its slot, and its position in that slot, and only its source changes, through `StructureDesigner::replace_wire_source`. | The consumers' arguments, including array order, are what the user built. A move changes only where the value comes from. |
| D6 | **A move never merges or drops a wire it holds.** Only two kinds of wire can disappear. One is the wire a single input displaces (D4). The others are wires the old destination drops when it loses the moved wire, exactly as it would on a delete (D17). When a target slot already holds a wire from the moved wire's source node, the move is refused (`DuplicateSource`), not merged. | `Argument::incoming_wires` holds at most one wire per source node. A plain connect merges into the existing wire. For a move, merging would delete a wire the user is holding, which a move must never do. Dissolve refuses the same case the same way. |
| D7 | **A move dropped in empty space, or on the starting pin, does nothing.** So does Escape during the drag, or a release over an invalid target. (A *new* wire dropped in empty space still opens the Add Node popup, as today, unless D14 or D19 applies.) | A missed drop is a common outcome, as `node_network/AGENTS.md` notes for comment anchors, and a miss must not delete wires. The Add Node popup does not fit a drag that can hold several wires. |
| D8 | **One Rust command, plan then commit, one undo step.** `plan_move_wires` is read-only. `move_wires` re-plans, refuses if the plan refuses, and records one `NetworkSnapshotCommand` over the top-level network, labelled "Move wire" or "Move wires". | One snapshot covers wires in any body, displaced wires, comment anchors and adoption. A delete followed by a connect would be two undo steps and two refreshes, and the first would leave a half-done network if the second were refused. |
| D9 | **Rust owns every rule.** Unlike `can_connect_wire_scoped`, the plan checks the scope and zone-input rules itself rather than leaving them to Flutter. Flutter decides only the gesture (D1). | The plan must be authoritative, or a target can accept the drop and the commit refuse it. |
| D10 | **Comment anchors follow the wires.** Output-side: `replace_wire_source` already retargets them (`doc/design_insert_node_on_wire.md` D11). Input-side: a new sibling primitive, `replace_wire_destination`, moves one wire to a new slot. It also rewrites the destination fields of the comment anchors on that wire, provided the wire stays in the same storage scope. A wire moved into another scope loses its anchor, because the comment cannot come with it. A displaced wire's anchors are dropped by `drop_dangling_anchors`. | A note about a wire should still be about that wire after a move. Putting the rewrite in the primitive means no caller can forget it. |
| D11 | **Read-only (linked) networks: the pick-up is inert**, like every other canvas edit gesture (`lib/structure_designer/AGENTS.md` → Linked libraries). Rust refuses too. | — |
| D12 | **Edge auto-pan runs during wire drags (new and moved), node drags and rectangle selection**, and only while the pointer is inside an edge band of the canvas or past its edge. Its speed grows with how deep the pointer is in the band, and with how far past the edge it is, up to a cap. It is in screen pixels per second, so it feels the same at every zoom level. | These are the drags whose target can be off screen. A band rather than a hard edge lets the user control the speed. Past the edge has to count too, or a fast pointer flies over the band and stops panning. |
| D13 | **The dragged thing stays under the cursor while the canvas pans.** Each pan step moves dragged nodes by the same logical amount (through `dragSelectedNodes` / `dragNodePosition`) and shifts a selection rectangle's start corner by the same screen amount. A wire's free end needs nothing, because it is stored in canvas-local pixels. | Without it, a node dragged into the band slides away from the pointer and the selection rectangle stretches the wrong way. |
| D14 | **A wire release never acts on a pin that is no longer under the cursor.** If the canvas panned since the last pointer move, the release re-runs `Draggable`'s own hit test at the release position (see "The drop"). If the pin it finds differs from the one `Draggable` last chose (either one may be "no pin"), the release does nothing: no connect, no move, no Add Node popup. | `Draggable` re-checks its targets only on pointer moves. A release after panning with a still pointer would otherwise connect to the pin that *was* under the cursor before the pan, which may be off screen by now. The re-check must be the *same* hit test. A second geometry, such as the resolver's pin centre points, disagrees at the edge of a pin's hit box and at the zoom levels where pins are hidden. A legitimate drop would then be dropped silently. |
| D15 | **Auto-pan is view state.** It is not undoable. The viewport is persisted once, when the drag ends (`_syncCanvasViewportToKernel`), as after a manual pan. | Same status as every other pan and zoom. |
| D16 | **A move of exactly one wire held by its input end may drop on an output: that output becomes the wire's source.** The wire stays in its slot, at its position, with its anchors, through `replace_wire_source`. It is judged like a fresh connect between that output and the input (D3). More than one wire (Ctrl+drag on an array input) is `TooManyWires`, because the slot holds at most one wire per source node (D6). A move held by its output end may not drop on an input: `DirectionMismatch`. | Today a drag from a connected input onto an output re-sources that input. A move that refused this drop would take that gesture away from the end the user can see, which is the case this doc exists for. The rule is "the pin you drop on replaces the end you are holding, or the far end if you drop on the other kind of pin", and only the second half needs exactly one wire. |
| D17 | **A move is a disconnect where it leaves and a connect where it lands, and the commit runs the passes for both.** The old destination (input side), or the old source (output side), gets what a delete of the moved wire would do to it. An HOF whose `f` loses its wire gets its inline body back, with the footprint reflow `delete_selected_scoped` runs. An `apply` or `map` whose `f` loses its wire re-derives its layout, and repair drops the arg wires that no longer fit, as on a delete. A node whose `-1` pin loses its last consumer leaves function mode. These are not refused. | Leaving a wire is a disconnect whatever the gesture. Refusing these moves would make the move gesture stricter than a delete and redraw, which is what it replaces. Predicting repair's drops in the plan would mean a second implementation of repair. The snapshot makes the whole move one undo step, so nothing is lost for good. |
| D18 | **The live canvas clips everything it paints at its own bounds.** A `ClipRect` goes around the canvas `Stack` in `NodeNetworkState.build`. A dragged wire's free end stays at the real pointer (D13), so past the edge the wire leaves the canvas heading toward the pointer and is cut off at the border. It is not drawn over the viewport or the panels, and its end is not clamped to the boundary. The clip also covers the rubber bands of a move, the comment-anchor rubber band, wire hover glows near the edge, and the open-port tabs. The image export is unaffected: it lays out at the full content size, so nothing is outside its bounds. | Today the wire painters overflow the canvas. A `Stack` clips only children whose *layout* overflows, and the painters' `CustomPaint` is canvas-sized and simply paints outside it. Nodes, which do overflow in layout, are already clipped, so this only makes painting agree with them. Clipping, not clamping, is how node editors behave, because the editor region clips its own drawing. A clamped end would bend the curve into a different shape, and would put a convincing wire end on a boundary point where a release does nothing (D19). |
| D19 | **A wire release with the pointer outside the canvas does nothing**, for new and moved wires alike: no connect, no move, no Add Node popup. | Pin targets exist only inside the canvas, so the one thing such a release can do today is open the Add Node popup for an off-canvas drop position. That places a node out of sight. With D12, holding the pointer past the edge is a pan gesture, and letting go there means the user did not find a target, so it must behave like Escape. |

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

The domain query behind the `pin_wires` API (below), which the commit also
uses:

```rust
pub struct HeldWire {
    /// Where the wire is stored, and the wire, as the splice operations
    /// address it.
    pub storage_scope: Vec<u64>,
    pub anchor: WireAnchor,
    /// The end that stays put: the source for an input-side pick-up, the
    /// consumer's input for an output-side one.
    pub fixed_end: PinAddress,
}

pub fn pin_wires(&self, pin: &PinAddress) -> Vec<HeldWire>;
```

It is empty for an unconnected pin, and for a pin that does not exist (the
plan, not this query, reports `PinNotFound`). An input's wires come in slot
order. An output's come in a stable order: scope depth first, then
destination node id, then slot.

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
    /// Wires held by an output end, dropped on an input (D16).
    DirectionMismatch,
    /// `from == to`.
    SamePin,
    /// More than one wire, dropped on a single input (D4) or, held by their
    /// input end, on an output (D16).
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
  be a prefix too. The new depth is the length of the new effective scope
  minus the length of the source's scope.
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
- **Input → output (D16).** Exactly one wire. This is the output → output
  rule applied to that one wire: the wire's destination is the consumer and
  the drop target is the new source. Because the slot holds just the moved
  wire's source, `DuplicateSource` can arise only on an array input, from
  another wire in the slot.

### The commit

```rust
pub fn move_wires(&mut self, from: &PinAddress, to: &PinAddress)
    -> Result<(), WireMoveRefusal>;
```

1. Re-plan, and return the refusal if it refuses. Then snapshot the
   top-level network for undo, and capture the pre-edit footprints
   (`capture_footprint_chain`) of every node whose layout the move can
   change: old and new destinations, and on the output side the old and new
   sources. The capture must come before step 2, because moving a wire can
   already change a footprint (an HOF whose `f` loses its wire).
2. Move the wires:
   - **input side**: if the target is a single input, remove its displaced
     wire. Then call `replace_wire_destination` (D10) on each moved wire, in
     order. It removes the wire from the old slot, appends it to the new one
     with its new depth, and retargets the comment anchors on it when the
     storage scope is unchanged. Like `replace_wire_source`, it finds those
     anchors before it changes anything;
   - **output side**, and **input → output** (D16): `replace_wire_source` on
     every consumer, or on the one held wire.
3. Drop the moved wires from `selected_wires`.
4. Run the passes for both ends of the move (D17), reflowing against the
   footprints captured in step 1.
   - **Where wires leave** (old destinations; on the output side, the old
     source): the passes a delete of the wire runs. That is
     `rederive_node_type` where a derived layout depended on it (`apply`,
     `map`, and the repair that follows it). For an HOF whose `f` was wired,
     the inline body comes back, with its footprint reflow. A `-1` source
     whose last consumer moved away leaves function mode.
   - **Where wires land**: the passes a connect runs. That is record-reader
     adoption (`connect_adoption` / `install_connect_adoption`) for each new
     wire, `rederive_node_type` where a derived layout depends on it, and
     footprint reflow where it grew.
   - **Then, once**: `drop_dangling_anchors` in every touched scope. This
     runs after the repairs above, because those can drop wires that carry
     anchors (D17). Then `mark_node_data_changed_scoped` for every old and new
     destination, `apply_node_display_policy` over the touched top-level
     nodes, `validate_active_network`, `set_dirty`, `mark_full_refresh`.

   Today the connect half is spread across `connect_nodes`,
   `connect_nodes_scoped` and `connect_wire_scoped`, and the disconnect half
   sits inside `delete_selected[_scoped]`'s wire branch. Factor each half into
   one helper that those paths and this one share; do not add another copy.
   That refactor lands first, on its own (Phase 1a).
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

`APIPinAddress` and `APIWireEnd` are the api twins of `PinAddress` and
`HeldWire`, with `From` impls, per `rust/AGENTS.md`.

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
`model.wireMove`, a `WireMove { from, ends, cursor, hoveredTarget, cancelled }`,
through a `beginWireMove` that is gated on `activeNetworkReadOnly` like
`dragWire`. `ends` is the `pinWires` answer. It
leaves `draggedWire` null, so the open-port tabs (which listen to
`wireDragSource`) stay hidden and `onDragEnd` does not open the Add Node
popup. `beginWireMove` also requests the canvas's focus node, so that the
Escape below reaches the canvas's key handler. It does not rely on the
canvas's tap-down focus grab, which `Draggable` may beat in the gesture
arena.

### During

- In move mode, `onDragUpdate` calls `updateWireMove` instead of
  `dragWire`. That moves `wireMove.cursor` and bumps `dragRepaint`, which is
  the drag fast path (`node_network/AGENTS.md` → Performance invariants): no
  `notifyListeners`.
- The painters skip the held wires, matched by key against `wireMove`, and
  draw one rubber band from each fixed end to the cursor, in the wire's own
  colour. There is one exception. While the cursor is over an **output**
  that accepts a move held by its input end (D16), the painter draws the
  wire as it will be: from that output to the starting input. The
  rubber band would show the old source, which is the end that is about to
  change. `wireMove.hoveredTarget` records that output. The target's
  `onWillAcceptWithDetails` / `onLeave` set it, and it repaints through
  `dragRepaint`.
- Each pin's `DragTarget.onWillAcceptWithDetails` asks
  `model.canMoveWiresTo(pin)` while a move is on. The answer comes from a
  `PerDragPlanMemo` (`splice_drop_memo.dart`) keyed by the target pin's
  full identity: scope chain, node id, pin kind and pin index. Body node ids
  repeat across scopes, so a key on the node id alone would give one pin
  another pin's answer. Rust is asked once per pin per drag. A rejected
  target just does not accept, like an incompatible pin during a new-wire
  drag. Once the move is cancelled (below), `canMoveWiresTo` answers false
  for every pin, so no target highlights.
- **Escape**, handled by the canvas's key handler ahead of its other Escape
  uses, sets `wireMove.cancelled`. The rubber bands disappear, the held
  wires are drawn again, and the release does nothing (D7). Flutter's
  `Draggable` cannot be cancelled from outside, so the flag is what the drop
  path reads.

### Ending

- In move mode, `PinWidget.onAcceptWithDetails` calls
  `model.moveWires(target)` instead of `connectPins`. That goes to
  `sd_api.moveWires` and then `refreshFromKernel`, and the gesture's one full
  notify happens here. If Phase 3 has landed, the release check
  (`releaseDecision`, see "The drop") runs first, as on the other release
  paths.
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

A wire drag ends on one of three release paths:
`PinWidget.onAcceptWithDetails` (a new wire dropped on a pin), the
empty-space drop in `onDragEnd` (which opens the Add Node popup), and Phase
2's move drop. Each one first asks
`releaseDecision(insideCanvas, pannedSinceMove, chosenPin, pinUnderPointer)`
and does nothing when told to ignore the release. The function applies two
rules, in this order:

- D19: a release whose pointer is outside the canvas's bounds is ignored.
  `_handlePointerMove` records the last canvas-local pointer position
  (D12), and "outside" means that position is not in
  `Offset.zero & canvasSize`.
- D14: if a pan happened since the last pointer move, the pin `Draggable`
  chose (none, for a drop in empty space) is compared with the pin under
  the release position, and the release is ignored on a mismatch.

The pin under the release position is found **with the hit test `Draggable`
itself uses**, not with the resolver's geometry. Flutter's drag avatar runs
`WidgetsBinding.instance.hitTestInView` at the pointer and takes the
`RenderMetaData` entries that `DragTarget`s put in the tree.
`pinUnderPointer(globalPosition)` does the same. Each `PinWidget` wraps its
`DragTarget` in a `MetaData(metaData: pinReference)`, and the helper returns
the first `PinReference` it finds among the hit-test entries. The answer then
follows the real hit box (`PIN_HIT_AREA_WIDTH × PIN_HIT_AREA_HEIGHT`). At a
zoom level where pins are not built it returns no pin, and nothing has to be
kept in sync by hand. The hit test sees the layout of the last pan frame,
because the release arrives as a pointer event between frames, after that
frame was laid out.

D15: when a drag that panned ends, `_syncCanvasViewportToKernel` runs once.

## Phases

### Phase 1a: shared helpers (refactor only)

No behaviour changes. This lands and is committed before Phase 1b, so that a
regression in the refactor cannot hide behind the new feature.

- The connect-side and disconnect-side helpers (step 4 of the commit).
  `connect_nodes`, `connect_nodes_scoped` and `connect_wire_scoped` move onto
  the first, and `delete_selected[_scoped]`'s wire branch onto the second.
- `collect_consumers` generalised to one pin, with `ZoneInput` sources, and
  dissolve moved onto the shared version.

The net is the existing connect, delete, dissolve, reflow and undo tests,
which must stay green **unchanged**. The `map.f` leg of the disconnect half
is already pinned down: `reflow_test.rs`'s
`delete_f_wire_expands_map_and_reflows_single_step_undo`, and
`currying_test.rs`'s `map_phase4_f_disconnect_restores_stored_output_type`.
Two legs have no test yet. Write them first, in the files that own them, and
check they pass **before** the refactor starts:

- `currying_test.rs`: delete the wire into `apply.f` while its arg pins are
  wired. The layout goes back to the bare `[f]` default, so no arg wire
  survives, and one undo restores them all. (Writing this test found that
  the layout used to outlive the wire until the next reload, and that the
  wire commands could not undo the dropped arg wires. Both were fixed, with
  the same undo gap on a lower-arity rewire of `f`, before the refactor.)
- `function_pin_test.rs`: delete the last consumer of a `-1` pin. The source
  leaves function mode (`function_pin_consumed` is false), and its errors and
  layout follow.

### Phase 1b: Rust move operation

- `wire_move_ops.rs`: `PinAddress`, `HeldWire` and `pin_wires`,
  `plan_move_wires`, `move_wires`, and `WireMoveRefusal` with its `Display`.
- `replace_wire_destination` beside `replace_wire_source`.
- The three FRB functions and their api twins; codegen.

Tests, in `atomcad-structure-designer/tests/structure_designer/move_wires_test.rs`
(registered in `tests/structure_designer.rs`). They reuse the oracles of
`splice_support.rs`. Every plan and every refusal runs under
`assert_no_mutation`. Every commit runs under `assert_one_step_round_trip`,
which also checks that it is one undo step, that undo and redo restore the
networks, and the invariants. The bullets below therefore state only what
each case adds.

- `pin_wires`: none for an unconnected pin. A single input gives its one
  wire, and an array input gives its wires in slot order. An output gives
  every consumer, including body captures two scopes down and a zone input's
  readers, and nothing from a sibling output of the same node. Each entry's
  fixed end is the pin that stays put. Flutter takes its mode decision and
  its rubber bands from this answer alone, so a wrong count is a wrong
  gesture;
- **plan/commit agreement**, a sweep. The fixture holds one of each pin
  kind: top level and in a `map` body, single and array inputs, a
  multi-output node, a `-1` pin with a consumer, zone pins, and an
  unconnected pin. For **every** ordered pair of pins in it, `plan_move_wires`
  and `move_wires` must agree, on a fresh copy of the fixture each time: `Ok`
  commits under the round-trip oracle, and `Err` gives the same refusal with
  no mutation. This is D9's promise that the plan is authoritative, so a
  target that accepts the drop never has the commit refuse it. The
  hand-written cases below cannot enumerate it. Keep the fixture small
  (around ten nodes), because the sweep is quadratic in pins;
- each source/target combination:
  - a single input to a single input;
  - an array input with three wires to an empty array input (order kept),
    to an array input that already has wires (appended after them), and to
    a single input (`TooManyWires`);
  - an output with consumers in two scopes (top level and a `map` body
    capture) to another output;
  - the function pin's consumers to another node's function pin;
  - a zone-input pin's readers, inside its own body, to a body node's
    output;
- input → output (D16): a connected single input dropped on another
  output re-sources its wire in place, keeping its anchor. An array input
  holding one wire does the same, and its other slots are untouched. An
  array input with three wires gives `TooManyWires`, and an output dropped
  on an input gives `DirectionMismatch`;
- the disconnect side (D17), checked against a new **equivalence oracle**,
  `assert_same_networks(a, b)` in `splice_support.rs`. It compares
  `snapshot_all_networks` of two designers with `json_diff`. Both designers
  are built by the same fixture function. On one the wire is moved; on the
  other it is deleted (`delete_selected_scoped`) and then connected at the
  target. The two must be identical, node positions included. Cases: a wire
  moved off `map.f` (the body is back and the neighbours are reflowed), off
  `apply.f` with `arg0` wired, and the last consumer moved off a `-1` pin.
  The oracle holds only for **un-anchored** wires into **single-slot**
  destinations. A move deliberately differs from delete-and-connect in two
  ways: it keeps the wire's anchors (D10) and its position within an array
  slot (D5). The cases are chosen inside that domain; it is not a general
  rule;
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
- the refusal wording, once, in a `Display` table test.

`move_wires` joins the mutation alphabet in `library_links_readonly_test.rs`.

### Phase 2: Flutter drag

- `wireDragMode` in its own small file under `node_network/`, unit-tested in
  `test/wire_drag_mode_test.dart` (every cell of the rule table).
- Beside it, three more pure functions, unit-tested in
  `test/wire_move_test.dart` over hand-built API data classes, with no Rust
  library:
  - `pinKey(PinReference)`, the memo key. Two pins that differ only in scope
    chain, only in pin kind, or only in index get different keys.
  - `heldWireKey(APIWireEnd)`, which the painters match against to hide the
    held wires. For the same wire it must equal `WireHitResult.keyOf`, for a
    top-level wire, a body wire and a body-return wire. A drift between the
    two leaves the held wire drawn twice, or hides a different one.
  - `wireMoveSegments(move, pinPosition)`: what the painters draw. With no
    hover, there is one segment per held wire, from its fixed end to the
    cursor. Hovering an accepting output (D16) gives one segment from that
    output to the starting pin. A cancelled move gives none.

  The painters call these and add no logic of their own, so what is left
  for the walkthrough is only how the result looks.
- `WireMove` state on the model, `beginWireMove` / `updateWireMove` /
  `cancelWireMove` / `moveWires` / `canMoveWiresTo`, the memo, the Escape
  handling, and the painters (hidden wires and rubber bands).
- `PinWidget`: mode decision at drag start, acceptance and drop routed by
  mode.
- Reference guide: `doc/reference_guide/ui.md`, the wire paragraph next to
  "Left-click and drag from an output pin to an input pin", gains the rule
  table in prose. That includes dropping a wire held by its input end on an
  output (D16) and a line on Escape. `node_networks.md` needs nothing.
- `node_network/AGENTS.md`: the interaction-model list gains the move
  gesture, and a short note that a move is a separate drag state from
  `draggedWire`, which is why the open-port tabs and the empty-space popup
  stay out of it.

### Phase 3: edge auto-pan

Independent of Phases 1a–2: it can land first, and then covers new-wire drags,
node drags and rectangle selection. Phase 2 adds the move drag to its list
whenever it lands.

- `edgePanVelocity` and its constants, unit-tested in
  `test/edge_pan_test.dart`: zero in the interior; each edge and each corner
  (both axes at once); the cap is reached and held past the edge; symmetric
  for opposite edges.
- `edgePanStep(velocity, dt, scale)`, in the same file and the same test.
  It returns the logical pan step, the step pushed into the dragged nodes
  (D13), and the screen shift of a selection rectangle's start corner. The
  test pins the signs and the scale. At the right edge the content moves
  left, the dragged node moves right by the same logical amount, and the
  rectangle's start moves left in screen pixels. At scale 0.35 the logical
  step is 1/0.35 of the screen step. A sign error here is the likely bug,
  and it is invisible to the walkthrough at one edge.
- `releaseDecision(insideCanvas, pannedSinceMove, chosenPin,
  pinUnderPointer)`, the D19 and D14 rules as one pure function,
  unit-tested over its cases. Outside the canvas it always ignores the
  release, pan or no pan. Inside, with no pan, it proceeds. After a pan it
  proceeds if the two pins are the same, and ignores the release if they
  differ, counting "no pin" as a value on either side.
- The canvas clip (D18): a `ClipRect` around the live canvas `Stack`. This
  is a one-line change, independent of auto-pan, and can land first. It has
  no automated test: it is a paint property, and its check is the
  walkthrough's.
- `pinUnderPointer`, widget-tested in `test/pin_under_pointer_test.dart`
  (a `flutter test` widget test, not `integration_test/`). It pumps
  `MetaData`-wrapped hit boxes under a scaled, translated `Stack`, as on the
  canvas. A point just inside each corner of a box finds its pin. A point
  just outside finds none. A box that is not built (far zoom) is never
  found. A real `DragTarget` inside the wrapper does not confuse it: the
  helper skips the `DragTarget`'s own metadata entry and returns the
  `PinReference`.
- The ticker, the per-drag compensation (D13), the release check (D14) and
  the viewport sync (D15) in `NodeNetworkState`, built on the functions
  above. The release check adds the `MetaData` wrapper on `PinWidget`'s
  `DragTarget`.
- Reference guide: `doc/reference_guide/ui.md`, the canvas navigation
  section, gains one sentence, along the lines of "while dragging a wire, a
  node or a selection rectangle, hold the pointer near the canvas edge to
  pan".
- `node_network/AGENTS.md`: the interaction-model list gains edge auto-pan
  and the D19 release rule. A line under "The canvas is rendered twice"
  notes that the live canvas clips its painting (D18) and the export does
  not need to. The performance-invariants section notes that the ticker's
  `setState` is the one deliberate per-frame rebuild during a drag, and why
  (the whole canvas moves).

### Phase 4: manual walkthrough

Human-only, per `AGENTS.md`:

- a wire from a far-away node into B: drag B's input onto C's input. The
  wire now ends at C and B is unconnected, without ever scrolling to the
  far node. One Ctrl+Z restores it;
- a plain drag from a connected single input onto an incompatible pin: the
  pin does not take it, and the release changes nothing;
- a plain drag from B's connected input onto a nearby D's output: while the
  cursor is over D, the wire is drawn from D to B. On release, B reads D,
  and one Ctrl+Z restores the old source;
- drag the wire off a `map`'s `f` pin onto another `map`'s: the first map's
  inline body comes back and its neighbours make room for it, and one Ctrl+Z
  restores both maps;
- `union` with three wires: a plain drag adds a fourth; Ctrl+drag onto
  another `union`'s array input moves all three, in order; Ctrl+drag onto a
  single input is not accepted;
- Ctrl+drag a node's output onto an alternative node's output: every
  consumer, including capture wires into a body, now reads the alternative;
- Escape mid-drag, a drop in empty space, and a drop back on the starting
  pin: nothing changes, and no Add Node popup appears;
- a comment anchored to a moved wire still points at it;
- a linked (read-only) network: neither a plain drag on a connected input
  nor a Ctrl+drag picks anything up;
- the canvas stays smooth during the drag in a 100-node network;
- edge auto-pan: drag a new wire toward each edge and each corner. The canvas
  pans, faster the deeper the pointer goes, and stops when the pointer comes
  back inside. Connect to a pin that was off screen when the drag began. Do
  the same with a moved wire, a dragged node (it stays under the cursor, and
  inside its body for a body node) and a selection rectangle (its start
  corner stays on the content it started on). Release while panning with a
  still pointer: nothing connects to a pin that has scrolled away. Release
  after a pan with the pointer at the very edge of a pin's hit area: the
  drop is taken. Drag a new wire, then a moved one, out of the canvas over
  the viewport and the panels: the wire stops at the canvas border and
  points toward the pointer, and nothing is drawn over the viewport.
  Release there: nothing happens, with no Add Node popup and the moved
  wires back where they were. At each zoom level the speed feels the same. After switching networks and back,
  the view is where the drag left it. Tune the band and speed constants
  here;
- run the Flutter smoke test (`flutter test integration_test/`).

## Testing

Three layers, each placed where the logic lives. The tests themselves are in
the phase that builds what they test.

- **Rust, domain level** (`atomcad-structure-designer`'s
  `structure_designer` harness). Every rule lives in Rust (D9), so this layer
  carries the weight. It uses four oracles: *no mutation* for plans and
  refusals, *round trip* and *invariants* for commits (all from
  `splice_support.rs`), and *equivalence* to delete-and-connect for D17.
  The plan/commit agreement sweep covers every pin pair of one fixture.
  Refusals are asserted as `WireMoveRefusal` variants, never as strings.
  Phase 1a's refactor is held by the existing tests unchanged, plus the two
  disconnect legs written before it. The FRB wrappers are thin and untested,
  per `rust/AGENTS.md`.
- **Dart, pure functions** (`test/`, no Rust library). These are the mode
  decision, the memo and held-wire keys, the drawn segments, the edge-pan
  velocity and step, and the D14/D19 release decision. The widgets and the
  ticker are thin callers of these functions, by design. Logic that would
  otherwise sit in a `build`, a `paint` or a tick callback is pulled out so
  it can be tested.
- **Dart, widget test**: `pinUnderPointer`. It depends on Flutter's hit
  testing, so it is tested against real widgets, not re-derived geometry.

The **Phase 4 walkthrough** covers only what cannot be asserted
automatically: how the gesture feels and looks, Escape and focus, smoothness,
and the band and speed tuning. `flutter test integration_test/` is
human-only (`AGENTS.md`). Nothing in this design depends on it, and it is
listed in Phase 4 only as part of the maintainer's run.

## Open questions

None.
