import 'package:flutter/material.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Per-frame caches keyed by full scope chain terminating at an HOF node id
/// (`chain ++ [hof_id]`). Populated by [ScopeResolver.runLayoutPass]; every
/// pin-position query reads from these maps in O(1).
///
/// In phase U3 bodies have no rendered content, so [bodySizes] is just the
/// stored size; U4 extends the bottom-up walk with `content_bbox` and the
/// max-with-stored rule from `doc/design_zones_ui.md` §"Body sizing".
class LayoutCache {
  /// HOF body size in logical pixels, keyed by full scope chain terminating
  /// at the HOF id.
  final Map<List<BigInt>, Size> bodySizes = {};

  /// Body inner-top-left in screen coordinates, same keying. Folds in
  /// `panOffset` and `scale`.
  final Map<List<BigInt>, Offset> bodyOrigins = {};

  Size? lookupSize(List<BigInt> bodyChain) {
    for (final entry in bodySizes.entries) {
      if (_listEq(entry.key, bodyChain)) return entry.value;
    }
    return null;
  }

  Offset? lookupOrigin(List<BigInt> bodyChain) {
    for (final entry in bodyOrigins.entries) {
      if (_listEq(entry.key, bodyChain)) return entry.value;
    }
    return null;
  }

  static bool _listEq(List<BigInt> a, List<BigInt> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
}

/// Resolves scope-aware coordinate and pin-position queries for the node
/// network editor. Created fresh per frame from the current [root] view,
/// [panOffset], [scale], and [zoomLevel].
///
/// Phase U3 brings two capabilities online:
/// 1. The [LayoutCache] (built by [runLayoutPass]) that all pin-position
///    queries read from. Today's bodies have no rendered content so the cache
///    is trivial — `bodySize == storedSize`. U4 extends the bottom-up walk
///    with `content_bbox`.
/// 2. [PinKind.zoneInput] / [PinKind.zoneOutput] handling in
///    [tryPinScreenPosition] so wires that touch an HOF's inner-edge pins
///    can render.
///
/// See `doc/design_zones_ui.md` §"Layout pass" and §"Pin position resolution".
class ScopeResolver {
  final NodeNetworkView root;
  final Offset panOffset;
  final double scale;
  final ZoomLevel zoomLevel;
  final LayoutCache layout = LayoutCache();

  ScopeResolver({
    required this.root,
    required this.panOffset,
    required this.scale,
    required this.zoomLevel,
  }) {
    runLayoutPass();
  }

  /// Populate [layout] from the current `root`, transform, and live node
  /// positions. Must be called before any pin-position or hit-test query.
  /// O(N) in the total number of nodes in the network.
  ///
  /// Phase U3 walks top-level nodes only — bodies don't render content yet,
  /// so nested HOFs contribute nothing to the cache. The bottom-up
  /// machinery is in place but trivial; U4 extends it with `content_bbox`.
  void runLayoutPass() {
    layout.bodySizes.clear();
    layout.bodyOrigins.clear();
    for (final node in root.nodes.values) {
      final zone = node.zone;
      if (zone == null) continue;
      final chain = [node.id];
      // In U3 the body always renders at its stored size; the
      // `max(stored, content_bbox + padding)` rule is wired up in U4 once
      // body nodes start appearing inside.
      layout.bodySizes[chain] = Size(zone.storedWidth, zone.storedHeight);
      // Body inner-top-left in screen coordinates.
      final hofPos = apiVec2ToOffset(node.position);
      final bodyTopLeftLogical = hofPos +
          const Offset(BASE_HOF_BODY_LEFT_OFFSET, BASE_HOF_BODY_TOP_OFFSET);
      layout.bodyOrigins[chain] =
          logicalToScreen(bodyTopLeftLogical, panOffset, scale);
    }
  }

  /// Map a body-local point at [scopeChain] to screen space. In phase U3
  /// `scopeChain` is always empty at call sites (body content isn't authored
  /// yet); U4+ will resolve nested-body positions via the layout cache.
  Offset scopedToScreen(List<BigInt> scopeChain, Offset bodyLocal) {
    if (scopeChain.isEmpty) {
      return logicalToScreen(bodyLocal, panOffset, scale);
    }
    final origin = layout.lookupOrigin(scopeChain);
    if (origin == null) {
      // Body not in cache (the chain points at a non-HOF or an unknown id).
      // Fall back to top-level transform so we degrade gracefully rather than
      // returning a bogus origin.
      return logicalToScreen(bodyLocal, panOffset, scale);
    }
    return origin + bodyLocal * scale;
  }

  /// Inverse of [scopedToScreen] for the top-level (empty) scope.
  Offset screenToScopedLocal(List<BigInt> scopeChain, Offset screen) {
    if (scopeChain.isEmpty) {
      return screenToLogical(screen, panOffset, scale);
    }
    final origin = layout.lookupOrigin(scopeChain);
    if (origin == null) return screenToLogical(screen, panOffset, scale);
    return (screen - origin) / scale;
  }

  /// Walk the screen point down through containing bodies; return the deepest
  /// scope chain that contains it and the body-local coordinate.
  ///
  /// Phase U3 deliberately keeps body interiors non-interactive (clicks fall
  /// through to the HOF chrome) — body authoring lands in U4. So this still
  /// returns the top-level scope. The shape is in place for U4 to drop body
  /// containment in without touching call sites.
  ({List<BigInt> scopeChain, Offset bodyLocal}) findContainingScope(
      Offset screenPos) {
    return (
      scopeChain: const <BigInt>[],
      bodyLocal: screenToLogical(screenPos, panOffset, scale),
    );
  }

