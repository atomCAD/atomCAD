# Node Network Editor - Agent Instructions

Interactive visual node graph editor widget. Handles rendering, interaction, and manipulation of the node DAG.

## Files

| File | Purpose |
|------|---------|
| `node_network.dart` | Main editor widget: pan/zoom, selection, wire dragging, keyboard shortcuts |
| `node_network_content.dart` | What the canvas *contains*, shared by the live editor and the image export: the scope walk, the node-widget switch, the content bounding box |
| `export_network_image.dart` | *File → Export node network image*: the whole network as a PNG, rendered offscreen at full size |
| `network_region_capture.dart` | `query --layout --image`: one area of one scope (Rust resolves the frame) through the same snapshot, at 100% zoom, with rulers labelled in scope coordinates |
| `node_network_painter.dart` | Custom painter: grid, wires (Bezier curves), pin hit testing |
| `node_widget.dart` | Individual node rendering: pins, title, drag, context menu, HOF body region |
| `scope_resolver.dart` | Per-frame `ScopeResolver` + `LayoutCache` for scope-aware coordinates, hit testing, and pin-position resolution |
| `comment_node_widget.dart` | Special rendering for Comment nodes |
| `add_node_popup.dart` | Node type picker dialog with category filtering (+ the `inZoneBody` filter, below) |
| `node_title.dart` | What a title bar says under the current `NodeTitleMode` — type name or node name (`doc/design_node_names_in_ui.md` D4/D5). Every node kind goes through it; the size rule deliberately does not |
| `node_name_path.dart` | Composes a node's **name path** (`map4/e1`) from a `NodeNetworkView` — the spelling the AI is handed in error paths, diff hunk titles and the Layout log. Kept for the surfaces that *display* a path for a node already in hand (the name strip, *Copy node name*); anything that *matches* a path goes through Rust instead (`find_nodes_by_name` / `resolve_node_path`) |
| `open_port_tabs.dart` | The open-port `+` tabs (`doc/design_wire_declared_inputs.md`): the pure placement `openPortTabs` (tested in `test/open_port_tabs_test.dart`), the canvas layer and each tab's `DragTarget` |
| `wire_drag_mode.dart` / `wire_move.dart` | The move-wires drag (`doc/design_move_wires.md`): the gesture rule, the `WireMove` state, and the pure key / segment helpers (tested in `test/wire_drag_mode_test.dart`, `test/wire_move_test.dart`) |
| `edge_pan.dart` | Edge auto-pan (`doc/design_move_wires.md` Phase 3): the pure velocity / step / release-rule functions (tested in `test/edge_pan_test.dart`) and `pinUnderPointer` (widget-tested in `test/pin_under_pointer_test.dart`); the ticker that calls them is in `NodeNetworkState` |
| `snap_guides.dart` | Snapping guides while a node is dragged (`doc/design_deliberate_node_placement.md` B1): the pure `SnapSession` (targets, priority, guides; tested in `test/snap_guides_test.dart`) and `buildSnapSession`, which reads its candidates from the live view. See "Snapping guides" below |
| `find_node_picker.dart` | **Find Node** (`doc/design_node_names_in_ui.md` D7): the top-of-canvas "go to symbol" overlay and its rows. Matching, ranking and the path spelling are Rust's (`find_nodes_by_name`) — this file is the overlay, the keyboard and the landing |

## Coordinate System

Two spaces:
- **Logical space:** Where node positions are stored (pan-invariant)
- **Screen space:** Rendered pixel coordinates
- Conversion: `screen = (logical + panOffset) * scale`

## Zoom Levels

Three discrete levels with different detail:
1. **Normal (1.0):** Full detail — pins, labels, subtitles
2. **Medium (0.6):** Simplified — title only, smaller pins
3. **Far (0.35):** Minimal — text only, no pins

## Interaction Model

