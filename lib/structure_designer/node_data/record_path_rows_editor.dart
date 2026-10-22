import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/literal_fields_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/inputs/type_editor_dialog.dart';
import 'package:flutter_cad/structure_designer/node_data/record_def_dropdown.dart';
import 'package:flutter_cad/structure_designer/node_data/wire_type_offer.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// The `record` input pin, whose wire the node's root type is taken from.
const int _recordPinIndex = 0;

/// The shared body of the `record_get` and `record_set` property editors
/// (`doc/design_record_get_set.md` Phase 4): the schema dropdown, one row per
/// path (path, leaf type, delete button; a broken row in the error style), and
/// an "Add path" button opening a tree picker of the root's fields.
///
/// The root is the node's mode, decided in Rust
/// (`doc/design_anonymous_record_readers.md`): **named** (a def picked from the
/// dropdown), **anonymous** (a type taken from the wire, listed read-only under
/// the dropdown, with an empty schema) or **unset** (neither — no path can be
/// added). The dropdown is one undo step through
/// [StructureDesignerModel.setRecordPathNodeSchema], which moves the rows onto
/// the new root by their current names; **Use wire's type** re-adopts a stale
/// wire's type, as on `record_destructure`.
///
/// Rows are named by path, never by id. Every edit sends the row list the
/// getter returned with the change applied ([onSetData]) — a dangling row in
/// that list is kept by the kernel, one left out is deleted — so this widget
/// never has to know which row is which. A row edit sends the current schema
/// back (empty on an anonymous node, which keeps its root). Rejections come
/// back as an [APIResult] and are shown inline.
///
/// `record_set` passes [onSetLiteral] / [onClearLiteral], and each row with an
/// inline-editable leaf then gets the shared [LiteralFieldsEditor] (stored /
/// unset / wired states, as on `record_construct`), keyed by row index.
class RecordPathRowsEditor extends StatefulWidget {
  final BigInt nodeId;
  final String title;
  final String nodeTypeName;
  final APIRecordPathNodeData? data;
  final StructureDesignerModel model;
  final APIResult Function(String schema, List<APIRecordPath> paths) onSetData;
  final APIResult Function(int rowIndex, APILiteralValue value)? onSetLiteral;
  final APIResult Function(int rowIndex)? onClearLiteral;

  const RecordPathRowsEditor({
    super.key,
    required this.nodeId,
    required this.title,
    required this.nodeTypeName,
    required this.data,
    required this.model,
    required this.onSetData,
    this.onSetLiteral,
    this.onClearLiteral,
  });

  @override
  State<RecordPathRowsEditor> createState() => _RecordPathRowsEditorState();
}

class _RecordPathRowsEditorState extends State<RecordPathRowsEditor> {
  /// Last inline error from a setter (`null` when the last edit succeeded).
  String? _error;

  void _apply(APIResult result) {
    setState(() => _error = result.success ? null : result.errorMessage);
  }

  List<APIRecordPath> get _paths => widget.data!.rows
      .map((r) => APIRecordPath(names: r.path.names, dangling: r.path.dangling))
      .toList();

  void _setSchema(String schema) {
    _apply(widget.model.setRecordPathNodeSchema(widget.nodeId, schema));
  }

  void _removeRow(int index) {
    final paths = _paths..removeAt(index);
    _apply(widget.onSetData(widget.data!.schema, paths));
  }

  Future<void> _addPath() async {
    final data = widget.data!;
    final taken = data.rows
        .where((r) => !r.path.dangling)
        .map((r) => formatRecordPath(r.path.names))
        .toSet();
    final picked = await showRecordFieldPicker(
      context: context,
      rootLabel: data.schema.isEmpty ? "the wire's type" : data.schema,
      tree: widget.model.getRecordPathFieldTree(widget.nodeId),
      taken: taken,
    );
    if (picked == null || !mounted) return;
    final paths = _paths..add(APIRecordPath(names: picked, dangling: false));
    _apply(widget.onSetData(data.schema, paths));
  }

