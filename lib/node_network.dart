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
            children: (model.nodeNetworkView == null) ? [] : model.nodeNetworkView!.nodes.map((node) => NodeWidget(node: node)).toList(),
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
        feedback: NodeViewWidget(),
        childWhenDragging: Opacity(opacity: 0.5, child: NodeViewWidget()),
        onDragEnd: (details) {
          Provider.of<GraphModel>(context, listen: false)
              .updateNodePosition(node.id, details.offset);
        },
        child: NodeViewWidget(),
      ),
    );
  }
}

/// Visual representation of a node.
class NodeViewWidget extends StatelessWidget {
  const NodeViewWidget({super.key});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 80,
      height: 40,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: Colors.blue,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Text(
        "Node",
        style: TextStyle(color: Colors.white, fontWeight: FontWeight.bold),
      ),
    );
  }
}
