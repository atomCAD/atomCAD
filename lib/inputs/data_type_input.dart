import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
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
              // When switching to a base, seed the inner string. Custom keeps
              // any prior free-form string; Record starts empty (the user
              // picks a def from the dropdown below); other bases drop the
              // string entirely.
              String? customDataType;
              if (newValue == APIDataTypeBase.custom) {
                customDataType = widget.value.customDataType ?? '';
              } else if (newValue == APIDataTypeBase.record) {
                customDataType = widget.value.dataTypeBase ==
                        APIDataTypeBase.record
                    ? widget.value.customDataType ?? ''
                    : '';
              }
              widget.onChanged(APIDataType(
                dataTypeBase: newValue,
                customDataType: customDataType,
                // Custom owns its own array semantics inside the string;
                // Record participates in the array checkbox like built-ins.
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

        // Conditional record def dropdown (named records only — anonymous
        // record types are reachable from the expression language, not the
        // type-selector UI).
        if (widget.value.dataTypeBase == APIDataTypeBase.record)
          Padding(
            padding: const EdgeInsets.only(top: 8.0),
            child: _RecordDefDropdown(
              value: widget.value.customDataType ?? '',
              onChanged: (newName) {
                widget.onChanged(APIDataType(
                  dataTypeBase: APIDataTypeBase.record,
                  customDataType: newName,
                  array: widget.value.array,
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
      case APIDataTypeBase.record:
        return 'Record';
      case APIDataTypeBase.custom:
        return 'Custom...';
    }
  }
}

/// Dropdown of named record type defs in the project. Used by
/// `DataTypeInput`'s Record branch and by the per-node property editors for
/// `record_construct` / `record_destructure`. New defs are created from the
/// user-types panel (Phase 6) — this widget never creates them.
class _RecordDefDropdown extends StatelessWidget {
  final String value;
  final ValueChanged<String> onChanged;

  const _RecordDefDropdown({
    required this.value,
    required this.onChanged,
  });

  @override
  Widget build(BuildContext context) {
    // Includes built-in record defs alongside user defs so e.g.
    // `ElementMapping` is selectable without manual setup.
    final names = sd_api.getAllRecordTypeDefNames() ?? <String>[];
    final danglingButNotEmpty = value.isNotEmpty && !names.contains(value);
    // The dropdown's value must be one of its items. We always include the
    // empty-state sentinel; a dangling reference shows up as a synthetic
    // entry so the user sees the broken state rather than silently snapping
    // to a different def.
    final entries = <DropdownMenuItem<String>>[
      const DropdownMenuItem<String>(
        value: '',
        child: Text(
          '— No record type chosen —',
          style: TextStyle(fontStyle: FontStyle.italic),
        ),
      ),
      ...names.map(
        (name) => DropdownMenuItem<String>(
          value: name,
          child: Text(name),
        ),
      ),
      if (danglingButNotEmpty)
        DropdownMenuItem<String>(
          value: value,
          child: Text(
            '$value (missing)',
            style: const TextStyle(color: Colors.red),
          ),
        ),
    ];

    return DropdownButtonFormField<String>(
      value: value,
      decoration: AppInputDecorations.standard.copyWith(
        labelText: 'Record Type',
      ),
      items: entries,
      onChanged: (newValue) {
        if (newValue != null) {
          onChanged(newValue);
        }
      },
    );
  }
}
