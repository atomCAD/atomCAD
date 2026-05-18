import 'package:flutter/material.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/node_widget.dart'
    show PIN_HIT_AREA_WIDTH;
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
  /// Two phases:
  /// 1. **Bottom-up sizes.** For each HOF reached, recurse into its body
  ///    first so inner body sizes are known on return. Compute `content_bbox`
  ///    as the union of every body node's rect in body-local coordinates.
  ///    Then `bodySizes[chain] = max(stored, content_bbox + padding)`.
  /// 2. **Top-down origins.** Walk down the tree using known body sizes to
  ///    place each body's screen-space origin. The top-level frame's origin
  ///    is computed via `logicalToScreen`; nested bodies' origins are the
  ///    parent body's origin plus the HOF's position-in-parent times scale.
  ///
  /// See `doc/design_zones_ui.md` §"Layout pass — bottom-up sizes, top-down
  /// origins".
  void runLayoutPass() {
    layout.bodySizes.clear();
    layout.bodyOrigins.clear();
    // Phase 1: bottom-up sizes.
    for (final node in root.nodes.values) {
      final zone = node.zone;
      if (zone == null) continue;
      _computeBodySize(zone, [node.id]);
    }
    // Phase 2: top-down origins, starting from the top-level scope.
    for (final node in root.nodes.values) {
      final zone = node.zone;
      if (zone == null) continue;
      final chain = [node.id];
      final hofPos = apiVec2ToOffset(node.position);
      final bodyTopLeftLogical = hofPos +
          const Offset(BASE_HOF_BODY_LEFT_OFFSET, BASE_HOF_BODY_TOP_OFFSET);
      final origin = logicalToScreen(bodyTopLeftLogical, panOffset, scale);
      layout.bodyOrigins[chain] = origin;
      _placeChildBodies(zone, chain, origin);
    }
  }

  /// Recursive bottom-up size computation. After this returns, `layout
  /// .bodySizes[chain]` holds the body's rendered size = `max(stored,
  /// content_bbox + padding)`. Recurses into nested HOF bodies first so
  /// their sizes contribute to the outer body's `content_bbox`.
  void _computeBodySize(ZoneView zone, List<BigInt> chain) {
    // Recurse first so inner sizes feed the outer content_bbox via
    // getNodeSize (which reads the HOF body's stored size in U4 — bodies
    // grow but the HOF widget footprint follows by way of getNodeSize using
    // the cached body size on the next pass; for simplicity we use the
    // node's own getNodeSize which derives from stored_width/height).
    for (final innerNode in zone.nodes.values) {
      final innerZone = innerNode.zone;
      if (innerZone == null) continue;
      _computeBodySize(innerZone, [...chain, innerNode.id]);
    }

    double maxRight = 0;
    double maxBottom = 0;
    for (final node in zone.nodes.values) {
      final pos = apiVec2ToOffset(node.position);
      final size = getNodeSize(node, zoomLevel);
      // size is in *screen* coordinates; convert to body-local by /scale.
      final logicalSize = Size(size.width / scale, size.height / scale);
      final right = pos.dx + logicalSize.width;
      final bottom = pos.dy + logicalSize.height;
      if (right > maxRight) maxRight = right;
      if (bottom > maxBottom) maxBottom = bottom;
    }
    const padding = BASE_HOF_BODY_BOTTOM_PADDING;
    final contentWidth = maxRight + padding;
    final contentHeight = maxBottom + padding;
    final width = contentWidth > zone.storedWidth ? contentWidth : zone.storedWidth;
    final height =
        contentHeight > zone.storedHeight ? contentHeight : zone.storedHeight;
    layout.bodySizes[chain] = Size(width, height);
  }

  /// Recursive top-down origin placement for child bodies. The parent body's
  /// origin is [parentOrigin] (screen coords); each child body's origin is
  /// `parentOrigin + (hof.position + body-inner-offset) * scale`.
  void _placeChildBodies(
      ZoneView parent, List<BigInt> parentChain, Offset parentOrigin) {
    for (final innerNode in parent.nodes.values) {
      final innerZone = innerNode.zone;
      if (innerZone == null) continue;
      final innerChain = [...parentChain, innerNode.id];
      final hofPos = apiVec2ToOffset(innerNode.position);
      final bodyTopLeftLocal = hofPos +
          const Offset(BASE_HOF_BODY_LEFT_OFFSET, BASE_HOF_BODY_TOP_OFFSET);
      final origin = parentOrigin + bodyTopLeftLocal * scale;
      layout.bodyOrigins[innerChain] = origin;
      _placeChildBodies(innerZone, innerChain, origin);
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
  /// A click inside an HOF's body interior (not on a body node, not on the
  /// HOF's chrome) returns that body's scope. Used by right-click → Add Node
  /// and by the active-body click handler. See `doc/design_zones_ui.md`
  /// §"Coordinate system — Screen → logical".
  ({List<BigInt> scopeChain, Offset bodyLocal}) findContainingScope(
      Offset screenPos) {
    // Walk top-level nodes for any HOF whose body contains the point, then
    // recurse. Deepest containment wins.
    final result = _findContainingScopeIn(
      root.nodes.values,
      const <BigInt>[],
      screenPos,
    );
    if (result != null) return result;
    return (
      scopeChain: const <BigInt>[],
      bodyLocal: screenToLogical(screenPos, panOffset, scale),
    );
  }

  ({List<BigInt> scopeChain, Offset bodyLocal})? _findContainingScopeIn(
    Iterable<NodeView> nodes,
    List<BigInt> scopeChain,
    Offset screenPos,
  ) {
    for (final node in nodes) {
      final zone = node.zone;
      if (zone == null) continue;
      final bodyChain = [...scopeChain, node.id];
      final bodyOrigin = layout.lookupOrigin(bodyChain);
      final bodySize = layout.lookupSize(bodyChain);
      if (bodyOrigin == null || bodySize == null) continue;
      final bodyRect = bodyOrigin & (bodySize * scale);
      if (!bodyRect.contains(screenPos)) continue;
      // Try deeper first — nested bodies trump the outer one.
      final inner = _findContainingScopeIn(
        zone.nodes.values,
        bodyChain,
        screenPos,
      );
      if (inner != null) return inner;
      final bodyLocal = (screenPos - bodyOrigin) / scale;
      return (scopeChain: bodyChain, bodyLocal: bodyLocal);
    }
    return null;
  }

  /// Find the node containing [screenPos] anywhere in the tree. Returns the
  /// node and the scope chain its containing network occupies.
  ///
  /// U4: walks into bodies. The deepest containing node wins — a click on a
  /// body node returns that node, not its containing HOF.
  ({List<BigInt> scopeChain, NodeView node})? findNodeAtScreenPosition(
      Offset screenPos) {
    return _findNodeAt(root.nodes.values, const <BigInt>[], screenPos);
  }

  ({List<BigInt> scopeChain, NodeView node})? _findNodeAt(
    Iterable<NodeView> nodes,
    List<BigInt> scopeChain,
    Offset screenPos,
  ) {
    // First recurse into any HOF bodies — body content layers on top of its
    // HOF, so deeper hits win.
    for (final node in nodes) {
      final zone = node.zone;
      if (zone == null) continue;
      final bodyChain = [...scopeChain, node.id];
      final hit = _findNodeAt(zone.nodes.values, bodyChain, screenPos);
      if (hit != null) return hit;
    }
    // Then check nodes at this scope.
    for (final node in nodes) {
      final nodeScreenPos =
          scopedToScreen(scopeChain, apiVec2ToOffset(node.position));
      Size screenSize;
      if (node.nodeTypeName == 'Comment') {
        screenSize = Size(
          (node.commentWidth ?? 200.0) * scale,
          (node.commentHeight ?? 100.0) * scale,
        );
      } else {
        screenSize = getNodeSize(node, zoomLevel);
      }
      final nodeRect = nodeScreenPos & screenSize;
      if (!nodeRect.contains(screenPos)) continue;
      // Special-case HOF nodes: clicks landing inside the body region are
      // *not* on the HOF itself — they're on (the body's empty interior of)
      // the body, which is logically empty space at that scope. Returning
      // the HOF here would short-circuit right-click → Add Node and the
      // body-empty-space click-to-activate handler. Body nodes are handled
      // by the recursion above, so by the time we reach this branch we know
      // no body node was hit.
      if (node.zone != null) {
        final bodyChain = [...scopeChain, node.id];
        final bodyOrigin = layout.lookupOrigin(bodyChain);
        final bodySize = layout.lookupSize(bodyChain);
        if (bodyOrigin != null && bodySize != null) {
          final bodyRect = bodyOrigin & (bodySize * scale);
          if (bodyRect.contains(screenPos)) {
            // Click is in body empty space — fall through to the next node
            // (the HOF is reported as "not hit"). Caller's right-click
            // handler will then see this as empty space and open Add Node
            // popup parameterized by the body's scope.
            continue;
          }
        }
      }
      return (scopeChain: scopeChain, node: node);
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
    if (scopeChain.isEmpty) {
      return root.nodes[nodeId];
    }
    // Walk down the scope chain into nested HOF bodies. Returns null if any
    // step fails (the chain references a missing or non-HOF node) — wires
    // that point at deleted nodes degrade gracefully.
    Map<BigInt, NodeView> currentNodes = root.nodes;
    for (final hofId in scopeChain) {
      final hof = currentNodes[hofId];
      final zone = hof?.zone;
      if (zone == null) return null;
      currentNodes = zone.nodes;
    }
    return currentNodes[nodeId];
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
        // widget is positioned at `left: 0` inside the body Container, so its
        // CENTER (where the wire endpoint sits) is at body's left edge +
        // PIN_HIT_AREA_WIDTH/2. This is the same inset on the other axis as
        // a regular input pin on a node, just on the inside-facing edge.
        final z = zone!;
        final vertOffset = NODE_VERT_WIRE_OFFSET +
            (pin.pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
        final logicalPos = nodePos +
            Offset(BASE_HOF_BODY_LEFT_OFFSET + PIN_HIT_AREA_WIDTH / 2,
                vertOffset);
        final dataType = pin.pinIndex < z.zoneInputPins.length
            ? z.zoneInputPins[pin.pinIndex].effectiveDataType
            : pin.dataType;
        return (scopedToScreen(pin.scopeChain, logicalPos), dataType);
      case PinKind.zoneOutput:
        // Inner-right pin on the body region, positioned at `right: 0`
        // inside the body Container, so its CENTER is at body's right edge
        // − PIN_HIT_AREA_WIDTH/2. Reads body size from the layout cache
        // (rebuilt every frame via `runLayoutPass` — incorporates
        // content_bbox and live-drag positions) so wire rendering stays
        // consistent with the body rect drawn by the painter.
        final z = zone!;
        final bodySize = layout.lookupSize([...pin.scopeChain, node.id]) ??
            Size(z.storedWidth, z.storedHeight);
        final vertOffset = NODE_VERT_WIRE_OFFSET +
            (pin.pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
        final logicalPos = nodePos +
            Offset(
                BASE_HOF_BODY_LEFT_OFFSET +
                    bodySize.width -
                    PIN_HIT_AREA_WIDTH / 2,
                vertOffset);
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
