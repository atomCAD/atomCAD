import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Picker for a `Function((p0, p1, ..., pN-1) -> R)` data type. One row per
/// parameter (a nested [DataTypeInput] + delete button), an "Add parameter"
/// button, and a return-type [DataTypeInput].
///
/// Knows nothing about closures — function types have no parameter names
/// (the load-bearing invariant in `doc/design_custom_closure_kind.md`); the
/// closure editor's `_CustomParamRow` is a separate widget that *additionally*
/// carries a name. Sharing a base widget is left as a follow-up
/// (`doc/design_structural_function_and_iter_types.md` §"Open questions").
///
/// Function arity 0 (a thunk, `() -> R`) is a legal type, so the delete
/// button is enabled at all arities — even though the Custom-closure UI
/// defers authoring zero-arg *closures*. See
/// `doc/design_structural_function_and_iter_types.md` §"Editor (Flutter)".
class FunctionTypeInput extends StatelessWidget {
  final List<APIDataType> parameterTypes;
  final APIDataType outputType;
  final void Function(List<APIDataType> params, APIDataType output) onChanged;

  const FunctionTypeInput({
    super.key,
    required this.parameterTypes,
    required this.outputType,
    required this.onChanged,
  });

  static APIDataType _defaultParam() => const APIDataType(
        dataTypeBase: APIDataTypeBase.float,
        customDataType: null,
        array: false,
        children: [],
      );

  void _changeParam(int i, APIDataType value) {
    final newParams = <APIDataType>[
      for (int j = 0; j < parameterTypes.length; j++)
        (j == i) ? value : parameterTypes[j],
    ];
    onChanged(newParams, outputType);
  }

  void _removeParam(int i) {
    final newParams = <APIDataType>[
      for (int j = 0; j < parameterTypes.length; j++)
        if (j != i) parameterTypes[j],
    ];
    onChanged(newParams, outputType);
  }

  void _addParam() {
    final newParams = <APIDataType>[...parameterTypes, _defaultParam()];
    onChanged(newParams, outputType);
  }

  void _changeReturn(APIDataType value) {
    onChanged(parameterTypes, value);
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Padding(
          padding: EdgeInsets.only(bottom: 4.0),
          child: Text('Parameters', style: TextStyle(color: Colors.white70)),
        ),
        for (int i = 0; i < parameterTypes.length; i++)
          Padding(
            padding: const EdgeInsets.only(bottom: 8.0),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(
                  child: DataTypeInput(
                    label: 'Parameter ${i + 1}',
                    value: parameterTypes[i],
                    onChanged: (newValue) => _changeParam(i, newValue),
                  ),
                ),
                IconButton(
                  icon: const Icon(Icons.delete_outline),
                  tooltip: 'Remove parameter',
                  onPressed: () => _removeParam(i),
                ),
              ],
            ),
          ),
        Align(
          alignment: Alignment.centerLeft,
          child: TextButton.icon(
            icon: const Icon(Icons.add, size: 18),
            label: const Text('Add parameter'),
            onPressed: _addParam,
          ),
        ),
        const Divider(height: 24),
        const Padding(
          padding: EdgeInsets.only(bottom: 4.0),
          child: Text('Return Type', style: TextStyle(color: Colors.white70)),
        ),
        DataTypeInput(
          label: 'Return Type',
          value: outputType,
          onChanged: _changeReturn,
        ),
      ],
    );
  }
}
