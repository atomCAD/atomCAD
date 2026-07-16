import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// The four hint kinds plus "no hint", as a flat pickable set. `APIFieldEditorHint`
/// itself carries payloads (`Enum` entries, `Range` bounds), so it cannot be a
/// dropdown value directly — this enum is the choice, the payload is edited by
/// the sub-editor below the dropdown.
enum FieldHintKind { element, color, enumChoice, range }

/// The field type a hint describes, looked at *through* one `Optional[..]`
/// wrapper — the mirror of Rust's `DataType::record_field_pin_type()`, which the
/// applicability rules in `FieldEditorHint::validate_for` are stated against.
/// Returns `null` for anything that is not a plain scalar (arrays, records,
/// structural types), which no hint applies to.
APIDataTypeBase? hintTargetBase(APIDataType type) {
  // An `Array[Int]` field is not an `Int` field: Rust peels `Optional`, never
  // `Array`, so no hint applies here.
  if (type.array) return null;
  if (type.dataTypeBase != APIDataTypeBase.optional) return type.dataTypeBase;
  if (type.children.length != 1) return null;
  final inner = type.children.first;
  if (inner.array) return null;
  return inner.dataTypeBase;
}

/// Which hints are well-formed on a field of `type`, per §Applicability of
/// `doc/design_array_node_and_field_hints.md`. This is a **convenience filter**:
/// the Rust setter re-checks and rejects, so a stale UI can never smuggle a
/// mismatched hint into the registry.
List<FieldHintKind> applicableHintKinds(APIDataType type) {
  switch (hintTargetBase(type)) {
    case APIDataTypeBase.int:
      return const [FieldHintKind.element, FieldHintKind.range];
    case APIDataTypeBase.float:
      return const [FieldHintKind.range];
    case APIDataTypeBase.string:
      return const [FieldHintKind.enumChoice];
    case APIDataTypeBase.vec3:
      return const [FieldHintKind.color];
    default:
      return const [];
  }
}

FieldHintKind hintKindOf(APIFieldEditorHint hint) => switch (hint) {
      APIFieldEditorHint_Element() => FieldHintKind.element,
      APIFieldEditorHint_Color() => FieldHintKind.color,
      APIFieldEditorHint_Enum() => FieldHintKind.enumChoice,
      APIFieldEditorHint_Range() => FieldHintKind.range,
    };

/// True when `hint` is applicable to `type`. Used by the schema editor to drop a
/// hint that a *retype* invalidates, in the same update that carries the
/// retype — so the strict Rust-side rejection is never hit in normal UI flow.
bool isHintApplicable(APIFieldEditorHint hint, APIDataType type) =>
    applicableHintKinds(type).contains(hintKindOf(hint));

/// Mirrors the well-formedness half of Rust's `FieldEditorHint::validate_for`.
/// The schema editor gates its commit on this so a half-typed `Enum` entry or an
/// inverted `Range` parks in local state instead of bouncing off the backend as
/// a snackbar on every keystroke.
String? validateHintWellFormed(APIFieldEditorHint? hint) {
  switch (hint) {
    case null:
    case APIFieldEditorHint_Element():
    case APIFieldEditorHint_Color():
      return null;
    case APIFieldEditorHint_Enum(field0: final entries):
      if (entries.isEmpty) return 'Enum hint needs at least one entry';
      final seen = <String>{};
      for (final entry in entries) {
        if (entry.isEmpty) return 'Enum hint entries must not be empty';
        if (entry.trim() != entry) {
          return "Enum hint entry '$entry' has leading or trailing whitespace";
        }
        if (!seen.add(entry)) return "duplicate Enum hint entry '$entry'";
      }
      return null;
    case APIFieldEditorHint_Range(min: final min, max: final max):
      if (!min.isFinite || !max.isFinite) {
        return 'Range hint bounds must be finite';
      }
      if (min >= max) {
        return 'Range hint requires min < max (got min = $min, max = $max)';
      }
      return null;
  }
}

String _hintKindLabel(FieldHintKind kind) => switch (kind) {
      FieldHintKind.element => 'Element',
      FieldHintKind.color => 'Color',
      FieldHintKind.enumChoice => 'Enum',
      FieldHintKind.range => 'Range',
    };

