import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/inputs/ivec2_input.dart';
import 'dart:math' show gcd;

/// Editor widget for half_plane nodes
class HalfPlaneEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIHalfPlaneData? data;
  final StructureDesignerModel model;

  const HalfPlaneEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<HalfPlaneEditor> createState() => HalfPlaneEditorState();
}

class HalfPlaneEditorState extends State<HalfPlaneEditor> {
  /// Calculate the Miller index from two points
  String _calculateMillerIndex(APIIVec2 point1, APIIVec2 point2) {
    // Calculate direction vector
    final dx = point2.x - point1.x;
    final dy = point2.y - point1.y;

    // Special case for zero vector
    if (dx == 0 && dy == 0) {
      return "(0, 0)";
    }

    // Find greatest common divisor
    int divisor = dx.abs().gcd(dy.abs());
    if (divisor == 0) {
      // Handle case where one component is zero
      divisor = dx.abs() + dy.abs();
    }

    // Simplify to smallest numbers
    final normalizedDx = dx ~/ divisor;
    final normalizedDy = dy ~/ divisor;

    return "($normalizedDx, $normalizedDy)";
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    // Calculate Miller index from the two points
    final millerIndexText =
        _calculateMillerIndex(widget.data!.point1, widget.data!.point2);

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Half Plane Properties',
            nodeTypeName: 'half_plane',
          ),
          const SizedBox(height: 16),
          IVec2Input(
            label: 'Point 1',
            value: widget.data!.point1,
            onChanged: (newValue) {
              widget.model.setHalfPlaneData(
                widget.nodeId,
                APIHalfPlaneData(
                  point1: newValue,
                  point2: widget.data!.point2,
                ),
              );
            },
          ),
          const SizedBox(height: 12),
          IVec2Input(
            label: 'Point 2',
            value: widget.data!.point2,
            onChanged: (newValue) {
              widget.model.setHalfPlaneData(
                widget.nodeId,
                APIHalfPlaneData(
                  point1: widget.data!.point1,
                  point2: newValue,
                ),
              );
            },
          ),
          const SizedBox(height: 16),
          Card(
            elevation: 1,
            child: Padding(
              padding: const EdgeInsets.all(12.0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('Calculated Properties',
                      style: Theme.of(context).textTheme.titleSmall),
                  const SizedBox(height: 8),
                  Row(
                    children: [
                      Text('Miller Index:',
                          style: Theme.of(context)
                              .textTheme
                              .bodyMedium
                              ?.copyWith(fontWeight: FontWeight.bold)),
                      const SizedBox(width: 8),
                      Text(millerIndexText,
                          style: Theme.of(context).textTheme.bodyMedium),
                    ],
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 16),
        ],
      ),
    );
  }
}
