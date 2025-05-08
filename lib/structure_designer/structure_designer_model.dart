import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer_api.dart'
    as structure_designer_api;
import 'package:vector_math/vector_math.dart' as vector_math;
import 'package:flutter_cad/common/api_utils.dart';

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
  APIEditAtomTool? activeEditAtomTool = APIEditAtomTool.default_;
  DraggedWire? draggedWire; // not null if there is a wire dragging in progress

  StructureDesignerModel();

  void init() {
    nodeNetworkView = structure_designer_api.getNodeNetworkView();
    nodeNetworkNames = structure_designer_api.getNodeNetworkNames() ?? [];
  }

  bool isEditAtomActive() {
    return structure_designer_api.isEditAtomActive();
  }

  void setActiveEditAtomTool(APIEditAtomTool tool) {
    structure_designer_api.setActiveEditAtomTool(tool: tool);
    refreshFromKernel();
  }

  void selectAtomOrBondByRay(vector_math.Vector3 rayStart,
      vector_math.Vector3 rayDir, SelectModifier selectModifier) {
    structure_designer_api.selectAtomOrBondByRay(
      rayStart: Vector3ToAPIVec3(rayStart),
      rayDir: Vector3ToAPIVec3(rayDir),
      selectModifier: selectModifier,
    );
    refreshFromKernel();
  }

  void saveNodeNetworks(String filePath) {
    structure_designer_api.saveNodeNetworks(filePath: filePath);
    refreshFromKernel();
  }

  void loadNodeNetworks(String filePath) {
    structure_designer_api.loadNodeNetworks(filePath: filePath);
    refreshFromKernel();
  }

  void setActiveNodeNetwork(String nodeNetworkName) {
    structure_designer_api.setActiveNodeNetwork(
        nodeNetworkName: nodeNetworkName);
    refreshFromKernel();
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
      structure_designer_api.moveNode(
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

    structure_designer_api.connectNodes(
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
        structure_designer_api.selectNode(
          nodeId: nodeId,
        );
      }
      refreshFromKernel();
    }
  }

  void renameNodeNetwork(String oldName, String newName) {
    final success = structure_designer_api.renameNodeNetwork(
      oldName: oldName,
      newName: newName,
    );

    if (success) {
      // If this was the active network, update the view
      if (nodeNetworkView != null && nodeNetworkView!.name == oldName) {
        nodeNetworkView = structure_designer_api.getNodeNetworkView();
      }
      nodeNetworkNames = structure_designer_api.getNodeNetworkNames() ?? [];
      notifyListeners();
    }
  }

  void setReturnNodeId(BigInt? nodeId) {
    structure_designer_api.setReturnNodeId(nodeId: nodeId);
    refreshFromKernel();
  }

  void addNewNodeNetwork() {
    structure_designer_api.addNewNodeNetwork();

    // Refresh the list of node networks
    nodeNetworkNames = structure_designer_api.getNodeNetworkNames() ?? [];

    // If we want to automatically set the new network as active,
    // we would need to get its name first (it's the last one in the list)
    if (nodeNetworkNames.isNotEmpty) {
      final newNetworkName = nodeNetworkNames.last;
      setActiveNodeNetwork(newNetworkName);
    }
  }

  void setSelectedWire(
      BigInt sourceNodeId, BigInt destNodeId, BigInt destParamIndex) {
    if (nodeNetworkView == null) return;
    //TODO: only select a wire if not already selected.
    structure_designer_api.selectWire(
        sourceNodeId: sourceNodeId,
        destinationNodeId: destNodeId,
        destinationArgumentIndex: destParamIndex);
    refreshFromKernel();
  }

  void toggleNodeDisplay(BigInt nodeId) {
    if (nodeNetworkView == null) return;
    final node = nodeNetworkView!.nodes[nodeId];
    if (node == null) return;

    structure_designer_api.setNodeDisplay(
      nodeId: nodeId,
      isDisplayed: !node.displayed,
    );
    refreshFromKernel();
  }

  void removeSelected() {
    if (nodeNetworkView == null) return;
    structure_designer_api.deleteSelected();
    refreshFromKernel();
  }

  void deleteSelectedAtomsAndBonds() {
    if (nodeNetworkView == null) return;
    structure_designer_api.deleteSelectedAtomsAndBonds();
    refreshFromKernel();
  }

  void replaceSelectedAtoms(int atomicNumber) {
    if (nodeNetworkView == null) return;
    structure_designer_api.replaceSelectedAtoms(atomicNumber: atomicNumber);
    refreshFromKernel();
  }

  void editAtomUndo() {
    if (nodeNetworkView == null) return;
    structure_designer_api.editAtomUndo();
    refreshFromKernel();
  }

  void editAtomRedo() {
    if (nodeNetworkView == null) return;
    structure_designer_api.editAtomRedo();
    refreshFromKernel();
  }

  bool setEditAtomDefaultData(int replacementAtomicNumber) {
    if (nodeNetworkView == null) return false;
    final result = structure_designer_api.setEditAtomDefaultData(
        replacementAtomicNumber: replacementAtomicNumber);
    refreshFromKernel();
    return result;
  }

  bool setEditAtomAddAtomData(int atomicNumber) {
    if (nodeNetworkView == null) return false;
    final result = structure_designer_api.setEditAtomAddAtomData(
        atomicNumber: atomicNumber);
    refreshFromKernel();
    return result;
  }

  void addAtomByRay(int atomicNumber, vector_math.Vector3 planeNormal,
      vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    if (nodeNetworkView == null) return;
    structure_designer_api.addAtomByRay(
      atomicNumber: atomicNumber,
      planeNormal: Vector3ToAPIVec3(planeNormal),
      rayStart: Vector3ToAPIVec3(rayStart),
      rayDir: Vector3ToAPIVec3(rayDir),
    );
    refreshFromKernel();
  }

  BigInt createNode(String nodeTypeName, Offset position) {
    if (nodeNetworkView == null) return BigInt.zero;
    final nodeId = structure_designer_api.addNode(
      nodeTypeName: nodeTypeName,
      position: APIVec2(x: position.dx, y: position.dy),
    );
    refreshFromKernel();
    return nodeId;
  }

  void refreshFromKernel() {
    nodeNetworkView = structure_designer_api.getNodeNetworkView();
    nodeNetworkNames = structure_designer_api.getNodeNetworkNames() ?? [];
    activeEditAtomTool = structure_designer_api.getActiveEditAtomTool();
    notifyListeners();
  }
}
