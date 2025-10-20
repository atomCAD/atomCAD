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
import 'package:flutter_cad/src/rust/api/structure_designer/import_xyz_api.dart'
    as import_xyz_api;
import 'package:flutter_cad/src/rust/api/common_api.dart' as common_api;

enum PinType {
  input,
  output,
}

class PinReference {
  BigInt nodeId;
  PinType pinType;
  int pinIndex;
  String dataType;

  PinReference(this.nodeId, this.pinType, this.pinIndex, this.dataType);

  @override
  String toString() {
    return 'PinReference(nodeId: $nodeId, pinType: $pinType, pinIndex: $pinIndex dataType: $dataType)';
  }

  @override
  bool operator ==(Object other) {
    if (identical(this, other)) return true;
    if (other is! PinReference) return false;
    return nodeId == other.nodeId &&
        pinType == other.pinType &&
        pinIndex == other.pinIndex &&
        dataType == other.dataType;
  }

  @override
  int get hashCode => Object.hash(nodeId, pinType, pinIndex, dataType);
}

class DraggedWire {
  PinReference startPin;
  Offset wireEndPosition;

  DraggedWire(this.startPin, this.wireEndPosition);
}

/// Manages the entire node graph.
class StructureDesignerModel extends ChangeNotifier {
  List<APINetworkWithValidationErrors> nodeNetworkNames = [];
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

