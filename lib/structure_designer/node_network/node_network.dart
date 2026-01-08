import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/node_network/add_node_popup.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_widget.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network_painter.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

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

  // Geometry types (purple family - abstract shapes)
  'Geometry2D': Color(0xFFBA68C8), // Light purple
  'Geometry': Color(0xFF9C27B0), // Light deep purple

  // Physical types (green family - real-world matter)
  'Atomic': Color(0xFF66BB6A), // Light green

  // Crystal structure types (teal family - crystalline matter)
  'UnitCell': Color(0xFF26A69A), // Teal
  'Motif': Color(0xFF00ACC1), // Light blue-green (cyan)

  // Function types (amber family - computational operations)
  '->': Color(0xFFFFA726), // Amber
};
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
Size getNodeSize(NodeView node, ZoomLevel zoomLevel) {
  final scale = getZoomScale(zoomLevel);

  // Calculate estimated height at normal scale (for all zoom levels)
  // Title bar: ~30px, main body: max(inputs, output), subtitle: ~20px, padding: ~8px
  final titleHeight = 30.0;
  final inputPinsHeight =
      node.inputPins.length * BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM;
  final outputHeight = 25.0; // Single output pin height
  // Main body is a Row, so height = max(left inputs, right output)
  final mainBodyHeight =
      inputPinsHeight > outputHeight ? inputPinsHeight : outputHeight;
  final subtitleHeight =
      (node.subtitle != null && node.subtitle!.isNotEmpty) ? 20.0 : 0.0;
  final padding = 8.0;

  final normalHeight = titleHeight + mainBodyHeight + subtitleHeight + padding;

  if (zoomLevel == ZoomLevel.normal) {
    // Normal zoom - use calculated height
    return Size(BASE_NODE_WIDTH * scale, normalHeight * scale);
  } else {
    // Zoomed out - use proportionally scaled height with minimum aspect ratio
    // Ensure minimum height for text readability (at least 0.375 aspect ratio = height/width)
    final width = BASE_NODE_WIDTH * scale;
    final scaledHeight = normalHeight * scale;
    final minHeight =
        width * 0.375; // Minimum aspect ratio for at least one line of text

    final height = scaledHeight > minHeight ? scaledHeight : minHeight;
    return Size(width, height);
  }
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

  // Check for exact matches and partial matches in the type name
  for (final entry in DATA_TYPE_COLORS.entries) {
    if (typeName.contains(entry.key)) {
      return entry.value;
    }
  }

  // Return default color if no match found
  return DEFAULT_DATA_TYPE_COLOR;
}

/// Wraps the NodeNetworkPainter to add interaction capabilities
class NodeNetworkInteractionLayer extends StatelessWidget {
  final StructureDesignerModel model;
  final Offset panOffset;
  final ZoomLevel zoomLevel;

  const NodeNetworkInteractionLayer(
      {super.key,
      required this.model,
      required this.panOffset,
      required this.zoomLevel});

  /// Handles tap on wires for selection, or clears selection if clicking empty space
  void _handleWireTap(TapUpDetails details) {
    final painter =
        NodeNetworkPainter(model, panOffset: panOffset, zoomLevel: zoomLevel);
    final hit = painter.findWireAtPosition(details.localPosition);
    if (hit != null) {
      model.setSelectedWire(
        hit.sourceNodeId,
        hit.sourcePinIndex,
        hit.destNodeId,
        hit.destParamIndex,
      );
    } else {
      // Clicked on empty space - clear selection
      model.clearSelection();
    }
  }

