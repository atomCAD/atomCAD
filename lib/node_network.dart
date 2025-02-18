import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';
import 'package:flutter_cad/api_utils.dart';
import 'package:flutter_cad/add_node_popup.dart';

const double NODE_WIDTH = 120.0;
const double NODE_VERT_WIRE_OFFSET = 39.0;
const double NODE_VERT_WIRE_OFFSET_EMPTY = 46.0;
const double NODE_VERT_WIRE_OFFSET_PER_PARAM = 21.0;
const double CUBIC_SPLINE_HORIZ_OFFSET = 50.0;

Color getDataTypeColor(String dataType) {
  switch (dataType) {
    case 'Geometry':
      return Colors.blue;
    case 'Atomic':
      return Colors.orange;
    default:
      return Colors.grey; // Default color for unknown types
  }
}

class PinReference {
  BigInt nodeId;
  int pinIndex;
  String dataType;

  PinReference(this.nodeId, this.pinIndex, this.dataType);

  @override
  String toString() {
    return 'PinReference(nodeId: $nodeId, pinIndex: $pinIndex dataType: $dataType)';
  }

  @override
  bool operator ==(Object other) {
    if (identical(this, other)) return true;
    if (other is! PinReference) return false;
    return nodeId == other.nodeId &&
        pinIndex == other.pinIndex &&
        dataType == other.dataType;
  }

  @override
  int get hashCode => Object.hash(nodeId, pinIndex, dataType);
}

class DraggedWire {
  PinReference startPin;
  Offset wireEndPosition;

  DraggedWire(this.startPin, this.wireEndPosition);
}

/// Manages the entire node graph.
class GraphModel extends ChangeNotifier {
  NodeNetworkView? nodeNetworkView;
  DraggedWire? draggedWire; // not null if there is a wire dragging in progress

  GraphModel();

  void init(String nodeNetworkName) {
    nodeNetworkView = getNodeNetworkView(nodeNetworkName: nodeNetworkName);
  }

  // Called on each small update when dragging a node
  // Works only on the UI: do not update the position in the kernel
  void dragNodePosition(BigInt nodeId, Offset delta) {
      final node = nodeNetworkView!.nodes[nodeId]!;
      node.position = APIVec2(x: node.position.x + delta.dx, y: node.position.y + delta.dy);
      notifyListeners();    
  }

  /// Updates a node's position in the kernel and notifies listeners.
  void updateNodePosition(BigInt nodeId) {
    //print('updateNodePosition nodeId: ${nodeId} newPosition: ${newPosition}');
    if (nodeNetworkView != null) {
      final node = nodeNetworkView!.nodes[nodeId]!;
      moveNode(nodeNetworkName: nodeNetworkView!.name, nodeId: nodeId, position: APIVec2(x: node.position.x, y: node.position.y));
      _refreshFromKernel();
    }
  }

  void dragWire(PinReference startPin, Offset wireEndPosition) {
    draggedWire ??= DraggedWire(startPin, wireEndPosition);
    draggedWire!.wireEndPosition = wireEndPosition;
    notifyListeners();
  }

  void cancelDragWire() {
    if (draggedWire != null) {
      draggedWire = null;
      notifyListeners();
    }
  }

  void connectPins(PinReference pin1, PinReference pin2) {
    final outPin = pin1.pinIndex < 0 ? pin1 : pin2;
    final inPin = pin1.pinIndex < 0 ? pin2 : pin1;

    connectNodes(
      nodeNetworkName: nodeNetworkView!.name,
      sourceNodeId: outPin.nodeId,
      destNodeId: inPin.nodeId,
      destParamIndex: BigInt.from(inPin.pinIndex),
    );

    draggedWire = null;

    _refreshFromKernel();
  }

  void _refreshFromKernel() {
    if (nodeNetworkView != null) {
      nodeNetworkView = getNodeNetworkView(nodeNetworkName: nodeNetworkView!.name);
      notifyListeners();
    }
  }
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
                print("Node added: $selectedNode at ${details.localPosition}");
              }
            },
            child: Stack(
              children: (model.nodeNetworkView == null) ? [] : [
                CustomPaint(
                  painter: WirePainter(model),
                  child: Container(),
                ), ...(model.nodeNetworkView!.nodes.entries.map((entry) => NodeWidget(node: entry.value)).toList())
              ],
            )
          );
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
        width: 14,
        height: 14,
        decoration: multi ? BoxDecoration(
          border: Border.all(
            color: color, // Set the border color
            width: 5.0,        // Set the border width
          ),
          shape: BoxShape.circle,
          color: Colors.black,
        ) : BoxDecoration(
          shape: BoxShape.circle,
          color: color,
        )
      ),
    );
  }
}

