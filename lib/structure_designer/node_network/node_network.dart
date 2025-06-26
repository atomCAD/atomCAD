import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/node_network/add_node_popup.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_widget.dart';
import 'package:flutter_cad/structure_designer/node_network/wire_painter.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

// Node dimensions and layout constants
const double NODE_WIDTH = 130.0;
const double NODE_VERT_WIRE_OFFSET = 39.0;
const double NODE_VERT_WIRE_OFFSET_EMPTY = 46.0;
const double NODE_VERT_WIRE_OFFSET_PER_PARAM = 21.0;
const double CUBIC_SPLINE_HORIZ_OFFSET = 50.0;

// Wire appearance constants
const double WIRE_WIDTH_SELECTED = 4.0;
const double WIRE_WIDTH_NORMAL = 2.0;
const double WIRE_GLOW_OPACITY = 0.3;

const double HIT_TEST_WIRE_WIDTH = 12.0;

// Colors
const Color DEFAULT_DATA_TYPE_COLOR = Colors.grey;
const Map<String, Color> DATA_TYPE_COLORS = {
  'Geometry2D': Colors.purple,
  'Geometry': Colors.blue,
  'Atomic': Color.fromARGB(255, 30, 160, 30),
};
const Color WIRE_COLOR_SELECTED = Color(0xFFD84315);

/// Wraps the WirePainter to add interaction capabilities
class WireInteractionLayer extends StatelessWidget {
  final StructureDesignerModel model;
  final Offset panOffset;

  const WireInteractionLayer(
      {super.key, required this.model, required this.panOffset});

  /// Handles tap on wires for selection
  void _handleWireTapDown(TapDownDetails details) {
    final painter = WirePainter(model, panOffset: panOffset);
    final hit = painter.findWireAtPosition(details.localPosition);
    if (hit != null) {
      model.setSelectedWire(
        hit.sourceNodeId,
        hit.destNodeId,
        hit.destParamIndex,
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    return CustomPaint(
      painter: WirePainter(model, panOffset: panOffset),
      child: GestureDetector(
        behavior: HitTestBehavior.translucent,
        onTapDown: _handleWireTapDown,
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

  /// Store the current network name to detect changes
  String? _currentNetworkName;

  /// Whether we're currently panning with middle mouse button
  bool _isMiddleMousePanning = false;

  /// Store the last pointer position for middle mouse panning
  Offset? _lastMiddleMousePosition;

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
    if (!forceUpdate && _currentNetworkName == model.nodeNetworkView!.name)
      return;

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
  /// Adjusts for panning by subtracting the pan offset from the position
  bool _isClickOnNode(StructureDesignerModel model, Offset position) {
    if (model.nodeNetworkView == null) return false;

    // Adjust position for pan offset
    final adjustedPosition = position - _panOffset;

    for (final node in model.nodeNetworkView!.nodes.values) {
      final nodeRect = Rect.fromLTWH(
          node.position.x,
          node.position.y,
          NODE_WIDTH,
          // Approximate height calculation based on number of input pins
          NODE_VERT_WIRE_OFFSET +
              (node.inputPins.length * NODE_VERT_WIRE_OFFSET_PER_PARAM));

      if (nodeRect.contains(adjustedPosition)) {
        return true;
      }
    }
    return false;
  }

  /// Gets the node at the given position, if any
  /// Accounts for the current pan offset
  NodeView? getNodeAtPosition(StructureDesignerModel model, Offset position) {
    if (model.nodeNetworkView == null) return null;

    // Adjust position for pan offset
    final adjustedPosition = position - _panOffset;

    for (final node in model.nodeNetworkView!.nodes.values) {
      final nodeRect = Rect.fromLTWH(
          node.position.x,
          node.position.y,
          NODE_WIDTH,
          NODE_VERT_WIRE_OFFSET +
              (node.inputPins.length * NODE_VERT_WIRE_OFFSET_PER_PARAM));

      if (nodeRect.contains(adjustedPosition)) {
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
    // Only show add node popup if clicked on empty space (not on a node)
    // The nodes have their own context menu handling
    if (!_isClickOnNode(model, details.localPosition)) {
      String? selectedNode = await showAddNodePopup(context);
      if (selectedNode != null) {
        // Adjust position for pan offset when creating node
        final adjustedPosition = details.localPosition - _panOffset;
        model.createNode(selectedNode, adjustedPosition);
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
      WireInteractionLayer(model: model, panOffset: _panOffset),
      // Then all the nodes on top - NodeWidget now handles its own positioning with panOffset
      ...model.nodeNetworkView!.nodes.entries
          .map((entry) => NodeWidget(node: entry.value, panOffset: _panOffset))
    ];
  }

  /// Handle pointer down event - check for middle mouse button
  void _handlePointerDown(PointerDownEvent event) {
    // Check if middle mouse button (button 2)
    if (event.buttons == kTertiaryButton) {
      setState(() {
        _isMiddleMousePanning = true;
        _lastMiddleMousePosition = event.position;
      });
    }
  }

  /// Handle pointer move event for middle mouse panning
  void _handlePointerMove(PointerMoveEvent event) {
    if (_isMiddleMousePanning && _lastMiddleMousePosition != null) {
      setState(() {
        _panOffset += event.position - _lastMiddleMousePosition!;
        _lastMiddleMousePosition = event.position;
      });
    }
  }

  /// Handle pointer up event to end middle mouse panning
  void _handlePointerUp(PointerUpEvent event) {
    if (_isMiddleMousePanning) {
      setState(() {
        _isMiddleMousePanning = false;
        _lastMiddleMousePosition = null;
      });
    }
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
              if (event.logicalKey == LogicalKeyboardKey.delete) {
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
