import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/node_data/node_data_widget.dart';

/// Editor widget for float nodes
class FloatEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIFloatData? data;
  final StructureDesignerModel model;

  const FloatEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<FloatEditor> createState() => FloatEditorState();
}

class FloatEditorState extends State<FloatEditor> {
  // Direct API calls are made in onChanged handlers

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      key: PropertyEditorKeys.floatEditor,
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Float Properties',
            nodeTypeName: 'float',
          ),
          const SizedBox(height: 8),
          FloatInput(
            label: 'Value',
            value: widget.data!.value,
            inputKey: PropertyEditorKeys.floatValueInput,
            onChanged: (newValue) {
              widget.model.setFloatData(
                widget.nodeId,
                APIFloatData(
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
