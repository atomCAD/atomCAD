import 'package:flutter/material.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Resolves scope-aware coordinate and pin-position queries for the node
/// network editor. Created fresh per frame from the current [root] view,
/// [panOffset], [scale], and [zoomLevel] — cheap to construct because all
/// per-frame heavy lifting (the layout pass) lands in phase U3.
///
/// In phase U1 every scope chain is empty (there are no inline-zone bodies
/// authored yet), so [scopedToScreen] is equivalent to a top-level
/// `logicalToScreen` and [findContainingScope] always returns the top-level
/// scope. The scope-aware shape is in place so later phases can drop bodies in
/// without touching every call site. See `doc/design_zones_ui.md` §"Phase U1".
class ScopeResolver {
  final NodeNetworkView root;
  final Offset panOffset;
  final double scale;
  final ZoomLevel zoomLevel;

  ScopeResolver({
    required this.root,
    required this.panOffset,
    required this.scale,
    required this.zoomLevel,
  });

  /// Map a body-local point at [scopeChain] to screen space. In U1 the chain
  /// is always empty and this collapses to the top-level transform.
  Offset scopedToScreen(List<BigInt> scopeChain, Offset bodyLocal) {
    return logicalToScreen(bodyLocal, panOffset, scale);
  }

  /// Inverse of [scopedToScreen] for the top-level (empty) scope.
  Offset screenToScopedLocal(List<BigInt> scopeChain, Offset screen) {
    return screenToLogical(screen, panOffset, scale);
  }

  /// Walk the screen point down through containing bodies; return the deepest
  /// scope chain that contains it and the body-local coordinate. In U1
  /// (no bodies present) this always returns the top-level scope.
  ({List<BigInt> scopeChain, Offset bodyLocal}) findContainingScope(
      Offset screenPos) {
    return (
      scopeChain: const <BigInt>[],
      bodyLocal: screenToLogical(screenPos, panOffset, scale),
    );
  }

  /// Find the node containing [screenPos] anywhere in the tree. Returns the
  /// node and the scope chain its containing network occupies. In U1 walks
  /// top-level nodes only and returns an empty chain.
  ({List<BigInt> scopeChain, NodeView node})? findNodeAtScreenPosition(
      Offset screenPos) {
    final logicalPosition = screenToLogical(screenPos, panOffset, scale);
    for (final node in root.nodes.values) {
      Size logicalNodeSize;
      if (node.nodeTypeName == 'Comment') {
        logicalNodeSize =
            Size(node.commentWidth ?? 200.0, node.commentHeight ?? 100.0);
      } else {
        final nodeSize = getNodeSize(node, zoomLevel);
        logicalNodeSize =
            Size(nodeSize.width / scale, nodeSize.height / scale);
      }
      final nodeRect = apiVec2ToOffset(node.position) & logicalNodeSize;
      if (nodeRect.contains(logicalPosition)) {
        return (scopeChain: const <BigInt>[], node: node);
      }
    }
    return null;
  }

  /// True if [screenPos] lands on any node. Convenience wrapper over
  /// [findNodeAtScreenPosition] for hit tests that don't need the node.
  bool isPositionOnNode(Offset screenPos) =>
      findNodeAtScreenPosition(screenPos) != null;

  /// Resolve a pin to its on-screen position and the data type the pin
  /// actually carries (effective resolved type for output pins, declared type
  /// for input pins). Returns null when the node or pin index is no longer
  /// valid — callers tolerate this for wires that point at deleted nodes.
  (Offset, String)? tryPinScreenPosition(PinReference pin) {
    final node = _resolveNode(pin.scopeChain, pin.nodeId);
    if (node == null) return null;
    if (pin.pinKind == PinKind.externalInput) {
      if (pin.pinIndex < 0 || pin.pinIndex >= node.inputPins.length) {
        return null;
      }
    }
    return _pinScreenPosition(node, pin);
  }

  /// Like [tryPinScreenPosition] but asserts the pin resolves. Used by code
  /// paths that constructed the [PinReference] themselves from a live node.
  (Offset, String) pinScreenPosition(PinReference pin) {
    final result = tryPinScreenPosition(pin);
    if (result == null) {
      throw StateError('pinScreenPosition: unresolvable pin $pin');
    }
    return result;
  }

