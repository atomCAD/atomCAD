import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for the `print` node. The single property is `execute_only`,
/// a bool that gates whether the side-effect log push fires only under an
/// Execute pass (`true`) or on every evaluation including normal display
/// passes (`false`, default).
class PrintEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIPrintData? data;
  final StructureDesignerModel model;

  const PrintEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Print Properties',
            nodeTypeName: 'print',
          ),
          const SizedBox(height: 8),
          CheckboxListTile(
            title: const Text('Execute only'),
            subtitle: const Text(
              'When on, the print fires only during an Execute pass. '
              'When off, it fires on every evaluation (including display).',
              style: TextStyle(fontSize: 11),
            ),
            value: data!.executeOnly,
            onChanged: (newValue) {
              if (newValue != null) {
                model.setPrintData(
                  nodeId,
                  APIPrintData(executeOnly: newValue),
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
