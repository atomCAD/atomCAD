import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';

// Pin appearance constants
const double PIN_SIZE = 14.0;
const double PIN_BORDER_WIDTH = 5.0;
const double PIN_HIT_AREA_WIDTH = 24.0; // Larger hit area for easier dragging
const double PIN_HIT_AREA_HEIGHT = 22.0;

// Node appearance constants
const Color NODE_BACKGROUND_COLOR = Color(0xFF212121); // Colors.grey[900]
const Color NODE_BORDER_COLOR_SELECTED = Colors.orange;
const Color NODE_BORDER_COLOR_NORMAL = Colors.blueAccent;
const Color NODE_BORDER_COLOR_ERROR = Colors.red;
const double NODE_BORDER_WIDTH_SELECTED = 3.0;
const double NODE_BORDER_WIDTH_NORMAL = 2.0;
const double NODE_BORDER_RADIUS = 8.0;
const Color NODE_TITLE_COLOR_SELECTED = Color(0xFFD84315); // Colors.orange[800]
const Color NODE_TITLE_COLOR_NORMAL = Color(0xFF37474F); // Colors.blueGrey[800]
const Color NODE_TITLE_COLOR_RETURN = Color(0xFF0D47A1); // Dark blue

const double WIRE_GLOW_BLUR_RADIUS = 8.0;
const double WIRE_GLOW_SPREAD_RADIUS = 2.0;

Color getDataTypeColor(String dataType) {
  return DATA_TYPE_COLORS[dataType] ?? DEFAULT_DATA_TYPE_COLOR;
}

class PinViewWidget extends StatelessWidget {
  final String dataType;
  final bool multi;
  final String? outputString;

  const PinViewWidget(
      {super.key,
      required this.dataType,
      required this.multi,
      this.outputString});

  @override
  Widget build(BuildContext context) {
    final color = getDataTypeColor(dataType);

    String tooltipMessage = dataType;
    if (outputString != null && outputString!.isNotEmpty) {
      tooltipMessage = '$dataType\n$outputString';
    }

    return Tooltip(
      message: tooltipMessage,
      preferBelow: false,
      child: Center(
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
      ),
    );
  }
}

class PinWidget extends StatelessWidget {
  final PinReference pinReference;
  final bool multi;
  final String? outputString;
  PinWidget(
      {required this.pinReference, required this.multi, this.outputString})
      : super(
            key: ValueKey(pinReference.pinIndex +
                ((pinReference.pinType == PinType.output) ? 1000 : 0)));

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
    return Container(
      width: PIN_HIT_AREA_WIDTH,
      height: PIN_HIT_AREA_HEIGHT,
      child: DragTarget<PinReference>(
        builder: (context, candidateData, rejectedData) {
          return Draggable<PinReference>(
            data: pinReference,
            feedback: SizedBox.shrink(),
            childWhenDragging: Container(
              width: PIN_HIT_AREA_WIDTH,
              height: PIN_HIT_AREA_HEIGHT,
              child: Center(
                child: PinViewWidget(
                    dataType: pinReference.dataType,
                    multi: multi,
                    outputString: outputString),
              ),
            ),
            child: Container(
              width: PIN_HIT_AREA_WIDTH,
              height: PIN_HIT_AREA_HEIGHT,
              child: Center(
                child: PinViewWidget(
                    dataType: pinReference.dataType,
                    multi: multi,
                    outputString: outputString),
              ),
            ),
            onDragUpdate: (details) {
              final nodeNetworkBox = _findNodeNetworkRenderBox(context);
              if (nodeNetworkBox != null) {
                final position =
                    nodeNetworkBox.globalToLocal(details.globalPosition);
                Provider.of<StructureDesignerModel>(context, listen: false)
                    .dragWire(pinReference, position);
              }
            },
            onDragEnd: (details) {
              Provider.of<StructureDesignerModel>(context, listen: false)
                  .cancelDragWire();
            },
          );
        },
        onWillAcceptWithDetails: (details) {
          return Provider.of<StructureDesignerModel>(context, listen: false)
              .canConnectPins(details.data, pinReference);
        },
        onAcceptWithDetails: (details) {
          //print("Connected pin ${details.data} to pin $pinReference");
          Provider.of<StructureDesignerModel>(context, listen: false)
              .connectPins(details.data, pinReference);
        },
      ),
    );
  }
}

