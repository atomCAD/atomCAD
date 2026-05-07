import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for foreach nodes
class ForeachEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIForeachData? data;
  final StructureDesignerModel model;

  const ForeachEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ForeachEditor> createState() => ForeachEditorState();
}

class ForeachEditorState extends State<ForeachEditor> {
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
            title: 'Foreach Properties',
            nodeTypeName: 'foreach',
          ),
          const SizedBox(height: 8),

          // Input Type
          DataTypeInput(
            label: 'Input Type',
            value: widget.data!.inputType,
            onChanged: (newValue) {
              widget.model.setForeachData(
                widget.nodeId,
                APIForeachData(inputType: newValue),
              );
            },
          ),
        ],
      ),
    );
  }
}
