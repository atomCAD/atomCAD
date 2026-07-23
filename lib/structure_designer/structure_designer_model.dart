import 'package:flutter/foundation.dart' show listEquals;
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
import 'package:flutter_cad/src/rust/api/structure_designer/import_cif_api.dart'
    as import_cif_api;
import 'package:flutter_cad/src/rust/api/structure_designer/import_api.dart'
    as import_api;
import 'package:flutter_cad/src/rust/api/structure_designer/atom_edit_api.dart'
    as atom_edit_api;
import 'package:flutter_cad/src/rust/api/structure_designer/relax_api.dart'
    as relax_api;
import 'package:flutter_cad/src/rust/api/structure_designer/xray_api.dart'
    as xray_api;
import 'package:flutter_cad/src/rust/api/structure_designer/tag_api.dart'
    as tag_api;
import 'package:flutter_cad/src/rust/api/common_api.dart' as common_api;

/// Distinguishes the five kinds of pin slots a node can expose. Replaces the
/// legacy `(PinType, pinIndex == -1)` discriminator pair that today
/// distinguishes title-bar function pins from regular outputs, and accommodates
/// the inner-edge zone pins on HOF nodes.
///
/// [functionPin] is a transitional value — it goes away when the parent Rust
/// design's `DataType::Function` / `Closure` cleanup lands (see
/// `doc/design_zones_ui.md` §"U7 Polish").
///
/// Zone pins live on the inner edges of an HOF (zone-owning) node. From the
/// body's perspective, [zoneInput] pins are sources (carry iteration values
/// into the body) and [zoneOutput] pins are destinations (consume the body's
/// per-iteration result). They face inward — wires that touch them live in
/// the body's scope, not the HOF's containing network. See
/// `doc/design_zones_ui.md` §"Pin sets".
enum PinKind {
  /// Left-edge input pin. Consumes a wire.
  externalInput,

  /// Right-edge output pin. Produces a wire.
  externalOutput,

  /// Title-bar function pin (legacy `pinIndex == -1` output). Produces a wire.
  functionPin,

  /// Inner-left pin on an HOF node, facing into its body. From the body's
  /// perspective, it's a source: wires leave the pin and flow into body nodes.
  zoneInput,

  /// Inner-right pin on an HOF node, facing into its body. From the body's
  /// perspective, it's a destination: wires from body nodes terminate here.
  zoneOutput,
}

class PinReference {
  BigInt nodeId;

  /// Scope of the pin's owner node — empty for top-level. Always `const []`
  /// in phase U1 (no inline-zone authoring yet) but plumbed through every
  /// pin-handling code path so later phases can carry depth without touching
  /// every call site again.
  List<BigInt> scopeChain;
  PinKind pinKind;
  int pinIndex;
  String dataType;

  PinReference({
    required this.nodeId,
    this.scopeChain = const [],
    required this.pinKind,
    required this.pinIndex,
    required this.dataType,
  });

  /// True if this pin is a wire source (produces a value). Includes the
  /// inside-facing [PinKind.zoneInput] pin on an HOF, which sources iteration
  /// values into the body.
  bool get isOutput =>
      pinKind == PinKind.externalOutput ||
      pinKind == PinKind.functionPin ||
      pinKind == PinKind.zoneInput;

  /// True if this pin is a wire destination (consumes a value). Includes the
  /// inside-facing [PinKind.zoneOutput] pin on an HOF, which receives the
  /// body's per-iteration result.
  bool get isInput =>
      pinKind == PinKind.externalInput || pinKind == PinKind.zoneOutput;

  @override
  String toString() {
    return 'PinReference(nodeId: $nodeId, scopeChain: $scopeChain, pinKind: $pinKind, pinIndex: $pinIndex, dataType: $dataType)';
  }

  @override
  bool operator ==(Object other) {
    if (identical(this, other)) return true;
    if (other is! PinReference) return false;
    return nodeId == other.nodeId &&
        listEquals(scopeChain, other.scopeChain) &&
        pinKind == other.pinKind &&
        pinIndex == other.pinIndex &&
        dataType == other.dataType;
  }

  @override
  int get hashCode => Object.hash(
      nodeId, Object.hashAll(scopeChain), pinKind, pinIndex, dataType);
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

/// Convert a Dart-side scope chain (`List<BigInt>`, the type carried by
/// [PinReference.scopeChain] and the model's `activeScopeChain`) into the
/// `Uint64List` form expected by the FRB-generated Rust API. Empty input
/// (the common case throughout phase U2) yields a length-0 `Uint64List`.
Uint64List scopeChainToBytes(List<BigInt> scopeChain) {
  if (scopeChain.isEmpty) return Uint64List(0);
  final out = Uint64List(scopeChain.length);
  for (int i = 0; i < scopeChain.length; i++) {
    out[i] = scopeChain[i].toUnsigned(64);
  }
  return out;
}

/// Manages the entire node graph.
class StructureDesignerModel extends ChangeNotifier {
  List<APINetworkWithValidationErrors> nodeNetworkNames = [];

  /// Names of every record type def in the project, sorted alphabetically.
  /// Mirrors `getRecordTypeDefNames()` and is refreshed from the kernel
  /// alongside `nodeNetworkNames`.
  List<String> recordTypeDefNames = [];

  /// How many instance nodes reference each node type, across every network in
  /// the design (Find Usages, issue #414). Refreshed from the kernel alongside
  /// `nodeNetworkNames` by one batched walk, so the user-types panel can show a
  /// per-row count without issuing an FFI call per row.
  ///
  /// Keyed by *node type name*, which includes built-in types — always look a
  /// network up by name rather than iterating the map.
  Map<String, int> networkUsageCounts = {};

  /// Deliberately-created empty-folder paths (sorted). The tree view merges
  /// these with the folders implied by entity names. Refreshed from the kernel
  /// alongside `nodeNetworkNames`. See `doc/design_empty_folders.md`.
  List<String> folderNames = [];

  /// Name of the record type def currently being edited in the main content
  /// area's bottom panel. When non-null, the schema editor replaces the
  /// network editor; the active node network (for the 3D viewport) is
  /// unchanged. `null` means the network editor is shown.
  String? activeRecordDefName;
  NodeNetworkView? nodeNetworkView;
  APIEditAtomTool? activeEditAtomTool = APIEditAtomTool.default_;
  APIAtomEditTool? activeAtomEditTool = APIAtomEditTool.default_;
  int? atomEditSelectedElement;
  DraggedWire? draggedWire; // not null if there is a wire dragging in progress
  WireDropCallback?
      onWireDroppedInEmptySpace; // Callback for wire drop in empty space
  /// Callback to scroll the node network panel to a specific node.
  /// Registered by NodeNetworkState during init.
  ///
  /// [scopeChain] addresses a node inside an HOF / closure body (empty = the
  /// top-level network). [screenAnchor] is the point — in the node-network
  /// widget's local screen coordinates — that the target node's *center*
  /// should land on; `null` centers it in the viewport, which is the
  /// pre-Find-Usages behavior every other caller relies on.
  void Function(BigInt nodeId, {List<BigInt> scopeChain, Offset? screenAnchor})?
      onScrollToNode;
  bool directEditingMode = true;
  String _lastMinimizeMessage = '';
  String _lastAddHydrogenMessage = '';
  APIBondLengthMode _bondLengthMode = APIBondLengthMode.crystal;
  APIHybridization _hybridizationOverride = APIHybridization.auto;
  APIBondMode _bondMode = APIBondMode.covalent;

  String get lastMinimizeMessage => _lastMinimizeMessage;
  String get lastAddHydrogenMessage => _lastAddHydrogenMessage;
  APIBondLengthMode get bondLengthMode => _bondLengthMode;
  set bondLengthMode(APIBondLengthMode value) {
    _bondLengthMode = value;
    notifyListeners();
  }

  APIHybridization get hybridizationOverride => _hybridizationOverride;
  set hybridizationOverride(APIHybridization value) {
    _hybridizationOverride = value;
    notifyListeners();
  }

  APIBondMode get bondMode => _bondMode;
  set bondMode(APIBondMode value) {
    _bondMode = value;
    notifyListeners();
  }

  APICameraCanonicalView cameraCanonicalView = APICameraCanonicalView.custom;
  bool isOrthographic = false;

  /// Navigation-up-axis state (issue #349) for the camera-row indicator and the
  /// view-up dialog. Mirrored from the kernel each refresh. Null before the
  /// first refresh only.
  APIViewUpInfo? viewUpInfo;
  StructureDesignerPreferences? preferences;
  bool isDirty = false;
  String? filePath;

  /// Accumulated `print` node entries. Polled from the Rust per-CAD-instance
  /// buffer via `takePrintLog()` after each `refreshFromKernel`. Drives the
  /// bottom Console panel. Newest entries are appended at the end.
  /// See `doc/design_node_execution.md` (Phase 4 — Console panel).
  final List<APIPrintLogEntry> printLog = [];

  /// Number of print-log entries that have arrived since the Console panel
  /// was last opened. Drives the toolbar toggle's "new entries" dot. The
  /// Console panel resets this to 0 when it becomes visible.
  int unreadPrintLogCount = 0;

  /// Whether the Console panel is currently visible (docked at bottom). The
  /// toolbar toggle / `Ctrl+`` ` keyboard shortcut flips this.
  bool consolePanelVisible = false;

  /// Chain of HOF node IDs identifying the body that keyboard shortcuts
  /// (Delete, Ctrl+C/V/X/D, etc.) operate on. Empty means the active
  /// top-level network. Defaulted to `const []` until U4 introduces a click-
  /// into-body gesture that flips the active body. See
  /// `doc/design_zones_ui.md` §"Phase U2".
  List<BigInt> activeScopeChain = const [];

  /// Scope chain of the node currently shown in the property panel. Set by
  /// `NodeDataWidget` from the *selected* node's resolved scope, which can
  /// differ from [activeScopeChain] (e.g. clicking a body interior changes the
  /// active scope without changing the selection). Property getters/setters key
  /// off this so they address the right node even when a body node shares a
  /// numeric id with a top-level node.
  List<BigInt> propertyEditorScopeChain = const [];

  /// The property-editor scope as the `Uint64List` the Rust API expects.
  Uint64List get propertyEditorScopePath =>
      scopeChainToBytes(propertyEditorScopeChain);

  StructureDesignerModel();

  void init() {
    refreshFromKernel();
  }

  /// Sets the active body for keyboard shortcuts (Delete, Ctrl+C/V/D, …).
  /// Replaces only if changed to avoid spurious notifies. See
  /// `doc/design_zones_ui.md` §"The active body".
  void setActiveScopeChain(List<BigInt> scopeChain) {
    if (listEquals(activeScopeChain, scopeChain)) return;
    activeScopeChain = List<BigInt>.from(scopeChain);
    notifyListeners();
  }

  /// Update the stored body size of the HOF identified by ([scopeChain],
  /// [hofNodeId]). The renderer uses `max(stored, content_bbox + padding)`
  /// so the body can never shrink below its content; the Rust API also
  /// clamps to a minimum. See `doc/design_zones_ui.md` §"Body sizing".
  void setZoneSize(
      List<BigInt> scopeChain, BigInt hofNodeId, double width, double height) {
    structure_designer_api.setZoneSize(
      scopePath: scopeChainToBytes(scopeChain),
      hofNodeId: hofNodeId,
      width: width,
      height: height,
    );
    refreshFromKernel();
  }

