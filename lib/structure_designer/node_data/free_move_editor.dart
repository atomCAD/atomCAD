import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/vec3_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for free_move nodes.
/// Allows editing the translation vector in world space (Cartesian coordinates).
class FreeMoveEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIFreeMoveData? data;
  final StructureDesignerModel model;

  const FreeMoveEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<FreeMoveEditor> createState() => FreeMoveEditorState();
}

class FreeMoveEditorState extends State<FreeMoveEditor> {
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
          const NodeEditorHeader(
            title: 'Free Move Properties',
            nodeTypeName: 'free_move',
          ),
          const SizedBox(height: 16),
          Vec3Input(
            label: 'Translation (Å)',
            value: widget.data!.translation,
            onChanged: (newValue) {
              widget.model.setFreeMoveData(
                widget.nodeId,
                APIFreeMoveData(
                  translation: newValue,
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
