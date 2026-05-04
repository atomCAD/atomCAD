import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
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
          _RecordSchemaDropdown(
            value: data!.schema,
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

class _RecordSchemaDropdown extends StatelessWidget {
  final String value;
  final ValueChanged<String> onChanged;

  const _RecordSchemaDropdown({
    required this.value,
    required this.onChanged,
  });

  @override
  Widget build(BuildContext context) {
    final names = sd_api.getRecordTypeDefNames() ?? <String>[];
    final danglingButNotEmpty = value.isNotEmpty && !names.contains(value);

    final items = <DropdownMenuItem<String>>[
      const DropdownMenuItem<String>(
        value: '',
        child: Text(
          '— No schema chosen —',
          style: TextStyle(fontStyle: FontStyle.italic),
        ),
      ),
      ...names.map(
        (name) => DropdownMenuItem<String>(
          value: name,
          child: Text(name),
        ),
      ),
      if (danglingButNotEmpty)
        DropdownMenuItem<String>(
          value: value,
          child: Text(
            '$value (missing)',
            style: const TextStyle(color: Colors.red),
          ),
        ),
    ];

    return DropdownButtonFormField<String>(
      value: value,
      decoration: AppInputDecorations.standard.copyWith(
        labelText: 'Schema',
      ),
      items: items,
      onChanged: (newValue) {
        if (newValue != null) {
          onChanged(newValue);
        }
      },
    );
  }
}
