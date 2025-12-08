import 'package:flutter/material.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_widget.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';

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
      final source = _getPinPositionAndDataType(
          wire.sourceNodeId, PinType.output, wire.sourceOutputPinIndex);
      final dest = _getPinPositionAndDataType(
          wire.destNodeId, PinType.input, wire.destParamIndex.toInt());
      _drawWire(source.$1, dest.$1, canvas, paint, source.$2, wire.selected);
    }

    // Draw dragged wire on top
    if (graphModel.draggedWire != null) {
      final wireStart = _getPinPositionAndDataType(
          graphModel.draggedWire!.startPin.nodeId,
          graphModel.draggedWire!.startPin.pinType,
          graphModel.draggedWire!.startPin.pinIndex);
      final wireEndPos = graphModel.draggedWire!.wireEndPosition;
      if (graphModel.draggedWire!.startPin.pinType == PinType.output) {
        // start is source
        _drawWire(wireStart.$1, wireEndPos, canvas, paint, wireStart.$2, false);
      } else {
        // start is dest
        _drawWire(wireEndPos, wireStart.$1, canvas, paint, wireStart.$2, false);
      }
    }
  }

  (Offset, String) _getPinPositionAndDataType(
      BigInt nodeId, PinType pinType, int pinIndex) {
    // Now this is is a bit of a hacky solution.
    // We should probably use the real positions of the pin widgets instead of this logic to
    // approximate it independently.
    if (pinType == PinType.output) {
      // output pin (source pin)
      final sourceNode = graphModel.nodeNetworkView!.nodes[nodeId];
      final sourceVertOffset = (pinIndex == -1)
          ? NODE_VERT_WIRE_OFFSET_FUNCTION_PIN
          : (sourceNode!.inputPins.isEmpty
              ? NODE_VERT_WIRE_OFFSET_EMPTY
              : NODE_VERT_WIRE_OFFSET +
                  sourceNode.inputPins.length *
                      NODE_VERT_WIRE_OFFSET_PER_PARAM *
                      0.5);
      return (
        APIVec2ToOffset(sourceNode!.position) +
            Offset(NODE_WIDTH, sourceVertOffset) +
            panOffset,
        sourceNode.outputType
      );
    } else {
      // input pin (dest pin)
      final destNode = graphModel.nodeNetworkView!.nodes[nodeId];
      final destVertOffset = NODE_VERT_WIRE_OFFSET +
          (pinIndex.toDouble() + 0.5) * NODE_VERT_WIRE_OFFSET_PER_PARAM;
      return (
        APIVec2ToOffset(destNode!.position) +
            Offset(0.0, destVertOffset) +
            panOffset,
        destNode.inputPins[pinIndex].dataType
      );
    }
  }

  _drawWire(Offset sourcePos, Offset destPos, Canvas canvas, Paint paint,
      String dataType, bool selected) {
    paint.color = getDataTypeColor(dataType);
    paint.strokeWidth = selected ? WIRE_WIDTH_SELECTED : WIRE_WIDTH_NORMAL;

    if (selected) {
      paint.color = WIRE_COLOR_SELECTED;

      // Draw glow effect for selected wire
      final glowPaint = Paint()
        ..color = WIRE_COLOR_SELECTED.withOpacity(WIRE_GLOW_OPACITY)
        ..strokeWidth = paint.strokeWidth * 2
        ..style = PaintingStyle.stroke;

      canvas.drawPath(_getPath(sourcePos, destPos), glowPaint);
    }

    canvas.drawPath(_getPath(sourcePos, destPos), paint);
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
      final (sourcePos, _) = _getPinPositionAndDataType(
          wire.sourceNodeId, PinType.output, wire.sourceOutputPinIndex);
      final (destPos, _) = _getPinPositionAndDataType(
          wire.destNodeId, PinType.input, wire.destParamIndex.toInt());

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

  /// Draw a grid pattern that respects the pan offset
  void _drawGrid(Canvas canvas, Size size) {
    // Calculate grid boundaries based on visible area
    final Rect visibleRect = Offset.zero & size;

    // Apply clipping to prevent drawing outside the widget area
    canvas.clipRect(visibleRect);

    // Calculate the grid lines starting points based on pan offset
    // Ensure grid appears fixed to the world, not to the view
    double startX =
        ((visibleRect.left - panOffset.dx) / GRID_MINOR_SPACING).floor() *
                GRID_MINOR_SPACING +
            panOffset.dx;
    double startY =
        ((visibleRect.top - panOffset.dy) / GRID_MINOR_SPACING).floor() *
                GRID_MINOR_SPACING +
            panOffset.dy;
    double endX = visibleRect.right;
    double endY = visibleRect.bottom;

    // Create paints for major and minor grid lines
    final minorPaint = Paint()
      ..color = GRID_MINOR_COLOR
      ..strokeWidth = GRID_MINOR_LINE_WIDTH
      ..style = PaintingStyle.stroke;

    final majorPaint = Paint()
      ..color = GRID_MAJOR_COLOR
      ..strokeWidth = GRID_MAJOR_LINE_WIDTH
      ..style = PaintingStyle.stroke;

    // Draw vertical grid lines
    for (double x = startX; x <= endX; x += GRID_MINOR_SPACING) {
      // Determine if this is a major grid line
      bool isMajor = (((x - panOffset.dx) / GRID_MAJOR_SPACING).round() *
                      GRID_MAJOR_SPACING +
                  panOffset.dx -
                  x)
              .abs() <
          0.5;

      canvas.drawLine(Offset(x, visibleRect.top), Offset(x, visibleRect.bottom),
          isMajor ? majorPaint : minorPaint);
    }

    // Draw horizontal grid lines
    for (double y = startY; y <= endY; y += GRID_MINOR_SPACING) {
      // Determine if this is a major grid line
      bool isMajor = (((y - panOffset.dy) / GRID_MAJOR_SPACING).round() *
                      GRID_MAJOR_SPACING +
                  panOffset.dy -
                  y)
              .abs() <
          0.5;

      canvas.drawLine(Offset(visibleRect.left, y), Offset(visibleRect.right, y),
          isMajor ? majorPaint : minorPaint);
    }
  }

  @override
  bool shouldRepaint(NodeNetworkPainter oldDelegate) => true;
}