  /// Find the node containing [screenPos] anywhere in the tree. Returns the
  /// node and the scope chain its containing network occupies.
  ///
  /// Phase U3 walks top-level nodes only. Clicks inside an HOF's body region
  /// resolve to the HOF itself (the body is read-only in U3 — see
  /// `doc/design_zones_ui.md` §"Phase U3" gotcha).
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
    switch (pin.pinKind) {
      case PinKind.externalInput:
        if (pin.pinIndex < 0 || pin.pinIndex >= node.inputPins.length) {
          return null;
        }
        break;
      case PinKind.zoneInput:
        final zone = node.zone;
        if (zone == null) return null;
        if (pin.pinIndex < 0 || pin.pinIndex >= zone.zoneInputPins.length) {
          return null;
        }
        break;
      case PinKind.zoneOutput:
        final zone = node.zone;
        if (zone == null) return null;
        if (pin.pinIndex < 0 || pin.pinIndex >= zone.zoneOutputPins.length) {
          return null;
        }
        break;
      case PinKind.externalOutput:
      case PinKind.functionPin:
        break;
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
    // In U3 body content isn't surfaced through the API (`ZoneView.nodes` is
    // omitted), so a pin with a non-empty scope chain doesn't address any
    // node visible to the editor. Phase U4 will walk `Node.zone.nodes`.
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
    final zone = node.zone;
    switch (pin.pinKind) {
      case PinKind.functionPin:
        // Function pin sits at the HOF's outer right edge for symmetry with
        // regular nodes; the HOF's overall width grows to include the body.
        final nodeWidth = zone != null
            ? BASE_HOF_BODY_LEFT_OFFSET +
                zone.storedWidth +
                BASE_HOF_BODY_RIGHT_GUTTER
            : NODE_WIDTH;
        final logicalPos =
            nodePos + Offset(nodeWidth, NODE_VERT_WIRE_OFFSET_FUNCTION_PIN);
        return (
          scopedToScreen(pin.scopeChain, logicalPos),
          node.functionType,
        );
      case PinKind.externalOutput:
        // External output pins live on the HOF's outer right edge — past the
        // body region. For non-HOFs the right edge is `NODE_WIDTH` as before.
        final nodeWidth = zone != null
            ? BASE_HOF_BODY_LEFT_OFFSET +
                zone.storedWidth +
                BASE_HOF_BODY_RIGHT_GUTTER
            : NODE_WIDTH;
        final vertOffset = NODE_VERT_WIRE_OFFSET +
            (pin.pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
        final String dataType;
        if (pin.pinIndex >= 0 && pin.pinIndex < node.outputPins.length) {
          dataType = node.outputPins[pin.pinIndex].effectiveDataType;
        } else {
          dataType = node.outputType;
        }
        final logicalPos = nodePos + Offset(nodeWidth, vertOffset);
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
      case PinKind.zoneInput:
        // Inner-left pin on the body region, facing into the body. The pin
        // index is its slot in `zone.zoneInputPins`.
        final z = zone!;
        final vertOffset = NODE_VERT_WIRE_OFFSET +
            (pin.pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
        final logicalPos =
            nodePos + Offset(BASE_HOF_BODY_LEFT_OFFSET, vertOffset);
        final dataType = pin.pinIndex < z.zoneInputPins.length
            ? z.zoneInputPins[pin.pinIndex].effectiveDataType
            : pin.dataType;
        return (scopedToScreen(pin.scopeChain, logicalPos), dataType);
      case PinKind.zoneOutput:
        // Inner-right pin on the body region. Reads stored body width from
        // the layout cache so wire rendering stays consistent with the
        // body rect drawn by the painter.
        final z = zone!;
        final bodySize =
            layout.lookupSize([node.id]) ?? Size(z.storedWidth, z.storedHeight);
        final vertOffset = NODE_VERT_WIRE_OFFSET +
            (pin.pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
        final logicalPos = nodePos +
            Offset(BASE_HOF_BODY_LEFT_OFFSET + bodySize.width, vertOffset);
        final dataType = pin.pinIndex < z.zoneOutputPins.length
            ? z.zoneOutputPins[pin.pinIndex].dataType
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
      case PinKind.zoneInput:
      case PinKind.zoneOutput:
        // Zoomed-out mode collapses HOFs to their chrome (per the design's
        // collapsed-body rule for far zoom). The inner-edge zone pins have
        // no individual position at this scale; route their endpoint to the
        // HOF's center so wires don't disappear at the screen origin.
        // (Body-internal wires aren't surfaced in U3 anyway.)
        final centerY = nodeScreenPos.dy + nodeSize.height / 2;
        final centerX = nodeScreenPos.dx + nodeSize.width / 2;
        final zone = node.zone;
        final String dataType;
        if (zone != null) {
          if (pin.pinKind == PinKind.zoneInput &&
              pin.pinIndex >= 0 &&
              pin.pinIndex < zone.zoneInputPins.length) {
            dataType = zone.zoneInputPins[pin.pinIndex].effectiveDataType;
          } else if (pin.pinKind == PinKind.zoneOutput &&
              pin.pinIndex >= 0 &&
              pin.pinIndex < zone.zoneOutputPins.length) {
            dataType = zone.zoneOutputPins[pin.pinIndex].dataType;
          } else {
            dataType = pin.dataType;
          }
        } else {
          dataType = pin.dataType;
        }
        return (Offset(centerX, centerY), dataType);
    }
  }
}
