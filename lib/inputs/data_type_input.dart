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
  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // Dropdown for the base data type
        DropdownButtonFormField<APIDataTypeBase>(
          value: widget.value.dataTypeBase,
          decoration: AppInputDecorations.standard.copyWith(
            labelText: widget.label,
          ),
          items: APIDataTypeBase.values.map((base) {
            return DropdownMenuItem(
              value: base,
              child: Text(_getDataTypeBaseDisplayName(base)),
            );
          }).toList(),
          onChanged: (newValue) {
            if (newValue != null) {
              widget.onChanged(APIDataType(
                dataTypeBase: newValue,
                // Reset custom string if not custom, maintain if it is
                customDataType: newValue == APIDataTypeBase.custom
                    ? widget.value.customDataType ?? ''
                    : null,
                // Reset array status if switching to custom
                array: newValue == APIDataTypeBase.custom
                    ? false
                    : widget.value.array,
              ));
            }
          },
        ),

        // Conditional custom type input
        if (widget.value.dataTypeBase == APIDataTypeBase.custom)
          Padding(
            padding: const EdgeInsets.only(top: 8.0),
            child: StringInput(
              label: 'Custom Type',
              value: widget.value.customDataType ?? '',
              onChanged: (newCustomType) {
                widget.onChanged(APIDataType(
                  dataTypeBase: APIDataTypeBase.custom,
                  customDataType: newCustomType,
                  array:
                      false, // Custom types handle their own array logic via string parsing
                ));
              },
            ),
          ),

        // Conditional array checkbox
        if (widget.value.dataTypeBase != APIDataTypeBase.custom)
          CheckboxListTile(
            title: const Text('Array'),
            value: widget.value.array,
            onChanged: (newArrayValue) {
              if (newArrayValue != null) {
                widget.onChanged(APIDataType(
                  dataTypeBase: widget.value.dataTypeBase,
                  customDataType: widget.value.customDataType,
                  array: newArrayValue,
                ));
              }
            },
            controlAffinity: ListTileControlAffinity.leading,
            contentPadding: EdgeInsets.zero,
            dense: true,
          ),
      ],
    );
  }

  String _getDataTypeBaseDisplayName(APIDataTypeBase base) {
    switch (base) {
      case APIDataTypeBase.none:
        return 'None';
      case APIDataTypeBase.bool:
        return 'Boolean';
      case APIDataTypeBase.string:
        return 'String';
      case APIDataTypeBase.int:
        return 'Integer';
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
      case APIDataTypeBase.unitCell:
        return 'UnitCell';
      case APIDataTypeBase.drawingPlane:
        return 'DrawingPlane';
      case APIDataTypeBase.geometry2D:
        return 'Geometry2D';
      case APIDataTypeBase.geometry:
        return 'Geometry';
      case APIDataTypeBase.atomic:
        return 'Atomic';
      case APIDataTypeBase.motif:
        return 'Motif';
      case APIDataTypeBase.custom:
        return 'Custom...';
    }
  }
}
