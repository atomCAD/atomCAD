import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';

// Dash patterns for wires carrying unaligned Blueprint/Crystal values.
// Long-dash for motif-unaligned (softer warning), short-dash for lattice-unaligned
// (more visually fragmented = more broken). See doc/design_blueprint_alignment.md §6.1.
const double WIRE_DASH_MOTIF_UNALIGNED_ON = 10.0;
const double WIRE_DASH_MOTIF_UNALIGNED_OFF = 4.0;
const double WIRE_DASH_LATTICE_UNALIGNED_ON = 3.0;
const double WIRE_DASH_LATTICE_UNALIGNED_OFF = 3.0;

class WireHitResult {
  final BigInt sourceNodeId;
  final BigInt sourcePinIndex;
  final BigInt destNodeId;
  final BigInt destParamIndex;

  WireHitResult(this.sourceNodeId, this.sourcePinIndex, this.destNodeId,
      this.destParamIndex);
}

// Grid appearance constants
const double GRID_MAJOR_SPACING = 100.0;
const double GRID_MINOR_SPACING = 20.0;
const Color GRID_MAJOR_COLOR = Color(0xFFDDDDDD); // Light grey
const Color GRID_MINOR_COLOR = Color(0xFFEEEEEE); // Very light grey
const double GRID_MAJOR_LINE_WIDTH = 1.0;
const double GRID_MINOR_LINE_WIDTH = 1.0;

class NodeNetworkPainter extends CustomPainter {
  final StructureDesignerModel graphModel;
  final Offset panOffset;
  final ZoomLevel zoomLevel;

  NodeNetworkPainter(this.graphModel,
      {this.panOffset = Offset.zero, this.zoomLevel = ZoomLevel.normal});

  /// Build a [ScopeResolver] for the current frame. Returns null when no
  /// network is active. Constructed once per `paint` and once per
  /// `findWireAtPosition` call so the painter can be a CustomPainter (no
  /// per-frame mutable state held on the painter instance).
  ScopeResolver? _makeResolver() {
    final view = graphModel.nodeNetworkView;
    if (view == null) return null;
    return ScopeResolver(
      root: view,
      panOffset: panOffset,
      scale: getZoomScale(zoomLevel),
      zoomLevel: zoomLevel,
    );
  }

  /// Build a [PinReference] for the source endpoint of [wire], owner scope
  /// [scopeChain]. Function pins are encoded by pin index `-1`; everything
  /// else is a regular external output.
  PinReference _wireSourcePin(WireView wire, List<BigInt> scopeChain) {
    return PinReference(
      nodeId: wire.sourceNodeId,
      scopeChain: scopeChain,
      pinKind: wire.sourceOutputPinIndex == -1
          ? PinKind.functionPin
          : PinKind.externalOutput,
      pinIndex: wire.sourceOutputPinIndex,
      dataType: '',
    );
  }

  /// Build a [PinReference] for the destination endpoint of [wire].
  ///
  /// For body-return wires (`destination_argument_kind == ZoneOutput`), the
  /// dest is the HOF's zone-output pin: the HOF itself lives in the *parent*
  /// of [scopeChain] (since [scopeChain] is the body's scope, terminating at
  /// the HOF). For all other wires the dest is a regular `externalInput` in
  /// the same scope as the source.
  PinReference _wireDestPin(WireView wire, List<BigInt> scopeChain) {
    if (wire.destinationArgumentKind == APIArgumentKind.zoneOutput) {
      // The HOF's containing scope is scopeChain with its terminal element
      // (the HOF's own id) chopped off.
      final hofScope =
          scopeChain.isEmpty ? const <BigInt>[] : scopeChain.sublist(0, scopeChain.length - 1);
      return PinReference(
        nodeId: wire.destNodeId,
        scopeChain: hofScope,
        pinKind: PinKind.zoneOutput,
        pinIndex: wire.destParamIndex.toInt(),
        dataType: '',
      );
    }
    return PinReference(
      nodeId: wire.destNodeId,
      scopeChain: scopeChain,
      pinKind: PinKind.externalInput,
      pinIndex: wire.destParamIndex.toInt(),
      dataType: '',
    );
  }

