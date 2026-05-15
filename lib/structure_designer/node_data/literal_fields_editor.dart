import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/inputs/ivec2_input.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/vec2_input.dart';
import 'package:flutter_cad/inputs/vec3_input.dart';
import 'package:flutter_cad/structure_designer/node_data/matrix_cell.dart';

/// Auto-generated property panel for a list of typed literal-valued fields.
/// One row per field; complex/abstract types are filtered out upstream by the
/// data source. Each row has three visual states:
///
///   - **Stored**: a literal is stored — full opacity, clear button shown.
///   - **Placeholder**: no stored literal, pin unwired — dimmed, pre-seeded
///     with `default_value ?? typeZero`, still fully interactive. Labeled
///     `(default)` when a default exists, otherwise `(unset)`.
///   - **Wired**: the pin has a wire — dimmed + non-interactive; the stored
///     literal (if any) is preserved so it re-activates on disconnect.
///
/// Used by both `CustomNodeEditor` and `RecordConstructEditor`. See
/// `doc/design_record_construct_property_panel.md`.
class LiteralFieldsEditor extends StatelessWidget {
  /// Rendered above the field list. Use [SizedBox.shrink] to omit.
  final Widget header;

  /// The fields to render. May be empty (renders [emptyMessage] instead).
  final List<APILiteralField> fields;

  /// Italic note shown when [fields] is empty.
  final String emptyMessage;

  /// Called when the user edits a field's value.
  final void Function(String name, APILiteralValue value) onSet;

  /// Called when the user clicks the clear button on a stored row.
  final void Function(String name) onClear;

  /// Test-key prefix used to identify rows, clear buttons, inputs and matrix
  /// cells. Defaults to `literal_field`; existing call sites override to
  /// preserve their integration-test selectors.
  final String keyPrefix;

