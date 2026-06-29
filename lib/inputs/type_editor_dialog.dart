import 'package:flutter/material.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/inputs/function_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Compact human-readable summary of an `APIDataType`, for the "Edit" affordance
/// in [DataTypeInput]'s structural branches. Mirrors the substrate's text
/// format (`Iter[T]`, `(T0, T1) -> R`, `Array[X]`) without round-tripping
/// through the Rust parser — this is purely UI-side rendering.
String apiDataTypeToString(APIDataType type) {
  final inner = _innerToString(type);
  return type.array ? 'Array[$inner]' : inner;
}

String _innerToString(APIDataType type) {
  switch (type.dataTypeBase) {
    case APIDataTypeBase.iter:
      if (type.children.isEmpty) return 'Iter[?]';
      return 'Iter[${apiDataTypeToString(type.children.first)}]';
    case APIDataTypeBase.optional:
      if (type.children.isEmpty) return 'Optional[?]';
      return 'Optional[${apiDataTypeToString(type.children.first)}]';
    case APIDataTypeBase.function:
      if (type.children.isEmpty) return '() → ?';
      final n = type.children.length - 1;
      final params =
          type.children.sublist(0, n).map(apiDataTypeToString).join(', ');
      final ret = apiDataTypeToString(type.children[n]);
      return '($params) → $ret';
    case APIDataTypeBase.record:
      final name = type.customDataType ?? '';
      return name.isEmpty ? 'Record(?)' : 'Record($name)';
    case APIDataTypeBase.custom:
      final txt = type.customDataType ?? '';
      return txt.isEmpty ? '?' : txt;
    default:
      return _flatBaseLabel(type.dataTypeBase);
  }
}

String _flatBaseLabel(APIDataTypeBase base) {
  switch (base) {
    case APIDataTypeBase.none:
      return 'None';
    case APIDataTypeBase.bool:
      return 'Bool';
    case APIDataTypeBase.string:
      return 'String';
    case APIDataTypeBase.int:
      return 'Int';
    case APIDataTypeBase.float:
      return 'Float';
    case APIDataTypeBase.vec2:
      return 'Vec2';
    case APIDataTypeBase.vec3:
      return 'Vec3';
    case APIDataTypeBase.iVec2:
      return 'IVec2';
    case APIDataTypeBase.iVec3:
      return 'IVec3';
    case APIDataTypeBase.iMat2:
      return 'IMat2';
    case APIDataTypeBase.iMat3:
      return 'IMat3';
    case APIDataTypeBase.mat3:
      return 'Mat3';
    case APIDataTypeBase.latticeVecs:
      return 'LatticeVecs';
    case APIDataTypeBase.drawingPlane:
      return 'DrawingPlane';
    case APIDataTypeBase.geometry2D:
      return 'Geometry2D';
    case APIDataTypeBase.blueprint:
      return 'Blueprint';
    case APIDataTypeBase.hasAtoms:
      return 'HasAtoms';
    case APIDataTypeBase.crystal:
      return 'Crystal';
    case APIDataTypeBase.molecule:
      return 'Molecule';
    case APIDataTypeBase.hasStructure:
      return 'HasStructure';
    case APIDataTypeBase.hasFreeLinOps:
      return 'HasFreeLinOps';
    case APIDataTypeBase.motif:
      return 'Motif';
    case APIDataTypeBase.structure:
      return 'Structure';
    case APIDataTypeBase.unit:
      return 'Unit';
    case APIDataTypeBase.record:
    case APIDataTypeBase.iter:
    case APIDataTypeBase.optional:
    case APIDataTypeBase.function:
    case APIDataTypeBase.custom:
      // Handled by _innerToString.
      return '?';
  }
}

