import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:vector_math/vector_math.dart' as vector_math;
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/edit_atom_api.dart'
    as edit_atom_api;
import 'package:flutter_cad/src/rust/api/structure_designer/anchor_api.dart'
    as anchor_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as structure_designer_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/stamp_api.dart'
    as stamp_api;
import 'package:flutter_cad/src/rust/api/structure_designer/facet_shell_api.dart'
    as facet_shell_api;
import 'package:flutter_cad/src/rust/api/common_api.dart' as common_api;

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
  APICameraCanonicalView cameraCanonicalView = APICameraCanonicalView.custom;
  bool isOrthographic = false;
  StructureDesignerPreferences? preferences;

  StructureDesignerModel() {}

  void init() {
    refreshFromKernel();
  }

  void setCameraTransform(APITransform transform) {
    common_api.setCameraTransform(transform: transform);
    refreshFromKernel();
  }

  void setCameraCanonicalView(APICameraCanonicalView view) {
    common_api.setCameraCanonicalView(view: view);
    refreshFromKernel();
  }

  void setOrthographicMode(bool orthographic) {
    common_api.setOrthographicMode(orthographic: orthographic);
    refreshFromKernel();
  }

  void setPreferences(StructureDesignerPreferences preferences) {
    structure_designer_api.setStructureDesignerPreferences(
        preferences: preferences);
    refreshFromKernel();
  }

  void selectAnchorAtomByRay(
      vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    anchor_api.selectAnchorAtomByRay(
      rayStart: Vector3ToAPIVec3(rayStart),
      rayDir: Vector3ToAPIVec3(rayDir),
    );
    refreshFromKernel();
  }

  void addOrSelectStampPlacementByRay(
      vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    stamp_api.addOrSelectStampPlacementByRay(
      rayStart: Vector3ToAPIVec3(rayStart),
      rayDir: Vector3ToAPIVec3(rayDir),
    );
    refreshFromKernel();
  }

  void setStampRotation(BigInt nodeId, int rotation) {
    stamp_api.setStampRotation(
      nodeId: nodeId,
      rotation: rotation,
    );
    refreshFromKernel();
  }

  void deleteSelectedStampPlacement(BigInt nodeId) {
    stamp_api.deleteSelectedStampPlacement(
      nodeId: nodeId,
    );
    refreshFromKernel();
  }

  bool isNodeTypeActive(String nodeType) {
    return structure_designer_api.isNodeTypeActive(nodeType: nodeType);
  }

  void setActiveEditAtomTool(APIEditAtomTool tool) {
    edit_atom_api.setActiveEditAtomTool(tool: tool);
    refreshFromKernel();
  }

  void selectAtomOrBondByRay(vector_math.Vector3 rayStart,
      vector_math.Vector3 rayDir, SelectModifier selectModifier) {
    edit_atom_api.selectAtomOrBondByRay(
      rayStart: Vector3ToAPIVec3(rayStart),
      rayDir: Vector3ToAPIVec3(rayDir),
      selectModifier: selectModifier,
    );
    refreshFromKernel();
  }

  void clearSelection() {
    structure_designer_api.clearSelection();
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
    edit_atom_api.deleteSelectedAtomsAndBonds();
    refreshFromKernel();
  }

  void replaceSelectedAtoms(int atomicNumber) {
    if (nodeNetworkView == null) return;
    edit_atom_api.replaceSelectedAtoms(atomicNumber: atomicNumber);
    refreshFromKernel();
  }

  void transformSelected(APITransform absTransform) {
    if (nodeNetworkView == null) return;
    edit_atom_api.transformSelected(absTransform: absTransform);
    refreshFromKernel();
  }

  void editAtomUndo() {
    if (nodeNetworkView == null) return;
    edit_atom_api.editAtomUndo();
    refreshFromKernel();
  }

  void editAtomRedo() {
    if (nodeNetworkView == null) return;
    edit_atom_api.editAtomRedo();
    refreshFromKernel();
  }

  bool setEditAtomDefaultData(int replacementAtomicNumber) {
    if (nodeNetworkView == null) return false;
    final result = edit_atom_api.setEditAtomDefaultData(
        replacementAtomicNumber: replacementAtomicNumber);
    refreshFromKernel();
    return result;
  }

  bool setEditAtomAddAtomData(int atomicNumber) {
    if (nodeNetworkView == null) return false;
    final result =
        edit_atom_api.setEditAtomAddAtomData(atomicNumber: atomicNumber);
    refreshFromKernel();
    return result;
  }

  void addAtomByRay(int atomicNumber, vector_math.Vector3 planeNormal,
      vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    if (nodeNetworkView == null) return;
    edit_atom_api.addAtomByRay(
      atomicNumber: atomicNumber,
      planeNormal: Vector3ToAPIVec3(planeNormal),
      rayStart: Vector3ToAPIVec3(rayStart),
      rayDir: Vector3ToAPIVec3(rayDir),
    );
    refreshFromKernel();
  }

  void drawBondByRay(vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    if (nodeNetworkView == null) return;
    edit_atom_api.drawBondByRay(
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

  void setCuboidData(BigInt nodeId, APICuboidData data) {
    structure_designer_api.setCuboidData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setSphereData(BigInt nodeId, APISphereData data) {
    structure_designer_api.setSphereData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setExtrudeData(BigInt nodeId, APIExtrudeData data) {
    structure_designer_api.setExtrudeData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setHalfSpaceData(BigInt nodeId, APIHalfSpaceData data) {
    structure_designer_api.setHalfSpaceData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setRectData(BigInt nodeId, APIRectData data) {
    structure_designer_api.setRectData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setCircleData(BigInt nodeId, APICircleData data) {
    structure_designer_api.setCircleData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setHalfPlaneData(BigInt nodeId, APIHalfPlaneData data) {
    structure_designer_api.setHalfPlaneData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setRegPolyData(BigInt nodeId, APIRegPolyData data) {
    structure_designer_api.setRegPolyData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void refreshFromKernel() {
    nodeNetworkView = structure_designer_api.getNodeNetworkView();
    nodeNetworkNames = structure_designer_api.getNodeNetworkNames() ?? [];
    activeEditAtomTool = edit_atom_api.getActiveEditAtomTool();
    cameraCanonicalView = common_api.getCameraCanonicalView();
    isOrthographic = common_api.isOrthographic();
    preferences = structure_designer_api.getStructureDesignerPreferences();

    notifyListeners();
  }

  // Facet Shell API wrapper methods
  APIFacetShellData? getFacetShellData(BigInt nodeId) {
    if (nodeNetworkView == null) return null;
    return facet_shell_api.getFacetShellData(nodeId: nodeId);
  }

  bool setFacetShellCenter(BigInt nodeId, APIIVec3 center, int maxMillerIndex) {
    if (nodeNetworkView == null) return false;
    final result = facet_shell_api.setFacetShellCenter(
      nodeId: nodeId,
      center: center,
      maxMillerIndex: maxMillerIndex,
    );
    refreshFromKernel();
    return result;
  }

  bool addFacet(BigInt nodeId, APIFacet facet) {
    if (nodeNetworkView == null) return false;
    final result = facet_shell_api.addFacet(
      nodeId: nodeId,
      facet: facet,
    );
    refreshFromKernel();
    return result;
  }

  bool updateFacet(BigInt nodeId, BigInt index, APIFacet facet) {
    if (nodeNetworkView == null) return false;
    final result = facet_shell_api.updateFacet(
      nodeId: nodeId,
      index: index,
      facet: facet,
    );
    refreshFromKernel();
    return result;
  }

  bool removeFacet(BigInt nodeId, BigInt index) {
    if (nodeNetworkView == null) return false;
    final result = facet_shell_api.removeFacet(
      nodeId: nodeId,
      index: index,
    );
    refreshFromKernel();
    return result;
  }

  bool clearFacets(BigInt nodeId) {
    if (nodeNetworkView == null) return false;
    final result = facet_shell_api.clearFacets(nodeId: nodeId);
    refreshFromKernel();
    return result;
  }

  bool selectFacet(BigInt nodeId, BigInt? index) {
    if (nodeNetworkView == null) return false;
    final result = facet_shell_api.selectFacet(
      nodeId: nodeId,
      index: index,
    );
    refreshFromKernel();
    return result;
  }

  bool splitSymmetryMembers(BigInt nodeId, BigInt facetIndex) {
    if (nodeNetworkView == null) return false;
    final result = facet_shell_api.splitSymmetryMembers(
      nodeId: nodeId,
      facetIndex: facetIndex,
    );
    refreshFromKernel();
    return result;
  }
}
