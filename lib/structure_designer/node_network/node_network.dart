import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/node_network/add_node_popup.dart';
import 'package:flutter_cad/structure_designer/node_network/edge_pan.dart';
import 'package:flutter_cad/structure_designer/node_network/arrangement_labels.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_widget.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network_content.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network_painter.dart';
import 'package:flutter_cad/structure_designer/node_network/open_port_tabs.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';
import 'package:flutter_cad/structure_designer/node_network/splice_drop_memo.dart';
import 'package:flutter_cad/structure_designer/node_networks_list/user_type_drag.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/common/error_display.dart';

// Zoom levels
enum ZoomLevel {
  normal,
  zoomedOutMedium,
  zoomedOutFar,
}

/// Returns the scale factor for a given zoom level
/// This allows most layout constants to scale proportionally
double getZoomScale(ZoomLevel zoomLevel) {
  switch (zoomLevel) {
    case ZoomLevel.normal:
      return 1.0;
    case ZoomLevel.zoomedOutMedium:
      return 0.6;
    case ZoomLevel.zoomedOutFar:
      return 0.35;
  }
}

/// Discrete index mirroring the Rust `CanvasViewport.zoom_level` field
/// (0 = normal, 1 = medium, 2 = far). Used to persist the per-network canvas
/// viewport (issue #414 Phase 4).
int zoomLevelToIndex(ZoomLevel zoomLevel) {
  switch (zoomLevel) {
    case ZoomLevel.normal:
      return 0;
    case ZoomLevel.zoomedOutMedium:
      return 1;
    case ZoomLevel.zoomedOutFar:
      return 2;
  }
}

/// Inverse of [zoomLevelToIndex]; unrecognized indices fall back to normal.
ZoomLevel zoomLevelFromIndex(int index) {
  switch (index) {
    case 1:
      return ZoomLevel.zoomedOutMedium;
    case 2:
      return ZoomLevel.zoomedOutFar;
    default:
      return ZoomLevel.normal;
  }
}

// Base node dimensions and layout constants (for normal zoom level)
// These scale proportionally with zoom level via getZoomScale()
const double BASE_NODE_WIDTH = 160.0;
const double BASE_NODE_HEIGHT_MIN = 60.0; // Minimum height for zoomed out nodes
const double BASE_NODE_VERT_WIRE_OFFSET = 33.0;
const double BASE_NODE_VERT_WIRE_OFFSET_EMPTY = 42.0;
const double BASE_NODE_VERT_WIRE_OFFSET_FUNCTION_PIN = 16.0;
const double BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM = 22.0;
const double BASE_CUBIC_SPLINE_HORIZ_OFFSET = 50.0;
const double BASE_ZOOMED_OUT_PIN_SPACING =
    10.0; // Vertical spacing between input wires in zoomed-out mode

// HOF body layout constants (logical pixels, scaled by zoom).
// The body region sits inside the HOF widget between the external input column
// (on the left) and the external output column (on the right). Geometry:
//
//   ┌── HOF ───────────────────────────────────────────────────────┐
//   │ title                                          function-pin →│
//   │┌──────────────┬──────────────────────────────┬──────────────┐│
//   ││ ext inputs   │  body (translucent)          │  ext outputs ││
//   ││ ext●  "xs"   │ ●  element            result●│ "Iter" ●     ││
//   ││              │                              │              ││
//   │└──────────────┴──────────────────────────────┴──────────────┘│
//   │ (subtitle)                                                   │
//   └──────────────────────────────────────────────────────────────┘
//
// HOF_BODY_TOP_OFFSET is the body's top edge relative to the HOF widget's
// top edge (covers title + padding). HOF_BODY_LEFT_OFFSET is the body's
// left edge relative to the HOF's left edge (the external input column's
// width). HOF_BODY_RIGHT_GUTTER is the width of the external output column
// to the right of the body region.
const double BASE_HOF_BODY_TOP_OFFSET = 36.0;
const double BASE_HOF_BODY_LEFT_OFFSET = 70.0;
const double BASE_HOF_BODY_RIGHT_GUTTER = 70.0;
const double BASE_HOF_BODY_BOTTOM_PADDING = 8.0;
// Side length (logical px) of the body's bottom-right resize-handle square.
// Used both by `_BodyResizeHandle` (the widget) and by `_findNodeAt` (so a
// click on the handle hit-tests as "on the HOF" and doesn't start a parallel
// rectangle-selection drag from the outer Listener).
const double BASE_HOF_BODY_RESIZE_HANDLE_SIZE = 14.0;

// Trimmed body chrome for the `closure` node. A closure has no external input
// pins (captures arrive as ordinary capture wires drawn into the body) and its
// single `Function` output pin renders in the title bar (not a right-edge
// output column), so the wide left column and right gutter the four HOFs need
// are wasted on it. These small pads give the body maximal width while leaving
// room for the rounded border and the inner-edge zone pins. See
// `doc/design_closures.md` §"Editor (Flutter) changes" / Proposal 2.
const double CLOSURE_BODY_LEFT_PAD = 16.0;
const double CLOSURE_BODY_RIGHT_PAD = 16.0;

/// External input column width to the left of an HOF's body region. The four
/// HOFs reserve [BASE_HOF_BODY_LEFT_OFFSET] for their `xs` / `init` / `f` pins;
/// the `closure` node, which has no external inputs, uses a small pad so the
/// body extends left. Both the rendering ([NodeWidget]) and the wire-endpoint
/// math ([ScopeResolver]) read this single helper so the two never drift.
double hofBodyLeftOffset(NodeView node) => node.nodeTypeName == 'closure'
    ? CLOSURE_BODY_LEFT_PAD
    : BASE_HOF_BODY_LEFT_OFFSET;

/// External output column width to the right of an HOF's body region. The four
/// HOFs reserve [BASE_HOF_BODY_RIGHT_GUTTER] for their result output pin; the
/// `closure` node renders its `Function` output in the title bar instead, so it
/// uses a small pad. See [hofBodyLeftOffset].
double hofBodyRightGutter(NodeView node) => node.nodeTypeName == 'closure'
    ? CLOSURE_BODY_RIGHT_PAD
    : BASE_HOF_BODY_RIGHT_GUTTER;

/// True for an HOF whose single external output pin renders in the title bar
/// (the slot non-HOF nodes use for the legacy function pin) rather than in a
/// right-edge output column. Only the `closure` node qualifies: its lone output
/// is a `Function` value with no displayable viewport output, so the eye toggle
/// and the output column are pointless. Routing this through one predicate keeps
/// the title-bar rendering and the `externalOutput` wire-endpoint math in
/// lockstep.
bool hofOutputPinInTitleBar(NodeView node) => node.nodeTypeName == 'closure';

// Legacy constants for backward compatibility (normal zoom)
const double NODE_WIDTH = BASE_NODE_WIDTH;
const double NODE_VERT_WIRE_OFFSET = BASE_NODE_VERT_WIRE_OFFSET;
const double NODE_VERT_WIRE_OFFSET_EMPTY = BASE_NODE_VERT_WIRE_OFFSET_EMPTY;
const double NODE_VERT_WIRE_OFFSET_FUNCTION_PIN =
    BASE_NODE_VERT_WIRE_OFFSET_FUNCTION_PIN;
const double NODE_VERT_WIRE_OFFSET_PER_PARAM =
    BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM;
const double CUBIC_SPLINE_HORIZ_OFFSET = BASE_CUBIC_SPLINE_HORIZ_OFFSET;

// Hand-tuned font sizes per zoom level (don't scale linearly)
double getNodeTitleFontSize(ZoomLevel zoomLevel) {
  switch (zoomLevel) {
    case ZoomLevel.normal:
      return 14.0;
    case ZoomLevel.zoomedOutMedium:
      return 11.0;
    case ZoomLevel.zoomedOutFar:
      return 8.0;
  }
}

// Wire appearance constants
const double WIRE_WIDTH_SELECTED = 4.0;
const double WIRE_WIDTH_NORMAL = 2.0;
const double WIRE_GLOW_OPACITY = 0.3;

const double HIT_TEST_WIRE_WIDTH = 12.0;

// Colors
const Color DEFAULT_DATA_TYPE_COLOR = Colors.grey;
const Map<String, Color> DATA_TYPE_COLORS = {
  // Primitive numbers (warm colors)
  'Bool': Color(0xFFFF4D4D), // deep orange
  'Int': Color(0xFFFFB74D), // Light orange
  'Float': Color(0xFFFF8A65), // Light deep orange

  // Vector types (cool blues - mathematical coordinates)
  'Vec2': Color(0xFF4DD0E1), // Light cyan
  'Vec3': Color(0xFF64B5F6), // Light blue
  'IVec2': Color(0xFF81D4FA), // Light blue variant
  'IVec3': Color(0xFF9575CD), // Light indigo

  // Matrix types. 'IMat3' entry must come before 'Mat3' so that the
  // substring-match loop picks it for the integer variant before the float
  // variant's 'Mat3' key also matches.
  'IMat2':
      Color(0xFF7986CB), // Desaturated indigo (integer variant, mirrors IMat3)
  'IMat3': Color(0xFF7986CB), // Desaturated indigo (integer variant)
  'Mat3': Color(0xFF42A5F5), // Saturated blue (float variant)

  // Geometry types (purple family - abstract shapes)
  'Geometry2D': Color(0xFFBA68C8), // Light purple
  'Blueprint': Color(0xFF9C27B0), // Deep purple - latent atoms in a structure

  // Phase types (green family - materialized matter).
  // Abstract types (HasAtoms, HasStructure, HasFreeLinOps) have no entry here —
  // abstract-typed input pins render pie-sliced in the concrete satisfier
  // colors (see ABSTRACT_TYPE_CONCRETES and PinViewWidget).
  'Crystal': Color(0xFF558B2F), // Olive green - atoms bound to a structure
  'Molecule': Color(0xFF81C784), // Soft green - free atoms, no structure

  // Crystal structure types (teal family - crystalline matter)
  'LatticeVecs': Color(0xFF26A69A), // Teal
  'Motif': Color(0xFF00ACC1), // Light blue-green (cyan)
  'Structure': Color(0xFF00796B), // Deep teal (composite of lattice + motif)

  // Volumetric data (red family - sampled scalar functions of space).
  // No substring conflict with any key above, so declaration order here is
  // free; `[ScalarField]` and `Iter[ScalarField]` match through the same key.
  'ScalarField': Color(0xFFE57373), // Soft red
  // `Isosurface` is a `ScalarField` in rendered form, so it takes the same
  // family a shade deeper. No substring conflict with 'ScalarField'.
  'Isosurface': Color(0xFFC62828), // Deep red

  // Function types (amber family - computational operations)
  '->': Color(0xFFFFA726), // Amber

  // Unit (the discard / effect-only type) — dim grey, deliberately
  // visually quiet because Unit pins carry no value.
  'Unit': Color(0xFF757575),
};

// Records use a single neutral color regardless of named/anonymous so the
// visual reflects compatibility (structural), not identity (the def name).
// See `doc/design_record_types.md` Phase 9.
const Color RECORD_DATA_TYPE_COLOR = Color(0xFFB0BEC5); // Neutral blue-grey
const Color WIRE_COLOR_SELECTED = Color(0xFFD84315);

/// Converts a position from logical space to screen space.
/// Logical space is the coordinate system where node positions are stored.
/// Screen space is what's actually rendered on screen.
///
/// The transformation is: screen = (logical + panOffset) * scale
/// where panOffset is stored in logical coordinates.
Offset logicalToScreen(Offset logical, Offset panOffset, double scale) {
  return (logical + panOffset) * scale;
}

/// Converts a position from screen space to logical space.
/// This is the inverse of logicalToScreen.
///
/// The transformation is: logical = (screen / scale) - panOffset
Offset screenToLogical(Offset screen, Offset panOffset, double scale) {
  return (screen / scale) - panOffset;
}