  @override
  Widget build(BuildContext context) {
    final data = widget.data;
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }
    final theme = Theme.of(context);
    final anonymous = data.schema.isEmpty && data.fields.isNotEmpty;
    final unset = data.schema.isEmpty && data.fields.isEmpty;
    // Re-read on every build (see wire_type_offer.dart).
    final offer = wireTypeOfferAt(
        widget.model.getWireTypeOffers(widget.nodeId), _recordPinIndex);
    final hintStyle = theme.textTheme.bodyMedium?.copyWith(
      color: theme.colorScheme.onSurfaceVariant,
      fontStyle: FontStyle.italic,
    );
    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          NodeEditorHeader(
            title: widget.title,
            nodeTypeName: widget.nodeTypeName,
          ),
          const SizedBox(height: 8),
          RecordDefDropdown(
            value: data.schema,
            label: 'Schema',
            emptyHint: anonymous ? '— None (type from wire) —' : '— None —',
            model: widget.model,
            onChanged: _setSchema,
          ),
          const SizedBox(height: 8),
          if (unset)
            Text('Wire a record into `record`, or pick a schema.',
                style: hintStyle),
          if (anonymous) ...[
            Text('Type from wire', style: theme.textTheme.titleSmall),
            const SizedBox(height: 4),
            for (final field in data.fields)
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
              model: widget.model,
              nodeId: widget.nodeId,
              index: _recordPinIndex,
              offeredType: offer,
            ),
          ],
          // An unset node can still hold broken rows (its schema was set to
          // none), so they are listed for deletion; only adding needs a root.
          if (!unset || data.rows.isNotEmpty) ...[
            const SizedBox(height: 12),
            Text('Paths', style: theme.textTheme.titleSmall),
            const SizedBox(height: 4),
            if (data.rows.isEmpty)
              const Text(
                'No paths yet.',
                style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
              ),
            for (int i = 0; i < data.rows.length; i++) _buildRow(i),
            if (!unset) ...[
              const SizedBox(height: 4),
              TextButton.icon(
                key: const ValueKey('record_path_add'),
                icon: const Icon(Icons.add, size: 16),
                label: const Text('Add path'),
                onPressed: _addPath,
              ),
            ],
          ],
          if (_error != null) ...[
            const SizedBox(height: 8),
            ErrorBanner(message: _error!),
          ],
        ],
      ),
    );
  }

  Widget _buildRow(int index) {
    final row = widget.data!.rows[index];
    final broken = row.path.dangling || row.problem != null;
    final errorColor = Theme.of(context).colorScheme.error;
    final literal = row.literal;
    final onSetLiteral = widget.onSetLiteral;
    final onClearLiteral = widget.onClearLiteral;
    return Padding(
      key: ValueKey('record_path_row_$index'),
      padding: const EdgeInsets.only(bottom: 6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              if (broken)
                Padding(
                  padding: const EdgeInsets.only(right: 4),
                  child: Icon(Icons.link_off, size: 16, color: errorColor),
                ),
              Expanded(
                child: Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(
                        text: formatRecordPath(row.path.names),
                        style: TextStyle(
                          fontFamily: 'monospace',
                          color: broken ? errorColor : null,
                          decoration: row.path.dangling
                              ? TextDecoration.lineThrough
                              : null,
                        ),
                      ),
                      if (row.leafType != null)
                        TextSpan(
                          text: '  ${row.leafType}',
                          style: const TextStyle(color: Colors.grey),
                        ),
                    ],
                  ),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              IconButton(
                key: ValueKey('record_path_delete_$index'),
                icon: const Icon(Icons.delete_outline, size: 18),
                tooltip: 'Remove this path',
                visualDensity: VisualDensity.compact,
                onPressed: () => _removeRow(index),
              ),
            ],
          ),
          if (row.problem != null)
            Text(
              row.problem!,
              style: TextStyle(fontSize: 11, color: errorColor),
            ),
          if (literal != null && onSetLiteral != null && onClearLiteral != null)
            Padding(
              padding: const EdgeInsets.only(left: 12, top: 2),
              child: LiteralFieldsEditor(
                header: const SizedBox.shrink(),
                fields: [literal],
                emptyMessage: '',
                onSet: (_, value) => _apply(onSetLiteral(index, value)),
                onClear: (_) => _apply(onClearLiteral(index)),
                keyPrefix: 'record_set_row',
              ),
            ),
        ],
      ),
    );
  }
}

