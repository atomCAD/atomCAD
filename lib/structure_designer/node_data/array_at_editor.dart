import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Pin index of the `index` input pin on the `array_at` node. Mirrors
/// `rust/src/structure_designer/nodes/array_at.rs::get_node_type`.
const int ARRAY_AT_INDEX_PIN_INDEX = 1;

/// Editor widget for `array_at` nodes. Two stored properties:
///   - `element_type` drives the input pin's `Array[T]` and the output pin's
///     `T` declared types.
///   - `index` is the integer index used when the `index` input pin is not
///     wired. When the pin is connected, the wired Int overrides the stored
///     value at evaluation time — same convention as `collect.limit`.
class ArrayAtEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIArrayAtData? data;
  final StructureDesignerModel model;

  const ArrayAtEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<ArrayAtEditor> createState() => ArrayAtEditorState();
}

class ArrayAtEditorState extends State<ArrayAtEditor> {
  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final index = widget.data!.index;
    final indexPinConnected = _isPinConnected(ARRAY_AT_INDEX_PIN_INDEX);

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Array At Properties',
            nodeTypeName: 'array_at',
          ),
          const SizedBox(height: 8),
          DataTypeInput(
            label: 'Element Type',
            value: widget.data!.elementType,
            onChanged: (newValue) {
              _updateData(elementType: newValue);
            },
          ),
          const SizedBox(height: 12),
          if (indexPinConnected)
            const Padding(
              padding: EdgeInsets.only(bottom: 4),
              child: Text(
                'Index supplied by `index` input. Disconnect to edit inline.',
                style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
              ),
            ),
          Opacity(
            opacity: indexPinConnected ? 0.5 : 1.0,
            child: IgnorePointer(
              ignoring: indexPinConnected,
              child: IntInput(
                label: 'Index',
                value: index,
                minimumValue: 0,
                onChanged: (newValue) => _setIndex(newValue),
              ),
            ),
          ),
        ],
      ),
    );
  }

  void _setIndex(int newIndex) {
    _updateData(index: newIndex);
  }

  void _updateData({APIDataType? elementType, int? index}) {
    widget.model.setArrayAtData(
      widget.nodeId,
      APIArrayAtData(
        elementType: elementType ?? widget.data!.elementType,
        index: index ?? widget.data!.index,
      ),
    );
  }

  bool _isPinConnected(int pinIndex) {
    final view = widget.model.nodeNetworkView;
    if (view == null) return false;
    final target = BigInt.from(pinIndex);
    for (final wire in view.wires) {
      if (wire.destNodeId == widget.nodeId && wire.destParamIndex == target) {
        return true;
      }
    }
    return false;
  }
}