/// Helper function to get node dimensions based on zoom level.
/// Returns Size(width, height) for the given node at the specified zoom level.
/// For normal zoom, estimates height including title, pins, and subtitle.
/// For zoomed-out modes, uses proportionally scaled height with minimum aspect ratio.
///
/// For HOF (zone-owning) nodes, the footprint grows to include the body region:
/// width gains `body.storedWidth` (plus the external input/output gutters);
/// height gains the body's height. See `doc/design_zones_ui.md` §"Phase U3"
/// gotcha: "The HOF's overall screen footprint now grows to include the body."
Size getNodeSize(NodeView node, ZoomLevel zoomLevel) {
  final scale = getZoomScale(zoomLevel);

  // Calculate estimated height at normal scale (for all zoom levels)
  // Title bar: ~30px, main body: max(inputs, outputs), subtitle: ~20px, padding: ~8px
  final titleHeight = 30.0;
  // For HOF nodes the inner-edge zone pins also stack along the body's edges,
  // so the body-content row height must be at least the larger of the two
  // zone pin counts. Non-HOF nodes have empty zone-pin lists ⇒ no effect.
  final zone = node.zone;
  // A compact HOF renders as an ordinary node (no body region): drop its
  // zone-pin and stored-height contributions and use the regular base width,
  // so its footprint is driven by the external input/output pins. `collapsed`
  // is the Rust-resolved effective state (folds in Auto/Collapsed/Expanded
  // and `f`-connection). Matches `effectiveNodeSizeLogical`'s compact branch.
  final bool compactHof = zone != null && zone.collapsable && zone.collapsed;
  final zoneInputPinsHeight = (zone != null && !compactHof)
      ? zone.zoneInputPins.length * BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM
      : 0.0;
  final zoneOutputPinsHeight = (zone != null && !compactHof)
      ? zone.zoneOutputPins.length * BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM
      : 0.0;
  final inputPinsHeight =
      node.inputPins.length * BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM;
  final outputPinsHeight =
      node.outputPins.length * BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM;
  final minOutputHeight = 25.0; // Minimum output area height
  // For HOF nodes the body region itself contributes to the row height. In
  // U3 the body always renders at its stored size (no content_bbox computed
  // yet — U4 brings that in via the layout pass).
  final bodyContentHeight =
      (zone != null && !compactHof) ? zone.storedHeight : 0.0;
  final mainBodyHeight = [
    inputPinsHeight,
    outputPinsHeight,
    zoneInputPinsHeight,
    zoneOutputPinsHeight,
    bodyContentHeight,
    minOutputHeight,
  ].reduce((a, b) => a > b ? a : b);
  final subtitleHeight =
      (node.subtitle != null && node.subtitle!.isNotEmpty) ? 20.0 : 0.0;
  final padding = 8.0;

  final normalHeight = titleHeight + mainBodyHeight + subtitleHeight + padding;

  // Base width for the standard node; expanded HOFs add the body region plus
  // gutters, a compact HOF uses the regular node width.
  final baseWidth = (zone != null && !compactHof)
      ? hofBodyLeftOffset(node) + zone.storedWidth + hofBodyRightGutter(node)
      : BASE_NODE_WIDTH;

  if (zoomLevel == ZoomLevel.normal) {
    // Normal zoom - use calculated height
    return Size(baseWidth * scale, normalHeight * scale);
  } else {
    // Zoomed out - use proportionally scaled height with minimum aspect ratio
    // Ensure minimum height for text readability (at least 0.375 aspect ratio = height/width)
    final width = baseWidth * scale;
    final scaledHeight = normalHeight * scale;
    final minHeight =
        width * 0.375; // Minimum aspect ratio for at least one line of text

    final height = scaledHeight > minHeight ? scaledHeight : minHeight;
    return Size(width, height);
  }
}

/// The concrete type the pin actually carries in the current network, falling
/// back to the declared (possibly abstract / `SameAsInput(...)`) type when
/// resolution didn't produce anything.
extension OutputPinViewEffectiveType on OutputPinView {
  String get effectiveDataType => resolvedDataType ?? dataType;
}

/// Concrete types that satisfy each abstract data type, in a stable display
/// order. Mirror of the abstract→concrete rules in Rust
/// `rust/src/structure_designer/data_type.rs` `DataType::can_be_converted_to`
/// — keep in sync.
///
/// Used to render abstract-typed input pins as N-sliced pie circles, with one
/// slice per concrete satisfier colored as that concrete.
const Map<String, List<String>> ABSTRACT_TYPE_CONCRETES = {
  'HasAtoms': ['Crystal', 'Molecule'],
  'HasStructure': ['Blueprint', 'Crystal'],
  'HasFreeLinOps': ['Blueprint', 'Molecule'],
};

bool isAbstractDataType(String typeName) =>
    ABSTRACT_TYPE_CONCRETES.containsKey(typeName);

/// True when the (possibly array-wrapped) type string is a record type, in
/// either its named (`Record(Foo)`) or anonymous (`{x: Int, y: Int}`) form.
/// Includes wrapped array types like `[Record(Foo)]` and `[{x: Int}]`.
///
/// Records render in a single neutral color regardless of which named def
/// they reference: the visual encodes structural compatibility, not the
/// identity of the def. See `doc/design_record_types.md` Phase 9.
bool isRecordDataType(String typeName) {
  return typeName.contains('Record(') || typeName.contains('{');
}

/// If [typeName] denotes a named record (or array of named records),
/// returns the def name. Otherwise null. Anonymous records and non-record
/// types both return null.
String? extractNamedRecordDefName(String typeName) {
  // Strip leading `[` for array-wrapped types so both `Record(Foo)` and
  // `[Record(Foo)]` work. We don't bother with deeper nesting — record
  // types appear at most one array level deep on pins in v1.
  final stripped = typeName.startsWith('[') && typeName.endsWith(']')
      ? typeName.substring(1, typeName.length - 1)
      : typeName;
  const prefix = 'Record(';
  if (!stripped.startsWith(prefix) || !stripped.endsWith(')')) {
    return null;
  }
  return stripped.substring(prefix.length, stripped.length - 1);
}

/// Gets the appropriate color for a data type based on its name.
///
/// If the type name contains '->' it's treated as a function type.
/// Otherwise, it looks for any of the base type names in DATA_TYPE_COLORS.
/// For array types like [T], this will return the color of the base type T.
Color getDataTypeColor(String typeName) {
  // Check for function types first
  if (typeName.contains('->')) {
    return DATA_TYPE_COLORS['->']!;
  }

  // AnyFunction pins render as `Function*` (no leading-param constraint) or
  // `Function(T1, ..., *)` (starts-with constraint). They carry function
  // values, so we use the same amber Function family color. See
  // `doc/design_function_pin_unification.md` (Phase D).
  if (typeName.contains('Function')) {
    return DATA_TYPE_COLORS['->']!;
  }

  // Records (named or anonymous, possibly array-wrapped) all render in
  // the same neutral color.
  if (isRecordDataType(typeName)) {
    return RECORD_DATA_TYPE_COLOR;
  }

  // Check for exact matches and partial matches in the type name
  for (final entry in DATA_TYPE_COLORS.entries) {
    if (typeName.contains(entry.key)) {
      return entry.value;
    }
  }

  // Return default color if no match found
  return DEFAULT_DATA_TYPE_COLOR;
}

/// Wraps the NodeNetworkPainter to paint the grid and wires.
///
/// Two render modes:
/// - [overlay] `false` (default): sits at the BOTTOM of the canvas stack and
///   paints grid + top-level wires. It does **not** handle pointer events —
///   wire-tap selection is driven by [NodeNetworkState._handleCanvasTap] via
///   the outer [Listener], which (unlike a GestureDetector buried below the HOF
///   body regions in the Stack) receives taps in every scope, including inside
///   bodies.
/// - [overlay] `true`: sits at the TOP of the canvas stack (above the node
///   widgets) and paints body wires + the dragged wire. Wrapped in
///   [IgnorePointer] so it doesn't intercept node clicks. Body wires would
///   otherwise be hidden behind the HOF widget's opaque body Container, and
///   the dragged wire would vanish wherever it crosses an HOF's footprint.
class NodeNetworkInteractionLayer extends StatelessWidget {
  final StructureDesignerModel model;
  final Offset panOffset;
  final ZoomLevel zoomLevel;
  final bool overlay;

  /// Draw wires as unselected — see `NodeNetworkPainter.hideSelection`.
  final bool hideSelection;

  const NodeNetworkInteractionLayer(
      {super.key,
      required this.model,
      required this.panOffset,
      required this.zoomLevel,
      this.overlay = false,
      this.hideSelection = false});

  @override
  Widget build(BuildContext context) {
    if (overlay) {
      // Top layer: paints body wires + dragged wire. IgnorePointer so it
      // doesn't intercept clicks meant for the node widgets that sit below
      // it in the Stack.
      return IgnorePointer(
        child: CustomPaint(
          painter: NodeNetworkPainter(model,
              panOffset: panOffset,
              zoomLevel: zoomLevel,
              overlay: true,
              hideSelection: hideSelection,
              repaint:
                  Listenable.merge([model.dragRepaint, model.canvasHover])),
          child: Container(),
        ),
      );
    }
    return CustomPaint(
      painter: NodeNetworkPainter(model,
          panOffset: panOffset,
          zoomLevel: zoomLevel,
          hideSelection: hideSelection,
          repaint: Listenable.merge([model.dragRepaint, model.canvasHover])),
      // A plain Container (no gesture/opaque behavior) so the layer fills the
      // Stack for painting but stays transparent to pointer events — taps are
      // handled by the outer Listener (see `_handleCanvasTap`).
      child: Container(),
    );
  }
}

/// The main node network widget.
class NodeNetwork extends StatefulWidget {
  final StructureDesignerModel graphModel;

  const NodeNetwork({super.key, required this.graphModel});

  @override
  State<NodeNetwork> createState() => NodeNetworkState();
}

