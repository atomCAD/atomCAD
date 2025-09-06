import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

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
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Float Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          FloatInput(
            label: 'Value',
            value: widget.data!.value,
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