/// Opens a draggable modal that hosts the structural editor for `value` (a
/// [FunctionTypeInput] for Function, a nested [DataTypeInput] for Iter). The
/// base variant is fixed for the lifetime of the dialog — to change the base,
/// the user closes the dialog and uses the outer dropdown. Edits commit live
/// to [onChanged]; the dialog has a single "Close" button (no Apply/Cancel).
/// Recursion into another structural inner type opens a nested dialog
/// naturally (the inner [DataTypeInput] uses the same affordance).
Future<void> showTypeEditorDialog({
  required BuildContext context,
  required APIDataType initialValue,
  required ValueChanged<APIDataType> onChanged,
}) async {
  APIDataType current = initialValue;

  await showDialog<void>(
    context: context,
    barrierDismissible: false,
    builder: (dialogCtx) {
      return DraggableDialog(
        width: 520,
        dismissible: true,
        backgroundColor: Theme.of(dialogCtx).dialogBackgroundColor,
        child: StatefulBuilder(
          builder: (ctx, setLocalState) {
            void commit(APIDataType next) {
              setLocalState(() {
                current = next;
              });
              onChanged(next);
            }

            return Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    crossAxisAlignment: CrossAxisAlignment.center,
                    children: [
                      Expanded(
                        child: Text(
                          _dialogTitle(current),
                          style: Theme.of(ctx).textTheme.titleMedium,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                      IconButton(
                        icon: const Icon(Icons.close),
                        tooltip: 'Close',
                        onPressed: () => Navigator.of(ctx).pop(),
                      ),
                    ],
                  ),
                  const SizedBox(height: 4),
                  Text(
                    apiDataTypeToString(current),
                    style: Theme.of(ctx)
                        .textTheme
                        .bodyMedium
                        ?.copyWith(fontStyle: FontStyle.italic),
                  ),
                  const Divider(height: 24),
                  if (current.dataTypeBase == APIDataTypeBase.function)
                    FunctionTypeInput(
                      parameterTypes: _functionParams(current),
                      outputType: _functionReturn(current),
                      onChanged: (params, ret) => commit(
                        APIDataType(
                          dataTypeBase: current.dataTypeBase,
                          customDataType: current.customDataType,
                          array: current.array,
                          children: [...params, ret],
                        ),
                      ),
                    )
                  else if (current.dataTypeBase == APIDataTypeBase.iter)
                    DataTypeInput(
                      label: 'Element Type',
                      value: _iterElement(current),
                      onChanged: (newElement) => commit(
                        APIDataType(
                          dataTypeBase: current.dataTypeBase,
                          customDataType: current.customDataType,
                          array: current.array,
                          children: [newElement],
                        ),
                      ),
                    )
                  else if (current.dataTypeBase == APIDataTypeBase.optional)
                    DataTypeInput(
                      label: 'Inner Type',
                      // The inner editor blocks the ill-formed Optional inners
                      // (Optional/Iter/Unit/None). See doc/design_optional_type.md §3.
                      optionalInner: true,
                      value: _iterElement(current),
                      onChanged: (newElement) => commit(
                        APIDataType(
                          dataTypeBase: current.dataTypeBase,
                          customDataType: current.customDataType,
                          array: current.array,
                          children: [newElement],
                        ),
                      ),
                    ),
                  const SizedBox(height: 16),
                  Align(
                    alignment: Alignment.centerRight,
                    child: TextButton(
                      onPressed: () => Navigator.of(ctx).pop(),
                      child: const Text('Close'),
                    ),
                  ),
                ],
              ),
            );
          },
        ),
      );
    },
  );
}

String _dialogTitle(APIDataType type) {
  switch (type.dataTypeBase) {
    case APIDataTypeBase.function:
      return 'Function Type';
    case APIDataTypeBase.iter:
      return 'Iterator Type';
    case APIDataTypeBase.optional:
      return 'Optional Type';
    default:
      return 'Edit Type';
  }
}

const APIDataType _floatType = APIDataType(
  dataTypeBase: APIDataTypeBase.float,
  customDataType: null,
  array: false,
  children: [],
);

APIDataType _iterElement(APIDataType type) =>
    type.children.isEmpty ? _floatType : type.children.first;

List<APIDataType> _functionParams(APIDataType type) {
  if (type.children.isEmpty) return const [];
  return type.children.sublist(0, type.children.length - 1);
}

APIDataType _functionReturn(APIDataType type) {
  if (type.children.isEmpty) return _floatType;
  return type.children.last;
}
