import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for ivec3 nodes
class IVec3Editor extends StatefulWidget {
  final BigInt nodeId;
  final APIIVec3Data? data;
  final StructureDesignerModel model;

  const IVec3Editor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<IVec3Editor> createState() => IVec3EditorState();
}

class IVec3EditorState extends State<IVec3Editor> {
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
          const NodeEditorHeader(
            title: 'IVec3 Properties',
            nodeTypeName: 'ivec3',
          ),
          const SizedBox(height: 8),
          IVec3Input(
            label: 'Value',
            value: widget.data!.value,
            onChanged: (newValue) {
              widget.model.setIvec3Data(
                widget.nodeId,
                APIIVec3Data(
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
