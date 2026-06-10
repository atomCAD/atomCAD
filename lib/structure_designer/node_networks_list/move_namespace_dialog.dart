import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Which kind of user type the move dialog is editing.
enum _MoveKind { namespace, network, record }

/// Opens the "Move / rename namespace" dialog for the namespace [oldPrefix].
///
/// Unlike the inline tree rename (which only edits the last segment in place),
/// this dialog lets the user edit the *full* target path, so a single
/// operation can:
/// - promote contents up one level (rootward), e.g. `a.b` → `a`,
/// - promote them to the top level by clearing the field (`a.b` → ``),
/// - move the whole subtree elsewhere, e.g. `a.b` → `c.d`,
/// - or plainly rename it.
///
/// It shows a live preview of every resulting `old → new` rename and flags
/// name conflicts before committing. Returns the committed target prefix on
/// success (so the caller can migrate UI state such as tree expansion), or
/// `null` if the user cancelled or nothing changed.
///
/// [initialPath] seeds the editable field. It defaults to [oldPrefix] (a plain
/// move/rename starting point); a drag-and-drop caller passes the proposed drop
/// target instead, so the dialog opens pre-filled with the dropped-to location
/// for the user to confirm or tweak.
Future<String?> showMoveNamespaceDialog({
  required BuildContext context,
  required StructureDesignerModel model,
  required String oldPrefix,
  String? initialPath,
}) {
  return _show(context, model, oldPrefix, _MoveKind.namespace,
      initialPath: initialPath);
}

/// Opens the "Move / rename network" dialog for the network leaf [oldName].
///
/// Same full-path editing as the namespace variant, scoped to one network: the
/// user edits the network's fully-qualified name, so it can be moved up a
/// level (`a.b.x` → `a.x`), to the top level (`a.b.x` → `x`), or into a
/// different namespace (`a.b.x` → `c.x`) in a single operation — none of which
/// the segment-only inline rename can do. Returns the committed full name on
/// success, or `null` on cancel / no-op.
///
/// [initialPath] seeds the editable field (defaults to [oldName]); a
/// drag-and-drop caller passes the proposed drop target instead.
Future<String?> showMoveNetworkDialog({
  required BuildContext context,
  required StructureDesignerModel model,
  required String oldName,
  String? initialPath,
}) {
  return _show(context, model, oldName, _MoveKind.network,
      initialPath: initialPath);
}

/// Opens the "Move / rename record def" dialog for the record-def leaf
/// [oldName]. Same full-path editing as the network variant — a record def is
/// now a first-class citizen of the namespace hierarchy, so it can be moved up
/// a level, to the top level, or into a different namespace in one operation.
/// Returns the committed full name on success, or `null` on cancel / no-op.
///
/// [initialPath] seeds the editable field (defaults to [oldName]); a
/// drag-and-drop caller passes the proposed drop target instead.
Future<String?> showMoveRecordDialog({
  required BuildContext context,
  required StructureDesignerModel model,
  required String oldName,
  String? initialPath,
}) {
  return _show(context, model, oldName, _MoveKind.record,
      initialPath: initialPath);
}

Future<String?> _show(
  BuildContext context,
  StructureDesignerModel model,
  String oldPath,
  _MoveKind kind, {
  String? initialPath,
}) {
  return showDialog<String>(
    context: context,
    barrierDismissible: false,
    builder: (context) => DraggableDialog(
      width: 460,
      dismissible: true,
      child: _MoveDialogBody(
        model: model,
        oldPath: oldPath,
        kind: kind,
        initialPath: initialPath,
      ),
    ),
  );
}

/// Client-side guard for obviously-malformed paths that the Rust
/// `is_valid_user_name` check does not reject (it allows dots freely). An
/// empty path is allowed here only for namespaces (means "promote to root");
/// for a network leaf the empty case is left to the backend preview, which
/// rejects it as an invalid name.
String? _localPathError(String path) {
  if (path.isEmpty) return null;
  if (path.startsWith('.') || path.endsWith('.')) {
    return 'Path cannot start or end with a dot.';
  }
  if (path.contains('..')) {
    return 'Path cannot contain an empty segment.';
  }
  return null;
}

