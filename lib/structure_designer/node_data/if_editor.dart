import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for the `if` node. One stored property:
///   - `value_type` drives the `then` / `else` input pins and the output pin
///     type. The `cond` pin is always `Bool`.
///
/// Mirrors `rust/src/structure_designer/nodes/if_else.rs`.
class IfEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIIfData? data;
  final StructureDesignerModel model;

  const IfEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<IfEditor> createState() => IfEditorState();
}

class IfEditorState extends State<IfEditor> {
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
            title: 'If Properties',
            nodeTypeName: 'if',
          ),
          const SizedBox(height: 8),
          const Text(
            'Selects the `then` value when `cond` is true, otherwise the '
            '`else` value. Only the taken branch is evaluated.',
            style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
          ),
          const SizedBox(height: 12),
          DataTypeInput(
            label: 'Value Type',
            value: widget.data!.valueType,
            onChanged: (newValue) {
              _updateData(valueType: newValue);
            },
          ),
        ],
      ),
    );
  }

  void _updateData({APIDataType? valueType}) {
    widget.model.setIfData(
      widget.nodeId,
      APIIfData(
        valueType: valueType ?? widget.data!.valueType,
      ),
    );
  }
}
