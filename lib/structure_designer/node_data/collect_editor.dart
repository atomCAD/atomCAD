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

/// Editor widget for `collect` nodes. Two stored properties:
///   - `element_type` drives the input pin's `Iter[T]` and the output pin's
///     `Array[T]` declared types.
///   - `limit` (optional) caps the collected array size. When the `limit`
///     input pin is wired, the wired Int overrides the stored value at
///     evaluation time — see `doc/design_iter_display_via_collect.md`.
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
    final limitPinConnected = _isLimitPinConnected();

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
              widget.model.setCollectData(
                widget.nodeId,
                APICollectData(elementType: newValue, limit: limit),
              );
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
    widget.model.setCollectData(
      widget.nodeId,
      APICollectData(
        elementType: widget.data!.elementType,
        limit: newLimit,
      ),
    );
  }

  bool _isLimitPinConnected() {
    final view = widget.model.nodeNetworkView;
    if (view == null) return false;
    for (final wire in view.wires) {
      if (wire.destNodeId == widget.nodeId &&
          wire.destParamIndex == BigInt.one) {
        return true;
      }
    }
    return false;
  }
}