  NodeView? _resolveNode(List<BigInt> scopeChain, BigInt nodeId) {
    // In U1 the chain is always empty. Phase U3 onward will walk Node.zone
    // for each segment.
    if (scopeChain.isNotEmpty) return null;
    return root.nodes[nodeId];
  }

  (Offset, String) _pinScreenPosition(NodeView node, PinReference pin) {
    if (zoomLevel == ZoomLevel.normal) {
      return _pinPositionNormal(node, pin);
    } else {
      return _pinPositionZoomedOut(node, pin);
    }
  }

  (Offset, String) _pinPositionNormal(NodeView node, PinReference pin) {
    final nodePos = apiVec2ToOffset(node.position);
    switch (pin.pinKind) {
      case PinKind.functionPin:
        final logicalPos =
            nodePos + Offset(NODE_WIDTH, NODE_VERT_WIRE_OFFSET_FUNCTION_PIN);
        return (
          scopedToScreen(pin.scopeChain, logicalPos),
          node.functionType,
        );
      case PinKind.externalOutput:
        final vertOffset = NODE_VERT_WIRE_OFFSET +
            (pin.pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
        final String dataType;
        if (pin.pinIndex >= 0 && pin.pinIndex < node.outputPins.length) {
          dataType = node.outputPins[pin.pinIndex].effectiveDataType;
        } else {
          dataType = node.outputType;
        }
        final logicalPos = nodePos + Offset(NODE_WIDTH, vertOffset);
        return (scopedToScreen(pin.scopeChain, logicalPos), dataType);
      case PinKind.externalInput:
        final vertOffset = NODE_VERT_WIRE_OFFSET +
            (pin.pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
        final logicalPos = nodePos + Offset(0.0, vertOffset);
        final dataType =
            (pin.pinIndex >= 0 && pin.pinIndex < node.inputPins.length)
                ? node.inputPins[pin.pinIndex].dataType
                : pin.dataType;
        return (scopedToScreen(pin.scopeChain, logicalPos), dataType);
    }
  }

  (Offset, String) _pinPositionZoomedOut(NodeView node, PinReference pin) {
    final nodeSize = getNodeSize(node, zoomLevel);
    final nodeScreenPos =
        scopedToScreen(pin.scopeChain, apiVec2ToOffset(node.position));
    switch (pin.pinKind) {
      case PinKind.functionPin:
        // Legacy behaviour parity: in zoomed-out mode the function pin sits
        // at the right edge center and reports `outputType` (not
        // `functionType`). Preserved verbatim from the pre-refactor painter.
        final centerY = nodeScreenPos.dy + nodeSize.height / 2;
        return (
          Offset(nodeScreenPos.dx + nodeSize.width, centerY),
          node.outputType,
        );
      case PinKind.externalOutput:
        final rightEdgeX = nodeScreenPos.dx + nodeSize.width;
        final numOutputs = node.outputPins.length;
        if (numOutputs > 1 && pin.pinIndex >= 0) {
          final spacing = BASE_ZOOMED_OUT_PIN_SPACING * scale;
          final totalHeight = (numOutputs - 1) * spacing;
          final startY = nodeScreenPos.dy + (nodeSize.height - totalHeight) / 2;
          final outputY = startY + (pin.pinIndex * spacing);
          final dataType = pin.pinIndex < node.outputPins.length
              ? node.outputPins[pin.pinIndex].effectiveDataType
              : node.outputType;
          return (Offset(rightEdgeX, outputY), dataType);
        } else {
          final centerY = nodeScreenPos.dy + nodeSize.height / 2;
          return (Offset(rightEdgeX, centerY), node.outputType);
        }
      case PinKind.externalInput:
        final leftEdgeX = nodeScreenPos.dx;
        final numInputs = node.inputPins.length;
        final spacing = BASE_ZOOMED_OUT_PIN_SPACING * scale;
        final totalHeight = (numInputs - 1) * spacing;
        final startY = nodeScreenPos.dy + (nodeSize.height - totalHeight) / 2;
        final inputY = startY + (pin.pinIndex * spacing);
        final dataType =
            (pin.pinIndex >= 0 && pin.pinIndex < node.inputPins.length)
                ? node.inputPins[pin.pinIndex].dataType
                : pin.dataType;
        return (Offset(leftEdgeX, inputY), dataType);
    }
  }
}
