import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/node_data/record_def_dropdown.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Property editor for `record_destructure` nodes. Single property: a
/// record type def name picked from the project's registry. The
/// per-field output pins are derived in Rust from the chosen def.
class RecordDestructureEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIRecordSchemaData? data;
  final StructureDesignerModel model;

  const RecordDestructureEditor({
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
            title: 'Record Destructure',
            nodeTypeName: 'record_destructure',
          ),
          const SizedBox(height: 8),
          RecordDefDropdown(
            value: data!.schema,
            label: 'Schema',
            emptyHint: '— No schema chosen —',
            model: model,
            onChanged: (newName) {
              model.setRecordDestructureData(
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
