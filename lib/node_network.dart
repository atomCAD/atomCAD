import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/api_utils.dart';
import 'package:flutter_cad/add_node_popup.dart';
import 'package:flutter_cad/graph_model.dart';

// Node dimensions and layout constants
const double NODE_WIDTH = 120.0;
const double NODE_VERT_WIRE_OFFSET = 39.0;
const double NODE_VERT_WIRE_OFFSET_EMPTY = 46.0;
const double NODE_VERT_WIRE_OFFSET_PER_PARAM = 21.0;
const double CUBIC_SPLINE_HORIZ_OFFSET = 50.0;

// Pin appearance constants
const double PIN_SIZE = 14.0;
const double PIN_BORDER_WIDTH = 5.0;

// Node appearance constants
const Color NODE_BACKGROUND_COLOR = Color(0xFF212121); // Colors.grey[900]
const Color NODE_BORDER_COLOR_SELECTED = Colors.orange;
const Color NODE_BORDER_COLOR_NORMAL = Colors.blueAccent;
const double NODE_BORDER_WIDTH_SELECTED = 3.0;
const double NODE_BORDER_WIDTH_NORMAL = 2.0;
const double NODE_BORDER_RADIUS = 8.0;
const Color NODE_TITLE_COLOR_SELECTED = Color(0xFFD84315); // Colors.orange[800]
const Color NODE_TITLE_COLOR_NORMAL = Color(0xFF37474F); // Colors.blueGrey[800]

// Wire appearance constants
const double WIRE_WIDTH_SELECTED = 4.0;
const double WIRE_WIDTH_NORMAL = 2.0;
const double WIRE_GLOW_BLUR_RADIUS = 8.0;
const double WIRE_GLOW_SPREAD_RADIUS = 2.0;
const double WIRE_GLOW_OPACITY = 0.3;

const double HIT_TEST_WIRE_WIDTH = 12.0;

// Colors
const Color DEFAULT_DATA_TYPE_COLOR = Colors.grey;
const Map<String, Color> DATA_TYPE_COLORS = {
  'Geometry': Colors.blue,
  'Atomic': Color.fromARGB(255, 30, 160, 30),
};
const Color WIRE_COLOR_SELECTED = Color(0xFFD84315);

Color getDataTypeColor(String dataType) {
  return DATA_TYPE_COLORS[dataType] ?? DEFAULT_DATA_TYPE_COLOR;
}

/// The main node network widget.
class NodeNetwork extends StatelessWidget {
  final GraphModel graphModel;

  const NodeNetwork({super.key, required this.graphModel});

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: graphModel,
      child: Consumer<GraphModel>(
        builder: (context, model, child) {
          return GestureDetector(
              onSecondaryTapDown: (details) async {
                String? selectedNode = await showAddNodePopup(context);
                if (selectedNode != null) {
                  // Handle adding the selected node at the clicked position
                  print(
                      "Node added: $selectedNode at ${details.localPosition}");
                }
              },
              child: Stack(
                children: (model.nodeNetworkView == null)
                    ? []
                    : [
                        CustomPaint(
                          painter: WirePainter(model),
                          child: GestureDetector(
                            behavior: HitTestBehavior.translucent,
                            onTapDown: (details) {
                              final painter = WirePainter(model);
                              final hit = painter
                                  .findWireAtPosition(details.localPosition);
                              if (hit != null) {
                                model.setSelectedWire(
                                  hit.sourceNodeId,
                                  hit.destNodeId,
                                  hit.destParamIndex,
                                );
                              }
                            },
                            child: Container(),
                          ),
                        ),
                        ...(model.nodeNetworkView!.nodes.entries
                            .map((entry) => NodeWidget(node: entry.value))
                            .toList())
                      ],
              ));
        },
      ),
    );
  }
}

class PinViewWidget extends StatelessWidget {
  final String dataType;
  final bool multi;

  const PinViewWidget({super.key, required this.dataType, required this.multi});

