import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/node_data/derived_output_type_display.dart';

/// Editor widget for `zip_with` nodes (n-ary element-wise map, issue #382).
///
/// The N input lanes have fixed, position-derived pin names (`xs1..xsN`), so
/// the panel is just an ordered list of lane types — one `xs{i}` label +
/// `DataTypeInput` + delete button per lane — plus an "Add Input" button. No
/// name fields (lane identity is a hidden Rust-side stable id).
///
/// - **Retype / add** go through the whole-list positional merge
///   (`setZipWithData`), which preserves per-lane ids so external wires follow
///   their lane.
/// - **Delete** uses the id-accurate `removeZipWithLane` path so removing a
///   middle lane keeps the later lanes' wires (they renumber, the wires stay).
///   Disabled when only one lane remains (minimum arity is 1).
/// - **Output Type** is editable when `f` is disconnected (the stored
///   fallback); it swaps to the read-only [DerivedOutputTypeDisplay] when `f`
///   is wired (`node.derivedShape?.derivedFromInputPin == 'f'`).
///
/// See `doc/design_zip_with.md` (Phase 5).
class ZipWithEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIZipWithData? data;
  final StructureDesignerModel model;

  /// The wider node view, used to read the wired-`f` derivation state for
  /// the read-only output-type display.
  final NodeView node;

  const ZipWithEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
    required this.node,
  });

  /// Default for a freshly-added lane (matches the Rust `ZipWithData` default).
  static APIDataType _defaultType() => const APIDataType(
        dataTypeBase: APIDataTypeBase.float,
        customDataType: null,
        array: false,
        children: [],
      );

  /// Push a whole-list lane + output edit (positional id merge Rust-side).
  void _commit(List<APIDataType> laneTypes, APIDataType outputType) {
    model.setZipWithData(
      nodeId,
      APIZipWithData(laneTypes: laneTypes, outputType: outputType),
    );
  }

  void _changeLaneType(int index, APIDataType value) {
    final lanes = [...data!.laneTypes];
    lanes[index] = value;
    _commit(lanes, data!.outputType);
  }

  void _addLane() {
    final lanes = [...data!.laneTypes, _defaultType()];
    _commit(lanes, data!.outputType);
  }

  void _removeLane(int index) {
    // Id-accurate removal: surviving lanes keep their external wires.
    model.removeZipWithLane(nodeId, index);
  }

  void _changeOutputType(APIDataType value) {
    _commit([...data!.laneTypes], value);
  }

  @override
  Widget build(BuildContext context) {
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final lanes = data!.laneTypes;
    final canDelete = lanes.length > 1;
    final derivedFromF = node.derivedShape?.derivedFromInputPin == 'f';

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Zip With Properties',
            nodeTypeName: 'zip_with',
          ),
          const SizedBox(height: 8),

          const Padding(
            padding: EdgeInsets.only(bottom: 4.0),
            child: Text('Input Lanes', style: TextStyle(color: Colors.white70)),
          ),

          // One row per lane: fixed `xs{i}` label + type picker + delete.
          for (int i = 0; i < lanes.length; i++)
            Padding(
              padding: const EdgeInsets.only(bottom: 8.0),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: DataTypeInput(
                      label: 'xs${i + 1}',
                      value: lanes[i],
                      onChanged: (newValue) => _changeLaneType(i, newValue),
                    ),
                  ),
                  IconButton(
                    icon: const Icon(Icons.delete_outline),
                    tooltip: canDelete
                        ? 'Remove lane'
                        : 'zip_with needs at least one lane',
                    onPressed: canDelete ? () => _removeLane(i) : null,
                  ),
                ],
              ),
            ),

          Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              icon: const Icon(Icons.add, size: 18),
              label: const Text('Add Input'),
              onPressed: _addLane,
            ),
          ),

          const Divider(height: 24),

          // Output Type — derived from `f` when wired (read-only) or editable
          // when `f` is disconnected (the stored fallback).
          if (derivedFromF)
            DerivedOutputTypeDisplay(node: node)
          else
            DataTypeInput(
              label: 'Output Type',
              value: data!.outputType,
              onChanged: _changeOutputType,
            ),
        ],
      ),
    );
  }
}