  const LiteralFieldsEditor({
    super.key,
    required this.header,
    required this.fields,
    required this.emptyMessage,
    required this.onSet,
    required this.onClear,
    this.keyPrefix = 'literal_field',
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        header,
        if (header is! SizedBox) const SizedBox(height: 8),
        if (fields.isEmpty)
          Text(
            emptyMessage,
            style: const TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
          )
        else
          for (final field in fields) ...[
            _buildRow(context, field),
            const SizedBox(height: 10),
          ],
      ],
    );
  }

  Widget _buildRow(BuildContext context, APILiteralField field) {
    final effective =
        field.storedValue ?? field.defaultValue ?? _typeZero(field.dataType);
    final isWired = field.isWired;
    final isPlaceholder = field.storedValue == null;
    final hasDefault = field.defaultValue != null;
    final opacity = isWired ? 0.45 : (isPlaceholder ? 0.55 : 1.0);

    return Padding(
      key: ValueKey('${keyPrefix}_${field.name}'),
      padding: EdgeInsets.zero,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(text: field.name),
                      if (isPlaceholder && !isWired)
                        TextSpan(
                          text: hasDefault ? '  (default)' : '  (unset)',
                          style: const TextStyle(
                            fontStyle: FontStyle.italic,
                            color: Colors.grey,
                          ),
                        ),
                    ],
                  ),
                ),
              ),
              // Clear button is only meaningful in the Stored state.
              if (!isPlaceholder && !isWired)
                IconButton(
                  key: ValueKey('${keyPrefix}_clear_${field.name}'),
                  icon: const Icon(Icons.clear, size: 16),
                  tooltip: 'Clear stored value',
                  visualDensity: VisualDensity.compact,
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(),
                  onPressed: () => onClear(field.name),
                ),
            ],
          ),
          const SizedBox(height: 2),
          // Uniform wrapper: a stable element-tree shape keeps the inner
          // input widget's State (controller, cursor) alive across the
          // Placeholder -> Stored transition.
          IgnorePointer(
            ignoring: isWired,
            child: Opacity(
              opacity: opacity,
              child: _buildInput(context, field, effective),
            ),
          ),
          if (isWired)
            const Padding(
              padding: EdgeInsets.only(top: 2),
              child: Text(
                'Supplied by wired input. Disconnect to edit inline.',
                style: TextStyle(fontSize: 11, fontStyle: FontStyle.italic),
              ),
            ),
        ],
      ),
    );
  }

  void _set(APILiteralField field, APILiteralValue value) {
    onSet(field.name, value);
  }

  Widget _buildInput(
    BuildContext context,
    APILiteralField field,
    APILiteralValue effective,
  ) {
    final inputKey = Key('${keyPrefix}_input_${field.name}');
    switch (field.dataType) {
      case APISimpleParamType.bool:
        return CheckboxListTile(
          key: inputKey,
          title: const Text('Value'),
          value: _asBool(effective),
          onChanged: (v) {
            if (v != null) _set(field, APILiteralValue.bool(v));
          },
          controlAffinity: ListTileControlAffinity.leading,
          contentPadding: EdgeInsets.zero,
          dense: true,
        );
      case APISimpleParamType.int:
        return IntInput(
          label: '',
          inputKey: inputKey,
          value: _asInt(effective),
          onChanged: (v) => _set(field, APILiteralValue.int(v)),
        );
      case APISimpleParamType.float:
        return FloatInput(
          label: '',
          inputKey: inputKey,
          value: _asFloat(effective),
          onChanged: (v) => _set(field, APILiteralValue.float(v)),
        );
      case APISimpleParamType.str:
        return StringInput(
          label: '',
          inputKey: inputKey,
          value: _asStr(effective),
          onChanged: (v) => _set(field, APILiteralValue.str(v)),
        );
      case APISimpleParamType.iVec2:
        return IVec2Input(
          label: '',
          value: _asIVec2(effective),
          onChanged: (v) => _set(field, APILiteralValue.iVec2(v)),
        );
      case APISimpleParamType.iVec3:
        return IVec3Input(
          label: '',
          value: _asIVec3(effective),
          onChanged: (v) => _set(field, APILiteralValue.iVec3(v)),
        );
      case APISimpleParamType.vec2:
        return Vec2Input(
          label: '',
          value: _asVec2(effective),
          onChanged: (v) => _set(field, APILiteralValue.vec2(v)),
        );
      case APISimpleParamType.vec3:
        return Vec3Input(
          label: '',
          value: _asVec3(effective),
          onChanged: (v) => _set(field, APILiteralValue.vec3(v)),
        );
      case APISimpleParamType.iMat3:
        return _buildIMat3(field, _asIMat3(effective));
      case APISimpleParamType.mat3:
        return _buildMat3(field, _asMat3(effective));
    }
  }

  Widget _buildIMat3(APILiteralField field, List<Int32List> m) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var r = 0; r < 3; r++)
          Padding(
            padding: const EdgeInsets.only(bottom: 4),
            child: Row(
              children: [
                for (var c = 0; c < 3; c++)
                  Padding(
                    padding: const EdgeInsets.only(right: 4),
                    child: IntMatrixCell(
                      key: Key('${keyPrefix}_${field.name}_cell_${r}_$c'),
                      value: _imat3At(m, r, c),
                      enabled: true,
                      onChanged: (v) {
                        final next = <Int32List>[
                          for (var rr = 0; rr < 3; rr++)
                            Int32List.fromList([
                              for (var cc = 0; cc < 3; cc++)
                                _imat3At(m, rr, cc),
                            ]),
                        ];
                        next[r][c] = v;
                        _set(field, APILiteralValue.iMat3(next));
                      },
                    ),
                  ),
              ],
            ),
          ),
      ],
    );
  }

  Widget _buildMat3(APILiteralField field, List<Float64List> m) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var r = 0; r < 3; r++)
          Padding(
            padding: const EdgeInsets.only(bottom: 4),
            child: Row(
              children: [
                for (var c = 0; c < 3; c++)
                  Padding(
                    padding: const EdgeInsets.only(right: 4),
                    child: FloatMatrixCell(
                      key: Key('${keyPrefix}_${field.name}_cell_${r}_$c'),
                      value: _mat3At(m, r, c),
                      enabled: true,
                      onChanged: (v) {
                        final next = <Float64List>[
                          for (var rr = 0; rr < 3; rr++)
                            Float64List.fromList([
                              for (var cc = 0; cc < 3; cc++)
                                _mat3At(m, rr, cc),
                            ]),
                        ];
                        next[r][c] = v;
                        _set(field, APILiteralValue.mat3(next));
                      },
                    ),
                  ),
              ],
            ),
          ),
      ],
    );
  }

  // --- Defensive extractors: fall back to a type zero if the carried variant
  // does not match the field's declared type (e.g. a default that resolved to
  // a different simple type, or a stale stored value after a field retyping).

  static bool _asBool(APILiteralValue v) =>
      v is APILiteralValue_Bool ? v.field0 : false;

  static int _asInt(APILiteralValue v) =>
      v is APILiteralValue_Int ? v.field0 : 0;

  static double _asFloat(APILiteralValue v) =>
      v is APILiteralValue_Float ? v.field0 : 0.0;

  static String _asStr(APILiteralValue v) =>
      v is APILiteralValue_Str ? v.field0 : '';

  static APIIVec2 _asIVec2(APILiteralValue v) =>
      v is APILiteralValue_IVec2 ? v.field0 : const APIIVec2(x: 0, y: 0);

  static APIIVec3 _asIVec3(APILiteralValue v) =>
      v is APILiteralValue_IVec3 ? v.field0 : const APIIVec3(x: 0, y: 0, z: 0);

  static APIVec2 _asVec2(APILiteralValue v) =>
      v is APILiteralValue_Vec2 ? v.field0 : const APIVec2(x: 0, y: 0);

  static APIVec3 _asVec3(APILiteralValue v) =>
      v is APILiteralValue_Vec3 ? v.field0 : const APIVec3(x: 0, y: 0, z: 0);

  static List<Int32List> _asIMat3(APILiteralValue v) =>
      v is APILiteralValue_IMat3 ? v.field0 : _identityIMat3();

  static List<Float64List> _asMat3(APILiteralValue v) =>
      v is APILiteralValue_Mat3 ? v.field0 : _identityMat3();

  static int _imat3At(List<Int32List> m, int r, int c) =>
      (r < m.length && c < m[r].length) ? m[r][c] : 0;

  static double _mat3At(List<Float64List> m, int r, int c) =>
      (r < m.length && c < m[r].length) ? m[r][c] : 0.0;

  static List<Int32List> _identityIMat3() => [
        Int32List.fromList([1, 0, 0]),
        Int32List.fromList([0, 1, 0]),
        Int32List.fromList([0, 0, 1]),
      ];

  static List<Float64List> _identityMat3() => [
        Float64List.fromList([1.0, 0.0, 0.0]),
        Float64List.fromList([0.0, 1.0, 0.0]),
        Float64List.fromList([0.0, 0.0, 1.0]),
      ];

  static APILiteralValue _typeZero(APISimpleParamType t) {
    switch (t) {
      case APISimpleParamType.bool:
        return const APILiteralValue.bool(false);
      case APISimpleParamType.int:
        return const APILiteralValue.int(0);
      case APISimpleParamType.float:
        return const APILiteralValue.float(0.0);
      case APISimpleParamType.str:
        return const APILiteralValue.str('');
      case APISimpleParamType.iVec2:
        return const APILiteralValue.iVec2(APIIVec2(x: 0, y: 0));
      case APISimpleParamType.iVec3:
        return const APILiteralValue.iVec3(APIIVec3(x: 0, y: 0, z: 0));
      case APISimpleParamType.vec2:
        return const APILiteralValue.vec2(APIVec2(x: 0, y: 0));
      case APISimpleParamType.vec3:
        return const APILiteralValue.vec3(APIVec3(x: 0, y: 0, z: 0));
      case APISimpleParamType.iMat3:
        return APILiteralValue.iMat3(_identityIMat3());
      case APISimpleParamType.mat3:
        return APILiteralValue.mat3(_identityMat3());
    }
  }
}