  @override
  Widget build(BuildContext context) {
    final color = getDataTypeColor(dataType);

    return Center(
      child: Container(
          width: PIN_SIZE,
          height: PIN_SIZE,
          decoration: multi
              ? BoxDecoration(
                  border: Border.all(
                    color: color, // Set the border color
                    width: PIN_BORDER_WIDTH, // Set the border width
                  ),
                  shape: BoxShape.circle,
                  color: Colors.black,
                )
              : BoxDecoration(
                  shape: BoxShape.circle,
                  color: color,
                )),
    );
  }
}

class PinWidget extends StatelessWidget {
  final PinReference pinReference;
  final bool multi;
  PinWidget({required this.pinReference, required this.multi})
      : super(key: ValueKey(pinReference.pinIndex));

  RenderBox? _findNodeNetworkRenderBox(BuildContext context) {
    RenderBox? result;
    context.visitAncestorElements((element) {
      if (element.widget is NodeNetwork) {
        result = element.renderObject as RenderBox?;
        return false; // Stop visiting
      }
      return true; // Continue visiting
    });
    return result;
  }

  @override
  Widget build(BuildContext context) {
    return DragTarget<PinReference>(
      builder: (context, candidateData, rejectedData) {
        return Draggable<PinReference>(
          data: pinReference,
          feedback: SizedBox.shrink(),
          childWhenDragging:
              PinViewWidget(dataType: pinReference.dataType, multi: multi),
          child: PinViewWidget(dataType: pinReference.dataType, multi: multi),
          onDragUpdate: (details) {
            final nodeNetworkBox = _findNodeNetworkRenderBox(context);
            if (nodeNetworkBox != null) {
              final position =
                  nodeNetworkBox.globalToLocal(details.globalPosition);
              Provider.of<GraphModel>(context, listen: false)
                  .dragWire(pinReference, position);
            }
          },
          onDragEnd: (details) {
            Provider.of<GraphModel>(context, listen: false).cancelDragWire();
          },
        );
      },
      onWillAcceptWithDetails: (details) {
        return details.data.dataType ==
                pinReference.dataType && // same data type
            (details.data.pinIndex < 0) !=
                (pinReference.pinIndex < 0); // output to input
      },
      onAcceptWithDetails: (details) {
        //print("Connected pin ${details.data} to pin $pinReference");
        Provider.of<GraphModel>(context, listen: false)
            .connectPins(details.data, pinReference);
      },
    );
  }
}

/// Widget representing a single draggable node.
class NodeWidget extends StatelessWidget {
  final NodeView node;

  NodeWidget({required this.node}) : super(key: ValueKey(node.id));

