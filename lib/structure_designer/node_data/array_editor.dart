import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/type_editor_dialog.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/literal_fields_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Property editor for the `array` node — a one-node array literal: pick an
/// element type, then author the elements inline.
///
/// The node has **no input pins**, so there are no wires to preserve across an
/// element edit and no wire-stability machinery here: add / remove / reorder /
/// edit are all plain node-data mutations with standard undo. Only the
/// element-type change retypes the output pin — which invalidates downstream
/// wires' type checks (they are flagged, not dropped) — and the Rust op handles
/// that with a whole-network-snapshot undo that re-validates.
///
/// Element rows come from Rust as [APILiteralField]s — the same row type
/// `record_construct`'s panel uses — so the shared [LiteralFieldsEditor]
/// supplies every type widget, the Part A editor hints (element dropdowns,
/// color swatches, enum dropdowns, range sliders), and the `Optional`
/// set/unset tri-state for free.
///
/// See `doc/design_array_node_and_field_hints.md` Part B.
class ArrayEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIArrayNodeData? data;
  final StructureDesignerModel model;

  const ArrayEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ArrayEditor> createState() => _ArrayEditorState();
}

class _ArrayEditorState extends State<ArrayEditor> {
  /// Last kernel rejection (an out-of-range index from a stale panel, a
  /// non-literal-capable element type), shown inline. Cleared by the next
  /// successful edit.
  String? _error;

  void _run(APIResult Function() action) {
    final result = action();
    if (!mounted) return;
    setState(() {
      _error = result.success ? null : result.errorMessage;
    });
  }

