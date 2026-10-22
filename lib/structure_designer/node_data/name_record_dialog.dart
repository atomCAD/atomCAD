/// **Name this record…** (`doc/design_wire_declared_inputs.md`, Promotion):
/// turn an anonymous `record_construct` into a named record def in one step,
/// keeping every wire — or, when a def with exactly the same shape already
/// exists, point the node at that def instead.
///
/// Opened from the `record_construct` panel and from the node's context
/// menu; the latter may act on a node the panel is not showing, so the
/// dialog carries the node's own scope chain to every call.
library;

import 'package:flutter/material.dart';

import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

Future<void> showNameRecordDialog(
  BuildContext context,
  StructureDesignerModel model,
  BigInt nodeId,
  List<BigInt> scopeChain,
) {
  return showDialog<void>(
    context: context,
    barrierDismissible: false,
    builder: (context) => NameRecordDialog(
      model: model,
      nodeId: nodeId,
      scopeChain: scopeChain,
    ),
  );
}

class NameRecordDialog extends StatefulWidget {
  final StructureDesignerModel model;
  final BigInt nodeId;
  final List<BigInt> scopeChain;

  const NameRecordDialog({
    super.key,
    required this.model,
    required this.nodeId,
    required this.scopeChain,
  });

  @override
  State<NameRecordDialog> createState() => _NameRecordDialogState();
}

class _NameRecordDialogState extends State<NameRecordDialog> {
  final TextEditingController _nameController = TextEditingController();
  String? _error;

  /// Defs with exactly this node's shape, read once when the dialog opens.
  late final List<String> _sameShape = widget.model
      .getMatchingRecordDefsFor(widget.nodeId, scopeChain: widget.scopeChain);

  @override
  void dispose() {
    _nameController.dispose();
    super.dispose();
  }

  void _create() {
    final name = _nameController.text.trim();
    if (name.isEmpty) {
      setState(() => _error = 'Enter a name for the record type');
      return;
    }
    // Every name rule (identifier, taken, inside a linked library) is Rust's.
    final error = widget.model.promoteRecordConstruct(widget.nodeId, name,
        scopeChain: widget.scopeChain);
    if (error != null) {
      setState(() => _error = error);
      return;
    }
    Navigator.of(context).pop();
    showTransientSnackBar(context, 'Created record type "$name"');
  }

  void _useExisting(String defName) {
    widget.model.setRecordConstructData(
        widget.nodeId, APIRecordSchemaData(schema: defName),
        scopeChain: widget.scopeChain);
    Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return DraggableDialog(
      width: 420,
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Name this record', style: theme.textTheme.headlineSmall),
            const SizedBox(height: 8),
            Text(
              'Creates a record type with these fields and points this node '
              'at it. Every wire is kept.',
              style: theme.textTheme.bodySmall,
            ),
            const SizedBox(height: 16),
            TextField(
              key: const Key('name_record_dialog_name'),
              controller: _nameController,
              autofocus: true,
              decoration: const InputDecoration(
                labelText: 'Record type name',
                border: OutlineInputBorder(),
              ),
              onSubmitted: (_) => _create(),
            ),
            if (_error != null) ...[
              const SizedBox(height: 12),
              ErrorBanner(message: _error!),
            ],
            if (_sameShape.isNotEmpty) ...[
              const SizedBox(height: 16),
              Text('Same shape as:', style: theme.textTheme.titleSmall),
              const SizedBox(height: 4),
              ConstrainedBox(
                constraints: const BoxConstraints(maxHeight: 200),
                child: ListView(
                  shrinkWrap: true,
                  children: [
                    for (final def in _sameShape)
                      Row(
                        children: [
                          Expanded(
                            child: Text(def, overflow: TextOverflow.ellipsis),
                          ),
                          TextButton(
                            onPressed: () => _useExisting(def),
                            child: const Text('Use'),
                          ),
                        ],
                      ),
                  ],
                ),
              ),
            ],
            const SizedBox(height: 24),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: 8),
                ElevatedButton(
                  onPressed: _create,
                  child: const Text('Create'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
