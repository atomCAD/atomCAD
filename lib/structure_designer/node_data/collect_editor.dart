import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Default value seeded into the limit field on first toggle of the
/// "Limit elements" checkbox. Small enough that "I just want a peek" is the
/// path of least resistance, large enough to be useful for most streams.
const int DEFAULT_COLLECT_LIMIT = 100;

/// Pin index of the `limit` input pin on the `collect` node. Mirrors
/// `rust/src/structure_designer/nodes/collect.rs::get_node_type`.
const int COLLECT_LIMIT_PIN_INDEX = 1;

/// Pin index of the `offset` input pin on the `collect` node.
const int COLLECT_OFFSET_PIN_INDEX = 2;

/// Editor widget for `collect` nodes. Three stored properties:
///   - `element_type` drives the input pin's `Iter[T]` and the output pin's
///     `Array[T]` declared types.
///   - `limit` (optional) caps the collected array size. When the `limit`
///     input pin is wired, the wired Int overrides the stored value at
///     evaluation time — see `doc/design_iter_display_via_collect.md`.
///   - `offset` skips that many elements at the front of the stream before
///     collecting starts. `0` is the natural identity. When the `offset`
///     input pin is wired, the wired Int overrides the stored value.
class CollectEditor extends StatefulWidget {
  final BigInt nodeId;
  final APICollectData? data;
  final StructureDesignerModel model;

  const CollectEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<CollectEditor> createState() => CollectEditorState();
}

class CollectEditorState extends State<CollectEditor> {
  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final limit = widget.data!.limit;
    final offset = widget.data!.offset;
    final limitPinConnected = _isPinConnected(COLLECT_LIMIT_PIN_INDEX);
    final offsetPinConnected = _isPinConnected(COLLECT_OFFSET_PIN_INDEX);

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Collect Properties',
            nodeTypeName: 'collect',
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
          if (limitPinConnected)
            const Padding(
              padding: EdgeInsets.only(bottom: 4),
              child: Text(
                'Limit supplied by `limit` input. Disconnect to edit inline.',
                style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
              ),
            ),
          Opacity(
            opacity: limitPinConnected ? 0.5 : 1.0,
            child: IgnorePointer(
              ignoring: limitPinConnected,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Checkbox(
                        value: limit != null,
                        onChanged: (checked) => _onLimitCheckboxChanged(
                          checked ?? false,
                        ),
                      ),
                      const Text('Limit elements'),
                    ],
                  ),
                  if (limit != null)
                    Padding(
                      padding: const EdgeInsets.only(left: 32),
                      child: IntInput(
                        label: 'Limit',
                        value: limit,
                        minimumValue: 0,
                        onChanged: (newValue) => _setLimit(newValue),
                      ),
                    ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 12),
          if (offsetPinConnected)
            const Padding(
              padding: EdgeInsets.only(bottom: 4),
              child: Text(
                'Offset supplied by `offset` input. Disconnect to edit inline.',
                style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
              ),
            ),
          Opacity(
            opacity: offsetPinConnected ? 0.5 : 1.0,
            child: IgnorePointer(
              ignoring: offsetPinConnected,
              child: IntInput(
                label: 'Offset',
                value: offset,
                minimumValue: 0,
                onChanged: (newValue) => _setOffset(newValue),
              ),
            ),
          ),
        ],
      ),
    );
  }

  void _onLimitCheckboxChanged(bool checked) {
    if (checked) {
      _setLimit(DEFAULT_COLLECT_LIMIT);
    } else {
      _setLimit(null);
    }
  }

  void _setLimit(int? newLimit) {
    _updateData(limit: () => newLimit);
  }

  void _setOffset(int newOffset) {
    _updateData(offset: newOffset);
  }

  /// Single-channel update path. `limit` is wrapped in a thunk so callers can
  /// pass `() => null` to clear it without colliding with "field not provided".
  void _updateData({
    APIDataType? elementType,
    int? Function()? limit,
    int? offset,
  }) {
    widget.model.setCollectData(
      widget.nodeId,
      APICollectData(
        elementType: elementType ?? widget.data!.elementType,
        limit: limit != null ? limit() : widget.data!.limit,
        offset: offset ?? widget.data!.offset,
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
