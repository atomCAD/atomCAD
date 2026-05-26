import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/inputs/function_type_input.dart';
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
  /// Default APIDataType for a freshly-seeded slot — matches the
  /// closure-editor / parameter-node "free slot defaults to Float" convention.
  static APIDataType _defaultFloat() => const APIDataType(
        dataTypeBase: APIDataTypeBase.float,
        customDataType: null,
        array: false,
        children: [],
      );

  /// Read `children[i]` with a Float fallback. Inner branches rely on the
  /// dropdown-change handler having seeded `children` to the right shape, so
  /// this is defensive — covers the transient case where a value arrives from
  /// somewhere other than this widget's own writes.
  APIDataType _childAt(int i) => (i < widget.value.children.length)
      ? widget.value.children[i]
      : _defaultFloat();

  /// Build a new APIDataType keeping base/customDataType/array, replacing
  /// `children`.
  APIDataType _withChildren(List<APIDataType> children) => APIDataType(
        dataTypeBase: widget.value.dataTypeBase,
        customDataType: widget.value.customDataType,
        array: widget.value.array,
        children: children,
      );

  /// `children[0..N-1]` (the parameter types) for a Function base.
  List<APIDataType> _functionParams() {
    final c = widget.value.children;
    if (c.isEmpty) return const [];
    return c.sublist(0, c.length - 1);
  }

  /// `children[N]` (the return type) for a Function base.
  APIDataType _functionReturn() {
    final c = widget.value.children;
    if (c.isEmpty) return _defaultFloat();
    return c.last;
  }

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
                customDataType =
                    widget.value.dataTypeBase == APIDataTypeBase.record
                        ? widget.value.customDataType ?? ''
                        : '';
              }
              // Seed `children` per
              // doc/design_structural_function_and_iter_types.md §"Defaults".
              // Iter ⇒ one child (the element type); Function ⇒ [param, return]
              // for arity 1 (matching the closure-editor default). Switching
              // *away* from these bases drops children back to const [];
              // switching between Iter and Function replaces the seed (no
              // carry-over). This single seeding point is what lets the inner
              // branches treat _childAt / _functionParams / _functionReturn
              // as total functions.
              final List<APIDataType> seededChildren;
              if (newValue == APIDataTypeBase.iter) {
                seededChildren = [_defaultFloat()];
              } else if (newValue == APIDataTypeBase.function) {
                seededChildren = [_defaultFloat(), _defaultFloat()];
              } else {
                seededChildren = const [];
              }
              widget.onChanged(APIDataType(
                dataTypeBase: newValue,
                customDataType: customDataType,
                // Custom owns its own array semantics inside the string;
                // Record participates in the array checkbox like built-ins.
                array: newValue == APIDataTypeBase.custom
                    ? false
                    : widget.value.array,
                children: seededChildren,
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
                  children: const [],
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
                  children: const [],
                ));
              },
            ),
          ),

        // Iter[T] branch — one nested DataTypeInput for the element type.
        if (widget.value.dataTypeBase == APIDataTypeBase.iter)
          Padding(
            padding: const EdgeInsets.only(top: 8.0),
            child: DataTypeInput(
              label: 'Element Type',
              value: _childAt(0),
              onChanged: (newElement) =>
                  widget.onChanged(_withChildren([newElement])),
            ),
          ),

        // Function((p0,...,pN-1) -> R) branch.
        if (widget.value.dataTypeBase == APIDataTypeBase.function)
          Padding(
            padding: const EdgeInsets.only(top: 8.0),
            child: FunctionTypeInput(
              parameterTypes: _functionParams(),
              outputType: _functionReturn(),
              onChanged: (params, ret) =>
                  widget.onChanged(_withChildren([...params, ret])),
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
                  children: widget.value.children,
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
      case APIDataTypeBase.unit:
        return 'Unit';
      case APIDataTypeBase.record:
        return 'Record';
      case APIDataTypeBase.iter:
        return 'Iter[T]';
      case APIDataTypeBase.function:
        return 'Function(args…) → R';
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