  /// Called when an HOF body resize drag begins. Captures the body's pre-drag
  /// dimensions so the matching [endZoneResize] records one coalesced undo
  /// command (mirrors [beginMoveNodes] / comment-node resize). No refresh — the
  /// per-frame [setZoneSize] calls drive the live update.
  void beginZoneResize(List<BigInt> scopeChain, BigInt hofNodeId) {
    structure_designer_api.beginZoneResize(
      scopePath: scopeChainToBytes(scopeChain),
      hofNodeId: hofNodeId,
    );
  }

  /// Called when an HOF body resize drag ends. Pushes a single
  /// `SetZoneSizeCommand` if the body changed size.
  void endZoneResize() {
    structure_designer_api.endZoneResize();
  }

  /// Set a collapsable HOF node's collapse mode (Auto / Collapsed / Expanded).
  /// Forwards [scopeChain] as `scope_path` so the (possibly nested) body's HOF
  /// is resolved. No-op Rust-side for non-collapsable nodes. See
  /// `doc/design_hof_node_collapse.md`.
  void setCollapseMode(
      List<BigInt> scopeChain, BigInt hofNodeId, APICollapseMode mode) {
    structure_designer_api.setCollapseMode(
      scopePath: scopeChainToBytes(scopeChain),
      hofNodeId: hofNodeId,
      mode: mode,
    );
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

  // View-up axis (issue #349). Camera is global to the active network, so these
  // take no scope_path (like the other camera methods above). Each returns the
  // kernel's error string (`null` on success) so the dialog can surface it
  // inline; the refresh mirrors the new `viewUpInfo` and re-renders the
  // viewport with the re-aligned turntable.
  String? setViewUpFromMillerPlane(APIIVec3 hkl) {
    final error = common_api.setViewUpFromMillerPlane(hkl: hkl);
    refreshFromKernel();
    return error;
  }

  String? setViewUpFromLatticeDirection(APIIVec3 uvw) {
    final error = common_api.setViewUpFromLatticeDirection(uvw: uvw);
    refreshFromKernel();
    return error;
  }

  String? setViewUpFromActiveDrawingPlane() {
    final error = common_api.setViewUpFromActiveDrawingPlane();
    refreshFromKernel();
    return error;
  }

  void resetViewUp() {
    common_api.resetViewUp();
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

  /// Returns true if the active interactive node is atom_edit or motif_edit
  /// (both share AtomEditData via dual-registration).
  bool get isAtomEditLikeActive =>
      isNodeTypeActive('atom_edit') || isNodeTypeActive('motif_edit');

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

  void clearSelection({List<BigInt> scopeChain = const []}) {
    structure_designer_api.clearSelection(
      scopePath: scopeChainToBytes(scopeChain),
    );
    refreshFromKernel();
  }

  /// Clear the selection (and active node) at every scope reachable from the
  /// top-level network. Used when the user clicks on empty top-level space —
  /// every body's active node should be cleared too so it no longer renders
  /// active. Walks the tree once Rust-side via `clear_selection_all_scopes`.
  void clearSelectionAllScopes() {
    structure_designer_api.clearSelectionAllScopes();
    refreshFromKernel();
  }

  // ===== MULTI-NODE SELECTION METHODS =====

  /// Toggle node in selection (for Ctrl+click)
  /// If selected, removes it; if not selected, adds it
  bool toggleNodeSelection(BigInt nodeId,
      {List<BigInt> scopeChain = const []}) {
    final result = structure_designer_api.toggleNodeSelection(
      scopePath: scopeChainToBytes(scopeChain),
      nodeId: nodeId,
    );
    refreshFromKernel();
    return result;
  }

  /// Add node to selection without clearing existing selection (for Shift+click)
  bool addNodeToSelection(BigInt nodeId, {List<BigInt> scopeChain = const []}) {
    final result = structure_designer_api.addNodeToSelection(
      scopePath: scopeChainToBytes(scopeChain),
      nodeId: nodeId,
    );
    refreshFromKernel();
    return result;
  }

  /// Select multiple nodes (for rectangle selection)
  bool selectNodes(List<BigInt> nodeIds, {List<BigInt> scopeChain = const []}) {
    final uint64Ids = Uint64List(nodeIds.length);
    for (int i = 0; i < nodeIds.length; i++) {
      uint64Ids[i] = nodeIds[i].toUnsigned(64);
    }
    final result = structure_designer_api.selectNodes(
      scopePath: scopeChainToBytes(scopeChain),
      nodeIds: uint64Ids,
    );
    refreshFromKernel();
    return result;
  }

  /// Toggle multiple nodes in selection (for Ctrl+rectangle)
  void toggleNodesSelection(List<BigInt> nodeIds,
      {List<BigInt> scopeChain = const []}) {
    final uint64Ids = Uint64List(nodeIds.length);
    for (int i = 0; i < nodeIds.length; i++) {
      uint64Ids[i] = nodeIds[i].toUnsigned(64);
    }
    structure_designer_api.toggleNodesSelection(
      scopePath: scopeChainToBytes(scopeChain),
      nodeIds: uint64Ids,
    );
    refreshFromKernel();
  }

  /// Get all selected node IDs
  List<BigInt> getSelectedNodeIds() {
    final ids = structure_designer_api.getSelectedNodeIds();
    // Uint64List already contains BigInt values
    return ids.toList();
  }

  /// Move all selected nodes by delta (commits position to kernel)
  void moveSelectedNodes(Offset delta, {List<BigInt> scopeChain = const []}) {
    structure_designer_api.moveSelectedNodes(
      scopePath: scopeChainToBytes(scopeChain),
      deltaX: delta.dx,
      deltaY: delta.dy,
    );
    refreshFromKernel();
  }

  /// Logical-pixel floor for a body node's top-left inside a zone body. A node
  /// inside an HOF/closure body lives in body-local coordinates whose interior
  /// origin is `(0, 0)`; the body grows to fit content on the right/down but
  /// has no leftward/upward growth, so a node dragged past `(0, 0)` escapes the
  /// visible body rect. Clamping the drag to this inset keeps body nodes inside
  /// the rect (and clears the zone-input pin column on the left edge).
  static const double _ZONE_BODY_DRAG_INSET = 8.0;

  /// Clamps a drag [delta] so no node in [movedNodes] is pushed left/up past the
  /// body-interior floor ([_ZONE_BODY_DRAG_INSET]), keeping a multi-node
  /// selection rigid (the shared delta is clamped, not each node). Returns
  /// [delta] unchanged at the top level (empty [scopeChain]) or when there is
  /// nothing to move. Only blocks moving *further* past the floor — an
  /// already-escaped node (e.g. from an older file) can still be dragged back
  /// inward without snapping.
  Offset _clampZoneBodyDragDelta(
      Offset delta, Iterable<NodeView> movedNodes, List<BigInt> scopeChain) {
    if (scopeChain.isEmpty) return delta;
    double? minX, minY;
    for (final node in movedNodes) {
      if (minX == null || node.position.x < minX) minX = node.position.x;
      if (minY == null || node.position.y < minY) minY = node.position.y;
    }
    if (minX == null || minY == null) return delta;
    // Lower bound on the delta: how far left/up the group may still move before
    // the leftmost/topmost node hits the floor. min(0, ...) so an already-escaped
    // node yields a bound of 0 (no further-left motion, but rightward passes).
    final slackX = _ZONE_BODY_DRAG_INSET - minX;
    final slackY = _ZONE_BODY_DRAG_INSET - minY;
    final lowerX = slackX < 0 ? slackX : 0.0;
    final lowerY = slackY < 0 ? slackY : 0.0;
    return Offset(
      delta.dx < lowerX ? lowerX : delta.dx,
      delta.dy < lowerY ? lowerY : delta.dy,
    );
  }

  /// Drag all selected nodes by delta (UI-only, does not commit to kernel).
  /// Mutates positions in the body scope identified by [scopeChain]. Inside a
  /// zone body the delta is clamped so the selection can't escape the body rect
  /// on the left/top (see [_clampZoneBodyDragDelta]).
  void dragSelectedNodes(Offset delta, {List<BigInt> scopeChain = const []}) {
    if (nodeNetworkView == null) return;
    final containerNodes = _nodesAtScope(scopeChain);
    if (containerNodes == null) return;
    final selected = containerNodes.values.where((node) => node.selected);
    final clamped = _clampZoneBodyDragDelta(delta, selected, scopeChain);
    for (final node in selected) {
      node.position = APIVec2(
          x: node.position.x + clamped.dx, y: node.position.y + clamped.dy);
    }
    notifyListeners();
  }

  /// Commit positions of all selected nodes in [scopeChain] to the kernel.
  void updateSelectedNodesPosition({List<BigInt> scopeChain = const []}) {
    if (nodeNetworkView == null) return;
    final containerNodes = _nodesAtScope(scopeChain);
    if (containerNodes == null) return;
    final scopePath = scopeChainToBytes(scopeChain);
    for (final node in containerNodes.values) {
      if (node.selected) {
        structure_designer_api.moveNode(
            scopePath: scopePath,
            nodeId: node.id,
            position: APIVec2(x: node.position.x, y: node.position.y));
      }
    }
    refreshFromKernel();
  }

  /// Returns the node map at [scopeChain] (top-level if empty), or null if
  /// the chain can't be walked.
  Map<BigInt, NodeView>? _nodesAtScope(List<BigInt> scopeChain) {
    if (nodeNetworkView == null) return null;
    Map<BigInt, NodeView> currentNodes = nodeNetworkView!.nodes;
    for (final hofId in scopeChain) {
      final hof = currentNodes[hofId];
      final zone = hof?.zone;
      if (zone == null) return null;
      currentNodes = zone.nodes;
    }
    return currentNodes;
  }

  // ===== MULTI-WIRE SELECTION METHODS =====

  /// Toggle wire in selection (for Ctrl+click). [scopeChain] is the scope the
  /// wire lives in (empty = top-level); see the single-scope selection
  /// invariant in `structure_designer.rs::clear_selection_in_other_scopes`.
  bool toggleWireSelection(BigInt sourceNodeId, int sourceOutputPinIndex,
      BigInt destNodeId, BigInt destParamIndex,
      {List<BigInt> scopeChain = const []}) {
    final result = structure_designer_api.toggleWireSelection(
      scopePath: scopeChainToBytes(scopeChain),
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
      BigInt destNodeId, BigInt destParamIndex,
      {List<BigInt> scopeChain = const []}) {
    final result = structure_designer_api.addWireToSelection(
      scopePath: scopeChainToBytes(scopeChain),
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
  void addNodesToSelection(List<BigInt> nodeIds,
      {List<BigInt> scopeChain = const []}) {
    final uint64Ids = Uint64List(nodeIds.length);
    for (int i = 0; i < nodeIds.length; i++) {
      uint64Ids[i] = nodeIds[i].toUnsigned(64);
    }
    structure_designer_api.addNodesToSelection(
      scopePath: scopeChainToBytes(scopeChain),
      nodeIds: uint64Ids,
    );
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
  void selectNodesAndWires(List<BigInt> nodeIds, List<WireView> wires,
      {List<BigInt> scopeChain = const []}) {
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
        scopePath: scopeChainToBytes(scopeChain),
        nodeIds: uint64Ids,
        wires: wireIdentifiers);
    refreshFromKernel();
  }

  /// Add nodes and wires to existing selection (for Shift+rectangle)
  void addNodesAndWiresToSelection(List<BigInt> nodeIds, List<WireView> wires,
      {List<BigInt> scopeChain = const []}) {
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
        scopePath: scopeChainToBytes(scopeChain),
        nodeIds: uint64Ids,
        wires: wireIdentifiers);
    refreshFromKernel();
  }

  /// Toggle nodes and wires in selection (for Ctrl+rectangle)
  void toggleNodesAndWiresSelection(List<BigInt> nodeIds, List<WireView> wires,
      {List<BigInt> scopeChain = const []}) {
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
        scopePath: scopeChainToBytes(scopeChain),
        nodeIds: uint64Ids,
        wires: wireIdentifiers);
    refreshFromKernel();
  }

  /// Returns the active node's id anywhere in the scope tree, preferring
  /// nodes inside the [activeScopeChain] body so the property panel follows
  /// the user's most recent body click. Falls back to the top-level network.
  BigInt? getActiveNodeId() {
    if (nodeNetworkView == null) return null;
    // Prefer the active scope: if the user just clicked into a body, that
    // body's active node drives the property panel.
    if (activeScopeChain.isNotEmpty) {
      final activeBodyNodes = _nodesAtScope(activeScopeChain);
      if (activeBodyNodes != null) {
        for (final entry in activeBodyNodes.entries) {
          if (entry.value.active) return entry.key;
        }
      }
    }
    // Fall back to whichever node is active anywhere in the tree. Walks
    // body content recursively so clicking back into top-level (which sets
    // activeScopeChain = []) still finds a deeper body's active node if
    // none is active at the top level.
    return _findActiveNodeIdRecursive(nodeNetworkView!.nodes);
  }

  BigInt? _findActiveNodeIdRecursive(Map<BigInt, NodeView> nodes) {
    for (final entry in nodes.entries) {
      if (entry.value.active) return entry.key;
    }
    for (final entry in nodes.entries) {
      final zone = entry.value.zone;
      if (zone == null) continue;
      final inner = _findActiveNodeIdRecursive(zone.nodes);
      if (inner != null) return inner;
    }
    return null;
  }

  void newProject() {
    if (directEditingMode) {
      structure_designer_api.newProjectDirectEditing();
    } else {
      structure_designer_api.newProject();
    }
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

  /// Parameter-id repairs performed during the most recent successful load
  /// (F6 of `doc/design_parameter_wire_stability.md`). Empty when none were
  /// needed. Read by the file-open handlers to show a one-time "auto-repaired"
  /// modal.
  List<String> lastLoadParamIdRepairs = [];

  APIResult loadNodeNetworks(String filePath) {
    final result = structure_designer_api.loadNodeNetworks(filePath: filePath);
    lastLoadParamIdRepairs =
        result.success ? structure_designer_api.takeLoadParamIdRepairs() : [];
    refreshFromKernel();
    return result;
  }

  void setActiveNodeNetwork(String nodeNetworkName) {
    // The backend clears its own active record def when a network is
    // activated (§8); `refreshFromKernel` mirrors that back.
    structure_designer_api.setActiveNodeNetwork(
        nodeNetworkName: nodeNetworkName);
    refreshFromKernel();
  }

  /// Switch the main content area's bottom panel to the schema editor for
  /// `name`. Pass `null` to clear and fall back to the network editor. The
  /// active network (and viewport) is untouched. The active record def is
  /// backend-owned (§8), so this writes it through to the kernel and mirrors
  /// it back on refresh.
  void setActiveRecordDef(String? name) {
    if (activeRecordDefName == name) return;
    structure_designer_api.setActiveRecordDefName(name: name);
    refreshFromKernel();
  }

  /// Adds a new record type def with the given name and an empty field list.
  /// On success, activates the new def in the schema editor.
  /// Returns null on success, or an error message.
  String? addRecordTypeDef(String name) {
    final result = structure_designer_api.addRecordTypeDef(name: name);
    if (result.success) {
      // Open the new def in the schema editor. The active record def is
      // backend-owned (§8), so write it through; `refreshFromKernel` mirrors.
      structure_designer_api.setActiveRecordDefName(name: name);
      refreshFromKernel();
      return null;
    }
    return result.errorMessage;
  }

  /// Deletes the record type def with the given name. Returns null on
  /// success, or an error message. The backend clears its own active record
  /// def when the active one is deleted (§8); we just mirror on refresh.
  String? deleteRecordTypeDef(String name) {
    final result = structure_designer_api.deleteRecordTypeDef(name: name);
    if (result.success) {
      refreshFromKernel();
      return null;
    }
    return result.errorMessage;
  }

  /// Renames a record type def. Returns null on success, or an error message.
  /// The backend remaps its own active record def across the rename (§8); we
  /// just mirror on refresh.
  String? renameRecordTypeDef(String oldName, String newName) {
    final result = structure_designer_api.renameRecordTypeDef(
        oldName: oldName, newName: newName);
    if (result.success) {
      refreshFromKernel();
      return null;
    }
    return result.errorMessage;
  }

  /// Replaces the field list of an existing record type def. Returns null on
  /// success, or an error message (e.g. a cycle would be introduced).
  String? updateRecordTypeDef(String name, List<APIRecordTypeField> fields) {
    final result =
        structure_designer_api.updateRecordTypeDef(name: name, fields: fields);
    if (result.success) {
      refreshFromKernel();
      return null;
    }
    return result.errorMessage;
  }

  /// Returns the full record type def (name + fields) for `name`, or null.
  APIRecordTypeDef? getRecordTypeDef(String name) {
    return structure_designer_api.getRecordTypeDef(name: name);
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

  // ===== UNDO/REDO =====

  /// Whether there are commands that can be undone.
  bool get canUndo => structure_designer_api.canUndo();

  /// Whether there are commands that can be redone.
  bool get canRedo => structure_designer_api.canRedo();

  /// Description of the command that would be undone, or null.
  String? get undoDescription => structure_designer_api.undoDescription();

  /// Description of the command that would be redone, or null.
  String? get redoDescription => structure_designer_api.redoDescription();

  /// Undo the last command. Returns the description of the undone command, or null.
  String? undo() {
    final description = structure_designer_api.undoDescription();
    final result = structure_designer_api.undo();
    if (result) {
      refreshFromKernel();
      return description;
    }
    return null;
  }

  /// Redo the last undone command. Returns the description of the redone command, or null.
  String? redo() {
    final description = structure_designer_api.redoDescription();
    final result = structure_designer_api.redo();
    if (result) {
      refreshFromKernel();
      return description;
    }
    return null;
  }

  // ===== MOVE COALESCING =====

  /// Called when a node drag begins. Captures positions for undo coalescing.
  void beginMoveNodes({List<BigInt> scopeChain = const []}) {
    structure_designer_api.beginMoveNodes(
      scopePath: scopeChainToBytes(scopeChain),
    );
  }

  /// Called when a node drag ends. Creates a single MoveNodes undo command.
  void endMoveNodes({List<BigInt> scopeChain = const []}) {
    structure_designer_api.endMoveNodes(
      scopePath: scopeChainToBytes(scopeChain),
    );
  }

  // Called on each small update when dragging a node
  // Works only on the UI: do not update the position in the kernel
  void dragNodePosition(BigInt nodeId, Offset delta,
      {List<BigInt> scopeChain = const []}) {
    final node = _findNodeInScope(nodeId, scopeChain);
    if (node == null) return;
    final clamped = _clampZoneBodyDragDelta(delta, [node], scopeChain);
    node.position = APIVec2(
        x: node.position.x + clamped.dx, y: node.position.y + clamped.dy);
    notifyListeners();
  }

  /// Updates a node's position in the kernel and notifies listeners.
  void updateNodePosition(BigInt nodeId, {List<BigInt> scopeChain = const []}) {
    if (nodeNetworkView == null) return;
    final node = _findNodeInScope(nodeId, scopeChain);
    if (node == null) return;
    structure_designer_api.moveNode(
        scopePath: scopeChainToBytes(scopeChain),
        nodeId: nodeId,
        position: APIVec2(x: node.position.x, y: node.position.y));
    refreshFromKernel();
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

  /// True iff [prefix] is a (non-strict) prefix of [chain] — i.e. the first
  /// `prefix.length` ids of `chain` match `prefix` element-wise. Used to test
  /// the scope-chain prefix relationship from `doc/design_zones_ui.md`
  /// §"Computing source_scope_depth at wire creation".
  static bool _isScopePrefix(List<BigInt> prefix, List<BigInt> chain) {
    if (prefix.length > chain.length) return false;
    for (int i = 0; i < prefix.length; i++) {
      if (prefix[i] != chain[i]) return false;
    }
    return true;
  }

  /// True if a [PinKind.zoneInput] [source] may legally wire to a destination
  /// whose evaluation scope is [effectiveDestScope].
  ///
  /// A zone-input pin produces an HOF body's iteration value (`element` /
  /// `acc`), which only exists *inside that HOF's own body*. So the destination
  /// must live at the HOF's body scope (`source.scopeChain ++ [source.nodeId]`)
  /// or deeper — i.e. that body scope must be a prefix of [effectiveDestScope].
  /// A sibling of the HOF, or any node in an unrelated zone, references a value
  /// that isn't in scope there. The Rust authority (`can_connect_wire_scoped`)
  /// deliberately delegates this scope-containment check to the caller, so it
  /// must be enforced here. Captures flow the other way (an ancestor node's
  /// output *into* a body) and are unaffected; non-`zoneInput` sources are
  /// unconstrained by this rule.
  static bool _zoneInputSourceInScope(
      PinReference source, List<BigInt> effectiveDestScope) {
    if (source.pinKind != PinKind.zoneInput) return true;
    final bodyScope = [...source.scopeChain, source.nodeId];
    return _isScopePrefix(bodyScope, effectiveDestScope);
  }

  bool canConnectPins(PinReference pin1, PinReference pin2) {
    if (pin1.isOutput == pin2.isOutput) {
      return false;
    }

    final outPin = pin1.isOutput ? pin1 : pin2;
    final inPin = pin1.isInput ? pin1 : pin2;

    if (!outPin.isOutput || !inPin.isInput) {
      return false;
    }

    if (inPin.pinIndex < 0) {
      return false;
    }

    // Compute the destination's *evaluation scope* per the design doc formula:
    // for ZoneOutput destinations the body scope is one level deeper than the
    // HOF's own scope. For all other destinations the evaluation scope equals
    // the destination's scope chain.
    final effectiveDestScope = inPin.pinKind == PinKind.zoneOutput
        ? [...inPin.scopeChain, inPin.nodeId]
        : inPin.scopeChain;

    // The source's containing scope must be a prefix of the destination's
    // evaluation scope. Otherwise the source can't be evaluated at the
    // destination's call site.
    if (!_isScopePrefix(outPin.scopeChain, effectiveDestScope)) return false;
    // A zone-input source's iteration value is only in scope inside its own
    // HOF body; reject drops onto siblings or unrelated zones.
    if (!_zoneInputSourceInScope(outPin, effectiveDestScope)) return false;
    final sourceScopeDepth =
        effectiveDestScope.length - outPin.scopeChain.length;

    if (inPin.pinKind == PinKind.zoneOutput) {
      // Body-return wire: source must live in the body (depth 0 relative to
      // the body's scope, i.e. depth `bodyScope.length - source.scope.length`
      // in path-prefix terms above). Capture-into-zone-output is rejected —
      // the body-return wire's source is always body-local.
      if (sourceScopeDepth != 0) return false;
      if (outPin.pinKind != PinKind.externalOutput &&
          outPin.pinKind != PinKind.functionPin) {
        return false;
      }
      // Accept on structural match — strict type checking for body-return
      // wires is U5 polish work.
      return true;
    }

    // Same-scope NodeOutput → use the existing predicate. Local wires
    // through `can_connect_wire_scoped` also work, but `canConnectNodes`
    // preserves the U4-era code path exactly.
    if (sourceScopeDepth == 0 && outPin.pinKind != PinKind.zoneInput) {
      return structure_designer_api.canConnectNodes(
        scopePath: scopeChainToBytes(inPin.scopeChain),
        sourceNodeId: outPin.nodeId,
        sourceOutputPinIndex: outPin.pinIndex,
        destNodeId: inPin.nodeId,
        destParamIndex: BigInt.from(inPin.pinIndex),
      );
    }

    // Cross-scope wire: capture or iteration-value reference. ZoneInput sources
    // can only land here (they have `sourceScopeDepth >= 1` when wired into
    // their own HOF's body).
    final sourcePin = _pinReferenceToApiSourcePin(outPin);
    if (sourcePin == null) return false;
    return structure_designer_api.canConnectWire(
      destScopePath: scopeChainToBytes(inPin.scopeChain),
      sourceNodeId: outPin.nodeId,
      sourcePin: sourcePin,
      sourceScopeDepth: sourceScopeDepth,
      destNodeId: inPin.nodeId,
      destParamIndex: BigInt.from(inPin.pinIndex),
    );
  }

  void connectPins(PinReference pin1, PinReference pin2) {
    if (pin1.isOutput == pin2.isOutput) {
      return;
    }

    final outPin = pin1.isOutput ? pin1 : pin2;
    final inPin = pin1.isInput ? pin1 : pin2;

    if (!outPin.isOutput || !inPin.isInput) {
      return;
    }

    if (inPin.pinIndex < 0) {
      return;
    }

    final effectiveDestScope = inPin.pinKind == PinKind.zoneOutput
        ? [...inPin.scopeChain, inPin.nodeId]
        : inPin.scopeChain;

    if (!_isScopePrefix(outPin.scopeChain, effectiveDestScope)) return;
    // A zone-input source's iteration value is only in scope inside its own
    // HOF body; reject drops onto siblings or unrelated zones.
    if (!_zoneInputSourceInScope(outPin, effectiveDestScope)) return;
    final sourceScopeDepth =
        effectiveDestScope.length - outPin.scopeChain.length;

    if (inPin.pinKind == PinKind.zoneOutput) {
      // Body-return wire: source lives inside the HOF's body. The wire is
      // stored on the HOF's `zone_output_arguments`. See
      // `doc/design_zones_ui.md` §"Wire-creation API generalisation"
      // → Body return row.
      if (sourceScopeDepth != 0) return;
      structure_designer_api.connectZoneOutputWire(
        bodyScopePath: scopeChainToBytes(effectiveDestScope),
        sourceNodeId: outPin.nodeId,
        sourceOutputPinIndex: outPin.pinIndex,
        zoneOutputIndex: BigInt.from(inPin.pinIndex),
      );
    } else if (sourceScopeDepth == 0 && outPin.pinKind != PinKind.zoneInput) {
      // Same-scope regular wire — preserve the U4 code path.
      structure_designer_api.connectNodes(
        scopePath: scopeChainToBytes(inPin.scopeChain),
        sourceNodeId: outPin.nodeId,
        sourceOutputPinIndex: outPin.pinIndex,
        destNodeId: inPin.nodeId,
        destParamIndex: BigInt.from(inPin.pinIndex),
      );
    } else {
      // Cross-scope wire: capture (NodeOutput, depth ≥ 1) or iteration-value
      // reference (ZoneInput source, depth ≥ 1). U5 of `design_zones_ui.md`.
      final sourcePin = _pinReferenceToApiSourcePin(outPin);
      if (sourcePin == null) return;
      structure_designer_api.connectWire(
        destScopePath: scopeChainToBytes(inPin.scopeChain),
        sourceNodeId: outPin.nodeId,
        sourcePin: sourcePin,
        sourceScopeDepth: sourceScopeDepth,
        destNodeId: inPin.nodeId,
        destParamIndex: BigInt.from(inPin.pinIndex),
      );
    }

    draggedWire = null;

    refreshFromKernel();
  }

  /// Translate a Flutter [PinReference] into the FRB `APISourcePin` enum.
  /// Returns null for input pins (not source-shaped) — only [PinKind.externalOutput],
  /// [PinKind.functionPin], and [PinKind.zoneInput] are accepted.
  APISourcePin? _pinReferenceToApiSourcePin(PinReference pin) {
    switch (pin.pinKind) {
      case PinKind.externalOutput:
      case PinKind.functionPin:
        return APISourcePin.nodeOutput(pinIndex: pin.pinIndex);
      case PinKind.zoneInput:
        if (pin.pinIndex < 0) return null;
        return APISourcePin.zoneInput(pinIndex: pin.pinIndex);
      case PinKind.externalInput:
      case PinKind.zoneOutput:
        return null;
    }
  }

  /// Auto-connects a source pin to the first compatible pin on a target node.
  /// Used after creating a node from wire drop in empty space.
  bool autoConnectToNode(
    BigInt sourceNodeId,
    int sourcePinIndex,
    bool sourceIsOutput,
    BigInt targetNodeId, {
    List<BigInt> scopeChain = const [],
  }) {
    final result = structure_designer_api.autoConnectToNode(
      scopePath: scopeChainToBytes(scopeChain),
      sourceNodeId: sourceNodeId,
      sourcePinIndex: sourcePinIndex,
      sourceIsOutput: sourceIsOutput,
      targetNodeId: targetNodeId,
    );
    refreshFromKernel();
    return result;
  }

  void setSelectedNode(BigInt nodeId, {List<BigInt> scopeChain = const []}) {
    if (nodeNetworkView == null) return;
    final node = _findNodeInScope(nodeId, scopeChain);
    if (node != null && !node.selected) {
      structure_designer_api.selectNode(
        scopePath: scopeChainToBytes(scopeChain),
        nodeId: nodeId,
      );
    }
    refreshFromKernel();
  }

  /// Look up a node either at the top level or inside a nested body scope.
  /// Returns null if the path can't be walked or the node isn't found.
  NodeView? _findNodeInScope(BigInt nodeId, List<BigInt> scopeChain) {
    if (nodeNetworkView == null) return null;
    Map<BigInt, NodeView> currentNodes = nodeNetworkView!.nodes;
    for (final hofId in scopeChain) {
      final hof = currentNodes[hofId];
      final zone = hof?.zone;
      if (zone == null) return null;
      currentNodes = zone.nodes;
    }
    return currentNodes[nodeId];
  }

  /// Scrolls the node network panel to center the given node.
  void scrollToNode(BigInt nodeId) {
    onScrollToNode?.call(nodeId);
  }

  /// Navigate to one usage of a custom network (Find Usages, issue #414 —
  /// `doc/design_find_usages.md` D4).
  ///
  /// Activating the host network routes through [setActiveNodeNetwork], so the
  /// hop is recorded in the Rust navigation history for free and *Back* returns
  /// to the network the jump started from. The instance is then selected in its
  /// own scope and scrolled into view.
  ///
  /// [screenAnchor] makes the landing *anchored*: the target node's center is
  /// placed at that point (the source node's center at right-click time), so
  /// the node the user was looking at stays put on screen. Callers with no
  /// meaningful source position (the user-types panel entry points) omit it and
  /// get a viewport-centered landing. The zoom level is never changed.
  void jumpToUsage(APINetworkUsage usage, {Offset? screenAnchor}) {
    // `Uint64List` already holds `BigInt`s — no per-element conversion.
    final scopeChain = usage.scopePath.toList();
    setActiveNodeNetwork(usage.hostNetwork);
    setSelectedNode(usage.nodeId, scopeChain: scopeChain);
    // Keyboard ops (Delete, Ctrl+C/D, …) should act on the body we landed in,
    // not on whatever body was active in the network we came from.
    setActiveScopeChain(scopeChain);
    onScrollToNode?.call(usage.nodeId,
        scopeChain: scopeChain, screenAnchor: screenAnchor);
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

  /// Find the currently selected node together with the scope chain of the
  /// body it lives in. Unlike [getSelectedNodeId] (top-level only), this
  /// descends into HOF zone bodies, so a selection inside a zone resolves
  /// correctly. Prefers the active body's selection, then falls back to a full
  /// tree walk. Returns null if nothing is selected.
  ///
  /// Keyboard shortcuts that act on a single node (e.g. Ctrl+D duplicate) use
  /// this so they address the right node — a body node and a top-level node can
  /// share a numeric id (per-body `next_node_id` counters), so the node id is
  /// only meaningful together with its scope chain.
  ({BigInt nodeId, List<BigInt> scopeChain})? getSelectedNodeWithScope() {
    final view = nodeNetworkView;
    if (view == null) return null;
    // Active body first: if the user clicked into a body, that body's
    // selected node is the one keyboard shortcuts should act on.
    if (activeScopeChain.isNotEmpty) {
      Map<BigInt, NodeView> current = view.nodes;
      bool valid = true;
      for (final hofId in activeScopeChain) {
        final zone = current[hofId]?.zone;
        if (zone == null) {
          valid = false;
          break;
        }
        current = zone.nodes;
      }
      if (valid) {
        for (final entry in current.entries) {
          if (entry.value.selected) {
            return (nodeId: entry.key, scopeChain: activeScopeChain);
          }
        }
      }
    }
    // Fall back: walk the whole scope tree, tracking the scope chain.
    return _findSelectedNodeWithScope(view.nodes, const <BigInt>[]);
  }

  ({BigInt nodeId, List<BigInt> scopeChain})? _findSelectedNodeWithScope(
    Map<BigInt, NodeView> nodes,
    List<BigInt> scopeChain,
  ) {
    for (final entry in nodes.entries) {
      if (entry.value.selected) {
        return (nodeId: entry.key, scopeChain: scopeChain);
      }
    }
    for (final entry in nodes.entries) {
      final zone = entry.value.zone;
      if (zone == null) continue;
      final inner = _findSelectedNodeWithScope(
        zone.nodes,
        [...scopeChain, entry.key],
      );
      if (inner != null) return inner;
    }
    return null;
  }

  bool renameNodeNetwork(String oldName, String newName) {
    final success = structure_designer_api.renameNodeNetwork(
      oldName: oldName,
      newName: newName,
    );

    if (success) {
      // Always refresh the view - comment nodes in any network may reference
      // the renamed network via backticks and need to display updated text
      refreshFromKernel();
    }
    return success;
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
      // The deleted network's own nodes were usages of *other* networks, so the
      // counts shift even though a referenced network can never be deleted.
      networkUsageCounts = structure_designer_api.getNetworkUsageCounts();
      notifyListeners();
      return null; // Success
    } else {
      return result.errorMessage; // Return error message
    }
  }

  /// Duplicate the node network [sourceName] under an auto-generated unique
  /// name (`<name>_copy`, then `<name>_copy_2`, …). The copy is shallow: inline
  /// zone bodies are copied, while references to other named networks stay
  /// references. The backend activates the new copy. Returns null on success or
  /// an error message on failure.
  String? duplicateNodeNetwork(String sourceName) {
    final result = structure_designer_api.duplicateNodeNetwork(
      sourceName: sourceName,
    );
    if (result.success) {
      // The backend already activated the copy; refresh picks it up as the
      // active network view and reloads the network list.
      refreshFromKernel();
      return null;
    }
    return result.errorMessage;
  }

  bool renameNamespace(String oldPrefix, String newPrefix) {
    final success = structure_designer_api.renameNamespace(
      oldPrefix: oldPrefix,
      newPrefix: newPrefix,
    );
    if (success) {
      refreshFromKernel();
    }
    return success;
  }

  /// Read-only preview of moving/renaming the namespace [oldPrefix] to
  /// [newPrefix] (empty [newPrefix] promotes its contents to the root).
  /// Drives the move-namespace dialog; does not mutate state.
  APINamespaceRenamePreview previewNamespaceRename(
      String oldPrefix, String newPrefix) {
    return structure_designer_api.previewNamespaceRename(
      oldPrefix: oldPrefix,
      newPrefix: newPrefix,
    );
  }

  /// Read-only preview of moving/renaming a single leaf [oldName] — a node
  /// network or a record type def — to the fully-qualified [newName]. The kind
  /// is detected by the backend. Returns a single-item preview; does not
  /// mutate state.
  APINamespaceRenamePreview previewLeafRename(String oldName, String newName) {
    return structure_designer_api.previewLeafRename(
      oldName: oldName,
      newName: newName,
    );
  }

  String? deleteNamespace(String prefix) {
    final result = structure_designer_api.deleteNamespace(prefix: prefix);
    if (result.success) {
      refreshFromKernel();
      return null;
    }
    return result.errorMessage;
  }

  // --- CLI Access Rules ---

  /// Check whether CLI write access is locked for a given network/namespace name.
  bool isCliWriteLocked(String name) {
    return structure_designer_api.isCliWriteLocked(networkName: name);
  }

  /// Set CLI access for a namespace or network name.
  /// `allowed = true` means CLI can write, `allowed = false` means locked.
  void setCliAccess(String name, {required bool allowed}) {
    structure_designer_api.setCliAccess(name: name, allowed: allowed);
    notifyListeners();
  }

  /// Get all CLI access rules as a map of prefix -> allowed.
  Map<String, bool> getCliAccessRules() {
    final rules = structure_designer_api.getCliAccessRules();
    return {for (final rule in rules) rule.$1: rule.$2};
  }

  void setReturnNodeId(BigInt? nodeId) {
    // Only the top-level network has a return node — bodies emit through
    // zone-output pins instead. The `scope_path` parameter is plumbed for
    // API-shape symmetry per `doc/design_zones_ui.md`.
    structure_designer_api.setReturnNodeId(
      scopePath: Uint64List(0),
      nodeId: nodeId,
    );
    refreshFromKernel();
  }

  void addNewNodeNetwork() {
    // Rust returns the generated name (and has already activated it). The
    // registry is a HashMap, so we cannot infer the new network from list
    // order — selecting `nodeNetworkNames.last` picked a random network
    // (issue #315). Select the returned name explicitly instead.
    final newNetworkName = structure_designer_api.addNewNodeNetwork();
    if (newNetworkName.isNotEmpty) {
      setActiveNodeNetwork(newNetworkName);
    } else {
      refreshFromKernel();
    }
  }

  /// Adds a new node network with an auto-generated unique name under
  /// `namespace` (a dot-delimited folder path; empty string = root) and
  /// activates it. Returns the generated qualified name, or null on failure.
  String? addNewNodeNetworkInNamespace(String namespace) {
    final newNetworkName = structure_designer_api.addNewNodeNetworkInNamespace(
        namespace: namespace);
    if (newNetworkName.isNotEmpty) {
      setActiveNodeNetwork(newNetworkName);
      return newNetworkName;
    }
    refreshFromKernel();
    return null;
  }

  /// Creates an empty folder at `path` (dot-delimited). Returns null on
  /// success, or an error message (collision / invalid name). See
  /// `doc/design_empty_folders.md`.
  String? addFolder(String path) {
    final result = structure_designer_api.addFolder(path: path);
    refreshFromKernel();
    return result.success ? null : result.errorMessage;
  }

  /// Adds a new empty record type def with an auto-generated unique name under
  /// `namespace` (a dot-delimited folder path; empty string = root) and
  /// activates it in the schema editor. Returns the generated qualified name,
  /// or null on failure.
  String? addNewRecordTypeDefInNamespace(String namespace) {
    final newDefName = structure_designer_api.addNewRecordTypeDefInNamespace(
        namespace: namespace);
    if (newDefName.isNotEmpty) {
      // Open the new def in the schema editor. The active record def is
      // backend-owned (§8), so write it through; `refreshFromKernel` mirrors.
      structure_designer_api.setActiveRecordDefName(name: newDefName);
      refreshFromKernel();
      return newDefName;
    }
    refreshFromKernel();
    return null;
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
      BigInt destNodeId, BigInt destParamIndex,
      {List<BigInt> scopeChain = const []}) {
    if (nodeNetworkView == null) return;
    //TODO: only select a wire if not already selected.
    structure_designer_api.selectWire(
        scopePath: scopeChainToBytes(scopeChain),
        sourceNodeId: sourceNodeId,
        sourceOutputPinIndex: sourceOutputPinIndex.toInt(),
        destinationNodeId: destNodeId,
        destinationArgumentIndex: destParamIndex);
    refreshFromKernel();
  }

  /// Resolve a node by id **within its scope**. `scopeChain` is the chain of
  /// zone-owning node ids down to the body the node lives in; empty = the
  /// top-level network. Returns `null` if any hop is missing or is not a
  /// zone-owning node.
  NodeView? _resolveNodeInScope(BigInt nodeId, List<BigInt> scopeChain) {
    final view = nodeNetworkView;
    if (view == null) return null;
    Map<BigInt, NodeView> nodes = view.nodes;
    for (final hofId in scopeChain) {
      final zone = nodes[hofId]?.zone;
      if (zone == null) return null;
      nodes = zone.nodes;
    }
    return nodes[nodeId];
  }

  void toggleNodeDisplay(BigInt nodeId, {List<BigInt> scopeChain = const []}) {
    if (nodeNetworkView == null) return;
    // Resolve through the scope chain: a body node's numeric id routinely
    // collides with a top-level one (per-body `next_node_id` counters), so a
    // bare `nodes[nodeId]` lookup would read the wrong node's `displayed` flag
    // and toggle the body node to the wrong state.
    final node = _resolveNodeInScope(nodeId, scopeChain);
    if (node == null) return;

    structure_designer_api.setNodeDisplay(
      scopePath: scopeChainToBytes(scopeChain),
      nodeId: nodeId,
      isDisplayed: !node.displayed,
    );
    refreshFromKernel();
  }

  void toggleOutputPinDisplay(BigInt nodeId, int pinIndex,
      {List<BigInt> scopeChain = const []}) {
    if (nodeNetworkView == null) return;
    structure_designer_api.toggleOutputPinDisplay(
      scopePath: scopeChainToBytes(scopeChain),
      nodeId: nodeId,
      pinIndex: pinIndex,
    );
    refreshFromKernel();
  }

  void removeSelected({List<BigInt> scopeChain = const []}) {
    if (nodeNetworkView == null) return;
    structure_designer_api.deleteSelected(
      scopePath: scopeChainToBytes(scopeChain),
    );
    refreshFromKernel();
  }

  // ===== COPY / PASTE / CUT =====

  /// Copies the current selection to the clipboard.
  /// Returns true if something was copied, false if selection was empty.
  bool copySelection({List<BigInt> scopeChain = const []}) {
    return structure_designer_api.copySelection(
      scopePath: scopeChainToBytes(scopeChain),
    );
  }

  /// Pastes clipboard content at the given position (network coordinates).
  void pasteAtPosition(double x, double y,
      {List<BigInt> scopeChain = const []}) {
    structure_designer_api.pasteAtPosition(
      scopePath: scopeChainToBytes(scopeChain),
      x: x,
      y: y,
    );
    refreshFromKernel();
  }

  /// Cuts the current selection (copy + delete).
  /// Returns true if something was cut.
  bool cutSelection({List<BigInt> scopeChain = const []}) {
    final result = structure_designer_api.cutSelection(
      scopePath: scopeChainToBytes(scopeChain),
    );
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

  void setEditAtomSelectedElement(int atomicNumber) {
    if (nodeNetworkView == null) return;
    edit_atom_api.setEditAtomSelectedElement(atomicNumber: atomicNumber);
    refreshFromKernel();
  }

  // ===== ATOM_EDIT (NEW DIFF-BASED NODE) METHODS =====

  void setActiveAtomEditTool(APIAtomEditTool tool) {
    atom_edit_api.setActiveAtomEditTool(tool: tool);
    _bondLengthMode = APIBondLengthMode.crystal;
    _hybridizationOverride = APIHybridization.auto;
    _bondMode = APIBondMode.covalent;
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
      hybridizationOverride: _hybridizationOverride,
    );
    refreshFromKernel();
  }

  void atomEditAddAtomAtPosition(int atomicNumber, APIVec3 position) {
    if (nodeNetworkView == null) return;
    atom_edit_api.atomEditAddAtomAtPosition(
      atomicNumber: atomicNumber,
      position: position,
      hybridizationOverride: _hybridizationOverride,
    );
    refreshFromKernel();
  }

  // Note: atomEditDrawBondByRay removed — replaced by drag-to-bond interaction
  // in _AtomEditAddBondDelegate (structure_designer_viewport.dart).

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

  void setAtomEditTolerance(double value) {
    atom_edit_api.atomEditSetTolerance(value: value);
    refreshFromKernel();
  }

  void toggleAtomEditErrorOnStaleEntries() {
    atom_edit_api.atomEditToggleErrorOnStaleEntries();
    refreshFromKernel();
  }

  void toggleAtomEditContinuousMinimization() {
    atom_edit_api.atomEditToggleContinuousMinimization();
    refreshFromKernel();
  }

  void setAtomEditSelectedElement(int atomicNumber) {
    if (nodeNetworkView == null) return;
    atom_edit_api.setAtomEditSelectedElement(atomicNumber: atomicNumber);
    atomEditSelectedElement = atomicNumber;
    refreshFromKernel();
  }

  void atomEditMinimize(APIMinimizeFreezeMode freezeMode) {
    _lastMinimizeMessage =
        atom_edit_api.atomEditMinimize(freezeMode: freezeMode);
    refreshFromKernel();
    notifyListeners();
  }

  void atomEditAddHydrogen({required bool selectedOnly}) {
    _lastAddHydrogenMessage =
        atom_edit_api.atomEditAddHydrogen(selectedOnly: selectedOnly);
    refreshFromKernel();
    notifyListeners();
  }

  String _lastRemoveHydrogenMessage = '';
  String get lastRemoveHydrogenMessage => _lastRemoveHydrogenMessage;

  void atomEditRemoveHydrogen({required bool selectedOnly}) {
    _lastRemoveHydrogenMessage =
        atom_edit_api.atomEditRemoveHydrogen(selectedOnly: selectedOnly);
    refreshFromKernel();
    notifyListeners();
  }

  // ===== MODIFY MEASUREMENT =====

  String atomEditModifyDistance(
      double targetDistance, bool moveFirst, bool moveFragment) {
    final msg = atom_edit_api.atomEditModifyDistance(
        targetDistance: targetDistance,
        moveFirst: moveFirst,
        moveFragment: moveFragment);
    refreshFromKernel();
    return msg;
  }

  String atomEditModifyAngle(
      double targetAngleDegrees, bool moveArmA, bool moveFragment) {
    final msg = atom_edit_api.atomEditModifyAngle(
        targetAngleDegrees: targetAngleDegrees,
        moveArmA: moveArmA,
        moveFragment: moveFragment);
    refreshFromKernel();
    return msg;
  }

  String atomEditModifyDihedral(
      double targetAngleDegrees, bool moveASide, bool moveFragment) {
    final msg = atom_edit_api.atomEditModifyDihedral(
        targetAngleDegrees: targetAngleDegrees,
        moveASide: moveASide,
        moveFragment: moveFragment);
    refreshFromKernel();
    return msg;
  }

  double? atomEditGetDefaultBondLength() {
    return atom_edit_api.atomEditGetDefaultBondLength(
        bondLengthMode: _bondLengthMode);
  }

  double? atomEditGetDefaultAngle() {
    return atom_edit_api.atomEditGetDefaultAngle();
  }

  // ===== GUIDED ATOM PLACEMENT =====

  GuidedPlacementApiResult atomEditStartGuidedPlacement(
    vector_math.Vector3 rayStart,
    vector_math.Vector3 rayDir,
    int atomicNumber,
    APIHybridization hybridizationOverride,
    APIBondMode bondMode,
    APIBondLengthMode bondLengthMode,
  ) {
    final result = atom_edit_api.atomEditStartGuidedPlacement(
      rayStart: vector3ToApiVec3(rayStart),
      rayDir: vector3ToApiVec3(rayDir),
      atomicNumber: atomicNumber,
      hybridizationOverride: hybridizationOverride,
      bondMode: bondMode,
      bondLengthMode: bondLengthMode,
    );
    refreshFromKernel();
    return result;
  }

  bool atomEditPlaceGuidedAtom(
    vector_math.Vector3 rayStart,
    vector_math.Vector3 rayDir,
  ) {
    final placed = atom_edit_api.atomEditPlaceGuidedAtom(
      rayStart: vector3ToApiVec3(rayStart),
      rayDir: vector3ToApiVec3(rayDir),
    );
    refreshFromKernel();
    return placed;
  }

  void atomEditCancelGuidedPlacement() {
    atom_edit_api.atomEditCancelGuidedPlacement();
    refreshFromKernel();
  }

  // ===== PLACEMENT GUIDELINE TOOL (issue #368) =====

  /// Build the frozen line from the tool-local defining set (1/2/3 atoms).
  /// `direction` is used only for the 1-atom directional line. Returns an empty
  /// string on success or an error message (for a SnackBar) on degenerate input.
  String guidelineCreateFromDefining(APIVec3 direction) {
    final error =
        atom_edit_api.guidelineCreateFromDefining(direction: direction);
    refreshFromKernel();
    notifyListeners();
    return error;
  }

  /// Set the active point's along-line position `t` (slides the picked atom in
  /// Move mode, the ghost marker in Place mode).
  void guidelineSetPosition(double t) {
    atom_edit_api.guidelineSetPosition(t: t);
    refreshFromKernel();
    notifyListeners();
  }

  /// Place a free atom of the panel element at the ghost marker (→ Move).
  bool guidelinePlaceAtom() {
    final placed = atom_edit_api.guidelinePlaceAtom();
    refreshFromKernel();
    notifyListeners();
    return placed;
  }

  /// Clear the guideline and return to `Define` (Clear button / Escape).
  void guidelineClear() {
    atom_edit_api.guidelineClear();
    refreshFromKernel();
    notifyListeners();
  }

  /// Remember the 1-atom direction (persists across Clear / re-Define).
  void guidelineSetEnteredDirection(APIVec3 direction) {
    atom_edit_api.guidelineSetEnteredDirection(direction: direction);
    refreshFromKernel();
    notifyListeners();
  }

  /// Lightweight panel rebuild during a guideline drag so the position field
  /// tracks the marker / atom live (#368). Deliberately does NOT call
  /// `refreshFromKernel()` — the Guideline card re-reads its view directly via
  /// FFI on rebuild, so a plain notify is enough and avoids a per-frame re-fetch
  /// of the whole node-network view.
  void notifyGuidelineToolSync() {
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

  BigInt createNode(
    String nodeTypeName,
    Offset position, {
    APIDragSource? dragSource,
    List<BigInt> scopeChain = const [],
  }) {
    if (nodeNetworkView == null) return BigInt.zero;
    final nodeId = structure_designer_api.addNode(
      scopePath: scopeChainToBytes(scopeChain),
      nodeTypeName: nodeTypeName,
      position: APIVec2(x: position.dx, y: position.dy),
      dragSource: dragSource,
    );
    refreshFromKernel();
    return nodeId;
  }

  BigInt duplicateNode(BigInt nodeId, {List<BigInt> scopeChain = const []}) {
    if (nodeNetworkView == null) return BigInt.zero;
    final scopePath = scopeChainToBytes(scopeChain);
    final newNodeId = structure_designer_api.duplicateNode(
      scopePath: scopePath,
      nodeId: nodeId,
    );
    if (newNodeId != BigInt.zero) {
      structure_designer_api.selectNode(
        scopePath: scopePath,
        nodeId: newNodeId,
      );
    }
    refreshFromKernel();
    return newNodeId;
  }

  /// Promote a node to a parameter.
  ///
  /// Inserts a `parameter` node typed after the node's output pin 0,
  /// wires that pin into the parameter's default input, and rewires every
  /// downstream consumer of the source's pin 0 to read from the parameter.
  /// Returns the API result; UI is refreshed regardless of success so any
  /// partial state is reflected.
  APIPromoteToParameterResult promoteNodeToParameter(BigInt nodeId) {
    final result =
        structure_designer_api.promoteNodeToParameter(nodeId: nodeId);
    refreshFromKernel();
    return result;
  }

  /// Inline the custom-network instance `nodeId` (in scope `scopeChain`):
  /// replace it with a copy of its network's contents, spliced into the
  /// containing network/body in place. The named definition is left untouched.
  /// Returns the API result; UI is refreshed regardless of success.
  /// See `doc/design_inline_custom_node.md`.
  InlineResult inlineCustomNode(BigInt nodeId,
      {List<BigInt> scopeChain = const []}) {
    final result = structure_designer_api.inlineCustomNode(
      scopePath: scopeChainToBytes(scopeChain),
      nodeId: nodeId,
    );
    refreshFromKernel();
    return result;
  }

  /// Whether `nodeId` (in scope `scopeChain`) can be converted to a closure —
  /// a custom-network instance used as a function (or unconsumed) with a return
  /// node. Gates the context-menu item.
  bool canConvertInstanceToClosure(BigInt nodeId,
      {List<BigInt> scopeChain = const []}) {
    return structure_designer_api.canConvertInstanceToClosure(
      scopePath: scopeChainToBytes(scopeChain),
      nodeId: nodeId,
    );
  }

  /// Whether `nodeId` (in scope `scopeChain`) can be extracted to a network —
  /// a `closure` node with a result wire. Gates the context-menu item.
  bool canExtractClosureToNetwork(BigInt nodeId,
      {List<BigInt> scopeChain = const []}) {
    return structure_designer_api.canExtractClosureToNetwork(
      scopePath: scopeChainToBytes(scopeChain),
      nodeId: nodeId,
    );
  }

  /// Convert the custom-network instance `nodeId` (in scope `scopeChain`) into a
  /// `closure` node (*Network → Closure*): its body becomes a copy of the
  /// instance's network, wired input pins become captures, unwired pins become
  /// closure parameters. Returns the API result; UI is refreshed regardless of
  /// success. See `doc/design_closure_network_conversion.md`.
  ConversionResult convertInstanceToClosure(BigInt nodeId,
      {List<BigInt> scopeChain = const []}) {
    final result = structure_designer_api.convertInstanceToClosure(
      scopePath: scopeChainToBytes(scopeChain),
      nodeId: nodeId,
    );
    refreshFromKernel();
    return result;
  }

  /// Extract the `closure` node `nodeId` (in scope `scopeChain`) into a new named
  /// custom network `name` (*Closure → Network*): lifts the closure's body into a
  /// fresh standalone network (with parameter nodes for both the closure's
  /// parameters and its captures) and replaces the closure with an instance of
  /// that network. Returns the API result; UI is refreshed regardless of success.
  /// See `doc/design_closure_network_conversion.md`.
  ConversionResult extractClosureToNetwork(BigInt nodeId, String name,
      {List<BigInt> scopeChain = const []}) {
    final result = structure_designer_api.extractClosureToNetwork(
      scopePath: scopeChainToBytes(scopeChain),
      nodeId: nodeId,
      name: name,
    );
    refreshFromKernel();
    return result;
  }

  /// Run an explicit Execute pass on `nodeId` in the active network.
  ///
  /// Synchronous FFI call (the `with_*_cad_instance` helpers require
  /// single-threaded UI access — see `doc/design_node_execution.md`
  /// "Why not async (worker thread) FFI"). The call blocks until the Rust
  /// side completes; the caller is responsible for showing a modal placard
  /// before the call so the user gets visual feedback.
  ///
  /// Returns `null` if no network is active. Otherwise returns the
  /// `APIExecuteResult` (with `ok` flag and optional `error` message), or
  /// throws on FFI / Rust panic.
  APIExecuteResult? executeNode(BigInt nodeId,
      {List<BigInt> scopeChain = const []}) {
    final networkName = nodeNetworkView?.name;
    if (networkName == null) return null;
    final result = structure_designer_api.executeNode(
      networkName: networkName,
      scopePath: scopeChainToBytes(scopeChain),
      nodeId: nodeId,
    );
    // An Execute pass might mutate displayed iterators / record state
    // through side effects, but more importantly, recorded node errors and
    // pin display strings are written into the per-pass context. A
    // refresh keeps the node-graph error indicators and subtitles current.
    refreshFromKernel();
    return result;
  }

  void setIntData(BigInt nodeId, APIIntData data) {
    structure_designer_api.setIntData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setStringData(BigInt nodeId, APIStringData data) {
    structure_designer_api.setStringData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setRecordConstructData(BigInt nodeId, APIRecordSchemaData data) {
    structure_designer_api.setRecordConstructData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setRecordDestructureData(BigInt nodeId, APIRecordSchemaData data) {
    structure_designer_api.setRecordDestructureData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setProductData(BigInt nodeId, APIRecordSchemaData data) {
    structure_designer_api.setProductData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setBoolData(BigInt nodeId, APIBoolData data) {
    structure_designer_api.setBoolData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setPrintData(BigInt nodeId, APIPrintData data) {
    structure_designer_api.setPrintData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setFloatData(BigInt nodeId, APIFloatData data) {
    structure_designer_api.setFloatData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  /// Returns the editable (simple-typed) parameters of a custom node, or
  /// `null` if `nodeId` is not a custom node. An empty list means a custom
  /// node with no simple-typed parameters.
  List<APILiteralField>? getCustomNodeParams(BigInt nodeId) =>
      structure_designer_api.getCustomNodeParams(
          scopePath: propertyEditorScopePath, nodeId: nodeId);

  void setCustomNodeLiteral(
      BigInt nodeId, String paramName, APILiteralValue value) {
    structure_designer_api.setCustomNodeLiteral(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        paramName: paramName,
        value: value);
    refreshFromKernel();
  }

  void clearCustomNodeLiteral(BigInt nodeId, String paramName) {
    structure_designer_api.clearCustomNodeLiteral(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        paramName: paramName);
    refreshFromKernel();
  }

  /// Returns the editable (simple-typed) fields of a `record_construct`
  /// node's chosen schema, or `null` if the node is not `record_construct`
  /// or its schema is empty / dangling. An empty list means a schema with no
  /// simple-typed fields.
  List<APILiteralField>? getRecordConstructFields(BigInt nodeId) =>
      structure_designer_api.getRecordConstructFields(
          scopePath: propertyEditorScopePath, nodeId: nodeId);

  void setRecordConstructLiteral(
      BigInt nodeId, String fieldName, APILiteralValue value) {
    structure_designer_api.setRecordConstructLiteral(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        fieldName: fieldName,
        value: value);
    refreshFromKernel();
  }

  void clearRecordConstructLiteral(BigInt nodeId, String fieldName) {
    structure_designer_api.clearRecordConstructLiteral(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        fieldName: fieldName);
    refreshFromKernel();
  }

  // --- `array` node (doc/design_array_node_and_field_hints.md Part B) ---
  //
  // The `array` node has no input pins, so every element edit is a pure
  // node-data mutation with standard undo. Only `setArrayElementType` retypes
  // the output pin (and can drop outgoing wires), which the Rust op handles
  // with a whole-network-snapshot undo. Each setter returns the kernel's
  // `APIResult` so the panel can surface a rejection inline.

  /// The `element_type`s an `array` node accepts, straight from the Rust
  /// predicate — the picker never offers a type the setter would reject.
  List<APIDataType> getArrayElementTypeOptions() =>
      structure_designer_api.getArrayElementTypeOptions();

  APIArrayNodeData? getArrayNodeData(BigInt nodeId) => structure_designer_api
      .getArrayNodeData(scopePath: propertyEditorScopePath, nodeId: nodeId);

  APIResult setArrayElementType(BigInt nodeId, APIDataType elementType) {
    final result = structure_designer_api.setArrayElementType(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        elementType: elementType);
    refreshFromKernel();
    return result;
  }

  APIResult addArrayElement(BigInt nodeId, int index) {
    final result = structure_designer_api.addArrayElement(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        index: index);
    refreshFromKernel();
    return result;
  }

  APIResult removeArrayElement(BigInt nodeId, int index) {
    final result = structure_designer_api.removeArrayElement(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        index: index);
    refreshFromKernel();
    return result;
  }

  APIResult moveArrayElement(BigInt nodeId, int from, int to) {
    final result = structure_designer_api.moveArrayElement(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        from: from,
        to: to);
    refreshFromKernel();
    return result;
  }

  APIResult setArrayElementLiteral(
      BigInt nodeId, int index, APILiteralValue value) {
    final result = structure_designer_api.setArrayElementLiteral(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        index: index,
        value: value);
    refreshFromKernel();
    return result;
  }

  /// Resets the element to its seeded default — the stale-row "clear" action.
  APIResult clearArrayElementLiteral(BigInt nodeId, int index) {
    final result = structure_designer_api.clearArrayElementLiteral(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        index: index);
    refreshFromKernel();
    return result;
  }

  APIResult setArrayElementFieldLiteral(
      BigInt nodeId, int index, String fieldName, APILiteralValue value) {
    final result = structure_designer_api.setArrayElementFieldLiteral(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        index: index,
        fieldName: fieldName,
        value: value);
    refreshFromKernel();
    return result;
  }

  APIResult clearArrayElementFieldLiteral(
      BigInt nodeId, int index, String fieldName) {
    final result = structure_designer_api.clearArrayElementFieldLiteral(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        index: index,
        fieldName: fieldName);
    refreshFromKernel();
    return result;
  }

  void setIvec2Data(BigInt nodeId, APIIVec2Data data) {
    structure_designer_api.setIvec2Data(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setIvec3Data(BigInt nodeId, APIIVec3Data data) {
    structure_designer_api.setIvec3Data(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setSupercellData(BigInt nodeId, APISupercellData data) {
    structure_designer_api.setSupercellData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setImat2RowsData(BigInt nodeId, APIIMat2RowsData data) {
    structure_designer_api.setImat2RowsData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setImat2ColsData(BigInt nodeId, APIIMat2ColsData data) {
    structure_designer_api.setImat2ColsData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setImat2DiagData(BigInt nodeId, APIIMat2DiagData data) {
    structure_designer_api.setImat2DiagData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setPlaneTilingVectorsData(
      BigInt nodeId, APIPlaneTilingVectorsData data) {
    structure_designer_api.setPlaneTilingVectorsData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setImat3RowsData(BigInt nodeId, APIIMat3RowsData data) {
    structure_designer_api.setImat3RowsData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setImat3ColsData(BigInt nodeId, APIIMat3ColsData data) {
    structure_designer_api.setImat3ColsData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setImat3DiagData(BigInt nodeId, APIIMat3DiagData data) {
    structure_designer_api.setImat3DiagData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setMat3RowsData(BigInt nodeId, APIMat3RowsData data) {
    structure_designer_api.setMat3RowsData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setMat3ColsData(BigInt nodeId, APIMat3ColsData data) {
    structure_designer_api.setMat3ColsData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setMat3DiagData(BigInt nodeId, APIMat3DiagData data) {
    structure_designer_api.setMat3DiagData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setRangeData(BigInt nodeId, APIRangeData data) {
    structure_designer_api.setRangeData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setVec2Data(BigInt nodeId, APIVec2Data data) {
    structure_designer_api.setVec2Data(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setVec3Data(BigInt nodeId, APIVec3Data data) {
    structure_designer_api.setVec3Data(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setCuboidData(BigInt nodeId, APICuboidData data) {
    structure_designer_api.setCuboidData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setSphereData(BigInt nodeId, APISphereData data) {
    structure_designer_api.setSphereData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setFreeSphereData(BigInt nodeId, APIFreeSphereData data) {
    structure_designer_api.setFreeSphereData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setRelaxData(BigInt nodeId, APIRelaxData data) {
    relax_api.setRelaxData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setXrayData(BigInt nodeId, APIXrayData data) {
    xray_api.setXrayData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setTagData(BigInt nodeId, APITagData data) {
    tag_api.setTagData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setUntagData(BigInt nodeId, APIUntagData data) {
    tag_api.setUntagData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setFreeCircleData(BigInt nodeId, APIFreeCircleData data) {
    structure_designer_api.setFreeCircleData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setExtrudeData(BigInt nodeId, APIExtrudeData data) {
    structure_designer_api.setExtrudeData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setHalfSpaceData(BigInt nodeId, APIHalfSpaceData data) {
    structure_designer_api.setHalfSpaceData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setDrawingPlaneData(BigInt nodeId, APIDrawingPlaneData data) {
    structure_designer_api.setDrawingPlaneData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setRectData(BigInt nodeId, APIRectData data) {
    structure_designer_api.setRectData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setCircleData(BigInt nodeId, APICircleData data) {
    structure_designer_api.setCircleData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setHalfPlaneData(BigInt nodeId, APIHalfPlaneData data) {
    structure_designer_api.setHalfPlaneData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setRegPolyData(BigInt nodeId, APIRegPolyData data) {
    structure_designer_api.setRegPolyData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setGeoTransData(BigInt nodeId, APIGeoTransData data) {
    structure_designer_api.setGeoTransData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  APILatticeSymopData? getLatticeSymopData(BigInt nodeId) {
    return structure_designer_api.getLatticeSymopData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  void setLatticeSymopData(BigInt nodeId, APILatticeSymopData data) {
    structure_designer_api.setLatticeSymopData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  APIStructureMoveData? getStructureMoveData(BigInt nodeId) {
    return structure_designer_api.getStructureMoveData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  void setStructureMoveData(BigInt nodeId, APIStructureMoveData data) {
    structure_designer_api.setStructureMoveData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  APIStructureRotData? getStructureRotData(BigInt nodeId) {
    return structure_designer_api.getStructureRotData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  void setStructureRotData(BigInt nodeId, APIStructureRotData data) {
    structure_designer_api.setStructureRotData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setFreeMoveData(BigInt nodeId, APIFreeMoveData data) {
    structure_designer_api.setFreeMoveData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setFreeRotData(BigInt nodeId, APIFreeRotData data) {
    structure_designer_api.setFreeRotData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setParameterData(BigInt nodeId, APIParameterData data) {
    structure_designer_api.setParameterData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setMApData(BigInt nodeId, APIMapData data) {
    structure_designer_api.setMapData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setFilterData(BigInt nodeId, APIFilterData data) {
    structure_designer_api.setFilterData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setForeachData(BigInt nodeId, APIForeachData data) {
    structure_designer_api.setForeachData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setCollectData(BigInt nodeId, APICollectData data) {
    structure_designer_api.setCollectData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  /// Whole-list lane + output-type edit on a `zip_with` node (the positional id
  /// merge). Ids are managed Rust-side; the shared `ZipWithLaneEditCommand`
  /// undo capture is created by the Rust setter, not here.
  void setZipWithData(BigInt nodeId, APIZipWithData data) {
    structure_designer_api.setZipWithData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  /// Id-accurate removal of one `zip_with` lane (the delete button). Surviving
  /// lanes keep their external wires; body wires remap in the same step.
  void removeZipWithLane(BigInt nodeId, int laneIndex) {
    structure_designer_api.removeZipWithLane(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        laneIndex: BigInt.from(laneIndex));
    refreshFromKernel();
  }

  /// Whole-data edit on a `switch` node (selector type, value type, case list).
  /// Case values cross as strings; Rust parses them per selector type and the
  /// value-keyed id merge (plus its shared `NodeStructureEditCommand` undo
  /// capture) runs Rust-side. Returns the `APIResult` so the editor can display
  /// a parse/duplicate error inline; nothing is mutated on failure.
  APIResult setSwitchData(BigInt nodeId, APISwitchData data) {
    final result = structure_designer_api.setSwitchData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
    return result;
  }

  void setPatchBuildData(BigInt nodeId, APIPatchBuildData data) {
    structure_designer_api.setPatchBuildData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setPatchLatticefillData(BigInt nodeId, APIPatchLatticeFillData data) {
    structure_designer_api.setPatchLatticefillData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setArrayAtData(BigInt nodeId, APIArrayAtData data) {
    structure_designer_api.setArrayAtData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setArrayAppendData(BigInt nodeId, APIArrayAppendData data) {
    structure_designer_api.setArrayAppendData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setArrayConcatData(BigInt nodeId, APIArrayConcatData data) {
    structure_designer_api.setArrayConcatData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setArrayLenData(BigInt nodeId, APIArrayLenData data) {
    structure_designer_api.setArrayLenData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setIfData(BigInt nodeId, APIIfData data) {
    structure_designer_api.setIfData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setFoldData(BigInt nodeId, APIFoldData data) {
    structure_designer_api.setFoldData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setClosureData(BigInt nodeId, APIClosureData data) {
    structure_designer_api.setClosureData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setApplyData(BigInt nodeId, APIApplyData data) {
    structure_designer_api.setApplyData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  /// Override one input pin's role in the node's `-1` function-pin view
  /// (Auto / Delayed / Supplied). The Rust setter normalizes `Auto` to entry
  /// removal, revalidates, and pushes the undo command. See
  /// `doc/design_function_pin_roles.md`.
  void setFunctionPinRole(
      BigInt nodeId, int pinIndex, APIFunctionPinRole role) {
    structure_designer_api.setFunctionPinRole(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        pinIndex: BigInt.from(pinIndex),
        role: role);
    refreshFromKernel();
  }

  APISequenceData? getSequenceData(BigInt nodeId) {
    return structure_designer_api.getSequenceData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  void setSequenceData(BigInt nodeId, APISequenceData data) {
    structure_designer_api.setSequenceData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  APIResult setExprData(BigInt nodeId, APIExprData data) {
    final result = structure_designer_api.setExprData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
    return result;
  }

  void setMotifData(BigInt nodeId, APIMotifData data) {
    structure_designer_api.setMotifData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  APIMotifData? getMotifData(BigInt nodeId) {
    return structure_designer_api.getMotifData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  void setMaterializeData(BigInt nodeId, APIMaterializeData data) {
    structure_designer_api.setMaterializeData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  APIMaterializeData? getMaterializeData(BigInt nodeId) {
    return structure_designer_api.getMaterializeData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  void setPassivateData(BigInt nodeId, APIPassivateData data) {
    structure_designer_api.setPassivateData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  APIPassivateData? getPassivateData(BigInt nodeId) {
    return structure_designer_api.getPassivateData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  void setMotifSubData(BigInt nodeId, APIMotifSubData data) {
    structure_designer_api.setMotifSubData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  APIMotifSubData? getMotifSubData(BigInt nodeId) {
    return structure_designer_api.getMotifSubData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  APIParameterData? getParameterData(BigInt nodeId) {
    return structure_designer_api.getParameterData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  APIExprData? getExprData(BigInt nodeId) {
    return structure_designer_api.getExprData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  void setImportXyzData(BigInt nodeId, APIImportXYZData data) {
    structure_designer_api.setImportXyzData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  APIImportXYZData? getImportXyzData(BigInt nodeId) {
    return structure_designer_api.getImportXyzData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  void setExportAtomsData(BigInt nodeId, APIExportAtomsData data) {
    structure_designer_api.setExportAtomsData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  APIExportAtomsData? getExportAtomsData(BigInt nodeId) {
    return structure_designer_api.getExportAtomsData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  APIResult importXyz(BigInt nodeId) {
    var result = import_xyz_api.importXyz(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
    refreshFromKernel();
    return result;
  }

  void setImportCifData(BigInt nodeId, APIImportCIFData data) {
    structure_designer_api.setImportCifData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  APIImportCIFData? getImportCifData(BigInt nodeId) {
    return structure_designer_api.getImportCifData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  APIResult importCif(BigInt nodeId) {
    var result = import_cif_api.importCif(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
    refreshFromKernel();
    return result;
  }

  void setInferBondsData(BigInt nodeId, APIInferBondsData data) {
    structure_designer_api.setInferBondsData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  APIInferBondsData? getInferBondsData(BigInt nodeId) {
    return structure_designer_api.getInferBondsData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  void setAtomReplaceData(BigInt nodeId, APIAtomReplaceData data) {
    structure_designer_api.setAtomReplaceData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  APIAtomReplaceData? getAtomReplaceData(BigInt nodeId) {
    return structure_designer_api.getAtomReplaceData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  void setApplyDiffData(BigInt nodeId, APIApplyDiffData data) {
    structure_designer_api.setApplyDiffData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setAtomComposeDiffData(BigInt nodeId, APIAtomComposeDiffData data) {
    structure_designer_api.setAtomComposediffData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setAtomCutData(BigInt nodeId, APIAtomCutData data) {
    structure_designer_api.setAtomCutData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void setLatticeVecsData(BigInt nodeId, APILatticeVecsData data) {
    structure_designer_api.setLatticeVecsData(
        scopePath: scopeChainToBytes(propertyEditorScopeChain),
        nodeId: nodeId,
        data: data);
    refreshFromKernel();
  }

  void refreshFromKernel() {
    nodeNetworkView = structure_designer_api.getNodeNetworkView();
    nodeNetworkNames =
        structure_designer_api.getNodeNetworksWithValidation() ?? [];
    recordTypeDefNames = structure_designer_api.getRecordTypeDefNames() ?? [];
    folderNames = structure_designer_api.getFolderNames() ?? [];
    networkUsageCounts = structure_designer_api.getNetworkUsageCounts();
    // The active record def is backend-owned (§8): mirror it here so the
    // schema-editor selection follows record renames/moves and survives
    // undo/redo (the backend remaps/clears it inside the relevant commands).
    activeRecordDefName = structure_designer_api.getActiveRecordDefName();
    // Defensive: drop a dangling reference if the backend value somehow points
    // at a def that no longer exists (e.g. undo of an add that auto-activated).
    if (activeRecordDefName != null &&
        !recordTypeDefNames.contains(activeRecordDefName)) {
      activeRecordDefName = null;
    }
    activeEditAtomTool = edit_atom_api.getActiveEditAtomTool();
    activeAtomEditTool = atom_edit_api.getActiveAtomEditTool();
    cameraCanonicalView = common_api.getCameraCanonicalView();
    isOrthographic = common_api.isOrthographic();
    viewUpInfo = common_api.getViewUp();
    preferences = structure_designer_api.getStructureDesignerPreferences();
    isDirty = structure_designer_api.isDesignDirty();
    filePath = structure_designer_api.getDesignFilePath();
    directEditingMode = structure_designer_api.getDirectEditingMode();

    // Drain any `print` node entries pushed during this refresh's eval pass
    // into the Console panel buffer. Drain-on-read keeps the Rust-side
    // `print_log` from growing indefinitely as long as the panel is
    // occasionally polled — which it is, after every refresh. If the panel
    // is closed, the entries still accumulate here for when the user opens
    // it. See `doc/design_node_execution.md` (Phase 4 — FFI).
    final newEntries = structure_designer_api.takePrintLog();
    if (newEntries.isNotEmpty) {
      printLog.addAll(newEntries);
      if (!consolePanelVisible) {
        unreadPrintLogCount += newEntries.length;
      }
    }

    notifyListeners();
  }

  /// Clear the Console panel log (Rust-side buffer + Flutter-side mirror).
  void clearPrintLog() {
    structure_designer_api.clearPrintLog();
    printLog.clear();
    unreadPrintLogCount = 0;
    notifyListeners();
  }

  /// Toggle the Console panel's docked-bottom visibility. Resetting the
  /// "new entries" counter when the panel becomes visible keeps the toolbar
  /// dot in sync.
  void toggleConsolePanel() {
    consolePanelVisible = !consolePanelVisible;
    if (consolePanelVisible) {
      unreadPrintLogCount = 0;
    }
    notifyListeners();
  }

  // Facet Shell API wrapper methods. These act on the node shown in the
  // property panel, so they pass `propertyEditorScopePath` (which can be a body
  // scope) rather than addressing by bare id.
  APIFacetShellData? getFacetShellData(BigInt nodeId) {
    if (nodeNetworkView == null) return null;
    return facet_shell_api.getFacetShellData(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
  }

  bool setFacetShellCenter(BigInt nodeId, APIIVec3 center, int maxMillerIndex) {
    if (nodeNetworkView == null) return false;
    final result = facet_shell_api.setFacetShellCenter(
      scopePath: propertyEditorScopePath,
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
      scopePath: propertyEditorScopePath,
      nodeId: nodeId,
      facet: facet,
    );
    refreshFromKernel();
    return result;
  }

  bool updateFacet(BigInt nodeId, BigInt index, APIFacet facet) {
    if (nodeNetworkView == null) return false;
    final result = facet_shell_api.updateFacet(
      scopePath: propertyEditorScopePath,
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
      scopePath: propertyEditorScopePath,
      nodeId: nodeId,
      index: index,
    );
    refreshFromKernel();
    return result;
  }

  bool clearFacets(BigInt nodeId) {
    if (nodeNetworkView == null) return false;
    final result = facet_shell_api.clearFacets(
        scopePath: propertyEditorScopePath, nodeId: nodeId);
    refreshFromKernel();
    return result;
  }

  bool selectFacet(BigInt nodeId, BigInt? index) {
    if (nodeNetworkView == null) return false;
    final result = facet_shell_api.selectFacet(
      scopePath: propertyEditorScopePath,
      nodeId: nodeId,
      index: index,
    );
    refreshFromKernel();
    return result;
  }

  bool splitSymmetryMembers(BigInt nodeId, BigInt facetIndex) {
    if (nodeNetworkView == null) return false;
    final result = facet_shell_api.splitSymmetryMembers(
      scopePath: propertyEditorScopePath,
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

  // --- Direct Editing Mode ---

  bool get canSwitchToDirectEditingMode =>
      structure_designer_api.canSwitchToDirectEditingMode();

  void switchToDirectEditingMode() {
    structure_designer_api.setDirectEditingMode(mode: true);
    refreshFromKernel();
  }

  void switchToNodeNetworkMode() {
    structure_designer_api.setDirectEditingMode(mode: false);
    refreshFromKernel();
  }

  /// Whether any network in the design has validation errors.
  bool get hasValidationErrors =>
      nodeNetworkNames.any((n) => n.validationErrors != null);

  /// Imports an XYZ file into the active atom_edit node's diff layer.
  /// Atoms and bonds are merged as pure additions (incremental import).
  /// Returns an empty string on success, or an error message on failure.
  String importXyzIntoAtomEdit(String filePath) {
    final result =
        structure_designer_api.importXyzIntoAtomEdit(filePath: filePath);
    refreshFromKernel();
    return result;
  }
}
