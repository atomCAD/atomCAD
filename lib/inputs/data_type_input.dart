import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// A widget for editing APIDataType values, allowing selection between built-in and custom data types
class DataTypeInput extends StatefulWidget {
  final String label;
  final APIDataType value;
  final ValueChanged<APIDataType> onChanged;

  const DataTypeInput({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
  });

  @override
  State<DataTypeInput> createState() => _DataTypeInputState();
}

class _DataTypeInputState extends State<DataTypeInput> {
  bool get _isBuiltIn => widget.value.builtInDataType != null;
  bool get _isCustom => widget.value.customDataType != null;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // Label
        if (widget.label.isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(bottom: 8.0),
            child: Text(
              widget.label,
              style: Theme.of(context).textTheme.labelMedium,
            ),
          ),

        // Built-in vs Custom selection
        Row(
          children: [
            Expanded(
              child: RadioListTile<bool>(
                title: const Text('Built-in'),
                value: true,
                groupValue: _isBuiltIn,
                onChanged: (value) {
                  if (value == true && !_isBuiltIn) {
                    // Switch to built-in type, default to 'none'
                    widget.onChanged(APIDataType(
                      builtInDataType: APIBuiltInDataType.none,
                      customDataType: null,
                      array: widget.value.array,
                    ));
                  }
                },
                contentPadding: EdgeInsets.zero,
                dense: true,
              ),
            ),
            Expanded(
              child: RadioListTile<bool>(
                title: const Text('Custom'),
                value: false,
                groupValue: _isBuiltIn,
                onChanged: (value) {
                  if (value == false && _isBuiltIn) {
                    // Switch to custom type, default to empty string
                    widget.onChanged(APIDataType(
                      builtInDataType: null,
                      customDataType: '',
                      array: widget.value.array,
                    ));
                  }
                },
                contentPadding: EdgeInsets.zero,
                dense: true,
              ),
            ),
          ],
        ),

        const SizedBox(height: 8),

        // Built-in type dropdown or custom type input
        if (_isBuiltIn)
          DropdownButtonFormField<APIBuiltInDataType>(
            value: widget.value.builtInDataType,
            decoration: AppInputDecorations.standard.copyWith(
              labelText: 'Built-in Type',
            ),
            items: APIBuiltInDataType.values.map((dataType) {
              return DropdownMenuItem(
                value: dataType,
                child: Text(_getBuiltInDataTypeDisplayName(dataType)),
              );
            }).toList(),
            onChanged: (newValue) {
              if (newValue != null) {
                widget.onChanged(APIDataType(
                  builtInDataType: newValue,
                  customDataType: null,
                  array: widget.value.array,
                ));
              }
            },
          )
        else
          StringInput(
            label: 'Custom Type',
            value: widget.value.customDataType ?? '',
            onChanged: (newValue) {
              widget.onChanged(APIDataType(
                builtInDataType: null,
                customDataType: newValue,
                array: widget.value.array,
              ));
            },
          ),

        const SizedBox(height: 8),

        // Array checkbox
        CheckboxListTile(
          title: const Text('Array'),
          value: widget.value.array,
          onChanged: (newValue) {
            if (newValue != null) {
              widget.onChanged(APIDataType(
                builtInDataType: widget.value.builtInDataType,
                customDataType: widget.value.customDataType,
                array: newValue,
              ));
            }
          },
          contentPadding: EdgeInsets.zero,
          dense: true,
        ),
      ],
    );
  }

  String _getBuiltInDataTypeDisplayName(APIBuiltInDataType dataType) {
    switch (dataType) {
      case APIBuiltInDataType.none:
        return 'None';
      case APIBuiltInDataType.bool:
        return 'Boolean';
      case APIBuiltInDataType.string:
        return 'String';
      case APIBuiltInDataType.int:
        return 'Integer';
      case APIBuiltInDataType.float:
        return 'Float';
      case APIBuiltInDataType.vec2:
        return 'Vec2';
      case APIBuiltInDataType.vec3:
        return 'Vec3';
      case APIBuiltInDataType.iVec2:
        return 'IVec2';
      case APIBuiltInDataType.iVec3:
        return 'IVec3';
      case APIBuiltInDataType.geometry2D:
        return 'Geometry2D';
      case APIBuiltInDataType.geometry:
        return 'Geometry';
      case APIBuiltInDataType.atomic:
        return 'Atomic';
    }
  }
}