/// Widget representing a single draggable node.
class NodeWidget extends StatelessWidget {
  final NodeView node;
  final Offset panOffset;

  NodeWidget({required this.node, required this.panOffset})
      : super(key: ValueKey(node.id));

  @override
  Widget build(BuildContext context) {
    // Create the base node widget content
    Widget nodeContent = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        // Title Bar
        GestureDetector(
          onTapDown: (details) {
            final model =
                Provider.of<StructureDesignerModel>(context, listen: false);
            model.setSelectedNode(node.id);
          },
          onPanStart: (details) {
            final model =
                Provider.of<StructureDesignerModel>(context, listen: false);
            model.setSelectedNode(node.id);
          },
          onPanUpdate: (details) {
            // The dragNodePosition updates the model's absolute position
            // The UI applies panOffset separately during rendering
            // So we just pass the raw delta to the model
            Provider.of<StructureDesignerModel>(context, listen: false)
                .dragNodePosition(node.id, details.delta);
          },
          onPanEnd: (details) {
            Provider.of<StructureDesignerModel>(context, listen: false)
                .updateNodePosition(node.id);
          },
          onSecondaryTapDown: (details) {
            final model =
                Provider.of<StructureDesignerModel>(context, listen: false);
            model.setSelectedNode(node.id);

            final RenderBox overlay =
                Overlay.of(context).context.findRenderObject() as RenderBox;
            final RelativeRect position = RelativeRect.fromRect(
              Rect.fromPoints(
                details.globalPosition,
                details.globalPosition,
              ),
              Offset.zero & overlay.size,
            );

            showMenu(
              context: context,
              position: position,
              items: [
                PopupMenuItem(
                  value: 'return',
                  child: Text(node.returnNode
                      ? 'Unset as return node'
                      : 'Set as return node'),
                ),
              ],
            ).then((value) {
              if (value == 'return') {
                final model =
                    Provider.of<StructureDesignerModel>(context, listen: false);
                if (node.returnNode) {
                  // Unset as return node (pass null to clear the return node)
                  model.setReturnNodeId(null);
                } else {
                  // Set as return node (pass the node ID)
                  model.setReturnNodeId(node.id);
                }
              }
            });
          },
          child: Container(
            padding:
                const EdgeInsets.only(top: 4, bottom: 4, left: 8, right: 2),
            decoration: BoxDecoration(
              color: node.selected
                  ? NODE_TITLE_COLOR_SELECTED
                  : (node.returnNode
                      ? NODE_TITLE_COLOR_RETURN
                      : NODE_TITLE_COLOR_NORMAL),
              borderRadius: BorderRadius.vertical(
                  top: Radius.circular(NODE_BORDER_RADIUS - 2)),
            ),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    node.nodeTypeName,
                    style: const TextStyle(
                      color: Colors.white,
                      fontWeight: FontWeight.bold,
                      fontSize: 14,
                    ),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                GestureDetector(
                  onTap: () {
                    final model = Provider.of<StructureDesignerModel>(context,
                        listen: false);
                    model.toggleNodeDisplay(node.id);
                  },
                  child: Icon(
                    node.displayed ? Icons.visibility : Icons.visibility_off,
                    color: Colors.white,
                    size: 20,
                  ),
                ),
                const SizedBox(width: 4),
                // Function pin
                PinWidget(
                  pinReference: PinReference(
                      node.id, PinType.output, -1, node.functionType),
                  multi: false,
                ),
              ],
            ),
          ),
        ),
        // Main Body
        Padding(
          padding: const EdgeInsets.all(2),
          child: Row(
            children: [
              // Left Side (Inputs)
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: node.inputPins
                      .asMap()
                      .entries
                      .map((entry) => _buildInputPin(
                          entry.value.name,
                          PinReference(node.id, PinType.input, entry.key,
                              entry.value.dataType),
                          entry.value.multi))
                      .toList(),
                ),
              ),
              // Right Side (Output)
              PinWidget(
                pinReference:
                    PinReference(node.id, PinType.output, 0, node.outputType),
                multi: false,
                outputString: node.outputString,
              ),
            ],
          ),
        ),
        // Subtitle (if present)
        if (node.subtitle != null && node.subtitle!.isNotEmpty)
          Container(
            width: double.infinity,
            padding: const EdgeInsets.only(left: 8, right: 8, bottom: 4),
            child: Tooltip(
              message: node.subtitle!,
              preferBelow: true,
              child: Text(
                node.subtitle!,
                style: const TextStyle(
                  color: Colors.white70,
                  fontSize: 12,
                  fontStyle: FontStyle.italic,
                ),
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.center,
              ),
            ),
          ),
      ],
    );

    // Create container with node appearance
    Widget nodeWidget = Container(
      width: NODE_WIDTH,
      decoration: BoxDecoration(
        color: NODE_BACKGROUND_COLOR,
        borderRadius: BorderRadius.circular(NODE_BORDER_RADIUS),
        border: Border.all(
            color: node.error != null
                ? NODE_BORDER_COLOR_ERROR
                : (node.selected
                    ? NODE_BORDER_COLOR_SELECTED
                    : NODE_BORDER_COLOR_NORMAL),
            width: node.selected
                ? NODE_BORDER_WIDTH_SELECTED
                : NODE_BORDER_WIDTH_NORMAL),
        boxShadow: node.error != null
            ? [
                BoxShadow(
                    color:
                        NODE_BORDER_COLOR_ERROR.withOpacity(WIRE_GLOW_OPACITY),
                    blurRadius: WIRE_GLOW_BLUR_RADIUS,
                    spreadRadius: WIRE_GLOW_SPREAD_RADIUS)
              ]
            : (node.selected
                ? [
                    BoxShadow(
                        color: NODE_BORDER_COLOR_SELECTED
                            .withOpacity(WIRE_GLOW_OPACITY),
                        blurRadius: WIRE_GLOW_BLUR_RADIUS,
                        spreadRadius: WIRE_GLOW_SPREAD_RADIUS)
                  ]
                : null),
      ),
      child: nodeContent,
    );

    // Add tooltip for nodes with errors
    if (node.error != null && node.error!.isNotEmpty) {
      nodeWidget = Tooltip(
        message: node.error!,
        textStyle: const TextStyle(fontSize: 14, color: Colors.white),
        decoration: BoxDecoration(
          color: Colors.red.shade700,
          borderRadius: BorderRadius.circular(4),
        ),
        waitDuration: const Duration(milliseconds: 500),
        showDuration: const Duration(seconds: 5),
        padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 12),
        preferBelow: true,
        verticalOffset: 35.0,
        child: nodeWidget,
      );
    }

    // Position the node, taking into account both the node's position and the pan offset
    return Positioned(
      left: node.position.x + panOffset.dx,
      top: node.position.y + panOffset.dy,
      child: nodeWidget,
    );
  }

  /// Creates a labeled input pin.
  Widget _buildInputPin(String label, PinReference pinReference, bool multi) {
    return Row(
      children: [
        PinWidget(pinReference: pinReference, multi: multi),
        SizedBox(width: 2),
        Expanded(
          child: Text(
            label,
            style: TextStyle(color: Colors.white, fontSize: 14),
            overflow: TextOverflow.ellipsis,
          ),
        ),
      ],
    );
  }
}