class NodeNetworkState extends State<NodeNetwork>
    with SingleTickerProviderStateMixin {
  /// Focus node for keyboard events
  final focusNode = FocusNode();

  /// Current pan offset for the network view
  Offset _panOffset = Offset.zero;

  /// Current zoom level
  ZoomLevel _zoomLevel = ZoomLevel.normal;

  /// Store the current network name to detect changes
  String? _currentNetworkName;

  /// Whether we're currently panning with middle mouse button
  bool _isMiddleMousePanning = false;

  /// Whether we're currently panning with Shift + right mouse button
  bool _isShiftRightMousePanning = false;

  /// Store the last pointer position for panning
  Offset? _lastPanPosition;

  /// Whether the current pan gesture actually moved the view. Used to avoid
  /// persisting (and dirtying the design) on a bare middle-click with no drag.
  bool _didPan = false;

  /// Rectangle selection state
  Rect? _selectionRect; // Current rectangle being drawn (screen coords)
  Offset? _selectionRectStart; // Start point of rectangle drag (screen coords)

  /// Scope the rectangle drag started in. Empty = top-level. Captured at
  /// pointer-down so the rectangle is confined to one body for its entire
  /// drag — even if the cursor wanders into another scope. Per
  /// `doc/design_zones_ui.md` §"Phase U4 → Gotchas".
  List<BigInt> _selectionRectScope = const [];

  /// Last known mouse position in screen coordinates (for Ctrl+V paste)
  Offset _lastMousePosition = Offset.zero;

  // ===== EDGE AUTO-PAN (`doc/design_move_wires.md` Phase 3) =====

  /// Runs while a wire, node or selection-rectangle drag holds the pointer
  /// in the edge band or past the edge. Each tick pans the canvas — the one
  /// deliberate per-frame `setState` of a drag, since the whole canvas moves.
  late final Ticker _edgePanTicker;
  Duration _edgePanLastElapsed = Duration.zero;

  /// The pointer of the gesture in progress, canvas-local and global. Kept
  /// past the canvas's bounds: a drag that started here keeps reporting.
  Offset? _dragPointerLocal;
  Offset? _dragPointerGlobal;

  /// The canvas panned since the last pointer move, so `Draggable`'s choice
  /// of drop target may be stale (D14). Read by the release check, which
  /// runs after the pointer-up.
  bool _autoPannedSinceMove = false;

  /// The canvas panned during this gesture: persist the viewport once, when
  /// the gesture ends (D15).
  bool _autoPannedThisGesture = false;

  @override
  void initState() {
    super.initState();
    // Initial calculation of pan offset for the current network
    WidgetsBinding.instance.addPostFrameCallback((_) {
      updatePanOffsetForCurrentNetwork(forceUpdate: false);
    });
    // Set up wire drop callback
    widget.graphModel.onWireDroppedInEmptySpace = _handleWireDropInEmptySpace;
    // Register scroll-to-node callback for click-to-activate
    widget.graphModel.onScrollToNode = _scrollToNode;
    widget.graphModel.onNodeDroppedOnWire = _spliceDroppedNode;
    widget.graphModel.onRequestCanvasFocus = _requestCanvasFocus;
    widget.graphModel.onCheckWireRelease = _isWireReleaseIgnored;
    widget.graphModel.snapCanvas = _snapCanvas;
    _edgePanTicker = createTicker(_onEdgePanTick);
  }

  /// [StructureDesignerModel.snapCanvas]: what a node drag's snapping needs
  /// to know about the canvas — the current transform (as a resolver) and
  /// which part of the network is on screen. Null before the first layout.
  ({ScopeResolver resolver, Size canvasSize})? _snapCanvas() {
    final view = widget.graphModel.nodeNetworkView;
    final size = _canvasSize;
    if (view == null || size == null) return null;
    return (
      resolver: ScopeResolver(
        root: view,
        panOffset: _panOffset,
        scale: getZoomScale(_zoomLevel),
        zoomLevel: _zoomLevel,
      ),
      canvasSize: size,
    );
  }

  /// [StructureDesignerModel.onRequestCanvasFocus]: a move drag started.
  void _requestCanvasFocus() {
    if (mounted) focusNode.requestFocus();
  }

  @override
  void dispose() {
    // Clear the callbacks when disposing — but only if they are still ours.
    // A document switch rebuilds the editor under a new key (D11 of
    // `doc/design_multiple_documents.md`), and the new state registers its
    // callbacks *before* this one is disposed; clearing them unconditionally
    // would leave the new editor deaf to click-to-activate and wire drops.
    if (widget.graphModel.onWireDroppedInEmptySpace ==
        _handleWireDropInEmptySpace) {
      widget.graphModel.onWireDroppedInEmptySpace = null;
    }
    if (widget.graphModel.onScrollToNode == _scrollToNode) {
      widget.graphModel.onScrollToNode = null;
    }
    if (widget.graphModel.onRequestCanvasFocus == _requestCanvasFocus) {
      widget.graphModel.onRequestCanvasFocus = null;
    }
    if (widget.graphModel.onNodeDroppedOnWire == _spliceDroppedNode) {
      widget.graphModel.onNodeDroppedOnWire = null;
    }
    if (widget.graphModel.onCheckWireRelease == _isWireReleaseIgnored) {
      widget.graphModel.onCheckWireRelease = null;
    }
    if (widget.graphModel.snapCanvas == _snapCanvas) {
      widget.graphModel.snapCanvas = null;
    }
    _edgePanTicker.dispose();
    focusNode.dispose();
    super.dispose();
  }

  /// Scrolls the node network view so the given node's center lands on
  /// [screenAnchor] — or on the viewport center when no anchor is given (the
  /// behavior every pre-Find-Usages caller relies on). The zoom level is never
  /// touched.
  ///
  /// [scopeChain] addresses a node inside an HOF / closure body; empty means
  /// the top-level network. A body node's position lives in its body's own
  /// coordinate frame, so it is resolved through the [ScopeResolver]'s body
  /// origins and converted back to top-level logical space (the conversion is
  /// pan-invariant, which is what lets us compute the *new* pan from it).
  ///
  /// If the target sits inside a collapsed body its content isn't on screen at
  /// all, so the landing retargets to the outermost ancestor HOF that is itself
  /// visible (`doc/design_find_usages.md` D6). The selection is untouched — it
  /// was already set by the caller — so opening the body reveals the node
  /// highlighted. Collapse state is user intent and is never auto-expanded.
  ///
  /// **`_currentNetworkName` must be set to the target network's name in the
  /// same `setState` that applies the pan.** The top-left auto-framing
  /// (`updatePanOffsetForCurrentNetwork`) runs from *post-frame* callbacks gated
  /// on that field, so a pan applied during a network switch is otherwise
  /// silently overwritten one frame later. (Click-to-activate never crosses a
  /// network switch, which is why it never needed this.)
  void _scrollToNode(BigInt nodeId,
      {List<BigInt> scopeChain = const [], Offset? screenAnchor}) {
    final view = widget.graphModel.nodeNetworkView;
    if (view == null) return;

    // Get the widget's render box size for centering
    final renderBox = context.findRenderObject() as RenderBox?;
    if (renderBox == null) return;
    final viewportSize = renderBox.size;

    final resolver = _makeResolver();
    if (resolver == null) return;

    // Retarget out of any collapsed body: walk outside-in and stop at the
    // first body whose content is hidden — its owner HOF is rendered (its own
    // container is not collapsed), so it is the outermost visible container of
    // the requested node.
    List<BigInt> chain = scopeChain;
    BigInt targetId = nodeId;
    for (int i = 1; i <= scopeChain.length; i++) {
      if (resolver.layout.isCollapsed(scopeChain.sublist(0, i))) {
        chain = scopeChain.sublist(0, i - 1);
        targetId = scopeChain[i - 1];
        break;
      }
    }

    final node = _resolveNodeInScope(view, chain, targetId);
    if (node == null) return;

    final scale = getZoomScale(_zoomLevel);
    // Screen footprint: HOFs use the cache-aware effective size so a grown
    // body is centered as a whole, matching what `NodeWidget` renders.
    final nodeSize = node.zone != null
        ? resolver.effectiveNodeSizeScreen(node, chain)
        : getNodeSize(node, _zoomLevel);
    final centerScreen =
        resolver.scopedToScreen(chain, apiVec2ToOffset(node.position)) +
            Offset(nodeSize.width / 2, nodeSize.height / 2);
    // Back to top-level logical space under the *current* pan, so the pan we
    // are about to install is well-defined.
    final nodeCenterLogical = screenToLogical(centerScreen, _panOffset, scale);

    // Pan offset so the node center maps to the anchor point:
    // anchor = (nodeCenterLogical + panOffset) * scale
    // panOffset = (anchor / scale) - nodeCenterLogical
    final anchor =
        screenAnchor ?? Offset(viewportSize.width / 2, viewportSize.height / 2);
    setState(() {
      _panOffset = (anchor / scale) - nodeCenterLogical;
      // Close the auto-framing gate. `updatePanOffsetForCurrentNetwork` runs
      // from post-frame callbacks keyed on `_currentNetworkName`, so a jump
      // that crosses a network switch would have its pan overwritten by the
      // top-left framing one frame later. Claiming the name here marks the pan
      // as already resolved for this network. (Same-network scrolls set it to
      // the value it already had — a no-op.)
      _currentNetworkName = view.name;
    });
  }

  /// Look up a node at [scopeChain] inside [view]. Returns null if the chain
  /// can't be walked (e.g. a body was deleted between the jump and the scroll).
  NodeView? _resolveNodeInScope(
      NodeNetworkView view, List<BigInt> scopeChain, BigInt nodeId) {
    Map<BigInt, NodeView> nodes = view.nodes;
    for (final hofId in scopeChain) {
      final zone = nodes[hofId]?.zone;
      if (zone == null) return null;
      nodes = zone.nodes;
    }
    return nodes[nodeId];
  }

  /// Handles a wire dropped in empty space (`doc/design_insert_node_on_wire.md`
  /// Phase 0): the Add Node popup filtered by the dragged pin's type, then a
  /// Rust plan of the ways the chosen node can connect, the
  /// [ArrangementDialog] only when there is more than one, and one commit —
  /// one undo step. The new node lands in the body whose interior the drop
  /// hit; Rust works out the wire's scope depth. Cancelling at any point
  /// leaves the network untouched.
  void _handleWireDropInEmptySpace(
      PinReference startPin, Offset dropPosition) async {
    final isOutput = startPin.isOutput;
    // For a drag *off* an input pin, the declared pin type may be deliberately
    // lossy (e.g. `map.f`'s `AnyFunction`, which carries the parameter prefix
    // but not the return type). Prefer the pin's concrete `dragHintType` when
    // present so the popup filter sees the full signature (Rust's plan reads
    // the same hint). See `doc/design_drag_aware_add_node.md` (Tier 2).
    final dataType = _dragSourceTypeFor(startPin);

    // Resolve the drop target's scope *before* opening the popup: the list is
    // filtered on it (a body scope hides node types that may not live in a
    // body — issue #417). The resolver is a pure function of the current model
    // state and the popup is modal, so the scope computed here is the same one
    // the post-popup code used to re-derive.
    final resolver = _makeResolver();
    if (resolver == null) return;
    final scope = resolver.findContainingScope(dropPosition);
    final logicalPosition = scope.bodyLocal;

    final selectedNodeType = await showAddNodePopup(
      context,
      filterByCompatibleType: dataType,
      draggingFromOutput: isOutput,
      inZoneBody: scope.scopeChain.isNotEmpty,
    );

    if (selectedNodeType == null || !mounted) return;

    final model = widget.graphModel;
    final plan =
        model.planWireDrop(startPin, scope.scopeChain, selectedNodeType);
    final refusal = plan.refusal;
    if (refusal != null) {
      showErrorSnackBar(context, refusal);
      return;
    }

    if (plan.arrangements.isEmpty) {
      // Nothing on the new node can take the wire: place it unconnected, as
      // the popup's choice still asks for it.
      model.createNode(
        selectedNodeType,
        logicalPosition,
        dragSource: APIDragSource(
          sourcePinType: dataType,
          draggingFromOutput: isOutput,
        ),
        scopeChain: scope.scopeChain,
      );
      return;
    }

    final arrangement = await _pickArrangement(
        plan.arrangements, 'Connect to $selectedNodeType');
    if (arrangement == null || !mounted) return;

    final error = model.addNodeAndConnect(
      startPin,
      scope.scopeChain,
      selectedNodeType,
      logicalPosition,
      arrangement,
    );
    if (error != null && mounted) showErrorSnackBar(context, error);
  }

  /// The arrangement to commit: the only one, or the user's pick in the
  /// [ArrangementDialog] when there are several (D3). Null when cancelled.
  Future<APIArrangement?> _pickArrangement(
      List<APIArrangement> arrangements, String title) async {
    if (arrangements.length == 1) return arrangements.first;
    final labels = arrangementRowLabels([
      for (final a in arrangements)
        ArrangementSides(
          inputName: a.input?.name,
          inputType: a.input?.dataType,
          outputName: a.output?.name,
          outputType: a.output?.dataType,
        ),
    ]);
    final index = await showArrangementDialog(
      context,
      title: title,
      rows: [
        for (final label in labels)
          ArrangementRow(
            label: label.label,
            typeLabel: label.typeLabel,
            color: getDataTypeColor(label.typeLabel),
          ),
      ],
    );
    return index == null ? null : arrangements[index];
  }

  /// The wire context menu (`doc/design_insert_node_on_wire.md` Phase 2):
  /// **Insert node…** on any wire, **Delete wire** on the ones the selection
  /// model can address (not body-return wires). The wire is selected first,
  /// as a left click would.
  Future<void> _showWireMenu(WireHitResult hit, TapDownDetails details,
      BuildContext context, StructureDesignerModel model) async {
    final deletable = hit.destArgumentKind == APIArgumentKind.external_;
    model.setActiveScopeChain(hit.scopeChain);
    if (deletable) {
      model.setSelectedWire(
        hit.sourceNodeId,
        hit.sourcePinIndex,
        hit.destNodeId,
        hit.destParamIndex,
        scopeChain: hit.scopeChain,
      );
    }

    final RenderBox overlay =
        Overlay.of(context).context.findRenderObject() as RenderBox;
    final value = await showMenu<String>(
      context: context,
      position: RelativeRect.fromRect(
        Rect.fromPoints(details.globalPosition, details.globalPosition),
        Offset.zero & overlay.size,
      ),
      items: [
        const PopupMenuItem(
          value: 'insert_node',
          child: Text('Insert node...'),
        ),
        if (deletable)
          const PopupMenuItem(
            value: 'delete_wire',
            child: Text('Delete wire'),
          ),
      ],
    );
    if (!mounted) return;
    if (value == 'insert_node') {
      await _insertNodeOnWire(hit, details.localPosition, model);
    } else if (value == 'delete_wire') {
      model.removeSelected(scopeChain: hit.scopeChain);
    }
  }

  /// Insert node on wire: the Add Node popup over exactly the types Rust can
  /// splice here (D10), a plan for the chosen one, the [ArrangementDialog]
  /// only when it is ambiguous, and one commit — one undo step. Flutter passes
  /// the clicked scope `C`; Rust derives the insertion scope from it (D6).
  /// Cancelling at any point leaves the network untouched.
  Future<void> _insertNodeOnWire(WireHitResult hit, Offset clickPosition,
      StructureDesignerModel model) async {
    final resolver = _makeResolver();
    if (resolver == null) return;
    final clickedScope = resolver.findContainingScope(clickPosition).scopeChain;
    final wireScope = hit.wireScope;
    final wire = hit.anchor;

    final categories = model.insertableNodeTypes(wireScope, wire, clickedScope);
    if (categories.isEmpty) {
      showTransientSnackBar(context, 'No node type can be inserted here');
      return;
    }
    // Rust has already dropped the types that may not live in the insertion
    // scope, which can be shallower than the clicked one, so no `inZoneBody`.
    final nodeType = await showAddNodePopup(context, categories: categories);
    if (nodeType == null || !mounted) return;
    await _spliceNewNode(model, nodeType, wireScope, wire, clickedScope);
  }

  /// Plan a new [nodeType] node into [wire], ask only when the plan is
  /// ambiguous, and commit — one undo step. Shared by the wire menu and the
  /// drop from the user-types panel (Phase 5).
  Future<void> _spliceNewNode(
      StructureDesignerModel model,
      String nodeType,
      List<BigInt> wireScope,
      APIWireAnchor wire,
      List<BigInt> clickedScope) async {
    final node = APINodeSource.new_(nodeTypeName: nodeType);
    final plan = model.planInsertOnWire(wireScope, wire, clickedScope, node);
    final refusal = plan.refusal;
    if (refusal != null) {
      showErrorSnackBar(context, refusal);
      return;
    }
    if (plan.arrangements.isEmpty) {
      // The list said it fits; the network changed in between.
      showErrorSnackBar(context, '$nodeType no longer fits this wire');
      return;
    }
    final arrangement =
        await _pickArrangement(plan.arrangements, 'Insert $nodeType');
    if (arrangement == null || !mounted) return;

    final error = model.insertNodeOnWire(
        wireScope, wire, clickedScope, node, arrangement);
    if (error != null && mounted) showErrorSnackBar(context, error);
  }

  /// Drag onto a wire (`doc/design_insert_node_on_wire.md` Phase 4): the
  /// node drag was released over a highlighted wire and its move is already
  /// committed (its own undo step). Plan again — the highlight's answer may
  /// be a whole drag old — ask only when the plan is ambiguous, and splice
  /// as a second step. Cancelling leaves the node where it was dropped.
  Future<void> _spliceDroppedNode(
      BigInt nodeId, List<BigInt> scopeChain, WireHitResult hit) async {
    final model = widget.graphModel;
    final node = APINodeSource.existing(nodeId: nodeId);
    final plan =
        model.planInsertOnWire(hit.wireScope, hit.anchor, scopeChain, node);
    final refusal = plan.refusal;
    if (refusal != null) {
      showErrorSnackBar(context, refusal);
      return;
    }
    if (plan.arrangements.isEmpty) return;
    final arrangement =
        await _pickArrangement(plan.arrangements, 'Insert node');
    if (arrangement == null || !mounted) return;
    final error = model.insertNodeOnWire(
        hit.wireScope, hit.anchor, scopeChain, node, arrangement);
    if (error != null && mounted) showErrorSnackBar(context, error);
  }

  /// The type to use as the drag source when a wire is dragged off [startPin].
  ///
  /// Normally the pin's declared [PinReference.dataType], but an external input
  /// pin may expose a concrete `dragHintType` that overrides a deliberately
  /// lossy declared type (e.g. `map.f`'s `AnyFunction` → the concrete
  /// `(input_type) -> output_type`). See `doc/design_drag_aware_add_node.md`.
  String _dragSourceTypeFor(PinReference startPin) {
    if (startPin.pinKind == PinKind.externalInput) {
      final node = _lookupNodeInScope(startPin.nodeId, startPin.scopeChain);
      if (node != null &&
          startPin.pinIndex >= 0 &&
          startPin.pinIndex < node.inputPins.length) {
        final hint = node.inputPins[startPin.pinIndex].dragHintType;
        if (hint != null && hint.isNotEmpty) return hint;
      }
    }
    return startPin.dataType;
  }

  /// Walk the model view to find a node by id at the given scope.
  NodeView? _lookupNodeInScope(BigInt nodeId, List<BigInt> scope) {
    final view = widget.graphModel.nodeNetworkView;
    if (view == null) return null;
    Map<BigInt, NodeView> current = view.nodes;
    for (final hofId in scope) {
      final hof = current[hofId];
      final zone = hof?.zone;
      if (zone == null) return null;
      current = zone.nodes;
    }
    return current[nodeId];
  }

  /// Construct a ScopeResolver tied to the current frame's view + transform.
  /// Returns null when there is no active node network — callers should bail.
  ScopeResolver? _makeResolver() {
    final view = widget.graphModel.nodeNetworkView;
    if (view == null) return null;
    return ScopeResolver(
      root: view,
      panOffset: _panOffset,
      scale: getZoomScale(_zoomLevel),
      zoomLevel: _zoomLevel,
    );
  }

  /// Calculate an appropriate pan offset based on node positions
  /// This is called whenever the active node network changes or when manually triggered
  /// via the View menu
  ///
  /// If forceUpdate is true, it will recalculate the pan offset even if the network hasn't changed
  void updatePanOffsetForCurrentNetwork({bool forceUpdate = false}) {
    final model = widget.graphModel;
    if (model.nodeNetworkView == null) return;

    // Skip if the network hasn't changed and we're not forcing an update
    if (!forceUpdate && _currentNetworkName == model.nodeNetworkView!.name) {
      return;
    }

    // Update the current network name
    _currentNetworkName = model.nodeNetworkView!.name;

    // Precedence chain (issue #414 Phase 4, `doc/design_find_usages.md` D7):
    //   anchored jump > stored viewport > top-left framing.
    // Anchored jumps never reach here — `_scrollToNode` closes the
    // `_currentNetworkName` gate itself. An explicit "reset view"
    // (forceUpdate) always re-frames the top-left node. Otherwise, restore
    // this network's stored canvas viewport (pan + zoom) if it has one, so
    // switching back lands where the user left off; fall through to top-left
    // framing when there is none (fresh network / old file).
    if (!forceUpdate) {
      final viewport = sd_api.getActiveNetworkCanvasViewport();
      if (viewport != null) {
        setState(() {
          _panOffset = Offset(viewport.panX, viewport.panY);
          _zoomLevel = zoomLevelFromIndex(viewport.zoomLevel);
        });
        return;
      }
    }

    // If there are no nodes, center the view
    if (model.nodeNetworkView!.nodes.isEmpty) {
      setState(() {
        _panOffset = Offset.zero;
      });
      return;
    }

    // Find the minimum x and y coordinates
    double minX = double.infinity;
    double minY = double.infinity;

    for (final node in model.nodeNetworkView!.nodes.values) {
      if (node.position.x < minX) minX = node.position.x;
      if (node.position.y < minY) minY = node.position.y;
    }

    // Set the pan offset to position the top-left node with a small margin
    const margin = 20.0;
    setState(() {
      _panOffset = Offset(-minX + margin, -minY + margin);
    });
  }

  /// Checks if the given screen position lands on any node, anywhere in the
  /// scope tree. In U1 the search is top-level only.
  bool _isClickOnNode(StructureDesignerModel model, Offset position) {
    final resolver = _makeResolver();
    if (resolver == null) return false;
    return resolver.isPositionOnNode(position);
  }

  /// Gets the node at the given screen position, if any. In U1 the search is
  /// top-level only.
  NodeView? getNodeAtPosition(StructureDesignerModel model, Offset position) {
    final resolver = _makeResolver();
    if (resolver == null) return null;
    return resolver.findNodeAtScreenPosition(position)?.node;
  }

  // ===== RECTANGLE SELECTION HELPERS =====

  /// Start rectangle selection at the given screen position. Records the
  /// containing scope so the final overlap test only picks nodes in that
  /// body (or top-level if the drag started outside any body).
  void _handleSelectionRectStart(Offset position) {
    final resolver = _makeResolver();
    final scope =
        resolver?.findContainingScope(position).scopeChain ?? const <BigInt>[];
    setState(() {
      _selectionRectStart = position;
      _selectionRect = Rect.fromPoints(position, position);
      _selectionRectScope = scope;
    });
  }

  /// Update the rectangle selection as the mouse moves
  void _handleSelectionRectUpdate(Offset position) {
    if (_selectionRectStart != null) {
      setState(() {
        _selectionRect = Rect.fromPoints(_selectionRectStart!, position);
      });
    }
  }

  /// Finish rectangle selection and apply to model. Confined to the scope
  /// the drag started in: a rectangle drawn inside a body picks body nodes
  /// only; a rectangle drawn at top-level picks top-level nodes only.
  void _handleSelectionRectEnd(StructureDesignerModel model) {
    final resolver = _makeResolver();
    if (_selectionRect == null || resolver == null) {
      _clearSelectionRect();
      return;
    }

    final rect = _selectionRect!;
    final scope = _selectionRectScope;

    // Walk the scope's nodes (top-level or body), testing screen-space
    // overlap. The resolver's layout cache gives the correct screen origin
    // for body-scope positions.
    final container = _nodesForScope(resolver, scope);
    final List<BigInt> nodesInRect = [];
    if (container != null) {
      for (final node in container.values) {
        final nodeScreenPos = resolver.scopedToScreen(
          scope,
          apiVec2ToOffset(node.position),
        );
        final nodeSize = getNodeSize(node, _zoomLevel);
        final nodeRect = Rect.fromLTWH(
          nodeScreenPos.dx,
          nodeScreenPos.dy,
          nodeSize.width,
          nodeSize.height,
        );
        if (rect.overlaps(nodeRect)) {
          nodesInRect.add(node.id);
        }
      }
    }

    // Collect wires in the same scope that overlap the rectangle. Rectangle
    // select is limited to *regular same-scope* wires (see `_wireRectSelectable`)
    // because `_wireOverlapsRect` resolves both endpoints in `scope`. Captures
    // and iteration-value references (whose source lives in an ancestor scope)
    // are still selectable by clicking directly on the wire — the click path's
    // hit-test (`findWireAtPosition`) resolves cross-scope endpoints. Zone-output
    // wires are never selectable.
    final scopeWires = _wiresForScope(resolver, scope);
    final List<WireView> wiresInRect = [];
    for (final wire in scopeWires) {
      if (!_wireRectSelectable(wire)) continue;
      if (_wireOverlapsRect(wire, rect, resolver, scope)) {
        wiresInRect.add(wire);
      }
    }

    // Apply selection based on modifier keys. The combined node+wire batch
    // APIs are scope-aware now, so the same path serves both top-level and
    // body scopes. The Rust side enforces the single-scope invariant.
    final isCtrl = HardwareKeyboard.instance.isControlPressed;
    final isShift = HardwareKeyboard.instance.isShiftPressed;

    // Move the active scope to the drag's scope so subsequent keyboard
    // shortcuts target it (a no-op for top-level).
    model.setActiveScopeChain(scope);
    if (isCtrl) {
      model.toggleNodesAndWiresSelection(nodesInRect, wiresInRect,
          scopeChain: scope);
    } else if (isShift) {
      model.addNodesAndWiresToSelection(nodesInRect, wiresInRect,
          scopeChain: scope);
    } else {
      model.selectNodesAndWires(nodesInRect, wiresInRect, scopeChain: scope);
    }

    _clearSelectionRect();
  }

  /// Whether [wire] can be picked up by a *rectangle* drag: a regular
  /// same-scope output wire (or the function pin). This is intentionally
  /// narrower than the click path (the painter's `_isSelectableWire`, which also
  /// accepts captures and iteration-value references): `_wireOverlapsRect`
  /// resolves both endpoints in the drag's scope, so it can only place
  /// same-scope wires. Cross-scope wires are selectable by clicking the wire.
  bool _wireRectSelectable(WireView wire) =>
      wire.sourcePin is APISourcePin_NodeOutput &&
      wire.sourceScopeDepth == 0 &&
      wire.destinationArgumentKind == APIArgumentKind.external_;

  /// Walk [resolver]'s root to the body identified by [scopeChain] and return
  /// its stored wire list (top-level wires for an empty chain). Returns an
  /// empty list if the path can't be resolved.
  List<WireView> _wiresForScope(
      ScopeResolver resolver, List<BigInt> scopeChain) {
    if (scopeChain.isEmpty) return resolver.root.wires;
    Map<BigInt, NodeView> current = resolver.root.nodes;
    for (int i = 0; i < scopeChain.length; i++) {
      final hof = current[scopeChain[i]];
      final zone = hof?.zone;
      if (zone == null) return const [];
      if (i == scopeChain.length - 1) return zone.wires;
      current = zone.nodes;
    }
    return const [];
  }

  /// Walk [resolver]'s root to the body identified by [scopeChain] and
  /// return its `nodes` map. Returns null if the path can't be resolved.
  Map<BigInt, NodeView>? _nodesForScope(
      ScopeResolver resolver, List<BigInt> scopeChain) {
    Map<BigInt, NodeView> current = resolver.root.nodes;
    for (final hofId in scopeChain) {
      final hof = current[hofId];
      final zone = hof?.zone;
      if (zone == null) return null;
      current = zone.nodes;
    }
    return current;
  }

  /// Clear the selection rectangle state
  void _clearSelectionRect() {
    setState(() {
      _selectionRect = null;
      _selectionRectStart = null;
      _selectionRectScope = const [];
    });
  }

  /// Check if a wire's Bezier curve overlaps the selection rectangle. Routes
  /// pin endpoint resolution through [ScopeResolver] so source/dest endpoints
  /// match the painter's positions exactly (the duplicate pin-position math
  /// that used to live here is gone — see `doc/design_zones_ui.md` §R2).
  bool _wireOverlapsRect(
      WireView wire, Rect rect, ScopeResolver resolver, List<BigInt> scope) {
    // Only regular same-scope wires reach here (filtered by `_wireSelectable`),
    // so both endpoints live in `scope`.
    final sourcePin = PinReference(
      nodeId: wire.sourceNodeId,
      scopeChain: scope,
      pinKind: wire.sourceOutputPinIndex == -1
          ? PinKind.functionPin
          : PinKind.externalOutput,
      pinIndex: wire.sourceOutputPinIndex,
      dataType: '',
    );
    final destPin = PinReference(
      nodeId: wire.destNodeId,
      scopeChain: scope,
      pinKind: PinKind.externalInput,
      pinIndex: wire.destParamIndex.toInt(),
      dataType: '',
    );

    final source = resolver.tryPinScreenPosition(sourcePin);
    final dest = resolver.tryPinScreenPosition(destPin);
    if (source == null || dest == null) return false;
    final sourcePos = source.$1;
    final destPos = dest.$1;

    // Quick bounding box check first
    final wireBounds = _getWireBoundingBox(sourcePos, destPos);
    if (!rect.overlaps(wireBounds)) return false;

    // Sample points along the Bezier curve and check if any are in rect
    const samples = 20;
    for (int i = 0; i <= samples; i++) {
      final t = i / samples;
      final point = _sampleBezierPoint(sourcePos, destPos, t);
      if (rect.contains(point)) return true;
    }

    return false;
  }

  /// Get bounding box for a Bezier wire
  Rect _getWireBoundingBox(Offset start, Offset end) {
    // Use CUBIC_SPLINE_HORIZ_OFFSET as the control point offset
    final cp1 = Offset(start.dx + CUBIC_SPLINE_HORIZ_OFFSET, start.dy);
    final cp2 = Offset(end.dx - CUBIC_SPLINE_HORIZ_OFFSET, end.dy);

    final minX =
        [start.dx, end.dx, cp1.dx, cp2.dx].reduce((a, b) => math.min(a, b));
    final maxX =
        [start.dx, end.dx, cp1.dx, cp2.dx].reduce((a, b) => math.max(a, b));
    final minY =
        [start.dy, end.dy, cp1.dy, cp2.dy].reduce((a, b) => math.min(a, b));
    final maxY =
        [start.dy, end.dy, cp1.dy, cp2.dy].reduce((a, b) => math.max(a, b));

    return Rect.fromLTRB(minX, minY, maxX, maxY);
  }

  /// Sample a point on the Bezier curve at parameter t (0..1)
  Offset _sampleBezierPoint(Offset start, Offset end, double t) {
    final cp1 = Offset(start.dx + CUBIC_SPLINE_HORIZ_OFFSET, start.dy);
    final cp2 = Offset(end.dx - CUBIC_SPLINE_HORIZ_OFFSET, end.dy);

    // Cubic Bezier: B(t) = (1-t)³P0 + 3(1-t)²tP1 + 3(1-t)t²P2 + t³P3
    final u = 1 - t;
    final tt = t * t;
    final uu = u * u;
    final uuu = uu * u;
    final ttt = tt * t;

    return Offset(
      uuu * start.dx + 3 * uu * t * cp1.dx + 3 * u * tt * cp2.dx + ttt * end.dx,
      uuu * start.dy + 3 * uu * t * cp1.dy + 3 * u * tt * cp2.dy + ttt * end.dy,
    );
  }

  /// Build the selection rectangle overlay widget. When the drag started
  /// inside an HOF body, the rendered rectangle is clipped to that body's
  /// screen rect so the selection visually stays inside the body — matching
  /// the overlap-test scope confinement done at drag end. See
  /// `doc/design_zones_ui.md` §"Phase U7" → selection-rect clipping.
  Widget _buildSelectionRectangle() {
    if (_selectionRect == null) return const SizedBox.shrink();

    Rect rendered = _selectionRect!;
    if (_selectionRectScope.isNotEmpty) {
      final resolver = _makeResolver();
      final origin = resolver?.layout.lookupOrigin(_selectionRectScope);
      final size = resolver?.layout.lookupSize(_selectionRectScope);
      if (origin != null && size != null) {
        final bodyRect = origin & (size * getZoomScale(_zoomLevel));
        rendered = rendered.intersect(bodyRect);
      }
    }
    if (rendered.isEmpty) return const SizedBox.shrink();

    return Positioned.fromRect(
      rect: rendered,
      child: IgnorePointer(
        child: Container(
          decoration: BoxDecoration(
            border: Border.all(color: Colors.blue, width: 1),
            color: Colors.blue.withValues(alpha: 0.1),
          ),
        ),
      ),
    );
  }

  /// Handles tap down in the main area
  void _handleTapDown(TapDownDetails details) {
    // A tap that lands on a note currently open for in-place editing (issue
    // #421) belongs to that note's text field. This handler runs on the 100 ms
    // press deadline — *before* the gesture arena resolves in the field's
    // favour — so grabbing canvas focus here would pull the caret out on every
    // click inside the note. Every other tap position still takes focus, which
    // is exactly what commits and closes the in-place editor.
    final editing = widget.graphModel.inPlaceEditRef;
    if (editing != null) {
      final hit =
          _makeResolver()?.findNodeAtScreenPosition(details.localPosition);
      if (hit != null &&
          widget.graphModel.isInPlaceEditTarget(hit.node.id, hit.scopeChain)) {
        return;
      }
    }
    focusNode.requestFocus();
  }

  /// Handles secondary (right-click) tap for context menu
  Future<void> _handleSecondaryTapDown(TapDownDetails details,
      BuildContext context, StructureDesignerModel model) async {
    // Don't show context menu if Shift is pressed (used for panning)
    if (HardwareKeyboard.instance.isShiftPressed) {
      return;
    }
    // A linked network is read-only: the background menu only creates
    // (Add Node / Paste), so it is not offered at all.
    if (model.activeNetworkReadOnly) {
      focusNode.requestFocus();
      return;
    }

    // Only show context menu if clicked on empty space (not on a node)
    // The nodes have their own context menu handling
    if (!_isClickOnNode(model, details.localPosition)) {
      // A wire under the click gets the wire menu instead of the background
      // one. Body-return wires count too: they take an inserted node.
      final wireHit = NodeNetworkPainter(model,
              panOffset: _panOffset, zoomLevel: _zoomLevel)
          .findWireAtPosition(details.localPosition, includeBodyReturn: true);
      if (wireHit != null) {
        await _showWireMenu(wireHit, details, context, model);
        focusNode.requestFocus();
        return;
      }

      final resolver = _makeResolver();
      // The scope the click landed in. Right-clicking inside an HOF body
      // resolves to that body's scope so the new node is created inside the
      // body. See `doc/design_zones_ui.md` §"Add Node popup".
      final scopeHit = resolver?.findContainingScope(details.localPosition);
      final scopeChain = scopeHit?.scopeChain ?? const <BigInt>[];
      final logicalPosition = scopeHit?.bodyLocal ??
          screenToLogical(
              details.localPosition, _panOffset, getZoomScale(_zoomLevel));

      // Make the clicked scope active so subsequent keyboard shortcuts target
      // the right body.
      model.setActiveScopeChain(scopeChain);

      if (model.hasClipboardContent()) {
        final RenderBox overlay =
            Overlay.of(context).context.findRenderObject() as RenderBox;
        final RelativeRect position = RelativeRect.fromRect(
          Rect.fromPoints(details.globalPosition, details.globalPosition),
          Offset.zero & overlay.size,
        );

        final value = await showMenu<String>(
          context: context,
          position: position,
          items: [
            const PopupMenuItem(
              value: 'add_node',
              child: Text('Add Node...'),
            ),
            const PopupMenuItem(
              value: 'paste',
              child: Text('Paste (Ctrl+V)'),
            ),
          ],
        );

        if (!context.mounted) return;
        if (value == 'add_node') {
          String? selectedNode = await showAddNodePopup(context,
              inZoneBody: scopeChain.isNotEmpty);
          if (selectedNode != null) {
            model.createNode(selectedNode, logicalPosition,
                scopeChain: scopeChain);
          }
        } else if (value == 'paste') {
          final error = model.pasteAtPosition(
              logicalPosition.dx, logicalPosition.dy,
              scopeChain: scopeChain);
          if (error != null && context.mounted) {
            showErrorDialog(
                context: context, title: 'Cannot Paste', message: error);
          }
        }
      } else {
        String? selectedNode =
            await showAddNodePopup(context, inZoneBody: scopeChain.isNotEmpty);
        if (selectedNode != null) {
          model.createNode(selectedNode, logicalPosition,
              scopeChain: scopeChain);
        }
      }
    }
    focusNode.requestFocus();
  }

  /// Whether a network dragged out of the user-types panel may be dropped on
  /// this canvas. The candidates are exactly the custom networks the Add Node
  /// popup offers (Rust's `get_node_type_views` — which leaves out networks
  /// under a transitive library mount), so the drop can never place what the
  /// popup would not. Queried once per drag-enter, not per move.
  bool _canDropUserType(StructureDesignerModel model, UserTypeDragData data) {
    final name = data.placeableNetworkName;
    if (name == null) return false;
    if (model.nodeNetworkView == null || model.activeNetworkReadOnly) {
      return false;
    }
    final categories = sd_api.getNodeTypeViews() ?? const [];
    return categories.any((c) =>
        c.category == NodeTypeCategory.custom &&
        c.nodes.any((n) => n.name == name));
  }

  /// Places the dropped network as a node, its top-left at the pointer. The
  /// scope is resolved the way right-click → Add Node resolves it, so a drop
  /// inside an HOF / closure body lands in that body.
  void _dropUserType(
      StructureDesignerModel model, String networkName, Offset globalPosition) {
    final box = context.findRenderObject() as RenderBox?;
    if (box == null) return;
    final localPosition = box.globalToLocal(globalPosition);
    final scopeHit = _makeResolver()?.findContainingScope(localPosition);
    final scopeChain = scopeHit?.scopeChain ?? const <BigInt>[];
    final logicalPosition = scopeHit?.bodyLocal ??
        screenToLogical(localPosition, _panOffset, getZoomScale(_zoomLevel));
    model.setActiveScopeChain(scopeChain);
    model.createNode(networkName, logicalPosition, scopeChain: scopeChain);
    focusNode.requestFocus();
  }

  /// Asks Rust whether the dragged network can be inserted into a wire,
  /// once per (wire, clicked scope) per drag (`doc/design_insert_node_on_wire.md`
  /// Phase 5). The clicked scope is part of the key: a capture wire splices
  /// at a different depth either side of the body border.
  PerDragPlanMemo<(WireHitResult, List<BigInt>)>? _userTypeSpliceMemo;

  /// Whether the drag hovering the canvas was accepted on entry. `onMove`
  /// also fires for a rejected drag, which must light nothing up.
  bool _userTypeDropAccepted = false;

  void _endUserTypeDrag(StructureDesignerModel model) {
    _userTypeSpliceMemo = null;
    _userTypeDropAccepted = false;
    model.setUserTypeDropWire(null);
  }

  /// The wire under [globalPosition] that [networkName] can be inserted
  /// into, with the clicked scope `C` there, or null. A pointer over a node
  /// targets no wire, as for the wire context menu.
  (WireHitResult, List<BigInt>)? _userTypeSpliceTarget(
      StructureDesignerModel model, String networkName, Offset globalPosition) {
    final box = context.findRenderObject() as RenderBox?;
    final resolver = _makeResolver();
    if (box == null || resolver == null) return null;
    final localPosition = box.globalToLocal(globalPosition);
    if (resolver.isPositionOnNode(localPosition)) return null;
    final hit =
        NodeNetworkPainter(model, panOffset: _panOffset, zoomLevel: _zoomLevel)
            .findWireAtPosition(localPosition,
                includeBodyReturn: true, resolver: resolver);
    if (hit == null) return null;
    final clickedScope = resolver.findContainingScope(localPosition).scopeChain;
    final node = APINodeSource.new_(nodeTypeName: networkName);
    final memo =
        _userTypeSpliceMemo ??= PerDragPlanMemo<(WireHitResult, List<BigInt>)>(
      (w) => model
          .planInsertOnWire(w.$1.wireScope, w.$1.anchor, w.$2, node)
          .arrangements
          .isNotEmpty,
      (w) => '${w.$1.key}@${w.$2.join('/')}',
    );
    final target = (hit, clickedScope);
    return memo.accepts(target) ? target : null;
  }

  /// Wraps the canvas in the drop target for networks dragged out of the
  /// user-types panel. [canvas] stays the first child of the builder's
  /// `Stack` whether or not a drag hovers, so the highlight never remounts
  /// the node widgets. A wire the network can be inserted into lights up
  /// under the pointer (painted through [StructureDesignerModel.dragRepaint],
  /// never a rebuild); dropping there splices it in.
  Widget _buildUserTypeDropTarget(StructureDesignerModel model, Widget canvas) {
    return DragTarget<UserTypeDragData>(
      onWillAcceptWithDetails: (details) {
        _endUserTypeDrag(model);
        return _userTypeDropAccepted = _canDropUserType(model, details.data);
      },
      onMove: (details) {
        final name = details.data.placeableNetworkName;
        if (!_userTypeDropAccepted || name == null) return;
        model.setUserTypeDropWire(
            _userTypeSpliceTarget(model, name, details.offset)?.$1);
      },
      onLeave: (_) => _endUserTypeDrag(model),
      onAcceptWithDetails: (details) {
        final name = details.data.placeableNetworkName;
        final target = name == null
            ? null
            : _userTypeSpliceTarget(model, name, details.offset);
        _endUserTypeDrag(model);
        if (name == null) return;
        if (target == null) {
          _dropUserType(model, name, details.offset);
          return;
        }
        final (hit, clickedScope) = target;
        model.setActiveScopeChain(hit.scopeChain);
        _spliceNewNode(model, name, hit.wireScope, hit.anchor, clickedScope)
            .then((_) {
          if (mounted) focusNode.requestFocus();
        });
      },
      builder: (context, candidates, rejected) {
        return Stack(
          fit: StackFit.passthrough,
          children: [
            canvas,
            if (candidates.isNotEmpty)
              Positioned.fill(
                child: IgnorePointer(
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      border: Border.all(
                        color: Theme.of(context).colorScheme.primary,
                        width: 2,
                      ),
                    ),
                  ),
                ),
              ),
          ],
        );
      },
    );
  }

  // Left-click panning has been replaced by middle mouse button panning

  /// Builds the stack children for the node network. HOF body nodes are
  /// flattened into the same Stack — each is positioned via the scope
  /// resolver against its scope chain. This keeps the widget tree shallow
  /// and lets every node share the same pan/zoom transform via the resolver,
  /// rather than nesting body widgets inside their HOF.
  List<Widget> _buildStackChildren(StructureDesignerModel model) {
    final view = model.nodeNetworkView;
    if (view == null) {
      return [];
    }

    final List<Widget> children = [];
    // Bottom wire layer: grid + top-level wires only. Top-level wires render
    // *under* the node widgets so external pin circles cover the wire ends
    // (the conventional look that preserves visual symmetry with the pre-zones
    // editor).
    children.add(NodeNetworkInteractionLayer(
        model: model, panOffset: _panOffset, zoomLevel: _zoomLevel));

    // Build a resolver once and reuse it so we don't pay the layout-pass
    // cost twice during the same frame (the painter constructs its own;
    // future work could share the cache via Provider).
    final resolver = ScopeResolver(
      root: view,
      panOffset: _panOffset,
      scale: getZoomScale(_zoomLevel),
      zoomLevel: _zoomLevel,
    );
    appendCanvasNodeWidgets(
      children: children,
      rootView: view,
      resolver: resolver,
      builder: (node, scopeChain, nodeResolver) =>
          _buildNodeWidget(node, scopeChain, view, nodeResolver),
    );

    // Top wire layer (overlay): body wires + dragged wire. These need to
    // paint above the node widgets — the HOF's body Container has an opaque
    // background that would otherwise hide wires drawn underneath it. See
    // `doc/design_zones_ui.md` §"Wire rendering across scopes".
    children.add(NodeNetworkInteractionLayer(
        model: model,
        panOffset: _panOffset,
        zoomLevel: _zoomLevel,
        overlay: true));

    // Open-port `+` tabs, shown only while a wire is dragged. Above the nodes
    // so a tab hanging over the node beneath it still takes the drop, and
    // outside every node's footprint so no node grows or moves. Not canvas
    // *content*: the image export never shows them. See `open_port_tabs.dart`.
    children.add(Positioned.fill(
        child: OpenPortTabLayer(model: model, resolver: resolver)));

    children.add(_buildSelectionRectangle());
    return children;
  }

  /// Build the Stack child for a single node in [scopeChain] — the interactive
  /// canvas's node builder, handed to [appendCanvasNodeWidgets]. The widget
  /// itself comes from the shared [canvasNodeWidget]; what this adds is the
  /// drag fast path.
  ///
  /// The node widget is wrapped in a [ListenableBuilder] on the model's
  /// `dragRepaint` notifier — the node-drag fast path. Drag ticks bump that
  /// notifier instead of `notifyListeners()`, so on each tick only the
  /// drag-affected nodes (the dragged ones, plus a dragged HOF's body nodes,
  /// whose screen positions follow it) rebuild with a fresh resolver; every
  /// other node returns the identical prebuilt [child], which
  /// `Element.updateChild` short-circuits — no rebuild, no re-layout. This is
  /// what keeps dragging O(dragged) instead of O(network).
  ///
  /// The trace menu's hover highlight (`model.traceHoveredNodeId`) and the
  /// canvas hover (`model.canvasHover`) ride the same wrapper: a hover change
  /// rebuilds only the nodes whose highlight differs from the one their
  /// prebuilt child was drawn with.
  Widget _buildNodeWidget(
    NodeView node,
    List<BigInt> scopeChain,
    NodeNetworkView rootView,
    ScopeResolver resolver,
  ) {
    final model = widget.graphModel;
    final builtHighlighted = model.isTraceHovered(node.id, scopeChain);
    final builtHovered = model.isNodeHovered(node.id, scopeChain);
    return ListenableBuilder(
      // The scoped key sits on the Stack's direct child (this wrapper) so
      // sibling reconciliation keeps working; see NodeWidgetKeys.nodeWidget.
      key: NodeWidgetKeys.nodeWidget(node.id, scopeChain: scopeChain),
      listenable: Listenable.merge(
          [model.dragRepaint, model.traceHoveredNodeId, model.canvasHover]),
      builder: (context, child) {
        final dragAffected = model.isNodeDragAffected(node.id, scopeChain);
        final highlighted = model.isTraceHovered(node.id, scopeChain);
        final hovered = model.isNodeHovered(node.id, scopeChain);
        if (!dragAffected &&
            highlighted == builtHighlighted &&
            hovered == builtHovered) {
          return child!;
        }
        return _nodeWidgetFor(node, scopeChain, rootView,
            dragAffected ? _dragFrameResolver(rootView) : resolver,
            traceHighlighted: highlighted, hovered: hovered);
      },
      child: _nodeWidgetFor(node, scopeChain, rootView, resolver,
          traceHighlighted: builtHighlighted, hovered: builtHovered),
    );
  }

  Widget _nodeWidgetFor(
    NodeView node,
    List<BigInt> scopeChain,
    NodeNetworkView rootView,
    ScopeResolver resolver, {
    bool traceHighlighted = false,
    bool hovered = false,
  }) =>
      canvasNodeWidget(
        node: node,
        scopeChain: scopeChain,
        rootView: rootView,
        resolver: resolver,
        panOffset: _panOffset,
        zoomLevel: _zoomLevel,
        titleMode: widget.graphModel.nodeTitleMode,
        // Non-null only for a custom node whose network has a thumbnail: the
        // lookup is by network name, and only networks are in the list.
        thumbnail: widget.graphModel.networkThumbnailImage(node.nodeTypeName),
        traceHighlighted: traceHighlighted,
        hovered: hovered,
      );

  /// Resolver for drag-tick rebuilds of drag-affected nodes. The build-time
  /// resolver is stale during a drag (a dragged HOF's body origin moves every
  /// tick), so affected nodes need a fresh layout pass — but only ONE per
  /// tick, shared across all affected nodes, not one per node.
  int _dragResolverTick = -1;
  ScopeResolver? _dragResolver;
  ScopeResolver _dragFrameResolver(NodeNetworkView rootView) {
    final tick = widget.graphModel.dragRepaint.value;
    if (_dragResolver == null || _dragResolverTick != tick) {
      _dragResolver = ScopeResolver(
        root: rootView,
        panOffset: _panOffset,
        scale: getZoomScale(_zoomLevel),
        zoomLevel: _zoomLevel,
      );
      _dragResolverTick = tick;
    }
    return _dragResolver!;
  }

  /// Handle pointer down event - check for middle mouse button, Shift + right mouse, or left-click for rectangle selection
  void _handlePointerDown(PointerDownEvent event) {
    _clearHover();
    _trackDragPointer(event);
    _autoPannedSinceMove = false;
    _autoPannedThisGesture = false;
    // Check if middle mouse button (button 2)
    if (event.buttons == kTertiaryButton) {
      setState(() {
        _isMiddleMousePanning = true;
        _lastPanPosition = event.position;
      });
    }
    // Check for Shift + right mouse button
    else if (event.buttons == kSecondaryMouseButton &&
        HardwareKeyboard.instance.isShiftPressed) {
      setState(() {
        _isShiftRightMousePanning = true;
        _lastPanPosition = event.position;
      });
    }
    // Left-click on empty space starts rectangle selection
    else if (event.buttons == kPrimaryButton) {
      if (!_isClickOnNode(widget.graphModel, event.localPosition)) {
        _handleSelectionRectStart(event.localPosition);
      }
    }
  }

  /// Track the cursor during plain hover (no button pressed) so Ctrl+V knows
  /// where the mouse is. `onPointerMove` only fires while a button is held, so
  /// without this `_lastMousePosition` would stay frozen at the end of the
  /// last drag and Ctrl+V would paste into the wrong scope (the stale point's
  /// containing scope) instead of the zone actually under the cursor.
  void _handlePointerHover(PointerHoverEvent event) {
    _lastMousePosition = event.localPosition;
    _scheduleHoverUpdate(event.localPosition);
  }

  // ===== HOVER HIGHLIGHT =====
  // The node or wire under the pointer (`model.canvasHover`). Hover events
  // come only while no button is held, so no drag ever updates it; a press
  // clears it and the release re-hit-tests where the pointer stopped.

  Offset? _pendingHoverPosition;
  bool _hoverFrameScheduled = false;

  /// Hit-test [position] for the hover highlight at the next frame, not on
  /// every mouse event: the wire hit test builds a band per wire.
  void _scheduleHoverUpdate(Offset position) {
    _pendingHoverPosition = position;
    if (_hoverFrameScheduled) return;
    _hoverFrameScheduled = true;
    SchedulerBinding.instance.scheduleFrameCallback((_) {
      _hoverFrameScheduled = false;
      final pending = _pendingHoverPosition;
      _pendingHoverPosition = null;
      if (!mounted || pending == null) return;
      widget.graphModel.canvasHover.value = _hoverAt(pending);
    });
  }

  void _clearHover() {
    _pendingHoverPosition = null;
    widget.graphModel.canvasHover.value = null;
  }

  /// The node under [position], else the wire, as the click tests rank them
  /// (a node wins over a wire passing under it). Comment notes are not
  /// highlighted, and a wire under one is not reached.
  CanvasHover? _hoverAt(Offset position) {
    final model = widget.graphModel;
    final resolver = _makeResolver();
    if (resolver == null) return null;
    final nodeHit = resolver.findNodeAtScreenPosition(position);
    if (nodeHit != null) {
      return nodeHit.node.nodeTypeName == 'Comment'
          ? null
          : CanvasHover.node(nodeHit.node.id, nodeHit.scopeChain);
    }
    final wire =
        NodeNetworkPainter(model, panOffset: _panOffset, zoomLevel: _zoomLevel)
            .findWireAtPosition(position,
                includeBodyReturn: true, resolver: resolver);
    return wire == null ? null : CanvasHover.wire(wire.key);
  }

  /// Handle pointer move event for panning (middle mouse or Shift + right mouse) and rectangle selection
  void _handlePointerMove(PointerMoveEvent event) {
    // Always track mouse position for paste (Ctrl+V)
    _lastMousePosition = event.localPosition;
    _trackDragPointer(event);
    _autoPannedSinceMove = false;

    if ((_isMiddleMousePanning || _isShiftRightMousePanning) &&
        _lastPanPosition != null) {
      setState(() {
        // Convert screen-space delta to logical-space delta
        final scale = getZoomScale(_zoomLevel);
        final screenDelta = event.position - _lastPanPosition!;
        _panOffset += screenDelta / scale;
        _lastPanPosition = event.position;
        _didPan = true;
      });
    }
    // Rectangle selection - just update the visual, no Rust calls during drag
    else if (_selectionRectStart != null && event.buttons == kPrimaryButton) {
      _handleSelectionRectUpdate(event.localPosition);
    }
    _updateEdgePan();
  }

  /// Handle pointer up event to end panning or rectangle selection
  void _handlePointerUp(PointerUpEvent event) {
    _scheduleHoverUpdate(event.localPosition);
    // Before any drop: this Listener sees the up ahead of the gesture
    // recognizers, so the release check reads where the pointer let go.
    _trackDragPointer(event);
    _endEdgePanGesture();
    if (_isMiddleMousePanning || _isShiftRightMousePanning) {
      final didPan = _didPan;
      setState(() {
        _isMiddleMousePanning = false;
        _isShiftRightMousePanning = false;
        _lastPanPosition = null;
        _didPan = false;
      });
      // Persist the settled pan (end of gesture, not per-frame). Skip a bare
      // click with no drag so it doesn't needlessly dirty the design.
      if (didPan) _syncCanvasViewportToKernel();
    }
    // Left-button release that began on non-node space. Distinguish a *tap*
    // (click — select/clear a wire) from a *drag* (rectangle selection). A
    // tap leaves the rectangle at (near) zero size. Handling the tap here, in
    // the outer Listener, is what makes wire selection work **inside HOF body
    // regions**: the bottom-layer painter's old GestureDetector sat below the
    // body region in the Stack and never received body-interior taps, whereas
    // this Listener (an ancestor of the whole canvas) always does.
    else if (_selectionRectStart != null) {
      final rect = _selectionRect;
      final isTap = rect == null ||
          (rect.width <= _TAP_MAX_DRAG && rect.height <= _TAP_MAX_DRAG);
      if (isTap) {
        _handleCanvasTap(event.localPosition);
        _clearSelectionRect();
      } else {
        _handleSelectionRectEnd(widget.graphModel);
      }
    }
  }

  /// Max screen-space drag extent (px) still treated as a click rather than a
  /// rectangle selection. Tolerates pointer jitter on a tap.
  static const double _TAP_MAX_DRAG = 4.0;

  /// Handle a left-click that wasn't a drag, on non-node canvas, in **any
  /// scope**. Hit-tests wires across all scopes (the painter resolves
  /// cross-scope endpoints); on a hit, selects the wire in its own scope
  /// (Ctrl toggles, Shift adds) and makes that scope active so keyboard ops
  /// (Delete) target it. On a miss, resets to the top level and clears
  /// selection everywhere. The Rust side enforces the single-scope invariant.
  void _handleCanvasTap(Offset localPosition) {
    final model = widget.graphModel;
    final painter =
        NodeNetworkPainter(model, panOffset: _panOffset, zoomLevel: _zoomLevel);
    final hit = painter.findWireAtPosition(localPosition);
    if (hit != null) {
      model.setActiveScopeChain(hit.scopeChain);
      if (HardwareKeyboard.instance.isControlPressed) {
        model.toggleWireSelection(
          hit.sourceNodeId,
          hit.sourcePinIndex.toInt(),
          hit.destNodeId,
          hit.destParamIndex,
          scopeChain: hit.scopeChain,
        );
      } else if (HardwareKeyboard.instance.isShiftPressed) {
        model.addWireToSelection(
          hit.sourceNodeId,
          hit.sourcePinIndex.toInt(),
          hit.destNodeId,
          hit.destParamIndex,
          scopeChain: hit.scopeChain,
        );
      } else {
        model.setSelectedWire(
          hit.sourceNodeId,
          hit.sourcePinIndex,
          hit.destNodeId,
          hit.destParamIndex,
          scopeChain: hit.scopeChain,
        );
      }
    } else {
      // Empty canvas: reset to the top level and clear selection everywhere so
      // no body's selection visually lingers.
      model.setActiveScopeChain(const <BigInt>[]);
      model.clearSelectionAllScopes();
    }
  }

  /// Handle trackpad/Magic Mouse pan-zoom start
  void _handlePointerPanZoomStart(PointerPanZoomStartEvent event) {
    // Initialize pan-zoom gesture if needed
  }

  /// Handle trackpad/Magic Mouse pan-zoom updates for panning
  void _handlePointerPanZoomUpdate(PointerPanZoomUpdateEvent event) {
    // Only handle panning when Shift is pressed
    if (HardwareKeyboard.instance.isShiftPressed &&
        (event.panDelta.dx.abs() > 0.1 || event.panDelta.dy.abs() > 0.1)) {
      setState(() {
        // Convert screen-space delta to logical-space delta
        final scale = getZoomScale(_zoomLevel);
        _panOffset += event.panDelta / scale;
      });
    }
  }

  /// Handle trackpad/Magic Mouse pan-zoom end
  void _handlePointerPanZoomEnd(PointerPanZoomEndEvent event) {
    // Clean up pan-zoom gesture if needed
  }

  /// Handle mouse scroll for zooming with zoom-to-cursor behavior
  void _handlePointerScroll(PointerScrollEvent event) {
    // The canvas moves under a still pointer.
    _scheduleHoverUpdate(event.localPosition);
    // Determine new zoom level
    ZoomLevel newZoomLevel = _zoomLevel;

    if (event.scrollDelta.dy > 0) {
      // Zoom out
      switch (_zoomLevel) {
        case ZoomLevel.normal:
          newZoomLevel = ZoomLevel.zoomedOutMedium;
          break;
        case ZoomLevel.zoomedOutMedium:
          newZoomLevel = ZoomLevel.zoomedOutFar;
          break;
        case ZoomLevel.zoomedOutFar:
          return; // Already at max zoom out
      }
    } else if (event.scrollDelta.dy < 0) {
      // Zoom in
      switch (_zoomLevel) {
        case ZoomLevel.normal:
          return; // Already at max zoom in
        case ZoomLevel.zoomedOutMedium:
          newZoomLevel = ZoomLevel.normal;
          break;
        case ZoomLevel.zoomedOutFar:
          newZoomLevel = ZoomLevel.zoomedOutMedium;
          break;
      }
    } else {
      return;
    }

    // Calculate new pan offset to keep cursor position fixed
    // The point under the cursor in logical space should remain under the cursor
    final oldScale = getZoomScale(_zoomLevel);
    final newScale = getZoomScale(newZoomLevel);

    // Convert cursor position from screen to logical coordinates
    final cursorScreen = event.localPosition;
    final cursorLogical = screenToLogical(cursorScreen, _panOffset, oldScale);

    // Calculate new pan offset so that cursorLogical maps back to cursorScreen
    // cursorScreen = (cursorLogical + newPanOffset) * newScale
    // newPanOffset = (cursorScreen / newScale) - cursorLogical
    final newPanOffset = (cursorScreen / newScale) - cursorLogical;

    setState(() {
      _zoomLevel = newZoomLevel;
      _panOffset = newPanOffset;
    });
    // Each discrete zoom step is its own settle point — persist it.
    _syncCanvasViewportToKernel();
  }

  // ===== EDGE AUTO-PAN =====

  void _trackDragPointer(PointerEvent event) {
    _dragPointerLocal = event.localPosition;
    _dragPointerGlobal = event.position;
  }

  Size? get _canvasSize {
    final box = context.findRenderObject() as RenderBox?;
    return box != null && box.hasSize ? box.size : null;
  }

  /// Whether one of the drags edge auto-pan serves is in progress (D12): a
  /// new or moved wire (not a cancelled move), a node, or a selection
  /// rectangle. A middle-mouse pan already moves the canvas by hand.
  bool get _edgePanDragActive {
    if (_isMiddleMousePanning || _isShiftRightMousePanning) return false;
    final model = widget.graphModel;
    final move = model.wireMove;
    return model.draggedWire != null ||
        (move != null && !move.cancelled) ||
        model.isNodeDragInProgress ||
        _selectionRectStart != null;
  }

  /// Start or stop the ticker for the pointer's current place.
  void _updateEdgePan() {
    final pointer = _dragPointerLocal;
    final size = _canvasSize;
    final wanted = pointer != null &&
        size != null &&
        _edgePanDragActive &&
        edgePanVelocity(pointer, size) != Offset.zero;
    if (wanted && !_edgePanTicker.isActive) {
      _edgePanLastElapsed = Duration.zero;
      _edgePanTicker.start();
    } else if (!wanted && _edgePanTicker.isActive) {
      _edgePanTicker.stop();
    }
  }

  /// One frame of edge auto-pan: pan the canvas, and keep what is dragged
  /// under the pointer (D13). A wire's free end needs nothing — it is stored
  /// in canvas-local pixels, which is where the pointer still is.
  void _onEdgePanTick(Duration elapsed) {
    final dt = math.min(
        (elapsed - _edgePanLastElapsed).inMicroseconds / 1e6, EDGE_PAN_MAX_DT);
    _edgePanLastElapsed = elapsed;
    final pointer = _dragPointerLocal;
    final size = _canvasSize;
    if (!mounted || pointer == null || size == null || !_edgePanDragActive) {
      _edgePanTicker.stop();
      return;
    }
    final velocity = edgePanVelocity(pointer, size);
    if (velocity == Offset.zero) {
      _edgePanTicker.stop();
      return;
    }
    if (dt <= 0) return;
    final step = edgePanStep(velocity, dt, getZoomScale(_zoomLevel));
    final model = widget.graphModel;
    if (model.isNodeDragInProgress) model.nudgeDraggedNodes(step.nodeDelta);
    setState(() {
      _panOffset += step.panDelta;
      final start = _selectionRectStart;
      if (start != null) {
        final shifted = start + step.rectStartShift;
        _selectionRectStart = shifted;
        _selectionRect = Rect.fromPoints(shifted, pointer);
      }
    });
    _autoPannedSinceMove = true;
    _autoPannedThisGesture = true;
  }

  /// The gesture ended (pointer up or cancel): stop panning and, if it
  /// panned, persist the viewport once (D15). [_autoPannedSinceMove] stays
  /// set — the release check that follows the up still needs it.
  void _endEdgePanGesture() {
    if (_edgePanTicker.isActive) _edgePanTicker.stop();
    if (_autoPannedThisGesture) {
      _autoPannedThisGesture = false;
      _syncCanvasViewportToKernel();
    }
  }

  /// [StructureDesignerModel.onCheckWireRelease]: the D19 / D14 rule
  /// ([releaseDecision]) for a release onto [chosenPin], the pin `Draggable`
  /// chose (null for empty space).
  bool _isWireReleaseIgnored(PinReference? chosenPin) {
    final local = _dragPointerLocal;
    final global = _dragPointerGlobal;
    final size = _canvasSize;
    if (!mounted || local == null || global == null || size == null) {
      return false;
    }
    final insideCanvas = (Offset.zero & size).contains(local);
    // The hit test matters only after a pan, so only then is it run.
    final underPointer = insideCanvas && _autoPannedSinceMove
        ? pinUnderPointer(global, View.of(context).viewId)
        : null;
    return releaseDecision(
          insideCanvas: insideCanvas,
          pannedSinceMove: _autoPannedSinceMove,
          chosenPin: chosenPin,
          pinUnderPointer: underPointer,
        ) ==
        ReleaseDecision.ignore;
  }

  /// Persist the current pan + zoom onto the active network so switching back
  /// (including Back/Forward) restores this exact view instead of re-framing
  /// the top-left node (issue #414 Phase 4, `doc/design_find_usages.md` D7).
  /// Mirrors `sync_camera_to_active_network` for the 3D camera; not
  /// undo-tracked. No `refreshFromKernel` — this is pure view state.
  void _syncCanvasViewportToKernel() {
    sd_api.setActiveNetworkCanvasViewport(
      panX: _panOffset.dx,
      panY: _panOffset.dy,
      zoomLevel: zoomLevelToIndex(_zoomLevel),
    );
  }

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: widget.graphModel,
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, child) {
          // Check if the node network has changed and update pan offset if needed
          if (model.nodeNetworkView != null &&
              _currentNetworkName != model.nodeNetworkView!.name) {
            // Use post-frame callback to avoid setState during build
            WidgetsBinding.instance.addPostFrameCallback((_) {
              updatePanOffsetForCurrentNetwork(forceUpdate: false);
            });
          }
          return Focus(
            focusNode: focusNode,
            autofocus: true,
            onKeyEvent: (node, event) {
              // Only handle node-network shortcuts when this focus node is the primary focus.
              // This prevents interfering with text input fields in sibling panels.
              if (FocusManager.instance.primaryFocus != focusNode) {
                return KeyEventResult.ignored;
              }

              // Only act on key down to avoid double-triggering on key up and to reduce
              // the risk of triggering platform-specific HardwareKeyboard inconsistencies.
              if (event is! KeyDownEvent) {
                return KeyEventResult.ignored;
              }

              // Escape during a move drag cancels it, ahead of every other
              // use of Escape (`doc/design_move_wires.md`).
              if (event.logicalKey == LogicalKeyboardKey.escape &&
                  model.cancelWireMove()) {
                return KeyEventResult.handled;
              }

              //print("node_network.dart event.logicalKey: " +
              //    event.logicalKey.toString() +
              //    " event.physicalKey: " +
              //    event.physicalKey.toString());
              // Ctrl+Z (undo) and Ctrl+Shift+Z / Ctrl+Y (redo) are handled
              // by the top-level StructureDesigner keyboard handler.
              if (HardwareKeyboard.instance.isControlPressed &&
                  event.logicalKey == LogicalKeyboardKey.keyD) {
                if (model.nodeNetworkView == null) {
                  return KeyEventResult.ignored;
                }

                // Resolve the selection together with its scope chain so
                // duplicate works for a node selected inside a zone body — a
                // body node and a top-level node can share a numeric id, so the
                // id alone is ambiguous (see `rust/AGENTS.md` §"Addressing
                // Nodes Across Scopes"). `getSelectedNodeId()` only searches the
                // top level, which is why Ctrl+D used to silently do nothing
                // inside a zone.
                final selected = model.getSelectedNodeWithScope();
                if (selected == null) {
                  return KeyEventResult.ignored;
                }

                model.duplicateNode(selected.nodeId,
                    scopeChain: selected.scopeChain);
                return KeyEventResult.handled;
              }
              // Ctrl+C: Copy selection
              if (HardwareKeyboard.instance.isControlPressed &&
                  event.logicalKey == LogicalKeyboardKey.keyC) {
                model.copySelection(scopeChain: model.activeScopeChain);
                return KeyEventResult.handled;
              }
              // Ctrl+V: Paste at cursor position. When the cursor is over a
              // zone body, paste into that body — `findContainingScope`
              // resolves the scope (and body-local position) under the mouse,
              // exactly as right-click → Paste does. Falls back to the
              // top-level network when the cursor is over empty canvas.
              if (HardwareKeyboard.instance.isControlPressed &&
                  event.logicalKey == LogicalKeyboardKey.keyV) {
                if (model.hasClipboardContent()) {
                  final resolver = _makeResolver();
                  final scopeHit =
                      resolver?.findContainingScope(_lastMousePosition);
                  final logicalPos = scopeHit?.bodyLocal ??
                      screenToLogical(_lastMousePosition, _panOffset,
                          getZoomScale(_zoomLevel));
                  final pasteScope = scopeHit?.scopeChain ?? const <BigInt>[];
                  final error = model.pasteAtPosition(
                      logicalPos.dx, logicalPos.dy,
                      scopeChain: pasteScope);
                  if (error != null) {
                    showErrorDialog(
                        context: context,
                        title: 'Cannot Paste',
                        message: error);
                  }
                }
                return KeyEventResult.handled;
              }
              // Ctrl+X: Cut selection
              if (HardwareKeyboard.instance.isControlPressed &&
                  event.logicalKey == LogicalKeyboardKey.keyX) {
                model.cutSelection(scopeChain: model.activeScopeChain);
                return KeyEventResult.handled;
              }
              // Ctrl+Delete: Dissolve the one selected node
              // (`doc/design_insert_node_on_wire.md` D15). Ahead of the plain
              // Delete branch, and it never falls through to deleting: a
              // refusal leaves the network untouched and says why.
              if (HardwareKeyboard.instance.isControlPressed &&
                  (event.logicalKey == LogicalKeyboardKey.delete ||
                      event.physicalKey == PhysicalKeyboardKey.delete)) {
                final scopeChain = model.activeScopeChain;
                final selected = model.selectedNodeIdsAt(scopeChain);
                final refusal = selected.length == 1
                    ? model.dissolveNode(selected.single,
                        scopeChain: scopeChain)
                    : 'Select one node to dissolve';
                if (refusal != null) showTransientSnackBar(context, refusal);
                return KeyEventResult.handled;
              }
              if (event.logicalKey == LogicalKeyboardKey.delete ||
                  event.logicalKey == LogicalKeyboardKey.backspace ||
                  event.physicalKey == PhysicalKeyboardKey.delete) {
                model.removeSelected(scopeChain: model.activeScopeChain);
                return KeyEventResult.handled;
              }
              return KeyEventResult.ignored;
            },
            child: MouseRegion(
              onExit: (_) => _clearHover(),
              onEnter: (event) {
                // Not while a note is open for in-place editing (issue #421):
                // the user moving the pointer out to a side panel and back
                // would otherwise silently close the editor mid-sentence.
                if (!focusNode.hasFocus && model.inPlaceEditRef == null) {
                  focusNode.requestFocus();
                }
              },
              child: Listener(
                onPointerDown: _handlePointerDown,
                onPointerHover: _handlePointerHover,
                onPointerMove: _handlePointerMove,
                onPointerUp: _handlePointerUp,
                onPointerCancel: (_) => _endEdgePanGesture(),
                onPointerSignal: (event) {
                  if (event is PointerScrollEvent) {
                    _handlePointerScroll(event);
                  }
                },
                onPointerPanZoomStart: _handlePointerPanZoomStart,
                onPointerPanZoomUpdate: _handlePointerPanZoomUpdate,
                onPointerPanZoomEnd: _handlePointerPanZoomEnd,
                child: GestureDetector(
                  key: const Key('node_network_canvas'),
                  onTapDown: _handleTapDown,
                  onSecondaryTapDown: (details) =>
                      _handleSecondaryTapDown(details, context, model),
                  child: _buildUserTypeDropTarget(
                    model,
                    // The live canvas clips what it paints at its bounds
                    // (D18): a wire dragged past the edge is cut off at the
                    // border, not drawn over the viewport or the panels.
                    ClipRect(
                      child: Stack(
                        children: _buildStackChildren(model),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}
