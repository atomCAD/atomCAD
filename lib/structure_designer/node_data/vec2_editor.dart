import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/vec2_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for vec2 nodes
class Vec2Editor extends StatefulWidget {
  final BigInt nodeId;
  final APIVec2Data? data;
  final StructureDesignerModel model;

  const Vec2Editor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<Vec2Editor> createState() => Vec2EditorState();
}

class Vec2EditorState extends State<Vec2Editor> {
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
          Text('Vec2 Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          Vec2Input(
            label: 'Value',
            value: widget.data!.value,
            onChanged: (newValue) {
              widget.model.setVec2Data(
                widget.nodeId,
                APIVec2Data(
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