  @override
  void paint(Canvas canvas, Size size) {
    final resolver = _makeResolver();
    if (resolver == null) return;

    // Draw grid first so it's behind everything else
    _drawGrid(canvas, size);

    Paint paint = Paint()
      ..color = Colors.black
      ..strokeWidth = WIRE_WIDTH_NORMAL
      ..style = PaintingStyle.stroke;

    // Draw wires for the top-level network, and recursively for every body
    // — the design specifies the outermost layer paints all wires so that
    // pin endpoint resolution can reach into any scope. See
    // `doc/design_zones_ui.md` §"Wire rendering across scopes".
    _drawWiresForNetwork(resolver, resolver.root, const <BigInt>[], canvas, paint);

    // Draw dragged wire on top
    if (graphModel.draggedWire != null) {
      final startPin = graphModel.draggedWire!.startPin;
      final wireStart = resolver.tryPinScreenPosition(startPin);
      if (wireStart == null) {
        return;
      }
      final wireEndPos = graphModel.draggedWire!.wireEndPosition;
      final alignment = startPin.isOutput
          ? _getSourcePinAlignment(startPin.nodeId, startPin.pinIndex)
          : null;
      if (startPin.isOutput) {
        // start is source
        _drawWire(wireStart.$1, wireEndPos, canvas, paint, wireStart.$2, false,
            alignment);
      } else {
        // start is dest
        _drawWire(wireEndPos, wireStart.$1, canvas, paint, wireStart.$2, false,
            alignment);
      }
    }
  }

  /// Recursively paint every wire reachable from the top-level network.
  /// Wires at body depth are painted using `pinScreenPosition` against the
  /// body's `scopeChain` so cross-frame positions resolve through the
  /// layout cache. See `doc/design_zones_ui.md` §"Wire rendering across scopes".
  void _drawWiresForNetwork(
    ScopeResolver resolver,
    NodeNetworkView network,
    List<BigInt> scopeChain,
    Canvas canvas,
    Paint paint,
  ) {
    _drawWiresAtScope(resolver, network.wires, scopeChain, canvas, paint);
    for (final node in network.nodes.values) {
      final zone = node.zone;
      if (zone == null) continue;
      final innerChain = [...scopeChain, node.id];
      _drawWiresInZone(resolver, zone, innerChain, canvas, paint);
    }
  }

  void _drawWiresInZone(
    ScopeResolver resolver,
    ZoneView zone,
    List<BigInt> scopeChain,
    Canvas canvas,
    Paint paint,
  ) {
    _drawWiresAtScope(resolver, zone.wires, scopeChain, canvas, paint);
    for (final node in zone.nodes.values) {
      final inner = node.zone;
      if (inner == null) continue;
      final innerChain = [...scopeChain, node.id];
      _drawWiresInZone(resolver, inner, innerChain, canvas, paint);
    }
  }

  void _drawWiresAtScope(
    ScopeResolver resolver,
    List<WireView> wires,
    List<BigInt> scopeChain,
    Canvas canvas,
    Paint paint,
  ) {
    for (var wire in wires) {
      final source =
          resolver.tryPinScreenPosition(_wireSourcePin(wire, scopeChain));
      final dest =
          resolver.tryPinScreenPosition(_wireDestPin(wire, scopeChain));
      if (source == null || dest == null) continue;
      // Alignment is only resolved for top-level nodes (no per-pin alignment
      // surfaced for body-scope evaluation in U4); body wires render solid.
      final alignment = scopeChain.isEmpty
          ? _getSourcePinAlignment(wire.sourceNodeId, wire.sourceOutputPinIndex)
          : null;
      _drawWire(source.$1, dest.$1, canvas, paint, source.$2, wire.selected,
          alignment);
    }
  }

  /// Looks up the alignment carried by the given output pin on the source node.
  /// Returns `null` when the pin has no alignment (non-Blueprint/Crystal, or
  /// not yet evaluated) or when the pin index is the function pin (-1).
  APIAlignment? _getSourcePinAlignment(BigInt nodeId, int pinIndex) {
    if (pinIndex < 0) return null;
    final node = graphModel.nodeNetworkView?.nodes[nodeId];
    if (node == null || pinIndex >= node.outputPins.length) return null;
    return node.outputPins[pinIndex].alignment;
  }

