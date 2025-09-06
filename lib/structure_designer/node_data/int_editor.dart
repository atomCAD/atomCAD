import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for int nodes
class IntEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIIntData? data;
  final StructureDesignerModel model;

  const IntEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<IntEditor> createState() => IntEditorState();
}

class IntEditorState extends State<IntEditor> {
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
          Text('Int Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          IntInput(
            label: 'Value',
            value: widget.data!.value,
            onChanged: (newValue) {
              widget.model.setIntData(
                widget.nodeId,
                APIIntData(
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
