import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Property editor for `product` nodes. Single property: a record type def
/// name (the *target*) picked from the project's registry. The Flutter
/// dropdown reuses `APIRecordSchemaData` for symmetry with
/// `record_construct` / `record_destructure`; on the Rust side it maps onto
/// `ProductData.target`.
class ProductEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIRecordSchemaData? data;
  final StructureDesignerModel model;

  const ProductEditor({
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
            title: 'Product',
            nodeTypeName: 'product',
          ),
          const SizedBox(height: 8),
          _RecordTargetDropdown(
            value: data!.schema,
            onChanged: (newName) {
              model.setProductData(
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

class _RecordTargetDropdown extends StatelessWidget {
  final String value;
  final ValueChanged<String> onChanged;

  const _RecordTargetDropdown({
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
          '— No target chosen —',
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
        labelText: 'Target',
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
