import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network.dart';

// Pin appearance constants
const double PIN_SIZE = 14.0;
const double PIN_BORDER_WIDTH = 5.0;

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
        return details.data.dataType ==
                pinReference.dataType && // same data type
            (details.data.pinIndex < 0) !=
                (pinReference.pinIndex < 0); // output to input
      },
      onAcceptWithDetails: (details) {
        //print("Connected pin ${details.data} to pin $pinReference");
        Provider.of<StructureDesignerModel>(context, listen: false)
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
            padding: const EdgeInsets.symmetric(vertical: 4, horizontal: 8),
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
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text(
                  node.nodeTypeName,
                  style: const TextStyle(
                    color: Colors.white,
                    fontWeight: FontWeight.bold,
                    fontSize: 14,
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
              ],
            ),
          ),
        ),
        // Main Body
        Padding(
          padding: const EdgeInsets.all(8),
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
                        PinReference(node.id, entry.key, entry.value.dataType),
                        entry.value.multi))
                    .toList(),
              ),
              const Spacer(),
              // Right Side (Output)
              PinWidget(
                pinReference: PinReference(node.id, -1, node.outputType),
                multi: false,
              ),
            ],
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
        child: nodeWidget,
      );
    }

    return Positioned(
      left: node.position.x,
      top: node.position.y,
      child: nodeWidget,
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
          style: TextStyle(color: Colors.white, fontSize: 14),
        ),
      ],
    );
  }
}
