import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for fold nodes
class FoldEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIFoldData? data;
  final StructureDesignerModel model;

  const FoldEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<FoldEditor> createState() => FoldEditorState();
}

class FoldEditorState extends State<FoldEditor> {
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
            title: 'Fold Properties',
            nodeTypeName: 'fold',
          ),
          const SizedBox(height: 8),

          // Element Type
          DataTypeInput(
            label: 'Element Type',
            value: widget.data!.elementType,
            onChanged: (newValue) {
              widget.model.setFoldData(
                widget.nodeId,
                APIFoldData(
                  elementType: newValue,
                  accumulatorType: widget.data!.accumulatorType,
                ),
              );
            },
          ),
          const SizedBox(height: 8),

          // Accumulator Type
          DataTypeInput(
            label: 'Accumulator Type',
            value: widget.data!.accumulatorType,
            onChanged: (newValue) {
              widget.model.setFoldData(
                widget.nodeId,
                APIFoldData(
                  elementType: widget.data!.elementType,
                  accumulatorType: newValue,
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
