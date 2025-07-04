import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for extrude nodes
class ExtrudeEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIExtrudeData? data;
  final StructureDesignerModel model;

  const ExtrudeEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ExtrudeEditor> createState() => ExtrudeEditorState();
}

class ExtrudeEditorState extends State<ExtrudeEditor> {
  // Direct API calls are made in onChanged handlers

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Extrude Properties',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            IntInput(
              label: 'Height',
              value: widget.data!.height,
              minimumValue: 1,
              onChanged: (newValue) {
                widget.model.setExtrudeData(
                  widget.nodeId,
                  APIExtrudeData(
                    height: newValue,
                  ),
                );
              },
            ),
          ],
        ),
      ),
    );
  }
}
