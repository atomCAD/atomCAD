import 'package:flutter/material.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

import 'package:flutter_cad/structure_designer/node_network/node_network.dart';

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

  (Offset, String)? _tryGetPinPositionAndDataType(
      BigInt nodeId, PinType pinType, int pinIndex) {
    final view = graphModel.nodeNetworkView;
    if (view == null) {
      return null;
    }

    final node = view.nodes[nodeId];
    if (node == null) {
      return null;
    }

    if (pinType == PinType.input) {
      if (pinIndex < 0 || pinIndex >= node.inputPins.length) {
        return null;
      }
    }

    return _getPinPositionAndDataType(nodeId, pinType, pinIndex);
  }

  @override
  void paint(Canvas canvas, Size size) {
    if (graphModel.nodeNetworkView == null) {
      return;
    }

    // Draw grid first so it's behind everything else
    _drawGrid(canvas, size);

    Paint paint = Paint()
      ..color = Colors.black
      ..strokeWidth = WIRE_WIDTH_NORMAL
      ..style = PaintingStyle.stroke;

    // Draw regular wires first
    for (var wire in graphModel.nodeNetworkView!.wires) {
      final source = _tryGetPinPositionAndDataType(
          wire.sourceNodeId, PinType.output, wire.sourceOutputPinIndex);
      final dest = _tryGetPinPositionAndDataType(
          wire.destNodeId, PinType.input, wire.destParamIndex.toInt());

      if (source == null || dest == null) {
        continue;
      }

      final alignment = _getSourcePinAlignment(
          wire.sourceNodeId, wire.sourceOutputPinIndex);
      _drawWire(source.$1, dest.$1, canvas, paint, source.$2, wire.selected,
          alignment);
    }

    // Draw dragged wire on top
    if (graphModel.draggedWire != null) {
      final wireStart = _tryGetPinPositionAndDataType(
          graphModel.draggedWire!.startPin.nodeId,
          graphModel.draggedWire!.startPin.pinType,
          graphModel.draggedWire!.startPin.pinIndex);
      if (wireStart == null) {
        return;
      }
      final wireEndPos = graphModel.draggedWire!.wireEndPosition;
      final startPin = graphModel.draggedWire!.startPin;
      final alignment = startPin.pinType == PinType.output
          ? _getSourcePinAlignment(startPin.nodeId, startPin.pinIndex)
          : null;
      if (startPin.pinType == PinType.output) {
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

  /// Looks up the alignment carried by the given output pin on the source node.
  /// Returns `null` when the pin has no alignment (non-Blueprint/Crystal, or
  /// not yet evaluated) or when the pin index is the function pin (-1).
  APIAlignment? _getSourcePinAlignment(BigInt nodeId, int pinIndex) {
    if (pinIndex < 0) return null;
    final node = graphModel.nodeNetworkView?.nodes[nodeId];
    if (node == null || pinIndex >= node.outputPins.length) return null;
    return node.outputPins[pinIndex].alignment;
  }

  (Offset, String) _getPinPositionAndDataType(
      BigInt nodeId, PinType pinType, int pinIndex) {
    if (zoomLevel == ZoomLevel.normal) {
      return _getPinPositionNormal(nodeId, pinType, pinIndex);
    } else {
      return _getPinPositionZoomedOut(nodeId, pinType, pinIndex);
    }
  }

  /// Calculate pin position for normal zoom level with detailed pins
  (Offset, String) _getPinPositionNormal(
      BigInt nodeId, PinType pinType, int pinIndex) {
    final scale = getZoomScale(zoomLevel);

    if (pinType == PinType.output) {
      // output pin (source pin)
      final sourceNode = graphModel.nodeNetworkView!.nodes[nodeId]!;
      final double sourceVertOffset;
      final String dataType;
      if (pinIndex == -1) {
        // Function pin in title bar
        sourceVertOffset = NODE_VERT_WIRE_OFFSET_FUNCTION_PIN;
        dataType = sourceNode.functionType;
      } else {
        // Result output pin(s) — use same vertical spacing as input pins
        sourceVertOffset = NODE_VERT_WIRE_OFFSET +
            (pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
        // Get data type from the output pin definition; prefer the resolved
        // concrete type over the declared (possibly abstract) one for coloring.
        if (pinIndex < sourceNode.outputPins.length) {
          dataType = sourceNode.outputPins[pinIndex].effectiveDataType;
        } else {
          dataType = sourceNode.outputType;
        }
      }
      // Use central coordinate transformation
      final logicalPos = apiVec2ToOffset(sourceNode.position) +
          Offset(NODE_WIDTH, sourceVertOffset);
      return (logicalToScreen(logicalPos, panOffset, scale), dataType);
    } else {
      // input pin (dest pin)
      final destNode = graphModel.nodeNetworkView!.nodes[nodeId]!;
      final destVertOffset = NODE_VERT_WIRE_OFFSET +
          (pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
      // Use central coordinate transformation
      final logicalPos =
          apiVec2ToOffset(destNode.position) + Offset(0.0, destVertOffset);
      return (
        logicalToScreen(logicalPos, panOffset, scale),
        destNode.inputPins[pinIndex].dataType
      );
    }
  }

  /// Calculate pin position for zoomed-out mode with edge-based connections
  (Offset, String) _getPinPositionZoomedOut(
      BigInt nodeId, PinType pinType, int pinIndex) {
    final node = graphModel.nodeNetworkView!.nodes[nodeId]!;
    final scale = getZoomScale(zoomLevel);
    final nodeSize = getNodeSize(node, zoomLevel);
    // Use central coordinate transformation
    final nodePos =
        logicalToScreen(apiVec2ToOffset(node.position), panOffset, scale);

    if (pinType == PinType.output) {
      // Output wires connect to right edge
      final rightEdgeX = nodePos.dx + nodeSize.width;
      final numOutputs = node.outputPins.length;
      if (numOutputs > 1 && pinIndex >= 0) {
        // Multi-output: distribute output pins vertically
        final spacing = BASE_ZOOMED_OUT_PIN_SPACING * scale;
        final totalHeight = (numOutputs - 1) * spacing;
        final startY = nodePos.dy + (nodeSize.height - totalHeight) / 2;
        final outputY = startY + (pinIndex * spacing);
        final dataType = pinIndex < node.outputPins.length
            ? node.outputPins[pinIndex].effectiveDataType
            : node.outputType;
        return (Offset(rightEdgeX, outputY), dataType);
      } else {
        // Single output or function pin: centered vertically
        final centerY = nodePos.dy + nodeSize.height / 2;
        return (Offset(rightEdgeX, centerY), node.outputType);
      }
    } else {
      // Input wires connect to left edge with small vertical offset per input
      final leftEdgeX = nodePos.dx;
      final numInputs = node.inputPins.length;

      // Distribute input connections vertically with small spacing
      final spacing = BASE_ZOOMED_OUT_PIN_SPACING * scale;
      final totalHeight = (numInputs - 1) * spacing;
      final startY = nodePos.dy + (nodeSize.height - totalHeight) / 2;
      final inputY = startY + (pinIndex * spacing);

      return (Offset(leftEdgeX, inputY), node.inputPins[pinIndex].dataType);
    }
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
    if (graphModel.nodeNetworkView == null) return null;

    // We don't need to adjust the position here because _getPinPositionAndDataType
    // already adds the panOffset to the returned positions
    for (var wire in graphModel.nodeNetworkView!.wires) {
      final source = _tryGetPinPositionAndDataType(
          wire.sourceNodeId, PinType.output, wire.sourceOutputPinIndex);
      final dest = _tryGetPinPositionAndDataType(
          wire.destNodeId, PinType.input, wire.destParamIndex.toInt());

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
