import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for string nodes
class StringEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIStringData? data;
  final StructureDesignerModel model;

  const StringEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<StringEditor> createState() => StringEditorState();
}

class StringEditorState extends State<StringEditor> {
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
          Text('String Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          StringInput(
            label: 'Value',
            value: widget.data!.value,
            onChanged: (newValue) {
              widget.model.setStringData(
                widget.nodeId,
                APIStringData(
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