  void selectFacetShellFacetByRay(
      vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    facet_shell_api.selectFacetByRay(
      rayStart: Vector3ToAPIVec3(rayStart),
      rayDir: Vector3ToAPIVec3(rayDir),
    );
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

  APIResult loadNodeNetworks(String filePath) {
    final result = structure_designer_api.loadNodeNetworks(filePath: filePath);
    refreshFromKernel();
    return result;
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

  bool canConnectPins(PinReference pin1, PinReference pin2) {
    final outPin = pin1.pinType == PinType.output ? pin1 : pin2;
    final inPin = pin1.pinType == PinType.input ? pin1 : pin2;

    return structure_designer_api.canConnectNodes(
      sourceNodeId: outPin.nodeId,
      sourceOutputPinIndex: outPin.pinIndex,
      destNodeId: inPin.nodeId,
      destParamIndex: BigInt.from(inPin.pinIndex),
    );
  }

  void connectPins(PinReference pin1, PinReference pin2) {
    final outPin = pin1.pinType == PinType.output ? pin1 : pin2;
    final inPin = pin1.pinType == PinType.input ? pin1 : pin2;

    structure_designer_api.connectNodes(
      sourceNodeId: outPin.nodeId,
      sourceOutputPinIndex: outPin.pinIndex,
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
      nodeNetworkNames =
          structure_designer_api.getNodeNetworksWithValidation() ?? [];
      notifyListeners();
    }
  }

  String? deleteNodeNetwork(String networkName) {
    final result = structure_designer_api.deleteNodeNetwork(
      networkName: networkName,
    );

    if (result.success) {
      // Clear the active network view if it was the deleted network
      if (nodeNetworkView != null && nodeNetworkView!.name == networkName) {
        nodeNetworkView = null;
      }
      nodeNetworkNames =
          structure_designer_api.getNodeNetworksWithValidation() ?? [];
      notifyListeners();
      return null; // Success
    } else {
      return result.errorMessage; // Return error message
    }
  }

  void setReturnNodeId(BigInt? nodeId) {
    structure_designer_api.setReturnNodeId(nodeId: nodeId);
    refreshFromKernel();
  }

  void addNewNodeNetwork() {
    structure_designer_api.addNewNodeNetwork();

    // Refresh the list of node networks
    nodeNetworkNames =
        structure_designer_api.getNodeNetworksWithValidation() ?? [];

    // If we want to automatically set the new network as active,
    // we would need to get its name first (it's the last one in the list)
    if (nodeNetworkNames.isNotEmpty) {
      final newNetworkName = nodeNetworkNames.last.name;
      setActiveNodeNetwork(newNetworkName);
    }
  }

  void setSelectedWire(BigInt sourceNodeId, BigInt sourceOutputPinIndex,
      BigInt destNodeId, BigInt destParamIndex) {
    if (nodeNetworkView == null) return;
    //TODO: only select a wire if not already selected.
    structure_designer_api.selectWire(
        sourceNodeId: sourceNodeId,
        sourceOutputPinIndex: sourceOutputPinIndex.toInt(),
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

  BigInt duplicateNode(BigInt nodeId) {
    if (nodeNetworkView == null) return BigInt.zero;
    final newNodeId = structure_designer_api.duplicateNode(nodeId: nodeId);
    refreshFromKernel();
    return newNodeId;
  }

  void setIntData(BigInt nodeId, APIIntData data) {
    structure_designer_api.setIntData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setStringData(BigInt nodeId, APIStringData data) {
    structure_designer_api.setStringData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setBoolData(BigInt nodeId, APIBoolData data) {
    structure_designer_api.setBoolData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setFloatData(BigInt nodeId, APIFloatData data) {
    structure_designer_api.setFloatData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setIvec2Data(BigInt nodeId, APIIVec2Data data) {
    structure_designer_api.setIvec2Data(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setIvec3Data(BigInt nodeId, APIIVec3Data data) {
    structure_designer_api.setIvec3Data(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setRangeData(BigInt nodeId, APIRangeData data) {
    structure_designer_api.setRangeData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setVec2Data(BigInt nodeId, APIVec2Data data) {
    structure_designer_api.setVec2Data(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setVec3Data(BigInt nodeId, APIVec3Data data) {
    structure_designer_api.setVec3Data(nodeId: nodeId, data: data);
    refreshFromKernel();
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

  void setGeoTransData(BigInt nodeId, APIGeoTransData data) {
    structure_designer_api.setGeoTransData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  APILatticeSymopData? getLatticeSymopData(BigInt nodeId) {
    return structure_designer_api.getLatticeSymopData(nodeId: nodeId);
  }

  void setLatticeSymopData(BigInt nodeId, APILatticeSymopData data) {
    structure_designer_api.setLatticeSymopData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setAtomTransData(BigInt nodeId, APIAtomTransData data) {
    structure_designer_api.setAtomTransData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setParameterData(BigInt nodeId, APIParameterData data) {
    structure_designer_api.setParameterData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setMApData(BigInt nodeId, APIMapData data) {
    structure_designer_api.setMapData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  APIResult setExprData(BigInt nodeId, APIExprData data) {
    final result =
        structure_designer_api.setExprData(nodeId: nodeId, data: data);
    refreshFromKernel();
    return result;
  }

  void setMotifData(BigInt nodeId, APIMotifData data) {
    structure_designer_api.setMotifData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  APIMotifData? getMotifData(BigInt nodeId) {
    return structure_designer_api.getMotifData(nodeId: nodeId);
  }

  void setAtomFillData(BigInt nodeId, APIAtomFillData data) {
    structure_designer_api.setAtomFillData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  APIAtomFillData? getAtomFillData(BigInt nodeId) {
    return structure_designer_api.getAtomFillData(nodeId: nodeId);
  }

  APIParameterData? getParameterData(BigInt nodeId) {
    return structure_designer_api.getParameterData(nodeId: nodeId);
  }

  APIExprData? getExprData(BigInt nodeId) {
    return structure_designer_api.getExprData(nodeId: nodeId);
  }

  void setImportXyzData(BigInt nodeId, APIImportXYZData data) {
    structure_designer_api.setImportXyzData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  APIImportXYZData? getImportXyzData(BigInt nodeId) {
    return structure_designer_api.getImportXyzData(nodeId: nodeId);
  }

  void setExportXyzData(BigInt nodeId, APIExportXYZData data) {
    structure_designer_api.setExportXyzData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  APIExportXYZData? getExportXyzData(BigInt nodeId) {
    return structure_designer_api.getExportXyzData(nodeId: nodeId);
  }

  APIResult importXyz(BigInt nodeId) {
    var result = import_xyz_api.importXyz(nodeId: nodeId);
    refreshFromKernel();
    return result;
  }

  void setAtomCutData(BigInt nodeId, APIAtomCutData data) {
    structure_designer_api.setAtomCutData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setUnitCellData(BigInt nodeId, APIUnitCellData data) {
    structure_designer_api.setUnitCellData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void refreshFromKernel() {
    nodeNetworkView = structure_designer_api.getNodeNetworkView();
    nodeNetworkNames =
        structure_designer_api.getNodeNetworksWithValidation() ?? [];
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

  /// Exports all visible atomic structures as a single XYZ file
  APIResult exportVisibleAtomicStructuresAsXyz(String filePath) {
    final ret = structure_designer_api.exportVisibleAtomicStructuresAsXyz(
        filePath: filePath);
    refreshFromKernel();
    return ret;
  }
}
