import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/add_node_popup.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_widget.dart';
import 'package:flutter_cad/structure_designer/wire_painter.dart';

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

/// Widget specifically for handling wire painting and interaction
class WireInteractionLayer extends StatelessWidget {
  final StructureDesignerModel model;

  const WireInteractionLayer({super.key, required this.model});

  /// Handles tap on wires for selection
  void _handleWireTapDown(TapDownDetails details) {
    final painter = WirePainter(model);
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
      painter: WirePainter(model),
      child: GestureDetector(
        behavior: HitTestBehavior.translucent,
        onTapDown: _handleWireTapDown,
        child: Container(),
      ),
    );
  }
}

/// The main node network widget.
class NodeNetwork extends StatelessWidget {
  final StructureDesignerModel graphModel;
  final focusNode = FocusNode();

  NodeNetwork({super.key, required this.graphModel});

  /// Checks if the given position is on top of any node
  bool _isClickOnNode(StructureDesignerModel model, Offset position) {
    if (model.nodeNetworkView == null) return false;

    for (final node in model.nodeNetworkView!.nodes.values) {
      final nodeRect = Rect.fromLTWH(
          node.position.x,
          node.position.y,
          NODE_WIDTH,
          // Approximate height calculation based on number of input pins
          NODE_VERT_WIRE_OFFSET +
              (node.inputPins.length * NODE_VERT_WIRE_OFFSET_PER_PARAM));

      if (nodeRect.contains(position)) {
        return true;
      }
    }
    return false;
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
        model.createNode(selectedNode, details.localPosition);
      }
    }
    focusNode.requestFocus();
  }

  /// Builds the stack children for the node network
  List<Widget> _buildStackChildren(StructureDesignerModel model) {
    if (model.nodeNetworkView == null) {
      return [];
    }

    return [
      WireInteractionLayer(model: model),
      ...(model.nodeNetworkView!.nodes.entries
          .map((entry) => NodeWidget(node: entry.value))
          .toList())
    ];
  }

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: graphModel,
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, child) {
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
              child: GestureDetector(
                onTapDown: _handleTapDown,
                onSecondaryTapDown: (details) =>
                    _handleSecondaryTapDown(details, context, model),
                child: Stack(
                  children: _buildStackChildren(model),
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}
