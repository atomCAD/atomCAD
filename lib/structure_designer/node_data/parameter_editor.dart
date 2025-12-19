import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for parameter nodes
class ParameterEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIParameterData? data;
  final StructureDesignerModel model;

  const ParameterEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ParameterEditor> createState() => ParameterEditorState();
}

class ParameterEditorState extends State<ParameterEditor> {
  @override
  void dispose() {
    super.dispose();
  }

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
            title: 'Parameter Properties',
            nodeTypeName: 'parameter',
          ),
          const SizedBox(height: 8),

          // Parameter Name
          StringInput(
            label: 'Parameter Name',
            value: widget.data?.paramName ?? '',
            onChanged: (value) {
              widget.model.setParameterData(
                widget.nodeId,
                APIParameterData(
                  paramIndex: widget.data?.paramIndex ?? BigInt.zero,
                  paramName: value,
                  dataType: widget.data?.dataType ??
                      const APIDataType(
                          dataTypeBase: APIDataTypeBase.none,
                          customDataType: null,
                          array: false),
                  sortOrder: widget.data?.sortOrder ?? 0,
                  error: widget.data?.error,
                ),
              );
            },
          ),
          const SizedBox(height: 8),

          // Data Type Input
          DataTypeInput(
            label: 'Data Type',
            value: widget.data!.dataType,
            onChanged: (newValue) {
              widget.model.setParameterData(
                widget.nodeId,
                APIParameterData(
                  paramIndex: widget.data!.paramIndex,
                  paramName: widget.data!.paramName,
                  dataType: newValue,
                  sortOrder: widget.data!.sortOrder,
                  error: widget.data!.error,
                ),
              );
            },
          ),
          const SizedBox(height: 8),

          // Error message display
          if (widget.data?.error != null)
            Padding(
              padding: const EdgeInsets.only(top: 8.0),
              child: Container(
                width: double.infinity,
                padding: const EdgeInsets.all(8.0),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.errorContainer,
                  borderRadius: BorderRadius.circular(4.0),
                  border: Border.all(
                    color: Theme.of(context).colorScheme.error,
                    width: 1.0,
                  ),
                ),
                child: Text(
                  widget.data!.error!,
                  style: TextStyle(
                    color: Theme.of(context).colorScheme.onErrorContainer,
                    fontSize: 12.0,
                  ),
                ),
              ),
            ),

          // Sort Order
          IntInput(
            label: 'Sort Order',
            value: widget.data!.sortOrder,
            onChanged: (newValue) {
              widget.model.setParameterData(
                widget.nodeId,
                APIParameterData(
                  paramIndex: widget.data!.paramIndex,
                  paramName: widget.data!.paramName,
                  dataType: widget.data!.dataType,
                  sortOrder: newValue,
                  error: widget.data!.error,
                ),
              );
            },
          ),
          const SizedBox(height: 16),

          // Parameter Index (readonly, calculated by Rust)
          TextField(
            controller:
                TextEditingController(text: widget.data!.paramIndex.toString()),
            decoration: const InputDecoration(
              labelText: 'Parameter Index (calculated)',
              border: OutlineInputBorder(),
              enabled: false,
            ),
            readOnly: true,
          ),
        ],
      ),
    );
  }
}