  @override
  Widget build(BuildContext context) {
    return Positioned(
        left: node.position.x,
        top: node.position.y,
        child: Container(
          width: NODE_WIDTH,
          decoration: BoxDecoration(
            color: NODE_BACKGROUND_COLOR,
            borderRadius: BorderRadius.circular(NODE_BORDER_RADIUS),
            border: Border.all(
                color: node.selected
                    ? NODE_BORDER_COLOR_SELECTED
                    : NODE_BORDER_COLOR_NORMAL,
                width: node.selected
                    ? NODE_BORDER_WIDTH_SELECTED
                    : NODE_BORDER_WIDTH_NORMAL),
            boxShadow: node.selected
                ? [
                    BoxShadow(
                        color: NODE_BORDER_COLOR_SELECTED
                            .withOpacity(WIRE_GLOW_OPACITY),
                        blurRadius: WIRE_GLOW_BLUR_RADIUS,
                        spreadRadius: WIRE_GLOW_SPREAD_RADIUS)
                  ]
                : null,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              // Title Bar
              GestureDetector(
                onPanStart: (details) {
                  final model = Provider.of<GraphModel>(context, listen: false);
                  model.setSelectedNode(node.id);
                },
                onPanUpdate: (details) {
                  Provider.of<GraphModel>(context, listen: false)
                      .dragNodePosition(node.id, details.delta);
                },
                onPanEnd: (details) {
                  Provider.of<GraphModel>(context, listen: false)
                      .updateNodePosition(node.id);
                },
                child: Container(
                  padding: EdgeInsets.symmetric(vertical: 4, horizontal: 8),
                  decoration: BoxDecoration(
                    color: node.selected
                        ? NODE_TITLE_COLOR_SELECTED
                        : NODE_TITLE_COLOR_NORMAL,
                    borderRadius: BorderRadius.vertical(
                        top: Radius.circular(NODE_BORDER_RADIUS - 2)),
                  ),
                  child: Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      Text(
                        node.nodeTypeName,
                        style: TextStyle(
                          color: Colors.white,
                          fontWeight: FontWeight.bold,
                          fontSize: 15,
                        ),
                      ),
                      GestureDetector(
                        onTap: () {
                          final model =
                              Provider.of<GraphModel>(context, listen: false);
                          model.toggleNodeDisplay(node.id);
                        },
                        child: Icon(
                          node.displayed
                              ? Icons.visibility
                              : Icons.visibility_off,
                          color: Colors.white,
                          size: 20,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
              // Main Body
              Padding(
                padding: EdgeInsets.all(8),
                child: Row(
                  children: [
                    // Left Side (Inputs)
                    Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: node.inputPins
                          .asMap()
                          .entries
                          .map((entry) => _buildInputPin(
                              entry.value.name,
                              PinReference(
                                  node.id, entry.key, entry.value.dataType),
                              entry.value.multi))
                          .toList(),
                    ),
                    Spacer(),
                    // Right Side (Output)
                    PinWidget(
                      pinReference: PinReference(node.id, -1, node.outputType),
                      multi: false,
                    ),
                  ],
                ),
              ),
            ],
          ),
        ));
  }

  /// Creates a labeled input pin.
  Widget _buildInputPin(String label, PinReference pinReference, bool multi) {
    return Row(
      children: [
        PinWidget(pinReference: pinReference, multi: multi),
        SizedBox(width: 6),
        Text(
          label,
          style: TextStyle(color: Colors.white, fontSize: 15),
        ),
      ],
    );
  }
}

class WireHitResult {
  final BigInt sourceNodeId;
  final BigInt destNodeId;
  final BigInt destParamIndex;

  WireHitResult(this.sourceNodeId, this.destNodeId, this.destParamIndex);
}

class WirePainter extends CustomPainter {
  final GraphModel graphModel;

  WirePainter(this.graphModel);

  @override
  void paint(Canvas canvas, Size size) {
    if (graphModel.nodeNetworkView == null) {
      return;
    }

    Paint paint = Paint()
      ..color = Colors.black
      ..strokeWidth = WIRE_WIDTH_NORMAL
      ..style = PaintingStyle.stroke;

    // Draw regular wires first
    for (var wire in graphModel.nodeNetworkView!.wires) {
      final source = _getPinPositionAndDataType(wire.sourceNodeId, -1);
      final dest = _getPinPositionAndDataType(
          wire.destNodeId, wire.destParamIndex.toInt());
      _drawWire(source.$1, dest.$1, canvas, paint, source.$2, wire.selected);
    }

    // Draw dragged wire on top
    if (graphModel.draggedWire != null) {
      final wireStart = _getPinPositionAndDataType(
          graphModel.draggedWire!.startPin.nodeId,
          graphModel.draggedWire!.startPin.pinIndex);
      final wireEndPos = graphModel.draggedWire!.wireEndPosition;
      if (graphModel.draggedWire!.startPin.pinIndex < 0) {
        // start is source
        _drawWire(wireStart.$1, wireEndPos, canvas, paint, wireStart.$2, false);
      } else {
        // start is dest
        _drawWire(wireEndPos, wireStart.$1, canvas, paint, wireStart.$2, false);
      }
    }
  }

  (Offset, String) _getPinPositionAndDataType(BigInt nodeId, int pinIndex) {
    // Now this is is a bit of a hacky solution.
    // We should probably use the real positions of the pin widgets instead of this logic to
    // approximate it independently.
    if (pinIndex < 0) {
      // output pin (source pin)
      final sourceNode = graphModel.nodeNetworkView!.nodes[nodeId];
      final sourceVertOffset = sourceNode!.inputPins.isEmpty
          ? NODE_VERT_WIRE_OFFSET_EMPTY
          : NODE_VERT_WIRE_OFFSET +
              sourceNode.inputPins.length *
                  NODE_VERT_WIRE_OFFSET_PER_PARAM *
                  0.5;
      return (
        APIVec2ToOffset(sourceNode.position) +
            Offset(NODE_WIDTH, sourceVertOffset),
        sourceNode.outputType
      );
    } else {
      // input pin (dest pin)
      final destNode = graphModel.nodeNetworkView!.nodes[nodeId];
      final destVertOffset = NODE_VERT_WIRE_OFFSET +
          (pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
      return (
        APIVec2ToOffset(destNode!.position) + Offset(0.0, destVertOffset),
        destNode.inputPins[pinIndex].dataType
      );
    }
  }

  _drawWire(Offset sourcePos, Offset destPos, Canvas canvas, Paint paint,
      String dataType, bool selected) {
    paint.color = getDataTypeColor(dataType);
    paint.strokeWidth = selected ? WIRE_WIDTH_SELECTED : WIRE_WIDTH_NORMAL;

    if (selected) {
      paint.color = WIRE_COLOR_SELECTED;

      // Draw glow effect for selected wire
      final glowPaint = Paint()
        ..color = WIRE_COLOR_SELECTED.withOpacity(WIRE_GLOW_OPACITY)
        ..strokeWidth = paint.strokeWidth * 2
        ..style = PaintingStyle.stroke;

      canvas.drawPath(_getPath(sourcePos, destPos), glowPaint);
    }

    canvas.drawPath(_getPath(sourcePos, destPos), paint);
  }

  Path _getPath(Offset sourcePos, Offset destPos) {
    final controlPoint1 = sourcePos + Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);
    final controlPoint2 = destPos - Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);

    return Path()
      ..moveTo(sourcePos.dx, sourcePos.dy)
      ..cubicTo(
        controlPoint1.dx,
        controlPoint1.dy,
        controlPoint2.dx,
        controlPoint2.dy,
        destPos.dx,
        destPos.dy,
      );
  }

  Path _getBand(Offset sourcePos, Offset destPos, double width) {
    final hw = width * 0.5;
    final off = destPos.dx > sourcePos.dx ? width : (-width);

    final sourcePos1 = Offset(sourcePos.dx, sourcePos.dy + hw);
    final sourcePos2 = Offset(sourcePos.dx, sourcePos.dy - hw);
    final destPos1 = Offset(destPos.dx, destPos.dy + hw);
    final destPos2 = Offset(destPos.dx, destPos.dy - hw);

    final controlPointStart1 =
        sourcePos1 + Offset(CUBIC_SPLINE_HORIZ_OFFSET - off, 0);
    final controlPointEnd1 = destPos1 - Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);

    final controlPointStart2 =
        sourcePos2 + Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);
    final controlPointEnd2 =
        destPos2 - Offset(CUBIC_SPLINE_HORIZ_OFFSET - off, 0);