class _MoveDialogBody extends StatefulWidget {
  final StructureDesignerModel model;
  final String oldPath;
  final _MoveKind kind;

  /// Pre-fills the editable target field. Null falls back to [oldPath].
  final String? initialPath;

  const _MoveDialogBody({
    required this.model,
    required this.oldPath,
    required this.kind,
    this.initialPath,
  });

  @override
  State<_MoveDialogBody> createState() => _MoveDialogBodyState();
}

class _MoveDialogBodyState extends State<_MoveDialogBody> {
  late final TextEditingController _controller;
  late final FocusNode _focusNode;
  APINamespaceRenamePreview? _preview;
  String? _localError;

  bool get _isNamespace => widget.kind == _MoveKind.namespace;

  /// The user-facing noun for the leaf kind being moved (singular).
  String get _leafNoun =>
      widget.kind == _MoveKind.record ? 'record def' : 'network';

  @override
  void initState() {
    super.initState();
    _controller =
        TextEditingController(text: widget.initialPath ?? widget.oldPath);
    _focusNode = FocusNode();
    _recompute();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _controller.selection = TextSelection(
        baseOffset: 0,
        extentOffset: _controller.text.length,
      );
      _focusNode.requestFocus();
    });
  }

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  String get _newPath => _controller.text.trim();

  void _recompute() {
    final newPath = _newPath;
    _localError = _localPathError(newPath);
    if (_localError != null) {
      _preview = null;
    } else if (_isNamespace) {
      _preview = widget.model.previewNamespaceRename(widget.oldPath, newPath);
    } else {
      // Both leaf kinds (network and record def) preview through the same
      // kind-agnostic backend entry point.
      _preview = widget.model.previewLeafRename(widget.oldPath, newPath);
    }
  }

  bool get _canApply {
    if (_localError != null) return false;
    final preview = _preview;
    if (preview == null || !preview.applicable) return false;
    // No-op: nothing to commit.
    return _newPath != widget.oldPath;
  }

  void _commit() {
    if (!_canApply) return;
    final newPath = _newPath;
    final bool success;
    switch (widget.kind) {
      case _MoveKind.namespace:
        success = widget.model.renameNamespace(widget.oldPath, newPath);
      case _MoveKind.network:
        success = widget.model.renameNodeNetwork(widget.oldPath, newPath);
      case _MoveKind.record:
        // renameRecordTypeDef returns an error message (or null on success).
        success =
            widget.model.renameRecordTypeDef(widget.oldPath, newPath) == null;
    }
    if (!mounted) return;
    if (success) {
      Navigator.of(context).pop(newPath);
    } else {
      // The preview said this was applicable, so a failure here is a race
      // (state changed underneath us). Re-preview to surface the new reason.
      setState(_recompute);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final preview = _preview;

    final title =
        _isNamespace ? 'Move / rename namespace' : 'Move / rename $_leafNoun';
    final subtitle = _isNamespace
        ? 'Editing "${widget.oldPath}". '
            'Clear the field to promote its contents to the top level.'
        : 'Editing "${widget.oldPath}". '
            'Use a dotted path to move it into a namespace, '
            'or a bare name for the top level.';
    final fieldLabel =
        _isNamespace ? 'New namespace path' : 'New $_leafNoun name';

    return Padding(
      padding: const EdgeInsets.all(20),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title, style: theme.textTheme.titleMedium),
          const SizedBox(height: 4),
          Text(
            subtitle,
            style: theme.textTheme.bodySmall?.copyWith(color: theme.hintColor),
          ),
          const SizedBox(height: 16),
          TextField(
            controller: _controller,
            focusNode: _focusNode,
            autofocus: true,
            decoration: InputDecoration(
              labelText: fieldLabel,
              hintText: _isNamespace ? '(empty = top level)' : null,
              border: const OutlineInputBorder(),
              isDense: true,
            ),
            onChanged: (_) => setState(_recompute),
            onSubmitted: (_) => _commit(),
          ),
          const SizedBox(height: 12),
          _buildStatus(theme, preview),
          const SizedBox(height: 8),
          _buildPreviewList(theme, preview),
          const SizedBox(height: 16),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              TextButton(
                onPressed: () => Navigator.of(context).pop(),
                child: const Text('Cancel'),
              ),
              const SizedBox(width: 8),
              ElevatedButton(
                onPressed: _canApply ? _commit : null,
                child: const Text('Apply'),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _buildStatus(ThemeData theme, APINamespaceRenamePreview? preview) {
    final error = theme.colorScheme.error;
    if (_localError != null) {
      return _statusLine(theme, _localError!, error);
    }
    if (preview == null) {
      return const SizedBox.shrink();
    }
    if (preview.isEmpty) {
      // A namespace folder can hold networks and record defs, so speak of
      // "items" generically; a leaf names its own kind.
      final leafCap =
          widget.kind == _MoveKind.record ? 'Record def' : 'Network';
      final msg = _isNamespace
          ? 'No items under this namespace.'
          : '$leafCap not found.';
      return _statusLine(theme, msg, theme.hintColor);
    }
    if (preview.hasInvalidNames) {
      final msg = _isNamespace
          ? 'Some resulting names are not valid.'
          : 'Not a valid name.';
      return _statusLine(theme, msg, error);
    }
    if (preview.hasConflicts) {
      final n = preview.items.where((i) => i.conflict).length;
      final msg = _isNamespace
          ? '$n name conflict${n == 1 ? '' : 's'} — resolve before applying.'
          : 'A type with that name already exists.';
      return _statusLine(theme, msg, error);
    }
    if (_newPath == widget.oldPath) {
      return _statusLine(theme, 'No change.', theme.hintColor);
    }
    if (_isNamespace) {
      final n = preview.items.length;
      // An empty folder has no entity items but is still applicable (it moves
      // the folder marker). See `doc/design_empty_folders.md`.
      if (n == 0) {
        return _statusLine(theme, 'Will move the folder.', theme.hintColor);
      }
      return _statusLine(
          theme, 'Will rename $n item${n == 1 ? '' : 's'}.', theme.hintColor);
    }
    return _statusLine(theme, 'Will move the $_leafNoun.', theme.hintColor);
  }

  Widget _statusLine(ThemeData theme, String text, Color color) {
    return Text(text, style: theme.textTheme.bodySmall?.copyWith(color: color));
  }

  Widget _buildPreviewList(
      ThemeData theme, APINamespaceRenamePreview? preview) {
    // A single-network preview is fully described by the status line + the
    // text field, so the per-row list only adds value for batch (namespace)
    // moves. Keep it for namespaces only.
    if (!_isNamespace || preview == null || preview.items.isEmpty) {
      return const SizedBox.shrink();
    }
    final error = theme.colorScheme.error;
    return Container(
      constraints: const BoxConstraints(maxHeight: 220),
      decoration: BoxDecoration(
        border: Border.all(color: theme.dividerColor),
        borderRadius: BorderRadius.circular(4),
      ),
      child: ListView.builder(
        shrinkWrap: true,
        itemCount: preview.items.length,
        itemBuilder: (context, index) {
          final item = preview.items[index];
          return Padding(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    '${item.oldName}  →  ${item.newName}',
                    style: theme.textTheme.bodySmall?.copyWith(
                      fontFamily: 'monospace',
                      color: item.conflict ? error : null,
                    ),
                  ),
                ),
                if (item.conflict)
                  Tooltip(
                    message: 'A type named "${item.newName}" already exists.',
                    child: Icon(Icons.error_outline, size: 16, color: error),
                  ),
              ],
            ),
          );
        },
      ),
    );
  }
}
