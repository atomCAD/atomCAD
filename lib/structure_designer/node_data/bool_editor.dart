import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

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
          Text('Bool Properties',
              style: Theme.of(context).textTheme.titleMedium),
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
