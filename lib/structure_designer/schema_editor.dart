import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/structure_designer/identifier_validation.dart';
import 'package:flutter_cad/structure_designer/schema_field_hint_editor.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Editor for a single record type def's authored field list. Replaces the
/// network editor in the main content area when a record def is the active
/// item in the user-types panel.
///
/// Edits commit to the registry on every successful change (no Apply button).
/// Cycle violations are reported via a snackbar; on failure the local UI
/// state reverts to the last server-known field list.
class SchemaEditor extends StatefulWidget {
  final StructureDesignerModel model;
  final String defName;

  const SchemaEditor({
    super.key,
    required this.model,
    required this.defName,
  });

  @override
  State<SchemaEditor> createState() => _SchemaEditorState();
}

class _SchemaEditorState extends State<SchemaEditor> {
  /// Local in-flight copy of the def's field list. We mutate this directly
  /// for snappy UI then commit via `update_record_type_def`. On commit
  /// failure (cycle, etc.) we revert to the registry's view by re-fetching.
  List<APIRecordTypeField> _fields = [];
  String? _lastFetchedDefName;

  /// Per-row name controllers. Reused across builds so cursor position is
  /// preserved while editing.
  final Map<int, TextEditingController> _nameControllers = {};
  final Map<int, FocusNode> _nameFocusNodes = {};

  @override
  void initState() {
    super.initState();
    _refetch();
  }

  @override
  void didUpdateWidget(SchemaEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    // Refetch when the active def changes or when the underlying registry
    // may have moved (rename, undo, external edit).
    if (oldWidget.defName != widget.defName ||
        _lastFetchedDefName != widget.defName) {
      _refetch();
    }
  }

  void _refetch() {
    final def = widget.model.getRecordTypeDef(widget.defName);
    setState(() {
      _fields = def?.fields.toList() ?? [];
      _lastFetchedDefName = widget.defName;
    });
    _syncControllers();
  }

  void _syncControllers() {
    // Drop controllers/focus nodes for rows that no longer exist.
    final activeIndices = {for (int i = 0; i < _fields.length; i++) i};
    for (final key in _nameControllers.keys.toList()) {
      if (!activeIndices.contains(key)) {
        _nameControllers.remove(key)?.dispose();
        _nameFocusNodes.remove(key)?.dispose();
      }
    }
    for (int i = 0; i < _fields.length; i++) {
      final ctrl = _nameControllers.putIfAbsent(
          i, () => TextEditingController(text: _fields[i].name));
      if (ctrl.text != _fields[i].name) {
        ctrl.text = _fields[i].name;
      }
      _nameFocusNodes.putIfAbsent(i, FocusNode.new);
    }
  }

  @override
  void dispose() {
    for (final c in _nameControllers.values) {
      c.dispose();
    }
    for (final f in _nameFocusNodes.values) {
      f.dispose();
    }
    super.dispose();
  }

