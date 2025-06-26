import 'package:flutter/material.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_widget.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';

class WireHitResult {
  final BigInt sourceNodeId;
  final BigInt destNodeId;
  final BigInt destParamIndex;

  WireHitResult(this.sourceNodeId, this.destNodeId, this.destParamIndex);
}

class WirePainter extends CustomPainter {
  final StructureDesignerModel graphModel;
  final Offset panOffset;

  WirePainter(this.graphModel, {this.panOffset = Offset.zero});

  @override
  void paint(Canvas canvas, Size size) {
    if (graphModel.nodeNetworkView == null) {
      return;
    }

    Paint paint = Paint()
      ..color = Colors.black
      ..strokeWidth = WIRE_WIDTH_NORMAL
      ..style = PaintingStyle.stroke;

    // Draw regular wires first
    for (var wire in graphModel.nodeNetworkView!.wires) {
      final source = _getPinPositionAndDataType(wire.sourceNodeId, -1);
      final dest = _getPinPositionAndDataType(
          wire.destNodeId, wire.destParamIndex.toInt());
      _drawWire(source.$1, dest.$1, canvas, paint, source.$2, wire.selected);
    }

    // Draw dragged wire on top
    if (graphModel.draggedWire != null) {
      final wireStart = _getPinPositionAndDataType(
          graphModel.draggedWire!.startPin.nodeId,
          graphModel.draggedWire!.startPin.pinIndex);
      final wireEndPos = graphModel.draggedWire!.wireEndPosition;
      if (graphModel.draggedWire!.startPin.pinIndex < 0) {
        // start is source
        _drawWire(wireStart.$1, wireEndPos, canvas, paint, wireStart.$2, false);
      } else {
        // start is dest
        _drawWire(wireEndPos, wireStart.$1, canvas, paint, wireStart.$2, false);
      }
    }
  }

  (Offset, String) _getPinPositionAndDataType(BigInt nodeId, int pinIndex) {
    // Now this is is a bit of a hacky solution.
    // We should probably use the real positions of the pin widgets instead of this logic to
    // approximate it independently.
    if (pinIndex < 0) {
      // output pin (source pin)
      final sourceNode = graphModel.nodeNetworkView!.nodes[nodeId];
      final sourceVertOffset = sourceNode!.inputPins.isEmpty
          ? NODE_VERT_WIRE_OFFSET_EMPTY
          : NODE_VERT_WIRE_OFFSET +
              sourceNode.inputPins.length *
                  NODE_VERT_WIRE_OFFSET_PER_PARAM *
                  0.5;
      return (
        APIVec2ToOffset(sourceNode.position) +
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
      final (sourcePos, _) = _getPinPositionAndDataType(wire.sourceNodeId, -1);
      final (destPos, _) = _getPinPositionAndDataType(
          wire.destNodeId, wire.destParamIndex.toInt());

      final hitTestPath = _getBand(sourcePos, destPos, HIT_TEST_WIRE_WIDTH);
      if (hitTestPath.contains(position)) {
        return WireHitResult(
            wire.sourceNodeId, wire.destNodeId, wire.destParamIndex);
      }
    }
    return null;
  }

  @override
  bool shouldRepaint(WirePainter oldDelegate) => true;
}
