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
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Auto-generated property panel for custom nodes (node types implemented by
/// user-defined node networks). Renders one row per simple-typed input pin,
/// reusing the built-in primitive input widgets unmodified.
///
/// Each row has three visual states:
///   - **Stored**: a literal is stored — full opacity, clear button shown.
///   - **Placeholder**: no stored literal, pin unwired — dimmed, pre-seeded
///     with the resolved default, still fully interactive.
///   - **Wired**: the pin has a wire — dimmed + non-interactive; the stored
///     literal (if any) is preserved so it re-activates on disconnect.
///
/// See `doc/design_custom_node_property_panel.md`.
class CustomNodeEditor extends StatelessWidget {
  final BigInt nodeId;
  final String nodeTypeName;
  final List<APILiteralField> params;
  final StructureDesignerModel model;

  const CustomNodeEditor({
    super.key,
    required this.nodeId,
    required this.nodeTypeName,
    required this.params,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          NodeEditorHeader(
            title: 'Custom Node Properties',
            nodeTypeName: nodeTypeName,
          ),
          const SizedBox(height: 8),
          if (params.isEmpty)
            const Text(
              'This custom node has no editable parameters.',
              style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
            )
          else
            for (final param in params) ...[
              _buildRow(context, param),
              const SizedBox(height: 10),
            ],
        ],
      ),
    );
  }

  Widget _buildRow(BuildContext context, APILiteralField param) {
    final effective = param.storedValue ??
        param.defaultValue ??
        _typeZero(param.dataType);
    final isWired = param.isWired;
    final isPlaceholder = param.storedValue == null;
    final opacity = isWired ? 0.45 : (isPlaceholder ? 0.55 : 1.0);

    return Padding(
      key: ValueKey('custom_param_${param.name}'),
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
                      TextSpan(text: param.name),
                      if (isPlaceholder && !isWired)
                        const TextSpan(
                          text: '  (default)',
                          style: TextStyle(
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
                  key: ValueKey('custom_param_clear_${param.name}'),
                  icon: const Icon(Icons.clear, size: 16),
                  tooltip: 'Clear stored value',
                  visualDensity: VisualDensity.compact,
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(),
                  onPressed: () =>
                      model.clearCustomNodeLiteral(nodeId, param.name),
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
              child: _buildInput(context, param, effective),
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

  void _set(APILiteralField param, APILiteralValue value) {
    model.setCustomNodeLiteral(nodeId, param.name, value);
  }

  Widget _buildInput(
    BuildContext context,
    APILiteralField param,
    APILiteralValue effective,
  ) {
    final inputKey = Key('custom_param_input_${param.name}');
    switch (param.dataType) {
      case APISimpleParamType.bool:
        return CheckboxListTile(
          key: inputKey,
          title: const Text('Value'),
          value: _asBool(effective),
          onChanged: (v) {
            if (v != null) _set(param, APILiteralValue.bool(v));
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
          onChanged: (v) => _set(param, APILiteralValue.int(v)),
        );
      case APISimpleParamType.float:
        return FloatInput(
          label: '',
          inputKey: inputKey,
          value: _asFloat(effective),
          onChanged: (v) => _set(param, APILiteralValue.float(v)),
        );
      case APISimpleParamType.str:
        return StringInput(
          label: '',
          inputKey: inputKey,
          value: _asStr(effective),
          onChanged: (v) => _set(param, APILiteralValue.str(v)),
        );
      case APISimpleParamType.iVec2:
        return IVec2Input(
          label: '',
          value: _asIVec2(effective),
          onChanged: (v) => _set(param, APILiteralValue.iVec2(v)),
        );
      case APISimpleParamType.iVec3:
        return IVec3Input(
          label: '',
          value: _asIVec3(effective),
          onChanged: (v) => _set(param, APILiteralValue.iVec3(v)),
        );
      case APISimpleParamType.vec2:
        return Vec2Input(
          label: '',
          value: _asVec2(effective),
          onChanged: (v) => _set(param, APILiteralValue.vec2(v)),
        );
      case APISimpleParamType.vec3:
        return Vec3Input(
          label: '',
          value: _asVec3(effective),
          onChanged: (v) => _set(param, APILiteralValue.vec3(v)),
        );
      case APISimpleParamType.iMat3:
        return _buildIMat3(param, _asIMat3(effective));
      case APISimpleParamType.mat3:
        return _buildMat3(param, _asMat3(effective));
    }
  }

  Widget _buildIMat3(APILiteralField param, List<Int32List> m) {
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
                      key: Key('custom_param_${param.name}_cell_${r}_$c'),
                      value: _imat3At(m, r, c),
                      enabled: true,
                      onChanged: (v) {
                        final next = <Int32List>[
                          for (var rr = 0; rr < 3; rr++)
                            Int32List.fromList([
                              for (var cc = 0; cc < 3; cc++) _imat3At(m, rr, cc),
                            ]),
                        ];
                        next[r][c] = v;
                        _set(param, APILiteralValue.iMat3(next));
                      },
                    ),
                  ),
              ],
            ),
          ),
      ],
    );
  }

  Widget _buildMat3(APILiteralField param, List<Float64List> m) {
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
                      key: Key('custom_param_${param.name}_cell_${r}_$c'),
                      value: _mat3At(m, r, c),
                      enabled: true,
                      onChanged: (v) {
                        final next = <Float64List>[
                          for (var rr = 0; rr < 3; rr++)
                            Float64List.fromList([
                              for (var cc = 0; cc < 3; cc++) _mat3At(m, rr, cc),
                            ]),
                        ];
                        next[r][c] = v;
                        _set(param, APILiteralValue.mat3(next));
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
  // does not match the parameter's declared type (e.g. a default pin that
  // resolved to a different simple type). ---

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