/// A freshly picked hint's payload defaults. `Enum` seeds one entry and `Range`
/// seeds 0..1 so the pick is immediately well-formed and commits on the spot —
/// an empty entry list would be rejected before the user could type into it.
APIFieldEditorHint _seedHint(FieldHintKind kind) => switch (kind) {
      FieldHintKind.element => const APIFieldEditorHint.element(),
      FieldHintKind.color => const APIFieldEditorHint.color(),
      FieldHintKind.enumChoice => const APIFieldEditorHint.enum_(['option1']),
      FieldHintKind.range => const APIFieldEditorHint.range(min: 0.0, max: 1.0),
    };

/// Per-field editor-hint control for one `SchemaEditor` row: a kind dropdown
/// filtered to the hints the field's type admits, plus a payload sub-editor for
/// the two hints that carry one (`Enum` entries, `Range` bounds).
///
/// Renders nothing at all when the field's type admits no hint — most field
/// types don't, and an always-present dead dropdown would be noise in every row.
class SchemaFieldHintEditor extends StatelessWidget {
  final APIDataType fieldType;
  final APIFieldEditorHint? hint;

  /// Fires with the new hint (`null` = cleared). The parent commits.
  final ValueChanged<APIFieldEditorHint?> onChanged;

  const SchemaFieldHintEditor({
    super.key,
    required this.fieldType,
    required this.hint,
    required this.onChanged,
  });

  @override
  Widget build(BuildContext context) {
    final kinds = applicableHintKinds(fieldType);
    if (kinds.isEmpty) return const SizedBox.shrink();

    // A hint the current type no longer admits (only reachable from a def
    // authored outside this panel — the editor clears such hints on retype).
    // Show it as a flagged extra entry rather than silently reading as "None".
    final current = hint == null ? null : hintKindOf(hint!);
    final isForeign = current != null && !kinds.contains(current);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        DropdownButtonFormField<FieldHintKind?>(
          decoration: const InputDecoration(
            isDense: true,
            labelText: 'Editor hint',
            contentPadding: EdgeInsets.symmetric(horizontal: 8, vertical: 8),
            border: OutlineInputBorder(),
          ),
          isExpanded: true,
          value: current,
          items: [
            const DropdownMenuItem<FieldHintKind?>(
              value: null,
              child: Text('None'),
            ),
            if (isForeign)
              DropdownMenuItem<FieldHintKind?>(
                value: current,
                child: Text(
                  '${_hintKindLabel(current)}  — not valid for this type',
                  style: const TextStyle(
                    fontStyle: FontStyle.italic,
                    color: Colors.redAccent,
                  ),
                ),
              ),
            for (final kind in kinds)
              DropdownMenuItem<FieldHintKind?>(
                value: kind,
                child: Text(_hintKindLabel(kind)),
              ),
          ],
          onChanged: (kind) {
            if (kind == current) return;
            onChanged(kind == null ? null : _seedHint(kind));
          },
        ),
        if (hint case APIFieldEditorHint_Enum(field0: final entries))
          if (kinds.contains(FieldHintKind.enumChoice))
            _EnumEntriesEditor(
              entries: entries,
              onChanged: (v) => onChanged(APIFieldEditorHint.enum_(v)),
            ),
        if (hint case APIFieldEditorHint_Range(min: final min, max: final max))
          if (kinds.contains(FieldHintKind.range))
            _RangeBoundsEditor(
              min: min,
              max: max,
              onChanged: (lo, hi) =>
                  onChanged(APIFieldEditorHint.range(min: lo, max: hi)),
            ),
      ],
    );
  }
}

/// The `Enum` hint's entry list: one text field per choice, plus add/remove.
/// Entries commit on focus loss / Enter (the `switch_editor` cadence) so a
/// half-typed entry never round-trips through the registry.
class _EnumEntriesEditor extends StatefulWidget {
  final List<String> entries;
  final ValueChanged<List<String>> onChanged;

  const _EnumEntriesEditor({required this.entries, required this.onChanged});

  @override
  State<_EnumEntriesEditor> createState() => _EnumEntriesEditorState();
}

class _EnumEntriesEditorState extends State<_EnumEntriesEditor> {
  final List<TextEditingController> _controllers = [];
  final List<FocusNode> _focusNodes = [];

  @override
  void initState() {
    super.initState();
    _syncControllers();
  }

  @override
  void didUpdateWidget(_EnumEntriesEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    _syncControllers();
  }

  @override
  void dispose() {
    for (final c in _controllers) {
      c.dispose();
    }
    for (final f in _focusNodes) {
      f.dispose();
    }
    super.dispose();
  }

