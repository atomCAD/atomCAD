import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for bool nodes
class BoolEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIBoolData? data;
  final StructureDesignerModel model;

  const BoolEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<BoolEditor> createState() => BoolEditorState();
}

class BoolEditorState extends State<BoolEditor> {
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
            title: 'Bool Properties',
            nodeTypeName: 'bool',
          ),
          const SizedBox(height: 8),
          CheckboxListTile(
            title: const Text('Value'),
            value: widget.data!.value,
            onChanged: (newValue) {
              if (newValue != null) {
                widget.model.setBoolData(
                  widget.nodeId,
                  APIBoolData(
                    value: newValue,
                  ),
                );
              }
            },
            controlAffinity: ListTileControlAffinity.leading,
            contentPadding: EdgeInsets.zero,
          ),
        ],
      ),
    );
  }
}