- **Pan:** Middle mouse drag, or Shift+right-click drag
- **Zoom:** Mouse wheel (zoom-to-cursor)
- **Select node:** Click (Ctrl=toggle, Shift=add)
- **Rectangle select:** Click+drag on empty space
- **Wire creation:** Drag from pin → drop on compatible pin
- **Auto-connect:** Drop wire in empty space → opens `AddNodePopup` filtered by type, then **plans before it changes anything** (`doc/design_insert_node_on_wire.md` Phase 0): `model.planWireDrop` returns the valid arrangements (which input takes the wire, or which output feeds the slot — every non-function output, not only pin 0, and the open port when no pin fits), `ArrangementDialog` asks only when there is more than one, and `model.addNodeAndConnect` commits node + wire as one undo step. Rust computes the scope depth; cancelling leaves the network untouched. Row labels come from the pure `arrangementRowLabels` (`arrangement_labels.dart`, tested in `test/arrangement_labels_test.dart`)
- **Move wires:** a plain drag from a connected single input, or a Ctrl+drag from any connected pin, picks up that pin's wires and drops them on another pin (`doc/design_move_wires.md`). The mode is decided once at drag start by the pure `wireDragMode` (`wire_drag_mode.dart`) from `sd_api.pinWires`; Rust owns every acceptance rule (`moveWiresRefusal`, asked once per pin through a `PerDragPlanMemo` keyed by `pinKey`) and commits with `moveWires`. A move is **a separate drag state** (`model.wireMove`, `wire_move.dart`) and leaves `draggedWire` null — that is what keeps the open-port tabs hidden and the source pin's `onDragEnd` from opening the Add Node popup. The painters hide the held wires by `heldWireKey`, which must equal `WireHitResult.keyOf` (both go through `WireHitResult.storageKey`), and draw `wireMoveSegments`. Escape sets `wireMove.cancelled` rather than clearing it: `Draggable` cannot be cancelled from outside, and a cleared move would let the next `onDragUpdate` start a new wire
- **Edge auto-pan:** during a wire drag (new or moved), a node drag or a rectangle selection, a pointer in the edge band or past the edge pans the canvas (`edge_pan.dart`). It works because the canvas `Listener` keeps receiving the pointer's moves past its bounds for any drag that *started* on the canvas — which is also why drags from the user-types panel do not pan. Each tick moves dragged nodes by the opposite logical step (`model.nudgeDraggedNodes`) and the rectangle's start corner with the content. **Every wire release path asks `model.wireReleaseIgnored(chosenPin)` first** (`PinWidget.onAcceptWithDetails`, the empty-space branch of `onDragEnd`): a release outside the canvas does nothing (D19), and after a pan with a still pointer the pin `Draggable` chose — it re-hit-tests only on pointer moves — must equal `pinUnderPointer`, Draggable's own hit test, read through the `MetaData(PinReference)` wrapper around each pin's `DragTarget`. The canvas `Listener` sees the pointer-up before the gesture recognizers deliver the drop, which is what lets it record the release position first. A new wire drop target owes the same check
- **Open port:** while a wire is dragged, nodes with `hasOpenInputPort` show a `+` tab below them; dropping on it declares an input from the wire (see "Open-port tabs" below)
- **Keyboard:** Ctrl+C/X/V (copy/cut/paste), Del (delete), Ctrl+Del (dissolve the one selected node; handled *before* the Del branch and never falls through to it — a refusal is a transient snackbar), Ctrl+D (duplicate)

## The canvas is rendered twice (live editor + image export)

*File → Export node network image* (and the CLI's `query --layout --image`,
`network_region_capture.dart`) renders the same canvas a second time, off
screen, laid out at the full size of the content rather than the viewport, and
rasterizes it to a PNG (`lib/common/offscreen_capture.dart` explains the private
build/pipeline-owner trick and what a detached tree may depend on).

The consequence for anything you add to the canvas: **content rendering belongs
in `node_network_content.dart`, not in `NodeNetworkState`.** A new kind of node
widget, or a change to which nodes/bodies get walked, must go through
`appendCanvasNodeWidgets` / `canvasNodeWidget` or it will be missing from every
exported image while looking perfectly fine on screen. What legitimately stays
in `NodeNetworkState` is what a still image has no use for: pan/zoom state,
pointer handling, the selection rectangle, and the drag fast path below.

The capture is a single build → layout → paint pass, so nothing that needs a
later frame (a post-frame callback, an animation, a hover) can appear in it.

The live canvas wraps its `Stack` in a `ClipRect`: the wire painters'
`CustomPaint` is canvas-sized but paints wherever a curve goes, and a `Stack`
clips only children whose *layout* overflows, so a wire dragged past the edge
used to be drawn over the viewport and the panels. The export needs no clip —
it lays out at the full content size.

It renders the current model state verbatim with **one** exception: the
`hideSelection` flag, threaded from `NodeNetworkCanvasSnapshot` through
`canvasNodeWidget` into `NodeWidget` / `CommentNodeWidget` / `NodeNetworkPainter`,
draws everything as neither selected nor active. Styling code reads the derived
`_drawSelected` / `_drawActive` (and `NodeNetworkPainter._drawSelected(...)`)
rather than `node.selected` / `node.active`, so new highlight styling must go
through them too; interaction handlers keep reading the real flags.

**Suppressing highlights at render time is deliberate, not laziness** — the
obvious alternative, clearing the selection around the export, also clears the
active node (`NodeNetwork::clear_selection`), which re-runs the display policy
and, under a selection-driven policy, re-evaluates the 3D scene twice for the
sake of a picture.

## Performance invariants (drag fast path + shared resolver)

Two invariants keep the canvas responsive in 100+-node networks (profiled:
a full `notifyListeners()` rebuild-and-relayout of every node widget was ~57%
of frame time; per-widget `ScopeResolver` construction made rebuilds O(N²)):

