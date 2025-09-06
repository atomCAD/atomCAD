import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/vec3_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for vec3 nodes
class Vec3Editor extends StatefulWidget {
  final BigInt nodeId;
  final APIVec3Data? data;
  final StructureDesignerModel model;

  const Vec3Editor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<Vec3Editor> createState() => Vec3EditorState();
}

class Vec3EditorState extends State<Vec3Editor> {
  // Direct API calls are made in onChanged handlers

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Vec3 Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          Vec3Input(
            label: 'Value',
            value: widget.data!.value,
            onChanged: (newValue) {
              widget.model.setVec3Data(
                widget.nodeId,
                APIVec3Data(
                  value: newValue,
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