  @override
  Widget build(BuildContext context) {
    final data = widget.data;
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(title: 'Array', nodeTypeName: 'array'),
          const SizedBox(height: 8),
          _buildElementTypePicker(data),
          const SizedBox(height: 12),
          Text('Elements', style: Theme.of(context).textTheme.titleSmall),
          const SizedBox(height: 4),
          if (data.elements.isEmpty)
            const Text(
              'No elements yet. Add one below — an empty array is still a '
              'valid array.',
              style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
            )
          else
            for (var i = 0; i < data.elements.length; i++)
              _buildElementCard(context, data, i),
          const SizedBox(height: 4),
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              key: const Key('array_add_element'),
              icon: const Icon(Icons.add, size: 18),
              label: const Text('Add element'),
              // Append: the new element is seeded with its type's defaults so
              // it evaluates immediately rather than erroring until edited.
              onPressed: () => _run(() => widget.model
                  .addArrayElement(widget.nodeId, data.elements.length)),
            ),
          ),
          if (_error != null) ...[
            const SizedBox(height: 8),
            _buildErrorBanner(context, _error!),
          ],
        ],
      ),
    );
  }

  /// Element-type dropdown. The option list comes from Rust
  /// (`is_literal_capable`), so it can never offer a type the setter rejects —
  /// structural types, `Function` / `Iter` / `Unit`, nested arrays and
  /// record types with non-simple fields are simply absent. A stored type that
  /// is not in the list (a record def deleted out from under the node, or a
  /// hand-authored text-format type) is shown as a flagged synthetic entry
  /// rather than silently snapping to something else.
  Widget _buildElementTypePicker(APIArrayNodeData data) {
    final options = widget.model.getArrayElementTypeOptions();
    final byLabel = <String, APIDataType>{
      for (final option in options) apiDataTypeToString(option): option,
    };
    final current = apiDataTypeToString(data.elementType);
    final isForeign = !byLabel.containsKey(current);

    return DropdownButtonFormField<String>(
      key: const Key('array_element_type'),
      value: current,
      isExpanded: true,
      decoration: AppInputDecorations.standard.copyWith(
        labelText: 'Element Type',
      ),
      items: [
        if (isForeign)
          DropdownMenuItem<String>(
            value: current,
            child: Text(
              '$current  — not an authorable element type',
              style: const TextStyle(
                fontStyle: FontStyle.italic,
                color: Colors.redAccent,
              ),
            ),
          ),
        for (final label in byLabel.keys)
          DropdownMenuItem<String>(value: label, child: Text(label)),
      ],
      onChanged: (label) {
        final chosen = label == null ? null : byLabel[label];
        if (chosen == null) return;
        _run(() => widget.model.setArrayElementType(widget.nodeId, chosen));
      },
    );
  }

  Widget _buildElementCard(
    BuildContext context,
    APIArrayNodeData data,
    int index,
  ) {
    final theme = Theme.of(context);
    final element = data.elements[index];
    final isLast = index == data.elements.length - 1;

    return Container(
      key: ValueKey('array_element_$index'),
      margin: const EdgeInsets.only(bottom: 8),
      padding: const EdgeInsets.fromLTRB(8, 4, 4, 8),
      decoration: BoxDecoration(
        border: Border.all(color: theme.dividerColor),
        borderRadius: BorderRadius.circular(4),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Text(
                '[$index]',
                style: theme.textTheme.bodySmall
                    ?.copyWith(fontFamily: 'monospace'),
              ),
              const Spacer(),
              IconButton(
                key: ValueKey('array_move_up_$index'),
                icon: const Icon(Icons.arrow_upward, size: 16),
                tooltip: 'Move up',
                visualDensity: VisualDensity.compact,
                onPressed: index == 0
                    ? null
                    : () => _run(() => widget.model
                        .moveArrayElement(widget.nodeId, index, index - 1)),
              ),
              IconButton(
                key: ValueKey('array_move_down_$index'),
                icon: const Icon(Icons.arrow_downward, size: 16),
                tooltip: 'Move down',
                visualDensity: VisualDensity.compact,
                onPressed: isLast
                    ? null
                    : () => _run(() => widget.model
                        .moveArrayElement(widget.nodeId, index, index + 1)),
              ),
              IconButton(
                key: ValueKey('array_remove_$index'),
                icon: const Icon(Icons.delete_outline, size: 16),
                tooltip: 'Remove element',
                visualDensity: VisualDensity.compact,
                onPressed: () => _run(() =>
                    widget.model.removeArrayElement(widget.nodeId, index)),
              ),
            ],
          ),
          if (element.stale)
            _buildStaleRow(context, index)
          else
            LiteralFieldsEditor(
              header: const SizedBox.shrink(),
              fields: element.fields,
              emptyMessage: 'This record type has no editable fields.',
              onSet: (name, value) => _run(() => data.isRecord
                  ? widget.model.setArrayElementFieldLiteral(
                      widget.nodeId, index, name, value)
                  : widget.model
                      .setArrayElementLiteral(widget.nodeId, index, value)),
              onClear: (name) => _run(() => data.isRecord
                  ? widget.model
                      .clearArrayElementFieldLiteral(widget.nodeId, index, name)
                  // A simple element has no "absent" state — clearing resets it
                  // to the element type's default.
                  : widget.model
                      .clearArrayElementLiteral(widget.nodeId, index)),
              keyPrefix: 'array_element_${index}_field',
            ),
        ],
      ),
    );
  }

  /// A stored literal that no longer fits the element type — left behind by an
  /// element-type change or a record-def edit. It is **kept, not dropped**: the
  /// array reports a localized eval error naming this index, and the row offers
  /// a reset rather than deciding for the user.
  Widget _buildStaleRow(BuildContext context, int index) {
    final theme = Theme.of(context);
    return Row(
      children: [
        Icon(Icons.warning_amber_rounded,
            size: 16, color: theme.colorScheme.error),
        const SizedBox(width: 6),
        Expanded(
          child: Text(
            'Stored value does not match the element type.',
            style: TextStyle(fontSize: 12, color: theme.colorScheme.error),
          ),
        ),
        TextButton(
          key: ValueKey('array_reset_$index'),
          onPressed: () => _run(() =>
              widget.model.clearArrayElementLiteral(widget.nodeId, index)),
          child: const Text('Reset'),
        ),
      ],
    );
  }

  Widget _buildErrorBanner(BuildContext context, String message) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(8),
      decoration: BoxDecoration(
        color: scheme.errorContainer,
        border: Border.all(color: scheme.error),
        borderRadius: BorderRadius.circular(4),
      ),
      child: Text(
        message,
        style: TextStyle(fontSize: 12, color: scheme.onErrorContainer),
      ),
    );
  }
}
