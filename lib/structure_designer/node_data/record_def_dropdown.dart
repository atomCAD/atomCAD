import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Shared dropdown of named record type defs in the project, with a trailing
/// "Edit definition…" button that activates the bound def in the user-types
/// panel and switches the main editing area to the schema editor. Used by
/// the `record_construct` / `record_destructure` / `product` per-node
/// property editors. See `doc/design_record_types.md` Phase 9.
class RecordDefDropdown extends StatelessWidget {
  final String value;
  final String label;
  final String emptyHint;
  final StructureDesignerModel model;
  final ValueChanged<String> onChanged;

  const RecordDefDropdown({
    super.key,
    required this.value,
    required this.label,
    required this.emptyHint,
    required this.model,
    required this.onChanged,
  });

  @override
  Widget build(BuildContext context) {
    final names = sd_api.getRecordTypeDefNames() ?? <String>[];
    final danglingButNotEmpty = value.isNotEmpty && !names.contains(value);
    final boundToValidDef = value.isNotEmpty && names.contains(value);

    final items = <DropdownMenuItem<String>>[
      DropdownMenuItem<String>(
        value: '',
        child: Text(
          emptyHint,
          style: const TextStyle(fontStyle: FontStyle.italic),
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

    return Row(
      children: [
        Expanded(
          child: DropdownButtonFormField<String>(
            value: value,
            decoration: AppInputDecorations.standard.copyWith(
              labelText: label,
            ),
            items: items,
            onChanged: (newValue) {
              if (newValue != null) {
                onChanged(newValue);
              }
            },
          ),
        ),
        const SizedBox(width: 8),
        Tooltip(
          message: boundToValidDef
              ? 'Open the schema editor for "$value"'
              : (value.isEmpty
                  ? 'No schema chosen'
                  : 'Schema "$value" is not registered'),
          child: TextButton.icon(
            icon: const Icon(Icons.edit_outlined, size: 16),
            label: const Text('Edit definition…'),
            onPressed: boundToValidDef
                ? () => model.setActiveRecordDef(value)
                : null,
          ),
        ),
      ],
    );
  }
}
