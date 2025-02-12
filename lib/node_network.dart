import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';

/// Manages the entire node graph.
class GraphModel extends ChangeNotifier {
  NodeNetworkView? nodeNetworkView;

  GraphModel();

  void init(String nodeNetworkName) {
    nodeNetworkView = getNodeNetworkView(nodeNetworkName: nodeNetworkName);
  }

  /// Updates a node's position and notifies listeners.
  void updateNodePosition(BigInt nodeId, Offset newPosition) {
    //print('updateNodePosition nodeId: ${nodeId} newPosition: ${newPosition}');
    if (nodeNetworkView != null) {
      moveNode(nodeNetworkName: nodeNetworkView!.name, nodeId: nodeId, position: APIVec2(x: newPosition.dx, y: newPosition.dy));
      _refresh();
    }
  }

  void _refresh() {
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
          return Stack(
            children: (model.nodeNetworkView == null) ? [] : model.nodeNetworkView!.nodes.entries.map((entry) => NodeWidget(node: entry.value)).toList(),
          );
        },
      ),
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
      child: Draggable(
        feedback: DefaultTextStyle(
          style: DefaultTextStyle.of(context).style, // Preserve text style
          child: NodeViewWidget(node: node),
        ),
        childWhenDragging: SizedBox.shrink(), //Opacity(opacity: 0.5, child: NodeViewWidget()),
        onDragEnd: (details) {
          Provider.of<GraphModel>(context, listen: false)
              .updateNodePosition(node.id, details.offset);
        },
        child: NodeViewWidget(node: node),
      ),
    );
  }
}

/// Visual representation of a node.
class NodeViewWidget extends StatelessWidget {
  final NodeView node;

  const NodeViewWidget({required this.node, super.key});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 160,
      decoration: BoxDecoration(
        color: Colors.grey[900],
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: Colors.blueAccent, width: 2),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // Title Bar
          Container(
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
          // Main Body
          Padding(
            padding: EdgeInsets.all(8),
            child: Row(
              children: [
                // Left Side (Inputs)
                Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: node.inputPins.map((inputPin) => _buildInputPin(inputPin.name)).toList(),
                ),
                Spacer(),
                // Right Side (Output)
                _buildOutputPin(),
              ],
            ),
          ),
        ],
      ),
    );
  }

  /// Creates a labeled input pin.
  Widget _buildInputPin(String label) {
    return Row(
      children: [
        Container(
          width: 12,
          height: 12,
          decoration: BoxDecoration(
            color: Colors.blue,
            shape: BoxShape.circle,
          ),
        ),
        SizedBox(width: 6),
        Text(
          label,
          style: TextStyle(color: Colors.white, fontSize: 15),
        ),
      ],
    );
  }

  /// Creates an output pin without a label.
  Widget _buildOutputPin() {
    return Container(
      width: 12,
      height: 12,
      decoration: BoxDecoration(
        color: Colors.orange,
        shape: BoxShape.circle,
      ),
    );
  }
}
