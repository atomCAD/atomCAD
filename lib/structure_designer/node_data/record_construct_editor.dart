import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/node_data/record_def_dropdown.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Property editor for `record_construct` nodes. Single property: a record
/// type def name picked from the project's registry. Pin layout is derived
/// in Rust from the chosen def.
class RecordConstructEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIRecordSchemaData? data;
  final StructureDesignerModel model;

  const RecordConstructEditor({
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
            title: 'Record Construct',
            nodeTypeName: 'record_construct',
          ),
          const SizedBox(height: 8),
          RecordDefDropdown(
            value: data!.schema,
            label: 'Schema',
            emptyHint: '— No schema chosen —',
            model: model,
            onChanged: (newName) {
              model.setRecordConstructData(
                nodeId,
                APIRecordSchemaData(schema: newName),
              );
            },
          ),
        ],
      ),
    );
  }
}
