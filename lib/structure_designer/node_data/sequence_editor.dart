import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for sequence nodes
class SequenceEditor extends StatefulWidget {
  final BigInt nodeId;
  final APISequenceData? data;
  final StructureDesignerModel model;

  const SequenceEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<SequenceEditor> createState() => SequenceEditorState();
}

class SequenceEditorState extends State<SequenceEditor> {
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
            title: 'Sequence Properties',
            nodeTypeName: 'sequence',
          ),
          const SizedBox(height: 8),

          // Element Type
          DataTypeInput(
            label: 'Element Type',
            value: widget.data!.elementType,
            onChanged: (newValue) {
              widget.model.setSequenceData(
                widget.nodeId,
                APISequenceData(
                  elementType: newValue,
                  inputCount: widget.data!.inputCount,
                ),
              );
            },
          ),
          const SizedBox(height: 8),

          // Input Count
          IntInput(
            label: 'Count',
            value: widget.data!.inputCount,
            minimumValue: 1,
            onChanged: (newValue) {
              widget.model.setSequenceData(
                widget.nodeId,
                APISequenceData(
                  elementType: widget.data!.elementType,
                  inputCount: newValue,
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