  /// Positional controllers, refreshed only for fields the user is not typing
  /// in — same discipline as `switch_editor`'s case list.
  void _syncControllers() {
    while (_controllers.length < widget.entries.length) {
      final index = _controllers.length;
      final focusNode = FocusNode();
      focusNode.addListener(() {
        if (!focusNode.hasFocus) _commitEntry(index);
      });
      _controllers.add(TextEditingController());
      _focusNodes.add(focusNode);
    }
    while (_controllers.length > widget.entries.length) {
      _controllers.removeLast().dispose();
      _focusNodes.removeLast().dispose();
    }
    for (int i = 0; i < widget.entries.length; i++) {
      if (!_focusNodes[i].hasFocus &&
          _controllers[i].text != widget.entries[i]) {
        _controllers[i].text = widget.entries[i];
      }
    }
  }

  void _commitEntry(int index) {
    if (!mounted || index >= widget.entries.length) return;
    final text = _controllers[index].text;
    if (text == widget.entries[index]) return;
    final updated = [...widget.entries];
    updated[index] = text;
    widget.onChanged(updated);
  }

  String _nextEntry() {
    final existing = widget.entries.toSet();
    var i = widget.entries.length + 1;
    while (existing.contains('option$i')) {
      i++;
    }
    return 'option$i';
  }

  @override
  Widget build(BuildContext context) {
    final canDelete = widget.entries.length > 1;
    return Padding(
      padding: const EdgeInsets.only(top: 6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (int i = 0; i < widget.entries.length; i++)
            Padding(
              padding: const EdgeInsets.only(bottom: 4),
              child: Row(
                children: [
                  Expanded(
                    child: TextField(
                      controller: _controllers[i],
                      focusNode: _focusNodes[i],
                      style: AppTextStyles.small,
                      decoration: InputDecoration(
                        isDense: true,
                        contentPadding: const EdgeInsets.symmetric(
                          horizontal: 8,
                          vertical: 6,
                        ),
                        border: const OutlineInputBorder(),
                        errorText: _entryError(i),
                        errorStyle: const TextStyle(fontSize: 10),
                      ),
                      onSubmitted: (_) => _commitEntry(i),
                    ),
                  ),
                  IconButton(
                    tooltip: canDelete
                        ? 'Remove choice'
                        : 'An Enum hint needs at least one choice',
                    visualDensity: VisualDensity.compact,
                    onPressed: canDelete
                        ? () =>
                            widget.onChanged([...widget.entries]..removeAt(i))
                        : null,
                    icon: const Icon(Icons.close, size: 16),
                  ),
                ],
              ),
            ),
          TextButton.icon(
            onPressed: () =>
                widget.onChanged([...widget.entries, _nextEntry()]),
            icon: const Icon(Icons.add, size: 14),
            label: Text('Add choice', style: AppTextStyles.small),
            style: TextButton.styleFrom(
              padding: const EdgeInsets.symmetric(horizontal: 8),
              minimumSize: Size.zero,
              tapTargetSize: MaterialTapTargetSize.shrinkWrap,
            ),
          ),
        ],
      ),
    );
  }

  /// Per-entry echo of the well-formedness rules, so the offending field says
  /// why the def is not committing rather than the whole row going quiet.
  String? _entryError(int index) {
    final entry = widget.entries[index];
    if (entry.isEmpty) return 'Must not be empty';
    if (entry.trim() != entry) return 'No leading/trailing spaces';
    for (int i = 0; i < widget.entries.length; i++) {
      if (i != index && widget.entries[i] == entry) return 'Duplicate';
    }
    return null;
  }
}

/// The `Range` hint's bounds. `min < max` is enforced by the commit gate, not
/// here — the fields stay freely editable so passing through an inverted
/// intermediate state (typing a new min above the old max) is not blocked.
class _RangeBoundsEditor extends StatelessWidget {
  final double min;
  final double max;
  final void Function(double min, double max) onChanged;

  const _RangeBoundsEditor({
    required this.min,
    required this.max,
    required this.onChanged,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(top: 6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: FloatInput(
                  label: 'Min',
                  value: min,
                  onChanged: (v) => onChanged(v, max),
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: FloatInput(
                  label: 'Max',
                  value: max,
                  onChanged: (v) => onChanged(min, v),
                ),
              ),
            ],
          ),
          if (min >= max)
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Text(
                'Min must be below Max',
                style: AppTextStyles.small.copyWith(color: Colors.red),
              ),
            ),
        ],
      ),
    );
  }
}
