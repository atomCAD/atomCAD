import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer_api.dart';

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
class StructureDesignerModel extends ChangeNotifier {
  List<String> nodeNetworkNames = [];
  NodeNetworkView? nodeNetworkView;
  DraggedWire? draggedWire; // not null if there is a wire dragging in progress

  StructureDesignerModel();

  void init(String nodeNetworkName) {
    setActiveNodeNetwork(nodeNetworkName: nodeNetworkName);
    nodeNetworkView = getNodeNetworkView();
  }

  // Called on each small update when dragging a node
  // Works only on the UI: do not update the position in the kernel
  void dragNodePosition(BigInt nodeId, Offset delta) {
    final node = nodeNetworkView!.nodes[nodeId]!;
    node.position =
        APIVec2(x: node.position.x + delta.dx, y: node.position.y + delta.dy);
    notifyListeners();
  }

  /// Updates a node's position in the kernel and notifies listeners.
  void updateNodePosition(BigInt nodeId) {
    //print('updateNodePosition nodeId: ${nodeId} newPosition: ${newPosition}');
    if (nodeNetworkView != null) {
      final node = nodeNetworkView!.nodes[nodeId]!;
      moveNode(
          nodeId: nodeId,
          position: APIVec2(x: node.position.x, y: node.position.y));
      refreshFromKernel();
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
      sourceNodeId: outPin.nodeId,
      destNodeId: inPin.nodeId,
      destParamIndex: BigInt.from(inPin.pinIndex),
    );

    draggedWire = null;

    refreshFromKernel();
  }

  void setSelectedNode(BigInt nodeId) {
    if (nodeNetworkView != null) {
      if (!nodeNetworkView!.nodes[nodeId]!.selected) {
        selectNode(
          nodeId: nodeId,
        );
      }
      refreshFromKernel();
    }
  }

  void setSelectedWire(
      BigInt sourceNodeId, BigInt destNodeId, BigInt destParamIndex) {
    if (nodeNetworkView == null) return;
    //TODO: only select a wire if not already selected.
    selectWire(
        sourceNodeId: sourceNodeId,
        destinationNodeId: destNodeId,
        destinationArgumentIndex: destParamIndex);
    refreshFromKernel();
  }

  void toggleNodeDisplay(BigInt nodeId) {
    if (nodeNetworkView == null) return;
    final node = nodeNetworkView!.nodes[nodeId];
    if (node == null) return;

    setNodeDisplay(
      nodeId: nodeId,
      isDisplayed: !node.displayed,
    );
    refreshFromKernel();
  }

  void removeSelected() {
    if (nodeNetworkView == null) return;
    deleteSelected();
    refreshFromKernel();
  }

  BigInt createNode(String nodeTypeName, Offset position) {
    if (nodeNetworkView == null) return BigInt.zero;
    final nodeId = addNode(
      nodeTypeName: nodeTypeName,
      position: APIVec2(x: position.dx, y: position.dy),
    );
    refreshFromKernel();
    return nodeId;
  }

  void refreshFromKernel() {
    if (nodeNetworkView != null) {
      nodeNetworkView = getNodeNetworkView();
      nodeNetworkNames = getNodeNetworkNames() ?? [];
      notifyListeners();
    }
  }
}
