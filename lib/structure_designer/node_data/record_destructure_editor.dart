import 'package:flutter/material.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/inputs/type_editor_dialog.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/node_data/record_def_dropdown.dart';
import 'package:flutter_cad/structure_designer/node_data/wire_type_offer.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// The `record` input pin, whose wire the node's type is taken from.
const int _recordPinIndex = 0;

/// Property editor for `record_destructure` nodes
/// (`doc/design_anonymous_record_readers.md`). The node is in one of three
/// modes, all decided in Rust from its data: **named** (a def picked from the
/// dropdown), **anonymous** (a type taken from the wire, listed read-only
/// under the dropdown) or **unset** (neither). The fields of an anonymous
/// node always describe its input, so they are never edited here; the
/// **Use wire's type** button re-adopts a stale wire's type.
class RecordDestructureEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIRecordDestructureData? data;
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
    final theme = Theme.of(context);
    final anonymous = data!.schema.isEmpty && data!.fields.isNotEmpty;
    final unset = data!.schema.isEmpty && data!.fields.isEmpty;
    // Re-read on every build (see wire_type_offer.dart).
    final offer =
        wireTypeOfferAt(model.getWireTypeOffers(nodeId), _recordPinIndex);
    final hintStyle = theme.textTheme.bodyMedium?.copyWith(
      color: theme.colorScheme.onSurfaceVariant,
    );

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
            emptyHint: anonymous ? '— None (type from wire) —' : '— None —',
            model: model,
            onChanged: (newName) {
              final error = model.setRecordDestructureSchema(nodeId, newName);
              if (error != null) showErrorSnackBar(context, error);
            },
          ),
          const SizedBox(height: 8),
          if (unset)
            Text(
              'Wire a record into `record`, or pick a schema.',
              style: hintStyle?.copyWith(fontStyle: FontStyle.italic),
            ),
          if (anonymous) ...[
            Text('Type from wire', style: theme.textTheme.titleSmall),
            const SizedBox(height: 4),
            for (final field in data!.fields)
              Padding(
                padding: const EdgeInsets.only(left: 8.0, bottom: 2.0),
                child: SelectableText(
                  '${field.name}: ${apiDataTypeToString(field.dataType)}',
                  style: theme.textTheme.bodyMedium
                      ?.copyWith(fontFamily: 'monospace'),
                ),
              ),
          ],
          if (offer != null) ...[
            const SizedBox(height: 4),
            WireTypeOfferButton(
              model: model,
              nodeId: nodeId,
              index: _recordPinIndex,
              offeredType: offer,
            ),
          ],
        ],
      ),
    );
  }
}
