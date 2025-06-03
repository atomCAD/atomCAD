import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/inputs/ivec2_input.dart';
import 'dart:math' show gcd;

/// Editor widget for half_plane nodes
class HalfPlaneEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIHalfPlaneData? data;

  const HalfPlaneEditor({
    super.key,
    required this.nodeId,
    required this.data,
  });

  @override
  State<HalfPlaneEditor> createState() => HalfPlaneEditorState();
}

class HalfPlaneEditorState extends State<HalfPlaneEditor> {
  APIHalfPlaneData? _stagedData;

  @override
  void initState() {
    super.initState();
    setState(() {
      _stagedData = widget.data;
    });
  }

  @override
  void didUpdateWidget(HalfPlaneEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.data != widget.data) {
      setState(() {
        _stagedData = widget.data;
      });
    }
  }

  void _updateStagedData(APIHalfPlaneData newData) {
    setState(() => _stagedData = newData);
  }

  void _applyChanges() {
    if (_stagedData != null) {
      setHalfPlaneData(
        nodeId: widget.nodeId,
        data: _stagedData!,
      );
      // No need to update _data here as it will be updated in the parent widget
    }
  }

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
    if (_stagedData == null) {
      return const Center(child: CircularProgressIndicator());
    }

    // Calculate Miller index from the two points
    final millerIndexText =
        _calculateMillerIndex(_stagedData!.point1, _stagedData!.point2);

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Half Plane Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 16),
            IVec2Input(
              label: 'Point 1',
              value: _stagedData!.point1,
              onChanged: (newValue) {
                _updateStagedData(APIHalfPlaneData(
                  point1: newValue,
                  point2: _stagedData!.point2,
                ));
              },
            ),
            const SizedBox(height: 12),
            IVec2Input(
              label: 'Point 2',
              value: _stagedData!.point2,
              onChanged: (newValue) {
                _updateStagedData(APIHalfPlaneData(
                  point1: _stagedData!.point1,
                  point2: newValue,
                ));
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
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: _stagedData != widget.data
                      ? () {
                          setState(() => _stagedData = widget.data);
                        }
                      : null,
                  child: const Text('Reset'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  onPressed: _stagedData != widget.data ? _applyChanges : null,
                  child: const Text('Apply'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
