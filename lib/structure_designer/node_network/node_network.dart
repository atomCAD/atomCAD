import 'dart:math' as math;

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Uint64List;
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/node_network/add_node_popup.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_widget.dart';
import 'package:flutter_cad/structure_designer/node_network/comment_node_widget.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network_painter.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/common/api_utils.dart';

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

  const NodeNetworkInteractionLayer(
      {super.key,
      required this.model,
      required this.panOffset,
      required this.zoomLevel,
      this.overlay = false});

  @override
  Widget build(BuildContext context) {
    if (overlay) {
      // Top layer: paints body wires + dragged wire. IgnorePointer so it
      // doesn't intercept clicks meant for the node widgets that sit below
      // it in the Stack.
      return IgnorePointer(
        child: CustomPaint(
          painter: NodeNetworkPainter(model,
              panOffset: panOffset, zoomLevel: zoomLevel, overlay: true),
          child: Container(),
        ),
      );
    }
    return CustomPaint(
      painter:
          NodeNetworkPainter(model, panOffset: panOffset, zoomLevel: zoomLevel),
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

class NodeNetworkState extends State<NodeNetwork> {
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
  }

  @override
  void dispose() {
    // Clear the callbacks when disposing
    widget.graphModel.onWireDroppedInEmptySpace = null;
    widget.graphModel.onScrollToNode = null;
    focusNode.dispose();
    super.dispose();
  }

  /// Scrolls the node network view to center the given node.
  void _scrollToNode(BigInt nodeId) {
    final node = widget.graphModel.nodeNetworkView?.nodes[nodeId];
    if (node == null) return;

    // Get the widget's render box size for centering
    final renderBox = context.findRenderObject() as RenderBox?;
    if (renderBox == null) return;
    final viewportSize = renderBox.size;

    final scale = getZoomScale(_zoomLevel);
    final nodeSize = getNodeSize(node, _zoomLevel);
    // Node center in logical space
    final nodeCenterLogical = Offset(
      node.position.x + nodeSize.width / scale / 2,
      node.position.y + nodeSize.height / scale / 2,
    );
    // Pan offset so node center maps to viewport center:
    // screenCenter = (nodeCenterLogical + panOffset) * scale
    // panOffset = (screenCenter / scale) - nodeCenterLogical
    final viewportCenter =
        Offset(viewportSize.width / 2, viewportSize.height / 2);
    setState(() {
      _panOffset = (viewportCenter / scale) - nodeCenterLogical;
    });
  }

  /// Handles wire dropped in empty space - shows filtered Add Node popup.
  /// The new node lands in the body whose interior the drop hit. Auto-connect
  /// runs in both same-scope and cross-scope cases (U5): a same-scope drop
  /// uses the Rust `getCompatiblePinsForAutoConnect` shortcut for top-level
  /// pairs; a cross-scope drop falls back to per-pin `canConnectPins` checks
  /// (which now route through `can_connect_wire_scoped` for captures) so the
  /// dropped node receives a capture wire to the outer source.
  void _handleWireDropInEmptySpace(
      PinReference startPin, Offset dropPosition) async {
    final isOutput = startPin.isOutput;
    // For a drag *off* an input pin, the declared pin type may be deliberately
    // lossy (e.g. `map.f`'s `AnyFunction`, which carries the parameter prefix
    // but not the return type). Prefer the pin's concrete `dragHintType` when
    // present so both the popup filter and the created node's type inference
    // see the full signature. See `doc/design_drag_aware_add_node.md` (Tier 2).
    final dataType = _dragSourceTypeFor(startPin);

    final selectedNodeType = await showAddNodePopup(
      context,
      filterByCompatibleType: dataType,
      draggingFromOutput: isOutput,
    );

    if (selectedNodeType == null || !mounted) return;

    final resolver = _makeResolver();
    if (resolver == null) return;
    final scope = resolver.findContainingScope(dropPosition);
    final logicalPosition = scope.bodyLocal;

    final newNodeId = widget.graphModel.createNode(
      selectedNodeType,
      logicalPosition,
      dragSource: APIDragSource(
        sourcePinType: dataType,
        draggingFromOutput: isOutput,
      ),
      scopeChain: scope.scopeChain,
    );
    if (newNodeId == BigInt.zero) return;

    // Top-level same-scope drops still use the existing Rust shortcut so the
    // path is unchanged for non-zone authoring (the shortcut returns
    // `Vec::new()` for non-empty `scope_path`, so it can't be used for body
    // scopes — those fall through to the Flutter-side per-pin check below).
    final sourceScope = startPin.scopeChain;
    final sameScope = sourceScope.length == scope.scopeChain.length &&
        () {
          for (int i = 0; i < sourceScope.length; i++) {
            if (sourceScope[i] != scope.scopeChain[i]) return false;
          }
          return true;
        }();

    List<(int, String, String)> compatiblePins;
    if (sameScope && scope.scopeChain.isEmpty) {
      compatiblePins = sd_api.getCompatiblePinsForAutoConnect(
        scopePath: _scopeChainToBytes(scope.scopeChain),
        sourceNodeId: startPin.nodeId,
        sourcePinIndex: startPin.pinIndex,
        sourceIsOutput: isOutput,
        targetNodeId: newNodeId,
      );
    } else {
      // Body-scope or cross-scope drop. Look up the new node in the refreshed
      // view and probe each of its pins with `canConnectPins` (which handles
      // capture / iteration-value / body-return semantics).
      compatiblePins = _findCompatiblePinsCrossScope(
        startPin,
        newNodeId,
        scope.scopeChain,
        isOutput,
      );
    }

    if (compatiblePins.isEmpty) return;

    int targetPinIndex;
    if (compatiblePins.length == 1) {
      targetPinIndex = compatiblePins.first.$1;
    } else {
      if (!mounted) return;
      final pinOptions = compatiblePins
          .map((p) => CompatiblePinOption(
                pinIndex: p.$1,
                pinName: p.$2,
                dataType: p.$3,
              ))
          .toList();

      final selectedPinIndex = await showPinSelectionDialog(
        context,
        pins: pinOptions,
        nodeTypeName: selectedNodeType,
      );

      if (selectedPinIndex == null || !mounted) return;
      targetPinIndex = selectedPinIndex;
    }

    if (isOutput) {
      widget.graphModel.connectPins(
        startPin,
        PinReference(
          nodeId: newNodeId,
          scopeChain: scope.scopeChain,
          pinKind: PinKind.externalInput,
          pinIndex: targetPinIndex,
          dataType: '',
        ),
      );
    } else {
      widget.graphModel.connectPins(
        PinReference(
          nodeId: newNodeId,
          scopeChain: scope.scopeChain,
          pinKind: PinKind.externalOutput,
          pinIndex: targetPinIndex,
          dataType: '',
        ),
        startPin,
      );
    }
  }

  /// Walk the freshly-created [newNodeId] in [targetScope] and return its
  /// pins that pass `canConnectPins` against [sourcePin]. Used when the Rust
  /// `getCompatiblePinsForAutoConnect` shortcut doesn't apply (body-scope
  /// drop, or cross-scope drop with an outer source).
  ///
  /// The return shape mirrors `getCompatiblePinsForAutoConnect`:
  /// `(pinIndex, pinName, dataType)`.
  List<(int, String, String)> _findCompatiblePinsCrossScope(
    PinReference sourcePin,
    BigInt newNodeId,
    List<BigInt> targetScope,
    bool sourceIsOutput,
  ) {
    final newNode = _lookupNodeInScope(newNodeId, targetScope);
    if (newNode == null) return const [];
    final result = <(int, String, String)>[];
    if (sourceIsOutput) {
      // Source is an output (or zone-input); find compatible inputs on the
      // new node.
      for (int i = 0; i < newNode.inputPins.length; i++) {
        final pin = newNode.inputPins[i];
        final candidate = PinReference(
          nodeId: newNodeId,
          scopeChain: targetScope,
          pinKind: PinKind.externalInput,
          pinIndex: i,
          dataType: pin.dataType,
        );
        if (widget.graphModel.canConnectPins(sourcePin, candidate)) {
          result.add((i, pin.name, pin.dataType));
        }
      }
    } else {
      // Source is an input; test each output pin on the new node.
      for (int i = 0; i < newNode.outputPins.length; i++) {
        final pin = newNode.outputPins[i];
        final candidate = PinReference(
          nodeId: newNodeId,
          scopeChain: targetScope,
          pinKind: PinKind.externalOutput,
          pinIndex: pin.index,
          dataType: pin.effectiveDataType,
        );
        if (widget.graphModel.canConnectPins(candidate, sourcePin)) {
          result.add((pin.index, pin.name, pin.effectiveDataType));
        }
      }
    }
    return result;
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

  /// Convert a Dart scope chain to the Rust API's `Uint64List` representation.
  Uint64List _scopeChainToBytes(List<BigInt> scopeChain) {
    final result = Uint64List(scopeChain.length);
    for (int i = 0; i < scopeChain.length; i++) {
      result[i] = scopeChain[i].toUnsigned(64);
    }
    return result;
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
    focusNode.requestFocus();
  }

  /// Handles secondary (right-click) tap for context menu
  Future<void> _handleSecondaryTapDown(TapDownDetails details,
      BuildContext context, StructureDesignerModel model) async {
    // Don't show context menu if Shift is pressed (used for panning)
    if (HardwareKeyboard.instance.isShiftPressed) {
      return;
    }

    // Only show context menu if clicked on empty space (not on a node)
    // The nodes have their own context menu handling
    if (!_isClickOnNode(model, details.localPosition)) {
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
          String? selectedNode = await showAddNodePopup(context);
          if (selectedNode != null) {
            model.createNode(selectedNode, logicalPosition,
                scopeChain: scopeChain);
          }
        } else if (value == 'paste') {
          model.pasteAtPosition(logicalPosition.dx, logicalPosition.dy,
              scopeChain: scopeChain);
        }
      } else {
        String? selectedNode = await showAddNodePopup(context);
        if (selectedNode != null) {
          model.createNode(selectedNode, logicalPosition,
              scopeChain: scopeChain);
        }
      }
    }
    focusNode.requestFocus();
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
    _appendNodesRecursive(children, view, const <BigInt>[], view, resolver);

    // Top wire layer (overlay): body wires + dragged wire. These need to
    // paint above the node widgets — the HOF's body Container has an opaque
    // background that would otherwise hide wires drawn underneath it. See
    // `doc/design_zones_ui.md` §"Wire rendering across scopes".
    children.add(NodeNetworkInteractionLayer(
        model: model,
        panOffset: _panOffset,
        zoomLevel: _zoomLevel,
        overlay: true));

    children.add(_buildSelectionRectangle());
    return children;
  }

  /// Append every NodeWidget reachable from the top-level network — first the
  /// outer scope's nodes (HOFs included), then each HOF's body nodes
  /// recursively. Body nodes appear *above* their HOF in the Stack so they
  /// can receive pointer events first.
  ///
  /// [resolver] is consulted to decide whether to descend into each HOF's
  /// body: a body that's collapsed (rendered too small to be readable —
  /// see U6) is skipped, since the HOF widget itself already swaps in the
  /// `[N nodes]` placeholder for that case.
  /// Build the widget for a single node in [scopeChain]. Comment nodes get the
  /// special [CommentNodeWidget] rendering at every scope (top level *and*
  /// inside HOF/closure bodies); all other nodes get the generic [NodeWidget].
  /// Both widgets are scope-aware (positioning + key + API calls), so the same
  /// routing works for the top-level walk and the recursive zone-body walk.
  Widget _buildNodeWidget(
    NodeView node,
    List<BigInt> scopeChain,
    NodeNetworkView rootView,
  ) {
    if (node.nodeTypeName == 'Comment') {
      return CommentNodeWidget(
        key: NodeWidgetKeys.nodeWidget(node.id, scopeChain: scopeChain),
        node: node,
        panOffset: _panOffset,
        zoomLevel: _zoomLevel,
        rootView: rootView,
        scopeChain: scopeChain,
      );
    }
    return NodeWidget(
      node: node,
      panOffset: _panOffset,
      zoomLevel: _zoomLevel,
      rootView: rootView,
      scopeChain: scopeChain,
    );
  }

  void _appendNodesRecursive(
    List<Widget> children,
    NodeNetworkView view,
    List<BigInt> scopeChain,
    NodeNetworkView rootView,
    ScopeResolver resolver,
  ) {
    for (final entry in view.nodes.entries) {
      children.add(_buildNodeWidget(entry.value, scopeChain, rootView));
    }
    // Then walk into each HOF's body — body nodes are drawn after their
    // owner HOF so they layer on top. Skip if the body is collapsed.
    for (final entry in view.nodes.entries) {
      final node = entry.value;
      final zone = node.zone;
      if (zone == null) continue;
      final bodyChain = [...scopeChain, node.id];
      if (resolver.isBodyCollapsed(bodyChain)) continue;
      _appendZoneNodesRecursive(children, zone, bodyChain, rootView, resolver);
    }
  }

  void _appendZoneNodesRecursive(
    List<Widget> children,
    ZoneView zone,
    List<BigInt> scopeChain,
    NodeNetworkView rootView,
    ScopeResolver resolver,
  ) {
    for (final entry in zone.nodes.entries) {
      children.add(_buildNodeWidget(entry.value, scopeChain, rootView));
    }
    for (final entry in zone.nodes.entries) {
      final node = entry.value;
      final inner = node.zone;
      if (inner == null) continue;
      final innerChain = [...scopeChain, node.id];
      if (resolver.isBodyCollapsed(innerChain)) continue;
      _appendZoneNodesRecursive(
          children, inner, innerChain, rootView, resolver);
    }
  }

  /// Handle pointer down event - check for middle mouse button, Shift + right mouse, or left-click for rectangle selection
  void _handlePointerDown(PointerDownEvent event) {
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
  }

  /// Handle pointer move event for panning (middle mouse or Shift + right mouse) and rectangle selection
  void _handlePointerMove(PointerMoveEvent event) {
    // Always track mouse position for paste (Ctrl+V)
    _lastMousePosition = event.localPosition;

    if ((_isMiddleMousePanning || _isShiftRightMousePanning) &&
        _lastPanPosition != null) {
      setState(() {
        // Convert screen-space delta to logical-space delta
        final scale = getZoomScale(_zoomLevel);
        final screenDelta = event.position - _lastPanPosition!;
        _panOffset += screenDelta / scale;
        _lastPanPosition = event.position;
      });
    }
    // Rectangle selection - just update the visual, no Rust calls during drag
    else if (_selectionRectStart != null && event.buttons == kPrimaryButton) {
      _handleSelectionRectUpdate(event.localPosition);
    }
  }

  /// Handle pointer up event to end panning or rectangle selection
  void _handlePointerUp(PointerUpEvent event) {
    if (_isMiddleMousePanning || _isShiftRightMousePanning) {
      setState(() {
        _isMiddleMousePanning = false;
        _isShiftRightMousePanning = false;
        _lastPanPosition = null;
      });
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
                  model.pasteAtPosition(logicalPos.dx, logicalPos.dy,
                      scopeChain: pasteScope);
                }
                return KeyEventResult.handled;
              }
              // Ctrl+X: Cut selection
              if (HardwareKeyboard.instance.isControlPressed &&
                  event.logicalKey == LogicalKeyboardKey.keyX) {
                model.cutSelection(scopeChain: model.activeScopeChain);
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
              onEnter: (event) {
                if (!focusNode.hasFocus) {
                  focusNode.requestFocus();
                }
              },
              child: Listener(
                onPointerDown: _handlePointerDown,
                onPointerHover: _handlePointerHover,
                onPointerMove: _handlePointerMove,
                onPointerUp: _handlePointerUp,
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
                  child: Stack(
                    children: _buildStackChildren(model),
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