  /// Commit the current `_fields` list to the registry. On failure, show a
  /// snackbar with the error and revert by re-fetching.
  void _commit() {
    final error = widget.model.updateRecordTypeDef(widget.defName, _fields);
    if (error != null) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(error)),
        );
      }
      _refetch();
    } else {
      // Server-side may have canonicalized; refetch to stay in sync.
      _refetch();
    }
  }

  /// Validates that field names within the in-flight list are non-empty,
  /// valid identifiers, and distinct, and that every editor hint is
  /// well-formed. Returns the offending field name (highlighted with a red ring
  /// in the UI) or null if everything is fine.
  ///
  /// Hints are gated here rather than left to the backend because their
  /// sub-editors are *typed into*: an entry cleared on the way to a new name, or
  /// a `Range` min raised above the old max, is a transient state, not an error
  /// worth a snackbar and a revert. The offending row parks in local state and
  /// says so inline until it is well-formed again.
  String? _findDuplicateOrInvalid() {
    final seen = <String>{};
    for (final f in _fields) {
      if (validateUserName(f.name) != null) {
        return f.name;
      }
      if (!seen.add(f.name)) {
        return f.name;
      }
      if (validateHintWellFormed(f.hint) != null) {
        return f.name;
      }
    }
    return null;
  }

  bool _isFieldNameValid(int index) {
    final f = _fields[index];
    if (validateUserName(f.name) != null) return false;
    // Duplicate check
    for (int i = 0; i < _fields.length; i++) {
      if (i != index && _fields[i].name == f.name) return false;
    }
    return true;
  }

  void _addField() {
    // Default new field type: Int. Keeps the row minimally usable until the
    // user picks something more meaningful.
    final newField = APIRecordTypeField(
      // New row: no editing identity yet — the backend allocates a fresh
      // FieldId on commit. See doc/design_record_field_identity.md.
      id: null,
      name: _uniqueDefaultName(),
      dataType: const APIDataType(
        dataTypeBase: APIDataTypeBase.int,
        customDataType: null,
        array: false,
        children: [],
      ),
      hint: null,
    );
    setState(() {
      _fields = [..._fields, newField];
    });
    _syncControllers();
    if (_findDuplicateOrInvalid() == null) {
      _commit();
    }
  }

  String _uniqueDefaultName() {
    int n = _fields.length;
    while (true) {
      final candidate = 'field${n + 1}';
      if (!_fields.any((f) => f.name == candidate)) return candidate;
      n++;
    }
  }

  void _deleteField(int index) {
    setState(() {
      _fields = [..._fields]..removeAt(index);
    });
    _syncControllers();
    if (_findDuplicateOrInvalid() == null) {
      _commit();
    }
  }

  void _reorderField(int oldIndex, int newIndex) {
    if (newIndex > oldIndex) newIndex -= 1;
    setState(() {
      final updated = [..._fields];
      final item = updated.removeAt(oldIndex);
      updated.insert(newIndex, item);
      _fields = updated;
    });
    _syncControllers();
    if (_findDuplicateOrInvalid() == null) {
      _commit();
    }
  }

  void _commitNameChange(int index, String newName) {
    if (_fields[index].name == newName) return;
    setState(() {
      _fields = [
        for (int i = 0; i < _fields.length; i++)
          if (i == index)
            APIRecordTypeField(
              // Preserve the row's editing identity so the rename is matched by
              // id, not name — this is what keeps wires attached (#377).
              id: _fields[i].id,
              name: newName,
              dataType: _fields[i].dataType,
              hint: _fields[i].hint,
            )
          else
            _fields[i],
      ];
    });
    if (_findDuplicateOrInvalid() == null) {
      _commit();
    }
  }

  void _commitTypeChange(int index, APIDataType newType) {
    // A hint the new type does not admit is dropped in the *same* update that
    // carries the retype — the backend rejects the pair outright, and bouncing
    // the user's retype off a hint they can no longer even see would be
    // baffling. Hints are cosmetic; a retype is the real edit.
    final hint = _fields[index].hint;
    final keptHint =
        (hint != null && isHintApplicable(hint, newType)) ? hint : null;
    setState(() {
      _fields = [
        for (int i = 0; i < _fields.length; i++)
          if (i == index)
            APIRecordTypeField(
              // Retype keeps the field's identity (and its wire).
              id: _fields[i].id,
              name: _fields[i].name,
              dataType: newType,
              hint: keptHint,
            )
          else
            _fields[i],
      ];
    });
    if (_findDuplicateOrInvalid() == null) {
      _commit();
    }
  }

  void _commitHintChange(int index, APIFieldEditorHint? newHint) {
    setState(() {
      _fields = [
        for (int i = 0; i < _fields.length; i++)
          if (i == index)
            APIRecordTypeField(
              id: _fields[i].id,
              name: _fields[i].name,
              dataType: _fields[i].dataType,
              hint: newHint,
            )
          else
            _fields[i],
      ];
    });
    if (_findDuplicateOrInvalid() == null) {
      _commit();
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _buildHeader(),
        const Divider(height: 1),
        Expanded(child: _buildFieldList()),
        _buildAddFieldButton(),
      ],
    );
  }

  Widget _buildHeader() {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: Colors.grey.shade200,
        border: Border(
          bottom: BorderSide(color: Colors.grey.shade400, width: 1),
        ),
      ),
      child: Row(
        children: [
          const Icon(Icons.data_object, size: 18),
          const SizedBox(width: 8),
          Text(widget.defName,
              style:
                  AppTextStyles.regular.copyWith(fontWeight: FontWeight.w600)),
          const Spacer(),
          Text(
            '${_fields.length} field${_fields.length == 1 ? '' : 's'}',
            style: AppTextStyles.small.copyWith(color: Colors.grey.shade700),
          ),
        ],
      ),
    );
  }

  Widget _buildFieldList() {
    if (_fields.isEmpty) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Text(
            'No fields. Use "+ Add field" to add one.',
            style: AppTextStyles.small.copyWith(color: Colors.grey.shade600),
          ),
        ),
      );
    }
    return ReorderableListView.builder(
      buildDefaultDragHandles: false,
      itemCount: _fields.length,
      onReorder: _reorderField,
      itemBuilder: (context, index) => _buildFieldRow(index),
    );
  }

  Widget _buildFieldRow(int index) {
    final f = _fields[index];
    final controller = _nameControllers[index]!;
    final focusNode = _nameFocusNodes[index]!;
    final nameValid = _isFieldNameValid(index);
    final tooltipMessage = !nameValid
        ? (validateUserName(f.name) ?? 'Field "${f.name}" is already declared')
        : '';
    return Padding(
      key: ValueKey('field_row_$index:${f.name}'),
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          ReorderableDragStartListener(
            index: index,
            child: const Padding(
              padding: EdgeInsets.only(top: 12),
              child: Icon(Icons.drag_indicator, size: 18, color: Colors.grey),
            ),
          ),
          const SizedBox(width: 8),
          // Name input
          SizedBox(
            width: 140,
            child: Tooltip(
              message: tooltipMessage,
              child: Focus(
                onFocusChange: (hasFocus) {
                  if (!hasFocus) {
                    _commitNameChange(index, controller.text);
                  }
                },
                child: TextField(
                  controller: controller,
                  focusNode: focusNode,
                  decoration: InputDecoration(
                    isDense: true,
                    contentPadding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 8,
                    ),
                    enabledBorder: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(4),
                      borderSide: BorderSide(
                        color: nameValid ? Colors.grey : Colors.red,
                        width: nameValid ? 1.0 : 1.5,
                      ),
                    ),
                    focusedBorder: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(4),
                      borderSide: BorderSide(
                        color: nameValid ? Colors.blue : Colors.red,
                        width: 2.0,
                      ),
                    ),
                  ),
                  style: AppTextStyles.regular,
                  onSubmitted: (v) => _commitNameChange(index, v),
                  inputFormatters: const <TextInputFormatter>[],
                ),
              ),
            ),
          ),
          const SizedBox(width: 12),
          // Type cell
          Expanded(
            child: DataTypeInput(
              label: 'Type',
              // Record fields are the only place `Optional[T]` is meaningful.
              allowOptional: true,
              value: f.dataType,
              onChanged: (newType) => _commitTypeChange(index, newType),
            ),
          ),
          const SizedBox(width: 12),
          // Editor-hint cell. Collapses to nothing for the many field types no
          // hint applies to, so only the rows that can carry one show a control.
          SizedBox(
            width: 200,
            child: SchemaFieldHintEditor(
              fieldType: f.dataType,
              hint: f.hint,
              onChanged: (newHint) => _commitHintChange(index, newHint),
            ),
          ),
          // Delete button
          IconButton(
            tooltip: 'Delete field',
            onPressed: () => _deleteField(index),
            icon: const Icon(Icons.delete_outline, size: 20),
          ),
        ],
      ),
    );
  }

  Widget _buildAddFieldButton() {
    return Container(
      padding: const EdgeInsets.all(8),
      decoration: BoxDecoration(
        border: Border(
          top: BorderSide(color: Colors.grey.shade300, width: 1),
        ),
      ),
      child: Align(
        alignment: Alignment.centerLeft,
        child: TextButton.icon(
          onPressed: _addField,
          icon: const Icon(Icons.add, size: 18),
          label: const Text('Add field'),
        ),
      ),
    );
  }
}