class PinWidget extends StatelessWidget {
  final PinReference pinReference;
  final bool multi;
  PinWidget({required this.pinReference, required this.multi})
    : super(key: ValueKey(pinReference.pinIndex));

  @override
  Widget build(BuildContext context) {
    return DragTarget<PinReference>(
      builder: (context, candidateData, rejectedData) {
        return Draggable<PinReference>(
          data: pinReference,
          feedback: SizedBox.shrink(),
          childWhenDragging: PinViewWidget(dataType: pinReference.dataType, multi: multi),
          child: PinViewWidget(dataType: pinReference.dataType, multi: multi),
          onDragUpdate: (details) {
            Provider.of<GraphModel>(context, listen: false)
              .dragWire(pinReference, details.globalPosition);            
          },
          onDragEnd: (details) {
            Provider.of<GraphModel>(context, listen: false)
              .cancelDragWire();
          },
        );
      },
      onWillAcceptWithDetails: (details) {
        return 
          details.data.dataType == pinReference.dataType && // same data type
          (details.data.pinIndex < 0) != (pinReference.pinIndex < 0); // output to input
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
      child:  Container(
        width: NODE_WIDTH,
        decoration: BoxDecoration(
          color: Colors.grey[900],
          borderRadius: BorderRadius.circular(8),
          border: Border.all(color: Colors.blueAccent, width: 2),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            // Title Bar
            GestureDetector(
              onPanStart: (details) {
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
                  color: Colors.blueGrey[800],
                  borderRadius: BorderRadius.vertical(top: Radius.circular(6)),
                ),
                child: Text(
                  node.nodeTypeName,
                  style: TextStyle(
                    color: Colors.white,
                    fontWeight: FontWeight.bold,
                    fontSize: 15,
                  ),
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
                    children: node.inputPins.asMap().entries.map(
                      (entry) => _buildInputPin(
                        entry.value.name,
                        PinReference(node.id, entry.key, entry.value.dataType),
                        entry.value.multi
                      )
                    ).toList(),
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
      )
    );
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
      ..strokeWidth = 2
      ..style = PaintingStyle.stroke;

    for (var wire in graphModel.nodeNetworkView!.wires) {
      final source = _getPinPositionAndDataType(wire.sourceNodeId, -1);
      final dest = _getPinPositionAndDataType(wire.destNodeId, wire.destParamIndex.toInt());
      _drawWire(source.$1, dest.$1, canvas, paint, source.$2);
    }

    if (graphModel.draggedWire != null) {
      final wireStart = _getPinPositionAndDataType(graphModel.draggedWire!.startPin.nodeId, graphModel.draggedWire!.startPin.pinIndex);
      final wireEndPos = graphModel.draggedWire!.wireEndPosition;
      if (graphModel.draggedWire!.startPin.pinIndex < 0) { // start is source
        _drawWire(wireStart.$1, wireEndPos, canvas, paint, wireStart.$2); 
      } else { // start is dest
        _drawWire(wireEndPos, wireStart.$1, canvas, paint, wireStart.$2); 
      }
    }    
  }

  (Offset, String) _getPinPositionAndDataType(BigInt nodeId, int pinIndex) {
    // Now this is is a bit of a hacky solution.
    // We should probably use the real positions of the pin widgets instead of this logic to
    // approximate it independently.
    if (pinIndex < 0) { // output pin (source pin)
      final sourceNode = graphModel.nodeNetworkView!.nodes[nodeId];
      final sourceVertOffset = sourceNode!.inputPins.isEmpty ? NODE_VERT_WIRE_OFFSET_EMPTY : NODE_VERT_WIRE_OFFSET + sourceNode.inputPins.length * NODE_VERT_WIRE_OFFSET_PER_PARAM * 0.5;
      return (APIVec2ToOffset(sourceNode.position) + Offset(NODE_WIDTH, sourceVertOffset), sourceNode.outputType);
    } else { // input pin (dest pin)
      final destNode = graphModel.nodeNetworkView!.nodes[nodeId];
      final destVertOffset = NODE_VERT_WIRE_OFFSET + (pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
      return (APIVec2ToOffset(destNode!.position) + Offset(0.0, destVertOffset), destNode.inputPins[pinIndex].dataType);
    }
  }

  _drawWire(Offset sourcePos, Offset destPos, Canvas canvas, Paint paint, String dataType) {
    paint.color = getDataTypeColor(dataType);

    final controlPoint1 = sourcePos + Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);
    final controlPoint2 = destPos - Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);

    final path = Path()
      ..moveTo(sourcePos.dx, sourcePos.dy)
      ..cubicTo(
        controlPoint1.dx, controlPoint1.dy,
        controlPoint2.dx, controlPoint2.dy,
        destPos.dx, destPos.dy,
      );
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(WirePainter oldDelegate) => true;
}
