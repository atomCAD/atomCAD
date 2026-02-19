import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Uint64List;
import 'package:vector_math/vector_math.dart' as vector_math;
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/edit_atom_api.dart'
    as edit_atom_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as structure_designer_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/facet_shell_api.dart'
    as facet_shell_api;
import 'package:flutter_cad/src/rust/api/structure_designer/import_xyz_api.dart'
    as import_xyz_api;
import 'package:flutter_cad/src/rust/api/structure_designer/import_api.dart'
    as import_api;
import 'package:flutter_cad/src/rust/api/structure_designer/atom_edit_api.dart'
    as atom_edit_api;
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

/// Callback signature for wire drop in empty space.
/// Called when a wire is dragged from a pin and dropped in empty space.
typedef WireDropCallback = void Function(
    PinReference startPin, Offset dropPosition);

/// Manages the entire node graph.
class StructureDesignerModel extends ChangeNotifier {
  List<APINetworkWithValidationErrors> nodeNetworkNames = [];
  NodeNetworkView? nodeNetworkView;
  APIEditAtomTool? activeEditAtomTool = APIEditAtomTool.default_;
  APIAtomEditTool? activeAtomEditTool = APIAtomEditTool.default_;
  DraggedWire? draggedWire; // not null if there is a wire dragging in progress
  WireDropCallback?
      onWireDroppedInEmptySpace; // Callback for wire drop in empty space
  String _lastMinimizeMessage = '';

  String get lastMinimizeMessage => _lastMinimizeMessage;
  APICameraCanonicalView cameraCanonicalView = APICameraCanonicalView.custom;
  bool isOrthographic = false;
  StructureDesignerPreferences? preferences;
  bool isDirty = false;
  String? filePath;