  _drawWire(Offset sourcePos, Offset destPos, Canvas canvas, Paint paint,
      String dataType, bool selected, APIAlignment? alignment) {
    paint.color = getDataTypeColor(dataType);
    paint.strokeWidth = selected ? WIRE_WIDTH_SELECTED : WIRE_WIDTH_NORMAL;

    final path = _getPath(sourcePos, destPos);

    if (selected) {
      paint.color = WIRE_COLOR_SELECTED;

      // Draw glow effect for selected wire (always solid — alignment dashes
      // would be imperceptible under the wider glow stroke).
      final glowPaint = Paint()
        ..color = WIRE_COLOR_SELECTED.withValues(alpha: WIRE_GLOW_OPACITY)
        ..strokeWidth = paint.strokeWidth * 2
        ..style = PaintingStyle.stroke;

      canvas.drawPath(path, glowPaint);
    }

    final dashPattern = _dashPatternFor(alignment);
    if (dashPattern == null) {
      canvas.drawPath(path, paint);
    } else {
      canvas.drawPath(_dashedPath(path, dashPattern.$1, dashPattern.$2), paint);
    }
  }

  /// Returns `(onLength, offLength)` for the given alignment, or `null` for a
  /// solid wire (Aligned / no alignment info).
  (double, double)? _dashPatternFor(APIAlignment? alignment) {
    switch (alignment) {
      case APIAlignment.motifUnaligned:
        return (WIRE_DASH_MOTIF_UNALIGNED_ON, WIRE_DASH_MOTIF_UNALIGNED_OFF);
      case APIAlignment.latticeUnaligned:
        return (
          WIRE_DASH_LATTICE_UNALIGNED_ON,
          WIRE_DASH_LATTICE_UNALIGNED_OFF
        );
      case APIAlignment.aligned:
      case null:
        return null;
    }
  }

  /// Extracts a dashed sub-path of the Bezier by walking its arc-length via
  /// `PathMetric`, alternating `on` and `off` segments. No dependencies.
  Path _dashedPath(Path source, double on, double off) {
    final result = Path();
    for (final metric in source.computeMetrics()) {
      double distance = 0.0;
      bool draw = true;
      while (distance < metric.length) {
        final next =
            (distance + (draw ? on : off)).clamp(0.0, metric.length).toDouble();
        if (draw) {
          result.addPath(metric.extractPath(distance, next), Offset.zero);
        }
        distance = next;
        draw = !draw;
      }
    }
    return result;
  }