/// Path text as the text format writes it: segments joined with `.`, a
/// segment containing `.` in backticks.
String formatRecordPath(List<String> names) =>
    names.map((n) => n.contains('.') || n.isEmpty ? '`$n`' : n).join('.');

/// Opens the field [tree] of a root record type (named in the title by
/// [rootLabel]) and returns the picked path (segment names), or `null` if the
/// dialog was dismissed. Record-typed fields expand;
/// any field, record-typed or not, can be picked. Paths in [taken] (formatted
/// with [formatRecordPath]) are shown but not pickable.
Future<List<String>?> showRecordFieldPicker({
  required BuildContext context,
  required String rootLabel,
  required List<APIRecordFieldNode> tree,
  required Set<String> taken,
}) {
  return showDialog<List<String>>(
    context: context,
    barrierDismissible: false,
    builder: (context) => DraggableDialog(
      width: 360,
      dismissible: true,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Add a path into $rootLabel',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            ConstrainedBox(
              constraints: const BoxConstraints(maxHeight: 400),
              child: tree.isEmpty
                  ? const Text('This record type has no fields.',
                      style: TextStyle(fontStyle: FontStyle.italic))
                  : SingleChildScrollView(
                      child: _FieldTree(
                        nodes: tree,
                        prefix: const [],
                        taken: taken,
                        onPick: (path) => Navigator.of(context).pop(path),
                      ),
                    ),
            ),
            const SizedBox(height: 8),
            Align(
              alignment: Alignment.centerRight,
              child: TextButton(
                onPressed: () => Navigator.of(context).pop(),
                child: const Text('Cancel'),
              ),
            ),
          ],
        ),
      ),
    ),
  );
}

class _FieldTree extends StatefulWidget {
  final List<APIRecordFieldNode> nodes;
  final List<String> prefix;
  final Set<String> taken;
  final ValueChanged<List<String>> onPick;

  const _FieldTree({
    required this.nodes,
    required this.prefix,
    required this.taken,
    required this.onPick,
  });

  @override
  State<_FieldTree> createState() => _FieldTreeState();
}

class _FieldTreeState extends State<_FieldTree> {
  final Set<String> _expanded = {};

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final node in widget.nodes) ..._buildNode(node),
      ],
    );
  }

  List<Widget> _buildNode(APIRecordFieldNode node) {
    final path = [...widget.prefix, node.name];
    final text = formatRecordPath(path);
    final isTaken = widget.taken.contains(text);
    final expanded = _expanded.contains(node.name);
    return [
      InkWell(
        key: ValueKey('record_field_pick_$text'),
        onTap: isTaken ? null : () => widget.onPick(path),
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 4),
          child: Row(
            children: [
              SizedBox(
                width: 24,
                child: node.steppable
                    ? InkWell(
                        onTap: () => setState(() {
                          if (!_expanded.remove(node.name)) {
                            _expanded.add(node.name);
                          }
                        }),
                        child: Icon(
                          expanded ? Icons.expand_more : Icons.chevron_right,
                          size: 18,
                        ),
                      )
                    : null,
              ),
              Expanded(
                child: Text.rich(
                  TextSpan(children: [
                    TextSpan(
                      text: node.name,
                      style: TextStyle(color: isTaken ? Colors.grey : null),
                    ),
                    TextSpan(
                      text: '  ${node.dataType}',
                      style: const TextStyle(color: Colors.grey),
                    ),
                    if (isTaken)
                      const TextSpan(
                        text: '  (added)',
                        style: TextStyle(
                            color: Colors.grey, fontStyle: FontStyle.italic),
                      ),
                  ]),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ],
          ),
        ),
      ),
      if (node.steppable && expanded)
        Padding(
          padding: const EdgeInsets.only(left: 16),
          child: _FieldTree(
            nodes: node.children,
            prefix: path,
            taken: widget.taken,
            onPick: widget.onPick,
          ),
        ),
    ];
  }
}
