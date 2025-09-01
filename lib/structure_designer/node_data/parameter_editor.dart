import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

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
          Text('Parameter Properties',
              style: Theme.of(context).textTheme.titleMedium),
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
                  dataType: widget.data?.dataType ?? APIDataType.none,
                  multi: widget.data?.multi ?? false,
                  sortOrder: widget.data?.sortOrder ?? 0,
                ),
              );
            },
          ),
          const SizedBox(height: 8),

          // Data Type Dropdown
          DropdownButtonFormField<APIDataType>(
            value: widget.data!.dataType,
            decoration: const InputDecoration(
              labelText: 'Data Type',
              border: OutlineInputBorder(),
            ),
            items: APIDataType.values.map((dataType) {
              return DropdownMenuItem(
                value: dataType,
                child: Text(getApiDataTypeDisplayName(dataType: dataType)),
              );
            }).toList(),
            onChanged: (newValue) {
              if (newValue != null) {
                widget.model.setParameterData(
                  widget.nodeId,
                  APIParameterData(
                    paramIndex: widget.data!.paramIndex,
                    paramName: widget.data!.paramName,
                    dataType: newValue,
                    multi: widget.data!.multi,
                    sortOrder: widget.data!.sortOrder,
                  ),
                );
              }
            },
          ),
          const SizedBox(height: 8),

          // Multi Checkbox
          CheckboxListTile(
            title: const Text('Multi'),
            subtitle: const Text('Accept multiple inputs'),
            value: widget.data!.multi,
            onChanged: (newValue) {
              if (newValue != null) {
                widget.model.setParameterData(
                  widget.nodeId,
                  APIParameterData(
                    paramIndex: widget.data!.paramIndex,
                    paramName: widget.data!.paramName,
                    dataType: widget.data!.dataType,
                    multi: newValue,
                    sortOrder: widget.data!.sortOrder,
                  ),
                );
              }
            },
          ),
          const SizedBox(height: 8),

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
                  multi: widget.data!.multi,
                  sortOrder: newValue,
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
