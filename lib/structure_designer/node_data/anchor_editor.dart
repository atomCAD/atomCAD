import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Editor widget for anchor nodes
class AnchorEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIAnchorData? data;

  const AnchorEditor({
    super.key,
    required this.nodeId,
    required this.data,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Anchor position', 
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          _buildAnchorPositionDisplay(),
        ],
      ),
    );
  }

  Widget _buildAnchorPositionDisplay() {
    if (data == null || data!.position == null) {
      return const Padding(
        padding: EdgeInsets.symmetric(vertical: 8.0),
        child: Text('No anchor selected yet.'),
      );
    }

    // Convert coordinates from crystal lattice units (4x cubic cell units) to fractional values
    final position = data!.position!;
    final x = position.x / 4.0;
    final y = position.y / 4.0;
    final z = position.z / 4.0;

    return Card(
      elevation: 1,
      child: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.spaceAround,
          children: [
            _buildCoordinateDisplay('X', x),
            _buildCoordinateDisplay('Y', y),
            _buildCoordinateDisplay('Z', z),
          ],
        ),
      ),
    );
  }

  Widget _buildCoordinateDisplay(String label, double value) {
    return Column(
      children: [
        Text(label, style: const TextStyle(fontWeight: FontWeight.bold)),
        const SizedBox(height: 4),
        Text(value.toStringAsFixed(2)),
      ],
    );
  }
}
