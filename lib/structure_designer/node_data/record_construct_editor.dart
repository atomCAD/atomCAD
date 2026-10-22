import 'package:flutter/material.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/literal_fields_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/name_record_dialog.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/node_data/record_def_dropdown.dart';
import 'package:flutter_cad/structure_designer/node_data/wire_type_offer.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Property editor for `record_construct` nodes. Shows the schema dropdown
/// plus an inline editor row per simple-typed field via the shared
/// [LiteralFieldsEditor].
///
/// With no schema chosen the node is an **anonymous record**
/// (`doc/design_wire_declared_inputs.md`): it declares its own fields, so the
/// panel adds a field list styled like the `expr` parameter list (name, type,
/// reorder, delete, Add field), the Take type from wire offers, and
/// **Name this record…**, which promotes the fields to a named def.
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
    final anonymous = data!.schema.isEmpty;
    final localFields =
        anonymous ? model.getRecordConstructLocalFields(nodeId) : null;
    final fields = model.getRecordConstructFields(nodeId);
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
            emptyHint: '— Anonymous record —',
            model: model,
            onChanged: (newName) {
              model.setRecordConstructData(
                nodeId,
                APIRecordSchemaData(schema: newName),
              );
            },
          ),
          if (localFields != null) ...[
            const SizedBox(height: 12),
            _AnonymousFieldList(
              model: model,
              nodeId: nodeId,
              fields: localFields,
            ),
          ],
          if (fields != null && (!anonymous || fields.isNotEmpty)) ...[
            const SizedBox(height: 12),
            LiteralFieldsEditor(
              header: anonymous
                  ? Text('Values',
                      style: Theme.of(context).textTheme.titleSmall)
                  : const SizedBox.shrink(),
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

/// The anonymous mode's own field list. Every edit re-sends the whole list
/// through `model.setRecordConstructFields`; Rust keeps field ids by name,
/// then by position (as it does for `expr` parameters), so a rename or a
/// reorder keeps every wire and a delete drops only its own.
class _AnonymousFieldList extends StatelessWidget {
  final StructureDesignerModel model;
  final BigInt nodeId;
  final List<APIRecordLocalField> fields;

  const _AnonymousFieldList({
    required this.model,
    required this.nodeId,
    required this.fields,
  });

  static const APIDataType _defaultFieldType = APIDataType(
    dataTypeBase: APIDataTypeBase.float,
    customDataType: null,
    array: false,
    children: [],
  );

  void _commit(BuildContext context, List<APIRecordLocalField> newFields) {
    final error = model.setRecordConstructFields(nodeId, newFields);
    if (error != null) showErrorSnackBar(context, error);
  }

  String _newFieldName() {
    final taken = fields.map((f) => f.name).toSet();
    if (!taken.contains('field')) return 'field';
    var n = 2;
    while (taken.contains('field_$n')) {
      n++;
    }
    return 'field_$n';
  }

  @override
  Widget build(BuildContext context) {
    // An open-port drop leaves the new field's row to focus; Take type from
    // wire offers are re-read on every build (see wire_type_offer.dart).
    final focusIndex = model.takePendingNameFocus(nodeId);
    final offers = model.getWireTypeOffers(nodeId);
    final theme = Theme.of(context);

    Widget iconButton(IconData icon, String tooltip, VoidCallback? onPressed,
            {Color? color}) =>
        SizedBox(
          width: 28,
          height: 36,
          child: IconButton(
            onPressed: onPressed,
            icon: Icon(icon, size: 18),
            tooltip: tooltip,
            color: color,
            padding: EdgeInsets.zero,
          ),
        );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Text('Fields', style: theme.textTheme.titleSmall),
            ElevatedButton.icon(
              onPressed: () => _commit(context, [
                ...fields,
                APIRecordLocalField(
                    name: _newFieldName(), dataType: _defaultFieldType),
              ]),
              icon: const Icon(Icons.add),
              label: const Text('Add field'),
              style: ElevatedButton.styleFrom(
                padding:
                    const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              ),
            ),
          ],
        ),
        const SizedBox(height: 8),
        if (fields.isEmpty)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 8.0),
            child: Text(
              'No fields yet. Drop a wire on the node\'s + tab, or click '
              '"Add field".',
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
        for (int index = 0; index < fields.length; index++)
          Padding(
            padding: const EdgeInsets.only(bottom: 4.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      flex: 3,
                      child: StringInput(
                        label: 'Name',
                        value: fields[index].name,
                        requestFocus: index == focusIndex,
                        onChanged: (newName) => _commit(context, [
                          for (int i = 0; i < fields.length; i++)
                            i == index
                                ? APIRecordLocalField(
                                    name: newName, dataType: fields[i].dataType)
                                : fields[i],
                        ]),
                      ),
                    ),
                    const SizedBox(width: 6),
                    Expanded(
                      flex: 3,
                      child: DataTypeInput(
                        label: 'Type',
                        value: fields[index].dataType,
                        onChanged: (newType) => _commit(context, [
                          for (int i = 0; i < fields.length; i++)
                            i == index
                                ? APIRecordLocalField(
                                    name: fields[i].name, dataType: newType)
                                : fields[i],
                        ]),
                      ),
                    ),
                    if (wireTypeOfferAt(offers, index) != null)
                      const WireTypeOfferMarker(),
                    iconButton(
                      Icons.arrow_upward,
                      'Move up',
                      index == 0
                          ? null
                          : () => _commit(context, [
                                ...fields.sublist(0, index - 1),
                                fields[index],
                                fields[index - 1],
                                ...fields.sublist(index + 1),
                              ]),
                    ),
                    iconButton(
                      Icons.arrow_downward,
                      'Move down',
                      index == fields.length - 1
                          ? null
                          : () => _commit(context, [
                                ...fields.sublist(0, index),
                                fields[index + 1],
                                fields[index],
                                ...fields.sublist(index + 2),
                              ]),
                    ),
                    iconButton(
                      Icons.delete,
                      'Delete field',
                      () => _commit(context, [
                        for (int i = 0; i < fields.length; i++)
                          if (i != index) fields[i],
                      ]),
                      color: theme.colorScheme.error,
                    ),
                  ],
                ),
                if (wireTypeOfferAt(offers, index) case final offer?)
                  WireTypeOfferButton(
                    model: model,
                    nodeId: nodeId,
                    index: index,
                    offeredType: offer,
                  ),
              ],
            ),
          ),
        const SizedBox(height: 4),
        OutlinedButton.icon(
          onPressed: fields.isEmpty
              ? null
              : () => showNameRecordDialog(
                  context, model, nodeId, model.propertyEditorScopeChain),
          icon: const Icon(Icons.label_outline, size: 18),
          label: const Text('Name this record…'),
        ),
      ],
    );
  }
}