  StructureDesignerModel();

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
      rayStart: vector3ToApiVec3(rayStart),
      rayDir: vector3ToApiVec3(rayDir),
    );
    refreshFromKernel();
  }

  bool isNodeTypeActive(String nodeType) {
    return structure_designer_api.isNodeTypeActive(nodeType: nodeType);
  }

  List<APINodeCategoryView>? getCompatibleNodeTypes(
      String sourceType, bool draggingFromOutput) {
    return structure_designer_api.getCompatibleNodeTypes(
        sourceTypeStr: sourceType, draggingFromOutput: draggingFromOutput);
  }

  void setActiveEditAtomTool(APIEditAtomTool tool) {
    edit_atom_api.setActiveEditAtomTool(tool: tool);
    refreshFromKernel();
  }

  void selectAtomOrBondByRay(vector_math.Vector3 rayStart,
      vector_math.Vector3 rayDir, SelectModifier selectModifier) {
    edit_atom_api.selectAtomOrBondByRay(
      rayStart: vector3ToApiVec3(rayStart),
      rayDir: vector3ToApiVec3(rayDir),
      selectModifier: selectModifier,
    );
    refreshFromKernel();
  }

  void clearSelection() {
    structure_designer_api.clearSelection();
    refreshFromKernel();
  }

  // ===== MULTI-NODE SELECTION METHODS =====

  /// Toggle node in selection (for Ctrl+click)
  /// If selected, removes it; if not selected, adds it
  bool toggleNodeSelection(BigInt nodeId) {
    final result = structure_designer_api.toggleNodeSelection(nodeId: nodeId);
    refreshFromKernel();
    return result;
  }

  /// Add node to selection without clearing existing selection (for Shift+click)
  bool addNodeToSelection(BigInt nodeId) {
    final result = structure_designer_api.addNodeToSelection(nodeId: nodeId);
    refreshFromKernel();
    return result;
  }

  /// Select multiple nodes (for rectangle selection)
  bool selectNodes(List<BigInt> nodeIds) {
    final uint64Ids = Uint64List(nodeIds.length);
    for (int i = 0; i < nodeIds.length; i++) {
      uint64Ids[i] = nodeIds[i].toUnsigned(64);
    }
    final result = structure_designer_api.selectNodes(nodeIds: uint64Ids);
    refreshFromKernel();
    return result;
  }

  /// Toggle multiple nodes in selection (for Ctrl+rectangle)
  void toggleNodesSelection(List<BigInt> nodeIds) {
    final uint64Ids = Uint64List(nodeIds.length);
    for (int i = 0; i < nodeIds.length; i++) {
      uint64Ids[i] = nodeIds[i].toUnsigned(64);
    }
    structure_designer_api.toggleNodesSelection(nodeIds: uint64Ids);
    refreshFromKernel();
  }

  /// Get all selected node IDs
  List<BigInt> getSelectedNodeIds() {
    final ids = structure_designer_api.getSelectedNodeIds();
    // Uint64List already contains BigInt values
    return ids.toList();
  }

  /// Move all selected nodes by delta (commits position to kernel)
  void moveSelectedNodes(Offset delta) {
    structure_designer_api.moveSelectedNodes(
        deltaX: delta.dx, deltaY: delta.dy);
    refreshFromKernel();
  }

  /// Drag all selected nodes by delta (UI-only, does not commit to kernel)
  void dragSelectedNodes(Offset delta) {
    if (nodeNetworkView == null) return;
    final selectedIds = getSelectedNodeIds();
    for (final nodeId in selectedIds) {
      final node = nodeNetworkView!.nodes[nodeId];
      if (node != null) {
        node.position = APIVec2(
            x: node.position.x + delta.dx, y: node.position.y + delta.dy);
      }
    }
    notifyListeners();
  }

  /// Commit positions of all selected nodes to the kernel
  void updateSelectedNodesPosition() {
    if (nodeNetworkView == null) return;
    final selectedIds = getSelectedNodeIds();
    for (final nodeId in selectedIds) {
      final node = nodeNetworkView!.nodes[nodeId];
      if (node != null) {
        structure_designer_api.moveNode(
            nodeId: nodeId,
            position: APIVec2(x: node.position.x, y: node.position.y));
      }
    }
    refreshFromKernel();
  }

  // ===== MULTI-WIRE SELECTION METHODS =====

  /// Toggle wire in selection (for Ctrl+click)
  bool toggleWireSelection(BigInt sourceNodeId, int sourceOutputPinIndex,
      BigInt destNodeId, BigInt destParamIndex) {
    final result = structure_designer_api.toggleWireSelection(
      sourceNodeId: sourceNodeId,
      sourceOutputPinIndex: sourceOutputPinIndex,
      destinationNodeId: destNodeId,
      destinationArgumentIndex: destParamIndex,
    );
    refreshFromKernel();
    return result;
  }

  /// Add wire to selection without clearing existing selection (for Shift+click)
  bool addWireToSelection(BigInt sourceNodeId, int sourceOutputPinIndex,
      BigInt destNodeId, BigInt destParamIndex) {
    final result = structure_designer_api.addWireToSelection(
      sourceNodeId: sourceNodeId,
      sourceOutputPinIndex: sourceOutputPinIndex,
      destinationNodeId: destNodeId,
      destinationArgumentIndex: destParamIndex,
    );
    refreshFromKernel();
    return result;
  }

  // ===== BATCH SELECTION METHODS (for rectangle selection) =====

  /// Add multiple nodes to selection (for Shift+rectangle)
  void addNodesToSelection(List<BigInt> nodeIds) {
    final uint64Ids = Uint64List(nodeIds.length);
    for (int i = 0; i < nodeIds.length; i++) {
      uint64Ids[i] = nodeIds[i].toUnsigned(64);
    }
    structure_designer_api.addNodesToSelection(nodeIds: uint64Ids);
    refreshFromKernel();
  }

  /// Toggle multiple wires in selection (for Ctrl+rectangle)
  void toggleWiresSelection(List<WireView> wires) {
    final wireIdentifiers = wires
        .map((w) => WireIdentifier(
              sourceNodeId: w.sourceNodeId,
              sourceOutputPinIndex: w.sourceOutputPinIndex,
              destinationNodeId: w.destNodeId,
              destinationArgumentIndex: w.destParamIndex,
            ))
        .toList();
    structure_designer_api.toggleWiresSelection(wires: wireIdentifiers);
    refreshFromKernel();
  }

  /// Add multiple wires to selection (for Shift+rectangle)
  void addWiresToSelection(List<WireView> wires) {
    final wireIdentifiers = wires
        .map((w) => WireIdentifier(
              sourceNodeId: w.sourceNodeId,
              sourceOutputPinIndex: w.sourceOutputPinIndex,
              destinationNodeId: w.destNodeId,
              destinationArgumentIndex: w.destParamIndex,
            ))
        .toList();
    structure_designer_api.addWiresToSelection(wires: wireIdentifiers);
    refreshFromKernel();
  }

  /// Select nodes and wires together (for rectangle selection - replaces current selection)
  void selectNodesAndWires(List<BigInt> nodeIds, List<WireView> wires) {
    final uint64Ids = Uint64List(nodeIds.length);
    for (int i = 0; i < nodeIds.length; i++) {
      uint64Ids[i] = nodeIds[i].toUnsigned(64);
    }
    final wireIdentifiers = wires
        .map((w) => WireIdentifier(
              sourceNodeId: w.sourceNodeId,
              sourceOutputPinIndex: w.sourceOutputPinIndex,
              destinationNodeId: w.destNodeId,
              destinationArgumentIndex: w.destParamIndex,
            ))
        .toList();
    structure_designer_api.selectNodesAndWires(
        nodeIds: uint64Ids, wires: wireIdentifiers);
    refreshFromKernel();
  }

  /// Add nodes and wires to existing selection (for Shift+rectangle)
  void addNodesAndWiresToSelection(List<BigInt> nodeIds, List<WireView> wires) {
    final uint64Ids = Uint64List(nodeIds.length);
    for (int i = 0; i < nodeIds.length; i++) {
      uint64Ids[i] = nodeIds[i].toUnsigned(64);
    }
    final wireIdentifiers = wires
        .map((w) => WireIdentifier(
              sourceNodeId: w.sourceNodeId,
              sourceOutputPinIndex: w.sourceOutputPinIndex,
              destinationNodeId: w.destNodeId,
              destinationArgumentIndex: w.destParamIndex,
            ))
        .toList();
    structure_designer_api.addNodesAndWiresToSelection(
        nodeIds: uint64Ids, wires: wireIdentifiers);
    refreshFromKernel();
  }

  /// Toggle nodes and wires in selection (for Ctrl+rectangle)
  void toggleNodesAndWiresSelection(
      List<BigInt> nodeIds, List<WireView> wires) {
    final uint64Ids = Uint64List(nodeIds.length);
    for (int i = 0; i < nodeIds.length; i++) {
      uint64Ids[i] = nodeIds[i].toUnsigned(64);
    }
    final wireIdentifiers = wires
        .map((w) => WireIdentifier(
              sourceNodeId: w.sourceNodeId,
              sourceOutputPinIndex: w.sourceOutputPinIndex,
              destinationNodeId: w.destNodeId,
              destinationArgumentIndex: w.destParamIndex,
            ))
        .toList();
    structure_designer_api.toggleNodesAndWiresSelection(
        nodeIds: uint64Ids, wires: wireIdentifiers);
    refreshFromKernel();
  }

  /// Check if a node is the active node (for properties panel / gadget)
  BigInt? getActiveNodeId() {
    if (nodeNetworkView == null) return null;
    for (final entry in nodeNetworkView!.nodes.entries) {
      if (entry.value.active) {
        return entry.key;
      }
    }
    return null;
  }

  void newProject() {
    structure_designer_api.newProject();
    refreshFromKernel();
  }

  APIResult saveNodeNetworksAs(String filePath) {
    final result =
        structure_designer_api.saveNodeNetworksAs(filePath: filePath);
    refreshFromKernel();
    return result;
  }

  APIResult saveNodeNetworks() {
    final result = structure_designer_api.saveNodeNetworks();
    refreshFromKernel();
    return result;
  }

  /// Returns true if Save operation is available (design is dirty and has a file path)
  bool get canSave => isDirty && filePath != null;

  /// Returns true if Save As operation is available (always true)
  bool get canSaveAs => true;

  /// Returns the current file name for display in title bar
  String get displayFileName {
    if (filePath != null) {
      return filePath!
          .split('\\')
          .last
          .split('/')
          .last; // Handle both Windows and Unix paths
    }
    return 'Untitled';
  }

  /// Returns the window title with dirty indicator
  String get windowTitle {
    final fileName = displayFileName;
    return isDirty ? '$fileName*' : fileName;
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

  void validateActiveNetwork() {
    structure_designer_api.validateActiveNetwork();
    refreshFromKernel();
  }

  /// Apply auto-layout to the active node network.
  /// Uses the layout algorithm configured in preferences.
  void autoLayoutNetwork() {
    structure_designer_api.layoutActiveNetwork();
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

  /// Handles wire drop in empty space by invoking the callback.
  /// Called from PinWidget.onDragEnd when wire is dropped (not on a valid pin).
  void handleWireDropInEmptySpace(PinReference startPin, Offset dropPosition) {
    onWireDroppedInEmptySpace?.call(startPin, dropPosition);
  }

  bool canConnectPins(PinReference pin1, PinReference pin2) {
    if (pin1.pinType == pin2.pinType) {
      return false;
    }

    final outPin = pin1.pinType == PinType.output ? pin1 : pin2;
    final inPin = pin1.pinType == PinType.input ? pin1 : pin2;

    if (outPin.pinType != PinType.output || inPin.pinType != PinType.input) {
      return false;
    }

    if (inPin.pinIndex < 0) {
      return false;
    }

    return structure_designer_api.canConnectNodes(
      sourceNodeId: outPin.nodeId,
      sourceOutputPinIndex: outPin.pinIndex,
      destNodeId: inPin.nodeId,
      destParamIndex: BigInt.from(inPin.pinIndex),
    );
  }

  void connectPins(PinReference pin1, PinReference pin2) {
    if (pin1.pinType == pin2.pinType) {
      return;
    }

    final outPin = pin1.pinType == PinType.output ? pin1 : pin2;
    final inPin = pin1.pinType == PinType.input ? pin1 : pin2;

    if (outPin.pinType != PinType.output || inPin.pinType != PinType.input) {
      return;
    }

    if (inPin.pinIndex < 0) {
      return;
    }

    structure_designer_api.connectNodes(
      sourceNodeId: outPin.nodeId,
      sourceOutputPinIndex: outPin.pinIndex,
      destNodeId: inPin.nodeId,
      destParamIndex: BigInt.from(inPin.pinIndex),
    );

    draggedWire = null;

    refreshFromKernel();
  }

  /// Auto-connects a source pin to the first compatible pin on a target node.
  /// Used after creating a node from wire drop in empty space.
  bool autoConnectToNode(
    BigInt sourceNodeId,
    int sourcePinIndex,
    bool sourceIsOutput,
    BigInt targetNodeId,
  ) {
    final result = structure_designer_api.autoConnectToNode(
      sourceNodeId: sourceNodeId,
      sourcePinIndex: sourcePinIndex,
      sourceIsOutput: sourceIsOutput,
      targetNodeId: targetNodeId,
    );
    refreshFromKernel();
    return result;
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

  BigInt? getSelectedNodeId() {
    if (nodeNetworkView == null) return null;
    for (final node in nodeNetworkView!.nodes.values) {
      if (node.selected) {
        return node.id;
      }
    }
    return null;
  }

  void renameNodeNetwork(String oldName, String newName) {
    final success = structure_designer_api.renameNodeNetwork(
      oldName: oldName,
      newName: newName,
    );

    if (success) {
      // Always refresh the view - comment nodes in any network may reference
      // the renamed network via backticks and need to display updated text
      refreshFromKernel();
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

  /// Navigates back in node network history
  bool navigateBack() {
    final success = structure_designer_api.navigateBack();
    if (success) {
      refreshFromKernel();
    }
    return success;
  }

  /// Navigates forward in node network history
  bool navigateForward() {
    final success = structure_designer_api.navigateForward();
    if (success) {
      refreshFromKernel();
    }
    return success;
  }

  /// Checks if we can navigate backward in network history
  bool canNavigateBack() {
    return structure_designer_api.canNavigateBack();
  }

  /// Checks if we can navigate forward in network history
  bool canNavigateForward() {
    return structure_designer_api.canNavigateForward();
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

  // ===== COPY / PASTE / CUT =====

  /// Copies the current selection to the clipboard.
  /// Returns true if something was copied, false if selection was empty.
  bool copySelection() {
    return structure_designer_api.copySelection();
  }

  /// Pastes clipboard content at the given position (network coordinates).
  void pasteAtPosition(double x, double y) {
    structure_designer_api.pasteAtPosition(x: x, y: y);
    refreshFromKernel();
  }

  /// Cuts the current selection (copy + delete).
  /// Returns true if something was cut.
  bool cutSelection() {
    final result = structure_designer_api.cutSelection();
    if (result) {
      refreshFromKernel();
    }
    return result;
  }

  /// Returns true if the clipboard has content available for pasting.
  bool hasClipboardContent() {
    return structure_designer_api.hasClipboardContent();
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

  // ===== ATOM_EDIT (NEW DIFF-BASED NODE) METHODS =====

  void setActiveAtomEditTool(APIAtomEditTool tool) {
    atom_edit_api.setActiveAtomEditTool(tool: tool);
    refreshFromKernel();
  }

  void atomEditSelectByRay(vector_math.Vector3 rayStart,
      vector_math.Vector3 rayDir, SelectModifier selectModifier) {
    atom_edit_api.atomEditSelectByRay(
      rayStart: vector3ToApiVec3(rayStart),
      rayDir: vector3ToApiVec3(rayDir),
      selectModifier: selectModifier,
    );
    refreshFromKernel();
  }

  void atomEditAddAtomByRay(int atomicNumber, vector_math.Vector3 planeNormal,
      vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    if (nodeNetworkView == null) return;
    atom_edit_api.atomEditAddAtomByRay(
      atomicNumber: atomicNumber,
      planeNormal: vector3ToApiVec3(planeNormal),
      rayStart: vector3ToApiVec3(rayStart),
      rayDir: vector3ToApiVec3(rayDir),
    );
    refreshFromKernel();
  }

  void atomEditDrawBondByRay(
      vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    if (nodeNetworkView == null) return;
    atom_edit_api.atomEditDrawBondByRay(
      rayStart: vector3ToApiVec3(rayStart),
      rayDir: vector3ToApiVec3(rayDir),
    );
    refreshFromKernel();
  }

  void atomEditDeleteSelected() {
    if (nodeNetworkView == null) return;
    atom_edit_api.atomEditDeleteSelected();
    refreshFromKernel();
  }

  void atomEditReplaceSelected(int atomicNumber) {
    if (nodeNetworkView == null) return;
    atom_edit_api.atomEditReplaceSelected(atomicNumber: atomicNumber);
    refreshFromKernel();
  }

  void atomEditTransformSelected(APITransform absTransform) {
    if (nodeNetworkView == null) return;
    atom_edit_api.atomEditTransformSelected(absTransform: absTransform);
    refreshFromKernel();
  }

  void toggleAtomEditOutputDiff() {
    atom_edit_api.atomEditToggleOutputDiff();
    refreshFromKernel();
  }

  void toggleAtomEditShowAnchorArrows() {
    atom_edit_api.atomEditToggleShowAnchorArrows();
    refreshFromKernel();
  }

  void toggleAtomEditIncludeBaseBondsInDiff() {
    atom_edit_api.atomEditToggleIncludeBaseBondsInDiff();
    refreshFromKernel();
  }

  void toggleAtomEditShowGadget() {
    atom_edit_api.atomEditToggleShowGadget();
    refreshFromKernel();
  }

  bool setAtomEditDefaultData(int replacementAtomicNumber) {
    if (nodeNetworkView == null) return false;
    final result = atom_edit_api.setAtomEditDefaultData(
        replacementAtomicNumber: replacementAtomicNumber);
    refreshFromKernel();
    return result;
  }

  bool setAtomEditAddAtomData(int atomicNumber) {
    if (nodeNetworkView == null) return false;
    final result =
        atom_edit_api.setAtomEditAddAtomData(atomicNumber: atomicNumber);
    refreshFromKernel();
    return result;
  }

  void atomEditMinimize(APIMinimizeFreezeMode freezeMode) {
    _lastMinimizeMessage =
        atom_edit_api.atomEditMinimize(freezeMode: freezeMode);
    refreshFromKernel();
    notifyListeners();
  }

  void addAtomByRay(int atomicNumber, vector_math.Vector3 planeNormal,
      vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    if (nodeNetworkView == null) return;
    edit_atom_api.addAtomByRay(
      atomicNumber: atomicNumber,
      planeNormal: vector3ToApiVec3(planeNormal),
      rayStart: vector3ToApiVec3(rayStart),
      rayDir: vector3ToApiVec3(rayDir),
    );
    refreshFromKernel();
  }

  void drawBondByRay(vector_math.Vector3 rayStart, vector_math.Vector3 rayDir) {
    if (nodeNetworkView == null) return;
    edit_atom_api.drawBondByRay(
      rayStart: vector3ToApiVec3(rayStart),
      rayDir: vector3ToApiVec3(rayDir),
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
    if (newNodeId != BigInt.zero) {
      structure_designer_api.selectNode(nodeId: newNodeId);
    }
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

  void setDrawingPlaneData(BigInt nodeId, APIDrawingPlaneData data) {
    structure_designer_api.setDrawingPlaneData(nodeId: nodeId, data: data);
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

  APILatticeMoveData? getLatticeMoveData(BigInt nodeId) {
    return structure_designer_api.getLatticeMoveData(nodeId: nodeId);
  }

  void setLatticeMoveData(BigInt nodeId, APILatticeMoveData data) {
    structure_designer_api.setLatticeMoveData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  APILatticeRotData? getLatticeRotData(BigInt nodeId) {
    return structure_designer_api.getLatticeRotData(nodeId: nodeId);
  }

  void setLatticeRotData(BigInt nodeId, APILatticeRotData data) {
    structure_designer_api.setLatticeRotData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setAtomMoveData(BigInt nodeId, APIAtomMoveData data) {
    structure_designer_api.setAtomMoveData(nodeId: nodeId, data: data);
    refreshFromKernel();
  }

  void setAtomRotData(BigInt nodeId, APIAtomRotData data) {
    structure_designer_api.setAtomRotData(nodeId: nodeId, data: data);
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
    activeAtomEditTool = atom_edit_api.getActiveAtomEditTool();
    cameraCanonicalView = common_api.getCameraCanonicalView();
    isOrthographic = common_api.isOrthographic();
    preferences = structure_designer_api.getStructureDesignerPreferences();
    isDirty = structure_designer_api.isDesignDirty();
    filePath = structure_designer_api.getDesignFilePath();

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

  /// Exports all visible atomic structures as a single file (XYZ or MOL format)
  /// File format is determined by the file extension (.xyz or .mol)
  APIResult exportVisibleAtomicStructures(String filePath) {
    final ret = structure_designer_api.exportVisibleAtomicStructures(
        filePath: filePath);
    refreshFromKernel();
    return ret;
  }

  /// Imports selected node networks from a .cnnd library file
  ///
  /// This method handles the complete import process:
  /// 1. Loads the library file
  /// 2. Imports the selected networks with optional name prefix
  /// 3. Refreshes the UI to show the newly imported networks
  ///
  /// Returns APIResult indicating success or failure
  APIResult importFromCnndLibrary(
      String libraryFilePath, List<String> networkNames, String? namePrefix) {
    try {
      // Load the library file
      final loadResult =
          import_api.loadImportLibrary(filePath: libraryFilePath);
      if (!loadResult.success) {
        return loadResult;
      }

      // Import the selected networks
      final importResult = import_api.importNetworksAndClear(
        networkNames: networkNames,
        namePrefix: namePrefix,
      );

      // Refresh the UI regardless of import result
      refreshFromKernel();

      return importResult;
    } catch (e) {
      // Ensure UI is refreshed even on error
      refreshFromKernel();
      return APIResult(
        success: false,
        errorMessage: 'Import failed: $e',
      );
    }
  }
}