  Path _getPath(Offset sourcePos, Offset destPos) {
    final controlPoint1 = sourcePos + Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);
    final controlPoint2 = destPos - Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);

    return Path()
      ..moveTo(sourcePos.dx, sourcePos.dy)
      ..cubicTo(
        controlPoint1.dx,
        controlPoint1.dy,
        controlPoint2.dx,
        controlPoint2.dy,
        destPos.dx,
        destPos.dy,
      );
  }

  Path _getBand(Offset sourcePos, Offset destPos, double width) {
    final hw = width * 0.5;
    final off = destPos.dx > sourcePos.dx ? width : (-width);

    final sourcePos1 = Offset(sourcePos.dx, sourcePos.dy + hw);
    final sourcePos2 = Offset(sourcePos.dx, sourcePos.dy - hw);
    final destPos1 = Offset(destPos.dx, destPos.dy + hw);
    final destPos2 = Offset(destPos.dx, destPos.dy - hw);

    final controlPointStart1 =
        sourcePos1 + Offset(CUBIC_SPLINE_HORIZ_OFFSET - off, 0);
    final controlPointEnd1 = destPos1 - Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);

    final controlPointStart2 =
        sourcePos2 + Offset(CUBIC_SPLINE_HORIZ_OFFSET, 0);
    final controlPointEnd2 =
        destPos2 - Offset(CUBIC_SPLINE_HORIZ_OFFSET - off, 0);

    return Path()
      ..moveTo(sourcePos1.dx, sourcePos1.dy)
      ..cubicTo(
        controlPointStart1.dx,
        controlPointStart1.dy,
        controlPointEnd1.dx,
        controlPointEnd1.dy,
        destPos1.dx,
        destPos1.dy,
      )
      ..lineTo(destPos2.dx, destPos2.dy)
      ..cubicTo(
        controlPointEnd2.dx,
        controlPointEnd2.dy,
        controlPointStart2.dx,
        controlPointStart2.dy,
        sourcePos2.dx,
        sourcePos2.dy,
      )
      ..close();
  }

  WireHitResult? findWireAtPosition(Offset position) {
    final resolver = _makeResolver();
    if (resolver == null) return null;

    // `position` is already in screen coordinates and the resolver returns
    // pin endpoints in screen coordinates, so no further transform is needed.
    // Top-level wires only — U4 doesn't surface body-wire selection via the
    // interaction-layer click. (Body wire selection from a click on the wire
    // is a U5 polish item.)
    for (var wire in resolver.root.wires) {
      final source =
          resolver.tryPinScreenPosition(_wireSourcePin(wire, const <BigInt>[]));
      final dest =
          resolver.tryPinScreenPosition(_wireDestPin(wire, const <BigInt>[]));

      if (source == null || dest == null) {
        continue;
      }

      final (sourcePos, _) = source;
      final (destPos, _) = dest;

      final hitTestPath = _getBand(sourcePos, destPos, HIT_TEST_WIRE_WIDTH);
      if (hitTestPath.contains(position)) {
        return WireHitResult(
            wire.sourceNodeId,
            BigInt.from(wire.sourceOutputPinIndex),
            wire.destNodeId,
            wire.destParamIndex);
      }
    }
    return null;
  }

  /// Draw a grid pattern that scales with zoom level
  void _drawGrid(Canvas canvas, Size size) {
    final scale = getZoomScale(zoomLevel);
    final Rect visibleRect = Offset.zero & size;

    // Apply clipping to prevent drawing outside the widget area
    canvas.clipRect(visibleRect);

    // At zoomed-out levels, only show major grid lines with minor color
    final bool showMinorLines = (zoomLevel == ZoomLevel.normal);

    // Create paints for grid lines
    final minorPaint = Paint()
      ..color = GRID_MINOR_COLOR
      ..strokeWidth = GRID_MINOR_LINE_WIDTH
      ..style = PaintingStyle.stroke;

    final majorPaint = Paint()
      ..color = showMinorLines ? GRID_MAJOR_COLOR : GRID_MINOR_COLOR
      ..strokeWidth =
          showMinorLines ? GRID_MAJOR_LINE_WIDTH : GRID_MINOR_LINE_WIDTH
      ..style = PaintingStyle.stroke;

    // Convert visible screen rect to logical coordinates
    final logicalTopLeft =
        screenToLogical(visibleRect.topLeft, panOffset, scale);
    final logicalBottomRight =
        screenToLogical(visibleRect.bottomRight, panOffset, scale);

    // Calculate grid line positions in logical space
    final gridSpacing =
        showMinorLines ? GRID_MINOR_SPACING : GRID_MAJOR_SPACING;
    final startX = (logicalTopLeft.dx / gridSpacing).floor() * gridSpacing;
    final startY = (logicalTopLeft.dy / gridSpacing).floor() * gridSpacing;

    // Draw vertical grid lines
    for (double logicalX = startX;
        logicalX <= logicalBottomRight.dx;
        logicalX += gridSpacing) {
      final screenX = logicalToScreen(Offset(logicalX, 0), panOffset, scale).dx;

      // Check if this is a major grid line
      final isMajor =
          (logicalX / GRID_MAJOR_SPACING).round() * GRID_MAJOR_SPACING ==
              logicalX;

      // In normal mode, draw both minor and major. In zoomed-out mode, only major
      final shouldDraw = showMinorLines || isMajor;

      if (shouldDraw) {
        canvas.drawLine(
            Offset(screenX, visibleRect.top),
            Offset(screenX, visibleRect.bottom),
            isMajor ? majorPaint : minorPaint);
      }
    }

    // Draw horizontal grid lines
    for (double logicalY = startY;
        logicalY <= logicalBottomRight.dy;
        logicalY += gridSpacing) {
      final screenY = logicalToScreen(Offset(0, logicalY), panOffset, scale).dy;

      // Check if this is a major grid line
      final isMajor =
          (logicalY / GRID_MAJOR_SPACING).round() * GRID_MAJOR_SPACING ==
              logicalY;

      // In normal mode, draw both minor and major. In zoomed-out mode, only major
      final shouldDraw = showMinorLines || isMajor;

      if (shouldDraw) {
        canvas.drawLine(
            Offset(visibleRect.left, screenY),
            Offset(visibleRect.right, screenY),
            isMajor ? majorPaint : minorPaint);
      }
    }
  }

  @override
  bool shouldRepaint(NodeNetworkPainter oldDelegate) => true;
}