    return Path()
      ..moveTo(sourcePos1.dx, sourcePos1.dy)
      ..cubicTo(
        controlPointStart1.dx,
        controlPointStart1.dy,
        controlPointEnd1.dx,
        controlPointEnd1.dy,
        destPos1.dx,
        destPos1.dy,
      )
      ..lineTo(destPos2.dx, destPos2.dy)
      ..cubicTo(
        controlPointEnd2.dx,
        controlPointEnd2.dy,
        controlPointStart2.dx,
        controlPointStart2.dy,
        sourcePos2.dx,
        sourcePos2.dy,
      )
      ..close();
  }

  WireHitResult? findWireAtPosition(Offset position) {
    if (graphModel.nodeNetworkView == null) return null;

    for (var wire in graphModel.nodeNetworkView!.wires) {
      final (sourcePos, _) = _getPinPositionAndDataType(wire.sourceNodeId, -1);
      final (destPos, _) = _getPinPositionAndDataType(
          wire.destNodeId, wire.destParamIndex.toInt());

      final hitTestPath = _getBand(sourcePos, destPos, HIT_TEST_WIRE_WIDTH);
      if (hitTestPath.contains(position)) {
        return WireHitResult(
            wire.sourceNodeId, wire.destNodeId, wire.destParamIndex);
      }
    }
    return null;
  }

  @override
  bool shouldRepaint(WirePainter oldDelegate) => true;
}
