import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

/// Represents a single node in the graph.
class NodeModel {
  final int id;
  Offset position;

  NodeModel({required this.id, required this.position});
}

/// Manages the entire node graph.
class GraphModel extends ChangeNotifier {
  final List<NodeModel> nodes;

  GraphModel({required this.nodes});

  /// Updates a node's position and notifies listeners.
  void updateNodePosition(int nodeId, Offset newPosition) {
    print('updateNodePosition nodeId: ${nodeId} newPosition: ${newPosition}');
    final node = nodes.firstWhere((n) => n.id == nodeId);
    node.position = newPosition;
    notifyListeners();
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
            children: model.nodes.map((node) => NodeWidget(node: node)).toList(),
          );
        },
      ),
    );
  }
}

/// Widget representing a single draggable node.
class NodeWidget extends StatelessWidget {
  final NodeModel node;

  NodeWidget({required this.node}) : super(key: ValueKey(node.id));

  @override
  Widget build(BuildContext context) {
    return Positioned(
      left: node.position.dx,
      top: node.position.dy,
      child: Draggable(
        feedback: NodeView(),
        childWhenDragging: Opacity(opacity: 0.5, child: NodeView()),
        onDragEnd: (details) {
          Provider.of<GraphModel>(context, listen: false)
              .updateNodePosition(node.id, details.offset);
        },
        child: NodeView(),
      ),
    );
  }
}

/// Visual representation of a node.
class NodeView extends StatelessWidget {
  const NodeView({super.key});

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
