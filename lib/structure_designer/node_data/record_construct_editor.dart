import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/literal_fields_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/node_data/record_def_dropdown.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Property editor for `record_construct` nodes. Shows the schema dropdown
/// plus, when a schema is chosen, an inline editor row per simple-typed
/// field of the chosen def via the shared [LiteralFieldsEditor].
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
    final schemaChosen = data!.schema.isNotEmpty;
    final fields =
        schemaChosen ? model.getRecordConstructFields(nodeId) : null;
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
          if (fields != null) ...[
            const SizedBox(height: 12),
            LiteralFieldsEditor(
              header: const SizedBox.shrink(),
              fields: fields,
              emptyMessage: 'This record type has no editable fields.',
              onSet: (name, value) =>
                  model.setRecordConstructLiteral(nodeId, name, value),
              onClear: (name) =>
                  model.clearRecordConstructLiteral(nodeId, name),
              keyPrefix: 'record_construct_field',
            ),
          ],
        ],
      ),
    );
  }
}