  @override
  Widget build(BuildContext context) {
    return CustomPaint(
      painter:
          NodeNetworkPainter(model, panOffset: panOffset, zoomLevel: zoomLevel),
      child: GestureDetector(
        behavior: HitTestBehavior.translucent,
        onTapUp: _handleWireTap,
        child: Container(),
      ),
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

  @override
  void initState() {
    super.initState();
    // Initial calculation of pan offset for the current network
    WidgetsBinding.instance.addPostFrameCallback((_) {
      updatePanOffsetForCurrentNetwork(forceUpdate: false);
    });
  }

  @override
  void dispose() {
    focusNode.dispose();
    super.dispose();
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

  /// Checks if the given position is on top of any node
  /// Converts screen position to logical space for hit testing
  bool _isClickOnNode(StructureDesignerModel model, Offset position) {
    if (model.nodeNetworkView == null) return false;

    // Convert screen position to logical coordinates
    final scale = getZoomScale(_zoomLevel);
    final logicalPosition = screenToLogical(position, _panOffset, scale);

    for (final node in model.nodeNetworkView!.nodes.values) {
      final nodePos = Offset(node.position.x, node.position.y);
      final nodeSize = getNodeSize(node, _zoomLevel);
      // Node size is already in screen space, convert to logical space
      final logicalNodeSize =
          Size(nodeSize.width / scale, nodeSize.height / scale);
      final nodeRect = nodePos & logicalNodeSize;

      if (nodeRect.contains(logicalPosition)) {
        return true;
      }
    }
    return false;
  }

  /// Gets the node at the given position, if any
  /// Converts screen position to logical space for hit testing
  NodeView? getNodeAtPosition(StructureDesignerModel model, Offset position) {
    if (model.nodeNetworkView == null) return null;

    // Convert screen position to logical coordinates
    final scale = getZoomScale(_zoomLevel);
    final logicalPosition = screenToLogical(position, _panOffset, scale);

    for (final node in model.nodeNetworkView!.nodes.values) {
      final nodePos = Offset(node.position.x, node.position.y);
      final nodeSize = getNodeSize(node, _zoomLevel);
      // Node size is already in screen space, convert to logical space
      final logicalNodeSize =
          Size(nodeSize.width / scale, nodeSize.height / scale);
      final nodeRect = nodePos & logicalNodeSize;

      if (nodeRect.contains(logicalPosition)) {
        return node;
      }
    }
    return null;
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

    // Only show add node popup if clicked on empty space (not on a node)
    // The nodes have their own context menu handling
    if (!_isClickOnNode(model, details.localPosition)) {
      String? selectedNode = await showAddNodePopup(context);
      if (selectedNode != null) {
        // Convert screen position to logical coordinates for node creation
        final scale = getZoomScale(_zoomLevel);
        final logicalPosition =
            screenToLogical(details.localPosition, _panOffset, scale);
        model.createNode(selectedNode, logicalPosition);
      }
    }
    focusNode.requestFocus();
  }

  // Left-click panning has been replaced by middle mouse button panning

  /// Builds the stack children for the node network
  List<Widget> _buildStackChildren(StructureDesignerModel model) {
    if (model.nodeNetworkView == null) {
      return [];
    }

    // The Stack will handle all the nodes and wires with appropriate transformations
    return [
      // Wire layer at the bottom
      NodeNetworkInteractionLayer(
          model: model, panOffset: _panOffset, zoomLevel: _zoomLevel),
      // Then all the nodes on top - NodeWidget now handles its own positioning with panOffset
      ...model.nodeNetworkView!.nodes.entries.map((entry) => NodeWidget(
          node: entry.value, panOffset: _panOffset, zoomLevel: _zoomLevel))
    ];
  }

  /// Handle pointer down event - check for middle mouse button or Shift + right mouse
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
  }

  /// Handle pointer move event for panning (middle mouse or Shift + right mouse)
  void _handlePointerMove(PointerMoveEvent event) {
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
  }

  /// Handle pointer up event to end panning
  void _handlePointerUp(PointerUpEvent event) {
    if (_isMiddleMousePanning || _isShiftRightMousePanning) {
      setState(() {
        _isMiddleMousePanning = false;
        _isShiftRightMousePanning = false;
        _lastPanPosition = null;
      });
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
              if (HardwareKeyboard.instance.isControlPressed &&
                  event.logicalKey == LogicalKeyboardKey.keyD) {
                if (model.nodeNetworkView == null) {
                  return KeyEventResult.ignored;
                }

                final selectedNodeId = model.getSelectedNodeId();
                if (selectedNodeId == null) {
                  return KeyEventResult.ignored;
                }

                model.duplicateNode(selectedNodeId);
                return KeyEventResult.handled;
              }
              if (event.logicalKey == LogicalKeyboardKey.delete ||
                  event.logicalKey == LogicalKeyboardKey.backspace ||
                  event.physicalKey == PhysicalKeyboardKey.delete) {
                model.removeSelected();
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
