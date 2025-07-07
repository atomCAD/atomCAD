import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/miller_index_map.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for half_space nodes
class HalfSpaceEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIHalfSpaceData? data;
  final StructureDesignerModel model;

  const HalfSpaceEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<HalfSpaceEditor> createState() => HalfSpaceEditorState();
}

class HalfSpaceEditorState extends State<HalfSpaceEditor> {
  // Direct API calls are made in onChanged handlers

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(4.0),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Half Space Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            // Miller Index Map (visualization)
            MillerIndexMap(
              label: 'Miller Index Map',
              value: widget.data!.millerIndex,
              onChanged: (newValue) {
                widget.model.setHalfSpaceData(
                  widget.nodeId,
                  APIHalfSpaceData(
                    millerIndex: newValue,
                    center: widget.data!.center,
                  ),
                );
              },
              maxValue: 4,
              mapWidth: 360,
              mapHeight: 180,
              dotColor: Theme.of(context).brightness == Brightness.dark
                  ? Colors.grey.shade600
                  : Colors.grey.shade400,
              selectedDotColor: Theme.of(context).colorScheme.primary,
            ),
            const SizedBox(height: 12),
            // Traditional numeric input for Miller Index
            IVec3Input(
              label: 'Miller Index (numeric)',
              value: widget.data!.millerIndex,
              minimumValue: APIIVec3(x: -4, y: -4, z: -4),
              maximumValue: APIIVec3(x: 4, y: 4, z: 4),
              onChanged: (newValue) {
                widget.model.setHalfSpaceData(
                  widget.nodeId,
                  APIHalfSpaceData(
                    millerIndex: newValue,
                    center: widget.data!.center,
                  ),
                );
              },
            ),
            const SizedBox(height: 8),
            IVec3Input(
              label: 'Center',
              value: widget.data!.center,
              onChanged: (newValue) {
                widget.model.setHalfSpaceData(
                  widget.nodeId,
                  APIHalfSpaceData(
                    millerIndex: widget.data!.millerIndex,
                    center: newValue,
                  ),
                );
              },
            ),
          ],
        ),
      ),
    );
  }
}