1. **One `ScopeResolver` per frame.** `NodeNetworkState._buildStackChildren`
   constructs a single resolver and passes it down to every `NodeWidget` /
   `CommentNodeWidget`. The resolver's constructor runs a full O(N) layout
   pass — **never construct one per widget in a `build` method.** Event
   handlers (context menu, drag end, hit tests) may construct their own; those
   are one-shot. The two `NodeNetworkPainter` layers also build their own per
   `paint` — deliberate, because paints triggered by the drag fast path need
   positions fresher than the last widget build.

2. **Continuous drags never call `notifyListeners()`.** Per-pointer-delta
   notifications rebuild the whole canvas plus every other Consumer. Instead,
   `StructureDesignerModel.dragRepaint` (a `ValueNotifier<int>` tick) drives:
   - `NodeNetworkState._buildNodeWidget` wraps every node in a
     `ListenableBuilder`; on a tick only *drag-affected* nodes (per
     `model.isNodeDragAffected` — the dragged nodes plus a dragged
     HOF/closure's body content) rebuild with a fresh shared-per-tick resolver
     (`_dragFrameResolver`); all others return the identical prebuilt child,
     which `Element.updateChild` short-circuits (no rebuild, no layout).
   - Both wire painters receive `repaint: model.dragRepaint`, so wires track
     the in-place-mutated positions (and the wire-drag rubber band) without
     any widget rebuild.
   The gesture-end commit (`updateNodePosition` / `updateSelectedNodesPosition`
   → `refreshFromKernel`, which clears the drag state) does the one full
   notify. **A new continuous-drag interaction must follow this pattern**; a
   new widget that renders drag-mutated state must either live under the
   per-node `ListenableBuilder` or listen to `dragRepaint` itself.

   The one deliberate exception is the edge auto-pan ticker: each tick is a
   `setState` on `NodeNetworkState` that rebuilds the whole canvas, because
   the whole canvas moves — the same cost a middle-mouse pan pays per pointer
   move. It runs only while the pointer is in the edge band.

## Snapping guides (node drag)

A dragged node snaps to straight wires, shared edges, the `normal` gap and
equal spacing (`snap_guides.dart`, `doc/design_deliberate_node_placement.md`
B1). It rides the drag fast path above, so it is held to the same rules:

- **The session is built once per drag, never per tick.** The model asks the
  editor for the canvas (`model.snapCanvas`, a resolver at the current pan and
  zoom plus the canvas size) on the drag's first tick and builds a
  `SnapSession` whose targets are pre-reduced to "a value for the grabbed
  node's left / top" and sorted; a tick is a binary search per axis. An edge
  auto-pan drops the session (the on-screen candidates changed) and the next
  step rebuilds it.
- **The model tracks the unsnapped position** (`_snapRaw`) and moves the node
  by *target − current*, never by the pointer delta. That is what keeps a
  snap from sticking, and what lets Alt (checked per tick) put the node back
  under the pointer mid-drag.
- **Only the grabbed node snaps.** `dragSelectedNodes` takes
  `grabbedNodeId`; the rest of the selection moves by the grabbed node's
  delta. A new drag gesture that moves nodes should pass it too, or it simply
  does not snap.
- **Insert-on-wire wins.** While `_spliceDropTarget` is set (last tick's
  highlight), the node follows the pointer unsnapped.
- **The guides are interaction, not content**: the overlay painter draws
  `model.snapResult` in the drag's scope (`model.snapScope`), and skips them
  under `hideSelection`, so the image export never shows them.

## Editable widgets on the canvas (the focus-steal trap)

Comment notes are editable in place (issue #421): double-clicking one swaps its
`Text` widgets for `TextField`s inside the same canvas `Stack`. Any future
canvas-embedded editable has to clear the same two hazards, both of which come
from the canvas needing keyboard focus for its own shortcuts:

1. **`NodeNetworkState` grabs focus on tap-down and on mouse-enter.** The
   tap-down grab is the nastier one: `TapGestureRecognizer` fires `onTapDown` on
   the 100 ms press deadline, *before* the arena resolves in the editable's
   favour, so it runs even for clicks the field ultimately wins — and pulls the
   caret straight back out. Both grabs consult
   `StructureDesignerModel.inPlaceEditRef` (mouse-enter skips outright;
   tap-down skips only for a tap that hit the note being edited, since every
   other tap must still take focus — that is what closes the editor). The flag
   is deliberately non-notifying: it is written from focus callbacks.
2. **The node's own drag handlers fight the field's selection drag.** Detach
   `onPanStart`/`onPanUpdate`/`onPanEnd` while editing rather than relying on the
   arena to favour the child.

**Do not open the editor with `onDoubleTap`.** `DoubleTapGestureRecognizer`
*holds the gesture arena* on the first tap (`gestures/multitap.dart`
`_registerFirstTap`), which breaks the node's ordinary click handling in two
ways: a real double-click makes it win and **reject** the sibling
`TapGestureRecognizer`, so `onTapDown` never fires and the node is never
selected; and a plain single click has its `onTapDown` delayed by the whole
`kDoubleTapTimeout`. `CommentNodeWidget._handleTapDown` compares timestamps
itself instead, which costs nothing and hands the handler the click position.

Whatever opens the editor should also **select the node** — a double-click's
second tap and the context-menu route both bypass the normal selection path,
and an unselected node leaves the properties panel pointed somewhere else.

To put the caret **where the user clicked**, read it off the rendered widget
rather than recomputing it from layout constants: hold a `GlobalKey` on the
static `Text`, and before the rebuild swaps it out call
`RenderParagraph.getPositionForOffset(renderObject.globalToLocal(globalPos))`.
That uses the line breaks actually on screen and folds in padding, canvas zoom,
and scroll offset for free. Assign the resulting `TextSelection` to the
controller *after* `requestFocus` — assigning `.text` leaves the selection
invalid, which `EditableText` resolves to end-of-text when focus arrives.

Two further rules the note editor follows, worth copying:

- **Do not `refreshFromKernel()` per keystroke.** Push the value to the kernel
  and refresh once when editing ends. A full refresh hands the widget a fresh
  `NodeView`; resyncing a focused controller from it is the classic caret-jump
  bug, and the on-screen text is the field's own content anyway.
- **The field must own its vertical scroll** — see `lib/AGENTS.md` → "A Text
  Field Must Own Its Own Vertical Scroll" (issue #422). The note's static branch
  keeps its `SingleChildScrollView`; the editing branch uses `expands: true`
  instead.

## Open-port tabs (declared inputs)

`expr` and an anonymous `record_construct` declare their own inputs, and while a
wire is dragged they show a `+` tab hanging below the node
(`open_port_tabs.dart`, `doc/design_wire_declared_inputs.md`). Three rules:

1. **The tabs live in their own layer of the canvas `Stack`, above the nodes and
   outside every node's footprint — never inside `NodeWidget`.** Flutter does
   not hit-test a child outside its parent's bounds, so a tab hanging out of
   the node widget would never receive the drop. And the footprint must not
   change: `getNodeSize` and the Rust estimators know nothing of the tab, so
   saved layouts — positioned for today's node heights, with no reflow on load
   — never overlap or move. Being on top is also what lets a tab that covers
   the node beneath it win the hit test.
2. **The layer listens to `model.wireDragSource`, not to `draggedWire`.** The
   notifier changes only when a drag starts and ends, so the layer rebuilds
   twice per drag and the drag fast path (above) stays intact. `draggedWire`
   is a setter that keeps the two in step; don't bypass it with the field.
3. **A tab drop clears `draggedWire`** (inside `model.addInputFromWire`), as
   `connectPins` does; otherwise the source pin's `onDragEnd` treats the drop
   as an empty-space drop and opens the add-node popup.

The tabs are interaction, not content: the image export does not draw them.
Rust owns every rule (which nodes have a port, which types it accepts, the
derived name); Flutter adds only the scope-prefix and zone-input rules of
`canConnectPins`, which Rust delegates to the caller.

## Wire Rendering

Wires use cubic Bezier curves with data-type-based coloring:
- Selected wires get a glow effect
- The hovered wire is drawn thicker with a glow in its own colour (a selected
  wire keeps its orange look). Hover is `model.canvasHover`, matched in the
  draw loop by `WireHitResult.keyOf`, so it is drawn in the wire's own layer
  from that frame's positions. It is set by `_scheduleHoverUpdate` at most once
  per frame, from hover events only (no button held), so no drag touches it;
  a press clears it and the release re-hit-tests. Like the trace hover, it is a
  dedicated notifier, never `notifyListeners`, and the image export never
  draws it
- Hit testing uses expanded area for easier clicking
- Pin positions calculated differently per zoom level

## Comment anchor leader lines

A comment note can be *anchored* to a wire or a node it documents
(`doc/design_wire_annotations.md`); the association is drawn as a dashed grey
leader line from the note's border to the wire's Bezier midpoint or the target
node's border. Three things about it are easy to get wrong:

1. **It is painted by `NodeNetworkPainter`, not by a widget** — bottom layer for
   top-level notes (beside the top-level wires), overlay layer for body notes
   (beside the body wires, so it clears the HOF's opaque body background), and
   the drag rubber band in the overlay. That is what puts it in the exported
   PNG as well as on screen: both canvases build the same painter layers. It
   also inherits `repaint: model.dragRepaint`, so a leader tracks a dragged note
   or a dragged target with no rebuild, and `hideSelection` through
   `_drawSelected`.
2. **Flutter never resolves an anchor.** `NodeView.commentAnchors` arrives
   already resolved by Rust — dangling anchors dropped, wire slot indices
   re-derived through the `destination_param_id` precedence — so the painter
   only has to match the published index against the scope's wire list. Do not
   reimplement that precedence here, and do not filter anchors client-side.
3. **The drag gesture sends no parameter id.** `_anchorAtDropPosition`
   (`comment_node_widget.dart`) builds a `WireAnchor` with
   `destinationParamId: null`; `StructureDesigner::set_comment_anchors`
   canonicalizes it and fills the id in. A future caller that constructs anchors
   itself gets the same treatment for free — but only by going through that
   setter.

The handle lives in the note's bottom-left corner with its own
`GestureDetector`, mirroring the resize handle (see the focus-steal trap above:
a dedicated handle, and inert while the note is open for in-place editing).
**Drag anchors; a plain click on an already-anchored handle detaches.**
Dropping on empty space deliberately does *nothing* — a wire is a ~10 px hit
band, so a miss is the common outcome of an anchoring drag and must not be
indistinguishable from an intentional detach; empty space is also already the
*constructive* gesture for a dragged wire (it opens the Add Node popup). Both
the handle click and the context menu route through `_removeAnchor`.

## Data Type Colors

| Type | Color Family |
|------|-------------|
| Bool/Int/Float | Warm orange |
| Vec2/Vec3/IVec | Cool blue |
| Geometry2D/Blueprint | Purple |
| Crystal/Molecule | Green |
| LatticeVecs/Motif/Structure | Teal/cyan |
| Functions | Amber |

`DATA_TYPE_COLORS` in `node_network.dart` matches by substring, so array types `[T]` pick up the base `T` color. The abstract types `HasAtoms`, `HasStructure`, and `HasFreeLinOps` have **no** entry in the color map. Instead, an input pin declared with an abstract type is rendered as a pie-sliced circle, one equal slice per concrete satisfier, colored with that concrete's color (see `ABSTRACT_TYPE_CONCRETES` and `_PinPainter`). Output pins are always concrete and render single-colored; wires color from the source's concrete type.

## Node Widget States

- **Active:** Thick border, full glow (0xFFD84315)
- **Selected:** Medium border, partial glow (0xFFE08000)
- **Error:** Red border with glow
- **Hovered:** the title bar blends `NODE_TITLE_HOVER_LIGHTEN` toward white,
  on top of whatever title colour the node has. The pointer is over it on the
  canvas (`model.canvasHover`); a separate channel from the border, so it never
  competes with the states above or below. Not drawn on comment notes
- **Trace-hovered:** Magenta (0xFFD500F9) thick border, strong glow — wins over all the
  others. The pointer is on the node's row in the viewport's right-click trace
  menu (`model.traceHoveredNodeId`, listened to by the per-node
  `ListenableBuilder` beside `dragRepaint`); never set by the image export
- **Normal:** Blue border

## Node context menu

It opens from **one** `onSecondaryTapDown` wrapping the whole node in
`NodeWidget.build` (issue #436 — it used to be the title bar only). Don't add a
second one on an inner part: two pending tap recognizers both fire `onTapDown`
at the press deadline, so the menu would open twice. The handler defers to
`resolver.findNodeAtScreenPosition`, the same test the canvas's background menu
uses, so a right-click in an HOF body's empty space still reaches the canvas
(Add Node into the body) instead of opening the HOF's menu.

The right-click menu (`node_widget.dart` `_handleContextMenu`) is **grouped into
titled sections** — Navigate / Edit / Refactor / Node / Body (`doc/design_find_usages.md`
D9). It is built imperatively via a local `addSection(title, section)` helper, not
as one flat `items: [...]` literal: `addSection` skips empty sections, emits a
disabled-header `PopupMenuItem` plus a leading `PopupMenuDivider` between adjacent
non-empty sections (none before the first). This is why a section whose items are
all gated off (e.g. Refactor on a non-custom body node) leaves no orphan header.

**To add a new item, put it inside the right section's list with its own gating
`if`, keyed by a unique `value` string handled in the `.then()` dispatch — do not
append a bare `PopupMenuItem` after the sections.** A flat append lands outside the
grouping and reads as an untitled stray. Every existing item's `value` and gate
(`isCustomNode`, `scopeChain.isEmpty`, `canFactor`, `canConvertToClosure`,
`canExtractToNetwork`, `isCollapsableHof`) is preserved by the grouping — the
reshuffle changed only order and headers, not behavior.

## Wire context menu

Right-clicking a wire (`_handleSecondaryTapDown` → `_showWireMenu`, ahead of
the background menu) offers **Insert node…** and, on a selectable wire,
**Delete wire** (`doc/design_insert_node_on_wire.md` Phase 2). Three things to
know:

1. **The hit test is `findWireAtPosition(…, includeBodyReturn: true)`.**
   Body-return (`ZoneOutput`) wires stay unselectable for clicks, but they take
   an inserted node. `WireHitResult.scopeChain` is the scope the wire is
   *listed* under — the body, for a body-return wire — while
   `WireHitResult.wireScope` is where Rust *stores* it (the HOF's network).
   Every splice call takes `wireScope` and `WireHitResult.anchor`.
2. **Flutter passes the clicked scope, never the insertion scope.**
   `findContainingScope(click)` is `C`; Rust clamps it into the wire's scope
   range (D6). For the same reason the Add Node popup gets Rust's exact list
   (`insertableNodeTypes`, passed as `categories:`) and no `inZoneBody` filter:
   Rust filtered against the insertion scope, which may be shallower than `C`.
3. **Plan, pick, commit** — the same `_pickArrangement` the wire drop uses, so
   the dialog only appears when the plan has several rows and a cancel changes
   nothing.

**Dragging an unconnected node onto a wire** (Phase 4) reaches the same
splice with `APINodeSource.existing`. It lives in the drag fast path, so:

- **The hit test runs in the painters, the decision in the model.** Each
  painter layer calls `model.spliceDropTargetForTick` from `paint`; it computes
  once per `dragRepaint` tick (whichever layer paints first), hit-testing a
  vertical line of points down the dragged node's centre, title bar to bottom
  edge, with that frame's resolver (`findWiresAlong(…, resolver:)`, which
  builds each wire's hit band once for all the points). The highest wire the
  node fits wins. Nothing notifies.
- **Rust is asked once per wire per drag**, through `PerDragPlanMemo`
  (`splice_drop_memo.dart`, tested in `test/splice_drop_memo_test.dart`),
  keyed by `WireHitResult.key`. Only a single-node drag with no wire in its
  own scope asks at all; Rust still refuses a wired node, so the Dart check is
  only a filter.
- **The highlight is taken before the move commits**
  (`model.takeSpliceDropTarget()` in `_handleNodeDragEnd`), because the commit's
  `refreshFromKernel` ends the drag state. The move is its own undo step and
  the splice a second one (`onNodeDroppedOnWire` → `_spliceDroppedNode`, which
  plans again before committing).

**Dropping a network from the user-types panel onto a wire** (Phase 5) is the
canvas `DragTarget`'s `onMove`: it hit-tests the pointer (no wire when over a
node), asks a `PerDragPlanMemo` keyed by wire **and** clicked scope, and
publishes the answer with `model.setUserTypeDropWire` — a dragRepaint bump on
change, never a notify — which the painters draw like the node-drag highlight.
`onMove` also fires for a drag the target rejected, hence
`_userTypeDropAccepted`. The drop commits through `_spliceNewNode`, the wire
menu's own tail.

## Constants (must match Rust `node_layout.rs`)

- `BASE_NODE_WIDTH = 160`
- `BASE_NODE_VERT_WIRE_OFFSET = 33`
- `BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM = 22`

## Zones (inline HOF bodies)

The four HOF nodes (`map`, `filter`, `fold`, `foreach`) render a **translucent body region** inside the node, between the external input and output columns (`_ZoneBodyRegion` in `node_widget.dart`). The body region carries the zone-input pins (inner-left) and zone-output pins (inner-right) plus a bottom-right resize handle. Body-internal nodes are *not* nested inside the HOF widget; they're added as siblings to the top-level `Stack` in `node_network.dart` via the recursive `_appendNodesRecursive` walk, then positioned via the `ScopeResolver` against their scope chain. This keeps the widget tree shallow and lets every node share the same pan/zoom transform.

**Because body nodes and top-level nodes are siblings in one `Stack`, and per-body `next_node_id` counters let them share a numeric id, any widget key for a body-renderable widget MUST include the scope chain.** `NodeWidgetKeys.nodeWidget(id, scopeChain: …)` does this — a bare `Key('node_widget_$id')` produces duplicate keys among siblings, which Flutter mis-reconciles into stale/orphaned "ghost" widgets that survive rebuilds, network switches, and zoom. (Inner pin keys stay bare-id: they live in distinct per-node subtrees, so they only need sibling-uniqueness, which the scoped parent key guarantees.)

### Scope chains

A node's **scope chain** is the `List<BigInt>` of HOF node ids identifying the body it lives in:
- `const []` — top-level network.
- `[hof_42]` — body of node `42` at the top level.
- `[hof_42, hof_99]` — body of node `99` inside the body of node `42`.

`PinReference.scopeChain` carries the scope chain of the pin's owner node; every mutation method on `StructureDesignerModel` takes an optional `scopeChain` parameter (default `const []`) that it forwards to the Rust API's `scope_path: Vec<u64>`. Wires carry `sourceScopeDepth` (how many ancestor frames up the source lives) and `destinationArgumentKind` (`External` for normal wires, `ZoneOutput` for body-return wires) so cross-scope wires resolve correctly.

### `ScopeResolver` and `LayoutCache`

`ScopeResolver` is constructed per frame from the current `NodeNetworkView`, `panOffset`, `scale`, and `zoomLevel`. Its constructor runs `runLayoutPass()`, which builds a `LayoutCache`:

1. **Bottom-up sizes** — for each HOF (recursively, deepest first) compute `bodySizes[chain] = max(stored, content_bbox + padding)`. Body content includes nested HOFs' *effective* sizes (cached from the previous step), so an inner body that grew past its stored size cascades into the outer body's content bbox.
2. **Top-down origins** — for each HOF (recursively, outer first) compute `bodyOrigins[chain]` = parent body's screen origin + the HOF's position-in-parent times scale.
3. **Collapse decisions** — bodies whose rendered screen-space height falls below `BODY_COLLAPSE_HEIGHT_THRESHOLD` (60 logical px) are added to `collapsedBodies`. A collapsed body renders a `[N nodes]` placeholder instead of its content; nested bodies inherit the collapse (an ancestor collapse subsumes its descendants).

`pinScreenPosition(PinReference)` reads from the cache in O(1) — wire rendering, hit testing, drag overlay, and selection-rect overlap all share the same coordinate authority. `findNodeAtScreenPosition` walks the scope tree returning the deepest containing node + its scope chain. `findContainingScope` does the equivalent for empty space (used by right-click → Add Node).

### Pin kinds

`PinReference.pinKind` is one of `externalInput`, `externalOutput`, `functionPin` (the title-bar `-1` output — "the whole node viewed as a function of all its inputs"; suppressed on HOFs by `node_widget.dart`'s title-bar conditional), `zoneInput` (inner-left source on an HOF body), `zoneOutput` (inner-right destination on an HOF body). Wire creation routes through `canConnectPins`/`connectPins` which compute `source_scope_depth = effectiveDestScope.length - source.scopeChain.length` where `effectiveDestScope = dest.scopeChain ++ [dest.nodeId]` for `ZoneOutput` destinations.

The `functionPin` is a **real, working function value** (`doc/design_function_pins.md`) — *not* a dead legacy pin. Drag it into an HOF's `f` pin or `apply.f` and Rust synthesizes a `Function` closure from the node and all its inputs. The drag flows through the generic `PinWidget`/`canConnectPins`/`connectPins` machinery with no functionPin-specific code: it's same-scope (`sourceScopeDepth == 0`), so it routes through `connectNodes` with `sourceOutputPinIndex: -1`, which Rust accepts/stores. Rust is the authority on the type match and the mutual-exclusion rule (a node's function pin and its input pins can't both be wired). When the pin is **consumed**, `NodeView.functionPinConsumed` is true and `_buildOutputPin` greys the node's output-pin eye(s) (tooltip → `apply`), mirroring the Rust scene-skip — derived, not stored, so disconnecting `f` restores the eye.

Note that closures' `Function`-typed pins (the `closure` node's output, the HOFs' optional `f` input, the `apply` node's `f` input) are **ordinary** `externalInput` / `externalOutput` pins that happen to carry a `Function` data type (amber color) — they are *not* the `functionPin` kind, even though they too carry `Function` values. Dragging a `closure` output into an `f` input flows through the normal pin-to-pin path; Rust's `can_be_converted_to` does the structural compatibility check. See `doc/design_closures.md`.

### Body-node visibility eyes (0-ary closures)

Body nodes normally have **no eye icon** — `_buildOutputPin` hides the eye area for anything in a body. The one exception is a body whose whole enclosing chain is parameter-less (`Custom`, zero params) `closure` nodes: such a body's nodes *are* scene-evaluable and get working per-pin eyes, whose handlers pass `scopeChain` through to `toggleOutputPinDisplay` (issue #409, `doc/design_zero_ary_closure_body_display.md`).

**Flutter never re-derives the arity rule.** Rust folds it top-down in `build_zone_view` and publishes it as `ZoneView.bodySceneEvaluable` — already cumulative through the chain, so the free function `isScopeSceneEvaluable(root, scopeChain)` (`scope_resolver.dart`) only walks to the deepest hop and reads that one flag. It shares its definition (`displayed_node_refs::is_body_scene_evaluable`) with the scene collection that decides which bodies actually render, so eyes can't drift from what the viewport shows. Adding a parameter to any closure in the chain clears the flag and the eyes vanish; the body's stored display flags are **dormant, not cleared**, so removing the parameter brings the previous state back.

Body geometry is visible in the viewport but **not click-activatable** (`viewport_pick` filters to top-level refs — the disambiguation overlay / `scrollToNode` / solo-hide are bare-node-id keyed). Collapse is a canvas concern only: a collapsed body keeps rendering its displayed pins in 3D, the eyes are just not on screen.

### Node types that may not live in a body

`showAddNodePopup(context, inZoneBody: …)` filters out node types whose `APINodeTypeView.allowedInZoneBody` is false — today only `parameter`, which declares an input pin of the enclosing *network* and is meaningless inside a body (issue #417). **Flutter never re-derives the rule**: it comes from `node_type_registry::allowed_in_zone_body` in Rust, which also backs the `add_node_scoped` / paste / duplicate refusals, so the menu can't offer something the backend would reject.

Every call site must pass `inZoneBody: scopeChain.isNotEmpty`. The wire-drop-in-empty-space path (`_handleWireDropInEmptySpace`) therefore resolves `findContainingScope(dropPosition)` **before** opening the popup rather than after — the resolver is a pure function of the current model state and the popup is modal, so the scope is the same either way.

### Active scope chain

`StructureDesignerModel.activeScopeChain: List<BigInt>` is the body that keyboard shortcuts (Delete, Ctrl+C / V / X / D) operate on. Clicking on a body's interior, on a body-internal node, or right-clicking inside a body sets the active scope. Clicking on the top-level canvas resets it to `const []`. The Rust side does not mirror this — every mutation API receives `scope_path` explicitly from the call site.

### Selection rectangle

Selection rectangles are scope-confined: the scope is captured at pointer-down (whatever scope the drag started in) and the final node/wire overlap test is restricted to that scope's nodes. The rendered rectangle is also clipped to the body's screen rect (U7 polish) so it doesn't visually escape the body region.

### Closures: the `f`-pin override, derived shapes, and shared shape editor

The four HOFs gained an optional `f: Function` input pin and there are two new function-value nodes: `closure` (zone-bearing, exposes its body as a `Function` output) and `apply` (bodyless, calls a `Function` once or partially). The `closure` node's body **renders and edits for free** through the generic zone machinery above — it is `has_zone()`, so the body region, inner zone pins, recursive body nodes, resize handle, collapse, and hit-testing are all inherited with no closure-specific rendering code.

The **inline-body / `f`-pin toggle**: when an HOF's `f` pin is wired, its inline body is ignored at eval, so the editor hides it. `ScopeResolver.runLayoutPass` collects every such body chain into `LayoutCache.functionOverriddenBodies` (Phase 4 of the layout pass, via `_isFunctionPinConnected`) and *also* adds it to `collapsedBodies`, so the existing body-skip checks in the node walk and painter hide the content with no extra conditions. The HOF then renders a distinct `_ZoneFunctionOverridePlaceholder` ("driven by `f`") instead of the `[N nodes]` collapse placeholder — `node_widget.dart` checks `ScopeResolver.isFunctionOverridden(chain)`. Only the four HOF types declare an `f` pin, so `closure` bodies are never flagged. Disconnecting `f` restores the inline body.

**Derived pin layouts (apply, map)** — `doc/design_function_pin_unification.md` Phase D. `apply.f` is declared `AnyFunction { vec![] }` (any function) and `map.f` is `AnyFunction { vec![element_type] }` (any function whose first param matches the input element type). Both nodes' detailed pin layouts (apply's arg pins, map's derived output type) are installed by Rust post-passes from the wired source's canonical-flat signature. The Flutter side just reads `NodeView.derived_shape.derived_from_input_pin: Option<String>` to switch between the connected and disconnected editor surfaces — see the closure-editor / apply-editor / map-editor split documented in `node_data/AGENTS.md`. `AnyFunction` pins render in the same amber color as concrete `Function` pins (no separate color rule); their tooltip is built from a node-specific extra line passed via `PinWidget.extraTooltipLine` (apply: "apply will call it on the wired arguments"; map: "applied per element of the stream").

**Custom closures + partial apply** — `ClosureKind::Custom` allows authoring closures with arbitrary parameter names/types, including 0-arity thunks (`() → R`). On `apply`, arg pins materialize from the wired source's signature; the user wires a **contiguous prefix** of arg pins for partial application (validated Rust-side; the editor doesn't enforce ordering interactively beyond surfacing the validator error). Partially-applied `apply` outputs are concrete `Function` values that flow into downstream `apply` / HOF.f pins like any other function value.

The shape property editor (`ClosureShapeEditor`) for `closure` lives on the `node_data` side (`closure_editor.dart`); `apply` has its own placard (`apply_editor.dart`). See `node_data/AGENTS.md`.

Design docs: `doc/design_zones_ui.md`, `doc/design_closures.md`, `doc/design_currying.md`, `doc/design_function_pin_unification.md`.
